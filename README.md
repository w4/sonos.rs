# sonos.rs

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
