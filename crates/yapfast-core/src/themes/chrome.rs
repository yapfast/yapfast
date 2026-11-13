//! A theme's `chrome`: the fixed words and small marks the window draws
//! around people's text, so a theme can make it read as a terminal, a zine or
//! a ledger without touching what anything does.
//!
//! Everything here is plain text from closed lists: labels with a few
//! placeholders, marks before and after names, which pictures to leave out,
//! the server rail as a list of names, the composer's prompt and caret, and a
//! status column in the member list. Screen readers keep hearing the stock
//! names, and nothing a theme writes is ever read as markup.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::layout::closed;
use super::package::{Error, colour_at, named, number, object};

/// The longest a label is, in characters.
pub const MAX_LABEL: usize = 48;
/// The longest a mark before or after a name is.
pub const MAX_MARK: usize = 8;
/// The longest a status in the member list's column is.
pub const MAX_STATUS: usize = 12;
/// The longest the composer's prompt is.
pub const MAX_PROMPT: usize = 4;

closed! {
    /// A fixed string the window shows, which a theme may reword.
    Label {
        /// A line over the member list, such as `$ who`.
        MembersTitle => "members_title",
        /// Each group over the member list: `{role}` and `{n}`.
        MembersGroup => "members_group",
        /// A channel category's heading: `{name}`.
        Category => "category",
        /// The composer's hint in a channel: `{channel}`.
        Composer => "composer",
        /// The divider over the first unread message.
        New => "new",
        /// Who is typing: `{name}` (one name, or several joined) and `{n}`.
        Typing => "typing",
        /// The big line at the very start of a channel: `{channel}`.
        Welcome => "welcome",
        /// The small line at the start of a channel's history: `{channel}`.
        Start => "start",
        /// A line over the server rail in its text mode.
        RailTitle => "rail_title",
        /// A line under the servers in the rail's text mode: `{server}`.
        RailFooter => "rail_footer",
        /// The home row (direct messages) in the rail's text mode.
        RailHome => "rail_home",
        /// A reaction's count when reactions are text: `{n}`.
        Reaction => "reaction",
        /// The root line over a tree of channels: `{server}`.
        TreeRoot => "tree_root",
    }
}

impl Label {
    /// The placeholders it may use.
    pub fn placeholders(self) -> &'static [&'static str] {
        match self {
            Self::MembersGroup => &["role", "n"],
            Self::Category => &["name"],
            Self::Composer | Self::Welcome | Self::Start => &["channel"],
            Self::Typing => &["name", "n"],
            Self::RailFooter | Self::TreeRoot => &["server"],
            // The member list's title may count the members.
            Self::Reaction | Self::MembersTitle => &["n"],
            Self::New | Self::RailTitle | Self::RailHome => &[],
        }
    }
}

closed! {
    /// Where a mark goes before or after a name.
    Place {
        /// A channel's row in the channel list.
        Channel => "channel",
        /// A server's name in the rail's text mode.
        Server => "server",
        /// A person's row in the member list.
        Member => "member",
        /// The author of a message.
        Author => "author",
        /// The channel's name in the bar over the messages.
        Header => "header",
    }
}

closed! {
    /// What a theme may leave out. Nothing here takes an action away: each
    /// stays a click or a key away, and named for screen readers.
    Hide {
        /// Avatars in the member list.
        MemberAvatars => "member-avatars",
        /// The icon before a channel's name, in the list and the bar.
        ChannelIcons => "channel-icons",
        /// The icon buttons in the bar over the messages: they show while
        /// the pointer is on the bar or one has the keyboard.
        HeaderButtons => "header-buttons",
        /// Servers' icons in the rail: their names as text rows instead, the
        /// same as `rail.mode: "text"`.
        ServerIcons => "server-icons",
        /// Your own avatar in the account panel.
        AccountAvatar => "account-avatar",
        /// The Activity cards over the member list.
        ActivityCards => "activity-cards",
        /// Reactions' chips: each is its emoji and its count as text, such as
        /// `(+3)`.
        ReactionChips => "reaction-chips",
    }
}

closed! {
    /// A list a theme may draw as a file tree, as the `tree` command does:
    /// `├──` before each row, `└──` before a group's last, `│` carrying on
    /// past rows nested under it.
    TreeList {
        /// Channels under their categories, threads and who is in a voice
        /// channel under the channel.
        Channels => "channels",
        /// Members under their groups.
        Members => "members",
        /// The direct messages list.
        Dms => "dms",
        /// The server rail in its text mode.
        Rail => "rail",
        /// A reply's quote as `└─>` instead of the curved spine, and a
        /// thread under its first message as `├─ thread: name`.
        Replies => "replies",
    }
}

