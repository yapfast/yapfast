//! Discord voice's audio on this machine. LiveKit's device module only serves
//! its own calls, so Discord voice opens the chosen microphone and speakers
//! itself, through [`crate::audio_io`]. The microphone goes through WebRTC's
//! echo cancellation, DeepFilterNet's noise suppression (or RNNoise's), WebRTC's
//! gain control, Yapfast's leveling and the microphone's volume, and a gate
//! that speech opens.
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use livekit::webrtc::native::apm::AudioProcessingModule;
use tokio::sync::mpsc::UnboundedSender;
use yapfast_discord::voice::{CHANNELS, FRAME, Listener, Microphone, SAMPLE_RATE, Surplus, Track};

use crate::{
    audio_io::{LastingInput, LastingOutput},
    noise::{Denoiser, Sent},
};

/// Whether Discord voice has audio on this platform yet.
pub const AVAILABLE: bool = crate::audio_io::AVAILABLE;

/// Someone counts as speaking for this long after their last loud frame.
const SPEAKING_HOLD: Duration = Duration::from_millis(300);
/// Frames quieter than this, in RMS dBFS, don't count as speech.
const SPEAKING_FLOOR: f32 = -50.0;

/// Hears a Discord call: mixes everyone into the speakers, with the
/// soundboard's sounds, and notes who spoke.
pub struct Hearing {
    output: Mutex<Option<SyncSender<Vec<i16>>>>,
    deafened: AtomicBool,
    heard: Mutex<HashMap<u64, Instant>>,
    sounds: Mutex<Vec<Playing>>,
    ended: UnboundedSender<String>,
}

/// A soundboard sound playing: 48 kHz interleaved stereo, how far it has
/// played, and its own volume.
struct Playing {
    samples: Vec<i16>,
    at: usize,
    volume: f32,
}

/// Sounds playing at once at most; another stops the oldest.
const SOUNDS_AT_ONCE: usize = 8;

impl Hearing {
    pub fn new(ended: UnboundedSender<String>) -> Self {
        Self {
            output: Mutex::default(),
            deafened: AtomicBool::new(false),
            heard: Mutex::default(),
            sounds: Mutex::default(),
            ended,
        }
    }

    pub fn set_output(&self, output: Option<SyncSender<Vec<i16>>>) {
        *self.output.lock().unwrap() = output;
    }

    /// Deafening stops the sounds playing, as in Discord.
    pub fn set_deafened(&self, deafened: bool) {
        self.deafened.store(deafened, Ordering::Relaxed);
        if deafened {
            self.sounds.lock().unwrap().clear();
        }
    }

    /// Plays a soundboard sound, 48 kHz interleaved stereo, at its own
    /// `volume` times the speakers'; not while deafened.
    pub fn play(&self, samples: Vec<i16>, volume: f32) {
        if self.deafened.load(Ordering::Relaxed) {
            return;
        }
        let mut sounds = self.sounds.lock().unwrap();
        if sounds.len() == SOUNDS_AT_ONCE {
            sounds.remove(0);
        }
        sounds.push(Playing {
            samples,
            at: 0,
            volume,
        });
    }

    /// The Discord users heard speaking just now.
    pub fn speaking(&self) -> Vec<u64> {
        let now = Instant::now();
        let mut heard = self.heard.lock().unwrap();
        heard.retain(|_, at| now.duration_since(*at) < SPEAKING_HOLD);
        heard.keys().copied().collect()
    }
}

impl Listener for Hearing {
    fn tick(&self, voices: &[(u64, &[i16])]) {
        let now = Instant::now();
        {
            let mut heard = self.heard.lock().unwrap();
            for (user, samples) in voices {
                if decibels(samples) > SPEAKING_FLOOR {
                    heard.insert(*user, now);
                }
            }
        }
        if self.deafened.load(Ordering::Relaxed) {
            return;
        }
        let output = self.output.lock().unwrap();
        let Some(output) = output.as_ref() else {
            return;
        };
        let volume =
            |user: u64| crate::volume::heard(crate::volume::Kind::Voice, &user.to_string());
        let mut frame = spare_frame();
        let mut sounds = self.sounds.lock().unwrap();
        mix(
            voices,
            volume,
            &mut sounds,
            crate::volume::speakers(),
            &mut frame,
        );
        drop(sounds);
        // A full queue means the speakers are behind; dropping a frame keeps
        // the delay from growing.
        match output.try_send(frame) {
            Ok(()) => {}
            Err(TrySendError::Full(frame)) => recycle(frame),
            Err(TrySendError::Disconnected(frame)) => {
                tracing::warn!("Discord speakers stopped");
                recycle(frame);
            }
        }
    }

    fn disconnected(&self, reason: &str) {
        let _ = self.ended.send(reason.to_owned());
    }
}

/// Samples in one 20 ms frame of the call, interleaved stereo.
const FRAME_SAMPLES: usize = FRAME * CHANNELS as usize;

/// Frames the speakers played come back here to be mixed into again, so a
/// call doesn't allocate one every 20 ms. The queues they pass through bound
/// how many there are.
static SPARE: Mutex<Vec<Vec<i16>>> = Mutex::new(Vec::new());
/// More than the queues hold, so none is dropped in a call.
const SPARE_MOST: usize = 16;

fn spare_frame() -> Vec<i16> {
    let spare = SPARE.lock().unwrap().pop();
    spare.unwrap_or_else(|| vec![0; FRAME_SAMPLES])
}

fn recycle(frame: Vec<i16>) {
    let mut spare = SPARE.lock().unwrap();
    if spare.len() < SPARE_MOST {
        spare.push(frame);
    }
}

