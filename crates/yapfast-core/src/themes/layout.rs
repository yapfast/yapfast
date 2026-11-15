//! A theme's `layout`: where the window's panes sit and how a message is
//! laid out, from closed lists.
//!
//! `layout` moves what the stock window already has: the server rail to the
//! left, right or top, the member list to either side or away, the composer
//! above or below the messages, and messages as Yapfast draws them (`flat`),
//! as a phone's messenger does (`bubbles`), as a terminal's chat log does
//! (`irc`) or as cards. It also sets the width of every hairline and which
//! outlines are dashed or dotted, and may cut people's names from a set of
//! fonts, letter by letter, as a ransom note.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

use super::lists::{ChannelRows, MemberRows, RailTiles};
use super::package::art::{Layer, MAX_LAYERS, Source, read_layer};
use super::package::{
    Error, Paint, Shadow, colour_at, named, number, object, paint_at, read_shadow, relative,
};

/// The most pictures one card carries.
pub const MAX_CARD_ART: usize = 4;
/// The most pictures one of a card's places picks from, group by group.
pub const MAX_CARD_CHOICES: usize = 6;
/// The widest a card's picture is, in logical pixels; it never shows wider
/// than its card.
pub const MAX_CARD_ART_WIDTH: f32 = 1600.0;

/// Declares a closed list of names: the enum, its names in `theme.json`, and
/// its index, which the interface reads as an int.
macro_rules! closed {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name {
            #[default]
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            pub fn named(name: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|item| item.name() == name)
            }

            /// Its place in [`Self::ALL`]: what the interface is told.
            pub fn index(self) -> i32 {
                Self::ALL.iter().position(|item| *item == self).unwrap_or(0) as i32
            }
        }
    };
}

pub(super) use closed;

closed! {
    /// Which edge of the window the server rail runs along.
    RailSide {
        Left => "left",
        Right => "right",
        Top => "top",
        /// No rail: the sidebar is one tree, as a file explorer shows one,
        /// of the direct messages, the servers and folders, and the open
        /// server's channels under it.
        Tree => "tree",
        /// No rail: a theme's top bar's workspaces switch servers.
        Hidden => "hidden",
    }
}

closed! {
    /// Where the member list docks, or that it doesn't.
    MembersSide {
        Right => "right",
        Left => "left",
        Hidden => "hidden",
        /// On the right as a narrow strip of avatars that opens to the
        /// whole list while the pointer is on it.
        Collapsed => "collapsed",
    }
}

closed! {
    /// How the member list lays out its people.
    MembersStyle {
        /// A row each, as stock.
        List => "list",
        /// Square tiles, several across, each the avatar with the name under it.
        Grid => "grid",
    }
}

closed! {
    /// Whether the composer sits under the messages or over them.
    ComposerSide { Bottom => "bottom", Top => "top" }
}

closed! {
    /// How a message is laid out.
    MessageStyle {
        /// Avatar, name and time over the text, as stock.
        Flat => "flat",
        /// Each message in a bubble; your own on the right without a name, as
        /// a phone's messenger shows them.
        Bubbles => "bubbles",
        /// One line a message: a time column, the name right-aligned in its
        /// own column, then the text, as a terminal's chat log.
        Irc => "irc",
        /// Each run of messages from one person on a card of its own, which
        /// a theme may tilt a little, as scraps pasted on a page.
        Cards => "cards",
    }
}

closed! {
    /// How an outline is drawn.
    Stroke { Solid => "solid", Dashed => "dashed", Dotted => "dotted" }
}

closed! {
    /// The outlines a theme may dash or dot.
    BorderPlace {
        Composer => "composer",
        Card => "card",
        Menu => "menu",
        Dialog => "dialog",
        Tooltip => "tooltip",
        /// A card or bubble around messages (`cards`, `bubbles`).
        Message => "message",
        Embed => "embed",
        Code => "code",
        Tile => "tile",
        /// The day and "New" lines across the messages.
        Divider => "divider",
        /// The bar of actions over a message under the pointer.
        Actions => "actions",
        /// The edges between the rail, the sidebar, the conversation and
        /// the member list: drawn only when a theme names them.
        Panes => "panes",
    }
}

/// A dashed or dotted outline. Its width and colour follow the surface's
/// `shape` border, then the stock outline, unless set here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub stroke: Stroke,
    /// Pixels of each dash; a dot is as long as the line is wide.
    pub dash: f32,
    /// Pixels between dashes or dots.
    pub gap: f32,
    pub width: Option<f32>,
    /// ARGB.
    pub colour: Option<u32>,
}

/// The `irc` message style's columns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Irc {
    /// Pixels of the time column, 0 to hide it.
    pub time_width: Option<f32>,
    /// Pixels of the name column; a longer name is cut short.
    pub nick_width: Option<f32>,
    /// Between the name and the text, such as `│` or `>`; at most 3 characters.
    pub separator: Option<String>,
    /// The separator drawn on every line of a message, the wrapped ones
    /// too, as a rule down the gutter its text hangs from, rather than on
    /// its first line only.
    pub rule: Option<bool>,
}

/// The `bubbles` message style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bubbles {
    /// The widest a bubble grows, as a percentage of the column (40–95).
    pub max_width: Option<f32>,
    /// Grounds of your own bubbles and everyone else's, a colour or a
    /// gradient, and the text on your own (ARGB); the others' text is `text`.
    pub own: Option<Paint>,
    pub other: Option<Paint>,
    pub own_text: Option<u32>,
    /// Corner radius in pixels.
    pub radius: Option<f32>,
    /// Pictures laid on each of your own bubbles and on everyone else's,
    /// over the ground and under the text, such as a gel's gloss: at most
    /// four each, still.
    pub own_art: Vec<Layer>,
    pub other_art: Vec<Layer>,
    /// A drawn gel on each of your own bubbles and on everyone else's: a
    /// gloss over the top, a glow along the bottom, a rim and a shadow.
    /// Nothing is drawn unless set.
    pub own_gel: Gel,
    pub other_gel: Gel,
    /// Pixels inside a bubble, across and down (12 and 6 unset).
    pub padding: Option<(f32, f32)>,
}

/// A bubble's top and bottom padding when the theme sets none.
pub const BUBBLE_PADDING: (f32, f32) = (12.0, 6.0);

/// Shapes drawn on a bubble, crisp at any size, as a glossy gel button's:
/// each part is optional.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gel {
    pub gloss: Option<Gloss>,
    pub glow: Option<Glow>,
    /// ARGB: a 1-pixel light line just inside the edge, brightest at the
    /// top and again, fainter, along the bottom.
    pub rim_light: Option<u32>,
    /// ARGB: a 1-pixel line on the edge itself.
    pub rim_dark: Option<u32>,
    pub shadow: Option<Shadow>,
}

impl Gel {
    pub fn is_empty(&self) -> bool {
        *self == Gel::default()
    }
}

/// The gloss: a rounded shape inset from the bubble's top, its colour
/// fading from `from` to `to` (alphas) downward, ending in a clean edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gloss {
    /// ARGB; its alpha scales `from` and `to`.
    pub colour: u32,
    pub from: f32,
    pub to: f32,
    /// A share of the bubble's height (0.1–0.8)...
    pub height: f32,
    /// ...but never more pixels than this (4–96).
    pub max: f32,
    /// Pixels in from the sides and down from the top.
    pub inset: (f32, f32),
    /// Its corners' radius; the bubble's less the inset when unset.
    pub radius: Option<f32>,
}

