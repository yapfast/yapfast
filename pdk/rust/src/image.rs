//! Pictures a plugin makes: RGBA pixels written as a PNG or an animated GIF,
//! in pure Rust and with nothing to link, so a petpet-style plugin can draw
//! frames and send them with [`crate::account::attach`].
//!
//! Pixels are rows of `[r, g, b, a]`, top to bottom, `width * height * 4`
//! bytes. The PNG is stored without compression (zlib's stored blocks): it is
//! exact and small enough for an emoji or a sticker, and needs no deflate
//! code in the module. The GIF is LZW-compressed as the format requires, with
//! a palette made from the frames' own colours when they have 255 or fewer,
//! and a fixed 6×7×6 colour cube otherwise; a pixel under half opacity is
//! transparent.

use crate::Error;
use crate::Result;

/// RGBA pixels and their size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
}

impl Image {
    /// A clear picture of this size.
    pub fn new(width: u16, height: u16) -> Self {
        Image {
            width,
            height,
            rgba: vec![0; usize::from(width) * usize::from(height) * 4],
        }
    }

    fn check(&self) -> Result<()> {
        let wanted = usize::from(self.width) * usize::from(self.height) * 4;
        if self.width == 0 || self.height == 0 || self.rgba.len() != wanted {
            return Err(Error::msg(format!(
                "a {}×{} picture is {wanted} bytes of RGBA, and this one is {}",
                self.width,
                self.height,
                self.rgba.len()
            )));
        }
        Ok(())
    }

    /// One pixel, or nothing outside the picture.
    pub fn pixel(&self, x: u16, y: u16) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = (usize::from(y) * usize::from(self.width) + usize::from(x)) * 4;
        self.rgba.get(at..at + 4).map(|p| [p[0], p[1], p[2], p[3]])
    }

    /// Sets one pixel; outside the picture does nothing.
    pub fn set(&mut self, x: u16, y: u16, rgba: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let at = (usize::from(y) * usize::from(self.width) + usize::from(x)) * 4;
        self.rgba[at..at + 4].copy_from_slice(&rgba);
    }

    /// `source` scaled to `width`×`height` and drawn over this picture at
    /// (`x`, `y`), nearest pixel, alpha over: how a petpet frame squashes the
    /// avatar under the hand.
    pub fn draw_scaled(&mut self, source: &Image, x: i32, y: i32, width: u16, height: u16) {
        if width == 0 || height == 0 {
            return;
        }
        for dy in 0..height {
            for dx in 0..width {
                let sx = (u32::from(dx) * u32::from(source.width) / u32::from(width)) as u16;
                let sy = (u32::from(dy) * u32::from(source.height) / u32::from(height)) as u16;
                let (Some(top), tx, ty) =
                    (source.pixel(sx, sy), x + i32::from(dx), y + i32::from(dy))
                else {
                    continue;
                };
                let (Ok(tx), Ok(ty)) = (u16::try_from(tx), u16::try_from(ty)) else {
                    continue;
                };
                let Some(under) = self.pixel(tx, ty) else {
                    continue;
                };
                self.set(tx, ty, over(top, under));
            }
        }
    }
}

/// `top` drawn over `under`.
fn over(top: [u8; 4], under: [u8; 4]) -> [u8; 4] {
    let a = u32::from(top[3]);
    if a == 255 {
        return top;
    }
    let b = u32::from(under[3]) * (255 - a) / 255;
    let out = a + b;
    if out == 0 {
        return [0; 4];
    }
    let mix = |t: u8, u: u8| ((u32::from(t) * a + u32::from(u) * b) / out) as u8;
    [
        mix(top[0], under[0]),
        mix(top[1], under[1]),
        mix(top[2], under[2]),
        out as u8,
    ]
}

