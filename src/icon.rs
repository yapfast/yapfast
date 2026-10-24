//! Yapfast's icon on its window. Linux desktops find it from the window's app ID,
//! which names the desktop entry in packaging/linux, which an AppImage installs
//! for itself (`crate::desktop_entry`); Windows shows the executable's icon
//! (build.rs) in Explorer, and the window loads the same one.

use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::window::WindowAttributes;
use std::io::Cursor;

/// The window's app ID on Linux, and the name of its desktop entry and icon.
pub const APP_ID: &str = "io.github.yapfast.Yapfast";

/// Gives each window Slint makes the icon.
pub fn attributes(attributes: WindowAttributes) -> WindowAttributes {
    platform::attributes(attributes)
}

/// Changes the running window's icon to one of the available Yapfast palettes.
pub fn set_style(window: &slint::Window, style: i32) {
    let colour = match style {
        1 => "#748bff",
        2 => "#54d68b",
        _ => "#ffbf47",
    };
    let svg = include_str!("../packaging/icon.svg").replace("#ffbf47", colour);
    let icon = slint::Image::load_from_svg_data(svg.as_bytes())
        .ok()
        .and_then(|image| image.to_rgba8())
        .and_then(|pixels| {
            slint::winit_030::winit::window::Icon::from_rgba(
                pixels.as_bytes().to_vec(),
                pixels.width(),
                pixels.height(),
            )
            .ok()
        });
    window.with_winit_window(|winit| winit.set_window_icon(icon));
}

/// Decodes an icon file into a centered 64×64 RGBA image, rejecting oversized
/// files and dimensions before allocating the decoded bitmap.
pub fn decode_custom(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "icon file is larger than 16 MB"
    );
    let reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let (width, height) = reader.into_dimensions()?;
    anyhow::ensure!(
        width > 0
            && height > 0
            && width <= 4096
            && height <= 4096
            && u64::from(width) * u64::from(height) <= 16_777_216,
        "icon dimensions must be 4096×4096 or smaller"
    );
    let source = image::load_from_memory(bytes)?.to_rgba8();
    let scaled = image::imageops::thumbnail(&source, 64, 64);
    let (width, height) = scaled.dimensions();
    let mut square = image::RgbaImage::new(64, 64);
    image::imageops::overlay(
        &mut square,
        &scaled,
        i64::from((64 - width) / 2),
        i64::from((64 - height) / 2),
    );
    Ok(square.into_raw())
}

pub fn encode_custom_png(rgba: &[u8]) -> anyhow::Result<Vec<u8>> {
    let image = image::RgbaImage::from_raw(64, 64, rgba.to_vec())
        .ok_or_else(|| anyhow::anyhow!("invalid custom icon pixels"))?;
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut png, image::ImageFormat::Png)?;
    Ok(png.into_inner())
}

pub fn set_custom(window: &slint::Window, rgba: &[u8]) {
    let icon = slint::winit_030::winit::window::Icon::from_rgba(rgba.to_vec(), 64, 64).ok();
    window.with_winit_window(|winit| winit.set_window_icon(icon));
}

#[cfg(target_os = "linux")]
mod platform {
    use slint::winit_030::winit::window::{Icon, WindowAttributes};

    /// X11 shows the window's own icon where the desktop entry isn't
    /// installed; Wayland takes the icon only from the entry, and ignores it.
    pub fn attributes(attributes: WindowAttributes) -> WindowAttributes {
        let icon = slint::Image::load_from_svg_data(include_bytes!("../packaging/icon.svg"))
            .ok()
            .and_then(|image| image.to_rgba8())
            .and_then(|pixels| {
                Icon::from_rgba(pixels.as_bytes().to_vec(), pixels.width(), pixels.height()).ok()
            });
        attributes.with_window_icon(icon)
    }
}

#[cfg(windows)]
mod platform {
    use slint::winit_030::winit::{
        dpi::PhysicalSize,
        platform::windows::{IconExtWindows, WindowAttributesExtWindows},
        window::{Icon, WindowAttributes},
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXICON, SM_CXSMICON, SYSTEM_METRICS_INDEX,
    };

    /// The icon in packaging/windows/yapfast.rc.
    const ICON: u16 = 1;

    /// The title bar's small icon and the taskbar's large one, each loaded at
    /// the size Windows draws it rather than scaled from one picture.
    pub fn attributes(attributes: WindowAttributes) -> WindowAttributes {
        let icon = |metric: SYSTEM_METRICS_INDEX| {
            // SAFETY: GetSystemMetrics only reads a setting.
            let side = unsafe { GetSystemMetrics(metric) } as u32;
            Icon::from_resource(ICON, Some(PhysicalSize::new(side, side))).ok()
        };
        attributes
            .with_window_icon(icon(SM_CXSMICON))
            .with_taskbar_icon(icon(SM_CXICON))
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    pub fn attributes(
        attributes: slint::winit_030::winit::window::WindowAttributes,
    ) -> slint::winit_030::winit::window::WindowAttributes {
        attributes
    }
}
