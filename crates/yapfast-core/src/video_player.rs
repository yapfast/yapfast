//! Plays a video file in the stream worker ([`crate::stream_worker::Job::Play`]),
//! so FFmpeg reads files people sent outside Yapfast's window, and its memory
//! goes when playing stops. The file may still be downloading: reading past
//! its end waits for more.
//!
//! Three threads share one [`State`]: the decoder fills queues of pictures
//! and sound, a few hundred milliseconds ahead; the sound thread plays the
//! sound on PulseAudio, matched to the clock; and the job's own thread shows
//! each picture when the clock reaches it, takes Yapfast's controls and reports
//! back. The clock is the system's, held while there's nothing to play, as
//! when the download falls behind.

use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Seek, SeekFrom},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc as channel,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};

use crate::{
    audio_io::Output,
    frames::Writer,
    stream_worker::{Control, Report},
    video::{Next, Source, Video, fit},
};

/// What the sound plays at: 48 kHz stereo, as Yapfast's other sound does.
const RATE: u32 = 48_000;
const CHANNELS: usize = 2;
/// How far ahead the decoder fills its queues.
const QUEUED_PICTURES: usize = 8;
const QUEUED_SOUND_MS: u64 = 2_000;
/// How far ahead the decoder keeps sound while the pictures' queue is full.
const SOUND_AHEAD_MS: u64 = 500;
/// How far sound may stray from the clock before it's skipped or waited for.
const SOUND_SLACK_MS: u64 = 80;

/// A file Yapfast is still downloading, which reading waits for.
struct Download {
    file: File,
    progress: Arc<Progress>,
    /// Set when a read should give up: Yapfast stops or moves elsewhere.
    interrupt: Arc<AtomicBool>,
}

/// How a download is going, which Yapfast tells the worker.
#[derive(Default)]
pub struct Progress {
    /// [`DOWNLOADING`], [`DOWNLOADED`] or [`FAILED`].
    state: AtomicU8,
    /// The file's size, once known; 0 before.
    size: AtomicU64,
}

const DOWNLOADING: u8 = 0;
const DOWNLOADED: u8 = 1;
const FAILED: u8 = 2;

impl Progress {
    fn new(size: Option<u64>) -> Self {
        Self {
            state: AtomicU8::new(DOWNLOADING),
            size: AtomicU64::new(size.unwrap_or(0)),
        }
    }

    fn downloaded(&self, size: u64) {
        self.size.store(size, Ordering::Relaxed);
        self.state.store(DOWNLOADED, Ordering::Release);
    }

    fn failed(&self) {
        self.state.store(FAILED, Ordering::Release);
    }
}

impl Read for Download {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let read = self.file.read(buffer)?;
            if read > 0 || buffer.is_empty() {
                return Ok(read);
            }
            match self.progress.state.load(Ordering::Acquire) {
                DOWNLOADED => return Ok(0),
                FAILED => return Err(std::io::Error::other("the download failed")),
                _ => {}
            }
            if self.interrupt.load(Ordering::Relaxed) {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Seek for Download {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(to)
    }
}

impl Source for Download {
    fn size(&mut self) -> Option<u64> {
        Some(self.progress.size.load(Ordering::Relaxed)).filter(|size| *size > 0)
    }
}

/// The time in the video, which runs while it plays.
#[derive(Clone, Copy)]
struct Clock {
    at_ms: u64,
    since: Option<Instant>,
}

impl Clock {
    fn now(&self) -> u64 {
        self.at_ms
            + self
                .since
                .map_or(0, |since| since.elapsed().as_millis() as u64)
    }

    fn run(&mut self, running: bool) {
        if running != self.since.is_some() {
            self.at_ms = self.now();
            self.since = running.then(Instant::now);
        }
    }