impl Gloss {
    /// The most of its colour the gloss lays on anything at or below `y`
    /// pixels from the bubble's top, on any bubble: the band is tallest,
    /// and fades slowest, at `max`.
    pub fn alpha_below(&self, y: f32) -> f32 {
        let (top, bottom) = (self.inset.1, self.inset.1 + self.max);
        let at =
            |y: f32| self.from + (self.to - self.from) * ((y - top) / self.max).clamp(0.0, 1.0);
        let strength = f32::from(u8::try_from(self.colour >> 24).unwrap_or(255)) / 255.0;
        let most = if y >= bottom {
            0.0
        } else if self.from >= self.to {
            at(y.max(top))
        } else {
            self.to
        };
        most * strength
    }
}

/// A soft light rising from the bubble's bottom edge, as light refracted
/// through a gel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    /// ARGB at the very bottom, fading to nothing `height` pixels up.
    pub colour: u32,
    pub height: f32,
}

impl Glow {
    /// The most of its colour the glow lays `y` pixels or more above the
    /// bubble's bottom.
    pub fn alpha_above(&self, y: f32) -> f32 {
        let strength = f32::from(u8::try_from(self.colour >> 24).unwrap_or(255)) / 255.0;
        strength * (1.0 - y / self.height).clamp(0.0, 1.0)
    }
}

/// The `cards` message style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cards {
    /// The most a card leans either way, in degrees (0–4); each card takes
    /// its own lean in that range from who and when. Only the card leans:
    /// text never rotates.
    pub tilt: Option<f32>,
    /// Pixels between cards.
    pub gap: Option<f32>,
    /// Pixels inside a card.
    pub padding: Option<f32>,
    /// ARGB ground of a card; `raised` when not set.
    pub ground: Option<u32>,
    /// Each card's shadow, a hard offset when its blur is 0.
    pub shadow: Option<Shadow>,
    /// Pictures pasted on each card, such as a strip of tape over its top
    /// edge: they lean with it.
    pub art: Vec<CardArt>,
    /// Words on the card's tab: over its first picture, or a tab of their own.
    pub tab: Option<CardTab>,
    /// Pixels each card steps aside, to the left or the right by its
    /// group, so cards down the page alternate sides (0–48).
    pub stagger: Option<f32>,
    /// A hovered or mentioned card's tint lies over its pictures too.
    pub tint_art: bool,
}

/// The words on a message card's tab, such as its author's name.
#[derive(Clone, Debug, PartialEq)]
pub struct CardTab {
    /// Plain text, with `{name}` for the author and `{time}` for the time.
    pub text: String,
    /// The type role whose face it takes: `labels` when unset.
    pub role: Option<super::package::Role>,
    pub size: Option<f32>,
    pub weight: Option<i32>,
    /// ARGB; `text` when unset.
    pub colour: Option<u32>,
    pub uppercase: bool,
    /// ARGB ground of a tab of its own, where the card has no picture.
    pub ground: Option<u32>,
}

/// The member list as tiles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    /// Tiles across (2–6).
    pub columns: u32,
    /// Pixels of each avatar (32–96).
    pub size: f32,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            columns: 3,
            size: 56.0,
        }
    }
}

/// The sidebar as one tree, with `rail: "tree"`.
#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    /// Opening a folder closes the others (`one`), or leaves them (`many`).
    pub expand_many: bool,
    /// The root line and the direct messages' row.
    pub root: String,
    pub dms: String,
}

impl Default for Tree {
    fn default() -> Self {
        Self {
            expand_many: false,
            root: "~/".into(),
            dms: "dm".into(),
        }
    }
}

/// A picture on a card's top or bottom edge, leaning with the card.
#[derive(Clone, Debug, PartialEq)]
pub struct CardArt {
    /// The picture or texture, with its opacity, blend (with the card's
    /// ground), blur, dim and colour. Fit is within `size`.
    pub layer: Layer,
    /// Other pictures this place picks from, card by card, with the same
    /// settings; the layer's own picture is the first choice.
    pub choices: Vec<PathBuf>,
    /// As wide as the card (`"size": ["full", 18]`).
    pub full: bool,
    /// Across the card, 0 its left and 1 its right.
    pub across: f32,
    /// On the card's bottom edge rather than its top.
    pub bottom: bool,
    /// Logical pixels, width and height; the picture's own size when unset.
    pub size: Option<(f32, f32)>,
    /// Logical pixels it moves from its place on the edge.
    pub offset: (f32, f32),
    /// Degrees it turns beyond the card's own lean.
    pub rotation: f32,
}

/// The keys a card's picture takes beyond a layer's.
const CARD_ART_KEYS: [&str; 3] = ["size", "offset", "rotation"];

/// People's names cut letter by letter from different fonts, each letter on a
/// scrap of its own, as a ransom note: `"names": { "style": "ransom", … }`.
/// Someone's own name style wins, and the person can turn it off with *Plain
/// names*.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Names {
    /// Font files in the theme's folder that the letters cycle through
    /// (2–8); the names and body faces when none are listed.
    pub fonts: Vec<PathBuf>,
    /// ARGB `(ground, ink)` pairs the letters' scraps cycle through, at most 6.
    pub papers: Vec<(u32, u32)>,
}

/// Where the panes sit and how messages are laid out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub rail: Option<RailSide>,
    pub members: Option<MembersSide>,
    pub composer: Option<ComposerSide>,
    pub messages: Option<MessageStyle>,
    /// The widest the messages and composer grow, centred in the
    /// conversation (480–1600 pixels).
    pub chat_max_width: Option<f32>,
    /// `false` hides avatars beside messages.
    pub chat_avatars: Option<bool>,
    pub irc: Irc,
    pub bubbles: Bubbles,
    pub cards: Cards,
    /// Pixels of every hairline outline and rule the stock window draws at
    /// one pixel (0–4).
    pub outline_width: Option<f32>,
    pub borders: BTreeMap<BorderPlace, Border>,
    /// People's names as ransom notes.
    pub names: Option<Names>,
    /// The server rail's tiles, the channel list's rows and the member
    /// list's rows; see [`super::lists`].
    pub rail_tiles: Option<RailTiles>,
    pub channel_rows: Option<ChannelRows>,
    pub member_rows: Option<MemberRows>,
    /// The member list as rows or tiles.
    pub members_style: Option<MembersStyle>,
    pub grid: Grid,
    /// The tree with `rail: "tree"`.
    pub tree: Tree,
}

impl Layout {
    /// Whether it changes anything at all.
    pub fn is_stock(&self) -> bool {
        *self == Self::default()
    }

    /// The pictures it names, once each.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = Vec::new();
        let lists = super::lists::files(
            self.rail_tiles.as_ref(),
            self.channel_rows.as_ref(),
            self.member_rows.as_ref(),
        );
        let layers = self
            .bubbles
            .own_art
            .iter()
            .chain(&self.bubbles.other_art)
            .chain(self.cards.art.iter().map(|art| &art.layer))
            .chain(lists.into_iter().map(|art| &art.layer));
        let paths = layers
            .filter_map(|layer| match &layer.source {
                Source::Image(path) => Some(path),
                Source::Texture(_) => None,
            })
            .chain(self.cards.art.iter().flat_map(|art| &art.choices));
        for path in paths {
            if !files.contains(path) {
                files.push(path.clone());
            }
        }
        files
    }
}

