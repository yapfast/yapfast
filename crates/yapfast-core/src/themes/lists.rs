//! A theme's lists: the server rail's tiles (`layout.rail_tiles`), the
//! channel list's rows (`layout.channel_rows`) and the member list's rows
//! (`layout.member_rows`), from closed lists and ranges.
//!
//! Each list keeps the stock rows' behaviour (labels, keys, menus, drag) and
//! only changes how a row is drawn: its size, font, art behind and over it,
//! a selected look, a mark for hover, and short text at its right. Art is
//! one layer or a list of up to three, picked per item by a seed that comes
//! from the item's id, so a server keeps its frame through restarts and
//! reorders. Every key defaults to the stock look; a theme without these
//! blocks changes nothing.

use serde_json::Value;

use super::layout::closed;
use super::package::art::{Fit, Layer, Source, read_layer};
use super::package::{Error, colour_at, named, number, object};

/// The most pictures an art slot picks between.
pub const MOST_CHOICES: usize = 3;

closed! {
    /// A rail tile's outline.
    RailShape { Square => "square", Circle => "circle", Card => "card", Ticket => "ticket", Tall => "tall" }
}

impl RailShape {
    /// Its width and height when the theme gives none.
    pub fn size(self) -> (f32, f32) {
        match self {
            Self::Square | Self::Circle => (48.0, 48.0),
            Self::Card => (44.0, 52.0),
            Self::Ticket => (56.0, 40.0),
            Self::Tall => (40.0, 64.0),
        }
    }
}

closed! {
    /// What an item does under the pointer, beyond the stock ground.
    HoverKind { None => "none", Magnify => "magnify", Lift => "lift", Tilt => "tilt" }
}

impl HoverKind {
    /// The amount it takes when the theme gives none, and its range.
    pub fn amount(self) -> (f32, (f32, f32)) {
        match self {
            Self::None => (0.0, (0.0, 0.0)),
            Self::Magnify => (1.12, (1.02, 1.25)),
            Self::Lift => (3.0, (1.0, 6.0)),
            Self::Tilt => (3.0, (1.0, 8.0)),
        }
    }
}

closed! {
    /// The ground a row lights to under the pointer.
    HoverGround { None => "none", Hover => "hover", RowHover => "row-hover" }
}

closed! {
    /// Where a rail tile's name goes.
    LabelPlace { None => "none", Below => "below", Inside => "inside" }
}

closed! {
    /// How text is cased.
    Case { AsIs => "as-is", Upper => "upper", Lower => "lower" }
}

closed! {
    /// The theme's type role a list's text takes its face, size and weight
    /// from.
    FontRole { Body => "body", Names => "names", Labels => "labels", Headings => "headings", Code => "code", Small => "small" }
}

closed! {
    /// Where the rail's focus ring goes: round the whole tile, or round the
    /// frame's picture window.
    FocusPlace { Tile => "tile", Window => "window" }
}

closed! {
    /// What marks an unread channel at the row's right.
    UnreadMark { Badge => "badge", Glyph => "glyph", None => "none" }
}

closed! {
    /// How a member's presence shows.
    StatusStyle { Dot => "dot", Glyph => "glyph", Text => "text", None => "none" }
}

closed! {
    /// The line under a member's name.
    SubLine { Activity => "activity", Role => "role", None => "none" }
}

closed! {
    /// How a category heading's words are drawn.
    HeaderStyle { Plain => "plain", Ransom => "ransom" }
}

closed! {
    /// The edge pill beside a rail tile.
    PillMode { On => "on", Off => "off", Unread => "unread" }
}

/// One picture in an art slot: a layer as `art.surfaces` takes it (no
/// drift), its drawn size and place, its own turn, its nine-slice edges and,
/// on a frame, where the server picture or avatar sits inside it.
#[derive(Clone, Debug, PartialEq)]
pub struct TileArt {
    pub layer: Layer,
    /// Logical pixels; the slot's box when `None`.
    pub size: Option<(f32, f32)>,
    /// Logical pixels it moves from its place.
    pub offset: (f32, f32),
    /// Degrees it turns, on top of the item's tilt.
    pub rotation: f32,
    /// Nine-slice edges in the picture's own pixels, top, right, bottom
    /// and left: the corners keep their size as it stretches.
    pub slice: Option<[f32; 4]>,
    /// On a frame: where the picture sits inside it, x, y, width and
    /// height in logical pixels.
    pub window: Option<[f32; 4]>,
    /// The picture's pixels per logical pixel: 2 for art drawn @2x.
    pub density: f32,
    /// Pixel art: drawn with hard edges when scaled, never smoothed.
    pub pixelated: bool,
    /// The window's corner radius, its fifth number.
    pub window_radius: f32,
}

/// The keys an art slot's picture takes beyond a layer's.
const TILE_ART_KEYS: [&str; 7] = ["size", "offset", "rotation", "slice", "window", "density", "pixelated"];

/// A glyph, or a picture, for a mark such as an unread star or a status.
#[derive(Clone, Debug, PartialEq)]
pub enum Glyph {
    Text(String),
    Art(TileArt),
}

/// A rail tile's selected look.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RailSelected {
    pub frame: Option<TileArt>,
    pub over: Option<TileArt>,
    /// Logical pixels; on a right rail +x is toward the window's edge,
    /// mirrored on a left one.
    pub offset: (f32, f32),
    pub scale: Option<f32>,
    /// Milliseconds `over` scales in from 1.4 when the tile becomes selected.
    pub stamp_ms: f32,
}

/// `layout.rail_tiles`.
#[derive(Clone, Debug, PartialEq)]
pub struct RailTiles {
    pub shape: RailShape,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    pub gap: f32,
    pub pad: f32,
    pub tilt: f32,
    pub label: LabelPlace,
    pub label_font: FontRole,
    pub label_size: f32,
    pub label_colour: Option<u32>,
    pub label_ground: Option<u32>,
    pub label_case: Case,
    /// Where an `inside` label sits in the tile, x, y, width and height.
    pub label_box: Option<[f32; 4]>,
    pub frame: Vec<TileArt>,
    pub over: Vec<TileArt>,
    pub below: Option<TileArt>,
    pub between: Option<TileArt>,
    pub between_height: f32,
    pub reflection: bool,
    pub hover: HoverKind,
    pub hover_amount: f32,
    pub hover_ms: f32,
    pub selected: RailSelected,
    pub pill: PillMode,
    pub focus: FocusPlace,
    pub folder_frame: Option<TileArt>,
    pub folder_open_frame: Option<TileArt>,
    /// The line under the home tile; the stock one when `None`.
    pub divider: Option<Divider>,
    pub badge: MentionBadge,
    /// The focus ring's corner, on a `tile` ring.
    pub focus_radius: Option<f32>,
}

/// A short line across the rail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Divider {
    pub size: (f32, f32),
    pub colour: u32,
    pub dotted: bool,
}

impl RailTiles {
    /// The rail's width for these tiles: the stock 72 unless they need
    /// more, at most 96.
    pub fn rail_width(&self) -> f32 {
        (self.width + 2.0 * self.pad + 2.0 * self.selected.offset.0.abs() + 8.0).clamp(72.0, 96.0)
    }

