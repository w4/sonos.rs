# sonos.rs

[![License](https://img.shields.io/github/license/w4/reaper.svg?style=flat-square)](https://github.com/w4/sonos.rs) [![Downloads](https://img.shields.io/crates/d/sonos.svg?style=flat-square)](https://crates.io/crates/sonos) [![Version](https://img.shields.io/crates/v/sonos.svg?style=flat-square)](https://crates.io/crates/sonos)

sonos.rs is a Sonos controller library written in Rust. Currently it only supports playback operations (play,
pause, stop, skip, add track to queue, remove track from queue) with no support for search operations as of yet.

Example:

```rust
extern crate sonos;

let devices = sonos::discover().unwrap();
let bedroom = devices.iter()
    .find(|d| d.name == "Bedroom")
    .ok_or("Couldn't find bedroom")
    .unwrap();

let track = bedroom.track().unwrap();
let volume = bedroom.volume().unwrap();

bedroom.play();
println!("Now playing {} - {} at {}% volume.", track.title, track.artist, volume);
```
