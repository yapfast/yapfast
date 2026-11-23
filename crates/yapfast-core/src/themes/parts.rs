//! Named parts: every piece of the window a theme can style by a stable,
//! dotted name (`sidebar.channel-row`, `menu.row`), with the states it
//! takes (hover, pressed, selected, unread, muted, focused) and the
//! properties it takes, from a short, bounded list Yapfast already draws
//! cheaply: a ground, ink, a border, corners, a shadow, opacity, padding and
//! a font weight and size step.
//!
//! [`REGISTRY`] is the one list. `theme.json`'s `parts` layer is read against
//! it ([`read_parts`]), `yapfast theme check` refuses what it doesn't hold
//! ([`Parts::unknown`]), the table in `docs/themes.md` is made from it
//! ([`docs_table`], kept in step by a test), `yapfast theme parts --json`
//! prints it for tools such as a theme maker ([`registry_json`]), and the
//! window's inspector names what it lists.
//!
//! A part's style is resolved once, when the theme loads: its own values,
//! then each state's in [`STATES`]' order over them, so the window only ever
//! looks one up. Values a part leaves out keep what the rest of the theme
//! (its colours, shapes and lists) or Yapfast's stock look draws.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::{Map, Value, json};

use super::package::{Error, Paint, Resolved, Shadow, colour_at, number, object, paint_at};

/// A state a part can be in, as one bit of a set of them.
pub type States = u8;

pub const HOVER: States = 1;
pub const PRESSED: States = 1 << 1;
pub const SELECTED: States = 1 << 2;
pub const UNREAD: States = 1 << 3;
pub const MUTED: States = 1 << 4;
pub const FOCUSED: States = 1 << 5;

/// Every state by its name in `theme.json`, in the order they're laid over
/// a part's own values: a later state's value wins where two are on at once,
/// so a selected row under the pointer shows its selected look.
pub const STATES: [(States, &str); 6] = [
    (UNREAD, "unread"),
    (MUTED, "muted"),
    (HOVER, "hover"),
    (FOCUSED, "focused"),
    (PRESSED, "pressed"),
    (SELECTED, "selected"),
];

/// A property a part can take, as one bit of a set of them.
pub type Props = u16;

pub const GROUND: Props = 1;
pub const INK: Props = 1 << 1;
pub const BORDER: Props = 1 << 2;
pub const BORDER_COLOUR: Props = 1 << 3;
pub const RADIUS: Props = 1 << 4;
pub const SHADOW: Props = 1 << 5;
pub const OPACITY: Props = 1 << 6;
pub const PADDING: Props = 1 << 7;
pub const WEIGHT: Props = 1 << 8;
pub const SIZE_STEP: Props = 1 << 9;
pub const HIDDEN: Props = 1 << 10;
pub const CASE: Props = 1 << 11;
pub const ITALIC: Props = 1 << 12;
pub const PICTURE: Props = 1 << 13;
pub const ICON_INK: Props = 1 << 14;

/// Every property by its name in `theme.json`, with what it takes.
pub const PROPS: [(Props, &str, &str); 15] = [
    (
        GROUND,
        "ground",
        "a colour or a linear() gradient, as `colours` writes them",
    ),
    (INK, "ink", "the colour of its words and glyphs"),
    (
        BORDER,
        "border",
        "its border's width, 0 to 8 pixels: one number, or [top, right, bottom, left]",
    ),
    (BORDER_COLOUR, "border_colour", "the border's colour"),
    (
        RADIUS,
        "radius",
        "its corners, 0 to 999 pixels (999 a pill): one number, or [top left, top right, bottom right, bottom left]",
    ),
    (
        SHADOW,
        "shadow",
        "`none`, or `{ blur, x, y, colour }` as `shape` writes one",
    ),
    (
        OPACITY,
        "opacity",
        "0 to 1, laid on its ground, ink, border, shadow and picture; 0 leaves it invisible but in its place",
    ),
    (
        PADDING,
        "padding",
        "0 to 48 pixels: one number, [vertical, horizontal] or [top, right, bottom, left]",
    ),
    (WEIGHT, "weight", "its words' weight, 100 to 900"),
    (
        SIZE_STEP,
        "size_step",
        "pixels added to its words' size, -4 to 12",
    ),
    (
        HIDDEN,
        "hidden",
        "true takes it out of the window and its room with it; in its own values or unread, muted or selected",
    ),
    (
        CASE,
        "case",
        "its words in `upper`, `lower` or `none` (as written)",
    ),
    (ITALIC, "italic", "true slants its words"),
    (
        PICTURE,
        "picture",
        "a picture in the theme under what's in it: a file, or `{ file, fit }` with fit `cover`, `contain` or `tile`",
    ),
    (
        ICON_INK,
        "icon_ink",
        "the colour of its icons; its ink when left out",
    ),
];

/// A box: its ground, border, corners, shadow and picture.
const BOX: Props = GROUND | BORDER | BORDER_COLOUR | RADIUS | SHADOW | PICTURE;
/// Words: their ink, weight, size, slant and case.
const WORDS: Props = INK | WEIGHT | SIZE_STEP | ITALIC | CASE;
/// What every part takes: it can be faded or taken out.
const EVERY: Props = OPACITY | HIDDEN;

/// One named part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartInfo {
    /// Its stable, dotted name, grouped by area.
    pub name: &'static str,
    /// What it is, in a few words.
    pub about: &'static str,
    /// The states it takes.
    pub states: States,
    /// The properties it takes.
    pub props: Props,
    /// The colour tokens it's drawn with when nothing styles it: its ground
    /// and its ink. The check measures a part's ink on its ground against
    /// these, so no part reads worse than the stock pair it replaces.
    pub ground: Option<&'static str>,
    pub ink: Option<&'static str>,
}

const fn part(
    name: &'static str,
    about: &'static str,
    states: States,
    props: Props,
    ground: Option<&'static str>,
    ink: Option<&'static str>,
) -> PartInfo {
    PartInfo {
        name,
        about,
        states,
        props: props | EVERY,
        ground,
        ink,
    }
}

