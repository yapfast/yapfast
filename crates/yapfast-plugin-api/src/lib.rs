//! The wire protocol between Yapfast and the plugin helper process.
//!
//! M0 carries one request or response per frame over stdin and stdout. A frame
//! is a fixed 13-byte header and a JSON payload:
//!
//! ```text
//! u32 payload length (little endian)
//! u8  kind            (the discriminants in [`Kind`])
//! u64 id              (little endian; pairs a response with its request)
//! ..  payload         (the JSON of a [`Request`] or [`Response`])
//! ```
//!
//! The length counts payload bytes only. A whole frame is at most
//! [`MAX_FRAME_LEN`], so a payload is at most [`MAX_PAYLOAD_LEN`]. See
//! `docs/plugin-protocol.md`.

use std::fmt;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The bytes a frame's fixed header takes: the length, kind and id.
pub const HEADER_LEN: usize = 4 + 1 + 8;

/// The largest whole frame, header and payload, in bytes.
pub const MAX_FRAME_LEN: usize = 1024 * 1024;

/// The largest payload a frame may carry: [`MAX_FRAME_LEN`] less the header.
pub const MAX_PAYLOAD_LEN: usize = MAX_FRAME_LEN - HEADER_LEN;

/// What a frame carries.
///
/// The discriminants are the wire format and must not change: a helper and a
/// parent built from different versions still agree on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Kind {
    /// The helper has started and takes requests.
    Ready = 1,
    /// The parent asks the helper to instantiate a module.
    Load = 2,
    /// The helper has instantiated a module.
    Loaded = 3,
    /// The parent calls a method on a loaded plugin.
    Call = 4,
    /// The helper answers a call.
    Reply = 5,
    /// The parent asks the helper to drop an instance.
    Unload = 6,
    /// The helper has dropped an instance.
    Unloaded = 7,
    /// The parent asks the helper to exit.
    Shutdown = 8,
    /// Either side reports a request it could not serve.
    Error = 9,
}

impl Kind {
    /// This kind's byte on the wire.
    pub const fn as_u8(self) -> u8 {
        todo!()
    }

    /// The kind a wire byte names, or `None` if it names none.
    pub const fn from_u8(byte: u8) -> Option<Self> {
        todo!()
    }
}

/// One frame: a kind, the id pairing it with its counterpart, and its payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    kind: Kind,
    id: u64,
    payload: Vec<u8>,
}

impl Frame {
    /// Builds a frame, refusing a payload that would make the frame too large.
    pub fn new(kind: Kind, id: u64, payload: Vec<u8>) -> Result<Self, FrameError> {
        todo!()
    }

    /// The kind of frame.
    pub fn kind(&self) -> Kind {
        todo!()
    }

    /// The id pairing this frame with its counterpart.
    pub fn id(&self) -> u64 {
        todo!()
    }

    /// The payload bytes, the JSON of a request or response.
    pub fn payload(&self) -> &[u8] {
        todo!()
    }

    /// Writes the frame and flushes the writer.
    ///
    /// A pipe has no autoflush; without the flush the parent would wait for a
    /// frame the helper has written but not sent.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        todo!()
    }

    /// Reads one frame, or `None` at a clean end of stream.
    ///
    /// End of stream before any header byte is a clean stop. End of stream
    /// partway through a header or payload is [`ReadError::UnexpectedEof`]: the
    /// peer died mid-frame, and the stream cannot be resynchronised.
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Option<Frame>, ReadError> {
        todo!()
    }
}

/// Why a frame could not be built.
#[derive(Debug)]
pub enum FrameError {
    /// The payload is larger than [`MAX_PAYLOAD_LEN`].
    TooLarge {
        /// The offered payload length.
        len: usize,
    },
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { len } => {
                write!(f, "payload of {len} bytes exceeds the limit of {MAX_PAYLOAD_LEN}")
            }
        }
    }
}

impl std::error::Error for FrameError {}

