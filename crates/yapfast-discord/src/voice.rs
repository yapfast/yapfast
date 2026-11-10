//! Discord voice: a call over Yapfast's own voice connection
//! (`call_connection.rs`), with DAVE, using the voice server the gateway
//! assigned. Cameras go both ways in the same connection
//! (`call_video.rs`).

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc as channel,
    },
    time::Duration,
};

use anyhow::{Context, Result, bail};
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use crate::{
    call_connection::{self, Order, Sending},
    call_stats::CallStats,
    call_video::{self, CameraSender, VideoFrame},
    ready::PRIVATE,
    rtp,
    service::VoiceServer,
    voice_connection::{Identity, lock},
};

/// How long a call whose voice connection Discord ended waits for Discord
/// to say where it moved the call, before the call ends.
pub const MOVE_GRACE: Duration = Duration::from_secs(5);

pub const SAMPLE_RATE: u32 = 48_000;
/// Received audio is interleaved stereo.
pub const CHANNELS: u16 = 2;
/// Samples per channel in one 20 ms frame.
pub const FRAME: usize = SAMPLE_RATE as usize / 50;

/// Receives a call's audio and connection changes, on the call's thread.
pub trait Listener: Send + Sync + 'static {
    /// Every 20 ms: each speaking user's decoded audio, as interleaved stereo
    /// at 48 kHz, by Discord user id.
    fn tick(&self, voices: &[(u64, &[i16])]);
    /// The voice connection dropped.
    fn disconnected(&self, reason: &str);
    /// Once, on the call's thread, before it connects. The thread decodes
    /// and mixes what plays, so this can raise its priority.
    fn on_call_thread(&self) {}
}

/// Watches a queue of audio filled on one clock and played on another,
/// which never run quite together: a sender's sound card and the call's
/// ticks, or the ticks and the speakers. When the filling one runs fast the
/// queue grows a frame at a time, and the delay with it; this says when a
/// frame can go, once the queue has had frames to spare for a whole window.
pub struct Surplus {
    /// The fewest frames that waited as one was taken, in this window.
    fewest: Option<usize>,
    /// Frames taken in this window.
    counted: u8,
    /// The fewest that waited in the last whole window.
    spare: usize,
    /// How many wait as one is taken while frames keep time: a jitter
    /// buffer's cushion, which isn't surplus.
    cushion: usize,
}

/// Frames are counted in windows of this many: 1 s.
const SURPLUS_WINDOW: u8 = 50;
/// When this many frames more than the cushion waited throughout a window,
/// more than the lateness of their arrivals needed, a quiet one goes.
const SURPLUS: usize = 1;
/// With this many more than the cushion, a frame goes quiet or not.
const SURPLUS_MOST: usize = 4;

impl Default for Surplus {
    /// A queue with no cushion of its own but the frame that comes while
    /// the last plays.
    fn default() -> Self {
        Self::with_cushion(1)
    }
}

impl Surplus {
    /// A queue that keeps `cushion` frames waiting as one is taken.
    pub fn with_cushion(cushion: usize) -> Self {
        Self {
            fewest: None,
            counted: 0,
            spare: 0,
            cushion,
        }
    }

    /// Notes how many frames wait as one is taken to play.
    pub fn note(&mut self, waiting: usize) {
        self.fewest = Some(self.fewest.map_or(waiting, |fewest| fewest.min(waiting)));
        self.counted += 1;
        if self.counted == SURPLUS_WINDOW {
            self.spare = self.fewest.take().unwrap_or(0);
            self.counted = 0;
        }
    }

    /// Whether to skip the frame just taken, for the next: at most one a
    /// window, and only a `quiet` one unless the queue is long.
    pub fn skips(&mut self, quiet: impl FnOnce() -> bool) -> bool {
        let surplus = self.spare.saturating_sub(self.cushion);
        let skips = surplus >= SURPLUS_MOST || (surplus >= SURPLUS && quiet());
        if skips {
            self.spare = 0;
        }
        skips
    }
}

