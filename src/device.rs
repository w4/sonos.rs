extern crate reqwest;
extern crate xmltree;

use std::net::IpAddr;
use error::*;
use self::xmltree::Element;
use self::reqwest::header::{ContentType, Headers};

#[derive(Debug)]
pub struct Device {
    pub ip: IpAddr,
    pub model: String,
    pub model_number: String,
    pub software_version: String,
    pub hardware_version: String,
    pub serial_number: String,
    pub room: Room,
}

#[derive(Debug)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub queue_position: u64,
    pub uri: String,
    pub duration: String,
    pub relative_time: String,
}

#[derive(Debug, PartialEq)]
pub struct Room {
    pub room: String,
}

#[derive(Debug, PartialEq)]
pub enum TransportState {
    Stopped,
    Playing,
    PausedPlayback,
    Transitioning,
}

impl From<String> for Room {
    fn from(str: String) -> Self {
        Room { room: str }
    }
}

impl Device {
    // Create a new instance of this struct from an IP address
    pub fn from_ip(ip: IpAddr) -> Result<Device> {
        let resp = reqwest::get(&format!("http://{}:1400/xml/device_description.xml", ip))
            .chain_err(|| "Failed to grab device description")?;

        if !resp.status().is_success() {
            return Err("Received a bad response from device".into());
        }

        let mut device = Device {
            ip,
            model: "".to_string(),
            model_number: "".to_string(),
            software_version: "".to_string(),
            hardware_version: "".to_string(),
            serial_number: "".to_string(),
            room: "".to_string().into(),
        };

        Device::parse_response(&mut device, resp);

        Ok(device)
    }

    fn element_to_string(el: &Element) -> String {
        el.text.to_owned().unwrap()
    }

    fn parse_response(device: &mut Device, r: reqwest::Response) {
        let elements = Element::parse(r).unwrap();
        let device_description = elements
            .get_child("device")
            .expect("The device gave us a bad response.");

        for el in &device_description.children {
            match el.name.as_str() {
                "modelName" => device.model = Device::element_to_string(el),
                "modelNumber" => device.model_number = Device::element_to_string(el),
                "softwareVersion" => device.software_version = Device::element_to_string(el),
                "hardwareVersion" => device.hardware_version = Device::element_to_string(el),
                "serialNum" => device.serial_number = Device::element_to_string(el),
                "roomName" => device.room = Device::element_to_string(el).into(),
                _ => {}
            }
        }
    }

    // Call the Sonos SOAP endpoint
    fn soap(&self, endpoint: &str, service: &str, action: &str, payload: &str) -> Result<Element> {
        let mut headers = Headers::new();
        headers.set(ContentType::xml());
        headers.set_raw("SOAPAction", format!("{}#{}", service, action));

        let client = reqwest::Client::new();

        let request = client
            .post(&format!("http://{}:1400/{}", self.ip, endpoint))
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

    // Play the current track
    pub fn play(&self) -> Result<()> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Play",
            "<InstanceID>0</InstanceID><Speed>1</Speed>",
        )?;