/// Sums everyone's 20 ms of interleaved stereo into `out`, each at the volume
/// chosen for them, and the next 20 ms of each sound at its own volume times
/// the `speakers'`, clipping at full scale. Sounds that ended go.
fn mix(
    voices: &[(u64, &[i16])],
    volume: impl Fn(u64) -> f32,
    sounds: &mut Vec<Playing>,
    speakers: f32,
    out: &mut [i16],
) {
    let mut sum = [0f32; FRAME_SAMPLES];
    for (user, samples) in voices {
        let gain = volume(*user);
        if gain == 0.0 {
            continue;
        }
        for (total, sample) in sum.iter_mut().zip(samples.iter()) {
            *total += f32::from(*sample) * gain;
        }
    }
    for sound in sounds.iter_mut() {
        let gain = sound.volume * speakers;
        let next = &sound.samples[sound.at.min(sound.samples.len())..];
        for (total, sample) in sum.iter_mut().zip(next) {
            *total += f32::from(*sample) * gain;
        }
        sound.at += FRAME_SAMPLES;
    }
    sounds.retain(|sound| sound.at < sound.samples.len());
    for (sample, total) in out.iter_mut().zip(sum) {
        *sample = total
            .round()
            .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
    }
}

/// RMS level in dBFS.
fn decibels(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return -100.0;
    }
    let power = samples
        .iter()
        .map(|sample| (f32::from(*sample) / 32768.0).powi(2))
        .sum::<f32>()
        / samples.len() as f32;
    10.0 * power.max(1e-10).log10()
}

/// The gate opens on a block the suppressor takes for speech that's louder
/// than the sensitivity, in RMS dBFS after processing, or on any block this
/// many dB louder still, once [`OPEN_AFTER`] such blocks come in a row.
const NOT_SPEECH: f32 = 18.0;
/// What's left of a click comes out of DeepFilterNet louder than the
/// sensitivity, over two blocks since its frames overlap, and its speech
/// estimate takes it for speech: one such block opened the gate on 7 of 33
/// mouse clicks, two on 4. While open, one block holds it.
const OPEN_AFTER: u32 = 3;
/// It stays open this many 10 ms blocks after the last that would open it.
const GATE_HOLD: u32 = 40;
/// Blocks kept from before the gate opens and sent when it does: a word's
/// quiet start, such as an "h", comes before it's loud or clear enough to open
/// it. Speech goes out this much later while the gate stays open.
const BEFORE_OPENING: usize = 10;
/// The suppressor takes a few blocks to learn the noise, which passes
/// meanwhile, so the gate stays shut for this many after the microphone opens.
const WARM_UP: u32 = 20;

/// Sends the microphone only while someone talks, starting a little before.
struct Gate {
    /// The quietest speech it opens on, in RMS dBFS.
    sensitivity: f32,
    hold: u32,
    heard: u32,
    /// How many blocks in a row would have opened it.
    opening: u32,
    /// The last blocks while shut, oldest first.
    before: VecDeque<[f32; BLOCK]>,
    /// What the last block sends.
    sent: Vec<[f32; BLOCK]>,
}

impl Default for Gate {
    fn default() -> Self {
        Self {
            sensitivity: crate::noise::DEFAULT_SENSITIVITY,
            hold: 0,
            heard: 0,
            opening: 0,
            before: VecDeque::with_capacity(BEFORE_OPENING),
            sent: Vec::with_capacity(BEFORE_OPENING + 1),
        }
    }
}

impl Gate {
    fn is_open(&self) -> bool {
        self.hold > 0
    }

    /// Takes the next 10 ms block, with its level and whether the suppressor
    /// takes it for speech, and returns the blocks to send: none while closed,
    /// and on opening, those kept from before too.
    fn block(&mut self, samples: &[f32; BLOCK], level: f32, speech: bool) -> &[[f32; BLOCK]] {
        let was_open = self.is_open();
        self.heard = self.heard.saturating_add(1);
        let warm = self.heard > WARM_UP;
        let floor = if speech {
            self.sensitivity
        } else {
            self.sensitivity + NOT_SPEECH
        };
        self.opening = if warm && level > floor {
            self.opening.saturating_add(1)
        } else {
            0
        };
        if self.opening >= OPEN_AFTER || (self.opening > 0 && was_open) {
            self.hold = GATE_HOLD;
        } else {
            self.hold = self.hold.saturating_sub(1);
        }
        self.sent.clear();
        if !self.is_open() {
            if self.before.len() == BEFORE_OPENING {
                self.before.pop_front();
            }
            self.before.push_back(*samples);
            return &self.sent;
        }
        if !was_open {
            self.sent.extend(self.before.drain(..));
        }
        self.sent.push(*samples);
        &self.sent
    }

    /// Takes the next block in push-to-talk, which sends it while `open`
    /// rather than when it hears speech, as Discord's does.
    fn pass(&mut self, samples: &[f32; BLOCK], open: bool) -> &[[f32; BLOCK]] {
        self.heard = self.heard.saturating_add(1);
        self.opening = 0;
        // Back in voice activation, the next block decides.
        self.hold = u32::from(open);
        self.before.clear();
        self.sent.clear();
        if open {
            self.sent.push(*samples);
        }
        &self.sent
    }
}

/// The playback side's copy of what it played, for echo cancellation.
pub type Reverse = Arc<Mutex<Receiver<Vec<i16>>>>;

/// Plays the call through the chosen speakers until dropped.
pub struct Speakers {
    stop: Arc<AtomicBool>,
}

