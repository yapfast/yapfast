//! Camera video in Discord calls, over the call's own voice connection.
//! Yapfast encodes its camera in H264 on the camera's thread and hands each
//! frame to the call. Everyone else's camera arrives as whole frames, which
//! one thread decodes into a feed made at the size its tile shows. A camera
//! whose tile is out of sight isn't asked for, and its decoder goes.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::{Duration, Instant},
};

use anyhow::Result;
use tokio::sync::Notify;
use yapfast_discord::{
    call_video::{CameraSender, VideoFrame},
    stream_connection::{Frame, SEND_CODEC},
    voice::Call,
};

use crate::{
    camera::{self, Camera},
    decode::Decoder,
    encode::{Effort, Encoder, timestamp},
    picture::{Feed, I420, Turn},
};

/// The camera's bitrate, in kbit/s: about what Discord's clients send 720p
/// at.
const BITRATE: u32 = 1500;
/// How often the decoding thread lets go of decoders for tiles gone from
/// sight.
const PRUNE: Duration = Duration::from_secs(1);
/// Discord's ladder of camera qualities, as its clients work it out from a
/// budget of 1280×720 pixels: each value asked for, and the pixels a
/// sender asked for it films at. Its bitrate follows the pixels, 2.5 Mbit/s
/// at 100 down to 264 kbit/s at 50 (416×234).
const LADDER: [(u8, u32); 9] = [
    (20, 256 * 144),
    (30, 320 * 180),
    (40, 384 * 216),
    (50, 416 * 234),
    (60, 512 * 288),
    (70, 640 * 360),
    (80, 672 * 378),
    (90, 896 * 504),
    (100, 1280 * 720),
];
/// The pixels all the cameras asked for share.
const BUDGET: u32 = 1280 * 720;

/// This account's camera, filming into the call until dropped or stopped.
pub(crate) struct Filming {
    /// What the self-view shows.
    pub feed: Feed,
    camera: Option<Camera>,
    sender: Arc<CameraSender>,
    /// The capture clock's start, the same across cameras.
    started: Instant,
}

impl Filming {
    /// Sends a camera to the call, once [`Self::open`] has opened one.
    pub fn start(call: &mut Call) -> Self {
        Self {
            feed: Feed::new(),
            camera: None,
            sender: Arc::new(call.film(camera::WIDTH, camera::HEIGHT, camera::FPS)),
            started: Instant::now(),
        }
    }

    /// Opens the camera `id`, or the first, off the call's loop, once the
    /// one filming has closed: one camera at a time. It films into the call
    /// as soon as it opens, and resolves with its first picture, for
    /// [`Self::opened`].
    pub fn open(&mut self, id: Option<&str>) -> impl Future<Output = Result<Camera>> + use<> {
        // A device that exports what it films hands each picture over as the
        // card's copy of it as well, for the encoder to read where it lies.
        // Whether this build can read one is a fact about the build, so the choice
        // needs no encoder and no device: the camera's own thread opens both, at
        // the first picture.
        #[cfg(target_os = "linux")]
        let delivery = match Self::films_on_the_card() {
            true => camera::Delivery::Card(self.card_sink()),
            false => camera::Delivery::Planes(self.sink()),
        };
        // Said once, where the way is chosen: a camera filmed on the processor is
        // not left to a silence the log cannot explain. The card's own module says
        // which way it went when it was asked.
        #[cfg(target_os = "linux")]
        if !cfg!(dmabufenc) {
            tracing::info!(
                "filming the camera on the processor: this build cannot import a picture into an encoder"
            );
        } else if !crate::encode::hardware() {
            tracing::info!("filming the camera on the processor: the GPU's encoder is off");
        }
        #[cfg(not(target_os = "linux"))]
        let delivery = camera::Delivery::Planes(self.sink());
        Camera::start_filming(self.camera.take(), id.map(str::to_owned), delivery)
    }

    /// Keeps the camera [`Self::open`] opened.
    pub fn opened(&mut self, camera: Camera) {
        self.camera = Some(camera);
    }

