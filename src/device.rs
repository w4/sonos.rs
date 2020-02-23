use std::net::IpAddr;
use std::time::Duration;

use xmltree::{Element, XMLNode};
use reqwest::header::HeaderMap;
use regex::Regex;

use crate::error::*;
use failure::Error;
use std::borrow::Cow;
use std::num::ParseIntError;

#[derive(Debug)]
pub struct Speaker {
    pub ip: IpAddr,
    pub model: String,
    pub model_number: String,
    pub software_version: String,
    pub hardware_version: String,
    pub serial_number: String,
    pub name: String,
    pub uuid: String,
}

#[derive(Debug)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub queue_position: u64,
    pub uri: String,
    pub duration: Duration,
    pub running_time: Duration,
}

#[derive(Debug, PartialEq)]
pub enum TransportState {
    Stopped,
    Playing,
    PausedPlayback,
    PausedRecording,
    Recording,
    Transitioning,
}

lazy_static! {
    static ref COORDINATOR_REGEX: Regex = Regex::new(r"^https?://(.+?):1400/xml")
        .expect("Failed to create regex");
}

fn get_child_element<'a>(el: &'a Element, name: &str) -> Result<&'a Element, Error> {
    el.get_child(name)
        .ok_or_else(|| SonosError::ParseError(format!("missing {} element", name)).into())
}

fn get_child_element_text<'a>(el: &'a Element, name: &str) -> Result<Cow<'a, str>, Error> {
   get_child_element(el, name)?
        .get_text()
        .ok_or_else(|| SonosError::ParseError(format!("no text on {} element", name)).into())
}

impl Speaker {
    /// Create a new instance of this struct from an IP address
    pub async fn from_ip(ip: IpAddr) -> Result<Speaker, Error> {
        let resp = reqwest::get(&format!("http://{}:1400/xml/device_description.xml", ip)).await?;

        if !resp.status().is_success() {
            return Err(SonosError::BadResponse(resp.status().as_u16()).into());
        }

        let root = Element::parse(resp.bytes().await?.as_ref())?;
        let device_description = get_child_element(&root, "device")?;

        Ok(Speaker {
            ip,
            model: get_child_element_text(device_description, "modelName")?.into_owned(),
            model_number: get_child_element_text(device_description, "modelNumber")?.into_owned(),
            software_version: get_child_element_text(device_description, "softwareVersion")?.into_owned(),
            hardware_version: get_child_element_text(device_description, "hardwareVersion")?.into_owned(),
            serial_number: get_child_element_text(device_description, "serialNum")?.into_owned(),
            name: get_child_element_text(device_description, "roomName")?.into_owned(),
            // we slice the UDN to remove "uuid:"
            uuid: get_child_element_text(device_description, "UDN")?[5..].to_string(),
        })
    }

    /// Get the coordinator for this speaker.
    #[deprecated(note = "Broken on Sonos 9.1")]
    pub async fn coordinator(&self) -> Result<IpAddr, Error> {
        let resp = reqwest::get(&format!("http://{}:1400/status/topology", self.ip)).await?;

        if !resp.status().is_success() {
            return Err(SonosError::BadResponse(resp.status().as_u16()).into());
        }

        let content = resp.text().await?;

        // parse the topology xml
        let elements = Element::parse(content.as_bytes())?;

        if elements.children.is_empty() {
            // on Sonos 9.1 this API will always return an empty string in which case we'll return
            // the current speaker's IP as the 'coordinator'
            return Ok(self.ip);
        }

        let zone_players = get_child_element(&elements, "ZonePlayers")?;

        // get the group identifier from the given player
        let group = &zone_players
            .children
            .iter()
            .map(XMLNode::as_element)
            .filter(Option::is_some)
            .map(Option::unwrap)
            .find(|child| child.attributes["uuid"] == self.uuid)
            .ok_or_else(|| SonosError::DeviceNotFound(self.uuid.to_string()))?
            .attributes["group"];

        let parent = zone_players.children.iter()
            // get the coordinator for the given group
            .map(XMLNode::as_element)
            .filter(Option::is_some)
            .map(Option::unwrap)
            .find(|child|
                child.attributes.get("coordinator").unwrap_or(&"false".to_string()) == "true" &&
                    child.attributes.get("group").unwrap_or(&"".to_string()) == group)
            .ok_or_else(|| SonosError::DeviceNotFound(self.uuid.to_string()))?
            .attributes
            .get("location")
            .ok_or_else(|| SonosError::ParseError("missing group identifier".to_string()))?;

        Ok(COORDINATOR_REGEX
            .captures(parent)
            .ok_or_else(|| SonosError::ParseError("couldn't parse coordinator url".to_string()))?[1]
            .parse()?)
    }