impl Drop for Speakers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// The thread that runs a call's microphone, one [`capture`] after another.
/// Between them, while muted, it waits with the noise suppressor, which
/// isn't Send, so that unmuting or switching microphones doesn't load
/// DeepFilterNet again, 0.2 s of a core. It keeps the suppressor's 39 MiB
/// until the call drops it, which ends the thread.
#[derive(Default)]
pub struct MicrophoneThread(Mutex<Option<mpsc::Sender<CaptureRequest>>>);

/// What a capture needs, sent to the microphone thread.
struct CaptureRequest {
    microphone: Option<String>,
    reverse: Reverse,
    out: Microphone,
    track: Track,
    stop: Arc<AtomicBool>,
    talking: Arc<AtomicBool>,
    opened: mpsc::SyncSender<Result<()>>,
}

impl MicrophoneThread {
    /// Hands `request` to the thread, starting it if it isn't running.
    fn start(&self, request: CaptureRequest) -> Result<()> {
        let mut running = self.0.lock().unwrap();
        let request = match &*running {
            Some(requests) => match requests.send(request) {
                Ok(()) => return Ok(()),
                // A capture panicked and ended the thread.
                Err(mpsc::SendError(request)) => request,
            },
            None => request,
        };
        let (requests, waiting) = mpsc::channel::<CaptureRequest>();
        thread::Builder::new()
            .name("yapfast-discord-in".into())
            .spawn(move || {
                let mut denoiser = None;
                for request in waiting {
                    denoiser = request.run(denoiser);
                }
            })
            .context("could not start the Discord microphone")?;
        let _ = requests.send(request);
        *running = Some(requests);
        Ok(())
    }
}

/// Sends the processed microphone to the call until dropped.
pub struct Capture {
    stop: Arc<AtomicBool>,
    talking: Arc<AtomicBool>,
    track: Track,
}

impl Capture {
    /// Whether the voice gate is open: the microphone hears speech.
    pub fn talking(&self) -> bool {
        self.talking.load(Ordering::Relaxed)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.track.pause();
    }
}

/// The Discord microphone's level as the voice gate hears it, after all the
/// processing, and whether the gate would be open: for setting the
/// sensitivity. It opens the microphone itself and sends nothing, so it works
/// outside calls and beside one. Stops when dropped.
pub struct GateMeter {
    shared: Arc<MeterShared>,
}

struct MeterShared {
    /// RMS dBFS, as f32 bits, held a moment so speech reads steadily.
    level: AtomicU32,
    open: AtomicBool,
    stop: AtomicBool,
    failed: AtomicBool,
}

impl GateMeter {
    /// Starts measuring `microphone`, a device id from
    /// [`crate::voice::State`], or the system default for `None`.
    pub fn start(microphone: Option<&str>) -> Self {
        let shared = Arc::new(MeterShared {
            level: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            open: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            failed: AtomicBool::new(!AVAILABLE),
        });
        let microphone = microphone.map(str::to_owned);
        let worker = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("yapfast-gate-meter".into())
            .spawn(move || {
                if let Err(error) = measure(microphone.as_deref(), &worker) {
                    tracing::warn!(%error, "could not measure the Discord microphone");
                    worker.failed.store(true, Ordering::Relaxed);
                }
            });
        if spawned.is_err() {
            shared.failed.store(true, Ordering::Relaxed);
        }
        Self { shared }
    }

    /// In RMS dBFS; minus infinity before the first block.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.shared.level.load(Ordering::Relaxed))
    }

    /// Whether the gate would be open, sending the microphone.
    pub fn sending(&self) -> bool {
        self.shared.open.load(Ordering::Relaxed)
    }

    /// The microphone could not be opened or stopped delivering sound.
    pub fn failed(&self) -> bool {
        self.shared.failed.load(Ordering::Relaxed)
    }
}

impl Drop for GateMeter {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

/// Frames waiting for the speakers; more than this and new ones are dropped.
const QUEUE: usize = 4;
/// What the speakers keep queued: three frames.
const SPEAKER_QUEUE: Duration = Duration::from_millis(60);
/// Roughly how late the microphone hears what the speakers played.
const ECHO_DELAY_MS: i32 = 60;
/// Capture works in WebRTC's 10 ms blocks, which are the suppressors'
/// frames too.
const BLOCK: usize = SAMPLE_RATE as usize / 100;
const _: () = assert!(BLOCK == crate::noise::BLOCK);

/// Opens the speakers and returns them with the queue that feeds them.
/// Every played frame also goes to `reverse` for echo cancellation.
pub fn speakers(
    speaker: Option<&str>,
    reverse: SyncSender<Vec<i16>>,
) -> Result<(Speakers, SyncSender<Vec<i16>>)> {
    let speaker = speaker.map(str::to_owned);
    let (frames, queue) = mpsc::sync_channel::<Vec<i16>>(QUEUE);
    let (opened, result) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    thread::Builder::new()
        .name("yapfast-discord-out".into())
        .spawn(move || {
            // Speakers that go away, such as Bluetooth headphones, open
            // again when they come back.
            let stream = LastingOutput::<i16>::open(
                speaker.as_deref(),
                "Discord voice",
                SAMPLE_RATE,
                CHANNELS,
                Some(SPEAKER_QUEUE),
            );
            let mut stream = match stream {
                Ok(stream) => {
                    let _ = opened.send(Ok(()));
                    stream
                }
                Err(error) => {
                    let _ = opened.send(Err(error));
                    return;
                }
            };
            // The call's ticks run on the system's clock and the speakers
            // on their own, so frames come a little faster or slower than
            // they play: 50 ppm adds 90 ms in half an hour. What came while
            // the last frame was written waits here, and a quiet frame goes
            // while more wait than the ticks' unevenness needs.
            let mut waiting = VecDeque::with_capacity(QUEUE + 1);
            let mut surplus = Surplus::default();
            while !stopped.load(Ordering::Relaxed) {
                while let Ok(frame) = queue.try_recv() {
                    waiting.push_back(frame);
                    if waiting.len() > QUEUE
                        && let Some(oldest) = waiting.pop_front()
                    {
                        recycle(oldest);
                    }
                }
                let frame = match waiting.pop_front() {
                    Some(frame) => frame,
                    None => match queue.recv_timeout(Duration::from_millis(100)) {
                        Ok(frame) => frame,
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => break,
                    },
                };
                surplus.note(waiting.len());
                if surplus.skips(|| decibels(&frame) < SPEAKING_FLOOR) {
                    recycle(frame);
                    continue;
                }
                stream.write(&frame);
                if let Err(TrySendError::Full(frame) | TrySendError::Disconnected(frame)) =
                    reverse.try_send(frame)
                {
                    recycle(frame);
                }
            }
            stream.drain();
        })
        .context("could not start Discord speakers")?;
    result
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| anyhow!("The speakers didn't open in time."))?
        .context("Could not open your speakers. Choose another device.")?;
    Ok((Speakers { stop }, frames))
}

