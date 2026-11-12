//! `theme.json`: what a theme says, read into named values.
//!
//! The file is strict JSON. Every layer is optional, so a theme can be one
//! colour or a full redesign. Layers at the top level apply to every variant;
//! a variant (`dark`, `light`) names its built-in base and may override any
//! layer for itself. [`Package::resolve`] turns one variant into [`Resolved`],
//! the values the interface applies, each already checked against its range,
//! so applying a theme never has to judge a value.
//!
//! Names Yapfast doesn't know are ignored and listed in [`Package::notes`], so
//! a theme written for a later Yapfast opens in this one without the parts it
//! can't draw.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::{MANIFEST, PART_CONTRACTS, PARTS, THEME_API};

pub mod art;

/// The colours a theme sets, as `ui/theme.slint` names them; `src/ui/theme.rs`
/// checks this list against its own.
pub const COLOURS: [&str; 37] = [
    "rail",
    "sidebar",
    "panel",
    "chat",
    "stage",
    "raised",
    "field",
    "sunken",
    "hover",
    "active",
    "row-hover",
    "line",
    "edge",
    "outline",
    "outline-chat",
    "outline-field",
    "text",
    "secondary",
    "muted",
    "faint",
    "accent",
    "accent-hover",
    "on-accent",
    "accent-soft",
    "danger",
    "danger-hover",
    "on-danger",
    "live",
    "loud",
    "link",
    "button-default",
    "button-default-hover",
    "overlay",
    "shimmer",
    "reply-spine",
    "tooltip",
    "avatar",
];

/// Colours only a theme sets (plugins can't), each following another until a
/// theme does: what it follows is [`follows`].
pub const EXTRA_COLOURS: [&str; 23] = [
    "accent-text",
    "on-tooltip",
    "selection",
    "focus-ring",
    "scrollbar",
    "scrollbar-track",
    "scrim",
    "menu",
    "menu-line",
    "composer",
    "placeholder",
    "mentioned",
    "mentioned-hover",
    "composer-focus",
    "row-selected",
    "pill",
    "toggle-off",
    "knob-off",
    "code",
    "embed",
    "members",
    "app-badge",
    "account",
];

/// The colour an extra colour shows while a theme doesn't set it, for
/// checking text on it.
pub fn follows(token: &str) -> Option<&'static str> {
    Some(match token {
        "accent-text" | "focus-ring" => "accent",
        "on-tooltip" => "text",
        "menu" => "raised",
        "composer" => "field",
        "menu-line" => "line",
        "placeholder" => "muted",
        "row-selected" => "active",
        "pill" => "text",
        "toggle-off" => "field",
        "knob-off" => "secondary",
        "code" => "panel",
        "embed" => "sidebar",
        "members" => "chat",
        "account" => "panel",
        _ => return None,
    })
}

/// Warnings, destructive actions and the Block, Ban and Delete rows are drawn
/// in these, so no theme sets them.
pub const LOCKED_COLOURS: [&str; 3] = ["danger", "danger-hover", "on-danger"];

/// The colours that paint a whole surface and so take a gradient as well as a
/// colour. Wherever a plain colour is needed (a mix, a contrast check) the
/// gradient's first stop stands in.
pub const BRUSHES: [&str; 6] = ["rail", "sidebar", "chat", "panel", "raised", "accent"];

/// Each text colour and the surfaces it's drawn on, which it must stay
/// readable against.
pub const TEXT_ON: [(&str, &[&str]); 17] = [
    ("text", &SURFACES),
    ("secondary", &SURFACES),
    ("muted", &SURFACES),
    ("link", &SURFACES),
    ("accent-text", &SURFACES),
    ("on-accent", &["accent"]),
    ("text", &["raised"]),
    ("text", &["menu"]),
    ("text", &["composer"]),
    ("on-tooltip", &["tooltip"]),
    ("text", &["row-selected"]),
    ("text", &["code"]),
    ("text", &["embed"]),
    ("text", &["members"]),
    ("text", &["account"]),
    ("secondary", &["account"]),
    ("muted", &["account"]),
];

/// The surfaces most text sits on.
const SURFACES: [&str; 4] = ["sidebar", "panel", "chat", "field"];

/// The contrast WCAG asks of body text.
pub const READABLE: f64 = 4.5;

/// The surfaces a theme shapes: corner radius, border and shadow. Names are
/// public API under `theme_api` 1; parts use the same names later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Surface {
    /// Labelled buttons: primary, secondary, danger.
    Button,
    /// Text fields and search boxes.
    Field,
    /// The message box.
    Composer,
    /// Profile cards and other cards that float over the window.
    Card,
    /// Menus that open from a button or a right click.
    Menu,
    /// Popups with a dimmed backdrop.
    Dialog,
    /// The window's tooltip.
    Tooltip,
    /// Reactions, tags and other small pills.
    Chip,
    /// A message's hover and highlight.
    Message,
    /// Servers in the rail.
    RailIcon,
    /// Avatars everywhere: `radius` is a percentage of the avatar's size, 50
    /// for a circle and 0 for a square.
    Avatar,
    /// Cards in settings, such as the themes in Appearance.
    Tile,
    /// Code blocks: their outline.
    Code,
    /// Embeds: their outline.
    Embed,
    /// Switches: `radius` in pixels, 999 a pill as stock.
    Toggle,
}

impl Surface {
    pub const ALL: [Self; 15] = [
        Self::Button,
        Self::Field,
        Self::Composer,
        Self::Card,
        Self::Menu,
        Self::Dialog,
        Self::Tooltip,
        Self::Chip,
        Self::Message,
        Self::RailIcon,
        Self::Avatar,
        Self::Tile,
        Self::Code,
        Self::Embed,
        Self::Toggle,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Field => "field",
            Self::Composer => "composer",
            Self::Card => "card",
            Self::Menu => "menu",
            Self::Dialog => "dialog",
            Self::Tooltip => "tooltip",
            Self::Chip => "chip",
            Self::Message => "message",
            Self::RailIcon => "rail-icon",
            Self::Avatar => "avatar",
            Self::Tile => "tile",
            Self::Code => "code",
            Self::Embed => "embed",
            Self::Toggle => "toggle",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|surface| surface.name() == name)
    }

    /// The largest radius it takes: a percentage for avatars, pixels for the
    /// rest, where 999 is a pill.
    fn max_radius(self) -> f32 {
        match self {
            Self::Avatar => 50.0,
            _ => 999.0,
        }
    }
}

/// The kinds of text a theme sets a typeface and size for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// Messages and nearly everything else.
    Body,
    /// Page titles, section headings, channel and server names.
    Headings,
    /// People's names on messages, in the member list and on profiles.
    Names,
    /// Code blocks and inline code.
    Code,
    /// Small capital labels over groups, such as THEME in Appearance.
    Labels,
}

impl Role {
    pub const ALL: [Self; 5] = [
        Self::Body,
        Self::Headings,
        Self::Names,
        Self::Code,
        Self::Labels,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Headings => "headings",
            Self::Names => "names",
            Self::Code => "code",
            Self::Labels => "labels",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.name() == name)
    }

    /// The sizes it takes, in logical pixels.
    fn sizes(self) -> (f32, f32) {
        match self {
            Self::Body => (12.0, 20.0),
            Self::Headings => (12.0, 32.0),
            Self::Names => (12.0, 24.0),
            Self::Code => (11.0, 20.0),
            Self::Labels => (10.0, 16.0),
        }
    }
}

/// A named curve from the fixed set a theme picks from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Curve {
    Linear,
    Ease,
    EaseIn,
    #[default]
    EaseOut,
    EaseInOut,
    /// Overshoots a little and settles: Slint's ease-out-back.
    Spring,
    Bounce,
    Elastic,
}

impl Curve {
    pub const ALL: [Self; 8] = [
        Self::Linear,
        Self::Ease,
        Self::EaseIn,
        Self::EaseOut,
        Self::EaseInOut,
        Self::Spring,
        Self::Bounce,
        Self::Elastic,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Ease => "ease",
            Self::EaseIn => "ease-in",
            Self::EaseOut => "ease-out",
            Self::EaseInOut => "ease-in-out",
            Self::Spring => "spring",
            Self::Bounce => "bounce",
            Self::Elastic => "elastic",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|curve| curve.name() == name)
    }

    /// Its index in [`Self::ALL`], which `ui/theme.slint`'s `Motion` maps
    /// back to Slint's easing.
    pub fn index(self) -> i32 {
        Self::ALL
            .iter()
            .position(|curve| *curve == self)
            .unwrap_or(0) as i32
    }
}

/// What hovering a button, a server or a row does, from a fixed set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hover {
    /// Only the colour changes, as stock.
    #[default]
    Fade,
    None,
    /// Rises two pixels.
    Lift,
    /// A soft halo in the accent colour.
    Glow,
    /// Grows by 4%.
    Scale,
}

