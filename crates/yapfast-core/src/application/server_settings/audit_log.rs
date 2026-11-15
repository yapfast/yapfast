//! Audit Log, as Discord's server settings show it: who did what, newest
//! first, with the lines each row's arrow opens. Each page Discord sends is
//! put into Discord's sentences once, as it arrives, and only those are
//! kept. The templates are Discord's own, from its web client; see "Audit
//! Log" in docs/discord/server-settings.md.

use std::collections::HashMap;

use serde_json::Value;
use yapfast_lang::{tr, trn};

use super::*;
use crate::chat::{AuditChange, AuditEntry, AuditLog, AuditQuery, AuditUser};

/// How many entries Discord sends at a time; a page this long may have
/// older ones.
pub const AUDIT_PAGE: usize = 50;

/// Entries within this many seconds of the first, by the same moderator to
/// the same thing, show as one row, as in Discord's client.
const GROUP_SECONDS: u64 = 30 * 60;

/// The most entries a row takes in.
const GROUP_MOST: usize = 50;

/// Discord's epoch, in Unix milliseconds, from which snowflakes count.
const DISCORD_EPOCH: u64 = 1_420_070_400_000;

/// Discord's "Filter by Action", in its order: each action's number and
/// name.
pub const AUDIT_ACTIONS: [(u16, &str); 68] = [
    (1, "Update Server"),
    (10, "Create Channel"),
    (11, "Update Channel"),
    (12, "Delete Channel"),
    (13, "Create Channel Permissions"),
    (14, "Update Channel Permissions"),
    (15, "Delete Channel Permissions"),
    (16, "Move Channel"),
    (20, "Kick Member"),
    (21, "Prune Members"),
    (22, "Ban Member"),
    (23, "Unban Member"),
    (24, "Update Member"),
    (25, "Update Member Roles"),
    (26, "Move Member"),
    (27, "Disconnect Member"),
    (28, "Add Bot"),
    (110, "Create Thread"),
    (111, "Update Thread"),
    (112, "Delete Thread"),
    (30, "Create Role"),
    (31, "Update Role"),
    (32, "Delete Role"),
    (33, "Move Role"),
    (163, "Create Customization Question"),
    (164, "Update Customization Question"),
    (165, "Delete Customization Question"),
    (166, "Create Onboarding"),
    (167, "Update Onboarding"),
    (190, "Create Server Guide"),
    (191, "Update Server Guide"),
    (40, "Create Invite"),
    (41, "Update Invite"),
    (42, "Delete Invite"),
    (50, "Create Webhook"),
    (51, "Update Webhook"),
    (52, "Delete Webhook"),
    (60, "Create Emoji"),
    (61, "Update Emoji"),
    (62, "Delete Emoji"),
    (72, "Delete Messages"),
    (73, "Bulk Delete Messages"),
    (74, "Pin Message"),
    (75, "Unpin Message"),
    (80, "Create Integration"),
    (81, "Update Integration"),
    (82, "Delete Integration"),
    (90, "Create Sticker"),
    (91, "Update Sticker"),
    (92, "Delete Sticker"),
    (83, "Start Stage"),
    (84, "Update Stage"),
    (85, "End Stage"),
    (100, "Create Event"),
    (101, "Update Event"),
    (102, "Cancel Event"),
    (121, "Update Command Permissions"),
    (143, "AutoMod Block Message"),
    (140, "Create AutoMod Rule"),
    (141, "Update AutoMod Rule"),
    (142, "Cancel AutoMod Rule"),
    (171, "Feature Item on Home"),
    (172, "Remove Item from Home"),
    (130, "Create Soundboard Sound"),
    (131, "Update Soundboard Sound"),
    (132, "Delete Soundboard Sound"),
    (192, "Create Voice Channel Status"),
    (193, "Delete Voice Channel Status"),
];

/// The permissions a change grants or denies, by bit, in the order and
/// with the names Discord's log lists them.
const PERMISSION_NAMES: [(u8, &str); 52] = [
    (0, "Create Invite"),
    (1, "Kick Members"),
    (2, "Ban Members"),
    (3, "Administrator"),
    (4, "Manage Channels"),
    (5, "Manage Server"),
    (19, "View Server Insights"),
    (41, "View Server Subscription Insights"),
    (26, "Change Nickname"),
    (27, "Manage Nicknames"),
    (28, "Manage Roles"),
    (29, "Manage Webhooks"),
    (43, "Create Expressions"),
    (30, "Manage Expressions"),
    (7, "View Audit Log"),
    (10, "View Channels"),
    (11, "Send Messages"),
    (12, "Send TTS Messages"),
    (31, "Use Application Commands"),
    (13, "Manage Messages"),
    (14, "Embed Links"),
    (15, "Attach Files"),
    (16, "Read Message History"),
    (17, "Mention @everyone, @here, and All Roles"),
    (18, "Use External Emoji"),
    (37, "Use External Stickers"),
    (6, "Add Reactions"),
    (20, "Connect"),
    (21, "Speak"),
    (22, "Mute Members"),
    (23, "Deafen Members"),
    (24, "Move Members"),
    (25, "Use Voice Activity"),
    (8, "Priority Speaker"),
    (9, "Video"),
    (42, "Use Soundboard"),
    (45, "Use External Sounds"),
    (32, "Request to Speak"),
    (39, "Use Activities"),
    (44, "Create Events"),
    (33, "Manage Events"),
    (35, "Create Public Threads"),
    (36, "Create Private Threads"),
    (38, "Send Messages in Threads"),
    (34, "Manage Threads"),
    (40, "Timeout Members"),
    (48, "Set Voice Channel Status"),
    (49, "Create Polls"),
    (46, "Send Voice Messages"),
    (50, "Use External Apps"),
    (51, "Pin Messages"),
    (52, "Bypass Slowmode"),
];

/// A piece of a sentence: plain, in bold, or a time in bold, which the
/// screen words as "Today at 16:20".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span<'a> {
    Text(&'a str),
    Bold(&'a str),
    /// Unix seconds.
    Time(u64),
}

/// Marks bold in [`Words`], and a time's start and end.
const BOLD: char = '\u{1}';
const TIME: char = '\u{2}';

/// A sentence of Audit Log's, held as one string: a row holds only this,
/// not a string for each of its pieces.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Words(String);

impl Words {
    fn new(pieces: Vec<Piece>) -> Self {
        let mut words = String::new();
        // The marks can't come from Discord's names.
        let push = |words: &mut String, text: &str| {
            words.extend(text.chars().filter(|c| *c != BOLD && *c != TIME));
        };
        for piece in pieces {
            match piece {
                Piece::Text(text) => push(&mut words, &text),
                Piece::Bold(text) => {
                    words.push(BOLD);
                    push(&mut words, &text);
                    words.push(BOLD);
                }
                Piece::Time(seconds) => {
                    words.push(TIME);
                    words.push_str(&seconds.to_string());
                    words.push(TIME);
                }
            }
        }
        words.shrink_to_fit();
        Self(words)
    }

    /// The sentence's pieces, in order.
    pub fn spans(&self) -> impl Iterator<Item = Span<'_>> {
        let mut rest = self.0.as_str();
        std::iter::from_fn(move || {
            let mut chars = rest.chars();
            let mark = chars.next()?;
            let (inside, after) = if mark == BOLD || mark == TIME {
                let inside = chars.as_str();
                let end = inside.find(mark).unwrap_or(inside.len());
                (&inside[..end], inside.get(end + 1..).unwrap_or_default())
            } else {
                let end = rest.find([BOLD, TIME]).unwrap_or(rest.len());
                (&rest[..end], &rest[end..])
            };
            rest = after;
            Some(match mark {
                BOLD => Span::Bold(inside),
                TIME => Span::Time(inside.parse().unwrap_or_default()),
                _ => Span::Text(inside),
            })
        })
    }
}

/// A piece of a sentence as it's made.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Text(String),
    Bold(String),
    Time(u64),
}

/// Whether an action made, changed or removed something, which colours
/// its lines' numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditKind {
    Create,
    Update,
    Delete,
}

/// A line a row's arrow opens, and the items under it, such as the
/// permissions a role was granted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditLine {
    pub words: Words,
    pub items: Vec<String>,
}

/// A row of Audit Log: an entry, or several close together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditRow {
    /// Its oldest entry's id, from which an older page starts.
    pub id: u64,
    /// Who did it, in [`AuditLogPage::actors`]; none for AutoMod, or
    /// someone Discord didn't name.
    pub actor: Option<u32>,
    pub title: Words,
    /// When its first and last entries were, in Unix seconds.
    pub start: u64,
    pub end: u64,
    pub kind: AuditKind,
    pub lines: Vec<AuditLine>,
}