/// `layout.cards.art`: one picture or a list of up to two.
fn read_card_art(
    value: &Value,
    place: &str,
    notes: &mut Vec<String>,
) -> Result<Vec<CardArt>, Error> {
    let items: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    };
    if items.len() > MAX_CARD_ART {
        return Err(Error(format!(
            "\"{place}\" puts at most {MAX_CARD_ART} pictures on a card"
        )));
    }
    let mut read = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        let at = format!("{place}[{index}]");
        let fields = object(item, &at)?;
        let mut picture: serde_json::Map<String, Value> = fields
            .iter()
            .filter(|(key, _)| !CARD_ART_KEYS.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        // `"image": ["a.png", "b.png"]`: each card picks one by its group.
        let mut choices = Vec::new();
        if let Some(Value::Array(images)) = fields.get("image") {
            if images.is_empty() || images.len() > MAX_CARD_CHOICES {
                return Err(Error(format!(
                    "\"{at}.image\" lists 1 to {MAX_CARD_CHOICES} pictures to pick from"
                )));
            }
            for (number, image) in images.iter().enumerate() {
                let here = format!("{at}.image[{number}]");
                let text = image
                    .as_str()
                    .ok_or_else(|| Error(format!("\"{here}\" is a file name")))?;
                choices.push(relative(text, &here)?);
            }
            picture.insert("image".to_owned(), images[0].clone());
            choices.remove(0);
        }
        let layer = read_layer(&Value::Object(picture), &at, notes)?;
        if layer.moves() {
            return Err(Error(format!(
                "\"{at}\" stays still on its card: no drift or frames"
            )));
        }
        let mut art = CardArt {
            across: layer.position.0,
            bottom: layer.position.1 > 0.5,
            layer,
            choices,
            full: false,
            size: None,
            offset: (0.0, 0.0),
            rotation: 0.0,
        };
        for (key, value) in fields {
            let here = format!("{at}.{key}");
            match key.as_str() {
                "size" => {
                    let pair = value
                        .as_array()
                        .filter(|items| items.len() == 2)
                        .ok_or_else(|| Error(format!("\"{here}\" is [width, height]")))?;
                    art.full = pair[0].as_str() == Some("full");
                    let width = if art.full {
                        MAX_CARD_ART_WIDTH
                    } else {
                        number(&pair[0], &format!("{here}[0]"), (4.0, MAX_CARD_ART_WIDTH))?
                    };
                    art.size = Some((
                        width,
                        number(&pair[1], &format!("{here}[1]"), (4.0, 240.0))?,
                    ));
                }
                "offset" => {
                    let pair = value
                        .as_array()
                        .filter(|items| items.len() == 2)
                        .ok_or_else(|| Error(format!("\"{here}\" is [x, y]")))?;
                    art.offset = (
                        number(&pair[0], &format!("{here}[0]"), (-64.0, 64.0))?,
                        number(&pair[1], &format!("{here}[1]"), (-64.0, 64.0))?,
                    );
                }
                "rotation" => art.rotation = number(value, &here, (-45.0, 45.0))?,
                _ => {}
            }
        }
        read.push(art);
    }
    Ok(read)
}

pub(super) fn read_layout(
    value: &Value,
    place: &str,
    layout: &mut Layout,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    for (key, value) in object(value, place)? {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "rail" => layout.rail = Some(named(value, &here, RailSide::ALL, RailSide::name)?),
            "members" => {
                layout.members = Some(named(value, &here, MembersSide::ALL, MembersSide::name)?);
            }
            "composer" => {
                layout.composer = Some(named(value, &here, ComposerSide::ALL, ComposerSide::name)?);
            }
            "messages" => {
                layout.messages = Some(named(value, &here, MessageStyle::ALL, MessageStyle::name)?);
            }
            "chat_max_width" => {
                layout.chat_max_width = Some(number(value, &here, (480.0, 1600.0))?)
            }
            "chat_avatars" => layout.chat_avatars = Some(boolean(value, &here)?),
            "rail_tiles" => {
                layout.rail_tiles = Some(super::lists::read_rail_tiles(value, &here, notes)?)
            }
            "channel_rows" => {
                layout.channel_rows = Some(super::lists::read_channel_rows(value, &here, notes)?)
            }
            "member_rows" => {
                layout.member_rows = Some(super::lists::read_member_rows(value, &here, notes)?)
            }
            "irc" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "time_width" => {
                            layout.irc.time_width = Some(number(value, &at, (0.0, 120.0))?)
                        }
                        "nick_width" => {
                            layout.irc.nick_width = Some(number(value, &at, (48.0, 240.0))?)
                        }
                        "separator" => layout.irc.separator = Some(short(value, &at, 3)?),
                        "rule" => layout.irc.rule = Some(boolean(value, &at)?),
                        _ => notes.push(format!("\"{at}\" isn't something irc takes; ignored")),
                    }
                }
            }
            "bubbles" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "max_width" => {
                            layout.bubbles.max_width = Some(number(value, &at, (40.0, 95.0))?)
                        }
                        "own" => {
                            let (ground, art, gel) = read_bubble(value, &at, notes)?;
                            layout.bubbles.own = ground.or(layout.bubbles.own.take());
                            layout.bubbles.own_art = art;
                            layout.bubbles.own_gel = gel;
                        }
                        "other" => {
                            let (ground, art, gel) = read_bubble(value, &at, notes)?;
                            layout.bubbles.other = ground.or(layout.bubbles.other.take());
                            layout.bubbles.other_art = art;
                            layout.bubbles.other_gel = gel;
                        }
                        "padding" => layout.bubbles.padding = Some(pair(value, &at, (4.0, 24.0))?),
                        "own_text" => layout.bubbles.own_text = Some(colour_at(value, &at)?),
                        "radius" => layout.bubbles.radius = Some(number(value, &at, (0.0, 999.0))?),
                        _ => notes.push(format!("\"{at}\" isn't something bubbles takes; ignored")),
                    }
                }
            }
            "cards" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "tilt" => layout.cards.tilt = Some(number(value, &at, (0.0, 4.0))?),
                        "gap" => layout.cards.gap = Some(number(value, &at, (0.0, 24.0))?),
                        "padding" => layout.cards.padding = Some(number(value, &at, (0.0, 24.0))?),
                        "ground" => layout.cards.ground = Some(colour_at(value, &at)?),
                        "shadow" => {
                            layout.cards.shadow = if value.as_str() == Some("none") {
                                None
                            } else {
                                Some(read_shadow(value, &at)?)
                            }
                        }
                        "art" => layout.cards.art = read_card_art(value, &at, notes)?,
                        "tab" => layout.cards.tab = Some(read_tab(value, &at, notes)?),
                        "stagger" => layout.cards.stagger = Some(number(value, &at, (0.0, 48.0))?),
                        "tint_art" => layout.cards.tint_art = boolean(value, &at)?,
                        _ => notes.push(format!("\"{at}\" isn't something cards takes; ignored")),
                    }
                }
            }
            "members_style" => {
                layout.members_style =
                    Some(named(value, &here, MembersStyle::ALL, MembersStyle::name)?);
            }
            "grid" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "columns" => {
                            layout.grid.columns = number(value, &at, (2.0, 6.0))?.round() as u32
                        }
                        "size" => layout.grid.size = number(value, &at, (32.0, 96.0))?,
                        _ => notes.push(format!("\"{at}\" isn't something grid takes; ignored")),
                    }
                }
            }
            "tree" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "expand" => {
                            layout.tree.expand_many = match value.as_str() {
                                Some("one") => false,
                                Some("many") => true,
                                _ => return Err(Error(format!("\"{at}\" is one or many"))),
                            }
                        }
                        "root" => layout.tree.root = short(value, &at, 24)?,
                        "dms" => layout.tree.dms = short(value, &at, 24)?,
                        _ => notes.push(format!("\"{at}\" isn't something tree takes; ignored")),
                    }
                }
            }
            _ => notes.push(format!("\"{here}\" isn't something layout takes; ignored")),
        }
    }
    Ok(())
}

