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

/// Gives a new window its title bar icon and the current taskbar icon.
#[cfg(windows)]
pub fn set_on(hwnd: windows_sys::Win32::Foundation::HWND, unread: u8) {
    use std::sync::OnceLock;
    use windows_sys::Win32::UI::WindowsAndMessaging::{ICON_SMALL, SM_CXSMICON};

    static SMALL: OnceLock<isize> = OnceLock::new();
    set(hwnd, ICON_SMALL, *SMALL.get_or_init(|| load(SM_CXSMICON)));
    set_taskbar(hwnd, unread);
}

#[cfg(windows)]
pub fn set_taskbar(hwnd: windows_sys::Win32::Foundation::HWND, unread: u8) {
    use std::sync::OnceLock;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, ICON_BIG, SM_CXICON};

    // The window borrows these handles. Lazily keep one for each label,
    // including after the window closes to the tray and opens again.
    static ICONS: [OnceLock<isize>; 11] = [const { OnceLock::new() }; 11];
    let unread = unread.min(10);
    let icon = *ICONS[usize::from(unread)].get_or_init(|| {
        if unread == 0 {
            return load(SM_CXICON);
        }
        // SAFETY: GetSystemMetrics takes only the metric constant.
        let size = unsafe { GetSystemMetrics(SM_CXICON) };
        crate::tray::taskbar_icon(size as u32, unread) as isize
    });
    let icon = if icon == 0 {
        *ICONS[0].get_or_init(|| load(SM_CXICON))
    } else {
        icon
    };
    set(hwnd, ICON_BIG, icon);
}

#[cfg(windows)]
fn load(metric: i32) -> isize {
    use windows_sys::Win32::{
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{GetSystemMetrics, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW},
    };
    // SAFETY: the resource is an icon in this executable and is addressed
    // by its integer ID.
    unsafe {
        let side = GetSystemMetrics(metric);
        LoadImageW(
            GetModuleHandleW(std::ptr::null()),
            RESOURCE as usize as *const u16,
            IMAGE_ICON,
            side,
            side,
            LR_DEFAULTCOLOR,
        ) as isize
    }
}

#[cfg(windows)]
fn set(hwnd: windows_sys::Win32::Foundation::HWND, which: u32, icon: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SETICON};

    if icon != 0 {
        // SAFETY: the caller holds the live window and the icon is cached
        // for the lifetime of the process.
        unsafe { SendMessageW(hwnd, WM_SETICON, which as usize, icon) };
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