    /// One summary line for `yapfast theme check`.
    pub fn summary(&self) -> String {
        let mut line = format!(
            "rail tiles: {} {}×{}, label {}",
            self.shape.name(),
            self.width,
            self.height,
            self.label.name()
        );
        if !self.frame.is_empty() {
            line += &format!(", {} frame{}", self.frame.len(), plural(self.frame.len()));
        }
        if self.hover != HoverKind::None {
            line += &format!(", hover {} {}", self.hover.name(), self.hover_amount);
        }
        if self.selected.offset != (0.0, 0.0) {
            line += &format!(
                ", selected offset {},{}",
                self.selected.offset.0, self.selected.offset.1
            );
        }
        if self.reflection {
            line += ", reflection";
        }
        line
    }
}

/// A hairline between rows, or a picture.
#[derive(Clone, Debug, PartialEq)]
pub enum Separator {
    Line {
        colour: u32,
        width: f32,
        inset: (f32, f32),
    },
    Art {
        art: TileArt,
        height: f32,
    },
}

/// A line under the selected row's name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Underline {
    pub colour: u32,
    pub width: f32,
    pub offset: f32,
}

/// A channel row's selected look.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChannelSelected {
    pub frame: Option<TileArt>,
    pub prefix: String,
    pub prefix_colour: Option<u32>,
    pub blink_ms: f32,
    pub after: Option<TileArt>,
    pub underline: Option<Underline>,
}

/// A channel row's icon.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowIcon {
    pub size: Option<f32>,
    /// Between the icon and the name.
    pub gap: Option<f32>,
    pub colour: Option<u32>,
    pub hover: Option<u32>,
    pub selected: Option<u32>,
    /// Takes the name's colour in every state.
    pub follow_ink: bool,
}

/// A list's mention count: its ground, its number's colour and its corner.
/// Destructive red stays Yapfast's own (`danger` can't be themed); a list's
/// count may take the theme's colours instead.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MentionBadge {
    pub ground: Option<u32>,
    pub ink: Option<u32>,
    pub radius: Option<f32>,
}

fn read_badge(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<MentionBadge, Error> {
    let mut badge = MentionBadge::default();
    for (key, value) in object(value, place)? {
        let at = format!("{place}.{key}");
        match key.as_str() {
            "ground" => badge.ground = Some(colour_at(value, &at)?),
            "ink" => badge.ink = Some(colour_at(value, &at)?),
            "radius" => badge.radius = Some(number(value, &at, (0.0, 12.0))?),
            _ => notes.push(format!("\"{at}\" isn't something a mention badge takes; ignored")),
        }
    }
    Ok(badge)
}

/// A row list's right-hand words.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChannelMeta {
    pub unread: Option<UnreadMark>,
    pub unread_glyph: Option<Glyph>,
    /// "voice · {n}": {n} is how many are in it.
    pub voice: String,
    pub font: Option<FontRole>,
    pub size: Option<f32>,
    pub colour: Option<u32>,
    /// Pixels between it and the row's right edge.
    pub inset: Option<f32>,
    pub badge: MentionBadge,
}

/// A list's headings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Headers {
    pub font: Option<FontRole>,
    pub size: Option<f32>,
    pub weight: Option<f32>,
    pub case: Option<Case>,
    /// With {name}, {n} (a category's place) or {role} and {n} (a group's
    /// count).
    pub text: String,
    pub style: HeaderStyle,
    pub height: Option<f32>,
    pub colour: Option<u32>,
    pub frame: Option<TileArt>,
}

/// A channel row's text colours over its art.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ink {
    pub read: Option<u32>,
    pub unread: Option<u32>,
    pub selected: Option<u32>,
    pub muted: Option<u32>,
    /// A shadow a pixel under the selected row's name.
    pub selected_shadow: Option<u32>,
    /// A read row's name under the pointer.
    pub hover: Option<u32>,
}

/// `layout.channel_rows`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChannelRows {
    pub height: Option<f32>,
    pub gap: Option<f32>,
    pub font: Option<FontRole>,
    pub size: Option<f32>,
    pub weight: Option<f32>,
    pub case: Option<Case>,
    pub inset: Option<f32>,
    pub frame: Vec<TileArt>,
    pub fit_text: bool,
    pub tilt: f32,
    pub separator: Option<Separator>,
    pub selected: ChannelSelected,
    pub hover: Option<HoverKind>,
    pub hover_amount: Option<f32>,
    pub hover_ms: f32,
    /// The ground a row lights to under the pointer: none, the `hover`
    /// colour or `row-hover`; none by default when the row has art, else
    /// `hover` as stock.
    pub hover_ground: Option<HoverGround>,
    /// The frame's opacity under the pointer.
    pub hover_frame_opacity: Option<f32>,
    pub meta: ChannelMeta,
    pub headers: Headers,
    pub ink: Ink,
    pub emboss: bool,
    pub icon: RowIcon,
    /// The focus ring this far outside the row, in the focus-ring colour;
    /// 0 for the stock ring on the row's edge.
    pub focus_outset: f32,
    pub focus_radius: Option<f32>,
}

/// A member's avatar ring.
#[derive(Clone, Debug, PartialEq)]
pub struct Ring {
    pub colour: u32,
    pub width: f32,
    pub glow: f32,
    /// The presences that wear it.
    pub presences: Vec<String>,
}

/// A member row's avatar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Avatar {
    /// 0 for none.
    pub size: Option<f32>,
    pub frame: Vec<TileArt>,
    pub tilt: f32,
    pub ring: Option<Ring>,
    /// The presence dot's size, and how far it sits past the avatar's corner.
    pub dot: Option<f32>,
    pub dot_out: Option<f32>,
    /// Pixels between the avatar and the name.
    pub gap: Option<f32>,
}

/// A member row's right-hand words, by presence and activity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemberMeta {
    pub online: Option<String>,
    pub idle: Option<String>,
    pub dnd: Option<String>,
    pub offline: Option<String>,
    pub playing: Option<String>,
    pub listening: Option<String>,
    pub watching: Option<String>,
    pub streaming: Option<String>,
    pub font: Option<FontRole>,
    pub size: Option<f32>,
    pub colour: Option<u32>,
    pub max_width: f32,
}

impl MemberMeta {
    pub fn is_set(&self) -> bool {
        [
            &self.online,
            &self.idle,
            &self.dnd,
            &self.offline,
            &self.playing,
            &self.listening,
            &self.watching,
            &self.streaming,
        ]
        .iter()
        .any(|text| text.is_some())
    }
}

/// A badge on each online member, such as "P1".
#[derive(Clone, Debug, PartialEq)]
pub struct Badge {
    /// With {online}, the member's place among those online, from 1.
    pub text: String,
    pub ground: u32,
    pub ink: u32,
    pub font: FontRole,
    pub online_only: bool,
}

/// The member list's title line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Title {
    pub font: Option<FontRole>,
    pub size: Option<f32>,
    pub colour: Option<u32>,
}

/// Presence glyphs, in Online, Idle, Do Not Disturb, Offline, Streaming
/// order.
pub const PRESENCES: [&str; 5] = ["online", "idle", "dnd", "offline", "streaming"];

