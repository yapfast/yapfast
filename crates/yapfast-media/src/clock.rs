//! Where a streamer's two clocks sit, from the sender reports it sends: the
//! sound's and the picture's, which a player puts pictures against.

use crate::lock;

use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use crate::rtcp::SenderReport;

/// The streamer's own account of its two clocks, shared by the receive
/// thread that writes it and the player that reads it: the last sender
/// report for its sound and for its picture, and the newest picture
/// timestamp that came, which is the picture the sound is compared against.
/// Reports come about once a second, so a lock costs nothing there; the
/// newest picture is one word written per packet, with no allocation at all.
#[derive(Default)]
pub struct StreamClock {
    audio: Mutex<Option<SenderReport>>,
    video: Mutex<Option<SenderReport>>,
    picture: AtomicU32,
    seen_picture: AtomicBool,
}

impl StreamClock {
    /// Where the sound's RTP timestamps sit on wall-clock time, as the
    /// streamer last said; `None` until a report on its sound SSRC comes.
    pub fn audio(&self) -> Option<SenderReport> {
        *lock(&self.audio)
    }

    /// The same for the picture's clock, which its video SSRC runs on.
    pub fn video(&self) -> Option<SenderReport> {
        *lock(&self.video)
    }

    /// The newest picture timestamp that came, on the video clock; `None`
    /// until a picture does.
    pub fn picture(&self) -> Option<u32> {
        self.seen_picture
            .load(Ordering::Acquire)
            .then(|| self.picture.load(Ordering::Relaxed))
    }

    /// Keeps the streamer's last report for its sound clock.
    pub fn heard_audio(&self, report: SenderReport) {
        *lock(&self.audio) = Some(report);
    }

    /// Keeps the streamer's last report for its picture clock.
    pub fn heard_video(&self, report: SenderReport) {
        *lock(&self.video) = Some(report);
    }

    /// Keeps `timestamp` as the newest picture that came.
    pub fn heard_picture(&self, timestamp: u32) {
        self.picture.store(timestamp, Ordering::Relaxed);
        self.seen_picture.store(true, Ordering::Release);
    }
}
