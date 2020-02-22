use crate::device::Speaker;

use std::time::Duration;
use regex::Regex;

use ssdp_client::URN;
use failure::Error;

use futures::prelude::*;

lazy_static! {
    static ref LOCATION_REGEX: Regex = Regex::new(r"^https?://(.+?):1400/xml")
        .expect("Failed to create regex");
}

/// Discover all speakers on the current network.
///
/// This method **will** block for 2 seconds while waiting for broadcast responses.
pub async fn discover() -> Result<Vec<Speaker>, Error> {
    let search_target = URN::device("schemas-upnp-org", "ZonePlayer", 1).into();
    let timeout = Duration::from_secs(2);
    let responses = ssdp_client::search(&search_target, timeout, 1).await?;
    futures::pin_mut!(responses);

    let mut speakers = Vec::new();

    while let Some(response) = responses.next().await {
        let response = response?;

        if let Some(ip) = LOCATION_REGEX.captures(response.location()).and_then(|x| x.get(1)).map(|x| x.as_str()) {
            speakers.push(Speaker::from_ip(ip.parse()?).await?);
        }
    }

    Ok(speakers)
}
