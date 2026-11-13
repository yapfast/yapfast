//! A call's media connection: this account's microphone out as Opus, and
//! everyone else's in, decoded per speaker for Yapfast to mix; and cameras
//! both ways (`call_video.rs`). It runs on a thread of its own with a small
//! runtime, as a Go Live stream does, over the same voice connection
//! (`voice_connection.rs`).

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc as channel,
    },
    time::Duration,
};

use anyhow::Result;
use serde_json::{Value, json};
use tokio::{
    sync::{mpsc, oneshot},
    time::{Instant, MissedTickBehavior},
};

use crate::{
    call_stats::{CallStats, PING_EVERY, Reported},
    call_video::{self, Camera, Receiving, VideoFrame},
    dave::Dave,
    opus, rtp,
    voice::{FRAME, Listener, MOVE_GRACE, Surplus},
    voice_connection::{Closed, Connection, Identity, lock, receive, send_json},
};

/// The microphone's bit rate and mode: stereo at 128 kbit/s in Opus's music
/// mode, as songbird sent it when it ran calls, and as Yapfast's voice
/// processing was tuned with.
const BITRATE: i32 = 128_000;
/// 20 ms of Opus silence, which Discord's clients send five of when they
/// stop talking.
const SILENCE: [u8; 3] = [0xf8, 0xff, 0xfe];
const SILENT_FRAMES: u8 = 5;
/// Op 5's flag for a microphone.
const MICROPHONE: u8 = 1;
/// How often UDP carries a keepalive, so a quiet connection stays open
/// through NAT, as songbird, which ran calls before, sent one.
const KEEPALIVE: Duration = Duration::from_secs(5);
/// A speaker's audio starts playing once this many packets wait: 60 ms.
const START: usize = 3;
/// At most this many packets wait, 400 ms; beyond, the oldest go, so a
/// stall never turns into lasting delay.
const MOST: usize = 20;
/// A speaker missing this many packets in a row has stopped: 100 ms.
const GONE: u8 = 5;
/// A decoded frame quieter than this, in mean square of full scale, can be
/// skipped unheard: -50 dBFS. The end of a word, such as an "s" or a
/// vowel dying away, is often quieter than -40 dBFS, and skipping it cut
/// the word short.
const QUIET: f32 = 1e-5;
/// A frame skipped fades into the next over this many samples per channel,
/// 5 ms, so the skip doesn't click.
const SPLICE: usize = FRAME / 4;
/// At most this many datagrams waiting in the socket are read before a
/// tick plays: after a stall, several speakers' audio and a camera's video,
/// as they came.
const CATCH_UP: usize = 64;
/// And for at most this long, a tenth of a tick, so however much a packet
/// costs to read, the next tick's microphone isn't held up. What's left is
/// read between ticks.
const CATCH_UP_TIME: Duration = Duration::from_millis(2);
/// A speaker heard from no more in this long gives up their decoder.
const FORGET: Duration = Duration::from_secs(10);
/// A tick this late means the call stalled rather than woke late: its
/// schedule starts again from now instead of rushing out what it missed.
const STALLED: Duration = Duration::from_millis(100);
/// While a speaker's audio is dropped for one reason, it's logged again
/// after this many more packets: 10 s.
const STILL_DROPPING: u32 = 500;
/// The most SSRCs whose audio is dropped that a call keeps count of.
const MAX_DROPPED: usize = 64;
/// Gateway drops in a row that resuming or reconnecting may follow.
const RETRIES: u32 = 3;
/// How long a connection holds before its drop no longer counts with
/// those before.
const STEADY: Duration = Duration::from_secs(60);
/// How much longer each resume after the first waits.
const RESUME_WAIT: Duration = Duration::from_secs(1);
/// How often the call's audio is summed up in the log, at debug level.
const DIAGNOSE_EVERY: Duration = Duration::from_secs(5);
/// Ticks' lateness is counted in steps of this many microseconds, up to
/// 32 ms; the last step counts any later.
const LATE_STEP: u64 = 250;
const LATE_STEPS: usize = 128;
/// Ticks the microphone had no whole frame for are logged once, then every
/// this many, 5 s of them.
const SHORT_LOGGED: u32 = 250;

/// What the microphone sends: written by Yapfast, read every 20 ms.
pub(crate) struct Sending {
    pub buffer: Arc<Mutex<VecDeque<f32>>>,
    pub playing: Arc<AtomicBool>,
    /// Samples dropped from the buffer as too old, counted by its writer.
    pub dropped: Arc<AtomicU64>,
}

/// Orders for a running call.
pub(crate) enum Order {
    /// Connect to another voice server, as for another channel.
    Move(Identity, oneshot::Sender<Result<()>>),
    /// Send the camera's frames, or stop.
    Film(Option<Camera>),
    /// Ask for these people's cameras, at a quality from 0 to 100 and the
    /// pixels they show at; no others.
    Want(HashMap<u64, (u8, u32)>),
    Leave,
}

/// Starts a call on a thread of its own. The receiver says whether the
/// first connection succeeded.
pub(crate) fn start(
    identity: Identity,
    listener: Arc<dyn Listener>,
    sending: Arc<Mutex<Option<Sending>>>,
    video: channel::SyncSender<VideoFrame>,
    stats: Arc<CallStats>,
) -> Result<(mpsc::UnboundedSender<Order>, oneshot::Receiver<Result<()>>)> {
    let (orders, ordered) = mpsc::unbounded_channel();
    let (ready, connected) = oneshot::channel();
    std::thread::Builder::new()
        .name("yapfast-discord-call".into())
        .spawn(move || {
            listener.on_call_thread();
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = ready.send(Err(error.into()));
                    return;
                }
            };
            runtime.block_on(run(
                identity, listener, sending, video, stats, ordered, ready,
            ));
        })?;
    Ok((orders, connected))
}

/// Why the media loop stopped.
enum Ended {
    Left,
    Moved(Identity, oneshot::Sender<Result<()>>),
    Dropped(anyhow::Error),
}