impl Hover {
    pub const ALL: [Self; 5] = [Self::Fade, Self::None, Self::Lift, Self::Glow, Self::Scale];

    pub fn name(self) -> &'static str {
        match self {
            Self::Fade => "fade",
            Self::None => "none",
            Self::Lift => "lift",
            Self::Glow => "glow",
            Self::Scale => "scale",
        }
    }

    pub fn index(self) -> i32 {
        Self::ALL
            .iter()
            .position(|hover| *hover == self)
            .unwrap_or(0) as i32
    }
}

/// What pressing does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Press {
    #[default]
    None,
    /// Shrinks to 96% while held.
    Scale,
    /// Moves a pixel down while held.
    Sink,
}

impl Press {
    pub const ALL: [Self; 3] = [Self::None, Self::Scale, Self::Sink];

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Scale => "scale",
            Self::Sink => "sink",
        }
    }

    pub fn index(self) -> i32 {
        Self::ALL
            .iter()
            .position(|press| *press == self)
            .unwrap_or(0) as i32
    }
}

/// How a popup, menu or card opens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transition {
    /// Grows from 80% as it fades in, as stock.
    #[default]
    Grow,
    /// Shows at once.
    None,
    /// Only fades in.
    Fade,
    /// Grows from 95% as it fades in: a gentler grow.
    Zoom,
    /// Grows past its size and settles back, on a spring.
    Pop,
}

impl Transition {
    pub const ALL: [Self; 5] = [Self::Grow, Self::None, Self::Fade, Self::Zoom, Self::Pop];

    pub fn name(self) -> &'static str {
        match self {
            Self::Grow => "grow",
            Self::None => "none",
            Self::Fade => "fade",
            Self::Zoom => "zoom",
            Self::Pop => "pop",
        }
    }

    pub fn index(self) -> i32 {
        Self::ALL
            .iter()
            .position(|transition| *transition == self)
            .unwrap_or(0) as i32
    }
}

/// Where a declared animation plays. Each is one place Yapfast draws, with
/// the properties it lets move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    /// A layer over the whole window that takes no input: `tint` and
    /// `opacity`, for a flicker, a scanline glow or a vignette pulse.
    Window,
    /// The accent colour itself: `colour`.
    Accent,
    /// The home button at the top of the rail.
    Home,
    /// The pill beside a server or channel with unread messages.
    Unread,
    /// The red count of mentions.
    Mention,
    /// The dots of someone typing.
    Typing,
    /// The open conversation's name in its header.
    ChannelName,
    /// The server's name above its channels.
    ServerName,
    /// The name on a profile card.
    ProfileName,
    /// Your own avatar in the account panel.
    OwnAvatar,
    /// Every server in the rail, each `phase` milliseconds behind the one
    /// above it.
    RailIcon,
}

impl Slot {
    pub const ALL: [Self; 11] = [
        Self::Window,
        Self::Accent,
        Self::Home,
        Self::Unread,
        Self::Mention,
        Self::Typing,
        Self::ChannelName,
        Self::ServerName,
        Self::ProfileName,
        Self::OwnAvatar,
        Self::RailIcon,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Accent => "accent",
            Self::Home => "home",
            Self::Unread => "unread",
            Self::Mention => "mention",
            Self::Typing => "typing",
            Self::ChannelName => "channel.name",
            Self::ServerName => "server.name",
            Self::ProfileName => "profile.name",
            Self::OwnAvatar => "own.avatar",
            Self::RailIcon => "rail.icon",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.name() == name)
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|slot| *slot == self).unwrap_or(0)
    }

    /// The properties an animation may move here.
    pub fn properties(self) -> &'static [Property] {
        match self {
            Self::Window => &[Property::Opacity, Property::Tint],
            Self::Accent => &[Property::Colour],
            _ => &[
                Property::Opacity,
                Property::X,
                Property::Y,
                Property::Scale,
                Property::Rotation,
            ],
        }
    }
}

/// What a declared animation moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Property {
    /// 0 to 1.
    Opacity,
    /// An offset in logical pixels, -24 to 24.
    X,
    Y,
    /// 0.5 to 1.5.
    Scale,
    /// Degrees, -360 to 360: a record spins with [0, 360].
    Rotation,
    /// A colour laid over the slot, alpha included.
    Tint,
    /// The slot's colour.
    Colour,
}

impl Property {
    pub const ALL: [Self; 7] = [
        Self::Opacity,
        Self::X,
        Self::Y,
        Self::Scale,
        Self::Rotation,
        Self::Tint,
        Self::Colour,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Opacity => "opacity",
            Self::X => "x",
            Self::Y => "y",
            Self::Scale => "scale",
            Self::Rotation => "rotation",
            Self::Tint => "tint",
            Self::Colour => "colour",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|property| property.name() == name)
    }

    pub fn is_colour(self) -> bool {
        matches!(self, Self::Tint | Self::Colour)
    }

    /// The numbers it takes.
    fn range(self) -> (f32, f32) {
        match self {
            Self::Opacity => (0.0, 1.0),
            Self::X | Self::Y => (-24.0, 24.0),
            Self::Scale => (0.5, 1.5),
            Self::Rotation => (-360.0, 360.0),
            Self::Tint | Self::Colour => (0.0, 0.0),
        }
    }
}

/// Text that a built-in effect can play on when it changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextSlot {
    ChannelName,
    ServerName,
    ProfileName,
    /// The title of a settings page.
    SettingsTitle,
    /// The welcome at the start of a conversation.
    Welcome,
}

impl TextSlot {
    pub const ALL: [Self; 5] = [
        Self::ChannelName,
        Self::ServerName,
        Self::ProfileName,
        Self::SettingsTitle,
        Self::Welcome,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::ChannelName => "channel.name",
            Self::ServerName => "server.name",
            Self::ProfileName => "profile.name",
            Self::SettingsTitle => "settings.title",
            Self::Welcome => "welcome",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.name() == name)
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|slot| *slot == self).unwrap_or(0)
    }
}

/// A built-in text effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Letters scramble and settle from left to right.
    Decrypt,
    /// Letters appear one at a time, behind a cursor.
    Typewriter,
}

impl Effect {
    pub fn name(self) -> &'static str {
        match self {
            Self::Decrypt => "decrypt",
            Self::Typewriter => "typewriter",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        [Self::Decrypt, Self::Typewriter]
            .into_iter()
            .find(|effect| effect.name() == name)
    }
}

/// How a declared animation repeats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    #[default]
    Loop,
    /// Forward, then back.
    Alternate,
    /// Once, when the theme is applied or the slot appears.
    Once,
}

/// A colour, or a gradient where the token paints a surface.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    /// ARGB.
    Colour(u32),
    /// An angle in degrees, as CSS measures it (0 points up, 90 right), and
    /// stops of ARGB at 0 to 1.
    Linear { angle: f32, stops: Vec<(u32, f32)> },
}

impl Paint {
    /// The colour that stands in for it where a single one is needed.
    pub fn solid(&self) -> u32 {
        match self {
            Self::Colour(colour) => *colour,
            Self::Linear { stops, .. } => stops.first().map_or(0, |stop| stop.0),
        }
    }

    /// Every colour it shows, for checking text on it.
    pub fn colours(&self) -> Vec<u32> {
        match self {
            Self::Colour(colour) => vec![*colour],
            Self::Linear { stops, .. } => stops.iter().map(|stop| stop.0).collect(),
        }
    }
}

/// A surface's shape, each value in its range.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    pub radius: Option<f32>,
    pub border: Option<f32>,
    /// ARGB.
    pub border_colour: Option<u32>,
    pub shadow: Option<Shadow>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub blur: f32,
    pub x: f32,
    pub y: f32,
    /// ARGB.
    pub colour: u32,
}

/// One role's type.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Type {
    /// A font file in the theme's folder, relative to it.
    pub font: Option<PathBuf>,
    pub size: Option<f32>,
    pub weight: Option<i32>,
    pub letter_spacing: Option<f32>,
    /// Headings only: drawn in capitals.
    pub uppercase: Option<bool>,
    /// Headings only: the open conversation's and the server's names, and
    /// page and dialog titles, apart from other headings.
    pub header_size: Option<f32>,
    pub title_size: Option<f32>,
}

/// Gaps, paddings and sizes, each in logical pixels and in its range.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spacing {
    pub message_gap: Option<f32>,
    pub message_group_gap: Option<f32>,
    pub channel_row_height: Option<f32>,
    pub member_row_height: Option<f32>,
    pub avatar_size: Option<f32>,
    pub chat_padding: Option<f32>,
    pub sidebar_width: Option<f32>,
    pub members_width: Option<f32>,
}

