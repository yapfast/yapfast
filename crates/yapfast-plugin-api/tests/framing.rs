//! The frame layer: bytes on the pipe, limits and end-of-stream.

use std::io::{self, Read, Write};

use yapfast_plugin_api::{
    Frame, FrameError, Kind, ReadError, HEADER_LEN, MAX_FRAME_LEN, MAX_PAYLOAD_LEN,
};

/// A reader that hands back at most `step` bytes per call, as a pipe may.
struct Trickle<'a> {
    bytes: &'a [u8],
    step: usize,
}

impl Read for Trickle<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len().min(self.step).min(self.bytes.len());
        buf[..len].copy_from_slice(&self.bytes[..len]);
        self.bytes = &self.bytes[len..];
        Ok(len)
    }
}

/// A writer that records each write and flush in order.
#[derive(Default)]
struct Recorder {
    bytes: Vec<u8>,
    events: Vec<&'static str>,
    flush_error: Option<io::ErrorKind>,
}

impl Write for Recorder {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(buf);
        self.events.push("write");
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.events.push("flush");
        match self.flush_error {
            Some(kind) => Err(io::Error::from(kind)),
            None => Ok(()),
        }
    }
}

/// A writer that takes at most `step` bytes per call.
struct Dribble {
    bytes: Vec<u8>,
    step: usize,
}