async fn run(
    mut identity: Identity,
    listener: Arc<dyn Listener>,
    sending: Arc<Mutex<Option<Sending>>>,
    video: channel::SyncSender<VideoFrame>,
    stats: Arc<CallStats>,
    mut orders: mpsc::UnboundedReceiver<Order>,
    ready: oneshot::Sender<Result<()>>,
) {
    let mut connection = match Connection::open(&identity, codecs(), &[]).await {
        Ok(connection) => {
            let _ = ready.send(Ok(()));
            connection
        }
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let mut media = match Media::new(video, stats) {
        Ok(media) => media,
        Err(error) => {
            listener.disconnected(&format!("{error:#}"));
            return;
        }
    };
    let mut drops = 0;
    loop {
        let up = std::time::Instant::now();
        let mut ended = media
            .run(&mut connection, &listener, &sending, &mut orders)
            .await;
        // Disconnected from the call: kicked from voice, or moved to a
        // channel on another voice server, which Discord names just after.
        // The call waits a while for the move before it ends.
        if let Ended::Dropped(error) = &ended
            && closed(error) == Some(4014)
        {
            listener.disconnected(&format!("{error:#}"));
            ended = media.wait_for_move(&connection, &mut orders).await;
        }
        match ended {
            Ended::Left => {
                let _ = connection.ws.close(None).await;
                return;
            }
            Ended::Moved(to, reply) => {
                let _ = connection.ws.close(None).await;
                match Connection::open(&to, codecs(), &[]).await {
                    Ok(moved) => {
                        connection = moved;
                        identity = to;
                        media.reset();
                        drops = 0;
                        let _ = reply.send(Ok(()));
                    }
                    Err(error) => {
                        let reason = format!("{error:#}");
                        let _ = reply.send(Err(error));
                        listener.disconnected(&reason);
                        return;
                    }
                }
            }
            Ended::Dropped(error) => {
                // Drops count in a row: a connection that held a while starts
                // the count again, so a long call survives the odd blip.
                if up.elapsed() >= STEADY {
                    drops = 0;
                }
                drops += 1;
                // The session ended: final.
                if closed(&error) == Some(4006) || drops > RETRIES {
                    listener.disconnected(&format!("{error:#}"));
                    return;
                }
                tracing::info!(
                    error = format!("{error:#}"),
                    "the call's voice connection dropped"
                );
                // The first try goes at once, so a blip passes unheard; the
                // rest wait longer each time, rather than hammering a voice
                // server that's struggling.
                tokio::time::sleep(RESUME_WAIT * (drops - 1)).await;
                if let Err(resume) = connection.resume(&identity).await {
                    tracing::info!(
                        error = format!("{resume:#}"),
                        "couldn't resume the call; connecting again"
                    );
                    match Connection::open(&identity, codecs(), &[]).await {
                        Ok(again) => {
                            connection = again;
                            media.reset();
                        }
                        Err(again) => {
                            listener.disconnected(&format!("{error:#}; then {again:#}"));
                            return;
                        }
                    }
                }
            }
        }
    }
}

/// Whether a tick reads another datagram waiting in the socket, having read
/// `read` in `taken`.
fn catches_up(read: usize, taken: Duration) -> bool {
    read < CATCH_UP && taken < CATCH_UP_TIME
}

/// Takes the next 20 ms of the microphone into `pcm`, in both channels,
/// and says whether a whole frame waited; if not, it takes nothing.
fn take_frame(buffer: &mut VecDeque<f32>, pcm: &mut [f32]) -> bool {
    if buffer.len() < FRAME {
        return false;
    }
    for (out, sample) in pcm.chunks_exact_mut(2).zip(buffer.drain(..FRAME)) {
        out.fill(sample);
    }
    true
}

/// The voice gateway's close code, when it closed with one.
fn closed(error: &anyhow::Error) -> Option<u16> {
    error
        .downcast_ref::<Closed>()
        .and_then(|closed| closed.code)
}

/// What a call offers: Opus, H264 for the camera both ways, and VP8 to
/// receive.
fn codecs() -> Value {
    json!([
        {"name": "opus", "type": "audio", "priority": 1000, "payload_type": rtp::OPUS},
        {"name": "H264", "type": "video", "priority": 1000, "payload_type": rtp::H264,
         "rtx_payload_type": rtp::H264_RTX, "encode": true, "decode": true},
        {"name": "VP8", "type": "video", "priority": 2000, "payload_type": rtp::VP8,
         "rtx_payload_type": rtp::VP8_RTX, "encode": false, "decode": true},
    ])
}

/// The call's audio and video, kept across moves and reconnections.
struct Media {
    encoder: opus::Encoder,
    /// One frame of the microphone, in both channels, and its Opus packet.
    pcm: Vec<f32>,
    packet: Vec<u8>,
    sequence: u16,
    /// Goes on 20 ms every tick, sending or not, as a microphone's clock
    /// does. WebRTC's jitter buffer, which plays this account in Discord's
    /// clients, takes a pause the timestamps don't show for packets arriving
    /// that much late, raises its delay to match, and holds back the start of
    /// what comes after the pause.
    timestamp: u32,
    /// Whether op 5 says this account is speaking, and the silent frames
    /// still to send since it stopped.
    speaking: bool,
    silent: u8,
    /// Ticks that sent nothing for lack of a whole frame of the microphone.
    short: u32,
    /// Everyone else, by their audio SSRC, and whose each SSRC is.
    speakers: HashMap<u32, Speaker>,
    users: HashMap<u32, u64>,
    /// Why each SSRC's audio is being dropped and how many packets have
    /// gone, and whether the microphone was last sent end-to-end encrypted,
    /// as logged.
    dropped: HashMap<u32, (String, u32)>,
    encrypting: Option<bool>,
    /// This account's camera, while it films, and everyone else's.
    filming: Option<call_video::Sending>,
    watching: Receiving,
    /// How the connection is doing, for Yapfast to show, and what Discord's
    /// last receiver reports said about this account's packets.
    stats: Arc<CallStats>,
    reported: Reported,
    diagnostics: Diagnostics,
}

/// What a call's audio did over [`DIAGNOSE_EVERY`], for one line in the
/// log at debug level: counts the ticks add to, with no logging of their
/// own.
struct Diagnostics {
    since: std::time::Instant,
    /// Ticks that played the microphone, and of those, the ones with less
    /// than a frame of it waiting and with none, which send nothing.
    mic_frames: u32,
    mic_underfilled: u32,
    mic_empty: u32,
    /// The fewest samples of the microphone that waited as a tick took a
    /// frame, and those its buffer dropped as too old.
    mic_fewest: Option<usize>,
    mic_dropped: u64,
    /// Ticks by how late they came, in [`LATE_STEP`]s, and the latest.
    late: [u32; LATE_STEPS],
    latest: Duration,
    /// The most datagrams a tick read before playing, and the longest that
    /// reading took.
    batch_most: usize,
    batch_longest: Duration,
    /// Audio packets DAVE dropped.
    dave_dropped: u32,
}

impl Diagnostics {
    fn new() -> Self {
        Self {
            since: std::time::Instant::now(),
            mic_frames: 0,
            mic_underfilled: 0,
            mic_empty: 0,
            mic_fewest: None,
            mic_dropped: 0,
            late: [0; LATE_STEPS],
            latest: Duration::ZERO,
            batch_most: 0,
            batch_longest: Duration::ZERO,
            dave_dropped: 0,
        }
    }

    /// A tick that played the microphone, with `waiting` samples of it.
    fn microphone(&mut self, waiting: usize, dropped: u64) {
        self.mic_frames += 1;
        self.mic_underfilled += u32::from(waiting > 0 && waiting < FRAME);
        self.mic_empty += u32::from(waiting == 0);
        self.mic_fewest = Some(
            self.mic_fewest
                .map_or(waiting, |fewest| fewest.min(waiting)),
        );
        self.mic_dropped += dropped;
    }

    fn tick(&mut self, late: Duration) {
        let step = (late.as_micros() as u64 / LATE_STEP).min(LATE_STEPS as u64 - 1);
        self.late[step as usize] += 1;
        self.latest = self.latest.max(late);
    }

    fn read(&mut self, datagrams: usize, took: Duration) {
        self.batch_most = self.batch_most.max(datagrams);
        self.batch_longest = self.batch_longest.max(took);
    }

    /// How late 99 ticks in 100 came at most, to a step.
    fn late_p99(&self) -> Duration {
        let ticks: u32 = self.late.iter().sum();
        let mut counted = 0;
        for (step, count) in self.late.iter().enumerate() {
            counted += count;
            if counted * 100 >= ticks * 99 {
                return Duration::from_micros((step as u64 + 1) * LATE_STEP);
            }
        }
        Duration::ZERO
    }

    /// Logs what was counted since the last time, with what the speakers
    /// counted, and starts again.
    fn log(&mut self, speakers: &mut HashMap<u32, Speaker>) {
        let (mut skipped, mut concealed) = (0, 0);
        for speaker in speakers.values_mut() {
            skipped += std::mem::take(&mut speaker.skipped);
            concealed += std::mem::take(&mut speaker.concealed);
        }
        tracing::debug!(
            ticks = self.late.iter().sum::<u32>(),
            mic_frames = self.mic_frames,
            mic_underfilled = self.mic_underfilled,
            mic_empty = self.mic_empty,
            mic_fewest = self.mic_fewest,
            mic_dropped = self.mic_dropped,
            late_p99_us = self.late_p99().as_micros() as u64,
            late_most_us = self.latest.as_micros() as u64,
            batch_most = self.batch_most,
            batch_longest_us = self.batch_longest.as_micros() as u64,
            speakers = speakers.len(),
            skipped,
            concealed,
            dave_dropped = self.dave_dropped,
            "the call's audio over the last 5 s"
        );
        *self = Self::new();
    }
}

impl Media {
    fn new(video: channel::SyncSender<VideoFrame>, stats: Arc<CallStats>) -> Result<Self> {
        Ok(Self {
            encoder: opus::Encoder::new(2, opus::Application::Audio, BITRATE)?,
            pcm: vec![0.0; FRAME * 2],
            packet: vec![0; 1500],
            sequence: rand::random(),
            timestamp: rand::random(),
            speaking: false,
            silent: 0,
            short: 0,
            speakers: HashMap::new(),
            users: HashMap::new(),
            dropped: HashMap::new(),
            encrypting: None,
            filming: None,
            watching: Receiving::new(video),
            stats,
            reported: Reported::default(),
            diagnostics: Diagnostics::new(),
        })
    }

    /// A new connection: speakers, cameras and their SSRCs start over.
    fn reset(&mut self) {
        self.speakers.clear();
        self.users.clear();
        self.dropped.clear();
        self.encrypting = None;
        self.speaking = false;
        self.silent = 0;
        self.watching.reset();
        self.reported.reset();
    }

    async fn run(
        &mut self,
        connection: &mut Connection,
        listener: &Arc<dyn Listener>,
        sending: &Mutex<Option<Sending>>,
        orders: &mut mpsc::UnboundedReceiver<Order>,
    ) -> Ended {
        for (op, data) in std::mem::take(&mut connection.early) {
            self.learn(op, &data);
            self.watching.learn(op, &data);
        }
        // Each heartbeat measures the ping, which Discord's web client
        // measures this often.
        let interval = connection.interval.min(PING_EVERY);
        let mut heartbeat = tokio::time::interval_at(Instant::now() + interval, interval);
        let mut ticks = tokio::time::interval(Duration::from_millis(20));
        // Windows wakes timers on its 15.6 ms clock unless the process asks
        // for a finer one, as Yapfast does during calls, so a 20 ms tick
        // can come at 31 ms. Catching up on the ticks missed keeps 50 frames a
        // second each way; delaying them sent and played two thirds of that.
        ticks.set_missed_tick_behavior(MissedTickBehavior::Burst);
        let mut keepalive = tokio::time::interval(KEEPALIVE);
        let mut datagram = vec![0u8; 2048];
        let result: Result<Ended> = async {
            // A new connection hears about the camera again, and what's
            // wanted of others'.
            if let Some(filming) = self.filming.as_mut() {
                filming.announce(connection).await?;
            }
            self.watching.ask(connection).await?;
            loop {
                tokio::select! {
                    order = orders.recv() => match order {
                        None | Some(Order::Leave) => return Ok(Ended::Left),
                        Some(Order::Move(to, reply)) => return Ok(Ended::Moved(to, reply)),
                        Some(Order::Film(Some(camera))) => {
                            let mut filming = call_video::Sending::new(camera, connection);
                            filming.announce(connection).await?;
                            self.filming = Some(filming);
                        }
                        Some(Order::Film(None)) => {
                            if self.filming.take().is_some() {
                                call_video::Sending::clear(connection).await?;
                            }
                        }
                        Some(Order::Want(wants)) => {
                            self.watching.want(wants);
                            self.watching.ask(connection).await?;
                        }
                    },
                    frame = async {
                        match self.filming.as_mut() {
                            Some(filming) => filming.next().await,
                            None => std::future::pending().await,
                        }
                    } => match (frame, self.filming.as_mut()) {
                        (Some(frame), Some(filming)) => filming.send(connection, frame).await?,
                        // The camera closed.
                        _ => {
                            self.filming = None;
                            call_video::Sending::clear(connection).await?;
                        }
                    },
                    due = ticks.tick() => {
                        self.diagnostics.tick(due.elapsed());
                        if due.elapsed() > STALLED {
                            // The ticks a stall skips pass in the timestamp
                            // all the same.
                            let skipped = due.elapsed().as_millis() / 20;
                            self.timestamp = self
                                .timestamp
                                .wrapping_add(skipped as u32 * FRAME as u32);
                            ticks.reset();
                        }
                        self.send(connection, sending).await?;
                        // After a stall the ticks missed come at once, and
                        // the select may run them before the packets that
                        // came meanwhile are read: the speakers would then
                        // conceal audio that's sitting in the socket, and
                        // play it late afterwards.
                        let started = std::time::Instant::now();
                        let mut read = 0;
                        while catches_up(read, started.elapsed()) {
                            match connection.udp.try_recv(&mut datagram) {
                                Ok(length) => self.receive(connection, &datagram[..length]).await?,
                                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                                Err(error) => return Err(error.into()),
                            }
                            read += 1;
                        }
                        self.diagnostics.read(read, started.elapsed());
                        self.hear(listener);
                        self.watching.repair(connection).await?;
                        if self.diagnostics.since.elapsed() >= DIAGNOSE_EVERY {
                            self.diagnostics.log(&mut self.speakers);
                        }
                    }
                    _ = heartbeat.tick() => {
                        connection.heartbeat().await?;
                        self.secure(connection);
                    }
                    _ = keepalive.tick() => {
                        let mut packet = [0u8; 8];
                        packet[..4].copy_from_slice(&connection.ssrc.to_be_bytes());
                        connection.udp.send(&packet).await?;
                    }
                    message = receive(&mut connection.ws, &mut connection.seq) => {
                        let handled = connection.handle(message?).await?;
                        // DAVE's messages change the group, and its code.
                        self.secure(connection);
                        if let Some((op, data)) = handled {
                            if op == 6 && let Some(round_trip) = Connection::acked(&data) {
                                self.stats.ping(round_trip);
                            }
                            self.learn(op, &data);
                            if self.watching.learn(op, &data) {
                                self.watching.ask(connection).await?;
                            }
                        }
                    }
                    length = connection.udp.recv(&mut datagram) => {
                        self.receive(connection, &datagram[..length?]).await?;
                    }
                }
            }
        }
        .await;
        result.unwrap_or_else(Ended::Dropped)
    }

    /// Waits, with no connection, for the call to move to another voice
    /// server, for [`MOVE_GRACE`]; orders meanwhile apply once it's there.
    /// Without a move it has ended.
    async fn wait_for_move(
        &mut self,
        connection: &Connection,
        orders: &mut mpsc::UnboundedReceiver<Order>,
    ) -> Ended {
        let given_up = tokio::time::sleep(MOVE_GRACE);
        tokio::pin!(given_up);
        loop {
            tokio::select! {
                order = orders.recv() => match order {
                    None | Some(Order::Leave) => return Ended::Left,
                    Some(Order::Move(to, reply)) => return Ended::Moved(to, reply),
                    // Announced on the new connection.
                    Some(Order::Film(camera)) => {
                        self.filming = camera.map(|camera| call_video::Sending::new(camera, connection));
                    }
                    Some(Order::Want(wants)) => self.watching.want(wants),
                },
                () = &mut given_up => return Ended::Left,
            }
        }
    }

    /// Notes whether frames leave end-to-end encrypted, and the group's
    /// privacy code.
    fn secure(&self, connection: &Connection) {
        self.stats
            .encryption(connection.dave.encrypting(), connection.dave.privacy_code());
    }

    /// Learns whose audio each SSRC carries from their speaking messages
    /// and their video ones. Someone already talking when Yapfast joins may
    /// send no speaking message again; their video message, sent for
    /// everyone in the call, names their audio too.
    fn learn(&mut self, op: u64, data: &Value) {
        let ssrc = match op {
            5 => &data["ssrc"],
            12 => &data["audio_ssrc"],
            _ => return,
        };
        if let (Some(user), Some(ssrc)) = (
            data["user_id"].as_str().and_then(|id| id.parse().ok()),
            ssrc.as_u64().filter(|&ssrc| ssrc != 0),
        ) {
            self.users.insert(ssrc as u32, user);
        }
    }

    /// Logs why a speaker's audio is dropped when the reason changes, and
    /// every 10 s while it lasts, since it otherwise vanishes unheard. The
    /// first dropped frame's size and Opus TOC byte tell silence from speech.
    fn report_drop(&mut self, ssrc: u32, reason: &str, dave: &Dave, frame: &[u8]) {
        let full = self.dropped.len() >= MAX_DROPPED;
        match self.dropped.get_mut(&ssrc) {
            Some((last, count)) if last == reason => {
                *count += 1;
                if *count % STILL_DROPPING == 0 {
                    tracing::info!(
                        ssrc,
                        reason,
                        dropped = *count,
                        dave = dave.status(),
                        "still dropping a speaker's audio in a call"
                    );
                }
            }
            // Packets can name any SSRC: past this many, a new one isn't
            // kept or logged.
            None if full => {}
            _ => {
                tracing::info!(
                    ssrc,
                    reason,
                    size = frame.len(),
                    toc = frame.first().copied(),
                    dave = dave.status(),
                    "dropping a speaker's audio in a call"
                );
                self.dropped.insert(ssrc, (reason.to_owned(), 1));
            }
        }
    }

    /// Sends 20 ms of the microphone while it plays, then five silent
    /// frames when it stops. Each call is a tick. A tick with less than a
    /// whole frame of the microphone waiting, as when its thread is held
    /// up, sends nothing: others' clients conceal the missing packet, which
    /// sounds smoother than a frame cut short with silence, and what waited
    /// goes in the next one.
    async fn send(
        &mut self,
        connection: &mut Connection,
        sending: &Mutex<Option<Sending>>,
    ) -> Result<()> {
        let taken = {
            let sending = lock(sending);
            match sending.as_ref() {
                Some(sending) if sending.playing.load(Ordering::Relaxed) => {
                    let mut buffer = lock(&sending.buffer);
                    let dropped = sending.dropped.swap(0, Ordering::Relaxed);
                    self.diagnostics.microphone(buffer.len(), dropped);
                    Some(take_frame(&mut buffer, &mut self.pcm))
                }
                _ => None,
            }
        };
        if taken == Some(false) {
            self.short += 1;
            if self.short % SHORT_LOGGED == 1 {
                tracing::debug!(
                    ticks = self.short,
                    "the call's microphone had no whole frame to send"
                );
            }
            self.timestamp = self.timestamp.wrapping_add(FRAME as u32);
            return Ok(());
        }
        let playing = taken == Some(true);
        let marker = playing && !self.speaking;
        if marker {
            send_json(
                &mut connection.ws,
                5,
                json!({"speaking": MICROPHONE, "delay": 0, "ssrc": connection.ssrc}),
            )
            .await?;
            self.speaking = true;
        }
        if playing && self.encrypting != Some(connection.dave.encrypting()) {
            self.encrypting = Some(connection.dave.encrypting());
            tracing::info!(
                encrypting = connection.dave.encrypting(),
                dave = connection.dave.status(),
                "the call's microphone is sending"
            );
        }
        if playing {
            let length = self.encoder.encode_float(&self.pcm, &mut self.packet)?;
            let opus = &self.packet[..length];
            if let Some(payload) = connection.dave.encrypt_opus(opus) {
                self.transmit(connection, &payload, marker).await?;
            }
            self.silent = SILENT_FRAMES;
        } else if self.silent > 0 {
            if let Some(payload) = connection.dave.encrypt_opus(&SILENCE) {
                self.transmit(connection, &payload, false).await?;
            }
            self.silent -= 1;
            if self.silent == 0 {
                self.speaking = false;
                send_json(
                    &mut connection.ws,
                    5,
                    json!({"speaking": 0, "delay": 0, "ssrc": connection.ssrc}),
                )
                .await?;
            }
        }
        self.timestamp = self.timestamp.wrapping_add(FRAME as u32);
        Ok(())
    }

    async fn transmit(
        &mut self,
        connection: &mut Connection,
        payload: &[u8],
        marker: bool,
    ) -> Result<()> {
        let mut packet = rtp::opus_packet(
            connection.ssrc,
            self.sequence,
            self.timestamp,
            marker,
            payload,
        );
        self.sequence = self.sequence.wrapping_add(1);
        let clear = rtp::rtp_clear_len(&packet).unwrap_or(packet.len());
        connection.transport.seal(&mut packet, clear)?;
        connection.udp.send(&packet).await?;
        Ok(())
    }

    /// Queues a received Opus packet for its speaker, hands a camera's
    /// packet on, and reads RTCP about this account's packets: their loss,
    /// and what the camera's viewers ask of it.
    async fn receive(&mut self, connection: &mut Connection, datagram: &[u8]) -> Result<()> {
        if rtp::is_rtcp(datagram) {
            if let Some(rtcp) = connection.transport.open(datagram, rtp::RTCP_CLEAR) {
                let ours = [connection.ssrc, connection.video];
                self.reported.learn(&self.stats, &rtcp, &ours);
                if let Some(filming) = self.filming.as_mut() {
                    filming.feedback(connection, &rtcp).await?;
                }
            }
            return Ok(());
        }
        let Some(clear) = rtp::rtp_clear_len(datagram) else {
            return Ok(());
        };
        let Some(plain) = connection.transport.open(datagram, clear) else {
            return Ok(());
        };
        let Some(packet) = rtp::parse(&plain) else {
            return Ok(());
        };
        if packet.payload_type != rtp::OPUS {
            self.watching.receive(connection, &packet).await?;
            return Ok(());
        }
        // DAVE keys each sender's frames by who they are, which only the
        // voice gateway tells.
        let Some(&user) = self.users.get(&packet.ssrc) else {
            self.report_drop(
                packet.ssrc,
                "whose SSRC it is is unknown",
                &connection.dave,
                packet.payload,
            );
            return Ok(());
        };
        // Silence passes in the clear even while sealed audio fails.
        let sealed = packet.payload.ends_with(&[0xfa, 0xfa]);
        let Some(opus) = connection.dave.decrypt_opus(user, packet.payload.to_vec()) else {
            self.diagnostics.dave_dropped += 1;
            let reason = connection.dave.unreadable.take().unwrap_or_default();
            self.report_drop(packet.ssrc, &reason, &connection.dave, packet.payload);
            // Its sequence number carried nothing to play, as Discord's
            // client, which plays by timestamp, takes a frame its decryptor
            // drops, rather than a frame lost to conceal.
            if let Some(speaker) = self.speakers.get_mut(&packet.ssrc) {
                speaker.skip(packet.sequence, packet.timestamp);
            }
            return Ok(());
        };
        if sealed && let Some((reason, dropped)) = self.dropped.remove(&packet.ssrc) {
            tracing::info!(
                ssrc = packet.ssrc,
                user,
                reason,
                dropped,
                "hearing a speaker again in a call"
            );
        }
        let speaker = match self.speakers.entry(packet.ssrc) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => match Speaker::new(user) {
                Ok(speaker) => entry.insert(speaker),
                Err(error) => {
                    tracing::warn!(%error, "couldn't decode a speaker");
                    return Ok(());
                }
            },
        };
        speaker.user = user;
        speaker.push(packet.sequence, packet.timestamp, opus);
        Ok(())
    }

    /// Plays everyone's next 20 ms, and forgets those long gone.
    fn hear(&mut self, listener: &Arc<dyn Listener>) {
        let now = std::time::Instant::now();
        self.speakers
            .retain(|_, speaker| now.duration_since(speaker.heard) < FORGET);
        let voices: Vec<(u64, &[i16])> = self
            .speakers
            .values_mut()
            .filter_map(|speaker| Some((speaker.user, speaker.next()?)))
            .collect();
        listener.tick(&voices);
    }
}