    /// Call the Sonos SOAP endpoint
    ///
    /// # Arguments
    /// * `endpoint` - The SOAP endpoint to call (eg. MediaRenderer/AVTransport/Control)
    /// * `service` - The SOAP service to call (eg. urn:schemas-upnp-org:service:AVTransport:1)
    /// * `action` - The action to call on the soap service (eg. Play)
    /// * `payload` - XML doc to pass inside the action call body
    /// * `coordinator` - Whether this SOAP call should be performed on the group coordinator or
    ///                   the speaker it was called on
    pub async fn soap(
        &self,
        endpoint: &str,
        service: &str,
        action: &str,
        payload: &str,
        coordinator: bool,
    ) -> Result<Element, Error> {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", "application/xml".parse()?);
        headers.insert("SOAPAction", format!("\"{}#{}\"", service, action).parse()?);

        let client = reqwest::Client::new();
        let coordinator = if coordinator {
            self.coordinator().await?
        } else {
            self.ip
        };

        debug!("Running {}#{} on {}", service, action, coordinator);

        let request = client
            .post(&format!("http://{}:1400/{}", coordinator, endpoint))
            .headers(headers)
            .body(format!(
                r#"
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"
                s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:{action} xmlns:u="{service}">
                        {payload}
                    </u:{action}>
                </s:Body>
            </s:Envelope>"#,
                service = service,
                action = action,
                payload = payload
            ))
            .send()
            .await?;

        let element = Element::parse(request.bytes().await?.as_ref())?;

        let body = get_child_element(&element, "Body")?;

        if let Some(fault) = body.get_child("Fault") {
            let error_code = fault
                .get_child("detail")
                .and_then(|c| c.get_child("UPnPError"))
                .and_then(|c| c.get_child("errorCode"))
                .and_then(|c| c.get_text())
                .ok_or_else(|| SonosError::ParseError("failed to parse error".to_string()))?
                .parse::<u64>()?;

            let state = AVTransportError::from(error_code);
            error!("Got state {:?} from {}#{} call.", state, service, action);
            Err(SonosError::from(state).into())
        } else {
            Ok(get_child_element(body, &format!("{}Response", action))?.clone())
        }
    }

    /// Play the current track
    pub async fn play(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Play",
            "<InstanceID>0</InstanceID><Speed>1</Speed>",
            true,
        ).await?;

