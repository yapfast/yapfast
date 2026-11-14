//! Yapfast's icon on its window. Linux desktops find it from the window's app ID,
//! which names the desktop entry in packaging/linux, which an AppImage installs
//! for itself (`crate::desktop_entry`); Windows shows the executable's icon
//! (build.rs) in Explorer, and the window loads the same one.

use slint::winit_030::winit::window::WindowAttributes;

pub use crate::APP_ID;

/// Gives each window Slint makes the icon.
pub fn attributes(attributes: WindowAttributes) -> WindowAttributes {
    platform::attributes(attributes)
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
