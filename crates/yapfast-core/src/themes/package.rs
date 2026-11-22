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

use super::{MANIFEST, THEME_API};

pub mod art;

/// The colours a theme sets: every colour of the window's
/// [`Theme`](crate::theme::Theme), by its name as `theme.json` writes it
/// ([`Theme::TOKENS`](crate::theme::Theme::TOKENS)). A built-in base sets
/// each.
pub const COLOURS: &[&str] = crate::theme::Theme::TOKENS;

/// The most characters a theme's `description` holds.
pub const MAX_DESCRIPTION: usize = 300;

/// Colours only a theme sets, each following another until a
/// theme does: what it follows is [`follows`].
pub const EXTRA_COLOURS: [&str; 30] = [
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
    "actions",
    "actions-hover",
    "row-selected-text",
    "card-button",
    "card-button-hover",
    "card-button-icon",
    "mention-chip",
];

/// The colour an extra colour shows while a theme doesn't set it, for
/// checking text on it.
pub fn follows(token: &str) -> Option<&'static str> {
    Some(match token {
        "accent-text" | "focus-ring" => "accent",
        "on-tooltip" => "text",
        "menu" | "actions" => "raised",
        "actions-hover" => "hover",
        "composer" => "field",
        "menu-line" => "line",
        "placeholder" => "muted",
        "row-selected" => "active",
        "row-selected-text" => "text",
        "pill" => "text",
        "toggle-off" => "field",
        "knob-off" => "secondary",
        "code" => "panel",
        "embed" => "sidebar",
        "members" => "chat",
        "account" => "panel",
        "mention-chip" => "link",
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
pub const TEXT_ON: [(&str, &[&str]); 19] = [
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
    ("row-selected-text", &["row-selected"]),
    ("text", &["code"]),
    ("text", &["embed"]),
    ("text", &["members"]),
    ("text", &["account"]),
    ("secondary", &["account"]),
    ("muted", &["account"]),
    ("text", &["actions"]),
    ("mention-chip", &["chat"]),
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
    /// The bar of actions over a message under the pointer: quick
    /// reactions, Reply, Forward, More.
    Actions,
    /// The round buttons over a profile card's banner.
    CardButton,
    /// A message row's hover, mention and jump bands: `radius` and `inset`;
    /// and `stripe`, a tint on every other row of the lists it names.
    Row,
    /// Channel categories' and member groups' headings, as bars: `ground`,
    /// `text`, and the surface's corners, border and shadow.
    Label,
    /// The bar over the conversation, with the channel's name: `ground`,
    /// `text`, corners, border and shadow.
    Header,
    /// The sections of a profile card (its bio, dates, roles and what the
    /// person is doing) as boxes: `ground`, `bar` and `bar_text` for each
    /// section's title bar, and the box's corners, border and shadow.
    CardSection,
    /// The tiles on a call's stage: people, their cameras.
    Call,
    /// What shows a stream or a shared screen: its tile on the stage and
    /// the picture in picture over the window.
    Stream,
    /// Notes over the window and the conversation: a toast, the
    /// zoom level, the new messages and jump to present bars.
    Toast,
    /// The emoji, GIF and sticker pickers.
    Picker,
    /// Popouts from the header and the title bar: pins, threads, the
    /// topic, adding people, the inbox.
    Popout,
    /// Scrollbars: the thumb's `radius`, and its `width` at rest and
    /// `hover_width` under the pointer.
    Scrollbar,
}

impl Surface {
    pub const ALL: [Self; 27] = [
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
        Self::Actions,
        Self::Row,
        Self::CardButton,
        Self::Label,
        Self::Header,
        Self::CardSection,
        Self::Call,
        Self::Stream,
        Self::Toast,
        Self::Picker,
        Self::Popout,
        Self::Scrollbar,
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
            Self::Actions => "actions",
            Self::Row => "row",
            Self::CardButton => "card-button",
            Self::Label => "label",
            Self::Header => "header",
            Self::CardSection => "card_section",
            Self::Call => "call",
            Self::Stream => "stream",
            Self::Toast => "toast",
            Self::Picker => "picker",
            Self::Popout => "popout",
            Self::Scrollbar => "scrollbar",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        let name = if name == "card-section" {
            "card_section"
        } else {
            name
        };
        Self::ALL.into_iter().find(|surface| surface.name() == name)
    }

    /// Whether it's a surface a theme gives its own ground and text.
    pub fn framed(self) -> bool {
        matches!(self, Self::Label | Self::Header | Self::CardSection)
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
    /// The small fixed text: reply previews, member status lines and the
    /// text rail's lines.
    Small,
    /// Messages' times.
    Time,
}

impl Role {
    pub const ALL: [Self; 7] = [
        Self::Body,
        Self::Headings,
        Self::Names,
        Self::Code,
        Self::Labels,
        Self::Small,
        Self::Time,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Headings => "headings",
            Self::Names => "names",
            Self::Code => "code",
            Self::Labels => "labels",
            Self::Small => "small",
            Self::Time => "time",
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
            Self::Small | Self::Time => (9.0, 16.0),
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
    /// Overshoots a little and settles: ease-out-back.
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

    /// Its index in [`Self::ALL`], for an interface that keeps curves as
    /// numbers.
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
    /// `row` only: pixels a band keeps from the column's sides and from the
    /// row's top and bottom.
    pub inset: Option<(f32, f32)>,
    /// `card` only: pixels kept above a profile card's banner, for chrome
    /// such as a title bar drawn by the card's art.
    pub inset_top: Option<f32>,
    /// `scrollbar` only: the thumb's width at rest and under the pointer.
    pub width: Option<f32>,
    pub hover_width: Option<f32>,
    /// `label`, `header` and `card_section`: the surface's own ground, a
    /// colour or a gradient, and the text on it, ARGB.
    pub ground: Option<Paint>,
    pub text: Option<u32>,
    /// `card_section`: each section's title bar, and its text.
    pub bar: Option<Paint>,
    pub bar_text: Option<u32>,
    /// `row`: a tint on every other row.
    pub stripe: Option<Stripe>,
    /// `label` and `card_section`: their words in `lower` or `upper` case
    /// (1 and 2), or as written.
    pub case: Option<u8>,
}

/// Every other row of a list tinted, as a ledger's or a zebra's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stripe {
    /// ARGB.
    pub colour: u32,
    pub channels: bool,
    pub members: bool,
    pub messages: bool,
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
    /// What the theme's motion does while Yapfast shows but another window
    /// has the focus; `slow` when left out.
    pub unfocused: Option<Unfocused>,
}

/// What a theme's always-on motion does while the window shows but isn't
/// focused. Hidden, nothing moves whatever this says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Unfocused {
    /// As focused.
    Full,
    /// At most 15 frames a second.
    #[default]
    Slow,
    /// Held where it is, and goes on from there with the focus.
    Still,
}

impl Unfocused {
    pub const ALL: [Self; 3] = [Self::Full, Self::Slow, Self::Still];

    pub fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Slow => "slow",
            Self::Still => "still",
        }
    }
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
    /// Colours and gradients over the base, by their name in [`COLOURS`] or
    /// [`EXTRA_COLOURS`].
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
    /// How people's pictures, names and profiles are drawn, when the person
    /// lets the theme: [`super::content`].
    pub content: super::content::Content,
    /// The words and marks around the window: [`super::chrome`].
    pub chrome: super::chrome::Chrome,
    /// Named parts' styles, by state: [`super::parts`].
    pub parts: super::parts::Parts,
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
    /// What the person can switch, choose or slide in it:
    /// [`super::options`]. A theme with ransom names offers one more,
    /// [`super::options::RANSOM`], unless it declares its own.
    pub options: Vec<super::options::ThemeOption>,
}