impl Spacing {
    /// Each value's name in `theme.json`, its range, and where it is.
    const FIELDS: [(&'static str, f32, f32); 8] = [
        ("message_gap", 0.0, 16.0),
        ("message_group_gap", 0.0, 40.0),
        ("channel_row_height", 24.0, 48.0),
        ("member_row_height", 32.0, 56.0),
        ("avatar_size", 24.0, 56.0),
        ("chat_padding", 4.0, 48.0),
        ("sidebar_width", 200.0, 420.0),
        ("members_width", 200.0, 400.0),
    ];

    fn slot(&mut self, name: &str) -> Option<&mut Option<f32>> {
        Some(match name {
            "message_gap" => &mut self.message_gap,
            "message_group_gap" => &mut self.message_group_gap,
            "channel_row_height" => &mut self.channel_row_height,
            "member_row_height" => &mut self.member_row_height,
            "avatar_size" => &mut self.avatar_size,
            "chat_padding" => &mut self.chat_padding,
            "sidebar_width" => &mut self.sidebar_width,
            "members_width" => &mut self.members_width,
            _ => return None,
        })
    }

    /// A density's values, which named values in the same theme override.
    fn density(name: &str) -> Option<Self> {
        match name {
            "compact" => Some(Self {
                message_gap: Some(0.0),
                message_group_gap: Some(6.0),
                channel_row_height: Some(26.0),
                member_row_height: Some(34.0),
                avatar_size: Some(32.0),
                chat_padding: Some(10.0),
                ..Self::default()
            }),
            "default" => Some(Self::default()),
            "roomy" => Some(Self {
                message_gap: Some(4.0),
                message_group_gap: Some(24.0),
                channel_row_height: Some(36.0),
                member_row_height: Some(48.0),
                avatar_size: Some(44.0),
                chat_padding: Some(24.0),
                ..Self::default()
            }),
            _ => None,
        }
    }
}

/// Motion: durations, the curve things move on, and what hovering, pressing,
/// opening and closing do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Motion {
    /// Milliseconds a hover's colour change takes; stock is 120.
    pub quick: Option<f32>,
    /// Milliseconds a popup takes to open or close; stock is 100.
    pub popup: Option<f32>,
    pub curve: Option<Curve>,
    pub hover: Option<Hover>,
    pub press: Option<Press>,
    pub open: Option<Transition>,
    /// How it closes: the opening played backwards (`reverse`, stock), or at
    /// once (`none`).
    pub close: Option<Close>,
    /// Milliseconds a hover takes to come and to go, apart; each follows
    /// `quick` when left out.
    pub hover_in: Option<f32>,
    pub hover_out: Option<f32>,
}

/// How a popup closes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Close {
    #[default]
    Reverse,
    None,
}

impl Close {
    pub const ALL: [Self; 2] = [Self::Reverse, Self::None];

    pub fn name(self) -> &'static str {
        match self {
            Self::Reverse => "reverse",
            Self::None => "none",
        }
    }
}

/// A declared animation: a property of a slot, moving through values.
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub slot: Slot,
    pub property: Property,
    /// Numbers for opacity, offsets, scale and rotation; ARGB for colours.
    pub values: Vec<f32>,
    pub colours: Vec<u32>,
    /// Milliseconds for one pass through the values.
    pub period: f32,
    /// Jumps between values instead of moving: the number of steps a pass
    /// takes, as CSS's `steps()`.
    pub steps: Option<u32>,
    pub easing: Curve,
    pub repeat: Repeat,
    /// Milliseconds before it starts.
    pub delay: f32,
    /// Costly enough to stay still in Balanced, as a whole-window flicker is.
    pub heavy: bool,
    /// A `cubic-bezier(x1, y1, x2, y2)` easing, which then stands in for
    /// `easing`.
    pub bezier: Option<[f32; 4]>,
    /// `rail.icon`: milliseconds each server runs behind the one above it.
    pub phase: f32,
}

/// One text slot's effect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextEffect {
    pub slot: TextSlot,
    pub effect: Effect,
    /// Milliseconds the effect takes for the whole text.
    pub duration: f32,
}

/// One variant of a theme, every layer applied over its base: what the
/// interface shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resolved {
    pub dark: bool,
    /// The built-in theme whose colours stand in for those this one doesn't
    /// set.
    pub base: String,
    /// Colours and gradients over the base, by their name in `ui/theme.slint`.
    pub colours: Vec<(&'static str, Paint)>,
    pub shapes: BTreeMap<Surface, Shape>,
    /// Every other corner by its stock size: small, medium, large.
    pub radii: [Option<f32>; 3],
    /// Every other shadow: unset, none at all, or one shadow for all.
    pub shadows: Option<Option<Shadow>>,
    pub types: BTreeMap<Role, Type>,
    pub spacing: Spacing,
    pub motion: Motion,
    pub animations: Vec<Animation>,
    pub effects: Vec<TextEffect>,
    /// Pictures, overlays, glass, text light, particles and event effects.
    pub art: art::Art,
    /// Sounds by the slot they play in: [`super::media`].
    pub sounds: BTreeMap<super::media::SoundSlot, super::media::SoundChoice>,
    /// Replaced icons by their name in `ui/icons/`.
    pub icons: BTreeMap<&'static str, super::media::IconChoice>,
    /// Where the panes sit and how messages are laid out.
    pub layout: super::layout::Layout,
    /// Mouse cursors by kind: [`super::cursors`].
    pub cursors: BTreeMap<super::cursors::Kind, super::cursors::Cursor>,
}

impl Resolved {
    /// The paint a colour token has here, if the theme sets it.
    pub fn paint(&self, token: &str) -> Option<&Paint> {
        self.colours
            .iter()
            .find(|(name, _)| *name == token)
            .map(|(_, paint)| paint)
    }

    /// Every font file it names, once each.
    pub fn fonts(&self) -> Vec<&Path> {
        let mut fonts: Vec<&Path> = Vec::new();
        for kind in self.types.values() {
            if let Some(font) = &kind.font
                && !fonts.contains(&font.as_path())
            {
                fonts.push(font);
            }
        }
        for font in self.layout.names.iter().flat_map(|names| &names.fonts) {
            if !fonts.contains(&font.as_path()) {
                fonts.push(font);
            }
        }
        fonts
    }
}

/// Light or dark: which variant of a theme shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// Why a `theme.json` can't be used, as one sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

macro_rules! bail {
    ($($arg:tt)*) => { return Err(Error(format!($($arg)*))) };
}

/// A theme as its `theme.json` describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    /// An SPDX expression, such as `OFL-1.1 AND CC-BY-4.0`.
    pub license: Option<String>,
    /// The repository it's published from.
    pub source: Option<String>,
    /// The repository of a theme this one remixes, credited on its page.
    pub based_on: Option<String>,
    pub min_yapfast: Option<String>,
    /// Screenshots for its listing and the gallery, relative to the folder.
    pub preview: Vec<PathBuf>,
    /// A small picture for lists, relative to the folder.
    pub icon: Option<PathBuf>,
    /// The variants it ships: one or both.
    pub dark: Option<Value>,
    pub light: Option<Value>,
    /// The layers every variant shares.
    shared: Value,
    /// What Yapfast ignored, each as a sentence for the theme's page.
    pub notes: Vec<String>,
}

/// The top-level keys of `theme.json`.
const TOP_KEYS: [&str; 15] = [
    "theme_api",
    "id",
    "name",
    "version",
    "description",
    "authors",
    "license",
    "source",
    "based_on",
    "min_yapfast",
    "preview",
    "icon",
    "variants",
    "$schema",
    "parts",
];

/// The layers, at the top level or in a variant.
const LAYER_KEYS: [&str; 12] = [
    "art",
    "layout",
    "colours",
    "shape",
    "type",
    "spacing",
    "motion",
    "animations",
    "effects",
    "sounds",
    "icons",
    "cursors",
];

/// The most previews a theme lists.
pub const MAX_PREVIEWS: usize = 6;
/// The most declared animations a variant runs.
pub const MAX_ANIMATIONS: usize = 16;

