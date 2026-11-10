//! A Go Live stream's media connection, to the server Discord assigned the
//! stream: sending this account's stream, or receiving someone else's. It's
//! a voice connection of its own (`voice_connection.rs`), identifying with
//! the stream's RTC server and video. Yapfast sends H264 and receives VP8 or
//! H264.

use std::{
    collections::HashMap,
    ops::Deref,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
        mpsc as channel,
    },
    time::Duration,
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tokio::{
    sync::{mpsc, oneshot},
    time::Instant,
};

use crate::{
    congestion::{self, Controller},
    rtp::{self, Assembler, Codec, Packetizer},
    stream::{self, StreamServer},
    voice_connection::{Connection, Identity, lock, receive, send_json},
};

/// How often a stream asks for a keyframe while frames are lost, and how
/// often it makes one when asked: at most one a half second, whichever side
/// of the stream it is.
const KEYFRAME_REQUESTS: Duration = Duration::from_millis(500);
/// How often a streamer sends an RTCP sender report.
const SENDER_REPORTS: Duration = Duration::from_secs(1);
/// How often a viewer logs what it received while no picture comes through.
const LOG_EVERY: Duration = Duration::from_secs(5);
/// The most packets resent for one NACK, which can name hundreds.
const RESENDS: usize = 128;
/// The most buffers the stream's sound is queued in, as many as the queue
/// holds.
const SOUND_BUFFERS: usize = 8;
/// The most video SSRCs a watched stream puts frames together for.
const MAX_SSRCS: usize = 4;
/// The sender reports the last round trips are measured against, by the
/// middle of their NTP time, which a viewer's receiver report echoes: more
/// than one report can wait unanswered, and fewer than a report a second over
/// a few seconds.
const ECHOED: usize = 4;
/// A frame's packets past the first `BURST` go out `PACE` every `STEP`,
/// about 37 Mbit/s: a 1080p keyframe sent at once can overflow buffers on
/// the way.
const BURST: usize = 8;
const PACE: usize = 4;
/// How long a batch past the first waits after the one before it. A batch is
/// a turn of the send loop, so sound, the gateway and viewers' feedback are
/// polled between them rather than after the whole frame.
const STEP: Duration = Duration::from_millis(1);

/// The most bit/s a stream of `width` by `height` at `fps` may take: what the
/// picture needs, which is bits per pixel per frame, measured here.
///
/// A share's quality is bits per pixel per frame and nothing else, so the
/// declaration follows the picture rather than a step at 720 lines:
/// [`BITS_PER_PIXEL`] times the pixels times the frames a second, floored at
/// what a small picture is worth and capped at [`MOST_SAFE`] — or at
/// [`MOST_EXPERIMENTAL`], which is what 3840x2160 at 60 is worth, with
/// Settings' switch on. 1080p60 declares 8.7 Mbit/s, which the measurements
/// below put at the same quality; a 3440x1440 share at 60 asks for 20.8 and is
/// declared 9 unless the switch raises the cap.
///
/// Measured here, four-second clips at the p4 low-latency setting the stream
/// uses, VMAF against the source: busy 3840x2160@60 through AV1 scores 92.9 at
/// 15 Mbit/s, 94.2 at 25, 95.2 at 35 and 96.5 at 50; HEVC 91.8/93.8/95.0/96.5
/// and H.264 91.8/94.1/94.9/96.2 at the same rates. Busy 3440x1440@60 through
/// AV1 scores 93.7 at 9 Mbit/s, 95.3 at 20 and 96.9 at 35. So 0.07 is where
/// busy content reaches VMAF 95 at every size, and it declares 20.8 Mbit/s for
/// the 3440x1440 case the measurement agrees with — a rate only the switch
/// below lets a stream ask for.
///
/// A screen share is far cheaper than that: a desktop at 3840x2160@60 scores
/// VMAF 98 on 2.4 Mbit/s and one at 3440x1440@60 scores 98 on 5.9, so a share
/// of a screen spends what it needs and leaves the rest of its cap unspent.
///
/// Discord's server takes the declaration on trust: a probe through a real
/// stream accepted 3840x2160 at 60, 120 and even 200 Mbit/s declared, and
/// 7680x4320@60, without refusing any of it. The declaration is therefore ours
/// to choose. It's the ceiling the controller in [`congestion`] works under:
/// the rate a stream actually sends at is that controller's to say.
///
/// Neither number is a limit Discord states. The reference documents
/// `max_bitrate` only as an optional integer, "the maximum bitrate to send in
/// bps", footnoted "not sent by the voice server", and gives no maximum,
/// default or tiers; its one value is the example Video (Send) payload's
/// `9000000` at 60 fps (docs.discord.food, Voice Connections, Stream
/// Structure). So op 12 announces a cap and the voice server takes it on trust:
/// a path that can't carry it is held back by the sender's own congestion
/// control instead, which reads the round trip and the loss back out of the
/// server's receiver reports (see [`congestion`]).
///
/// Discord's own web client is far more modest than this: its screen-share
/// ladder is `desktopBitrate {min: 500_000, max: 3_500_000, target: 600_000}`,
/// a Go Live stream opens at 1280x720 with those bounds, and the overrides it
/// computes per resolution are 3.5 or 5 Mbit/s at 1080p30 (the
/// `2026-05-frontier-tuning` guild trial) and 9 Mbit/s for 720p60 in a tier-1
/// guild under `2026-06-server-ladder`. So even [`MOST_SAFE`] is above what the
/// client it copies would use, and that is the direction to err in: a 1080p
/// share at 3.5 Mbit/s is soft, and the congestion control holds the rate below
/// whatever is declared anyway.
pub fn max_bitrate(width: u32, height: u32, fps: u32) -> u32 {
    if let Some(bits) = declared() {
        return bits;
    }
    let pixels = f64::from(width) * f64::from(height) * f64::from(fps);
    let most = if HIGH_BITRATE.load(Ordering::Relaxed) {
        MOST_EXPERIMENTAL
    } else {
        MOST_SAFE
    };
    ((pixels * BITS_PER_PIXEL) as u64).clamp(u64::from(LEAST), u64::from(most)) as u32
}

/// Whether a stream may declare more than [`MOST_SAFE`]: Settings' switch, off
/// until the UI says otherwise. It's read on every declaration rather than kept,
/// so a share started after the switch is flipped declares the new ceiling.
static HIGH_BITRATE: AtomicBool = AtomicBool::new(false);

/// Raises what a stream declares its picture worth to 35 Mbit/s, from 9.
///
/// The worker is a process of its own and op 12 carries the ceiling once, when
/// a share starts, so this has to be set in the process that declares the
/// stream: a choice made in Yapfast's own means nothing where the stream is
/// actually sent from. A stream already running declared its own ceiling when it
/// started and keeps it; the next one declares this.
pub fn set_high_bitrate(on: bool) {
    HIGH_BITRATE.store(on, Ordering::Relaxed);
}

/// Bits per pixel per frame a share is worth: measured, see [`max_bitrate`].
const BITS_PER_PIXEL: f64 = 0.07;
/// The least a share is declared, so a small or still picture isn't starved by
/// a formula: a screen's own floor is 500 kbit/s, and this is above it.
const LEAST: u32 = 3_500_000;
/// The most a stream declares by default, which is above 1080p60's 8.7
/// Mbit/s: Discord's own web client asks for far less — 5 Mbit/s at 1080p30
/// and 9 at 720p60 — and what lies past this is unmeasured against a real
/// viewer's machine and shared with everything else on the uplink, which is why
/// raising it is a setting rather than the default.
const MOST_SAFE: u32 = 9_000_000;
/// And the most a stream may declare with Settings' switch on: 3840x2160 at 60
/// through [`BITS_PER_PIXEL`], the largest share the interface offers, which is
/// far past what Discord Web declares. A probe accepted more than this from
/// Discord's server, so it's the path's to carry or not, and the measurement
/// that makes it worth asking for is above.
const MOST_EXPERIMENTAL: u32 = 35_000_000;

/// The bit/s a stream declares when a measurement overrides the cap, from
/// `YAPFAST_STREAM_MAX_BITRATE` in kbit/s: the only way to try a declaration
/// this client doesn't ship, since op 12 is otherwise a fixed ceiling. Read
/// once, because a stream declares it once.
fn declared() -> Option<u32> {
    static ONCE: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
    *ONCE.get_or_init(|| {
        let asked = std::env::var("YAPFAST_STREAM_MAX_BITRATE").ok()?;
        match asked.trim().parse::<u32>() {
            Ok(kbps) => Some(kbps.saturating_mul(1000)),
            Err(error) => {
                tracing::warn!(asked, %error, "YAPFAST_STREAM_MAX_BITRATE is not kbit/s");
                None
            }
        }
    })
}

/// Op 5's speaking flags for a stream's sound: shared sound, not a
/// microphone, as Discord's client marks it.
const SOUNDSHARE: u8 = 2;

/// The codec every Yapfast sender can stream in, as every other Go Live
/// sender known to work does. A GPU that encodes AV1 or H265 offers those
/// first.
pub const SEND_CODEC: Codec = Codec::H264;

/// One encoded frame to send, in `codec`, with its time in the 90 kHz clock.
/// Its bytes are its own until it's dropped, and go back to the encoder's
/// [`Pool`] then, so nothing copies them between the encoder and the
/// packetizer.
pub struct Frame {
    pub data: Encoded,
    pub codec: Codec,
    pub timestamp: u32,
    pub keyframe: bool,
}

/// The encoded bytes of one frame, in a buffer from its encoder's [`Pool`].
pub struct Encoded {
    buffer: Vec<u8>,
    len: usize,
    pool: Option<Arc<Pool>>,
}

