#[macro_use] extern crate log;
#[macro_use] extern crate failure;
#[macro_use] extern crate lazy_static;

mod discovery;
mod device;
mod error;

pub use device::Speaker;
pub use device::Track;
pub use device::TransportState;
pub use error::*;

pub use discovery::discover;