/// The microphone's processing, block by block: WebRTC cancels echo and
/// filters out rumble, the chosen suppressor takes out noise and hears
/// speech, WebRTC controls the gain of what's left, and Yapfast levels it and
/// sets its volume. The gate decides what's sent.
struct Processing {
    echo: AudioProcessingModule,
    /// Takes out the room's reverb while asked to.
    dereverb: Option<crate::dereverb::Dereverb>,
    denoiser: Denoiser,
    gain: AudioProcessingModule,
    leveler: crate::leveling::Leveler,
    gate: Gate,
    /// Sends instead of the gate in push-to-talk.
    talk: crate::push_to_talk::Talk,
    /// The last block's level as the gate heard it, in RMS dBFS.
    level: f32,
    samples: [f32; BLOCK],
}

impl Processing {
    /// Takes up `denoiser` if it's the suppressor chosen.
    fn new(denoiser: Option<Denoiser>) -> Self {
        let mut echo = AudioProcessingModule::new(true, false, true, false);
        let _ = echo.set_stream_delay_ms(ECHO_DELAY_MS);
        let chosen = crate::noise::chosen();
        Self {
            echo,
            dereverb: None,
            denoiser: denoiser
                .filter(|denoiser| denoiser.suppressor() == chosen)
                .unwrap_or_else(|| Denoiser::new(chosen, crate::noise::limit())),
            gain: AudioProcessingModule::new(false, true, false, false),
            leveler: crate::leveling::Leveler::default(),
            gate: Gate::default(),
            talk: crate::push_to_talk::Talk::default(),
            level: f32::NEG_INFINITY,
            samples: [0.0; BLOCK],
        }
    }

    /// Notes what the speakers played, for echo cancellation.
    fn played(&mut self, frame: &mut [i16]) {
        let _ = self
            .echo
            .process_reverse_stream(frame, SAMPLE_RATE as i32, i32::from(CHANNELS));
    }

    /// Processes 10 ms of mono microphone in place, and returns the blocks
    /// to send, as f32 in full scale: none while the gate is shut, or in
    /// push-to-talk while the key is up.
    fn block(&mut self, block: &mut [i16]) -> &[[f32; BLOCK]] {
        let _ = self.echo.process_stream(block, SAMPLE_RATE as i32, 1);
        let chosen = crate::noise::chosen();
        if self.denoiser.suppressor() != chosen {
            tracing::info!(?chosen, "Discord microphone switches noise suppression");
            self.denoiser = Denoiser::new(chosen, crate::noise::limit());
        }
        self.denoiser.limit(crate::noise::limit());
        for (value, sample) in self.samples.iter_mut().zip(block.iter()) {
            *value = f32::from(*sample);
        }
        if crate::noise::dereverb() {
            self.dereverb
                .get_or_insert_with(crate::dereverb::Dereverb::new)
                .block(&mut self.samples);
        } else {
            self.dereverb = None;
        }
        let talking = crate::push_to_talk::on().then(|| {
            self.talk.block(
                crate::push_to_talk::held(),
                crate::push_to_talk::release_delay_ms(),
            )
        });
        let sent = match talking {
            None => Sent::Gated,
            Some(true) => Sent::Always,
            Some(false) => Sent::Never,
        };
        let speech = self.denoiser.block(&mut self.samples, sent);
        for (sample, value) in block.iter_mut().zip(&self.samples) {
            *sample = value.clamp(-32768.0, 32767.0) as i16;
        }
        let _ = self.gain.process_stream(block, SAMPLE_RATE as i32, 1);
        // The gate hears the voice at its volume, as it's sent, but before
        // leveling, which could lift noise past it.
        let volume = crate::volume::microphone();
        let level = decibels(block) + 20.0 * volume.log10();
        self.level = level;
        for (value, sample) in self.samples.iter_mut().zip(block.iter()) {
            *value = f32::from(*sample);
        }
        self.leveler.process(
            &mut self.samples,
            SAMPLE_RATE,
            crate::leveling::enabled(),
            volume,
        );
        for (sample, value) in block.iter_mut().zip(&self.samples) {
            *sample = value.clamp(-32768.0, 32767.0) as i16;
        }
        for value in &mut self.samples {
            *value /= 32768.0;
        }
        if let Some(sending) = talking {
            return self.gate.pass(&self.samples, sending);
        }
        self.gate.sensitivity = crate::noise::sensitivity();
        self.gate.block(&self.samples, level, speech)
    }
}

