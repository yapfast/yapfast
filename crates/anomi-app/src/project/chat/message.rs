//! A message of the core as the timeline's row shows it: `message_row` and
//! `Layout::restyle` of the Slint app (`src/ui/messages.rs:481`,
//! `src/ui.rs:1876`). A row is built once, as its message comes or changes,
//! and moves from one projection to the next; the timeline then marks it as
//! the rows around it say (`super::timeline`). What it holds is what it
//! draws, made once: the body parsed into spans, the times written, the
//! pictures as handles over the core's own pixels.
//!
//! [`addresses`] names every picture a row shows, so the timeline asks for
//! those in view and builds a row again when one of them comes or goes.

mod author;
mod body;

use std::collections::HashSet;

use yapfast_core::application::{AppState, Application};
use yapfast_core::chat::{ChatMessage, Delivery as CoreDelivery, Reacts};
use yapfast_core::clock;
use yapfast_lang::{tr, trn};

use anomi_vm::chat::message::{Bar, Delivery, Fold, May, Message, Quick, Size, Thinking, Thread};
use anomi_vm::text::Text;

use super::{attachments, components, embeds, poll, reactions};
use crate::pictures::{self, Requests};

pub(crate) use body::{BLOCKED_LINK, sections as body_sections};

/// The message flag of a Discord app's answer only this account sees.
pub(crate) const EPHEMERAL: u64 = 1 << 6;
/// The message flag of a Discord app's answer still to come.
pub(crate) const LOADING: u64 = 1 << 7;
/// How long Discord's client waits for a deferred answer before it says the
/// app didn't respond: 15 minutes from the message's snowflake.
pub(crate) const THINKING_SECS: u64 = 15 * 60;

/// The settings every row is made with: when one changes, every row is made
/// again (`Marks::restyle` in the Slint app).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Look {
    pub twelve_hour: bool,
    pub seconds: bool,
    pub compact: bool,
    pub size: Size,
    pub link_previews: bool,
    pub collapse_long: bool,
    pub cosmetics: yapfast_core::application::Cosmetics,
    /// The open place is a server's: an author's name shows its style's
    /// font alone.
    pub in_server: bool,
    pub preview: bool,
    /// What this account may do here: write, react, start threads.
    pub writes: bool,
    pub reacts: Reacts,
    pub forwards: bool,
    pub threads: bool,
    /// Today, which a time names as "Today" or "Yesterday" from.
    pub today: Option<chrono::NaiveDate>,
    /// The order of a date's parts, as chosen.
    pub date_format: Option<i32>,
}

/// What a row is made from besides its message.
pub(crate) struct Context<'a> {
    pub state: &'a AppState,
    pub look: &'a Look,
    /// Spoilers shown, long messages opened and blocked ones shown, as
    /// `{item}/{index}`, `{item}/a{index}`, `{item}/more`, `{item}/blocked`.
    pub revealed: &'a HashSet<String>,
    /// Now, in Unix seconds.
    pub now: u64,
    /// What this device keeps of polls and bots' controls (I08).
    pub kept: &'a super::kept::Kept,
}

/// The message as its row shows it, unmarked: the timeline sets what the
/// rows around it say (`first`, `edge`, `highlighted`) and what the pointer
/// does (`lit`, `bar`).
pub(crate) fn build(cx: &Context<'_>, message: &ChatMessage, key: u64) -> Message {
    let look = cx.look;
    let blocked = collapsed(message, cx.revealed);
    let (time, short_time) = times(cx, message);
    let delivery = delivery(message);
    let acts = !message.deleted && delivery.is_none();
    let mut built = Message {
        key,
        compact: look.compact,
        size: look.size,
        author: author::author(cx, message, blocked),
        header: author::header(cx, message, blocked),
        notice: author::notice(message).filter(|_| !blocked),
        body: body::body(cx, message, blocked, &time),
        fold: match (look.collapse_long, expanded(&message.item, cx.revealed)) {
            (false, _) => Fold::Never,
            (true, false) => Fold::Folded,
            (true, true) => Fold::Open,
        },
        forwarded: message.forwarded && !blocked,
        edited: message.edited,
        deleted: message.deleted,
        delivery,
        thinking: thinking(message, cx.now),
        hidden_card: hidden_card(look, message),
        thread: message.thread.as_ref().map(|thread| Thread {
            name: thread.name.clone(),
            detail: thread_detail(thread),
        }),
        ephemeral: message.flags & EPHEMERAL != 0,
        preview: look.preview,
        time,
        short_time,
        ..Message::default()
    };
    if !blocked {
        built.reactions = reactions::chips(cx, message, key, acts);
        built.attachments = attachments::attachments(cx, message, key);
        if look.link_previews {
            built.embeds = embeds::embeds(cx, message, key);
        }
        if let Some(poll) = &message.poll {
            built.poll = poll::card(cx, message, poll, key);
        }
        built.components = components::components(cx, message, key);
        built.failed = (cx.state.interaction_failed.as_ref())
            .filter(|(item, _)| *item == message.item)
            .map(|(_, why)| why.clone());
    }
    built
}