        Ok(())
    }

    /// Pause the current track
    pub async fn pause(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Pause",
            "<InstanceID>0</InstanceID>",
            true,
        ).await?;

        Ok(())
    }

    /// Stop the current queue
    pub async fn stop(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Stop",
            "<InstanceID>0</InstanceID>",
            true,
        ).await?;

        Ok(())
    }

    /// Seek to a time on the current track
    pub async fn seek(&self, time: &Duration) -> Result<(), Error> {
        const SECS_PER_MINUTE: u64 = 60;
        const MINS_PER_HOUR: u64 = 60;
        const SECS_PER_HOUR: u64 = 3600;

        let seconds = time.as_secs() % SECS_PER_MINUTE;
        let minutes = (time.as_secs() / SECS_PER_MINUTE) % MINS_PER_HOUR;
        let hours = time.as_secs() / SECS_PER_HOUR;

        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Seek",
            &format!(
                "<InstanceID>0</InstanceID><Unit>REL_TIME</Unit><Target>{:02}:{:02}:{:02}</Target>",
                hours, minutes, seconds
            ),
            true,
        ).await?;

        Ok(())
    }

    /// Play the Line In connected to this Speaker
    pub async fn play_line_in(&self) -> Result<(), Error> {
        self.play_track(&format!("x-rincon-stream:{}", self.uuid)).await
    }

    /// Play the optical input connected to this Speaker
    pub async fn play_tv(&self) -> Result<(), Error> {
        self.play_track(&format!("x-sonos-htastream:{}:spdif", self.uuid)).await
    }

    /// Replace the current track with a new one
    pub async fn play_track(&self, uri: &str) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "SetAVTransportURI",
            &format!(
                r#"
                  <InstanceID>0</InstanceID>
                  <CurrentURI>{}</CurrentURI>
                  <CurrentURIMetaData></CurrentURIMetaData>"#,
                uri
            ),
            true,
        ).await?;

        Ok(())
    }

    /// Get the current volume
    pub async fn volume(&self) -> Result<u8, Error> {
        let res = self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "GetVolume",
            "<InstanceID>0</InstanceID><Channel>Master</Channel>",
            false,
        ).await?;

        Ok(get_child_element_text(&res, "CurrentVolume")?.parse::<u8>()?)
    }

    /// Set a new volume from 0-100.
    pub async fn set_volume(&self, volume: u8) -> Result<(), Error> {
        if volume > 100 {
            panic!("Volume must be between 0 and 100, got {}.", volume);
        }

        self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "SetVolume",
            &format!(
                r#"
                  <InstanceID>0</InstanceID>
                  <Channel>Master</Channel>
                  <DesiredVolume>{}</DesiredVolume>"#,
                volume
            ),
            false,
        ).await?;

        Ok(())
    }

    /// Check if this player is currently muted
    pub async fn muted(&self) -> Result<bool, Error> {
        let resp = self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "GetMute",
            "<InstanceID>0</InstanceID><Channel>Master</Channel>",
            false,
        ).await?;

        Ok(match get_child_element_text(&resp, "CurrentMute")?.as_ref() {
            "1" => true,
            "0" | _ => false,
        })
    }

    /// Mute this Speaker
    pub async fn mute(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "SetMute",
            "<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredMute>1</DesiredMute>",
            false,
        ).await?;

        Ok(())
    }

    /// Unmute this Speaker
    pub async fn unmute(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "SetMute",
            "<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredMute>0</DesiredMute>",
            false,
        ).await?;

        Ok(())
    }

    /// Get the transport state of this Speaker
    pub async fn transport_state(&self) -> Result<TransportState, Error> {
        let resp = self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "GetTransportInfo",
            "<InstanceID>0</InstanceID>",
            false,
        ).await?;

        Ok(match get_child_element_text(&resp, "CurrentTransportState")?.as_ref() {
            "PLAYING" => TransportState::Playing,
            "PAUSED_PLAYBACK" => TransportState::PausedPlayback,
            "PAUSED_RECORDING" => TransportState::PausedRecording,
            "RECORDING" => TransportState::Recording,
            "TRANSITIONING" => TransportState::Transitioning,
            "STOPPED" | _ => TransportState::Stopped,
        })
    }

    pub fn queue(&self) -> Queue {
        Queue::for_speaker(self)
    }

    /// Get information about the current track
    pub async fn track(&self) -> Result<Track, Error> {
        let resp = self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "GetPositionInfo",
            "<InstanceID>0</InstanceID>",
            true,
        ).await?;

        let metadata = get_child_element_text(&resp, "TrackMetaData")?;

        if metadata.as_ref() == "NOT_IMPLEMENTED" {
            return Err(SonosError::ParseError("track information is not supported from the current source".to_string()).into());
        }

        let metadata = Element::parse(metadata.as_bytes())?;

        let metadata = get_child_element(&metadata, "item")?;

        // convert the given hh:mm:ss to a Duration
        let mut duration = get_child_element_text(&resp, "TrackDuration")?
            .splitn(3, ':')
            .map(|s| s.parse::<u64>())
            .collect::<Vec<Result<u64, ParseIntError>>>()
            .into_iter();
        let duration = (
            duration.next().ok_or_else(|| SonosError::ParseError("invalid TrackDuration".to_string()))?,
            duration.next().ok_or_else(|| SonosError::ParseError("invalid TrackDuration".to_string()))?,
            duration.next().ok_or_else(|| SonosError::ParseError("invalid TrackDuration".to_string()))?,
        );
        let duration = Duration::from_secs((duration.0? * 3600) + (duration.1? * 60) + duration.2?);

        let mut running_time = get_child_element_text(&resp, "RelTime")?
            .splitn(3, ':')
            .map(|s| s.parse::<u64>())
            .collect::<Vec<Result<u64, ParseIntError>>>()
            .into_iter();
        let running_time = (
            running_time.next().ok_or_else(|| SonosError::ParseError("invalid RelTime".to_string()))?,
            running_time.next().ok_or_else(|| SonosError::ParseError("invalid RelTime".to_string()))?,
            running_time.next().ok_or_else(|| SonosError::ParseError("invalid RelTime".to_string()))?,
        );
        let running_time = Duration::from_secs((running_time.0? * 3600) + (running_time.1? * 60) + running_time.2?);

        Ok(Track {
            title: get_child_element_text(&metadata, "title")?.into_owned(),
            artist: get_child_element_text(&metadata, "creator")?.into_owned(),
            album: get_child_element_text(&metadata, "album").ok().map(Cow::into_owned),
            queue_position: get_child_element_text(&resp, "Track")?.parse::<u64>()?,
            uri: get_child_element_text(&resp, "TrackURI")?.into_owned(),
            duration,
            running_time,
        })
    }
}

