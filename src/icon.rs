//! Desktop icon artwork, imported icons, and pictures drawn from SVG.

use std::{
    io::{Cursor, Read, Write},
    path::Path,
};

use image::{ImageFormat, ImageReader, RgbaImage, imageops};
use yapfast_core::theme::{self, Theme};
use yapfast_iced::{IconChoice, IconTarget};

pub use crate::APP_ID;

/// A picture as straight (not premultiplied) RGBA, four bytes a pixel.
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

const ICON_SIZE: u32 = 256;
const MAX_FILE: u64 = 8 * 1024 * 1024;
const MAX_SIDE: u32 = 4096;

fn filename(target: IconTarget) -> &'static str {
    match target {
        IconTarget::Application => "application-icon.png",
        IconTarget::Tray => "tray-icon.png",
    }
}

/// Reads a supported image and fits it on a square canvas.
pub fn prepare(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_FILE + 1).read_to_end(&mut bytes))
        .map_err(|_| "Could not read the chosen image.")?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("Choose an image no larger than 8 MiB.".into());
    }
    let invalid = "Choose a readable PNG, ICO, JPEG or WebP image.";
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| invalid)?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Ico | ImageFormat::Jpeg | ImageFormat::WebP)
    ) {
        return Err(invalid.into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|err| match err {
        image::ImageError::Limits(_) => "Choose an image up to 4096 × 4096 pixels.",
        _ => invalid,
    })?;
    let mut resized = if (decoded.width(), decoded.height()) == (ICON_SIZE, ICON_SIZE) {
        decoded.into_rgba8()
    } else {
        decoded
            .resize(ICON_SIZE, ICON_SIZE, imageops::FilterType::Lanczos3)
            .into_rgba8()
    };
    if resized.dimensions() != (ICON_SIZE, ICON_SIZE) {
        let mut square = RgbaImage::new(ICON_SIZE, ICON_SIZE);
        imageops::replace(
            &mut square,
            &resized,
            i64::from((ICON_SIZE - resized.width()) / 2),
            i64::from((ICON_SIZE - resized.height()) / 2),
        );
        resized = square;
    }
    let mut png = Cursor::new(Vec::new());
    resized
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|_| "Could not prepare the icon.")?;
    Ok(png.into_inner())
}

/// Replaces the saved copy only once the new file is complete.
pub fn save(root: &Path, target: IconTarget, png: &[u8]) -> Result<(), String> {
    let save = "Could not save the icon in Yapfast’s data folder.";
    std::fs::create_dir_all(root).map_err(|_| save)?;
    let mut file = tempfile::NamedTempFile::new_in(root).map_err(|_| save)?;
    file.write_all(png).map_err(|_| save)?;
    file.persist(root.join(filename(target)))
        .map_err(|_| save)?;
    Ok(())
}

pub enum Artwork {
    Default,
    Theme(&'static Theme),
    Custom(RgbaImage),
}

impl Artwork {
    pub fn load(root: &Path, target: IconTarget, choice: &IconChoice) -> Self {
        match choice {
            IconChoice::Default => Self::Default,
            IconChoice::Theme(id) => {
                theme::find(id).map_or(Self::Default, |preset| Self::Theme(&preset.theme))
            }
            IconChoice::Custom => {
                let path = root.join(filename(target));
                let result = ImageReader::open(path).ok().and_then(|mut reader| {
                    let mut limits = image::Limits::default();
                    limits.max_image_width = Some(ICON_SIZE);
                    limits.max_image_height = Some(ICON_SIZE);
                    reader.limits(limits);
                    reader.decode().ok()
                });
                result.map_or(Self::Default, |image| Self::Custom(image.into_rgba8()))
            }
        }
    }

    #[cfg(any(target_os = "linux", windows))]
    pub fn window(&self, size: u32) -> Option<Rgba> {
        let svg = include_str!("../assets/icons/app/icon.svg");
        match self {
            Self::Default => rasterize(svg.as_bytes(), size),
            Self::Theme(theme) => {
                let svg = svg
                    .replace("#ffbf47", &format!("#{:06x}", theme.accent.rgb_part()))
                    .replace("#1d1607", &format!("#{:06x}", theme.on_accent.rgb_part()));
                rasterize(svg.as_bytes(), size)
            }
            Self::Custom(image) => Some(rgba(scale(image, size))),
        }
    }