/// Something done on Audit Log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditInput {
    /// Shows only what a user did, by id, or everyone's.
    User(Option<String>),
    /// Shows only an action, by its number, or all.
    Action(Option<u16>),
    /// Fetches older entries.
    More,
    /// Opens a row's lines, by its place in the list, or closes them.
    Toggle(usize),
}

/// Audit Log, as fetched so far.
#[derive(Clone, Debug, Default)]
pub struct AuditLogPage {
    pub rows: Vec<AuditRow>,
    /// Discord may have older entries.
    pub older: bool,
    /// What Discord is asked for.
    pub asking: Option<AuditQuery>,
    pub user: Option<String>,
    pub action: Option<u16>,
    /// Who the fetched entries name as having done something, for the
    /// user filter, by id.
    pub actors: Vec<(String, AuditUser)>,
    /// The row whose lines show, by its place in the list.
    pub open: Option<usize>,
    /// Discord didn't send the newest entries.
    pub failed: bool,
    /// The names of things the fetched entries deleted, by id.
    deleted: HashMap<String, String>,
}

impl AuditLogPage {
    /// Waiting for Discord.
    pub fn fetching(&self) -> bool {
        self.asking.is_some()
    }

    /// Discord sent `log`, for `query`: the first page, or older entries.
    pub(super) fn take(&mut self, query: &AuditQuery, log: AuditLog, known: &Known) {
        self.asking = None;
        self.failed = false;
        self.older = log.entries.len() >= AUDIT_PAGE;
        if query.before.is_none() {
            self.rows.clear();
            self.open = None;
        }
        let AuditLog {
            entries,
            users,
            names,
        } = log;
        for id in entries.iter().filter_map(|entry| entry.user.as_ref()) {
            if let Some(user) = users.get(id)
                && !self.actors.iter().any(|(known, _)| known == id)
            {
                self.actors.push((id.clone(), user.clone()));
            }
        }
        for entry in &entries {
            if let (AuditKind::Delete, Some(target)) = (kind(entry.action), &entry.target)
                && let Some(name) = deleted_name(entry)
            {
                self.deleted.insert(target.clone(), name);
            }
        }
        let render = Render {
            users: &users,
            actors: &self.actors,
            names: &names,
            deleted: &self.deleted,
            known,
        };
        let rows: Vec<AuditRow> = group(entries)
            .iter()
            .rev()
            .filter_map(|group| render.row(group))
            .collect();
        self.rows.extend(rows);
    }

    /// What to ask Discord for to show `user`'s and `action`'s entries
    /// from the newest, or `None` while it's busy, as Discord's client
    /// ignores a filter then.
    pub(super) fn filter(
        &mut self,
        user: Option<String>,
        action: Option<u16>,
    ) -> Option<AuditQuery> {
        if self.fetching() || (user == self.user && action == self.action) {
            return None;
        }
        self.user = user;
        self.action = action;
        self.rows.clear();
        self.open = None;
        self.older = false;
        self.ask(None)
    }

    /// What to ask Discord for to show older entries.
    pub(super) fn more(&mut self) -> Option<AuditQuery> {
        if self.fetching() || !self.older {
            return None;
        }
        let before = self.rows.last()?.id.to_string();
        self.ask(Some(before))
    }

    fn ask(&mut self, before: Option<String>) -> Option<AuditQuery> {
        let query = AuditQuery {
            before,
            user: self.user.clone(),
            action: self.action,
        };
        self.asking = Some(query.clone());
        Some(query)
    }

    /// Opens a row's lines, or closes them if they're open; one row's
    /// show at a time.
    pub(super) fn toggle(&mut self, row: usize) {
        if self.open == Some(row) {
            self.open = None;
        } else if self.rows.get(row).is_some_and(|row| !row.lines.is_empty()) {
            self.open = Some(row);
        }
    }

    /// Discord didn't send what was asked.
    pub(super) fn refused(&mut self) {
        let Some(query) = self.asking.take() else {
            return;
        };
        // Older entries that fail only stop loading, as in Discord's
        // client.
        if query.before.is_none() {
            self.failed = true;
            self.rows.clear();
        }
    }
}

/// What the open server knows, for naming the channels, roles and emoji
/// entries refer to.
pub(super) struct Known<'a> {
    pub app: &'a Application,
    pub server: &'a str,
}

impl Known<'_> {
    fn guild(&self) -> String {
        self.app
            .state
            .servers
            .iter()
            .find(|known| known.id == self.server)
            .map(|known| known.name.clone())
            .unwrap_or_default()
    }

    /// A channel as Discord's log names it: `#name` for one that holds
    /// messages, `name` for a voice channel, category or thread.
    fn channel(&self, id: &str) -> Option<String> {
        #[cfg(feature = "discord")]
        if let Some((server, _, _)) = self.app.discord_place(self.server) {
            if let Some(channel) = server.text_channels.iter().find(|channel| channel.id == id) {
                return Some(format!("#{}", channel.name));
            }
            return server
                .voice_channels
                .iter()
                .chain(&server.threads)
                .find(|channel| channel.id == id)
                .map(|channel| channel.name.clone())
                .or_else(|| {
                    server
                        .categories
                        .get(id)
                        .map(|category| category.name.clone())
                });
        }
        let _ = id;
        None
    }

    fn role(&self, id: &str) -> Option<String> {
        #[cfg(feature = "discord")]
        if let Some((server, _, _)) = self.app.discord_place(self.server) {
            return server.role_names.get(id).cloned();
        }
        let _ = id;
        None
    }

    fn emoji(&self, id: &str) -> Option<String> {
        #[cfg(feature = "discord")]
        if let Some((server, _, _)) = self.app.discord_place(self.server) {
            return server
                .emojis
                .iter()
                .find(|emoji| emoji.id == id)
                .map(|emoji| emoji.name.clone());
        }
        let _ = id;
        None
    }

    /// The server's id, as Discord has it.
    fn guild_id(&self) -> &str {
        discord::parse_server(self.server).unwrap_or_default()
    }
}

/// Entries shown as one row: the oldest, when the newest was, and all
/// their changes, oldest first, each entry's reason before its changes.
struct Group {
    entry: AuditEntry,
    end: u64,
    merged: usize,
    changes: Vec<AuditChange>,
}

/// When a snowflake was made, in Unix seconds.
fn seconds(id: &str) -> u64 {
    id.parse::<u64>()
        .map_or(0, |id| ((id >> 22) + DISCORD_EPOCH) / 1000)
}

/// Puts a page's entries, newest first, into rows, oldest first, as
/// Discord's client groups them.
fn group(entries: Vec<AuditEntry>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for mut entry in entries.into_iter().rev() {
        let mut changes = Vec::with_capacity(entry.changes.len() + 1);
        if let Some(reason) = entry.reason.take() {
            changes.push(change("reason", Value::String(reason)));
        }
        changes.append(&mut entry.changes);
        let option = |key: &str| entry.options[key].clone();
        match entry.action {
            21 => {
                let days = match option("delete_member_days") {
                    Value::Null => Value::from(1),
                    days => days,
                };
                changes.push(change("prune_delete_days", days));
            }
            143 => changes.push(change(
                "triggered_rule_name",
                option("auto_moderation_rule_name"),
            )),
            192 => changes.push(change("status", option("status"))),
            _ => {}
        }
        let time = seconds(&entry.id);
        if let Some(last) = groups.last_mut()
            && groupable(entry.action)
            && last.entry.action == entry.action
            && last.entry.target == entry.target
            && last.entry.user == entry.user
            && last.entry.options == entry.options
            && time.saturating_sub(seconds(&last.entry.id)) < GROUP_SECONDS
            && last.merged < GROUP_MOST
        {
            last.end = time;
            last.merged += 1;
            last.changes.append(&mut changes);
            continue;
        }
        groups.push(Group {
            entry,
            end: time,
            merged: 0,
            changes,
        });
    }
    groups
}

fn change(key: &str, new: Value) -> AuditChange {
    AuditChange {
        key: key.to_owned(),
        old: Value::Null,
        new,
    }
}

/// Whether entries of an action may share a row.
fn groupable(action: u16) -> bool {
    !matches!(
        action,
        40..=42 | 72..=75 | 26 | 27 | 16 | 33 | 28 | 121 | 21
    )
}

/// What an action did, as Discord's client colours it.
fn kind(action: u16) -> AuditKind {
    match action {
        10 | 13 | 23 | 28 | 30 | 40 | 50 | 60 | 74 | 80 | 83 | 90 | 100 | 110 | 130 | 140 | 163
        | 166 | 171 | 190 | 192 | 200 => AuditKind::Create,
        12 | 15 | 20 | 21 | 22 | 27 | 32 | 42 | 52 | 62 | 72 | 73 | 75 | 82 | 85 | 92 | 102
        | 112 | 132 | 142 | 143 | 165 | 172 | 193 | 202 => AuditKind::Delete,
        _ => AuditKind::Update,
    }
}