closed! {
    /// The theme colour a tree's lines take.
    TreeInk {
        Faint => "faint",
        Muted => "muted",
        Secondary => "secondary",
        Text => "text",
        Accent => "accent",
    }
}

closed! {
    /// How the server rail shows servers.
    RailMode { Icons => "icons", Text => "text" }
}

closed! {
    /// The composer's caret.
    Caret { Line => "line", Block => "block" }
}

closed! {
    /// A presence the member list's status column names.
    Presence {
        Online => "online",
        Idle => "idle",
        Dnd => "dnd",
        Offline => "offline",
        Streaming => "streaming",
    }
}

/// The theme's words and marks around the window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chrome {
    pub labels: BTreeMap<Label, String>,
    /// Marks before and after names, by place.
    pub prefix: BTreeMap<Place, String>,
    pub suffix: BTreeMap<Place, String>,
    /// What goes before and after a message's time, from a pattern such as
    /// `[HH:MM]`; the time itself follows the person's clock setting.
    pub time: Option<(String, String)>,
    pub hide: BTreeSet<Hide>,
    pub rail: Option<RailMode>,
    /// Pixels of the rail in its text mode (96–320).
    pub rail_width: Option<f32>,
    pub caret: Option<Caret>,
    /// Drawn before the composer's text, such as `$`.
    pub prompt: Option<String>,
    /// The member list's status column, by presence; `{n}` is the row's
    /// place in its group, from 0.
    pub status: BTreeMap<Presence, String>,
    /// The lists drawn as file trees.
    pub tree: BTreeSet<TreeList>,
    /// Their lines' colour: a theme colour by name, or ARGB.
    pub tree_ink: Option<TreeInk>,
    pub tree_colour: Option<u32>,
    /// Server folders in the text rail and channel categories as
    /// directories: the marks before an open and a closed one, and after
    /// the name, such as `▾`, `▸` and `/`.
    pub folders: Option<Folders>,
}

/// How a folder or category reads as a directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folders {
    pub open: String,
    pub closed: String,
    pub suffix: String,
}

impl Default for Folders {
    fn default() -> Self {
        Self {
            open: "▾".into(),
            closed: "▸".into(),
            suffix: "/".into(),
        }
    }
}

