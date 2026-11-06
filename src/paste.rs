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
                Err(error) => tracing::debug!(%error, "could not read the Wayland clipboard"),
            }
        }
        x11::read().unwrap_or_else(|error| {
            tracing::debug!(%error, "could not read the X11 clipboard");
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
            tracing::debug!(%error, "could not save a pasted picture as PNG");
            Clipboard::Other
        }
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

#[cfg(target_os = "linux")]
mod x11 {
    //! The CLIPBOARD selection, asked of its owner as the ICCCM has it: its
    //! targets, then the one Yapfast takes, each into a property on a window
    //! of Yapfast's own. Data too big for one request comes in parts (INCR).
    //! Text comes before pictures, as office suites offer a picture of the
    //! cells or text they copy.

    use std::error::Error;
    use std::time::{Duration, Instant};

    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ConnectionExt, CreateWindowAux, EventMask, Property, Window, WindowClass,
    };
    use x11rb::rust_connection::RustConnection;

    use super::{Clipboard, PICTURES, files};

    /// How long the clipboard's owner may take to answer, or to send the
    /// next part.
    const PATIENCE: Duration = Duration::from_secs(5);

    const TEXT: [&str; 6] = [
        "UTF8_STRING",
        "text/plain;charset=utf-8",
        "text/plain;charset=UTF-8",
        "STRING",
        "TEXT",
        "text/plain",
    ];

    pub fn read() -> Result<Clipboard, Box<dyn Error>> {
        let (connection, screen) = x11rb::connect(None)?;
        let names = [
            "CLIPBOARD",
            "YAPFAST_PASTE",
            "INCR",
            "TARGETS",
            "text/uri-list",
        ]
        .into_iter()
        .chain(TEXT)
        .chain(PICTURES.map(|(kind, _)| kind));
        let cookies = names
            .map(|name| connection.intern_atom(false, name.as_bytes()))
            .collect::<Result<Vec<_>, _>>()?;
        let atoms = cookies
            .into_iter()
            .map(|cookie| Ok(cookie.reply()?.atom))
            .collect::<Result<Vec<Atom>, Box<dyn Error>>>()?;
        let [clipboard, property, incr, targets, uri_list] = atoms[..5] else {
            unreachable!()
        };
        let (text, pictures) = atoms[5..].split_at(TEXT.len());

        let window = connection.generate_id()?;
        let root = connection.setup().roots[screen].root;
        connection.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        let reader = Reader {
            connection,
            window,
            clipboard,
            property,
            incr,
        };

        let Some(offered) = reader.get(targets)? else {
            return Ok(Clipboard::Other);
        };
        let offered: Vec<Atom> = offered
            .chunks_exact(4)
            .map(|atom| u32::from_ne_bytes(atom.try_into().unwrap()))
            .collect();
        if offered.contains(&uri_list)
            && let Some(list) = reader.get(uri_list)?
        {
            let found = files(&String::from_utf8_lossy(&list));
            if !found.is_empty() {
                return Ok(Clipboard::Files(found));
            }
        }
        if text.iter().any(|kind| offered.contains(kind)) {
            return Ok(Clipboard::Other);
        }
        for (kind, (_, extension)) in pictures.iter().zip(PICTURES) {
            if offered.contains(kind)
                && let Some(bytes) = reader.get(*kind)?
            {
                return Ok(Clipboard::Picture(bytes, extension));
            }
        }
        Ok(Clipboard::Other)
    }

    struct Reader {
        connection: RustConnection,
        window: Window,
        clipboard: Atom,
        property: Atom,
        incr: Atom,
    }

    impl Reader {
        /// The clipboard as `target`, or `None` if its owner can't give it.
        fn get(&self, target: Atom) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
            self.connection.convert_selection(
                self.window,
                self.clipboard,
                target,
                self.property,
                x11rb::CURRENT_TIME,
            )?;
            self.connection.flush()?;
            let Event::SelectionNotify(notify) = self.wait(|event| {
                matches!(event, Event::SelectionNotify(notify)
                    if notify.requestor == self.window && notify.target == target)
            })?
            else {
                unreachable!()
            };
            if notify.property == x11rb::NONE {
                return Ok(None);
            }
            // Taking the property deletes it, which asks for INCR's first part.
            let reply = self.take()?;
            if reply.type_ != self.incr {
                return Ok(Some(reply.value));
            }
            let mut bytes = Vec::new();
            loop {
                self.wait(|event| {
                    matches!(event, Event::PropertyNotify(change)
                        if change.window == self.window
                            && change.atom == self.property
                            && change.state == Property::NEW_VALUE)
                })?;
                let part = self.take()?.value;
                if part.is_empty() {
                    return Ok(Some(bytes));
                }
                bytes.extend_from_slice(&part);
            }
        }

        fn take(&self) -> Result<x11rb::protocol::xproto::GetPropertyReply, Box<dyn Error>> {
            let reply = self
                .connection
                .get_property(
                    true,
                    self.window,
                    self.property,
                    AtomEnum::ANY,
                    0,
                    u32::MAX / 4,
                )?
                .reply()?;
            Ok(reply)
        }

        fn wait(&self, wanted: impl Fn(&Event) -> bool) -> Result<Event, Box<dyn Error>> {
            let deadline = Instant::now() + PATIENCE;
            loop {
                while let Some(event) = self.connection.poll_for_event()? {
                    if wanted(&event) {
                        return Ok(event);
                    }
                }
                if Instant::now() > deadline {
                    return Err("the clipboard's owner stopped answering".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

#[cfg(windows)]
mod windows {
    //! Files from `CF_HDROP`; then text, which the composer pastes, since
    //! office suites put a bitmap of the cells or text they copy beside it;
    //! then a picture as it was copied, under the names apps register for
    //! each type (Snipping Tool and browsers register "PNG"); then a bitmap,
    //! which nearly every app copying a picture puts.

    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardFormatNameW,
        OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows_sys::Win32::UI::Shell::DragQueryFileW;

    use super::{Clipboard, PICTURES, bitmap, png};

    // The standard formats Yapfast takes, from WinUser.h.
    const CF_TEXT: u32 = 1;
    const CF_OEMTEXT: u32 = 7;
    const CF_DIB: u32 = 8;
    const CF_UNICODETEXT: u32 = 13;
    const CF_HDROP: u32 = 15;
    const CF_DIBV5: u32 = 17;

    pub fn read() -> Clipboard {
        let Some(open) = Open::new() else {
            tracing::debug!("could not open the clipboard");
            return Clipboard::Other;
        };
        let formats = open.formats();
        tracing::debug!(?formats, "the clipboard's formats");
        if formats.iter().any(|(format, _)| *format == CF_HDROP)
            && let Some(files) = open.files()
            && !files.is_empty()
        {
            return Clipboard::Files(files);
        }
        let text = [CF_TEXT, CF_OEMTEXT, CF_UNICODETEXT];
        if formats.iter().any(|(format, _)| text.contains(format)) {
            return Clipboard::Other;
        }
        for (kind, extension) in PICTURES {
            let expected = image::ImageFormat::from_mime_type(kind);
            for (format, name) in &formats {
                if !name.as_deref().is_some_and(|name| is_type(name, kind)) {
                    continue;
                }
                let Some(bytes) = open.data(*format) else {
                    continue;
                };
                // A name is only a convention between apps; the bytes say what they are.
                if image::guess_format(bytes.bytes()).ok() == expected {
                    return Clipboard::Picture(bytes.bytes().to_vec(), extension);
                }
            }
        }
        // The one the copying app put, rather than the one Windows makes of it.
        let Some((format, _)) = formats
            .iter()
            .find(|(format, _)| matches!(*format, CF_DIB | CF_DIBV5))
        else {
            return Clipboard::Other;
        };
        let pixels = open.data(*format).and_then(|dib| bitmap(dib.bytes()));
        drop(open);
        match pixels {
            Some(pixels) => png(&pixels),
            None => {
                tracing::debug!(format, "could not read the clipboard's bitmap");
                Clipboard::Other
            }
        }
    }

    /// Whether `name`, a registered clipboard format, is the picture type `kind`.
    fn is_type(name: &str, kind: &str) -> bool {
        let legacy = match kind {
            "image/png" => "PNG",
            "image/gif" => "GIF",
            "image/jpeg" => "JFIF",
            _ => "",
        };
        name.eq_ignore_ascii_case(kind) || name.eq_ignore_ascii_case(legacy)
    }

    /// The clipboard, open for reading until dropped.
    struct Open;

    impl Open {
        fn new() -> Option<Self> {
            // Another app may hold it for a moment.
            for _ in 0..10 {
                // SAFETY: no window to own it is needed to read it.
                if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                    return Some(Self);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            None
        }

        /// Its formats, in the order the copying app put them, those Windows
        /// makes from them last; with the names of registered ones.
        fn formats(&self) -> Vec<(u32, Option<String>)> {
            let mut formats = Vec::new();
            let mut format = 0;
            loop {
                // SAFETY: the clipboard is open.
                format = unsafe { EnumClipboardFormats(format) };
                if format == 0 {
                    return formats;
                }
                let mut name = [0u16; 128];
                // SAFETY: the buffer's length is what's passed.
                let length = unsafe {
                    GetClipboardFormatNameW(format, name.as_mut_ptr(), name.len() as i32)
                };
                let name = (length > 0).then(|| String::from_utf16_lossy(&name[..length as usize]));
                formats.push((format, name));
            }
        }

        fn files(&self) -> Option<Vec<PathBuf>> {
            // SAFETY: the clipboard is open, and CF_HDROP's handle is an HDROP.
            let list = unsafe { GetClipboardData(CF_HDROP) };
            if list.is_null() {
                return None;
            }
            // SAFETY: as above; u32::MAX asks for the count.
            let count = unsafe { DragQueryFileW(list, u32::MAX, std::ptr::null_mut(), 0) };
            let files = (0..count)
                .filter_map(|index| {
                    // SAFETY: as above; a null buffer asks for the length.
                    let length =
                        unsafe { DragQueryFileW(list, index, std::ptr::null_mut(), 0) } as usize;
                    let mut path = vec![0u16; length + 1];
                    // SAFETY: the buffer holds the path and its terminating null.
                    let copied = unsafe {
                        DragQueryFileW(list, index, path.as_mut_ptr(), path.len() as u32)
                    } as usize;
                    (copied > 0).then(|| PathBuf::from(OsString::from_wide(&path[..copied])))
                })
                .filter(|path| path.is_file())
                .collect();
            Some(files)
        }

        /// The format's data, read in place while the clipboard stays open.
        fn data(&self, format: u32) -> Option<Locked<'_>> {
            // SAFETY: the clipboard is open.
            let handle = unsafe { GetClipboardData(format) };
            if handle.is_null() {
                return None;
            }
            // SAFETY: the clipboard's data is global memory, which stays
            // while the clipboard is open and the lock is held.
            let start = unsafe { GlobalLock(handle) };
            if start.is_null() {
                return None;
            }
            // SAFETY: as above; the size may be rounded up past the data.
            let length = unsafe { GlobalSize(handle) };
            Some(Locked {
                handle,
                // SAFETY: the lock is held for as long as the slice lives.
                bytes: unsafe { std::slice::from_raw_parts(start.cast(), length) },
            })
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            // SAFETY: Open is made only once the clipboard is open.
            unsafe { CloseClipboard() };
        }
    }

    struct Locked<'clipboard> {
        handle: HANDLE,
        bytes: &'clipboard [u8],
    }

    impl Locked<'_> {
        fn bytes(&self) -> &[u8] {
            self.bytes
        }
    }

    impl Drop for Locked<'_> {
        fn drop(&mut self) {
            // SAFETY: the handle was locked in Open::data.
            unsafe { GlobalUnlock(self.handle) };
        }
    }
}

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
mod tests {
    use super::*;

    /// A `text/uri-list` is X11's and Wayland's, so a path in one is written
    /// as they offer it, as a URI; the desktop's own clipboard names files
    /// another way.
    #[cfg(target_os = "linux")]
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
    }

    #[test]
    fn reads_no_percent_escape_it_cannot_decode() {
        assert_eq!(percent_decode("%zz"), None);
        assert_eq!(percent_decode("%20"), Some(" ".into()));
    }

    /// A `BITMAPV5HEADER` for a 2×2 bitmap, with these masks.
    fn v5_header(bits: u16, compression: u32, masks: [u32; 4], height: i32) -> Vec<u8> {
        let mut header = vec![0u8; 124];
        header[0..4].copy_from_slice(&124u32.to_le_bytes());
        header[4..8].copy_from_slice(&2i32.to_le_bytes());
        header[8..12].copy_from_slice(&height.to_le_bytes());
        header[12..14].copy_from_slice(&1u16.to_le_bytes());
        header[14..16].copy_from_slice(&bits.to_le_bytes());
        header[16..20].copy_from_slice(&compression.to_le_bytes());
        for (at, mask) in (40..).step_by(4).zip(masks) {
            header[at..at + 4].copy_from_slice(&mask.to_le_bytes());
        }
        header
    }

    /// Two rows of two BGRA pixels, the bottom row first.
    const BGRA: [u8; 16] = [
        0, 0, 255, 128, 0, 255, 0, 255, // bottom: red at half alpha, green
        255, 0, 0, 255, 255, 255, 255, 0, // top: blue, white fully transparent
    ];
    const RGBA: [u8; 16] = [
        0, 0, 255, 255, 255, 255, 255, 0, // top
        255, 0, 0, 128, 0, 255, 0, 255, // bottom
    ];

    #[test]
    fn reads_alpha_from_a_32_bit_bitmap_without_masks() {
        // As Windows makes CF_DIBV5 of Chromium's CF_DIB: BI_RGB, with an
        // alpha mask, the size exact. arboard 3.6.1 fails on this one.
        let mut dib = v5_header(32, 0, [0, 0, 0, 0xff00_0000], 2);
        dib.extend(BGRA);
        assert_eq!(bitmap(&dib).unwrap().into_raw(), RGBA);
        // Rounded up past the data, as GlobalSize may report it.
        dib.extend([0; 16]);
        assert_eq!(bitmap(&dib).unwrap().into_raw(), RGBA);

        // A 40-byte header from the top down.
        let mut dib = v5_header(32, 0, [0; 4], -2);
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib.truncate(40);
        dib.extend(BGRA[8..].iter().chain(&BGRA[..8]));
        assert_eq!(bitmap(&dib).unwrap().into_raw(), RGBA);
    }

    #[test]
    fn reads_a_bitmap_with_masks_after_the_header() {
        // Snipping Tool's: BI_BITFIELDS, its masks in the header and again
        // after it.
        let masks = [0xff_0000, 0xff00, 0xff, 0xff00_0000];
        let mut dib = v5_header(32, 3, masks, 2);
        dib.extend(masks[..3].iter().flat_map(|mask| mask.to_le_bytes()));
        dib.extend(BGRA);
        assert_eq!(bitmap(&dib).unwrap().into_raw(), RGBA);
    }

    #[test]
    fn a_bitmap_without_alpha_is_opaque() {
        let mut opaque = BGRA;
        opaque
            .iter_mut()
            .skip(3)
            .step_by(4)
            .for_each(|alpha| *alpha = 0);
        let mut dib = v5_header(32, 0, [0; 4], 2);
        dib.extend(opaque);
        let pixels = bitmap(&dib).unwrap();
        assert!(pixels.pixels().all(|pixel| pixel[3] == 255));

        // 24-bit, rows padded to four bytes.
        let mut dib = v5_header(24, 0, [0; 4], 1);
        dib.extend([0, 0, 255, 0, 255, 0, 0, 0]);
        assert_eq!(
            bitmap(&dib).unwrap().into_raw(),
            [255, 0, 0, 255, 0, 255, 0, 255]
        );
    }

    #[test]
    fn refuses_a_short_bitmap() {
        let mut dib = v5_header(32, 0, [0; 4], 2);
        dib.extend(&BGRA[..12]);
        assert!(bitmap(&dib).is_none());
        assert!(bitmap(&dib[..20]).is_none());
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
