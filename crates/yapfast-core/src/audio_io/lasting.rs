//! Streams that outlast their device, for calls: Bluetooth headphones that
//! drop out and come back, or a default device the system changes. When a
//! write or read fails, the stream opens again on its device, or on the
//! default while that's gone; every [`LOOK`] it also sees whether it's on the
//! endpoint it would open now, and moves there if not, so it goes back to
//! the chosen device once that returns, and follows the default on Windows,
//! whose streams stay where they opened. While no device opens at all, the
//! stream keeps time with silence.

use std::time::{Duration, Instant};

use anyhow::Result;

use super::{Input, Output, Sample, platform};

/// How often a stream sees whether it's on the endpoint it should be.
const LOOK: Duration = Duration::from_secs(2);
/// How soon to try again while no device opens.
const RETRY: Duration = Duration::from_millis(500);

type Open<T> = Box<dyn Fn(Option<&str>) -> Result<T> + Send>;
/// [`platform::endpoint`], or a test's stand-in.
type Endpoint = fn(Option<&str>, bool) -> String;

/// A stream, the device it's for, and the endpoint it's on.
struct Lasting<T> {
    device: Option<String>,
    output: bool,
    name: &'static str,
    open: Open<T>,
    endpoint: Endpoint,
    stream: Option<T>,
    /// As `endpoint` names it.
    on: String,
    /// When to look again.
    look: Instant,
}

impl<T> Lasting<T> {
    fn open(
        device: Option<&str>,
        output: bool,
        name: &'static str,
        open: Open<T>,
        endpoint: Endpoint,
    ) -> Result<Self> {
        let mut lasting = Self {
            device: device.map(str::to_owned),
            output,
            name,
            open,
            endpoint,
            stream: None,
            on: String::new(),
            look: Instant::now() + LOOK,
        };
        let should = endpoint(device, output);
        lasting.reopen(should)?;
        Ok(lasting)
    }

    /// Opens the device, or the default if it won't, for a stream that
    /// should be on `should`.
    fn reopen(&mut self, should: String) -> Result<()> {
        // Let go of the old one first: some devices take one stream at a time.
        self.stream = None;
        let (stream, on) = match (self.open)(self.device.as_deref()) {
            Ok(stream) => (stream, should),
            Err(error) if self.device.is_some() => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    stream = self.name,
                    "using the default device"
                );
                ((self.open)(None)?, (self.endpoint)(None, self.output))
            }
            Err(error) => return Err(error),
        };
        self.stream = Some(stream);
        self.on = on;
        Ok(())
    }

    /// The stream to use now, after opening it again if it's gone or on
    /// another endpoint than it should be; none while no device opens.
    fn stream(&mut self) -> Option<&mut T> {
        let now = Instant::now();
        if now >= self.look {
            let should = (self.endpoint)(self.device.as_deref(), self.output);
            if self.stream.is_none() || should != self.on {
                match self.reopen(should) {
                    Ok(()) => tracing::info!(stream = self.name, "device opened again"),
                    Err(error) => tracing::debug!(
                        error = format!("{error:#}"),
                        stream = self.name,
                        "no device opens yet"
                    ),
                }
            }
            self.look = Instant::now() + if self.stream.is_some() { LOOK } else { RETRY };
        }
        self.stream.as_mut()
    }

    /// The stream failed: it opens again at the next use.
    fn lost(&mut self, error: &anyhow::Error) {
        tracing::warn!(
            error = format!("{error:#}"),
            stream = self.name,
            "device stopped; opening it again"
        );
        self.stream = None;
        self.look = Instant::now();
    }
}

/// How long `samples` of interleaved `channels` at `rate` take to play.
fn length(samples: usize, channels: u16, rate: u32) -> Duration {
    Duration::from_secs_f64(samples as f64 / f64::from(channels) / f64::from(rate))
}

/// Speakers that outlast their device; see the [module](self).
pub struct LastingOutput<S: Sample> {
    lasting: Lasting<Output<S>>,
    channels: u16,
    rate: u32,
}

impl<S: Sample> LastingOutput<S> {
    /// As [`Output::open_or_default`]: fails only if neither the speakers
    /// nor the default open.
    pub fn open(
        speaker: Option<&str>,
        name: &'static str,
        rate: u32,
        channels: u16,
        queue: Option<Duration>,
    ) -> Result<Self> {
        let open = move |speaker: Option<&str>| Output::open(speaker, name, rate, channels, queue);
        Ok(Self {
            lasting: Lasting::open(speaker, true, name, Box::new(open), platform::endpoint)?,
            channels,
            rate,
        })
    }