impl Chrome {
    /// Whether it changes anything at all.
    pub fn is_stock(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the rail lists servers as text.
    pub fn rail_text(&self) -> bool {
        self.rail == Some(RailMode::Text) || self.hide.contains(&Hide::ServerIcons)
    }

    /// What it changes, in a few words each, for `yapfast theme check`.
    pub fn summary(&self) -> Vec<String> {
        let mut parts = Vec::new();
        if !self.labels.is_empty() {
            parts.push(format!("{} labels", self.labels.len()));
        }
        let marks = self.prefix.len() + self.suffix.len() + usize::from(self.time.is_some());
        if marks > 0 {
            parts.push(format!("{marks} marks"));
        }
        for hidden in &self.hide {
            parts.push(format!("no {}", hidden.name()));
        }
        if self.rail_text() {
            parts.push("text rail".into());
        }
        if self.caret == Some(Caret::Block) {
            parts.push("block caret".into());
        }
        if self.prompt.is_some() {
            parts.push("prompt".into());
        }
        if !self.status.is_empty() {
            parts.push("status column".into());
        }
        if self.folders.is_some() {
            parts.push("folders".into());
        }
        if !self.tree.is_empty() {
            let lists: Vec<&str> = self.tree.iter().map(|list| list.name()).collect();
            parts.push(format!("trees ({})", lists.join(", ")));
        }
        parts
    }
}

/// Every placeholder a label may use.
pub const PLACEHOLDERS: [&str; 5] = ["channel", "server", "n", "role", "name"];

/// Fills a label's placeholders. A placeholder without a value stays empty.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let key = &after[..close];
                if let Some((_, value)) = values.iter().find(|(name, _)| *name == key) {
                    out.push_str(value);
                } else if !PLACEHOLDERS.contains(&key) {
                    // Not a placeholder at all: left as written.
                    out.push('{');
                    out.push_str(key);
                    out.push('}');
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// A member list group's heading, `Leads — 2` or `MEMBERS — 3`, split into
/// its name and count.
pub fn split_group(heading: &str) -> (&str, &str) {
    match heading.rsplit_once(" — ") {
        Some((role, count)) if count.chars().all(|c| c.is_ascii_digit()) && !count.is_empty() => {
            (role, count)
        }
        _ => (heading, ""),
    }
}

/// Who is typing, from the stock line, as names and how many.
pub fn split_typing(line: &str) -> (String, usize) {
    if line.starts_with("Several people") {
        return ("several people".into(), 4);
    }
    let names = line
        .strip_suffix(" is typing...")
        .or_else(|| line.strip_suffix(" are typing..."))
        .unwrap_or(line);
    let count = 1 + names.matches(", ").count() + names.matches(" and ").count();
    (names.to_owned(), count)
}

/// Plain text on one line: no control characters, and none of the marks that
/// turn text around, which would let a label reorder what's beside it.
fn plain(value: &Value, place: &str, most: usize) -> Result<String, Error> {
    let text = value
        .as_str()
        .ok_or_else(|| Error(format!("\"{place}\" has to be text")))?;
    let turning = |c: char| matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
    if text.chars().count() > most || text.chars().any(|c| c.is_control() || turning(c)) {
        return Err(Error(format!(
            "\"{place}\" is plain text, at most {most} characters, on one line"
        )));
    }
    Ok(text.to_owned())
}

/// Refuses a `{word}` the label doesn't take.
fn placeholders(text: &str, place: &str, allowed: &[&str]) -> Result<(), Error> {
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        let key = &after[..close];
        if !allowed.contains(&key) {
            let takes = if allowed.is_empty() {
                "none".to_owned()
            } else {
                allowed
                    .iter()
                    .map(|name| format!("{{{name}}}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            return Err(Error(format!(
                "\"{place}\" can't use {{{key}}}; it takes {takes}"
            )));
        }
        rest = &after[close + 1..];
    }
    Ok(())
}

fn marks(
    value: &Value,
    place: &str,
    into: &mut BTreeMap<Place, String>,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    for (key, value) in object(value, place)? {
        let at = format!("{place}.{key}");
        match Place::named(key) {
            Some(spot) => {
                into.insert(spot, plain(value, &at, MAX_MARK)?);
            }
            None => notes.push(format!("\"{at}\" isn't a place for a mark; ignored")),
        }
    }
    Ok(())
}

pub(super) fn read_chrome(
    value: &Value,
    place: &str,
    chrome: &mut Chrome,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    for (key, value) in object(value, place)? {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "labels" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    let Some(label) = Label::named(key) else {
                        notes.push(format!("\"{at}\" isn't a label Yapfast shows; ignored"));
                        continue;
                    };
                    let text = plain(value, &at, MAX_LABEL)?;
                    placeholders(&text, &at, label.placeholders())?;
                    chrome.labels.insert(label, text);
                }
            }
            "prefix" => marks(value, &here, &mut chrome.prefix, notes)?,
            "suffix" => marks(value, &here, &mut chrome.suffix, notes)?,
            "time" => {
                let pattern = plain(value, &here, 16)?;
                let Some((before, after)) = pattern.split_once("HH:MM") else {
                    return Err(Error(format!(
                        "\"{here}\" holds HH:MM where the time goes, such as \"[HH:MM]\""
                    )));
                };
                chrome.time = Some((before.to_owned(), after.to_owned()));
            }
            "hide" => {
                let items = value
                    .as_array()
                    .ok_or_else(|| Error(format!("\"{here}\" is a list")))?;
                for (index, item) in items.iter().enumerate() {
                    let at = format!("{here}[{index}]");
                    match item.as_str().and_then(Hide::named) {
                        Some(hidden) => {
                            chrome.hide.insert(hidden);
                        }
                        None => notes.push(format!(
                            "\"{at}\" isn't something a theme can leave out; ignored"
                        )),
                    }
                }
            }
            "rail" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "mode" => {
                            chrome.rail = Some(named(value, &at, RailMode::ALL, RailMode::name)?)
                        }
                        "width" => chrome.rail_width = Some(number(value, &at, (96.0, 320.0))?),
                        _ => {
                            notes.push(format!("\"{at}\" isn't something the rail takes; ignored"))
                        }
                    }
                }
            }
            "composer" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "caret" => chrome.caret = Some(named(value, &at, Caret::ALL, Caret::name)?),
                        "prompt" => chrome.prompt = Some(plain(value, &at, MAX_PROMPT)?),
                        _ => notes.push(format!(
                            "\"{at}\" isn't something the composer takes; ignored"
                        )),
                    }
                }
            }
            "members" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    if key != "columns" {
                        notes.push(format!("\"{at}\" isn't something members takes; ignored"));
                        continue;
                    }
                    for (key, value) in object(value, &at)? {
                        let spot = format!("{at}.{key}");
                        let Some(presence) = Presence::named(key) else {
                            notes.push(format!("\"{spot}\" isn't a presence; ignored"));
                            continue;
                        };
                        let text = plain(value, &spot, MAX_STATUS)?;
                        placeholders(&text, &spot, &["n"])?;
                        chrome.status.insert(presence, text);
                    }
                }
            }
            "folders" => {
                let folders = chrome.folders.get_or_insert_with(Folders::default);
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "open" => folders.open = plain(value, &at, MAX_PROMPT)?,
                        "closed" => folders.closed = plain(value, &at, MAX_PROMPT)?,
                        "suffix" => folders.suffix = plain(value, &at, MAX_PROMPT)?,
                        _ => notes.push(format!("\"{at}\" isn't something folders takes; ignored")),
                    }
                }
            }
            "tree" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "lists" => {
                            let items = value
                                .as_array()
                                .ok_or_else(|| Error(format!("\"{at}\" is a list")))?;
                            for (index, item) in items.iter().enumerate() {
                                match item.as_str().and_then(TreeList::named) {
                                    Some(list) => {
                                        chrome.tree.insert(list);
                                    }
                                    None => notes.push(format!(
                                        "\"{at}[{index}]\" isn't a list Yapfast draws as a tree; ignored"
                                    )),
                                }
                            }
                        }
                        "colour" => match value.as_str().and_then(TreeInk::named) {
                            Some(ink) => {
                                chrome.tree_ink = Some(ink);
                                chrome.tree_colour = None;
                            }
                            None => {
                                chrome.tree_colour = Some(colour_at(value, &at)?);
                                chrome.tree_ink = None;
                            }
                        },
                        _ => notes.push(format!("\"{at}\" isn't something tree takes; ignored")),
                    }
                }
            }
            _ => notes.push(format!("\"{here}\" isn't something chrome takes; ignored")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::package::{Mode, Package};

    fn theme(chrome: &str) -> Result<Package, Error> {
        Package::parse(&format!(
            r#"{{ "theme_api": 1, "id": "com.example.chrome", "name": "Chrome", "version": "1.0.0",
                "chrome": {chrome},
                "variants": {{ "dark": {{ "base": "yap" }} }} }}"#
        ))
    }

    #[test]
    fn reads_every_chrome_field() {
        let package = theme(
            r##"{
                "labels": { "members_title": "$ who", "members_group": "{role} ({n})",
                    "composer": "$ {channel}", "new": "-- new --", "typing": "{name} typing ({n})",
                    "rail_footer": "~ $ cd {server}", "reaction": "(+{n})" },
                "prefix": { "channel": "./", "author": "<" },
                "suffix": { "author": ">", "server": "/" },
                "time": "[HH:MM]",
                "hide": ["member-avatars", "header-buttons", "reaction-chips"],
                "rail": { "mode": "text", "width": 150 },
                "composer": { "caret": "block", "prompt": "$" },
                "members": { "columns": { "online": "pts/{n}", "idle": "idle" } },
                "tree": { "lists": ["channels", "members", "replies"], "colour": "faint" },
                "folders": { "closed": ">" }
            }"##,
        )
        .unwrap();
        assert!(package.notes.is_empty(), "{:?}", package.notes);
        let chrome = package.resolve(Mode::Dark).unwrap().chrome;
        assert_eq!(chrome.labels[&Label::MembersTitle], "$ who");
        assert_eq!(chrome.prefix[&Place::Channel], "./");
        assert_eq!(chrome.suffix[&Place::Author], ">");
        assert_eq!(chrome.time, Some(("[".into(), "]".into())));
        assert!(chrome.hide.contains(&Hide::HeaderButtons));
        assert!(chrome.rail_text());
        assert_eq!(chrome.rail_width, Some(150.0));
        assert_eq!(chrome.caret, Some(Caret::Block));
        assert_eq!(chrome.prompt.as_deref(), Some("$"));
        assert_eq!(chrome.status[&Presence::Online], "pts/{n}");
        assert!(chrome.tree.contains(&TreeList::Replies) && !chrome.tree.contains(&TreeList::Rail));
        assert_eq!(chrome.tree_ink, Some(TreeInk::Faint));
        let folders = chrome.folders.clone().unwrap();
        assert_eq!(
            (
                folders.open.as_str(),
                folders.closed.as_str(),
                folders.suffix.as_str()
            ),
            ("▾", ">", "/")
        );
        assert!(!chrome.is_stock());
    }

    #[test]
    fn refuses_unknown_placeholders_markup_breaks_and_long_text() {
        let error = theme(r#"{ "labels": { "new": "{n} new" } }"#).unwrap_err();
        assert!(
            error.0.contains("chrome.labels.new") && error.0.contains("none"),
            "{error}"
        );
        let error = theme(r#"{ "labels": { "composer": "{server}" } }"#).unwrap_err();
        assert!(error.0.contains("{channel}"), "{error}");
        assert!(theme(r#"{ "labels": { "new": "a\nb" } }"#).is_err());
        assert!(theme("{ \"prefix\": { \"author\": \"\\u202e<\" } }").is_err());
        assert!(theme(r#"{ "prefix": { "author": "123456789" } }"#).is_err());
        assert!(theme(r#"{ "time": "[HH]" }"#).is_err());
        assert!(theme(r#"{ "rail": { "width": 40 } }"#).is_err());
        assert!(theme(r#"{ "composer": { "prompt": "user@host $" } }"#).is_err());
        assert!(theme(r#"{ "tree": { "colour": "glow" } }"#).is_err());
        let hex = theme(r##"{ "tree": { "lists": ["rail"], "colour": "#2E7D45" } }"##).unwrap();
        assert_eq!(
            hex.resolve(Mode::Dark).unwrap().chrome.tree_colour,
            Some(0xff2e7d45)
        );
    }

    #[test]
    fn unknown_names_are_noted() {
        let package = theme(
            r#"{ "labels": { "footer": "x" }, "prefix": { "emoji": "x" }, "hide": ["everything"], "sparkle": 1 }"#,
        )
        .unwrap();
        assert_eq!(package.notes.len(), 4, "{:?}", package.notes);
        assert!(package.resolve(Mode::Dark).unwrap().chrome.is_stock());
    }

    #[test]
    fn a_variant_adds_to_the_shared_chrome() {
        let package = Package::parse(
            r##"{ "theme_api": 1, "id": "com.example.chrome", "name": "Chrome", "version": "1.0.0",
                "chrome": { "prefix": { "channel": "#" }, "labels": { "new": "new" } },
                "variants": { "dark": { "base": "yap", "chrome": { "prefix": { "channel": "./" } } } } }"##,
        )
        .unwrap();
        let chrome = package.resolve(Mode::Dark).unwrap().chrome;
        assert_eq!(chrome.prefix[&Place::Channel], "./");
        assert_eq!(chrome.labels[&Label::New], "new");
    }

    #[test]
    fn fills_placeholders() {
        assert_eq!(
            fill("{role} — {n}", &[("role", "Leads"), ("n", "2")]),
            "Leads — 2"
        );
        assert_eq!(fill("$ {channel}", &[("channel", "general")]), "$ general");
        assert_eq!(fill("{x} {", &[("n", "1")]), "{x} {");
        assert_eq!(fill("pts/{n}", &[]), "pts/");
        assert_eq!(split_group("Leads — 2"), ("Leads", "2"));
        assert_eq!(split_group("Online"), ("Online", ""));
        assert_eq!(split_typing("Sam is typing..."), ("Sam".into(), 1));
        assert_eq!(
            split_typing("Sam, Riley and Jo are typing..."),
            ("Sam, Riley and Jo".into(), 3)
        );
        assert_eq!(split_typing("Several people are typing...").1, 4);
    }

    #[test]
    fn the_template_reads_without_notes() {
        let package =
            Package::parse(include_str!("../../../../themes/template/theme.json")).unwrap();
        assert!(package.notes.is_empty(), "{:?}", package.notes);
        assert!(!package.resolve(Mode::Dark).unwrap().chrome.is_stock());
    }
}
