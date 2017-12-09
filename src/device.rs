extern crate regex;
extern crate reqwest;
extern crate xmltree;

use std::net::IpAddr;
use std::io::Read;
use std::time::Duration;
use error::*;
use self::xmltree::Element;
use self::reqwest::header::{ContentType, Headers};
use self::regex::Regex;

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
    pub album: String,
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
    Transitioning,
}


lazy_static! {
    static ref COORDINATOR_REGEX: Regex = Regex::new(r"^https?://(.+?):1400/xml")
        .expect("Failed to create regex");
}

/// Get the text of the given element as a String
fn element_to_string(el: &Element) -> String {
    el.text.to_owned().unwrap()
}

impl Speaker {
    /// Create a new instance of this struct from an IP address
    pub fn from_ip(ip: IpAddr) -> Result<Speaker> {
        let resp = reqwest::get(&format!("http://{}:1400/xml/device_description.xml", ip))
            .chain_err(|| "Failed to grab device description")?;

        if !resp.status().is_success() {
            return Err("Received a bad response from speaker".into());
        }

        let elements = Element::parse(resp).unwrap();
        let device_description = elements
            .get_child("device")
            .ok_or("The device gave us a bad response.")?;

        Ok(Speaker {
            ip,
            model: element_to_string(device_description
                .get_child("modelName")
                .ok_or("Failed to parse device description")?),
            model_number: element_to_string(device_description
                .get_child("modelNumber")
                .ok_or("Failed to parse device description")?),
            software_version: element_to_string(device_description
                .get_child("softwareVersion")
                .ok_or("Failed to parse device description")?),
            hardware_version: element_to_string(device_description
                .get_child("hardwareVersion")
                .ok_or("Failed to parse device description")?),
            serial_number: element_to_string(device_description
                .get_child("serialNum")
                .ok_or("Failed to parse device description")?),
            name: element_to_string(device_description
                .get_child("roomName")
                .ok_or("Failed to parse device description")?),
            // we slice the UDN to remove "uuid:"
            uuid: element_to_string(device_description
                .get_child("UDN")
                .ok_or("Failed to parse device description")?)[5..]
                .to_string(),
        })
    }

    /// Get the coordinator for this speaker.
    pub fn coordinator(&self) -> Result<IpAddr> {
        let mut resp = reqwest::get(&format!("http://{}:1400/status/topology", self.ip))
            .chain_err(|| "Failed to grab device description")?;

        if !resp.status().is_success() {
            return Err("Received a bad response from speaker".into());
        }

        let mut content = String::new();
        resp.read_to_string(&mut content)
            .chain_err(|| "Failed to read Sonos topology")?;

        // clean up xml so xmltree can read it
        let content = content.replace(
            "<?xml-stylesheet type=\"text/xsl\" href=\"/xml/review.xsl\"?>",
            "",
        );

        // parse the topology xml
        let elements = Element::parse(content.as_bytes()).chain_err(|| "Failed to parse xml")?;
        let zone_players = elements
            .get_child("ZonePlayers")
            .ok_or("The speaker gave us a bad response")?;

        // get the group identifier from the given player
        let group = zone_players
            .children
            .iter()
            .find(|ref child| child.attributes.get("uuid").unwrap() == &self.uuid)
            .ok_or("Failed to get device group")?
            .attributes
            .get("group")
            .unwrap();

        Ok(COORDINATOR_REGEX
            .captures(zone_players.children.iter()
                // get the coordinator for the given group
                .find(|ref child|
                    child.attributes.get("coordinator").unwrap_or(&"false".to_string()) == "true" &&
                        child.attributes.get("group").unwrap_or(&"".to_string()) == group)
                .ok_or(format!("Couldn't find coordinator for the given uuid ({})", self.uuid))?
                .attributes
                .get("location")
                .ok_or("Failed to parse coordinator URL")?)
            .ok_or("Failed to parse coordinator URL for IP")?[1]
            .parse()
            .chain_err(|| "Failed to parse IP address")?)
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
    ) -> Result<Element> {
        let mut headers = Headers::new();
        headers.set(ContentType::xml());
        headers.set_raw("SOAPAction", format!("{}#{}", service, action));

        let client = reqwest::Client::new();
        let coordinator = if coordinator { self.coordinator()? } else { self.ip };

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
            .chain_err(|| "Failed to call Sonos controller.")?;

        let element =
            Element::parse(request).chain_err(|| "Failed to parse XML from Sonos controller")?;

        Ok(
            element
                .get_child("Body")
                .ok_or("Failed to get body element")?
                .get_child(format!("{}Response", action))
                .ok_or("Failed to find response element")?
                .clone(),
        )
    }

