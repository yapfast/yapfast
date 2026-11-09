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

use yapfast_plugin_api::{Frame, Payload, ProtocolError, ReadError, Request, Response};

/// The id of the unsolicited `ready` frame: the protocol reserves 0 for it.
const READY_ID: u64 = 0;

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
    /// A response could not be encoded or did not fit a frame. This is a bug on
    /// the helper's side, not a request to answer.
    Encode(ProtocolError),
    /// A frame's envelope is not the JSON its kind promises.
    ///
    /// The frame's end is known, so the helper could answer and read on; the
    /// protocol says not to. An envelope the two sides disagree about is a
    /// version or bug mismatch, and guessing past it hides the cause.
    Protocol(ProtocolError),
}

impl fmt::Display for ServeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(f, "reading a request: {error}"),
            Self::Write(error) => write!(f, "writing a response: {error}"),
            Self::Encode(error) => write!(f, "encoding a response: {error}"),
            Self::Protocol(error) => {
                write!(
                    f,
                    "a request is not the envelope its frame promises: {error}"
                )
            }
        }
    }
}

impl std::error::Error for ServeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            Self::Write(error) => Some(error),
            Self::Encode(error) => Some(error),
            Self::Protocol(error) => Some(error),
        }
    }
}

impl From<io::Error> for ServeError {
    fn from(error: io::Error) -> Self {
        Self::Write(error)
    }
}

impl From<ProtocolError> for ServeError {
    fn from(error: ProtocolError) -> Self {
        Self::Encode(error)
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
    respond(output, READY_ID, Response::Ready)?;

    loop {
        let frame = match Frame::read_from(input) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Ok(Outcome::EndOfStream),
            Err(error) => return Err(ServeError::Read(error)),
        };
        let id = frame.id();

        // A malformed wire or envelope ends the stream: both sides then exit
        // rather than guess where the next frame starts. A request the helper
        // understands but cannot serve (an unknown export, a module that is not
        // WebAssembly) is an `error` answer, and the helper keeps serving.
        let request = Request::from_frame(&frame).map_err(ServeError::Protocol)?;

        match request {
            Request::Load { plugin, module } => match backend.load(&plugin, &module) {
                Ok(()) => respond(output, id, Response::Loaded)?,
                Err(message) => respond(output, id, Response::Error { message })?,
            },
            Request::Call {
                plugin,
                method,
                payload,
            } => {
                // The call's JSON is the export's input, byte for byte: the
                // guest receives the payload the parent sent, not a re-encoding
                // of it.
                match backend.call(&plugin, &method, payload.as_bytes()) {
                    Ok(output_bytes) => respond(output, id, reply_payload(&method, output_bytes))?,
                    Err(message) => respond(output, id, Response::Error { message })?,
                }
            }
            Request::Unload { plugin } => match backend.unload(&plugin) {
                Ok(()) => {
                    respond(output, id, Response::Unloaded)?;
                    // The helper is disposable: with nothing left to serve it
                    // stops, and the parent spawns a fresh one when needed.
                    if backend.instances() == 0 {
                        return Ok(Outcome::Idle);
                    }
                }
                Err(message) => respond(output, id, Response::Error { message })?,
            },
            Request::Shutdown => return Ok(Outcome::Shutdown),
        }
    }
}

/// Writes one response frame, with the id of the request it answers.
fn respond(output: &mut impl Write, id: u64, response: Response) -> Result<(), ServeError> {
    response.to_frame(id)?.write_to(output)?;
    Ok(())
}

/// Parses a call's JSON answer, where an empty answer is the protocol's `null`.
fn reply_payload(method: &str, output: Vec<u8>) -> Response {
    if output.is_empty() {
        return Response::reply(Payload::null());
    }
    match Payload::from_bytes(&output) {
        Ok(payload) => Response::reply(payload),
        Err(error) => Response::Error {
            message: format!("{method} answered with JSON that could not be read: {error}"),
        },
    }
}
