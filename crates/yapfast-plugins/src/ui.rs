//! What plugins change in Yapfast's interface, as data: the `ui` section of
//! a manifest and the rows it adds to menus, checked against what Yapfast
//! lets a plugin name, and the widget trees a plugin draws in its slots.
//!
//! A plugin can only name a change Yapfast lists here. Each list is closed,
//! so whatever isn't on one, such as a permission dialog, the composer's send
//! button or a call's mute, has no name to address. The enabled plugins'
//! changes come together in one [`UiOverrides`], which is `None` while no
//! plugin changes anything, so the interface tests one `Option` where a
//! change could apply. See `docs/plugins/interface.md`.

use std::{collections::BTreeMap, sync::Arc};

use serde::Deserialize;

use yapfast_plugin_api::manifest::{self, PartDecl};

use super::{Manifest, slots::Slot};

/// The most characters a label, a row's text or a plugin's name can have.
pub const LABEL_CHARS: usize = 64;

/// The most changes to built-in UI one plugin makes, and the most rows it
/// adds to menus.
pub const CHANGES: usize = 64;

/// The most rows and cells together in one widget tree, unless its slot
/// allows more ([`Limits`]); a list's item and a table's cell count as one.
pub const TREE_NODES: usize = 64;

/// The most rows and cells the larger surfaces take: a banner, a dialog, a
/// profile section, a picker's tab and a plugin's settings page.
pub const PAGE_NODES: usize = 256;

/// How deep containers nest: a section inside a tab inside a section, and no
/// further. The tree's own rows are the first level.
pub const TREE_DEPTH: usize = 4;

/// The most tabs one `tabs` cell has.
pub const TABS: usize = 8;

/// The most columns a grid lays its cells in, and a table has.
pub const COLUMNS: usize = 8;

/// The icons a button may name, which Yapfast draws from its own set so a
/// plugin's button looks like the ones beside it.
pub const ICONS: [&str; 16] = [
    "previous",
    "play",
    "pause",
    "next",
    "stop",
    "volume",
    "volume-off",
    "music",
    "star",
    "bell",
    "pin",
    "link",
    "download",
    "refresh",
    "search",
    "settings",
];

/// The most characters a tooltip says.
pub const TOOLTIP_CHARS: usize = 256;

/// The most bytes of text in one cell; longer text is cut.
pub const NODE_TEXT: usize = 4096;

/// The capability a manifest declares to use its `ui` section.
pub const MODIFY: &str = "ui.modify";

/// The menus a plugin can name: a closed list, of which Yapfast applies
/// changes to those it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Message,
    User,
    UserProfile,
    Channel,
    Thread,
    GroupDm,
    Server,
    ServerHeader,
    Role,
    Emoji,
    Sticker,
    Image,
    Stream,
    Game,
    ForumTag,
    Composer,
    Attach,
}

/// When a menu row applies: each field given must match, and an empty one
/// always does. The fields are a closed set, not an expression language.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    #[serde(default)]
    pub network: Option<NetworkName>,
    /// The menu is about this account, or its own message.
    #[serde(default)]
    pub own: Option<bool>,
    /// The menu is about a one-to-one conversation, or someone in one.
    #[serde(default)]
    pub dm: Option<bool>,
    #[serde(default)]
    pub has_attachment: Option<bool>,
    #[serde(default)]
    pub is_bot: Option<bool>,
    /// The person the menu is about has the role with this id.
    #[serde(default)]
    pub role: Option<String>,
}

/// A network as a manifest names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkName {
    #[default]
    Matrix,
    Discord,
}

/// What a menu is about, for [`When`] to test.
#[derive(Clone, Copy, Debug, Default)]
pub struct Subject<'a> {
    pub network: NetworkName,
    pub own: bool,
    pub dm: bool,
    pub has_attachment: bool,
    pub is_bot: bool,
    pub roles: &'a [String],
}

impl When {
    pub fn matches(&self, subject: &Subject<'_>) -> bool {
        let network = self
            .network
            .is_none_or(|network| subject.network == network);
        network
            && self.own.is_none_or(|own| own == subject.own)
            && self.dm.is_none_or(|dm| dm == subject.dm)
            && self
                .has_attachment
                .is_none_or(|has| has == subject.has_attachment)
            && self.is_bot.is_none_or(|bot| bot == subject.is_bot)
            && self
                .role
                .as_ref()
                .is_none_or(|role| subject.roles.contains(role))
    }
}

/// A part of the interface a plugin can hide or replace, by its ID. Each
/// was checked by hand: its neighbours close the gap it leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Part {
    /// The banner at the top of the channel list.
    SidebarBanner,
    /// Who's typing, above the composer.
    TimelineTyping,
    /// The message hover bar's buttons.
    MessageReply,
    MessageReact,
    MessageMore,
    /// The name and status beside the account's avatar, not its microphone
    /// and deafen buttons.
    AccountIdentity,
    /// The welcome block at the start of a channel's history.
    TimelineIntro,
}

impl Part {
    pub const ALL: [Self; 7] = [
        Self::SidebarBanner,
        Self::TimelineTyping,
        Self::MessageReply,
        Self::MessageReact,
        Self::MessageMore,
        Self::AccountIdentity,
        Self::TimelineIntro,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::SidebarBanner => "sidebar.banner",
            Self::TimelineTyping => "timeline.typing",
            Self::MessageReply => "message.actions.reply",
            Self::MessageReact => "message.actions.react",
            Self::MessageMore => "message.actions.more",
            Self::AccountIdentity => "account.identity",
            Self::TimelineIntro => "timeline.intro",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|part| part.id() == id)
    }

    /// What the review dialog and a plugin's page call it.
    pub const fn words(self) -> &'static str {
        match self {
            Self::SidebarBanner => "the channel list's banner",
            Self::TimelineTyping => "the typing indicator",
            Self::MessageReply => "a message's Reply button",
            Self::MessageReact => "a message's Add Reaction button",
            Self::MessageMore => "a message's More button",
            Self::AccountIdentity => "your name in the account panel",
            Self::TimelineIntro => "a channel's welcome",
        }
    }

    pub const fn hideable(self) -> bool {
        !matches!(self, Self::AccountIdentity | Self::TimelineIntro)
    }

    pub const fn replaceable(self) -> bool {
        matches!(
            self,
            Self::SidebarBanner | Self::AccountIdentity | Self::TimelineIntro
        )
    }

    const fn bit(self) -> u16 {
        1 << self as u16
    }
}

/// Names a plugin might try for parts no plugin can touch, refused as such
/// rather than as parts that no longer exist.
const PROTECTED_PARTS: [&str; 12] = [
    "account.mute",
    "account.deafen",
    "account.logout",
    "call.",
    "voice.",
    "composer.input",
    "composer.send",
    "plugins.",
    "dialog.",
    "login.",
    "security.",
    "link.confirm",
];

fn protected_part(id: &str) -> bool {
    PROTECTED_PARTS.iter().any(|name| {
        id == name.trim_end_matches('.') || (name.ends_with('.') && id.starts_with(name))
    })
}

/// The built-in menu rows a plugin can hide, relabel or move, by the id
/// `src/ui/menus.rs` gives each. A new built-in row is protected until it's
/// added here. No row drawn in the danger tone is here, nor any that shows
/// or controls a call, nor the friend and block rows.
pub const EDITABLE_ROWS: [&str; 41] = [
    "mark-read",
    "join-voice",
    "open-chat",
    "profile",
    "message",
    "pin",
    "unpin",
    "call",
    "add-note",
    "add-nickname",
    "edit-nickname",
    "close-dm",
    "invite-to-server",
    "ignore",
    "unignore",
    "mute",
    "unmute",
    "copy-link",
    "copy-id",
    "mention",
    "server-nickname",
    "roles",
    "channel-mute",
    "channel-notifications",
    "mute-server",
    "unmute-server",
    "server-notifications",
    "hide-muted",
    "show-all-channels",
    "invite",
    "pin-channel",
    "unpin-channel",
    "add-to-channel-list",
    "remove-from-channel-list",
    "unfollow-category",
    "join-thread",
    "leave-thread",
    "create-channel",
    "create-category",
    "edit-channel",
    "server-settings",
];

/// The built-in rows drawn in the danger tone, which can't be hidden,
/// relabelled or moved; `src/ui/menus.rs` checks this list against its own.
pub const DANGER_ROWS: [&str; 12] = [
    "unfriend",
    "block",
    "timeout",
    "kick",
    "ban",
    "server-mute",
    "server-deafen",
    "disconnect",
    "delete-channel",
    "leave-server",
    "leave-group",
    "transfer-ownership",
];

/// The colours a plugin can set, as `ui/theme.slint` names them;
/// `src/ui/theme.rs` checks this list against its own.
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

/// Yapfast's warnings are drawn in these, so no plugin sets them.
pub const LOCKED_COLOURS: [&str; 3] = ["danger", "danger-hover", "on-danger"];

/// The surfaces most text is drawn on.
const SURFACES: [&str; 5] = ["sidebar", "panel", "chat", "raised", "field"];