impl Encoded {
    /// The whole buffer, for an encoder to write a frame into: [`Self::finish`]
    /// says how much of it the frame is.
    pub fn spare(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    /// The number of bytes of [`Self::spare`] the encoder wrote.
    pub fn finish(&mut self, len: usize) {
        debug_assert!(len <= self.buffer.len());
        self.len = len;
    }
}

impl Deref for Encoded {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.buffer[..self.len]
    }
}

impl From<Vec<u8>> for Encoded {
    fn from(buffer: Vec<u8>) -> Self {
        let len = buffer.len();
        Self {
            buffer,
            len,
            pool: None,
        }
    }
}

impl Drop for Encoded {
    fn drop(&mut self) {
        let Some(pool) = self.pool.take() else {
            return;
        };
        let capacity = self.buffer.capacity();
        let buffer = std::mem::take(&mut self.buffer);
        // A buffer another size than the pool's is no use to it, and neither is
        // one past the few a stream works through: both are let go, and what the
        // pool holds goes down with them.
        if capacity == pool.capacity() {
            let mut idle = lock(&pool.idle);
            if idle.len() < BUFFERS {
                idle.push(buffer);
                return;
            }
        }
        pool.release(capacity);
    }
}

/// The most buffers a [`Pool`] keeps: one being encoded into, one queued to
/// send, and one being sent. The queue is four frames deep, so a queue that
/// fills allocates a buffer per frame beyond these and frees it with the
/// frame: memory is a product goal, and a pool that kept pace with a stall
/// would hold it for a burst that may not come.
const BUFFERS: usize = 3;
/// Where a [`Pool`]'s buffers start: two bytes a pixel is room for a keyframe
/// of noise, but a stream's frames are a small fraction of that, and most
/// never need more. One that does grows the pool.
const START: usize = 512 * 1024;

/// The buffers one encoder's frames are encoded into, reused as their frames
/// are sent. [`Pool::checkout`] hands one out and [`Encoded`]'s drop brings
/// it back; neither ever waits for one to come back.
pub struct Pool {
    /// The size buffers are handed out at now: a frame too big grows it.
    capacity: AtomicUsize,
    /// The most a frame of the encoder's size can need.
    max: usize,
    idle: Mutex<Vec<Vec<u8>>>,
    created: AtomicUsize,
    /// The bytes the pool's buffers hold now, idle and out on loan, and the
    /// most they have held: a stream's report shows both, so a pool that grew
    /// for one large frame says what it kept rather than how many buffers it
    /// made.
    live: AtomicUsize,
    peak: AtomicUsize,
}

impl Pool {
    /// A pool of buffers for an encoder's frames, which never need more than
    /// `max` bytes: enough for a keyframe of noise. They start at [`START`],
    /// or at `max` where that is already smaller.
    pub fn new(max: usize) -> Self {
        Self {
            capacity: AtomicUsize::new(max.min(START)),
            max,
            idle: Mutex::new(Vec::new()),
            created: AtomicUsize::new(0),
            live: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        }
    }

    /// A buffer of the pool's current size to encode a frame into: an idle
    /// one, or a new one when every buffer is still out.
    pub fn checkout(self: &Arc<Self>) -> Encoded {
        let capacity = self.capacity.load(Ordering::Relaxed);
        let idle = lock(&self.idle).pop();
        let buffer = match idle {
            Some(buffer) if buffer.capacity() == capacity => buffer,
            // A buffer of a size the pool has outgrown is no use to it: it is
            // let go, and what the pool holds goes down with it.
            stale => {
                if let Some(stale) = stale {
                    self.release(stale.capacity());
                }
                self.created.fetch_add(1, Ordering::Relaxed);
                self.hold(capacity);
                vec![0; capacity]
            }
        };
        Encoded {
            buffer,
            len: capacity,
            pool: Some(self.clone()),
        }
    }

    /// Doubles the size buffers are handed out at, up to the pool's max, for
    /// a frame too big for them: false when it is already as large as a
    /// frame of the encoder's size can need, so the caller gives up.
    pub fn grow(&self) -> bool {
        let capacity = self.capacity.load(Ordering::Relaxed);
        if capacity >= self.max {
            return false;
        }
        // The idle buffers of the size this leaves are ones the pool will never
        // hand out again: a checkout drops one it has outgrown, so they would sit
        // under the stack's new top for the rest of the stream, held for nothing.
        // Growing is the one moment they are all in one place to give back, so it
        // is where they go; a buffer still out is dropped by `Encoded`'s own size
        // check when it comes back.
        self.trim();
        self.capacity
            .store((capacity * 2).min(self.max), Ordering::Relaxed);
        true
    }

    /// The size of buffer it hands out now.
    pub fn capacity(&self) -> usize {
        self.capacity.load(Ordering::Relaxed)
    }

    /// The bytes the pool's buffers hold now, idle and out on loan, and the most
    /// they have held: a stream's report shows both, so a pool that grew for one
    /// large frame says what it kept rather than how many buffers it made.
    pub fn bytes(&self) -> (usize, usize) {
        (
            self.live.load(Ordering::Relaxed),
            self.peak.load(Ordering::Relaxed),
        )
    }

    /// Gives back the idle buffers now, for a machine short of memory. What is
    /// out on loan comes back to the pool's own check and stays or goes by the
    /// size it was made at, so nothing in flight is ever resized or freed.
    pub fn trim(&self) {
        let mut idle = lock(&self.idle);
        let bytes: usize = idle.iter().map(Vec::capacity).sum();
        idle.clear();
        drop(idle);
        self.release(bytes);
    }

    /// Counts a buffer the pool has just made, and the high-water mark with it.
    fn hold(&self, capacity: usize) {
        let live = self.live.fetch_add(capacity, Ordering::Relaxed) + capacity;
        self.peak.fetch_max(live, Ordering::Relaxed);
    }

    /// Counts a buffer the pool has let go of.
    fn release(&self, capacity: usize) {
        self.live.fetch_sub(capacity, Ordering::Relaxed);
    }

    /// How many buffers the pool has allocated so far, for tests and logs: a
    /// pool that keeps allocating is one the stream is outrunning.
    pub fn created(&self) -> usize {
        self.created.load(Ordering::Relaxed)
    }
}

/// One Opus frame of the stream's sound: 20 ms of stereo at 48 kHz, with
/// `timestamp` in samples.
pub struct Sound {
    pub opus: Vec<u8>,
    pub timestamp: u32,
}

/// The buffers the stream's sound is queued in: the mixer writes a frame into
/// one and the connection gives it back once the frame is on the wire, so a
/// stream sending 50 frames a second allocates for none of them. A queue that
/// backs up takes a buffer of its own rather than wait for one.
#[derive(Default)]
pub struct SoundBuffers(Mutex<Vec<Vec<u8>>>);

impl SoundBuffers {
    /// A buffer to write a frame into: one that came back, or a new one when
    /// every frame still waits to go out.
    pub fn take(&self) -> Vec<u8> {
        lock(&self.0).pop().unwrap_or_default()
    }
    /// Takes back the buffer of a frame that went out.
    fn give(&self, mut buffer: Vec<u8>) {
        // As many as the queue holds: more would be memory kept for a
        // connection that can't keep up with the mixer.
        let mut idle = lock(&self.0);
        if idle.len() < SOUND_BUFFERS {
            buffer.clear();
            idle.push(buffer);
        }
    }
}

/// Queues the stream's sound, from a thread of the sender's own.
#[derive(Clone)]
pub struct SoundSender {
    sounds: mpsc::Sender<Sound>,
    buffers: Arc<SoundBuffers>,
}

impl SoundSender {
    /// A buffer to write a frame of sound into, which comes back here once the
    /// frame is sent.
    pub fn buffer(&self) -> Vec<u8> {
        self.buffers.take()
    }
    /// Queues a frame of sound, dropping it if the network can't keep up.
    pub fn send(&self, sound: Sound) {
        let _ = self.sounds.try_send(sound);
    }
}

/// One whole frame received, decrypted and ready to decode.
pub struct ReceivedFrame {
    pub codec: Codec,
    pub data: Vec<u8>,
    pub keyframe: bool,
    /// The sender's RTP timestamp for the picture, on its 90 kHz clock: the
    /// one thing that ties a decoded picture to the sound that came with it,
    /// which the frames file and the reports carry on from here.
    pub timestamp: u32,
}

/// One Opus frame of a watched stream's sound as it came: its payload, its
/// place in the sender's numbering, and when it reached Yapfast.
#[derive(Debug)]
pub struct ReceivedSound {
    pub opus: Vec<u8>,
    pub sequence: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    /// When it arrived, on the monotonic clock that runs across threads in
    /// this process, which is what the player compares against.
    pub arrived: std::time::Instant,
}

impl ReceivedSound {
    /// `packet`'s numbering, and `opus`, its decrypted payload, which came
    /// at `arrived`.
    pub fn new(packet: &rtp::Packet, opus: Vec<u8>, arrived: std::time::Instant) -> Self {
        Self {
            opus,
            sequence: packet.sequence,
            timestamp: packet.timestamp,
            ssrc: packet.ssrc,
            arrived,
        }
    }
}

