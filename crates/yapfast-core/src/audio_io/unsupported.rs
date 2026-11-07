//! Systems Yapfast has no sound backend for yet.

use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use anyhow::{Result, bail};

use super::{Format, Waited};

const UNSUPPORTED: &str = "Yapfast has no sound on this system yet.";

pub struct Output;

impl Output {
    pub fn open(_: Option<&str>, _: &'static str, _: Format, _: Option<Duration>) -> Result<Self> {
        bail!(UNSUPPORTED)
    }

    pub fn write(&mut self, _: &[u8]) -> Result<Waited> {
        bail!(UNSUPPORTED)
    }

    pub fn drain(&mut self) -> Result<Waited> {
        bail!(UNSUPPORTED)
    }

    /// Nothing here waits for a device, so there's no wait to end.
    pub fn stop_with(&mut self, _: Arc<AtomicBool>) {}

    pub fn flush(&mut self) {}

    pub fn latency(&self) -> Duration {
        Duration::ZERO
    }

    /// Nothing is played on a build without sound, so it cannot say where it is.
    pub fn queued(&self) -> Option<u64> {
        None
    }
}

pub struct Input;

impl Input {
    pub fn open(_: Option<&str>, _: &'static str, _: Format, _: Duration) -> Result<Self> {
        bail!(UNSUPPORTED)
    }

    pub fn read(&mut self, _: &mut [u8]) -> Result<Waited> {
        bail!(UNSUPPORTED)
    }

    /// Nothing here waits for a device, so there's no wait to end.
    pub fn stop_with(&mut self, _: Arc<AtomicBool>) {}
}

#[cfg(feature = "discord")]
pub fn endpoint(_: Option<&str>, _: bool) -> Option<String> {
    None
}

#[cfg(feature = "discord")]
pub fn others(
    _: fn(&str) -> bool,
    _: super::Hear,
    _: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<std::thread::JoinHandle<()>> {
    bail!("Yapfast shares sound on Linux only for now")
}
