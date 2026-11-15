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
    /// An animated kind's first frame cut from a sheet: `images` are the
    /// sheet's sizes.
    pub cell: Option<Cell>,
}

/// Reads a layer's `"cursors"` into `cursors` and `animated`, over what an
/// earlier layer set; `null` gives a kind the system's cursor again.
pub(super) fn read_cursors(
    value: &Value,
    place: &str,
    cursors: &mut BTreeMap<Kind, Cursor>,
    animated: &mut Animated,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    let Value::Object(entries) = value else {
        bail!("\"{place}\" has to be an object");
    };
    // Read first: every sequence's frames take it.
    if let Some(suffix) = entries.get("hidpi") {
        animated.hidpi = Some(hidpi(suffix, &format!("{place}.hidpi"))?);
    }
    for (name, written) in entries {
        let at = format!("{place}.{name}");
        match name.as_str() {
            "hidpi" => continue,
            "transitions" => {
                read_transitions(written, &at, animated, notes)?;
                continue;
            }
            "motion" => {
                read_motion(written, &at, animated, notes)?;
                continue;
            }
            "trail" => {
                animated.trail = match written {
                    Value::Null => None,
                    _ => Some(read_trail(written, &at, animated.hidpi.as_deref(), notes)?),
                };
                continue;
            }
            _ => {}
        }
        let Some(kind) = Kind::named(name) else {
            notes.push(format!("\"{at}\" isn't a kind of cursor; ignored"));
            continue;
        };
        let fields = match written {
            Value::Null => {
                cursors.remove(&kind);
                animated.kinds.remove(&kind);
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
        if fields.contains_key("frames") {
            // An animated kind: its first frame stands still where motion
            // is reduced, and is what the system is first given.
            let sequence = read_sequence(written, &at, animated.hidpi.as_deref(), None, true)?;
            cursors.insert(
                kind,
                Cursor {
                    images: sequence.frames[0].files.clone(),
                    hotspot: sequence.hotspot,
                    cell: sequence.frames[0].cell,
                },
            );
            animated.kinds.insert(kind, sequence);
            continue;
        }
        animated.kinds.remove(&kind);
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
                cell: None,
            },
        );
    }
    Ok(())
}

/// The most frames a sequence has.
pub const MAX_FRAMES: usize = 64;
/// The most frame files a theme's cursors name, every size counted.
pub const MAX_FRAME_FILES: usize = 1024;
/// The shortest a frame shows: 60 a second.
pub const MIN_FRAME_MS: u32 = 16;
/// The largest trail sprite sheet, in pixels on either side.
pub const MAX_SHEET_SIDE: u32 = 2048;
/// The largest sheet file.
pub const MAX_SHEET_BYTES: usize = 2 << 20;
/// The most trail sprites alive at once.
pub const MAX_TRAIL: u32 = 160;

/// One frame of an animated cursor: the same picture at each size, smallest
/// first, or a cell of the same sheet at each size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub files: Vec<PathBuf>,
    pub cell: Option<Cell>,
}

impl Frame {
    fn whole(files: Vec<PathBuf>) -> Self {
        Self { files, cell: None }
    }
}

/// A frame's place on a sheet, in the first size's pixels: cells run left
/// to right, then down, as many to a row as the sheet is wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub index: u32,
    pub width: u32,
    pub height: u32,
}

impl Cell {
    /// Its rectangle on a sheet `sheet_width` pixels wide, drawn `scale`
    /// times the first size: x, y, width, height; none if it falls off.
    pub fn rect(self, sheet: (u32, u32), scale: u32) -> Option<(u32, u32, u32, u32)> {
        let (width, height) = (self.width * scale, self.height * scale);
        let columns = sheet.0 / width.max(1);
        if columns == 0 {
            return None;
        }
        let (x, y) = (
            (self.index % columns) * width,
            (self.index / columns) * height,
        );
        (y + height <= sheet.1).then_some((x, y, width, height))
    }
}

/// The cell `cell` cut from a decoded sheet drawn `scale` times the first
/// size.
pub fn cut(sheet: &Picture, cell: Cell, scale: u32) -> Option<Picture> {
    let (x, y, width, height) = cell.rect((sheet.width, sheet.height), scale)?;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for row in y..y + height {
        let start = ((row * sheet.width + x) * 4) as usize;
        rgba.extend_from_slice(&sheet.rgba[start..start + (width * 4) as usize]);
    }
    Some(Picture {
        width,
        height,
        rgba,
    })
}

/// Frames that play in turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sequence {
    pub frames: Vec<Frame>,
    /// Milliseconds each frame shows, one per frame.
    pub ms: Vec<u32>,
    /// It starts again at its end; otherwise it plays once.
    pub looped: bool,
    /// The point that clicks, in the first size's pixels.
    pub hotspot: (u32, u32),
}

impl Sequence {
    /// Milliseconds it takes once through.
    pub fn total(&self) -> u32 {
        self.ms.iter().sum()
    }

    /// The frame at `elapsed` milliseconds from its start, and the
    /// milliseconds until the next; `None` once a sequence that plays once
    /// is over.
    pub fn at(&self, elapsed: u32) -> Option<(usize, u32)> {
        let total = self.total().max(1);
        let mut left = if self.looped {
            elapsed % total
        } else if elapsed >= total {
            return None;
        } else {
            elapsed
        };
        for (index, ms) in self.ms.iter().enumerate() {
            if left < *ms {
                return Some((index, ms - left));
            }
            left -= ms;
        }
        Some((self.ms.len() - 1, 1))
    }
}

/// The directions pointer motion is sorted into, clockwise from east, as
/// screens count angles (y down).
pub const DIRECTIONS: [&str; 8] = ["e", "se", "s", "sw", "w", "nw", "n", "ne"];

/// The direction of a motion `(dx, dy)`, as an index into [`DIRECTIONS`].
pub fn direction(dx: f32, dy: f32) -> usize {
    let angle = dy.atan2(dx).to_degrees();
    ((angle / 45.0).round() as i32).rem_euclid(8) as usize
}