/// `layout.member_rows`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemberRows {
    pub height: Option<f32>,
    pub gap: Option<f32>,
    pub font: Option<FontRole>,
    pub size: Option<f32>,
    pub weight: Option<f32>,
    pub case: Option<Case>,
    pub avatar: Avatar,
    pub frame: Vec<TileArt>,
    pub separator: Option<Separator>,
    pub status: Option<StatusStyle>,
    /// By [`PRESENCES`].
    pub status_glyphs: [Option<Glyph>; 5],
    pub meta: MemberMeta,
    pub sub: Option<SubLine>,
    /// "{role} · {activity}".
    pub sub_text: String,
    pub badge: Option<Badge>,
    pub headers: Headers,
    pub selected_frame: Option<TileArt>,
    pub selected_over: Option<TileArt>,
    pub title: Title,
    pub hover_frame_opacity: Option<f32>,
    /// Pixels between the list's edges and a row, left and right.
    pub margin: Option<(f32, f32)>,
    /// Pixels inside a row before its avatar and after its meta.
    pub inset: Option<(f32, f32)>,
    /// Members as tiles: columns and the avatar's size, from
    /// `layout.members_style: "grid"` and `layout.grid`.
    pub grid: Option<(u32, f32)>,
    /// A tile takes the theme's `tile` shape (border, corner, shadow).
    pub tile_outline: bool,
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn boolean(value: &Value, place: &str) -> Result<bool, Error> {
    value
        .as_bool()
        .ok_or_else(|| Error(format!("\"{place}\" is true or false")))
}

fn text(value: &Value, place: &str, most: usize) -> Result<String, Error> {
    let text = value
        .as_str()
        .ok_or_else(|| Error(format!("\"{place}\" is text")))?;
    if text.chars().count() > most {
        return Err(Error(format!(
            "\"{place}\" is at most {most} characters"
        )));
    }
    Ok(text.to_owned())
}

fn numbers<const N: usize>(
    value: &Value,
    place: &str,
    range: (f32, f32),
    what: &str,
) -> Result<[f32; N], Error> {
    let items = value
        .as_array()
        .filter(|items| items.len() == N)
        .ok_or_else(|| Error(format!("\"{place}\" is [{what}]")))?;
    let mut read = [0.0; N];
    for (index, item) in items.iter().enumerate() {
        read[index] = number(item, &format!("{place}[{index}]"), range)?;
    }
    Ok(read)
}

/// One picture of an art slot.
fn read_one(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<TileArt, Error> {
    let fields = object(value, place)?;
    let picture: serde_json::Map<String, Value> = fields
        .iter()
        .filter(|(key, _)| !TILE_ART_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let mut layer = read_layer(&Value::Object(picture), place, notes)?;
    if layer.drift != (0.0, 0.0) {
        return Err(Error(format!(
            "\"{place}\" stays in its place: no drift"
        )));
    }
    if layer.fit == Fit::Tile {
        return Err(Error(format!("\"{place}\" doesn't tile")));
    }
    // A slot's picture fills the box it's given unless the theme says how.
    if !fields.contains_key("fit") && matches!(layer.source, Source::Image(_)) {
        layer.fit = Fit::Stretch;
    }
    let mut art = TileArt {
        layer,
        size: None,
        offset: (0.0, 0.0),
        rotation: 0.0,
        slice: None,
        window: None,
        density: 2.0,
        pixelated: false,
        window_radius: 0.0,
    };
    for (key, value) in fields {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "size" => {
                if value.as_str() != Some("full") {
                    let [width, height] = numbers::<2>(value, &here, (4.0, 400.0), "width, height")?;
                    art.size = Some((width, height));
                }
            }
            "offset" => {
                let [x, y] = numbers::<2>(value, &here, (-32.0, 32.0), "x, y")?;
                art.offset = (x, y);
            }
            "rotation" => art.rotation = number(value, &here, (-45.0, 45.0))?,
            "slice" => {
                let edges = if value.is_number() {
                    let edge = number(value, &here, (0.0, 128.0))?;
                    [edge; 4]
                } else {
                    numbers::<4>(value, &here, (0.0, 128.0), "top, right, bottom, left")?
                };
                art.slice = Some(edges);
            }
            "window" => {
                let mut items = value.clone();
                if let Some(list) = items.as_array_mut()
                    && list.len() == 5
                {
                    art.window_radius = number(&list[4], &format!("{here}[4]"), (0.0, 32.0))?;
                    list.pop();
                }
                art.window = Some(numbers::<4>(&items, &here, (0.0, 400.0), "x, y, width, height(, radius)")?);
            }
            "density" => art.density = number(value, &here, (1.0, 4.0))?,
            "pixelated" => art.pixelated = boolean(value, &here)?,
            _ => {}
        }
    }
    if let (Some([x, y, width, height]), Some((frame_w, frame_h))) = (art.window, art.size)
        && (x + width > frame_w + 0.5 || y + height > frame_h + 0.5)
    {
        return Err(Error(format!(
            "\"{place}.window\" reaches past the frame's size"
        )));
    }
    Ok(art)
}

/// An art slot: one picture, or with `most` > 1 a list of up to that many
/// that items pick between by their seed.
pub fn read_tile_art(
    value: &Value,
    place: &str,
    most: usize,
    notes: &mut Vec<String>,
) -> Result<Vec<TileArt>, Error> {
    let items: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    };
    if items.is_empty() || items.len() > most {
        return Err(Error(if most == 1 {
            format!("\"{place}\" is one picture")
        } else {
            format!("\"{place}\" is one picture or a list of up to {most}")
        }));
    }
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let at = if value.is_array() {
                format!("{place}[{index}]")
            } else {
                place.to_owned()
            };
            read_one(item, &at, notes)
        })
        .collect()
}

