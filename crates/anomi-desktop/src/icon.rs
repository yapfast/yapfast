//! Anomi's icon and the name the desktop knows its windows by. Linux desktops
//! find the icon from the window's app ID, which names the desktop entry in
//! packaging/linux, whose `Name=` is Anomi; X11 also shows the window's own
//! icon where the entry isn't installed. Windows shows the executable's icon
//! (build.rs).

/// The window's app ID on Linux (its X11 class and Wayland app ID), and the
/// name of its desktop entry and icon.
pub const APP_ID: &str = "io.github.yapfast.Yapfast";

/// The icon, 256 pixels square: Anomi's picture in its frame. A static, so
/// the window and the desktop entry share one copy.
pub static PNG: &[u8] = include_bytes!("../../../packaging/icon.png");

/// The icon's pixels as RGBA, with its width and height, for a window that
/// carries its own (X11). Decoded as each window opens; nothing keeps it.
pub fn rgba() -> Option<(Vec<u8>, u32, u32)> {
    let picture = image::load_from_memory_with_format(PNG, image::ImageFormat::Png).ok()?;
    let picture = picture.into_rgba8();
    let (width, height) = picture.dimensions();
    Some((picture.into_raw(), width, height))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_icon_reads_as_a_square() {
        let (pixels, width, height) = super::rgba().unwrap();
        assert_eq!((width, height), (256, 256));
        assert_eq!(pixels.len(), 256 * 256 * 4);
    }
}