impl Package {
    /// Reads `theme.json`'s text. Names Yapfast doesn't know are noted rather
    /// than refused; a value of the wrong kind or out of its range is refused
    /// with the place it's at.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let value: Value = serde_json::from_str(text)
            .map_err(|error| Error(format!("{MANIFEST} isn't valid JSON: {error}")))?;
        let Value::Object(top) = &value else {
            bail!("{MANIFEST} has to be an object");
        };
        let mut notes = Vec::new();
        let api = top
            .get("theme_api")
            .and_then(Value::as_u64)
            .ok_or_else(|| Error(format!("{MANIFEST} needs \"theme_api\": {THEME_API}")))?;
        if api != u64::from(THEME_API) {
            bail!("this theme is for theme API {api}, and this Yapfast reads {THEME_API}");
        }
        let id = string(top, "id")?.ok_or_else(|| Error("it needs an \"id\"".into()))?;
        check_id(&id)?;
        let name = string(top, "name")?.ok_or_else(|| Error("it needs a \"name\"".into()))?;
        if name.trim().is_empty() || name.chars().count() > 48 {
            bail!("\"name\" has to be 1 to 48 characters");
        }
        let version =
            string(top, "version")?.ok_or_else(|| Error("it needs a \"version\"".into()))?;
        check_version(&version)?;
        let description = string(top, "description")?.unwrap_or_default();
        if description.chars().count() > 300 {
            bail!("\"description\" is at most 300 characters");
        }
        let authors = match top.get("authors") {
            None => Vec::new(),
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| Error("\"authors\" is a list of names".into()))
                })
                .collect::<Result<_, _>>()?,
            Some(_) => bail!("\"authors\" is a list of names"),
        };
        let preview = match top.get("preview") {
            None => Vec::new(),
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .ok_or_else(|| Error("\"preview\" is a list of file names".into()))
                        .and_then(|text| relative(text, "preview"))
                })
                .collect::<Result<_, _>>()?,
            Some(_) => bail!("\"preview\" is a list of file names"),
        };
        if preview.len() > MAX_PREVIEWS {
            bail!("\"preview\" lists at most {MAX_PREVIEWS} pictures");
        }
        let icon = string(top, "icon")?
            .map(|text| relative(&text, "icon"))
            .transpose()?;
        for key in top.keys() {
            if !TOP_KEYS.contains(&key.as_str()) && !LAYER_KEYS.contains(&key.as_str()) {
                notes.push(format!(
                    "\"{key}\" isn't something this Yapfast reads; ignored"
                ));
            }
        }
        if let Some(parts) = top.get("parts") {
            parts_notes(parts, &mut notes);
        }
        let Some(Value::Object(variants)) = top.get("variants") else {
            bail!("it needs \"variants\" with \"dark\", \"light\" or both");
        };
        for key in variants.keys() {
            if key != "dark" && key != "light" {
                notes.push(format!(
                    "the variant \"{key}\" isn't dark or light; ignored"
                ));
            }
        }
        let dark = variants.get("dark").cloned();
        let light = variants.get("light").cloned();
        if dark.is_none() && light.is_none() {
            bail!("\"variants\" needs \"dark\", \"light\" or both");
        }
        let shared = Value::Object(
            top.iter()
                .filter(|(key, _)| LAYER_KEYS.contains(&key.as_str()))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        );
        let package = Package {
            id,
            name,
            version,
            description,
            authors,
            license: string(top, "license")?,
            source: string(top, "source")?,
            based_on: string(top, "based_on")?,
            min_yapfast: string(top, "min_yapfast")?,
            preview,
            icon,
            dark,
            light,
            shared,
            notes,
        };
        // Every variant is resolved once here, so a value out of its range is
        // refused when the theme is read, not when someone switches modes.
        let mut notes = Vec::new();
        for mode in package.modes() {
            package.resolve_noting(mode, &mut notes)?;
        }
        let mut package = package;
        for note in notes {
            if !package.notes.contains(&note) {
                package.notes.push(note);
            }
        }
        Ok(package)
    }

    /// The variants it ships.
    pub fn modes(&self) -> Vec<Mode> {
        let mut modes = Vec::new();
        if self.dark.is_some() {
            modes.push(Mode::Dark);
        }
        if self.light.is_some() {
            modes.push(Mode::Light);
        }
        modes
    }

    /// The variant to show when the app wants `wanted`: that one if the theme
    /// ships it, its only one otherwise.
    pub fn mode_for(&self, wanted: Mode) -> Mode {
        match wanted {
            Mode::Dark if self.dark.is_some() => Mode::Dark,
            Mode::Light if self.light.is_some() => Mode::Light,
            _ if self.dark.is_some() => Mode::Dark,
            _ => Mode::Light,
        }
    }

    /// One variant with every layer applied over its base.
    pub fn resolve(&self, mode: Mode) -> Result<Resolved, Error> {
        self.resolve_noting(mode, &mut Vec::new())
    }

    fn resolve_noting(&self, mode: Mode, notes: &mut Vec<String>) -> Result<Resolved, Error> {
        let variant = match mode {
            Mode::Dark => self.dark.as_ref(),
            Mode::Light => self.light.as_ref(),
        }
        .ok_or_else(|| Error(format!("it has no {} variant", mode.name())))?;
        let Value::Object(variant) = variant else {
            bail!("the {} variant has to be an object", mode.name());
        };
        let at = |layer: &str| format!("variants.{}.{layer}", mode.name());
        let base = variant
            .get("base")
            .and_then(Value::as_str)
            .ok_or_else(|| Error(format!("the {} variant needs a \"base\"", mode.name())))?
            .to_owned();
        for key in variant.keys() {
            if key != "base" && !LAYER_KEYS.contains(&key.as_str()) {
                notes.push(format!(
                    "\"{}\" isn't something this Yapfast reads; ignored",
                    at(key)
                ));
            }
        }
        let mut resolved = Resolved {
            dark: mode == Mode::Dark,
            base,
            ..Resolved::default()
        };
        // The shared layer first, then the variant's, each name the variant
        // sets replacing the shared one.
        let Value::Object(shared) = &self.shared else {
            unreachable!("built as an object");
        };
        for (layers, place) in [(shared, String::new()), (variant, at(""))] {
            let place = |layer: &str| {
                if place.is_empty() {
                    layer.to_owned()
                } else {
                    format!("{place}{layer}")
                }
            };
            if let Some(colours) = layers.get("colours") {
                read_colours(colours, &place("colours"), &mut resolved.colours, notes)?;
            }
            if let Some(shape) = layers.get("shape") {
                read_shapes(
                    shape,
                    &place("shape"),
                    &mut resolved.shapes,
                    &mut resolved.radii,
                    &mut resolved.shadows,
                    notes,
                )?;
                super::layout::read_outlines(shape, &place("shape"), &mut resolved.layout, notes)?;
            }
            if let Some(kind) = layers.get("type") {
                read_types(kind, &place("type"), &mut resolved.types, notes)?;
                super::layout::read_ransom(kind, &place("type"), &mut resolved.layout)?;
            }
            if let Some(spacing) = layers.get("spacing") {
                read_spacing(spacing, &place("spacing"), &mut resolved.spacing, notes)?;
            }
            if let Some(motion) = layers.get("motion") {
                read_motion(motion, &place("motion"), &mut resolved.motion, notes)?;
            }
            if let Some(animations) = layers.get("animations") {
                resolved.animations = read_animations(animations, &place("animations"), notes)?;
            }
            if let Some(effects) = layers.get("effects") {
                read_effects(effects, &place("effects"), &mut resolved.effects, notes)?;
            }
            if let Some(value) = layers.get("art") {
                art::read_art(value, &place("art"), &mut resolved.art, notes)?;
            }
            if let Some(sounds) = layers.get("sounds") {
                super::media::read_sounds(sounds, &place("sounds"), &mut resolved.sounds, notes)?;
            }
            if let Some(icons) = layers.get("icons") {
                super::media::read_icons(icons, &place("icons"), &mut resolved.icons, notes)?;
            }
            if let Some(layout) = layers.get("layout") {
                super::layout::read_layout(layout, &place("layout"), &mut resolved.layout, notes)?;
            }
            if let Some(cursors) = layers.get("cursors") {
                super::cursors::read_cursors(
                    cursors,
                    &place("cursors"),
                    &mut resolved.cursors,
                    notes,
                )?;
            }
        }
        Ok(resolved)
    }

    /// Every file the theme names, relative to its folder, once each: what an
    /// install copies and what [`super::check`] reads.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self.preview.clone();
        files.extend(self.icon.clone());
        for mode in self.modes() {
            if let Ok(resolved) = self.resolve(mode) {
                let media = resolved
                    .sounds
                    .values()
                    .flat_map(|choice| choice.files.iter())
                    .chain(resolved.icons.values().map(|icon| &icon.file))
                    .chain(
                        resolved
                            .cursors
                            .values()
                            .flat_map(|cursor| cursor.images.iter()),
                    );
                for file in resolved
                    .fonts()
                    .into_iter()
                    .chain(media.map(PathBuf::as_path))
                {
                    if !files.iter().any(|known| known == file) {
                        files.push(file.to_owned());
                    }
                }
                for picture in resolved.art.files() {
                    if !files.contains(&picture) {
                        files.push(picture);
                    }
                }
            }
        }
        files
    }
}

