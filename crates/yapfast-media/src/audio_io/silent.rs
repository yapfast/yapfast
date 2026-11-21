//! Stand-ins for the speakers and the microphone while [`super::silence_audio`]
//! holds: what's written is dropped and what's read is silence, each at the
//! pace a device would take or give it, so code waiting on a stream waits as
//! long as it would. Tests run on them, so a test is never heard and never
//! hears the room.

use std::time::{Duration, Instant};

use super::Format;

/// How long `bytes` of `format` play.
fn length(bytes: usize, format: Format) -> Duration {
    let frames = bytes / format.frame_bytes().max(1);
    Duration::from_secs_f64(frames as f64 / f64::from(format.rate.max(1)))
}

/// Speakers that play nothing.
pub struct Output {
    format: Format,
    /// How much may wait to play before a write waits.
    queue: Duration,
    /// When what's been written so far has played.
    played: Instant,
}

impl Output {
    pub fn open(format: Format, queue: Option<Duration>) -> Self {
        Self {
            format,
            queue: queue.unwrap_or(Duration::from_millis(40)),
            played: Instant::now(),
        }
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let now = Instant::now();
        self.played = self.played.max(now) + length(bytes.len(), self.format);
        let waiting = self.played - now;
        if waiting > self.queue {
            std::thread::sleep(waiting - self.queue);
        }
    }

    pub fn drain(&self) {
        std::thread::sleep(self.latency());
    }

    pub fn flush(&mut self) {
        self.played = Instant::now();
    }

    pub fn latency(&self) -> Duration {
        self.played.saturating_duration_since(Instant::now())
    }
}

/// A microphone that hears nothing.
pub struct Input {
    format: Format,
    /// When the next read's samples have been heard.
    heard: Instant,
}

impl Input {
    pub fn open(format: Format) -> Self {
        Self {
            format,
            heard: Instant::now(),
        }
    }

    pub fn read(&mut self, bytes: &mut [u8]) {
        bytes.fill(0);
        let now = Instant::now();
        self.heard = self.heard.max(now) + length(bytes.len(), self.format);
        std::thread::sleep(self.heard - now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_wait_as_a_device_would_and_reads_give_silence() {
        let format = Format {
            rate: 1000,
            channels: 1,
            float: false,
        };
        let mut output = Output::open(format, Some(Duration::from_millis(10)));
        let started = Instant::now();
        // 50 ms of samples, of which 10 ms may wait.
        output.write(&[0; 100]);
        assert!(started.elapsed() >= Duration::from_millis(35));
        assert!(output.latency() <= Duration::from_millis(10));
        output.drain();
        assert!(started.elapsed() >= Duration::from_millis(50));
        let mut input = Input::open(format);
        let mut heard = [1u8; 40];
        let started = Instant::now();
        input.read(&mut heard);
        assert!(started.elapsed() >= Duration::from_millis(18));
        assert_eq!(heard, [0; 40]);
    }
}
