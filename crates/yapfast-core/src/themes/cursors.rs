//! A theme's mouse cursors: `"cursors"` gives any of the window's cursor
//! kinds a picture of its own, as a PNG in the theme's folder with the point
//! that clicks, its hotspot. A kind the theme leaves out keeps the system's
//! cursor, and so does every kind while the theme's cursors are off or a
//! protected dialog is open.
//!
//! A cursor may list the same picture at several sizes, smallest first, for
//! screens scaled above 100%: the window shows the smallest at least as large
//! as the first one times its scale. Each picture is decoded here, under
//! limits, before anything shows it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::package::{Error, relative};

macro_rules! bail {
    ($($arg:tt)*) => { return Err(Error(format!($($arg)*))) };
}

/// The largest cursor picture, in pixels on either side: what Windows, X11
/// and Wayland compositors all show whole.
pub const MAX_SIDE: u32 = 128;
/// The largest cursor file.
pub const MAX_BYTES: usize = 256 << 10;
/// The most sizes one cursor lists.
pub const MAX_SIZES: usize = 4;

/// A kind of mouse cursor, by its CSS name, as the window's items ask for
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Default,
    Pointer,
    Text,
    Grab,
    Grabbing,
    NotAllowed,
    NoDrop,
    Wait,
    Progress,
    Help,
    Crosshair,
    Move,
    Copy,
    Alias,
    ColResize,
    RowResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    NResize,
    EResize,
    SResize,
    WResize,
    NeResize,
    NwResize,
    SeResize,
    SwResize,
}

impl Kind {
    pub const ALL: [Self; 28] = [
        Self::Default,
        Self::Pointer,
        Self::Text,
        Self::Grab,
        Self::Grabbing,
        Self::NotAllowed,
        Self::NoDrop,
        Self::Wait,
        Self::Progress,
        Self::Help,
        Self::Crosshair,
        Self::Move,
        Self::Copy,
        Self::Alias,
        Self::ColResize,
        Self::RowResize,
        Self::EwResize,
        Self::NsResize,
        Self::NeswResize,
        Self::NwseResize,
        Self::NResize,
        Self::EResize,
        Self::SResize,
        Self::WResize,
        Self::NeResize,
        Self::NwResize,
        Self::SeResize,
        Self::SwResize,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Pointer => "pointer",
            Self::Text => "text",
            Self::Grab => "grab",
            Self::Grabbing => "grabbing",
            Self::NotAllowed => "not-allowed",
            Self::NoDrop => "no-drop",
            Self::Wait => "wait",
            Self::Progress => "progress",
            Self::Help => "help",
            Self::Crosshair => "crosshair",
            Self::Move => "move",
            Self::Copy => "copy",
            Self::Alias => "alias",
            Self::ColResize => "col-resize",
            Self::RowResize => "row-resize",
            Self::EwResize => "ew-resize",
            Self::NsResize => "ns-resize",
            Self::NeswResize => "nesw-resize",
            Self::NwseResize => "nwse-resize",
            Self::NResize => "n-resize",
            Self::EResize => "e-resize",
            Self::SResize => "s-resize",
            Self::WResize => "w-resize",
            Self::NeResize => "ne-resize",
            Self::NwResize => "nw-resize",
            Self::SeResize => "se-resize",
            Self::SwResize => "sw-resize",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// One kind's cursor: its picture at each size, smallest first, and the
/// hotspot in the first one's pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub images: Vec<PathBuf>,
    pub hotspot: (u32, u32),
}

/// Reads a layer's `"cursors"` into `cursors`, over what an earlier layer
/// set; `null` gives a kind the system's cursor again.
pub(super) fn read_cursors(
    value: &Value,
    place: &str,
    cursors: &mut BTreeMap<Kind, Cursor>,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    let Value::Object(entries) = value else {
        bail!("\"{place}\" has to be an object");
    };
    for (name, written) in entries {
        let at = format!("{place}.{name}");
        let Some(kind) = Kind::named(name) else {
            notes.push(format!("\"{at}\" isn't a kind of cursor; ignored"));
            continue;
        };
        let fields = match written {
            Value::Null => {
                cursors.remove(&kind);
                continue;
            }
            Value::Object(fields) => fields,
            _ => bail!(
                "\"{at}\" has to be an object such as {{ \"image\": \"cursors/{name}.png\", \"hotspot\": [4, 2] }}"
            ),
        };
        let png = |value: &Value, here: &str| -> Result<PathBuf, Error> {
            let text = value
                .as_str()
                .ok_or_else(|| Error(format!("\"{here}\" names a PNG file in the theme")))?;
            let path = relative(text, here)?;
            let is_png = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
            if !is_png {
                bail!("\"{here}\" names \"{text}\": cursors are .png files");
            }
            Ok(path)
        };
        let mut images = Vec::new();
        let mut hotspot = None;
        for (key, value) in fields {
            let here = format!("{at}.{key}");
            match key.as_str() {
                "image" => {
                    images = match value {
                        Value::Array(sizes) => {
                            if sizes.is_empty() || sizes.len() > MAX_SIZES {
                                bail!("\"{here}\" lists 1 to {MAX_SIZES} sizes of the picture");
                            }
                            sizes
                                .iter()
                                .enumerate()
                                .map(|(index, size)| png(size, &format!("{here}[{index}]")))
                                .collect::<Result<_, _>>()?
                        }
                        _ => vec![png(value, &here)?],
                    }
                }
                "hotspot" => {
                    let point = value.as_array().and_then(|point| match point.as_slice() {
                        [x, y] => Some((x.as_u64()?, y.as_u64()?)),
                        _ => None,
                    });
                    let Some((x, y)) = point else {
                        bail!(
                            "\"{here}\" is the point that clicks, as [x, y] in pixels from the picture's top left"
                        );
                    };
                    if x >= u64::from(MAX_SIDE) || y >= u64::from(MAX_SIDE) {
                        bail!("\"{here}\" lies outside a {MAX_SIDE}-pixel picture");
                    }
                    hotspot = Some((x as u32, y as u32));
                }
                _ => notes.push(format!("\"{here}\" isn't something a cursor has; ignored")),
            }
        }
        if images.is_empty() {
            bail!("\"{at}\" needs an \"image\"");
        }
        cursors.insert(
            kind,
            Cursor {
                images,
                hotspot: hotspot.unwrap_or((0, 0)),
            },
        );
    }
    Ok(())
}

/// A cursor picture's pixels, straight RGBA, row by row.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Decodes a cursor picture, refusing anything but a PNG of at most
/// [`MAX_SIDE`] pixels a side: the one loader both the check and the window
/// use.
pub fn decode(bytes: &[u8]) -> Result<Picture, String> {
    if bytes.len() > MAX_BYTES {
        return Err("a cursor is at most 256 KiB".into());
    }
    if image::guess_format(bytes).ok() != Some(image::ImageFormat::Png) {
        return Err("isn't a PNG: cursors are PNG pictures".into());
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(1 << 20);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| format!("doesn't decode, or is larger than {MAX_SIDE} pixels on a side"))?;
    let rgba = decoded.into_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return Err("is empty".into());
    }
    Ok(Picture {
        width,
        height,
        rgba: rgba.into_raw(),
    })
}