/// What a pinned or found message says, its spoilers covered; none for a
/// message of files or a poll alone (`found_row`).
pub(crate) fn found_text(state: &AppState, message: &ChatMessage, time: &str) -> Option<Text> {
    let said = message.notice.is_some() || !message.body.trim().is_empty();
    said.then(|| body::found(state, message, time))
}

/// Every picture a row of `message` shows, by address: its author's
/// picture and decoration, their tag's badge, the picture of the person it
/// replies to or who used the command, its custom emoji's.
pub(crate) fn addresses<'a>(
    message: &'a ChatMessage,
    look: &Look,
    revealed: &HashSet<String>,
    each: &mut impl FnMut(&'a str),
) {
    if let Some(reply) = &message.reply {
        each(reply.avatar.as_deref().unwrap_or_default());
    } else if let Some(used) = &message.used {
        each(used.avatar.as_deref().unwrap_or_default());
    }
    if collapsed(message, revealed) {
        return;
    }
    each(message.avatar.as_deref().unwrap_or_default());
    let cosmetics = look.cosmetics;
    each(
        cosmetics
            .decoration(message.decoration.as_deref())
            .unwrap_or_default(),
    );
    if let Some(tag) = cosmetics.tag(message.tag.as_ref()) {
        each(tag.badge.as_deref().unwrap_or_default());
    }
    for emoji in &message.emoji {
        each(&emoji.picture);
    }
}

/// The hover bar of the message the pointer is on: what may be done to it
/// here, with the frequent reactions (`message-bar`, `ui/yapfast.slint:3953`).
pub(crate) fn bar(core: &Application, look: &Look, message: &ChatMessage) -> Bar {
    let state = &core.state;
    let writes = look.writes;
    let own_edit = message.own && message.can_edit;
    let may = May {
        react: message.can_react && look.reacts == Reacts::Any,
        edit: own_edit && writes,
        reply: !own_edit && message.can_reply && writes,
        forward: message.can_forward && look.forwards,
        pin: message.can_pin,
        thread: look.threads && message.can_reply && message.thread.is_none(),
        delete: message.can_delete && writes,
    };
    let frequent = if may.react {
        frequent(core, 3)
    } else {
        Vec::new()
    };
    Bar {
        frequent,
        may,
        pinned: message.pinned,
        shift: false,
        confirming: state.confirm_delete.as_deref() == Some(message.item.as_str()),
    }
}

/// The reactions this device uses most, `count` of them: Unicode's as
/// their glyphs, a server's with its picture once it came
/// (`show_reactions_offered`, `src/ui/refresh.rs:2632`). A server's emoji
/// used from a reaction is kept as `:name:`, found among the open room's
/// messages' emoji; one picked from the picker as `name:id`.
pub(crate) fn frequent(core: &Application, count: usize) -> Vec<Quick> {
    let state = &core.state;
    let keys = core.frequent_reactions(count);
    let addresses = quick_addresses(core, &keys);
    keys.into_iter()
        .zip(addresses)
        .map(|(key, address)| Quick {
            name: yapfast_core::emoji::shortcode(&key),
            picture: pictures::find(state, address.as_deref()),
            key,
        })
        .collect()
}

/// Asks for the pictures of the reactions the bar and the message menu
/// offer (at most four) that haven't come: a server's emoji picked from
/// the picker is kept as `name:id`, which no message on screen may show.
pub(crate) fn want_frequent(core: &Application, requests: &mut Requests) {
    let keys = core.frequent_reactions(4);
    let addresses = quick_addresses(core, &keys);
    requests.want(&core.state, addresses.iter().flatten().map(String::as_str));
}

/// The picture addresses of the reactions `keys`: none for Unicode's.
fn quick_addresses(core: &Application, keys: &[String]) -> Vec<Option<String>> {
    let state = &core.state;
    let named = |key: &str| {
        state
            .messages
            .iter()
            .flat_map(|message| &message.emoji)
            .find(|emoji| emoji.text == key)
            .map(|emoji| emoji.picture.clone())
    };
    let ids: HashSet<&str> = keys
        .iter()
        .filter(|key| !key.starts_with(':'))
        .filter_map(|key| key.split_once(':').map(|(_, id)| id))
        .collect();
    let by_id = if ids.is_empty() {
        Vec::new()
    } else {
        core.emoji_by_id(&ids)
    };
    keys.iter()
        .map(|key| {
            if key.starts_with(':') {
                named(key)
            } else {
                key.split_once(':')
                    .and_then(|(_, id)| by_id.iter().find(|emoji| emoji.id == id))
                    .map(|emoji| emoji.picture.clone())
            }
        })
        .collect()
}