/// Every part a theme can style. Names are public API under `theme_api` 1:
/// one is only ever added, never renamed or taken away, and a part only
/// gains states and properties.
pub const REGISTRY: &[PartInfo] = &[
    // The server rail.
    part(
        "rail",
        "the server rail's ground",
        0,
        GROUND | PICTURE,
        Some("rail"),
        None,
    ),
    part(
        "rail.home",
        "the rail's Direct Messages tile",
        HOVER | SELECTED | UNREAD,
        BOX | INK | ICON_INK,
        Some("raised"),
        Some("text"),
    ),
    part(
        "rail.server",
        "a server's tile in the rail: on a picture, its ground shows round it inside its padding and its ink rings it",
        HOVER | SELECTED | UNREAD | MUTED,
        BOX | INK | PADDING,
        Some("raised"),
        Some("text"),
    ),
    part(
        "rail.folder",
        "a closed server folder's tile",
        HOVER | UNREAD,
        BOX,
        None,
        None,
    ),
    // The channel list.
    part(
        "sidebar",
        "the channel list's ground",
        0,
        GROUND | PICTURE,
        Some("sidebar"),
        None,
    ),
    part(
        "sidebar.header",
        "the server's name over the channel list",
        HOVER,
        BOX | WORDS,
        Some("sidebar"),
        Some("text"),
    ),
    part(
        "sidebar.category",
        "a channel category's heading",
        HOVER,
        BOX | WORDS | PADDING,
        Some("sidebar"),
        Some("muted"),
    ),
    part(
        "sidebar.channel-row",
        "a channel's row",
        HOVER | SELECTED | UNREAD | MUTED,
        BOX | WORDS | PADDING,
        Some("sidebar"),
        Some("muted"),
    ),
    part(
        "sidebar.channel-row.icon",
        "the glyph before a channel's name",
        HOVER | SELECTED | UNREAD | MUTED,
        INK,
        Some("sidebar"),
        Some("muted"),
    ),
    part(
        "sidebar.dm-row",
        "a direct message's row",
        HOVER | SELECTED | UNREAD,
        BOX | INK | WEIGHT | SIZE_STEP | CASE | PADDING,
        Some("sidebar"),
        Some("muted"),
    ),
    // The account and voice panels at the channel list's foot.
    part(
        "account",
        "the account panel at the channel list's foot",
        0,
        BOX,
        Some("panel"),
        None,
    ),
    part(
        "account.button",
        "the account panel's microphone, sound and settings buttons",
        HOVER | PRESSED | SELECTED,
        BOX | INK | ICON_INK,
        Some("panel"),
        Some("secondary"),
    ),
    part(
        "voice.panel",
        "the voice-connected panel over the account panel",
        0,
        BOX | PADDING,
        Some("panel"),
        None,
    ),
    // The bar over the conversation.
    part(
        "header",
        "the bar over the conversation",
        0,
        BOX | WORDS,
        Some("chat"),
        Some("text"),
    ),
    part(
        "header.button",
        "the header's icon buttons",
        HOVER | PRESSED | SELECTED,
        BOX | INK | ICON_INK,
        Some("chat"),
        Some("secondary"),
    ),
    // The conversation.
    part(
        "timeline",
        "the conversation's ground",
        0,
        GROUND | PICTURE,
        Some("chat"),
        None,
    ),
    part(
        "timeline.message",
        "a message's row",
        HOVER | SELECTED,
        BOX | PADDING,
        Some("chat"),
        Some("text"),
    ),
    part(
        "timeline.message.author",
        "the name over a message",
        HOVER,
        WEIGHT | SIZE_STEP | ITALIC | CASE,
        Some("chat"),
        Some("text"),
    ),
    part(
        "timeline.message.time",
        "a message's time",
        HOVER,
        WORDS,
        Some("chat"),
        Some("faint"),
    ),
    part(
        "timeline.reaction",
        "a reaction under a message",
        HOVER | SELECTED,
        BOX | WORDS,
        Some("raised"),
        Some("secondary"),
    ),
    part(
        "timeline.embed",
        "a link's or a bot's embed",
        0,
        BOX,
        Some("embed"),
        None,
    ),
    part("timeline.code", "a code block", 0, BOX, Some("code"), None),
    part(
        "timeline.actions",
        "the actions over a message under the pointer",
        0,
        BOX,
        Some("actions"),
        None,
    ),
    // The message box.
    part(
        "composer",
        "the message box",
        FOCUSED,
        BOX,
        Some("composer"),
        None,
    ),
    part(
        "composer.button",
        "the message box's buttons: attach, GIF, stickers, emoji",
        HOVER | PRESSED,
        INK | ICON_INK,
        Some("composer"),
        Some("secondary"),
    ),
    // The member list.
    part(
        "members",
        "the member list's ground",
        0,
        GROUND | PICTURE,
        Some("members"),
        None,
    ),
    part(
        "members.heading",
        "a member group's heading",
        0,
        BOX | WORDS | PADDING,
        Some("members"),
        Some("muted"),
    ),
    part(
        "members.row",
        "a member's row",
        HOVER | SELECTED,
        BOX | PADDING,
        Some("members"),
        Some("text"),
    ),
    part(
        "members.row.name",
        "a member's name in their row",
        HOVER | SELECTED,
        WEIGHT | SIZE_STEP | CASE,
        Some("members"),
        Some("text"),
    ),
    // What opens over the window.
    part(
        "menu",
        "a menu from a button or a right click",
        0,
        BOX,
        Some("menu"),
        None,
    ),
    part(
        "menu.row",
        "a menu's row",
        HOVER | PRESSED,
        BOX | WORDS | ICON_INK,
        Some("menu"),
        Some("secondary"),
    ),
    part(
        "tooltip",
        "the window's tooltip",
        0,
        BOX | WORDS,
        Some("tooltip"),
        Some("text"),
    ),
    part(
        "dialog",
        "a popup with a dimmed backdrop",
        0,
        BOX,
        Some("raised"),
        None,
    ),
    part(
        "popout",
        "pins, threads, the inbox and other popouts from the bars",
        0,
        BOX,
        Some("sidebar"),
        None,
    ),
    part(
        "toast",
        "notes over the window and the conversation",
        0,
        BOX,
        Some("raised"),
        None,
    ),
    // Settings.
    part(
        "settings.nav-row",
        "a row of Settings' section list",
        HOVER | SELECTED,
        BOX | WORDS | ICON_INK,
        Some("sidebar"),
        Some("muted"),
    ),
    // Calls.
    part(
        "call.tile",
        "a person's tile on a call's stage",
        HOVER,
        BOX,
        Some("stage"),
        None,
    ),
    // Profiles.
    part(
        "profile.card",
        "a profile card over the window",
        0,
        BOX,
        Some("raised"),
        None,
    ),
    // Controls everywhere.
    part(
        "button",
        "a labelled button",
        HOVER | PRESSED,
        BOX | WORDS,
        None,
        None,
    ),
    part(
        "field",
        "a text field or search box",
        HOVER | FOCUSED,
        BOX,
        Some("field"),
        None,
    ),
];

