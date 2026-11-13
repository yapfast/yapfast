//! The shapes an encoded frame's bytes come in, which a packetizer and a
//! decoder both have to know: the NAL units of an H264 or H265 access unit, and
//! the OBUs of an AV1 temporal unit.

/// The NAL units of an Annex B access unit, without their start codes. As
/// libwebrtc finds them: a start code is 0 0 1, with at most one more zero
/// before it, so bytes of a unit that happen to be zero stay in it.
pub fn nal_units(frame: &[u8]) -> Vec<&[u8]> {
    NalUnits::new(frame).map(|unit| &frame[unit]).collect()
}

/// Those units one at a time, as the ranges of the frame they take, so the
/// packetizer plans a frame without a vector of its units.
pub struct NalUnits<'a> {
    pub frame: &'a [u8],
    /// Where to look for the next start code.
    pub at: usize,
    /// The start code found but not yet a unit's: where it began, and where
    /// the bytes after it begin.
    pub found: Option<(usize, usize)>,
}

impl<'a> NalUnits<'a> {
    pub fn new(frame: &'a [u8]) -> Self {
        Self {
            frame,
            at: 0,
            found: None,
        }
    }

    /// The next start code from `at`, which moves past it.
    pub fn find(&mut self) -> Option<(usize, usize)> {
        while self.at + 3 <= self.frame.len() {
            let at = self.at;
            if self.frame[at..at + 3] == [0, 0, 1] {
                let code = if at > 0 && self.frame[at - 1] == 0 {
                    at - 1
                } else {
                    at
                };
                self.at = at + 3;
                return Some((code, at + 3));
            }
            self.at = at + 1;
        }
        None
    }
}

impl Iterator for NalUnits<'_> {
    type Item = std::ops::Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // The code that opens the unit, and the one that ends it.
            let (_, start) = match self.found.take() {
                Some(found) => found,
                None => self.find()?,
            };
            let end = match self.find() {
                Some(found) => {
                    self.found = Some(found);
                    found.0
                }
                None => self.frame.len(),
            };
            // Two start codes in a row make a unit of no bytes, which isn't
            // one.
            if end > start {
                return Some(start..end);
            }
        }
    }
}

/// AV1 OBU types the packetizer and DAVE handle apart.
pub const OBU_SEQUENCE_HEADER: u8 = 1;
pub const OBU_TEMPORAL_DELIMITER: u8 = 2;
pub const OBU_TILE_LIST: u8 = 8;
pub const OBU_PADDING: u8 = 15;
/// An OBU header's extension and size flags.
pub const OBU_EXTENSION: u8 = 0x04;
pub const OBU_HAS_SIZE: u8 = 0x02;

/// One OBU of an AV1 temporal unit, as the packetizer hands it on: its header
/// without the size flag, then its payload, which the frame keeps apart from
/// it behind the size.
#[derive(Clone, Copy)]
pub struct Element {
    /// Where the OBU's header is in the frame, and how long the header is.
    pub at: usize,
    pub header: usize,
    /// Where its payload is, and how long it is.
    pub body: usize,
    pub len: usize,
}

impl Element {
    /// The element's bytes: its header without the size flag, then its
    /// payload.
    pub fn bytes(self, frame: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.header + self.len);
        bytes.push(frame[self.at] & !OBU_HAS_SIZE);
        bytes.extend_from_slice(&frame[self.at + 1..self.at + self.header]);
        bytes.extend_from_slice(&frame[self.body..self.body + self.len]);
        bytes
    }
}

/// Reads the OBU at `from` in `frame`: the element it makes, unless its type
/// is one the packetizer drops, and where the OBU after it begins. None for
/// an OBU that runs past the frame or sets a bit its header keeps clear.
pub fn read_obu(frame: &[u8], from: usize) -> Option<(Option<Element>, usize)> {
    let rest = &frame[from..];
    let &header = rest.first()?;
    // The forbidden bit and the reserved one.
    if header & 0x81 != 0 {
        return None;
    }
    let header_size = if header & OBU_EXTENSION != 0 { 2 } else { 1 };
    let headers = rest.get(..header_size)?;
    // The extension's reserved bits, after the temporal and spatial ids.
    if headers
        .get(1)
        .is_some_and(|extension| extension & 0x07 != 0)
    {
        return None;
    }
    let rest = &rest[header_size..];
    let (size, used) = if header & OBU_HAS_SIZE != 0 {
        read_leb128(rest)?
    } else {
        (rest.len(), 0)
    };
    let body = from + header_size + used;
    if size > frame.len() - body {
        return None;
    }
    let dropped = matches!(
        (header >> 3) & 0x0f,
        OBU_TEMPORAL_DELIMITER | OBU_TILE_LIST | OBU_PADDING
    );
    let element = (!dropped).then_some(Element {
        at: from,
        header: header_size,
        body,
        len: size,
    });
    Some((element, body + size))
}

/// The OBUs of an AV1 temporal unit, each as an OBU element: its header
/// (and extension) without the size flag, then its payload. As libwebrtc's
/// packetizer reads them: an OBU without a size runs to the end of the
/// frame, as DAVE leaves the last one with its trailer after it; temporal
/// delimiters, tile lists and padding are dropped. None when an OBU runs
/// past the frame, or sets a bit its header keeps clear.
pub fn obu_elements(frame: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut elements = vec![];
    let mut at = 0;
    while at < frame.len() {
        let (element, next) = read_obu(frame, at)?;
        at = next;
        if let Some(element) = element {
            elements.push(element.bytes(frame));
        }
    }
    Some(elements)
}

/// An unsigned LEB128 number and the bytes it took: at most eight, as AV1
/// allows.
pub fn read_leb128(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut value = 0u64;
    for (index, &byte) in bytes.iter().enumerate().take(8) {
        value |= u64::from(byte & 0x7f) << (7 * index);
        if byte & 0x80 == 0 {
            return Some((usize::try_from(value).ok()?, index + 1));
        }
    }
    None
}