/// The picture's size without decoding it, if it's a PNG.
fn size(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png)
        .into_dimensions()
        .ok()
}

/// Why a cursor can't be shown, each as a sentence: its pictures don't
/// decode, its sizes don't grow or keep their shape, or the hotspot isn't on
/// the first. `bytes` gives a file of the theme.
pub fn problems<'a>(
    kind: Kind,
    cursor: &Cursor,
    bytes: impl Fn(&Path) -> Option<&'a [u8]>,
) -> Vec<String> {
    let mut problems = Vec::new();
    let mut sizes = Vec::new();
    for image in &cursor.images {
        let Some(file) = bytes(image) else {
            continue;
        };
        match decode(file) {
            Ok(picture) => sizes.push((picture.width, picture.height)),
            Err(error) => {
                problems.push(format!("{}: {error}", image.display()));
                return problems;
            }
        }
    }
    let Some(&(width, height)) = sizes.first() else {
        return problems;
    };
    let (x, y) = cursor.hotspot;
    if x >= width || y >= height {
        problems.push(format!(
            "the {} cursor's hotspot [{x}, {y}] isn't on its {width}×{height} picture",
            kind.name()
        ));
    }
    for pair in sizes.windows(2) {
        let ((small_w, small_h), (large_w, large_h)) = (pair[0], pair[1]);
        let scale = f64::from(large_w) / f64::from(small_w);
        let shape = (f64::from(small_h) * scale - f64::from(large_h)).abs() <= scale.max(1.0);
        if large_w <= small_w || !shape {
            problems.push(format!(
                "the {} cursor's sizes go smallest first, each the same shape as the first",
                kind.name()
            ));
            break;
        }
    }
    problems
}

/// What a theme's cursors hold in memory, decoded: every size of each.
pub fn memory<'a>(
    cursors: &BTreeMap<Kind, Cursor>,
    bytes: impl Fn(&Path) -> Option<&'a [u8]>,
) -> u64 {
    let mut seen: Vec<&Path> = Vec::new();
    let mut total = 0;
    for image in cursors.values().flat_map(|cursor| &cursor.images) {
        if seen.contains(&image.as_path()) {
            continue;
        }
        seen.push(image);
        if let Some((width, height)) = bytes(image).and_then(size) {
            total += u64::from(width) * u64::from(height) * 4;
        }
    }
    total
}

/// Which of a cursor's pictures shows at `scale`: the smallest at least as
/// wide as the first times the scale, or the largest. `widths` are the
/// pictures', in order.
pub fn pick(widths: &[u32], scale: f32) -> usize {
    let Some(&first) = widths.first() else {
        return 0;
    };
    let wanted = (f64::from(first) * f64::from(scale.max(1.0))).round() as u32;
    widths
        .iter()
        .position(|&width| width >= wanted)
        .unwrap_or(widths.len().saturating_sub(1))
}