/// One of the streamer's sender reports: the RTP timestamp its clock read at
/// the NTP time the report names, and when the report reached Yapfast. The
/// middle 32 bits of the NTP time are kept, as a receiver report echoes them
/// and they are all a relative mapping needs; the whole 64 bits would say
/// the same to the second.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SenderReport {
    /// The middle 32 bits of the NTP time: seconds since 1900 in 16.16,
    /// keeping the low 16 bits of the seconds and the high 16 of the
    /// fraction, so the seconds turn over every 18 hours.
    pub ntp: u32,
    /// The RTP timestamp its clock carried at that instant.
    pub rtp: u32,
    /// When the report reached Yapfast, which says how fresh the mapping is.
    pub heard: std::time::Instant,
}

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
    fn heard_audio(&self, report: SenderReport) {
        *lock(&self.audio) = Some(report);
    }

    /// Keeps the streamer's last report for its picture clock.
    fn heard_video(&self, report: SenderReport) {
        *lock(&self.video) = Some(report);
    }

    /// Keeps `timestamp` as the newest picture that came.
    fn heard_picture(&self, timestamp: u32) {
        self.picture.store(timestamp, Ordering::Relaxed);
        self.seen_picture.store(true, Ordering::Release);
    }
}

/// A watched stream's sound on its way to a player, with the clock the
/// streamer's sender reports put its sound and picture on, so the player can
/// say how far behind the picture it plays. It ends with the connection.
pub struct SoundReceiver {
    sounds: channel::Receiver<ReceivedSound>,
    clock: Arc<StreamClock>,
}

impl SoundReceiver {
    /// `sounds` as they come, with `clock` the receive side writes into.
    pub fn new(sounds: channel::Receiver<ReceivedSound>, clock: Arc<StreamClock>) -> Self {
        Self { sounds, clock }
    }

    /// The sound waiting now, without waiting for more.
    pub fn try_iter(&self) -> channel::TryIter<'_, ReceivedSound> {
        self.sounds.try_iter()
    }

    /// The next sound, waiting at most `wait`: `Err` when none came in time
    /// or the connection ended.
    pub fn recv_timeout(&self, wait: Duration) -> Result<ReceivedSound, channel::RecvTimeoutError> {
        self.sounds.recv_timeout(wait)
    }

    /// What the streamer last said about where its clocks sit.
    pub fn clock(&self) -> Arc<StreamClock> {
        Arc::clone(&self.clock)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    Connecting,
    Connected,
    Ended(String),
}

struct Shared {
    link: Mutex<Link>,
    /// Sending: the next frame should be a keyframe. Receiving: ask the
    /// streamer for one.
    keyframe: AtomicBool,
    size: Mutex<(u32, u32)>,
    /// Receiving: where the streamer's sender reports put its sound and
    /// picture clocks, written by the receive thread and read by the player.
    clock: Arc<StreamClock>,
    /// Sending: the bit/s to encode at, as the send thread's [`Controller`]
    /// decides. Receiving: nothing reads one.
    target: AtomicU64,
    /// Sending: the codecs offered, most wanted first, and whether that
    /// changed since Discord was told. Receiving: the codecs decoded.
    offers: Mutex<Vec<Codec>>,
    offers_changed: AtomicBool,
    /// Sending: the codec Discord chose, in op 4 and then op 14, once it
    /// said.
    chosen: Mutex<Option<Codec>>,
    /// Sending: the video's bytes sent so far, resent ones too, as UDP
    /// payloads; the keyframes viewers asked for; and the packets resent.
    video_bytes: AtomicU64,
    keyframe_requests: AtomicU64,
    resent: AtomicU64,
}

impl Shared {
    /// The state a stream of `size` at `fps` shares between threads. A
    /// sender's target starts at what the controller opens a stream of that
    /// size at, under the ceiling [`max_bitrate`] declares for it: it's the
    /// encoder's rate before the first frame goes, and again after an encoder
    /// is opened mid-stream.
    fn new(size: (u32, u32), fps: u32) -> Arc<Self> {
        Arc::new(Self {
            link: Mutex::new(Link::Connecting),
            keyframe: AtomicBool::new(true),
            size: Mutex::new(size),
            clock: Arc::new(StreamClock::default()),
            target: AtomicU64::new(congestion::start(max_bitrate(size.0, size.1, fps).into())),
            offers: Mutex::new(vec![SEND_CODEC]),
            offers_changed: AtomicBool::new(false),
            chosen: Mutex::new(None),
            video_bytes: AtomicU64::new(0),
            keyframe_requests: AtomicU64::new(0),
            resent: AtomicU64::new(0),
        })
    }
}

/// Runs a connection on a thread of its own until it ends, recording why.
fn spawn(
    name: &str,
    shared: Arc<Shared>,
    run: impl FnOnce(Arc<Shared>) -> std::pin::Pin<Box<dyn Future<Output = Result<()>>>>
    + Send
    + 'static,
) -> Result<()> {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(anyhow::Error::from)
                .and_then(|runtime| runtime.block_on(run(shared.clone())));
            let reason = match result {
                Ok(()) => "stopped".to_owned(),
                Err(error) => {
                    tracing::warn!(error = format!("{error:#}"), "the stream connection ended");
                    format!("{error:#}")
                }
            };
            *lock(&shared.link) = Link::Ended(reason);
        })
        .context("could not start the stream connection")?;
    Ok(())
}

/// The user and channel ids a stream key names, as numbers.
fn ids(server: &StreamServer) -> Result<(u64, u64, u64)> {
    let (_, channel, streamer) = stream::parse(&server.stream_key).context("not a stream key")?;
    Ok((
        server.user_id.parse().context("not a user id")?,
        channel.parse().context("not a channel id")?,
        streamer.parse().context("not a user id")?,
    ))
}

/// Sends frames on a thread of its own; dropping it ends the connection.
pub struct StreamSender {
    frames: mpsc::Sender<Frame>,
    sounds: mpsc::Sender<Sound>,
    buffers: Arc<SoundBuffers>,
    shared: Arc<Shared>,
}

impl StreamSender {
    /// Connects to `server` and announces `width` by `height` video,
    /// offering to send in `offers`, most wanted first. Discord picks the
    /// first every viewer can decode; see [`Self::codec`].
    pub fn start(
        server: StreamServer,
        width: u32,
        height: u32,
        fps: u32,
        offers: Vec<Codec>,
    ) -> Result<Self> {
        let (user, channel, _) = ids(&server)?;
        let shared = Shared::new((width, height), fps);
        *lock(&shared.offers) = offers;
        // A few frames of slack; beyond that the network can't keep up.
        let (frames, receiver) = mpsc::channel(4);
        // 160 ms of sound.
        let (sounds, heard) = mpsc::channel(8);
        let buffers = Arc::new(SoundBuffers::default());
        let sending = buffers.clone();
        spawn("yapfast-stream-send", shared.clone(), move |shared| {
            Box::pin(async move {
                send(
                    &server,
                    user,
                    channel,
                    fps,
                    &shared,
                    receiver,
                    (heard, sending),
                )
                .await
            })
        })?;
        Ok(Self {
            frames,
            sounds,
            buffers,
            shared,
        })
    }

    /// Where the stream's sound goes.
    pub fn sound(&self) -> SoundSender {
        SoundSender {
            sounds: self.sounds.clone(),
            buffers: self.buffers.clone(),
        }
    }

    /// Queues a frame. A full queue drops it and asks for a keyframe, so
    /// viewers recover once the network catches up. Every frame dropped asks,
    /// however lately a keyframe went: the frame after a drop has to be whole,
    /// or the ones the encoder makes from it reference a picture nobody got.
    /// A queue whose connection has ended takes nothing and asks for nothing:
    /// the task that would read a keyframe request has stopped.
    pub fn send(&self, frame: Frame) {
        if let Err(mpsc::error::TrySendError::Full(_)) = self.frames.try_send(frame) {
            self.shared.keyframe.store(true, Ordering::Relaxed);
        }
    }

    /// Whether a frame queued now would be dropped: what an encoder asks
    /// before it spends a frame's work on one the network can't take. The
    /// queue is a few frames' slack against a stall, not a backlog to fill, so
    /// a full one means waiting for it to drain and dropping the frame at its
    /// source meanwhile.
    pub fn backlogged(&self) -> bool {
        self.frames.capacity() == 0
    }

    /// Announces a new frame size with the next frame.
    pub fn resize(&self, width: u32, height: u32) {
        *lock(&self.shared.size) = (width, height);
    }

    /// Whether the next frame should be a keyframe: at the start, after
    /// dropped frames, or when a viewer asked for one.
    pub fn take_keyframe_request(&self) -> bool {
        self.shared.keyframe.swap(false, Ordering::Relaxed)
    }

    /// Whether a keyframe is waiting to be made, without taking the request:
    /// what a desktop that isn't changing asks to know whether it is worth
    /// producing a picture at all, before it produces one.
    pub fn keyframe_wanted(&self) -> bool {
        self.shared.keyframe.load(Ordering::Relaxed)
    }

    /// Makes the next frame a whole one, without waiting for a decoder to ask:
    /// what a stream says when the pictures themselves change kind, as from the
    /// card's textures to the processor's, so a viewer's decoder starts on the
    /// new ones rather than depending on the last of the old.
    pub fn force_keyframe(&self) {
        self.shared.keyframe.store(true, Ordering::Relaxed);
    }

    /// The bit/s to encode at: what keeps the uplink's queue short under the
    /// stream's declared ceiling. See [`congestion`].
    pub fn target(&self) -> u64 {
        self.shared.target.load(Ordering::Relaxed)
    }

    pub fn link(&self) -> Link {
        lock(&self.shared.link).clone()
    }

    /// The codec Discord wants the frames in, once it said. It changes when
    /// a viewer joins who can't decode it, or leaves; frames in another
    /// codec are dropped.
    pub fn codec(&self) -> Option<Codec> {
        let chosen = (*lock(&self.shared.chosen))?;
        let offers = lock(&self.shared.offers);
        // Withdrawn, while Discord hasn't chosen again yet.
        Some(chosen)
            .filter(|chosen| offers.contains(chosen))
            .or(offers.first().copied())
    }