/// Each text colour and the surfaces it's drawn on, which it must stay
/// readable against.
pub const TEXT_ON: [(&str, &[&str]); 6] = [
    ("text", &SURFACES),
    ("secondary", &SURFACES),
    ("muted", &SURFACES),
    ("faint", &SURFACES),
    ("link", &SURFACES),
    ("on-accent", &["accent"]),
];

/// The contrast WCAG asks of body text.
pub const READABLE: f64 = 4.5;

/// A colour written `#rgb`, `#rrggbb` or `#rrggbbaa`, as ARGB.
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

/// How much two colours contrast, from 1 to 21, as WCAG measures it; a
/// translucent foreground is laid over the background first.
pub fn contrast(foreground: u32, background: u32) -> f64 {
    let channel = |colour: u32, shift: u32| f64::from((colour >> shift) & 0xff) / 255.0;
    let alpha = f64::from(foreground >> 24) / 255.0;
    let luminance = |rgb: [f64; 3]| {
        let linear = |value: f64| {
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
    };
    let back = [16, 8, 0].map(|shift| channel(background, shift));
    let front = [16, 8, 0]
        .map(|shift| channel(foreground, shift))
        .into_iter()
        .zip(back)
        .map(|(front, back)| front * alpha + back * (1.0 - alpha))
        .collect::<Vec<_>>();
    let (one, two) = (luminance([front[0], front[1], front[2]]), luminance(back));
    (one.max(two) + 0.05) / (one.min(two) + 0.05)
}

/// The colours of an overlay that would leave text hard to read, each with
/// why: a text colour, or a surface under text, that brings a pair under
/// [`READABLE`]. A theme whose own pair is already lower than that only has
/// to keep its own contrast. `theme` gives the unmodified theme's colour by
/// name.
pub fn unreadable(
    theme: impl Fn(&str) -> Option<u32>,
    overlay: &[(&'static str, u32)],
) -> Vec<(&'static str, String)> {
    let mut refused: Vec<(&'static str, String)> = Vec::new();
    loop {
        let shown = |name: &str| {
            overlay
                .iter()
                .find(|(token, _)| *token == name && !refused.iter().any(|(gone, _)| gone == token))
                .map(|&(_, colour)| colour)
                .or_else(|| theme(name))
        };
        let changed = |name: &str| {
            overlay.iter().any(|(token, _)| *token == name)
                && !refused.iter().any(|(gone, _)| *gone == name)
        };
        let failing = TEXT_ON.iter().find_map(|&(text, surfaces)| {
            surfaces.iter().find_map(|&surface| {
                if !changed(text) && !changed(surface) {
                    return None;
                }
                let (Some(front), Some(back)) = (shown(text), shown(surface)) else {
                    return None;
                };
                let own = theme(text)
                    .zip(theme(surface))
                    .map_or(READABLE, |(front, back)| contrast(front, back));
                let needed = READABLE.min(own) - 0.005;
                let ratio = contrast(front, back);
                (ratio < needed).then(|| {
                    let token = if changed(text) { text } else { surface };
                    let why = format!(
                        "{text} on {surface} would contrast {ratio:.1}:1, under {:.1}:1",
                        needed + 0.005
                    );
                    (token, why)
                })
            })
        });
        match failing {
            Some(refusal) => refused.push(refusal),
            None => return refused,
        }
    }
}

/// A size a plugin can set, in logical pixels, each clamped to its range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Size {
    TextSize,
    SidebarWidth,
    MembersWidth,
    ChannelRowHeight,
    MessageGap,
    MessageGroupGap,
}

impl Size {
    pub const ALL: [Self; 6] = [
        Self::TextSize,
        Self::SidebarWidth,
        Self::MembersWidth,
        Self::ChannelRowHeight,
        Self::MessageGap,
        Self::MessageGroupGap,
    ];

    /// Its name in a manifest, and in `ClientStyle` with `-plugin` after it.
    pub const fn id(self) -> &'static str {
        match self {
            Self::TextSize => "text-size",
            Self::SidebarWidth => "sidebar-width",
            Self::MembersWidth => "members-width",
            Self::ChannelRowHeight => "channel-row-height",
            Self::MessageGap => "message-gap",
            Self::MessageGroupGap => "message-group-gap",
        }
    }

    pub const fn range(self) -> (f32, f32) {
        match self {
            Self::TextSize => (12.0, 20.0),
            Self::SidebarWidth => (200.0, 420.0),
            Self::MembersWidth => (200.0, 360.0),
            Self::ChannelRowHeight => (24.0, 48.0),
            Self::MessageGap => (0.0, 12.0),
            Self::MessageGroupGap => (0.0, 32.0),
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|size| size.id() == id)
    }
}

/// A manifest's `ui` section.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ui {
    #[serde(default)]
    pub menu: Vec<RowEdit>,
    #[serde(default)]
    pub parts: Parts,
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
    #[serde(default)]
    pub sizes: BTreeMap<String, f64>,
}

/// A change to one built-in menu row.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowEdit {
    pub target: Target,
    pub row: String,
    #[serde(default)]
    pub hide: bool,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub when: When,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parts {
    #[serde(default)]
    pub hide: Vec<String>,
    #[serde(default)]
    pub replace: Vec<String>,
}

/// A row a plugin adds to a menu; choosing it calls the plugin's `action`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuItem {
    pub target: Target,
    pub label: String,
    /// The plugin's own picture for it. Not drawn yet: assets come with
    /// installing from git.
    #[serde(default)]
    pub icon: Option<String>,
    pub action: String,
    #[serde(default)]
    pub when: When,
}

/// A button a plugin adds where Yapfast already draws a row of its own, from
/// its manifest, so it shows without the plugin running: `rail_items` at the
/// bottom of the server rail, and `header_buttons` in a conversation's header.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Button {
    pub label: String,
    /// What choosing it tells the plugin, as `plugin/<index>/<action>`.
    pub action: String,
}

/// Where a moved row goes: before or after another row of its group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    Before(String),
    After(String),
}

/// A built-in row's change, checked.
#[derive(Clone, Debug, PartialEq)]
pub struct RowChange {
    pub target: Target,
    pub row: String,
    pub hide: bool,
    pub label: Option<String>,
    pub place: Option<Place>,
    pub when: When,
}

/// One checked change to built-in UI.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Row(RowChange),
    HidePart(Part),
    ReplacePart(Part),
    /// A colour by its token, as ARGB.
    Colour(&'static str, u32),
    Size(Size, f32),
}

/// What two changes collide on.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    Row(Target, String),
    Part(Part),
    Colour(&'static str),
    Size(Size),
}

impl Change {
    pub fn key(&self) -> Key {
        match self {
            Self::Row(row) => Key::Row(row.target, row.row.clone()),
            Self::HidePart(part) | Self::ReplacePart(part) => Key::Part(*part),
            Self::Colour(token, _) => Key::Colour(token),
            Self::Size(size, _) => Key::Size(*size),
        }
    }

    /// Two plugins making these changes want the same: both hide one part,
    /// or set one colour or size alike.
    fn agrees(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::HidePart(one), Self::HidePart(two)) => one == two,
            (Self::Colour(one, a), Self::Colour(two, b)) => one == two && a == b,
            (Self::Size(one, a), Self::Size(two, b)) => one == two && a == b,
            _ => false,
        }
    }

    /// The change in words, as the review dialog lists it.
    pub fn words(&self) -> String {
        match self {
            Self::Row(row) if row.hide => format!("Hides {} in a menu", row.row),
            Self::Row(row) => match &row.label {
                Some(label) => format!("Renames {} in a menu to “{label}”", row.row),
                None => format!("Moves {} in a menu", row.row),
            },
            Self::HidePart(part) => format!("Hides: {}", part.words()),
            Self::ReplacePart(part) => format!("Replaces: {}", part.words()),
            Self::Colour(token, _) => format!("Recolours the interface ({token})"),
            Self::Size(size, value) => format!("Sets {} to {value}", size.id()),
        }
    }
}

/// A row a plugin adds, checked.
#[derive(Clone, Debug, PartialEq)]
pub struct AddedRow {
    pub target: Target,
    pub label: String,
    pub action: String,
    pub when: When,
}

/// Something in a manifest Yapfast won't apply, and why, as the plugin's
/// page says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub what: String,
    pub why: String,
}

/// A plugin's interface changes after checking: what applies, and what
/// was refused.
#[derive(Clone, Debug, Default)]
pub struct Checked {
    pub id: String,
    pub name: String,
    pub changes: Vec<Change>,
    pub rows: Vec<AddedRow>,
    /// The buttons it adds to the server rail's bottom, checked.
    pub rail: Vec<Button>,
    /// The buttons it adds to a conversation's header, checked.
    pub header: Vec<Button>,
    pub refused: Vec<Refusal>,
}

/// A label as it can show: plain text on one line, 1 to [`LABEL_CHARS`]
/// characters.
fn label(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("it's empty".into());
    }
    if text.chars().count() > LABEL_CHARS {
        return Err(format!("it's longer than {LABEL_CHARS} characters"));
    }
    yapfast_plugin_api::text::check(text)?;
    Ok(text.to_owned())
}