fn one_art(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<TileArt, Error> {
    Ok(read_tile_art(value, place, 1, notes)?.remove(0))
}

fn glyph(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<Glyph, Error> {
    if value.is_string() {
        return Ok(Glyph::Text(text(value, place, 3)?));
    }
    Ok(Glyph::Art(one_art(value, place, notes)?))
}

fn font(value: &Value, place: &str) -> Result<FontRole, Error> {
    named(value, place, FontRole::ALL, FontRole::name)
}

fn case(value: &Value, place: &str) -> Result<Case, Error> {
    named(value, place, Case::ALL, Case::name)
}

fn read_separator(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<Separator, Error> {
    let fields = object(value, place)?;
    if fields.contains_key("image") || fields.contains_key("texture") {
        let height = fields
            .get("height")
            .map_or(Ok(1.0), |height| number(height, &format!("{place}.height"), (1.0, 16.0)))?;
        let picture: serde_json::Map<String, Value> = fields
            .iter()
            .filter(|(key, _)| key.as_str() != "height")
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        return Ok(Separator::Art {
            art: one_art(&Value::Object(picture), place, notes)?,
            height,
        });
    }
    let mut line = Separator::Line {
        colour: 0xff00_0000,
        width: 1.0,
        inset: (0.0, 0.0),
    };
    let Separator::Line {
        colour,
        width,
        inset,
    } = &mut line
    else {
        unreachable!()
    };
    for (key, value) in fields {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "colour" => *colour = colour_at(value, &here)?,
            "width" => *width = number(value, &here, (1.0, 3.0))?,
            "inset" => {
                let [left, right] = numbers::<2>(value, &here, (0.0, 64.0), "left, right")?;
                *inset = (left, right);
            }
            _ => notes.push(format!("\"{here}\" isn't something a separator takes; ignored")),
        }
    }
    Ok(line)
}

fn read_hover(
    fields: &serde_json::Map<String, Value>,
    place: &str,
    most_magnify: f32,
) -> Result<(Option<HoverKind>, Option<f32>, f32), Error> {
    let hover = fields
        .get("hover")
        .map(|value| named(value, &format!("{place}.hover"), HoverKind::ALL, HoverKind::name))
        .transpose()?;
    let amount = match (hover, fields.get("hover_amount")) {
        (Some(kind), Some(value)) if kind != HoverKind::None => {
            let (_, (low, high)) = kind.amount();
            let high = if kind == HoverKind::Magnify { high.min(most_magnify) } else { high };
            Some(number(value, &format!("{place}.hover_amount"), (low, high))?)
        }
        (Some(HoverKind::Magnify), None) if most_magnify < 1.12 => Some(most_magnify),
        (Some(kind), None) if kind != HoverKind::None => Some(kind.amount().0),
        _ => None,
    };
    let ms = fields
        .get("hover_ms")
        .map_or(Ok(120.0), |value| number(value, &format!("{place}.hover_ms"), (60.0, 300.0)))?;
    Ok((hover, amount, ms))
}

fn read_headers(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<Headers, Error> {
    let mut headers = Headers::default();
    for (key, value) in object(value, place)? {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "font" => headers.font = Some(font(value, &here)?),
            "size" => headers.size = Some(number(value, &here, (9.0, 28.0))?),
            "weight" => headers.weight = Some(number(value, &here, (100.0, 900.0))?),
            "case" => headers.case = Some(case(value, &here)?),
            "text" => headers.text = text(value, &here, 40)?,
            "style" => headers.style = named(value, &here, HeaderStyle::ALL, HeaderStyle::name)?,
            "height" => headers.height = Some(number(value, &here, (24.0, 56.0))?),
            "colour" => headers.colour = Some(colour_at(value, &here)?),
            "frame" => headers.frame = Some(one_art(value, &here, notes)?),
            _ => notes.push(format!("\"{here}\" isn't something headers take; ignored")),
        }
    }
    Ok(headers)
}

/// `layout.rail_tiles`.
pub fn read_rail_tiles(
    value: &Value,
    place: &str,
    notes: &mut Vec<String>,
) -> Result<RailTiles, Error> {
    let fields = object(value, place)?;
    let shape = fields
        .get("shape")
        .map_or(Ok(RailShape::Square), |value| {
            named(value, &format!("{place}.shape"), RailShape::ALL, RailShape::name)
        })?;
    let (width, height) = shape.size();
    let (hover, hover_amount, hover_ms) = read_hover(fields, place, 1.25)?;
    let mut tiles = RailTiles {
        shape,
        width,
        height,
        radius: 12.0,
        gap: 8.0,
        pad: 4.0,
        tilt: 0.0,
        label: LabelPlace::None,
        label_font: FontRole::Labels,
        label_size: 8.0,
        label_colour: None,
        label_ground: None,
        label_case: Case::AsIs,
        label_box: None,
        frame: Vec::new(),
        over: Vec::new(),
        below: None,
        between: None,
        between_height: 0.0,
        reflection: false,
        hover: hover.unwrap_or(HoverKind::None),
        hover_amount: hover_amount.unwrap_or(0.0),
        hover_ms,
        selected: RailSelected::default(),
        pill: PillMode::On,
        focus: FocusPlace::Tile,
        folder_frame: None,
        folder_open_frame: None,
        divider: None,
        badge: MentionBadge::default(),
        focus_radius: None,
    };
    for (key, value) in fields {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "shape" | "hover" | "hover_amount" | "hover_ms" => {}
            "width" => tiles.width = number(value, &here, (32.0, 64.0))?,
            "height" => tiles.height = number(value, &here, (32.0, 72.0))?,
            "radius" => tiles.radius = number(value, &here, (0.0, 32.0))?,
            "gap" => tiles.gap = number(value, &here, (0.0, 24.0))?,
            "pad" => tiles.pad = number(value, &here, (0.0, 12.0))?,
            "tilt" => tiles.tilt = number(value, &here, (0.0, 12.0))?,
            "label" => tiles.label = named(value, &here, LabelPlace::ALL, LabelPlace::name)?,
            "label_font" => tiles.label_font = font(value, &here)?,
            "label_size" => tiles.label_size = number(value, &here, (6.0, 14.0))?,
            "label_colour" => tiles.label_colour = Some(colour_at(value, &here)?),
            "label_ground" => tiles.label_ground = Some(colour_at(value, &here)?),
            "label_case" => tiles.label_case = case(value, &here)?,
            "label_lines" => {
                if number(value, &here, (1.0, 1.0)).is_err() {
                    return Err(Error(format!("\"{here}\" is always 1")));
                }
            }
            "label_box" => {
                tiles.label_box = Some(numbers::<4>(value, &here, (0.0, 72.0), "x, y, width, height")?);
            }
            "frame" => tiles.frame = read_tile_art(value, &here, MOST_CHOICES, notes)?,
            "over" => tiles.over = read_tile_art(value, &here, MOST_CHOICES, notes)?,
            "below" => tiles.below = Some(one_art(value, &here, notes)?),
            "between" => tiles.between = Some(one_art(value, &here, notes)?),
            "between_height" => tiles.between_height = number(value, &here, (0.0, 32.0))?,
            "reflection" => {
                tiles.reflection = match value {
                    Value::Bool(on) => *on,
                    other => number(other, &here, (0.0, 1.0))? >= 1.0,
                }
            }
            "pill" => {
                tiles.pill = match value {
                    Value::Bool(true) => PillMode::On,
                    Value::Bool(false) => PillMode::Off,
                    other => named(other, &here, PillMode::ALL, PillMode::name)?,
                }
            }
            "focus" => tiles.focus = named(value, &here, FocusPlace::ALL, FocusPlace::name)?,
            "badge" => tiles.badge = read_badge(value, &here, notes)?,
            "focus_radius" => tiles.focus_radius = Some(number(value, &here, (0.0, 40.0))?),
            "divider" => {
                let mut divider = Divider {
                    size: (32.0, 2.0),
                    colour: 0xff80_8080,
                    dotted: false,
                };
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "size" => {
                            let [width, height] = numbers::<2>(value, &at, (1.0, 64.0), "width, height")?;
                            divider.size = (width, height);
                        }
                        "colour" => divider.colour = colour_at(value, &at)?,
                        "style" => {
                            divider.dotted = match value.as_str() {
                                Some("dotted") => true,
                                Some("solid") => false,
                                _ => return Err(Error(format!("\"{at}\" is solid or dotted"))),
                            }
                        }
                        _ => notes.push(format!("\"{at}\" isn't something divider takes; ignored")),
                    }
                }
                tiles.divider = Some(divider);
            }
            "selected" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "frame" => tiles.selected.frame = Some(one_art(value, &at, notes)?),
                        "over" => tiles.selected.over = Some(one_art(value, &at, notes)?),
                        "offset" => {
                            let [x, y] = numbers::<2>(value, &at, (-12.0, 12.0), "x, y")?;
                            tiles.selected.offset = (x, y);
                        }
                        "scale" => tiles.selected.scale = Some(number(value, &at, (0.9, 1.2))?),
                        "stamp_ms" => tiles.selected.stamp_ms = number(value, &at, (0.0, 600.0))?,
                        _ => notes.push(format!("\"{at}\" isn't something selected takes; ignored")),
                    }
                }
            }
            "folder" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "frame" => tiles.folder_frame = Some(one_art(value, &at, notes)?),
                        "open_frame" => tiles.folder_open_frame = Some(one_art(value, &at, notes)?),
                        _ => notes.push(format!("\"{at}\" isn't something folder takes; ignored")),
                    }
                }
            }
            _ => notes.push(format!("\"{here}\" isn't something rail_tiles takes; ignored")),
        }
    }
    // What a hovered, lifted, turned or pushed tile needs round it.
    let reach = match tiles.hover {
        HoverKind::Magnify => tiles.height * (tiles.hover_amount - 1.0),
        HoverKind::Lift => tiles.hover_amount,
        HoverKind::Tilt | HoverKind::None => 0.0,
    };
    if reach > tiles.gap + 2.0 * tiles.pad + 0.5 {
        return Err(Error(format!(
            "\"{place}\": a hovered tile reaches {reach:.1}px past its own, more than the gap and pad leave"
        )));
    }
    if tiles.frame.iter().any(|frame| frame.layer.frames.is_some()) {
        notes.push(format!("\"{place}.frame\" plays frames on every tile; mind the cost"));
    }
    Ok(tiles)
}