/// One speaker's audio on its way to the mix: packets in order, waiting a
/// little for those that come late, and decoded one 20 ms frame a tick.
struct Speaker {
    user: u64,
    decoder: opus::Decoder,
    /// Packets by sequence number, oldest first, with their RTP timestamps.
    /// An empty one is a packet end-to-end encryption dropped, which plays as
    /// nothing, unless it held 20 ms of its own.
    waiting: VecDeque<(u16, u32, Vec<u8>)>,
    /// The sequence number to play next, while playing.
    next: Option<u16>,
    /// The RTP timestamp that follows what played last: a dropped packet
    /// with it, before a packet to play with a later one, held that much of
    /// the sender's audio.
    follows: Option<u32>,
    /// Packets missing in a row.
    missing: u8,
    /// Frames skipped for the one after, and concealed, for the log.
    skipped: u32,
    concealed: u32,
    /// Whether the packets wait longer than their lateness needs, as a
    /// sender whose clock runs fast makes them.
    surplus: Surplus,
    /// The last packet decoded, interleaved stereo: room for the longest an
    /// Opus packet can be, 120 ms. Voice comes in 20 ms packets, but music
    /// bots that pass a source's Opus through can send 40 or 60 ms ones.
    pcm: Vec<i16>,
    /// What of `pcm` is still to play, 20 ms a tick.
    left: std::ops::Range<usize>,
    heard: std::time::Instant,
}

