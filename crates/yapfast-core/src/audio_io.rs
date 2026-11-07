//! Yapfast's own sound on this machine, outside LiveKit's calls: Discord voice,
//! the input meter, ringing, Yapfast's sounds, and a stream's or a video's sound.
//! A device is named by its id from [`crate::voice::State`], as LiveKit lists
//! it, and `None` is the system's default. Streams block on the caller's own
//! thread: [`Input::read`] waits for the microphone, and [`Output::write`] for
//! room in the speakers' queue. Samples go to the system as they are, with no
//! copy of their own.
//!
//! Linux goes through PulseAudio, or PipeWire's PulseAudio server, and
//! Windows through WASAPI. Other systems have no sound here yet.

#[cfg(feature = "discord")]
use std::thread::JoinHandle;
use std::{
    marker::PhantomData,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use anyhow::{Result, ensure};

#[cfg(target_os = "linux")]
mod pulse;
#[cfg(target_os = "linux")]
use pulse as platform;

#[cfg(target_os = "windows")]
mod wasapi;
#[cfg(target_os = "windows")]
use wasapi as platform;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod unsupported;
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
use unsupported as platform;

#[cfg(feature = "discord")]
mod lasting;
#[cfg(feature = "discord")]
pub use lasting::{LastingInput, LastingOutput};

/// Whether this system has sound here.
pub const AVAILABLE: bool = cfg!(any(target_os = "linux", target_os = "windows"));

/// The rate every stream runs at, unless it asks for another.
pub const RATE: u32 = 48_000;

/// A sample a stream carries: 16-bit integers or 32-bit floats, in the
/// machine's own byte order.
pub trait Sample: Copy + Default + Send + 'static {
    #[doc(hidden)]
    const FLOAT: bool;
}

impl Sample for i16 {
    const FLOAT: bool = false;
}

impl Sample for f32 {
    const FLOAT: bool = true;
}

/// What a stream carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Format {
    pub rate: u32,
    pub channels: u16,
    pub float: bool,
}

impl Format {
    fn of<S: Sample>(rate: u32, channels: u16) -> Self {
        Self {
            rate,
            channels,
            float: S::FLOAT,
        }
    }

    pub fn frame_bytes(self) -> usize {
        usize::from(self.channels) * if self.float { 4 } else { 2 }
    }

    /// The bytes `duration` of this takes.
    pub fn bytes(self, duration: Duration) -> usize {
        (u128::from(self.rate) * duration.as_micros() / 1_000_000) as usize * self.frame_bytes()
    }
}

/// Samples as the bytes they're made of.
fn bytes<S: Sample>(samples: &[S]) -> &[u8] {
    // SAFETY: i16 and f32 have no padding and any alignment suits u8.
    unsafe { std::slice::from_raw_parts(samples.as_ptr().cast(), size_of_val(samples)) }
}

fn bytes_mut<S: Sample>(samples: &mut [S]) -> &mut [u8] {
    // SAFETY: as above, and every bit pattern is a valid i16 or f32.
    unsafe { std::slice::from_raw_parts_mut(samples.as_mut_ptr().cast(), size_of_val(samples)) }
}

/// What the next turn of a write does: wait for the engine to play the queue
/// it has, or fill `frames` of its room and, if the stream wasn't playing,
/// start it once they're queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) enum Step {
    Write { frames: u32, start: bool },
    Wait,
}

/// How a write goes on with `queued` frames already waiting in a queue of
/// `queue`, a stream `running` says whether it plays already, and
/// `frames_left` whole frames still to write. A full queue is what makes a
/// write wait; sound is heard as soon as any of it is queued, so the first
/// write starts the stream however short it is.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn write_step(queued: u32, queue: u32, running: bool, frames_left: u32) -> Step {
    let room = queue.saturating_sub(queued);
    if room == 0 {
        return Step::Wait;
    }
    let frames = room.min(frames_left);
    Step::Write {
        frames,
        start: frames > 0 && !running,
    }
}

/// How long a drain waits for `queue_frames` queued frames at `rate` to
/// play, plus `grace` for the engine to get going: never less than the queue
/// takes.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn drain_deadline(queue_frames: u32, rate: u32, grace: Duration) -> Duration {
    let rate = u64::from(rate.max(1));
    Duration::from_micros((u64::from(queue_frames) * 1_000_000).div_ceil(rate)) + grace
}

