error_chain! {
    errors {
        AVTransportError(error: AVTransportError) {
            description("An error occurred from AVTransport")
            display("Received error {:?} from Sonos speaker", error)
        }

        ParseError {
            description("An error occurred when attempting to parse SOAP XML from Sonos")
            display("Failed to parse Sonos response XML")
        }

        DeviceUnreachable {
            description("An error occurred when attempting to contact the device")
            display("Failed to call Sonos endpoint")
        }

        BadResponse {
            description("The device returned a bad response")
            display("Received a non-success response from Sonos")
        }

        DeviceNotFound(identifier: String) {
            description("An error occurred when trying to find device")
            display("Couldn't find a device by the given identifier ({})", identifier)
        }
    }
}

impl From<AVTransportError> for ErrorKind {
    fn from(error: AVTransportError) -> Self {
        ErrorKind::AVTransportError(error)
    }
}

#[derive(Debug)]
pub enum AVTransportError {
    /// No action by that name at this service.
    InvalidAction = 401,
    /// Could be any of the following: not enough in args, too many in args, no in arg by that name,
    /// one or more in args are of the wrong data type.
    InvalidArgs = 402,
    /// No state variable by that name at this service.
    InvalidVar = 404,
    /// May be returned in current state of service prevents invoking that action.
    ActionFailed = 501,
    /// The immediate transition from current transport state to desired transport state is not
    /// supported by this device.
    TransitionNotAvailable = 701,
    /// The media does not contain any contents that can be played.
    NoContents = 702,
    /// The media cannot be read (e.g., because of dust or a scratch).
    ReadError = 703,
    /// The storage format of the currently loaded media is not supported
    FormatNotSupported = 704,
    /// The transport is “hold locked”.
    TransportLocked = 705,
    /// The media cannot be written (e.g., because of dust or a scratch)
    WriteError = 706,
    /// The media is write-protected or is of a not writable type.
    MediaNotWriteable = 707,
    /// The storage format of the currently loaded media is not supported for recording by this
    /// device
    RecordingFormatNotSupported = 708,
    /// There is no free space left on the loaded media
    MediaFull = 709,
    /// The specified seek mode is not supported by the device
    SeekModeNotSupported = 710,
    /// The specified seek target is not specified in terms of the seek mode, or is not present on
    /// the media
    IllegalSeekTarget = 711,
    /// The specified play mode is not supported by the device
    PlayModeNotSupported = 712,
    /// The specified record quality is not supported by the device
    RecordQualityNotSupported = 713,
    /// The resource to be played has a mimetype which is not supported by the AVTransport service
    IllegalMimeType = 714,
    /// This indicates the resource is already being played by other means
    ContentBusy = 715,
    /// The specified playback speed is not supported by the AVTransport service
    PlaySpeedNotSupported = 717,
    /// The specified instanceID is invalid for this AVTransport
    InvalidInstanceId = 718,
    /// The DNS Server is not available (HTTP error 503)
    NoDnsServer = 737,
    /// Unable to resolve the Fully Qualified Domain Name. (HTTP error 502)
    BadDomainName = 738,
    /// The server that hosts the resource is unreachable or unresponsive (HTTP error 404/410).
    ServerError = 739,
    /// Error we've not come across before
    Unknown,
}

impl From<u64> for AVTransportError {
    fn from(code: u64) -> AVTransportError {
        match code {
            401 => AVTransportError::InvalidAction,
            402 => AVTransportError::InvalidArgs,
            404 => AVTransportError::InvalidVar,
            501 => AVTransportError::ActionFailed,
            701 => AVTransportError::TransitionNotAvailable,
            702 => AVTransportError::NoContents,
            703 => AVTransportError::ReadError,
            704 => AVTransportError::FormatNotSupported,
            705 => AVTransportError::TransportLocked,
            706 => AVTransportError::WriteError,
            707 => AVTransportError::MediaNotWriteable,
            708 => AVTransportError::RecordingFormatNotSupported,
            709 => AVTransportError::MediaFull,
            710 => AVTransportError::SeekModeNotSupported,
            711 => AVTransportError::IllegalSeekTarget,
            712 => AVTransportError::PlayModeNotSupported,
            713 => AVTransportError::RecordQualityNotSupported,
            714 => AVTransportError::IllegalMimeType,
            715 => AVTransportError::ContentBusy,
            717 => AVTransportError::PlaySpeedNotSupported,
            718 => AVTransportError::InvalidInstanceId,
            737 => AVTransportError::NoDnsServer,
            738 => AVTransportError::BadDomainName,
            739 => AVTransportError::ServerError,
            _ => AVTransportError::Unknown,
        }
    }
}