/// The registry's index of the part named `name`.
pub fn index(name: &str) -> Option<usize> {
    REGISTRY.iter().position(|part| part.name == name)
}

/// The part named `name`.
pub fn find(name: &str) -> Option<&'static PartInfo> {
    index(name).map(|index| &REGISTRY[index])
}

/// What a part, or one state of it, sets. A value it leaves out keeps the
/// one under it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub ground: Option<Paint>,
    /// ARGB.
    pub ink: Option<u32>,
    /// Top, right, bottom, left.
    pub border: Option<[f32; 4]>,
    /// ARGB.
    pub border_colour: Option<u32>,
    /// Top left, top right, bottom right, bottom left.
    pub radius: Option<[f32; 4]>,
    /// A shadow, or none at all (`"shadow": "none"`) as `Some(None)`.
    pub shadow: Option<Option<Shadow>>,
    pub opacity: Option<f32>,
    /// Top, right, bottom, left.
    pub padding: Option<[f32; 4]>,
    pub weight: Option<u16>,
    pub size_step: Option<f32>,
    /// Out of the window, room and all.
    pub hidden: Option<bool>,
    pub case: Option<Case>,
    pub italic: Option<bool>,
    pub picture: Option<Picture>,
    /// ARGB.
    pub icon_ink: Option<u32>,
}

/// A part's words' case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    /// As written, undoing a case set under it.
    None,
}

/// How a part's picture fills it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fit {
    /// All of it, cut to its shape.
    #[default]
    Cover,
    /// All of the picture, inside it.
    Contain,
    /// Repeated at its own size from the top left.
    Tile,
}

/// A part's picture: a file in the theme, and how it fills the part.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub file: std::path::PathBuf,
    pub fit: Fit,
}

impl Style {
    /// `over`'s values laid on these: each it sets replaces this one's.
    pub fn layer(&mut self, over: &Style) {
        macro_rules! take {
            ($($field:ident),*) => {
                $(if over.$field.is_some() {
                    self.$field.clone_from(&over.$field);
                })*
            };
        }
        take!(
            ground,
            ink,
            border,
            border_colour,
            radius,
            shadow,
            opacity,
            padding,
            weight,
            size_step,
            hidden,
            case,
            italic,
            picture,
            icon_ink
        );
    }

    /// Whether it sets nothing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One part's rules: its own values and each state's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rules {
    pub base: Style,
    /// In [`STATES`]' order.
    pub states: [Option<Style>; 6],
}

/// What styled a part in one set of states: its own values, and the states
/// whose values were laid over them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct From {
    pub base: bool,
    pub states: States,
}

impl Rules {
    /// The style it shows in `states`, and what made it.
    pub fn resolve(&self, states: States) -> (Style, From) {
        let mut style = self.base.clone();
        let mut from = From {
            base: !self.base.is_empty(),
            states: 0,
        };
        for ((state, _), layer) in STATES.iter().zip(&self.states) {
            if states & state != 0
                && let Some(layer) = layer
            {
                style.layer(layer);
                from.states |= state;
            }
        }
        (style, from)
    }
}

/// A variant's parts: the rules of each it styles, by its index in
/// [`REGISTRY`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parts {
    pub rules: BTreeMap<usize, Rules>,
    /// The names and properties this Yapfast doesn't have, each as a
    /// sentence. The window ignores them, so a theme for a later Yapfast still
    /// opens, and `yapfast theme check` refuses them.
    pub unknown: Vec<String>,
}

