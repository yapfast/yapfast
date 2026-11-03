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

use std::{marker::PhantomData, time::Duration};
#[cfg(feature = "discord")]
use std::{
    sync::{Arc, atomic::AtomicBool},
    thread::JoinHandle,
};

use anyhow::Result;

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

/// Speakers Yapfast plays to, until dropped.
pub struct Output<S: Sample> {
    stream: platform::Output,
    _samples: PhantomData<S>,
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
        let stream = platform::Output::open(speaker, name, Format::of::<S>(rate, channels), queue)?;
        Ok(Self {
            stream,
            _samples: PhantomData,
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

    /// Queues interleaved samples to play, waiting while the queue is full.
    pub fn write(&mut self, samples: &[S]) -> Result<()> {
        self.stream.write(bytes(samples))
    }

    /// Waits until everything queued has played.
    pub fn drain(&mut self) -> Result<()> {
        self.stream.drain()
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

    /// Fills `samples` with what the microphone hears next, interleaved,
    /// waiting for it.
    pub fn read(&mut self, samples: &mut [S]) -> Result<()> {
        self.stream.read(bytes_mut(samples))
    }
}

/// What [`others`] hears from the other applications playing.
#[cfg(feature = "discord")]
pub enum Heard<'a> {
    /// An application started playing, from this process when the system says.
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
/// thread of its own until `stop`. Yapfast's own processes are left out. On
/// Linux each application comes on its own, and those whose name or program
/// `skip` picks are left out too; Windows mixes them all into one.
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