/// `layout.channel_rows`.
pub fn read_channel_rows(
    value: &Value,
    place: &str,
    notes: &mut Vec<String>,
) -> Result<ChannelRows, Error> {
    let fields = object(value, place)?;
    let (hover, hover_amount, hover_ms) = read_hover(fields, place, 1.04)?;
    let mut rows = ChannelRows {
        hover,
        hover_amount,
        hover_ms,
        ..ChannelRows::default()
    };
    for (key, value) in fields {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "hover" | "hover_amount" | "hover_ms" => {}
            "height" => rows.height = Some(number(value, &here, (24.0, 56.0))?),
            "gap" => rows.gap = Some(number(value, &here, (0.0, 12.0))?),
            "font" => rows.font = Some(font(value, &here)?),
            "size" => rows.size = Some(number(value, &here, (9.0, 28.0))?),
            "weight" => rows.weight = Some(number(value, &here, (100.0, 900.0))?),
            "case" => rows.case = Some(case(value, &here)?),
            "inset" => rows.inset = Some(number(value, &here, (0.0, 24.0))?),
            "frame" => rows.frame = read_tile_art(value, &here, MOST_CHOICES, notes)?,
            "fit_text" => rows.fit_text = boolean(value, &here)?,
            "tilt" => rows.tilt = number(value, &here, (0.0, 4.0))?,
            "separator" => rows.separator = Some(read_separator(value, &here, notes)?),
            "hover_ground" => {
                rows.hover_ground = Some(match value {
                    Value::Bool(true) => HoverGround::Hover,
                    Value::Bool(false) => HoverGround::None,
                    other => named(other, &here, HoverGround::ALL, HoverGround::name)?,
                })
            }
            "hover_frame_opacity" => rows.hover_frame_opacity = Some(number(value, &here, (0.0, 1.0))?),
            "emboss" => rows.emboss = boolean(value, &here)?,
            "icon" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "size" => rows.icon.size = Some(number(value, &at, (8.0, 24.0))?),
                        "gap" => rows.icon.gap = Some(number(value, &at, (0.0, 16.0))?),
                        "colour" if value.as_str() == Some("ink") => rows.icon.follow_ink = true,
                        "colour" => rows.icon.colour = Some(colour_at(value, &at)?),
                        "hover" => rows.icon.hover = Some(colour_at(value, &at)?),
                        "selected" => rows.icon.selected = Some(colour_at(value, &at)?),
                        _ => notes.push(format!("\"{at}\" isn't something icon takes; ignored")),
                    }
                }
            }
            "focus" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "outset" => rows.focus_outset = number(value, &at, (0.0, 6.0))?,
                        "radius" => rows.focus_radius = Some(number(value, &at, (0.0, 32.0))?),
                        _ => notes.push(format!("\"{at}\" isn't something focus takes; ignored")),
                    }
                }
            }
            "ink" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    let colour = colour_at(value, &at);
                    match key.as_str() {
                        "read" => rows.ink.read = Some(colour?),
                        "unread" => rows.ink.unread = Some(colour?),
                        "selected" => rows.ink.selected = Some(colour?),
                        "muted" => rows.ink.muted = Some(colour?),
                        "selected_shadow" => rows.ink.selected_shadow = Some(colour?),
                        "hover" => rows.ink.hover = Some(colour?),
                        "emboss" => rows.emboss = boolean(value, &at)?,
                        _ => notes.push(format!("\"{at}\" isn't something ink takes; ignored")),
                    }
                }
            }
            "selected" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "frame" => rows.selected.frame = Some(one_art(value, &at, notes)?),
                        "prefix" => rows.selected.prefix = text(value, &at, 3)?,
                        "prefix_colour" => rows.selected.prefix_colour = Some(colour_at(value, &at)?),
                        "blink_ms" => {
                            let ms = number(value, &at, (0.0, 1200.0))?;
                            if ms > 0.0 && ms < 300.0 {
                                return Err(Error(format!("\"{at}\" is 0, or 300 to 1200")));
                            }
                            rows.selected.blink_ms = ms;
                        }
                        "after" => rows.selected.after = Some(one_art(value, &at, notes)?),
                        "underline" => {
                            let mut underline = Underline {
                                colour: 0xff00_0000,
                                width: 2.0,
                                offset: 2.0,
                            };
                            for (key, value) in object(value, &at)? {
                                let line = format!("{at}.{key}");
                                match key.as_str() {
                                    "colour" => underline.colour = colour_at(value, &line)?,
                                    "width" => underline.width = number(value, &line, (1.0, 4.0))?,
                                    "offset" => underline.offset = number(value, &line, (0.0, 8.0))?,
                                    _ => notes.push(format!("\"{line}\" isn't something underline takes; ignored")),
                                }
                            }
                            rows.selected.underline = Some(underline);
                        }
                        _ => notes.push(format!("\"{at}\" isn't something selected takes; ignored")),
                    }
                }
            }
            "meta" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "unread" => {
                            rows.meta.unread = Some(named(value, &at, UnreadMark::ALL, UnreadMark::name)?)
                        }
                        "unread_glyph" => rows.meta.unread_glyph = Some(glyph(value, &at, notes)?),
                        "voice" => rows.meta.voice = text(value, &at, 24)?,
                        "font" => rows.meta.font = Some(font(value, &at)?),
                        "size" => rows.meta.size = Some(number(value, &at, (9.0, 16.0))?),
                        "colour" => rows.meta.colour = Some(colour_at(value, &at)?),
                        "inset" => rows.meta.inset = Some(number(value, &at, (0.0, 32.0))?),
                        "badge" => rows.meta.badge = read_badge(value, &at, notes)?,
                        _ => notes.push(format!("\"{at}\" isn't something meta takes; ignored")),
                    }
                }
            }
            "headers" => rows.headers = read_headers(value, &here, notes)?,
            _ => notes.push(format!("\"{here}\" isn't something channel_rows takes; ignored")),
        }
    }
    if rows.selected.blink_ms > 0.0 && rows.selected.prefix.is_empty() {
        notes.push(format!("\"{place}.selected.blink_ms\" blinks nothing without a prefix"));
    }
    Ok(rows)
}

