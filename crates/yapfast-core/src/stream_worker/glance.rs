//! `stream_worker`'s glances: a still of each screen the share menu asks to see,
//! and the JSON the report carries their pixels in. What can be shared at all is
//! the media crate's `source`, which this worker lists and reads the same way.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use livekit::webrtc::video_frame::I420Buffer;

use super::*;

/// Takes a picture of each of `screens`, as [`Job::Glance`] asks. A screen
/// that gives none goes without, and the rest still come.
pub(crate) fn glance(
    screens: &[u64],
    max_width: u32,
    max_height: u32,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) {
    for &id in screens {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match glance_at(id, stop) {
            Ok(Some(picture)) => {
                let (width, height) = fit(picture.width(), picture.height(), max_width, max_height);
                let mut rgba = vec![0; (width * height * 4) as usize];
                let mut scratch = Vec::new();
                planes(&picture).to_rgba(&mut scratch, &mut rgba, width, height);
                report(Report::Screen {
                    id,
                    width,
                    height,
                    rgba,
                });
            }
            Ok(None) => tracing::info!(id, "no picture of the screen came"),
            Err(error) => tracing::warn!(id, "no picture of the screen: {error:#}"),
        }
    }
}

/// A picture of the screen listed by `id`, at its own size.
pub(crate) fn glance_at(id: u64, stop: &AtomicBool) -> Result<Option<I420Buffer>> {
    use livekit::webrtc::desktop_capturer::{
        CaptureError, DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions,
    };
    let mut capturer = DesktopCapturer::new(DesktopCapturerOptions::new(
        DesktopCaptureSourceType::Screen,
    ))
    .context("Screen sharing isn't available on this system.")?;
    let source = capturer
        .get_source_list()
        .into_iter()
        .find(|listed| listed.id() == id)
        .context("That screen is gone.")?;
    // What the preview of it shows: the screen's own brightness, whether it's
    // showing HDR or not.
    let tone = picture::tone_of_screen(id);
    // The last picture taken, and whether capture stopped for good.
    let taken = Arc::new(Mutex::new((None::<I420Buffer>, false)));
    let into = taken.clone();
    capturer.start_capture(Some(source), move |result| {
        let mut taken = into.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match result {
            Ok(frame) => {
                let (width, height) = (frame.width().max(1) as u32, frame.height().max(1) as u32);
                // The last picture is the buffer for this one, unless the
                // screen changed size under it.
                let mut picture = taken
                    .0
                    .take()
                    .unwrap_or_else(|| I420Buffer::new(width, height));
                if (picture.width(), picture.height()) != (width, height) {
                    picture = I420Buffer::new(width, height);
                }
                convert(&mut picture, frame.data(), frame.stride(), &tone);
                taken.0 = Some(picture);
            }
            Err(CaptureError::Temporary) => {}
            Err(CaptureError::Permanent) => taken.1 = true,
        }
    });
    // A screen's first picture can come black, before the system has
    // drawn it for the capturer.
    let deadline = Instant::now() + GLANCE_WAIT;
    loop {
        capturer.capture_frame();
        let mut taken = taken
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let lit = taken
            .0
            .as_ref()
            .is_some_and(|picture| !is_black(picture.data().0));
        if lit || taken.1 || Instant::now() >= deadline || stop.load(Ordering::Relaxed) {
            return Ok(taken.0.take());
        }
        drop(taken);
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Pixels in a report as base64: a third bigger than they are, where JSON's
/// array of numbers would be up to four times.
pub(crate) mod base64_pixels {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(pixels: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(pixels))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        // base64 has nothing JSON escapes, so the line lends it as it is.
        let text = <&str>::deserialize(deserializer)?;
        STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}