    fn set(&mut self, at_ms: u64) {
        self.at_ms = at_ms;
        if self.since.is_some() {
            self.since = Some(Instant::now());
        }
    }
}

struct Picture {
    time: u64,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Sound at [`RATE`] in [`CHANNELS`], from `time`; `at` frames of it played.
struct Sound {
    time: u64,
    samples: Vec<f32>,
    at: usize,
}

impl Sound {
    /// When its next frame plays.
    fn next_time(&self) -> u64 {
        self.time + (self.at / CHANNELS) as u64 * 1000 / u64::from(RATE)
    }

    fn frames_left(&self) -> usize {
        (self.samples.len() - self.at) / CHANNELS
    }
}

/// What the file is, once the decoder opened it.
struct Opened {
    duration_ms: Option<u64>,
    width: u32,
    height: u32,
    sound: bool,
}

struct State {
    /// Bumped by each move, so what was queued before it goes.
    generation: u64,
    pictures: VecDeque<Picture>,
    sound: VecDeque<Sound>,
    /// Frames of sound queued.
    sound_frames: usize,
    /// The decoder reached the end of the file.
    ended: bool,
    /// A move the decoder hasn't made yet.
    seek: Option<u64>,
    /// After a move, the next picture shows even while paused.
    show_next: bool,
    /// The largest pictures are drawn.
    size: (u32, u32),
    /// When the picture showing shows in the file.
    shown: Option<u64>,
    playing: bool,
    clock: Clock,
    volume: f32,
    stop: bool,
    opened: Option<Opened>,
    failed: Option<String>,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    interrupt: Arc<AtomicBool>,
}

impl Shared {
    fn new(size: (u32, u32), volume: f32, interrupt: Arc<AtomicBool>) -> Self {
        Self {
            state: Mutex::new(State {
                generation: 0,
                pictures: VecDeque::new(),
                sound: VecDeque::new(),
                sound_frames: 0,
                ended: false,
                seek: None,
                show_next: true,
                size,
                shown: None,
                playing: true,
                clock: Clock {
                    at_ms: 0,
                    since: None,
                },
                volume,
                stop: false,
                opened: None,
                failed: None,
            }),
            changed: Condvar::new(),
            interrupt,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// What the job needs besides its file.
pub struct Playing {
    pub path: std::path::PathBuf,
    /// The file's size, when Yapfast knows it before it's downloaded.
    pub size: Option<u64>,
    /// Pictures are drawn to fit this.
    pub width: u32,
    pub height: u32,
    /// Whether to play its sound, on these speakers or the default ones.
    pub sound: bool,
    pub speakers: Option<String>,
    pub volume: f32,
}

/// Plays the video until Yapfast stops it, reporting what it does.
pub fn play(
    job: Playing,
    mut writer: Writer,
    controls: channel::Receiver<Control>,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<()> {
    let progress = Arc::new(Progress::new(job.size));
    let interrupt = Arc::new(AtomicBool::new(false));
    let download = Download {
        file: File::open(&job.path).context("could not open the video")?,
        progress: progress.clone(),
        interrupt: interrupt.clone(),
    };
    let shared = Arc::new(Shared::new((job.width, job.height), job.volume, interrupt));
    let speaker = job
        .sound
        .then(|| {
            // About 100 ms queued: enough to ride out a busy moment.
            Output::<f32>::open_or_default(
                job.speakers.as_deref(),
                "Video",
                RATE,
                CHANNELS as u16,
                Some(Duration::from_millis(100)),
            )
            .context("could not open the speakers")
            .inspect_err(|error| {
                tracing::warn!(error = format!("{error:#}"), "playing the video silently")
            })
            .ok()
        })
        .flatten();
    let decoder = {
        let shared = shared.clone();
        let sound = speaker.is_some();
        std::thread::Builder::new()
            .name("yapfast-video-decoder".into())
            .spawn(move || decode(download, sound, &shared))
            .context("could not start decoding the video")?
    };
    let speaker = match speaker {
        Some(output) => {
            let shared = shared.clone();
            Some(
                std::thread::Builder::new()
                    .name("yapfast-video-sound".into())
                    .spawn(move || play_sound(output, &shared))
                    .context("could not start the video's sound")?,
            )
        }
        None => None,
    };
    let result = show(&shared, &mut writer, &controls, &progress, stop, report);
    shared.lock().stop = true;
    shared.interrupt.store(true, Ordering::Relaxed);
    shared.changed.notify_all();
    let _ = decoder.join();
    if let Some(speaker) = speaker {
        let _ = speaker.join();
    }
    result
}

/// The job's own thread: shows pictures as the clock reaches them and takes
/// Yapfast's controls, until it stops.
fn show(
    shared: &Shared,
    writer: &mut Writer,
    controls: &channel::Receiver<Control>,
    progress: &Progress,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<()> {
    let mut opened = false;
    let mut finished = false;
    while !stop.load(Ordering::Relaxed) {
        let wait = {
            let state = shared.lock();
            match state.pictures.front() {
                _ if !state.playing => Duration::from_millis(100),
                Some(picture) => Duration::from_millis(
                    picture.time.saturating_sub(state.clock.now()).clamp(1, 20),
                ),
                None => Duration::from_millis(10),
            }
        };
        match controls.recv_timeout(wait) {
            Ok(control) => {
                finished &= !matches!(control, Control::Seek(_) | Control::Play);
                take(shared, progress, control);
            }
            Err(channel::RecvTimeoutError::Timeout) => {}
            Err(channel::RecvTimeoutError::Disconnected) => break,
        }
        let mut state = shared.lock();
        if let Some(error) = state.failed.take() {
            anyhow::bail!(error);
        }
        if !opened && let Some(file) = &state.opened {
            opened = true;
            report(Report::Opened {
                duration_ms: file.duration_ms,
                width: file.width,
                height: file.height,
                sound: file.sound,
            });
        }
        // The clock waits while there's nothing to play.
        let now = state.clock.now();
        let waiting = state.pictures.is_empty()
            && !state.ended
            && !state.sound.iter().any(|sound| sound.next_time() >= now);
        let running = state.playing && !waiting;
        state.clock.run(running);
        let now = state.clock.now();
        let mut shown = false;
        while let Some(picture) = state.pictures.front() {
            let due = state.show_next || (state.playing && picture.time <= now);
            if !due {
                break;
            }
            // A picture whose successor is due too came too late to show.
            let late =
                !state.show_next && state.pictures.get(1).is_some_and(|next| next.time <= now);
            let picture = state.pictures.pop_front().expect("a picture");
            shown = true;
            if !late && writer.fits(picture.width, picture.height) {
                state.show_next = false;
                state.shown = Some(picture.time);
                writer.write_timed(picture.width, picture.height, picture.time, |pixels| {
                    pixels.copy_from_slice(&picture.rgba);
                });
            }
        }
        if shown {
            shared.changed.notify_all();
        }
        if state.ended && state.pictures.is_empty() && state.sound.is_empty() && state.playing {
            state.playing = false;
            state.clock.run(false);
            shared.changed.notify_all();
            if !finished {
                finished = true;
                report(Report::Finished);
            }
        }
    }
    Ok(())
}

/// Applies one of Yapfast's controls.
fn take(shared: &Shared, progress: &Progress, control: Control) {
    let mut state = shared.lock();
    match control {
        Control::Play => {
            if state.ended && state.pictures.is_empty() && state.sound.is_empty() {
                move_to(shared, &mut state, 0);
            }
            state.playing = true;
        }
        Control::Pause => {
            state.playing = false;
            state.clock.run(false);
        }
        Control::Seek(time_ms) => move_to(shared, &mut state, time_ms),
        Control::Volume(volume) => state.volume = volume,
        Control::Size { width, height } => {
            if state.size != (width, height) {
                state.size = (width, height);
                // A still picture is drawn again at the new size. Playing,
                // the next pictures come at it: moving back to the one
                // showing would decode again from the keyframe before it on
                // each change, holding the video while the window resizes.
                if let Some(shown) = state.shown.filter(|_| !state.playing || state.ended) {
                    move_to(shared, &mut state, shown);
                }
            }
        }
        Control::Downloaded { size } => match size {
            Some(size) => progress.downloaded(size),
            None => progress.failed(),
        },
        Control::Send(_) | Control::Switch(_) | Control::Hear(_) => {}
    }
    shared.changed.notify_all();
}

/// Moves to `time_ms`: what's queued goes, and the decoder starts again from
/// the keyframe before it.
fn move_to(shared: &Shared, state: &mut State, time_ms: u64) {
    state.generation += 1;
    state.seek = Some(time_ms);
    state.pictures.clear();
    state.sound.clear();
    state.sound_frames = 0;
    state.ended = false;
    state.show_next = true;
    state.clock.set(time_ms);
    shared.interrupt.store(true, Ordering::Relaxed);
}

/// The decoder's thread: opens the file, then keeps the queues filled.
fn decode(download: Download, sound: bool, shared: &Shared) {
    let Some(mut video) = Video::open(download, sound) else {
        let mut state = shared.lock();
        if !state.stop {
            state.failed = Some("Yapfast can't play this video.".into());
        }
        return;
    };
    let mut resampler = video
        .sound
        .map(|(rate, channels)| Resampler::new(rate, channels));
    shared.lock().opened = Some(Opened {
        duration_ms: video.duration_ms,
        width: video.width,
        height: video.height,
        sound: resampler.is_some(),
    });
    // Pictures and sound before a move's time are decoded, not shown.
    let mut skip_until = 0;
    let mut last_picture = 0;
    // Since the last move, the last picture skipped, and whether one came
    // to show.
    let mut skipped = None;
    let mut queued = false;
    // Reading on to sound held back all the pictures it may; it goes on
    // once some are taken.
    let mut held = false;
    loop {
        let (generation, size, sound_only) = {
            let mut state = shared.lock();
            let sound_only = loop {
                if state.stop {
                    return;
                }
                if let Some(time_ms) = state.seek.take() {
                    drop(state);
                    shared.interrupt.store(false, Ordering::Relaxed);
                    if let Err(error) = video.seek(time_ms) {
                        tracing::debug!(
                            error = format!("{error:#}"),
                            "could not move in the video"
                        );
                    }
                    if let Some(resampler) = resampler.as_mut() {
                        resampler.reset();
                    }
                    skip_until = time_ms;
                    skipped = None;
                    queued = false;
                    held = false;
                    state = shared.lock();
                    continue;
                }
                let sound_ms = state.sound_frames as u64 * 1000 / u64::from(RATE);
                if !state.ended && sound_ms < QUEUED_SOUND_MS {
                    if state.pictures.len() < QUEUED_PICTURES {
                        break false;
                    }
                    // Sound trails its pictures in the file: read on to it.
                    if resampler.is_some() && !held && sound_ms < SOUND_AHEAD_MS {
                        break true;
                    }
                }
                state = shared
                    .changed
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            };
            (state.generation, state.size, sound_only)
        };
        let next = if sound_only {
            match video.decode_sound() {
                Ok(Some(time)) => Ok(Next::Sound(time)),
                Ok(None) => {
                    held = true;
                    continue;
                }
                Err(error) => Err(error),
            }
        } else {
            held = false;
            video.decode()
        };
        let mut state = shared.lock();
        if state.generation != generation || state.seek.is_some() {
            continue;
        }
        match next {
            Ok(Next::Picture(time)) => {
                let time = time.unwrap_or(last_picture + 40);
                last_picture = time;
                if time < skip_until {
                    skipped = Some(time);
                    continue;
                }
                queued = true;
                drop(state);
                let (width, height) = fit(video.width, video.height, size.0, size.1);
                let mut rgba = vec![0; width as usize * height as usize * 4];
                if let Err(error) = video.picture(&mut rgba, width, height) {
                    tracing::debug!(
                        error = format!("{error:#}"),
                        "could not draw a video picture"
                    );
                    continue;
                }
                let mut state = shared.lock();
                if state.generation == generation {
                    state.pictures.push_back(Picture {
                        time,
                        width,
                        height,
                        rgba,
                    });
                }
            }
            Ok(Next::Sound(time)) => {
                let Some(resampler) = resampler.as_mut() else {
                    continue;
                };
                let sound = video.sound();
                resampler.follow(sound.rate, sound.channels);
                let samples = resampler.run(sound.samples);
                let time = time.unwrap_or_else(|| {
                    state.sound.back().map_or(skip_until, |sound| {
                        sound.time
                            + sound.samples.len() as u64 / CHANNELS as u64 * 1000 / u64::from(RATE)
                    })
                });
                let length = samples.len() / CHANNELS;
                if time + length as u64 * 1000 / u64::from(RATE) <= skip_until {
                    continue;
                }
                state.sound_frames += length;
                state.sound.push_back(Sound {
                    time,
                    samples,
                    at: 0,
                });
            }
            // A move past the last picture shows the last one.
            Ok(Next::End) if !queued && skipped.is_some() => state.seek = skipped.take(),
            Ok(Next::End) => state.ended = true,
            // A read a move or stop interrupted; the loop takes the move.
            Err(_) if shared.interrupt.load(Ordering::Relaxed) => {}
            Err(error) => {
                tracing::debug!(error = format!("{error:#}"), "the video stopped decoding");
                state.ended = true;
            }
        }
    }
}

/// The sound thread: plays queued sound when the clock reaches it.
fn play_sound(mut output: Output<f32>, shared: &Shared) {
    // 10 ms at a time.
    let frames = RATE as usize / 100;
    let mut generation = None;
    let mut flushed = true;
    let mut chunk = Vec::with_capacity(frames * CHANNELS);
    loop {
        let latency = output.latency().as_millis() as u64;
        let mut state = shared.lock();
        if state.stop {
            return;
        }
        if !state.playing || state.clock.since.is_none() || generation != Some(state.generation) {
            generation = Some(state.generation);
            if !flushed {
                drop(state);
                output.flush();
                flushed = true;
                continue;
            }
            if !state.playing || state.clock.since.is_none() {
                let _ = shared
                    .changed
                    .wait_timeout(state, Duration::from_millis(100));
                continue;
            }
        }
        flushed = false;
        // What's written now is heard after the speakers' latency.
        let target = state.clock.now() + latency;
        chunk.clear();
        while chunk.len() < frames * CHANNELS {
            let wanted = frames - chunk.len() / CHANNELS;
            let Some(sound) = state.sound.front_mut() else {
                break;
            };
            let time = sound.next_time();
            if time + SOUND_SLACK_MS < target {
                // Behind the clock: skip to it.
                let behind = ((target - time) * u64::from(RATE) / 1000) as usize;
                let skipped = behind.min(sound.frames_left());
                sound.at += skipped * CHANNELS;
                state.sound_frames -= skipped;
            } else if time > target + SOUND_SLACK_MS {
                // Ahead of it: silence until it's due.
                let early = ((time - target) * u64::from(RATE) / 1000) as usize;
                chunk.resize(chunk.len() + early.min(wanted) * CHANNELS, 0.0);
                break;
            } else {
                let taken = wanted.min(sound.frames_left());
                chunk.extend_from_slice(&sound.samples[sound.at..sound.at + taken * CHANNELS]);
                sound.at += taken * CHANNELS;
                state.sound_frames -= taken;
            }
            if state
                .sound
                .front()
                .is_some_and(|sound| sound.frames_left() == 0)
            {
                state.sound.pop_front();
            }
        }
        // Nothing to play keeps the speakers fed, so their latency holds.
        chunk.resize(frames * CHANNELS, 0.0);
        let volume = state.volume;
        drop(state);
        shared.changed.notify_all();
        if volume != 1.0 {
            for sample in &mut chunk {
                *sample *= volume;
            }
        }
        if let Err(error) = output.write(&chunk) {
            tracing::warn!(
                error = format!("{error:#}"),
                "stopped playing the video's sound"
            );
            return;
        }
    }
}

/// Turns sound of any rate and channels into [`RATE`] stereo, by linear
/// interpolation.
pub(crate) struct Resampler {
    channels: usize,
    /// Input frames for each output frame.
    step: f64,
    /// Where the next output frame falls, in input frames after `last`.
    position: f64,
    /// The previous input's last frame, or none at the start.
    last: Option<[f32; 2]>,
}

impl Resampler {
    pub(crate) fn new(rate: u32, channels: usize) -> Self {
        Self {
            channels,
            step: f64::from(rate) / f64::from(RATE),
            position: 0.0,
            last: None,
        }
    }

    fn reset(&mut self) {
        self.position = 0.0;
        self.last = None;
    }

    /// Takes sound at `rate` with `channels` from now on, as a file's
    /// decoder can change them partway.
    pub(crate) fn follow(&mut self, rate: u32, channels: usize) {
        if rate > 0 {
            self.step = f64::from(rate) / f64::from(RATE);
        }
        if channels > 0 {
            self.channels = channels;
        }
    }

    pub(crate) fn run(&mut self, input: &[f32]) -> Vec<f32> {
        let frames: Vec<[f32; 2]> = input
            .chunks_exact(self.channels.max(1))
            .map(stereo)
            .collect();
        let Some(&end) = frames.last() else {
            return Vec::new();
        };
        if self.step == 1.0 {
            return frames.into_iter().flatten().collect();
        }
        // At the start, the first frame stands in for the one before it.
        if self.last.is_none() {
            self.position = 1.0;
        }
        let first = *self.last.get_or_insert(frames[0]);
        let at = |index: usize| if index == 0 { first } else { frames[index - 1] };
        let mut output = Vec::with_capacity((frames.len() as f64 / self.step) as usize * 2 + 4);
        while self.position < frames.len() as f64 {
            let index = self.position as usize;
            let fraction = (self.position - index as f64) as f32;
            let (from, to) = (at(index), at(index + 1));
            output.push(from[0] + (to[0] - from[0]) * fraction);
            output.push(from[1] + (to[1] - from[1]) * fraction);
            self.position += self.step;
        }
        self.position -= frames.len() as f64;
        self.last = Some(end);
        output
    }
}

/// One frame of any channels as stereo: surround's centre goes to both sides.
fn stereo(frame: &[f32]) -> [f32; 2] {
    match frame {
        [mono] => [*mono, *mono],
        [left, right] => [*left, *right],
        [left, right, centre, rest @ ..] => {
            let (back_left, back_right) = match rest {
                [_, back_left, back_right, ..] => (*back_left, *back_right),
                _ => (0.0, 0.0),
            };
            [
                (left + 0.7 * centre + 0.7 * back_left) * 0.6,
                (right + 0.7 * centre + 0.7 * back_right) * 0.6,
            ]
        }
        [] => [0.0, 0.0],
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn resamples_to_48_khz_stereo() {
        let mut resampler = Resampler::new(24_000, 1);
        let first = resampler.run(&[0.0, 1.0, 0.0, -1.0]);
        let second = resampler.run(&[0.0, 1.0]);
        // Twice the frames, but for the last's, each on both sides,
        // continuing across calls.
        assert_eq!((first.len() + second.len()) / 2, 10);
        let left: Vec<f32> = first.iter().chain(&second).step_by(2).copied().collect();
        assert_eq!(left, [0.0, 0.5, 1.0, 0.5, 0.0, -0.5, -1.0, -0.5, 0.0, 0.5]);
        assert_eq!(
            first.iter().step_by(2).collect::<Vec<_>>(),
            first.iter().skip(1).step_by(2).collect::<Vec<_>>()
        );
        let same = Resampler::new(48_000, 2).run(&[0.1, 0.2, 0.3, 0.4]);
        assert_eq!(same, [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn surround_folds_into_stereo() {
        assert_eq!(stereo(&[0.5]), [0.5, 0.5]);
        assert_eq!(stereo(&[0.1, 0.2]), [0.1, 0.2]);
        let [left, right] = stereo(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert!(left > 0.0 && right == 0.0);
    }

    #[test]
    fn the_clock_holds_while_paused() {
        let mut clock = Clock {
            at_ms: 0,
            since: None,
        };
        assert_eq!(clock.now(), 0);
        clock.run(true);
        std::thread::sleep(Duration::from_millis(30));
        clock.run(false);
        let paused = clock.now();
        assert!(paused >= 30);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(clock.now(), paused);
        clock.set(5_000);
        assert_eq!(clock.now(), 5_000);
    }

    /// Plays a fixture as the worker does, the file arriving after a moment.
    #[test]
    fn plays_a_video_as_it_downloads() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("video.mp4");
        let video = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/fixtures/video-aac.mp4"),
        )
        .unwrap();
        std::fs::write(&path, &video[..video.len() / 2]).unwrap();
        let frames = crate::frames::Frames::create(160, 90).unwrap();
        let writer = Writer::open(&frames.path().unwrap()).unwrap();
        let (controls, received) = channel::channel();
        let (reports, reported) = channel::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let job = Playing {
            path: path.clone(),
            size: Some(video.len() as u64),
            width: 80,
            height: 80,
            sound: false,
            speakers: None,
            volume: 1.0,
        };
        let player = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                play(job, writer, received, &stop, &mut |report| {
                    let _ = reports.send(report);
                })
            })
        };
        let report = || reported.recv_timeout(Duration::from_secs(5)).unwrap();
        // Pictures after the first half wait for the rest of the file.
        let started = Instant::now();
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(&path, &video).unwrap();
        controls
            .send(Control::Downloaded {
                size: Some(video.len() as u64),
            })
            .unwrap();
        assert!(matches!(
            report(),
            Report::Opened {
                width: 160,
                height: 90,
                duration_ms: Some(1900..=2100),
                ..
            }
        ));
        let mut times = vec![];
        while !matches!(reported.try_recv(), Ok(Report::Finished)) {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "it plays to the end"
            );
            if let Some(((width, height), time)) =
                frames.take_timed(|width, height, _| (width, height))
            {
                assert_eq!((width, height), (80, 45), "fit to the size asked for");
                times.push(time);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        // The last picture comes with the end.
        times.extend(frames.take_timed(|_, _, _| ()).map(|(_, time)| time));
        // About two seconds, in order, mostly every picture.
        assert!(
            started.elapsed() >= Duration::from_millis(1900),
            "{:?}",
            started.elapsed()
        );
        assert!(times.windows(2).all(|pair| pair[0] < pair[1]), "{times:?}");
        assert!(
            times.len() > 25 && *times.last().unwrap() >= 1900,
            "{times:?}"
        );

        // Moving while it waits at the end shows the picture there; playing
        // goes on from it.
        controls.send(Control::Seek(1000)).unwrap();
        let shown = || {
            let waiting = Instant::now();
            loop {
                if let Some((_, time)) = frames.take_timed(|_, _, _| ()) {
                    return time;
                }
                assert!(
                    waiting.elapsed() < Duration::from_secs(5),
                    "a picture shows"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        assert_eq!(shown(), 1000);
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            frames.take_timed(|_, _, _| ()).is_none(),
            "paused at the end"
        );
        controls.send(Control::Play).unwrap();
        let mut last = shown();
        assert!(last > 1000);
        // Resizing while it plays, as while dragging the window's edge,
        // doesn't hold it back: pictures go on at the new size.
        let mut times = vec![last];
        for step in 0..20 {
            controls
                .send(Control::Size {
                    width: 80 + step % 2 * 20,
                    height: 80,
                })
                .unwrap();
            std::thread::sleep(Duration::from_millis(10));
            times.extend(frames.take_timed(|_, _, _| ()).map(|(_, time)| time));
        }
        assert!(times.windows(2).all(|pair| pair[0] < pair[1]), "{times:?}");
        assert!(*times.last().unwrap() >= last + 100, "{times:?}");
        last = *times.last().unwrap();
        controls.send(Control::Pause).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        if let Some((_, time)) = frames.take_timed(|_, _, _| ()) {
            last = time;
        }
        std::thread::sleep(Duration::from_millis(200));
        assert!(frames.take_timed(|_, _, _| ()).is_none(), "paused");

        // Larger, as in the viewer: the picture showing is drawn again at
        // the new size, and it stays paused.
        controls
            .send(Control::Size {
                width: 160,
                height: 160,
            })
            .unwrap();
        let redrawn = loop {
            if let Some(redrawn) = frames.take_timed(|width, height, _| (width, height)) {
                break redrawn;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(redrawn, ((160, 90), last));
        std::thread::sleep(Duration::from_millis(200));
        assert!(frames.take_timed(|_, _, _| ()).is_none(), "still paused");
        // Moving past the last picture shows the last one.
        controls.send(Control::Seek(60_000)).unwrap();
        assert!(shown() >= 1900);
        stop.store(true, Ordering::Relaxed);
        player.join().unwrap().unwrap();
    }

    /// In a file, sound trails the pictures it plays with, by up to a
    /// second. With pictures queued and none shown yet, the decoder still
    /// reads on to the sound that plays with them.
    #[test]
    fn sound_keeps_ahead_while_pictures_wait() {
        // As a phone or a fragmented MP4 has it: a second of pictures, then
        // the second's sound.
        for name in ["video-fragmented.mp4", "video-aac.mp4", "video-opus.webm"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../testing/fixtures")
                .join(name);
            let progress = Arc::new(Progress::new(None));
            progress.downloaded(std::fs::metadata(&path).unwrap().len());
            let interrupt = Arc::new(AtomicBool::new(false));
            let download = Download {
                file: File::open(&path).unwrap(),
                progress,
                interrupt: interrupt.clone(),
            };
            let shared = Arc::new(Shared::new((80, 80), 1.0, interrupt));
            let decoder = {
                let shared = shared.clone();
                std::thread::spawn(move || decode(download, true, &shared))
            };
            let started = Instant::now();
            while shared.lock().pictures.len() < QUEUED_PICTURES {
                assert!(started.elapsed() < Duration::from_secs(5), "{name}");
                std::thread::sleep(Duration::from_millis(5));
            }
            // Until it rests.
            std::thread::sleep(Duration::from_millis(200));
            {
                let state = shared.lock();
                let sound_ms = state.sound_frames as u64 * 1000 / u64::from(RATE);
                assert!(sound_ms >= SOUND_AHEAD_MS, "{name}: {sound_ms} ms of sound");
                assert_eq!(state.pictures.len(), QUEUED_PICTURES, "{name}");
            }
            shared.lock().stop = true;
            shared.changed.notify_all();
            decoder.join().unwrap();
        }
    }

    #[test]
    fn reading_waits_for_the_download() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("video");
        std::fs::write(&path, b"abc").unwrap();
        let progress = Arc::new(Progress::new(None));
        let mut download = Download {
            file: File::open(&path).unwrap(),
            progress: progress.clone(),
            interrupt: Arc::new(AtomicBool::new(false)),
        };
        assert_eq!(download.size(), None);
        let writer = {
            let (path, progress) = (path.clone(), progress.clone());
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
                std::io::Write::write_all(&mut file, b"def").unwrap();
                progress.downloaded(6);
            })
        };
        let mut read = Vec::new();
        download.read_to_end(&mut read).unwrap();
        writer.join().unwrap();
        assert_eq!(read, b"abcdef");
        assert_eq!(download.size(), Some(6));
    }
}