fn read_status_glyphs(
    value: &Value,
    place: &str,
    glyphs: &mut [Option<Glyph>; 5],
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    for (key, value) in object(value, place)? {
        let at = format!("{place}.{key}");
        match PRESENCES.iter().position(|presence| presence == key) {
            Some(index) => glyphs[index] = Some(glyph(value, &at, notes)?),
            None => notes.push(format!("\"{at}\" isn't a presence; ignored")),
        }
    }
    Ok(())
}

/// `layout.member_rows`.
pub fn read_member_rows(
    value: &Value,
    place: &str,
    notes: &mut Vec<String>,
) -> Result<MemberRows, Error> {
    let mut rows = MemberRows {
        meta: MemberMeta {
            max_width: 96.0,
            ..MemberMeta::default()
        },
        ..MemberRows::default()
    };
    for (key, value) in object(value, place)? {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "height" => rows.height = Some(number(value, &here, (32.0, 128.0))?),
            "gap" => rows.gap = Some(number(value, &here, (0.0, 12.0))?),
            "font" => rows.font = Some(font(value, &here)?),
            "size" => rows.size = Some(number(value, &here, (9.0, 28.0))?),
            "weight" => rows.weight = Some(number(value, &here, (100.0, 900.0))?),
            "case" => rows.case = Some(case(value, &here)?),
            "frame" => rows.frame = read_tile_art(value, &here, MOST_CHOICES, notes)?,
            "separator" => rows.separator = Some(read_separator(value, &here, notes)?),
            "status" => rows.status = Some(named(value, &here, StatusStyle::ALL, StatusStyle::name)?),
            "status_glyphs" => read_status_glyphs(value, &here, &mut rows.status_glyphs, notes)?,
            "sub" => rows.sub = Some(named(value, &here, SubLine::ALL, SubLine::name)?),
            "sub_text" => rows.sub_text = text(value, &here, 40)?,
            "hover_frame_opacity" => rows.hover_frame_opacity = Some(number(value, &here, (0.0, 1.0))?),
            "tile_outline" => rows.tile_outline = boolean(value, &here)?,
            "margin" => {
                let [left, right] = numbers::<2>(value, &here, (0.0, 32.0), "left, right")?;
                rows.margin = Some((left, right));
            }
            "inset" => {
                let [left, right] = numbers::<2>(value, &here, (0.0, 32.0), "left, right")?;
                rows.inset = Some((left, right));
            }
            "avatar" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "size" => {
                            let size = number(value, &at, (0.0, 64.0))?;
                            if size > 0.0 && size < 20.0 {
                                return Err(Error(format!("\"{at}\" is 0, or 20 to 64")));
                            }
                            rows.avatar.size = Some(size);
                        }
                        "frame" => rows.avatar.frame = read_tile_art(value, &at, MOST_CHOICES, notes)?,
                        "tilt" => rows.avatar.tilt = number(value, &at, (0.0, 8.0))?,
                        "dot" => rows.avatar.dot = Some(number(value, &at, (8.0, 20.0))?),
                        "dot_out" => rows.avatar.dot_out = Some(number(value, &at, (-4.0, 8.0))?),
                        "gap" => rows.avatar.gap = Some(number(value, &at, (0.0, 24.0))?),
                        "ring" => {
                            let mut ring = Ring {
                                colour: 0xff00_0000,
                                width: 2.0,
                                glow: 0.0,
                                presences: vec!["online".into(), "mobile".into()],
                            };
                            for (key, value) in object(value, &at)? {
                                let ring_at = format!("{at}.{key}");
                                match key.as_str() {
                                    "colour" => ring.colour = colour_at(value, &ring_at)?,
                                    "width" => ring.width = number(value, &ring_at, (1.0, 4.0))?,
                                    "glow" => ring.glow = number(value, &ring_at, (0.0, 16.0))?,
                                    "presences" => {
                                        ring.presences = value
                                            .as_array()
                                            .ok_or_else(|| Error(format!("\"{ring_at}\" is a list of presences")))?
                                            .iter()
                                            .map(|item| text(item, &ring_at, 10))
                                            .collect::<Result<_, _>>()?;
                                    }
                                    _ => notes.push(format!("\"{ring_at}\" isn't something ring takes; ignored")),
                                }
                            }
                            rows.avatar.ring = Some(ring);
                        }
                        _ => notes.push(format!("\"{at}\" isn't something avatar takes; ignored")),
                    }
                }
            }
            "meta" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    let words = || text(value, &at, 32);
                    match key.as_str() {
                        "online" => rows.meta.online = Some(words()?),
                        "idle" => rows.meta.idle = Some(words()?),
                        "dnd" => rows.meta.dnd = Some(words()?),
                        "offline" => rows.meta.offline = Some(words()?),
                        "playing" => rows.meta.playing = Some(words()?),
                        "listening" => rows.meta.listening = Some(words()?),
                        "watching" => rows.meta.watching = Some(words()?),
                        "streaming" => rows.meta.streaming = Some(words()?),
                        "font" => rows.meta.font = Some(font(value, &at)?),
                        "size" => rows.meta.size = Some(number(value, &at, (9.0, 16.0))?),
                        "colour" => rows.meta.colour = Some(colour_at(value, &at)?),
                        "max_width" => rows.meta.max_width = number(value, &at, (40.0, 140.0))?,
                        _ => notes.push(format!("\"{at}\" isn't something meta takes; ignored")),
                    }
                }
            }
            "badge" => {
                let mut badge = Badge {
                    text: "P{online}".into(),
                    ground: 0xffff_7eb6,
                    ink: 0xff34_1e4f,
                    font: FontRole::Labels,
                    online_only: true,
                };
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "text" => badge.text = text(value, &at, 12)?,
                        "ground" => badge.ground = colour_at(value, &at)?,
                        "ink" => badge.ink = colour_at(value, &at)?,
                        "font" => badge.font = font(value, &at)?,
                        "online_only" => badge.online_only = boolean(value, &at)?,
                        _ => notes.push(format!("\"{at}\" isn't something badge takes; ignored")),
                    }
                }
                rows.badge = Some(badge);
            }
            "headers" => rows.headers = read_headers(value, &here, notes)?,
            "selected" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "frame" => rows.selected_frame = Some(one_art(value, &at, notes)?),
                        "over" => rows.selected_over = Some(one_art(value, &at, notes)?),
                        _ => notes.push(format!("\"{at}\" isn't something selected takes; ignored")),
                    }
                }
            }
            "title" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "font" => rows.title.font = Some(font(value, &at)?),
                        "size" => rows.title.size = Some(number(value, &at, (9.0, 28.0))?),
                        "colour" => rows.title.colour = Some(colour_at(value, &at)?),
                        _ => notes.push(format!("\"{at}\" isn't something title takes; ignored")),
                    }
                }
            }
            _ => notes.push(format!("\"{here}\" isn't something member_rows takes; ignored")),
        }
    }
    Ok(rows)
}