    /// What the stream sent so far, and what viewers asked of it.
    pub fn sent(&self) -> SentSoFar {
        let load = |count: &AtomicU64| count.load(Ordering::Relaxed);
        SentSoFar {
            video_bytes: load(&self.shared.video_bytes),
            keyframe_requests: load(&self.shared.keyframe_requests),
            resent: load(&self.shared.resent),
        }
    }

    /// Stops offering `codec`, which the encoder can't produce any more:
    /// Discord then picks another, which [`Self::codec`] gives.
    pub fn withdraw(&self, codec: Codec) {
        let mut offers = lock(&self.shared.offers);
        if offers.len() > 1 && offers.contains(&codec) {
            offers.retain(|&offered| offered != codec);
            self.shared.offers_changed.store(true, Ordering::Relaxed);
        }
    }
}

/// A stream's video sent since it started, as [`StreamSender::sent`] says:
/// bytes as UDP payloads, resent ones too; keyframes viewers asked for; and
/// packets resent when a viewer reported them lost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SentSoFar {
    pub video_bytes: u64,
    pub keyframe_requests: u64,
    pub resent: u64,
}

/// Receives someone's stream on a thread of its own; dropping it ends the
/// connection.
pub struct StreamReceiver {
    frames: channel::Receiver<ReceivedFrame>,
    sounds: Option<channel::Receiver<ReceivedSound>>,
    shared: Arc<Shared>,
    _stop: oneshot::Sender<()>,
}

impl StreamReceiver {
    /// Watches the stream, telling Discord the viewer decodes `decodes`,
    /// most wanted first.
    pub fn start(server: StreamServer, decodes: Vec<Codec>) -> Result<Self> {
        let (user, channel, streamer) = ids(&server)?;
        let shared = Shared::new((0, 0), 0);
        *lock(&shared.offers) = decodes;
        // Frames waiting to be decoded; more means decoding can't keep up.
        let (frames, receiver) = channel::sync_channel(4);
        // Opus frames waiting to be played. A bound on memory, not on
        // latency: the player takes what came on every turn, and drops what
        // waited too long, so a full queue delays nothing by itself.
        let (sounds, heard) = channel::sync_channel(16);
        let (stop, stopped) = oneshot::channel();
        spawn("yapfast-stream-receive", shared.clone(), move |shared| {
            Box::pin(async move {
                receive_stream(
                    &server,
                    user,
                    channel,
                    streamer,
                    &shared,
                    (frames, sounds),
                    stopped,
                )
                .await
            })
        })?;
        Ok(Self {
            frames: receiver,
            sounds: Some(heard),
            shared,
            _stop: stop,
        })
    }

    /// The next whole frame, waiting at most `wait`.
    pub fn next(&self, wait: Duration) -> Option<ReceivedFrame> {
        self.frames.recv_timeout(wait).ok()
    }

    /// The stream's sound, decrypted Opus frames and the sender's numbering
    /// with each, for a player of its own, and the clock the streamer's
    /// sender reports put its sound and picture on. It ends with the
    /// connection.
    pub fn take_sounds(&mut self) -> Option<SoundReceiver> {
        Some(SoundReceiver {
            sounds: self.sounds.take()?,
            clock: Arc::clone(&self.shared.clock),
        })
    }

    /// Asks the streamer for a keyframe, as after a frame that wouldn't decode.
    pub fn request_keyframe(&self) {
        self.shared.keyframe.store(true, Ordering::Relaxed);
    }

    pub fn link(&self) -> Link {
        lock(&self.shared.link).clone()
    }
}

pub(crate) fn video_attributes(
    ssrc: u32,
    video: u32,
    rtx: u32,
    (width, height): (u32, u32),
    fps: u32,
) -> Value {
    json!({
        "audio_ssrc": ssrc,
        "video_ssrc": video,
        "rtx_ssrc": rtx,
        "streams": [{
            "type": "video",
            "rid": rtp::STREAM_RID,
            "ssrc": video,
            "active": true,
            "quality": 100,
            "rtx_ssrc": rtx,
            "max_bitrate": max_bitrate(width, height, fps),
            "max_framerate": fps,
            "max_resolution": {"type": "fixed", "width": width, "height": height},
        }],
    })
}

/// Which way the video goes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Send,
    Receive,
}

/// Connects to a stream's server: identifying with its RTC server and a
/// screen, and offering the codecs `role` needs.
/// Select protocol's codecs for a sender: Opus, then each of `offers` by
/// priority, and `withdrawn` ones it can't encode any more. A sender
/// decodes nothing of its own stream.
fn send_codecs(offers: &[Codec], withdrawn: &[Codec]) -> Value {
    let video = offers
        .iter()
        .map(|&codec| (codec, true))
        .chain(withdrawn.iter().map(|&codec| (codec, false)));
    codecs(video.map(|(codec, encode)| (codec, encode, matches!(codec, Codec::Vp8 | Codec::H264))))
}

/// Select protocol's codecs for a viewer: Opus, then each of `decodes` by
/// priority, to decode only.
fn receive_codecs(decodes: &[Codec]) -> Value {
    codecs(decodes.iter().map(|&codec| (codec, false, true)))
}

/// Opus, then each video codec with whether it's encoded and decoded, most
/// wanted first.
fn codecs(video: impl Iterator<Item = (Codec, bool, bool)>) -> Value {
    let mut codecs =
        vec![json!({"name": "opus", "type": "audio", "priority": 1000, "payload_type": rtp::OPUS})];
    for (index, (codec, encode, decode)) in video.enumerate() {
        let (payload_type, rtx_payload_type) = codec.payload_types();
        codecs.push(json!({
            "name": codec.name(), "type": "video", "priority": 1000 * (index + 1),
            "payload_type": payload_type, "rtx_payload_type": rtx_payload_type,
            "encode": encode, "decode": decode,
        }));
    }
    Value::Array(codecs)
}

async fn open(
    server: &StreamServer,
    user: u64,
    channel: u64,
    role: Role,
    shared: &Shared,
) -> Result<Connection> {
    // Discord made a stream's DAVE group id just before its RTC server id,
    // in the one stream seen so far; Dave moves to the id the voice server
    // uses if this guess is wrong.
    let group = server
        .rtc_server_id
        .parse::<u64>()
        .map_or(channel, |id| id.saturating_sub(1));
    let identity = Identity {
        endpoint: server.endpoint.clone(),
        server_id: server.rtc_server_id.clone(),
        user_id: server.user_id.clone(),
        session_id: server.session_id.clone(),
        token: server.token.clone(),
        streams: json!([{"type": "screen", "rid": rtp::STREAM_RID, "quality": 100}]),
        user,
        group,
    };
    // Yapfast encodes in what it offers, and decodes what it was given.
    let codecs = match role {
        Role::Send => send_codecs(&lock(&shared.offers), &[]),
        Role::Receive => receive_codecs(&lock(&shared.offers)),
    };
    let streamer: Vec<u64> = stream::parse(&server.stream_key)
        .and_then(|(_, _, streamer)| streamer.parse().ok())
        .into_iter()
        .collect();
    let connection = Connection::open(&identity, codecs, &streamer).await?;
    tracing::info!(rtc_server = %server.rtc_server_id, channel, "connected to a Discord stream");
    Ok(connection)
}

/// Whether a keyframe is made for a request that came at `now`: the first
/// is, and then one a `KEYFRAME_REQUESTS` after the keyframe last made for
/// one, `answered`. A viewer asks at most that often while frames are lost,
/// and a streamer makes at most that often too, because every viewer who lost
/// the same packet asks at once and a keyframe is many times a frame: a run of
/// them at a rate that just dropped is lost in its turn, which asks again.
fn keyframe_due(answered: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    answered.is_none_or(|answered| now.duration_since(answered) >= KEYFRAME_REQUESTS)
}