    /// Closes the camera, and tells the call it stopped.
    pub fn stop(mut self, call: &mut Call) {
        self.camera = None;
        self.feed.release();
        call.stop_filming();
    }

    /// Where a camera's pictures go: mirrored into the feed, and encoded
    /// into the call.
    fn sink(&self) -> camera::Sink {
        let mut sending = self.sending(false);
        Box::new(move |planes: &I420<'_>| sending.picture(&camera::Picture::of(planes)))
    }

    /// Where the pictures of a camera the card filmed go: the same, and each
    /// picture's descriptor, which the encoder reads where it lies.
    #[cfg(target_os = "linux")]
    fn card_sink(&self) -> camera::CardSink {
        let mut sending = self.sending(true);
        Box::new(move |picture: &camera::Picture<'_>| sending.picture(picture))
    }

    /// Whether a call's camera films into the card's own buffers, for an encoder
    /// to read each picture where it lies: this build imports a descriptor into
    /// an encoder, and the person asked for the GPU's encoder. Every other camera
    /// films into memory, where x264 encodes it at 720p better than the GPU's own
    /// encoder does once each picture is uploaded to it.
    #[cfg(target_os = "linux")]
    fn films_on_the_card() -> bool {
        cfg!(dmabufenc) && crate::encode::hardware()
    }

    /// What this camera's pictures go through, kept across them. The encoder
    /// opens at the first picture, on the camera's own thread.
    fn sending(&self, on_the_card: bool) -> Sending {
        Sending {
            view: Box::new(self.feed.mirrored_sink()),
            sender: self.sender.clone(),
            started: self.started,
            on_the_card,
            encoder: None,
            #[cfg(target_os = "linux")]
            in_place: on_the_card,
        }
    }
}

/// What one camera's pictures go through as they arrive, kept across them: the
/// mirror into the self-view, and the encoder the call's frames come from.
struct Sending {
    /// The self-view's feed, mirrored.
    view: camera::Sink,
    sender: Arc<CameraSender>,
    /// The capture clock's start, the same across cameras.
    started: Instant,
    /// Whether the camera films into the card's own buffers, which is what picks
    /// the encoder that reads a picture where it lies: a camera filmed in memory
    /// opens x264, which at 720p measures better than the GPU's encoder once each
    /// picture is uploaded to it.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    on_the_card: bool,
    /// Opened at the first picture's size, and again at a new one.
    encoder: Option<Encoder>,
    /// Whether the encoder reads a picture where the card holds it. Cleared for
    /// good once an encoder refuses a descriptor, so the pictures after it go by
    /// the planes that encoder reads.
    #[cfg(target_os = "linux")]
    in_place: bool,
}

impl Sending {
    /// The encoder for a camera's pictures, at `size` and a call's rate and
    /// bitrate, on the card where `hardware` asks for it.
    fn open(size: (u32, u32), hardware: bool) -> Option<Encoder> {
        Encoder::new(
            SEND_CODEC,
            size.0,
            size.1,
            camera::FPS,
            BITRATE,
            Effort::Light,
            hardware,
        )
        .inspect_err(|error| tracing::warn!(%error, "could not encode the camera"))
        .ok()
    }

    /// Mirrors a picture into the self-view, and encodes it into the call.
    fn picture(&mut self, picture: &camera::Picture<'_>) {
        let planes = picture.planes;
        (self.view)(planes);
        // The encoder takes even sizes, and starts again at a new one.
        let size = (planes.width & !1, planes.height & !1);
        let fresh = self
            .encoder
            .as_ref()
            .is_none_or(|encoder| encoder.size() != size);
        if fresh {
            // A camera the card films hands its picture over where it lies, and the
            // card's encoder reads it there; one that films into memory gets the
            // processor's, since the upload eats the card encoder's advantage at a
            // camera's size and rate (docs/measurements). A card delivery whose
            // device turned out not to export is one of these too.
            #[cfg(target_os = "linux")]
            let hardware = self.on_the_card && picture.card.is_some();
            #[cfg(not(target_os = "linux"))]
            let hardware = false;
            self.encoder = Self::open(size, hardware);
            // An encoder that uploads gives up what filming on the card is for, and
            // at a camera's size and rate the processor's encoder measures better
            // than that, so a picture on the card is encoded by one that reads it
            // there or by x264.
            #[cfg(target_os = "linux")]
            let uploading = self
                .encoder
                .as_ref()
                .filter(|encoder| picture.card.is_some() && !encoder.reads_descriptors())
                .map(|encoder| encoder.engine().to_owned());
            #[cfg(target_os = "linux")]
            if let Some(engine) = uploading {
                tracing::info!(
                    encoder = engine.as_str(),
                    "the card's encoder uploads a camera's picture, so the camera encodes on the processor"
                );
                self.encoder = Self::open(size, false);
            }
            #[cfg(target_os = "linux")]
            {
                self.in_place = self
                    .encoder
                    .as_ref()
                    .is_some_and(|encoder| encoder.reads_descriptors());
            }
        }
        let Some(encoder) = self.encoder.as_mut() else {
            return;
        };
        let keyframe = self.sender.take_keyframe_request() || fresh;
        let elapsed = self.started.elapsed();
        let pts = timestamp(elapsed);
        #[cfg(target_os = "linux")]
        let made = match picture.card {
            // A VA-API session imports the descriptor and reads the picture where
            // the card holds it, so no pixel of it comes back to the processor.
            Some(card) if self.in_place => {
                // The encoder is timed in milliseconds, and the packet with this
                // picture's time, where the two can differ by less than one.
                let time_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
                match encoder.encode_dmabuf(&card, keyframe, time_ms) {
                    Ok(made) => made,
                    // Every other encoder reads memory, which is what the planes
                    // are: this picture costs no upload, and the ones after it are
                    // handed straight to their planes.
                    Err(why) => {
                        // The card wouldn't take this picture where it lies, and this
                        // encoder reads the planes instead: the processor's copy of it,
                        // and of the pictures after it.
                        tracing::info!(
                            why = %why,
                            "the card wouldn't take the camera's picture where it lies"
                        );
                        self.in_place = false;
                        encoder.encode_picture(planes, pts, keyframe)
                    }
                }
            }
            _ => encoder.encode_picture(planes, pts, keyframe),
        };
        #[cfg(not(target_os = "linux"))]
        let made = encoder.encode_picture(planes, pts, keyframe);
        match made {
            Ok(Some(frame)) => self.sender.send(Frame {
                data: frame.data,
                codec: SEND_CODEC,
                timestamp: pts as u32,
                keyframe: frame.keyframe,
            }),
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "could not encode the camera"),
        }
    }
}

/// Everyone else's cameras, by Discord user id: a feed for each who films,
/// and what's been asked of the call for them.
pub(crate) struct Watching {
    feeds: Arc<Mutex<HashMap<u64, Feed>>>,
    /// Told when any of their tiles comes into sight, goes, or changes size.
    pub shown: Arc<Notify>,
    asked: HashMap<u64, (u8, u32)>,
}

impl Watching {
    /// Decodes the call's camera frames on a thread of their own, until the
    /// call ends.
    pub fn start(frames: Option<Receiver<VideoFrame>>) -> Self {
        let feeds = Arc::new(Mutex::new(HashMap::new()));
        if let Some(frames) = frames {
            let decoding = feeds.clone();
            if let Err(error) = std::thread::Builder::new()
                .name("yapfast-discord-video".into())
                .spawn(move || decode(&frames, &decoding))
            {
                tracing::warn!(%error, "could not start decoding cameras");
            }
        }
        Self {
            feeds,
            shown: Arc::new(Notify::new()),
            asked: HashMap::new(),
        }
    }

