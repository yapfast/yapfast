//! Recording the microphone, and sending a recording as a voice message:
//! `microphone`.
//!
//! [`start`] is answered when the recording ends, as a [`Pending`]: stopped by
//! [`stop`], by the person from the red bar Yapfast shows while it records, or
//! by its length. The answer is `{ recording, format, duration_ms, bytes,
//! waveform, stopped_by }`; [`read`] reads the file and [`send_voice`] sends an
//! Opus one as a Discord voice message, with its waveform. The person is asked
//! the first time.

use serde_json::json;

use crate::call::{self, HostReturn, Pending, method};
use crate::{Error, Result};

/// What a recording is written as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Ogg Opus, 48 kHz mono, as a voice message is.
    Opus,
    /// A WAV of 16-bit 48 kHz mono samples.
    Pcm,
}

fn later(method: &str, payload: serde_json::Value) -> Result<Pending> {
    match call::send(method, payload)? {
        HostReturn::Pending { tag } => Ok(Pending::from_tag(tag)),
        other => call::value(method, other).and_then(|value| {
            Err(Error::msg(format!(
                "`{method}` answered at once, with {value}, where an answer later was expected"
            )))
        }),
    }
}

/// Starts recording; `max_seconds` stops it by itself.
pub fn start(format: Format, max_seconds: Option<u32>) -> Result<Pending> {
    let format = match format {
        Format::Opus => "opus",
        Format::Pcm => "pcm",
    };
    later(
        method::MICROPHONE_START,
        json!({ "format": format, "max_seconds": max_seconds }),
    )
}

/// Stops this plugin's recording; [`start`]'s answer follows.
pub fn stop() -> Result<()> {
    call::ok(method::MICROPHONE_STOP, serde_json::Value::Null)?;
    Ok(())
}

/// A piece of a recording, from `offset`: the bytes, its size, and whether
/// this piece is its end.
pub fn read(recording: u64, offset: u64) -> Result<(Vec<u8>, u64, bool)> {
    let value = call::ok(
        method::MICROPHONE_READ,
        json!({ "body": recording, "offset": offset }),
    )?;
    let piece: crate::wire::UploadPiece = call::parse(method::MICROPHONE_READ, value)?;
    let bytes = crate::wire::base64::decode(&piece.data_base64)
        .ok_or_else(|| Error::msg("`microphone.read` answered bytes that aren't base64"))?;
    Ok((bytes, piece.bytes, piece.eof))
}

/// Sends an Opus recording in `room` as a voice message, under the send's own
/// rules: answered later, once Discord took it.
pub fn send_voice(room: &str, recording: u64) -> Result<Pending> {
    later(
        method::MESSAGES_VOICE,
        json!({ "room": room, "recording": recording }),
    )
}