pub struct QueueItem {
    pub position: u64,
    pub uri: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_art: String,
    pub duration: Duration,
}

pub struct Queue<'a> {
    speaker: &'a Speaker,
}
impl<'a> Queue<'a> {
    pub fn for_speaker(speaker: &'a Speaker) -> Self {
        Self {
            speaker,
        }
    }

    pub async fn list(&self) -> Result<Vec<QueueItem>, Error> {
        let res = self.speaker.soap(
            "MediaServer/ContentDirectory/Control",
            "urn:schemas-upnp-org:service:ContentDirectory:1",
            "Browse",
            r"
                <ObjectID>Q:0</ObjectID>
                <BrowseFlag>BrowseDirectChildren</BrowseFlag>
                <Filter></Filter>
                <StartingIndex>0</StartingIndex>
                <RequestedCount>1000</RequestedCount>
                <SortCriteria></SortCriteria>",
            true
        ).await?;

        let results = Element::parse(
            res.get_child("Result")
                .and_then(Element::get_text)
                .ok_or_else(|| SonosError::ParseError("missing Result element".to_string()))?
                .as_bytes()
        )?;

        let mut tracks = Vec::new();

        for child in results.children {
            if let Some(child) = child.as_element() {
                tracks.push(QueueItem {
                    position: child.attributes.get("id").cloned().unwrap_or_default().split('/').next_back().unwrap().parse().unwrap(),
                    uri: child.get_child("res")
                        .and_then(Element::get_text)
                        .map(|e| e.to_string())
                        .unwrap_or_default(),
                    title: child.get_child("title")
                        .and_then(Element::get_text)
                        .map(|e| e.to_string())
                        .unwrap_or_default(),
                    artist: child.get_child("creator")
                        .and_then(Element::get_text)
                        .map(|e| e.to_string())
                        .unwrap_or_default(),
                    album: child.get_child("album")
                        .and_then(Element::get_text)
                        .map(|e| e.to_string())
                        .unwrap_or_default(),
                    album_art: child.get_child("albumArtURI")
                        .and_then(Element::get_text)
                        .map(|e| e.to_string())
                        .unwrap_or_default(),
                    duration: {
                        let mut duration = child.get_child("res")
                            .map(|e| e.attributes.get("duration").cloned().unwrap_or_default())
                            .unwrap()
                            .splitn(3, ':')
                            .map(|s| s.parse::<u64>())
                            .collect::<Vec<Result<u64, std::num::ParseIntError>>>();
                        Duration::from_secs((duration.remove(0)? * 3600) + (duration.remove(0)? * 60) + duration.remove(0)?)
                    }
                });
            }
        }

        Ok(tracks)
    }