/// How the cursor of one kind answers the pointer's motion: a still frame
/// for each direction at a slow and a fast speed, a settle as it stops, and
/// an idle loop once it's been still a while.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Motion {
    /// `[direction][0 slow, 1 fast]`.
    pub moving: Vec<[Frame; 2]>,
    pub hotspot: (u32, u32),
    /// One per direction, as it stops after moving that way; or, when its
    /// frames name `{speed}`, one per direction and speed, the slow one
    /// first: `[direction * 2 + speed]`.
    pub settle: Vec<Sequence>,
    /// The settle frame a slow stop starts from.
    pub slow_start: usize,
    pub idle: Option<Sequence>,
    /// Milliseconds still before the idle loop starts.
    pub idle_after: u32,
}

/// A trail of fading sprites behind the pointer while it moves.
#[derive(Clone, Debug, PartialEq)]
pub struct Trail {
    /// The sprite sheet at each size, smallest first.
    pub sheet: Vec<PathBuf>,
    /// A sprite's side, in the first size's pixels.
    pub cell: u32,
    /// The sprites' top left corners on the first size.
    pub sprites: Vec<(u32, u32)>,
    /// Milliseconds one lives.
    pub life: f32,
    /// Logical pixels of motion between spawns, and how many each spawns.
    pub every: f32,
    pub per_step: u32,
    /// How far from the pointer one starts.
    pub jitter: f32,
    /// The most alive at once.
    pub count: u32,
    /// How many burst out on a click.
    pub press: u32,
    /// Opacity a new one starts at.
    pub opacity: f32,
    /// Now and then the trail glitches for a moment.
    pub corrupt: Option<Corrupt>,
}

/// A trail's glitch: every `every` milliseconds (a random time between the
/// two) for `lasts` (likewise), its sprites turn to `sprites`, brighten by
/// `boost`, and each band `band` pixels tall tears sideways up to `tear`.
/// Only the trail's own sprites move; nothing else redraws for it.
#[derive(Clone, Debug, PartialEq)]
pub struct Corrupt {
    pub every: (f32, f32),
    pub lasts: (f32, f32),
    pub sprites: Vec<(u32, u32)>,
    pub boost: f32,
    pub tear: f32,
    pub band: f32,
}

/// What animates a theme's cursors, beside their still pictures.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Animated {
    /// The suffix a frame's larger size takes before `.png`, such as `@2x`.
    pub hidpi: Option<String>,
    /// Kinds that animate on their own, such as `wait`.
    pub kinds: BTreeMap<Kind, Sequence>,
    /// Played once as the cursor changes from one kind to another.
    pub transitions: BTreeMap<(Kind, Kind), Sequence>,
    /// Played once as the button goes down and up, by the kind under it.
    pub press: BTreeMap<Kind, Sequence>,
    pub release: BTreeMap<Kind, Sequence>,
    /// Pointer motion, by kind.
    pub motion: BTreeMap<Kind, Motion>,
    /// Pointer speeds, logical pixels a second: still below `slow`, fast
    /// from `fast`.
    pub slow: f32,
    pub fast: f32,
    pub trail: Option<Trail>,
}

impl Animated {
    /// Whether anything moves.
    pub fn is_still(&self) -> bool {
        self.kinds.is_empty()
            && self.transitions.is_empty()
            && self.press.is_empty()
            && self.release.is_empty()
            && self.motion.is_empty()
            && self.trail.is_none()
    }

    /// Every sequence, once each.
    pub fn sequences(&self) -> Vec<&Sequence> {
        let mut all: Vec<&Sequence> = self
            .kinds
            .values()
            .chain(self.transitions.values())
            .chain(self.press.values())
            .chain(self.release.values())
            .collect();
        for motion in self.motion.values() {
            all.extend(&motion.settle);
            all.extend(&motion.idle);
        }
        all
    }

    /// Every frame of every sequence and motion.
    pub fn frames(&self) -> Vec<&Frame> {
        let mut frames: Vec<&Frame> = self
            .sequences()
            .into_iter()
            .flat_map(|sequence| sequence.frames.iter())
            .collect();
        for motion in self.motion.values() {
            frames.extend(motion.moving.iter().flatten());
        }
        frames
    }

    /// Every frame file it names, every size, once each.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = Vec::new();
        let mut add = |paths: &[PathBuf]| {
            for path in paths {
                if !files.contains(path) {
                    files.push(path.clone());
                }
            }
        };
        for sequence in self.sequences() {
            sequence.frames.iter().for_each(|frame| add(&frame.files));
        }
        for motion in self.motion.values() {
            motion
                .moving
                .iter()
                .flatten()
                .for_each(|frame| add(&frame.files));
        }
        if let Some(trail) = &self.trail {
            add(&trail.sheet);
        }
        files
    }

    /// Frames and trail sprites, in a few words, for the cost line.
    pub fn summary(&self) -> Option<String> {
        if self.is_still() {
            return None;
        }
        let frames: usize = self
            .sequences()
            .iter()
            .map(|sequence| sequence.frames.len())
            .sum::<usize>()
            + self
                .motion
                .values()
                .map(|motion| motion.moving.len() * 2)
                .sum::<usize>();
        let mut parts = vec![format!("{frames} cursor frames")];
        if !self.motion.is_empty() {
            parts.push("motion".into());
        }
        if let Some(trail) = &self.trail {
            parts.push(format!("a trail of up to {}", trail.count));
        }
        Some(parts.join(", "))
    }
}

/// `"@2x"`: a suffix for the larger size of every frame.
fn hidpi(value: &Value, at: &str) -> Result<String, Error> {
    let text = value.as_str().unwrap_or("");
    let fine = !text.is_empty()
        && text.len() <= 8
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '-' | '_' | '.'));
    if !fine {
        bail!("\"{at}\" is a suffix such as \"@2x\" a frame's larger size takes before .png");
    }
    Ok(text.to_owned())
}

/// A PNG path in the theme.
fn png_path(text: &str, here: &str) -> Result<PathBuf, Error> {
    let path = relative(text, here)?;
    let is_png = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
    if !is_png {
        bail!("\"{here}\" names \"{text}\": cursors are .png files");
    }
    Ok(path)
}

/// A frame from one path, with its larger size when `hidpi` is set.
fn frame_of(text: &str, here: &str, hidpi: Option<&str>) -> Result<Frame, Error> {
    Ok(Frame::whole(sizes_of(text, here, hidpi)?))
}

