//! What Ctrl+V in the composer finds on the clipboard: files copied in a file
//! manager, a picture copied from a browser or a screenshot tool, or else text,
//! which the composer pastes itself.
//!
//! On Wayland the clipboard is read through the data-control protocol, which
//! hands over a picture's bytes as they were copied, so a GIF stays animated.
//! Elsewhere arboard reads it; its pictures arrive as pixels and are saved as PNG.

use std::path::PathBuf;

pub enum Clipboard {
    Files(Vec<PathBuf>),
    /// A picture's file contents, and the extension for its type.
    Picture(Vec<u8>, &'static str),
    /// Text, or nothing Yapfast can attach.
    Other,
}

/// Picture types Discord shows inline, best first, with their extensions.
/// Only the Wayland path needs them: elsewhere a copied picture arrives as
/// pixels rather than as one of these, and goes to the composer as a PNG.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
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
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        match wayland::read() {
            Ok(clipboard) => return clipboard,
            Err(error) => tracing::debug!(%error, "could not read the Wayland clipboard"),
        }
    }
    // What a copy in Finder holds comes off the pasteboard directly: arboard
    // reads a list of files from Windows and the X11 and Wayland clipboards,
    // not from macOS's.
    #[cfg(target_os = "macos")]
    {
        let files = crate::appkit::pasteboard_files();
        if !files.is_empty() {
            return Clipboard::Files(files);
        }
    }
    read_arboard()
}

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
    let mut png = std::io::Cursor::new(Vec::new());
    match pixels.write_to(&mut png, image::ImageFormat::Png) {
        Ok(()) => Clipboard::Picture(png.into_inner(), "png"),
        Err(_) => Clipboard::Other,
    }
}

#[cfg(target_os = "linux")]
mod wayland {
    use std::io::Read;

    use wl_clipboard_rs::paste::{ClipboardType, Error, MimeType, Seat, get_contents};

    use super::{Clipboard, PICTURES, files};

    pub fn read() -> Result<Clipboard, Error> {
        let types =
            match wl_clipboard_rs::paste::get_mime_types(ClipboardType::Regular, Seat::Unspecified)
            {
                Ok(types) => types,
                Err(Error::ClipboardEmpty | Error::NoMimeType) => return Ok(Clipboard::Other),
                Err(error) => return Err(error),
            };
        let contents = |kind: &str| -> Result<Vec<u8>, Error> {
            let (mut pipe, _) = get_contents(
                ClipboardType::Regular,
                Seat::Unspecified,
                MimeType::Specific(kind),
            )?;
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes).map_err(Error::PipeCreation)?;
            Ok(bytes)
        };
        if types.contains("text/uri-list") {
            let list = contents("text/uri-list")?;
            let found = files(&String::from_utf8_lossy(&list));
            if !found.is_empty() {
                return Ok(Clipboard::Files(found));
            }
        }
        for (kind, extension) in PICTURES {
            if types.contains(kind) {
                return Ok(Clipboard::Picture(contents(kind)?, extension));
            }
        }
        Ok(Clipboard::Other)
    }
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
mod tests {
    use super::*;

    #[test]
    fn reads_local_files_from_a_uri_list() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a map #1.txt");
        std::fs::write(&file, "map").unwrap();
        let encoded = file
            .to_str()
            .unwrap()
            .replace(' ', "%20")
            .replace('#', "%23");
        let list = format!(
            "# copied\r\nfile://{encoded}\r\nfile://localhost{encoded}\r\nhttps://example.com/a.png\r\nfile:///no/such/file\r\n"
        );
        assert_eq!(files(&list), vec![file.clone(), file]);
        assert_eq!(percent_decode("%zz"), None);
    }

    /// Reads the desktop's clipboard, for checking by hand: copy files or a
    /// picture, then run with `--ignored --nocapture`.
    #[test]
    #[ignore = "reads the desktop clipboard"]
    fn reads_the_desktop_clipboard() {
        #[cfg(target_os = "linux")]
        if let Err(error) = wayland::read() {
            println!("wayland error {error:?}");
        }
        match read() {
            Clipboard::Files(files) => println!("files {files:?}"),
            Clipboard::Picture(bytes, extension) => println!("picture {extension} {}", bytes.len()),
            Clipboard::Other => println!("other"),
        }
    }
}