impl Write for Dribble {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = buf.len().min(self.step);
        self.bytes.extend_from_slice(&buf[..len]);
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn frame(kind: Kind, id: u64, payload: &[u8]) -> Frame {
    Frame::new(kind, id, payload.to_vec()).expect("frame within the limit")
}

fn wire(frame: &Frame) -> Vec<u8> {
    let mut bytes = Vec::new();
    frame.write_to(&mut bytes).expect("writing to a Vec");
    bytes
}

fn read(bytes: &[u8]) -> Result<Option<Frame>, ReadError> {
    Frame::read_from(&mut &bytes[..])
}

#[test]
fn kind_discriminants_are_the_wire_format() {
    let kinds = [
        Kind::Ready,
        Kind::Load,
        Kind::Loaded,
        Kind::Call,
        Kind::Reply,
        Kind::Unload,
        Kind::Unloaded,
        Kind::Shutdown,
        Kind::Error,
    ];
    let bytes = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    for (kind, byte) in kinds.into_iter().zip(bytes) {
        assert_eq!(kind.as_u8(), byte, "{kind:?}");
        assert_eq!(Kind::from_u8(byte), Some(kind), "{kind:?}");
    }
    assert_eq!(Kind::from_u8(0), None);
    assert_eq!(Kind::from_u8(10), None);
    assert_eq!(Kind::from_u8(u8::MAX), None);
}

#[test]
fn wire_is_length_kind_id_payload() {
    let id = 0x0102_0304_0506_0708;
    let payload = br#"{"method":"shout"}"#;
    let bytes = wire(&frame(Kind::Call, id, payload));

    assert_eq!(&bytes[..4], &(payload.len() as u32).to_le_bytes());
    assert_eq!(bytes[4], Kind::Call.as_u8());
    assert_eq!(&bytes[5..HEADER_LEN], &id.to_le_bytes());
    assert_eq!(&bytes[HEADER_LEN..], payload);
    assert_eq!(bytes.len(), HEADER_LEN + payload.len());
}

#[test]
fn round_trips_json_including_empty_and_non_ascii() {
    let payloads: &[&[u8]] = &[
        b"{}",
        b"",
        "{\"text\":\"Ærø — ☃\",\"emoji\":\"🎧\"}".as_bytes(),
        br#"{"nested":{"list":[1,null,true]},"n":-0.5}"#,
    ];
    for &payload in payloads {
        let sent = frame(Kind::Reply, 42, payload);
        let received = read(&wire(&sent)).expect("a whole frame").expect("one frame");
        assert_eq!(received, sent, "{:?}", String::from_utf8_lossy(payload));
        assert_eq!(received.payload(), payload);
    }
}

#[test]
fn reads_frames_delivered_a_byte_at_a_time() {
    let first = frame(Kind::Load, 1, br#"{"plugin":"echo"}"#);
    let second = frame(Kind::Shutdown, 2, b"{}");
    let mut bytes = wire(&first);
    bytes.extend_from_slice(&wire(&second));

    let mut reader = Trickle { bytes: &bytes, step: 1 };
    assert_eq!(Frame::read_from(&mut reader).unwrap(), Some(first));
    assert_eq!(Frame::read_from(&mut reader).unwrap(), Some(second));
    assert_eq!(Frame::read_from(&mut reader).unwrap(), None);

    let mut reader = Trickle { bytes: &bytes, step: 3 };
    assert_eq!(Frame::read_from(&mut reader).unwrap().unwrap().id(), 1);
    assert_eq!(Frame::read_from(&mut reader).unwrap().unwrap().id(), 2);
}

#[test]
fn end_of_stream_between_frames_is_clean() {
    let one = wire(&frame(Kind::Ready, 0, b"{}"));
    let mut reader = &one[..];
    assert!(Frame::read_from(&mut reader).unwrap().is_some());
    assert_eq!(Frame::read_from(&mut reader).unwrap(), None);

    let mut empty = &[][..];
    assert_eq!(Frame::read_from(&mut empty).unwrap(), None);
}

#[test]
fn end_of_stream_inside_the_header_is_unexpected() {
    let bytes = wire(&frame(Kind::Call, 7, b"{}"));
    for cut in 1..HEADER_LEN {
        match read(&bytes[..cut]) {
            Err(ReadError::UnexpectedEof { expected, got }) => {
                assert_eq!(expected, HEADER_LEN);
                assert_eq!(got, cut);
            }
            other => panic!("{cut} header bytes ended as {other:?}"),
        }
    }
}

#[test]
fn end_of_stream_inside_the_payload_is_unexpected() {
    let bytes = wire(&frame(Kind::Call, 7, b"0123456789"));
    let cut = bytes.len() - 1;
    match read(&bytes[..cut]) {
        Err(ReadError::UnexpectedEof { expected, got }) => {
            assert_eq!(expected, 10);
            assert_eq!(got, 9);
        }
        other => panic!("truncated payload ended as {other:?}"),
    }
}

#[test]
fn reads_the_largest_frame_the_protocol_allows() {
    let mut payload = vec![b' '; MAX_PAYLOAD_LEN];
    payload[0] = b'{';
    *payload.last_mut().unwrap() = b'}';

    let largest = frame(Kind::Call, u64::MAX, &payload);
    let bytes = wire(&largest);
    assert_eq!(bytes.len(), MAX_FRAME_LEN);
    assert_eq!(&bytes[..4], &(MAX_PAYLOAD_LEN as u32).to_le_bytes());

    let received = read(&bytes).unwrap().expect("one frame");
    assert_eq!(received.payload().len(), MAX_PAYLOAD_LEN);
    assert_eq!(received, largest);
}

#[test]
fn refuses_an_oversize_payload_before_allocating_it() {
    assert!(matches!(
        Frame::new(Kind::Call, 0, vec![0; MAX_PAYLOAD_LEN + 1]),
        Err(FrameError::TooLarge { len }) if len == MAX_PAYLOAD_LEN + 1
    ));

    // Only the header is on the stream: a read that tried the payload would
    // report a truncated frame instead of refusing the length.
    for declared in [MAX_PAYLOAD_LEN as u32 + 1, u32::MAX] {
        let mut header = Vec::new();
        header.extend_from_slice(&declared.to_le_bytes());
        header.push(Kind::Call.as_u8());
        header.extend_from_slice(&0u64.to_le_bytes());

        match read(&header) {
            Err(ReadError::FrameTooLarge { len }) => assert_eq!(len, declared),
            other => panic!("declared {declared} bytes read as {other:?}"),
        }
    }
}

#[test]
fn refuses_an_unknown_kind_before_the_payload() {
    // A zero-length payload, so the only way to fail is the kind byte.
    for byte in [0, 10, 42, u8::MAX] {
        let mut header = Vec::new();
        header.extend_from_slice(&0u32.to_le_bytes());
        header.push(byte);
        header.extend_from_slice(&0u64.to_le_bytes());
        match read(&header) {
            Err(ReadError::UnknownKind(refused)) => assert_eq!(refused, byte),
            other => panic!("kind {byte} read as {other:?}"),
        }
    }
}

#[test]
fn writes_flush_after_the_frame() {
    let mut recorder = Recorder::default();
    frame(Kind::Ready, 9, b"{}")
        .write_to(&mut recorder)
        .expect("writing");
    assert_eq!(recorder.flushed(), 1);
    assert_eq!(recorder.events.last(), Some(&"flush"));
    assert_eq!(recorder.bytes, wire(&frame(Kind::Ready, 9, b"{}")));
}

#[test]
fn writes_flush_a_header_only_frame() {
    let mut recorder = Recorder::default();
    frame(Kind::Shutdown, 0, b"")
        .write_to(&mut recorder)
        .expect("writing");
    assert_eq!(recorder.flushed(), 1);
    assert_eq!(recorder.bytes.len(), HEADER_LEN);
}

#[test]
fn a_failed_flush_fails_the_write() {
    let mut recorder = Recorder {
        flush_error: Some(io::ErrorKind::BrokenPipe),
        ..Recorder::default()
    };
    let error = frame(Kind::Ready, 0, b"{}")
        .write_to(&mut recorder)
        .expect_err("the flush failed");
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
}

#[test]
fn short_writes_still_send_the_whole_frame() {
    let sent = frame(Kind::Reply, 3, br#"{"ok":true}"#);
    let mut dribble = Dribble { bytes: Vec::new(), step: 2 };
    sent.write_to(&mut dribble).expect("writing");
    assert_eq!(dribble.bytes, wire(&sent));
}

impl Recorder {
    fn flushed(&self) -> usize {
        self.events.iter().filter(|event| **event == "flush").count()
    }
}