/// Why a frame could not be read.
#[derive(Debug)]
pub enum ReadError {
    /// The reader failed for a reason unrelated to the frame's shape.
    Io(io::Error),
    /// The length field names a payload larger than [`MAX_PAYLOAD_LEN`].
    ///
    /// The frame is refused from its header alone, before the payload buffer is
    /// allocated.
    FrameTooLarge {
        /// The length the header declared.
        len: u32,
    },
    /// The kind byte names no [`Kind`].
    UnknownKind(u8),
    /// The stream ended partway through a header or payload.
    UnexpectedEof {
        /// The bytes the part that ended needed.
        expected: usize,
        /// The bytes it delivered.
        got: usize,
    },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "reading a frame: {error}"),
            Self::FrameTooLarge { len } => write!(
                f,
                "frame of {len} payload bytes exceeds the limit of {MAX_PAYLOAD_LEN}"
            ),
            Self::UnknownKind(byte) => write!(f, "frame kind {byte} is not one this protocol defines"),
            Self::UnexpectedEof { expected, got } => {
                write!(f, "stream ended after {got} of {expected} bytes")
            }
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ReadError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// A request from the parent to the helper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Instantiate the module at an absolute path as `plugin`.
    Load {
        /// The plugin's id, as its manifest gives it.
        plugin: String,
        /// The module's absolute path.
        module: PathBuf,
    },
    /// Call `method` on a loaded plugin with a JSON payload.
    Call {
        /// The plugin's id.
        plugin: String,
        /// The method's name, as the plugin's payload names it.
        method: String,
        /// The call's JSON, passed to the plugin verbatim.
        #[serde(default)]
        payload: Value,
    },
    /// Drop a plugin's instance.
    Unload {
        /// The plugin's id.
        plugin: String,
    },
    /// Stop the helper.
    Shutdown,
}

impl Request {
    /// The frame kind this request travels as.
    pub fn kind(&self) -> Kind {
        todo!()
    }

    /// Checks the request's invariants, such as an absolute module path.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        todo!()
    }

    /// The request as the JSON a frame carries.
    pub fn to_value(&self) -> Result<Value, ProtocolError> {
        todo!()
    }

    /// Parses and checks a request from the JSON a frame carried.
    pub fn from_value(value: Value) -> Result<Self, ProtocolError> {
        todo!()
    }

    /// Wraps the request in a frame with `id`.
    pub fn to_frame(&self, id: u64) -> Result<Frame, ProtocolError> {
        todo!()
    }

    /// Parses a request from a frame, refusing a frame that is not a request.
    pub fn from_frame(frame: &Frame) -> Result<Self, ProtocolError> {
        todo!()
    }
}

/// A response from the helper to the parent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// The helper is up.
    Ready,
    /// The module is instantiated.
    Loaded,
    /// The answer to a [`Request::Call`]; `payload` is the plugin's JSON.
    Reply {
        /// The method's JSON answer.
        #[serde(default)]
        payload: Value,
    },
    /// The instance is gone.
    Unloaded,
    /// The helper could not serve a request; `message` says why.
    Error {
        /// A sentence for the parent's log.
        message: String,
    },
}

impl Response {
    /// The frame kind this response travels as.
    pub fn kind(&self) -> Kind {
        todo!()
    }

    /// The response as the JSON a frame carries.
    pub fn to_value(&self) -> Result<Value, ProtocolError> {
        todo!()
    }

    /// Parses a response from the JSON a frame carried.
    pub fn from_value(value: Value) -> Result<Self, ProtocolError> {
        todo!()
    }

    /// Wraps the response in a frame with `id`.
    pub fn to_frame(&self, id: u64) -> Result<Frame, ProtocolError> {
        todo!()
    }

    /// Parses a response from a frame, refusing a frame that is not a response.
    pub fn from_frame(frame: &Frame) -> Result<Self, ProtocolError> {
        todo!()
    }
}

/// Why a request or response could not be encoded, decoded or framed.
#[derive(Debug)]
pub enum ProtocolError {
    /// The payload is not the JSON its frame kind promises.
    Json(serde_json::Error),
    /// The frame cannot hold the response: it is too large.
    Frame(FrameError),
    /// A [`Request::Load`] named a module by a relative path.
    RelativeModulePath(PathBuf),
    /// The frame's kind is not a request kind.
    NotRequest(Kind),
    /// The frame's kind is not a response kind.
    NotResponse(Kind),
    /// The frame's kind and the JSON's `type` disagree.
    KindMismatch {
        /// The kind the frame header named.
        kind: Kind,
        /// The kind the JSON's `type` named.
        payload: Kind,
    },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(f, "payload is not the promised JSON: {error}"),
            Self::Frame(error) => error.fmt(f),
            Self::RelativeModulePath(path) => {
                write!(f, "module path {} is not absolute", path.display())
            }
            Self::NotRequest(kind) => write!(f, "a {kind:?} frame is not a request"),
            Self::NotResponse(kind) => write!(f, "a {kind:?} frame is not a response"),
            Self::KindMismatch { kind, payload } => {
                write!(f, "frame says {kind:?} but its payload says {payload:?}")
            }
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Frame(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for ProtocolError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<FrameError> for ProtocolError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}
