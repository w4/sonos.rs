#![cfg_attr(feature = "clippy", feature(plugin))]
#![cfg_attr(feature = "clippy", plugin(clippy))]

#[macro_use]
extern crate log;

#[macro_use]
extern crate error_chain;

#[macro_use]
extern crate lazy_static;

mod discovery;
mod device;
mod error;

pub use device::Speaker;
pub use device::Track;
pub use device::TransportState;
pub use error::*;

/// Discover devices.
///
/// You should only run this function once. It will block for
/// 2 seconds while it scans.
pub fn discover() -> Result<Vec<Speaker>> {
    discovery::discover()
}