pub struct Call {
    orders: mpsc::UnboundedSender<Order>,
    /// The microphone, once it plays.
    sending: Arc<Mutex<Option<Sending>>>,
    /// The server, or the DM, the connection identifies with.
    server_id: String,
    /// Everyone else's camera frames, until taken.
    video: Option<channel::Receiver<VideoFrame>>,
    stats: Arc<CallStats>,
}

/// Cameras' frames waiting to be decoded; beyond this many, decoding is
/// behind and the call waits for a keyframe.
const VIDEO_BACKLOG: usize = 8;

impl Call {
    /// Connects to the voice server for a channel in `guild_id`, or with
    /// [`PRIVATE`], for a DM or group DM call.
    pub async fn connect(
        server: &VoiceServer,
        guild_id: &str,
        channel_id: &str,
        listener: Arc<dyn Listener>,
    ) -> Result<Self> {
        // A call identifies with its channel where a server's would its server.
        let server_id = if guild_id == PRIVATE {
            channel_id
        } else {
            guild_id
        };
        let sending = Arc::new(Mutex::new(None));
        let identity = identity(server, server_id, channel_id)?;
        let (frames, video) = channel::sync_channel(VIDEO_BACKLOG);
        let stats = Arc::new(CallStats::default());
        let (orders, connected) =
            call_connection::start(identity, listener, sending.clone(), frames, stats.clone())?;
        // Errors say the step that failed; the caller says it was joining.
        connected
            .await
            .context("the call stopped while connecting")??;
        Ok(Self {
            orders,
            sending,
            server_id: server_id.to_owned(),
            video: Some(video),
            stats,
        })
    }

    /// Connects to `server` for `channel_id`, as when Discord moves the call
    /// to another voice server, or someone moves this account to another
    /// channel of the server. What plays carries on there.
    pub async fn move_to(&mut self, server: &VoiceServer, channel_id: &str) -> Result<()> {
        let identity = identity(server, &self.server_id, channel_id)?;
        let (reply, replied) = oneshot::channel();
        if self.orders.send(Order::Move(identity, reply)).is_err() {
            bail!("the call has ended");
        }
        replied
            .await
            .context("the call ended while moving")?
            .context("could not connect to Discord voice")
    }

    /// Sends this input's audio from now on, replacing any before it.
    pub fn play(&mut self, input: Input) -> Track {
        let playing = Arc::new(AtomicBool::new(true));
        *lock(&self.sending) = Some(Sending {
            buffer: input.buffer,
            playing: playing.clone(),
            dropped: input.dropped,
        });
        Track(playing)
    }

    /// Films from now on: the camera's frames, `width` by `height` at
    /// `fps`, go to the call through the sender, encoded in
    /// [`crate::stream_connection::SEND_CODEC`]. The camera stops when the
    /// sender drops, or with [`Self::stop_filming`].
    pub fn film(&mut self, width: u32, height: u32, fps: u32) -> CameraSender {
        let (sender, camera) = call_video::camera(width, height, fps);
        let _ = self.orders.send(Order::Film(Some(camera)));
        sender
    }

    pub fn stop_filming(&mut self) {
        let _ = self.orders.send(Order::Film(None));
    }

    /// Asks for these people's cameras, by Discord user id: each at a
    /// quality from 0 to 100, for a tile `pixels` in size. No others come.
    pub fn want_video(&mut self, wants: HashMap<u64, (u8, u32)>) {
        let _ = self.orders.send(Order::Want(wants));
    }

    /// How the call's connection is doing, as the call measures it.
    pub fn stats(&self) -> Arc<CallStats> {
        self.stats.clone()
    }

    /// Everyone else's camera frames, as they come, for one taker.
    pub fn video_frames(&mut self) -> Option<channel::Receiver<VideoFrame>> {
        self.video.take()
    }

    pub fn leave(&mut self) {
        let _ = self.orders.send(Order::Leave);
    }
}

