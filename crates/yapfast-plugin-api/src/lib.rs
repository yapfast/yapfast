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
//! [`MAX_FRAME_LEN`], so a payload is at most [`MAX_PAYLOAD_LEN`]. Frame id 0
//! belongs to the unsolicited [`Kind::Ready`] alone; every request and every
//! other response carries a non-zero id.
//!
//! A call's argument JSON and a reply's answer JSON are forwarded verbatim: a
//! payload decoded from a frame keeps the exact bytes it arrived as, which is
//! what [`Payload`] is for.
//!
//! See `docs/plugin-protocol.md`.

use std::fmt;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use serde_json::value::RawValue;

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
    ///
    /// Its frame uses id 0. It answers no request, so it has no id to copy.
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
        self as u8
    }

    /// The kind a wire byte names, or `None` if it names none.
    pub const fn from_u8(byte: u8) -> Option<Self> {
        Some(match byte {
            1 => Self::Ready,
            2 => Self::Load,
            3 => Self::Loaded,
            4 => Self::Call,
            5 => Self::Reply,
            6 => Self::Unload,
            7 => Self::Unloaded,
            8 => Self::Shutdown,
            9 => Self::Error,
            _ => return None,
        })
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
        if payload.len() > MAX_PAYLOAD_LEN {
            return Err(FrameError::TooLarge { len: payload.len() });
        }
        Ok(Self { kind, id, payload })
    }

    /// The kind of frame.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The id pairing this frame with its counterpart.
    ///
    /// 0 is reserved for the unsolicited [`Kind::Ready`]; a request or any
    /// other response must use a non-zero id.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The payload bytes, the JSON of a request or response.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Writes the frame and flushes the writer.
    ///
    /// A pipe has no autoflush; without the flush the parent would wait for a
    /// frame the helper has written but not sent.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        // Checked by `new`, so the length always fits the field.
        let len = u32::try_from(self.payload.len()).expect("a frame's payload fits a u32");
        let mut header = [0u8; HEADER_LEN];
        header[..4].copy_from_slice(&len.to_le_bytes());
        header[4] = self.kind.as_u8();
        header[5..].copy_from_slice(&self.id.to_le_bytes());

        writer.write_all(&header)?;
        writer.write_all(&self.payload)?;
        writer.flush()
    }

    /// Reads one frame, or `None` at a clean end of stream.
    ///
    /// End of stream before any header byte is a clean stop. End of stream
    /// partway through a header or payload is [`ReadError::UnexpectedEof`]: the
    /// peer died mid-frame, and the stream cannot be resynchronised.
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Option<Frame>, ReadError> {
        let mut header = [0u8; HEADER_LEN];
        if !read_exact(reader, &mut header, true)? {
            return Ok(None);
        }

        let len = u32::from_le_bytes(header[..4].try_into().expect("four bytes"));
        // Refuse an oversize frame from its header, before its payload buffer
        // exists: a peer must not be able to make us allocate what we would
        // then refuse to read.
        if len as usize > MAX_PAYLOAD_LEN {
            return Err(ReadError::FrameTooLarge { len });
        }
        let kind = Kind::from_u8(header[4]).ok_or(ReadError::UnknownKind(header[4]))?;
        let id = u64::from_le_bytes(header[5..].try_into().expect("eight bytes"));

        let mut payload = vec![0u8; len as usize];
        read_exact(reader, &mut payload, false)?;
        Ok(Some(Frame { kind, id, payload }))
    }
}

/// Fills `buf` from `reader`, reporting whether a clean end of stream came
/// before its first byte.
fn read_exact<R: Read>(
    reader: &mut R,
    buf: &mut [u8],
    eof_at_start_is_clean: bool,
) -> Result<bool, ReadError> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) if filled == 0 && eof_at_start_is_clean => return Ok(false),
            Ok(0) => {
                return Err(ReadError::UnexpectedEof {
                    expected: buf.len(),
                    got: filled,
                });
            }
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(ReadError::Io(error)),
        }
    }
    Ok(true)
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
                write!(
                    f,
                    "payload of {len} bytes exceeds the limit of {MAX_PAYLOAD_LEN}"
                )
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
    ///
    /// Fatal by policy in this M0 protocol: a peer that sends a kind this
    /// version does not know speaks a version whose frames cannot be trusted,
    /// so the receiver reports this and closes the stream rather than skipping
    /// a frame it would misinterpret. The length is known; the meaning is not.
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
            Self::UnknownKind(byte) => {
                write!(f, "frame kind {byte} is not one this protocol defines")
            }
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

