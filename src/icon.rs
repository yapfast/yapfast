//! Yapfast's icon on its window, and its pictures drawn from SVG. Linux
//! desktops find the icon from the window's app ID, which names the desktop
//! entry in packaging/linux, which an AppImage installs for itself
//! (`crate::desktop_entry`); X11 shows the window's own where the entry
//! isn't installed. Windows shows the executable's icon (build.rs) in
//! Explorer, and the window loads the same one. Each window toolkit gives
//! its windows these as it makes them.

pub use crate::APP_ID;

/// A picture as straight (not premultiplied) RGBA, four bytes a pixel.
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// An SVG drawn `size` pixels across its wider side, with resvg.
pub fn rasterize(svg: &[u8], size: u32) -> Option<Rgba> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let view = tree.size();
    let scale = size as f32 / view.width().max(view.height()).max(1.0);
    let width = (view.width() * scale).round().max(1.0) as u32;
    let height = (view.height() * scale).round().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let pixels = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let pixel = pixel.demultiply();
            [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
        })
        .collect();
    Some(Rgba {
        width,
        height,
        pixels,
    })
}

/// The window's icon on Linux, at the size packaging/icon.svg is drawn at.
#[cfg(target_os = "linux")]
pub fn window_icon() -> Option<Rgba> {
    rasterize(include_bytes!("../packaging/icon.svg"), 64)
}

/// The icon in packaging/windows/yapfast.rc.
#[cfg(windows)]
pub const RESOURCE: u16 = 1;

/// Gives a window made without an icon the title bar's small icon and the
/// taskbar's large one from the executable, each loaded at the size Windows
/// draws it rather than scaled from one picture.
#[cfg(windows)]
pub fn set_on(hwnd: isize) {
    use windows_sys::Win32::{
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            GetSystemMetrics, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW,
            SM_CXICON, SM_CXSMICON, SendMessageW, WM_SETICON,
        },
    };
    // SAFETY: the window is the caller's, alive across the calls; the icons
    // are loaded from this executable's own resources and stay with it.
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        for (which, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
            let side = GetSystemMetrics(metric);
            let icon = LoadImageW(
                module,
                RESOURCE as usize as *const u16,
                IMAGE_ICON,
                side,
                side,
                LR_DEFAULTCOLOR,
            );
            if !icon.is_null() {
                SendMessageW(hwnd as _, WM_SETICON, which as usize, icon as isize);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    #[test]
    fn the_window_icon_is_drawn_square() {
        let icon = super::window_icon().unwrap();
        assert_eq!((icon.width, icon.height), (64, 64));
        assert!(icon.pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
    }
}