/// What kind of thing an action is done to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Guild,
    Channel,
    Overwrite,
    User,
    Role,
    Invite,
    Webhook,
    Emoji,
    Sticker,
    Integration,
    Stage,
    Event,
    EventException,
    Thread,
    Command,
    Soundboard,
    AutoMod,
    Prompt,
    Onboarding,
    Home,
    HomeSettings,
    VoiceStatus,
    Verification,
    Profile,
}

fn target(action: u16) -> Target {
    match action {
        10..=12 | 16 | 73 => Target::Channel,
        13..=15 => Target::Overwrite,
        20..=28 | 72 | 74 | 75 | 143..=146 => Target::User,
        30..=33 => Target::Role,
        40..=42 => Target::Invite,
        50..=52 => Target::Webhook,
        60..=62 => Target::Emoji,
        80..=82 => Target::Integration,
        83..=85 => Target::Stage,
        90..=92 => Target::Sticker,
        100..=102 => Target::Event,
        110..=112 => Target::Thread,
        121 => Target::Command,
        130..=132 => Target::Soundboard,
        140..=142 => Target::AutoMod,
        163..=165 => Target::Prompt,
        166 | 167 => Target::Onboarding,
        171 | 172 => Target::Home,
        190 | 191 => Target::HomeSettings,
        192 | 193 => Target::VoiceStatus,
        200..=202 => Target::EventException,
        210 => Target::Verification,
        211 => Target::Profile,
        _ => Target::Guild,
    }
}

/// The name a deleting entry gives what it deleted, `#name` for a
/// channel that held messages.
fn deleted_name(entry: &AuditEntry) -> Option<String> {
    let old = |key: &str| {
        entry
            .changes
            .iter()
            .find(|change| change.key == key)
            .map(|change| &change.old)
    };
    let name = old("name").or_else(|| old("title"))?.as_str()?;
    let texty = old("type")
        .and_then(Value::as_u64)
        .is_some_and(|kind| matches!(kind, 0 | 5 | 15 | 16));
    Some(if target(entry.action) == Target::Channel && texty {
        format!("#{name}")
    } else {
        name.to_owned()
    })
}

/// A value as a template shows it.
fn shown(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        other => Some(other.to_string()),
    }
}

fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(on) => *on,
        Value::Number(number) => number.as_f64() != Some(0.0),
        Value::String(text) => !text.is_empty(),
        _ => true,
    }
}

/// An ISO time as Unix seconds.
fn iso_seconds(value: &Value) -> Option<u64> {
    let time = chrono::DateTime::parse_from_rfc3339(value.as_str()?).ok()?;
    u64::try_from(time.timestamp()).ok()
}

fn t(text: impl Into<String>) -> Piece {
    Piece::Text(text.into())
}

fn b(text: impl Into<String>) -> Piece {
    Piece::Bold(text.into())
}

/// The names of the permissions in `bits`, in Discord's order: a channel's
/// say "Manage Channel" and "View Channel".
fn permission_names(bits: u64, channel: bool) -> Vec<String> {
    PERMISSION_NAMES
        .iter()
        .filter(|(bit, _)| bits & (1 << bit) != 0)
        .map(|&(bit, name)| match (bit, channel) {
            (4, true) => tr!("Manage Channel").to_owned(),
            (10, true) => tr!("View Channel").to_owned(),
            _ => yapfast_lang::text(name).to_owned(),
        })
        .collect()
}

/// A line without items.
fn line(pieces: Vec<Piece>) -> AuditLine {
    AuditLine {
        words: Words::new(pieces),
        items: vec![],
    }
}

/// Words a page's rows.
struct Render<'a> {
    users: &'a HashMap<String, AuditUser>,
    actors: &'a [(String, AuditUser)],
    names: &'a HashMap<String, String>,
    deleted: &'a HashMap<String, String>,
    known: &'a Known<'a>,
}

