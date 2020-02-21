use std::net::IpAddr;
use std::io::Read;
use std::time::Duration;
use xmltree::{Element, XMLNode};
use reqwest::header::HeaderMap;
use regex::Regex;

use crate::error::*;
use failure::Error;

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

/// Get the text of the given element as a String
fn element_to_string(el: &Element) -> String {
    el.get_text().map(std::borrow::Cow::into_owned).unwrap_or_default()
}

impl Speaker {
    /// Create a new instance of this struct from an IP address
    pub fn from_ip(ip: IpAddr) -> Result<Speaker, Error> {
        let resp = reqwest::blocking::get(&format!("http://{}:1400/xml/device_description.xml", ip))?;

        if !resp.status().is_success() {
            return Err(SonosError::BadResponse(resp.status().as_u16()).into());
        }

        let elements = Element::parse(resp)?;
        let device_description = elements
            .get_child("device")
            .ok_or_else(|| SonosError::ParseError("missing root element"))?;

        Ok(Speaker {
            ip,
            model: element_to_string(device_description
                .get_child("modelName")
                .ok_or_else(|| SonosError::ParseError("missing model name"))?),
            model_number: element_to_string(device_description
                .get_child("modelNumber")
                .ok_or_else(|| SonosError::ParseError("missing model number"))?),
            software_version: element_to_string(device_description
                .get_child("softwareVersion")
                .ok_or_else(|| SonosError::ParseError("missing software version"))?),
            hardware_version: element_to_string(device_description
                .get_child("hardwareVersion")
                .ok_or_else(|| SonosError::ParseError("missing hardware version"))?),
            serial_number: element_to_string(device_description
                .get_child("serialNum")
                .ok_or_else(|| SonosError::ParseError("missing serial number"))?),
            name: element_to_string(device_description
                .get_child("roomName")
                .ok_or_else(|| SonosError::ParseError("missing room name"))?),
            // we slice the UDN to remove "uuid:"
            uuid: element_to_string(device_description
                .get_child("UDN")
                .ok_or_else(|| SonosError::ParseError("missing UDN"))?)[5..]
                .to_string(),
        })
    }

    /// Get the coordinator for this speaker.
    #[deprecated(note = "Broken on Sonos 9.1")]
    pub fn coordinator(&self) -> Result<IpAddr, Error> {
        let mut resp = reqwest::blocking::get(&format!("http://{}:1400/status/topology", self.ip))?;

        if !resp.status().is_success() {
            return Err(SonosError::BadResponse(resp.status().as_u16()).into());
        }

        let mut content = String::new();
        resp.read_to_string(&mut content)?;

        // clean up xml so xmltree can read it
        let content = content.replace(
            "<?xml-stylesheet type=\"text/xsl\" href=\"/xml/review.xsl\"?>",
            "",
        );

        // parse the topology xml
        let elements = Element::parse(content.as_bytes())?;

        if elements.children.is_empty() {
            // on Sonos 9.1 this API will always return an empty string in which case we'll return
            // the current speaker's IP as the 'coordinator'
            return Ok(self.ip);
        }

        let zone_players = elements
            .get_child("ZonePlayers")
            .ok_or_else(|| SonosError::ParseError("missing root element"))?;

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
            .ok_or_else(|| SonosError::ParseError("missing group identifier"))?;

        Ok(COORDINATOR_REGEX
            .captures(parent)
            .ok_or_else(|| SonosError::ParseError("couldn't parse coordinator url"))?[1]
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
    pub fn soap(
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

        let client = reqwest::blocking::Client::new();
        let coordinator = if coordinator {
            self.coordinator()?
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
            .send()?;

        let element = Element::parse(request)?;

        let body = element
            .get_child("Body")
            .ok_or_else(|| SonosError::ParseError("missing root element"))?;

        if let Some(fault) = body.get_child("Fault") {
            let error_code = element_to_string(fault
                .get_child("detail")
                .map(|c| c.get_child("UPnPError"))
                .flatten()
                .map(|c| c.get_child("errorCode"))
                .flatten()
                .ok_or_else(|| SonosError::ParseError("failed to parse error"))?)
                .parse::<u64>()?;

            let state = AVTransportError::from(error_code);
            error!("Got state {:?} from {}#{} call.", state, service, action);
            Err(SonosError::from(state).into())
        } else {
            Ok(body.get_child(format!("{}Response", action))
                .ok_or_else(|| SonosError::ParseError("failed to find root element"))?
                .clone())
        }
    }

    /// Play the current track
    pub fn play(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Play",
            "<InstanceID>0</InstanceID><Speed>1</Speed>",
            true,
        )?;

        Ok(())
    }

    /// Pause the current track
    pub fn pause(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Pause",
            "<InstanceID>0</InstanceID>",
            true,
        )?;

        Ok(())
    }

    /// Stop the current queue
    pub fn stop(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Stop",
            "<InstanceID>0</InstanceID>",
            true,
        )?;

        Ok(())
    }

    /// Skip the current track
    pub fn next(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Next",
            "<InstanceID>0</InstanceID>",
            true,
        )?;