impl Parts {
    /// The rules of the part named `name`, if the theme styles it.
    pub fn get(&self, name: &str) -> Option<&Rules> {
        self.rules.get(&index(name)?)
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Every picture its parts name, once each, relative to the theme.
    pub fn files(&self) -> Vec<std::path::PathBuf> {
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        for rules in self.rules.values() {
            for style in std::iter::once(&rules.base).chain(rules.states.iter().flatten()) {
                if let Some(picture) = &style.picture
                    && !files.contains(&picture.file)
                {
                    files.push(picture.file.clone());
                }
            }
        }
        files
    }
}

/// Reads a `parts` layer over what `parts` already holds: each value a
/// later layer sets (a variant's over the shared one) replaces that one
/// value.
pub(super) fn read_parts(
    value: &Value,
    place: &str,
    parts: &mut Parts,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    let mut unknown = |sentence: String, parts: &mut Parts| {
        notes.push(format!("{sentence}; ignored"));
        if !parts.unknown.contains(&sentence) {
            parts.unknown.push(sentence);
        }
    };
    for (name, written) in object(value, place)? {
        let at = format!("{place}.{name}");
        let Some(index) = index(name) else {
            let close = closest(name)
                .map_or_else(String::new, |near| format!(" (did you mean \"{near}\"?)"));
            unknown(
                format!(
                    "\"{at}\" isn't a part Yapfast has{close}; `yapfast theme parts` lists them"
                ),
                parts,
            );
            continue;
        };
        let info = &REGISTRY[index];
        let mut base = Style::default();
        let mut states: [Option<Style>; 6] = Default::default();
        for (key, value) in object(written, &at)? {
            let here = format!("{at}.{key}");
            if let Some(slot) = STATES.iter().position(|(_, state)| state == key) {
                let (state, _) = STATES[slot];
                if info.states & state == 0 {
                    unknown(
                        format!(
                            "\"{here}\": {} has no {key} state; it takes {}",
                            info.name,
                            names(info.states, &STATES.map(|(bit, name)| (bit, name)))
                                .unwrap_or_else(|| "none".into())
                        ),
                        parts,
                    );
                    continue;
                }
                let mut style = Style::default();
                for (key, value) in object(value, &here)? {
                    let there = format!("{here}.{key}");
                    if STATES.iter().any(|(_, state)| state == key) {
                        unknown(
                            format!("\"{there}\": states don't nest; write each at the part's top"),
                            parts,
                        );
                        continue;
                    }
                    if key == "hidden" && state & (HOVER | PRESSED | FOCUSED) != 0 {
                        // Gone under the pointer, it'd come straight back.
                        unknown(
                            format!(
                                "\"{there}\": a part can be hidden in its own values or unread, muted or selected, not {}",
                                STATES[slot].1
                            ),
                            parts,
                        );
                        continue;
                    }
                    if !read_prop(info, key, value, &there, &mut style)? {
                        unknown(prop_sentence(info, key, &there), parts);
                    }
                }
                states[slot] = Some(style);
                continue;
            }
            if !read_prop(info, key, value, &here, &mut base)? {
                unknown(prop_sentence(info, key, &here), parts);
            }
        }
        let rules = parts.rules.entry(index).or_default();
        rules.base.layer(&base);
        for (have, wrote) in rules.states.iter_mut().zip(states) {
            if let Some(wrote) = wrote {
                have.get_or_insert_with(Style::default).layer(&wrote);
            }
        }
    }
    Ok(())
}

fn prop_sentence(info: &PartInfo, key: &str, here: &str) -> String {
    if PROPS.iter().any(|(_, name, _)| *name == key) {
        format!(
            "\"{here}\": {} doesn't take {key}; it takes {}",
            info.name,
            names_of_props(info.props)
        )
    } else {
        format!(
            "\"{here}\" isn't a property or state Yapfast has; {} takes {}",
            info.name,
            names_of_props(info.props)
        )
    }
}

/// Reads one property into `style`; `false` when the part doesn't take it.
fn read_prop(
    info: &PartInfo,
    key: &str,
    value: &Value,
    here: &str,
    style: &mut Style,
) -> Result<bool, Error> {
    let Some((prop, ..)) = PROPS.iter().find(|(_, name, _)| *name == key) else {
        return Ok(false);
    };
    if info.props & prop == 0 {
        return Ok(false);
    }
    match *prop {
        GROUND => style.ground = Some(paint_at(value, here)?),
        INK => style.ink = Some(colour_at(value, here)?),
        BORDER => style.border = Some(four(value, here, (0.0, 8.0), "[top, right, bottom, left]")?),
        BORDER_COLOUR => style.border_colour = Some(colour_at(value, here)?),
        RADIUS => {
            style.radius = Some(four(
                value,
                here,
                (0.0, 999.0),
                "[top left, top right, bottom right, bottom left]",
            )?);
        }
        SHADOW => {
            style.shadow = Some(if value.as_str() == Some("none") {
                None
            } else {
                Some(super::package::read_shadow(value, here)?)
            });
        }
        OPACITY => style.opacity = Some(number(value, here, (0.0, 1.0))?),
        PADDING => {
            let side = |value: &Value, at: &str| number(value, at, (0.0, 48.0));
            style.padding = Some(match value {
                Value::Array(items) if items.len() == 2 => {
                    let vertical = side(&items[0], &format!("{here}[0]"))?;
                    let horizontal = side(&items[1], &format!("{here}[1]"))?;
                    [vertical, horizontal, vertical, horizontal]
                }
                Value::Array(items) if items.len() == 4 => {
                    let mut sides = [0.0; 4];
                    for (index, (item, side_value)) in items.iter().zip(&mut sides).enumerate() {
                        *side_value = side(item, &format!("{here}[{index}]"))?;
                    }
                    sides
                }
                Value::Array(_) => {
                    return Err(Error(format!(
                        "\"{here}\" is one number, [vertical, horizontal] or [top, right, bottom, left]"
                    )));
                }
                _ => [side(value, here)?; 4],
            });
        }
        WEIGHT => {
            let weight = number(value, here, (100.0, 900.0))?;
            if weight % 100.0 != 0.0 {
                return Err(Error(format!(
                    "\"{here}\" is {weight}; a weight is a hundred: 100, 200 … 900"
                )));
            }
            style.weight = Some(weight as u16);
        }
        SIZE_STEP => {
            let step = number(value, here, (-4.0, 12.0))?;
            style.size_step = Some(step);
        }
        HIDDEN => style.hidden = Some(boolean(value, here)?),
        ITALIC => style.italic = Some(boolean(value, here)?),
        CASE => {
            style.case = Some(match value.as_str() {
                Some("upper") => Case::Upper,
                Some("lower") => Case::Lower,
                Some("none") => Case::None,
                _ => return Err(Error(format!("\"{here}\" is upper, lower or none"))),
            });
        }
        ICON_INK => style.icon_ink = Some(colour_at(value, here)?),
        PICTURE => {
            let (file, fit) = match value {
                Value::String(file) => (file.as_str(), None),
                Value::Object(fields) => (
                    fields.get("file").and_then(Value::as_str).ok_or_else(|| {
                        Error(format!("\"{here}\" needs a \"file\" in the theme"))
                    })?,
                    fields.get("fit"),
                ),
                _ => {
                    return Err(Error(format!(
                        "\"{here}\" is a file in the theme, or {{ \"file\", \"fit\" }}"
                    )));
                }
            };
            let fit = match fit.map(|fit| fit.as_str()) {
                None => Fit::Cover,
                Some(Some("cover")) => Fit::Cover,
                Some(Some("contain")) => Fit::Contain,
                Some(Some("tile")) => Fit::Tile,
                Some(_) => {
                    return Err(Error(format!("\"{here}.fit\" is cover, contain or tile")));
                }
            };
            let file = super::package::relative(file, here)?;
            let picture = file
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase);
            if !matches!(picture.as_deref(), Some("png" | "jpg" | "jpeg" | "webp")) {
                return Err(Error(format!(
                    "\"{here}\" names {}: a part's picture is a .png, .jpg or .webp",
                    file.display()
                )));
            }
            style.picture = Some(Picture { file, fit });
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// `true` or `false`.
fn boolean(value: &Value, here: &str) -> Result<bool, Error> {
    value
        .as_bool()
        .ok_or_else(|| Error(format!("\"{here}\" is true or false")))
}

/// One number for all four, or four of them, each in `range`.
fn four(value: &Value, here: &str, range: (f32, f32), order: &str) -> Result<[f32; 4], Error> {
    match value {
        Value::Array(items) if items.len() == 4 => {
            let mut four = [0.0; 4];
            for (index, (item, slot)) in items.iter().zip(&mut four).enumerate() {
                *slot = number(item, &format!("{here}[{index}]"), range)?;
            }
            Ok(four)
        }
        Value::Array(_) => Err(Error(format!("\"{here}\" is one number or four: {order}"))),
        _ => Ok([number(value, here, range)?; 4]),
    }
}

/// The names of the states in `set`, joined, or `None` for none.
fn names(set: States, from: &[(States, &str); 6]) -> Option<String> {
    let names: Vec<&str> = from
        .iter()
        .filter(|(bit, _)| set & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    (!names.is_empty()).then(|| names.join(", "))
}

fn names_of_props(set: Props) -> String {
    PROPS
        .iter()
        .filter(|(bit, ..)| set & bit != 0)
        .map(|(_, name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The states a part takes, by name, in [`STATES`]' order.
pub fn state_names(set: States) -> Vec<&'static str> {
    STATES
        .iter()
        .filter(|(bit, _)| set & bit != 0)
        .map(|(_, name)| *name)
        .collect()
}

/// The properties a part takes, by name, in [`PROPS`]' order.
pub fn prop_names(set: Props) -> Vec<&'static str> {
    PROPS
        .iter()
        .filter(|(bit, ..)| set & bit != 0)
        .map(|(_, name, _)| *name)
        .collect()
}

/// The registered name nearest a mistyped one: the same once `_` and `-`
/// are one, or a name it starts or ends, so `sidebar.channel_row` and
/// `channel-row` point at `sidebar.channel-row`.
fn closest(name: &str) -> Option<&'static str> {
    let folded = name.to_ascii_lowercase().replace('_', "-");
    REGISTRY
        .iter()
        .map(|part| part.name)
        .find(|known| *known == folded)
        .or_else(|| {
            REGISTRY
                .iter()
                .map(|part| part.name)
                .find(|known| known.ends_with(&format!(".{folded}")))
        })
}

/// The table of parts in `docs/themes.md`, between its `parts` markers.
pub fn docs_table() -> String {
    let mut out =
        String::from("| Part | What it is | States | Properties |\n| --- | --- | --- | --- |\n");
    for part in REGISTRY {
        let states = state_names(part.states);
        let states = if states.is_empty() {
            "–".to_owned()
        } else {
            states.join(", ")
        };
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            part.name,
            part.about,
            states,
            prop_names(part.props).join(", ")
        );
    }
    out
}

/// The registry as `yapfast theme parts` prints it: each part's name and
/// what it is, its states and its properties.
pub fn listing() -> String {
    let mut out = String::new();
    for part in REGISTRY {
        let states = state_names(part.states);
        let _ = writeln!(
            out,
            "{}  {}\n    states: {}\n    properties: {}",
            part.name,
            part.about,
            if states.is_empty() {
                "none".to_owned()
            } else {
                states.join(", ")
            },
            prop_names(part.props).join(", ")
        );
    }
    out
}

/// The registry as `yapfast theme parts --json` prints it: each part's
/// name, what it is, its states, its properties and its stock colour
/// tokens, and each property's range.
pub fn registry_json() -> Value {
    let parts: Vec<Value> = REGISTRY
        .iter()
        .map(|part| {
            json!({
                "name": part.name,
                "about": part.about,
                "states": state_names(part.states),
                "properties": prop_names(part.props),
                "ground": part.ground,
                "ink": part.ink,
            })
        })
        .collect();
    let props: Map<String, Value> = PROPS
        .iter()
        .map(|(_, name, takes)| ((*name).to_owned(), Value::String((*takes).to_owned())))
        .collect();
    json!({
        "theme_api": super::THEME_API,
        "states": STATES.iter().map(|(_, name)| *name).collect::<Vec<_>>(),
        "properties": props,
        "parts": parts,
    })
}

/// Every set of `allowed`'s states, the empty one first.
fn every_set(allowed: States) -> Vec<States> {
    let bits: Vec<States> = STATES
        .iter()
        .map(|(bit, _)| *bit)
        .filter(|bit| allowed & bit != 0)
        .collect();
    (0..1u32 << bits.len())
        .map(|pick| {
            bits.iter()
                .enumerate()
                .filter(|(index, _)| pick & (1 << index) != 0)
                .fold(0, |set, (_, bit)| set | bit)
        })
        .collect()
}

/// [`every_set`], the smaller sets first.
fn every_set_smallest_first(allowed: States) -> Vec<States> {
    let mut sets = every_set(allowed);
    sets.sort_by_key(|set| (set.count_ones(), *set));
    sets
}

/// Every part whose ink would read under its stock pair's contrast (and at
/// most 4.5:1) on its ground, in any set of its states, as a sentence. A
/// part's opacity fades its ink and ground both, over the stock ground.
pub fn unreadable(variant: &Resolved) -> Vec<String> {
    let mut refused = Vec::new();
    for (index, rules) in &variant.parts.rules {
        let info = &REGISTRY[*index];
        if info.props & INK == 0 && info.props & GROUND == 0 {
            continue;
        }
        let (Some(ground_token), Some(ink_token)) = (info.ground, info.ink) else {
            continue;
        };
        let (Some(stock_grounds), Some(stock_inks)) = (
            super::check::shown_colours(variant, ground_token),
            super::check::shown_colours(variant, ink_token),
        ) else {
            continue;
        };
        let stock_ground = stock_grounds[0];
        let own = super::check::base_colour(&variant.base, ink_token)
            .zip(super::check::base_colour(&variant.base, ground_token))
            .map_or(super::package::READABLE, |(front, back)| {
                super::check::contrast(front, back)
            });
        let needed = super::package::READABLE.min(own) - 0.005;
        // A muted row is drawn at half its ink by Yapfast itself, as
        // Discord's: a muted state may fade as far, no further.
        let muted_needed = super::check::base_colour(&variant.base, ink_token)
            .zip(super::check::base_colour(&variant.base, ground_token))
            .map_or(needed, |(front, back)| {
                needed
                    .min(super::check::contrast(over(faded(front, MUTED_FADE), back), back) - 0.005)
            });
        // The fewest states that break it, once a part.
        for states in every_set_smallest_first(info.states) {
            let needed = if states & MUTED != 0 {
                muted_needed
            } else {
                needed
            };
            let (style, _) = rules.resolve(states);
            if style.ink.is_none() && style.ground.is_none() && style.opacity.is_none() {
                continue;
            }
            // Hidden or faded to nothing on purpose: nothing to read.
            if style.hidden == Some(true) || style.opacity == Some(0.0) {
                continue;
            }
            let opacity = style.opacity.unwrap_or(1.0);
            let grounds: Vec<u32> = style
                .ground
                .as_ref()
                .map_or_else(|| stock_grounds.clone(), Paint::colours)
                .into_iter()
                .map(|ground| over(faded(ground, opacity), stock_ground))
                .collect();
            let inks: Vec<u32> = style
                .ink
                .map_or_else(|| stock_inks.clone(), |ink| vec![ink]);
            let worst = inks
                .iter()
                .flat_map(|ink| {
                    grounds.iter().map(move |ground| {
                        super::check::contrast(over(faded(*ink, opacity), *ground), *ground)
                    })
                })
                .fold(f64::INFINITY, f64::min);
            if worst < needed {
                let state = if states == 0 {
                    String::new()
                } else {
                    format!(" ({})", state_names(states).join(" + "))
                };
                let sentence = format!(
                    "parts.{}{state}: its ink contrasts {worst:.1}:1 on its ground, under {:.1}:1, which may be hard to read",
                    info.name,
                    needed + 0.005
                );
                if !refused.contains(&sentence) {
                    refused.push(sentence);
                }
                break;
            }
        }
    }
    refused
}

/// How far Yapfast fades a muted row's ink by itself.
const MUTED_FADE: f32 = 0.5;

/// `colour` with its alpha times `opacity`.
fn faded(colour: u32, opacity: f32) -> u32 {
    let alpha = ((colour >> 24) as f32 * opacity).round() as u32;
    (alpha.min(255) << 24) | (colour & 0x00ff_ffff)
}

/// `top` laid over the opaque `under`.
fn over(top: u32, under: u32) -> u32 {
    let alpha = (top >> 24) as f32 / 255.0;
    let mix = |shift: u32| {
        let top = ((top >> shift) & 0xff) as f32;
        let under = ((under >> shift) & 0xff) as f32;
        ((top * alpha + under * (1.0 - alpha)).round() as u32) << shift
    };
    0xff00_0000 | mix(16) | mix(8) | mix(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::Package;
    use crate::themes::package::Mode;

    fn theme(parts: &str) -> Result<Package, Error> {
        Package::parse(&format!(
            r#"{{ "theme_api": 1, "id": "com.example.parts", "name": "Parts", "version": "1.0.0",
                 "parts": {parts},
                 "variants": {{ "dark": {{ "base": "yap" }} }} }}"#
        ))
    }

    #[test]
    fn the_registry_names_each_part_once_in_dotted_lowercase() {
        let mut seen = std::collections::BTreeSet::new();
        for part in REGISTRY {
            assert!(seen.insert(part.name), "{} twice", part.name);
            assert!(
                part.name.split('.').all(|step| !step.is_empty()
                    && step.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')),
                "{}",
                part.name
            );
            assert!(part.props != 0, "{} takes nothing", part.name);
            assert!(!part.about.is_empty());
            for token in part.ground.iter().chain(&part.ink) {
                assert!(
                    crate::themes::check::base_colour("yap", token).is_some()
                        || super::super::package::follows(token).is_some(),
                    "{}: {token}",
                    part.name
                );
            }
        }
        // The names the docs and the inspector rely on.
        for name in [
            "sidebar.channel-row",
            "sidebar.channel-row.icon",
            "rail.server",
            "rail.folder",
            "timeline.message",
            "timeline.message.author",
            "composer",
            "composer.button",
            "members.row",
            "header",
            "settings.nav-row",
            "menu",
            "menu.row",
            "tooltip",
            "voice.panel",
            "call.tile",
            "profile.card",
        ] {
            assert!(find(name).is_some(), "{name}");
        }
    }

    #[test]
    fn the_docs_table_is_the_registry() {
        let docs = include_str!("../../../../docs/themes.md");
        let start = "<!-- parts:start -->\n";
        let end = "<!-- parts:end -->";
        let from = docs
            .find(start)
            .expect("docs/themes.md has the parts markers")
            + start.len();
        let to = docs[from..].find(end).expect("and their end") + from;
        assert_eq!(
            &docs[from..to],
            docs_table(),
            "docs/themes.md's parts table is out of step with parts::REGISTRY; paste docs_table()"
        );
    }

    #[test]
    fn the_sample_theme_in_the_docs_passes_its_checks() {
        let folder =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/themes/parts-sample");
        let files = crate::themes::check::Files::read(&folder).expect("the sample's files");
        let report = crate::themes::check::check(&files, crate::themes::check::Strictness::Install);
        assert!(report.passed(), "{}", report.text());
        let resolved = report.package.unwrap().resolve(Mode::Dark).unwrap();
        assert!(resolved.parts.rules.len() >= 15);
    }

    #[test]
    fn the_json_lists_every_part_with_its_states_and_properties() {
        let json = registry_json();
        let parts = json["parts"].as_array().unwrap();
        assert_eq!(parts.len(), REGISTRY.len());
        let row = parts
            .iter()
            .find(|part| part["name"] == "sidebar.channel-row")
            .unwrap();
        assert_eq!(
            row["states"],
            json!(["unread", "muted", "hover", "selected"])
        );
        assert!(
            row["properties"]
                .as_array()
                .unwrap()
                .contains(&json!("padding"))
        );
        assert_eq!(json["properties"].as_object().unwrap().len(), PROPS.len());
    }

    #[test]
    fn states_layer_over_the_base_in_order() {
        let package = theme(
            r##"{ "sidebar.channel-row": {
                "ground": "#101010", "ink": "#e0e0e0", "radius": 4,
                "hover": { "ground": "#202020" },
                "unread": { "ink": "#ffffff", "weight": 700 },
                "selected": { "ground": "#303030", "ink": "#fafafa" } } }"##,
        )
        .unwrap();
        let resolved = package.resolve(Mode::Dark).unwrap();
        let rules = resolved.parts.get("sidebar.channel-row").unwrap();
        let (rest, from) = rules.resolve(0);
        assert_eq!(rest.ground, Some(Paint::Colour(0xff10_1010)));
        assert_eq!(
            from,
            From {
                base: true,
                states: 0
            }
        );
        let (hover, _) = rules.resolve(HOVER);
        assert_eq!(hover.ground, Some(Paint::Colour(0xff20_2020)));
        assert_eq!(hover.radius, Some([4.0; 4]), "the base's other values stay");
        // Selected is laid last: its ground beats hover's.
        let (both, from) = rules.resolve(HOVER | SELECTED | UNREAD);
        assert_eq!(both.ground, Some(Paint::Colour(0xff30_3030)));
        assert_eq!(both.ink, Some(0xfffa_fafa), "selected's ink beats unread's");
        assert_eq!(both.weight, Some(700), "unread's weight shows through");
        assert_eq!(
            from,
            From {
                base: true,
                states: HOVER | SELECTED | UNREAD
            }
        );
        // A state the part has but the theme doesn't style changes nothing.
        assert_eq!(rules.resolve(MUTED).0, rest);
    }

    #[test]
    fn a_variant_s_values_replace_the_shared_ones_one_by_one() {
        let package = Package::parse(
            r##"{ "theme_api": 1, "id": "com.example.parts", "name": "Parts", "version": "1.0.0",
                 "parts": { "menu.row": { "ink": "#dddddd", "radius": 2, "hover": { "ground": "#333333" } } },
                 "variants": { "dark": { "base": "yap",
                     "parts": { "menu.row": { "radius": 6, "hover": { "ink": "#ffffff" } } } } } }"##,
        )
        .unwrap();
        let resolved = package.resolve(Mode::Dark).unwrap();
        let rules = resolved.parts.get("menu.row").unwrap();
        let (style, _) = rules.resolve(HOVER);
        assert_eq!(style.ink, Some(0xffff_ffff));
        assert_eq!(style.radius, Some([6.0; 4]));
        assert_eq!(style.ground, Some(Paint::Colour(0xff33_3333)));
    }