    /// Queues interleaved samples to play, as [`Output::write`]; while no
    /// speakers open, waits as long as they'd have played instead.
    pub fn write(&mut self, samples: &[S]) {
        match self.lasting.stream() {
            Some(stream) => {
                if let Err(error) = stream.write(samples) {
                    self.lasting.lost(&error);
                }
            }
            None => std::thread::sleep(length(samples.len(), self.channels, self.rate)),
        }
    }

    /// Waits until everything queued has played.
    pub fn drain(&mut self) {
        if let Some(stream) = self.lasting.stream.as_mut() {
            let _ = stream.drain();
        }
    }
}

/// A microphone that outlasts its device; see the [module](self).
pub struct LastingInput<S: Sample> {
    lasting: Lasting<Input<S>>,
    channels: u16,
    rate: u32,
}

impl<S: Sample> LastingInput<S> {
    /// As [`Input::open`], or the default microphone if that won't open.
    pub fn open(
        microphone: Option<&str>,
        name: &'static str,
        rate: u32,
        channels: u16,
        fragment: Duration,
    ) -> Result<Self> {
        let open =
            move |microphone: Option<&str>| Input::open(microphone, name, rate, channels, fragment);
        Ok(Self {
            lasting: Lasting::open(microphone, false, name, Box::new(open), platform::endpoint)?,
            channels,
            rate,
        })
    }

    /// Fills `samples` with what the microphone hears next, as
    /// [`Input::read`]; with silence, as slowly as it would come, while no
    /// microphone opens.
    pub fn read(&mut self, samples: &mut [S]) {
        if let Some(stream) = self.lasting.stream() {
            match stream.read(samples) {
                Ok(()) => return,
                Err(error) => self.lasting.lost(&error),
            }
        }
        samples.fill(S::default());
        std::thread::sleep(length(samples.len(), self.channels, self.rate));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use anyhow::bail;

    use super::*;

    /// The devices connected, as the test's sound server has them: the
    /// headphones and the speakers, which are the default.
    static CONNECTED: Mutex<Vec<&str>> = Mutex::new(Vec::new());

    fn connected(device: &str) -> bool {
        CONNECTED.lock().unwrap().contains(&device)
    }

    /// The headphones while they're connected, each time anew, as
    /// PulseAudio numbers them; the default speakers otherwise.
    fn endpoint(device: Option<&str>, _: bool) -> String {
        match device {
            Some(device) if connected(device) => {
                format!("{device}#{}", CONNECTED.lock().unwrap().len())
            }
            _ => "speakers".into(),
        }
    }

    /// A stream opened on a device, or on the default speakers.
    fn open(device: Option<&str>) -> Result<&'static str> {
        match device {
            Some("headphones") if connected("headphones") => Ok("headphones"),
            Some(device) => bail!("{device} aren't connected"),
            None if connected("speakers") => Ok("speakers"),
            None => bail!("no speakers"),
        }
    }

    fn look_now(lasting: &mut Lasting<&'static str>) -> Option<&'static str> {
        lasting.look = Instant::now();
        lasting.stream().copied()
    }

    #[test]
    fn follows_headphones_away_and_back() {
        *CONNECTED.lock().unwrap() = vec!["speakers", "headphones"];
        let mut lasting =
            Lasting::open(Some("headphones"), true, "Test", Box::new(open), endpoint).unwrap();
        assert_eq!(lasting.stream().copied(), Some("headphones"));
        // They go away: the stream fails, and plays on the default meanwhile.
        CONNECTED
            .lock()
            .unwrap()
            .retain(|device| *device != "headphones");
        lasting.lost(&anyhow::anyhow!("gone"));
        assert_eq!(lasting.stream().copied(), Some("speakers"));
        // They come back, and the stream goes back to them.
        CONNECTED.lock().unwrap().push("headphones");
        assert_eq!(look_now(&mut lasting), Some("headphones"));
        // A server that moved the stream rather than failing it, as
        // PulseAudio does, numbers them anew when they return.
        CONNECTED.lock().unwrap().push("other");
        assert_eq!(look_now(&mut lasting), Some("headphones"), "opened again");
        assert_eq!(lasting.on, "headphones#3");
        // With nothing at all, there's no stream, and it tries again later.
        CONNECTED.lock().unwrap().clear();
        lasting.lost(&anyhow::anyhow!("gone"));
        assert_eq!(lasting.stream().copied(), None);
        assert!(lasting.look > Instant::now(), "waits before trying again");
    }

    #[test]
    fn silence_takes_as_long_as_the_samples_would() {
        assert_eq!(length(4_800, 2, 48_000), Duration::from_millis(50));
        assert_eq!(length(480, 1, 48_000), Duration::from_millis(10));
    }
}