/// The top-level keys of `theme.json`.
const TOP_KEYS: [&str; 16] = [
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
    "options",
    "sets",
];

/// The layers, at the top level or in a variant.
const LAYER_KEYS: [&str; 14] = [
    "art",
    "chrome",
    "content",
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
    "parts",
];

/// A layer older themes may carry that Yapfast no longer draws: custom
/// mouse cursors. It's noted, not refused, so those themes still install.
const RETIRED_KEYS: [&str; 1] = ["cursors"];

/// The note for a retired layer at `place` (`cursors`, or
/// `variants.dark.cursors`).
fn retired_note(place: &str) -> String {
    format!(
        "\"{place}\": Yapfast doesn't use custom cursors; the system's cursor shows, and the theme's cursor files aren't installed"
    )
}

/// The most authors a theme names, and the longest one.
pub const MAX_AUTHORS: usize = 8;
pub const MAX_AUTHOR: usize = 64;
/// The longest licence, source or `based_on` a theme gives.
pub const MAX_SHORT_FIELD: usize = 200;

/// Refuses text a theme wants shown that holds a character
/// [`super::refused_char`] names, so the review can't show a name turned
/// around, split over lines or hiding part of itself.
pub(crate) fn plain(field: &str, text: &str) -> Result<(), Error> {
    if let Some(c) = text.chars().find(|c| super::refused_char(*c)) {
        bail!(
            "\"{field}\" holds U+{:04X}, a control, line-break or text-direction character; take it out",
            u32::from(c)
        );
    }
    Ok(())
}

