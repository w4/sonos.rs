extern crate sonos;

use sonos::TransportState;

async fn get_speaker() -> sonos::Speaker {
    let devices = sonos::discover().await.unwrap();

    devices
        .into_iter()
        .find(|d| d.name == "Living Room")
        .ok_or("Couldn't find bedroom")
        .unwrap()
}

#[tokio::test]
async fn can_discover_devices() {
    let devices = sonos::discover().await.unwrap();
    assert!(!devices.is_empty(), "No devices discovered");
}

#[tokio::test]
async fn volume() {
    let device = get_speaker().await;
    device.set_volume(2).await.expect("Failed to get volume");
    assert_eq!(
        device.volume().await.expect("Failed to get volume"),
        2 as u8,
        "Volume was not updated."
    );
}

#[tokio::test]
async fn muted() {
    let device = get_speaker().await;
    device.mute().await.expect("Couldn't mute player");
    assert_eq!(
        device
            .muted()
            .await
            .expect("Failed to get current mute status"),
        true
    );
    device.unmute().await.expect("Couldn't unmute player");
    assert_eq!(
        device
            .muted()
            .await
            .expect("Failed to get current mute status"),
        false
    );
}

#[tokio::test]
async fn playback_state() {
    let device = get_speaker().await;

    device.play().await.expect("Couldn't play track");
    assert!(match device.transport_state().await.unwrap() {
        TransportState::Playing | TransportState::Transitioning => true,
        _ => false,
    });

    device.pause().await.expect("Couldn't pause track");
    assert!(match device.transport_state().await.unwrap() {
        TransportState::PausedPlayback | TransportState::Transitioning => true,
        _ => false,
    });

    device.stop().await.expect("Couldn't stop track");
    let state = device.transport_state().await.unwrap();
    // eprintln!("{:#?}", state);
    // This returns PausedPlayback on my speaker - is stop no longer supported?
    assert!(match state {
        TransportState::Stopped | TransportState::Transitioning => true,
        _ => false,
    });
}

#[tokio::test]
async fn track_info() {
    let device = get_speaker().await;
    device.track().await.expect("Failed to get track info");
}

#[tokio::test]
async fn seek() {
    let device = get_speaker().await;
    device
        .seek(&std::time::Duration::from_secs(30))
        .await
        .expect("Failed to seek to 30 seconds");
    assert_eq!(
        device
            .track()
            .await
            .expect("Failed to get track info")
            .running_time
            .as_secs(),
        30
    );
}

#[tokio::test]
async fn play() {
    let device = get_speaker().await;
    device.play().await.expect("Failed to play");
    device.pause().await.expect("Failed to pause");
}

#[tokio::test]
#[should_panic]
async fn fail_on_set_invalid_volume() {
    get_speaker()
        .await
        .set_volume(101)
        .await
        .expect_err("Didn't fail on invalid volume");
}