/// Runs the microphone through a call's processing for [`GateMeter`]
/// until it stops. The level falls 90 dB a second, as the input menu's
/// does, and rises at once.
fn measure(microphone: Option<&str>, shared: &MeterShared) -> Result<()> {
    let mut stream = open_microphone(microphone)?;
    let mut processing = Processing::new(None);
    let mut block = [0i16; BLOCK];
    let mut shown = f32::NEG_INFINITY;
    while !shared.stop.load(Ordering::Relaxed) {
        stream.read(&mut block);
        processing.block(&mut block);
        shown = processing.level.max(shown - 0.9);
        shared.level.store(shown.to_bits(), Ordering::Relaxed);
        shared
            .open
            .store(processing.gate.is_open(), Ordering::Relaxed);
    }
    Ok(())
}

/// Opens the microphone on `thread` and sends its processed audio while
/// someone talks: the track plays while the gate is open and pauses
/// otherwise, so Discord shows speaking only during speech.
pub fn capture(
    thread: &MicrophoneThread,
    microphone: Option<&str>,
    reverse: Reverse,
    out: Microphone,
    track: Track,
) -> Result<Capture> {
    let (opened, result) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    let talking = Arc::new(AtomicBool::new(false));
    thread.start(CaptureRequest {
        microphone: microphone.map(str::to_owned),
        reverse,
        out,
        track: track.clone(),
        stop: stop.clone(),
        talking: talking.clone(),
        opened,
    })?;
    result
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| anyhow!("The microphone didn't open in time."))?
        .context("Could not open your microphone. Check permissions or choose another device.")?;
    Ok(Capture {
        stop,
        talking,
        track,
    })
}

impl CaptureRequest {
    /// Captures until stopped, with `denoiser` if it's the suppressor
    /// chosen, and returns the one it used, or `denoiser` if the microphone
    /// didn't open.
    fn run(self, denoiser: Option<Denoiser>) -> Option<Denoiser> {
        let mut stream = match open_microphone(self.microphone.as_deref()) {
            Ok(stream) => {
                let _ = self.opened.send(Ok(()));
                stream
            }
            Err(error) => {
                let _ = self.opened.send(Err(error));
                return denoiser;
            }
        };
        // The modules aren't Send, so they live on this thread.
        let mut processing = Processing::new(denoiser);
        let reverse = self.reverse.lock().unwrap();
        // Drop what played before the microphone opened.
        while let Ok(played) = reverse.try_recv() {
            recycle(played);
        }
        let mut block = [0i16; BLOCK];
        while !self.stop.load(Ordering::Relaxed) {
            stream.read(&mut block);
            while let Ok(mut played) = reverse.try_recv() {
                processing.played(&mut played);
                recycle(played);
            }
            let was_open = processing.gate.is_open();
            let sent = processing.block(&mut block);
            // The gate sends while it's open.
            let open = !sent.is_empty();
            if open != was_open {
                self.talking.store(open, Ordering::Relaxed);
                let _ = if open {
                    self.out.clear();
                    self.track.play()
                } else {
                    self.track.pause()
                };
            }
            for samples in sent {
                self.out.push(samples);
            }
        }
        self.talking.store(false, Ordering::Relaxed);
        let _ = self.track.pause();
        Some(processing.denoiser)
    }
}

