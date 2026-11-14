//! What Copy puts on the clipboard: a picture's first frame, which arboard
//! offers as a PNG, and text that a person will paste somewhere.
//!
//! On Linux the clipboard holds a picture only while the app that copied it
//! answers for it, so a thread does that until something else is copied.

use std::borrow::Cow;

/// Puts `file`, a picture as it was downloaded, on the clipboard.
pub fn picture(file: Vec<u8>) {
    let copying = std::thread::Builder::new()
        .name("copy-picture".into())
        .spawn(move || {
            if let Err(error) = copy(&file) {
                tracing::warn!(%error, "could not copy a picture");
            }
        });
    if let Err(error) = copying {
        tracing::error!(%error, "could not start copying a picture");
    }
}

/// Puts `text` on the clipboard, from a thread of its own: the clipboard on
/// Linux is answered for by this app while it holds the text.
pub fn text(text: String) {
    let copying = std::thread::Builder::new()
        .name("copy-text".into())
        .spawn(move || {
            if let Err(error) = copy_text(&text) {
                tracing::warn!(%error, "could not copy text");
            }
        });
    if let Err(error) = copying {
        tracing::error!(%error, "could not start copying text");
    }
}

fn copy_text(text: &str) -> anyhow::Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    let set = clipboard.set();
    #[cfg(target_os = "linux")]
    let set = {
        use arboard::SetExtLinux;
        // Answers for the text until another copy takes its place.
        set.wait()
    };
    set.text(text.to_owned())?;
    Ok(())
}

fn copy(file: &[u8]) -> anyhow::Result<()> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(file)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let picture = reader.decode()?.into_rgba8();
    let image = arboard::ImageData {
        width: picture.width() as usize,
        height: picture.height() as usize,
        bytes: Cow::Owned(picture.into_raw()),
    };
    let mut clipboard = arboard::Clipboard::new()?;
    let set = clipboard.set();
    #[cfg(target_os = "linux")]
    let set = {
        use arboard::SetExtLinux;
        // Answers for the picture until another copy takes its place.
        set.wait()
    };
    set.image(image)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Copies a picture to the desktop's clipboard, for checking by hand:
    /// paste it elsewhere while the test waits.
    #[test]
    #[ignore = "writes the desktop clipboard"]
    fn copies_a_picture_to_the_desktop_clipboard() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(64, 48, image::Rgba([255, 128, 0, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        super::copy(&png.into_inner()).unwrap();
    }
}