/// Whether a drain with `padding` frames waiting has any to wait for.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn drain_must_wait(padding: u32) -> bool {
    padding > 0
}

/// What waiting for a device found: more to play or record, or the stop the
/// stream waits under, which ends the wait because the stream is ending
/// rather than because the device did anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) enum Waited {
    /// The engine played or recorded more.
    Signalled,
    /// The stream's stop is set: what it was about to play or record goes
    /// with the stream, so nothing is missing.
    Cancelled,
}

/// What one short slice of a wait for the engine leaves to do, `signalled`
/// from the slice and `quiet` for how long the engine hasn't: none waits
/// again, [`Waited::Signalled`] is done, and an engine quiet for all of
/// `silence` is a device that went away rather than one playing. A device
/// that signals every few milliseconds while it runs is only quiet for a
/// slice when it's stopping, so the slices decide nothing on their own.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn wait_step(
    signalled: bool,
    quiet: Duration,
    silence: Duration,
) -> Result<Option<Waited>> {
    if signalled {
        return Ok(Some(Waited::Signalled));
    }
    ensure!(quiet < silence, "the device stopped");
    Ok(None)
}

/// What the engine said about a recorded packet besides its samples: the
/// device had nothing to give and the packet holds silence, and samples
/// between this packet and the one before it were lost. Neither is the device
/// stopping, which is a call that fails or a wait that stays quiet rather
/// than anything a packet says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) struct Packet {
    /// The device gave silence in place of sound, as a muted microphone or a
    /// stream with nothing at its end does.
    pub silent: bool,
    /// The engine lost samples between this packet and the one before it.
    pub gap: bool,
}

/// What a packet says that the one before it didn't, each worth a line when
/// it starts: a microphone giving silence for a minute is one fact, and so is
/// a stream losing samples for a while, so neither costs a line a packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) enum News {
    /// The device has nothing to give, and the packets hold silence.
    Silence,
    /// Samples the engine had for the stream are gone.
    Gap,
    /// Samples sound again after silence or a gap.
    Sound,
}

/// What `packet` says that `last` didn't, `last` left holding what this one
/// said: a gap is news once however many packets follow it, silence is news
/// once for a run of silent packets, and sound is news once when the run
/// ends. A packet that both follows a gap and holds silence is the gap's.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn packet_news(packet: Packet, last: &mut Packet) -> Option<News> {
    let news = if packet.gap && !last.gap {
        Some(News::Gap)
    } else if packet.silent && !last.silent {
        Some(News::Silence)
    } else if !packet.silent && !packet.gap && (last.silent || last.gap) {
        Some(News::Sound)
    } else {
        None
    };
    *last = packet;
    news
}

/// What a platform's write, drain or read says to its caller: a stop ended
/// it because the stream is ending, so nothing failed and there's nothing to
/// report. A stream that outlasts its device opens one again over an error,
/// and a stop must never be one.
fn ended(waited: Result<Waited>) -> Result<()> {
    waited.map(|_| ())
}

/// Speakers Yapfast plays to, until dropped.
pub struct Output<S: Sample> {
    stream: platform::Output,
    _samples: PhantomData<S>,
    /// Frames of each channel handed to the device since it opened, which
    /// together with what it still holds queued is the position the sound is
    /// really at.
    written: u64,
    /// The bytes one frame of every channel is, which is what turns what was
    /// written into frames.
    frame_bytes: usize,
}

impl<S: Sample> Output<S> {
    /// Opens `speaker` for `channels` at `rate`. The system lists the stream
    /// as `name`. Writing waits once about `queue` is waiting to play, or
    /// what the system chooses for none.
    pub fn open(
        speaker: Option<&str>,
        name: &'static str,
        rate: u32,
        channels: u16,
        queue: Option<Duration>,
    ) -> Result<Self> {
        let format = Format::of::<S>(rate, channels);
        let frame_bytes = format.frame_bytes();
        let stream = platform::Output::open(speaker, name, format, queue)?;
        Ok(Self {
            stream,
            _samples: PhantomData,
            written: 0,
            frame_bytes,
        })
    }