    /// Play the current track
    pub fn play(&self) -> Result<()> {
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
    pub fn pause(&self) -> Result<()> {
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
    pub fn stop(&self) -> Result<()> {
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
    pub fn next(&self) -> Result<()> {
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
    pub fn previous(&self) -> Result<()> {
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
    pub fn seek(&self, time: &Duration) -> Result<()> {
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
                hours,
                minutes,
                seconds
            ),
            true,
        )?;

        Ok(())
    }

    /// Change the track, beginning at 1
    pub fn play_queue_item(&self, track: &u64) -> Result<()> {
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
    pub fn remove_track(&self, track: &u64) -> Result<()> {
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
    pub fn queue_track(&self, uri: &str) -> Result<()> {
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

    /// Replace the current track with a new one
    pub fn play_track(&self, uri: &str) -> Result<()> {
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
    pub fn clear_queue(&self) -> Result<()> {
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
    pub fn volume(&self) -> Result<u8> {
        let res = self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "GetVolume",
            "<InstanceID>0</InstanceID><Channel>Master</Channel>",
            false,
        )?;

        let volume = res.get_child("CurrentVolume")
            .ok_or("Failed to get current volume")?
            .text
            .to_owned()
            .ok_or("Failed to get text")?
            .parse::<u8>()
            .unwrap();

        Ok(volume)
    }

    /// Set a new volume from 0-100.
    pub fn set_volume(&self, volume: u8) -> Result<()> {
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
    pub fn muted(&self) -> Result<bool> {
        let resp = self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "GetMute",
            &format!("<InstanceID>0</InstanceID><Channel>Master</Channel>"),
            false,
        )?;

        Ok(match element_to_string(resp.get_child("CurrentMute")
            .ok_or("Failed to get current mute status")?)
            .as_str()
        {
            "1" => true,
            "0" => false,
            _ => false,
        })
    }

    /// Mute the current player
    pub fn mute(&self) -> Result<()> {
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
    pub fn unmute(&self) -> Result<()> {
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
    pub fn transport_state(&self) -> Result<TransportState> {
        let resp = self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "GetTransportInfo",
            "<InstanceID>0</InstanceID>",
            false,
        )?;

        Ok(
            match element_to_string(resp.get_child("CurrentTransportState")
                .ok_or("Failed to get current transport status")?)
                .as_str()
            {
                "STOPPED" => TransportState::Stopped,
                "PLAYING" => TransportState::Playing,
                "PAUSED_PLAYBACK" => TransportState::PausedPlayback,
                "TRANSITIONING" => TransportState::Transitioning,
                _ => TransportState::Stopped,
            },
        )
    }

    /// Get information about the current track
    pub fn track(&self) -> Result<Track> {
        let resp = self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "GetPositionInfo",
            "<InstanceID>0</InstanceID>",
            true,
        )?;

        let metadata = Element::parse(
            element_to_string(resp.get_child("TrackMetaData")
                .ok_or("Failed to get track metadata")?)
                .as_bytes(),
        ).chain_err(|| "Failed to parse XML from Sonos controller")?;

        let metadata = metadata
            .get_child("item")
            .chain_err(|| "Failed to parse XML from Sonos controller")?;

        // convert the given hh:mm:ss to a Duration
        let duration: Vec<u64> = element_to_string(resp.get_child("TrackDuration")
            .chain_err(|| "Failed to get track duration")?)
            .splitn(3, ":")
            .map(|s| s.parse::<u64>().unwrap())
            .collect();
        let duration = Duration::from_secs((duration[0] * 3600) + (duration[1] * 60) + duration[2]);

        let running_time: Vec<u64> = element_to_string(resp.get_child("RelTime")
            .chain_err(|| "Failed to get relative time")?)
            .splitn(3, ":")
            .map(|s| s.parse::<u64>().unwrap())
            .collect();
        let running_time = Duration::from_secs((running_time[0] * 3600) + (running_time[1] * 60) + running_time[2]);

        Ok(Track {
            title: element_to_string(metadata
                .get_child("title")
                .chain_err(|| "Failed to get title")?),
            artist: element_to_string(metadata
                .get_child("creator")
                .chain_err(|| "Failed to get artist")?),
            album: element_to_string(metadata
                .get_child("album")
                .chain_err(|| "Failed to get album")?),
            queue_position: element_to_string(resp.get_child("Track")
                .chain_err(|| "Failed to get queue position")?)
                .parse::<u64>()
                .unwrap(),
            uri: element_to_string(resp.get_child("TrackURI")
                .chain_err(|| "Failed to get track uri")?),
            duration,
            running_time,
        })
    }
}