/// A picture's sizes from one path, with its larger size when `hidpi` is set.
fn sizes_of(text: &str, here: &str, hidpi: Option<&str>) -> Result<Vec<PathBuf>, Error> {
    let first = png_path(text, here)?;
    let mut sizes = vec![first];
    if let Some(suffix) = hidpi {
        let stem = text.strip_suffix(".png").unwrap_or(text);
        sizes.push(png_path(&format!("{stem}{suffix}.png"), here)?);
    }
    Ok(sizes)
}

/// `{ "sheet", "cell": [w, h], "count", "first" }`: `count` frames cut from
/// one sheet, from cell `first` (plus `offset`) on.
fn sheet_frames(
    fields: &serde_json::Map<String, Value>,
    here: &str,
    hidpi: Option<&str>,
    dir: Option<&str>,
    offset: u32,
    count: Option<usize>,
) -> Result<Vec<Frame>, Error> {
    let sheet = match fields.get("sheet") {
        Some(Value::String(text)) => sizes_of(
            &pattern(text, dir, None, None),
            &format!("{here}.sheet"),
            hidpi,
        )?,
        Some(value @ Value::Array(_)) => frame_value(value, &format!("{here}.sheet"), None)?.files,
        _ => bail!("\"{here}\" needs \"sheet\", a PNG of the frames side by side"),
    };
    let cell = fields
        .get("cell")
        .and_then(Value::as_array)
        .and_then(|cell| match cell.as_slice() {
            [width, height] => Some((width.as_u64()?, height.as_u64()?)),
            _ => None,
        })
        .filter(|(width, height)| {
            (1..=u64::from(MAX_SIDE)).contains(width) && (1..=u64::from(MAX_SIDE)).contains(height)
        })
        .ok_or_else(|| {
            Error(format!(
                "\"{here}.cell\" is [width, height], each 1 to {MAX_SIDE} pixels"
            ))
        })?;
    let count = match count {
        Some(count) => count,
        None => fields
            .get("count")
            .and_then(Value::as_u64)
            .filter(|count| (1..=MAX_FRAMES as u64).contains(count))
            .ok_or_else(|| {
                Error(format!(
                    "\"{here}.count\" is how many frames, 1 to {MAX_FRAMES}"
                ))
            })? as usize,
    };
    let first = match fields.get("first") {
        Some(first) => first
            .as_u64()
            .filter(|first| *first < 4096)
            .ok_or_else(|| {
                Error(format!(
                    "\"{here}.first\" is the cell the frames start at, from 0"
                ))
            })? as u32,
        None => 0,
    };
    for key in fields.keys() {
        if !matches!(key.as_str(), "sheet" | "cell" | "count" | "first") {
            bail!("\"{here}.{key}\" isn't something a sheet of frames takes");
        }
    }
    Ok((0..count as u32)
        .map(|index| Frame {
            files: sheet.clone(),
            cell: Some(Cell {
                index: first + offset + index,
                width: cell.0 as u32,
                height: cell.1 as u32,
            }),
        })
        .collect())
}

/// A frame written as a path or a list of sizes.
fn frame_value(value: &Value, here: &str, hidpi: Option<&str>) -> Result<Frame, Error> {
    match value {
        Value::String(text) => frame_of(text, here, hidpi),
        Value::Array(sizes) if !sizes.is_empty() && sizes.len() <= MAX_SIZES => sizes
            .iter()
            .enumerate()
            .map(|(index, size)| {
                let at = format!("{here}[{index}]");
                let text = size
                    .as_str()
                    .ok_or_else(|| Error(format!("\"{at}\" names a PNG file in the theme")))?;
                png_path(text, &at)
            })
            .collect::<Result<_, _>>()
            .map(Frame::whole),
        _ => bail!("\"{here}\" is a PNG's path, or a list of 1 to {MAX_SIZES} sizes of it"),
    }
}

/// Fills a pattern's `{dir}`, `{speed}` and `{nn}` (00, 01, …).
fn pattern(text: &str, dir: Option<&str>, speed: Option<&str>, index: Option<usize>) -> String {
    let mut out = text.to_owned();
    if let Some(dir) = dir {
        out = out.replace("{dir}", dir);
    }
    if let Some(speed) = speed {
        out = out.replace("{speed}", speed);
    }
    if let Some(index) = index {
        out = out.replace("{nn}", &format!("{index:02}"));
    }
    out
}

/// Writes `speed` for `{speed}` in a sequence's frames, or its sheet.
fn speed_in(value: &mut Value, speed: &str) {
    fn fill(value: &mut Value, speed: &str) {
        match value {
            Value::String(text) => *text = text.replace("{speed}", speed),
            Value::Array(items) => items.iter_mut().for_each(|item| fill(item, speed)),
            Value::Object(fields) => {
                if let Some(sheet) = fields.get_mut("sheet") {
                    fill(sheet, speed);
                }
            }
            _ => {}
        }
    }
    if let Some(frames) = value.get_mut("frames") {
        fill(frames, speed);
    }
}

/// A sequence: `frames` as a list, or a pattern with `{nn}` and `count`;
/// `ms` one number for every frame or one each; `loop`; `hotspot`. `dir`
/// fills a pattern's `{dir}`.
fn read_sequence(
    value: &Value,
    at: &str,
    hidpi: Option<&str>,
    dir: Option<&str>,
    looped: bool,
) -> Result<Sequence, Error> {
    read_sequence_from(value, at, hidpi, dir, 0, looped)
}