/// A theme.json for one of the person's own colour themes, so it can be
/// shared as a package: every colour it has, over the built-in theme it
/// began as. `colours` are by their name in `ui/theme.slint`, as ARGB.
pub fn export(name: &str, base: &str, dark: bool, colours: &[(&str, u32)]) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|letter| {
            if letter.is_ascii_alphanumeric() {
                letter
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() {
        "my-theme".to_owned()
    } else {
        slug
    };
    let colours: serde_json::Map<String, Value> = colours
        .iter()
        .filter(|(token, _)| !LOCKED_COLOURS.contains(token))
        .map(|(token, colour)| ((*token).to_owned(), Value::String(hex(*colour))))
        .collect();
    let mode = if dark { "dark" } else { "light" };
    let value = serde_json::json!({
        "theme_api": THEME_API,
        "id": format!("local.{slug}"),
        "name": name,
        "version": "1.0.0",
        "description": "",
        "authors": [],
        "license": "CC0-1.0",
        "source": "",
        "preview": [],
        "variants": { mode: { "base": base, "colours": colours } },
    });
    let mut text = serde_json::to_string_pretty(&value).expect("JSON values serialise");
    text.push('\n');
    text
}

/// A folder name for an exported theme.
pub fn folder_name(name: &str) -> String {
    let text = export(name, "yap", true, &[]);
    let value: Value = serde_json::from_str(&text).expect("just written");
    value["id"]
        .as_str()
        .and_then(|id| id.strip_prefix("local."))
        .unwrap_or("my-theme")
        .to_owned()
}

/// A text field of an object, if it's there.
fn string(object: &serde_json::Map<String, Value>, key: &str) -> Result<Option<String>, Error> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(Error(format!("\"{key}\" has to be text"))),
    }
}

/// Reverse-DNS, as a plugin's: lowercase letters, digits, `-` and `_` in at
/// least two parts.
fn check_id(id: &str) -> Result<(), Error> {
    let parts: Vec<&str> = id.split('.').collect();
    let fine = id.len() <= 128
        && parts.len() >= 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'-'
                        || byte == b'_'
                })
        });
    if fine {
        Ok(())
    } else {
        Err(Error(format!(
            "\"id\" is reverse-DNS in lowercase, such as com.example.vaporwave, not \"{id}\""
        )))
    }
}

/// `1.2.0`, `1.2`, or either with a suffix such as `-beta.1`.
fn check_version(version: &str) -> Result<(), Error> {
    let core = version.split(['-', '+']).next().unwrap_or("");
    let numbers: Vec<&str> = core.split('.').collect();
    if (2..=3).contains(&numbers.len())
        && numbers
            .iter()
            .all(|number| !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
    {
        Ok(())
    } else {
        Err(Error(format!(
            "\"version\" is written as 1.2.0, not \"{version}\""
        )))
    }
}

/// A path inside the theme's folder: relative, with no `..`, no URL, and no
/// backslashes, so it means the same on Windows and Linux.
pub fn relative(text: &str, field: &str) -> Result<PathBuf, Error> {
    if text.contains("://") || text.starts_with("data:") || text.starts_with("//") {
        bail!("\"{field}\" names \"{text}\": a theme bundles its files, it never links to them");
    }
    if text.contains('\\') || text.is_empty() {
        bail!("\"{field}\" names \"{text}\", which isn't a path such as fonts/Body.ttf");
    }
    let path = Path::new(text);
    let inside = path
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
    if !inside {
        bail!("\"{field}\" names \"{text}\", which isn't inside the theme's folder");
    }
    Ok(path
        .components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .collect())
}

/// A colour as `#rgb`, `#rrggbb` or `#rrggbbaa`, as ARGB.
pub fn parse_colour(text: &str) -> Option<u32> {
    let digits = text.trim().strip_prefix('#')?;
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    match digits.len() {
        3 => {
            let nibble = |shift: u32| ((value >> shift) & 0xf) * 0x11;
            Some(0xff00_0000 | (nibble(8) << 16) | (nibble(4) << 8) | nibble(0))
        }
        6 => Some(0xff00_0000 | value),
        8 => Some(value.rotate_right(8)),
        _ => None,
    }
}

/// `#rrggbb`, or `#rrggbbaa` when it isn't opaque.
pub fn hex(colour: u32) -> String {
    match colour >> 24 {
        0xff => format!("#{:06x}", colour & 0xff_ffff),
        alpha => format!("#{:06x}{alpha:02x}", colour & 0xff_ffff),
    }
}

/// A gradient written `linear(<angle>deg, #colour [stop%], …)`.
fn parse_linear(text: &str) -> Option<Paint> {
    let inner = text.trim().strip_prefix("linear(")?.strip_suffix(')')?;
    let mut parts = inner.split(',').map(str::trim).peekable();
    let angle = match parts.peek() {
        Some(first) if first.ends_with("deg") => {
            let angle = first.trim_end_matches("deg").trim().parse::<f32>().ok()?;
            parts.next();
            angle
        }
        _ => 180.0,
    };
    let written: Vec<&str> = parts.collect();
    if !(2..=8).contains(&written.len()) || !angle.is_finite() {
        return None;
    }
    let count = written.len();
    let mut stops = Vec::with_capacity(count);
    for (index, stop) in written.into_iter().enumerate() {
        let mut words = stop.split_whitespace();
        let colour = parse_colour(words.next()?)?;
        let at = match words.next() {
            Some(percent) => percent.strip_suffix('%')?.parse::<f32>().ok()? / 100.0,
            None => index as f32 / (count - 1) as f32,
        };
        if words.next().is_some() || !(0.0..=1.0).contains(&at) {
            return None;
        }
        stops.push((colour, at));
    }
    Some(Paint::Linear {
        angle: angle.rem_euclid(360.0),
        stops,
    })
}

pub(super) fn object<'a>(
    value: &'a Value,
    place: &str,
) -> Result<&'a serde_json::Map<String, Value>, Error> {
    value
        .as_object()
        .ok_or_else(|| Error(format!("\"{place}\" has to be an object")))
}

pub(super) fn number(value: &Value, place: &str, (low, high): (f32, f32)) -> Result<f32, Error> {
    let number = value
        .as_f64()
        .ok_or_else(|| Error(format!("\"{place}\" has to be a number")))? as f32;
    if !(low..=high).contains(&number) {
        bail!("\"{place}\" is {number}, outside {low} to {high}");
    }
    Ok(number)
}

pub(super) fn colour_at(value: &Value, place: &str) -> Result<u32, Error> {
    value
        .as_str()
        .and_then(parse_colour)
        .ok_or_else(|| Error(format!("\"{place}\" has to be a colour such as #1e1f24")))
}

fn read_colours(
    value: &Value,
    place: &str,
    colours: &mut Vec<(&'static str, Paint)>,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    for (name, written) in object(value, place)? {
        let Some(token) = COLOURS
            .iter()
            .chain(EXTRA_COLOURS.iter())
            .copied()
            .find(|token| token == name)
        else {
            notes.push(format!(
                "\"{place}.{name}\" isn't a colour Yapfast has; ignored"
            ));
            continue;
        };
        if LOCKED_COLOURS.contains(&token) {
            notes.push(format!(
                "\"{place}.{name}\" can't be set: warnings and destructive actions keep Yapfast's own red"
            ));
            continue;
        }
        let Some(text) = written.as_str() else {
            bail!("\"{place}.{name}\" has to be a colour such as #1e1f24");
        };
        let paint = if text.trim_start().starts_with("linear(") {
            if !BRUSHES.contains(&token) {
                bail!(
                    "\"{place}.{name}\" can't be a gradient; only {} take one",
                    BRUSHES.join(", ")
                );
            }
            parse_linear(text).ok_or_else(|| {
                Error(format!(
                    "\"{place}.{name}\" isn't a gradient Yapfast reads: write linear(135deg, #ff71ce, #01cdfe)"
                ))
            })?
        } else {
            Paint::Colour(parse_colour(text).ok_or_else(|| {
                Error(format!(
                    "\"{place}.{name}\" is \"{text}\", which isn't a colour such as #1e1f24"
                ))
            })?)
        };
        colours.retain(|(set, _)| *set != token);
        colours.push((token, paint));
    }
    Ok(())
}