/// The most samples per channel an Opus packet decodes to: 120 ms.
const LONGEST: usize = FRAME * 6;

/// Interleaved channels of received audio.
const CHANNELS: usize = crate::voice::CHANNELS as usize;

/// Fades the start of a skipped frame, which carries on from what played
/// before it, into the start of the frame that plays instead.
fn splice(skipped: &[i16], into: &mut [i16]) {
    for (at, (sample, old)) in into.iter_mut().zip(skipped).enumerate() {
        let weight = (at / CHANNELS) as f32 / SPLICE as f32;
        *sample = (f32::from(*old) * (1.0 - weight) + f32::from(*sample) * weight).round() as i16;
    }
}

/// Whether sequence number `a` comes before `b`, across wrapping.
fn before(a: u16, b: u16) -> bool {
    a != b && b.wrapping_sub(a) < 0x8000
}

impl Speaker {
    fn new(user: u64) -> Result<Self> {
        Ok(Self {
            user,
            decoder: opus::Decoder::new(crate::voice::CHANNELS)?,
            waiting: VecDeque::with_capacity(MOST + 1),
            next: None,
            follows: None,
            missing: 0,
            skipped: 0,
            concealed: 0,
            surplus: Surplus::with_cushion(START - 1),
            pcm: vec![0; LONGEST * usize::from(crate::voice::CHANNELS)],
            left: 0..0,
            heard: std::time::Instant::now(),
        })
    }

