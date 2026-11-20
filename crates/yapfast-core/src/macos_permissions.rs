//! What macOS lets Yapfast use, and asking for it: the microphone, the camera
//! and Screen Recording. The first start asks for all of them in turn
//! (`src/permissions.rs`); the features themselves ask again when one is
//! missing.

use std::{sync::mpsc, time::Duration};

use objc2::runtime::Bool;
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureDevice, AVMediaType, AVMediaTypeAudio, AVMediaTypeVideo,
};
use objc2_core_graphics::{CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};

/// What the system knows of a permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The person was not asked yet, or the system can't say (Screen Recording
    /// only tells whether it is allowed). Asking is right.
    NotDetermined,
    Granted,
    /// Refused, or restricted by the system: asking again shows nothing.
    Denied,
}

/// What kind of capture device a permission is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Media {
    Microphone,
    Camera,
}

/// The status behind AVFoundation's authorization value.
pub fn status_from(status: AVAuthorizationStatus) -> Status {
    match status {
        AVAuthorizationStatus::Authorized => Status::Granted,
        AVAuthorizationStatus::NotDetermined => Status::NotDetermined,
        _ => Status::Denied,
    }
}

/// The status behind Screen Recording's yes or no.
pub fn status_from_screen_access(allowed: bool) -> Status {
    if allowed {
        Status::Granted
    } else {
        Status::NotDetermined
    }
}

fn media_type(media: Media) -> Option<&'static AVMediaType> {
    // SAFETY: these are constants of AVFoundation, read once it's loaded.
    unsafe {
        match media {
            Media::Microphone => AVMediaTypeAudio,
            Media::Camera => AVMediaTypeVideo,
        }
    }
}

/// Whether the microphone or the camera may be used, without asking.
pub fn media_status(media: Media) -> Status {
    let Some(kind) = media_type(media) else {
        tracing::error!(?media, "AVFoundation has no such media type");
        return Status::Denied;
    };
    // SAFETY: a plain class method with a valid media type.
    status_from(unsafe { AVCaptureDevice::authorizationStatusForMediaType(kind) })
}

/// Shows the system's prompt for the microphone or the camera and waits up to
/// `timeout` for the answer.
pub fn request_media(media: Media, timeout: Duration) -> Status {
    let Some(kind) = media_type(media) else {
        tracing::error!(?media, "AVFoundation has no such media type");
        return Status::Denied;
    };
    let (answer, answered) = mpsc::sync_channel::<bool>(1);
    let handler = block2::RcBlock::new(move |granted: Bool| {
        let _ = answer.try_send(granted.as_bool());
    });
    // SAFETY: the handler is copied by the call and may run on any queue.
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(kind, &handler) };
    match answered.recv_timeout(timeout) {
        Ok(true) => Status::Granted,
        Ok(false) => Status::Denied,
        Err(_) => {
            tracing::warn!(?media, "no answer to the permission prompt in time");
            Status::NotDetermined
        }
    }
}

/// Whether Yapfast may record the screen, without asking.
pub fn screen_recording_status() -> Status {
    status_from_screen_access(CGPreflightScreenCaptureAccess())
}

/// Shows the system's Screen Recording prompt. The call returns at once with
/// whether it was already allowed; the answer arrives later and counts from
/// the next start.
pub fn request_screen_recording() -> bool {
    CGRequestScreenCaptureAccess()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_values_map_to_statuses() {
        assert_eq!(
            status_from(AVAuthorizationStatus::Authorized),
            Status::Granted
        );
        assert_eq!(
            status_from(AVAuthorizationStatus::NotDetermined),
            Status::NotDetermined
        );
        assert_eq!(status_from(AVAuthorizationStatus::Denied), Status::Denied);
        assert_eq!(
            status_from(AVAuthorizationStatus::Restricted),
            Status::Denied
        );
    }

    #[test]
    fn screen_recording_is_asked_for_until_it_is_allowed() {
        assert_eq!(status_from_screen_access(true), Status::Granted);
        assert_eq!(status_from_screen_access(false), Status::NotDetermined);
    }
}