/// The picture as a PNG file.
pub fn png(image: &Image) -> Result<Vec<u8>> {
    image.check()?;
    let mut raw = Vec::with_capacity(image.rgba.len() + usize::from(image.height));
    for row in image.rgba.chunks(usize::from(image.width) * 4) {
        raw.push(0); // no filter
        raw.extend_from_slice(row);
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&u32::from(image.width).to_be_bytes());
    header.extend_from_slice(&u32::from(image.height).to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// zlib around deflate's stored blocks, with the Adler-32 that ends it.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut blocks = data.chunks(65_535).peekable();
    if blocks.peek().is_none() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(block) = blocks.next() {
        out.push(u8::from(blocks.peek().is_none()));
        let len = block.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

/// One frame of a GIF and how long it shows, in hundredths of a second.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub image: Image,
    pub delay_cs: u16,
}

/// The frames as an animated GIF that loops forever. Every frame is the size
/// of the first.
pub fn gif(frames: &[Frame]) -> Result<Vec<u8>> {
    let first = frames
        .first()
        .ok_or_else(|| Error::msg("a GIF needs a frame"))?;
    let (width, height) = (first.image.width, first.image.height);
    for frame in frames {
        frame.image.check()?;
        if (frame.image.width, frame.image.height) != (width, height) {
            return Err(Error::msg("every frame of a GIF is the size of the first"));
        }
    }
    let palette = Palette::of(frames);
    let mut out = b"GIF89a".to_vec();
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&[0xF7, 0, 0]); // a global table of 256 colours
    for colour in &palette.colours {
        out.extend_from_slice(colour);
    }
    for _ in palette.colours.len()..256 {
        out.extend_from_slice(&[0, 0, 0]);
    }
    // NETSCAPE2.0: loop forever.
    out.extend_from_slice(&[0x21, 0xFF, 0x0B]);
    out.extend_from_slice(b"NETSCAPE2.0");
    out.extend_from_slice(&[0x03, 0x01, 0x00, 0x00, 0x00]);
    for frame in frames {
        // Graphic control: restore to the background, so a clear pixel is
        // clear on every frame, and the transparent index.
        out.extend_from_slice(&[0x21, 0xF9, 0x04, 0b0000_1001]);
        out.extend_from_slice(&frame.delay_cs.to_le_bytes());
        out.extend_from_slice(&[TRANSPARENT, 0]);
        out.push(0x2C);
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.push(0);
        let indices: Vec<u8> = frame
            .image
            .rgba
            .chunks(4)
            .map(|pixel| palette.index([pixel[0], pixel[1], pixel[2], pixel[3]]))
            .collect();
        out.push(8); // the LZW minimum code size
        for block in lzw(&indices).chunks(255) {
            out.push(block.len() as u8);
            out.extend_from_slice(block);
        }
        out.push(0);
    }
    out.push(0x3B);
    Ok(out)
}

/// The index every clear pixel takes.
const TRANSPARENT: u8 = 255;

struct Palette {
    colours: Vec<[u8; 3]>,
    /// Exact colours, sorted, when the frames have few enough.
    exact: Option<Vec<[u8; 3]>>,
}

impl Palette {
    fn of(frames: &[Frame]) -> Self {
        let mut seen: Vec<[u8; 3]> = Vec::new();
        'frames: for frame in frames {
            for pixel in frame.image.rgba.chunks(4) {
                if pixel[3] < 128 {
                    continue;
                }
                let colour = [pixel[0], pixel[1], pixel[2]];
                if let Err(at) = seen.binary_search(&colour) {
                    if seen.len() == 255 {
                        seen.clear();
                        break 'frames;
                    }
                    seen.insert(at, colour);
                }
            }
        }
        if !seen.is_empty() {
            return Palette {
                colours: seen.clone(),
                exact: Some(seen),
            };
        }
        let mut colours = Vec::with_capacity(252);
        for r in 0..6u16 {
            for g in 0..7u16 {
                for b in 0..6u16 {
                    colours.push([(r * 51) as u8, (g * 255 / 6) as u8, (b * 51) as u8]);
                }
            }
        }
        Palette {
            colours,
            exact: None,
        }
    }

    fn index(&self, pixel: [u8; 4]) -> u8 {
        if pixel[3] < 128 {
            return TRANSPARENT;
        }
        let colour = [pixel[0], pixel[1], pixel[2]];
        match &self.exact {
            Some(exact) => exact.binary_search(&colour).map_or(0, |at| at as u8),
            None => {
                let step =
                    |value: u8, levels: u16| ((u16::from(value) * (levels - 1) + 127) / 255) as u8;
                step(colour[0], 6) * 42 + step(colour[1], 7) * 6 + step(colour[2], 6)
            }
        }
    }
}