/// At most [`MAX_AUTHORS`] names of 1 to [`MAX_AUTHOR`] characters each,
/// every one [`plain`].
fn check_authors(authors: &[String]) -> Result<(), Error> {
    if authors.len() > MAX_AUTHORS {
        bail!("\"authors\" names at most {MAX_AUTHORS} people");
    }
    for author in authors {
        if author.trim().is_empty() || author.chars().count() > MAX_AUTHOR {
            bail!("each of \"authors\" is 1 to {MAX_AUTHOR} characters");
        }
        plain("authors", author)?;
    }
    Ok(())
}

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
        plain("name", &name)?;
        let version =
            string(top, "version")?.ok_or_else(|| Error("it needs a \"version\"".into()))?;
        check_version(&version)?;
        let description = string(top, "description")?.unwrap_or_default();
        let length = description.chars().count();
        if length > MAX_DESCRIPTION {
            bail!(
                "\"description\" is {length} characters, and a theme's description is at most {MAX_DESCRIPTION}; shorten it"
            );
        }
        plain("description", &description)?;
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
        check_authors(&authors)?;
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
            if RETIRED_KEYS.contains(&key.as_str()) {
                notes.push(retired_note(key));
            } else if !TOP_KEYS.contains(&key.as_str()) && !LAYER_KEYS.contains(&key.as_str()) {
                notes.push(format!(
                    "\"{key}\" isn't something this Yapfast reads; ignored"
                ));
            }
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
        // A set may still change a retired layer (an older theme's cursor
        // colours): read, then ignored with the layer, and noted.
        let readable: Vec<&str> = LAYER_KEYS.iter().chain(&RETIRED_KEYS).copied().collect();
        let options = super::options::read_options(top.get("options"), top.get("sets"), &readable)?;
        if let Some(Value::Object(sets)) = top.get("sets") {
            for (name, changes) in sets {
                let retired = changes.as_object().is_some_and(|changes| {
                    changes.keys().any(|path| {
                        let steps: Vec<&str> = path.split('.').collect();
                        let layer = match steps.as_slice() {
                            ["variants", "dark" | "light", layer, ..] => *layer,
                            [layer, ..] => *layer,
                            [] => "",
                        };
                        RETIRED_KEYS.contains(&layer)
                    })
                });
                if retired {
                    notes.push(format!(
                        "\"sets.{name}\": its cursor changes are ignored; Yapfast doesn't use custom cursors"
                    ));
                }
            }
        }
        let shared = Value::Object(
            top.iter()
                .filter(|(key, _)| LAYER_KEYS.contains(&key.as_str()))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        );
        let short = |key: &str| -> Result<Option<String>, Error> {
            let text = string(top, key)?;
            if let Some(text) = &text {
                if text.chars().count() > MAX_SHORT_FIELD {
                    bail!("\"{key}\" is at most {MAX_SHORT_FIELD} characters");
                }
                plain(key, text)?;
            }
            Ok(text)
        };
        let (license, source, based_on) = (short("license")?, short("source")?, short("based_on")?);
        let package = Package {
            id,
            name,
            version,
            description,
            authors,
            license,
            source,
            based_on,
            min_yapfast: string(top, "min_yapfast")?,
            preview,
            icon,
            dark,
            light,
            shared,
            notes,
            options,
        };
        // Every variant is resolved once here, so a value out of its range is
        // refused when the theme is read, not when someone switches modes.
        let mut notes = Vec::new();
        // Yapfast's own options, which a theme doesn't declare: ransom names
        // when it cuts them, its sounds' volume when it ships sounds, light
        // or dark when it ships both. One the theme declares under the same
        // id stands instead.
        let (mut ransom, mut sounds) = (false, false);
        for mode in package.modes() {
            let resolved =
                package.resolve_noting(mode, &super::options::Choices::new(), &mut notes)?;
            ransom |= resolved.layout.names.is_some();
            sounds |= !resolved.sounds.is_empty();
        }
        let mut package = package;
        let both = package.dark.is_some() && package.light.is_some();
        for (wanted, made) in [
            (ransom, super::options::ransom_option as fn() -> _),
            (sounds, super::options::sounds_option),
            (both, super::options::mode_option),
        ] {
            let option = made();
            if wanted && !package.options.iter().any(|known| known.id == option.id) {
                package.options.push(option);
            }
        }
        for note in notes {
            if !package.notes.contains(&note) {
                package.notes.push(note);
            }
        }
        // Notes quote the theme's own names: shown as Yapfast's words only
        // once they're on one line and short.
        for note in &mut package.notes {
            *note = super::shown(note);
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

    /// One variant with every layer applied over its base, its options at
    /// their defaults.
    pub fn resolve(&self, mode: Mode) -> Result<Resolved, Error> {
        self.resolve_with(mode, &super::options::Choices::new())
    }

    /// One variant with every layer applied over its base and the person's
    /// `choices` for its options applied to its layers first.
    pub fn resolve_with(
        &self,
        mode: Mode,
        choices: &super::options::Choices,
    ) -> Result<Resolved, Error> {
        self.resolve_noting(mode, choices, &mut Vec::new())
    }

    /// [`Self::resolve_with`], also giving what it ignored: `theme check`
    /// refuses an option whose fields this Yapfast doesn't read.
    pub fn resolve_with_notes(
        &self,
        mode: Mode,
        choices: &super::options::Choices,
        notes: &mut Vec<String>,
    ) -> Result<Resolved, Error> {
        self.resolve_noting(mode, choices, notes)
    }

    fn resolve_noting(
        &self,
        mode: Mode,
        choices: &super::options::Choices,
        notes: &mut Vec<String>,
    ) -> Result<Resolved, Error> {
        let variant = match mode {
            Mode::Dark => self.dark.as_ref(),
            Mode::Light => self.light.as_ref(),
        }
        .ok_or_else(|| Error(format!("it has no {} variant", mode.name())))?;
        // Options rewrite the theme's own JSON, so what they set is read and
        // checked as the theme's own values are. A theme without options
        // reads its JSON as it is.
        let overrides = super::options::overrides(&self.options, choices);
        let changed;
        let (shared, variant) = if overrides.is_empty() {
            (&self.shared, variant)
        } else {
            let mut shared = self.shared.clone();
            let mut variant = variant.clone();
            super::options::apply(
                &overrides,
                &mut shared,
                &mut [(mode.name(), Some(&mut variant))],
            )?;
            changed = (shared, variant);
            (&changed.0, &changed.1)
        };
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
            if RETIRED_KEYS.contains(&key.as_str()) {
                notes.push(retired_note(&at(key)));
            } else if key != "base" && !LAYER_KEYS.contains(&key.as_str()) {
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
        let Value::Object(shared) = shared else {
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
            if let Some(content) = layers.get("content") {
                super::content::read_content(
                    content,
                    &place("content"),
                    &mut resolved.content,
                    notes,
                )?;
            }
            if let Some(chrome) = layers.get("chrome") {
                super::chrome::read_chrome(chrome, &place("chrome"), &mut resolved.chrome, notes)?;
            }
            if let Some(parts) = layers.get("parts") {
                super::parts::read_parts(parts, &place("parts"), &mut resolved.parts, notes)?;
            }
        }
        super::lists::check_with(&resolved, [shared, variant], notes)?;
        Ok(resolved)
    }

    /// Every file the theme names, relative to its folder, once each: what an
    /// install copies and what [`super::check`] reads. A file only an option
    /// names counts too.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self.preview.clone();
        files.extend(self.icon.clone());
        let combinations = super::options::combinations(&self.options);
        for (mode, (_, choices)) in self.modes().into_iter().flat_map(|mode| {
            combinations
                .iter()
                .map(move |combination| (mode, combination))
        }) {
            if let Ok(resolved) = self.resolve_with(mode, choices) {
                let media = resolved
                    .sounds
                    .values()
                    .flat_map(|choice| choice.files.iter())
                    .chain(resolved.icons.values().map(|icon| &icon.file));
                for file in resolved
                    .fonts()
                    .into_iter()
                    .chain(media.map(PathBuf::as_path))
                {
                    if !files.iter().any(|known| known == file) {
                        files.push(file.to_owned());
                    }
                }
                for picture in resolved
                    .art
                    .files()
                    .into_iter()
                    .chain(resolved.layout.files())
                {
                    if !files.contains(&picture) {
                        files.push(picture);
                    }
                }
            }
        }
        files
    }
}