/// Every picture the three lists name, once each.
pub fn files<'a>(
    rail: Option<&'a RailTiles>,
    channels: Option<&'a ChannelRows>,
    members: Option<&'a MemberRows>,
) -> Vec<&'a TileArt> {
    let mut all: Vec<&TileArt> = Vec::new();
    if let Some(rail) = rail {
        all.extend(&rail.frame);
        all.extend(&rail.over);
        all.extend(rail.below.iter().chain(&rail.between));
        all.extend(rail.selected.frame.iter().chain(&rail.selected.over));
        all.extend(rail.folder_frame.iter().chain(&rail.folder_open_frame));
    }
    if let Some(channels) = channels {
        all.extend(&channels.frame);
        if let Some(Separator::Art { art, .. }) = &channels.separator {
            all.push(art);
        }
        all.extend(channels.selected.frame.iter().chain(&channels.selected.after));
        if let Some(Glyph::Art(art)) = &channels.meta.unread_glyph {
            all.push(art);
        }
        all.extend(&channels.headers.frame);
    }
    if let Some(members) = members {
        all.extend(&members.avatar.frame);
        all.extend(&members.frame);
        if let Some(Separator::Art { art, .. }) = &members.separator {
            all.push(art);
        }
        for glyph in members.status_glyphs.iter().flatten() {
            if let Glyph::Art(art) = glyph {
                all.push(art);
            }
        }
        all.extend(&members.headers.frame);
        all.extend(members.selected_frame.iter().chain(&members.selected_over));
    }
    all
}

/// What a theme's lists need of the rest of it: a hidden rail needs the top
/// bar's workspaces to switch servers, the text rail and tiles can't both
/// draw the rail, and a file tree's lines don't survive tilted rows.
/// `layers` are the shared layer and the variant's, as written.
pub(super) fn check_with(
    resolved: &mut super::package::Resolved,
    layers: [&serde_json::Map<String, Value>; 2],
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    use super::chrome::{RailMode, TreeList};
    // The member grid is `layout.members_style: "grid"` with `layout.grid`'s
    // columns and size, which the theme trim reads too; the rows' look
    // draws its tiles when `member_rows` is set.
    let layout_of = |layer: &serde_json::Map<String, Value>| layer.get("layout").cloned();
    let mut grid = false;
    let mut columns = 3.0;
    let mut size = 48.0;
    for layout in layers.iter().filter_map(|layer| layout_of(layer)) {
        if let Some(style) = layout.get("members_style").and_then(Value::as_str) {
            grid = style == "grid";
        }
        if let Some(found) = layout.get("grid") {
            if let Some(value) = found.get("columns") {
                columns = number(value, "layout.grid.columns", (2.0, 6.0))?.round();
            }
            if let Some(value) = found.get("size") {
                size = number(value, "layout.grid.size", (32.0, 96.0))?;
            }
        }
    }
    if let Some(rows) = resolved.layout.member_rows.as_mut()
        && grid
    {
        rows.grid = Some((columns as u32, size));
    }
    let layout = &resolved.layout;
    let workspaces = layers
        .iter()
        .filter_map(|layer| layer.get("chrome")?.get("topbar")?.get("workspaces")?.as_f64())
        .next_back()
        .unwrap_or(0.0);
    if layout.rail == Some(super::layout::RailSide::Hidden) && workspaces < 1.0 {
        return Err(Error(
            "\"layout.rail\" is \"hidden\" without \"chrome.topbar.workspaces\": nothing would switch servers".into(),
        ));
    }
    if let Some(tiles) = &layout.rail_tiles {
        if resolved.chrome.rail == Some(RailMode::Text) {
            return Err(Error(
                "\"layout.rail_tiles\" and a text rail (\"chrome.rail\") both draw the rail: pick one".into(),
            ));
        }
        if matches!(layout.rail, Some(super::layout::RailSide::Hidden)) {
            notes.push("\"layout.rail_tiles\" is ignored: the rail is hidden".into());
        }
        let turns = resolved.animations.iter().any(|animation| {
            animation.slot == super::package::Slot::RailIcon
                && animation.property == super::package::Property::Rotation
        });
        if (tiles.tilt > 0.0 || tiles.hover == HoverKind::Tilt) && turns {
            notes.push("\"layout.rail_tiles.tilt\" and the rail's icon animation both turn the tiles".into());
        }
    }
    if let Some(rows) = &layout.channel_rows {
        if resolved.chrome.tree.contains(&TreeList::Channels) {
            if rows.tilt > 0.0 {
                return Err(Error(
                    "\"layout.channel_rows.tilt\" breaks the channel tree's lines: a tree's rows stay straight".into(),
                ));
            }
            if rows.fit_text {
                notes.push("\"layout.channel_rows.fit_text\" with a channel tree leaves gaps in its lines".into());
            }
        }
    }
    if let Some(rows) = &layout.member_rows {
        if resolved.chrome.tree.contains(&TreeList::Members) && rows.avatar.tilt > 0.0 {
            return Err(Error(
                "\"layout.member_rows.avatar.tilt\" breaks the member tree's lines: a tree's rows stay straight".into(),
            ));
        }
        if rows.meta.is_set() && !resolved.chrome.status.is_empty() {
            notes.push("\"layout.member_rows.meta\" replaces the status column (\"chrome.members.columns\")".into());
        }
    }
    Ok(())
}