    /// As [`Output::open`], but speakers that are gone or won't open give way
    /// to the default ones.
    pub fn open_or_default(
        speaker: Option<&str>,
        name: &'static str,
        rate: u32,
        channels: u16,
        queue: Option<Duration>,
    ) -> Result<Self> {
        match Self::open(speaker, name, rate, channels, queue) {
            Err(error) if speaker.is_some() => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    stream = name,
                    "playing on the default speakers"
                );
                Self::open(None, name, rate, channels, queue)
            }
            opened => opened,
        }
    }

    /// Ends a wait for the device as soon as `stop` is set, as the thread
    /// that stops this stream sets it: what the stream was about to play
    /// goes with the stream instead of holding the thread for the device.
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    pub fn stop_with(&mut self, stop: Arc<AtomicBool>) {
        self.stream.stop_with(stop);
    }

    /// Queues interleaved samples to play, waiting while the queue is full.
    /// The stop that ends the stream ends the write too, with sound nobody
    /// waits for any more left behind, which is no error.
    pub fn write(&mut self, samples: &[S]) -> Result<()> {
        let played = ended(self.stream.write(bytes(samples)));
        // Frames handed over, which come back as heard once the device has
        // played them: see `position`.
        if self.frame_bytes > 0 {
            self.written += (samples.len() * size_of::<S>() / self.frame_bytes) as u64;
        }
        played
    }

    /// The frames the device has played since it opened, and the frames it
    /// still holds queued: where the sound really is, rather than how much was
    /// handed over. `None` where the backend can't say, which is not the same
    /// as zero, and `None` while no device is open at all.
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    pub fn position(&self) -> Option<(u64, u64)> {
        let queued = self.stream.queued()?;
        Some((self.written.saturating_sub(queued), queued))
    }

    /// Waits until everything queued has played, or until the stream's stop
    /// ends the wait, which drops what's still queued rather than playing it
    /// late into whatever starts next.
    pub fn drain(&mut self) -> Result<()> {
        ended(self.stream.drain())
    }

    /// Drops everything queued and not yet played.
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    pub fn flush(&mut self) {
        self.stream.flush();
    }

    /// How long until what's written now is heard.
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    pub fn latency(&self) -> Duration {
        self.stream.latency()
    }
}

/// A microphone Yapfast records, until dropped.
pub struct Input<S: Sample> {
    stream: platform::Input,
    _samples: PhantomData<S>,
}

impl<S: Sample> Input<S> {
    /// Opens `microphone` for `channels` at `rate`, delivering about
    /// `fragment` at a time. The system lists the stream as `name`.
    pub fn open(
        microphone: Option<&str>,
        name: &'static str,
        rate: u32,
        channels: u16,
        fragment: Duration,
    ) -> Result<Self> {
        let stream =
            platform::Input::open(microphone, name, Format::of::<S>(rate, channels), fragment)?;
        Ok(Self {
            stream,
            _samples: PhantomData,
        })
    }

    /// Ends a wait for the microphone as soon as `stop` is set, as the
    /// thread that stops this stream sets it: a read ends rather than
    /// holding the thread for a device that stopped answering.
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    pub fn stop_with(&mut self, stop: Arc<AtomicBool>) {
        self.stream.stop_with(stop);
    }

    /// Fills `samples` with what the microphone hears next, interleaved,
    /// waiting for it; the stop that ends the stream fills the rest with
    /// silence instead, and is no error.
    pub fn read(&mut self, samples: &mut [S]) -> Result<()> {
        ended(self.stream.read(bytes_mut(samples)))
    }
}

/// What [`others`] hears from the other applications playing.
#[cfg(feature = "discord")]
pub enum Heard<'a> {
    /// An application started playing, and the process playing it when the
    /// system says which.
    Started {
        source: u32,
        process: Option<u32>,
    },
    /// Interleaved stereo at [`RATE`] from one.
    Samples {
        source: u32,
        samples: &'a [f32],
    },
    Stopped {
        source: u32,
    },
}

