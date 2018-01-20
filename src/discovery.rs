extern crate ssdp;

use self::ssdp::FieldMap;
use self::ssdp::header::{HeaderMut, HeaderRef, Man, MX, ST};
use self::ssdp::message::{Multicast, SearchRequest, SearchResponse};
pub(crate) use self::ssdp::SSDPError;
use device::Speaker;
use error::*;

const SONOS_URN: &str = "schemas-upnp-org:device:ZonePlayer:1";

/// Convenience method to grab a header from an SSDP search as a string.
fn get_header(msg: &SearchResponse, header: &str) -> Result<String> {
    let bytes = msg.get_raw(header).chain_err(|| ErrorKind::ParseError)?;

    String::from_utf8(bytes[0].clone()).chain_err(|| ErrorKind::ParseError)
}

/// Discover all speakers on the current network.
///
/// This method **will** block for 2 seconds while waiting for broadcast responses.
pub fn discover() -> Result<Vec<Speaker>> {
    let mut request = SearchRequest::new();

    request.set(Man); // required header for discovery
    request.set(MX(2)); // set maximum wait to 2 seconds
    request.set(ST::Target(FieldMap::URN(String::from(SONOS_URN)))); // we're only looking for sonos

    let mut speakers = Vec::new();

    for (msg, src) in request.multicast()? {
        let usn = get_header(&msg, "USN")?;

        if !usn.contains(SONOS_URN) {
            error!("Misbehaving client responded to our discovery ({})", usn);
            continue;
        }

        speakers.push(Speaker::from_ip(src.ip()).chain_err(|| ErrorKind::ParseError)?);
    }

    Ok(speakers)
}