/// The JSON a [`Request::Call`] passes to a guest export, or a
/// [`Response::Reply`] hands back.
///
/// A payload decoded from a frame keeps the exact bytes it arrived as, so
/// decoding and forwarding it cannot reorder keys, drop whitespace or rewrite a
/// number: key order, exponent forms and integers past `u64` all survive.
///
/// [`Payload::from`] a [`Value`] is the convenient construction, and carries no
/// byte fidelity: a `Value` has no source bytes, so its payload is that
/// `Value`'s own serialisation.
#[derive(Debug, Clone, Default)]
pub struct Payload(Box<RawValue>);

impl Payload {
    /// Parses a payload from JSON text, as a frame carries it.
    pub fn new(json: &str) -> Result<Self, ProtocolError> {
        Self::from_bytes(json.as_bytes())
    }

    /// Parses a payload from JSON bytes, as a guest export answered them.
    pub fn from_bytes(json: &[u8]) -> Result<Self, ProtocolError> {
        Ok(Self(serde_json::from_slice(json)?))
    }

    /// JSON `null`: the value an omitted payload reads as.
    pub fn null() -> Self {
        Self::default()
    }

    /// The payload's JSON text, exactly as it was read or built.
    pub fn as_str(&self) -> &str {
        self.0.get()
    }

    /// The payload's bytes, exactly as they cross the boundary.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.get().as_bytes()
    }

    /// The payload as a [`Value`], which loses the original bytes.
    pub fn to_value(&self) -> Result<Value, ProtocolError> {
        Ok(serde_json::from_str(self.0.get())?)
    }
}

impl From<Value> for Payload {
    fn from(value: Value) -> Self {
        let raw = serde_json::value::to_raw_value(&value).expect("a Value is JSON");
        Self(raw)
    }
}

impl PartialEq for Payload {
    /// Equal means the same bytes, not merely the same JSON meaning: the
    /// protocol forwards text, so two spellings of one value are two payloads.
    fn eq(&self, other: &Self) -> bool {
        self.0.get() == other.0.get()
    }
}

impl Eq for Payload {}

impl fmt::Display for Payload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.get())
    }
}

impl Serialize for Payload {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Payload {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Box::<RawValue>::deserialize(deserializer).map(Self)
    }
}

/// A request from the parent to the helper.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
        /// The guest export to call, for M0.
        ///
        /// This is the module's export name, and `payload` is that export's
        /// argument bytes. It is not the guest-side `yf_call` method envelope
        /// of later milestones: M0 has no method envelope, so one call is one
        /// export call.
        method: String,
        /// The call's JSON, passed to the export verbatim.
        payload: Payload,
    },
    /// Drop a plugin's instance.
    Unload {
        /// The plugin's id.
        plugin: String,
    },
    /// Stop the helper.
    ///
    /// There is no success answer: the helper closes the stream and exits. Its
    /// envelope is the JSON `{"type": "shutdown"}`; a bare frame is not one.
    Shutdown,
}

impl Request {
    /// Builds a [`Request::Call`], converting a [`Value`] payload if that is
    /// what the caller has.
    pub fn call(
        plugin: impl Into<String>,
        method: impl Into<String>,
        payload: impl Into<Payload>,
    ) -> Self {
        Self::Call {
            plugin: plugin.into(),
            method: method.into(),
            payload: payload.into(),
        }
    }

    /// The frame kind this request travels as.
    pub fn kind(&self) -> Kind {
        match self {
            Self::Load { .. } => Kind::Load,
            Self::Call { .. } => Kind::Call,
            Self::Unload { .. } => Kind::Unload,
            Self::Shutdown => Kind::Shutdown,
        }
    }