/// An action's or widget's id: letters, digits, `-`, `_`, `.` and `:`, as it
/// travels inside `plugin/<index>/<action>`, so an id may name what it is
/// about (`vote:d:123`).
fn plain_id(id: &str) -> bool {
    (1..=LABEL_CHARS).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

/// Checks a plugin's interface changes and menu rows against what Yapfast
/// lets a plugin name.
pub fn check(manifest: &Manifest) -> Checked {
    let mut checked = Checked {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        ..Checked::default()
    };
    let mut refuse = |what: String, why: &str| {
        checked.refused.push(Refusal {
            what,
            why: why.to_owned(),
        });
    };
    let mut rows = Vec::new();
    for item in &manifest.menus {
        let what = format!("menu row “{}”", item.label);
        if rows.len() == CHANGES {
            refuse(what, "the plugin adds more than 64 menu rows");
            continue;
        }
        let text = match label(&item.label) {
            Ok(text) => text,
            Err(why) => {
                refuse(what, &format!("its label won't show: {why}"));
                continue;
            }
        };
        if !plain_id(&item.action) {
            refuse(what, "its action isn't a plain name");
            continue;
        }
        rows.push(AddedRow {
            target: item.target,
            label: text,
            action: item.action.clone(),
            when: item.when.clone(),
        });
    }
    let mut changes: Vec<Change> = Vec::new();
    let ui = manifest.ui.clone().unwrap_or_default();
    let asks = !ui.menu.is_empty()
        || !ui.parts.hide.is_empty()
        || !ui.parts.replace.is_empty()
        || !ui.colors.is_empty()
        || !ui.sizes.is_empty();
    if asks && !manifest.has(MODIFY) {
        refuse(
            "its interface changes".into(),
            "it doesn't declare the ui.modify capability",
        );
    }
    let allowed = asks && manifest.has(MODIFY);
    let mut add = |change: Change, what: String, refuse: &mut dyn FnMut(String, &str)| {
        if changes.len() == CHANGES {
            refuse(what, "the plugin makes more than 64 changes");
        } else if changes.iter().any(|other| other.key() == change.key()) {
            refuse(what, "the plugin changes it twice");
        } else {
            changes.push(change);
        }
    };
    for edit in ui.menu.iter().filter(|_| allowed) {
        let what = format!("menu row {}", edit.row);
        if DANGER_ROWS.contains(&edit.row.as_str()) {
            refuse(what, "it's drawn in the danger tone, so it can't change");
            continue;
        }
        if !EDITABLE_ROWS.contains(&edit.row.as_str()) {
            refuse(what, "this row can't be changed, or no longer exists");
            continue;
        }
        let label = match edit.label.as_deref().map(label).transpose() {
            Ok(label) => label,
            Err(why) => {
                refuse(what, &format!("its label won't show: {why}"));
                continue;
            }
        };
        let place = match (&edit.before, &edit.after) {
            (Some(_), Some(_)) => {
                refuse(what, "it names both before and after");
                continue;
            }
            (Some(anchor), None) => Some(Place::Before(anchor.clone())),
            (None, Some(anchor)) => Some(Place::After(anchor.clone())),
            (None, None) => None,
        };
        if let Some(Place::Before(anchor) | Place::After(anchor)) = &place
            && !EDITABLE_ROWS.contains(&anchor.as_str())
        {
            refuse(what, "it would move beside a row that can't change");
            continue;
        }
        if !edit.hide && label.is_none() && place.is_none() {
            refuse(what, "it changes nothing");
            continue;
        }
        let change = Change::Row(RowChange {
            target: edit.target,
            row: edit.row.clone(),
            hide: edit.hide,
            label,
            place,
            when: edit.when.clone(),
        });
        add(change, what, &mut refuse);
    }
    let parts = ui.parts.hide.iter().map(|id| (id, false));
    let replaced = ui.parts.replace.iter().map(|id| (id, true));
    for (id, replace) in parts.chain(replaced).filter(|_| allowed) {
        let what = format!("part {id}");
        if protected_part(id) {
            refuse(what, "no plugin can change it");
            continue;
        }
        let Some(part) = Part::from_id(id) else {
            refuse(what, "this part no longer exists");
            continue;
        };
        let change = if replace {
            if !part.replaceable() {
                refuse(what, "this part can't be replaced");
                continue;
            }
            if manifest.wasm.is_none() {
                refuse(what, "replacing a part needs the plugin's wasm to draw it");
                continue;
            }
            Change::ReplacePart(part)
        } else {
            if !part.hideable() {
                refuse(what, "this part can't be hidden");
                continue;
            }
            Change::HidePart(part)
        };
        add(change, what, &mut refuse);
    }
    for (name, value) in ui.colors.iter().filter(|_| allowed) {
        let what = format!("colour {name}");
        if LOCKED_COLOURS.contains(&name.as_str()) {
            refuse(what, "Yapfast's warnings use it, so no plugin can set it");
            continue;
        }
        let Some(token) = COLOURS.into_iter().find(|token| token == name) else {
            refuse(what, "Yapfast has no colour by this name");
            continue;
        };
        let Some(colour) = parse_colour(value) else {
            refuse(what, "its value isn't #rgb, #rrggbb or #rrggbbaa");
            continue;
        };
        if TEXT_ON.iter().any(|(text, _)| *text == token) && colour >> 24 != 0xff {
            refuse(what, "a text colour must be opaque");
            continue;
        }
        add(Change::Colour(token, colour), what, &mut refuse);
    }
    for (name, &value) in ui.sizes.iter().filter(|_| allowed) {
        let what = format!("size {name}");
        let Some(size) = Size::from_id(name) else {
            refuse(what, "Yapfast has no size by this name");
            continue;
        };
        if !value.is_finite() {
            refuse(what, "it isn't a number of pixels");
            continue;
        }
        let (least, most) = size.range();
        add(
            Change::Size(size, (value as f32).clamp(least, most)),
            what,
            &mut refuse,
        );
    }
    checked.changes = changes;
    checked.rows = rows;
    // Buttons the manifest puts where Yapfast already draws rows of its own.
    // They need no capability: they are the plugin's own surface, as a slot is.
    fn buttons(items: &[Button], what: &str, refused: &mut Vec<Refusal>) -> Vec<Button> {
        let mut checked = Vec::new();
        for item in items {
            let name = format!("{what} “{}”", item.label);
            if checked.len() == CHANGES {
                refused.push(Refusal {
                    what: name,
                    why: format!("the plugin adds more than {CHANGES} of them"),
                });
                continue;
            }
            let label = match label(&item.label) {
                Ok(label) => label,
                Err(why) => {
                    refused.push(Refusal {
                        what: name,
                        why: format!("its label won't show: {why}"),
                    });
                    continue;
                }
            };
            if !plain_id(&item.action) {
                refused.push(Refusal {
                    what: name,
                    why: "its action isn't a plain name".to_owned(),
                });
                continue;
            }
            checked.push(Button {
                label,
                action: item.action.clone(),
            });
        }
        checked
    }
    // A name for a slot this build doesn't have names nothing that can show.
    for part in &manifest.parts {
        if Slot::from_id(&part.slot).is_none() {
            checked.refused.push(Refusal {
                what: format!("part {}", part.name),
                why: format!("Yapfast has no slot called `{}`", part.slot),
            });
        }
    }
    checked.rail = buttons(&manifest.rail_items, "rail item", &mut checked.refused);
    checked.header = buttons(
        &manifest.header_buttons,
        "header button",
        &mut checked.refused,
    );
    checked
}

/// A plugin as the interface names it: in an added row's id, by its
/// index, and beside what it draws, by its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginName {
    pub id: String,
    pub name: String,
}

/// Every change the enabled plugins make to the interface, which
/// [`derive`] builds from their checked manifests. Each change carries the
/// index of its plugin in `plugins`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiOverrides {
    pub plugins: Vec<PluginName>,
    /// One bit per [`Part`] hidden.
    hidden: u16,
    pub replaced: Vec<(Part, usize)>,
    pub rows: Vec<(RowChange, usize)>,
    pub added: Vec<(AddedRow, usize)>,
    /// Buttons at the bottom of the server rail, and in a conversation's
    /// header, each with its plugin's index.
    pub rail: Vec<(Button, usize)>,
    pub header: Vec<(Button, usize)>,
    pub colours: Vec<(&'static str, u32, usize)>,
    pub sizes: Vec<(Size, f32, usize)>,
}

impl UiOverrides {
    pub fn hides(&self, part: Part) -> bool {
        self.hidden & part.bit() != 0
    }

    pub fn size(&self, size: Size) -> Option<f32> {
        self.sizes
            .iter()
            .find(|(each, _, _)| *each == size)
            .map(|&(_, value, _)| value)
    }

    /// The id an added row reports when chosen.
    pub fn row_id(plugin: usize, action: &str) -> String {
        format!("{ROW_PREFIX}{plugin}/{action}")
    }

    /// The plugin and action an added row's id names.
    pub fn chosen<'a>(&self, id: &'a str) -> Option<(&PluginName, &'a str)> {
        let (plugin, action) = id.strip_prefix(ROW_PREFIX)?.split_once('/')?;
        let plugin = self.plugins.get(plugin.parse::<usize>().ok()?)?;
        Some((plugin, action))
    }
}

/// What an added menu row's id starts with.
pub const ROW_PREFIX: &str = "plugin/";