/// A name as the last part of a theme id and its folder's name: lowercase
/// letters and digits joined by `-`, `my-theme` when nothing is left.
pub fn slug(name: &str) -> String {
    let slug = name
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
    if slug.is_empty() {
        "my-theme".to_owned()
    } else {
        slug
    }
}

/// The licence file an exported or new theme starts with, until its author
/// writes their own.
pub const LICENCE_PLACEHOLDER: &str = "\
This theme's licence goes here. theme.json names CC0-1.0, which gives the
theme to the public domain (https://creativecommons.org/publicdomain/zero/1.0/);
change both if you choose another, and list every font, picture and sound you
add with its own licence.
";

/// A theme.json for one of the person's own colour themes, so it can be
/// shared as a package: every colour it has, over the built-in theme it
/// began as. `colours` are by their name in [`COLOURS`], as ARGB.
pub fn export(name: &str, author: &str, base: &str, dark: bool, colours: &[(&str, u32)]) -> String {
    let colours: serde_json::Map<String, Value> = colours
        .iter()
        .filter(|(token, _)| !LOCKED_COLOURS.contains(token))
        .map(|(token, colour)| ((*token).to_owned(), Value::String(hex(*colour))))
        .collect();
    let mode = if dark { "dark" } else { "light" };
    let author = author.trim();
    let value = serde_json::json!({
        "theme_api": THEME_API,
        "id": format!("local.{}", slug(name)),
        "name": name,
        "version": "1.0.0",
        "description": format!("{name}, a colour theme made in Yapfast."),
        "authors": if author.is_empty() { Vec::new() } else { vec![author] },
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
    slug(name)
}

/// Fields of an exported theme that are its own, never taken from the
/// custom theme's folder: [`export_with`].
const OWN_FIELDS: [&str; 6] = ["theme_api", "id", "name", "version", "authors", "source"];

/// [`export`], with the layers of a custom theme's own `theme.json` beside
/// its colours: its type, art, sounds and every other layer, its variant's
/// layers beside the colours (the custom theme's colours win), and its
/// `license` and `description` when it names them. The files those layers
/// name come along in [`write_export`].
pub fn export_with(
    name: &str,
    author: &str,
    base: &str,
    dark: bool,
    colours: &[(&str, u32)],
    layers: Option<&str>,
) -> Result<String, String> {
    let text = export(name, author, base, dark, colours);
    let Some(layers) = layers else {
        return Ok(text);
    };
    let mut value: Value = serde_json::from_str(&text).expect("just written");
    let layers: Value = serde_json::from_str(layers)
        .map_err(|error| format!("the theme's {MANIFEST} doesn't read: {error}"))?;
    let Value::Object(layers) = layers else {
        return Err(format!("the theme's {MANIFEST} isn't an object"));
    };
    let mode = if dark { "dark" } else { "light" };
    let top = value.as_object_mut().expect("written as an object");
    for (key, layer) in layers {
        match key.as_str() {
            key if OWN_FIELDS.contains(&key) => {}
            "description" | "license"
                if layer.as_str().is_none_or(|text| text.trim().is_empty()) => {}
            "variants" => {
                let Some(Value::Object(own)) = layer.get(mode).cloned() else {
                    continue;
                };
                let variant = top["variants"][mode]
                    .as_object_mut()
                    .expect("written as an object");
                for (key, layer) in own {
                    match key.as_str() {
                        "base" => {}
                        "colours" => {
                            let Value::Object(mut merged) = layer else {
                                continue;
                            };
                            if let Some(Value::Object(mine)) = variant.get("colours") {
                                merged.extend(mine.clone());
                            }
                            variant.insert(key, Value::Object(merged));
                        }
                        _ => {
                            variant.insert(key, layer);
                        }
                    }
                }
            }
            _ => {
                top.insert(key, layer);
            }
        }
    }
    let mut text = serde_json::to_string_pretty(&value).expect("JSON values serialise");
    text.push('\n');
    Package::parse(&text).map_err(|error| error.0)?;
    Ok(text)
}

/// Whether a file in a theme's folder is a licence or credit for what it
/// ships, which an export carries wherever it lies, as `fonts/OFL.txt`.
fn licence_like(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_uppercase())
        .unwrap_or_default();
    [
        "LICENSE", "LICENCE", "OFL", "COPYING", "CREDITS", "AUTHORS", "NOTICE",
    ]
    .iter()
    .any(|word| name.starts_with(word))
}

/// Writes an exported theme into `folder`, made if it isn't there: its
/// `theme.json`, every file it names, copied from `source` (the custom
/// theme's own folder), the licences and credits there, and a licence to
/// fill in when `source` has none at its top. Returns the files written, by
/// their path in the folder.
pub fn write_export(
    folder: &Path,
    theme_json: &str,
    source: Option<&Path>,
) -> Result<Vec<PathBuf>, String> {
    let io = |path: &Path, error: std::io::Error| format!("{}: {error}", path.display());
    let package = Package::parse(theme_json).map_err(|error| error.0)?;
    let mut copies: Vec<PathBuf> = package.files();
    if let Some(source) = source {
        let mut folders = vec![source.to_path_buf()];
        while let Some(at) = folders.pop() {
            let Ok(entries) = std::fs::read_dir(&at) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    folders.push(path);
                } else if kind.is_file() && licence_like(&path) {
                    let relative = path.strip_prefix(source).expect("walked from it");
                    if !copies.iter().any(|copy| copy == relative) {
                        copies.push(relative.to_path_buf());
                    }
                }
            }
        }
    }
    std::fs::create_dir_all(folder).map_err(|error| io(folder, error))?;
    std::fs::write(folder.join(MANIFEST), theme_json)
        .map_err(|error| io(&folder.join(MANIFEST), error))?;
    let mut written = vec![PathBuf::from(MANIFEST)];
    let root = source.and_then(|source| std::fs::canonicalize(source).ok());
    for relative in copies {
        // Only files inside the folder, as a theme's paths always are.
        if relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(format!("{} isn't inside the theme", relative.display()));
        }
        let from = source
            .map(|source| source.join(&relative))
            .filter(|from| {
                std::fs::symlink_metadata(from).is_ok_and(|meta| meta.is_file())
                    && super::check::within(root.as_deref(), from)
            })
            .ok_or_else(|| {
                format!(
                    "the theme names {}, which isn't in its folder",
                    relative.display()
                )
            })?;
        let to = folder.join(&relative);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|error| io(parent, error))?;
        }
        std::fs::copy(&from, &to).map_err(|error| io(&from, error))?;
        written.push(relative);
    }
    if !written.iter().any(|path| {
        super::check::LICENCE_FILES
            .iter()
            .any(|name| path == Path::new(name))
    }) {
        std::fs::write(folder.join("LICENSE"), LICENCE_PLACEHOLDER)
            .map_err(|error| io(&folder.join("LICENSE"), error))?;
        written.push(PathBuf::from("LICENSE"));
    }
    Ok(written)
}