        Ok(())
    }

    // Pause the current track
    pub fn pause(&self) -> Result<()> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Pause",
            "<InstanceID>0</InstanceID>",
        )?;

        Ok(())
    }

    // Stop the current queue
    pub fn stop(&self) -> Result<()> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Stop",
            "<InstanceID>0</InstanceID>",
        )?;

        Ok(())
    }

    // Skip the current track
    pub fn next(&self) -> Result<()> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Next",
            "<InstanceID>0</InstanceID>",
        )?;

        Ok(())
    }

    // Go to the previous track
    pub fn previous(&self) -> Result<()> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Previous",
            "<InstanceID>0</InstanceID>",
        )?;

        Ok(())
    }

    // Seek to a time on the current track
    pub fn seek(&self, hours: &u8, minutes: &u8, seconds: &u8) -> Result<()> {
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
        )?;

        Ok(())
    }

    // Change the track, beginning at 1
    pub fn play_queue_item(&self, track: &u64) -> Result<()> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "Seek",
            &format!(
                "<InstanceID>0</InstanceID><Unit>TRACK_NR</Unit><Target>{}</Target>",
                track
            ),
        )?;

        Ok(())
    }

    // Remove track at index from queue, beginning at 1
    pub fn remove_track(&self, track: &u64) -> Result<()> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "RemoveTrackFromQueue",
            &format!(
                "<InstanceID>0</InstanceID><ObjectID>Q:0/{}</ObjectID>",
                track
            ),
        )?;

        Ok(())
    }

    // Add a new track to the end of the queue
    pub fn queue_track(&self, uri: &str) -> Result<()> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "RemoveTrackFromQueue",
            &format!(
                r#"
                  <InstanceID>0</InstanceID>
                  <EnqueuedURI>{}</EnqueuedURI>
                  <EnqueuedURIMetaData></EnqueuedURIMetaData>
                  <DesiredFirstTrackNumberEnqueued>0</DesiredFirstTrackNumberEnqueued>
                  <EnqueueAsNext>0</EnqueueAsNext>"#,
                uri
            ),
        )?;

        Ok(())
    }

    // Add a track to the queue to play next
    pub fn play_next(&self, uri: &str) -> Result<()> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "RemoveTrackFromQueue",
            &format!(
                r#"
                  <InstanceID>0</InstanceID>
                  <EnqueuedURI>{}</EnqueuedURI>
                  <EnqueuedURIMetaData></EnqueuedURIMetaData>
                  <DesiredFirstTrackNumberEnqueued>0</DesiredFirstTrackNumberEnqueued>
                  <EnqueueAsNext>1</EnqueueAsNext>"#,
                uri
            ),
        )?;

        Ok(())
    }

    // Replace the current track with a new one
    pub fn play_track(&self, uri: &str) -> Result<()> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "RemoveTrackFromQueue",
            &format!(
                r#"
                  <InstanceID>0</InstanceID>
                  <CurrentURI>{}</CurrentURI>
                  <CurrentURIMetaData></CurrentURIMetaData>"#,
                uri
            ),
        )?;

        Ok(())
    }

    // Remove every track from the queue
    pub fn clear_queue(&self) -> Result<()> {
        self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "RemoveAllTracksFromQueue",
            "<InstanceID>0</InstanceID>",
        )?;

        Ok(())
    }

    // Get the current volume
    pub fn volume(&self) -> Result<u8> {
        let res = self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "GetVolume",
            "<InstanceID>0</InstanceID><Channel>Master</Channel>",
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

    // Set a new volume from 0-100.
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
        )?;
        Ok(())
    }

    // Check if this player is currently muted
    pub fn muted(&self) -> Result<bool> {
        let resp = self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "GetMute",
            &format!("<InstanceID>0</InstanceID><Channel>Master</Channel>"),
        )?;

        Ok(
            match Device::element_to_string(resp.get_child("CurrentMute")
                .ok_or("Failed to get current mute status")?)
                .as_str()
            {
                "1" => true,
                "0" => false,
                _ => false,
            },
        )
    }

    // Mute the current player
    pub fn mute(&self) -> Result<()> {
        self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "SetMute",
            "<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredMute>1</DesiredMute>",
        )?;

        Ok(())
    }

    // Unmute the current player
    pub fn unmute(&self) -> Result<()> {
        self.soap(
            "MediaRenderer/RenderingControl/Control",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "SetMute",
            "<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredMute>0</DesiredMute>",
        )?;

        Ok(())
    }

    // Get the transport state of the current player
    pub fn transport_state(&self) -> Result<TransportState> {
        let resp = self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "GetTransportInfo",
            "<InstanceID>0</InstanceID>",
        )?;


        Ok(
            match Device::element_to_string(resp.get_child("CurrentTransportState")
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

    // Get information about the current track
    pub fn track(&self) -> Result<Track> {
        let resp = self.soap(
            "MediaRenderer/AVTransport/Control",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "GetPositionInfo",
            "<InstanceID>0</InstanceID>",
        )?;

        let metadata = Element::parse(
            Device::element_to_string(resp.get_child("TrackMetaData")
                .ok_or("Failed to get track metadata")?)
                .as_bytes(),
        ).chain_err(|| "Failed to parse XML from Sonos controller")?;

        let metadata = metadata
            .get_child("item")
            .chain_err(|| "Failed to parse XML from Sonos controller")?;

        Ok(Track {
            title: Device::element_to_string(metadata
                .get_child("title")
                .chain_err(|| "Failed to get title")?),
            artist: Device::element_to_string(metadata
                .get_child("creator")
                .chain_err(|| "Failed to get artist")?),
            album: Device::element_to_string(metadata
                .get_child("album")
                .chain_err(|| "Failed to get album")?),
            queue_position: Device::element_to_string(resp.get_child("Track")
                .chain_err(|| "Failed to get queue position")?)
                .parse::<u64>()
                .unwrap(),
            uri: Device::element_to_string(resp.get_child("TrackURI")
                .chain_err(|| "Failed to get track uri")?),
            duration: Device::element_to_string(resp.get_child("TrackDuration")
                .chain_err(|| "Failed to get track duration")?),
            relative_time: Device::element_to_string(resp.get_child("RelTime")
                .chain_err(|| "Failed to get relative time")?),
        })
    }
}