    pub fn tray(&self, size: u32, unread: bool) -> Option<Rgba> {
        let accent = match self {
            Self::Default => 0xffbf47,
            Self::Theme(theme) => theme.accent.rgb_part(),
            Self::Custom(image) => {
                let mut pixels = scale(image, size);
                if unread {
                    let badge = rasterize(
                        br##"<svg xmlns="http://www.w3.org/2000/svg" width="78" height="78">
                            <circle cx="61" cy="61" r="16" fill="#202126"/>
                            <circle cx="61" cy="61" r="12" fill="#f0545e"/>
                        </svg>"##,
                        size,
                    )?;
                    let badge = RgbaImage::from_raw(badge.width, badge.height, badge.pixels)?;
                    imageops::overlay(&mut pixels, &badge, 0, 0);
                }
                return Some(rgba(pixels));
            }
        };
        let mut svg = include_str!("../assets/icons/ui/logo.svg").replacen(
            r##"fill="#fff" mask"##,
            &format!("fill=\"#{accent:06x}\" mask"),
            1,
        );
        if unread {
            svg = svg
                .replacen("</mask>", r#"<circle cx="74" cy="74" r="21" /></mask>"#, 1)
                .replacen(
                    "</svg>",
                    r##"<circle cx="74" cy="74" r="15" fill="#f0545e" /></svg>"##,
                    1,
                );
        }
        rasterize(svg.as_bytes(), size)
    }
}

fn scale(image: &RgbaImage, size: u32) -> RgbaImage {
    imageops::resize(image, size, size, imageops::FilterType::Lanczos3)
}

fn rgba(image: RgbaImage) -> Rgba {
    let (width, height) = image.dimensions();
    Rgba {
        width,
        height,
        pixels: image.into_raw(),
    }
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

/// The icon in packaging/windows/yapfast.rc.
#[cfg(windows)]
const RESOURCE: u16 = 1;

#[cfg(windows)]
pub(crate) struct NativeIcon(pub windows_sys::Win32::UI::WindowsAndMessaging::HICON);

#[cfg(windows)]
impl NativeIcon {
    pub fn from_rgba(mut image: Rgba) -> Option<Self> {
        use windows_sys::Win32::{
            System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::CreateIcon,
        };
        for pixel in image.pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        // Rows of the 1-bit mask are padded to 16 bits.
        let mask = vec![0u8; image.width.div_ceil(16) as usize * 2 * image.height as usize];
        // SAFETY: both buffers hold the requested number of rows and live across the call.
        let handle = unsafe {
            CreateIcon(
                GetModuleHandleW(std::ptr::null()),
                image.width as i32,
                image.height as i32,
                1,
                32,
                mask.as_ptr(),
                image.pixels.as_ptr(),
            )
        };
        (!handle.is_null()).then(|| Self(handle))
    }

    fn from_resource(size: i32) -> Option<Self> {
        use windows_sys::Win32::{
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW},
        };
        // SAFETY: RESOURCE identifies the icon in this executable's resource table.
        let handle = unsafe {
            LoadImageW(
                GetModuleHandleW(std::ptr::null()),
                RESOURCE as usize as *const u16,
                IMAGE_ICON,
                size,
                size,
                LR_DEFAULTCOLOR,
            )
        };
        (!handle.is_null()).then(|| Self(handle))
    }
}

#[cfg(windows)]
impl Drop for NativeIcon {
    fn drop(&mut self) {
        // SAFETY: this icon is owned here and no longer in use.
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(self.0) };
    }
}

#[cfg(windows)]
type WindowIcons = [NativeIcon; 2];
#[cfg(windows)]
const ICON_PROPERTY: windows_sys::core::PCWSTR = windows_sys::w!("YapfastWindowIcons");

/// Gives the running window its own icons at the Windows title bar and
/// taskbar sizes. The window owns both until they are replaced or it closes.
#[cfg(windows)]
pub fn set_on(window: windows_sys::Win32::Foundation::HWND, artwork: &Artwork, unread: u8) {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        UI::{
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                GetPropW, GetSystemMetrics, ICON_SMALL, RemovePropW, SM_CXICON, SM_CXSMICON,
                SendMessageW, SetPropW, WM_NCDESTROY, WM_SETICON,
            },
        },
    };

    const SUBCLASS_ID: usize = 0;

    unsafe extern "system" fn dispose(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        id: usize,
        _: usize,
    ) -> LRESULT {
        // SAFETY: the subclass runs on the window's thread. On destruction
        // its property owns the box installed by set_on.
        unsafe {
            if message == WM_NCDESTROY {
                RemoveWindowSubclass(window, Some(dispose), id);
                let icons = RemovePropW(window, ICON_PROPERTY).cast::<WindowIcons>();
                let result = DefSubclassProc(window, message, wparam, lparam);
                if !icons.is_null() {
                    drop(Box::from_raw(icons));
                }
                return result;
            }
            DefSubclassProc(window, message, wparam, lparam)
        }
    }

    let load = |metric| {
        // SAFETY: the metric has no pointer arguments and is a system icon size.
        let size = unsafe { GetSystemMetrics(metric) }.clamp(16, ICON_SIZE as i32);
        if matches!(artwork, Artwork::Default)
            && let Some(icon) = NativeIcon::from_resource(size)
        {
            return Some(icon);
        }
        NativeIcon::from_rgba(artwork.window(size as u32)?)
    };
    let (Some(small), Some(big)) = (load(SM_CXSMICON), load(SM_CXICON)) else {
        return;
    };
    let small_handle = small.0;
    let icons = Box::into_raw(Box::new([small, big]));
    // SAFETY: the caller owns the live window on this thread. The subclass
    // releases its icon property on destruction. Only the previous box can
    // be released after replacing the icons.
    unsafe {
        let previous = GetPropW(window, ICON_PROPERTY).cast::<WindowIcons>();
        if SetWindowSubclass(window, Some(dispose), SUBCLASS_ID, 0) == 0
            || SetPropW(window, ICON_PROPERTY, icons.cast()) == 0
        {
            drop(Box::from_raw(icons));
            return;
        }
        SendMessageW(
            window,
            WM_SETICON,
            ICON_SMALL as usize,
            small_handle as isize,
        );
        set_taskbar(window, unread);
        if !previous.is_null() {
            drop(Box::from_raw(previous));
        }
    }
}