/// A text field of an object, if it's there.
fn string(object: &serde_json::Map<String, Value>, key: &str) -> Result<Option<String>, Error> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(Error(format!("\"{key}\" has to be text"))),
    }
}

/// Reverse-DNS: lowercase letters, digits, `-` and `_` in at
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
    for part in path.components() {
        if let Component::Normal(part) = part
            && let Some(problem) = unportable(&part.to_string_lossy())
        {
            bail!("\"{field}\" names \"{text}\": {problem}");
        }
    }
    Ok(path
        .components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .collect())
}

/// Why a file or folder name can't be written on every system Yapfast runs
/// on, if it can't: Windows refuses some characters, its device names and a
/// name that ends in a dot or a space.
fn unportable(name: &str) -> Option<&'static str> {
    if name
        .chars()
        .any(|c| c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Some("a file name can't hold : < > \" | ? * or control characters");
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Some("a file name can't end in a dot or a space");
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end()
        .to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    device.then_some("Windows keeps that name for a device")
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

/// A colour, or a gradient written `linear(135deg, #ff71ce, #01cdfe)`.
pub(crate) fn paint_at(value: &Value, place: &str) -> Result<Paint, Error> {
    let text = value
        .as_str()
        .ok_or_else(|| Error(format!("\"{place}\" has to be a colour such as #1e1f24")))?;
    if text.trim_start().starts_with("linear(") {
        parse_linear(text).ok_or_else(|| {
            Error(format!(
                "\"{place}\" isn't a gradient Yapfast reads: write linear(135deg, #ff71ce, #01cdfe)"
            ))
        })
    } else {
        colour_at(value, place).map(Paint::Colour)
    }
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
                "inset" if surface == Surface::Row => {
                    shape.inset = Some(match value {
                        Value::Array(items) if items.len() == 2 => (
                            number(&items[0], &format!("{here}[0]"), (0.0, 32.0))?,
                            number(&items[1], &format!("{here}[1]"), (0.0, 8.0))?,
                        ),
                        _ => (number(value, &here, (0.0, 32.0))?, 0.0),
                    });
                }
                "inset_top" if surface == Surface::Card => {
                    shape.inset_top = Some(number(value, &here, (0.0, 64.0))?);
                }
                "ground" | "bar"
                    if surface.framed() && (key == "ground" || surface == Surface::CardSection) =>
                {
                    let paint = read_paint(value, &here)?;
                    if key == "ground" {
                        shape.ground = Some(paint);
                    } else {
                        shape.bar = Some(paint);
                    }
                }
                "text" if surface.framed() => shape.text = Some(colour_at(value, &here)?),
                "bar_text" if surface == Surface::CardSection => {
                    shape.bar_text = Some(colour_at(value, &here)?);
                }
                "stripe" if surface == Surface::Row => {
                    shape.stripe = Some(read_stripe(value, &here)?);
                }
                "case" if surface.framed() => {
                    shape.case = Some(match value.as_str() {
                        Some("lower") => 1,
                        Some("upper") => 2,
                        Some("as written") => 0,
                        _ => bail!("\"{here}\" is lower, upper or \"as written\""),
                    });
                }
                "width" if surface == Surface::Scrollbar => {
                    shape.width = Some(number(value, &here, (1.0, 8.0))?);
                }
                "hover_width" if surface == Surface::Scrollbar => {
                    shape.hover_width = Some(number(value, &here, (2.0, 10.0))?);
                }
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

/// A colour, or a gradient as `colours` writes one.
fn read_paint(value: &Value, here: &str) -> Result<Paint, Error> {
    let text = value
        .as_str()
        .ok_or_else(|| Error(format!("\"{here}\" has to be a colour such as #1e1f24")))?;
    if text.trim_start().starts_with("linear(") {
        return parse_linear(text).ok_or_else(|| {
            Error(format!(
                "\"{here}\" isn't a gradient Yapfast reads: write linear(135deg, #ff71ce, #01cdfe)"
            ))
        });
    }
    parse_colour(text).map(Paint::Colour).ok_or_else(|| {
        Error(format!(
            "\"{here}\" is \"{text}\", which isn't a colour such as #1e1f24"
        ))
    })
}

/// `"stripe": "#241521"`, or `{ "colour": "#241521", "lists": ["channels",
/// "members", "messages"] }` (all three when `lists` is left out).
fn read_stripe(value: &Value, here: &str) -> Result<Stripe, Error> {
    if value.is_string() {
        return Ok(Stripe {
            colour: colour_at(value, here)?,
            channels: true,
            members: true,
            messages: true,
        });
    }
    let fields = object(value, here)?;
    let colour = colour_at(
        fields
            .get("colour")
            .ok_or_else(|| Error(format!("\"{here}\" needs a \"colour\"")))?,
        &format!("{here}.colour"),
    )?;
    let mut stripe = Stripe {
        colour,
        channels: true,
        members: true,
        messages: true,
    };
    if let Some(lists) = fields.get("lists") {
        let at = format!("{here}.lists");
        let items = lists.as_array().ok_or_else(|| {
            Error(format!(
                "\"{at}\" is a list of channels, members and messages"
            ))
        })?;
        stripe.channels = false;
        stripe.members = false;
        stripe.messages = false;
        for item in items {
            match item.as_str() {
                Some("channels") => stripe.channels = true,
                Some("members") => stripe.members = true,
                Some("messages") => stripe.messages = true,
                _ => bail!("\"{at}\" lists channels, members or messages"),
            }
        }
    }
    Ok(stripe)
}

pub(super) fn read_shadow(value: &Value, here: &str) -> Result<Shadow, Error> {
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
            // Popups take a named curve: a cubic-bezier picks the nearest
            // one.
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
            "unfocused" => {
                motion.unfocused = Some(named(value, &here, &Unfocused::ALL, Unfocused::name)?);
            }
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

    fn with(key: &str, value: Value) -> Result<Package, Error> {
        let mut top: Value = serde_json::from_str(SMALL).unwrap();
        top[key] = value;
        Package::parse(&top.to_string())
    }

    #[test]
    fn shown_fields_refuse_turned_split_or_hidden_text() {
        for (key, value) in [
            ("name", Value::from("Yap\u{202e}fast")),
            ("name", Value::from("Two\nlines")),
            ("description", Value::from("Looks safe\u{200b}")),
            ("authors", serde_json::json!(["Ada", "Yap\u{2066}fast"])),
            ("license", Value::from("MIT\u{202d}")),
            ("based_on", Value::from("https://github.com/a/b\u{feff}")),
            ("source", Value::from("https://github.com/a/b\r")),
        ] {
            let error = with(key, value).unwrap_err();
            assert!(error.0.contains(key), "{key}: {error}");
            assert!(error.0.contains("U+"), "{key}: {error}");
        }
        // The joiner emoji are built with still reads.
        assert!(with("name", Value::from("Plum \u{1f469}\u{200d}\u{1f4bb}")).is_ok());
    }

    #[test]
    fn authors_are_capped_in_number_and_length() {
        let many: Vec<String> = (0..=MAX_AUTHORS).map(|n| format!("Ada {n}")).collect();
        assert!(with("authors", serde_json::json!(many)).is_err());
        assert!(with("authors", serde_json::json!(["a".repeat(200_000)])).is_err());
        assert!(with("authors", serde_json::json!([" "])).is_err());
        let fine = with("authors", serde_json::json!(["Ada", "Grace"])).unwrap();
        assert_eq!(fine.authors, ["Ada", "Grace"]);
        assert!(with("license", Value::from("x".repeat(MAX_SHORT_FIELD + 1))).is_err());
    }

    #[test]
    fn file_names_windows_cant_hold_are_refused() {
        for name in [
            "art/sky.png:hidden",
            "art/NUL.png",
            "fonts/con",
            "art/com1.txt.png",
            "art/sky.png.",
            "art/sky /a.png",
            "art/what?.png",
        ] {
            assert!(relative(name, "preview").is_err(), "{name}");
        }
        for name in [
            "art/sky.png",
            "art/console.png",
            "art/com10.png",
            "art/nul-ish.png",
        ] {
            assert!(relative(name, "preview").is_ok(), "{name}");
        }
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
    fn shapes_the_screens_beyond_the_conversation() {
        let package = Package::parse(
            r##"{
            "theme_api": 1, "id": "com.example.reach", "name": "Reach", "version": "1.0",
            "shape": { "call": { "radius": 4 }, "stream": { "border": 2 }, "toast": { "radius": 999 },
                       "picker": { "radius": 18 }, "popout": { "radius": 6 },
                       "scrollbar": { "radius": 1, "width": 4, "hover_width": 9 },
                       "toast_bar": { "radius": 1 }, "call_x": { "width": 3 } },
            "variants": { "dark": { "base": "yap" } }
        }"##,
        )
        .unwrap();
        let dark = package.resolve(Mode::Dark).unwrap();
        assert_eq!(dark.shapes[&Surface::Call].radius, Some(4.0));
        assert_eq!(dark.shapes[&Surface::Stream].border, Some(2.0));
        assert_eq!(dark.shapes[&Surface::Toast].radius, Some(999.0));
        assert_eq!(dark.shapes[&Surface::Picker].radius, Some(18.0));
        assert_eq!(dark.shapes[&Surface::Popout].radius, Some(6.0));
        let scrollbar = &dark.shapes[&Surface::Scrollbar];
        assert_eq!(
            (scrollbar.radius, scrollbar.width, scrollbar.hover_width),
            (Some(1.0), Some(4.0), Some(9.0))
        );
        assert_eq!(package.notes.len(), 2, "{:?}", package.notes);
        // Only scrollbars take a width, and only within their range.
        let other = Package::parse(&SMALL.replace(
            r#""variants""#,
            r#""shape": { "call": { "width": 3 } }, "variants""#,
        ))
        .unwrap();
        assert!(
            other
                .notes
                .iter()
                .any(|note| note.contains("shape.call.width")),
            "{:?}",
            other.notes
        );
        assert!(
            Package::parse(&SMALL.replace(
                r#""variants""#,
                r#""shape": { "scrollbar": { "width": 20 } }, "variants""#,
            ))
            .is_err()
        );
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
            "parts.message-row\" isn't a part Yapfast has",
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
            "Ada",
            "yap",
            true,
            &[("chat", 0xff2a_1a3a), ("danger", 0xff00_ff00)],
        );
        let package = Package::parse(&text).unwrap();
        assert_eq!(package.id, "local.plum-night");
        assert_eq!(package.name, "Plum Night!");
        assert_eq!(package.authors, ["Ada"]);
        assert_eq!(package.version, "1.0.0");
        assert_eq!(package.license.as_deref(), Some("CC0-1.0"));
        let resolved = package.resolve(Mode::Dark).unwrap();
        assert_eq!(resolved.paint("chat"), Some(&Paint::Colour(0xff2a_1a3a)));
        assert!(
            resolved.paint("danger").is_none(),
            "the danger colours stay Yapfast's"
        );
        assert_eq!(folder_name("Plum Night!"), "plum-night");
    }
}