    /// Keeps a feed for each of `filming`, and lets go of the others'.
    pub fn film(&mut self, filming: &[u64]) {
        let mut feeds = lock(&self.feeds);
        feeds.retain(|user, feed| {
            let kept = filming.contains(user);
            if !kept {
                feed.release();
            }
            kept
        });
        for &user in filming {
            feeds
                .entry(user)
                .or_insert_with(|| Feed::watched(self.shown.clone()));
        }
    }

    /// The feed of `user`'s camera, while they film.
    pub fn feed(&self, user: u64) -> Option<Feed> {
        lock(&self.feeds).get(&user).cloned()
    }

    /// Asks the call for each camera in sight as Discord's desktop app
    /// does, with `others` in the call besides this account, and for none
    /// out of sight, when that changed.
    pub fn ask(&mut self, call: &mut Call, others: usize) {
        let wants = wants(&lock(&self.feeds), others);
        if wants != self.asked {
            self.asked = wants.clone();
            call.want_video(wants);
        }
    }
}

/// The quality and pixels asked for each camera in sight: by its tile's
/// size in physical pixels, which the UI gives at the window's scale,
/// whichever way the camera's frames come. A phone's picture, turned
/// upright, shows narrower than its tile, so the tile's pixels are at least
/// what it shows at, never the smaller count of a picture on its side.
fn wants(feeds: &HashMap<u64, Feed>, others: usize) -> HashMap<u64, (u8, u32)> {
    feeds
        .iter()
        .filter_map(|(&user, feed)| {
            let (width, height) = feed.shown_at()?;
            let pixels = width * height;
            let quality = quality(feeds.len(), others, feed.shown_large(), pixels);
            Some((user, (quality, pixels)))
        })
        .collect()
}

/// The quality Discord's desktop app asks for a camera when `cameras` film
/// and `others` are in the call: the tile in focus at 100; in a call with
/// one other, the step that fills its tile of `pixels`; and otherwise the
/// largest step all the cameras fit in the budget at.
fn quality(cameras: usize, others: usize, large: bool, pixels: u32) -> u8 {
    let level = step(cameras, 0);
    if large && level != 100 {
        100
    } else if others == 1 {
        step(cameras, pixels)
    } else {
        level
    }
}

/// The ladder's step for a tile of `pixels`, or of any size for 0: the
/// first larger than the tile, or failing one, the largest at which all of
/// `cameras` fit in the budget. Discord's clients look at the tile first,
/// so it can go one step past that.
fn step(cameras: usize, pixels: u32) -> u8 {
    let mut step = LADDER[0].0;
    for (value, rung) in LADDER {
        if pixels > 0 && pixels < rung {
            return value;
        }
        if u64::from(rung) * cameras as u64 > u64::from(BUDGET) {
            break;
        }
        step = value;
    }
    step
}

/// Decodes each camera's frames into its feed, at the size it shows. A
/// camera gets a decoder when its tile is in sight, from a keyframe, and
/// loses it when the tile goes.
fn decode(frames: &Receiver<VideoFrame>, feeds: &Mutex<HashMap<u64, Feed>>) {
    // Each camera's decoder, the feed it writes, and the sink writing it.
    type Sink = Box<dyn FnMut(&I420<'_>, Turn) + Send>;
    let mut decoders: HashMap<u64, (Decoder, Feed, Sink)> = HashMap::new();
    let mut pruned = Instant::now();
    loop {
        match frames.recv_timeout(PRUNE) {
            Ok(frame) => {
                let feed = lock(feeds)
                    .get(&frame.user)
                    .filter(|feed| feed.shown_at().is_some())
                    .cloned();
                let Some(feed) = feed else {
                    decoders.remove(&frame.user);
                    continue;
                };
                let current = decoders
                    .get(&frame.user)
                    .is_some_and(|(decoder, shown, _)| {
                        decoder.codec() == frame.codec && *shown == feed
                    });
                if !current {
                    decoders.remove(&frame.user);
                    // A decoder starts from a keyframe.
                    if !frame.keyframe {
                        continue;
                    }
                    // One thread each: a tile is small, and every thread
                    // keeps frames of its own.
                    match Decoder::with_threads(frame.codec, 1) {
                        Ok(decoder) => {
                            let sink = Box::new(feed.turning_sink(false));
                            decoders.insert(frame.user, (decoder, feed, sink));
                        }
                        Err(error) => {
                            tracing::warn!(%error, "could not decode a camera");
                            continue;
                        }
                    }
                }
                let Some((decoder, _, sink)) = decoders.get_mut(&frame.user) else {
                    continue;
                };
                // A phone films on its side, and says how to turn it.
                let turn = Turn {
                    turns: frame.orientation.turns,
                    flip: frame.orientation.flip,
                };
                match decoder.decode(&frame.data) {
                    Ok(Some(picture)) => sink(&picture.i420(), turn),
                    Ok(None) => {}
                    Err(error) => {
                        tracing::debug!(%error, "dropped a camera's decoder");
                        decoders.remove(&frame.user);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if pruned.elapsed() >= PRUNE {
            pruned = Instant::now();
            let feeds = lock(feeds);
            decoders.retain(|user, (_, shown, _)| {
                feeds
                    .get(user)
                    .is_some_and(|feed| feed == shown && feed.shown_at().is_some())
            });
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Which way this build films a call's camera, and which encoder it opens for
    /// it: a report of the build and of this machine, rather than a rule.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "opens this build's encoder, which is a report rather than a rule"]
    fn the_camera_films_where_this_build_reads_it() {
        let on_the_card = Filming::films_on_the_card();
        let opened = Encoder::new(
            SEND_CODEC,
            camera::WIDTH & !1,
            camera::HEIGHT & !1,
            camera::FPS,
            BITRATE,
            Effort::Light,
            on_the_card,
        );
        println!(
            "this build imports a descriptor into an encoder: {}; the camera films on the card: {on_the_card}; an encoder opened: {}",
            cfg!(dmabufenc),
            opened.is_ok(),
        );
    }

    #[test]
    fn cameras_are_asked_for_as_discords_desktop_app_asks() {
        // A group call: by how many film, whatever the tile.
        for (cameras, want) in [(1, 100), (2, 90), (3, 80), (4, 70), (5, 60), (7, 50)] {
            assert_eq!(
                quality(cameras, 3, false, 420 * 236),
                want,
                "{cameras} cameras"
            );
        }
        // The tile in focus comes in full, unless all do.
        assert_eq!(quality(3, 3, true, 420 * 236), 100);
        // With one other, the step past the tile, as far as the cameras fit.
        assert_eq!(quality(1, 1, false, 420 * 236), 60);
        assert_eq!(quality(1, 1, false, 1280 * 720), 100);
        assert_eq!(quality(2, 1, false, 1280 * 720), 90);
        assert_eq!(
            quality(2, 1, false, 500_000),
            100,
            "Discord looks at the tile first"
        );
        assert_eq!(
            quality(1, 1, true, 320 * 180),
            40,
            "one camera goes by its tile, in focus too"
        );
    }

    /// A phone's camera, turned upright in its tile, is asked for by the
    /// whole tile in physical pixels, as a camera filming the right way up.
    #[test]
    fn a_turned_camera_is_asked_for_by_its_tile() {
        let feed = Feed::new();
        // A 400 by 225 point tile at a scale of 1.5.
        feed.show_at(Some((600, 338)));
        let mut sink = feed.turning_sink(false);
        // 8 by 4, sent on its side: upright it's 4 by 8.
        let (y, u, v) = ([128u8; 32], [128u8; 8], [128u8; 8]);
        let picture = I420 {
            width: 8,
            height: 4,
            y: &y,
            u: &u,
            v: &v,
            strides: [8, 4, 4],
        };
        sink(
            &picture,
            Turn {
                turns: 1,
                flip: false,
            },
        );
        assert_eq!(feed.take(|buffer| buffer.size()), Some((4, 8)), "upright");
        assert_eq!(
            feed.size_for(720, 1280),
            Some((190, 338)),
            "fits the tile's height"
        );
        let feeds = HashMap::from([(7, feed)]);
        assert_eq!(
            wants(&feeds, 1)[&7],
            (quality(1, 1, false, 600 * 338), 600 * 338),
            "the tile's pixels"
        );
    }
}
