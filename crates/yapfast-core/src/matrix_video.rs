//! Camera video in Matrix calls, over LiveKit. Yapfast publishes its camera as a
//! simulcast video track, and shows each remote camera through a feed made
//! at its tile's size. A camera whose tile is out of sight is paused at the
//! server, so nothing arrives to decode.

use std::{collections::HashMap, sync::Arc, time::Instant};

use anyhow::{Context, Result};
use futures_util::StreamExt;
use livekit::{
    options::TrackPublishOptions,
    prelude::{LocalTrack, LocalVideoTrack, RemoteVideoTrack, TrackSource},
    webrtc::{
        video_frame::{I420Buffer, VideoBuffer, VideoFrame, VideoRotation},
        video_source::{RtcVideoSource, VideoResolution, native::NativeVideoSource},
        video_stream::native::NativeVideoStream,
    },
};
use matrix_rtc_livekit::Call;
use matrix_rtc_media::{Dimensions, MediaConstraints, MediaStreamKind, VideoDetail};
use tokio::{sync::Notify, task::JoinHandle};

use crate::{
    camera::{self, Camera},
    picture::{Feed, I420},
};

/// The native side of a camera's video source, which copies each picture
/// into a pooled buffer.
type Source = cxx::SharedPtr<webrtc_sys::video_track::ffi::VideoTrackSource>;

/// This account's camera track, published once per call. Turning the camera
/// off mutes the track and closes the camera, rather than unpublishing it:
/// LiveKit leaves an unpublished track's sender, and its encoder, in the
/// connection, and publishing again adds another (about 17 MiB and nine
/// threads each time, measured). Element mutes its camera the same way.
pub(crate) struct Filming {
    /// What the self-view shows.
    pub feed: Feed,
    source: Source,
    track: LocalVideoTrack,
    camera: Option<Camera>,
    /// The track is in the call, muted or not.
    published: bool,
    /// The capture clock's start, the same across cameras.
    started: Instant,
}

impl Filming {
    /// Makes the camera's track, published once [`Self::open`] has opened
    /// a camera.
    pub fn new() -> Result<Self> {
        let video = NativeVideoSource::new(
            VideoResolution {
                width: camera::WIDTH,
                height: camera::HEIGHT,
            },
            false,
        );
        // Until a frame comes through its own `capture_frame`, the source
        // sends black ones from a task of its own; this one ends that task.
        // No track holds the source yet, so it goes nowhere.
        video.capture_frame(&VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: 0,
            frame_metadata: None,
            buffer: I420Buffer::new(2, 2),
        });
        // Made just now on this thread, so this is the same source.
        let source = webrtc_sys::video_track::ffi::last_video_track_source();
        anyhow::ensure!(!source.is_null(), "Could not start your camera's video.");
        let track =
            LocalVideoTrack::create_video_track("Yapfast camera", RtcVideoSource::Native(video));
        Ok(Self {
            feed: Feed::new(),
            source,
            track,
            camera: None,
            published: false,
            started: Instant::now(),
        })
    }

    /// Opens the camera `id`, or the first, off the call's loop, once the
    /// one filming has closed: one camera at a time. It films into the
    /// track as soon as it opens, and resolves with its first picture, for
    /// [`Self::opened`].
    pub fn open(&mut self, id: Option<&str>) -> impl Future<Output = Result<Camera>> + use<> {
        Camera::start(self.camera.take(), id.map(str::to_owned), self.sink())
    }

    /// Keeps the camera [`Self::open`] opened, and sends the track: published
    /// the first time, and unmuted after.
    pub async fn opened(&mut self, call: &Call, camera: Camera) -> Result<()> {
        self.camera = Some(camera);
        if self.published {
            self.track.unmute();
            return Ok(());
        }
        call.session()
            .room()
            .local_participant()
            .publish_track(
                LocalTrack::Video(self.track.clone()),
                TrackPublishOptions {
                    source: TrackSource::Camera,
                    simulcast: true,
                    ..Default::default()
                },
            )
            .await
            .context("Could not send your camera. Check your connection and try again.")?;
        self.published = true;
        // Each published track starts at key index 0; move it onto the
        // current one, as the microphone does.
        call.reassert_local_key_index();
        Ok(())
    }

    /// Whether a camera films, rather than none or one opening.
    #[cfg(all(test, target_os = "linux"))]
    pub fn filming(&self) -> bool {
        self.camera.is_some()
    }

    /// Closes the camera, mutes the track, and lets the self-view's and the
    /// source's buffers go.
    pub fn pause(&mut self) {
        self.track.mute();
        self.camera = None;
        self.feed.release();
        self.source.release_buffers();
    }

    /// Where a camera's pictures go: mirrored into the feed, and into the
    /// track.
    fn sink(&self) -> camera::Sink {
        let mut view = self.feed.mirrored_sink();
        let (source, started) = (self.source.clone(), self.started);
        Box::new(move |picture: &I420<'_>| {
            view(picture);
            let [stride_y, stride_u, stride_v] = picture.strides.map(|stride| stride as i32);
            // SAFETY: the planes hold the picture's rows at these strides,
            // and the source copies them before returning.
            unsafe {
                source.capture_i420(
                    picture.y.as_ptr(),
                    stride_y,
                    picture.u.as_ptr(),
                    stride_u,
                    picture.v.as_ptr(),
                    stride_v,
                    picture.width as i32,
                    picture.height as i32,
                    started.elapsed().as_micros() as i64,
                );
            }
        })
    }
}