    fn push(&mut self, sequence: u16, timestamp: u32, opus: Vec<u8>) {
        self.heard = std::time::Instant::now();
        self.insert(sequence, timestamp, opus);
    }

    /// Notes that packet `sequence` came but has nothing to play, so it
    /// isn't concealed as lost: senders' packets that end-to-end encryption
    /// drops, which can come many to a frame.
    fn skip(&mut self, sequence: u16, timestamp: u32) {
        self.insert(sequence, timestamp, Vec::new());
    }

    fn insert(&mut self, sequence: u16, timestamp: u32, opus: Vec<u8>) {
        if self.next.is_some_and(|next| before(sequence, next)) {
            return;
        }
        let at = self
            .waiting
            .iter()
            .rposition(|(waiting, _, _)| !before(sequence, *waiting))
            .map_or(0, |at| at + 1);
        if at > 0 && self.waiting[at - 1].0 == sequence {
            // A packet with something to play takes a skipped one's place.
            if self.waiting[at - 1].2.is_empty() {
                self.waiting[at - 1] = (sequence, timestamp, opus);
            }
            return;
        }
        self.waiting.insert(at, (sequence, timestamp, opus));
        // Skipped packets don't count, as they hold nothing to wait for,
        // but a run of them is bounded too. A stopped speaker stays
        // stopped: dropped packets piling up mustn't start it.
        while self.queued() > MOST || self.waiting.len() > MOST * 4 {
            self.waiting.pop_front();
            if self.next.is_some() {
                self.next = self.waiting.front().map(|(sequence, _, _)| *sequence);
            }
        }
    }

    /// The packets waiting with something to play.
    fn queued(&self) -> usize {
        self.waiting
            .iter()
            .filter(|(_, _, opus)| !opus.is_empty())
            .count()
    }

    /// Whether a dropped packet with `timestamp` held 20 ms of the sender's
    /// audio: it follows what played, and the next packet to play comes
    /// after it. Dropped packets that share a timestamp with a packet to
    /// play, or come in the same 20 ms as many do, held none.
    fn held_time(&self, timestamp: u32) -> bool {
        self.follows == Some(timestamp)
            && self
                .waiting
                .iter()
                .find(|(_, _, opus)| !opus.is_empty())
                .is_some_and(|(_, later, _)| (later.wrapping_sub(timestamp) as i32) > 0)
    }

    /// The next 20 ms, or nothing while waiting to start or once stopped.
    fn next(&mut self) -> Option<&[i16]> {
        if self.left.is_empty() {
            let samples = self.decode_next()?;
            self.left = 0..samples * usize::from(crate::voice::CHANNELS);
        }
        let start = self.left.start;
        let end = self
            .left
            .end
            .min(start + FRAME * usize::from(crate::voice::CHANNELS));
        self.left.start = end;
        Some(&self.pcm[start..end])
    }

    /// Decodes the next packet into `pcm`, or conceals a missing one, and
    /// says how many samples per channel it made.
    fn decode_next(&mut self) -> Option<usize> {
        let frame = FRAME * usize::from(crate::voice::CHANNELS);
        let next = match self.next {
            Some(next) => next,
            None if self.queued() >= START => self.waiting.front()?.0,
            None => return None,
        };
        let mut next = next;
        loop {
            match self.waiting.front() {
                Some((sequence, _, _)) if before(*sequence, next) => {}
                // Dropped, but it held 20 ms of the sender's audio: that's
                // concealed in its place, as Discord's clients, which play
                // by timestamp, conceal it, rather than played from the next
                // packet early, which would leave the cushion short.
                Some((sequence, timestamp, opus))
                    if *sequence == next && opus.is_empty() && self.held_time(*timestamp) =>
                {
                    self.waiting.pop_front();
                    break;
                }
                // Skipped: the one after plays in its place.
                Some((sequence, _, opus)) if *sequence == next && opus.is_empty() => {
                    next = next.wrapping_add(1);
                }
                _ => break,
            }
            self.waiting.pop_front();
        }
        self.next = Some(next);
        let decoded = match self.waiting.front() {
            Some((sequence, _, _)) if *sequence == next => {
                self.missing = 0;
                let mut decoded = self.decode(next);
                if let Ok(samples) = decoded
                    && self.skips(samples)
                {
                    let mut skipped = [0i16; SPLICE * CHANNELS];
                    skipped.copy_from_slice(&self.pcm[..SPLICE * CHANNELS]);
                    decoded = self.decode(next.wrapping_add(1));
                    if decoded.is_ok() {
                        splice(&skipped, &mut self.pcm);
                        self.skipped += 1;
                    }
                }
                decoded
            }
            waiting => {
                self.missing = self.missing.saturating_add(1);
                if self.missing >= GONE && waiting.is_none() {
                    // Stopped talking: wait for them to start again.
                    self.next = None;
                    self.follows = None;
                    self.missing = 0;
                    self.surplus = Surplus::with_cushion(START - 1);
                    return None;
                }
                let concealed = match waiting {
                    // Nothing after it has come either: it's late rather
                    // than lost, so it plays next. Each frame concealed so
                    // makes the queue a frame longer, as far as the packets'
                    // lateness, or the sender's slow clock, needs.
                    None => self.decoder.conceal(&mut self.pcm[..frame]),
                    // The next packet carries this one's forward error
                    // correction.
                    Some((sequence, _, opus))
                        if *sequence == next.wrapping_add(1) && !opus.is_empty() =>
                    {
                        self.next = Some(next.wrapping_add(1));
                        self.decoder.decode(opus, &mut self.pcm[..frame], true)
                    }
                    Some(_) => {
                        self.next = Some(next.wrapping_add(1));
                        self.decoder.conceal(&mut self.pcm[..frame])
                    }
                };
                if let Ok(samples) = concealed {
                    self.concealed += 1;
                    self.follows = self
                        .follows
                        .map(|follows| follows.wrapping_add(samples as u32));
                }
                concealed
            }
        };
        match decoded {
            Ok(samples) => Some(samples),
            Err(error) => {
                tracing::debug!(%error, "a call's audio packet didn't decode");
                None
            }
        }
    }

    /// Decodes packet `sequence`, the first waiting, into `pcm`.
    fn decode(&mut self, sequence: u16) -> Result<usize> {
        let (_, timestamp, opus) = self.waiting.pop_front().expect("a waiting packet");
        self.next = Some(sequence.wrapping_add(1));
        let decoded = self.decoder.decode(&opus, &mut self.pcm, false);
        if let Ok(samples) = decoded {
            self.follows = Some(timestamp.wrapping_add(samples as u32));
        }
        if decoded.is_err() {
            tracing::debug!(
                sequence,
                length = opus.len(),
                head = ?&opus[..opus.len().min(12)],
                tail = ?&opus[opus.len().saturating_sub(4)..],
                "an undecodable call audio packet"
            );
        }
        decoded
    }

    /// Counts what waits after a packet was taken, beyond the 40 ms that
    /// waits once playing starts, and says whether to skip
    /// the frame it decoded to, `samples` long, for the one after.
    fn skips(&mut self, samples: usize) -> bool {
        self.surplus.note(self.queued());
        let following = self.next.is_some_and(|next| {
            self.waiting
                .front()
                .is_some_and(|(sequence, _, opus)| *sequence == next && !opus.is_empty())
        });
        // A long packet, as music bots send, would skip more than a frame.
        if !following || samples != FRAME {
            return false;
        }
        let played = &self.pcm[..samples * usize::from(crate::voice::CHANNELS)];
        self.surplus.skips(|| {
            let power = played
                .iter()
                .map(|sample| (f32::from(*sample) / 32768.0).powi(2))
                .sum::<f32>()
                / played.len() as f32;
            power < QUIET
        })
    }
}

#[cfg(test)]
mod tests {
    /// The RTP timestamp of a sender's packet `sequence`, one 20 ms frame a
    /// packet.
    fn at(sequence: u16) -> u32 {
        u32::from(sequence) * FRAME as u32
    }

    use super::*;

