//! What Ctrl+V in the composer finds on the clipboard: files copied in a file
//! manager, a picture copied from a browser or a screenshot tool, or else text,
//! which the composer pastes itself.
//!
//! A picture keeps the bytes it was copied as, PNG, GIF, WebP or JPEG, so a
//! GIF stays animated. On Wayland the clipboard is read through the
//! data-control protocol, and on X11 its owner is asked for the CLIPBOARD
//! selection. On Windows the formats come by the names apps register for them,
//! and a picture copied only as a bitmap is saved as PNG. Elsewhere arboard
//! reads it; its pictures arrive as pixels and are saved as PNG.

use std::path::PathBuf;

pub enum Clipboard {
    Files(Vec<PathBuf>),
    /// A picture's file contents, and the extension for its type.
    Picture(Vec<u8>, &'static str),
    /// Text, or nothing Yapfast can attach.
    Other,
}

/// Picture types Discord shows inline, best first, with their extensions.
const PICTURES: [(&str, &str); 4] = [
    ("image/png", "png"),
    ("image/gif", "gif"),
    ("image/webp", "webp"),
    ("image/jpeg", "jpg"),
];

/// Reads the clipboard. Blocks while the copying app sends its data, so it
/// runs off the UI thread; Yapfast itself may be that app.
pub fn read() -> Clipboard {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            match wayland::read() {
                Ok(clipboard) => return clipboard,
                Err(error) => {
                    tracing::debug!(target: crate::LOG, %error, "could not read the Wayland clipboard")
                }
            }
        }
        x11::read().unwrap_or_else(|error| {
            tracing::debug!(target: crate::LOG, %error, "could not read the X11 clipboard");
            Clipboard::Other
        })
    }
    #[cfg(windows)]
    {
        windows::read()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        read_arboard()
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn read_arboard() -> Clipboard {
    let Ok(mut clipboard) = arboard::Clipboard::new() else {
        return Clipboard::Other;
    };
    if let Ok(files) = clipboard.get().file_list() {
        let files: Vec<_> = files.into_iter().filter(|path| path.is_file()).collect();
        if !files.is_empty() {
            return Clipboard::Files(files);
        }
    }
    if clipboard.get_text().is_ok() {
        return Clipboard::Other;
    }
    let Ok(picture) = clipboard.get_image() else {
        return Clipboard::Other;
    };
    let Some(pixels) = image::RgbaImage::from_raw(
        picture.width as u32,
        picture.height as u32,
        picture.bytes.into_owned(),
    ) else {
        return Clipboard::Other;
    };
    png(&pixels)
}

/// Pixels as a PNG file to attach.
#[cfg_attr(target_os = "linux", allow(dead_code))]
fn png(pixels: &image::RgbaImage) -> Clipboard {
    let mut png = std::io::Cursor::new(Vec::new());
    match pixels.write_to(&mut png, image::ImageFormat::Png) {
        Ok(()) => Clipboard::Picture(png.into_inner(), "png"),
        Err(error) => {
            tracing::debug!(target: crate::LOG, %error, "could not save a pasted picture as PNG");
            Clipboard::Other
        }
    }
}

#[cfg(target_os = "linux")]
mod wayland;

#[cfg(target_os = "linux")]
mod x11;

#[cfg(windows)]
mod windows;

/// Decodes a device-independent bitmap, Windows' clipboard picture: a
/// `BITMAPINFOHEADER` or a later version of it, then a colour table or
/// masks, then the pixels. 32-bit pixels without masks may carry alpha in
/// their unused fourth byte, as Chromium's do, which image's decoder would
/// drop, so they're read here. A bitmap whose alpha is zero throughout is
/// opaque.
#[cfg_attr(not(windows), allow(dead_code))]
fn bitmap(dib: &[u8]) -> Option<image::RgbaImage> {
    const BI_RGB: u32 = 0;
    let u32_at = |at: usize| Some(u32::from_le_bytes(dib.get(at..at + 4)?.try_into().ok()?));
    let header = u32_at(0)? as usize;
    let bits = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?);
    let mut pixels = if header >= 40 && bits == 32 && u32_at(16)? == BI_RGB {
        let width = u32_at(4)? as i32;
        let height = u32_at(8)? as i32;
        let colours = u32_at(32)? as usize;
        let columns = u32::try_from(width).ok().filter(|&width| width > 0)?;
        let rows = height.unsigned_abs();
        let stride = columns as usize * 4;
        let start = header.checked_add(colours.checked_mul(4)?)?;
        let end = start.checked_add(stride.checked_mul(rows as usize)?)?;
        let data = dib.get(start..end)?;
        let mut pixels = image::RgbaImage::new(columns, rows);
        let rows_out = pixels.chunks_exact_mut(stride);
        // Rows go from the bottom up unless the height is negative.
        let rows_in: Box<dyn Iterator<Item = &[u8]>> = if height > 0 {
            Box::new(data.chunks_exact(stride).rev())
        } else {
            Box::new(data.chunks_exact(stride))
        };
        for (out, row) in rows_out.zip(rows_in) {
            for (out, pixel) in out.chunks_exact_mut(4).zip(row.chunks_exact(4)) {
                out.copy_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
            }
        }
        pixels
    } else {
        let decoder =
            image::codecs::bmp::BmpDecoder::new_without_file_header(std::io::Cursor::new(dib))
                .ok()?;
        image::DynamicImage::from_decoder(decoder)
            .ok()?
            .into_rgba8()
    };
    if pixels.pixels().all(|pixel| pixel[3] == 0) {
        pixels.pixels_mut().for_each(|pixel| pixel[3] = u8::MAX);
    }
    Some(pixels)
}

/// The existing local files a `text/uri-list` names.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn files(list: &str) -> Vec<PathBuf> {
    list.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.strip_prefix("file://"))
        // A host may come before the path; only this machine's files can be read.
        .filter_map(|rest| rest.strip_prefix("localhost").or(Some(rest)))
        .filter(|path| path.starts_with('/'))
        .filter_map(percent_decode)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .collect()
}

fn percent_decode(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if byte == b'%' {
            let hex = std::str::from_utf8(tail.get(..2)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(byte);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests;