/// `layout.cards.tab`: `{ "text": "{name}", "font": "labels", "size": 12,
/// "weight": 800, "colour": "#0c0a0e", "uppercase": false, "ground": … }`.
fn read_tab(value: &Value, at: &str, notes: &mut Vec<String>) -> Result<CardTab, Error> {
    use super::package::Role;
    let fields = object(value, at)?;
    let mut tab = CardTab {
        text: "{name}".into(),
        role: None,
        size: None,
        weight: None,
        colour: None,
        uppercase: false,
        ground: None,
    };
    for (key, value) in fields {
        let here = format!("{at}.{key}");
        match key.as_str() {
            "text" => {
                let text = short(value, &here, 48)?;
                let mut rest = text.as_str();
                while let Some(open) = rest.find('{') {
                    let after = &rest[open + 1..];
                    let Some(close) = after.find('}') else { break };
                    let word = &after[..close];
                    if word != "name" && word != "time" {
                        return Err(Error(format!(
                            "\"{here}\" can't use {{{word}}}; it takes {{name}} and {{time}}"
                        )));
                    }
                    rest = &after[close + 1..];
                }
                tab.text = text;
            }
            "font" => {
                tab.role = Some(value.as_str().and_then(Role::named).ok_or_else(|| {
                    Error(format!("\"{here}\" names a type role, such as labels"))
                })?);
            }
            "size" => tab.size = Some(number(value, &here, (8.0, 24.0))?),
            "weight" => tab.weight = Some(number(value, &here, (100.0, 900.0))?.round() as i32),
            "colour" => tab.colour = Some(colour_at(value, &here)?),
            "ground" => tab.ground = Some(colour_at(value, &here)?),
            "uppercase" => tab.uppercase = boolean(value, &here)?,
            _ => notes.push(format!(
                "\"{here}\" isn't something a card's tab takes; ignored"
            )),
        }
    }
    Ok(tab)
}

/// A number, or `[across, down]`, each within `range`.
fn pair(value: &Value, place: &str, range: (f32, f32)) -> Result<(f32, f32), Error> {
    match value {
        Value::Array(items) if items.len() == 2 => Ok((
            number(&items[0], place, range)?,
            number(&items[1], place, range)?,
        )),
        Value::Array(_) => Err(Error(format!("\"{place}\" is a number or [across, down]"))),
        _ => {
            let both = number(value, place, range)?;
            Ok((both, both))
        }
    }
}

/// `{ "colour", "from", "to", "height", "max", "inset", "radius" }`.
fn read_gloss(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<Gloss, Error> {
    let mut gloss = Gloss {
        colour: 0xffff_ffff,
        from: 0.6,
        to: 0.1,
        height: 0.5,
        max: 24.0,
        inset: (2.0, 1.0),
        radius: None,
    };
    for (key, value) in object(value, place)? {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "colour" => gloss.colour = colour_at(value, &here)?,
            "from" => gloss.from = number(value, &here, (0.0, 1.0))?,
            "to" => gloss.to = number(value, &here, (0.0, 1.0))?,
            "height" => gloss.height = number(value, &here, (0.1, 0.8))?,
            "max" => gloss.max = number(value, &here, (4.0, 96.0))?,
            "inset" => gloss.inset = pair(value, &here, (0.0, 16.0))?,
            "radius" => gloss.radius = Some(number(value, &here, (0.0, 999.0))?),
            _ => notes.push(format!("\"{here}\" isn't something a gloss takes; ignored")),
        }
    }
    Ok(gloss)
}

/// `{ "colour", "height" }`.
fn read_glow(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<Glow, Error> {
    let mut glow = Glow {
        colour: 0x66ff_ffff,
        height: 12.0,
    };
    for (key, value) in object(value, place)? {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "colour" => glow.colour = colour_at(value, &here)?,
            "height" => glow.height = number(value, &here, (2.0, 48.0))?,
            _ => notes.push(format!("\"{here}\" isn't something a glow takes; ignored")),
        }
    }
    Ok(glow)
}

/// A bubble's ground: a colour or gradient, or `{ "ground": …, "art": […] }`
/// with pictures laid on it, and a drawn gel: `gloss`, `glow`, `rim` and
/// `shadow`.
fn read_bubble(
    value: &Value,
    place: &str,
    notes: &mut Vec<String>,
) -> Result<(Option<Paint>, Vec<Layer>, Gel), Error> {
    let Value::Object(fields) = value else {
        return Ok((Some(paint_at(value, place)?), Vec::new(), Gel::default()));
    };
    let mut ground = None;
    let mut art = Vec::new();
    let mut gel = Gel::default();
    for (key, value) in fields {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "ground" => ground = Some(paint_at(value, &here)?),
            "gloss" => gel.gloss = Some(read_gloss(value, &here, notes)?),
            "glow" => gel.glow = Some(read_glow(value, &here, notes)?),
            "rim" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "light" => gel.rim_light = Some(colour_at(value, &at)?),
                        "dark" => gel.rim_dark = Some(colour_at(value, &at)?),
                        _ => notes.push(format!("\"{at}\" isn't something a rim takes; ignored")),
                    }
                }
            }
            "shadow" => gel.shadow = Some(read_shadow(value, &here)?),
            "art" => {
                let items: Vec<&Value> = match value {
                    Value::Array(items) => items.iter().collect(),
                    other => vec![other],
                };
                if items.len() > MAX_LAYERS {
                    return Err(Error(format!(
                        "\"{here}\" stacks at most {MAX_LAYERS} layers"
                    )));
                }
                for (index, item) in items.into_iter().enumerate() {
                    let at = format!("{here}[{index}]");
                    let layer = read_layer(item, &at, notes)?;
                    if layer.moves() {
                        return Err(Error(format!(
                            "\"{at}\" stays still on its bubble: no drift or frames"
                        )));
                    }
                    art.push(layer);
                }
            }
            _ => notes.push(format!(
                "\"{here}\" isn't something a bubble takes; ignored"
            )),
        }
    }
    Ok((ground, art, gel))
}

/// The outlines in `shape`: `outline_width`, every hairline's width, and
/// `borders`, the outlines dashed or dotted. [`super::package`] reads the
/// rest of `shape` and leaves these two to this.
pub(super) fn read_outlines(
    value: &Value,
    place: &str,
    layout: &mut Layout,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    let fields = object(value, place)?;
    if let Some(width) = fields.get("outline_width") {
        layout.outline_width = Some(number(
            width,
            &format!("{place}.outline_width"),
            (0.0, 4.0),
        )?);
    }
    if let Some(borders) = fields.get("borders") {
        let here = format!("{place}.borders");
        for (name, value) in object(borders, &here)? {
            let at = format!("{here}.{name}");
            let Some(place) = BorderPlace::named(name) else {
                notes.push(format!("\"{at}\" isn't an outline Yapfast dashes; ignored"));
                continue;
            };
            layout
                .borders
                .insert(place, read_border(value, &at, notes)?);
        }
    }
    Ok(())
}

/// The keys of `type.names` that cut names as ransom notes; the rest of the
/// role is [`super::package`]'s.
pub(super) const RANSOM_KEYS: [&str; 3] = ["style", "fonts", "papers"];

/// `type.names.style`: `"ransom"` cuts names into letters.
pub(super) fn read_ransom(value: &Value, place: &str, layout: &mut Layout) -> Result<(), Error> {
    let Some(names) = object(value, place)?.get("names") else {
        return Ok(());
    };
    let at = format!("{place}.names");
    if object(names, &at)?.contains_key("style") {
        layout.names = Some(read_names(names, &at)?);
    }
    Ok(())
}