async fn send(
    server: &StreamServer,
    user: u64,
    channel: u64,
    fps: u32,
    shared: &Shared,
    mut frames: mpsc::Receiver<Frame>,
    (mut sounds, buffers): (mpsc::Receiver<Sound>, Arc<SoundBuffers>),
) -> Result<()> {
    let mut connection = open(server, user, channel, Role::Send, shared).await?;
    let (ssrc, video, rtx) = (connection.ssrc, connection.video, connection.rtx);
    let mut size = *lock(&shared.size);
    // What says how fast the stream sends: its ceiling is the rate op 12
    // declares for the picture, and it starts below it, at what the controller
    // opens a stream of that size at.
    let mut pacing = Controller::new(max_bitrate(size.0, size.1, fps).into());
    shared.target.store(pacing.target(), Ordering::Relaxed);
    send_json(
        &mut connection.ws,
        12,
        video_attributes(ssrc, video, rtx, size, fps),
    )
    .await?;
    send_json(
        &mut connection.ws,
        5,
        json!({"speaking": SOUNDSHARE, "delay": 0, "ssrc": ssrc}),
    )
    .await?;
    *lock(&shared.link) = Link::Connected;

    // Discord's choice, or the first offered while it hasn't said.
    let offered = lock(&shared.offers).clone();
    let chosen = connection.codec.unwrap_or(offered[0]);
    if !offered.contains(&chosen) {
        bail!("Discord asked for {chosen:?} video, and Yapfast offered {offered:?}");
    }
    *lock(&shared.chosen) = Some(chosen);
    tracing::info!(codec = ?chosen, ?offered, "Discord chose the stream's codec");
    let mut withdrawn: Vec<Codec> = vec![];
    let mut packetizer = Packetizer::new(video, chosen);
    let mut history = rtp::History::default();
    let interval = connection.interval;
    let mut heartbeat = tokio::time::interval_at(Instant::now() + interval, interval);
    let mut datagram = vec![0u8; 1500];
    // The frame in flight's packets, planned once, and the datagram each is
    // written into and sealed in: the buffers keep their room from one frame
    // to the next.
    let mut plan = rtp::Plan::default();
    let mut out = Vec::new();
    // The frame partway out, if there is one: a batch of its packets a turn
    // of the loop, between which the plan above keeps them planned and this
    // keeps the bytes they are written from.
    let mut flying: Option<Sending> = None;
    // For RTCP sender reports: packets and payload bytes sent so far.
    let (mut packets, mut octets) = (0u32, 0u32);
    let mut reported = Instant::now() - SENDER_REPORTS;
    // The sender reports a viewer's receiver report can echo, by the middle of
    // their NTP time and when each went: the round trip is measured from the
    // one a report names. Video only, so sound's reports aren't kept.
    let mut echoed = [(0u32, std::time::Instant::now()); ECHOED];
    let mut reports_sent = 0usize;
    // When a keyframe was last made for a viewer, which is how often one may
    // be made; see `keyframe_due`.
    let mut answered: Option<std::time::Instant> = None;
    // For the log: whether frames went out encrypted, and what came back.
    let mut encrypted = None;
    let mut sent = Sent::default();
    let mut log = tokio::time::interval_at(Instant::now() + LOG_EVERY, LOG_EVERY);
    // The sound's RTP numbering and sender report counts, on the audio SSRC.
    let mut sound_sequence: u16 = rand::random();
    let (mut sound_packets, mut sound_octets) = (0u32, 0u32);
    let mut sound_reported = Instant::now() - SENDER_REPORTS;
    loop {
        tokio::select! {
            // Sound, the gateway and viewers' feedback come before more video:
            // sound is queued on a bounded channel whose sender drops it when
            // the reader is busy, so every turn the loop spends writing a
            // frame's packets is sound nobody hears.
            Some(sound) = sounds.recv() => {
                let Some(payload) = connection.dave.encrypt_opus(&sound.opus) else {
                    continue;
                };
                // The frame is written into the buffer the video packets go
                // out in too: sound and picture take turns here, and the
                // buffer keeps its room for both.
                rtp::write_audio_into(
                    &mut out,
                    ssrc,
                    sound_sequence,
                    sound.timestamp,
                    sound_packets == 0,
                    &payload,
                );
                sound_sequence = sound_sequence.wrapping_add(1);
                sound_packets = sound_packets.wrapping_add(1);
                sound_octets = sound_octets.wrapping_add(payload.len() as u32);
                let clear = rtp::rtp_clear_len(&out).unwrap_or(out.len());
                connection.transport.seal(&mut out, clear)?;
                connection.udp.send(&out).await?;
                // The frame's own bytes are spent: the buffer goes back to the
                // mixer for the frame after it.
                buffers.give(sound.opus);
                sent.sounds += 1;
                if sound_reported.elapsed() >= SENDER_REPORTS {
                    sound_reported = Instant::now();
                    let mut report = rtp::sender_report(
                        ssrc,
                        std::time::SystemTime::now(),
                        sound.timestamp,
                        sound_packets,
                        sound_octets,
                    );
                    connection.transport.seal(&mut report, rtp::RTCP_CLEAR)?;
                    connection.udp.send(&report).await?;
                }
            }
            message = receive(&mut connection.ws, &mut connection.seq) => {
                // Not heartbeat acks, nor the keys in the ready message and
                // session description.
                if let Some((op, data)) = connection.handle(message?).await?
                    && !matches!(op, 2 | 4 | 6)
                {
                    let mut text = data.to_string();
                    text.truncate(400);
                    tracing::debug!(op, data = %text, "the stream server said");
                    // Session update: a viewer can't decode the codec, or the
                    // one who couldn't left.
                    if op == 14
                        && let Some(codec) = data["video_codec"].as_str().and_then(Codec::from_name)
                    {
                        if offered.contains(&codec) && !withdrawn.contains(&codec) {
                            tracing::info!(?codec, "Discord switched the stream's codec");
                            *lock(&shared.chosen) = Some(codec);
                            shared.keyframe.store(true, Ordering::Relaxed);
                        } else {
                            tracing::warn!(?codec, "Discord asked for a codec Yapfast didn't offer");
                        }
                    }
                    // Tell whoever connected about the video again, in case
                    // the server doesn't pass it on to late viewers.
                    if op == 11 {
                        send_json(&mut connection.ws, 12, video_attributes(ssrc, video, rtx, size, fps)).await?;
                        send_json(&mut connection.ws, 5, json!({"speaking": SOUNDSHARE, "delay": 0, "ssrc": ssrc})).await?;
                    }
                }
            }
            _ = heartbeat.tick() => connection.heartbeat().await?,
            // The next batch of the frame in flight, a turn's worth of its
            // packets: the arms above are polled between batches, so a
            // keyframe of two hundred packets is fifty turns of this loop,
            // each as short as a batch, rather than one turn lasting as long
            // as the keyframe takes to put on the wire.
            _ = tokio::time::sleep_until(flying.as_ref().map_or_else(Instant::now, |frame| frame.due)),
                if flying.is_some() =>
            {
                // Taken so that the arm only holds the frame again if it has
                // packets left: a frame goes out whole, never part of one.
                let Some(mut frame) = flying.take() else {
                    continue;
                };
                let mut done = false;
                // One turn of the loop writes a batch, so one clock read says
                // when its packets went out; a packet's age is a coarse thing
                // beside the window it is kept for.
                let now = std::time::Instant::now();
                for _ in 0..frame.batch() {
                    let Some(last) = packetizer.write(&mut plan, &frame.payload, &mut out) else {
                        done = true;
                        break;
                    };
                    frame.sent_one();
                    // The history keeps the packet as it went, before the
                    // encryption writes over the rest of the buffer.
                    history.remember(&out, now);
                    let clear = rtp::rtp_clear_len(&out).unwrap_or(out.len());
                    packets = packets.wrapping_add(1);
                    octets = octets.wrapping_add((out.len() - clear) as u32);
                    connection.transport.seal(&mut out, clear)?;
                    connection.udp.send(&out).await?;
                    shared.video_bytes.fetch_add(out.len() as u64, Ordering::Relaxed);
                    // What the controller reads to know what the stream is
                    // sending, and doesn't rise far past it.
                    pacing.sent(now, out.len());
                    if last {
                        done = true;
                        break;
                    }
                }
                if done {
                    // The frame is out, so report on it if a second has gone
                    // by, and let the loop take the next one.
                    if reported.elapsed() >= SENDER_REPORTS {
                        reported = Instant::now();
                        let mut report = rtp::sender_report(
                            video,
                            std::time::SystemTime::now(),
                            frame.timestamp,
                            packets,
                            octets,
                        );
                        // A viewer's receiver report echoes this report's
                        // middle NTP word, which is what times the round
                        // trip: the newest `ECHOED` are kept, since a report
                        // about an older one measures the wrong path.
                        if let Some(&(_, middle, _)) = rtp::sender_reports(&report).first() {
                            echoed[reports_sent % ECHOED] = (middle, std::time::Instant::now());
                            reports_sent += 1;
                        }
                        connection.transport.seal(&mut report, rtp::RTCP_CLEAR)?;
                        connection.udp.send(&report).await?;
                    }
                } else {
                    frame.wait();
                    flying = Some(frame);
                }
            }
            received = connection.udp.recv(&mut datagram) => {
                let packet = &mut datagram[..received?];
                if !rtp::is_rtcp(packet) {
                    sent.rtp_back += 1;
                    continue;
                }
                let Some(rtcp) = connection.transport.open(packet, rtp::RTCP_CLEAR) else {
                    sent.unopened += 1;
                    continue;
                };
                // Naming a report's parts is for the five-second debug line and
                // nothing else: two strings a report, and reports arrive in
                // bursts when the path is losing packets.
                if tracing::enabled!(tracing::Level::DEBUG) {
                    for (kind, said) in rtp::rtcp_kinds(rtcp) {
                        *sent.rtcp.entry(kind.clone()).or_default() += 1;
                        if !said.is_empty() {
                            sent.said.insert(kind, said);
                        }
                    }
                }
                // One clock read for what this report says, and what it means
                // is always how long ago it said it.
                let now = std::time::Instant::now();
                if rtp::wants_keyframe(rtcp) {
                    sent.requests += 1;
                    shared.keyframe_requests.fetch_add(1, Ordering::Relaxed);
                    if keyframe_due(answered, now) {
                        answered = Some(now);
                        shared.keyframe.store(true, Ordering::Relaxed);
                    }
                }
                if let Some(estimate) = rtp::bandwidth_estimate(rtcp, video)
                    && let Some(reason) = pacing.set_estimate(estimate, now)
                {
                    retarget(shared, &pacing, reason, None);
                }
                if let Some(reception) = rtp::reception(rtcp, video) {
                    // The round trip the report measured, from the sender
                    // report it echoes: one this end sent, if the report is
                    // about this stream rather than one before it.
                    let trip = echoed
                        .iter()
                        .find(|&&(middle, _)| reception.last_report != 0 && middle == reception.last_report)
                        .and_then(|&(_, sent)| rtp::round_trip(sent, now, reception.delay));
                    if let Some(reason) = pacing.report(now, trip, &reception) {
                        retarget(shared, &pacing, reason, Some((trip, reception.fraction_lost)));
                    }
                }
                for lost in rtp::nacked(rtcp, video).into_iter().take(RESENDS) {
                    let Some(mut again) = history.get(lost, now).and_then(|original| packetizer.retransmit(original, rtx)) else {
                        sent.too_late += 1;
                        continue;
                    };
                    let clear = rtp::rtp_clear_len(&again).unwrap_or(again.len());
                    connection.transport.seal(&mut again, clear)?;
                    connection.udp.send(&again).await?;
                    shared.video_bytes.fetch_add(again.len() as u64, Ordering::Relaxed);
                    // A resend is the stream's traffic as much as a first
                    // send, so the rate the controller reads includes it.
                    pacing.sent(now, again.len());
                    shared.resent.fetch_add(1, Ordering::Relaxed);
                    sent.resent += 1;
                }
            }
            // A frame is taken only between the ones in flight: the channel is
            // bounded, and what doesn't fit is dropped with a keyframe asked
            // for, so a slow network pushes back rather than queueing video.
            frame = frames.recv(), if flying.is_none() => {
                let Some(frame) = frame else {
                    let _ = connection.ws.close(None).await;
                    return Ok(());
                };
                let wanted = *lock(&shared.size);
                if wanted != size {
                    size = wanted;
                    send_json(&mut connection.ws, 12, video_attributes(ssrc, video, rtx, size, fps)).await?;
                    // The picture changed size, so what it's worth changed: a
                    // bigger one declares more, and the controller takes that
                    // as its ceiling from here on.
                    if let Some(reason) = pacing.set_cap(max_bitrate(size.0, size.1, fps).into(), std::time::Instant::now()) {
                        retarget(shared, &pacing, reason, None);
                    }
                }
                let encrypting = connection.dave.encrypting();
                if encrypted != Some(encrypting) {
                    encrypted = Some(encrypting);
                    tracing::info!(
                        end_to_end = encrypting,
                        dave = connection.dave.status(),
                        "sending stream frames"
                    );
                }
                if shared.offers_changed.swap(false, Ordering::Relaxed) {
                    let offers = lock(&shared.offers).clone();
                    withdrawn = offered.iter().copied().filter(|codec| !offers.contains(codec)).collect();
                    tracing::info!(?offers, ?withdrawn, "offering fewer codecs");
                    send_json(&mut connection.ws, 14, json!({"codecs": send_codecs(&offers, &withdrawn)})).await?;
                }
                // Frames encoded before the encoder followed Discord's choice.
                if Some(frame.codec) != *lock(&shared.chosen) {
                    continue;
                }
                packetizer.set_codec(frame.codec);
                let Some(payload) = connection.dave.encrypt(frame.codec, &frame.data) else {
                    continue;
                };
                sent.frames += 1;
                sent.keyframes += u32::from(frame.keyframe);
                packetizer.plan_into(&mut plan, &payload, frame.timestamp, frame.keyframe);
                // The frame's buffer goes back to the encoder's pool now: the
                // packets are written from the encrypted copy, which the plan
                // points into until the frame is out.
                flying = Some(Sending::new(payload, frame.timestamp));
            }
            _ = log.tick() => {
                let target_kbps = pacing.target() / 1000;
                let sending_kbps = pacing.sending(std::time::Instant::now()).map(|sending| sending / 1000);
                let received_kbps = pacing.received().map(|received| received / 1000);
                tracing::debug!(?sent, packets, octets, target_kbps, sending_kbps, received_kbps, "stream sent");
                sent = Sent::default();
            }
        }
    }
}