/// As [`read_sequence`], a sheet's frames starting `offset` cells on.
fn read_sequence_from(
    value: &Value,
    at: &str,
    hidpi: Option<&str>,
    dir: Option<&str>,
    offset: u32,
    looped: bool,
) -> Result<Sequence, Error> {
    let Value::Object(fields) = value else {
        bail!("\"{at}\" is an object with \"frames\" and \"ms\"");
    };
    let mut sequence = Sequence {
        frames: Vec::new(),
        ms: Vec::new(),
        looped,
        hotspot: (0, 0),
    };
    let here = format!("{at}.frames");
    match fields.get("frames") {
        Some(Value::Array(items)) => {
            for (index, item) in items.iter().enumerate() {
                let text = item.as_str().map(|text| pattern(text, dir, None, None));
                sequence.frames.push(match text {
                    Some(text) => frame_of(&text, &format!("{here}[{index}]"), hidpi)?,
                    None => frame_value(item, &format!("{here}[{index}]"), hidpi)?,
                });
            }
        }
        Some(Value::String(text)) => {
            if !text.contains("{nn}") {
                bail!("\"{here}\" is a list, or a pattern with {{nn}} where 00, 01, … go");
            }
            let count = fields
                .get("count")
                .and_then(Value::as_u64)
                .filter(|count| (1..=MAX_FRAMES as u64).contains(count))
                .ok_or_else(|| {
                    Error(format!(
                        "\"{at}.count\" is how many frames, 1 to {MAX_FRAMES}"
                    ))
                })?;
            for index in 0..count as usize {
                let text = pattern(text, dir, None, Some(index));
                sequence.frames.push(frame_of(&text, &here, hidpi)?);
            }
        }
        Some(Value::Object(sheet)) => {
            sequence.frames = sheet_frames(sheet, &here, hidpi, dir, offset, None)?;
        }
        _ => bail!("\"{at}\" needs \"frames\""),
    }
    if sequence.frames.is_empty() || sequence.frames.len() > MAX_FRAMES {
        bail!("\"{here}\" has 1 to {MAX_FRAMES} frames");
    }
    let frame_ms = |value: &Value, here: &str| -> Result<u32, Error> {
        value
            .as_u64()
            .filter(|ms| (u64::from(MIN_FRAME_MS)..=5000).contains(ms))
            .map(|ms| ms as u32)
            .ok_or_else(|| {
                Error(format!(
                    "\"{here}\" is milliseconds a frame shows, {MIN_FRAME_MS} to 5000"
                ))
            })
    };
    let here = format!("{at}.ms");
    sequence.ms = match fields.get("ms") {
        None => vec![100; sequence.frames.len()],
        Some(Value::Array(each)) => {
            if each.len() != sequence.frames.len() {
                bail!("\"{here}\" has one number for each frame, or one for all");
            }
            each.iter()
                .enumerate()
                .map(|(index, ms)| frame_ms(ms, &format!("{here}[{index}]")))
                .collect::<Result<_, _>>()?
        }
        Some(ms) => vec![frame_ms(ms, &here)?; sequence.frames.len()],
    };
    for (key, value) in fields {
        let here = format!("{at}.{key}");
        match key.as_str() {
            "frames" | "ms" | "count" => {}
            "loop" => {
                sequence.looped = value
                    .as_bool()
                    .ok_or_else(|| Error(format!("\"{here}\" is true or false")))?;
            }
            "hotspot" => sequence.hotspot = read_hotspot(value, &here)?,
            _ => bail!("\"{here}\" isn't something a cursor's frames take"),
        }
    }
    Ok(sequence)
}

fn read_hotspot(value: &Value, here: &str) -> Result<(u32, u32), Error> {
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
    Ok((x as u32, y as u32))
}

/// `"default>pointer"`, `"press"`, `"release:pointer"`, … each a sequence
/// played once.
fn read_transitions(
    value: &Value,
    place: &str,
    animated: &mut Animated,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    let Value::Object(entries) = value else {
        bail!("\"{place}\" has to be an object");
    };
    for (name, written) in entries {
        let at = format!("{place}.{name}");
        let kinds = |text: &str| Kind::named(text);
        enum Key {
            Change(Kind, Kind),
            Press(Kind),
            Release(Kind),
        }
        let key = if let Some((from, to)) = name.split_once('>') {
            match (kinds(from), kinds(to)) {
                (Some(from), Some(to)) if from != to => Some(Key::Change(from, to)),
                _ => None,
            }
        } else {
            let (event, kind) = name.split_once(':').unwrap_or((name, "default"));
            match (event, kinds(kind)) {
                ("press", Some(kind)) => Some(Key::Press(kind)),
                ("release", Some(kind)) => Some(Key::Release(kind)),
                _ => None,
            }
        };
        let Some(key) = key else {
            notes.push(format!(
                "\"{at}\" isn't a change such as \"default>pointer\", \"press\" or \"release:pointer\"; ignored"
            ));
            continue;
        };
        let sequence = match written {
            Value::Null => None,
            _ => Some(read_sequence(
                written,
                &at,
                animated.hidpi.as_deref(),
                None,
                false,
            )?),
        };
        let (map, key) = match key {
            Key::Change(from, to) => {
                match sequence {
                    Some(sequence) => animated.transitions.insert((from, to), sequence),
                    None => animated.transitions.remove(&(from, to)),
                };
                continue;
            }
            Key::Press(kind) => (&mut animated.press, kind),
            Key::Release(kind) => (&mut animated.release, kind),
        };
        match sequence {
            Some(sequence) => map.insert(key, sequence),
            None => map.remove(&key),
        };
    }
    Ok(())
}

