# sonos.rs

sonos.rs is a Sonos controller library written in Rust.

Example:

```rust
extern crate sonos;

let devices = sonos::discover().unwrap();

for device in devices {
    device.play();
}
```

If your rooms change you have to rediscover.