/// Runs a recording through the microphone's processing: raw mono 16-bit
/// 48 kHz from `YAPFAST_AUDIO_IN`, written processed to `YAPFAST_AUDIO_OUT`, and
/// what was sent to `YAPFAST_AUDIO_SENT`, with silence where the gate was
/// closed. `YAPFAST_SUPPRESSOR` chooses the suppressor (`rnnoise`, `off`), and
/// `YAPFAST_NOISE_LIMIT` limits its attenuation, in dB. `YAPFAST_AUDIO_FAR`, raw
/// mono 16-bit 48 kHz as long as the input, is what the speakers played,
/// for echo cancellation, given a 20 ms stereo frame at a time as a call
/// gives it; then `YAPFAST_AUDIO_ECHOED` gets the microphone with only echo
/// cancelled. `YAPFAST_LEVELING=off` turns off evening out the voice.
/// `YAPFAST_PUSH_TO_TALK`, a byte for each block, not 0 while the key is
/// down, puts the microphone in push-to-talk.
#[test]
#[ignore = "needs YAPFAST_AUDIO_IN and YAPFAST_AUDIO_OUT"]
fn processes_a_recording() {
    if let Ok(name) = std::env::var("YAPFAST_SUPPRESSOR") {
        crate::noise::set(serde_json::from_value(name.into()).unwrap());
    }
    if let Ok(limit) = std::env::var("YAPFAST_NOISE_LIMIT") {
        crate::noise::set_limit(limit.parse().unwrap());
    }
    if std::env::var("YAPFAST_DEREVERB").is_ok_and(|value| value == "on") {
        crate::noise::set_dereverb(true);
    }
    if std::env::var("YAPFAST_DECLICK").is_ok_and(|value| value == "off") {
        crate::noise::set_declick(false);
    }
    if std::env::var("YAPFAST_LEVELING").is_ok_and(|value| value == "off") {
        crate::leveling::set_enabled(false);
    }
    let input = std::fs::read(std::env::var("YAPFAST_AUDIO_IN").unwrap()).unwrap();
    let read = |name| -> Vec<i16> {
        std::fs::read(std::env::var(name).unwrap())
            .unwrap()
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect()
    };
    let far = std::env::var("YAPFAST_AUDIO_FAR")
        .is_ok()
        .then(|| read("YAPFAST_AUDIO_FAR"));
    let keys = std::env::var("YAPFAST_PUSH_TO_TALK")
        .ok()
        .map(|path| std::fs::read(path).unwrap());
    crate::push_to_talk::set_on(keys.is_some());
    // The same echo cancellation alone, to hear what it leaves.
    let mut echo = AudioProcessingModule::new(true, false, true, false);
    let mut echoed = Vec::new();
    let mut processing = Processing::new(None);
    let (mut processed, mut sent) = (Vec::new(), Vec::new());
    let (mut opened, mut blocks) = (0, 0);
    let mut took = std::time::Duration::ZERO;
    for bytes in input.chunks_exact(BLOCK * 2) {
        let mut block: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        if let Some(far) = &far
            && blocks % 2 == 0
        {
            let start = blocks as usize * BLOCK;
            let mut frame: Vec<i16> = far[start.min(far.len())..(start + FRAME).min(far.len())]
                .iter()
                .flat_map(|sample| [*sample, *sample])
                .collect();
            frame.resize(FRAME * 2, 0);
            processing.played(&mut frame.clone());
            let _ = echo.process_reverse_stream(&mut frame, SAMPLE_RATE as i32, 2);
            let mut alone = block.clone();
            let _ = echo.process_stream(&mut alone, SAMPLE_RATE as i32, 1);
            echoed.extend(alone);
        } else if far.is_some() {
            let mut alone = block.clone();
            let _ = echo.process_stream(&mut alone, SAMPLE_RATE as i32, 1);
            echoed.extend(alone);
        }
        if let Some(keys) = &keys {
            crate::push_to_talk::hold(keys.get(blocks as usize).is_some_and(|key| *key != 0));
        }
        let was_open = processing.gate.is_open();
        let started = std::time::Instant::now();
        let out = processing.block(&mut block).to_vec();
        took += started.elapsed();
        if processing.gate.is_open() && !was_open {
            opened += 1;
            println!(
                "opened at {:.2} s, sending {} blocks",
                blocks as f32 / 100.0,
                out.len()
            );
        }
        if !processing.gate.is_open() && was_open {
            println!("closed at {:.2} s", blocks as f32 / 100.0);
        }
        blocks += 1;
        processed.extend(block.iter().flat_map(|sample| sample.to_le_bytes()));
        if out.is_empty() {
            sent.extend(std::iter::repeat_n(0u8, BLOCK * 2));
        }
        for samples in out {
            sent.extend(samples.iter().flat_map(|value| {
                ((value * 32768.0).clamp(-32768.0, 32767.0) as i16).to_le_bytes()
            }));
        }
    }
    println!(
        "{blocks} blocks, the gate opened {opened} times, {:?} a block",
        took / blocks
    );
    std::fs::write(std::env::var("YAPFAST_AUDIO_OUT").unwrap(), processed).unwrap();
    if let Ok(path) = std::env::var("YAPFAST_AUDIO_SENT") {
        std::fs::write(path, sent).unwrap();
    }
    if let Ok(path) = std::env::var("YAPFAST_AUDIO_ECHOED") {
        let bytes: Vec<u8> = echoed
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        std::fs::write(path, bytes).unwrap();
    }
}