/// `"motion"`: by kind, the frames for each direction and speed, a settle
/// and an idle loop; and the speeds that sort motion.
fn read_motion(
    value: &Value,
    place: &str,
    animated: &mut Animated,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    let Value::Object(entries) = value else {
        bail!("\"{place}\" has to be an object");
    };
    for (name, written) in entries {
        let at = format!("{place}.{name}");
        match name.as_str() {
            "slow" => {
                animated.slow = super::package::number(written, &at, (1.0, 2000.0))?;
                continue;
            }
            "fast" => {
                animated.fast = super::package::number(written, &at, (1.0, 10000.0))?;
                continue;
            }
            _ => {}
        }
        let Some(kind) = Kind::named(name) else {
            notes.push(format!("\"{at}\" isn't a kind of cursor; ignored"));
            continue;
        };
        let Value::Object(fields) = written else {
            if written.is_null() {
                animated.motion.remove(&kind);
                continue;
            }
            bail!("\"{at}\" has to be an object with \"move\"");
        };
        let hidpi = animated.hidpi.clone();
        let hidpi = hidpi.as_deref();
        let hotspot = match fields.get("hotspot") {
            Some(value) => read_hotspot(value, &format!("{at}.hotspot"))?,
            None => (0, 0),
        };
        let here = format!("{at}.move");
        let moving = match fields.get("move") {
            Some(Value::String(text)) if text.contains("{dir}") && text.contains("{speed}") => {
                DIRECTIONS
                    .iter()
                    .map(|dir| -> Result<[Frame; 2], Error> {
                        Ok([
                            frame_of(&pattern(text, Some(dir), Some("slow"), None), &here, hidpi)?,
                            frame_of(&pattern(text, Some(dir), Some("fast"), None), &here, hidpi)?,
                        ])
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
            // One sheet: each direction's slow frame, then its fast one.
            Some(Value::Object(sheet)) => {
                let frames = sheet_frames(sheet, &here, hidpi, None, 0, Some(16))?;
                frames
                    .chunks(2)
                    .map(|pair| [pair[0].clone(), pair[1].clone()])
                    .collect()
            }
            _ => bail!(
                "\"{here}\" is a pattern with {{dir}} (e, se, s, sw, w, nw, n, ne) and {{speed}} (slow, fast), or a sheet of 16 cells"
            ),
        };
        let mut motion = Motion {
            moving,
            hotspot,
            settle: Vec::new(),
            slow_start: 0,
            idle: None,
            idle_after: 600,
        };
        for (key, value) in fields {
            let here = format!("{at}.{key}");
            match key.as_str() {
                "move" | "hotspot" => {}
                "settle" => {
                    let mut value = value.clone();
                    let start = value
                        .as_object_mut()
                        .and_then(|fields| fields.remove("slow_start"));
                    // A sheet without `{dir}` holds every direction's frames,
                    // one direction after another, e first.
                    let one_sheet = value.get("frames").is_some_and(|frames| {
                        frames.is_object() && !frames.to_string().contains("{dir}")
                    });
                    let by_direction = match value.get("frames") {
                        Some(Value::String(text)) => text.contains("{dir}"),
                        Some(Value::Array(items)) => items
                            .iter()
                            .any(|item| item.as_str().is_some_and(|text| text.contains("{dir}"))),
                        Some(Value::Object(_)) => true,
                        _ => false,
                    };
                    if !by_direction {
                        bail!(
                            "\"{here}.frames\" names each direction's frames with {{dir}}, or is a sheet"
                        );
                    }
                    if let Some(object) = value.as_object_mut()
                        && !object.contains_key("hotspot")
                    {
                        object.insert("hotspot".into(), serde_json::json!([hotspot.0, hotspot.1]));
                    }
                    // `{speed}` in its frames: one settle for each speed.
                    let by_speed = value
                        .get("frames")
                        .is_some_and(|frames| frames.to_string().contains("{speed}"));
                    let speeds: &[Option<&str>] = if by_speed {
                        &[Some("slow"), Some("fast")]
                    } else {
                        &[None]
                    };
                    let mut settle = Vec::new();
                    let per_direction = value
                        .get("frames")
                        .and_then(|frames| frames.get("count"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as u32;
                    for (index, dir) in DIRECTIONS.iter().enumerate() {
                        for speed in speeds {
                            let mut value = value.clone();
                            if let Some(speed) = speed {
                                speed_in(&mut value, speed);
                            }
                            let offset = if one_sheet {
                                index as u32 * per_direction
                            } else {
                                0
                            };
                            settle.push(read_sequence_from(
                                &value,
                                &here,
                                hidpi,
                                Some(dir),
                                offset,
                                false,
                            )?);
                        }
                    }
                    motion.settle = settle;
                    if let Some(start) = start {
                        let frames = motion.settle[0].frames.len();
                        motion.slow_start = start
                            .as_u64()
                            .filter(|start| (*start as usize) < frames)
                            .ok_or_else(|| {
                                Error(format!(
                                    "\"{here}.slow_start\" is a frame of the settle, from 0"
                                ))
                            })? as usize;
                    }
                }
                "idle" => {
                    let mut value = value.clone();
                    if let Some(object) = value.as_object_mut()
                        && !object.contains_key("hotspot")
                    {
                        object.insert("hotspot".into(), serde_json::json!([hotspot.0, hotspot.1]));
                    }
                    motion.idle = Some(read_sequence(&value, &here, hidpi, None, true)?);
                }
                "idle_after" => {
                    motion.idle_after =
                        super::package::number(value, &here, (0.0, 60000.0))? as u32;
                }
                _ => notes.push(format!("\"{here}\" isn't something motion takes; ignored")),
            }
        }
        animated.motion.insert(kind, motion);
    }
    if animated.slow <= 0.0 {
        animated.slow = 60.0;
    }
    if animated.fast <= animated.slow {
        animated.fast = animated.slow.max(700.0);
    }
    Ok(())
}

/// `"trail"`: sprites cut from a sheet, fading behind the moving pointer.
fn read_trail(
    value: &Value,
    at: &str,
    hidpi: Option<&str>,
    notes: &mut Vec<String>,
) -> Result<Trail, Error> {
    let Value::Object(fields) = value else {
        bail!("\"{at}\" has to be an object with \"sheet\", \"cell\" and \"sprites\"");
    };
    let sheet = fields
        .get("sheet")
        .ok_or_else(|| Error(format!("\"{at}\" needs \"sheet\"")))
        .and_then(|sheet| frame_value(sheet, &format!("{at}.sheet"), hidpi))?
        .files;
    let mut trail = Trail {
        sheet,
        cell: 16,
        sprites: Vec::new(),
        life: 1300.0,
        every: 9.0,
        per_step: 2,
        jitter: 16.0,
        count: 96,
        press: 6,
        opacity: 0.6,
        corrupt: None,
    };
    let number = super::package::number;
    for (key, value) in fields {
        let here = format!("{at}.{key}");
        match key.as_str() {
            "sheet" => {}
            "cell" => trail.cell = number(value, &here, (4.0, 128.0))? as u32,
            "sprites" => {
                let items = value
                    .as_array()
                    .filter(|items| !items.is_empty() && items.len() <= 64)
                    .ok_or_else(|| {
                        Error(format!(
                            "\"{here}\" lists 1 to 64 sprites, each [x, y] on the sheet"
                        ))
                    })?;
                trail.sprites = items
                    .iter()
                    .map(|item| match item.as_array().map(Vec::as_slice) {
                        Some([x, y]) => Some((x.as_u64()? as u32, y.as_u64()? as u32)),
                        _ => None,
                    })
                    .collect::<Option<_>>()
                    .ok_or_else(|| Error(format!("\"{here}\" lists [x, y] corners")))?;
            }
            "life" => trail.life = number(value, &here, (100.0, 5000.0))?,
            "every" => trail.every = number(value, &here, (1.0, 200.0))?,
            "per_step" => trail.per_step = number(value, &here, (1.0, 8.0))? as u32,
            "jitter" => trail.jitter = number(value, &here, (0.0, 64.0))?,
            "count" => trail.count = number(value, &here, (1.0, MAX_TRAIL as f32))? as u32,
            "press" => trail.press = number(value, &here, (0.0, 32.0))? as u32,
            "opacity" => trail.opacity = number(value, &here, (0.05, 1.0))?,
            "corrupt" => trail.corrupt = Some(read_corrupt(value, &here)?),
            _ => notes.push(format!("\"{here}\" isn't something a trail takes; ignored")),
        }
    }
    if trail.sprites.is_empty() {
        bail!("\"{at}\" needs \"sprites\"");
    }
    Ok(trail)
}

/// `{ "every": [min, max], "lasts": [min, max], "sprites", "boost", "tear", "band" }`.
fn read_corrupt(value: &Value, at: &str) -> Result<Corrupt, Error> {
    let Value::Object(fields) = value else {
        bail!("\"{at}\" has to be an object");
    };
    let number = super::package::number;
    let range =
        |key: &str, (low, high): (f32, f32), unset: (f32, f32)| -> Result<(f32, f32), Error> {
            let here = format!("{at}.{key}");
            match fields.get(key) {
                None => Ok(unset),
                Some(Value::Array(pair)) if pair.len() == 2 => {
                    let (from, to) = (
                        number(&pair[0], &here, (low, high))?,
                        number(&pair[1], &here, (low, high))?,
                    );
                    if from > to {
                        bail!("\"{here}\" goes from the least to the most");
                    }
                    Ok((from, to))
                }
                Some(one) => {
                    let one = number(one, &here, (low, high))?;
                    Ok((one, one))
                }
            }
        };
    let mut corrupt = Corrupt {
        every: range("every", (500.0, 60000.0), (2500.0, 7000.0))?,
        lasts: range("lasts", (40.0, 2000.0), (140.0, 300.0))?,
        sprites: Vec::new(),
        boost: 1.8,
        tear: 18.0,
        band: 10.0,
    };
    for (key, value) in fields {
        let here = format!("{at}.{key}");
        match key.as_str() {
            "every" | "lasts" => {}
            "sprites" => {
                corrupt.sprites = value
                    .as_array()
                    .filter(|items| items.len() <= 64)
                    .and_then(|items| {
                        items
                            .iter()
                            .map(|item| match item.as_array().map(Vec::as_slice) {
                                Some([x, y]) => Some((x.as_u64()? as u32, y.as_u64()? as u32)),
                                _ => None,
                            })
                            .collect::<Option<Vec<_>>>()
                    })
                    .ok_or_else(|| {
                        Error(format!(
                            "\"{here}\" lists up to 64 [x, y] corners on the sheet"
                        ))
                    })?;
            }
            "boost" => corrupt.boost = number(value, &here, (1.0, 4.0))?,
            "tear" => corrupt.tear = number(value, &here, (0.0, 64.0))?,
            "band" => corrupt.band = number(value, &here, (2.0, 64.0))?,
            _ => bail!("\"{here}\" isn't something a trail's glitch takes"),
        }
    }
    Ok(corrupt)
}

/// A sheet of frames or a trail's sprites, decoded under its own limits.
pub fn decode_sheet(bytes: &[u8]) -> Result<Picture, String> {
    if bytes.len() > MAX_SHEET_BYTES {
        return Err("a sheet is at most 2 MiB".into());
    }
    if image::guess_format(bytes).ok() != Some(image::ImageFormat::Png) {
        return Err("isn't a PNG".into());
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SHEET_SIDE);
    limits.max_image_height = Some(MAX_SHEET_SIDE);
    limits.max_alloc = Some(u64::from(MAX_SHEET_SIDE) * u64::from(MAX_SHEET_SIDE) * 4);
    reader.limits(limits);
    let rgba = reader
        .decode()
        .map_err(|_| {
            format!("doesn't decode, or is larger than {MAX_SHEET_SIDE} pixels on a side")
        })?
        .into_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(Picture {
        width,
        height,
        rgba: rgba.into_raw(),
    })
}

/// Why a theme's animated cursors can't show: frames that don't decode,
/// too many files, hotspots off their pictures, sprites off the sheet.
pub fn animated_problems<'a>(
    animated: &Animated,
    bytes: impl Fn(&Path) -> Option<&'a [u8]>,
) -> Vec<String> {
    let mut problems = Vec::new();
    let files = animated.files();
    if files.len() > MAX_FRAME_FILES {
        problems.push(format!(
            "the cursors name {} frame files, more than {MAX_FRAME_FILES}",
            files.len()
        ));
    }
    // Files cut into cells, and the trail's: sheets, under their own limits.
    let mut sheets: Vec<&PathBuf> = animated
        .trail
        .iter()
        .flat_map(|trail| trail.sheet.iter())
        .collect();
    for frame in animated.frames() {
        if frame.cell.is_some() {
            sheets.extend(&frame.files);
        }
    }
    for path in &files {
        let Some(file) = bytes(path) else { continue };
        let on_sheet = sheets.contains(&path);
        let decoded = if on_sheet {
            decode_sheet(file)
        } else {
            decode(file)
        };
        if let Err(error) = decoded {
            problems.push(format!("{}: {error}", path.display()));
        }
    }
    let size = |path: &PathBuf| bytes(path).and_then(size);
    let mut check = |frame: &Frame, hotspot: (u32, u32), what: &str| {
        let Some(first) = frame.files.first() else {
            return;
        };
        // A cell is its frame; it has to lie on each size of its sheet.
        let shown = match frame.cell {
            Some(cell) => {
                let sizes: Vec<(u32, u32)> = frame.files.iter().filter_map(size).collect();
                let first_width = sizes.first().map_or(0, |size| size.0);
                for (width, height) in &sizes {
                    let scale = (width / first_width.max(1)).max(1);
                    if cell.rect((*width, *height), scale).is_none() {
                        let sentence = format!(
                            "{}: cell {} isn't on its {width}×{height} sheet",
                            first.display(),
                            cell.index
                        );
                        if !problems.contains(&sentence) {
                            problems.push(sentence);
                        }
                    }
                }
                Some((cell.width, cell.height))
            }
            None => size(first),
        };
        if let Some((width, height)) = shown
            && (hotspot.0 >= width || hotspot.1 >= height)
        {
            let sentence = format!(
                "the {what} hotspot [{}, {}] isn't on its {width}×{height} frame",
                hotspot.0, hotspot.1
            );
            if !problems.contains(&sentence) {
                problems.push(sentence);
            }
        }
    };
    for sequence in animated.sequences() {
        for frame in &sequence.frames {
            check(frame, sequence.hotspot, "animated cursor's");
        }
    }
    for motion in animated.motion.values() {
        for frame in motion.moving.iter().flatten() {
            check(frame, motion.hotspot, "motion");
        }
    }
    if let Some(trail) = &animated.trail
        && let Some((width, height)) = trail.sheet.first().and_then(size)
        && trail
            .sprites
            .iter()
            .chain(
                trail
                    .corrupt
                    .iter()
                    .flat_map(|corrupt| corrupt.sprites.iter()),
            )
            .any(|(x, y)| x + trail.cell > width || y + trail.cell > height)
    {
        problems.push(format!(
            "a trail sprite lies off its {width}×{height} sheet"
        ));
    }
    problems
}

/// What the animated cursors hold decoded, at most: every frame at every
/// size (a sheet's as its cells), and the trail's sheet.
pub fn animated_memory<'a>(animated: &Animated, bytes: impl Fn(&Path) -> Option<&'a [u8]>) -> u64 {
    let mut total = 0;
    let mut seen: Vec<&PathBuf> = Vec::new();
    for frame in animated.frames() {
        let sizes: Vec<(u32, u32)> = frame
            .files
            .iter()
            .filter_map(|path| bytes(path).and_then(size))
            .collect();
        match frame.cell {
            Some(cell) => {
                let first = sizes.first().map_or(1, |size| size.0.max(1));
                for (width, _) in &sizes {
                    let scale = u64::from((width / first).max(1));
                    total += u64::from(cell.width) * u64::from(cell.height) * scale * scale * 4;
                }
            }
            None => {
                for (path, (width, height)) in frame.files.iter().zip(&sizes) {
                    if !seen.contains(&path) {
                        seen.push(path);
                        total += u64::from(*width) * u64::from(*height) * 4;
                    }
                }
            }
        }
    }
    for path in animated.trail.iter().flat_map(|trail| trail.sheet.iter()) {
        if let Some((width, height)) = bytes(path).and_then(size) {
            total += u64::from(width) * u64::from(height) * 4;
        }
    }
    total
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
    // A first frame cut from a sheet is checked with the animated frames.
    if cursor.cell.is_some() {
        return problems;
    }
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
    for image in cursors
        .values()
        .filter(|cursor| cursor.cell.is_none())
        .flat_map(|cursor| &cursor.images)
    {
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
        read_cursors(
            &value,
            "cursors",
            &mut cursors,
            &mut Animated::default(),
            &mut notes,
        )
        .unwrap();
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
            &mut Animated::default(),
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
            let error = read_cursors(
                &value,
                "cursors",
                &mut cursors,
                &mut Animated::default(),
                &mut notes,
            )
            .unwrap_err();
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
            cell: None,
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
    fn reads_frames_transitions_motion_and_a_trail() {
        let mut cursors = BTreeMap::new();
        let mut animated = Animated::default();
        let mut notes = Vec::new();
        let value = serde_json::json!({
            "hidpi": "@2x",
            "default": { "image": "c/default.png", "hotspot": [16, 16] },
            "wait": { "frames": "c/wait/{nn}.png", "count": 3, "ms": [50, 50, 140], "hotspot": [32, 32] },
            "transitions": {
                "default>pointer": { "frames": ["c/to-pointer/00.png", "c/to-pointer/01.png"], "ms": 35 },
                "press": { "frames": "c/press/{nn}.png", "count": 2 },
                "release:pointer": { "frames": "c/release/{nn}.png", "count": 2, "ms": 40 },
                "spin": { "frames": "c/x/{nn}.png", "count": 1 }
            },
            "motion": {
                "slow": 60, "fast": 700,
                "default": {
                    "move": "c/move/{dir}-{speed}.png", "hotspot": [32, 32],
                    "settle": { "frames": "c/settle/{dir}/{nn}.png", "count": 6, "ms": [50, 50, 50, 50, 50, 70], "slow_start": 2 },
                    "idle": { "frames": "c/idle/{nn}.png", "count": 4, "ms": 150 },
                    "idle_after": 600
                }
            },
            "trail": { "sheet": "c/trail.png", "cell": 16, "sprites": [[0, 0], [16, 0]], "count": 64 }
        });
        read_cursors(&value, "cursors", &mut cursors, &mut animated, &mut notes).unwrap();
        assert_eq!(notes.len(), 1, "{notes:?}");
        let wait = &animated.kinds[&Kind::Wait];
        assert_eq!(wait.frames.len(), 3);
        assert!(wait.looped);
        assert_eq!(
            wait.frames[1].files,
            vec![
                PathBuf::from("c/wait/01.png"),
                PathBuf::from("c/wait/01@2x.png")
            ]
        );
        assert_eq!(cursors[&Kind::Wait].images, wait.frames[0].files);
        assert_eq!(wait.at(0), Some((0, 50)));
        assert_eq!(wait.at(120), Some((2, 120)));
        assert_eq!(wait.at(240), Some((0, 50)), "it loops");
        let morph = &animated.transitions[&(Kind::Default, Kind::Pointer)];
        assert!(!morph.looped);
        assert_eq!(morph.at(80), None, "it plays once");
        assert!(animated.press.contains_key(&Kind::Default));
        assert!(animated.release.contains_key(&Kind::Pointer));
        let motion = &animated.motion[&Kind::Default];
        assert_eq!(motion.moving.len(), 8);
        assert_eq!(
            motion.moving[1][1].files[0],
            PathBuf::from("c/move/se-fast.png")
        );
        assert_eq!(
            motion.settle[6].frames[5].files[0],
            PathBuf::from("c/settle/n/05.png")
        );
        assert_eq!(motion.settle[0].hotspot, (32, 32));
        assert_eq!(motion.slow_start, 2);
        let mut by_speed = Animated::default();
        read_cursors(
            &serde_json::json!({ "motion": { "pointer": {
                "move": "p/move/{dir}-{speed}.png",
                "settle": { "frames": "p/settle/{dir}-{speed}/{nn}.png", "count": 9, "ms": 40 } } } }),
            "cursors",
            &mut BTreeMap::new(),
            &mut by_speed,
            &mut notes,
        )
        .unwrap();
        let settle = &by_speed.motion[&Kind::Pointer].settle;
        assert_eq!(settle.len(), 16);
        assert_eq!(
            settle[3].frames[8].files[0],
            PathBuf::from("p/settle/se-fast/08.png")
        );
        assert!(motion.idle.as_ref().unwrap().looped);
        assert_eq!((animated.slow, animated.fast), (60.0, 700.0));
        let trail = animated.trail.as_ref().unwrap();
        assert_eq!(trail.sheet.len(), 2);
        assert_eq!(trail.count, 64);
        // Two sizes of 3 + 2 + 2 + 2 + 16 + 48 + 4 frames, and the sheet.
        assert_eq!(
            animated.files().len(),
            2 * (3 + 2 + 2 + 2 + 16 + 48 + 4) + 2
        );
        assert!(animated.summary().unwrap().contains("motion"));
    }

    #[test]
    fn a_trail_may_glitch() {
        let mut animated = Animated::default();
        read_cursors(
            &serde_json::json!({ "trail": { "sheet": "t.png", "sprites": [[0, 16]],
                "corrupt": { "every": [2500, 7000], "lasts": 200, "sprites": [[0, 0]], "tear": 12 } } }),
            "cursors",
            &mut BTreeMap::new(),
            &mut animated,
            &mut Vec::new(),
        )
        .unwrap();
        let corrupt = animated.trail.unwrap().corrupt.unwrap();
        assert_eq!(corrupt.every, (2500.0, 7000.0));
        assert_eq!(corrupt.lasts, (200.0, 200.0));
        assert_eq!((corrupt.tear, corrupt.boost), (12.0, 1.8));
        let bad = serde_json::json!({ "trail": { "sheet": "t.png", "sprites": [[0, 0]],
            "corrupt": { "every": [7000, 2500] } } });
        assert!(
            read_cursors(
                &bad,
                "cursors",
                &mut BTreeMap::new(),
                &mut Animated::default(),
                &mut Vec::new()
            )
            .is_err()
        );
    }

    #[test]
    fn frames_cut_from_sheets() {
        let mut cursors = BTreeMap::new();
        let mut animated = Animated::default();
        let mut notes = Vec::new();
        let value = serde_json::json!({
            "hidpi": "@2x",
            "wait": { "frames": { "sheet": "s/wait.png", "cell": [64, 64], "count": 12 }, "ms": 50, "hotspot": [32, 32] },
            "motion": { "default": {
                "move": { "sheet": "s/move.png", "cell": [64, 64] }, "hotspot": [32, 32],
                "settle": { "frames": { "sheet": "s/settle-{speed}.png", "cell": [64, 64], "count": 9 }, "ms": 40 } } }
        });
        read_cursors(&value, "cursors", &mut cursors, &mut animated, &mut notes).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let wait = &animated.kinds[&Kind::Wait];
        assert_eq!(wait.frames.len(), 12);
        assert_eq!(wait.frames[3].cell.map(|cell| cell.index), Some(3));
        assert_eq!(
            wait.frames[3].files,
            [PathBuf::from("s/wait.png"), PathBuf::from("s/wait@2x.png")]
        );
        assert_eq!(cursors[&Kind::Wait].cell.map(|cell| cell.index), Some(0));
        let motion = &animated.motion[&Kind::Default];
        // se (1), fast: cell 3.
        assert_eq!(motion.moving[1][1].cell.map(|cell| cell.index), Some(3));
        // One sheet a speed, its directions one after another: n (6) fast
        // starts at 6 × 9.
        let n_fast = &motion.settle[6 * 2 + 1];
        assert_eq!(
            n_fast.frames[0].files[0],
            PathBuf::from("s/settle-fast.png")
        );
        assert_eq!(n_fast.frames[0].cell.map(|cell| cell.index), Some(54));
        // Four sheets at two sizes each: wait, move and a settle a speed.
        assert_eq!(animated.files().len(), 8);
        // Cells run across the sheet's width and down.
        let cell = Cell {
            index: 9,
            width: 64,
            height: 64,
        };
        assert_eq!(cell.rect((512, 128), 1), Some((64, 64, 64, 64)));
        assert_eq!(cell.rect((1024, 256), 2), Some((128, 128, 128, 128)));
        assert_eq!(cell.rect((512, 64), 1), None);
        let sheet = Picture {
            width: 4,
            height: 2,
            rgba: (0..32).collect(),
        };
        let cut = cut(
            &sheet,
            Cell {
                index: 3,
                width: 2,
                height: 1,
            },
            1,
        )
        .unwrap();
        assert_eq!(cut.rgba, (24..32).collect::<Vec<u8>>());
    }

    #[test]
    fn refuses_bad_frames() {
        let mut notes = Vec::new();
        for (value, why) in [
            (
                serde_json::json!({ "wait": { "frames": "c/{nn}.png" } }),
                "count",
            ),
            (
                serde_json::json!({ "wait": { "frames": "c/a.png", "count": 2 } }),
                "{nn}",
            ),
            (
                serde_json::json!({ "wait": { "frames": ["a.png", "b.png"], "ms": [10, 50] } }),
                "16 to 5000",
            ),
            (
                serde_json::json!({ "wait": { "frames": ["a.png"], "ms": [50, 50] } }),
                "each frame",
            ),
            (
                serde_json::json!({ "motion": { "default": { "move": "m/{dir}.png" } } }),
                "{speed}",
            ),
            (
                serde_json::json!({ "trail": { "sheet": "t.png" } }),
                "sprites",
            ),
            (serde_json::json!({ "hidpi": "/../x" }), "suffix"),
        ] {
            let error = read_cursors(
                &value,
                "cursors",
                &mut BTreeMap::new(),
                &mut Animated::default(),
                &mut notes,
            )
            .unwrap_err();
            assert!(error.0.contains(why), "{error} lacks {why}");
        }
    }

    #[test]
    fn directions_follow_screen_angles() {
        assert_eq!(DIRECTIONS[direction(1.0, 0.0)], "e");
        assert_eq!(DIRECTIONS[direction(1.0, 1.0)], "se");
        assert_eq!(DIRECTIONS[direction(0.0, 1.0)], "s");
        assert_eq!(DIRECTIONS[direction(-1.0, 0.1)], "w");
        assert_eq!(DIRECTIONS[direction(0.0, -1.0)], "n");
        assert_eq!(DIRECTIONS[direction(0.7, -0.7)], "ne");
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