/// Hears what every other application plays, at [`RATE`] in stereo, on a
/// thread of its own until `stop`. Yapfast's own processes are left out, and
/// nothing else is: on Linux each application comes on its own, and those
/// whose name or program `skip` picks are left out too, so another Discord
/// client's call is; Windows mixes them all into one stream and can leave out
/// only Yapfast's processes, so its call is shared.
/// Returns once it hears, or with why it can't.
#[cfg(feature = "discord")]
pub fn others(
    skip: fn(&str) -> bool,
    heard: impl FnMut(Heard<'_>) + Send + 'static,
    stop: Arc<AtomicBool>,
) -> Result<JoinHandle<()>> {
    platform::others(skip, Box::new(heard), stop)
}

#[cfg(feature = "discord")]
type Hear = Box<dyn FnMut(Heard<'_>) + Send>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_count_their_bytes() {
        let stereo = Format::of::<i16>(RATE, 2);
        assert_eq!(stereo.frame_bytes(), 4);
        assert_eq!(stereo.bytes(Duration::from_millis(60)), 11_520);
        let video = Format::of::<f32>(RATE, 2);
        assert_eq!(video.bytes(Duration::from_millis(100)), 38_400);
        let meter = Format::of::<i16>(16_000, 1);
        assert_eq!(meter.bytes(Duration::from_millis(20)), 640);
    }

    /// A sound shorter than the queue starts the stream as it's queued, so
    /// it plays rather than waiting for a full queue.
    #[test]
    fn a_short_write_starts_the_stream() {
        assert_eq!(
            write_step(0, 4_800, false, 240),
            Step::Write {
                frames: 240,
                start: true,
            }
        );
    }

    /// A stream that stopped, as [`Output::drain`] stops it, starts again on
    /// its first sound.
    #[test]
    fn a_write_after_drain_starts_again() {
        assert_eq!(
            write_step(0, 4_800, false, 960),
            Step::Write {
                frames: 960,
                start: true
            }
        );
        // Once it plays, only the first write started it.
        assert_eq!(
            write_step(0, 4_800, true, 960),
            Step::Write {
                frames: 960,
                start: false
            }
        );
    }

    /// A write with the queue full waits, and starts a stream that isn't
    /// playing, once, however much it still has to write.
    #[test]
    fn a_full_queue_waits() {
        assert_eq!(write_step(4_800, 4_800, true, 240), Step::Wait);
        assert_eq!(write_step(4_800, 4_800, false, 240), Step::Wait);
        // More waiting than the queue, as an engine's buffer may hold.
        assert_eq!(write_step(5_000, 4_800, true, 240), Step::Wait);
        assert_eq!(
            write_step(4_800, 4_800, false, 0),
            Step::Wait,
            "a full queue waits before a partial frame is even considered"
        );
    }

    /// The bytes left over that don't make a whole frame are no frames to
    /// write, and starting the stream has to wait for the frames there are.
    #[test]
    fn leftover_partial_frame_is_not_a_frame() {
        assert_eq!(
            write_step(0, 4_800, false, 0),
            Step::Write {
                frames: 0,
                start: false
            }
        );
        // Never more than the room the queue has.
        assert_eq!(
            write_step(4_560, 4_800, false, 480),
            Step::Write {
                frames: 240,
                start: true
            }
        );
    }

    #[test]
    fn drain_deadline_covers_the_queue() {
        let grace = Duration::from_secs(1);
        assert_eq!(
            drain_deadline(4_800, RATE, grace),
            Duration::from_millis(100) + grace
        );
        // The clamp: a queue smaller than the engine's buffer.
        assert_eq!(
            drain_deadline(480, RATE, grace),
            Duration::from_millis(10) + grace
        );
    }

    #[test]
    fn drain_waits_only_with_padding() {
        assert!(drain_must_wait(1));
        assert!(!drain_must_wait(0), "nothing queued needs no wait");
    }

    /// A wait slices rather than trusting one long sleep: a slice the engine
    /// didn't signal is nothing wrong, but the whole of `silence` quiet is a
    /// device that went away.
    #[test]
    fn a_quiet_slice_waits_again() {
        let silence = Duration::from_secs(2);
        assert_eq!(
            wait_step(false, Duration::from_millis(100), silence).unwrap(),
            None
        );
        assert_eq!(
            wait_step(true, Duration::from_millis(100), silence).unwrap(),
            Some(Waited::Signalled)
        );
        assert!(
            wait_step(false, silence, silence).is_err(),
            "an engine quiet for two seconds is gone"
        );
    }

    /// A device that gives silence says so once, however long it does: a
    /// microphone muted at the device for a minute is one line of log, and so
    /// is the sound coming back.
    #[test]
    fn a_run_of_silent_packets_is_one_line() {
        let mut said = Packet::default();
        let quiet = Packet {
            silent: true,
            gap: false,
        };
        assert_eq!(packet_news(quiet, &mut said), Some(News::Silence));
        assert_eq!(packet_news(quiet, &mut said), None, "already said");
        assert_eq!(packet_news(quiet, &mut said), None, "and still said");
        let sound = Packet::default();
        assert_eq!(packet_news(sound, &mut said), Some(News::Sound));
        assert_eq!(
            packet_news(sound, &mut said),
            None,
            "sound isn't news twice"
        );
    }

    /// Samples lost are their own news: never silence, never a device that
    /// stopped, and said once for a run of packets after the gap.
    #[test]
    fn lost_samples_are_not_silence() {
        let mut said = Packet::default();
        let gap = Packet {
            silent: false,
            gap: true,
        };
        assert_eq!(packet_news(gap, &mut said), Some(News::Gap));
        assert_eq!(packet_news(gap, &mut said), None, "one gap, one line");
        assert_eq!(packet_news(Packet::default(), &mut said), Some(News::Sound));
        // A packet quiet after a gap is what follows a gap, not the same one
        // twice: the gap is the news, and it's been said.
        let mut said = Packet::default();
        let both = Packet {
            silent: true,
            gap: true,
        };
        assert_eq!(packet_news(both, &mut said), Some(News::Gap));
        let quiet = Packet {
            silent: true,
            gap: false,
        };
        assert_eq!(packet_news(quiet, &mut said), None);
        assert_eq!(packet_news(Packet::default(), &mut said), Some(News::Sound));
    }

    /// A stop is what ending a stream looks like, not a device lost: what the
    /// platform hands back after one says nothing, so nothing that outlasts
    /// its device has an error to open one again over.
    #[test]
    fn a_stopped_wait_is_no_lost_device() {
        assert!(ended(Ok(Waited::Cancelled)).is_ok());
        assert!(ended(Ok(Waited::Signalled)).is_ok());
        // A device going quiet is the one failure, and it stays one.
        assert!(ended(Err(anyhow::anyhow!("the device stopped"))).is_err());
    }

    /// Writes a sound shorter than the queue and hears it play without a
    /// drain: only a stream that started as it was queued is heard.
    #[test]
    #[ignore = "needs Windows speakers"]
    fn a_short_sound_plays_once_written() {
        let mut output = Output::<f32>::open(
            None,
            "Test sound",
            RATE,
            2,
            Some(Duration::from_millis(100)),
        )
        .unwrap();
        let mut sound = vec![0f32; RATE as usize / 50 * 2];
        for (at, pair) in sound.chunks_exact_mut(2).enumerate() {
            pair.fill(tone(at));
        }
        output.write(&sound).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        // The engine's own latency doesn't change for a stream, so what a
        // played-out sound leaves is the same as once drained; a sound never
        // started leaves 20 ms more.
        let unplayed = output.latency();
        output.drain().unwrap();
        let played = output.latency();
        assert!(
            unplayed.saturating_sub(played) < Duration::from_millis(15),
            "the sound was still queued: {unplayed:?} against {played:?} once drained"
        );
    }

    /// Plays a tone on the default speakers and hears it on the default
    /// microphone, which must be what the speakers play, as on a private
    /// PulseAudio whose only devices are null sinks.
    #[test]
    #[ignore = "needs a sound server whose default input hears its default output"]
    fn hears_what_it_plays() {
        let block = RATE as usize / 100;
        let mut input =
            Input::<i16>::open(None, "Test input", RATE, 1, Duration::from_millis(10)).unwrap();
        // Hearing first, so the tone's start isn't lost to the input starting.
        let mut heard = vec![0i16; RATE as usize * 2];
        for chunk in heard[..RATE as usize * 3 / 10].chunks_mut(block) {
            input.read(chunk).unwrap();
        }
        let playing = std::thread::spawn(play_tone);
        for chunk in heard.chunks_mut(block) {
            input.read(chunk).unwrap();
        }
        playing.join().unwrap();
        let peak = heard
            .iter()
            .map(|sample| sample.unsigned_abs())
            .max()
            .unwrap();
        // A second of a 440 Hz tone crosses zero 880 times; silence doesn't.
        let crossings = heard
            .windows(2)
            .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
            .count();
        let loud = |sample: &i16| sample.unsigned_abs() > 1000;
        let at = |index: Option<usize>| index.map(|index| index as f32 / RATE as f32);
        let (first, last) = (heard.iter().position(loud), heard.iter().rposition(loud));
        eprintln!(
            "peak {peak}, {crossings} zero crossings, heard from {:?} s to {:?} s",
            at(first),
            at(last)
        );
        assert!(peak > 6000, "heard too little: peak {peak}");
        assert!(
            (800..960).contains(&crossings),
            "not all of the tone: {crossings} crossings"
        );
    }

    /// Opens every microphone and speaker LiveKit lists by its id, and
    /// reads and writes a little on each.
    #[test]
    #[ignore = "needs sound devices"]
    fn opens_every_listed_device() {
        let crate::voice::Devices {
            microphones,
            speakers,
            ..
        } = crate::voice::list_devices().unwrap();
        assert!(
            !microphones.is_empty() && !speakers.is_empty(),
            "no devices"
        );
        for microphone in &microphones {
            eprintln!("microphone {:?}: {}", microphone.name, microphone.id);
            let mut input = Input::<i16>::open(
                Some(&microphone.id),
                "Test input",
                RATE,
                1,
                Duration::from_millis(10),
            )
            .unwrap();
            input.read(&mut [0; RATE as usize / 100]).unwrap();
        }
        for speaker in &speakers {
            eprintln!("speakers {:?}: {}", speaker.name, speaker.id);
            let mut output = Output::<i16>::open(
                Some(&speaker.id),
                "Test silence",
                RATE,
                2,
                Some(Duration::from_millis(60)),
            )
            .unwrap();
            output.write(&[0; RATE as usize / 10 * 2]).unwrap();
            output.drain().unwrap();
        }
        let gone = if cfg!(windows) {
            "{0.0.0.00000000}.{00000000-0000-0000-0000-000000000000}"
        } else {
            "name:Unplugged speakers"
        };
        assert!(Output::<i16>::open(Some(gone), "Test", RATE, 2, None).is_err());
    }

    fn tone(at: usize) -> f32 {
        0.25 * (at as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin()
    }

    /// Plays a second of a 440 Hz tone on the default speakers.
    fn play_tone() {
        let mut output =
            Output::<f32>::open(None, "Test tone", RATE, 2, Some(Duration::from_millis(60)))
                .unwrap();
        let mut frame = [0f32; RATE as usize / 50 * 2];
        for start in (0..RATE as usize).step_by(frame.len() / 2) {
            for (at, pair) in frame.chunks_exact_mut(2).enumerate() {
                pair.fill(tone(start + at));
            }
            output.write(&frame).unwrap();
        }
        output.drain().unwrap();
    }

    /// For [`hears_other_applications`], from another process.
    #[test]
    #[ignore = "plays a tone"]
    fn plays_a_tone() {
        play_tone();
    }

    /// Hears a tone another process plays, and not one this process plays.
    #[cfg(feature = "discord")]
    #[test]
    #[ignore = "needs a sound server that hears other applications"]
    fn hears_other_applications() {
        use std::sync::{Mutex, atomic::Ordering};

        let heard = Arc::new(Mutex::new(Vec::<f32>::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let into = heard.clone();
        let hearing = others(
            |_| false,
            move |heard| {
                if let Heard::Samples { samples, .. } = heard {
                    into.lock().unwrap().extend_from_slice(samples);
                }
            },
            stop.clone(),
        )
        .unwrap();
        let loudest = |heard: &[f32]| {
            heard
                .iter()
                .fold(0f32, |peak, sample| peak.max(sample.abs()))
        };
        play_tone();
        std::thread::sleep(Duration::from_millis(300));
        let own = loudest(&std::mem::take(&mut *heard.lock().unwrap()));
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "audio_io::tests::plays_a_tone",
                "--exact",
                "--ignored",
                "--quiet",
            ])
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(child.success());
        std::thread::sleep(Duration::from_millis(300));
        stop.store(true, Ordering::Relaxed);
        hearing.join().unwrap();
        let other = heard.lock().unwrap().clone();
        let crossings = other
            .chunks_exact(2)
            .map(|pair| pair[0])
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0))
            .count();
        eprintln!(
            "own tone peak {own}, other's peak {}, {crossings} crossings in {} frames",
            loudest(&other),
            other.len() / 2
        );
        assert!(own < 0.01, "heard its own tone");
        assert!(loudest(&other) > 0.2, "didn't hear the other's tone");
        assert!((800..960).contains(&crossings), "not all of the tone");
    }

    #[test]
    fn samples_are_their_bytes() {
        let samples = [1i16, -2];
        assert_eq!(
            bytes(&samples),
            [1i16.to_ne_bytes(), (-2i16).to_ne_bytes()].concat()
        );
        let mut floats = [0f32; 2];
        bytes_mut(&mut floats)[..4].copy_from_slice(&0.5f32.to_ne_bytes());
        assert_eq!(floats, [0.5, 0.0]);
    }
}