/// GIF's LZW over 8-bit indices: codes grow from 9 bits to 12, and the table
/// starts again when it is full.
fn lzw(indices: &[u8]) -> Vec<u8> {
    const CLEAR: u16 = 256;
    const END: u16 = 257;
    let mut out = Vec::new();
    let (mut bits, mut held) = (0u32, 0u32);
    let mut write = |code: u16, size: u32, out: &mut Vec<u8>| {
        held |= u32::from(code) << bits;
        bits += size;
        while bits >= 8 {
            out.push(held as u8);
            held >>= 8;
            bits -= 8;
        }
    };
    let mut table: std::collections::HashMap<(u16, u8), u16> = std::collections::HashMap::new();
    let mut next = 258u16;
    let mut size = 9u32;
    write(CLEAR, size, &mut out);
    let mut current: Option<u16> = None;
    for &index in indices {
        let Some(prefix) = current else {
            current = Some(u16::from(index));
            continue;
        };
        if let Some(&code) = table.get(&(prefix, index)) {
            current = Some(code);
            continue;
        }
        write(prefix, size, &mut out);
        if next < 4096 {
            table.insert((prefix, index), next);
            next += 1;
            if next > (1 << size) && size < 12 {
                size += 1;
            }
        } else {
            write(CLEAR, size, &mut out);
            table.clear();
            next = 258;
            size = 9;
        }
        current = Some(u16::from(index));
    }
    if let Some(prefix) = current {
        write(prefix, size, &mut out);
    }
    write(END, size, &mut out);
    if bits > 0 {
        out.push(held as u8);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker() -> Image {
        let mut image = Image::new(4, 3);
        for y in 0..3 {
            for x in 0..4 {
                let on = (x + y) % 2 == 0;
                image.set(x, y, if on { [255, 0, 0, 255] } else { [0, 0, 255, 0] });
            }
        }
        image
    }

    /// GIF's LZW read back, to check the writer against.
    fn unlzw(data: &[u8]) -> Vec<u8> {
        let mut table: Vec<Vec<u8>> = (0..=255u8).map(|i| vec![i]).collect();
        table.push(Vec::new());
        table.push(Vec::new());
        let (mut pos, mut size) = (0usize, 9usize);
        let read = |pos: &mut usize, size: usize| -> u16 {
            let mut code = 0u16;
            for bit in 0..size {
                let at = *pos + bit;
                if data[at / 8] >> (at % 8) & 1 == 1 {
                    code |= 1 << bit;
                }
            }
            *pos += size;
            code
        };
        let mut out = Vec::new();
        let mut previous: Option<Vec<u8>> = None;
        loop {
            let code = read(&mut pos, size);
            if code == 256 {
                table.truncate(258);
                size = 9;
                previous = None;
                continue;
            }
            if code == 257 {
                break;
            }
            let entry = if usize::from(code) < table.len() {
                table[usize::from(code)].clone()
            } else {
                let mut entry = previous.clone().unwrap();
                entry.push(entry[0]);
                entry
            };
            out.extend_from_slice(&entry);
            if let Some(mut grown) = previous.take() {
                grown.push(entry[0]);
                table.push(grown);
                if table.len() == (1 << size) && size < 12 {
                    size += 1;
                }
            }
            previous = Some(entry);
        }
        out
    }

    #[test]
    fn a_png_is_a_png_and_holds_the_pixels() {
        let image = checker();
        let file = png(&image).unwrap();
        assert!(file.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(&file[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(file[16..20].try_into().unwrap()), 4);
        // The stored block carries every row with its filter byte.
        let idat = file.windows(4).position(|w| w == b"IDAT").unwrap();
        let length = u32::from_be_bytes(file[idat - 4..idat].try_into().unwrap()) as usize;
        let zlib = &file[idat + 4..idat + 4 + length];
        let stored = &zlib[7..zlib.len() - 4];
        assert_eq!(stored.len(), 3 * (1 + 4 * 4));
        assert_eq!(&stored[1..5], &[255, 0, 0, 255]);
        assert_eq!(crc32(b"IEND"), 0xAE42_6082, "the PNG spec's IEND CRC");
        assert!(
            png(&Image {
                width: 2,
                height: 2,
                rgba: vec![0; 3]
            })
            .is_err()
        );
    }

    #[test]
    fn a_gif_loops_its_frames_and_reads_back() {
        let frames = vec![
            Frame {
                image: checker(),
                delay_cs: 6,
            },
            Frame {
                image: Image::new(4, 3),
                delay_cs: 6,
            },
        ];
        let file = gif(&frames).unwrap();
        assert!(file.starts_with(b"GIF89a"));
        assert!(file.ends_with(&[0x3B]));
        assert!(file.windows(11).any(|w| w == b"NETSCAPE2.0"));
        assert_eq!(file.iter().filter(|&&b| b == 0x2C).count() >= 2, true);
        let palette = Palette::of(&frames);
        assert_eq!(palette.colours, vec![[255, 0, 0]]);
        let indices: Vec<u8> = checker()
            .rgba
            .chunks(4)
            .map(|p| palette.index([p[0], p[1], p[2], p[3]]))
            .collect();
        assert_eq!(indices[0], 0);
        assert_eq!(indices[1], TRANSPARENT);
        assert!(gif(&[]).is_err());
    }

    /// With `YAPFAST_PDK_IMAGES=<dir>`, writes a big noisy GIF and a PNG there
    /// for a real decoder to read.
    #[test]
    fn writes_files_a_real_decoder_reads() {
        let Some(dir) = std::env::var_os("YAPFAST_PDK_IMAGES") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let mut frames = Vec::new();
        for f in 0..4u32 {
            let mut image = Image::new(112, 112);
            for y in 0..112u16 {
                for x in 0..112u16 {
                    let v = (u32::from(x) * 31 + u32::from(y) * 17 + f * 53) as u8;
                    image.set(
                        x,
                        y,
                        [v, v.wrapping_mul(3), 255 - v, if x < 4 { 0 } else { 255 }],
                    );
                }
            }
            frames.push(Frame { image, delay_cs: 5 });
        }
        std::fs::write(dir.join("test.gif"), gif(&frames).unwrap()).unwrap();
        std::fs::write(dir.join("test.png"), png(&frames[0].image).unwrap()).unwrap();
        let few = vec![Frame {
            image: checker(),
            delay_cs: 10,
        }];
        std::fs::write(dir.join("few.gif"), gif(&few).unwrap()).unwrap();
    }

    #[test]
    fn lzw_round_trips_long_runs_and_a_full_table() {
        let mut indices: Vec<u8> = (0..20_000u32).map(|i| (i * 7 % 251) as u8).collect();
        indices.extend(std::iter::repeat_n(3u8, 5_000));
        assert_eq!(unlzw(&lzw(&indices)), indices);
        assert_eq!(unlzw(&lzw(&[9])), vec![9]);
    }

    #[test]
    fn a_squashed_avatar_draws_over_the_frame() {
        let mut frame = Image::new(8, 8);
        let mut avatar = Image::new(2, 2);
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            avatar.set(x, y, [0, 255, 0, 255]);
        }
        frame.draw_scaled(&avatar, 2, 4, 4, 2);
        assert_eq!(frame.pixel(2, 4), Some([0, 255, 0, 255]));
        assert_eq!(frame.pixel(5, 5), Some([0, 255, 0, 255]));
        assert_eq!(frame.pixel(1, 4), Some([0, 0, 0, 0]));
    }
}