    #[test]
    fn every_property_reads_in_its_range() {
        let package = theme(
            r##"{ "sidebar.category": {
                "ground": "linear(90deg, #101010, #202020)", "ink": "#cccccc", "border": 1,
                "border_colour": "#ffffff40", "radius": 999, "shadow": { "blur": 4, "y": 1 },
                "opacity": 0.9, "padding": [2, 8], "weight": 600, "size_step": -1 } }"##,
        )
        .unwrap();
        let resolved = package.resolve(Mode::Dark).unwrap();
        let (style, _) = resolved.parts.get("sidebar.category").unwrap().resolve(0);
        assert!(matches!(style.ground, Some(Paint::Linear { .. })));
        assert_eq!(style.padding, Some([2.0, 8.0, 2.0, 8.0]));
        assert_eq!(style.weight, Some(600));
        assert_eq!(style.size_step, Some(-1.0));
        assert!(matches!(style.shadow, Some(Some(_))));
        let none = theme(r#"{ "menu": { "shadow": "none" } }"#).unwrap();
        let resolved = none.resolve(Mode::Dark).unwrap();
        assert_eq!(resolved.parts.get("menu").unwrap().base.shadow, Some(None));
        let four = theme(r#"{ "timeline.message": { "padding": [1, 2, 3, 4] } }"#).unwrap();
        let resolved = four.resolve(Mode::Dark).unwrap();
        assert_eq!(
            resolved.parts.get("timeline.message").unwrap().base.padding,
            Some([1.0, 2.0, 3.0, 4.0])
        );
    }

    #[test]
    fn the_wider_properties_read_and_layer() {
        let package = theme(
            r##"{ "sidebar.channel-row": {
                "border": [0, 0, 2, 0], "radius": [8, 8, 0, 0], "case": "upper", "italic": true,
                "picture": { "file": "art/row.png", "fit": "tile" },
                "muted": { "hidden": true, "case": "none" }, "hover": { "opacity": 0 } },
              "menu.row": { "icon_ink": "#ff00ff", "picture": "art/menu.webp" } }"##,
        )
        .unwrap();
        let resolved = package.resolve(Mode::Dark).unwrap();
        let rules = resolved.parts.get("sidebar.channel-row").unwrap();
        let (rest, _) = rules.resolve(0);
        assert_eq!(rest.border, Some([0.0, 0.0, 2.0, 0.0]));
        assert_eq!(rest.radius, Some([8.0, 8.0, 0.0, 0.0]));
        assert_eq!(rest.case, Some(Case::Upper));
        assert_eq!(rest.italic, Some(true));
        assert_eq!(
            rest.picture,
            Some(Picture {
                file: "art/row.png".into(),
                fit: Fit::Tile
            })
        );
        let (muted, _) = rules.resolve(MUTED);
        assert_eq!(muted.hidden, Some(true));
        assert_eq!(muted.case, Some(Case::None));
        assert_eq!(rules.resolve(HOVER).0.opacity, Some(0.0));
        let (menu, _) = resolved.parts.get("menu.row").unwrap().resolve(0);
        assert_eq!(menu.icon_ink, Some(0xffff_00ff));
        assert_eq!(menu.picture.unwrap().fit, Fit::Cover);
        // The theme installs and checks its parts' pictures.
        let files = package.files();
        assert!(files.contains(&"art/row.png".into()), "{files:?}");
        assert!(files.contains(&"art/menu.webp".into()), "{files:?}");
    }

    #[test]
    fn a_part_hides_only_where_the_pointer_cant_bring_it_back() {
        let package = theme(r#"{ "menu.row": { "hover": { "hidden": true } } }"#).unwrap();
        let resolved = package.resolve(Mode::Dark).unwrap();
        assert!(
            resolved.parts.unknown.join("\n").contains(
                "\"parts.menu.row.hover.hidden\": a part can be hidden in its own values or unread, muted or selected, not hover"
            ),
            "{:?}",
            resolved.parts.unknown
        );
        for (parts, place) in [
            (r#"{ "menu": { "hidden": "yes" } }"#, "parts.menu.hidden"),
            (
                r#"{ "menu.row": { "case": "title" } }"#,
                "parts.menu.row.case",
            ),
            (r#"{ "menu": { "border": [1, 2] } }"#, "parts.menu.border"),
            (
                r#"{ "menu": { "radius": [1, 2, 3, 1000] } }"#,
                "parts.menu.radius[3]",
            ),
            (
                r#"{ "menu": { "picture": "art/x.svg" } }"#,
                "parts.menu.picture",
            ),
            (
                r#"{ "menu": { "picture": { "file": "a.png", "fit": "fill" } } }"#,
                "parts.menu.picture.fit",
            ),
            (
                r#"{ "menu": { "picture": "../outside.png" } }"#,
                "parts.menu.picture",
            ),
        ] {
            let error = theme(parts).unwrap_err();
            assert!(error.0.contains(place), "{parts}: {error}");
        }
    }

    #[test]
    fn values_out_of_range_are_refused_with_their_place() {
        for (parts, place) in [
            (r#"{ "menu": { "border": 9 } }"#, "parts.menu.border"),
            (
                r#"{ "menu.row": { "hover": { "opacity": 1.5 } } }"#,
                "parts.menu.row.hover.opacity",
            ),
            (
                r#"{ "timeline.message": { "padding": 60 } }"#,
                "parts.timeline.message.padding",
            ),
            (
                r#"{ "timeline.message": { "padding": [1, 2, 3] } }"#,
                "parts.timeline.message.padding",
            ),
            (
                r#"{ "sidebar.category": { "weight": 650 } }"#,
                "parts.sidebar.category.weight",
            ),
            (
                r#"{ "sidebar.category": { "size_step": 20 } }"#,
                "parts.sidebar.category.size_step",
            ),
            (r#"{ "menu": { "ground": "red" } }"#, "parts.menu.ground"),
            (r##"{ "menu": "#ffffff" }"##, "parts.menu"),
        ] {
            let error = theme(parts).unwrap_err();
            assert!(error.0.contains(place), "{parts}: {error}");
        }
    }

    #[test]
    fn unknown_parts_states_and_properties_are_named_and_kept_for_the_check() {
        let package = theme(
            r##"{ "sidebar.channel_row": { "ink": "#ffffff" },
                  "whole-window": {},
                  "menu": { "hover": { "ground": "#000000" }, "font": "x.ttf" },
                  "rail": { "ink": "#ffffff" },
                  "menu.row": { "hover": { "selected": {} } } }"##,
        )
        .unwrap();
        let resolved = package.resolve(Mode::Dark).unwrap();
        let unknown = resolved.parts.unknown.join("\n");
        for expected in [
            "\"parts.sidebar.channel_row\" isn't a part Yapfast has (did you mean \"sidebar.channel-row\"?)",
            "\"parts.whole-window\" isn't a part",
            "\"parts.menu.hover\": menu has no hover state",
            "\"parts.menu.font\" isn't a property or state",
            "\"parts.rail.ink\": rail doesn't take ink; it takes ground",
            "\"parts.menu.row.hover.selected\": states don't nest",
        ] {
            assert!(unknown.contains(expected), "{expected} in {unknown}");
        }
        // The window ignores them: the theme still opens, with notes.
        assert!(
            package
                .notes
                .iter()
                .any(|note| note.contains("whole-window"))
        );
        assert!(
            resolved
                .parts
                .get("menu")
                .is_some_and(|rules| rules.base.is_empty())
        );
    }

    #[test]
    fn a_part_s_ink_has_to_read_on_its_ground_in_every_state() {
        let package = theme(
            r##"{ "sidebar.channel-row": { "hover": { "ground": "#8a8e94" } },
                  "menu.row": { "ink": "#f0f0f0", "selected": {} },
                  "tooltip": { "ink": "#ffffff", "opacity": 0.3 } }"##,
        );
        // "selected" isn't one of menu.row's states: noted, not refused here.
        let package = package.unwrap();
        let resolved = package.resolve(Mode::Dark).unwrap();
        let refused = unreadable(&resolved).join("\n");
        assert!(
            refused.contains("parts.sidebar.channel-row (hover): its ink contrasts"),
            "{refused}"
        );
        assert!(refused.contains("parts.tooltip:"), "{refused}");
        assert!(!refused.contains("parts.menu.row"), "{refused}");
        let fine = theme(
            r##"{ "sidebar.channel-row": { "ink": "#f2f3f5", "hover": { "ground": "#2b2d31" },
                  "muted": { "opacity": 0.55 } } }"##,
        )
        .unwrap();
        // A muted row may fade as far as Yapfast's own muted rows do.
        assert!(unreadable(&fine.resolve(Mode::Dark).unwrap()).is_empty());
        let faint =
            theme(r##"{ "sidebar.channel-row": { "muted": { "opacity": 0.3 } } }"##).unwrap();
        assert!(
            unreadable(&faint.resolve(Mode::Dark).unwrap())
                .join("\n")
                .contains("parts.sidebar.channel-row (muted)")
        );
    }
}