/// Hands the encoder the controller's new target, and logs why it changed,
/// with what the receiver report that changed it said: the round trip it
/// measured, and the packets lost out of 256. The encoder is told on the next
/// frame, if the target moved a tenth of its rate.
fn retarget(
    shared: &Shared,
    pacing: &Controller,
    reason: congestion::Reason,
    report: Option<(Option<Duration>, u8)>,
) {
    shared.target.store(pacing.target(), Ordering::Relaxed);
    tracing::debug!(
        kbps = pacing.target() / 1000,
        %reason,
        round_trip_ms = report.and_then(|(trip, _)| trip).map(|trip| trip.as_millis() as u64),
        lowest_ms = pacing.baseline().map(|trip| trip.as_millis() as u64),
        lost = report.map(|(_, lost)| lost),
        "changing the stream's bitrate"
    );
}

/// A frame partway out, a batch of its packets a turn of the send loop.
///
/// A keyframe is hundreds of packets. Written in one turn it holds the loop for
/// tens of milliseconds, and the sound queued meanwhile is dropped when its
/// bounded channel fills. So a frame goes out a batch a turn: `BURST` in the
/// turn it starts, then `PACE` a turn with `STEP` between batches. The wire
/// sees what it saw when the loop slept inside the frame — the same burst, the
/// same pace past it — but the loop is back between batches to poll sound, the
/// gateway and feedback.
///
/// Nothing is kept between turns that the frame didn't already have: the plan
/// it was planned into, which the next frame reuses, and the encrypted bytes
/// its packets are written from. Sending it in batches copies nothing more.
struct Sending {
    /// The frame as its packets carry it, end-to-end encrypted once before
    /// the first packet went and kept until the last has.
    payload: Vec<u8>,
    /// How many of its packets have gone out, which decides the next batch's
    /// size. The plan counts them too, but keeps the count to itself.
    sent: usize,
    /// When the next batch may go: at once for the first, `STEP` after the
    /// batch before for the rest.
    due: Instant,
    /// The frame's time in the 90 kHz clock, which the sender report that
    /// follows its last packet names.
    timestamp: u32,
}

impl Sending {
    /// Takes the bytes a frame's packets are written from, and the frame's
    /// time in the 90 kHz clock. Its first batch is due at once.
    fn new(payload: Vec<u8>, timestamp: u32) -> Self {
        Self {
            payload,
            sent: 0,
            due: Instant::now(),
            timestamp,
        }
    }

    /// How many of the frame's packets this turn writes: what's left of its
    /// first `BURST`, then `PACE` a turn.
    fn batch(&self) -> usize {
        if self.sent < BURST {
            BURST - self.sent
        } else {
            PACE
        }
    }

    /// Notes that one of the frame's packets went out.
    fn sent_one(&mut self) {
        self.sent += 1;
    }

    /// Holds the rest of the frame for `STEP`, so the loop polls everything
    /// else before the batch after this one.
    fn wait(&mut self) {
        self.due = Instant::now() + STEP;
    }
}

/// What a streamer sent and heard back since it last logged.
#[derive(Debug, Default)]
struct Sent {
    frames: u32,
    keyframes: u32,
    /// Opus frames of the stream's sound.
    sounds: u32,
    /// RTCP by packet type and feedback format, as `type/format`.
    rtcp: std::collections::BTreeMap<String, u32>,
    /// What the last of each kind said.
    said: std::collections::BTreeMap<String, String>,
    /// Keyframe requests among them.
    requests: u32,
    /// Packets resent when a viewer reported them lost, and those already
    /// gone from the history.
    resent: u32,
    too_late: u32,
    /// RTCP that wouldn't decrypt.
    unopened: u32,
    /// RTP, which a streamer isn't sent.
    rtp_back: u32,
}

/// What the server said about the streamer's stream: its SSRCs and codec.
#[derive(Default)]
struct Streamer {
    /// The SSRC its sound comes on: from op 12, or else the one the first
    /// Opus packet came on.
    audio: Option<u32>,
    video: Option<u32>,
    rtx: Option<u32>,
    codec: Option<Codec>,
}

impl Streamer {
    fn learn(&mut self, op: u64, data: &Value) {
        match op {
            // The streamer's SSRCs.
            12 => {
                let ssrc = |value: &Value| {
                    value
                        .as_u64()
                        .filter(|&ssrc| ssrc != 0)
                        .map(|ssrc| ssrc as u32)
                };
                self.audio = ssrc(&data["audio_ssrc"]).or(self.audio);
                self.video = ssrc(&data["video_ssrc"]).or(self.video);
                self.rtx = ssrc(&data["rtx_ssrc"]).or(self.rtx);
            }
            // Session update: the codec may change.
            14 => {
                if let Some(codec) = data["video_codec"].as_str().and_then(Codec::from_name) {
                    self.codec = Some(codec);
                }
            }
            _ => {}
        }
    }

    /// The codec of a packet, by its payload type or, from the streamer's
    /// video SSRC, by what Discord chose.
    fn codec(&self, packet: &rtp::Packet) -> Option<Codec> {
        if Some(packet.ssrc) == self.rtx {
            return None;
        }
        Codec::from_payload_type(packet.payload_type).or_else(|| {
            (Some(packet.ssrc) == self.video)
                .then_some(self.codec)
                .flatten()
        })
    }
}

/// What a viewer's connection has received, logged while no picture
/// reaches the decoder so a stuck stream shows where it stops.
#[derive(Debug, Default)]
struct Received {
    packets: u64,
    rtcp: u64,
    unopened: u64,
    unparsed: u64,
    /// Packets of a payload type that isn't the streamer's video.
    other: u64,
    video: u64,
    frames: u64,
    unreadable: u64,
    decoded: u64,
    /// Opus frames of the stream's sound, and those that wouldn't decrypt.
    sounds: u64,
    unheard: u64,
    /// Packets on the RTX SSRC: resent video, and padding.
    resent: u64,
    padding: u64,
    /// Video packets that came out of order, those that came after they
    /// were asked for again, those that came too late or twice, and those
    /// never seen.
    reordered: u64,
    recovered: u64,
    late: u64,
    given_up: u64,
    /// Packets asked for again (NACK), and keyframes asked for (PLI).
    asked: u64,
    keyframe_requests: u64,
    /// Frames dropped because the decoder was behind.
    behind: u64,
    payload_types: std::collections::BTreeSet<u8>,
    ssrcs: std::collections::BTreeSet<u32>,
}

/// One SSRC's video on its way to frames.
struct Video {
    reorder: rtp::Reorder,
    assembler: Assembler,
    /// For receiver reports: the counts at the last one.
    reported: (u32, u32),
}