/// `"dashed"`, or `{ "style": "dotted", "dash": 2, "gap": 4, "width": 2,
/// "colour": "#141414" }`.
fn read_border(value: &Value, at: &str, notes: &mut Vec<String>) -> Result<Border, Error> {
    let stock = |stroke: Stroke| Border {
        stroke,
        dash: if stroke == Stroke::Dotted { 2.0 } else { 6.0 },
        gap: 4.0,
        width: None,
        colour: None,
    };
    if value.is_string() {
        return Ok(stock(named(value, at, Stroke::ALL, Stroke::name)?));
    }
    let fields = object(value, at)?;
    let stroke = match fields.get("style") {
        Some(style) => named(style, &format!("{at}.style"), Stroke::ALL, Stroke::name)?,
        None => Stroke::Dashed,
    };
    let mut border = stock(stroke);
    for (key, value) in fields {
        let here = format!("{at}.{key}");
        match key.as_str() {
            "style" => {}
            "dash" => border.dash = number(value, &here, (1.0, 24.0))?,
            "gap" => border.gap = number(value, &here, (1.0, 24.0))?,
            "width" => border.width = Some(number(value, &here, (1.0, 4.0))?),
            "colour" => border.colour = Some(colour_at(value, &here)?),
            _ => notes.push(format!(
                "\"{here}\" isn't something an outline takes; ignored"
            )),
        }
    }
    Ok(border)
}

/// `{ "style": "ransom", "fonts": [...], "papers": [[ground, ink], ...] }`.
fn read_names(value: &Value, at: &str) -> Result<Names, Error> {
    let fields = object(value, at)?;
    match fields.get("style").and_then(Value::as_str) {
        Some("ransom") => {}
        other => {
            return Err(Error(format!(
                "\"{at}.style\" is ransom, not \"{}\"",
                other.unwrap_or("")
            )));
        }
    }
    let fonts = match fields.get("fonts") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .ok_or_else(|| Error(format!("\"{at}.fonts\" is a list of font files")))
                    .and_then(|text| relative(text, &format!("{at}.fonts")))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(Error(format!("\"{at}.fonts\" is a list of font files"))),
    };
    if fonts.len() == 1 || fonts.len() > 8 {
        return Err(Error(format!("\"{at}.fonts\" lists 2 to 8 fonts")));
    }
    let papers = match fields.get("papers") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let here = format!("{at}.papers[{index}]");
                match item.as_array().map(Vec::as_slice) {
                    Some([ground, ink]) => Ok((colour_at(ground, &here)?, colour_at(ink, &here)?)),
                    _ => Err(Error(format!(
                        "\"{here}\" is a pair of colours, the scrap's and its letter's"
                    ))),
                }
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => {
            return Err(Error(format!(
                "\"{at}.papers\" is a list of [ground, ink] colour pairs"
            )));
        }
    };
    if papers.len() > 6 {
        return Err(Error(format!("\"{at}.papers\" lists at most 6 pairs")));
    }
    Ok(Names { fonts, papers })
}

fn boolean(value: &Value, place: &str) -> Result<bool, Error> {
    value
        .as_bool()
        .ok_or_else(|| Error(format!("\"{place}\" has to be true or false")))
}