    /// Skip the current track
    pub async fn next(&self) -> Result<(), Error> {
        self.speaker.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Next",
            "<InstanceID>0</InstanceID>",
            true,
        ).await?;

        self.speaker.play().await?;

        Ok(())
    }

    /// Go to the previous track
    pub async fn previous(&self) -> Result<(), Error> {
        self.speaker.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Previous",
            "<InstanceID>0</InstanceID>",
            true,
        ).await?;

        self.speaker.play().await?;

        Ok(())
    }

    /// Change the track, beginning at 1
    pub async fn skip_to(&self, track: &u64) -> Result<(), Error> {
        self.speaker.play_track(&format!("x-rincon-queue:{}#0", self.speaker.uuid)).await?;

        self.speaker.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Seek",
            &format!(
                "<InstanceID>0</InstanceID><Unit>TRACK_NR</Unit><Target>{}</Target>",
                track
            ),
            true,
        ).await?;

        self.speaker.play().await?;

        Ok(())
    }

    /// Remove track at index from queue, beginning at 1
    pub async fn remove(&self, track: &u64) -> Result<(), Error> {
        self.speaker.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "RemoveTrackFromQueue",
            &format!(
                "<InstanceID>0</InstanceID><ObjectID>Q:0/{}</ObjectID>",
                track
            ),
            true,
        ).await?;

        Ok(())
    }

    /// Add a new track to the end of the queue
    pub async fn add_end(&self, uri: &str) -> Result<(), Error> {
        self.speaker.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "AddURIToQueue",
            &format!(
                r#"
                  <InstanceID>0</InstanceID>
                  <EnqueuedURI>{}</EnqueuedURI>
                  <EnqueuedURIMetaData></EnqueuedURIMetaData>
                  <DesiredFirstTrackNumberEnqueued>0</DesiredFirstTrackNumberEnqueued>
                  <EnqueueAsNext>0</EnqueueAsNext>"#,
                uri
            ),
            true,
        ).await?;

        Ok(())
    }

    /// Add a track to the queue to play next
    pub async fn add_next(&self, uri: &str) -> Result<(), Error> {
        self.speaker.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "AddURIToQueue",
            &format!(
                r#"
                  <InstanceID>0</InstanceID>
                  <EnqueuedURI>{}</EnqueuedURI>
                  <EnqueuedURIMetaData></EnqueuedURIMetaData>
                  <DesiredFirstTrackNumberEnqueued>0</DesiredFirstTrackNumberEnqueued>
                  <EnqueueAsNext>1</EnqueueAsNext>"#,
                uri
            ),
            true,
        ).await?;

        Ok(())
    }

    /// Remove every track from the queue
    pub async fn clear(&self) -> Result<(), Error> {
        self.speaker.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "RemoveAllTracksFromQueue",
            "<InstanceID>0</InstanceID>",
            true,
        ).await?;

        Ok(())
    }
}