/// The stable seed of an id: FNV-1a, kept to 31 bits so Slint's ints hold
/// it. It doesn't change across restarts, reorders or theme switches.
pub fn seed(id: &str) -> i32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in id.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    (hash & 0x7fff_ffff) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rail(value: Value) -> Result<RailTiles, Error> {
        read_rail_tiles(&value, "layout.rail_tiles", &mut Vec::new())
    }

    #[test]
    fn reads_rail_tiles_with_their_shape_defaults() {
        let tiles = rail(json!({ "shape": "card" })).unwrap();
        assert_eq!((tiles.width, tiles.height), (44.0, 52.0));
        assert_eq!(tiles.rail_width(), 72.0);
        let tiles = rail(json!({
            "shape": "card", "width": 44, "height": 52, "gap": 10, "pad": 4, "label": "inside",
            "label_font": "labels", "label_size": 8, "label_case": "upper", "label_colour": "#341E4F",
            "label_box": [4, 36, 36, 9],
            "frame": [{ "image": "art/a.png", "slice": [10, 8, 14, 8], "window": [6, 6, 32, 26] },
                      { "image": "art/b.png", "window": [6, 6, 32, 26] },
                      { "image": "art/c.png", "window": [6, 6, 32, 26] }],
            "hover": "lift", "hover_amount": 3, "hover_ms": 120, "pill": "unread", "focus": "window",
            "selected": { "offset": [6, 0], "stamp_ms": 320,
                          "over": { "image": "art/s.png", "size": [14, 14], "position": "top right", "offset": [4, -4] } }
        }))
        .unwrap();
        assert_eq!(tiles.frame.len(), 3);
        assert_eq!(tiles.frame[0].slice, Some([10.0, 8.0, 14.0, 8.0]));
        assert_eq!(tiles.frame[0].window, Some([6.0, 6.0, 32.0, 26.0]));
        assert_eq!(tiles.frame[0].layer.fit, Fit::Stretch);
        assert_eq!(tiles.hover, HoverKind::Lift);
        assert_eq!(tiles.hover_amount, 3.0);
        assert_eq!(tiles.pill, PillMode::Unread);
        assert_eq!(tiles.focus, FocusPlace::Window);
        assert_eq!(tiles.selected.offset, (6.0, 0.0));
        let sticker = tiles.selected.over.as_ref().unwrap();
        assert_eq!(sticker.size, Some((14.0, 14.0)));
        assert_eq!(sticker.layer.position, (1.0, 0.0));
        assert_eq!(tiles.rail_width(), 72.0);
        assert!(tiles.summary().contains("card 44×52"));
        assert_eq!(rail(json!({ "pill": false })).unwrap().pill, PillMode::Off);
    }

    #[test]
    fn refuses_rail_tiles_out_of_range() {
        for (value, place) in [
            (json!({ "width": 80 }), "width"),
            (json!({ "height": 20 }), "height"),
            (json!({ "tilt": 13 }), "tilt"),
            (json!({ "shape": "hexagon" }), "shape"),
            (json!({ "hover": "magnify", "hover_amount": 1.4 }), "hover_amount"),
            (json!({ "frame": [{ "image": "a.png" }, { "image": "b.png" }, { "image": "c.png" }, { "image": "d.png" }] }), "frame"),
            (json!({ "frame": { "image": "a.png", "drift": [3, 0] } }), "drift"),
            (json!({ "selected": { "offset": [13, 0] } }), "offset"),
            (json!({ "frame": { "image": "a.png", "size": [40, 40], "window": [0, 0, 44, 40] } }), "window"),
            (json!({ "label_lines": 2 }), "label_lines"),
        ] {
            let error = rail(value).unwrap_err();
            assert!(error.0.contains(place) || error.0.contains("rail_tiles"), "{place}: {}", error.0);
        }
        // A magnified tile that would reach past its slot.
        let error = rail(json!({ "height": 72, "hover": "magnify", "hover_amount": 1.25, "gap": 0, "pad": 0 })).unwrap_err();
        assert!(error.0.contains("reaches"), "{}", error.0);
    }

    #[test]
    fn reads_channel_rows() {
        let mut notes = Vec::new();
        let rows = read_channel_rows(
            &json!({
                "height": 30, "gap": 6, "font": "code", "weight": 700, "case": "upper", "inset": 10,
                "fit_text": true, "tilt": 1.5,
                "frame": [{ "image": "art/a.png", "slice": 12 }, { "image": "art/b.png", "slice": 12 }],
                "selected": { "frame": { "image": "art/h.png", "slice": [8, 20, 8, 20] }, "prefix": "▶", "blink_ms": 530,
                              "underline": { "colour": "#9E263B", "width": 2, "offset": 3 } },
                "separator": { "colour": "#29252A", "width": 1, "inset": [12, 12] },
                "hover": "lift", "hover_amount": 2, "hover_ms": 90,
                "meta": { "unread": "glyph", "unread_glyph": "●", "voice": "voice · {n}", "font": "code", "size": 11, "colour": "#6E6870" },
                "headers": { "font": "labels", "size": 11, "case": "lower", "text": "// {name}", "height": 34, "colour": "#D4566B", "style": "ransom" },
                "ink": { "read": "#FFFDF7C7", "unread": "#FFFDF7", "selected": "#141414", "emboss": true }
            }),
            "layout.channel_rows",
            &mut notes,
        )
        .unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(rows.frame.len(), 2);
        assert_eq!(rows.frame[0].slice, Some([12.0; 4]));
        assert_eq!(rows.hover, Some(HoverKind::Lift));
        assert_eq!(rows.hover_amount, Some(2.0));
        assert_eq!(rows.hover_ms, 90.0);
        assert_eq!(rows.meta.unread_glyph, Some(Glyph::Text("●".into())));
        assert_eq!(rows.headers.style, HeaderStyle::Ransom);
        assert_eq!(rows.ink.selected, Some(0xff14_1414));
        assert!(rows.emboss);
        assert!(matches!(rows.separator, Some(Separator::Line { width, .. }) if width == 1.0));
        for (value, place) in [
            (json!({ "height": 60 }), "height"),
            (json!({ "tilt": 5 }), "tilt"),
            (json!({ "hover": "magnify", "hover_amount": 1.1 }), "hover_amount"),
            (json!({ "selected": { "blink_ms": 100, "prefix": ">" } }), "blink_ms"),
            (json!({ "selected": { "prefix": "abcd" } }), "prefix"),
            (json!({ "meta": { "voice": "a very long voice template text here" } }), "voice"),
        ] {
            let error = read_channel_rows(&value, "layout.channel_rows", &mut Vec::new()).unwrap_err();
            assert!(error.0.contains(place), "{place}: {}", error.0);
        }
    }

    #[test]
    fn reads_member_rows() {
        let mut notes = Vec::new();
        let rows = read_member_rows(
            &json!({
                "height": 46, "gap": 6, "frame": { "image": "art/card.png", "slice": 32 },
                "avatar": { "size": 34, "tilt": 4, "frame": { "image": "art/bubble.png", "window": [0, 0, 34, 34] },
                            "ring": { "colour": "#3A9BEA", "width": 2, "glow": 8, "presences": ["online", "mobile"] } },
                "status": "glyph",
                "status_glyphs": { "online": { "image": "art/heart.png" }, "idle": "½" },
                "meta": { "online": "online", "listening": "listening · {detail|name}", "max_width": 140 },
                "sub": "role", "sub_text": "{role} · {activity}",
                "badge": { "text": "P{online}", "ground": "#FF7EB6", "ink": "#341E4F" },
                "headers": { "text": "{role} · {n}", "case": "upper", "height": 32 },
                "selected": { "frame": { "image": "art/sel.png" }, "over": { "image": "art/heart.png", "size": [20, 20] } },
                "title": { "font": "labels", "size": 15, "colour": "#141414" }
            }),
            "layout.member_rows",
            &mut notes,
        )
        .unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(rows.avatar.size, Some(34.0));
        assert_eq!(rows.avatar.ring.as_ref().unwrap().glow, 8.0);
        assert!(matches!(rows.status_glyphs[0], Some(Glyph::Art(_))));
        assert_eq!(rows.status_glyphs[1], Some(Glyph::Text("½".into())));
        assert_eq!(rows.meta.max_width, 140.0);
        assert!(rows.meta.is_set());
        assert_eq!(rows.badge.as_ref().unwrap().ground, 0xffff_7eb6);
        assert!(rows.selected_over.is_some());
        assert_eq!(rows.title.size, Some(15.0));
        assert_eq!(files(None, None, Some(&rows)).len(), 5);
        for (value, place) in [
            (json!({ "avatar": { "size": 10 } }), "size"),
            (json!({ "avatar": { "tilt": 9 } }), "tilt"),
            (json!({ "meta": { "max_width": 200 } }), "max_width"),
            (json!({ "status": "sparkles" }), "status"),
        ] {
            let error = read_member_rows(&value, "layout.member_rows", &mut Vec::new()).unwrap_err();
            assert!(error.0.contains(place), "{place}: {}", error.0);
        }
    }

    #[test]
    fn a_seed_stays_the_same_for_an_id() {
        assert_eq!(seed("1400000000000000001"), seed("1400000000000000001"));
        assert_ne!(seed("a"), seed("b"));
        assert!(seed("anything at all") >= 0);
        // FNV-1a of "a", to 31 bits.
        assert_eq!(seed("a"), (0xe40c_292c_u32 & 0x7fff_ffff) as i32);
    }
}