    /// Checks the request's invariants, such as an absolute module path.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Load { module, .. } if !module.is_absolute() => {
                Err(ProtocolError::RelativeModulePath(module.clone()))
            }
            _ => Ok(()),
        }
    }

    /// The request as the JSON a frame carries.
    ///
    /// Going through a [`Value`] loses a decoded payload's original bytes; use
    /// [`Request::to_frame`] to forward a payload verbatim.
    pub fn to_value(&self) -> Result<Value, ProtocolError> {
        self.validate()?;
        Ok(serde_json::to_value(self)?)
    }

    /// Parses and checks a request from a [`Value`].
    ///
    /// A `Value` has no source bytes, so the payload's bytes are the `Value`'s
    /// own serialisation; [`Request::from_frame`] is what keeps the bytes a
    /// frame carried.
    pub fn from_value(value: Value) -> Result<Self, ProtocolError> {
        let request: Self = serde_json::from_slice(&serde_json::to_vec(&value)?)?;
        request.validate()?;
        Ok(request)
    }

    /// Wraps the request in a frame with `id`, which may not be 0.
    pub fn to_frame(&self, id: u64) -> Result<Frame, ProtocolError> {
        self.validate()?;
        check_id(self.kind(), id)?;
        let payload = serde_json::to_vec(self)?;
        Ok(Frame::new(self.kind(), id, payload)?)
    }

    /// Parses a request from a frame, refusing a frame that is not a request.
    pub fn from_frame(frame: &Frame) -> Result<Self, ProtocolError> {
        match frame.kind() {
            Kind::Load | Kind::Call | Kind::Unload | Kind::Shutdown => {}
            other => return Err(ProtocolError::NotRequest(other)),
        }
        check_id(frame.kind(), frame.id())?;
        let request: Self = serde_json::from_slice(frame.payload())?;
        request.validate()?;
        if request.kind() != frame.kind() {
            return Err(ProtocolError::KindMismatch {
                kind: frame.kind(),
                payload: request.kind(),
            });
        }
        Ok(request)
    }
}

/// Refuses a frame id the protocol reserves, and a `ready` that is not alone at
/// id 0.
fn check_id(kind: Kind, id: u64) -> Result<(), ProtocolError> {
    match (kind, id) {
        (Kind::Ready, 0) => Ok(()),
        (Kind::Ready, id) => Err(ProtocolError::ReadyWithId(id)),
        (kind, 0) => Err(ProtocolError::ReservedId(kind)),
        _ => Ok(()),
    }
}

/// A response from the helper to the parent.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// The helper is up.
    ///
    /// Unsolicited, so its frame uses id 0 and no other frame may.
    Ready,
    /// The module is compiled and linked; Extism instantiates it on its first call.
    Loaded,
    /// The answer to a [`Request::Call`]; `payload` is the plugin's JSON.
    Reply {
        /// The export's JSON answer, forwarded verbatim.
        payload: Payload,
    },
    /// The instance is gone.
    Unloaded,
    /// A request that could not be served; `message` says why.
    ///
    /// In M0 this envelope travels from the helper to the parent. It is never
    /// a [`Request`], and its id pairs it with the exchange that failed.
    Error {
        /// A sentence for the other side's log.
        message: String,
    },
}

impl Response {
    /// Builds a [`Response::Reply`], converting a [`Value`] payload if that is
    /// what the caller has.
    pub fn reply(payload: impl Into<Payload>) -> Self {
        Self::Reply {
            payload: payload.into(),
        }
    }

    /// The frame kind this response travels as.
    pub fn kind(&self) -> Kind {
        match self {
            Self::Ready => Kind::Ready,
            Self::Loaded => Kind::Loaded,
            Self::Reply { .. } => Kind::Reply,
            Self::Unloaded => Kind::Unloaded,
            Self::Error { .. } => Kind::Error,
        }
    }

    /// The response as the JSON a frame carries.
    ///
    /// Going through a [`Value`] loses a decoded payload's original bytes; use
    /// [`Response::to_frame`] to forward a payload verbatim.
    pub fn to_value(&self) -> Result<Value, ProtocolError> {
        Ok(serde_json::to_value(self)?)
    }

    /// Parses a response from a [`Value`].
    ///
    /// A `Value` has no source bytes, so a payload's bytes are the `Value`'s own
    /// serialisation; [`Response::from_frame`] is what keeps the bytes a frame
    /// carried.
    pub fn from_value(value: Value) -> Result<Self, ProtocolError> {
        Ok(serde_json::from_slice(&serde_json::to_vec(&value)?)?)
    }

    /// Wraps the response in a frame with `id`: 0 for [`Response::Ready`], and
    /// non-zero for anything that answers a request.
    pub fn to_frame(&self, id: u64) -> Result<Frame, ProtocolError> {
        check_id(self.kind(), id)?;
        let payload = serde_json::to_vec(self)?;
        Ok(Frame::new(self.kind(), id, payload)?)
    }