fn read_shapes(
    value: &Value,
    place: &str,
    shapes: &mut BTreeMap<Surface, Shape>,
    radii: &mut [Option<f32>; 3],
    shadows: &mut Option<Option<Shadow>>,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    for (name, written) in object(value, place)? {
        if name == "radii" {
            let at = format!("{place}.radii");
            for (key, value) in object(written, &at)? {
                let index = match key.as_str() {
                    "small" => 0,
                    "medium" => 1,
                    "large" => 2,
                    _ => {
                        notes.push(format!(
                            "\"{at}.{key}\" isn't small, medium or large; ignored"
                        ));
                        continue;
                    }
                };
                radii[index] = Some(number(value, &format!("{at}.{key}"), (0.0, 999.0))?);
            }
            continue;
        }
        // Outlines are the layout's: see super::layout::read_outlines.
        if name == "outline_width" || name == "borders" {
            continue;
        }
        if name == "shadows" {
            let at = format!("{place}.shadows");
            *shadows = Some(if written.as_str() == Some("none") {
                None
            } else {
                Some(read_shadow(written, &at)?)
            });
            continue;
        }
        let Some(surface) = Surface::named(name) else {
            notes.push(format!(
                "\"{place}.{name}\" isn't a surface Yapfast has; ignored"
            ));
            continue;
        };
        let at = format!("{place}.{name}");
        let shape = shapes.entry(surface).or_default();
        for (key, value) in object(written, &at)? {
            let here = format!("{at}.{key}");
            match key.as_str() {
                "radius" => shape.radius = Some(number(value, &here, (0.0, surface.max_radius()))?),
                "border" => shape.border = Some(number(value, &here, (0.0, 4.0))?),
                "border_colour" => shape.border_colour = Some(colour_at(value, &here)?),
                "shadow" => {
                    shape.shadow = if value.as_str() == Some("none") {
                        Some(Shadow {
                            blur: 0.0,
                            x: 0.0,
                            y: 0.0,
                            colour: 0,
                        })
                    } else {
                        Some(read_shadow(value, &here)?)
                    };
                }
                _ => notes.push(format!(
                    "\"{here}\" isn't something a surface takes; ignored"
                )),
            }
        }
    }
    Ok(())
}

fn read_shadow(value: &Value, here: &str) -> Result<Shadow, Error> {
    let fields = object(value, here)?;
    let get = |key: &str, range: (f32, f32), default: f32| {
        fields.get(key).map_or(Ok(default), |value| {
            number(value, &format!("{here}.{key}"), range)
        })
    };
    Ok(Shadow {
        blur: get("blur", (0.0, 48.0), 12.0)?,
        x: get("x", (-16.0, 16.0), 0.0)?,
        y: get("y", (-16.0, 32.0), 4.0)?,
        colour: fields.get("colour").map_or(Ok(0x6600_0000), |value| {
            colour_at(value, &format!("{here}.colour"))
        })?,
    })
}

fn read_types(
    value: &Value,
    place: &str,
    types: &mut BTreeMap<Role, Type>,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    for (name, written) in object(value, place)? {
        let Some(role) = Role::named(name) else {
            notes.push(format!(
                "\"{place}.{name}\" isn't a kind of text Yapfast has; ignored"
            ));
            continue;
        };
        let at = format!("{place}.{name}");
        let kind = types.entry(role).or_default();
        for (key, value) in object(written, &at)? {
            let here = format!("{at}.{key}");
            match key.as_str() {
                "font" => {
                    let text = value.as_str().ok_or_else(|| {
                        Error(format!("\"{here}\" names a font file in the theme"))
                    })?;
                    let path = relative(text, &here)?;
                    let extension = path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .map(str::to_ascii_lowercase);
                    if !matches!(extension.as_deref(), Some("ttf" | "otf")) {
                        bail!("\"{here}\" names \"{text}\": fonts are .ttf or .otf files");
                    }
                    kind.font = Some(path);
                }
                "size" => kind.size = Some(number(value, &here, role.sizes())?),
                "weight" => {
                    let weight = number(value, &here, (100.0, 900.0))?;
                    kind.weight = Some(weight.round() as i32);
                }
                "letter_spacing" => kind.letter_spacing = Some(number(value, &here, (-2.0, 4.0))?),
                "header_size" if role == Role::Headings => {
                    kind.header_size = Some(number(value, &here, (12.0, 32.0))?);
                }
                "title_size" if role == Role::Headings => {
                    kind.title_size = Some(number(value, &here, (12.0, 40.0))?);
                }
                // Ransom names are the layout's: see super::layout::read_ransom.
                key if role == Role::Names && super::layout::RANSOM_KEYS.contains(&key) => {}
                "uppercase" if role == Role::Headings => {
                    kind.uppercase = Some(
                        value
                            .as_bool()
                            .ok_or_else(|| Error(format!("\"{here}\" is true or false")))?,
                    );
                }
                _ => notes.push(format!("\"{here}\" isn't something {name} takes; ignored")),
            }
        }
    }
    Ok(())
}

fn read_spacing(
    value: &Value,
    place: &str,
    spacing: &mut Spacing,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    let fields = object(value, place)?;
    // A density first, so values named beside it win.
    if let Some(density) = fields.get("density") {
        let name = density.as_str().unwrap_or("");
        let preset = Spacing::density(name).ok_or_else(|| {
            Error(format!(
                "\"{place}.density\" is compact, default or roomy, not \"{name}\""
            ))
        })?;
        for (field, ..) in Spacing::FIELDS {
            let value = preset.clone().slot(field).and_then(|value| *value);
            if value.is_some()
                && let Some(slot) = spacing.slot(field)
            {
                *slot = value;
            }
        }
    }
    for (key, value) in fields {
        if key == "density" {
            continue;
        }
        let here = format!("{place}.{key}");
        let Some(&(_, low, high)) = Spacing::FIELDS.iter().find(|(name, ..)| name == key) else {
            notes.push(format!("\"{here}\" isn't a spacing Yapfast has; ignored"));
            continue;
        };
        let number = number(value, &here, (low, high))?;
        if let Some(slot) = spacing.slot(key) {
            *slot = Some(number);
        }
    }
    Ok(())
}

pub(super) fn named<T: Copy>(
    value: &Value,
    place: &str,
    all: &[T],
    name: impl Fn(T) -> &'static str,
) -> Result<T, Error> {
    let text = value.as_str().unwrap_or("");
    all.iter()
        .copied()
        .find(|item| name(*item) == text)
        .ok_or_else(|| {
            let names: Vec<&str> = all.iter().map(|item| name(*item)).collect();
            Error(format!(
                "\"{place}\" is one of {}, not \"{text}\"",
                names.join(", ")
            ))
        })
}

fn read_motion(
    value: &Value,
    place: &str,
    motion: &mut Motion,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    for (key, value) in object(value, place)? {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "quick" => motion.quick = Some(number(value, &here, (0.0, 1000.0))?),
            "popup" => motion.popup = Some(number(value, &here, (0.0, 1000.0))?),
            // Slint's popups take a named curve: a cubic-bezier picks the
            // nearest one.
            "curve"
                if value
                    .as_str()
                    .is_some_and(|text| text.starts_with("cubic-bezier(")) =>
            {
                let points = bezier(value, &here)?;
                motion.curve = Some(nearest_curve(points));
            }
            "curve" => motion.curve = Some(named(value, &here, &Curve::ALL, Curve::name)?),
            "hover" => motion.hover = Some(named(value, &here, &Hover::ALL, Hover::name)?),
            "press" => motion.press = Some(named(value, &here, &Press::ALL, Press::name)?),
            "open" => motion.open = Some(named(value, &here, &Transition::ALL, Transition::name)?),
            "close" => motion.close = Some(named(value, &here, &Close::ALL, Close::name)?),
            "hover_in" => motion.hover_in = Some(number(value, &here, (0.0, 2000.0))?),
            "hover_out" => motion.hover_out = Some(number(value, &here, (0.0, 2000.0))?),
            _ => notes.push(format!("\"{here}\" isn't something motion takes; ignored")),
        }
    }
    Ok(())
}

/// `cubic-bezier(x1, y1, x2, y2)`, as CSS writes it: x from 0 to 1, y from
/// -1 to 2.
fn bezier(value: &Value, place: &str) -> Result<[f32; 4], Error> {
    let text = value.as_str().unwrap_or("");
    let inner = text
        .trim()
        .strip_prefix("cubic-bezier(")
        .and_then(|rest| rest.strip_suffix(')'));
    let numbers: Option<Vec<f32>> = inner.map(|inner| {
        inner
            .split(',')
            .filter_map(|part| part.trim().parse::<f32>().ok())
            .collect()
    });
    match numbers.as_deref() {
        Some(&[x1, y1, x2, y2])
            if (0.0..=1.0).contains(&x1)
                && (0.0..=1.0).contains(&x2)
                && (-1.0..=2.0).contains(&y1)
                && (-1.0..=2.0).contains(&y2) =>
        {
            Ok([x1, y1, x2, y2])
        }
        _ => {
            bail!("\"{place}\" is \"{text}\", not cubic-bezier(x1, y1, x2, y2) with x from 0 to 1")
        }
    }
}

/// The named curve closest to a cubic-bezier, for where only named ones go.
fn nearest_curve(points: [f32; 4]) -> Curve {
    let at = |curve: Curve, t: f32| super::motion::ease(curve, t);
    let wanted =
        |t: f32| super::motion::cubic_bezier(points[0], points[1], points[2], points[3], t);
    [
        Curve::Linear,
        Curve::Ease,
        Curve::EaseIn,
        Curve::EaseOut,
        Curve::EaseInOut,
        Curve::Spring,
    ]
    .into_iter()
    .min_by(|one, two| {
        let error = |curve: Curve| {
            (1..10)
                .map(|step| (at(curve, step as f32 / 10.0) - wanted(step as f32 / 10.0)).abs())
                .sum::<f32>()
        };
        error(*one).total_cmp(&error(*two))
    })
    .unwrap_or_default()
}