impl Render<'_> {
    fn row(&self, group: &Group) -> Option<AuditRow> {
        let entry = &group.entry;
        let kept = matches!(entry.action, 16 | 21 | 26 | 27 | 33 | 150 | 151);
        let target = self.target(entry, &group.changes);
        if target.is_none() && !kept {
            return None;
        }
        let actor = entry.user.as_ref().and_then(|id| self.users.get(id));
        let actor_name = match (actor, entry.options["integration_type"].as_str()) {
            (Some(user), _) => user.name.clone(),
            (None, Some("twitch")) => "Twitch".to_owned(),
            (None, Some("youtube")) => "YouTube".to_owned(),
            (None, Some(_)) => tr!("Unknown Integration").to_owned(),
            (None, None) => tr!("Unknown User").to_owned(),
        };
        // AutoMod's rows show its avatar in Discord's client, not the
        // user's; Yapfast has none to show.
        let automod = matches!(entry.action, 143..=146);
        Some(AuditRow {
            id: entry.id.parse().unwrap_or_default(),
            actor: entry
                .user
                .as_ref()
                .filter(|_| !automod)
                .and_then(|id| self.actors.iter().position(|(known, _)| known == id))
                .and_then(|index| u32::try_from(index).ok()),
            title: Words::new(self.title(
                entry,
                actor_name,
                target.unwrap_or_default(),
                &group.changes,
            )),
            start: seconds(&entry.id),
            end: group.end,
            kind: kind(entry.action),
            lines: self.lines(entry, &group.changes),
        })
    }

    /// What an entry was done to, as Discord's client names it.
    fn target(&self, entry: &AuditEntry, changes: &[AuditChange]) -> Option<String> {
        let id = entry.target.as_deref();
        let changed = |key: &str| {
            changes
                .iter()
                .filter(|change| change.key == key)
                .find_map(|change| shown(&change.new).or_else(|| shown(&change.old)))
                .filter(|text| !text.is_empty())
        };
        let named = |key: &str| {
            let id = id?;
            self.deleted
                .get(id)
                .cloned()
                .or_else(|| changed(key))
                .or_else(|| Some(id.to_owned()))
        };
        let listed = |key: &str| {
            id.and_then(|id| self.names.get(id).cloned())
                .or_else(|| named(key))
        };
        match target(entry.action) {
            Target::Guild
            | Target::Onboarding
            | Target::Home
            | Target::Verification
            | Target::Profile => Some(self.known.guild()),
            Target::HomeSettings => Some(tr!("Server Guide").to_owned()),
            Target::Channel | Target::Overwrite => id
                .and_then(|id| self.known.channel(id))
                .or_else(|| named("name")),
            Target::VoiceStatus => id
                .and_then(|id| self.known.channel(id))
                .or_else(|| named("status")),
            Target::User => id
                .and_then(|id| self.users.get(id))
                .map(|user| user.name.clone())
                .or_else(|| named("nick")),
            Target::Role => id
                .and_then(|id| self.known.role(id))
                .or_else(|| named("name")),
            Target::Emoji => id
                .and_then(|id| self.known.emoji(id))
                .or_else(|| named("name")),
            Target::Invite => changed("code").or_else(|| id.map(str::to_owned)),
            Target::Integration => listed("type"),
            Target::Webhook
            | Target::Thread
            | Target::Event
            | Target::EventException
            | Target::AutoMod
            | Target::Command => listed("name"),
            Target::Sticker | Target::Soundboard => named("name"),
            Target::Stage => named("topic"),
            Target::Prompt => changed("title").or_else(|| id.map(|_| tr!("[empty]").to_owned())),
        }
    }

    /// A channel an entry's options name.
    fn channel(&self, entry: &AuditEntry) -> String {
        let id = entry.options["channel_id"].as_str().unwrap_or_default();
        self.known
            .channel(id)
            .or_else(|| self.deleted.get(id).cloned())
            .unwrap_or_else(|| id.to_owned())
    }

    fn user(&self, id: &str) -> Option<String> {
        self.users.get(id).map(|user| user.name.clone())
    }

    /// Discord's sentence for an entry.
    fn title(
        &self,
        entry: &AuditEntry,
        actor: String,
        target: String,
        changes: &[AuditChange],
    ) -> Vec<Piece> {
        let options = &entry.options;
        let count = number(&options["count"]).unwrap_or_default();
        let did = |verb: &str| vec![b(actor.clone()), t(format!(" {verb} ")), b(target.clone())];
        let only = |what: &str| vec![b(actor.clone()), t(format!(" {what}"))];
        let new_type = || {
            changes
                .iter()
                .find(|change| change.key == "type")
                .and_then(|change| change.new.as_u64())
        };
        let in_channel = |mut spans: Vec<Piece>| {
            spans.extend([t(tr!(" in ")), b(self.channel(entry))]);
            spans
        };
        match entry.action {
            1 | 11 => did(tr!("made changes to")),
            10 => did(match new_type() {
                Some(13) => tr!("created a stage channel"),
                Some(2) => tr!("created a voice channel"),
                Some(4) => tr!("created a category"),
                Some(15) => tr!("created a forum channel"),
                Some(16) => tr!("created a media channel"),
                Some(5) => tr!("created an announcement channel"),
                _ => tr!("created a text channel"),
            }),
            12 => did(tr!("removed")),
            13 => did(tr!("created channel overrides for")),
            14 => did(tr!("updated channel overrides for")),
            15 => did(tr!("removed channel overrides for")),
            16 => vec![
                b(actor),
                t(tr!(" moved ")),
                b(trn!("a channel", "{n} channels", count)),
            ],
            20 => did(tr!("kicked")),
            21 => {
                let removed = number(&options["members_removed"])
                    .filter(|removed| *removed != 0)
                    .unwrap_or(count);
                vec![
                    b(actor),
                    t(tr!(" pruned ")),
                    b(trn!("a member", "{n} members", removed)),
                ]
            }
            22 => did(tr!("banned")),
            23 => did(tr!("removed the ban for")),
            24 => did(tr!("updated")),
            25 => did(tr!("updated roles for")),
            26 | 27 => self.moved(entry, actor, count),
            28 => {
                let mut spans = did(tr!("added"));
                spans.push(t(tr!(" to the server")));
                spans
            }
            30 => did(tr!("created the role")),
            31 => did(tr!("updated the role")),
            32 => did(tr!("deleted the role")),
            33 => vec![
                b(actor),
                t(tr!(" moved ")),
                b(trn!("a role", "{n} roles", count)),
            ],
            40 => did(tr!("created an invite")),
            41 => did(tr!("updated an invite")),
            42 => did(tr!("deleted an invite")),
            50 => did(tr!("created the webhook")),
            51 => did(tr!("updated the webhook")),
            52 => did(tr!("deleted the webhook")),
            60 => did(tr!("created the emoji")),
            61 => did(tr!("updated the emoji")),
            62 => did(tr!("deleted the emoji")),
            72 => in_channel(vec![
                b(actor),
                t(tr!(" deleted ")),
                b(trn!("a message", "{n} messages", count)),
                t(tr!(" by ")),
                b(target),
            ]),
            73 => vec![
                b(actor),
                t(tr!(" deleted ")),
                b(trn!("a message", "{n} messages", count)),
                t(tr!(" in ")),
                b(target),
            ],
            74 => in_channel(did(tr!("pinned a message by"))),
            75 => in_channel(did(tr!("unpinned a message by"))),
            80 => did(tr!("added an integration for")),
            81 => did(tr!("updated the integration for")),
            82 => did(tr!("deleted the integration for")),
            83 => vec![
                b(actor),
                t(tr!(" started the stage for ")),
                b(self.channel(entry)),
            ],
            84 => vec![
                b(actor),
                t(tr!(" updated the stage for ")),
                b(self.channel(entry)),
            ],
            85 if entry.user.is_none() => vec![
                t(tr!("Discord ended the stage for ")),
                b(self.channel(entry)),
                t(tr!(" due to inactivity.")),
            ],
            85 => vec![
                b(actor),
                t(tr!(" ended the stage for ")),
                b(self.channel(entry)),
            ],
            90 => did(tr!("created the sticker")),
            91 => did(tr!("updated the sticker")),
            92 => did(tr!("deleted the sticker")),
            100 => did(tr!("scheduled the event")),
            101 => did(tr!("updated the scheduled event")),
            102 => did(tr!("canceled the scheduled event")),
            110 => did(match new_type() {
                Some(12) => tr!("created a private thread"),
                Some(10) => tr!("created an announcement thread"),
                _ => tr!("created a thread"),
            }),
            111 => did(tr!("made changes to the thread")),
            112 => did(tr!("deleted the thread")),
            121 => did(tr!("updated permissions for")),
            130 => only(tr!("uploaded a Soundboard sound")),
            131 => only(tr!("updated a Soundboard sound")),
            132 => only(tr!("deleted a Soundboard sound")),
            140 => did(tr!("created AutoMod rule")),
            141 => did(tr!("updated AutoMod rule")),
            142 => did(tr!("deleted AutoMod rule")),
            143 => in_channel(vec![
                t(tr!("AutoMod blocked a message sent by ")),
                b(target),
            ]),
            144 if options["auto_moderation_rule_trigger_type"].as_str() == Some("6") => vec![
                t(tr!("AutoMod flagged ")),
                b(actor),
                t(tr!(" for violating content in their user profile")),
            ],
            144 => in_channel(vec![
                t(tr!("AutoMod flagged a message sent by ")),
                b(target),
            ]),
            145 => vec![
                t(tr!("AutoMod timed out ")),
                b(target),
                t(tr!(" for a message posted in ")),
                b(self.channel(entry)),
            ],
            146 => vec![
                t(tr!("AutoMod quarantined ")),
                b(actor),
                t(tr!(" for violating content in their user profile")),
            ],
            150 => only(tr!("applied for monetization")),
            151 => only(tr!("accepted terms for monetization")),
            163 => only(tr!("created a new customization question")),
            164 => did(tr!("updated the customization question")),
            165 => did(tr!("deleted the customization question")),
            166 => only(tr!("started onboarding for this server")),
            167 => only(tr!("updated onboarding for this server")),
            171 => only(
                match changes
                    .iter()
                    .find(|change| change.key == "entity_type")
                    .and_then(|change| change.new.as_str())
                {
                    Some("message") => tr!("featured a message on Home"),
                    Some("forum_post") => tr!("featured a forum post on Home"),
                    _ => tr!("featured something on Home"),
                },
            ),
            172 => only(tr!("removed an item from Home")),
            // Discord's client has no sentence for a blocked harmful link.
            180 => vec![],
            190 => only(tr!("created the Server Guide for this server")),
            191 => only(tr!("updated the Server Guide for this server")),
            192 => vec![
                b(actor),
                t(tr!(" set a Voice Channel Status in ")),
                b(self.channel(entry)),
            ],
            193 => vec![
                b(actor),
                t(tr!(" removed a Voice Channel Status in ")),
                b(self.channel(entry)),
            ],
            200..=202 => {
                let verb = if entry.action == 202 {
                    tr!("reset an event recurrence for")
                } else {
                    tr!("updated an event recurrence for")
                };
                let day = options["event_exception_id"]
                    .as_str()
                    .map(|id| crate::clock::day(seconds(id)))
                    .unwrap_or_default();
                let mut spans = did(verb);
                spans.extend([t(tr!(" at ")), b(day)]);
                spans
            }
            210 => only(tr!("updated member application settings")),
            211 => only(tr!("updated server profile settings")),
            212 => only(tr!("updated Pin Messages permission")),
            213 => only(tr!("updated Bypass Slowmode permission")),
            _ => only(tr!("made changes")),
        }
    }

    /// Discord's sentence for moving or disconnecting members from voice.
    fn moved(&self, entry: &AuditEntry, actor: String, count: u64) -> Vec<Piece> {
        let options = &entry.options;
        let moving = entry.action == 26;
        let unique = number(&options["unique_count"]).filter(|unique| *unique >= 1);
        let member = options["member_id"].as_str().and_then(|id| self.user(id));
        let who = match (unique, options["unique_overflow"].as_str()) {
            (None, _) => None,
            (Some(unique), Some("1")) => Some(tr!("{}+ users", unique)),
            (Some(1), _) if member.is_some() => member,
            (Some(unique), _) => Some(trn!("{n} user", "{n} users", unique)),
        };
        let mut spans = vec![
            b(actor),
            t(if moving {
                tr!(" moved ")
            } else {
                tr!(" disconnected ")
            }),
            b(who
                .clone()
                .unwrap_or_else(|| trn!("a user", "{n} users", count))),
        ];
        if moving {
            spans.extend([t(tr!(" to ")), b(self.channel(entry))]);
        } else {
            spans.push(t(tr!(" from voice")));
        }
        if who.is_some() {
            spans.extend([t(" ("), b(trn!("{n} action", "{n} actions", count)), t(")")]);
        }
        spans
    }

    /// The lines an entry's arrow opens, numbered as they show.
    fn lines(&self, entry: &AuditEntry, changes: &[AuditChange]) -> Vec<AuditLine> {
        let target = target(entry.action);
        // A deletion shows only its reason, but for a kick, prune or ban.
        let reason_only =
            kind(entry.action) == AuditKind::Delete && !matches!(entry.action, 20..=22);
        let mut lines = vec![];
        for change in changes {
            let key = change.key.as_str();
            if (reason_only && key != "reason") || hidden(target, key) {
                continue;
            }
            if key == "reason" && target != Target::Profile {
                if let Some(reason) = shown(&change.new) {
                    lines.push(line(vec![t(tr!("With reason ")), b(reason)]));
                }
                continue;
            }
            self.change(entry, target, change, &mut lines);
        }
        lines
    }

    /// A value as Discord's log shows it for its key.
    fn value(&self, key: &str, value: &Value) -> Option<String> {
        let channel = |id: &str| self.known.channel(id).unwrap_or_else(|| id.to_owned());
        Some(match key {
            "channel_id"
            | "afk_channel_id"
            | "system_channel_id"
            | "rules_channel_id"
            | "public_updates_channel_id" => channel(value.as_str()?),
            "owner_id" => {
                let id = value.as_str()?;
                self.user(id).unwrap_or_else(|| id.to_owned())
            }
            "afk_timeout" => (number(value)? / 60).to_string(),
            "bitrate" => (number(value)? / 1000).to_string(),
            "color" => format!("#{:06X}", number(value)?),
            "max_age" => match number(value)? {
                1800 => tr!("30 minutes").to_owned(),
                3600 => tr!("1 hour").to_owned(),
                21600 => tr!("6 hours").to_owned(),
                43200 => tr!("12 hours").to_owned(),
                86400 => tr!("1 day").to_owned(),
                604_800 => tr!("7 days").to_owned(),
                1_209_600 => tr!("14 days").to_owned(),
                2_592_000 => tr!("30 days").to_owned(),
                5_184_000 => tr!("60 days").to_owned(),
                0 => tr!("Never").to_owned(),
                other => other.to_string(),
            },
            "video_quality_mode" => match number(value)? {
                2 => "720p".to_owned(),
                _ => tr!("Auto").to_owned(),
            },
            "type" => match number(value)? {
                0 => tr!("Text Channel").to_owned(),
                2 => tr!("Voice Channel").to_owned(),
                4 => tr!("Category").to_owned(),
                5 => tr!("Announcement Channel").to_owned(),
                13 => tr!("Stage Channel").to_owned(),
                15 => tr!("Forum Channel").to_owned(),
                16 => tr!("Media Channel").to_owned(),
                other => other.to_string(),
            },
            _ => shown(value)?,
        })
    }

    /// The lines one change gives, in Discord's words for its target.
    fn change(
        &self,
        entry: &AuditEntry,
        target: Target,
        change: &AuditChange,
        lines: &mut Vec<AuditLine>,
    ) {
        let key = change.key.as_str();
        let (old, new) = (&change.old, &change.new);
        // Splits first: a change of bits gives a line for each way.
        match (target, key) {
            (Target::Role, "permissions") => {
                let (was, now) = (number(old).unwrap_or(0), number(new).unwrap_or(0));
                for (bits, what) in [(now & !was, tr!("Granted")), (was & !now, tr!("Denied"))] {
                    if bits == 0 {
                        continue;
                    }
                    let items = permission_names(bits, false);
                    lines.push(AuditLine {
                        words: Words::new(vec![
                            b(what),
                            t(format!(
                                " {}",
                                trn!("permission", "permissions", items.len() as u64)
                            )),
                        ]),
                        items,
                    });
                }
                return;
            }
            (Target::Channel | Target::Overwrite, "allow" | "deny") => {
                let subtarget = self.subtarget(entry);
                let (was, now) = (number(old).unwrap_or(0), number(new).unwrap_or(0));
                let set = if key == "allow" {
                    tr!("Granted")
                } else {
                    tr!("Denied")
                };
                for (bits, what) in [(now & !was, set), (was & !now, tr!("Reset"))] {
                    if bits == 0 {
                        continue;
                    }
                    let items = permission_names(bits, true);
                    lines.push(AuditLine {
                        words: Words::new(vec![
                            b(what),
                            t(tr!(
                                " {} for ",
                                trn!("a permission", "permissions", items.len() as u64)
                            )),
                            b(subtarget.clone()),
                        ]),
                        items,
                    });
                }
                return;
            }
            (Target::Guild, "system_channel_flags") => {
                let (was, now) = (number(old).unwrap_or(0), number(new).unwrap_or(0));
                for (bit, what) in [
                    (1, tr!("welcome messages")),
                    (2, tr!("boost notifications")),
                    (4, tr!("server setup tips")),
                    (8, tr!("welcome stickers")),
                ] {
                    if (was ^ now) & bit != 0 {
                        lines.push(line(vec![
                            t(tr!("Set system channel {} to ", what)),
                            b(if now & bit == 0 {
                                tr!("true")
                            } else {
                                tr!("false")
                            }),
                        ]));
                    }
                }
                return;
            }
            (Target::Channel | Target::Thread, "flags") => {
                let (was, now) = (number(old).unwrap_or(0), number(new).unwrap_or(0));
                let flags = [
                    (
                        1,
                        tr!("Removed channel from Home"),
                        tr!("Added channel back to Home"),
                    ),
                    (2, tr!("Pinned"), tr!("Unpinned")),
                    (
                        4,
                        tr!("Removed channel from Active Now"),
                        tr!("Added channel back to Active Now"),
                    ),
                ];
                let removed = flags
                    .iter()
                    .filter(|(bit, _, _)| was & !now & bit != 0)
                    .map(|(_, _, removed)| removed.to_string());
                let added = flags
                    .iter()
                    .filter(|(bit, _, _)| now & !was & bit != 0)
                    .map(|(_, added, _)| added.to_string());
                let items: Vec<String> = removed.chain(added).collect();
                if !items.is_empty() {
                    lines.push(AuditLine {
                        words: Words::new(vec![t(if target == Target::Thread {
                            tr!("Updated thread properties")
                        } else {
                            tr!("Changed channel settings")
                        })]),
                        items,
                    });
                }
                return;
            }
            (Target::User, "$add" | "$remove") => {
                let items: Vec<String> = new
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|role| role["name"].as_str().map(str::to_owned))
                    .collect();
                let what = if key == "$add" {
                    tr!("Added")
                } else {
                    tr!("Removed")
                };
                lines.push(AuditLine {
                    words: Words::new(vec![
                        b(what),
                        t(format!(
                            " {}",
                            if items.len() == 1 {
                                tr!("a role")
                            } else {
                                tr!("some roles")
                            }
                        )),
                    ]),
                    items,
                });
                return;
            }
            (Target::Invite, "role_ids") => {
                let items: Vec<String> = new
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|id| self.known.role(id.as_str()?))
                    .collect();
                lines.push(AuditLine {
                    words: Words::new(vec![
                        t(tr!("With ")),
                        b(if items.len() == 1 {
                            tr!("a role")
                        } else {
                            tr!("some roles")
                        }),
                    ]),
                    items,
                });
                return;
            }
            (Target::Channel, "available_tags") => {
                if let Some(spans) = tag_change(old, new) {
                    lines.push(line(spans));
                }
                return;
            }
            (Target::Command, _) => {
                if let Some(spans) = self.command_permission(change) {
                    lines.push(line(spans));
                }
                return;
            }
            _ => {}
        }
        let old_text = self.value(key, old);
        let new_text = self.value(key, new);
        if let Some(spans) = self.template(target, key, change, old_text, new_text) {
            lines.push(line(spans));
        }
    }

    /// Who a channel's permission override is for: a member's name, or a
    /// role's.
    fn subtarget(&self, entry: &AuditEntry) -> String {
        let options = &entry.options;
        let id = options["id"].as_str().unwrap_or_default();
        match options["type"].as_str() {
            Some("1") => self.user(id).unwrap_or_else(|| id.to_owned()),
            _ => options["role_name"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| self.known.role(id))
                .unwrap_or_else(|| id.to_owned()),
        }
    }

    /// An app command's permission for a role, member or channel.
    fn command_permission(&self, change: &AuditChange) -> Option<Vec<Piece>> {
        let subject = if change.new.is_null() {
            &change.old
        } else {
            &change.new
        };
        let id = subject["id"].as_str()?;
        let name = match subject["type"].as_u64()? {
            1 => self.known.role(id),
            2 => self.user(id),
            _ => {
                let all = self
                    .known
                    .guild_id()
                    .parse::<u64>()
                    .ok()
                    .map(|guild| (guild - 1).to_string());
                if all.as_deref() == Some(id) {
                    Some(tr!("all channels").to_owned())
                } else {
                    self.known.channel(id)
                }
            }
        }
        .unwrap_or_else(|| id.to_owned());
        Some(match change.new["permission"].as_bool() {
            Some(true) => vec![b(tr!("Granted")), t(tr!(" permission for ")), b(name)],
            Some(false) => vec![b(tr!("Denied")), t(tr!(" permission for ")), b(name)],
            None => vec![b(tr!("Removed override")), t(tr!(" for ")), b(name)],
        })
    }

    /// The line for a change, as Discord's templates for its target word
    /// it; none for a change they don't show.
    fn template(
        &self,
        target: Target,
        key: &str,
        change: &AuditChange,
        old: Option<String>,
        new: Option<String>,
    ) -> Option<Vec<Piece>> {
        let (old_value, new_value) = (&change.old, &change.new);
        let on = truthy(new_value);
        let level = number(new_value);
        let new_text = || new.clone().unwrap_or_default();
        // "Set the name to X".
        let set = |text: &str| Some(vec![t(format!("{text} ")), b(new_text())]);
        // "Changed the name from A to B".
        let from_to = |text: &str| {
            Some(vec![
                t(tr!("{} from ", text)),
                b(old.clone().unwrap_or_default()),
                t(tr!(" to ")),
                b(new_text()),
            ])
        };
        // The first when there was no old value, else the second.
        let made = |first: &str, then: &str| {
            if old_value.is_null() {
                set(first)
            } else {
                from_to(then)
            }
        };
        let either = |yes: &str, no: &str| Some(vec![t(if on { yes } else { no })]);
        let bold_first = |word: &str, rest: &str| Some(vec![b(word), t(rest)]);
        let seconds_plural = |zero: &str, one_other: &str| {
            let count = level.unwrap_or(0);
            if count == 0 {
                Some(vec![t(zero)])
            } else {
                Some(vec![
                    t(format!("{one_other} ")),
                    b(trn!("{n} second", "{n} seconds", count)),
                ])
            }
        };
        let minutes_plural = |zero: &str, one_other: &str| {
            let count = level.unwrap_or(0);
            if count == 0 {
                Some(vec![t(zero)])
            } else {
                Some(vec![
                    t(format!("{one_other} ")),
                    b(trn!("{n} minute", "{n} minutes", count)),
                ])
            }
        };
        let both = !old_value.is_null() && !new_value.is_null();
        let time = |text: &str| {
            let seconds = iso_seconds(new_value)?;
            Some(vec![t(format!("{text} ")), Piece::Time(seconds)])
        };
        let names = |list: &Value, role: bool| -> String {
            let names: Vec<String> = list
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|id| {
                    let id = id.as_str()?;
                    Some(
                        if role {
                            self.known.role(id)
                        } else {
                            self.known.channel(id)
                        }
                        .unwrap_or_else(|| id.to_owned()),
                    )
                })
                .collect();
            if names.is_empty() {
                tr!("None").to_owned()
            } else {
                names.join(", ")
            }
        };
        let quoted = |list: &Value| -> String {
            list.as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|word| format!("'{word}'"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match (target, key) {
            (Target::Guild, _) => match key {
                "name" => set(tr!("Set the server name to")),
                "description" if new.is_none() => {
                    bold_first(tr!("Cleared"), tr!(" the server description"))
                }
                "description" => set(tr!("Set the server description to")),
                "icon_hash" => Some(vec![t(tr!("Set the server icon"))]),
                "splash_hash" => Some(vec![t(tr!("Set the server invite background"))]),
                "discovery_splash_hash" => {
                    Some(vec![t(tr!("Set the Server Discovery background"))])
                }
                "banner_hash" if new.is_none() => Some(vec![t(tr!("Removed the server banner"))]),
                "banner_hash" => Some(vec![t(tr!("Set the server banner"))]),
                "owner_id" => set(tr!("Passed ownership to")),
                "region" => set(tr!("Set the voice region to")),
                "preferred_locale" => set(tr!("Set the preferred locale to")),
                "afk_channel_id" if new.is_none() => {
                    bold_first(tr!("Cleared"), tr!(" the inactive channel"))
                }
                "afk_channel_id" => set(tr!("Set the inactive channel to")),
                "afk_timeout" => Some(vec![
                    t(tr!("Set the inactive timeout to ")),
                    b(new_text()),
                    t(tr!(" minutes")),
                ]),
                "system_channel_id" if new.is_none() => {
                    bold_first(tr!("Disabled"), tr!(" the welcome notification messages"))
                }
                "system_channel_id" => set(tr!("Set the welcome notification channel to")),
                "rules_channel_id" if new.is_none() => {
                    bold_first(tr!("Cleared"), tr!(" the rules channel"))
                }
                "rules_channel_id" => set(tr!("Set the rules channel to")),
                "public_updates_channel_id" if new.is_none() => {
                    bold_first(tr!("Cleared"), tr!(" the Community Server updates channel"))
                }
                "public_updates_channel_id" => {
                    set(tr!("Set the Community Server updates channel to"))
                }
                "mfa_level" => match level? {
                    0 => bold_first(
                        tr!("Disabled"),
                        tr!(" two-factor authentication requirement"),
                    ),
                    1 => bold_first(
                        tr!("Enabled"),
                        tr!(" two-factor authentication requirement"),
                    ),
                    _ => None,
                },
                "widget_enabled" if on => bold_first(tr!("Enabled"), tr!(" the widget")),
                "widget_enabled" => bold_first(tr!("Disabled"), tr!(" the widget")),
                "widget_channel_id" if new.is_none() => {
                    bold_first(tr!("Removed"), tr!(" the widget channel"))
                }
                "widget_channel_id" => set(tr!("Set the widget channel to")),
                "verification_level" => Some(vec![
                    t(tr!("Set the server verification level to ")),
                    b(*[
                        tr!("None"),
                        tr!("Low"),
                        tr!("Medium"),
                        tr!("High"),
                        tr!("Very High"),
                    ]
                    .get(usize::try_from(level?).ok()?)?),
                ]),
                "default_message_notifications" => Some(vec![
                    t(tr!("Set the default message notification setting to ")),
                    b(*[tr!("All Messages"), tr!("Only Mentions")]
                        .get(usize::try_from(level?).ok()?)?),
                ]),
                "vanity_url_code" if new.is_none() => {
                    bold_first(tr!("Removed"), tr!(" the Custom Invite Link"))
                }
                "vanity_url_code" => set(tr!("Set the Custom Invite Link to")),
                "explicit_content_filter" => match level? {
                    0 => bold_first(tr!("Disabled"), tr!(" the explicit content filter")),
                    1 => Some(vec![
                        t(tr!(
                            "Set the explicit content filter to scan messages from "
                        )),
                        b(tr!("members without a role")),
                    ]),
                    2 => Some(vec![
                        t(tr!(
                            "Set the explicit content filter to scan messages from "
                        )),
                        b(tr!("all members")),
                    ]),
                    _ => None,
                },
                "premium_progress_bar_enabled" => Some(vec![
                    t(tr!("Turned ")),
                    b(if on { tr!("on") } else { tr!("off") }),
                    t(tr!(" the Boost progress bar")),
                ]),
                "triggered_rule_name" => set(tr!("Detected by rule")),
                _ => None,
            },
            (Target::Channel | Target::Overwrite, _) => match key {
                "name" => made(tr!("Set the name to"), tr!("Changed the name")),
                "position" => made(tr!("In position"), tr!("Moved")),
                "topic" if both => set(tr!("Changed the topic to")),
                "topic" if new.is_some() => set(tr!("Set the topic to")),
                "topic" if old.is_some() => bold_first(tr!("Cleared"), tr!(" the topic")),
                "bitrate" if old_value.is_null() => set(tr!("Set the bitrate to")),
                "bitrate" => set(tr!("Changed the bitrate to")),
                "rtc_region" if both => from_to(tr!("Changed the region override")),
                "rtc_region" if new.is_some() => set(tr!("Set the region override to")),
                "rtc_region" if old.is_some() => Some(vec![t(tr!("Removed the region override"))]),
                "user_limit" if old_value.is_null() => set(tr!("Set the user limit to")),
                "user_limit" => set(tr!("Changed the user limit to")),
                "rate_limit_per_user" if old_value.is_null() => {
                    seconds_plural(tr!("Set slowmode disabled"), tr!("Set slowmode to"))
                }
                "rate_limit_per_user" => {
                    seconds_plural(tr!("Disabled slowmode"), tr!("Set slowmode to"))
                }
                "nsfw" => either(
                    tr!("Marked the channel as age-restricted"),
                    tr!("Unmarked the channel as age-restricted"),
                ),
                "type" => made(tr!("Set the type to"), tr!("Changed the type")),
                "video_quality_mode" => set(tr!("Set the video quality mode to")),
                "default_auto_archive_duration" if old_value.is_null() => minutes_plural(
                    tr!("Removed default hide duration"),
                    tr!("Set default hide duration to"),
                ),
                "default_auto_archive_duration" => minutes_plural(
                    tr!("Removed default hide duration"),
                    tr!("Changed default hide duration to"),
                ),
                "default_thread_rate_limit_per_user" if both => Some(vec![
                    t(tr!("Changed default thread slowmode to ")),
                    b(trn!("{n} second", "{n} seconds", level?)),
                ]),
                "default_thread_rate_limit_per_user" if new.is_some() => Some(vec![
                    t(tr!("Set default thread slowmode to ")),
                    b(trn!("{n} second", "{n} seconds", level?)),
                ]),
                "default_thread_rate_limit_per_user" if old.is_some() => {
                    Some(vec![t(tr!("Disabled default thread slowmode"))])
                }
                _ => None,
            },
            (Target::User, _) => match key {
                "nick" if both => from_to(tr!("Changed their nickname")),
                "nick" if new.is_some() => set(tr!("Set their nickname to")),
                "nick" if old.is_some() => Some(vec![
                    b(tr!("Removed")),
                    t(tr!(" their nickname of ")),
                    b(old.clone()?),
                ]),
                "deaf" => Some(vec![
                    b(if on {
                        yapfast_lang::text_in("AuditLog", "Deafened")
                    } else {
                        tr!("Undeafened")
                    }),
                    t(tr!(" them")),
                ]),
                "mute" => Some(vec![
                    b(if on {
                        yapfast_lang::text_in("AuditLog", "Muted")
                    } else {
                        tr!("Unmuted")
                    }),
                    t(tr!(" them")),
                ]),
                "prune_delete_days" => Some(vec![
                    t(tr!("For ")),
                    b(trn!("a day", "{n} days", level?)),
                    t(tr!(" of inactivity")),
                ]),
                "communication_disabled_until" if new.is_some() => {
                    time(tr!("Set timeout for user until"))
                }
                "communication_disabled_until" if old.is_some() => {
                    Some(vec![t(tr!("Removed timeout"))])
                }
                "bypasses_verification" => either(
                    tr!("manually verified them"),
                    tr!("removed their manual verification"),
                ),
                "triggered_rule_name" => set(tr!("Detected by rule")),
                _ => None,
            },
            (Target::Role, _) => match key {
                "name" => made(tr!("With the name"), tr!("Changed the name")),
                "description" => made(tr!("With the description"), tr!("Changed the description")),
                "color" if new.as_deref() == Some("#000000") => Some(vec![t(tr!("With no color"))]),
                "color" => set(tr!("Set the color to")),
                "colors" => {
                    let hex = |key: &str| number(&new_value[key]).map(|rgb| format!("#{rgb:06X}"));
                    if new_value["secondary_color"].is_null() {
                        Some(vec![t(tr!("With no gradient"))])
                    } else {
                        let colors: Vec<String> =
                            ["primary_color", "secondary_color", "tertiary_color"]
                                .iter()
                                .filter_map(|key| hex(key))
                                .collect();
                        Some(vec![t(tr!("Set the gradient to [{}]", colors.join(", ")))])
                    }
                }
                "hoist" => either(tr!("Hoisted"), tr!("Not hoisted")),
                "mentionable" => either(tr!("Mentionable"), tr!("Not mentionable")),
                "icon_hash" => Some(vec![t(tr!("Set the icon"))]),
                "unicode_emoji" => Some(vec![t(tr!("Set the unicode emoji"))]),
                _ => None,
            },
            (Target::Invite, _) => match key {
                "code" => set(tr!("With code")),
                "channel_id" => set(tr!("For channel")),
                "max_uses" if level == Some(0) => Some(vec![
                    t(tr!("Which has ")),
                    b(tr!("unlimited")),
                    t(tr!(" uses")),
                ]),
                "max_uses" => Some(vec![
                    t(tr!("Which expires after ")),
                    b(new_text()),
                    t(tr!(" uses")),
                ]),
                "max_age" if level == Some(0) => {
                    Some(vec![t(tr!("Which ")), b(tr!("never")), t(tr!(" expires"))])
                }
                "max_age" => set(tr!("Which expires after")),
                "temporary" => Some(vec![
                    t(tr!("With temporary ")),
                    b(if on { tr!("on") } else { tr!("off") }),
                ]),
                "flags" if level == Some(1) => Some(vec![
                    t(tr!("Which is a ")),
                    b(tr!("guest")),
                    t(tr!(" invite")),
                ]),
                _ => None,
            },
            (Target::Webhook, _) => match key {
                "channel_id" if old_value.is_null() => set(tr!("With channel")),
                "channel_id" => set(tr!("Changed the channel to")),
                "name" => made(tr!("With name"), tr!("Changed the name")),
                "avatar_hash" => Some(vec![t(tr!("Changed the avatar"))]),
                _ => None,
            },
            (Target::Emoji, "name") => made(tr!("With the name"), tr!("Changed the name")),
            (Target::Sticker, _) => match key {
                "name" => made(tr!("With the name"), tr!("Changed the name")),
                "tags" => made(tr!("With the tags"), tr!("Changed the tags")),
                "description" => made(tr!("With the description"), tr!("Changed the description")),
                _ => None,
            },
            (Target::Integration, _) => match key {
                "enable_emoticons" => {
                    either(tr!("Enabled emoticons"), tr!("Disabled custom emoticons"))
                }
                "expire_behavior" => Some(vec![
                    t(tr!("Set the expired sub behavior to ")),
                    b(*[tr!("Remove Role"), tr!("Kick")].get(usize::try_from(level?).ok()?)?),
                ]),
                "expire_grace_period" => Some(vec![
                    t(tr!("Set the expire grace period to ")),
                    b(trn!("a day", "{n} days", level?)),
                ]),
                _ => None,
            },
            (Target::Stage, _) => match key {
                "topic" if old_value.is_null() => set(tr!("Set the topic to")),
                "topic" => set(tr!("Changed the topic to")),
                "privacy_level" => privacy(level?),
                _ => None,
            },
            (Target::Event, _) => match key {
                "name" => set(tr!("With the name")),
                "description" => set(tr!("Set the description to")),
                "privacy_level" => privacy(level?),
                "status" => Some(vec![
                    t(tr!("Set the status to ")),
                    b(*[
                        "",
                        tr!("Scheduled"),
                        tr!("Active"),
                        tr!("Completed"),
                        tr!("Canceled"),
                    ]
                    .get(usize::try_from(level?).ok()?)
                    .filter(|status| !status.is_empty())?),
                ]),
                "entity_type" => match level? {
                    0 => Some(vec![
                        t(tr!("Linked to ")),
                        b(yapfast_lang::text_in("AuditLog", "Nothing")),
                    ]),
                    1 => Some(vec![t(tr!("Linked with a ")), b(tr!("Stage"))]),
                    2 => Some(vec![t(tr!("Linked with a ")), b(tr!("Voice Channel"))]),
                    3 => Some(vec![t(tr!("Linked ")), b(tr!("externally"))]),
                    _ => None,
                },
                "channel_id" if new.is_none() => Some(vec![t(tr!("Removed the channel"))]),
                "channel_id" => set(tr!("Set the channel to")),
                "location" if new.is_none() => Some(vec![t(tr!("Removed the location"))]),
                "location" => set(tr!("Set the location to")),
                "image_hash" if new.is_none() => Some(vec![t(tr!("Removed the cover image"))]),
                "image_hash" => Some(vec![t(tr!("Set the cover image"))]),
                _ => None,
            },
            (Target::EventException, _) => match key {
                "scheduled_start_time" if new.is_none() => {
                    Some(vec![t(tr!("Reset the start time"))])
                }
                // Discord's client says "start" for the end, too.
                "scheduled_start_time" | "scheduled_end_time" if new.is_some() => {
                    time(tr!("Set the start time to"))
                }
                "scheduled_end_time" => Some(vec![t(tr!("Reset the end time"))]),
                "is_canceled"
                    if on && (!old_value.is_null() || new_value == &Value::Bool(true)) =>
                {
                    Some(vec![t(tr!("Canceled this event"))])
                }
                "is_canceled" if truthy(old_value) && !on => {
                    Some(vec![t(tr!("Restored this event"))])
                }
                _ => None,
            },
            (Target::Thread, _) => match key {
                "name" => made(tr!("Set the name to"), tr!("Changed the name")),
                "archived" => either(tr!("Closed the thread"), tr!("Reopened the thread")),
                "locked" => either(
                    tr!("Locked the thread, restricting it to only be opened by moderators"),
                    tr!("Unlocked the thread, allowing it to be opened by non-moderators"),
                ),
                "invitable" => either(
                    tr!("Allowed non-moderators to add members to the thread"),
                    tr!("Disallowed non-moderators from adding members to the thread"),
                ),
                "auto_archive_duration" if old_value.is_null() => minutes_plural(
                    tr!("Removed auto hide duration"),
                    tr!("Set auto hide duration to"),
                ),
                "auto_archive_duration" => minutes_plural(
                    tr!("Removed auto hide duration"),
                    tr!("Changed auto hide duration to"),
                ),
                "rate_limit_per_user" if old_value.is_null() => {
                    seconds_plural(tr!("Set slowmode disabled"), tr!("Set slowmode to"))
                }
                "rate_limit_per_user" => {
                    seconds_plural(tr!("Disabled slowmode"), tr!("Set slowmode to"))
                }
                _ => None,
            },
            (Target::AutoMod, _) => match key {
                "name" => set(tr!("Set the name to")),
                "trigger_type" => set(tr!("Set the trigger type to")),
                "event_type" => set(tr!("Set the event type to")),
                "enabled" => {
                    let enabled = if new_value.is_null() {
                        old_value
                    } else {
                        new_value
                    };
                    Some(vec![t(if enabled == &Value::Bool(true) {
                        tr!("Enabled rule")
                    } else {
                        tr!("Disabled rule")
                    })])
                }
                "exempt_roles" => Some(vec![
                    t(tr!("Set exempt roles to ")),
                    b(names(new_value, true)),
                ]),
                "exempt_channels" => Some(vec![
                    t(tr!("Set exempt channels to ")),
                    b(names(new_value, false)),
                ]),
                "trigger_metadata" => match new_value["keyword_filter"].as_array() {
                    Some(_) => Some(vec![
                        t(tr!("Set trigger metadata ")),
                        b(tr!("keywords to {}", quoted(&new_value["keyword_filter"]))),
                    ]),
                    None => Some(vec![
                        t(tr!("Set trigger metadata ")),
                        b(new_value.to_string()),
                    ]),
                },
                "$add_keyword_filter" => {
                    Some(vec![t(tr!("added keywords ")), b(quoted(new_value))])
                }
                "$remove_keyword_filter" => {
                    Some(vec![t(tr!("removed keywords ")), b(quoted(new_value))])
                }
                "$add_regex_patterns" => {
                    Some(vec![t(tr!("added regex patterns ")), b(quoted(new_value))])
                }
                "$remove_regex_patterns" => Some(vec![
                    t(tr!("removed regex patterns ")),
                    b(quoted(new_value)),
                ]),
                "$add_allow_list" => Some(vec![
                    t(tr!("added allow list keywords ")),
                    b(quoted(new_value)),
                ]),
                "$remove_allow_list" => Some(vec![
                    t(tr!("removed allow list keywords ")),
                    b(quoted(new_value)),
                ]),
                _ => None,
            },
            (Target::Soundboard, _) => match key {
                "name" => made(tr!("With the name"), tr!("Changed the name")),
                "volume" => made(tr!("With the volume set to"), tr!("Changed the volume")),
                "emoji_name" if both => from_to(tr!("Changed the emoji")),
                "emoji_name" if new.is_some() => set(tr!("With the emoji")),
                "emoji_name" if old.is_some() => {
                    Some(vec![t(tr!("Removed the emoji ")), b(old.clone()?)])
                }
                _ => None,
            },
            (Target::VoiceStatus, "status") => set(tr!("Set to")),
            (Target::Prompt, _) => match key {
                "title" if old_value.is_null() => Some(vec![t(tr!("Set the title to empty"))]),
                "title" => from_to(tr!("Changed the title")),
                "description" => made(
                    tr!("Set the description to"),
                    tr!("Changed the description"),
                ),
                "options" => Some(vec![t(tr!("Changed answers for customization question"))]),
                "single_select" => either(
                    tr!("Set the customization question to single select"),
                    tr!("Set the customization question to multi select"),
                ),
                "required" => either(
                    tr!("Set the customization question to required"),
                    tr!("Set the customization question to optional"),
                ),
                _ => None,
            },
            (Target::Onboarding, _) => match key {
                "default_channel_ids" => {
                    let count = new_value.as_array().map_or(0, Vec::len) as u64;
                    Some(vec![t(tr!(
                        "Set {}",
                        trn!("{n} default channel", "{n} default channels", count)
                    ))])
                }
                "enable_default_channels" => either(
                    tr!("Set default channels to enabled"),
                    tr!("Set default channels to disabled"),
                ),
                "enable_onboarding_prompts" => either(
                    tr!("Set customization questions to enabled"),
                    tr!("Set customization questions to disabled"),
                ),
                "enabled" => either(tr!("Enabled Onboarding"), tr!("Disabled Onboarding")),
                "mode" => match level? {
                    1 => Some(vec![t(tr!("Set onboarding to Advanced Mode"))]),
                    0 => Some(vec![t(tr!("Set onboarding to Regular Mode"))]),
                    _ => None,
                },
                _ => None,
            },
            (Target::HomeSettings, _) => match key {
                "welcome_message" => Some(vec![t(tr!("Changed the welcome message"))]),
                "new_member_actions" => Some(vec![t(tr!("Changed the new member To Dos"))]),
                "resource_channels" => Some(vec![t(tr!("Changed resource channels"))]),
                _ => None,
            },
            (Target::Verification, _) => match key {
                "verification_enabled" => Some(vec![t(if new_value == &Value::Bool(true) {
                    tr!("Enabled join settings")
                } else {
                    tr!("Disabled join settings")
                })]),
                "manual_approval_enabled" => Some(vec![t(if new_value == &Value::Bool(true) {
                    tr!("Manual verification enabled")
                } else {
                    tr!("Manual verification disabled")
                })]),
                _ => None,
            },
            (Target::Profile, _) => match key {
                "description" => Some(vec![t(tr!("Updated server description"))]),
                "brand_color_primary" => Some(vec![t(tr!("Updated profile banner color"))]),
                "custom_banner_hash" => Some(vec![t(tr!("Updated profile banner"))]),
                "traits" => Some(vec![t(tr!("Updated profile traits"))]),
                "game_application_ids" => Some(vec![t(tr!("Updated profile games played"))]),
                "visibility" => Some(vec![t(tr!("Updated profile visibility"))]),
                "server_tag" if new.is_none() => {
                    bold_first(tr!("Disabled"), tr!(" the server tag"))
                }
                "server_tag" => set(tr!("Updated server tag to")),
                _ => None,
            },
            _ => None,
        }
    }
}