impl Drop for Call {
    fn drop(&mut self) {
        self.leave();
    }
}

/// Who a call's connection identifies as. Its DAVE group is the channel.
fn identity(server: &VoiceServer, server_id: &str, channel_id: &str) -> Result<Identity> {
    Ok(Identity {
        endpoint: server.endpoint.clone(),
        server_id: server_id.to_owned(),
        user_id: server.user_id.clone(),
        session_id: server.session_id.clone(),
        token: server.token.clone(),
        streams: json!([{"type": "video", "rid": rtp::STREAM_RID, "quality": 100}]),
        user: id(&server.user_id)?,
        group: id(channel_id)?,
    })
}

fn id(value: &str) -> Result<u64> {
    match value.parse::<u64>() {
        Ok(id) if id != 0 => Ok(id),
        _ => bail!("{value} isn't a Discord id"),
    }
}

/// Starts and stops a playing input, such as the microphone between words.
/// Stopping sends five silent frames and says this account stopped
/// speaking, as Discord's clients do.
#[derive(Clone)]
pub struct Track(Arc<AtomicBool>);

impl Track {
    pub fn play(&self) -> Result<()> {
        self.0.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn pause(&self) -> Result<()> {
        self.0.store(false, Ordering::Relaxed);
        Ok(())
    }
}

/// Audio for [`Call::play`]: the microphone's buffer.
pub struct Input {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    dropped: Arc<AtomicU64>,
}

/// Where Yapfast writes the processed microphone, as mono f32 at 48 kHz.
#[derive(Clone)]
pub struct Microphone {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    /// Samples dropped as too old, for the call's log.
    dropped: Arc<AtomicU64>,
}

/// At most this much microphone audio waits to be sent; older audio is dropped
/// so a stall never turns into lasting delay. It leaves room for the 100 ms
/// Yapfast sends from just before its voice gate opens.
const MICROPHONE_BACKLOG: usize = SAMPLE_RATE as usize / 4;

impl Microphone {
    pub fn push(&self, samples: &[f32]) {
        let mut buffer = lock(&self.buffer);
        buffer.extend(samples);
        let excess = buffer.len().saturating_sub(MICROPHONE_BACKLOG);
        if excess > 0 {
            buffer.drain(..excess);
            self.dropped.fetch_add(excess as u64, Ordering::Relaxed);
        }
    }

    pub fn clear(&self) {
        lock(&self.buffer).clear();
    }
}

/// A live microphone input for [`Call::play`], and its writer. The call
/// takes 20 ms of it every 20 ms while it plays, and sends nothing on a
/// tick when less than that waits, as when the writer is behind.
pub fn live_microphone() -> (Microphone, Input) {
    let buffer: Arc<Mutex<VecDeque<f32>>> =
        Arc::new(Mutex::new(VecDeque::with_capacity(MICROPHONE_BACKLOG)));
    let dropped = Arc::new(AtomicU64::new(0));
    (
        Microphone {
            buffer: buffer.clone(),
            dropped: dropped.clone(),
        },
        Input { buffer, dropped },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backlog_keeps_the_newest_audio() {
        let (microphone, input) = live_microphone();
        microphone.push(&vec![0.0; MICROPHONE_BACKLOG]);
        microphone.push(&[1.0]);
        let buffer = lock(&input.buffer);
        assert_eq!(buffer.len(), MICROPHONE_BACKLOG);
        assert_eq!(buffer.back(), Some(&1.0));
    }

    #[test]
    fn a_dm_call_identifies_with_its_channel_and_keys_dave_by_it() {
        let server = VoiceServer {
            user_id: "1".into(),
            session_id: "s".into(),
            token: "t".into(),
            endpoint: "voice.example".into(),
        };
        let call = identity(&server, "20", "30").unwrap();
        assert_eq!((call.user, call.group), (1, 30));
        assert_eq!(call.server_id, "20");
        assert!(identity(&server, "20", "x").is_err());
    }
}