/// Two plugins changing one thing differently: the one that keeps the
/// change, and the one that stays enabled without it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub key: Key,
    pub kept: String,
    pub lost: String,
}

/// The overrides of the plugins given, in order, and where they collide.
/// The first plugin keeps a change both make, unless `choices` names the
/// plugin the person chose for it. `None` when no plugin changes anything.
pub fn derive(
    plugins: &[Checked],
    choices: &BTreeMap<Key, String>,
) -> (Option<Arc<UiOverrides>>, Vec<Conflict>) {
    let mut overrides = UiOverrides::default();
    let mut conflicts = Vec::new();
    // Each key's change, and the plugin making it.
    let mut taken: BTreeMap<Key, (usize, &Change)> = BTreeMap::new();
    for (index, plugin) in plugins.iter().enumerate() {
        for change in &plugin.changes {
            let key = change.key();
            match taken.get(&key) {
                Some((_, other)) if other.agrees(change) => {}
                Some(&(holder, _)) => {
                    let chosen = choices.get(&key);
                    let wins = chosen == Some(&plugin.id);
                    let (kept, lost) = if wins {
                        taken.insert(key.clone(), (index, change));
                        (index, holder)
                    } else {
                        (holder, index)
                    };
                    conflicts.push(Conflict {
                        key,
                        kept: plugins[kept].id.clone(),
                        lost: plugins[lost].id.clone(),
                    });
                }
                None => {
                    taken.insert(key, (index, change));
                }
            }
        }
    }
    for (_, (index, change)) in taken {
        match change {
            Change::Row(row) => overrides.rows.push((row.clone(), index)),
            Change::HidePart(part) => overrides.hidden |= part.bit(),
            Change::ReplacePart(part) => overrides.replaced.push((*part, index)),
            Change::Colour(token, colour) => overrides.colours.push((token, *colour, index)),
            Change::Size(size, value) => overrides.sizes.push((*size, *value, index)),
        }
    }
    for (index, plugin) in plugins.iter().enumerate() {
        overrides
            .added
            .extend(plugin.rows.iter().map(|row| (row.clone(), index)));
        overrides
            .rail
            .extend(plugin.rail.iter().map(|row| (row.clone(), index)));
        overrides
            .header
            .extend(plugin.header.iter().map(|row| (row.clone(), index)));
    }
    let unused = overrides.hidden == 0
        && overrides.replaced.is_empty()
        && overrides.rows.is_empty()
        && overrides.added.is_empty()
        && overrides.rail.is_empty()
        && overrides.header.is_empty()
        && overrides.colours.is_empty()
        && overrides.sizes.is_empty();
    if unused {
        return (None, conflicts);
    }
    overrides.plugins = plugins
        .iter()
        .map(|plugin| PluginName {
            id: plugin.id.clone(),
            name: plugin.name.clone(),
        })
        .collect();
    (Some(Arc::new(overrides)), conflicts)
}

/// What a widget tree's cell is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Text,
    Heading,
    Muted,
    Code,
    Badge,
    Icon,
    Image,
    Button,
    Link,
    Toggle,
    Select,
    Input,
    Slider,
    Progress,
    Divider,
    Spacer,
    /// A drop-down of `options`, `value` the chosen one.
    Dropdown,
    /// A number field: `value`, between `min` and `max` when they're given,
    /// in steps of `step`.
    Number,
    /// A text field of several lines.
    Textarea,
    /// A colour, `#rrggbb`, chosen from swatches or typed.
    Color,
    /// Something is happening: a turning mark beside `text`.
    Spinner,
    /// `items`, each markdown, numbered when `ordered`.
    List,
    /// `header` over `table`'s rows, each cell markdown.
    Table,
    /// `tabs`, one shown at a time; which one the window keeps.
    Tabs,
    /// `text` over `rows`, folded open or shut by the person.
    Section,
    /// `cells` laid out in `columns`.
    Grid,
    /// A picture over its label in one card of a fixed width, a button when it
    /// has an `id`: a picker's choice, laid out in a grid that lines up.
    Tile,
    /// The plugin's manifest settings, drawn and saved as its page draws them,
    /// where this row is: only on the plugin's own settings page, and nothing
    /// anywhere else. `options` names the settings' keys it shows, every
    /// shown setting when empty.
    SettingsForm,
}

impl Kind {
    /// It reports what the person does with it, by its id.
    pub const fn interactive(self) -> bool {
        matches!(
            self,
            Self::Button
                | Self::Link
                | Self::Toggle
                | Self::Select
                | Self::Input
                | Self::Slider
                | Self::Dropdown
                | Self::Number
                | Self::Textarea
                | Self::Color
        )
    }

    /// The window keeps something of it by its id (an open tab, a folded
    /// section), so it needs one as a control does.
    pub const fn keeps_state(self) -> bool {
        matches!(self, Self::Tabs | Self::Section)
    }

    /// It holds other rows or cells.
    pub const fn container(self) -> bool {
        matches!(self, Self::Tabs | Self::Section | Self::Grid)
    }
}

/// What a slot lets a tree be: how many rows and cells, how deep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub nodes: usize,
    pub depth: usize,
}

impl Limits {
    /// A small surface's: a toast, a message's decoration, a row of buttons.
    pub const SMALL: Self = Self {
        nodes: TREE_NODES,
        depth: TREE_DEPTH,
    };
    /// A large surface's: a banner, a dialog, a page.
    pub const PAGE: Self = Self {
        nodes: PAGE_NODES,
        depth: TREE_DEPTH,
    };
}

/// A cell's colour, as a token the theme maps: plugins name no colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    #[default]
    Normal,
    Muted,
    Accent,
    Success,
    Warning,
    Danger,
}

/// A plugin's widgets for one slot: rows of cells, as Slint can't draw a
/// tree of any depth.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tree {
    #[serde(default)]
    pub rows: Vec<Row>,
    /// The part it is drawn as, `<plugin-id>/<name>`, when its plugin's
    /// manifest names one for its slot ([`part_id`]). The bridge sets it, not
    /// the tree's JSON, so a plugin can only be a part its manifest declares.
    #[serde(skip)]
    pub part: Option<String>,
    /// What a dialog is about, which its widgets' inputs name: set by the
    /// bridge from `ui.open_dialog`'s `subject`.
    #[serde(skip)]
    pub subject: Option<String>,
    /// What a click on a toast does, when `ui.toast` named a room or an id:
    /// set by the bridge, never by the tree's JSON.
    #[serde(skip)]
    pub click: Option<Box<Click>>,
}

/// A click a plugin asked for on its own toast or notification: the room it
/// opens, and the id its `clicked` input carries (empty when it gave none).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Click {
    pub room: Option<String>,
    pub id: String,
}

impl Click {
    /// The click `room` and `id` ask for, or none when they name neither: a
    /// toast or notification without them is only a line.
    pub fn asked(room: Option<String>, id: Option<String>) -> Option<Self> {
        let room = room.filter(|room| !room.trim().is_empty());
        if room.is_none() && id.is_none() {
            return None;
        }
        Some(Self {
            room,
            id: id.unwrap_or_default(),
        })
    }
}