/// The cameras of everyone else in the call, by their LiveKit identity.
#[derive(Default)]
pub(crate) struct Watching {
    cameras: HashMap<String, (Feed, JoinHandle<()>)>,
    /// Told when any of their tiles comes into sight, goes, or changes size.
    pub shown: Arc<Notify>,
}

impl Watching {
    /// Shows `identity`'s newly subscribed camera through a feed of its own,
    /// once [`Self::constrain_all`] asks for it.
    pub fn subscribed(&mut self, identity: String, track: &RemoteVideoTrack) {
        let feed = Feed::watched(self.shown.clone());
        let mut sink = feed.sink();
        let mut frames = NativeVideoStream::new(track.rtc_track());
        let receiving = tokio::spawn(async move {
            while let Some(frame) = frames.next().await {
                // Decoders give I420 already; this takes it without a copy.
                let buffer = frame.buffer.to_i420();
                let (y, u, v) = buffer.data();
                let (stride_y, stride_u, stride_v) = buffer.strides();
                sink(&I420 {
                    width: buffer.width(),
                    height: buffer.height(),
                    y,
                    u,
                    v,
                    strides: [stride_y, stride_u, stride_v],
                });
            }
        });
        if let Some((_, old)) = self.cameras.insert(identity, (feed, receiving)) {
            old.abort();
        }
    }

    /// Lets go of `identity`'s camera, its decoder and its buffers.
    pub fn unsubscribed(&mut self, identity: &str) {
        if let Some((feed, receiving)) = self.cameras.remove(identity) {
            receiving.abort();
            feed.release();
        }
    }

    pub fn feed(&self, identity: &str) -> Option<&Feed> {
        self.cameras.get(identity).map(|(feed, _)| feed)
    }

    /// Asks the server for each camera at the size its tile shows, and
    /// pauses those out of sight. `members` maps identities to member ids.
    pub fn constrain_all(&self, call: &Call, members: &HashMap<String, String>) {
        for (identity, (feed, _)) in &self.cameras {
            if let Some(member) = members.get(identity) {
                constrain(call, member, feed);
            }
        }
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        for (_, receiving) in self.cameras.values() {
            receiving.abort();
        }
    }
}

fn constrain(call: &Call, member: &str, feed: &Feed) {
    let shown = feed.shown_at();
    call.set_constraints(
        member,
        MediaStreamKind::Camera,
        MediaConstraints {
            visible: shown.is_some(),
            detail: shown.map_or(VideoDetail::Auto, |(width, height)| {
                VideoDetail::Dimensions(Dimensions { width, height })
            }),
            ..MediaConstraints::default()
        },
    );
}