fn read_animations(
    value: &Value,
    place: &str,
    notes: &mut Vec<String>,
) -> Result<Vec<Animation>, Error> {
    let items = value
        .as_array()
        .ok_or_else(|| Error(format!("\"{place}\" is a list of animations")))?;
    if items.len() > MAX_ANIMATIONS {
        bail!("\"{place}\" lists at most {MAX_ANIMATIONS} animations");
    }
    let mut animations = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let at = format!("{place}[{index}]");
        let fields = object(item, &at)?;
        let slot_name = fields.get("slot").and_then(Value::as_str).unwrap_or("");
        let Some(slot) = Slot::named(slot_name) else {
            notes.push(format!(
                "\"{at}.slot\" is \"{slot_name}\", which isn't a slot Yapfast has; that animation is left out"
            ));
            continue;
        };
        let property_name = fields.get("property").and_then(Value::as_str).unwrap_or("");
        let Some(property) = Property::named(property_name) else {
            bail!("\"{at}.property\" is \"{property_name}\", which isn't one Yapfast animates");
        };
        if !slot.properties().contains(&property) {
            let names: Vec<&str> = slot
                .properties()
                .iter()
                .map(|property| property.name())
                .collect();
            bail!(
                "\"{at}\" moves {} of {}, which only moves {}",
                property.name(),
                slot.name(),
                names.join(", ")
            );
        }
        let values = fields
            .get("values")
            .and_then(Value::as_array)
            .ok_or_else(|| Error(format!("\"{at}.values\" is a list")))?;
        if !(2..=32).contains(&values.len()) {
            bail!("\"{at}.values\" lists 2 to 32 values");
        }
        let (mut numbers, mut colours) = (Vec::new(), Vec::new());
        for (step, value) in values.iter().enumerate() {
            let here = format!("{at}.values[{step}]");
            if property.is_colour() {
                colours.push(colour_at(value, &here)?);
            } else {
                numbers.push(number(value, &here, property.range())?);
            }
        }
        let period = fields.get("period").map_or(Ok(1000.0), |value| {
            number(value, &format!("{at}.period"), (100.0, 60_000.0))
        })?;
        let steps = fields
            .get("steps")
            .map(|value| {
                number(value, &format!("{at}.steps"), (1.0, 120.0)).map(|steps| steps as u32)
            })
            .transpose()?;
        let written_bezier = fields.get("easing").filter(|value| {
            value
                .as_str()
                .is_some_and(|text| text.starts_with("cubic-bezier("))
        });
        let bezier = written_bezier
            .map(|value| bezier(value, &format!("{at}.easing")))
            .transpose()?;
        let easing = match fields.get("easing") {
            Some(_) if bezier.is_some() => Curve::Linear,
            Some(value) => named(value, &format!("{at}.easing"), &Curve::ALL, Curve::name)?,
            None => Curve::Linear,
        };
        let phase = fields.get("phase").map_or(Ok(0.0), |value| {
            number(value, &format!("{at}.phase"), (0.0, 5000.0))
        })?;
        let repeat = match fields.get("repeat").and_then(Value::as_str) {
            None | Some("loop") => Repeat::Loop,
            Some("alternate") => Repeat::Alternate,
            Some("once") => Repeat::Once,
            Some(other) => bail!("\"{at}.repeat\" is loop, alternate or once, not \"{other}\""),
        };
        let delay = fields.get("delay").map_or(Ok(0.0), |value| {
            number(value, &format!("{at}.delay"), (0.0, 60_000.0))
        })?;
        let heavy = fields.get("heavy").and_then(Value::as_bool).unwrap_or(false)
            // The whole window moving is heavy whatever the theme says.
            || slot == Slot::Window
            || slot == Slot::Accent;
        for key in fields.keys() {
            if ![
                "slot", "property", "values", "period", "steps", "easing", "repeat", "delay",
                "heavy", "name", "phase",
            ]
            .contains(&key.as_str())
            {
                notes.push(format!(
                    "\"{at}.{key}\" isn't something an animation takes; ignored"
                ));
            }
        }
        animations.push(Animation {
            slot,
            property,
            values: numbers,
            colours,
            period,
            steps,
            easing,
            repeat,
            delay,
            heavy,
            bezier,
            phase,
        });
    }
    Ok(animations)
}

fn read_effects(
    value: &Value,
    place: &str,
    effects: &mut Vec<TextEffect>,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    for (name, written) in object(value, place)? {
        let here = format!("{place}.{name}");
        let Some(slot) = TextSlot::named(name) else {
            notes.push(format!("\"{here}\" isn't a text slot Yapfast has; ignored"));
            continue;
        };
        let (effect_name, duration) = match written {
            Value::String(effect) => (effect.as_str(), None),
            Value::Object(fields) => (
                fields.get("effect").and_then(Value::as_str).unwrap_or(""),
                fields.get("duration"),
            ),
            Value::Null => {
                effects.retain(|effect| effect.slot != slot);
                continue;
            }
            _ => bail!("\"{here}\" is an effect's name, or an object with \"effect\""),
        };
        let effect = Effect::named(effect_name).ok_or_else(|| {
            Error(format!(
                "\"{here}\" is decrypt or typewriter, not \"{effect_name}\""
            ))
        })?;
        let duration = duration.map_or(Ok(600.0), |value| {
            number(value, &format!("{here}.duration"), (100.0, 3000.0))
        })?;
        effects.retain(|effect| effect.slot != slot);
        effects.push(TextEffect {
            slot,
            effect,
            duration,
        });
    }
    Ok(())
}