/// The hotspot, given on the picture's `first` size, on its `shown` size,
/// kept on the picture.
pub fn scaled_hotspot(hotspot: (u32, u32), first: (u32, u32), shown: (u32, u32)) -> (u32, u32) {
    let scale = |at: u32, from: u32, to: u32| {
        if from == 0 {
            return 0;
        }
        ((u64::from(at) * u64::from(to) / u64::from(from)) as u32).min(to.saturating_sub(1))
    };
    (
        scale(hotspot.0, first.0, shown.0),
        scale(hotspot.1, first.1, shown.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn reads_one_picture_or_sizes_and_null_takes_a_kind_back() {
        let mut cursors = BTreeMap::new();
        let mut notes = Vec::new();
        let value = serde_json::json!({
            "default": { "image": "cursors/arrow.png", "hotspot": [1, 2] },
            "pointer": { "image": ["cursors/hand.png", "cursors/hand@2x.png"], "hotspot": [9, 3] },
            "sparkle": { "image": "cursors/x.png" },
        });
        read_cursors(&value, "cursors", &mut cursors, &mut notes).unwrap();
        assert_eq!(cursors.len(), 2);
        assert_eq!(cursors[&Kind::Pointer].images.len(), 2);
        assert_eq!(cursors[&Kind::Default].hotspot, (1, 2));
        assert_eq!(
            notes,
            ["\"cursors.sparkle\" isn't a kind of cursor; ignored"]
        );
        read_cursors(
            &serde_json::json!({ "default": null }),
            "cursors",
            &mut cursors,
            &mut notes,
        )
        .unwrap();
        assert!(!cursors.contains_key(&Kind::Default));
    }

    #[test]
    fn refuses_links_other_formats_and_bad_hotspots() {
        let mut cursors = BTreeMap::new();
        let mut notes = Vec::new();
        for (value, why) in [
            (
                serde_json::json!({ "text": { "image": "https://x.test/a.png" } }),
                "bundles",
            ),
            (
                serde_json::json!({ "text": { "image": "cursors/a.svg" } }),
                "cursors are .png",
            ),
            (
                serde_json::json!({ "text": { "image": "a.png", "hotspot": [1] } }),
                "point that clicks",
            ),
            (
                serde_json::json!({ "text": { "image": "a.png", "hotspot": [400, 1] } }),
                "outside",
            ),
            (
                serde_json::json!({ "text": { "hotspot": [1, 1] } }),
                "needs an",
            ),
            (
                serde_json::json!({ "text": { "image": ["a.png", "b.png", "c.png", "d.png", "e.png"] } }),
                "1 to 4",
            ),
        ] {
            let error = read_cursors(&value, "cursors", &mut cursors, &mut notes).unwrap_err();
            assert!(error.0.contains(why), "{error} lacks {why}");
        }
    }

    #[test]
    fn decode_takes_small_pngs_only() {
        assert_eq!(decode(&png(32, 32)).unwrap().rgba.len(), 32 * 32 * 4);
        assert!(decode(&png(129, 16)).is_err());
        assert!(decode(b"GIF89a").is_err());
    }

    #[test]
    fn problems_check_hotspot_and_sizes() {
        let small = png(32, 32);
        let large = png(64, 64);
        let squat = png(64, 20);
        let files = |name: &str| -> Option<&[u8]> {
            match name {
                "s.png" => Some(&small),
                "l.png" => Some(&large),
                "q.png" => Some(&squat),
                _ => None,
            }
        };
        let cursor = |images: &[&str], hotspot| Cursor {
            images: images.iter().map(PathBuf::from).collect(),
            hotspot,
        };
        let get = |path: &Path| files(path.to_str().unwrap());
        assert!(problems(Kind::Pointer, &cursor(&["s.png", "l.png"], (4, 4)), get).is_empty());
        assert!(problems(Kind::Pointer, &cursor(&["s.png"], (40, 4)), get)[0].contains("hotspot"));
        assert!(
            problems(Kind::Pointer, &cursor(&["l.png", "s.png"], (4, 4)), get)[0]
                .contains("smallest first")
        );
        assert!(
            problems(Kind::Pointer, &cursor(&["s.png", "q.png"], (4, 4)), get)[0]
                .contains("same shape")
        );
        let mut all = BTreeMap::new();
        all.insert(Kind::Pointer, cursor(&["s.png", "l.png"], (0, 0)));
        all.insert(Kind::Default, cursor(&["s.png"], (0, 0)));
        assert_eq!(memory(&all, get), (32 * 32 + 64 * 64) * 4);
    }

    #[test]
    fn picks_the_size_for_the_scale() {
        assert_eq!(pick(&[32, 48, 64], 1.0), 0);
        assert_eq!(pick(&[32, 48, 64], 1.25), 1);
        assert_eq!(pick(&[32, 48, 64], 2.0), 2);
        assert_eq!(pick(&[32, 48, 64], 3.0), 2);
        assert_eq!(pick(&[32], 2.0), 0);
        assert_eq!(scaled_hotspot((9, 3), (32, 32), (64, 64)), (18, 6));
        assert_eq!(scaled_hotspot((31, 31), (32, 32), (32, 32)), (31, 31));
    }
}
