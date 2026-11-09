//! The helper's loop over the framed protocol.
//!
//! One request per frame, one response per request, and an unsolicited
//! [`Response::Ready`] before the first. The loop knows nothing about
//! WebAssembly: it asks a [`Backend`] to load, call and unload, and a test can
//! answer instead of Extism.
//!
//! Three things end the loop:
//!
//! - a [`Request::Shutdown`], which has no answer: the helper closes the stream
//!   and exits;
//! - a clean end of stream, as when the parent closes stdin;
//! - the unload of the last instance. The helper is disposable, so it stops as
//!   soon as it holds nothing, and the parent spawns a fresh one on the next
//!   trigger. That answer is written and flushed before the loop ends.

use std::fmt;
use std::io::{self, Read, Write};
use std::path::Path;

use yapfast_plugin_api::{Frame, ReadError, Request, Response};

/// Why the helper stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The parent sent [`Request::Shutdown`].
    Shutdown,
    /// The last instance was unloaded, so the helper has nothing left to serve.
    Idle,
    /// The parent closed the stream without asking the helper to stop.
    EndOfStream,
}

/// What the loop needs from an engine.
///
/// Failures are sentences for the parent, since the protocol's [`Response::Error`]
/// carries one: the parent logs it and may show it to the user.
pub trait Backend {
    /// Instantiates the module at `module` as `plugin`.
    fn load(&mut self, plugin: &str, module: &Path) -> Result<(), String>;

    /// Calls the export `method` on a loaded plugin, with `payload` as its
    /// input bytes, and returns the export's output bytes.
    fn call(&mut self, plugin: &str, method: &str, payload: &[u8]) -> Result<Vec<u8>, String>;

    /// Drops a plugin's instance.
    fn unload(&mut self, plugin: &str) -> Result<(), String>;

    /// How many instances the helper holds.
    fn instances(&self) -> usize;
}

/// Why the loop stopped early.
#[derive(Debug)]
pub enum ServeError {
    /// A frame could not be read. A frame whose end is unknown leaves the
    /// stream unusable, so this is fatal; a status of 0 from `read` is not an
    /// error, it is [`Outcome::EndOfStream`].
    Read(ReadError),
    /// A frame could not be written, as when the parent has gone.
    Write(io::Error),
}

impl fmt::Display for ServeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl std::error::Error for ServeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        todo!()
    }
}

impl From<io::Error> for ServeError {
    fn from(error: io::Error) -> Self {
        todo!()
    }
}

/// Serves requests from `input` and writes responses to `output` until
/// something ends the loop.
///
/// Every response is written and flushed before the next request is read, so a
/// parent waiting on a pipe always sees the answer. Nothing else is written to
/// `output`: a diagnostic there would corrupt the stream.
pub fn serve(
    input: &mut impl Read,
    output: &mut impl Write,
    backend: &mut impl Backend,
) -> Result<Outcome, ServeError> {
    let _ = (input, output, backend);
    todo!()
}

/// Writes one response frame, with the id of the request it answers.
fn respond(output: &mut impl Write, id: u64, response: Response) -> Result<(), ServeError> {
    let _ = (output, id, response);
    todo!()
}

/// Parses a call's JSON answer, where an empty answer is the protocol's `null`.
fn reply_payload(method: &str, output: Vec<u8>) -> Response {
    let _ = (method, output);
    todo!()
}