/// A short text, at most `most` characters and without control characters.
fn short(value: &Value, place: &str, most: usize) -> Result<String, Error> {
    let text = value
        .as_str()
        .ok_or_else(|| Error(format!("\"{place}\" has to be text")))?;
    if text.chars().count() > most || text.chars().any(super::refused_char) {
        return Err(Error(format!(
            "\"{place}\" is plain text, at most {most} characters, on one line"
        )));
    }
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::package::{Mode, Package};

    fn theme(layers: &str) -> Result<Package, Error> {
        Package::parse(&format!(
            r#"{{ "theme_api": 1, "id": "com.example.lay", "name": "Lay", "version": "1.0.0",
                {layers}
                "variants": {{ "dark": {{ "base": "yap" }} }} }}"#
        ))
    }

    #[test]
    fn reads_the_trim_fields() {
        use crate::themes::chrome::{Label, SystemKind};
        use crate::themes::content::NameColours;
        use crate::themes::package::art::{ArtSurface, TextRole};
        use crate::themes::package::{Role, Surface};
        let package = theme(
            r##""colours": { "mention-chip": "#5cff85" },
            "shape": {
                "label": { "ground": "linear(90deg, #ff4fa8, #e0458f)", "text": "#0c0a0e", "radius": 4, "case": "lower" },
                "header": { "ground": "#131016", "text": "#f6eef3", "border": 2, "border_colour": "#ff4fa8" },
                "card_section": { "ground": "#1b161e", "bar": "#a3175f", "bar_text": "#ffffff" },
                "row": { "stripe": { "colour": "#241521", "lists": ["channels", "members"] } }
            },
            "type": { "small": { "size": 12 }, "time": { "size": 11, "weight": 600 } },
            "art": { "surfaces": { "header": { "texture": "grain" }, "label": { "texture": "paper" },
                                   "footer": { "texture": "halftone", "fit": "contain" } },
                     "footer_height": 80,
                     "text": { "body": { "glow": { "colour": "#5cff85", "size": 2 } } } },
            "layout": { "messages": "cards", "rail": "tree", "tree": { "expand": "many" },
                        "members_style": "grid", "grid": { "columns": 3, "size": 50 },
                        "cards": { "tab": { "text": "{name}", "font": "labels", "colour": "#0c0a0e" } } },
            "content": { "names": { "palette": ["#5cff85", "#c8ff4a", "#7affc0"] } },
            "chrome": { "prefix": { "member_by_role": ["@", "+"] },
                        "labels": { "topic": "[{topic}]", "member_count": "{n} users", "day": "-- Day changed to {date} --" },
                        "system": { "join": "-->", "leave": "<--", "notice": "-!-" },
                        "status": { "text": "[{time}] [{name}] [{server}/{channel}] {typing}", "position": "below" } },"##,
        )
        .unwrap();
        assert!(package.notes.is_empty(), "{:?}", package.notes);
        let resolved = package.resolve(Mode::Dark).unwrap();
        let label = &resolved.shapes[&Surface::Label];
        assert!(label.ground.is_some() && label.text == Some(0xff0c_0a0e) && label.case == Some(1));
        assert_eq!(
            resolved.shapes[&Surface::CardSection].bar_text,
            Some(0xffff_ffff)
        );
        let stripe = resolved.shapes[&Surface::Row].stripe.unwrap();
        assert!(stripe.channels && stripe.members && !stripe.messages);
        assert_eq!(resolved.types[&Role::Time].weight, Some(600));
        assert!(resolved.art.surfaces.contains_key(&ArtSurface::Footer));
        assert_eq!(resolved.art.footer_height, Some(80.0));
        assert!(resolved.art.text.contains_key(&TextRole::Body));
        let layout = &resolved.layout;
        assert_eq!(layout.rail, Some(RailSide::Tree));
        assert!(layout.tree.expand_many);
        assert_eq!(layout.members_style, Some(MembersStyle::Grid));
        assert_eq!((layout.grid.columns, layout.grid.size), (3, 50.0));
        assert_eq!(layout.cards.tab.as_ref().unwrap().text, "{name}");
        assert_eq!(resolved.content.names.colours, NameColours::Palette);
        assert_eq!(resolved.content.names.palette().len(), 3);
        let chrome = &resolved.chrome;
        assert_eq!(chrome.member_by_role, ["@", "+"]);
        assert_eq!(chrome.labels[&Label::MemberCount], "{n} users");
        assert_eq!(chrome.system[&SystemKind::Join], "-->");
        assert!(!chrome.status_line.as_ref().unwrap().above);
        assert!(resolved.paint("mention-chip").is_some());
        // The same person always gets the same colour.
        let names = &resolved.content.names;
        assert_eq!(names.palette_colour("123"), names.palette_colour("123"));
    }

    #[test]
    fn refuses_trim_out_of_its_lists() {
        for layers in [
            r#""layout": { "grid": { "columns": 9 } },"#,
            r#""layout": { "tree": { "expand": "all" } },"#,
            r#""layout": { "cards": { "tab": { "text": "{server}" } } },"#,
            r#""chrome": { "status": { "text": "{weather}" } },"#,
            r#""chrome": { "status": { "position": "middle", "text": "x" } },"#,
            r#""chrome": { "prefix": { "member_by_role": [] } },"#,
            r##""shape": { "row": { "stripe": { "colour": "#000", "lists": ["dms"] } } },"##,
            r#""shape": { "label": { "case": "title" } },"#,
            r##""content": { "names": { "palette": ["#fff"] } },"##,
        ] {
            assert!(theme(layers).is_err(), "{layers}");
        }
    }

    #[test]
    fn reads_every_layout_field() {
        let package = theme(
            r##""layout": {
                "rail": "top", "members": "left", "composer": "top", "messages": "irc",
                "chat_max_width": 900, "chat_avatars": false,
                "irc": { "time_width": 56, "nick_width": 120, "separator": "│", "rule": true },
                "bubbles": { "max_width": 70, "own": "#ff71ce", "other": "#202020", "own_text": "#000000", "radius": 18 },
                "cards": { "tilt": 1.5, "gap": 8, "padding": 12, "ground": "#202020" }
            },
            "shape": { "outline_width": 2, "toggle": { "radius": 0 },
                "borders": { "card": "dashed", "divider": { "style": "dotted", "gap": 3, "width": 2, "colour": "#141414" } } },
            "type": { "names": { "size": 15, "style": "ransom", "fonts": ["a.ttf", "b.otf"],
                           "papers": [["#ffffff", "#000000"], ["#ff71ce", "#141414"]] } },"##,
        )
        .unwrap();
        assert!(package.notes.is_empty(), "{:?}", package.notes);
        let resolved = package.resolve(Mode::Dark).unwrap();
        let layout = &resolved.layout;
        assert_eq!(layout.rail, Some(RailSide::Top));
        assert_eq!(layout.members, Some(MembersSide::Left));
        assert_eq!(layout.composer, Some(ComposerSide::Top));
        assert_eq!(layout.messages, Some(MessageStyle::Irc));
        assert_eq!(layout.chat_max_width, Some(900.0));
        assert_eq!(layout.chat_avatars, Some(false));
        assert_eq!(layout.outline_width, Some(2.0));
        assert_eq!(layout.irc.time_width, Some(56.0));
        assert_eq!(layout.irc.nick_width, Some(120.0));
        assert_eq!(layout.irc.separator.as_deref(), Some("│"));
        assert_eq!(layout.irc.rule, Some(true));
        assert_eq!(layout.bubbles.max_width, Some(70.0));
        assert_eq!(layout.bubbles.own, Some(Paint::Colour(0xffff_71ce)));
        assert_eq!(layout.bubbles.other, Some(Paint::Colour(0xff20_2020)));
        assert_eq!(layout.bubbles.own_text, Some(0xff00_0000));
        assert_eq!(layout.bubbles.radius, Some(18.0));
        assert_eq!(layout.cards.tilt, Some(1.5));
        assert_eq!(layout.cards.gap, Some(8.0));
        assert_eq!(layout.cards.padding, Some(12.0));
        assert_eq!(layout.cards.ground, Some(0xff20_2020));
        let card = layout.borders[&BorderPlace::Card];
        assert_eq!(
            (card.stroke, card.dash, card.gap, card.width),
            (Stroke::Dashed, 6.0, 4.0, None)
        );
        let divider = layout.borders[&BorderPlace::Divider];
        assert_eq!(divider.stroke, Stroke::Dotted);
        assert_eq!(divider.gap, 3.0);
        assert_eq!(divider.width, Some(2.0));
        assert_eq!(divider.colour, Some(0xff14_1414));
        let names = layout.names.as_ref().unwrap();
        assert_eq!(
            names.papers,
            vec![(0xffff_ffff, 0xff00_0000), (0xffff_71ce, 0xff14_1414)]
        );
        assert!(!layout.is_stock());
        // Ransom fonts are the theme's fonts, which an install copies and
        // the check parses.
        let fonts: Vec<_> = resolved
            .fonts()
            .into_iter()
            .map(|font| font.to_owned())
            .collect();
        assert_eq!(fonts, vec![PathBuf::from("a.ttf"), PathBuf::from("b.otf")]);
        let files = package.files();
        assert!(files.contains(&PathBuf::from("a.ttf")) && files.contains(&PathBuf::from("b.otf")));
        let stock = theme("").unwrap().resolve(Mode::Dark).unwrap();
        assert!(stock.layout.is_stock());
    }

    #[test]
    fn reads_the_hover_bar_rows_and_card_extras() {
        let package = theme(
            r##""colours": { "actions": "#202020", "actions-hover": "#ff71ce33" },
            "shape": { "actions": { "radius": 0, "border": 2, "border_colour": "#141414",
                                    "shadow": { "blur": 0, "x": 4, "y": 4, "colour": "#ff48b0" } },
                       "row": { "radius": 6, "inset": [8, 2] },
                       "borders": { "actions": "dashed" } },
            "art": { "surfaces": { "actions": { "texture": "paper", "opacity": 0.3 } },
                     "glass": { "surfaces": ["actions"] },
                     "events": { "row": { "effect": "highlighter", "colour": "#f3e37a" } } },
            "layout": { "messages": "cards",
                "cards": { "shadow": { "blur": 0, "x": 5, "y": 6, "colour": "#14141440" },
                           "art": [{ "image": "art/tape.png", "position": "bottom right", "size": [84, 22],
                                     "offset": [-12, 3], "rotation": -5 },
                                   { "texture": "kraft" }] } },"##,
        )
        .unwrap();
        assert!(package.notes.is_empty(), "{:?}", package.notes);
        let resolved = package.resolve(Mode::Dark).unwrap();
        use crate::themes::package::Surface;
        use crate::themes::package::art::{ArtSurface, EventSlot};
        let actions = &resolved.shapes[&Surface::Actions];
        assert_eq!(actions.border, Some(2.0));
        assert_eq!(
            actions.shadow.map(|shadow| (shadow.x, shadow.blur)),
            Some((4.0, 0.0))
        );
        assert_eq!(resolved.shapes[&Surface::Row].inset, Some((8.0, 2.0)));
        assert!(resolved.paint("actions-hover").is_some());
        assert!(resolved.layout.borders.contains_key(&BorderPlace::Actions));
        assert!(resolved.art.surfaces.contains_key(&ArtSurface::Actions));
        assert!(resolved.art.events.contains_key(&EventSlot::Row));
        let cards = &resolved.layout.cards;
        assert_eq!(cards.shadow.map(|shadow| shadow.y), Some(6.0));
        assert_eq!(cards.art.len(), 2);
        let tape = &cards.art[0];
        assert!(tape.bottom);
        assert_eq!(tape.across, 1.0);
        assert_eq!(tape.size, Some((84.0, 22.0)));
        assert_eq!(tape.offset, (-12.0, 3.0));
        assert_eq!(tape.rotation, -5.0);
        assert!(!cards.art[1].bottom);
        assert_eq!(resolved.layout.files(), vec![PathBuf::from("art/tape.png")]);
        assert!(package.files().contains(&PathBuf::from("art/tape.png")));

        // A row's inset alone, and an inset on any other surface is noted.
        let package =
            theme(r#""shape": { "row": { "inset": 4 }, "card": { "inset": 4 } },"#).unwrap();
        let resolved = package.resolve(Mode::Dark).unwrap();
        assert_eq!(resolved.shapes[&Surface::Row].inset, Some((4.0, 0.0)));
        assert!(
            package
                .notes
                .iter()
                .any(|note| note.contains("shape.card.inset")),
            "{:?}",
            package.notes
        );
    }

    #[test]
    fn reads_gel_bubbles_card_choices_and_the_collapsed_list() {
        let package = theme(
            r##""layout": { "messages": "bubbles", "members": "collapsed",
                "bubbles": {
                    "own": { "ground": "linear(180deg, #7fd4ff, #2a7fff)",
                             "art": [{ "image": "art/gloss.png", "fit": "stretch", "opacity": 0.6 },
                                     { "texture": "grain", "size": [40, 12], "position": "top right" }] },
                    "other": "linear(#ffffff, #e8eef8 80%)" },
                "cards": { "stagger": 18, "tint_art": true,
                           "art": [{ "image": ["art/tape-a.png", "art/tape-b.png", "art/tape-c.png"],
                                     "position": "top", "size": [72, 20] },
                                   { "texture": "paper", "position": "bottom", "size": ["full", 10] },
                                   { "texture": "kraft" }, { "texture": "grain" }] } },
            "art": { "glass": { "gap": 48 },
                     "surfaces": { "profile-pane": { "image": "art/pad.png", "size": [96, 96], "position": [0.5, 0.9] } } },"##,
        )
        .unwrap();
        assert!(package.notes.is_empty(), "{:?}", package.notes);
        let resolved = package.resolve(Mode::Dark).unwrap();
        let layout = &resolved.layout;
        assert_eq!(layout.members, Some(MembersSide::Collapsed));
        assert_eq!(MembersSide::Collapsed.index(), 3);
        assert!(matches!(layout.bubbles.own, Some(Paint::Linear { .. })));
        assert_eq!(
            layout.bubbles.other.as_ref().map(Paint::colours),
            Some(vec![0xffff_ffff, 0xffe8_eef8])
        );
        assert_eq!(layout.bubbles.own_art.len(), 2);
        assert_eq!(layout.bubbles.own_art[1].size, Some((40.0, 12.0)));
        assert!(layout.bubbles.other_art.is_empty());
        let cards = &layout.cards;
        assert_eq!(cards.stagger, Some(18.0));
        assert!(cards.tint_art);
        assert_eq!(cards.art.len(), MAX_CARD_ART);
        assert_eq!(
            cards.art[0].choices,
            vec![
                PathBuf::from("art/tape-b.png"),
                PathBuf::from("art/tape-c.png")
            ]
        );
        assert!(cards.art[1].full && cards.art[1].bottom);
        let files = layout.files();
        for file in [
            "art/gloss.png",
            "art/tape-a.png",
            "art/tape-b.png",
            "art/tape-c.png",
        ] {
            assert!(files.contains(&PathBuf::from(file)), "{file} in {files:?}");
            assert!(package.files().contains(&PathBuf::from(file)), "{file}");
        }
        use crate::themes::package::art::ArtSurface;
        assert_eq!(
            resolved.art.glass.as_ref().map(|glass| glass.gap),
            Some(48.0)
        );
        let pane = &resolved.art.surfaces[&ArtSurface::ProfilePane][0];
        assert_eq!(pane.size, Some((96.0, 96.0)));
        assert_eq!(pane.position, (0.5, 0.9));
    }

    #[test]
    fn reads_a_drawn_gel_on_bubbles() {
        let package = theme(
            r##""layout": { "messages": "bubbles",
                "bubbles": {
                    "padding": [14, 9],
                    "own": { "ground": "linear(180deg, #2a7fff, #0b4fb8)",
                             "gloss": { "from": 0.5, "to": 0.05, "height": 0.45, "max": 20, "inset": [3, 1], "radius": 8 },
                             "glow": { "colour": "#8fd8ff80", "height": 10 },
                             "rim": { "light": "#ffffffaa", "dark": "#06307a" },
                             "shadow": { "blur": 6, "y": 2, "colour": "#0b3a8040" } },
                    "other": { "ground": "#ffffff", "gloss": {} } } },"##,
        )
        .unwrap();
        assert!(package.notes.is_empty(), "{:?}", package.notes);
        let resolved = package.resolve(Mode::Dark).unwrap();
        let bubbles = &resolved.layout.bubbles;
        assert_eq!(bubbles.padding, Some((14.0, 9.0)));
        let gel = &bubbles.own_gel;
        let gloss = gel.gloss.unwrap();
        assert_eq!(
            (gloss.from, gloss.to, gloss.height, gloss.max),
            (0.5, 0.05, 0.45, 20.0)
        );
        assert_eq!((gloss.inset, gloss.radius), ((3.0, 1.0), Some(8.0)));
        assert_eq!(
            gel.glow,
            Some(Glow {
                colour: 0x808f_d8ff,
                height: 10.0
            })
        );
        assert_eq!(
            (gel.rim_light, gel.rim_dark),
            (Some(0xaaff_ffff), Some(0xff06_307a))
        );
        assert_eq!(gel.shadow.map(|shadow| shadow.blur), Some(6.0));
        // A gloss's own defaults.
        let plain = bubbles.other_gel.gloss.unwrap();
        assert_eq!((plain.from, plain.to, plain.max), (0.6, 0.1, 24.0));
        assert!(bubbles.other_gel.glow.is_none());
        // Unset, nothing is drawn.
        let stock = theme(r#""layout": { "messages": "bubbles" },"#).unwrap();
        let stock = stock.resolve(Mode::Dark).unwrap();
        assert!(
            stock.layout.bubbles.own_gel.is_empty() && stock.layout.bubbles.other_gel.is_empty()
        );
        assert_eq!(stock.layout.bubbles.padding, None);
    }

    #[test]
    fn a_gloss_and_a_glow_say_how_much_they_lay_where_text_is() {
        let gloss = Gloss {
            colour: 0xffff_ffff,
            from: 0.6,
            to: 0.0,
            height: 0.5,
            max: 20.0,
            inset: (2.0, 2.0),
            radius: None,
        };
        // Above its top, the whole of `from`; halfway down, half.
        assert!((gloss.alpha_below(0.0) - 0.6).abs() < 1e-6);
        assert!((gloss.alpha_below(12.0) - 0.3).abs() < 1e-6);
        assert_eq!(gloss.alpha_below(22.0), 0.0);
        // A gloss that brightens downward is at its brightest at its edge.
        let rising = Gloss {
            from: 0.1,
            to: 0.4,
            ..gloss
        };
        assert!((rising.alpha_below(15.0) - 0.4).abs() < 1e-6);
        // A translucent colour lays less.
        let faint = Gloss {
            colour: 0x80ff_ffff,
            ..gloss
        };
        assert!((faint.alpha_below(0.0) - 0.6 * 128.0 / 255.0).abs() < 1e-6);
        let glow = Glow {
            colour: 0xffff_ffff,
            height: 10.0,
        };
        assert!((glow.alpha_above(6.0) - 0.4).abs() < 1e-6);
        assert_eq!(glow.alpha_above(12.0), 0.0);
    }

    #[test]
    fn a_gel_is_read_carefully() {
        for (layout, place) in [
            (
                r#""layout": { "bubbles": { "own": { "gloss": { "from": 2 } } } },"#,
                "layout.bubbles.own.gloss.from",
            ),
            (
                r#""layout": { "bubbles": { "own": { "gloss": { "max": 200 } } } },"#,
                "layout.bubbles.own.gloss.max",
            ),
            (
                r#""layout": { "bubbles": { "other": { "glow": { "height": 1 } } } },"#,
                "layout.bubbles.other.glow.height",
            ),
            (
                r#""layout": { "bubbles": { "other": { "rim": { "light": "white-ish" } } } },"#,
                "layout.bubbles.other.rim.light",
            ),
            (
                r#""layout": { "bubbles": { "padding": [1, 2, 3] } },"#,
                "layout.bubbles.padding",
            ),
            (
                r#""layout": { "bubbles": { "padding": 40 } },"#,
                "layout.bubbles.padding",
            ),
        ] {
            let error = theme(layout).unwrap_err().to_string();
            assert!(error.contains(place), "{error} names {place}");
        }
    }

    #[test]
    fn a_pane_keeps_its_content_clear_of_its_art() {
        use crate::themes::package::art::{ArtSurface, Reserve};
        let package = theme(
            r#""art": { "surfaces": {
                "sidebar": [{ "texture": "paper" },
                            { "image": "art/keys.png", "size": [200, 280], "position": "bottom", "reserve": { "bottom": 300 } },
                            { "texture": "grain", "reserve": { "bottom": 120, "left": 8 } }],
                "rail": { "texture": "paper", "reserve": { "top": 40 } } } },"#,
        )
        .unwrap();
        let resolved = package.resolve(Mode::Dark).unwrap();
        assert_eq!(
            resolved.art.reserve(ArtSurface::Sidebar),
            Some(Reserve {
                top: 0.0,
                bottom: 300.0,
                left: 8.0,
                right: 0.0
            })
        );
        assert_eq!(resolved.art.reserve(ArtSurface::Chat), None);
        assert!(
            package
                .notes
                .iter()
                .any(|note| note.contains("art.surfaces.rail[0].reserve")),
            "{:?}",
            package.notes
        );
        let error = theme(r#""art": { "surfaces": { "chat": { "texture": "paper", "reserve": { "top": 5000 } } } },"#)
            .unwrap_err();
        assert!(
            error.0.contains("art.surfaces.chat[0].reserve.top"),
            "{}",
            error.0
        );
    }

    #[test]
    fn refuses_card_art_that_moves_or_crowds() {
        for (layers, place) in [
            (
                r#""layout": { "cards": { "art": [{ "texture": "paper", "drift": [4, 0] }] } },"#,
                "layout.cards.art[0]",
            ),
            (
                r#""layout": { "cards": { "art": [{ "texture": "paper" }, { "texture": "paper" }, { "texture": "paper" }, { "texture": "paper" }, { "texture": "paper" }] } },"#,
                "layout.cards.art",
            ),
            (
                r#""layout": { "cards": { "art": { "image": ["a.png", "b.png", "c.png", "d.png", "e.png", "f.png", "g.png"] } } },"#,
                "layout.cards.art[0].image",
            ),
            (
                r#""layout": { "cards": { "art": { "texture": "paper", "size": [2000, 10] } } },"#,
                "layout.cards.art[0].size[0]",
            ),
            (
                r#""layout": { "cards": { "stagger": 80 } },"#,
                "layout.cards.stagger",
            ),
            (
                r#""layout": { "bubbles": { "own": { "art": { "texture": "paper", "drift": [3, 0] } } } },"#,
                "layout.bubbles.own.art[0]",
            ),
            (
                r#""layout": { "bubbles": { "other": "linear(nope)" } },"#,
                "layout.bubbles.other",
            ),
            (
                r#""art": { "surfaces": { "chat": { "texture": "paper", "size": [2, 40] } } },"#,
                "art.surfaces.chat[0].size[0]",
            ),
            (r#""art": { "glass": { "gap": 80 } },"#, "art.glass.gap"),
            (
                r#""layout": { "cards": { "art": { "texture": "paper", "size": [2, 10] } } },"#,
                "layout.cards.art[0].size[0]",
            ),
            (
                r#""layout": { "cards": { "art": { "texture": "paper", "rotation": 90 } } },"#,
                "layout.cards.art[0].rotation",
            ),
            (
                r#""shape": { "row": { "inset": [8, 20] } },"#,
                "shape.row.inset[1]",
            ),
            (
                r#""art": { "events": { "row": "gloss" } },"#,
                "art.events.row",
            ),
        ] {
            let error = theme(layers)
                .and_then(|package| package.resolve(Mode::Dark))
                .unwrap_err();
            assert!(error.0.contains(place), "{layers}: {}", error.0);
        }
    }

    #[test]
    fn refuses_values_out_of_range_with_their_place() {
        for (layers, place) in [
            (r#""layout": { "rail": "bottom" },"#, "layout.rail"),
            (
                r#""layout": { "chat_max_width": 100 },"#,
                "layout.chat_max_width",
            ),
            (r#""shape": { "outline_width": 9 },"#, "shape.outline_width"),
            (
                r#""layout": { "cards": { "tilt": 30 } },"#,
                "layout.cards.tilt",
            ),
            (
                r#""layout": { "irc": { "separator": "-->>" } },"#,
                "layout.irc.separator",
            ),
            (
                r#""shape": { "borders": { "card": "wavy" } },"#,
                "shape.borders.card",
            ),
            (
                r#""layout": { "chat_avatars": "no" },"#,
                "layout.chat_avatars",
            ),
            (
                r#""type": { "names": { "style": "plain" } },"#,
                "type.names.style",
            ),
            (
                r#""type": { "names": { "style": "ransom", "fonts": ["a.ttf"] } },"#,
                "type.names.fonts",
            ),
            (
                r##""type": { "names": { "style": "ransom", "papers": [["#ffffff"]] } },"##,
                "type.names.papers[0]",
            ),
        ] {
            let error = theme(layers).unwrap_err();
            assert!(error.0.contains(place), "{layers}: {error}");
        }
    }

    #[test]
    fn unknown_names_are_noted() {
        let package = theme(
            r#""layout": { "sidebar": "right", "irc": { "colour": "red" } },
               "shape": { "borders": { "rail": "dashed" } },
               "type": { "names": { "style": "ransom", "tilt": 3 } },
               "parts": { "composer": { "variant": "prompt" } },"#,
        )
        .unwrap();
        let notes = package.notes.join("\n");
        for expected in [
            "layout.sidebar",
            "layout.irc.colour",
            "shape.borders.rail",
            "type.names.tilt",
            "parts.composer",
        ] {
            assert!(notes.contains(expected), "{expected} in {notes}");
        }
    }

    #[test]
    fn a_variant_overrides_the_shared_layout() {
        let package = Package::parse(
            r#"{ "theme_api": 1, "id": "com.example.lay", "name": "Lay", "version": "1.0.0",
                "layout": { "messages": "irc", "rail": "right" },
                "variants": {
                    "dark": { "base": "yap" },
                    "light": { "base": "sakura", "layout": { "messages": "bubbles" } }
                } }"#,
        )
        .unwrap();
        let dark = package.resolve(Mode::Dark).unwrap();
        let light = package.resolve(Mode::Light).unwrap();
        assert_eq!(dark.layout.messages, Some(MessageStyle::Irc));
        assert_eq!(light.layout.messages, Some(MessageStyle::Bubbles));
        assert_eq!(light.layout.rail, Some(RailSide::Right));
    }

    #[test]
    fn the_template_reads_without_notes() {
        let package =
            Package::parse(include_str!("../../../../themes/template/theme.json")).unwrap();
        assert!(package.notes.is_empty(), "{:?}", package.notes);
        assert!(!package.resolve(Mode::Dark).unwrap().layout.is_stock());
    }
}
