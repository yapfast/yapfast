//! Systems Yapfast has no sound backend for yet.

use std::time::Duration;

use anyhow::{Result, bail};

use super::Format;

const UNSUPPORTED: &str = "Yapfast has no sound on this system yet.";

pub struct Output;

impl Output {
    pub fn open(_: Option<&str>, _: &'static str, _: Format, _: Option<Duration>) -> Result<Self> {
        bail!(UNSUPPORTED)
    }

    pub fn write(&mut self, _: &[u8]) -> Result<()> {
        bail!(UNSUPPORTED)
    }

    pub fn drain(&mut self) -> Result<()> {
        bail!(UNSUPPORTED)
    }

    pub fn flush(&mut self) {}

    pub fn latency(&self) -> Duration {
        Duration::ZERO
    }
}

pub struct Input;

impl Input {
    pub fn open(_: Option<&str>, _: &'static str, _: Format, _: Duration) -> Result<Self> {
        bail!(UNSUPPORTED)
    }

    pub fn read(&mut self, _: &mut [u8]) -> Result<()> {
        bail!(UNSUPPORTED)
    }
}

#[cfg(feature = "discord")]
pub fn endpoint(_: Option<&str>, _: bool) -> String {
    String::new()
}

#[cfg(feature = "discord")]
pub fn others(
    _: fn(&str) -> bool,
    _: super::Hear,
    _: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<std::thread::JoinHandle<()>> {
    bail!("Yapfast shares sound on Linux only for now")
}