/// The part a plugin's tree in `slot` is drawn as: `asked`, which the
/// manifest's `parts` must give that slot, or else the first name they give it;
/// `None` when they give it none. Refused, with the reason, for a name the
/// manifest doesn't declare or gives another slot.
pub fn part_id(
    plugin: &str,
    parts: &[PartDecl],
    slot: Slot,
    asked: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(asked) = asked else {
        return Ok(parts
            .iter()
            .find(|part| part.slot == slot.id())
            .map(|part| manifest::part_id(plugin, &part.name)));
    };
    match parts.iter().find(|part| part.name == asked) {
        None => Err(format!("the manifest names no part `{asked}`")),
        Some(part) if part.slot != slot.id() => Err(format!(
            "the manifest gives the part `{asked}` to the {} slot, not {}",
            part.slot,
            slot.id()
        )),
        Some(part) => Ok(Some(manifest::part_id(plugin, &part.name))),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    /// Drawn inside a frame, as a card.
    #[serde(default)]
    pub framed: bool,
    /// Steps in from the start, 0 to 3.
    #[serde(default)]
    pub indent: u8,
    #[serde(default)]
    pub cells: Vec<Cell>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    #[serde(default)]
    pub kind: Kind,
    /// What an interactive cell reports itself as.
    #[serde(default)]
    pub id: String,
    /// Markdown for text, a label for a control.
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub tone: Tone,
    /// An input's text, a select's chosen option, a slider's or a progress
    /// bar's number.
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub checked: bool,
    #[serde(default = "enabled")]
    pub enabled: bool,
    /// An icon's or image's picture: the plugin's own asset, or a URL
    /// through the network gate. Not drawn until the runtime fetches them.
    #[serde(default)]
    pub src: String,
    #[serde(default)]
    pub options: Vec<String>,
    /// A slider's or progress bar's range.
    #[serde(default)]
    pub min: f32,
    #[serde(default = "one")]
    pub max: f32,
    /// A number field's step; 0 takes any number.
    #[serde(default)]
    pub step: f32,
    /// Said when the pointer rests on it, plain text.
    #[serde(default)]
    pub tooltip: String,
    /// A button's icon, one of Yapfast's own by name ([`ICONS`]); the text
    /// is then what the tooltip and a screen reader say. A name this build
    /// doesn't have draws the text instead.
    #[serde(default)]
    pub icon: String,
    /// A section's rows.
    #[serde(default)]
    pub rows: Vec<Row>,
    /// A grid's cells.
    #[serde(default)]
    pub cells: Vec<Cell>,
    /// How many columns a grid lays its cells in, 1 to [`COLUMNS`].
    #[serde(default)]
    pub columns: u8,
    /// A `tabs` cell's tabs.
    #[serde(default)]
    pub tabs: Vec<Pane>,
    /// A list's items, markdown.
    #[serde(default)]
    pub items: Vec<String>,
    /// A list is numbered.
    #[serde(default)]
    pub ordered: bool,
    /// A table's column names, and its rows of cells.
    #[serde(default)]
    pub header: Vec<String>,
    #[serde(default)]
    pub table: Vec<Vec<String>>,
    /// A section starts folded shut.
    #[serde(default)]
    pub collapsed: bool,
    /// The part of an image's or tile's picture shown, `[x, y, width,
    /// height]` in its own pixels: one frame of an atlas.
    #[serde(default)]
    pub crop: Option<[u32; 4]>,
    /// The picture drawn pixel-sharp (nearest neighbour), for pixel art.
    #[serde(default)]
    pub pixelated: bool,
    /// An image's or tile picture's size on screen, 8 to [`PICTURE_SIDE`].
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
    /// Drawn as the chosen one: an accent ring round it.
    #[serde(default)]
    pub selected: bool,
    /// What an image or tile shows while the pointer is on it: another
    /// picture, or (with `hover_crop` alone) another part of the same.
    #[serde(default)]
    pub hover_src: String,
    #[serde(default)]
    pub hover_crop: Option<[u32; 4]>,
}

/// The largest an image's or a tile's picture is drawn, either way.
pub const PICTURE_SIDE: f32 = 512.0;

impl Cell {
    /// A picture that is a button: an image or a tile with an id.
    pub fn pressable(&self) -> bool {
        matches!(self.kind, Kind::Image | Kind::Tile) && !self.id.is_empty()
    }
}

/// One tab of a `tabs` cell: its label and what it shows.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pane {
    pub label: String,
    #[serde(default)]
    pub rows: Vec<Row>,
}

/// Where an image comes from, once checked: a file of the plugin's own
/// package, or an address on a host its manifest may reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source<'a> {
    /// `asset:icons/logo.png`: a path inside the plugin's folder.
    Asset(&'a str),
    /// `https://…`, and its host.
    Web { url: &'a str, host: &'a str },
}

/// The picture kinds a plugin's image may be.
const PICTURES: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

/// What an image's `src` names, or `None` for anything else: a path that
/// climbs out of the package, an absolute one, another scheme, another kind
/// of file.
pub fn source(src: &str) -> Option<Source<'_>> {
    if let Some(path) = src.strip_prefix("asset:") {
        let fine = !path.is_empty()
            && path.len() <= 256
            && !path.starts_with('/')
            && !path.contains('\\')
            && path
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..")
            && path.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/')
            })
            && path
                .rsplit_once('.')
                .is_some_and(|(_, kind)| PICTURES.contains(&kind.to_ascii_lowercase().as_str()));
        return fine.then_some(Source::Asset(path));
    }
    let rest = src.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    let fine = !host.is_empty()
        && src.len() <= 2048
        && !host.contains('@')
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b':'));
    fine.then_some(Source::Web { url: src, host })
}

fn enabled() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            kind: Kind::Text,
            id: String::new(),
            text: String::new(),
            tone: Tone::Normal,
            value: String::new(),
            checked: false,
            enabled: true,
            src: String::new(),
            options: Vec::new(),
            min: 0.0,
            max: 1.0,
            step: 0.0,
            tooltip: String::new(),
            icon: String::new(),
            rows: Vec::new(),
            cells: Vec::new(),
            columns: 0,
            tabs: Vec::new(),
            items: Vec::new(),
            ordered: false,
            header: Vec::new(),
            table: Vec::new(),
            collapsed: false,
            crop: None,
            pixelated: false,
            width: None,
            height: None,
            selected: false,
            hover_src: String::new(),
            hover_crop: None,
        }
    }
}

/// Cuts text to at most `bytes`, at a character's end.
fn cut(text: &mut String, bytes: usize) {
    if text.len() > bytes {
        let end = (0..=bytes)
            .rev()
            .find(|&end| text.is_char_boundary(end))
            .unwrap_or(0);
        text.truncate(end);
    }
}

impl Tree {
    /// A tree from a plugin's JSON, checked and capped for a small surface
    /// ([`Limits::SMALL`]); see [`Tree::checked_within`].
    pub fn parse(text: &str) -> Result<Self, String> {
        Self::parse_within(text, Limits::SMALL)
    }

    /// A tree from a plugin's JSON, checked against `limits`.
    pub fn parse_within(text: &str, limits: Limits) -> Result<Self, String> {
        let tree: Self = serde_json::from_str(text).map_err(|error| error.to_string())?;
        tree.checked_within(limits)
    }

    pub fn checked(self) -> Result<Self, String> {
        self.checked_within(Limits::SMALL)
    }

    /// The tree as it may be drawn: refused when it has more rows and cells
    /// than `limits.nodes` (a list's item and a table's cell count as one
    /// each), nests deeper than `limits.depth`, or has an interactive or
    /// stateful cell without a plain id; text over [`NODE_TEXT`] bytes is
    /// cut, and characters that hide or turn text around are taken out.
    pub fn checked_within(mut self, limits: Limits) -> Result<Self, String> {
        let nodes = count(&self.rows);
        if nodes > limits.nodes {
            return Err(format!(
                "it has {nodes} rows and cells, over {}",
                limits.nodes
            ));
        }
        check_rows(&mut self.rows, 1, limits.depth)?;
        Ok(self)
    }

    /// Every cell of the tree, those inside containers too.
    pub fn cells(&self) -> Vec<&Cell> {
        fn walk<'a>(rows: &'a [Row], into: &mut Vec<&'a Cell>) {
            for row in rows {
                for cell in &row.cells {
                    walk_cell(cell, into);
                }
            }
        }
        fn walk_cell<'a>(cell: &'a Cell, into: &mut Vec<&'a Cell>) {
            into.push(cell);
            walk(&cell.rows, into);
            for inner in &cell.cells {
                walk_cell(inner, into);
            }
            for pane in &cell.tabs {
                walk(&pane.rows, into);
            }
        }
        let mut cells = Vec::new();
        walk(&self.rows, &mut cells);
        cells
    }
}

/// The rows and cells in `rows`, those of containers too.
fn count(rows: &[Row]) -> usize {
    rows.iter()
        .map(|row| 1 + row.cells.iter().map(count_cell).sum::<usize>())
        .sum()
}

fn count_cell(cell: &Cell) -> usize {
    1 + count(&cell.rows)
        + cell.cells.iter().map(count_cell).sum::<usize>()
        + cell
            .tabs
            .iter()
            .map(|pane| 1 + count(&pane.rows))
            .sum::<usize>()
        + cell.items.len()
        + cell.header.len()
        + cell.table.iter().map(Vec::len).sum::<usize>()
}

fn check_rows(rows: &mut [Row], depth: usize, most: usize) -> Result<(), String> {
    if depth > most {
        return Err(format!("its containers nest deeper than {most} levels"));
    }
    for row in rows {
        row.indent = row.indent.min(3);
        for cell in &mut row.cells {
            check_cell(cell, depth, most)?;
        }
    }
    Ok(())
}

/// Plain words on one line, cut to `chars`: an option, a label, a tooltip.
fn line(text: &str, chars: usize) -> String {
    let mut kept: String = yapfast_plugin_api::text::strip(text)
        .chars()
        .take(chars)
        .collect();
    kept.retain(|character| !character.is_control());
    kept
}

/// Markdown a plugin wrote, its hiding characters taken out and cut.
fn words(text: &str) -> String {
    let mut kept = yapfast_plugin_api::text::strip(text);
    cut(&mut kept, NODE_TEXT);
    kept
}