    #[test]
    fn a_tick_takes_only_a_whole_frame_of_the_microphone() {
        let mut pcm = vec![1.0; FRAME * 2];
        let mut buffer: VecDeque<f32> = (0..FRAME / 2).map(|at| at as f32).collect();
        assert!(!take_frame(&mut buffer, &mut pcm));
        assert_eq!(
            buffer.len(),
            FRAME / 2,
            "what waited stays for the next tick"
        );
        buffer.extend((FRAME / 2..FRAME * 3 / 2).map(|at| at as f32));
        assert!(take_frame(&mut buffer, &mut pcm));
        assert_eq!(&pcm[..4], &[0.0, 0.0, 1.0, 1.0]);
        assert_eq!(pcm[FRAME * 2 - 1], (FRAME - 1) as f32);
        assert_eq!(buffer.front(), Some(&(FRAME as f32)));
    }

    fn tone(encoder: &mut opus::Encoder, frame: usize) -> Vec<u8> {
        let pcm: Vec<f32> = (0..FRAME)
            .map(|at| {
                ((frame * FRAME + at) as f32 * 330.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5
            })
            .collect();
        let mut packet = vec![0; 1500];
        let length = encoder.encode_float(&pcm, &mut packet).unwrap();
        packet.truncate(length);
        packet
    }

    #[test]
    fn a_speaker_waits_reorders_conceals_and_stops() {
        let mut encoder = opus::Encoder::new(1, opus::Application::Voice, BITRATE).unwrap();
        let mut speaker = Speaker::new(7).unwrap();
        let packets: Vec<Vec<u8>> = (0..8).map(|frame| tone(&mut encoder, frame)).collect();
        // Two packets aren't enough to start.
        speaker.push(65_534, at(65_534), packets[0].clone());
        speaker.push(65_535, at(65_535), packets[1].clone());
        assert!(speaker.next().is_none(), "waits for 60 ms");
        // The fourth arrives before the third, across the wrap.
        speaker.push(1, at(1), packets[3].clone());
        speaker.push(0, at(0), packets[2].clone());
        for _ in 0..4 {
            assert_eq!(speaker.next().map(<[i16]>::len), Some(FRAME * 2));
        }
        assert!(speaker.waiting.is_empty());
        // A packet that comes too late is dropped, not played.
        speaker.push(65_535, at(65_535), packets[1].clone());
        assert!(speaker.waiting.is_empty());
        // Packet 2 is lost: it's concealed from packet 3's error correction.
        speaker.push(3, at(3), packets[5].clone());
        assert!(speaker.next().is_some(), "packet 2, rebuilt");
        assert!(speaker.next().is_some(), "packet 3");
        // Then silence: concealed briefly, then the speaker stops.
        let played = std::iter::from_fn(|| speaker.next().map(|_| ())).count();
        assert_eq!(played, usize::from(GONE) - 1);
        assert!(speaker.next.is_none());
    }

    #[test]
    fn the_end_of_speech_plays_to_the_last_frame() {
        // 1.5 s of speech, a word dying away at -45 dBFS, then the five
        // silent frames Discord's clients send as they stop, one packet a
        // tick. None is skipped: the 40 ms waiting is the cushion, not a
        // surplus, and the dying word isn't quiet enough to go unheard.
        let mut encoder = opus::Encoder::new(1, opus::Application::Voice, BITRATE).unwrap();
        let mut speaker = Speaker::new(7).unwrap();
        let mut packets: Vec<Vec<u8>> = (0..75).map(|frame| tone(&mut encoder, frame)).collect();
        for frame in 75..78 {
            let pcm: Vec<f32> = (0..FRAME)
                .map(|at| ((frame * FRAME + at) as f32 * 0.04).sin() * 0.008)
                .collect();
            let mut packet = vec![0; 1500];
            let length = encoder.encode_float(&pcm, &mut packet).unwrap();
            packet.truncate(length);
            packets.push(packet);
        }
        packets.extend(std::iter::repeat_n(
            SILENCE.to_vec(),
            usize::from(SILENT_FRAMES),
        ));
        let mut played = 0;
        for tick in 0..packets.len() + 20 {
            if let Some(packet) = packets.get(tick) {
                speaker.push(tick as u16, at(tick as u16), packet.clone());
            }
            played += usize::from(speaker.next().is_some());
        }
        // Then concealed briefly before the speaker stops.
        assert_eq!(played, packets.len() + usize::from(GONE) - 1);
    }

    #[test]
    fn a_late_packet_is_concealed_then_played() {
        let mut encoder = opus::Encoder::new(1, opus::Application::Voice, BITRATE).unwrap();
        let mut speaker = Speaker::new(7).unwrap();
        let packets: Vec<Vec<u8>> = (0..5).map(|frame| tone(&mut encoder, frame)).collect();
        for sequence in 0..3 {
            speaker.push(
                sequence,
                at(sequence),
                packets[usize::from(sequence)].clone(),
            );
        }
        for _ in 0..3 {
            assert!(speaker.next().is_some());
        }
        // Packet 3 is late: its 20 ms is concealed rather than silent.
        let concealed = speaker.next().expect("a concealed frame");
        assert!(concealed.iter().any(|sample| *sample != 0));
        // It comes, and plays next, with packet 4 waiting behind it.
        speaker.push(3, at(3), packets[3].clone());
        speaker.push(4, at(4), packets[4].clone());
        assert!(speaker.next().is_some());
        assert_eq!(speaker.next, Some(4));
        assert_eq!(speaker.waiting.len(), 1);
    }

    #[test]
    fn a_skipped_frame_fades_into_the_next() {
        let skipped = [1000i16; SPLICE * CHANNELS];
        let mut next = vec![-1000i16; FRAME * CHANNELS];
        splice(&skipped, &mut next);
        assert_eq!(&next[..2], &[1000, 1000]);
        assert!(next.windows(2).all(|pair| pair[1] <= pair[0]), "no jump");
        assert!(
            next[SPLICE * CHANNELS..]
                .iter()
                .all(|sample| *sample == -1000)
        );
    }

    /// A skipped frame's start carries on from the frame that played before
    /// it, as the decoder made it right after, so the splice starts where
    /// playing left off: even into a frame of the opposite phase, it moves
    /// no faster than the tone itself.
    #[test]
    fn a_splice_carries_on_from_what_played() {
        let mut encoder = opus::Encoder::new(1, opus::Application::Voice, BITRATE).unwrap();
        let mut speaker = Speaker::new(7).unwrap();
        // A tone, inverted from frame 4, which plays in frame 3's place.
        for sequence in 0..6u16 {
            let sign = if sequence >= 4 { -1.0 } else { 1.0 };
            let pcm: Vec<f32> = (0..FRAME)
                .map(|at| {
                    let at = usize::from(sequence) * FRAME + at;
                    sign * (at as f32 * 330.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5
                })
                .collect();
            let mut packet = vec![0; 1500];
            let length = encoder.encode_float(&pcm, &mut packet).unwrap();
            packet.truncate(length);
            speaker.push(sequence, at(sequence), packet);
        }
        let mut played: Vec<i16> = Vec::new();
        for _ in 0..3 {
            played.extend_from_slice(speaker.next().unwrap());
        }
        // Frames to spare for a whole second: the next one goes.
        speaker.surplus = Surplus::with_cushion(START - 1);
        for _ in 0..50 {
            speaker.surplus.note(10);
        }
        played.extend_from_slice(speaker.next().unwrap());
        assert_eq!(speaker.next, Some(5), "frame 3 was skipped");
        let steps = |samples: &[i16]| {
            samples
                .iter()
                .zip(&samples[CHANNELS..])
                .map(|(a, b)| (i32::from(*b) - i32::from(*a)).abs())
                .max()
                .unwrap()
        };
        let joined = 3 * FRAME * CHANNELS;
        // The tone's own steps, after the decoder settles.
        let tone = steps(&played[FRAME * CHANNELS..joined]);
        let splice = steps(&played[joined - CHANNELS..joined + SPLICE * CHANNELS + CHANNELS]);
        assert!(
            splice <= tone * 5 / 4,
            "the splice steps {splice}, the tone {tone}"
        );
    }

    #[test]
    fn a_long_packet_plays_over_as_many_ticks() {
        // A music bot passing a source's Opus through can send 60 ms
        // packets: each plays as three 20 ms frames, then the next.
        let mut encoder = opus::Encoder::new(2, opus::Application::Audio, BITRATE).unwrap();
        let mut speaker = Speaker::new(7).unwrap();
        let music: Vec<f32> = (0..FRAME * 3 * 2)
            .map(|at| (at as f32 * 0.01).sin() * 0.25)
            .collect();
        for sequence in 0..START as u16 {
            let mut packet = vec![0; 4000];
            let length = encoder.encode_float(&music, &mut packet).unwrap();
            packet.truncate(length);
            speaker.push(sequence, at(sequence), packet);
        }
        for tick in 0..START * 3 {
            assert_eq!(
                speaker.next().map(<[i16]>::len),
                Some(FRAME * 2),
                "tick {tick}"
            );
            assert_eq!(speaker.waiting.len(), START - 1 - tick / 3);
        }
    }

    #[test]
    fn learns_whose_audio_from_speaking_and_video_messages() {
        let (frames, _) = channel::sync_channel(1);
        let mut media = Media::new(frames, Arc::default()).unwrap();
        media.learn(5, &json!({"user_id": "7", "ssrc": 100, "speaking": 1}));
        // Already talking when Yapfast joined: only op 12 names their audio.
        media.learn(
            12,
            &json!({"user_id": "8", "audio_ssrc": 200, "video_ssrc": 0, "streams": []}),
        );
        media.learn(
            12,
            &json!({"user_id": "9", "audio_ssrc": 0, "video_ssrc": 0}),
        );
        assert_eq!(media.users, HashMap::from([(100, 7), (200, 8)]));
    }

    /// Plays `seconds` of someone talking without a pause, their packets
    /// sent `ppm` slower than the ticks (faster if negative) and each
    /// arriving up to `jitter` ms late, and returns how many came too late
    /// to play and the most that waited.
    fn talk_without_a_pause(ppm: f64, jitter: u64, seconds: u64) -> (usize, usize) {
        let mut encoder = opus::Encoder::new(1, opus::Application::Voice, BITRATE).unwrap();
        let mut speaker = Speaker::new(7).unwrap();
        // Speech has quiet moments: 60 ms in every 200 here.
        let packets: Vec<Vec<u8>> = (0..50)
            .map(|frame| {
                if frame % 10 >= 7 {
                    let mut packet = vec![0; 1500];
                    let length = encoder.encode_float(&[0.0; FRAME], &mut packet).unwrap();
                    packet.truncate(length);
                    packet
                } else {
                    tone(&mut encoder, frame)
                }
            })
            .collect();
        let mut random = 12_345u64;
        let mut arrivals: Vec<(u64, u16)> = (0..seconds * 50)
            .map(|index| {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                let late = (random >> 33) % (jitter * 1000 + 1);
                let sent = (index as f64 * 20_000.0 * (1.0 + ppm * 1e-6)) as u64;
                (sent + late, index as u16)
            })
            .collect();
        arrivals.sort();
        let (mut late, mut most) = (0, 0);
        let mut arrivals = arrivals.into_iter().peekable();
        for tick in 0..seconds * 50 {
            while let Some((_, sequence)) = arrivals.next_if(|(at, _)| *at <= tick * 20_000) {
                if speaker.next.is_some_and(|next| before(sequence, next)) {
                    late += 1;
                }
                speaker.push(
                    sequence,
                    at(sequence),
                    packets[usize::from(sequence) % 50].clone(),
                );
            }
            most = most.max(speaker.waiting.len());
            speaker.next();
        }
        (late, most)
    }

    #[test]
    fn a_slow_senders_clock_doesnt_make_them_choppy() {
        // A sound card 50 ppm slow takes 90 ms in 30 minutes; 2000 ppm
        // takes as much in 45 s.
        let (late, _) = talk_without_a_pause(2000.0, 15, 120);
        assert!(late < 10, "{late} packets came too late to play");
    }

    #[test]
    fn a_fast_senders_clock_doesnt_build_up_delay() {
        let (_, most) = talk_without_a_pause(-2000.0, 15, 120);
        assert!(most <= START + 2, "{most} packets waited");
    }

    /// A sender whose every other packet end-to-end encryption drops, as a
    /// user's log showed for hours, plays a frame a tick, not half speed.
    #[test]
    fn dropped_packets_play_as_nothing_rather_than_lost() {
        let mut encoder = opus::Encoder::new(1, opus::Application::Voice, BITRATE).unwrap();
        let mut speaker = Speaker::new(7).unwrap();
        for frame in 0..10u16 {
            // Each dropped packet shares the next frame's timestamp.
            speaker.push(frame * 2, at(frame), tone(&mut encoder, usize::from(frame)));
            speaker.skip(frame * 2 + 1, at(frame + 1));
        }
        for tick in 0..10 {
            assert!(speaker.next().is_some(), "tick {tick}");
        }
        // All ten played; only the last skipped packet is left, which the
        // next tick passes over.
        assert_eq!(speaker.queued(), 0, "{:?}", speaker.waiting);
        assert_eq!(speaker.next, Some(19));
    }

    /// A burst of dropped packets between two frames, as at the start of
    /// people's speech in that log, neither pushes frames out of the queue
    /// nor plays as half a second concealed.
    #[test]
    fn a_burst_of_dropped_packets_keeps_the_frames_around_it() {
        let mut encoder = opus::Encoder::new(1, opus::Application::Voice, BITRATE).unwrap();
        let mut speaker = Speaker::new(7).unwrap();
        // The burst comes within a frame, sharing a timestamp.
        speaker.push(0, at(0), tone(&mut encoder, 0));
        for sequence in 1..=30 {
            speaker.skip(sequence, at(1));
        }
        for (frame, sequence) in (31..34).enumerate() {
            speaker.push(
                sequence,
                at(frame as u16 + 1),
                tone(&mut encoder, frame + 1),
            );
        }
        assert_eq!(speaker.queued(), 4);
        for _ in 0..4 {
            assert!(speaker.next().is_some());
        }
        assert!(speaker.waiting.is_empty(), "{:?}", speaker.waiting);
        // A packet that comes after its sequence number was skipped plays.
        speaker.skip(34, at(4));
        speaker.push(34, at(4), tone(&mut encoder, 5));
        assert_eq!(speaker.queued(), 1);
        // Skipped ones alone don't start a speaker.
        let mut quiet = Speaker::new(8).unwrap();
        for sequence in 0..10 {
            quiet.skip(sequence, at(sequence));
        }
        assert!(quiet.next().is_none());
    }

    /// A sender whose frames all stop decrypting, as when their key for a
    /// new epoch hasn't come, stops after 100 ms and stays silent: the
    /// dropped packets that pile up while it's stopped don't start it again
    /// to conceal frames that never were.
    #[test]
    fn dropped_packets_alone_dont_start_a_stopped_speaker() {
        let mut encoder = opus::Encoder::new(1, opus::Application::Voice, BITRATE).unwrap();
        let mut speaker = Speaker::new(7).unwrap();
        for sequence in 0..10u16 {
            speaker.push(
                sequence,
                at(sequence),
                tone(&mut encoder, usize::from(sequence)),
            );
        }
        let mut sounding = 0;
        for tick in 0..400u16 {
            if tick >= 10 {
                speaker.skip(tick, at(tick));
            }
            sounding += usize::from(speaker.next().is_some());
        }
        // Ten frames, then concealed until it's gone.
        assert_eq!(sounding, 10 + usize::from(GONE) - 1);
        assert!(speaker.next.is_none());
        // Speech that decrypts again plays at once.
        for sequence in 400..403u16 {
            speaker.push(
                sequence,
                at(sequence),
                tone(&mut encoder, usize::from(sequence)),
            );
        }
        assert!(speaker.next().is_some());
        assert_eq!(speaker.next, Some(401));
    }

    /// What a call's thread does through a 300 ms stall on a machine where
    /// reading a packet is slow, `catch_up` deciding how many a tick reads
    /// before it plays: someone talking, a camera's video at 300 packets a
    /// second, and RTCP. Time is counted, not measured: a datagram of
    /// audio takes 100 µs, RTCP 150 and video 300, about fifty times what
    /// opening and parsing one takes here (0.6 to 3 µs, and DAVE about as
    /// much again for audio), as on a slow processor that a game keeps busy. Between ticks it reads a packet at a time, and a due
    /// tick waits for the packet being read, as `select!` does. Returns the
    /// longest a tick's microphone waited, once due, for the tick before to
    /// finish reading (the stall's missed ticks come at once, so each waits
    /// for the last), the audio packets that came too late to play, and the
    /// ticks from the stall on that played nothing for the speaker.
    fn through_a_stall(catch_up: impl Fn(usize, Duration) -> bool) -> (Duration, usize, usize) {
        #[derive(Clone, Copy)]
        enum Kind {
            Audio(u16),
            Video,
            Rtcp,
        }
        let mut encoder = opus::Encoder::new(1, opus::Application::Voice, BITRATE).unwrap();
        let packet = tone(&mut encoder, 0);
        let mut speaker = Speaker::new(7).unwrap();
        let micros = |micros: u64| Duration::from_micros(micros);
        let (stall, stalled_for, end) = (micros(1_000_000), micros(300_000), micros(3_000_000));
        let mut arrivals: Vec<(Duration, Kind)> = Vec::new();
        for frame in 0..(end.as_micros() / 20_000) as u16 {
            arrivals.push((
                micros(3_000 + u64::from(frame) * 20_000),
                Kind::Audio(frame),
            ));
        }
        for at in (0..end.as_micros() as u64).step_by(3_333) {
            arrivals.push((micros(at + 500), Kind::Video));
        }
        for at in (0..end.as_micros() as u64).step_by(100_000) {
            arrivals.push((micros(at + 7_000), Kind::Rtcp));
        }
        arrivals.sort_by_key(|(at, _)| *at);
        let cost = |kind: Kind| match kind {
            Kind::Audio(_) => micros(100),
            Kind::Video => micros(300),
            Kind::Rtcp => micros(150),
        };
        let mut socket = arrivals.into_iter().peekable();
        let mut read = |now: &mut Duration, speaker: &mut Speaker, late: &mut usize| {
            let (_, kind) = socket.next_if(|(at, _)| *at <= *now)?;
            if let Kind::Audio(sequence) = kind {
                if speaker.next.is_some_and(|next| before(sequence, next)) {
                    *late += 1;
                }
                speaker.push(sequence, at(sequence), packet.clone());
            }
            *now += cost(kind);
            Some(())
        };
        let (mut now, mut due, mut sent) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
        let (mut held, mut late, mut silent) = (Duration::ZERO, 0, 0);
        while due < end {
            if now >= stall && now < stall + stalled_for {
                // Held up by something else, not by reading.
                now = stall + stalled_for;
                sent = now;
            }
            if now < due {
                // Between ticks: one packet, or wait for the next to come.
                if read(&mut now, &mut speaker, &mut late).is_none() {
                    now = due.min(now + micros(100));
                }
                continue;
            }
            // A tick: the microphone goes out, the socket is read, and
            // everyone plays.
            held = held.max(now - due.max(sent));
            sent = now;
            let started = now;
            let mut count = 0;
            while catch_up(count, now - started)
                && read(&mut now, &mut speaker, &mut late).is_some()
            {
                count += 1;
            }
            let played = speaker.next().is_some();
            if due >= stall && !played {
                silent += 1;
            }
            due += micros(20_000);
        }
        (held, late, silent)
    }

    #[test]
    fn reading_after_a_stall_doesnt_hold_up_the_microphone() {
        let (held, late, silent) = through_a_stall(catches_up);
        // Never longer than the budget and a packet.
        assert!(
            held <= CATCH_UP_TIME + Duration::from_micros(300),
            "{held:?}"
        );
        // The speaker's audio was all read in time to play, and played on
        // every tick from the stall on, concealed while it waited.
        assert_eq!((late, silent), (0, 0));
        // Reading up to 64 packets whatever they cost held the ticks up.
        let (held, _, _) = through_a_stall(|read, _| read < CATCH_UP);
        assert!(held > Duration::from_millis(10), "{held:?}");
    }

    /// A sender's 48 frames, one packet a tick, with two more packets that
    /// DAVE drops after the 20th: each holding a frame's time of its own if
    /// `own_time`, coming on their own ticks, or else sharing the 20th's
    /// timestamp and coming with it. Returns the ticks that sounded and the
    /// fewest packets that waited to play after a tick once the dropped
    /// ones were passed, two ticks after the second came.
    fn two_dropped(own_time: bool) -> (usize, usize) {
        let mut encoder = opus::Encoder::new(1, opus::Application::Voice, BITRATE).unwrap();
        let mut speaker = Speaker::new(7).unwrap();
        // By tick, what comes: (sequence, timestamp, dropped).
        let mut ticks: Vec<Vec<(u16, u32, bool)>> = Vec::new();
        for frame in 0..48u16 {
            let sequence = if frame < 20 { frame } else { frame + 2 };
            let timestamp = if own_time { at(sequence) } else { at(frame) };
            ticks.push(vec![(sequence, timestamp, false)]);
            if frame == 19 {
                let dropped = [(20, at(20), true), (21, at(21), true)];
                if own_time {
                    ticks.extend(dropped.map(|packet| vec![packet]));
                } else {
                    ticks
                        .last_mut()
                        .unwrap()
                        .extend(dropped.map(|(sequence, _, _)| (sequence, at(19), true)));
                }
            }
        }
        let (mut sounded, mut fewest) = (0, usize::MAX);
        for tick in 0..ticks.len() + 20 {
            for &(sequence, timestamp, dropped) in ticks.get(tick).into_iter().flatten() {
                if dropped {
                    speaker.skip(sequence, timestamp);
                } else {
                    speaker.push(sequence, timestamp, tone(&mut encoder, tick));
                }
            }
            sounded += usize::from(speaker.next().is_some());
            if (24..ticks.len()).contains(&tick) {
                fewest = fewest.min(speaker.queued());
            }
        }
        (sounded, fewest)
    }

    /// Dropped packets each a frame later than the last, as from a sender
    /// some of whose frames don't decrypt, held 20 ms of their audio:
    /// concealed in their place, they keep the sender's timing, and the
    /// 40 ms cushion that waits for late packets. Played from the next
    /// packet instead, the frames after came 40 ms early, and nothing was
    /// left waiting for a late one.
    #[test]
    fn dropped_packets_that_held_time_are_concealed_in_place() {
        let (sounded, fewest) = two_dropped(true);
        assert_eq!(sounded, 50 + usize::from(GONE) - 1);
        assert_eq!(fewest, START - 1);
    }

    /// Dropped packets that share a timestamp held no time of their own,
    /// as in the log #176 fixed: they play as nothing, and the frames
    /// around them keep the cushion.
    #[test]
    fn dropped_packets_sharing_a_timestamp_play_as_nothing() {
        let (sounded, fewest) = two_dropped(false);
        assert_eq!(sounded, 48 + usize::from(GONE) - 1);
        assert_eq!(fewest, START - 1);
    }

    #[test]
    fn diagnostics_count_the_microphone_and_late_ticks() {
        let mut diagnostics = Diagnostics::new();
        for waiting in [FRAME * 2, FRAME / 2, 0, FRAME] {
            diagnostics.microphone(waiting, 10);
        }
        assert_eq!(
            (
                diagnostics.mic_frames,
                diagnostics.mic_underfilled,
                diagnostics.mic_empty
            ),
            (4, 1, 1)
        );
        assert_eq!(
            (diagnostics.mic_fewest, diagnostics.mic_dropped),
            (Some(0), 40)
        );
        // 99 ticks on time and one 40 ms late: 99 in 100 within the first
        // step, and the latest beyond the steps counted.
        for _ in 0..99 {
            diagnostics.tick(Duration::from_micros(100));
        }
        diagnostics.tick(Duration::from_millis(40));
        assert_eq!(diagnostics.late_p99(), Duration::from_micros(LATE_STEP));
        assert_eq!(diagnostics.latest, Duration::from_millis(40));
        diagnostics.tick(Duration::from_millis(40));
        assert_eq!(
            diagnostics.late_p99(),
            Duration::from_micros(LATE_STEP * LATE_STEPS as u64)
        );
        let mut speakers = HashMap::from([(1, Speaker::new(7).unwrap())]);
        speakers.get_mut(&1).unwrap().concealed = 3;
        diagnostics.log(&mut speakers);
        assert_eq!(speakers[&1].concealed, 0);
        assert_eq!(diagnostics.mic_frames, 0);
    }

    #[test]
    fn a_long_stall_keeps_only_the_newest() {
        let mut encoder = opus::Encoder::new(1, opus::Application::Voice, BITRATE).unwrap();
        let mut speaker = Speaker::new(7).unwrap();
        let packet = tone(&mut encoder, 0);
        for sequence in 0..(MOST as u16 + 10) {
            speaker.push(sequence, at(sequence), packet.clone());
        }
        assert_eq!(speaker.waiting.len(), MOST);
        assert_eq!(
            speaker.waiting.front().map(|(sequence, _, _)| *sequence),
            Some(10)
        );
    }
}
