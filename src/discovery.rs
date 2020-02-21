use ssdp::FieldMap;
use ssdp::header::{HeaderMut, HeaderRef, Man, MX, ST};
use ssdp::message::{Multicast, SearchRequest, SearchResponse};

use failure::{Error, SyncFailure};

use crate::device::Speaker;
use crate::error::*;

const SONOS_URN: &str = "schemas-upnp-org:device:ZonePlayer:1";

/// Convenience method to grab a header from an SSDP search as a string.
fn get_header(msg: &SearchResponse, header: &str) -> Result<String, Error> {
    let bytes = msg.get_raw(header).ok_or_else(|| SonosError::ParseError("failed to find header"))?;

    Ok(String::from_utf8(bytes[0].clone())?)
}

/// Discover all speakers on the current network.
///
/// This method **will** block for 2 seconds while waiting for broadcast responses.
pub fn discover() -> Result<Vec<Speaker>, Error> {
    let mut request = SearchRequest::new();

    request.set(Man); // required header for discovery
    request.set(MX(2)); // set maximum wait to 2 seconds
    request.set(ST::Target(FieldMap::URN(String::from(SONOS_URN)))); // we're only looking for sonos

    let mut speakers = Vec::new();

    for (msg, src) in request.multicast().map_err(SyncFailure::new)? {
        let usn = get_header(&msg, "USN")?;

        if !usn.contains(SONOS_URN) {
            error!("Misbehaving client responded to our discovery ({})", usn);
            continue;
        }

        speakers.push(Speaker::from_ip(src.ip())?);
    }

    Ok(speakers)
}
