//! The input level meter. It reads the chosen microphone on this machine, raw
//! and before any processing, and never involves a call: it works while muted,
//! outside calls and in the preview.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};

use crate::audio_io::Input;

/// Measures one microphone until dropped.
pub struct InputMeter {
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    /// 0 to 1 over the last 60 dB, as f32 bits.
    level: AtomicU32,
    stop: AtomicBool,
    failed: AtomicBool,
}

/// Whether this platform can show the level.
pub const AVAILABLE: bool = crate::audio_io::AVAILABLE;

impl InputMeter {
    /// Starts measuring `microphone`, a device id from [`crate::voice::State`],
    /// or the system default for `None`.
    /// Tests (`test-fixtures`) open no device: the level stays at 0.
    pub fn start(microphone: Option<&str>) -> Self {
        let shared = Arc::new(Shared::default());
        if cfg!(feature = "test-fixtures") {
            return Self { shared };
        }
        let microphone = microphone.map(str::to_owned);
        let worker = shared.clone();
        // Opening and reading the device block, so the meter has its own
        // thread. It is left to finish its last read after a drop.
        let spawned = std::thread::Builder::new()
            .name("yapfast-meter".into())
            .spawn(move || {
                if let Err(error) = measure(microphone.as_deref(), &worker) {
                    tracing::warn!(%error, "could not measure the microphone");
                    worker.failed.store(true, Ordering::Relaxed);
                }
            });
        if spawned.is_err() {
            shared.failed.store(true, Ordering::Relaxed);
        }
        Self { shared }
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.shared.level.load(Ordering::Relaxed))
    }

    /// The microphone could not be opened or stopped delivering sound.
    pub fn failed(&self) -> bool {
        self.shared.failed.load(Ordering::Relaxed)
    }
}

impl Drop for InputMeter {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

/// 16 kHz mono is plenty for a level, and 20 ms reads keep it lively.
const RATE: u32 = 16_000;
const CHUNK: usize = RATE as usize / 50;

fn measure(microphone: Option<&str>, shared: &Shared) -> anyhow::Result<()> {
    use anyhow::Context;
    let mut input = Input::<i16>::open(
        microphone,
        "Input level",
        RATE,
        1,
        Duration::from_millis(20),
    )
    .context("could not open the microphone")?;
    let mut chunk = [0i16; CHUNK];
    let mut shown = 0.0f32;
    while !shared.stop.load(Ordering::Relaxed) {
        input.read(&mut chunk).context("the microphone stopped")?;
        let power = chunk
            .iter()
            .map(|sample| f32::from(*sample) / 32768.0)
            .map(|sample| sample * sample)
            .sum::<f32>()
            / CHUNK as f32;
        // Rise at once and fall gently, so speech reads as a steady bar.
        shown = meter_level(power.sqrt()).max(shown - 0.03);
        shared.level.store(shown.to_bits(), Ordering::Relaxed);
    }
    Ok(())
}

/// Maps an RMS amplitude to the meter's 0 to 1 over the last 60 dB.
fn meter_level(rms: f32) -> f32 {
    let decibels = 20.0 * rms.max(1e-6).log10();
    ((decibels + 60.0) / 60.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_span_sixty_decibels() {
        assert_eq!(meter_level(0.0), 0.0);
        assert_eq!(meter_level(0.001), 0.0);
        assert!((meter_level(0.0316) - 0.5).abs() < 0.01);
        assert_eq!(meter_level(1.0), 1.0);
    }
}