/// Parts aren't drawn yet: each declared part gets a note saying the stock
/// part shows, and why, so its author knows it's waiting on Yapfast and not
/// broken.
fn parts_notes(parts: &Value, notes: &mut Vec<String>) {
    let Some(parts) = parts.as_object() else {
        notes.push("\"parts\" has to be an object; ignored".into());
        return;
    };
    for (name, part) in parts {
        if !PARTS.contains(&name.as_str()) {
            notes.push(format!(
                "\"parts.{name}\" isn't a part Yapfast has; ignored"
            ));
            continue;
        }
        let contract = part.get("contract").and_then(Value::as_str).unwrap_or("");
        let offered = PART_CONTRACTS
            .iter()
            .find(|(part, ..)| part == name)
            .map(|(_, major, minor)| (*major, *minor));
        let wanted = contract.split_once('.').and_then(|(major, minor)| {
            Some((major.parse::<u32>().ok()?, minor.parse::<u32>().ok()?))
        });
        let note = match (offered, wanted) {
            (None, _) => format!("\"parts.{name}\" needs a later Yapfast; the stock {name} shows"),
            (Some(_), None) => {
                format!("\"parts.{name}.contract\" is written as 1.0; the stock {name} shows")
            }
            (Some((major, minor)), Some((wanted_major, wanted_minor)))
                if wanted_major != major || wanted_minor > minor =>
            {
                format!(
                    "\"parts.{name}\" is built for contract {wanted_major}.{wanted_minor}, and this \
                     Yapfast offers {major}.{minor}; the stock {name} shows"
                )
            }
            (Some(_), Some(_)) => continue,
        };
        notes.push(note);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: &str = r##"{
        "theme_api": 1,
        "id": "com.example.plum",
        "name": "Plum",
        "version": "1.0.0",
        "variants": { "dark": { "base": "yap", "colours": { "chat": "#2a1a3a" } } }
    }"##;

    #[test]
    fn reads_a_theme_of_one_colour() {
        let package = Package::parse(SMALL).unwrap();
        assert_eq!(package.id, "com.example.plum");
        assert_eq!(package.modes(), vec![Mode::Dark]);
        let resolved = package.resolve(Mode::Dark).unwrap();
        assert_eq!(resolved.base, "yap");
        assert_eq!(resolved.paint("chat"), Some(&Paint::Colour(0xff2a_1a3a)));
        assert!(package.notes.is_empty(), "{:?}", package.notes);
    }

    #[test]
    fn a_variant_overrides_the_shared_layers() {
        let package = Package::parse(
            r##"{
            "theme_api": 1, "id": "com.example.two", "name": "Two", "version": "1.0",
            "colours": { "accent": "#ff0000", "chat": "#101010" },
            "shape": { "button": { "radius": 999 } },
            "variants": {
                "dark": { "base": "yap" },
                "light": { "base": "sakura", "colours": { "chat": "#fafafa" }, "shape": { "button": { "radius": 2 } } }
            }
        }"##,
        )
        .unwrap();
        let dark = package.resolve(Mode::Dark).unwrap();
        let light = package.resolve(Mode::Light).unwrap();
        assert_eq!(dark.paint("chat"), Some(&Paint::Colour(0xff10_1010)));
        assert_eq!(light.paint("chat"), Some(&Paint::Colour(0xfffa_fafa)));
        assert_eq!(light.paint("accent"), Some(&Paint::Colour(0xffff_0000)));
        assert_eq!(dark.shapes[&Surface::Button].radius, Some(999.0));
        assert_eq!(light.shapes[&Surface::Button].radius, Some(2.0));
        assert!(!light.dark);
    }

    #[test]
    fn follows_the_app_when_it_ships_both_and_keeps_its_one_otherwise() {
        let only_dark = Package::parse(SMALL).unwrap();
        assert_eq!(only_dark.mode_for(Mode::Light), Mode::Dark);
        let both = Package::parse(&SMALL.replace(
            r#""variants": { "dark""#,
            r#""variants": { "light": { "base": "sakura" }, "dark""#,
        ))
        .unwrap();
        assert_eq!(both.mode_for(Mode::Light), Mode::Light);
        assert_eq!(both.mode_for(Mode::Dark), Mode::Dark);
    }

    #[test]
    fn unknown_names_are_noted_not_refused() {
        let package = Package::parse(
            r##"{
            "theme_api": 1, "id": "com.example.later", "name": "Later", "version": "1.0.0",
            "haptics": { "press": "short" },
            "parts": { "message-row": { "contract": "1.0" }, "whole-window": {} },
            "variants": { "dark": { "base": "yap",
                "colours": { "sparkle": "#ffffff", "danger": "#00ff00" },
                "shape": { "hologram": { "radius": 2 } },
                "effects": { "footer": "decrypt" } } }
        }"##,
        )
        .unwrap();
        let notes = package.notes.join("\n");
        for expected in [
            "\"haptics\"",
            "parts.message-row\" needs a later Yapfast",
            "parts.whole-window",
            "sparkle",
            "danger\" can't be set",
            "hologram",
            "footer",
        ] {
            assert!(notes.contains(expected), "{expected} in {notes}");
        }
        assert!(
            package
                .resolve(Mode::Dark)
                .unwrap()
                .paint("danger")
                .is_none()
        );
    }

    #[test]
    fn refuses_values_out_of_range_with_their_place() {
        let error = Package::parse(
            r##"{ "theme_api": 1, "id": "com.example.big", "name": "Big", "version": "1.0.0",
            "variants": { "dark": { "base": "yap", "spacing": { "avatar_size": 400 } } } }"##,
        )
        .unwrap_err();
        assert!(
            error.0.contains("variants.dark.spacing.avatar_size"),
            "{error}"
        );
        assert!(error.0.contains("24 to 56"), "{error}");
    }

    #[test]
    fn refuses_another_api_and_a_bad_id() {
        let later = SMALL.replace("\"theme_api\": 1", "\"theme_api\": 2");
        assert!(
            Package::parse(&later)
                .unwrap_err()
                .0
                .contains("theme API 2")
        );
        let id = SMALL.replace("com.example.plum", "Plum Theme");
        assert!(Package::parse(&id).unwrap_err().0.contains("reverse-DNS"));
    }

    #[test]
    fn files_stay_inside_the_folder_and_are_never_links() {
        for (path, says) in [
            ("https://example.com/font.ttf", "never links"),
            ("../secret.ttf", "inside the theme's folder"),
            ("/etc/passwd.ttf", "inside the theme's folder"),
            ("fonts\\\\Body.ttf", "isn't a path"),
            ("fonts/Body.woff2", ".ttf or .otf"),
        ] {
            let text = SMALL.replace(
                r##""colours": { "chat": "#2a1a3a" }"##,
                &format!(r#""type": {{ "body": {{ "font": "{path}" }} }}"#),
            );
            let error = Package::parse(&text).unwrap_err();
            assert!(error.0.contains(says), "{path}: {error}");
        }
        let fine = SMALL.replace(
            r##""colours": { "chat": "#2a1a3a" }"##,
            r#""type": { "body": { "font": "./fonts/Body.ttf", "size": 15 } }"#,
        );
        let package = Package::parse(&fine).unwrap();
        assert_eq!(package.files(), vec![PathBuf::from("fonts/Body.ttf")]);
    }

    #[test]
    fn reads_gradients_only_where_a_brush_goes() {
        let text = SMALL.replace("\"#2a1a3a\"", "\"linear(135deg, #ff71ce, #01cdfe 80%)\"");
        let resolved = Package::parse(&text).unwrap().resolve(Mode::Dark).unwrap();
        assert_eq!(
            resolved.paint("chat"),
            Some(&Paint::Linear {
                angle: 135.0,
                stops: vec![(0xffff_71ce, 0.0), (0xff01_cdfe, 0.8)]
            })
        );
        let text = SMALL.replace(
            r##""chat": "#2a1a3a""##,
            r##""text": "linear(#ffffff, #000000)""##,
        );
        assert!(
            Package::parse(&text)
                .unwrap_err()
                .0
                .contains("can't be a gradient")
        );
    }

    #[test]
    fn reads_declared_animations_and_effects() {
        let text = SMALL.replace(
            r##""colours": { "chat": "#2a1a3a" }"##,
            r##""animations": [
                { "slot": "window", "property": "opacity", "values": [0, 0.04, 0], "period": 120, "steps": 3 },
                { "slot": "unread", "property": "scale", "values": [1, 1.2], "period": 800, "easing": "ease-in-out", "repeat": "alternate" }
            ],
            "effects": { "channel.name": "decrypt", "profile.name": { "effect": "typewriter", "duration": 900 } }"##,
        );
        let resolved = Package::parse(&text).unwrap().resolve(Mode::Dark).unwrap();
        assert_eq!(resolved.animations.len(), 2);
        assert!(
            resolved.animations[0].heavy,
            "the whole window is always heavy"
        );
        assert_eq!(resolved.animations[1].repeat, Repeat::Alternate);
        assert_eq!(resolved.effects.len(), 2);
        let wrong = text.replace("\"property\": \"scale\"", "\"property\": \"tint\"");
        assert!(Package::parse(&wrong).unwrap_err().0.contains("only moves"));
    }

    #[test]
    fn a_density_is_a_starting_point_for_named_spacing() {
        let text = SMALL.replace(
            r##""colours": { "chat": "#2a1a3a" }"##,
            r#""spacing": { "density": "compact", "avatar_size": 28 }"#,
        );
        let spacing = Package::parse(&text)
            .unwrap()
            .resolve(Mode::Dark)
            .unwrap()
            .spacing;
        assert_eq!(spacing.avatar_size, Some(28.0));
        assert_eq!(spacing.message_group_gap, Some(6.0));
    }

    #[test]
    fn reads_header_and_title_sizes_and_the_newer_colours() {
        let text = SMALL.replace(
            r##""colours": { "chat": "#2a1a3a" }"##,
            r##""colours": { "pill": "#3fd6b8", "row-selected": "#2a4c5c", "code": "#101010", "composer-focus": "#ffffff80" },
                "type": { "headings": { "header_size": 18, "title_size": 28 }, "body": { "weight": 300 } }"##,
        );
        let resolved = Package::parse(&text).unwrap().resolve(Mode::Dark).unwrap();
        assert_eq!(resolved.paint("pill"), Some(&Paint::Colour(0xff3f_d6b8)));
        assert_eq!(resolved.types[&Role::Headings].header_size, Some(18.0));
        assert_eq!(resolved.types[&Role::Headings].title_size, Some(28.0));
        assert_eq!(resolved.types[&Role::Body].weight, Some(300));
    }

    #[test]
    fn a_custom_theme_exports_as_a_package_that_reads_back() {
        let text = export(
            "Plum Night!",
            "yap",
            true,
            &[("chat", 0xff2a_1a3a), ("danger", 0xff00_ff00)],
        );
        let package = Package::parse(&text).unwrap();
        assert_eq!(package.id, "local.plum-night");
        assert_eq!(package.name, "Plum Night!");
        let resolved = package.resolve(Mode::Dark).unwrap();
        assert_eq!(resolved.paint("chat"), Some(&Paint::Colour(0xff2a_1a3a)));
        assert!(
            resolved.paint("danger").is_none(),
            "the danger colours stay Yapfast's"
        );
        assert_eq!(folder_name("Plum Night!"), "plum-night");
    }

    #[test]
    fn a_part_for_a_contract_yapfast_offers_is_quiet() {
        // No part is offered yet, so every declared part is noted; this keeps
        // the note's wording for the major mismatch honest once one is.
        let package = Package::parse(&SMALL.replace(
            "\"variants\"",
            r#""parts": { "member-row": { "contract": "2.0" } }, "variants""#,
        ))
        .unwrap();
        assert!(package.notes.iter().any(|note| note.contains("member-row")));
    }
}