/// Opens the microphone, mono in 10 ms blocks, to open again if it goes
/// away and comes back.
fn open_microphone(microphone: Option<&str>) -> Result<LastingInput<i16>> {
    LastingInput::open(
        microphone,
        "Discord microphone",
        SAMPLE_RATE,
        1,
        Duration::from_millis(10),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixing_adds_voices_and_clips() {
        let loud = vec![30_000i16; FRAME * 2];
        let quiet = vec![100i16; FRAME * 2];
        let as_they_come = |_| 1.0;
        let mixed = |voices: &[(u64, &[i16])], volume: &dyn Fn(u64) -> f32| {
            let mut out = vec![1i16; FRAME * 2];
            mix(voices, volume, &mut Vec::new(), 1.0, &mut out);
            out
        };
        assert_eq!(
            mixed(&[(1, &loud), (2, &loud), (3, &quiet)], &as_they_come)[0],
            i16::MAX
        );
        assert_eq!(mixed(&[(3, &quiet)], &as_they_come)[0], 100);
        assert!(mixed(&[], &as_they_come).iter().all(|sample| *sample == 0));
        // Each voice at its own volume.
        let chosen = |user| if user == 3 { 1.5 } else { 0.0 };
        assert_eq!(mixed(&[(1, &loud), (3, &quiet)], &chosen)[0], 150);
    }

    #[test]
    fn sounds_play_into_the_mix_until_they_end() {
        // A sound a frame and a half long, at half its volume and the
        // speakers' half.
        let mut sounds = vec![Playing {
            samples: vec![4000; FRAME_SAMPLES * 3 / 2],
            at: 0,
            volume: 0.5,
        }];
        let quiet = vec![100i16; FRAME_SAMPLES];
        let mut out = vec![0i16; FRAME_SAMPLES];
        mix(&[(3, &quiet)], |_| 1.0, &mut sounds, 0.5, &mut out);
        assert!(out.iter().all(|sample| *sample == 1100));
        mix(&[], |_| 1.0, &mut sounds, 0.5, &mut out);
        assert_eq!(out[FRAME_SAMPLES / 2 - 1], 1000);
        assert_eq!(out[FRAME_SAMPLES / 2], 0);
        assert!(sounds.is_empty(), "the sound ended");
    }

    #[test]
    fn the_gate_opens_on_speech_with_what_came_just_before() {
        let mut gate = Gate::default();
        let block = |value: f32| [value; BLOCK];
        for _ in 0..WARM_UP {
            assert!(
                gate.block(&block(0.0), -20.0, true).is_empty(),
                "warming up"
            );
        }
        for index in 0..15 {
            assert!(
                gate.block(&block(index as f32), -70.0, true).is_empty(),
                "too quiet to be speech"
            );
            assert!(!gate.is_open());
        }
        // A soft start the suppressor hears as speech opens it after a few
        // blocks, with the 100 ms before.
        let opened = 15 + OPEN_AFTER - 1;
        for index in 15..opened {
            assert!(gate.block(&block(index as f32), -50.0, true).is_empty());
        }
        let firsts: Vec<f32> = gate
            .block(&block(opened as f32), -50.0, true)
            .iter()
            .map(|samples| samples[0])
            .collect();
        assert!(gate.is_open());
        assert_eq!(
            firsts,
            (opened - BEFORE_OPENING as u32..=opened)
                .map(|index| index as f32)
                .collect::<Vec<_>>()
        );
        // Then one block at a time, through a pause shorter than the hold.
        for index in 0..GATE_HOLD - 1 {
            assert_eq!(gate.block(&block(0.0), -70.0, false).len(), 1, "{index}");
        }
        assert!(gate.block(&block(0.0), -70.0, false).is_empty());
        assert!(!gate.is_open());
        // Loud noise opens it whatever the suppressor says; quiet noise doesn't.
        assert!(gate.block(&block(0.0), -55.0, false).is_empty());
        for _ in 1..OPEN_AFTER {
            assert!(gate.block(&block(1.0), -30.0, false).is_empty());
        }
        // With the block that shut it and the quiet noise.
        assert_eq!(
            gate.block(&block(1.0), -30.0, false).len(),
            OPEN_AFTER as usize + 2
        );
    }

    #[test]
    fn a_click_alone_doesnt_open_the_gate() {
        let mut gate = Gate::default();
        let block = [0.0; BLOCK];
        for _ in 0..WARM_UP {
            gate.block(&block, -100.0, false);
        }
        // Blocks taken for speech, or loud ones, one fewer than open it, then
        // quiet.
        for (level, speech) in [(-40.0, true), (-20.0, false)] {
            for _ in 1..OPEN_AFTER {
                assert!(gate.block(&block, level, speech).is_empty());
            }
            assert!(gate.block(&block, -70.0, false).is_empty());
            assert!(!gate.is_open());
        }
    }

    #[test]
    fn the_sensitivity_sets_how_loud_sound_must_be_to_open_the_gate() {
        let mut gate = Gate {
            sensitivity: -30.0,
            ..Gate::default()
        };
        let block = [0.0; BLOCK];
        for _ in 0..WARM_UP {
            gate.block(&block, -100.0, false);
        }
        // Enough blocks of each to open it. Speech must be louder than the
        // sensitivity, and other sounds 18 dB louder still.
        let opens = |gate: &mut Gate, level, speech| {
            for _ in 1..OPEN_AFTER {
                gate.block(&block, level, speech);
            }
            !gate.block(&block, level, speech).is_empty()
        };
        assert!(!opens(&mut gate, -35.0, true));
        assert!(!opens(&mut gate, -15.0, false));
        assert!(opens(&mut gate, -25.0, true));
        let mut gate = Gate {
            sensitivity: -30.0,
            ..Gate::default()
        };
        for _ in 0..WARM_UP {
            gate.block(&block, -100.0, false);
        }
        assert!(opens(&mut gate, -10.0, false));
    }

    /// Joins the fake Discord's Lounge, unmutes, and beeps where the
    /// microphone hears it: the call sends Opus that isn't silence and says
    /// it's speaking. Needs `bun testing/fake_discord.ts`,
    /// `YAPFAST_DISCORD_TEST_SERVER`, and a sound server whose default input
    /// hears its default output, such as a private PulseAudio of null sinks,
    /// or Wine on one.
    #[test]
    #[ignore = "needs testing/fake_discord.ts, YAPFAST_DISCORD_TEST_SERVER and sound devices"]
    fn a_call_sends_what_the_microphone_hears() {
        // A beep isn't speech, but it's loud enough to open the gate anyway
        // with the suppressor off, which would take a steady tone out.
        crate::noise::set(crate::noise::Suppressor::Off);
        let call = in_the_lounge(async |server| {
            tokio::task::spawn_blocking(beep).await.unwrap();
            tokio::time::sleep(Duration::from_millis(500)).await;
            newest(&spoken(server))
        });
        assert_eq!(call["undecryptable"], 0);
        assert!(call["voiced"].as_u64().unwrap() > 50, "too little was sent");
        // Once the beeps stop, it says it stopped, as Discord's clients do.
        assert_eq!(call["spoke"], true, "it should say it's speaking");
    }

    /// As [`a_call_sends_what_the_microphone_hears`], in push-to-talk: beeps
    /// with the key up send nothing, and with it held they're sent, and the
    /// call says it's speaking until the key comes up. Run it alone, with
    /// `--exact`: it turns push-to-talk on for the process.
    #[test]
    #[ignore = "needs testing/fake_discord.ts, YAPFAST_DISCORD_TEST_SERVER and sound devices"]
    fn push_to_talk_sends_only_while_the_key_is_held() {
        crate::noise::set(crate::noise::Suppressor::Off);
        crate::push_to_talk::set_on(true);
        crate::push_to_talk::set_release_delay(crate::push_to_talk::DEFAULT_RELEASE_DELAY);
        let (up, held) = in_the_lounge(async |server| {
            tokio::task::spawn_blocking(beep).await.unwrap();
            tokio::time::sleep(Duration::from_millis(500)).await;
            let up = newest(&spoken(server));
            crate::push_to_talk::hold(true);
            tokio::task::spawn_blocking(beep).await.unwrap();
            crate::push_to_talk::hold(false);
            tokio::time::sleep(Duration::from_millis(500)).await;
            (up, newest(&spoken(server)))
        });
        crate::push_to_talk::set_on(false);
        eprintln!("with the key up the fake heard {up}, held {held}");
        assert_eq!(
            up["voiced"].as_u64().unwrap_or(0),
            0,
            "sent with the key up"
        );
        assert_ne!(up["spoke"], true, "said it spoke with the key up");
        assert!(held["voiced"].as_u64().unwrap() > 50, "too little was sent");
        assert_eq!(held["spoke"], true, "it should say it's speaking");
        assert_eq!(held["speaking"], 0, "it should say it stopped with the key");
    }

    /// Joins the fake Discord's Lounge, unmutes, runs `during` with the
    /// fake's address, and leaves.
    fn in_the_lounge<T>(during: impl AsyncFnOnce(&str) -> T) -> T {
        use crate::voice::{Command, Phase, State};
        let server = yapfast_discord::client::test_server().expect("YAPFAST_DISCORD_TEST_SERVER");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(tokio::task::LocalSet::new().run_until(async {
            let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
            let discord =
                yapfast_discord::service::Discord::start(yapfast_discord::token::load().unwrap())
                    .unwrap();
            let mut online = discord.snapshots();
            tokio::time::timeout(
                Duration::from_secs(10),
                online.wait_for(|snapshot| {
                    snapshot.status == yapfast_discord::service::Status::Online
                }),
            )
            .await
            .expect("the fake Discord didn't come online")
            .unwrap();
            let (commands, mut received) = tokio::sync::mpsc::unbounded_channel();
            let (snapshots, mut states) = tokio::sync::watch::channel(State::default());
            let mut state = State::default();
            let call = crate::discord_voice::call(
                &discord,
                "1400000000000000100",
                "1400000000000000103",
                &mut state,
                &mut received,
                &snapshots,
                None,
            );
            let speak = async {
                let connected = tokio::time::timeout(
                    Duration::from_secs(20),
                    states.wait_for(|state| state.phase == Phase::Connected),
                )
                .await
                .is_ok();
                assert!(
                    connected,
                    "the call didn't connect: {:?}",
                    states.borrow().error
                );
                commands.send(Command::Mute(false)).unwrap();
                let result = during(server).await;
                commands.send(Command::Leave).unwrap();
                result
            };
            tokio::pin!(call);
            // The call ending first is a failure to join.
            let result = tokio::select! {
                ended = &mut call => panic!("the call ended: {ended:?}"),
                result = speak => result,
            };
            call.await.unwrap();
            // Long enough for leaving to reach the fake before the process ends.
            tokio::time::sleep(Duration::from_millis(500)).await;
            result
        }))
    }

    /// What the fake heard from the newest call, the one with the highest
    /// SSRC.
    fn newest(sent: &serde_json::Value) -> serde_json::Value {
        eprintln!("the fake heard {sent}");
        sent.as_object()
            .unwrap()
            .iter()
            .max_by_key(|(ssrc, _)| ssrc.parse::<u32>().unwrap())
            .expect("the fake heard no call")
            .1
            .clone()
    }

    /// Two seconds of beeps on the default speakers.
    fn beep() {
        let mut output = crate::audio_io::Output::<i16>::open(
            None,
            "Test beeps",
            SAMPLE_RATE,
            1,
            Some(Duration::from_millis(60)),
        )
        .unwrap();
        let mut block = [0i16; BLOCK];
        for index in 0..200 {
            for (at, sample) in block.iter_mut().enumerate() {
                let time = (index * BLOCK + at) as f32 / SAMPLE_RATE as f32;
                let on = index % 40 < 25;
                *sample = if on {
                    (8000.0 * (time * 440.0 * std::f32::consts::TAU).sin()) as i16
                } else {
                    0
                };
            }
            output.write(&block).unwrap();
        }
        output.drain().unwrap();
    }

    /// What the fake Discord heard from each call.
    fn spoken(server: &str) -> serde_json::Value {
        use std::io::{Read, Write};
        let address = server.trim_start_matches("http://").trim_end_matches('/');
        let mut connection = std::net::TcpStream::connect(address).unwrap();
        write!(
            connection,
            "GET /_control/voice-audio HTTP/1.0\r\nHost: {address}\r\n\r\n"
        )
        .unwrap();
        let mut response = String::new();
        connection.read_to_string(&mut response).unwrap();
        let body = response.split_once("\r\n\r\n").unwrap().1;
        serde_json::from_str(body).unwrap()
    }

    #[test]
    fn hearing_tracks_who_spoke() {
        let (ended, _) = tokio::sync::mpsc::unbounded_channel();
        let hearing = Hearing::new(ended);
        let speech = vec![8_000i16; FRAME * 2];
        let hiss = vec![10i16; FRAME * 2];
        hearing.tick(&[(1, &speech), (2, &hiss)]);
        assert_eq!(hearing.speaking(), vec![1]);
    }
}