#[cfg(windows)]
pub fn set_taskbar(window: windows_sys::Win32::Foundation::HWND, unread: u8) {
    use std::{mem::ManuallyDrop, sync::OnceLock};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetPropW, GetSystemMetrics, ICON_BIG, SM_CXICON, SendMessageW, WM_SETICON,
    };

    static BADGES: [OnceLock<isize>; 10] = [const { OnceLock::new() }; 10];
    // SAFETY: calls for this window run on its owning thread. Its icon
    // property, when present, is the box installed by set_on.
    let base = unsafe {
        GetPropW(window, ICON_PROPERTY)
            .cast::<WindowIcons>()
            .as_ref()
    }
    .map_or(0, |icons| icons[1].0 as isize);
    let icon = if unread == 0 {
        base
    } else {
        let badge = *BADGES[usize::from(unread.min(10) - 1)].get_or_init(|| {
            // SAFETY: the metric has no pointer arguments.
            let size = unsafe { GetSystemMetrics(SM_CXICON) }.clamp(16, ICON_SIZE as i32);
            crate::tray::taskbar_badge(size as u32, unread)
                .and_then(NativeIcon::from_rgba)
                .map_or(0, |icon| ManuallyDrop::new(icon).0 as isize)
        });
        if badge == 0 { base } else { badge }
    };
    if icon != 0 {
        // SAFETY: the window is live and the icon is owned by its property
        // or held in the process-lifetime badge cache.
        unsafe { SendMessageW(window, WM_SETICON, ICON_BIG as usize, icon) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn the_window_icon_is_drawn_square() {
        let icon = Artwork::Default.window(64).unwrap();
        assert_eq!((icon.width, icon.height), (64, 64));
        assert!(
            icon.pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] == 255)
        );
    }

    #[test]
    fn a_custom_icon_is_kept_independently_and_the_tray_badges_it() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let source = root.join("source.png");
        let green = image::Rgba([40, 180, 60, 255]);
        RgbaImage::from_pixel(64, 32, green).save(&source).unwrap();
        let prepared = prepare(&source).unwrap();
        save(root, IconTarget::Tray, &prepared).unwrap();
        std::fs::remove_file(source).unwrap();

        let artwork = Artwork::load(root, IconTarget::Tray, &IconChoice::Custom);
        let Artwork::Custom(image) = &artwork else {
            panic!("the saved tray icon should survive removal of its source");
        };
        assert_eq!(image.dimensions(), (ICON_SIZE, ICON_SIZE));
        assert_eq!(image.get_pixel(128, 128), &green);
        assert_eq!(image.get_pixel(128, 0)[3], 0);
        assert!(matches!(
            Artwork::load(root, IconTarget::Application, &IconChoice::Custom),
            Artwork::Default
        ));

        let red = |pixel: &[u8]| pixel == [0xf0, 0x54, 0x5e, 255];
        assert!(
            !artwork
                .tray(32, false)
                .unwrap()
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| red(pixel))
        );
        assert!(
            artwork
                .tray(32, true)
                .unwrap()
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| red(pixel))
        );
    }
}
