extern crate soncon;

#[test]
fn can_discover_devices() {
    let devices = soncon::discover().unwrap();
    assert!(devices.len() > 0, "No devices discovered");
}

#[test]
fn volume() {
    let device = &soncon::discover().unwrap()[0];
    device.set_volume(2).expect("Failed to get volume");
    assert_eq!(
        device.volume().expect("Failed to get volume"),
        2 as u8,
        "Volume was not updated."
    );
}

#[test]
fn muted() {
    let device = &soncon::discover().unwrap()[0];
    device.mute().expect("Couldn't mute player");
    assert_eq!(
        device.muted().expect("Failed to get current mute status"),
        true
    );
    device.unmute().expect("Couldn't unmute player");
    assert_eq!(
        device.muted().expect("Failed to get current mute status"),
        false
    );
}

#[test]
fn playback_state() {
    let device = &soncon::discover().unwrap()[0];

    device.play().expect("Couldn't play track");
    assert!(match device.transport_state().unwrap() {
        soncon::TransportState::Playing => true,
        soncon::TransportState::Transitioning => true,
        _ => false,
    });

    device.pause().expect("Couldn't pause track");
    assert!(match device.transport_state().unwrap() {
        soncon::TransportState::PausedPlayback => true,
        soncon::TransportState::Transitioning => true,
        _ => false,
    });

    device.stop().expect("Couldn't stop track");
    assert!(match device.transport_state().unwrap() {
        soncon::TransportState::Stopped => true,
        soncon::TransportState::Transitioning => true,
        _ => false,
    });
}

#[test]
fn track_info() {
    let device = &soncon::discover().unwrap()[0];
    println!("{:#?}", device.track().expect("Failed to get track info"));
}

#[test]
fn play() {
    let device = &soncon::discover().unwrap()[0];
    device.play().expect("Failed to play");
    device.pause().expect("Failed to pause");
}

#[test]
#[should_panic]
fn fail_on_set_invalid_volume() {
    soncon::discover().unwrap()[0]
        .set_volume(101)
        .expect_err("Didn't fail on invalid volume");
}
