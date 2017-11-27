#![cfg_attr(feature = "clippy", feature(plugin))]
#![cfg_attr(feature = "clippy", plugin(clippy))]

#[macro_use]
extern crate log;

#[macro_use]
extern crate error_chain;

mod discovery;
mod device;
mod error;

pub use device::Device;
pub use device::TransportState;
pub use error::*;

pub fn discover() -> Result<Vec<Device>> {
    discovery::discover()
}