/// A stage's or event's privacy, as Discord words it.
fn privacy(level: u64) -> Option<Vec<Piece>> {
    let name = match level {
        2 => tr!("Closed"),
        1 => tr!("Public"),
        _ => return None,
    };
    Some(vec![t(tr!("Set the privacy level to ")), b(name)])
}

/// A forum's change of tags, as Discord's log words the first: one added,
/// else one removed, else one renamed.
fn tag_change(old: &Value, new: &Value) -> Option<Vec<Piece>> {
    let tags = |list: &Value| -> Vec<(String, String)> {
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(|tag| {
                Some((
                    shown(&tag["id"])?,
                    tag["name"].as_str().unwrap_or_default().to_owned(),
                ))
            })
            .collect()
    };
    let (was, now) = (tags(old), tags(new));
    let missing = |from: &[(String, String)], to: &[(String, String)]| {
        from.iter()
            .find(|(id, _)| !to.iter().any(|(other, _)| other == id))
            .map(|(_, name)| name.clone())
    };
    if let Some(name) = missing(&now, &was) {
        return Some(vec![t(tr!("Added tag ")), b(name)]);
    }
    if let Some(name) = missing(&was, &now) {
        return Some(vec![t(tr!("Deleted tag ")), b(name)]);
    }
    now.iter().find_map(|(id, name)| {
        let (_, before) = was.iter().find(|(other, _)| other == id)?;
        (before != name).then(|| {
            vec![
                t(tr!("Updated tag from ")),
                b(before.clone()),
                t(tr!(" to ")),
                b(name.clone()),
            ]
        })
    })
}