    /// Parses a response from a frame, refusing a frame that is not a response.
    pub fn from_frame(frame: &Frame) -> Result<Self, ProtocolError> {
        match frame.kind() {
            Kind::Ready | Kind::Loaded | Kind::Reply | Kind::Unloaded | Kind::Error => {}
            other => return Err(ProtocolError::NotResponse(other)),
        }
        check_id(frame.kind(), frame.id())?;
        let response: Self = serde_json::from_slice(frame.payload())?;
        if response.kind() != frame.kind() {
            return Err(ProtocolError::KindMismatch {
                kind: frame.kind(),
                payload: response.kind(),
            });
        }
        Ok(response)
    }
}

/// The envelope's `type` tag.
///
/// Every kind is here, including the other direction's, so that a tag which
/// disagrees with its frame kind is reported as
/// [`ProtocolError::KindMismatch`] rather than as unreadable JSON.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Tag {
    Ready,
    Load,
    Loaded,
    Call,
    Reply,
    Unload,
    Unloaded,
    Shutdown,
    Error,
}

impl Tag {
    fn kind(self) -> Kind {
        match self {
            Self::Ready => Kind::Ready,
            Self::Load => Kind::Load,
            Self::Loaded => Kind::Loaded,
            Self::Call => Kind::Call,
            Self::Reply => Kind::Reply,
            Self::Unload => Kind::Unload,
            Self::Unloaded => Kind::Unloaded,
            Self::Shutdown => Kind::Shutdown,
            Self::Error => Kind::Error,
        }
    }
}

/// An envelope's fields, before its tag says which envelope it is.
///
/// A flat struct rather than an internally tagged enum: the tagged-enum form
/// buffers every field through [`serde_json::Value`], which rewrites the
/// payload and loses the bytes the protocol forwards. A struct hands the field
/// to [`RawValue`] as the frame holds it.
#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    tag: Tag,
    #[serde(default)]
    plugin: Option<String>,
    #[serde(default)]
    module: Option<PathBuf>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    payload: Option<Payload>,
    #[serde(default)]
    message: Option<String>,
}

impl Envelope {
    fn into_request(self) -> Result<Request, String> {
        let kind = self.tag.kind();
        match self.tag {
            Tag::Load => Ok(Request::Load {
                plugin: required(self.plugin, "plugin")?,
                module: required(self.module, "module")?,
            }),
            Tag::Call => Ok(Request::Call {
                plugin: required(self.plugin, "plugin")?,
                method: required(self.method, "method")?,
                payload: self.payload.unwrap_or_default(),
            }),
            Tag::Unload => Ok(Request::Unload {
                plugin: required(self.plugin, "plugin")?,
            }),
            Tag::Shutdown => Ok(Request::Shutdown),
            _ => Err(format!("a {kind:?} frame is not a request")),
        }
    }

    fn into_response(self) -> Result<Response, String> {
        let kind = self.tag.kind();
        match self.tag {
            Tag::Ready => Ok(Response::Ready),
            Tag::Loaded => Ok(Response::Loaded),
            Tag::Reply => Ok(Response::Reply {
                payload: self.payload.unwrap_or_default(),
            }),
            Tag::Unloaded => Ok(Response::Unloaded),
            Tag::Error => Ok(Response::Error {
                message: required(self.message, "message")?,
            }),
            _ => Err(format!("a {kind:?} frame is not a response")),
        }
    }
}

fn required<T>(value: Option<T>, field: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("missing field `{field}`"))
}

impl<'de> Deserialize<'de> for Request {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let envelope = Envelope::deserialize(deserializer)?;
        envelope
            .into_request()
            .map_err(<D::Error as serde::de::Error>::custom)
    }
}

impl<'de> Deserialize<'de> for Response {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let envelope = Envelope::deserialize(deserializer)?;
        envelope
            .into_response()
            .map_err(<D::Error as serde::de::Error>::custom)
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
    /// A request, or a response other than `ready`, used frame id 0.
    ///
    /// 0 is reserved for the unsolicited [`Kind::Ready`]: a request with id 0
    /// would never be answered, and an answer with id 0 would pair with
    /// nothing.
    ReservedId(Kind),
    /// A [`Response::Ready`] used a frame id other than 0.
    ///
    /// `ready` is unsolicited, so there is no request id to copy.
    ReadyWithId(u64),
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
            Self::ReservedId(kind) => {
                write!(f, "a {kind:?} frame used id 0, which is reserved for ready")
            }
            Self::ReadyWithId(id) => {
                write!(f, "a ready frame used id {id} instead of 0")
            }
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