async fn receive_stream(
    server: &StreamServer,
    user: u64,
    channel: u64,
    streamer_id: u64,
    shared: &Shared,
    // Where whole video frames and the sound's Opus frames go.
    (frames, sounds): (
        channel::SyncSender<ReceivedFrame>,
        channel::SyncSender<ReceivedSound>,
    ),
    mut stopped: oneshot::Receiver<()>,
) -> Result<()> {
    let mut connection = open(server, user, channel, Role::Receive, shared).await?;
    // The streamer's clocks, for the player to compare its sound with the
    // picture; this thread is the only writer.
    let clock = &shared.clock;
    let mut streamer = Streamer {
        codec: connection.codec,
        ..Streamer::default()
    };
    for (op, data) in std::mem::take(&mut connection.early) {
        streamer.learn(op, &data);
    }
    // Media sink wants: the server forwards video at the quality a client
    // asks for, here everything at full quality.
    send_json(&mut connection.ws, 15, json!({"any": 100})).await?;
    *lock(&shared.link) = Link::Connected;

    let mut incoming: HashMap<u32, Video> = HashMap::new();
    let mut asked: Option<std::time::Instant> = None;
    let interval = connection.interval;
    let mut heartbeat = tokio::time::interval_at(Instant::now() + interval, interval);
    let mut keyframes = tokio::time::interval(KEYFRAME_REQUESTS);
    // Gives up on missing packets and asks for them again even while
    // nothing arrives.
    let mut repairs = tokio::time::interval(Duration::from_millis(20));
    let mut reports = tokio::time::interval(Duration::from_secs(1));
    // The streamer's last sender report, as its NTP time's middle and when
    // it came, which receiver reports echo for the round trip.
    let mut last_report: Option<(u32, std::time::Instant)> = None;
    // A report for an SSRC this stream doesn't use says nothing about its
    // clocks; said once rather than once a report.
    let mut told_other = false;
    let mut datagram = vec![0u8; 2048];
    let mut received = Received::default();
    let mut log = tokio::time::interval_at(Instant::now() + LOG_EVERY, LOG_EVERY);
    // Frames handed to the decoder when the log last looked.
    let mut flowing = 0;
    loop {
        let mut lost = false;
        tokio::select! {
            _ = &mut stopped => {
                let _ = connection.ws.close(None).await;
                return Ok(());
            }
            _ = heartbeat.tick() => connection.heartbeat().await?,
            message = receive(&mut connection.ws, &mut connection.seq) => {
                if let Some((op, data)) = connection.handle(message?).await? {
                    if !matches!(op, 2 | 4 | 6) {
                        let mut text = data.to_string();
                        text.truncate(400);
                        tracing::debug!(op, data = %text, "the stream server said");
                    }
                    streamer.learn(op, &data);
                }
            }
            _ = keyframes.tick() => {}
            _ = repairs.tick() => {}
            _ = reports.tick() => {
                let Some(media) = streamer.video else { continue };
                let Some(video) = incoming.get_mut(&media) else { continue };
                let Some((highest, lost)) = video.reorder.counts() else { continue };
                let (expected, missed) = (
                    highest.wrapping_sub(video.reported.0),
                    lost.saturating_sub(video.reported.1),
                );
                let fraction = (u64::from(missed) * 256 / u64::from(expected.max(1))).min(255) as u8;
                video.reported = (highest, lost);
                let (middle, delay) = last_report.map_or((0, 0), |(middle, heard)| {
                    (middle, (heard.elapsed().as_secs_f64() * 65536.0) as u32)
                });
                let mut report = rtp::receiver_report(connection.ssrc, media, (highest, lost, fraction), (middle, delay));
                connection.transport.seal(&mut report, rtp::RTCP_CLEAR)?;
                connection.udp.send(&report).await?;
            }
            _ = log.tick() => {
                tracing::debug!(?received, "stream received");
                if received.decoded == flowing {
                    tracing::info!(
                        ?received,
                        dave = connection.dave.status(),
                        unreadable = connection.dave.unreadable.as_deref(),
                        streamer_video = ?streamer.video,
                        streamer_codec = ?streamer.codec,
                        "no stream frames reached the decoder lately"
                    );
                } else if flowing == 0 {
                    tracing::info!(frames = received.decoded, "stream frames are reaching the decoder");
                }
                flowing = received.decoded;
            }
            length = connection.udp.recv(&mut datagram) => {
                let packet = &mut datagram[..length?];
                received.packets += 1;
                if rtp::is_rtcp(packet) {
                    received.rtcp += 1;
                    let rtcp = connection.transport.open(packet, rtp::RTCP_CLEAR).unwrap_or_default();
                    for (sender, ntp, rtp) in rtp::sender_reports(rtcp) {
                        let heard = std::time::Instant::now();
                        let report = SenderReport { ntp, rtp, heard };
                        if Some(sender) == streamer.audio {
                            clock.heard_audio(report);
                        } else if Some(sender) == streamer.video {
                            last_report = Some((ntp, heard));
                            clock.heard_video(report);
                        } else if !told_other {
                            told_other = true;
                            tracing::debug!(
                                sender,
                                audio = ?streamer.audio,
                                video = ?streamer.video,
                                "a sender report names an SSRC this stream doesn't use"
                            );
                        }
                    }
                    continue;
                }
                let Some(clear) = rtp::rtp_clear_len(packet) else {
                    received.unparsed += 1;
                    continue;
                };
                let Some(plain) = connection.transport.open(packet, clear) else {
                    received.unopened += 1;
                    continue;
                };
                let Some(packet) = rtp::parse(plain) else {
                    received.unparsed += 1;
                    continue;
                };
                received.payload_types.insert(packet.payload_type);
                if received.ssrcs.len() < 64 {
                    received.ssrcs.insert(packet.ssrc);
                }
                if packet.payload_type == rtp::OPUS {
                    received.sounds += 1;
                    // The streamer's sound SSRC, when op 12 didn't name one:
                    // a sender report on it is what places its timestamps on
                    // wall-clock time.
                    streamer.audio.get_or_insert(packet.ssrc);
                    let arrived = std::time::Instant::now();
                    match connection.dave.decrypt_opus(streamer_id, packet.payload) {
                        // A full queue drops sound the player can't keep up
                        // with; the player drops what waited too long anyway.
                        Some(opus) => drop(sounds.try_send(ReceivedSound::new(
                            &packet,
                            opus.into_owned(),
                            arrived,
                        ))),
                        None => received.unheard += 1,
                    }
                    continue;
                }
                let held = if streamer.rtx.is_some() && Some(packet.ssrc) == streamer.rtx {
                    received.resent += 1;
                    match streamer.video.and_then(|video| rtp::Held::unwrap_rtx(&packet, video)) {
                        Some(held) => held,
                        None => {
                            received.padding += 1;
                            continue;
                        }
                    }
                } else {
                    rtp::Held::new(&packet)
                };
                let Some(codec) = streamer.codec(&held.packet()) else {
                    received.other += 1;
                    continue;
                };
                received.video += 1;
                streamer.video.get_or_insert(held.ssrc);
                // Every packet of a frame carries its timestamp, so the
                // newest packet that comes is the newest picture to compare
                // the sound against.
                let picture = held.packet().timestamp;
                // Packets can name any SSRC: past a few, a new one isn't
                // given buffers of its own.
                if incoming.len() >= MAX_SSRCS && !incoming.contains_key(&held.ssrc) {
                    received.other += 1;
                    continue;
                }
                let video = incoming.entry(held.ssrc).or_insert_with(|| Video {
                    reorder: rtp::Reorder::default(),
                    assembler: Assembler::new(codec),
                    reported: (0, 0),
                });
                if video.assembler.codec() != codec {
                    video.assembler = Assembler::new(codec);
                }
                let resent = Some(packet.ssrc) == streamer.rtx;
                match video.reorder.insert(held, std::time::Instant::now()) {
                    rtp::Arrival::New if !resent => clock.heard_picture(picture),
                    rtp::Arrival::New | rtp::Arrival::Recovered if resent => received.recovered += 1,
                    rtp::Arrival::New | rtp::Arrival::Recovered => received.reordered += 1,
                    rtp::Arrival::Late => received.late += 1,
                }
            }
        }
        let now = std::time::Instant::now();
        for (&ssrc, video) in &mut incoming {
            let codec = video.assembler.codec();
            let (ready, skipped) = video.reorder.ready(now);
            received.given_up += skipped;
            for held in ready {
                let pushed = video.assembler.push(&held.packet());
                lost |= pushed.lost;
                let Some(frame) = pushed.frame else { continue };
                received.frames += 1;
                let Some(data) = connection.dave.decrypt(streamer_id, frame.data) else {
                    received.unreadable += 1;
                    continue;
                };
                received.decoded += 1;
                let frame = ReceivedFrame {
                    codec,
                    data,
                    keyframe: frame.keyframe,
                    timestamp: frame.timestamp,
                };
                if let Err(channel::TrySendError::Full(_)) = frames.try_send(frame) {
                    // The decoder is behind; the frames after this one
                    // need it, so wait for a keyframe.
                    received.behind += 1;
                    video.assembler.wait_for_keyframe();
                    lost = true;
                }
            }
            let missing = video.reorder.to_ask(now);
            if !missing.is_empty() {
                received.asked += missing.len() as u64;
                let mut request = rtp::nack(connection.ssrc, ssrc, &missing);
                connection.transport.seal(&mut request, rtp::RTCP_CLEAR)?;
                connection.udp.send(&request).await?;
            }
        }
        // Ask for a keyframe after a loss, or when the decoder asked.
        if (lost || shared.keyframe.load(Ordering::Relaxed))
            && keyframe_due(asked, now)
            && let Some(media) = streamer.video
        {
            shared.keyframe.store(false, Ordering::Relaxed);
            asked = Some(now);
            received.keyframe_requests += 1;
            let mut request = rtp::picture_loss(connection.ssrc, media);
            connection.transport.seal(&mut request, rtp::RTCP_CLEAR)?;
            connection.udp.send(&request).await?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_the_streamers_video() {
        let mut streamer = Streamer::default();
        streamer.learn(
            12,
            &json!({"user_id": "2", "audio_ssrc": 10, "video_ssrc": 11, "rtx_ssrc": 12}),
        );
        streamer.learn(14, &json!({"video_codec": "H264"}));
        let packet = |ssrc: u32, payload_type: u8| rtp::Packet {
            payload_type,
            marker: false,
            sequence: 0,
            timestamp: 0,
            ssrc,
            payload: &[],
            orientation: None,
        };
        assert_eq!(
            streamer.codec(&packet(11, rtp::VP8)),
            Some(Codec::Vp8),
            "by payload type"
        );
        assert_eq!(
            streamer.codec(&packet(11, 96)),
            Some(Codec::H264),
            "by Discord's choice"
        );
        assert_eq!(
            streamer.codec(&packet(12, rtp::VP8)),
            None,
            "retransmissions aren't frames"
        );
        assert_eq!(streamer.codec(&packet(30, 96)), None);
    }

    #[test]
    fn a_frame_goes_out_a_batch_a_turn() {
        // Each turn of the send loop writes one batch and polls the rest of
        // its arms before the next, so a frame of 250 packets is 62 turns for
        // sound, feedback and the gateway to be heard in, rather than one
        // turn that lasts as long as the frame takes to put on the wire.
        let mut frame = Sending::new(Vec::new(), 0);
        let mut left = 250;
        let mut sizes = vec![];
        let mut heard = 0;
        while left > 0 {
            // Sound is polled before video, so every turn starts by hearing
            // whatever sound is waiting, then writes a batch.
            heard += 1;
            let batch = frame.batch().min(left);
            assert!(batch > 0, "a turn always writes a packet, or spins");
            for _ in 0..batch {
                frame.sent_one();
            }
            left -= batch;
            sizes.push(batch);
        }
        assert_eq!(sizes[0], BURST, "a frame starts at once");
        let (last, paced) = sizes.split_last().expect("250 packets, in batches");
        assert!(
            paced[1..].iter().all(|&batch| batch == PACE),
            "and goes on a paced batch a turn: {sizes:?}"
        );
        assert!(
            *last > 0 && *last <= PACE,
            "until what's left of it ends the frame: {sizes:?}"
        );
        assert_eq!(
            sizes.len(),
            62,
            "250 packets, none of them in a burst of more than {BURST}"
        );
        assert_eq!(heard, sizes.len(), "sound is heard between every batch");
    }

    #[test]
    fn answers_one_request_for_a_keyframe_a_half_second() {
        // Viewers who lost the same packet ask together, and a rate cut loses
        // what it sends, so requests come in runs. One keyframe a half second
        // answers them, the first at once: a run of keyframes at a cut's rate
        // would be lost in its turn, which asks for another.
        let start = std::time::Instant::now();
        let mut answered = None;
        let mut made = vec![];
        for at in [0u64, 1, 499, 500, 900, 1000, 1499, 1500] {
            let now = start + Duration::from_millis(at);
            if keyframe_due(answered, now) {
                answered = Some(now);
                made.push(at);
            }
        }
        assert_eq!(
            made,
            [0, 500, 1000, 1500],
            "one a half second, the first at once"
        );
        // A request a half second after the one before it is answered, so a
        // stream keeps up with a viewer asking as often as it may.
        let asked = start + Duration::from_millis(2000);
        assert!(keyframe_due(Some(asked), asked + KEYFRAME_REQUESTS));
        assert!(!keyframe_due(
            Some(asked),
            asked + KEYFRAME_REQUESTS - Duration::from_millis(1)
        ));
    }

    #[test]
    fn a_full_queue_drops_the_newest_frame_and_asks_for_a_whole_one() {
        // The frame that doesn't fit is the one dropped, not one of those
        // ahead of it: they are the pictures the next frames are encoded from,
        // and nothing is encoded from a frame that never got queued. So the
        // frame after it has to be whole, or the encoder's next frames would
        // reference a picture no viewer received.
        let (frames, mut queued) = mpsc::channel(4);
        let (sounds, _heard) = mpsc::channel(8);
        let shared = Shared::new((320, 180), 30);
        shared.keyframe.store(false, Ordering::Relaxed);
        let sender = StreamSender {
            frames,
            sounds,
            buffers: Arc::default(),
            shared: shared.clone(),
        };
        let frame = |timestamp, keyframe| Frame {
            data: Vec::new().into(),
            codec: SEND_CODEC,
            timestamp,
            keyframe,
        };
        for at in 0..4 {
            assert!(!sender.backlogged(), "{at} frames fit");
            sender.send(frame(at, false));
        }
        assert!(
            sender.backlogged(),
            "four frames is as far ahead of the network as it can get"
        );
        assert!(
            !shared.keyframe.load(Ordering::Relaxed),
            "nothing dropped yet"
        );
        sender.send(frame(4, false));
        assert!(
            shared.keyframe.load(Ordering::Relaxed),
            "the dropped frame's chain ends here"
        );
        let kept: Vec<u32> = std::iter::from_fn(|| queued.try_recv().ok())
            .map(|frame| frame.timestamp)
            .collect();
        assert_eq!(
            kept,
            [0, 1, 2, 3],
            "the newest went, not the ones before it"
        );
        assert!(!sender.backlogged(), "and room came back");
    }

    #[test]
    fn a_stream_starts_below_its_declared_ceiling() {
        // The declaration is the picture's worth, 0.07 bit/pixel/frame between
        // 3.5 and 9 Mbit/s, and the controller opens a stream under it at
        // congestion::start's rate: that is what an encoder is opened at
        // mid-stream, and what the first frame is encoded at before a report
        // has said anything about the path.
        let (frames, _queued) = mpsc::channel(4);
        let (sounds, _heard) = mpsc::channel(8);
        let sender = |(width, height), fps| StreamSender {
            frames: frames.clone(),
            sounds: sounds.clone(),
            buffers: Arc::default(),
            shared: Shared::new((width, height), fps),
        };
        assert_eq!(max_bitrate(1920, 1080, 60), 8_709_120, "under the cap");
        assert_eq!(max_bitrate(3840, 2160, 60), MOST_SAFE, "4K60 is cut to it");
        assert_eq!(max_bitrate(3440, 1440, 60), MOST_SAFE, "and so is 1440p60");
        assert_eq!(max_bitrate(640, 360, 30), 3_500_000, "the floor");
        for (size, fps) in [((1920, 1080), 60), ((1280, 720), 30), ((3840, 2160), 30)] {
            let ceiling = u64::from(max_bitrate(size.0, size.1, fps));
            let sender = sender(size, fps);
            assert_eq!(sender.target(), congestion::start(ceiling));
            assert!(sender.target() < ceiling, "a stream opens under its cap");
            assert!(
                sender.target() >= congestion::FLOOR,
                "and not under the floor a screen is given"
            );
        }
        // The experimental cap is what the formula gives 4K60, and the safe one
        // is under it, so the switch is the only thing that spends the rest.
        let four_k = (3840.0 * 2160.0 * 60.0 * BITS_PER_PIXEL) as u32;
        assert!(MOST_EXPERIMENTAL >= four_k);
        assert!(MOST_SAFE < four_k);
    }

    /// Growing the pool gives back the idle buffers of the size it leaves.
    ///
    /// Three idle buffers are the most the pool keeps, one each for a frame
    /// being encoded, one queued and one being sent. A checkout discards an idle
    /// buffer the size it has outgrown, but only the one at the top of the stack:
    /// once a new-size buffer is back, serial checkouts cycle it and never reach
    /// the smaller ones underneath. Growing is where they are all in hand, so it
    /// is where they go, and the memory is counted here rather than the buffers.
    #[test]
    fn growing_the_pool_gives_back_the_buffers_it_outgrew() {
        const OLD: usize = 512 * 1024;
        let pool = Arc::new(Pool::new(4 * OLD));
        assert_eq!(pool.capacity(), OLD);
        // What the pool holds while it is idle: the bytes of every buffer it
        // could hand out, which is the number a stream's memory is about.
        let idle_bytes = || -> usize { lock(&pool.idle).iter().map(Vec::capacity).sum() };
        assert_eq!(idle_bytes(), 0, "nothing allocated yet");

        let held: Vec<Encoded> = (0..BUFFERS).map(|_| pool.checkout()).collect();
        drop(held);
        assert_eq!(idle_bytes(), BUFFERS * OLD, "three idle buffers");

        assert!(pool.grow());
        assert_eq!(pool.capacity(), 2 * OLD);
        // The three buffers of the size it left are gone rather than stranded
        // under the new top of the stack.
        assert_eq!(idle_bytes(), 0, "idle buffers of the size the pool outgrew");

        // One buffer checked out and returned, frame after frame: the pool hands
        // the same one back, and what it holds between frames is that one buffer
        // at the size it hands out now.
        let mut live = 0;
        for frame in 0..10 {
            let mut buffer = pool.checkout();
            live = buffer.spare().len();
            buffer.finish(4);
            assert_eq!(live, 2 * OLD, "frame {frame} was encoded into {live} bytes");
            drop(buffer);
            assert_eq!(
                idle_bytes(),
                2 * OLD,
                "frame {frame} left the idle pool holding {} bytes",
                idle_bytes()
            );
        }
        // Two buffers allocated in all: the three of the old size, freed by the
        // growth, and the one the frames after it go through.
        assert_eq!(pool.created(), BUFFERS + 1);
        assert!(live > OLD, "the pool grew rather than kept the old size");
    }
}
