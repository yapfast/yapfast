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

use super::package::art::{Layer, Source, read_layer};
use super::package::{Error, Shadow, colour_at, named, number, object, read_shadow, relative};

/// The most pictures one card carries.
pub const MAX_CARD_ART: usize = 2;

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

closed! {
    /// Which edge of the window the server rail runs along.
    RailSide { Left => "left", Right => "right", Top => "top" }
}

closed! {
    /// Where the member list docks, or that it doesn't.
    MembersSide { Right => "right", Left => "left", Hidden => "hidden" }
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
}

/// The `bubbles` message style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bubbles {
    /// The widest a bubble grows, as a percentage of the column (40–95).
    pub max_width: Option<f32>,
    /// ARGB grounds of your own bubbles and everyone else's, and the text on
    /// your own; the others' text is `text`.
    pub own: Option<u32>,
    pub other: Option<u32>,
    pub own_text: Option<u32>,
    /// Corner radius in pixels.
    pub radius: Option<f32>,
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
}

/// A picture on a card's top or bottom edge, leaning with the card.
#[derive(Clone, Debug, PartialEq)]
pub struct CardArt {
    /// The picture or texture, with its opacity, blend (with the card's
    /// ground), blur, dim and colour. Fit is within `size`.
    pub layer: Layer,
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
}

impl Layout {
    /// Whether it changes anything at all.
    pub fn is_stock(&self) -> bool {
        *self == Self::default()
    }

    /// The pictures it names, once each.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = Vec::new();
        for art in &self.cards.art {
            if let Source::Image(path) = &art.layer.source
                && !files.contains(path)
            {
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
        let picture: serde_json::Map<String, Value> = fields
            .iter()
            .filter(|(key, _)| !CARD_ART_KEYS.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
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
                    art.size = Some((
                        number(&pair[0], &format!("{here}[0]"), (4.0, 480.0))?,
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
                        "own" => layout.bubbles.own = Some(colour_at(value, &at)?),
                        "other" => layout.bubbles.other = Some(colour_at(value, &at)?),
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
                        _ => notes.push(format!("\"{at}\" isn't something cards takes; ignored")),
                    }
                }
            }
            _ => notes.push(format!("\"{here}\" isn't something layout takes; ignored")),
        }
    }
    Ok(())
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
    if text.chars().count() > most || text.chars().any(char::is_control) {
        return Err(Error(format!(
            "\"{place}\" is at most {most} characters, on one line"
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
    fn reads_every_layout_field() {
        let package = theme(
            r##""layout": {
                "rail": "top", "members": "left", "composer": "top", "messages": "irc",
                "chat_max_width": 900, "chat_avatars": false,
                "irc": { "time_width": 56, "nick_width": 120, "separator": "│" },
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
        assert_eq!(layout.bubbles.max_width, Some(70.0));
        assert_eq!(layout.bubbles.own, Some(0xffff_71ce));
        assert_eq!(layout.bubbles.other, Some(0xff20_2020));
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
    fn refuses_card_art_that_moves_or_crowds() {
        for (layers, place) in [
            (
                r#""layout": { "cards": { "art": [{ "texture": "paper", "drift": [4, 0] }] } },"#,
                "layout.cards.art[0]",
            ),
            (
                r#""layout": { "cards": { "art": [{ "texture": "paper" }, { "texture": "paper" }, { "texture": "paper" }] } },"#,
                "layout.cards.art",
            ),
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