        Ok(())
    }

    /// Go to the previous track
    pub fn previous(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Previous",
            "<InstanceID>0</InstanceID>",
            true,
        )?;

        Ok(())
    }

    /// Seek to a time on the current track
    pub fn seek(&self, time: &Duration) -> Result<(), Error> {
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
        )?;

        Ok(())
    }

    /// Change the track, beginning at 1
    pub fn play_queue_item(&self, track: &u64) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Seek",
            &format!(
                "<InstanceID>0</InstanceID><Unit>TRACK_NR</Unit><Target>{}</Target>",
                track
            ),
            true,
        )?;

        Ok(())
    }

    /// Remove track at index from queue, beginning at 1
    pub fn remove_track(&self, track: &u64) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "RemoveTrackFromQueue",
            &format!(
                "<InstanceID>0</InstanceID><ObjectID>Q:0/{}</ObjectID>",
                track
            ),
            true,
        )?;

        Ok(())
    }

    /// Add a new track to the end of the queue
    pub fn queue_track(&self, uri: &str) -> Result<(), Error> {
        self.soap(
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
        )?;

        Ok(())
    }

    /// Add a track to the queue to play next
    pub fn queue_next(&self, uri: &str) -> Result<(), Error> {
        self.soap(
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
        )?;

        Ok(())
    }

    /// Replace the current track with a new one
    pub fn play_track(&self, uri: &str) -> Result<(), Error> {
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
        )?;

        Ok(())
    }

    /// Remove every track from the queue
    pub fn clear_queue(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "RemoveAllTracksFromQueue",
            "<InstanceID>0</InstanceID>",
            true,
        )?;

        Ok(())
    }

    /// Get the current volume
    pub fn volume(&self) -> Result<u8, Error> {
        let res = self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "GetVolume",
            "<InstanceID>0</InstanceID><Channel>Master</Channel>",
            false,
        )?;

        let volume = element_to_string(res.get_child("CurrentVolume").ok_or_else(|| SonosError::ParseError("failed to find CurrentVolume element"))?)
            .parse::<u8>()?;

        Ok(volume)
    }

    /// Set a new volume from 0-100.
    pub fn set_volume(&self, volume: u8) -> Result<(), Error> {
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
        )?;
        Ok(())
    }

    /// Check if this player is currently muted
    pub fn muted(&self) -> Result<bool, Error> {
        let resp = self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "GetMute",
            "<InstanceID>0</InstanceID><Channel>Master</Channel>",
            false,
        )?;

        Ok(match element_to_string(resp.get_child("CurrentMute")
            .ok_or_else(|| SonosError::ParseError("failed to find CurrentMute element"))?)
            .as_str()
        {
            "1" => true,
            "0" | _ => false,
        })
    }

    /// Mute the current player
    pub fn mute(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "SetMute",
            "<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredMute>1</DesiredMute>",
            false,
        )?;

        Ok(())
    }

    /// Unmute the current player
    pub fn unmute(&self) -> Result<(), Error> {
        self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "SetMute",
            "<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredMute>0</DesiredMute>",
            false,
        )?;

        Ok(())
    }

    /// Get the transport state of the current player
    pub fn transport_state(&self) -> Result<TransportState, Error> {
        let resp = self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "GetTransportInfo",
            "<InstanceID>0</InstanceID>",
            false,
        )?;

        Ok(
            match element_to_string(resp.get_child("CurrentTransportState")
                .ok_or_else(|| SonosError::ParseError("failed to find CurrentTransportState element"))?)
                .as_str()
            {
                "PLAYING" => TransportState::Playing,
                "PAUSED_PLAYBACK" => TransportState::PausedPlayback,
                "PAUSED_RECORDING" => TransportState::PausedRecording,
                "RECORDING" => TransportState::Recording,
                "TRANSITIONING" => TransportState::Transitioning,
                "STOPPED" | _ => TransportState::Stopped,
            },
        )
    }

    /// Get information about the current track
    pub fn track(&self) -> Result<Track, Error> {
        let resp = self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "GetPositionInfo",
            "<InstanceID>0</InstanceID>",
            true,
        )?;

        let metadata = Element::parse(
            element_to_string(resp.get_child("TrackMetaData")
                .ok_or_else(|| SonosError::ParseError("failed to find TrackMetaData element"))?)
                .as_bytes(),
        )?;

        let metadata = metadata
            .get_child("item")
            .ok_or_else(|| SonosError::ParseError("failed to find item element"))?;

        // convert the given hh:mm:ss to a Duration
        let mut duration = element_to_string(resp.get_child("TrackDuration")
            .ok_or_else(|| SonosError::ParseError("failed to find TrackDuration element"))?)
            .splitn(3, ':')
            .map(|s| s.parse::<u64>())
            .collect::<Vec<Result<u64, std::num::ParseIntError>>>();
        let duration = Duration::from_secs((duration.remove(0)? * 3600) + (duration.remove(0)? * 60) + duration.remove(0)?);

        let mut running_time = element_to_string(resp.get_child("RelTime")
            .ok_or_else(|| SonosError::ParseError("failed to find RelTime element"))?)
            .splitn(3, ':')
            .map(|s| s.parse::<u64>())
            .collect::<Vec<Result<u64, std::num::ParseIntError>>>();
        let running_time = Duration::from_secs(
            (running_time.remove(0)? * 3600) + (running_time.remove(0)? * 60) + running_time.remove(0)?,
        );

        Ok(Track {
            title: element_to_string(metadata
                .get_child("title")
                .ok_or_else(|| SonosError::ParseError("failed to find title element"))?),
            artist: element_to_string(metadata
                .get_child("creator")
                .ok_or_else(|| SonosError::ParseError("failed to find creator element"))?),
            album: metadata.get_child("album").map(element_to_string),
            queue_position: element_to_string(resp.get_child("Track").ok_or_else(|| SonosError::ParseError("failed to find track element"))?)
                .parse::<u64>()?,
            uri: element_to_string(resp.get_child("TrackURI")
                .ok_or_else(|| SonosError::ParseError("failed to find TrackURI element"))?),
            duration,
            running_time,
        })
    }
}
