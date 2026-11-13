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

/// The most rows and cells together in one widget tree.
pub const TREE_NODES: usize = 64;

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
    if text.chars().any(char::is_control) {
        return Err("it has a line break or control character".into());
    }
    Ok(text.to_owned())
}

/// An action's or widget's id: letters, digits, `-`, `_` and `.`, as it
/// travels inside `plugin/<index>/<action>`.
fn plain_id(id: &str) -> bool {
    (1..=LABEL_CHARS).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
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
}

impl Kind {
    /// It reports what the person does with it, by its id.
    pub const fn interactive(self) -> bool {
        matches!(
            self,
            Self::Button | Self::Link | Self::Toggle | Self::Select | Self::Input | Self::Slider
        )
    }
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
    /// A tree from a plugin's JSON, checked and capped: refused when it has
    /// more than [`TREE_NODES`] rows and cells, or an interactive cell
    /// without a plain id; text over [`NODE_TEXT`] bytes is cut.
    pub fn parse(text: &str) -> Result<Self, String> {
        let tree: Self = serde_json::from_str(text).map_err(|error| error.to_string())?;
        tree.checked()
    }

    pub fn checked(mut self) -> Result<Self, String> {
        let nodes: usize = self.rows.iter().map(|row| 1 + row.cells.len()).sum();
        if nodes > TREE_NODES {
            return Err(format!("it has {nodes} rows and cells, over {TREE_NODES}"));
        }
        for row in &mut self.rows {
            row.indent = row.indent.min(3);
            for cell in &mut row.cells {
                if cell.kind.interactive() && !plain_id(&cell.id) {
                    return Err(format!("a {:?} cell has no plain id", cell.kind));
                }
                cut(&mut cell.text, NODE_TEXT);
                cut(&mut cell.value, NODE_TEXT);
                if cell.options.len() > TREE_NODES {
                    return Err("a select has more than 64 options".into());
                }
                for option in &mut cell.options {
                    let mut kept: String = option.chars().take(LABEL_CHARS).collect();
                    kept.retain(|character| !character.is_control());
                    *option = kept;
                }
                if !(cell.min.is_finite() && cell.max.is_finite() && cell.min < cell.max) {
                    cell.min = 0.0;
                    cell.max = 1.0;
                }
            }
        }
        Ok(self)
    }
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
}