/// Keys a target's lines leave out, as Discord's client does.
fn hidden(target: Target, key: &str) -> bool {
    match target {
        Target::Channel => matches!(key, "id" | "permission_overwrites"),
        Target::Overwrite => matches!(key, "type" | "id" | "permission_overwrites"),
        Target::Invite => matches!(key, "inviter_id" | "uses"),
        Target::Webhook => matches!(key, "type" | "application_id"),
        Target::Integration => matches!(key, "type" | "name"),
        Target::Thread => matches!(key, "id" | "type"),
        Target::Sticker => matches!(
            key,
            "id" | "type" | "asset" | "format_type" | "available" | "guild_id"
        ),
        Target::Home => key == "entity_type",
        Target::Onboarding => key == "prompts",
        Target::Soundboard => matches!(key, "id" | "sound_id"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_keep_their_pieces_in_one_string() {
        let words = Words::new(vec![
            b("alice"),
            t(" set it until "),
            Piece::Time(1_792_599_600),
            b(""),
            // A mark in a name doesn't break the sentence.
            t(" for \u{1}bob"),
        ]);
        let spans: Vec<Span> = words.spans().collect();
        assert_eq!(
            spans,
            [
                Span::Bold("alice"),
                Span::Text(" set it until "),
                Span::Time(1_792_599_600),
                Span::Bold(""),
                Span::Text(" for bob"),
            ]
        );
        assert_eq!(Words::default().spans().count(), 0);
    }
}