/// The time beside the author, and in the gutter of a grouped message the
/// clock alone, on the chosen clock (`Layout::restyle`).
fn times(cx: &Context<'_>, message: &ChatMessage) -> (String, String) {
    let look = cx.look;
    if message.timestamp == 0 {
        let short = message
            .time
            .split_whitespace()
            .next()
            .filter(|time| time.contains(':'))
            .unwrap_or_default();
        return (message.time.clone(), short.to_owned());
    }
    let sent = clock::sent(message.timestamp, cx.now, look.twelve_hour, look.seconds);
    // The gutter is narrow: the clock alone, without AM and PM.
    let time = clock::time_with(message.timestamp, look.twelve_hour, look.seconds);
    let short = time
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
    (sent, short)
}

fn delivery(message: &ChatMessage) -> Option<Delivery> {
    let (text, retry) = match message.delivery {
        CoreDelivery::Sent => return None,
        CoreDelivery::Sending => (tr!("Sending..."), false),
        CoreDelivery::Waiting => (
            tr!("Not sent yet. Yapfast sends it when the connection returns."),
            false,
        ),
        // An app's command, whose row says why.
        CoreDelivery::Failed if message.item.starts_with("c:") => (tr!("Not sent."), true),
        CoreDelivery::Failed => (tr!("Not sent. The server refused this message."), true),
    };
    Some(Delivery {
        text: text.to_owned(),
        failed: message.delivery == CoreDelivery::Failed,
        retry,
        cancel: true,
        // One with files keeps its line, with Cancel, while they upload.
        quiet: message.delivery == CoreDelivery::Sending && message.attachments.is_empty(),
    })
}

/// An app's answer still to come, and after 15 minutes the app's silence.
fn thinking(message: &ChatMessage, now: u64) -> Option<Thinking> {
    (message.flags & LOADING != 0).then(|| {
        if message.timestamp != 0 && now >= message.timestamp + THINKING_SECS {
            Thinking::TimedOut
        } else {
            Thinking::Waiting
        }
    })
}

/// When a message's "thinking" turns to the app's silence, while it waits.
pub(crate) fn thinking_ends(message: &ChatMessage) -> Option<u64> {
    (message.flags & LOADING != 0 && message.timestamp != 0)
        .then_some(message.timestamp + THINKING_SECS)
}

/// A card of a message that is only one, while link previews are off.
fn hidden_card(look: &Look, message: &ChatMessage) -> Option<String> {
    use yapfast_core::chat::Placement;
    if look.link_previews
        || !message.body.trim().is_empty()
        || message.attachments.iter().any(|attachment| {
            matches!(
                attachment.placement,
                Placement::Message | Placement::Sticker
            )
        })
    {
        return None;
    }
    let embed = message.embeds.first()?;
    Some(if embed.title.is_empty() {
        tr!("A card, hidden while link previews are off.").to_owned()
    } else {
        tr!(
            "A card, \"{}\", hidden while link previews are off.",
            embed.title
        )
    })
}

/// What a thread's chip says after its name: how many messages it has, as
/// Discord counts them, and whether it's archived.
fn thread_detail(thread: &yapfast_core::chat::ThreadLink) -> String {
    let count = match thread.messages {
        None => None,
        Some(0) => Some(tr!("No messages yet").to_owned()),
        Some(count) => Some(trn!("{n} message", "{n} messages", count)),
    };
    match (count, thread.archived) {
        (Some(count), true) => tr!("{}, archived", count),
        (Some(count), false) => count,
        (None, true) => tr!("Archived").into(),
        (None, false) => String::new(),
    }
}

/// Whether Show more was clicked on a long message.
fn expanded(item: &str, revealed: &HashSet<String>) -> bool {
    revealed.contains(&fold_key(item))
}

/// What `revealed` holds for a long message shown whole.
pub(crate) fn fold_key(item: &str) -> String {
    format!("{item}/more")
}

/// What `revealed` holds for a blocked person's message shown.
pub(crate) fn blocked_key(item: &str) -> String {
    format!("{item}/blocked")
}

/// Whether a message shows collapsed, being from someone blocked.
pub(crate) fn collapsed(message: &ChatMessage, revealed: &HashSet<String>) -> bool {
    message.blocked && !revealed.contains(&blocked_key(&message.item))
}