fn check_cell(cell: &mut Cell, depth: usize, most: usize) -> Result<(), String> {
    if (cell.kind.interactive() || cell.kind.keeps_state() || cell.pressable())
        && !plain_id(&cell.id)
    {
        return Err(format!("a {:?} cell has no plain id", cell.kind));
    }
    // Words that hide themselves or turn a line around are taken out, not
    // refused: a tree is a plugin's drawing, not its name.
    cell.text = words(&cell.text);
    cell.value = words(&cell.value);
    cell.tooltip = line(&cell.tooltip, TOOLTIP_CHARS);
    if !ICONS.contains(&cell.icon.as_str()) {
        cell.icon.clear();
    }
    if cell.options.len() > TREE_NODES {
        return Err("a select has more than 64 options".into());
    }
    for option in &mut cell.options {
        *option = line(option, LABEL_CHARS);
    }
    if !(cell.min.is_finite() && cell.max.is_finite() && cell.min < cell.max) {
        cell.min = 0.0;
        cell.max = 1.0;
    }
    if !cell.step.is_finite() || cell.step < 0.0 {
        cell.step = 0.0;
    }
    match cell.kind {
        // A colour the field can't read starts empty rather than refusing
        // the whole drawing.
        Kind::Color if parse_colour(&cell.value).is_none() => cell.value.clear(),
        Kind::Number if !cell.value.is_empty() && cell.value.trim().parse::<f64>().is_err() => {
            cell.value.clear();
        }
        Kind::Icon | Kind::Image | Kind::Tile if source(&cell.src).is_none() => cell.src.clear(),
        _ => {}
    }
    if !cell.hover_src.is_empty() && source(&cell.hover_src).is_none() {
        cell.hover_src.clear();
    }
    // A size out of range is the picture's own instead.
    for side in [&mut cell.width, &mut cell.height] {
        if side.is_some_and(|side| !side.is_finite() || !(8.0..=PICTURE_SIDE).contains(&side)) {
            *side = None;
        }
    }
    // Only a container holds anything, and only what its kind draws.
    if cell.kind != Kind::Section {
        cell.rows.clear();
    }
    if cell.kind != Kind::Grid {
        cell.cells.clear();
    }
    if cell.kind != Kind::Tabs {
        cell.tabs.clear();
    }
    if cell.kind != Kind::List {
        cell.items.clear();
    }
    if cell.kind != Kind::Table {
        cell.header.clear();
        cell.table.clear();
    }
    match cell.kind {
        Kind::Section => check_rows(&mut cell.rows, depth + 1, most)?,
        Kind::Grid => {
            if depth + 1 > most {
                return Err(format!("its containers nest deeper than {most} levels"));
            }
            cell.columns = cell.columns.clamp(1, COLUMNS as u8);
            for inner in &mut cell.cells {
                check_cell(inner, depth + 1, most)?;
            }
        }
        Kind::Tabs => {
            if cell.tabs.is_empty() || cell.tabs.len() > TABS {
                return Err(format!(
                    "a tabs cell has {} tabs, not 1 to {TABS}",
                    cell.tabs.len()
                ));
            }
            for pane in &mut cell.tabs {
                pane.label = line(&pane.label, LABEL_CHARS);
                if pane.label.trim().is_empty() {
                    return Err("a tab has no label".into());
                }
                check_rows(&mut pane.rows, depth + 1, most)?;
            }
        }
        Kind::List => {
            for item in &mut cell.items {
                *item = words(item);
            }
        }
        Kind::Table => {
            if cell.header.len() > COLUMNS || cell.table.iter().any(|row| row.len() > COLUMNS) {
                return Err(format!("a table has more than {COLUMNS} columns"));
            }
            for name in &mut cell.header {
                *name = line(name, LABEL_CHARS);
            }
            for row in &mut cell.table {
                for item in row.iter_mut() {
                    *item = words(item);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(text: &str) -> Manifest {
        Manifest::parse(text).unwrap()
    }

    /// A manifest-only plugin that declares `ui.modify`, with this `ui`.
    fn with_ui(ui: &str) -> Manifest {
        manifest(&format!(
            r#"{{ "api": 1, "id": "com.example.t", "name": "T",
                "permissions": {{ "capabilities": ["ui.modify"] }}, "ui": {ui} }}"#
        ))
    }

    fn refusals(checked: &Checked) -> Vec<&str> {
        checked
            .refused
            .iter()
            .map(|refusal| refusal.why.as_str())
            .collect()
    }

    #[test]
    fn a_tree_is_the_part_its_manifest_names_for_its_slot() {
        let named = manifest(
            r#"{ "api": 1, "id": "com.example.player", "name": "Player", "wasm": "p.wasm",
                "parts": [{ "name": "now-playing", "slot": "banner" },
                          { "name": "lyrics", "slot": "dialog" },
                          { "name": "confirm", "slot": "dialog" }] }"#,
        );
        let parts = &named.parts;
        let id = |slot, asked| part_id("com.example.player", parts, slot, asked);
        assert_eq!(
            id(Slot::Banner, None),
            Ok(Some("com.example.player/now-playing".into()))
        );
        assert_eq!(
            id(Slot::Dialog, None),
            Ok(Some("com.example.player/lyrics".into())),
            "a tree that names no part is the first its slot has"
        );
        assert_eq!(
            id(Slot::Dialog, Some("confirm")),
            Ok(Some("com.example.player/confirm".into()))
        );
        assert_eq!(id(Slot::Toast, None), Ok(None), "a slot without a name");
        assert_eq!(
            id(Slot::Toast, Some("now-playing")),
            Err("the manifest gives the part `now-playing` to the banner slot, not toast".into()),
            "one name is one slot's"
        );
        assert_eq!(
            id(Slot::Banner, Some("queue")),
            Err("the manifest names no part `queue`".into())
        );
        assert!(check(&named).refused.is_empty());
        // A slot this build doesn't have is refused on the plugin's page, and a
        // name a theme couldn't address refuses the manifest.
        let elsewhere = manifest(
            r#"{ "api": 1, "id": "a.b", "name": "x", "wasm": "p.wasm",
                "parts": [{ "name": "side", "slot": "panel" }] }"#,
        );
        assert_eq!(
            refusals(&check(&elsewhere)),
            ["Yapfast has no slot called `panel`"]
        );
        assert!(
            Manifest::parse(
                r#"{ "api": 1, "id": "a.b", "name": "x",
                    "parts": [{ "name": "Side", "slot": "banner" }] }"#
            )
            .is_err()
        );
        assert!(
            Manifest::parse(
                r#"{ "api": 1, "id": "a.b", "name": "x",
                    "parts": [{ "name": "a", "slot": "banner" }, { "name": "a", "slot": "toast" }] }"#
            )
            .is_err()
        );
        assert!(
            Tree::parse(r#"{ "part": "com.example.other/now-playing", "rows": [] }"#).is_err(),
            "a tree can't name itself, only its manifest can"
        );
    }

    #[test]
    fn the_ui_section_takes_no_field_it_doesnt_know() {
        let text = r#"{ "api": 1, "id": "a.b", "name": "x", "ui": { "colours": {} } }"#;
        assert!(Manifest::parse(text).is_err());
        let text = r#"{ "api": 1, "id": "a.b", "name": "x",
            "ui": { "menu": [{ "target": "user", "row": "pin", "hide": true, "when": { "ever": true } }] } }"#;
        assert!(Manifest::parse(text).is_err());
        let text = r#"{ "api": 1, "id": "a.b", "name": "x",
            "ui": { "menu": [{ "target": "inbox", "row": "pin", "hide": true }] } }"#;
        assert!(Manifest::parse(text).is_err(), "targets are a closed list");
    }

    #[test]
    fn changes_need_ui_modify() {
        let checked = check(&manifest(
            r#"{ "api": 1, "id": "a.b", "name": "x", "ui": { "parts": { "hide": ["timeline.typing"] } } }"#,
        ));
        assert!(checked.changes.is_empty());
        assert_eq!(
            refusals(&checked),
            ["it doesn't declare the ui.modify capability"]
        );
        // Rows added to menus need no capability.
        let checked = check(&manifest(
            r#"{ "api": 1, "id": "a.b", "name": "x",
                "menus": [{ "target": "user", "label": "Wave", "action": "wave" }] }"#,
        ));
        assert_eq!(checked.rows.len(), 1);
        assert!(checked.refused.is_empty());
    }

    #[test]
    fn labels_are_one_line_of_at_most_64_characters() {
        let long = "x".repeat(65);
        let checked = check(&with_ui(&format!(
            r#"{{ "menu": [
                {{ "target": "user", "row": "profile", "label": "{long}" }},
                {{ "target": "user", "row": "mention", "label": "Ping\nthem" }},
                {{ "target": "user", "row": "message", "label": "  " }},
                {{ "target": "user", "row": "copy-id", "label": "{}" }}
            ] }}"#,
            "é".repeat(64)
        )));
        assert_eq!(checked.changes.len(), 1, "64 characters fit, in any script");
        assert_eq!(checked.refused.len(), 3);
        let added = check(&manifest(&format!(
            r#"{{ "api": 1, "id": "a.b", "name": "x",
                "menus": [{{ "target": "user", "label": "{long}", "action": "a" }},
                          {{ "target": "user", "label": "Ok", "action": "a/b" }}] }}"#
        )));
        assert!(added.rows.is_empty());
        assert_eq!(added.refused.len(), 2);
    }

    #[test]
    fn danger_rows_and_rows_not_listed_are_locked() {
        let checked = check(&with_ui(
            r#"{ "menu": [
                { "target": "user", "row": "block", "hide": true },
                { "target": "server", "row": "leave-server", "label": "Stay" },
                { "target": "user", "row": "self-mute", "hide": true },
                { "target": "user", "row": "copy-id", "after": "ban" },
                { "target": "user", "row": "copy-id", "before": "mention", "after": "profile" },
                { "target": "user", "row": "profile" },
                { "target": "user", "row": "copy-id", "before": "mention" }
            ] }"#,
        ));
        assert_eq!(
            refusals(&checked),
            [
                "it's drawn in the danger tone, so it can't change",
                "it's drawn in the danger tone, so it can't change",
                "this row can't be changed, or no longer exists",
                "it would move beside a row that can't change",
                "it names both before and after",
                "it changes nothing",
            ]
        );
        assert_eq!(checked.changes.len(), 1);
        for row in DANGER_ROWS {
            assert!(!EDITABLE_ROWS.contains(&row));
        }
    }

    #[test]
    fn parts_are_a_closed_list_and_protected_ones_are_refused() {
        let checked = check(&with_ui(
            r#"{ "parts": {
                "hide": ["sidebar.banner", "timeline.typing", "composer.send", "call.leave",
                         "timeline.gone", "account.identity", "sidebar.banner"],
                "replace": ["account.identity"] } }"#,
        ));
        assert_eq!(
            checked.changes,
            [
                Change::HidePart(Part::SidebarBanner),
                Change::HidePart(Part::TimelineTyping),
            ]
        );
        assert_eq!(
            refusals(&checked),
            [
                "no plugin can change it",
                "no plugin can change it",
                "this part no longer exists",
                "this part can't be hidden",
                "the plugin changes it twice",
                "replacing a part needs the plugin's wasm to draw it",
            ]
        );
        let with_wasm = manifest(
            r#"{ "api": 1, "id": "a.b", "name": "x", "wasm": "plugin.wasm",
                "permissions": { "capabilities": ["ui.modify"] },
                "ui": { "parts": { "replace": ["account.identity", "timeline.typing"] } } }"#,
        );
        let checked = check(&with_wasm);
        assert_eq!(
            checked.changes,
            [Change::ReplacePart(Part::AccountIdentity)]
        );
        assert_eq!(refusals(&checked), ["this part can't be replaced"]);
    }

    #[test]
    fn colours_are_tokens_and_danger_ones_are_locked() {
        let checked = check(&with_ui(
            r##"{ "colors": { "accent": "#7aa2ff", "danger": "#00ff00", "on-danger": "#fff",
                "glow": "#fff", "link": "red", "text": "#ffffff80", "shimmer": "#ffffff10" } }"##,
        ));
        assert_eq!(
            checked.changes,
            [
                Change::Colour("accent", 0xff7a_a2ff),
                Change::Colour("shimmer", 0x10ff_ffff),
            ]
        );
        assert_eq!(checked.refused.len(), 5);
    }

    #[test]
    fn contrast_follows_wcag() {
        assert!((contrast(0xff00_0000, 0xffff_ffff) - 21.0).abs() < 0.01);
        assert!((contrast(0xffff_ffff, 0xffff_ffff) - 1.0).abs() < 0.01);
        // #767676 on white is the classic 4.54:1.
        assert!((contrast(0xff76_7676, 0xffff_ffff) - 4.54).abs() < 0.01);
        assert_eq!(parse_colour("#abc"), Some(0xffaa_bbcc));
        assert_eq!(parse_colour("#11223344"), Some(0x4411_2233));
    }

    #[test]
    fn hard_to_read_text_colours_are_refused() {
        let theme = |name: &str| match name {
            "text" => Some(0xffee_eeee),
            "muted" => Some(0xff6a_6a6a),
            "accent" => Some(0xffff_bf47),
            "on-accent" => Some(0xff1d_1607),
            _ if SURFACES.contains(&name) => Some(0xff1e_1f24),
            _ => None,
        };
        // Dark grey text on a dark surface, and a surface the text vanishes
        // on, are refused; a readable text colour is kept.
        let refused = unreadable(theme, &[("text", 0xff33_3333), ("link", 0xff99_ccff)]);
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].0, "text");
        let refused = unreadable(theme, &[("chat", 0xffdd_dddd)]);
        assert_eq!(refused[0].0, "chat");
        // Muted text below 4.5:1 in the theme itself only has to stay as
        // readable as it was.
        let own = contrast(0xff6a_6a6a, 0xff1e_1f24);
        assert!(own < READABLE);
        assert!(unreadable(theme, &[("muted", 0xff6b_6b6b)]).is_empty());
        assert_eq!(unreadable(theme, &[("muted", 0xff50_5050)]).len(), 1);
        // An accent the text on it can't be read on.
        assert_eq!(unreadable(theme, &[("accent", 0xff22_2222)])[0].0, "accent");
    }

    #[test]
    fn sizes_are_named_and_clamped() {
        let checked = check(&with_ui(
            r#"{ "sizes": { "message-group-gap": 8, "text-size": 100, "sidebar-width": -5, "radius": 4 } }"#,
        ));
        assert_eq!(
            checked.changes,
            [
                Change::Size(Size::MessageGroupGap, 8.0),
                Change::Size(Size::SidebarWidth, 200.0),
                Change::Size(Size::TextSize, 20.0),
            ]
        );
        assert_eq!(refusals(&checked), ["Yapfast has no size by this name"]);
    }

    #[test]
    fn a_plugin_makes_at_most_64_changes() {
        let colours: Vec<String> = COLOURS
            .iter()
            .filter(|name| !LOCKED_COLOURS.contains(name))
            .map(|name| format!(r##""{name}": "#808080""##))
            .collect();
        let rows: Vec<String> = EDITABLE_ROWS
            .iter()
            .map(|row| format!(r#"{{ "target": "user", "row": "{row}", "hide": true }}"#))
            .collect();
        let checked = check(&with_ui(&format!(
            r#"{{ "menu": [{}], "colors": {{ {} }} }}"#,
            rows.join(","),
            colours.join(",")
        )));
        assert_eq!(checked.changes.len(), CHANGES);
        assert!(refusals(&checked).contains(&"the plugin makes more than 64 changes"));
    }

    #[test]
    fn when_matches_each_field_given() {
        let roles = ["7".to_owned()];
        let subject = Subject {
            network: NetworkName::Discord,
            own: false,
            dm: true,
            roles: &roles,
            ..Subject::default()
        };
        assert!(When::default().matches(&subject));
        let when = |json: &str| serde_json::from_str::<When>(json).unwrap();
        assert!(when(r#"{ "network": "discord", "own": false, "role": "7" }"#).matches(&subject));
        assert!(!when(r#"{ "network": "matrix" }"#).matches(&subject));
        assert!(!when(r#"{ "dm": false }"#).matches(&subject));
        assert!(!when(r#"{ "is_bot": true }"#).matches(&subject));
        assert!(!when(r#"{ "role": "8" }"#).matches(&subject));
        assert!(!when(r#"{ "has_attachment": true }"#).matches(&subject));
    }

    fn plugin(id: &str, ui: &str) -> Checked {
        let mut checked = check(&with_ui(ui));
        checked.id = id.into();
        checked
    }

    #[test]
    fn nothing_to_change_derives_no_overrides() {
        let quiet = check(&manifest(r#"{ "api": 1, "id": "a.b", "name": "x" }"#));
        assert_eq!(derive(&[quiet], &BTreeMap::new()), (None, vec![]));
        assert_eq!(derive(&[], &BTreeMap::new()), (None, vec![]));
    }

    #[test]
    fn plugins_hiding_one_part_agree_and_other_overlaps_conflict() {
        let one = plugin(
            "one",
            r##"{ "parts": { "hide": ["timeline.typing"] }, "colors": { "accent": "#7aa2ff" },
                 "menu": [{ "target": "user", "row": "copy-id", "label": "Copy user ID" }] }"##,
        );
        let two = plugin(
            "two",
            r##"{ "parts": { "hide": ["timeline.typing"] }, "colors": { "accent": "#ff0000", "link": "#7aa2ff" },
                 "sizes": { "message-gap": 4 },
                 "menu": [{ "target": "user", "row": "copy-id", "hide": true }] }"##,
        );
        let (overrides, conflicts) = derive(&[one.clone(), two.clone()], &BTreeMap::new());
        let overrides = overrides.unwrap();
        assert!(overrides.hides(Part::TimelineTyping));
        assert!(!overrides.hides(Part::SidebarBanner));
        assert_eq!(
            overrides.colours,
            [("accent", 0xff7a_a2ff, 0), ("link", 0xff7a_a2ff, 1)]
        );
        assert_eq!(overrides.size(Size::MessageGap), Some(4.0));
        assert_eq!(overrides.rows[0].0.label.as_deref(), Some("Copy user ID"));
        let keys: Vec<_> = conflicts.iter().map(|conflict| &conflict.key).collect();
        assert_eq!(
            keys,
            [
                &Key::Row(Target::User, "copy-id".into()),
                &Key::Colour("accent")
            ]
        );
        assert!(
            conflicts
                .iter()
                .all(|conflict| conflict.kept == "one" && conflict.lost == "two")
        );
        // The person chose the second plugin for the accent.
        let choices = BTreeMap::from([(Key::Colour("accent"), "two".to_owned())]);
        let (overrides, conflicts) = derive(&[one, two], &choices);
        assert_eq!(overrides.unwrap().colours[0], ("accent", 0xffff_0000, 1));
        assert_eq!(conflicts[1].kept, "two");
    }

    #[test]
    fn added_rows_name_their_plugin() {
        let checked = check(&manifest(
            r#"{ "api": 1, "id": "a.b", "name": "Waver",
                "menus": [{ "target": "user", "label": "Wave", "action": "wave" }] }"#,
        ));
        let (overrides, _) = derive(&[Checked::default(), checked], &BTreeMap::new());
        let overrides = overrides.unwrap();
        let id = UiOverrides::row_id(1, "wave");
        assert_eq!(id, "plugin/1/wave");
        let (plugin, action) = overrides.chosen(&id).unwrap();
        assert_eq!((plugin.name.as_str(), action), ("Waver", "wave"));
        assert!(overrides.chosen("plugin/9/wave").is_none());
        assert!(overrides.chosen("copy-id").is_none());
    }

    #[test]
    fn rail_items_and_header_buttons_are_checked_and_reach_their_plugin() {
        let manifest = manifest(
            r#"{ "api": 1, "id": "com.example.t", "name": "T",
                "rail_items": [{ "label": "Wave", "action": "wave" }],
                "header_buttons": [
                    { "label": "Keep", "action": "keep" },
                    { "label": "  ", "action": "empty" },
                    { "label": "Bad action", "action": "no spaces" }
                ] }"#,
        );
        let checked = check(&manifest);
        assert_eq!(
            checked.rail.len(),
            1,
            "a plugin's own surface needs no ui.modify"
        );
        assert_eq!(checked.rail[0].action, "wave");
        assert_eq!(checked.header.len(), 1);
        let refused: Vec<&str> = checked.refused.iter().map(|row| row.why.as_str()).collect();
        assert_eq!(refused.len(), 2);
        assert!(refused[0].contains("label won't show"), "{refused:?}");
        assert!(refused[1].contains("isn't a plain name"), "{refused:?}");
        // The overrides derive them with their plugin, whose id is what a click
        // reports.
        let (overrides, conflicts) = derive(&[checked], &BTreeMap::new());
        assert!(conflicts.is_empty());
        let overrides = overrides.unwrap();
        assert_eq!(overrides.rail[0].0.label, "Wave");
        assert_eq!(overrides.header[0].0.action, "keep");
        let id = UiOverrides::row_id(0, &overrides.rail[0].0.action);
        assert_eq!(overrides.chosen(&id).unwrap().1, "wave");
    }

    #[test]
    fn trees_are_capped() {
        let tree = Tree::parse(
            r#"{ "rows": [{ "indent": 9, "cells": [
                { "kind": "heading", "text": "Hi", "tone": "accent" },
                { "kind": "slider", "id": "volume", "min": 5, "max": 1 },
                { "kind": "toggle", "id": "on", "checked": true }
            ] }] }"#,
        )
        .unwrap();
        assert_eq!(tree.rows[0].indent, 3);
        assert_eq!(tree.rows[0].cells[1].max, 1.0, "a backwards range resets");
        assert!(tree.rows[0].cells[2].enabled);
        assert!(
            Tree::parse(r#"{ "rows": [{ "cells": [{ "kind": "button", "text": "Go" }] }] }"#)
                .is_err()
        );
        assert!(Tree::parse(r#"{ "rows": [{ "cells": [{ "kind": "blink" }] }] }"#).is_err());
        let many = format!(r#"{{ "rows": [{}] }}"#, vec!["{}"; 65].join(","));
        assert!(Tree::parse(&many).is_err());
        let long = format!(
            r#"{{ "rows": [{{ "cells": [{{ "text": "{}é" }}] }}] }}"#,
            "x".repeat(NODE_TEXT - 1)
        );
        let tree = Tree::parse(&long).unwrap();
        assert_eq!(
            tree.rows[0].cells[0].text.len(),
            NODE_TEXT - 1,
            "cut at a character's end"
        );
    }

    /// The widget set's containers, inputs and pictures parse, and each is
    /// held to its caps.
    #[test]
    fn the_richer_widgets_parse_and_are_capped() {
        let tree = Tree::parse_within(
            r##"{ "rows": [{ "cells": [
                { "kind": "tabs", "id": "pages", "tabs": [
                    { "label": "One", "rows": [{ "cells": [
                        { "kind": "section", "id": "more", "text": "More", "collapsed": true,
                          "rows": [{ "cells": [{ "kind": "grid", "columns": 9, "cells": [
                              { "kind": "badge", "text": "a" }, { "kind": "spinner", "text": "Loading" }
                          ] }] }] }
                    ] }] },
                    { "label": "Two" }
                ] },
                { "kind": "list", "ordered": true, "items": ["**one**", "two"] },
                { "kind": "table", "header": ["Name", "Count"], "table": [["a", "1"], ["b", "2"]] },
                { "kind": "number", "id": "n", "value": "four", "min": 0, "max": 10, "step": -1 },
                { "kind": "color", "id": "c", "value": "red" },
                { "kind": "textarea", "id": "t", "value": "two\nlines" },
                { "kind": "dropdown", "id": "d", "options": ["x", "y"], "value": "y",
                  "tooltip": "pick \u202eone" },
                { "kind": "image", "src": "asset:../secret.png" },
                { "kind": "image", "src": "asset:art/logo.webp" },
                { "kind": "icon", "src": "http://example.com/a.png" }
            ] }] }"##,
            Limits::PAGE,
        )
        .unwrap();
        let cells = &tree.rows[0].cells;
        let grid = &cells[0].tabs[0].rows[0].cells[0].rows[0].cells[0];
        assert_eq!(grid.kind, Kind::Grid);
        assert_eq!(usize::from(grid.columns), COLUMNS, "columns clamp");
        assert!(cells[0].tabs[0].rows[0].cells[0].collapsed);
        assert_eq!(cells[1].items[0], "**one**");
        assert_eq!(cells[2].table.len(), 2);
        assert!(
            cells[3].value.is_empty(),
            "a number that isn't one starts empty"
        );
        assert_eq!(cells[3].step, 0.0);
        assert!(
            cells[4].value.is_empty(),
            "a colour that isn't one starts empty"
        );
        assert_eq!(cells[5].value, "two\nlines");
        assert_eq!(cells[6].tooltip, "pick one", "no direction override");
        assert!(cells[7].src.is_empty(), "no climbing out of the package");
        assert_eq!(source(&cells[8].src), Some(Source::Asset("art/logo.webp")));
        assert!(cells[9].src.is_empty(), "only https");
        assert_eq!(
            source("https://cdn.example.com/a.png?x=1"),
            Some(Source::Web {
                url: "https://cdn.example.com/a.png?x=1",
                host: "cdn.example.com"
            })
        );
        assert!(source("https://user@evil.example/a.png").is_none());
        assert!(source("asset:a.exe").is_none());
        // Every cell is counted, the table's and list's too.
        assert_eq!(tree.cells().len(), 14);
    }

    /// Containers nest only so deep, a tabs cell has tabs with labels, a
    /// stateful cell has an id, and a table has so many columns.
    #[test]
    fn containers_are_held_to_their_rules() {
        let section = |inner: &str| {
            format!(
                r#"{{ "kind": "section", "id": "s", "text": "S", "rows": [{{ "cells": [{inner}] }}] }}"#
            )
        };
        let mut nested = r#"{ "text": "deep" }"#.to_owned();
        for _ in 0..3 {
            nested = section(&nested);
        }
        let three = format!(r#"{{ "rows": [{{ "cells": [{nested}] }}] }}"#);
        assert!(Tree::parse(&three).is_ok(), "four levels are fine");
        let four = format!(r#"{{ "rows": [{{ "cells": [{}] }}] }}"#, section(&nested));
        let refused = Tree::parse(&four).unwrap_err();
        assert!(refused.contains("nest deeper"), "{refused}");
        assert!(
            Tree::parse(r#"{ "rows": [{ "cells": [{ "kind": "tabs", "id": "t" }] }] }"#).is_err()
        );
        assert!(
            Tree::parse(r#"{ "rows": [{ "cells": [{ "kind": "tabs", "id": "t", "tabs": [{ "label": " " }] }] }] }"#)
                .is_err()
        );
        assert!(
            Tree::parse(r#"{ "rows": [{ "cells": [{ "kind": "section", "text": "S" }] }] }"#)
                .is_err(),
            "a section keeps its fold by its id"
        );
        let wide = format!(
            r#"{{ "rows": [{{ "cells": [{{ "kind": "table", "header": [{}] }}] }}] }}"#,
            ["\"x\""; COLUMNS + 1].join(",")
        );
        assert!(Tree::parse_within(&wide, Limits::PAGE).is_err());
        // A list's items count: 70 of them are over a small slot's 64 but
        // fit a page.
        let long = format!(
            r#"{{ "rows": [{{ "cells": [{{ "kind": "list", "items": [{}] }}] }}] }}"#,
            ["\"x\""; 70].join(",")
        );
        assert!(Tree::parse(&long).is_err());
        assert!(Tree::parse_within(&long, Limits::PAGE).is_ok());
        // A cell only holds what its kind draws.
        let tree = Tree::parse(
            r#"{ "rows": [{ "cells": [{ "kind": "text", "text": "t", "items": ["a"], "rows": [{}] }] }] }"#,
        )
        .unwrap();
        assert!(tree.rows[0].cells[0].items.is_empty());
        assert!(tree.rows[0].cells[0].rows.is_empty());
    }
}
