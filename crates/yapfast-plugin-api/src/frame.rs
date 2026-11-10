//! Framing for the pipe between Yapfast and the plugin helper.
//!
//! One frame is a fixed header and a payload:
//!
//! ```text
//! u32 payload length (little-endian)
//! u8  kind
//! u64 id (little-endian)
//! payload bytes
//! ```
//!
//! The payload is JSON forwarded verbatim, so neither side decodes a message it
//! is only relaying. `id` pairs a request with its answer; events and log lines
//! carry `0`. Length counts the payload alone, and a frame is at most
//! [`MAX_FRAME`]; a length above it is a broken stream rather than a big
//! message, so reading it fails instead of allocating.

use std::io::{self, Read, Write};

/// The largest payload a frame may carry: 1 MiB.
pub const MAX_FRAME: usize = 1 << 20;

/// The header's length in bytes.
pub const HEADER_LEN: usize = 4 + 1 + 8;

/// What a frame carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FrameKind {
    /// The first frame either way: protocol version and helper version.
    Hello = 1,
    /// A request the other side answers with [`FrameKind::Reply`].
    Request = 2,
    /// The answer to a [`FrameKind::Request`], or to a guest's host call.
    Reply = 3,
    /// A batch of events for a guest, carrying no answer.
    Event = 4,
    /// Something the helper reports as it happens: status, fault, exit.
    Report = 5,
    /// One line of a plugin's log, ring-buffered by the parent.
    Log = 6,
    /// The helper is going away, and why.
    Bye = 7,
}

impl FrameKind {
    /// The kind with this byte, if it is one we know.
    pub fn from_u8(byte: u8) -> Option<Self> {
        Some(match byte {
            1 => FrameKind::Hello,
            2 => FrameKind::Request,
            3 => FrameKind::Reply,
            4 => FrameKind::Event,
            5 => FrameKind::Report,
            6 => FrameKind::Log,
            7 => FrameKind::Bye,
            _ => return None,
        })
    }
}

/// One frame, decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: FrameKind,
    /// Pairs a request with its answer; `0` when nothing pairs with it.
    pub id: u64,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(kind: FrameKind, id: u64, payload: impl Into<Vec<u8>>) -> Self {
        Frame {
            kind,
            id,
            payload: payload.into(),
        }
    }
}

/// Writes one frame, and flushes it.
///
/// A write of a frame larger than [`MAX_FRAME`] is refused rather than
/// truncated: the peer would read a short frame and lose the stream.
pub fn write_frame(mut out: impl Write, frame: &Frame) -> io::Result<()> {
    if frame.payload.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "a frame payload is at most {MAX_FRAME} bytes, not {}",
                frame.payload.len()
            ),
        ));
    }
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(&(frame.payload.len() as u32).to_le_bytes());
    header[4] = frame.kind as u8;
    header[5..13].copy_from_slice(&frame.id.to_le_bytes());
    out.write_all(&header)?;
    out.write_all(&frame.payload)?;
    out.flush()
}

/// Reads one frame.
///
/// `Ok(None)` is the peer closing the pipe between frames, which is how the
/// helper is told to exit. A partial header or payload is
/// [`io::ErrorKind::UnexpectedEof`]; a length over [`MAX_FRAME`] or an unknown
/// kind is [`io::ErrorKind::InvalidData`].
pub fn read_frame(mut input: impl Read) -> io::Result<Option<Frame>> {
    let mut header = [0u8; HEADER_LEN];
    if !read_exact_or_eof(&mut input, &mut header)? {
        return Ok(None);
    }
    let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
    if length > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a frame payload is at most {MAX_FRAME} bytes, not {length}"),
        ));
    }
    let kind = FrameKind::from_u8(header[4]).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unknown frame kind {}", header[4]),
        )
    })?;
    let id = u64::from_le_bytes([
        header[5], header[6], header[7], header[8], header[9], header[10], header[11], header[12],
    ]);
    let mut payload = vec![0u8; length];
    input.read_exact(&mut payload)?;
    Ok(Some(Frame { kind, id, payload }))
}

/// Fills `buf`, returning `false` when the pipe closed before the first byte.
fn read_exact_or_eof(input: &mut impl Read, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match input.read(&mut buf[filled..]) {
            Ok(0) if filled == 0 => return Ok(false),
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the pipe closed inside a frame header",
                ));
            }
            Ok(n) => filled += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(kind: FrameKind, id: u64, payload: &[u8]) {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &Frame::new(kind, id, payload)).unwrap();
        let read = read_frame(&bytes[..]).unwrap().unwrap();
        assert_eq!(read.kind, kind);
        assert_eq!(read.id, id);
        assert_eq!(read.payload, payload);
        // Nothing is left over from the one frame.
        assert!(read_frame(&bytes[..]).unwrap().is_some() || payload.len() < MAX_FRAME);
    }

    #[test]
    fn carries_payloads_and_ids() {
        round_trip(FrameKind::Request, 0, b"");
        round_trip(FrameKind::Reply, u64::MAX, b"{\"ok\":true}");
        round_trip(FrameKind::Event, 7, &vec![0x5a; 64 * 1024]);
        round_trip(FrameKind::Log, 0, &vec![0x5a; MAX_FRAME]);
    }

    #[test]
    fn eof_between_frames_is_clean() {
        assert_eq!(read_frame(&b""[..]).unwrap(), None);
    }

    #[test]
    fn a_truncated_frame_fails() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &Frame::new(FrameKind::Request, 1, b"abc")).unwrap();
        for cut in 1..bytes.len() {
            let error = read_frame(&bytes[..cut]).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof, "cut at {cut}");
        }
    }

    #[test]
    fn an_oversized_or_unknown_frame_is_refused() {
        let mut bytes = ((MAX_FRAME as u32) + 1).to_le_bytes().to_vec();
        bytes.push(FrameKind::Request as u8);
        bytes.extend_from_slice(&0u64.to_le_bytes());
        assert_eq!(
            read_frame(&bytes[..]).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );

        let mut bytes = 0u32.to_le_bytes().to_vec();
        bytes.push(200);
        bytes.extend_from_slice(&0u64.to_le_bytes());
        assert_eq!(
            read_frame(&bytes[..]).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn refuses_to_write_an_oversized_frame() {
        let frame = Frame::new(FrameKind::Event, 0, vec![0u8; MAX_FRAME + 1]);
        let error = write_frame(Vec::new(), &frame).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
