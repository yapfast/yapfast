use std::io::Read;

use wl_clipboard_rs::paste::{ClipboardType, Error, MimeType, Seat, get_contents};

use super::{Clipboard, PICTURES, files};

pub fn read() -> Result<Clipboard, Error> {
    let types =
        match wl_clipboard_rs::paste::get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
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
