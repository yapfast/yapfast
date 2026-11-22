//! The bytes of a file the `attachment_pre_upload` hook is about.
//!
//! Inside the hook a plugin may [`read`] the file the person picked, by the
//! `upload` its subject names. One that needs longer answers
//! [`HookOutcome::hold`](crate::hook), and the file is its own until it calls
//! [`finish`]: it can [`write`] a file to go in its place, or send the file to
//! a host of its own with [`HttpRequest::with_upload`](crate::http), which
//! Yapfast streams from disk. `uploads.intercept`.

use serde_json::json;

use crate::Result;
use crate::call::{self, method};
use crate::wire::{self, UploadPiece};

/// The most one [`read`] answers.
pub const MAX_PIECE: u32 = 512 * 1024;

/// What goes with the message once the plugin is done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Send {
    /// The person's file.
    Original,
    /// The file written with [`write`].
    Replacement,
    /// Nothing.
    Nothing,
}

/// One piece of the person's file, from `offset`, at most [`MAX_PIECE`]
/// bytes: the bytes, the whole file's size, and whether this piece is the
/// last.
pub fn read(upload: u64, offset: u64, length: u32) -> Result<(Vec<u8>, u64, bool)> {
    let value = call::ok(
        method::UPLOADS_READ,
        json!({ "upload": upload, "offset": offset, "length": length.min(MAX_PIECE) }),
    )?;
    let piece: UploadPiece = call::parse(method::UPLOADS_READ, value)?;
    let bytes = wire::base64::decode(&piece.data_base64)
        .ok_or_else(|| crate::Error::msg("`uploads.read` answered bytes that aren't base64"))?;
    Ok((bytes, piece.bytes, piece.eof))
}

/// The whole file, a piece at a time: for a file small enough to hold.
pub fn read_all(upload: u64) -> Result<Vec<u8>> {
    let mut all = Vec::new();
    loop {
        let (piece, _, eof) = read(upload, all.len() as u64, MAX_PIECE)?;
        all.extend_from_slice(&piece);
        if eof || piece.is_empty() {
            return Ok(all);
        }
    }
}

/// The next piece of the file that goes in place of the person's, at
/// `offset`, where the last piece ended. Answers the bytes written so far.
pub fn write(upload: u64, offset: u64, bytes: &[u8]) -> Result<u64> {
    let value = call::ok(
        method::UPLOADS_WRITE,
        json!({ "upload": upload, "offset": offset, "data_base64": wire::base64::encode(bytes) }),
    )?;
    Ok(value["written"].as_u64().unwrap_or_default())
}

/// Ends the hold: what goes, under which name, and text for the composer.
pub fn finish(upload: u64, send: Send, name: Option<&str>, insert: Option<&str>) -> Result<()> {
    let send = match send {
        Send::Original => "original",
        Send::Replacement => "replacement",
        Send::Nothing => "nothing",
    };
    call::ok(
        method::UPLOADS_FINISH,
        json!({ "upload": upload, "send": send, "name": name, "insert": insert }),
    )?;
    Ok(())
}
