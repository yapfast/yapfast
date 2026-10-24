//! Discord messages as Yapfast shows them. Parsing keeps the raw content, and
//! rendering replaces mentions with the names known when it runs, so names
//! learned later show on messages already loaded.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
};

use serde_json::Value;

use crate::{
    components::{self, Card},
    embed::{self, Embed},
    ready::{self, Server},
};

/// Discord counts from the first second of 2015.
pub(crate) const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;

const DEFAULT: u64 = 0;
const CALL: u64 = 3;
const PINNED: u64 = 6;
const MEMBER_JOIN: u64 = 7;
const THREAD_CREATED: u64 = 18;
const REPLY: u64 = 19;
const CHAT_INPUT_COMMAND: u64 = 20;
const THREAD_STARTER_MESSAGE: u64 = 21;
const CONTEXT_MENU_COMMAND: u64 = 23;
const POLL_RESULT: u64 = 46;

/// The reference of a forward, which carries a copy of the message it
/// forwards in `message_snapshots`.
const FORWARD: u64 = 1;

/// The key of `authorizing_integration_owners` for an app installed on a
/// user's account rather than in a server.
const USER_INSTALL: &str = "1";

/// The message flag that hides its embeds.
const SUPPRESS_EMBEDS: u64 = 1 << 2;
/// The message flags Discord's client forwards a message with: crossposts,
/// hidden embeds, threads, @silent, voice messages, forwards and the like,
/// but not ephemeral or loading messages, or flags it doesn't know.
const FORWARDABLE_FLAGS: u64 = 1 << 0
    | 1 << 1
    | SUPPRESS_EMBEDS
    | 1 << 4
    | 1 << 5
    | 1 << 8
    | 1 << 9
    | 1 << 10
    | 1 << 12
    | 1 << 13
    | 1 << 14
    | 1 << 15
    | 1 << 19
    | 1 << 20;

/// Missing sticker formats default to PNG.
const STICKER_PNG: u64 = 1;
/// The size Discord shows stickers at.
const STICKER_SIZE: u32 = 160;

/// The attachment flag that marks a spoiler.
pub(crate) const IS_SPOILER: u64 = 1 << 3;
/// The flag of an attachment, or an embed's picture, that moves.
const IS_ANIMATED: u64 = 1 << 5;

/// Whether an attachment or an embed's picture moves: Discord marks it
/// animated, or it's a GIF.
pub(crate) fn animated(media: &Value) -> bool {
    media["flags"].as_u64().unwrap_or_default() & IS_ANIMATED != 0
        || media["content_type"].as_str() == Some("image/gif")
}

/// A page of a channel's history, oldest first.
#[derive(Debug)]
pub struct Page {
    /// The messages Yapfast shows, which may be fewer than Discord sent.
    pub messages: Vec<Message>,
    /// The snowflake of the oldest message Discord sent, shown or not;
    /// earlier history is before it.
    pub oldest: Option<u64>,
    /// Discord sent a whole page, so there may be more beyond it: before
    /// it, or for a page after a snowflake, after it.
    pub full: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    /// The client's nonce, on messages this account sent.
    pub nonce: Option<String>,
    pub author_id: String,
    /// The author's name from the message itself, used until a better one is known.
    pub author: String,
    /// The author is a bot or webhook, which Discord tags.
    #[serde(default)]
    pub bot: bool,
    /// Discord Markdown with mentions still as `<@id>`.
    pub content: String,
    /// Shown after the author's name instead of content, for joins, pins
    /// and calls.
    pub system: Option<String>,
    /// What such a notice tells of, which its icon and links show.
    #[serde(default)]
    pub notice: Option<Notice>,
    pub edited: bool,
    pub reply: Option<Reply>,
    /// Its files, then its embeds' pictures.
    pub attachments: Vec<Attachment>,
    pub embeds: Vec<Embed>,
    /// Its buttons and menus, or for a message of components alone, all it
    /// shows; their pictures and files are among `attachments`.
    #[serde(default)]
    pub components: Vec<Card>,
    pub reactions: Vec<Reaction>,
    /// Content can be edited by its author: an ordinary message or reply.
    pub editable: bool,
    /// Who it mentions, by user id, including the author of the message it
    /// replies to when the reply pings them.
    #[serde(default)]
    pub mentions: Vec<String>,
    /// The roles it mentions, by id, as Discord counts them: only those the
    /// author may mention.
    #[serde(default)]
    pub mention_roles: Vec<String>,
    /// It mentions @everyone or @here, and its author may.
    #[serde(default)]
    pub mention_everyone: bool,
    /// The thread started from it, or that a notice of one names.
    #[serde(default)]
    pub thread: Option<ThreadLink>,
    /// Among its channel's pinned messages.
    #[serde(default)]
    pub pinned: bool,
    /// Forwarded from another message, whose content, files and embeds it
    /// shows as its own.
    #[serde(default)]
    pub forwarded: bool,
    /// Can be forwarded, as Discord's client decides from the message
    /// alone: written, and without a poll, a call or an activity.
    #[serde(default)]
    pub forwardable: bool,
    /// Not even its author may delete it: a call's notice, or a thread's
    /// first message, which shows the one it was started from.
    #[serde(default)]
    pub undeletable: bool,
    /// Who ran the command it answers, for an app installed on their
    /// account, who may delete it as their own.
    #[serde(default)]
    pub invoker: Option<String>,
    /// The poll it asks, if it's one.
    #[serde(default)]
    pub poll: Option<Poll>,
}

/// A message's poll, its votes as Discord counted them when the message
/// came, with those that came live since.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Poll {
    pub question: String,
    pub answers: Vec<PollAnswer>,
    /// When it closes, in Unix milliseconds.
    pub expiry: Option<u64>,
    /// One may pick several answers.
    pub multiselect: bool,
    /// The message carried the votes; without them they aren't known,
    /// rather than none.
    pub counted: bool,
    /// Discord counted the votes for the last time, once it closed.
    pub finalized: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollAnswer {
    pub id: u64,
    pub text: String,
    /// Its emoji as a reaction's key: the emoji itself, or `:name:` for a
    /// custom one, whose id follows.
    pub emoji: Option<(String, Option<String>)>,
    pub votes: usize,
    /// This account voted for it.
    pub me: bool,
}

impl Poll {
    /// A message's `poll` object.
    fn of(poll: &Value) -> Option<Self> {
        let counts = poll["results"]["answer_counts"].as_array();
        let answers = poll["answers"]
            .as_array()?
            .iter()
            .filter_map(|answer| {
                let id = answer["answer_id"].as_u64()?;
                let media = &answer["poll_media"];
                let count = counts
                    .into_iter()
                    .flatten()
                    .find(|count| count["id"].as_u64() == Some(id));
                let emoji = &media["emoji"];
                Some(PollAnswer {
                    id,
                    text: media["text"].as_str().unwrap_or_default().to_owned(),
                    emoji: match (emoji["id"].as_str(), emoji["name"].as_str()) {
                        // Discord's client sends a custom one without its name.
                        (Some(id), name) => Some((
                            format!(":{}:", name.filter(|name| !name.is_empty()).unwrap_or(id)),
                            Some(id.to_owned()),
                        )),
                        (None, Some(name)) if !name.is_empty() => Some((name.to_owned(), None)),
                        _ => None,
                    },
                    votes: count.and_then(|count| count["count"].as_u64()).unwrap_or(0) as usize,
                    me: count.and_then(|count| count["me_voted"].as_bool()) == Some(true),
                })
            })
            .collect();
        Some(Self {
            question: poll["question"]["text"].as_str()?.to_owned(),
            answers,
            expiry: poll["expiry"].as_str().and_then(iso_ms),
            multiselect: poll["allow_multiselect"].as_bool().unwrap_or(false),
            counted: poll["results"].is_object(),
            finalized: poll["results"]["is_finalized"].as_bool().unwrap_or(false),
        })
    }

    /// Whether it no longer takes votes at `now`, in Unix milliseconds.
    pub fn closed(&self, now: u64) -> bool {
        self.finalized || self.expiry.is_some_and(|expiry| expiry <= now)
    }
}

/// A thread a message leads to, as the message carried it; the thread as
/// Yapfast knows it now, if it does, is newer.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ThreadLink {
    pub id: String,
    pub name: String,
    /// Its messages after the first.
    pub messages: u64,
    pub archived: bool,
}

impl ThreadLink {
    /// A thread's channel object.
    fn of(thread: &Value) -> Option<Self> {
        Some(Self {
            id: thread["id"].as_str()?.to_owned(),
            name: thread["name"].as_str().unwrap_or_default().to_owned(),
            messages: thread["message_count"].as_u64().unwrap_or_default(),
            archived: thread["thread_metadata"]["archived"]
                .as_bool()
                .unwrap_or(false),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Reply {
    Message {
        /// The original message's id.
        id: String,
        author_id: String,
        author: String,
        content: String,
    },
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Attachment {
    pub name: String,
    pub size: Option<u64>,
    pub url: Arc<str>,
    pub image: bool,
    /// The image's width and height.
    pub dimensions: Option<(u32, u32)>,
    /// The image scaled down by Discord's media proxy, for previews.
    pub preview_url: Option<String>,
    /// The image scaled down to fit the viewer, by the same proxy.
    pub view_url: Option<String>,
    /// A still picture to show when the preview, a video, can't be shown:
    /// a Tenor or Giphy GIF's first frame.
    #[serde(default)]
    pub still_url: Option<String>,
    /// A video that plays in place of the picture, which is its poster: an
    /// attached video, or a link's through the media proxy.
    #[serde(default)]
    pub video_url: Option<String>,
    /// The address a GIF or moving picture linked on its own was sent as,
    /// which Discord's client keeps it among the favourites by: a GIF's page,
    /// or the picture's own address.
    #[serde(default, alias = "page_url")]
    pub link_url: Option<Arc<str>>,
    /// It moves: Discord marks it animated, or it's a GIF.
    #[serde(default)]
    pub animated: bool,
    pub shown: Shown,
    /// Marked as a spoiler: by a name starting `SPOILER_`, or by its flags.
    pub spoiler: bool,
}

/// Where a message's picture shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Shown {
    /// With the message's attached files and images.
    #[default]
    Message,
    /// Large, in the embed at this index.
    Embed(usize),
    /// Small, beside the text of the embed at this index.
    Thumbnail(usize),
    /// Large, as the poster of the video embed at this index.
    Poster(usize),
    /// On its own, without a frame: a sticker.
    Sticker,
    /// In the component row with this slot: [`crate::components::Row::Media`]
    /// or a section's thumbnail.
    Component(usize),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Reaction {
    /// What Yapfast shows: the emoji itself, or `:name:` for a custom one.
    pub key: String,
    /// What Discord's API takes: the emoji itself, or `name:id`.
    pub emoji: String,
    pub count: usize,
    pub me: bool,
}

impl Message {
    /// Whether it calls on this account, `me`, as Discord highlights it: by
    /// name, through one of its roles in `server`, or with @everyone or
    /// @here, unless its notification settings there suppress those; or as
    /// a reply to its message. Its own messages never do.
    pub fn mentions_me(&self, me: &str, server: Option<&ready::Server>) -> bool {
        self.names_me(me, server) || self.replies_to(me)
    }

    /// Whether it calls on `me` other than by replying: by name, a role or
    /// @everyone, as [`Self::mentions_me`] says. A reply that pings puts
    /// the author it replies to in `mentions` too, so for them only their
    /// name in its text counts.
    pub fn names_me(&self, me: &str, server: Option<&ready::Server>) -> bool {
        if self.author_id == me {
            return false;
        }
        let named = self.mentions.iter().any(|user| user == me)
            && (!self.replies_to(me)
                || self.content.contains(&format!("<@{me}>"))
                || self.content.contains(&format!("<@!{me}>")));
        let everyone =
            self.mention_everyone && server.is_some_and(|server| !server.suppress_everyone);
        let role = server.is_some_and(|server| {
            !server.suppress_roles
                && self
                    .mention_roles
                    .iter()
                    .any(|role| server.my_roles.contains(role))
        });
        named || everyone || role
    }

    /// Whether it replies to a message of `me`'s, not being its own.
    pub fn replies_to(&self, me: &str) -> bool {
        self.author_id != me
            && matches!(&self.reply, Some(Reply::Message { author_id, .. }) if author_id == me)
    }

    /// Seconds since the Unix epoch, from the message's snowflake.
    pub fn timestamp(&self) -> u64 {
        snowflake_ms(&self.id) / 1000
    }

    pub fn order(&self) -> u64 {
        self.id.parse().unwrap_or_default()
    }

    /// Adds or removes this account's or someone else's reaction.
    pub fn react(&mut self, key: &str, emoji: &str, me: bool, added: bool) {
        match self
            .reactions
            .iter()
            .position(|reaction| reaction.key == key)
        {
            Some(index) => {
                let reaction = &mut self.reactions[index];
                if added {
                    reaction.count += 1;
                    reaction.me |= me;
                } else {
                    reaction.count = reaction.count.saturating_sub(1);
                    reaction.me &= !me;
                    if reaction.count == 0 {
                        self.reactions.remove(index);
                    }
                }
            }
            None if added => self.reactions.push(Reaction {
                key: key.to_owned(),
                emoji: emoji.to_owned(),
                count: 1,
                me,
            }),
            None => {}
        }
    }

    /// Adds or removes this account's or someone else's vote for `answer`.
    pub fn vote(&mut self, answer: u64, me: bool, added: bool) {
        let Some(answer) = self
            .poll
            .as_mut()
            .and_then(|poll| poll.answers.iter_mut().find(|known| known.id == answer))
        else {
            return;
        };
        if added {
            answer.votes += 1;
            answer.me |= me;
        } else {
            answer.votes = answer.votes.saturating_sub(1);
            answer.me &= !me;
        }
    }
}

pub fn snowflake_ms(id: &str) -> u64 {
    id.parse::<u64>()
        .map_or(0, |id| (id >> 22) + DISCORD_EPOCH_MS)
}

/// The smallest snowflake at a time in Unix milliseconds.
pub fn snowflake_at(ms: u64) -> u64 {
    ms.saturating_sub(DISCORD_EPOCH_MS) << 22
}

/// Unix milliseconds of an ISO 8601 time as Discord writes them, such as
/// `2026-10-06T14:05:00.123000+00:00`.
pub fn iso_ms(text: &str) -> Option<u64> {
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    let rest = text.get(19..)?;
    let (fraction, zone) = match rest.strip_prefix('.') {
        Some(rest) => {
            let digits = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            let millis = format!("{:0<3}", &rest[..digits.min(3)]);
            (millis.parse::<i64>().ok()?, &rest[digits..])
        }
        None => (0, rest),
    };
    let offset = match zone {
        "" | "Z" => 0,
        zone => {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            let hours: i64 = zone.get(1..3)?.parse().ok()?;
            let minutes: i64 = zone.get(4..6)?.parse().ok()?;
            sign * (hours * 60 + minutes) * 60
        }
    };
    // Howard Hinnant's days-from-civil algorithm.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let seconds = days * 86_400 + hour * 3600 + minute * 60 + second - offset;
    u64::try_from(seconds * 1000 + fraction).ok()
}

/// A nonce like the official client's: a snowflake for the current time.
pub fn nonce() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    ((now.saturating_sub(DISCORD_EPOCH_MS)) << 22).to_string()
}

/// How long a call lasted, as Discord says it.
fn lasted(ms: u64) -> String {
    let minutes = (ms + 30_000) / 60_000;
    match minutes {
        0 => "a few seconds".into(),
        1 => "a minute".into(),
        2..45 => format!("{minutes} minutes"),
        45..90 => "an hour".into(),
        _ => format!("{} hours", (minutes + 30) / 60),
    }
}

/// Whether a message of this kind was written, by a person or by an app
/// answering one's command, rather than a notice such as a join or a pin.
pub(crate) fn written(kind: u64) -> bool {
    matches!(
        kind,
        DEFAULT | REPLY | CHAT_INPUT_COMMAND | CONTEXT_MENU_COMMAND
    )
}

/// What a notice in place of a message tells of, as Discord's client marks
/// it with an icon and links.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Notice {
    /// Its author pinned a message, by id, which the notice links to.
    Pinned(Option<String>),
    Joined,
    ThreadStarted,
    Call,
    PollResult,
}

/// A message object, from the API or the gateway. Kinds Yapfast doesn't show,
/// such as boosts, give `None`.
pub fn parse(value: &Value) -> Option<Message> {
    let kind = value["type"].as_u64().unwrap_or(DEFAULT);
    // A thread's first message shows the one it was started from, as
    // Discord's client does.
    if kind == THREAD_STARTER_MESSAGE {
        let original = &value["referenced_message"];
        if !original.is_object() {
            return None;
        }
        let mut message = parse(original)?;
        message.id = value["id"].as_str()?.into();
        message.channel_id = value["channel_id"].as_str().unwrap_or_default().into();
        message.nonce = None;
        message.editable = false;
        message.forwardable = false;
        message.undeletable = true;
        message.invoker = None;
        message.thread = None;
        return Some(message);
    }
    let system = match kind {
        kind if written(kind) => None,
        THREAD_CREATED => Some(format!(
            "started a thread: {}.",
            value["content"].as_str().unwrap_or_default()
        )),
        MEMBER_JOIN => Some("joined the server.".to_owned()),
        // Its author is the poll's.
        POLL_RESULT => Some(poll_result(&value["embeds"][0])),
        PINNED => Some("pinned a message.".to_owned()),
        // Discord updates it with when the call ended.
        CALL => Some(
            match value["call"]["ended_timestamp"].as_str().and_then(iso_ms) {
                Some(ended) => {
                    let started = value["id"].as_str().map_or(ended, snowflake_ms);
                    format!(
                        "started a call that lasted {}.",
                        lasted(ended.saturating_sub(started))
                    )
                }
                None => "started a call.".to_owned(),
            },
        ),
        _ => return None,
    };
    let author = &value["author"];
    // A forward's own content is empty: what it shows is the copy of the
    // original it carries.
    let forwarded = value["message_reference"]["type"].as_u64() == Some(FORWARD);
    let shown = match &value["message_snapshots"][0]["message"] {
        snapshot @ Value::Object(_) if forwarded => snapshot,
        _ => value,
    };
    let reply = (kind == REPLY).then(|| match &value["referenced_message"] {
        Value::Object(_) => {
            let original = &value["referenced_message"];
            Reply::Message {
                id: original["id"].as_str().unwrap_or_default().into(),
                author_id: original["author"]["id"].as_str().unwrap_or_default().into(),
                author: author_name(original).into(),
                content: original["content"].as_str().unwrap_or_default().into(),
            }
        }
        _ => Reply::Deleted,
    });
    let attachments = shown["attachments"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|attachment| {
            let kind = attachment["content_type"].as_str().unwrap_or_default();
            let image = kind.starts_with("image/");
            // The media proxy makes a video's poster from its first frame.
            let video = kind.starts_with("video/");
            let url: Arc<str> = attachment["url"].as_str()?.into();
            let name = attachment["filename"].as_str()?;
            Some(Attachment {
                name: name.into(),
                size: attachment["size"].as_u64(),
                image,
                dimensions: (image || video)
                    .then(|| {
                        let side = |name: &str| u32::try_from(attachment[name].as_u64()?).ok();
                        Some((side("width")?, side("height")?))
                    })
                    .flatten(),
                preview_url: (image || video)
                    .then(|| scaled_url(attachment, PREVIEW_WIDTH, PREVIEW_HEIGHT, image))
                    .flatten(),
                view_url: image
                    .then(|| scaled_url(attachment, VIEW_WIDTH, VIEW_HEIGHT, true))
                    .flatten(),
                video_url: video.then(|| url.to_string()),
                link_url: None,
                animated: image && animated(attachment),
                url,
                still_url: None,
                shown: Shown::Message,
                spoiler: name.starts_with("SPOILER_")
                    || attachment["flags"].as_u64().unwrap_or_default() & IS_SPOILER != 0,
            })
        })
        .collect::<Vec<_>>();
    let reactions = value["reactions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|reaction| {
            let (key, emoji) = emoji(&reaction["emoji"])?;
            Some(Reaction {
                key,
                emoji,
                count: reaction["count"].as_u64().unwrap_or(1) as usize,
                me: reaction["me"].as_bool().unwrap_or(false),
            })
        })
        .collect();
    let mut content = shown["content"].as_str().unwrap_or_default().to_owned();
    let embeds = if shown["flags"].as_u64().unwrap_or_default() & SUPPRESS_EMBEDS == 0 {
        embed::parse(&shown["embeds"])
    } else {
        embed::parse(&Value::Null)
    };
    if embeds.hides.iter().any(|link| link == content.trim()) {
        content.clear();
    }
    // A message of components alone shows its files only where its
    // components place them.
    let components = components::parse(&shown["components"]);
    let mut attachments =
        if shown["flags"].as_u64().unwrap_or_default() & components::IS_COMPONENTS_V2 == 0 {
            attachments
        } else {
            Vec::new()
        };
    // A link in a spoiler spoils its preview too.
    let spoiled = spoils_links(&content);
    attachments.extend(embeds.pictures.into_iter().map(|mut picture| {
        picture.spoiler |= spoiled;
        picture
    }));
    // All sticker formats use the picture pipeline, including Lottie JSON.
    for sticker in shown["sticker_items"].as_array().into_iter().flatten() {
        let (Some(id), Some(name)) = (sticker["id"].as_str(), sticker["name"].as_str()) else {
            continue;
        };
        let format = sticker["format_type"].as_u64().unwrap_or(STICKER_PNG);
        let url = ready::sticker(id, format);
        attachments.push(Attachment {
            name: name.into(),
            size: None,
            url: url.as_str().into(),
            image: true,
            dimensions: Some((STICKER_SIZE, STICKER_SIZE)),
            preview_url: Some(url.clone()),
            view_url: Some(url),
            still_url: None,
            video_url: None,
            link_url: None,
            animated: false,
            shown: Shown::Sticker,
            spoiler: false,
        });
    }
    attachments.extend(components.media);
    Some(Message {
        id: value["id"].as_str()?.into(),
        channel_id: value["channel_id"].as_str().unwrap_or_default().into(),
        nonce: match &value["nonce"] {
            Value::String(nonce) => Some(nonce.clone()),
            Value::Number(nonce) => Some(nonce.to_string()),
            _ => None,
        },
        author_id: author["id"].as_str()?.into(),
        author: author_name(value).into(),
        bot: author["bot"].as_bool().unwrap_or(false),
        content,
        notice: system.as_ref().and_then(|_| match kind {
            PINNED => Some(Notice::Pinned(
                value["message_reference"]["message_id"]
                    .as_str()
                    .map(str::to_owned),
            )),
            MEMBER_JOIN => Some(Notice::Joined),
            THREAD_CREATED => Some(Notice::ThreadStarted),
            CALL => Some(Notice::Call),
            POLL_RESULT => Some(Notice::PollResult),
            _ => None,
        }),
        system,
        edited: value["edited_timestamp"].is_string(),
        reply,
        attachments,
        embeds: embeds.embeds,
        components: components.cards,
        reactions,
        // A poll's question and answers stay as they were sent.
        editable: matches!(kind, DEFAULT | REPLY) && !forwarded && value["poll"].is_null(),
        mentions: ids(&value["mentions"], |user| user["id"].as_str()),
        mention_roles: ids(&value["mention_roles"], Value::as_str),
        mention_everyone: value["mention_everyone"].as_bool().unwrap_or(false),
        thread: match kind {
            // The notice names the thread in its reference, and its name
            // in its content.
            THREAD_CREATED => {
                value["message_reference"]["channel_id"]
                    .as_str()
                    .map(|id| ThreadLink {
                        id: id.to_owned(),
                        name: value["content"].as_str().unwrap_or_default().to_owned(),
                        ..Default::default()
                    })
            }
            // A forum post's first message has the post's id, and links
            // to no thread but the one it's in.
            _ => ThreadLink::of(&value["thread"])
                .filter(|thread| value["channel_id"].as_str() != Some(thread.id.as_str())),
        },
        pinned: value["pinned"].as_bool().unwrap_or(false),
        forwarded,
        forwardable: written(kind)
            && value["flags"].as_u64().unwrap_or_default() & !FORWARDABLE_FLAGS == 0
            && [
                "poll",
                "call",
                "activity",
                "activity_instance",
                "shared_client_theme",
            ]
            .iter()
            .all(|field| value[*field].is_null()),
        undeletable: kind == CALL,
        invoker: invoker(&value["interaction_metadata"]),
        poll: Poll::of(&shown["poll"]),
    })
}

/// Who ran the command an app's message answers, when the app answered
/// as installed on their account alone, as Discord's client lets them
/// delete it.
fn invoker(interaction: &Value) -> Option<String> {
    let owners = interaction["authorizing_integration_owners"].as_object()?;
    if owners.len() != 1 || !owners.contains_key(USER_INSTALL) {
        return None;
    }
    interaction["user"]["id"].as_str().map(str::to_owned)
}

/// The notice of a closed poll, from the fields of its embed, said after
/// its author's name as Discord's client says it: "'s poll … has closed."
fn poll_result(embed: &Value) -> String {
    let field = |name: &str| {
        embed["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|field| field["name"] == name)
            .and_then(|field| field["value"].as_str())
    };
    let question = field("poll_question_text").unwrap_or_default();
    let total = field("total_votes").and_then(|votes| votes.parse::<u64>().ok());
    let winner = match (field("victor_answer_text"), field("victor_answer_votes")) {
        (Some(text), Some(votes)) => {
            let votes: u64 = votes.parse().unwrap_or_default();
            let emoji = field("victor_answer_emoji_name")
                .filter(|name| !name.is_empty() && field("victor_answer_emoji_id").is_none())
                .map(|name| format!("{name} "))
                .unwrap_or_default();
            let share = total
                .filter(|total| *total > 0)
                .map(|total| format!(" ({}%)", (votes * 100 + total / 2) / total))
                .unwrap_or_default();
            format!(
                "Winning answer: {emoji}{text}, {}{share}.",
                votes_text(votes)
            )
        }
        _ => "There was no winner.".to_owned(),
    };
    format!("'s poll \u{201c}{question}\u{201d} has closed. {winner}")
}

/// "1 vote", "3 votes".
pub fn votes_text(votes: u64) -> String {
    if votes == 1 {
        "1 vote".to_owned()
    } else {
        format!("{votes} votes")
    }
}

/// The ids `id` finds in a list.
fn ids<'a>(list: &'a Value, id: impl Fn(&'a Value) -> Option<&'a str>) -> Vec<String> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(id)
        .map(str::to_owned)
        .collect()
}

/// Whether a spoiler in `content` holds a link.
fn spoils_links(content: &str) -> bool {
    let parts: Vec<&str> = content.split("||").collect();
    let pairs = parts.len().saturating_sub(1) / 2;
    parts
        .iter()
        .skip(1)
        .step_by(2)
        .take(pairs)
        .any(|inside| inside.contains("http"))
}

/// The size Yapfast shows image previews at, at most.
pub const PREVIEW_WIDTH: u64 = 360;
pub const PREVIEW_HEIGHT: u64 = 270;

/// The image viewer's largest size.
pub const VIEW_WIDTH: u64 = 1920;
pub const VIEW_HEIGHT: u64 = 1200;

/// Asks Discord's media proxy for the image fitted into `max_width` by
/// `max_height` and never enlarged, as the official client does, so a phone
/// photo doesn't download in full. It comes as WebP, but a GIF stays a GIF,
/// so it moves. With `animated`, as Discord's client asks, an animated
/// picture, such as an AVIF, comes as an animated WebP rather than its first
/// frame; a video's poster asks without, for its first frame alone.
pub(crate) fn scaled_url(
    attachment: &Value,
    max_width: u64,
    max_height: u64,
    animated: bool,
) -> Option<String> {
    let url = attachment["proxy_url"].as_str()?;
    let (width, height) = (
        attachment["width"].as_u64()?,
        attachment["height"].as_u64()?,
    );
    if width == 0 || height == 0 {
        return None;
    }
    let scale = f64::min(
        1.0,
        f64::min(
            max_width as f64 / width as f64,
            max_height as f64 / height as f64,
        ),
    );
    let fit = |side: u64| ((side as f64 * scale).round() as u64).max(1);
    let separator = if url.contains('?') { '&' } else { '?' };
    let gif = attachment["content_type"].as_str() == Some("image/gif")
        || url
            .split('?')
            .next()
            .is_some_and(|path| path.ends_with(".gif"));
    let format = match (gif, animated) {
        (true, _) => "",
        (false, true) => "animated=true&format=webp&",
        (false, false) => "format=webp&",
    };
    Some(format!(
        "{url}{separator}{format}width={}&height={}",
        fit(width),
        fit(height)
    ))
}

/// A custom emoji a message shows: in its text as `<:name:id>`, or `<a:name:id>`
/// when animated, or as a reaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomEmoji {
    pub name: String,
    pub id: String,
    pub animated: bool,
}

/// The custom emoji in a message's text, each once, in order.
pub fn custom_emoji(content: &str) -> Vec<CustomEmoji> {
    let mut found: Vec<CustomEmoji> = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        let (animated, emoji) = match tag.strip_prefix("a:") {
            Some(emoji) => (true, emoji),
            None => (false, tag.strip_prefix(':').unwrap_or_default()),
        };
        if let Some((name, id)) = emoji.split_once(':')
            && !name.is_empty()
            && !id.is_empty()
            && id.bytes().all(|byte| byte.is_ascii_digit())
            && !found.iter().any(|known| known.id == id)
        {
            found.push(CustomEmoji {
                name: name.to_owned(),
                id: id.to_owned(),
                animated,
            });
        }
    }
    found
}

/// A reaction's emoji object as Yapfast's key and the API's form.
pub fn emoji(emoji: &Value) -> Option<(String, String)> {
    let name = emoji["name"].as_str()?;
    Some(match emoji["id"].as_str() {
        Some(id) => (format!(":{name}:"), format!("{name}:{id}")),
        None => (name.to_owned(), name.to_owned()),
    })
}

/// The name a message shows for its author: the server nickname it carries,
/// else the display name or username.
fn author_name(message: &Value) -> &str {
    message["member"]["nick"]
        .as_str()
        .or_else(|| ready::display_name(&message["author"]))
        .unwrap_or("Discord user")
}

/// [`Names`] as saved to disk, where maps keyed by server and user become
/// lists.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct SavedNames {
    users: Vec<(String, String)>,
    nicknames: Vec<(String, String, String)>,
    members: Vec<(String, String)>,
    avatars: Vec<(String, String)>,
    member_avatars: Vec<(String, String, String)>,
    roles: Vec<(String, String, Vec<String>)>,
    #[serde(default)]
    usernames: Vec<(String, String)>,
    #[serde(default)]
    bots: Vec<String>,
}

/// [`Names`] serialized as [`SavedNames`]; see [`Names::saving`].
pub struct SavingNames<'a>(&'a Names);

impl serde::Serialize for SavingNames<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        /// A list serialized from the iterator its function makes.
        struct List<F>(F);
        impl<F: Fn() -> I, I: Iterator<Item: serde::Serialize>> serde::Serialize for List<F> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_seq((self.0)())
            }
        }
        let names = self.0;
        let mut saved = serializer.serialize_struct("SavedNames", 8)?;
        saved.serialize_field(
            "users",
            &List(|| names.users.iter().map(|(user, name)| (Text(*user), name))),
        )?;
        saved.serialize_field(
            "nicknames",
            &List(|| {
                names
                    .nicknames
                    .iter()
                    .map(|((guild, user), nick)| (Text(*guild), Text(*user), nick))
            }),
        )?;
        saved.serialize_field(
            "members",
            &List(|| {
                names
                    .members
                    .iter()
                    .map(|(guild, user)| (Text(*guild), Text(*user)))
            }),
        )?;
        saved.serialize_field(
            "avatars",
            &List(|| names.avatars.iter().map(|(user, hash)| (Text(*user), hash))),
        )?;
        saved.serialize_field(
            "member_avatars",
            &List(|| {
                names
                    .member_avatars
                    .iter()
                    .map(|((guild, user), avatar)| (Text(*guild), Text(*user), avatar))
            }),
        )?;
        saved.serialize_field(
            "roles",
            &List(|| {
                names
                    .roles
                    .iter()
                    .map(|((guild, user), roles)| (Text(*guild), Text(*user), roles))
            }),
        )?;
        saved.serialize_field(
            "usernames",
            &List(|| {
                names
                    .usernames
                    .iter()
                    .map(|(user, name)| (Text(*user), name))
            }),
        )?;
        saved.serialize_field("bots", &List(|| names.bots.iter().copied().map(Text)))?;
        saved.end()
    }
}

/// A Discord id, a snowflake, as [`Names`] keeps people and servers by:
/// eight bytes, where its text takes 24 and an allocation of its own.
type Id = u64;

fn id(text: &str) -> Option<Id> {
    text.parse().ok()
}

/// A server's and a member's ids.
fn member_key(guild: &str, user: &str) -> Option<(Id, Id)> {
    Some((id(guild)?, id(user)?))
}

/// An id saved as the text Discord sends it as, as the file always had it.
struct Text(Id);

impl serde::Serialize for Text {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

/// Who is who, learned from the gateway and from messages.
#[derive(Default)]
pub struct Names {
    /// Display names by user id.
    users: HashMap<Id, String>,
    /// Usernames by user id, such as `ann` for Ann.
    usernames: HashMap<Id, String>,
    /// The bots and apps among them, which nobody invites to a server.
    bots: HashSet<Id>,
    /// Server nicknames by server and user id.
    nicknames: HashMap<(Id, Id), String>,
    /// Members whose server profile is known, with or without a nickname.
    members: HashSet<(Id, Id)>,
    /// Avatar hashes by user id, of those who set one.
    avatars: HashMap<Id, String>,
    /// Server avatar hashes by server and user id.
    member_avatars: HashMap<(Id, Id), String>,
    /// Members' roles by server and user id, for the colour of their name.
    /// Member lists and the core share each, rather than copying it.
    roles: HashMap<(Id, Id), Arc<[String]>>,
    /// When members' timeouts end, in Unix milliseconds, by server and user
    /// id: only those timed out, while Yapfast runs.
    timeouts: HashMap<(Id, Id), u64>,
    /// Changes whenever a name, avatar or role of someone known changes, so
    /// what was shown with them can tell it's out of date. Someone new
    /// changes nothing shown, which was rendered after learning them.
    version: u64,
    /// When each person was last learned or looked up, by user id, counted
    /// in `ticks`; those not seen for longest go first once there are too
    /// many. Looking up only reads, so these count atomically.
    seen: HashMap<Id, AtomicU64>,
    ticks: AtomicU64,
}

/// People whose names are kept, at most. Yapfast shows the authors of the
/// channels it keeps, 41 of up to 200 messages each, and 40 member lists of
/// 100 rows; every message Discord sends from any server teaches it one
/// more. One person in one server, with two roles, takes about 260 bytes.
const KEPT_PEOPLE: usize = 20_000;

/// Sets `key` to `value`, or removes it for none; whether that changed it.
fn changed<K: std::hash::Hash + Eq, V: PartialEq>(
    map: &mut HashMap<K, V>,
    key: K,
    value: Option<V>,
) -> bool {
    match value {
        Some(value) if map.get(&key) == Some(&value) => false,
        Some(value) => {
            map.insert(key, value);
            true
        }
        None => map.remove(&key).is_some(),
    }
}

impl Names {
    /// These names as [`SavedNames`], serialized straight from the maps
    /// rather than from a copy of them: 20,000 people take 17 MiB.
    pub fn saving(&self) -> SavingNames<'_> {
        SavingNames(self)
    }

    pub fn restore(saved: SavedNames) -> Self {
        let keyed = |list: Vec<(String, String, String)>| {
            list.into_iter()
                .filter_map(|(guild, user, value)| Some((member_key(&guild, &user)?, value)))
                .collect()
        };
        let by_user = |list: Vec<(String, String)>| {
            list.into_iter()
                .filter_map(|(user, value)| Some((id(&user)?, value)))
                .collect()
        };
        let mut names = Self {
            users: by_user(saved.users),
            nicknames: keyed(saved.nicknames),
            members: saved
                .members
                .iter()
                .filter_map(|(guild, user)| member_key(guild, user))
                .collect(),
            avatars: by_user(saved.avatars),
            member_avatars: keyed(saved.member_avatars),
            roles: saved
                .roles
                .into_iter()
                .filter_map(|(guild, user, roles)| Some((member_key(&guild, &user)?, roles.into())))
                .collect(),
            usernames: by_user(saved.usernames),
            bots: saved.bots.iter().filter_map(|user| id(user)).collect(),
            ..Self::default()
        };
        // Those kept from before go before anyone learned since.
        let people: HashSet<Id> = names
            .users
            .keys()
            .chain(names.avatars.keys())
            .chain(names.usernames.keys())
            .chain(names.members.iter().map(|(_, user)| user))
            .copied()
            .collect();
        names.seen = people
            .into_iter()
            .zip(1..)
            .map(|(user, at)| (user, AtomicU64::new(at)))
            .collect();
        names.ticks = AtomicU64::new(names.seen.len() as u64);
        if names.seen.len() > KEPT_PEOPLE {
            names.forget();
        }
        names
    }

    /// Notes that `user` was looked up just now; whether they were known.
    fn touch(&self, user: Id) -> bool {
        let Some(at) = self.seen.get(&user) else {
            return false;
        };
        at.store(self.ticks.fetch_add(1, Relaxed) + 1, Relaxed);
        true
    }

    /// Notes that `user` was learned just now, and forgets the quarter of
    /// people not seen for longest once there are too many; whether they
    /// were known before.
    fn saw(&mut self, user: Id) -> bool {
        if self.touch(user) {
            return true;
        }
        let now = self.ticks.fetch_add(1, Relaxed) + 1;
        self.seen.insert(user, AtomicU64::new(now));
        if self.seen.len() > KEPT_PEOPLE {
            self.forget();
        }
        false
    }

    /// Forgets the quarter of people not seen for longest. What was shown
    /// with them stays as it was, so the version stays.
    fn forget(&mut self) {
        let mut times: Vec<u64> = self.seen.values().map(|at| at.load(Relaxed)).collect();
        let quarter = times.len() / 4;
        let (_, &mut until, _) = times.select_nth_unstable(quarter);
        self.seen.retain(|_, at| *at.get_mut() > until);
        let seen = &self.seen;
        self.users.retain(|user, _| seen.contains_key(user));
        self.usernames.retain(|user, _| seen.contains_key(user));
        self.bots.retain(|user| seen.contains_key(user));
        self.avatars.retain(|user, _| seen.contains_key(user));
        self.nicknames
            .retain(|(_, user), _| seen.contains_key(user));
        self.members.retain(|(_, user)| seen.contains_key(user));
        self.member_avatars
            .retain(|(_, user), _| seen.contains_key(user));
        self.roles.retain(|(_, user), _| seen.contains_key(user));
        self.timeouts.retain(|(_, user), _| seen.contains_key(user));
    }

    /// Changes whenever what [`Names::name`], [`Names::avatar`],
    /// [`Names::roles`] or [`Names::render`] give may have.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn learn_user(&mut self, user: &Value) {
        let Some(id) = user["id"].as_str().and_then(id) else {
            return;
        };
        let known = self.saw(id);
        let mut learned = false;
        if let Some(name) = ready::display_name(user) {
            learned |= changed(&mut self.users, id, Some(name.to_owned()));
        }
        if let Some(username) = user["username"].as_str() {
            self.usernames.insert(id, username.to_owned());
        }
        // Partial user objects leave it out.
        match user["bot"].as_bool() {
            Some(true) => {
                self.bots.insert(id);
            }
            Some(false) => {
                self.bots.remove(&id);
            }
            None => {}
        }
        // Partial user objects leave the avatar out; null removes it.
        match user.get("avatar") {
            Some(Value::String(hash)) => {
                learned |= changed(&mut self.avatars, id, Some(hash.clone()));
            }
            Some(Value::Null) => learned |= changed(&mut self.avatars, id, None),
            _ => {}
        }
        self.version += u64::from(known && learned);
    }

    /// Learns a member object; its user id is in `user` or `user_id`.
    pub fn learn_member(&mut self, guild: &str, member: &Value) {
        let user = member["user_id"]
            .as_str()
            .or_else(|| member["user"]["id"].as_str())
            .and_then(id);
        let known = user.is_some_and(|user| self.seen.contains_key(&user));
        self.learn_user(&member["user"]);
        let (Some(user), Some(guild)) = (user, id(guild)) else {
            return;
        };
        self.saw(user);
        let key = (guild, user);
        let nick = member["nick"].as_str().map(str::to_owned);
        let mut learned = changed(&mut self.nicknames, key, nick);
        let avatar = member["avatar"].as_str().map(str::to_owned);
        learned |= changed(&mut self.member_avatars, key, avatar);
        // Some member objects leave the roles out.
        if let Some(roles) = member["roles"].as_array() {
            let roles: Arc<[String]> = roles
                .iter()
                .filter_map(|role| role.as_str().map(str::to_owned))
                .collect();
            learned |= changed(&mut self.roles, key, Some(roles));
        }
        // Present when known, null when they aren't timed out; a time past
        // is a timeout that ended.
        if let Some(until) = member.get("communication_disabled_until") {
            let until = until.as_str().and_then(iso_ms);
            changed(&mut self.timeouts, key, until);
        }
        self.members.insert(key);
        self.version += u64::from(known && learned);
    }

    /// Forgets what's known of someone as a member of a server they left,
    /// were kicked from or banned from.
    pub fn forget_member(&mut self, guild: &str, user: &str) {
        let Some(key) = member_key(guild, user) else {
            return;
        };
        let learned = self.nicknames.remove(&key).is_some()
            | self.member_avatars.remove(&key).is_some()
            | self.roles.remove(&key).is_some();
        self.timeouts.remove(&key);
        self.members.remove(&key);
        self.version += u64::from(learned);
    }

    /// When a member's timeout ends, in Unix milliseconds, if they're timed
    /// out as far as Yapfast knows; it may have ended already.
    pub fn timed_out_until(&self, guild: &str, user: &str) -> Option<u64> {
        self.timeouts.get(&member_key(guild, user)?).copied()
    }

    /// The members known to hold `role` in a server, by user id.
    pub fn members_with_role(&self, guild: &str, role: &str) -> Vec<String> {
        let Some(guild) = id(guild) else {
            return Vec::new();
        };
        let mut members: Vec<String> = self
            .roles
            .iter()
            .filter(|((server, _), roles)| {
                *server == guild && roles.iter().any(|held| held == role)
            })
            .map(|((_, user), _)| user.to_string())
            .collect();
        members.sort();
        members
    }

    /// A member's roles in a server, as far as they're known.
    pub fn roles(&self, guild: &str, user: &str) -> &[String] {
        self.shared(guild, user).map_or(&[], |roles| roles)
    }

    /// A member's roles in a server, as far as they're known, shared rather
    /// than copied; none when they aren't known.
    pub fn shared_roles(&self, guild: &str, user: &str) -> Option<Arc<[String]>> {
        self.shared(guild, user).cloned()
    }

    fn shared(&self, guild: &str, user: &str) -> Option<&Arc<[String]>> {
        let key = member_key(guild, user)?;
        self.touch(key.1);
        self.roles.get(&key)
    }

    /// Learns the author and the mentioned users of a message object, and
    /// those of the message it replies to.
    pub fn learn_message(&mut self, guild: Option<&str>, message: &Value) {
        self.learn_user(&message["author"]);
        // Discord doesn't nest a reply's original inside the original.
        if message["referenced_message"].is_object() {
            self.learn_message(guild, &message["referenced_message"]);
        }
        if let (Some(guild), Value::Object(member)) = (guild, &message["member"]) {
            let mut member = Value::Object(member.clone());
            member["user"] = message["author"].clone();
            self.learn_member(guild, &member);
        }
        for user in message["mentions"].as_array().into_iter().flatten() {
            self.learn_user(user);
            if let (Some(guild), Value::Object(member)) = (guild, &user["member"]) {
                let mut member = Value::Object(member.clone());
                member["user"] = user.clone();
                self.learn_member(guild, &member);
            }
        }
    }

    /// Whether a user is a bot or an app, as far as it's known.
    pub fn is_bot(&self, user: &str) -> bool {
        id(user).is_some_and(|user| self.bots.contains(&user))
    }

    pub fn knows_member(&self, guild: &str, user: &str) -> bool {
        member_key(guild, user).is_some_and(|key| self.members.contains(&key))
    }

    /// Known users whose name starts with `query`, or has a later word that does,
    /// by id; any letter case.
    pub fn find_users(&self, query: &str) -> Vec<String> {
        let query = query.to_lowercase();
        let mut found: Vec<(bool, &String, String)> = self
            .users
            .iter()
            .filter_map(|(id, name)| {
                let lower = name.to_lowercase();
                let starts = lower.starts_with(&query);
                let later = lower
                    .split_whitespace()
                    .skip(1)
                    .any(|word| word.starts_with(&query));
                (starts || later).then(|| (!starts, name, id.to_string()))
            })
            .collect();
        found.sort();
        found.into_iter().map(|(_, _, id)| id).collect()
    }

    /// A user's display name or username, outside any server.
    pub fn user_name(&self, user: &str) -> Option<&str> {
        let user = id(user)?;
        self.touch(user);
        self.users.get(&user).map(String::as_str)
    }

    /// A user's username, such as `ann`.
    pub fn username(&self, user: &str) -> Option<&str> {
        self.usernames.get(&id(user)?).map(String::as_str)
    }

    /// A member's nickname in a server, when they set one.
    pub fn nickname(&self, guild: &str, user: &str) -> Option<&str> {
        self.nicknames
            .get(&member_key(guild, user)?)
            .map(String::as_str)
    }

    pub fn name(&self, guild: &str, user: &str) -> Option<&str> {
        let user_id = id(user)?;
        self.touch(user_id);
        id(guild)
            .and_then(|guild| self.nicknames.get(&(guild, user_id)))
            .or_else(|| self.users.get(&user_id))
            .map(String::as_str)
    }

    /// The address of the avatar Discord shows for a user: their server
    /// avatar in `guild`, else their own, else a default one.
    pub fn avatar(&self, guild: Option<&str>, user: &str) -> String {
        let user_id = id(user);
        if let Some(user_id) = user_id {
            self.touch(user_id);
        }
        if let Some(guild) = guild
            && let Some(hash) =
                member_key(guild, user).and_then(|key| self.member_avatars.get(&key))
        {
            return ready::member_avatar(guild, user, hash);
        }
        let hash = user_id.and_then(|user| self.avatars.get(&user));
        ready::avatar(user, hash.map(String::as_str))
    }

    /// Discord Markdown for Yapfast: mentions, custom emoji and timestamps
    /// become text.
    pub fn render(&self, server: Option<&Server>, content: &str) -> String {
        self.render_tags(server, content, |_, _, _| {})
    }

    /// [`Names::render`], with the users and roles the message mentions:
    /// each with the text it shows for them and where that text starts, each
    /// time, as Discord's client draws a mention where its tag was, and
    /// where `@everyone` or `@here` is.
    pub fn render_mentions(
        &self,
        server: Option<&Server>,
        content: &str,
    ) -> (String, Vec<Mention>) {
        let mut mentions: Vec<Mention> = Vec::new();
        let rendered = self.render_tags(server, content, |tag, at, text| {
            let target = if tag == "@everyone" || tag == "@here" {
                MentionTarget::Everyone
            } else if let Some(id) = tag.strip_prefix("@&") {
                MentionTarget::Role(id.to_owned())
            } else if let Some(id) = tag.strip_prefix("@!").or_else(|| tag.strip_prefix('@')) {
                MentionTarget::User(id.to_owned())
            } else {
                return;
            };
            match mentions
                .iter_mut()
                .find(|known| known.target == target && known.text == text)
            {
                Some(known) => known.at.push(at),
                None => mentions.push(Mention {
                    text: text.to_owned(),
                    target,
                    at: vec![at],
                }),
            }
        });
        (rendered, mentions)
    }

    /// [`Names::render`], telling `rendered` of each tag shown as text: the
    /// tag, where its text starts and the text. `@everyone` and `@here` are
    /// tags of their own, as Discord's client finds them anywhere but after
    /// a backslash.
    fn render_tags(
        &self,
        server: Option<&Server>,
        content: &str,
        mut rendered_tag: impl FnMut(&str, usize, &str),
    ) -> String {
        let guild = server.map_or("", |server| server.id.as_str());
        let mut rendered = String::with_capacity(content.len());
        let mut rest = content;
        while let Some(start) = rest.find('<') {
            push_text(&mut rendered, &rest[..start], &mut rendered_tag);
            rest = &rest[start..];
            let Some(end) = rest.find('>') else {
                break;
            };
            let tag = &rest[1..end];
            match self.tag(server, guild, tag) {
                Some(text) => {
                    rendered_tag(tag, rendered.len(), &text);
                    rendered.push_str(&text);
                    rest = &rest[end + 1..];
                }
                None => {
                    rendered.push('<');
                    rest = &rest[1..];
                }
            }
        }
        push_text(&mut rendered, rest, &mut rendered_tag);
        rendered
    }

    fn tag(&self, server: Option<&Server>, guild: &str, tag: &str) -> Option<String> {
        let digits = |id: &str| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit());
        if let Some(id) = tag.strip_prefix("@&") {
            digits(id).then(|| {
                let name = server.and_then(|server| server.role_names.get(id));
                format!("@{}", name.map_or("role", String::as_str))
            })
        } else if let Some(id) = tag.strip_prefix("@!").or_else(|| tag.strip_prefix('@')) {
            digits(id).then(|| format!("@{}", self.name(guild, id).unwrap_or("Discord user")))
        } else if let Some(id) = tag.strip_prefix('#') {
            digits(id).then(|| {
                // Anyone who may start a thread names it, and the name
                // shows inside others' Markdown: it's escaped to show as
                // typed.
                let name = server.and_then(|server| server.channel_name(id));
                format!("#{}", escape_markdown(name.unwrap_or("channel")))
            })
        } else if let Some(emoji) = tag.strip_prefix("a:").or_else(|| tag.strip_prefix(':')) {
            let (name, id) = emoji.split_once(':')?;
            (digits(id) && !name.is_empty()).then(|| format!(":{name}:"))
        } else if let Some(time) = tag.strip_prefix("t:") {
            let seconds = time.split(':').next()?.parse::<i64>().ok()?;
            Some(date_time(seconds))
        } else {
            None
        }
    }
}

/// Pushes text outside tags onto `rendered`, telling `rendered_tag` of each
/// `@everyone` and `@here` in it that no backslash escapes.
fn push_text(rendered: &mut String, text: &str, rendered_tag: &mut impl FnMut(&str, usize, &str)) {
    for (at, _) in text.match_indices('@') {
        let escaped = text[..at]
            .bytes()
            .rev()
            .take_while(|&byte| byte == b'\\')
            .count()
            % 2
            == 1;
        let group = ["@everyone", "@here"]
            .into_iter()
            .find(|group| text[at..].starts_with(group));
        if let Some(group) = group
            && !escaped
        {
            rendered_tag(group, rendered.len() + at, group);
        }
    }
    rendered.push_str(text);
}

/// Someone or something a message mentions, as [`Names::render_mentions`]
/// finds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mention {
    /// The text the message shows for them, such as `@Ann`.
    pub text: String,
    pub target: MentionTarget,
    /// Where the text starts in the rendered message, a byte offset each
    /// time it mentions them.
    pub at: Vec<usize>,
}

/// Who a [`Mention`] is of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MentionTarget {
    /// A user, by id.
    User(String),
    /// A role, by id.
    Role(String),
    /// `@everyone` or `@here`.
    Everyone,
}

/// `text` with Markdown's punctuation escaped by backslashes.
fn escape_markdown(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        if character.is_ascii_punctuation() {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// A Unix time in the local time zone, such as `2026-10-06 14:05`.
fn date_time(seconds: i64) -> String {
    local(seconds).format("%Y-%m-%d %H:%M").to_string()
}

/// A day in Unix milliseconds as profiles show it, in the local time zone: `6 Oct 2026`.
pub fn date(ms: u64) -> String {
    local(i64::try_from(ms / 1000).unwrap_or(i64::MAX))
        .format("%-d %b %Y")
        .to_string()
}

fn local(seconds: i64) -> chrono::DateTime<chrono::Local> {
    chrono::DateTime::from_timestamp(seconds, 0)
        .unwrap_or_default()
        .with_timezone(&chrono::Local)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_pin_notice_links_to_the_message_pinned() {
        let notice = parse(&json!({
            "id": "5", "channel_id": "1", "type": 6, "content": "",
            "author": {"id": "2", "username": "bob"},
            "message_reference": {"channel_id": "1", "message_id": "4"},
        }))
        .unwrap();
        assert_eq!(notice.system.as_deref(), Some("pinned a message."));
        assert_eq!(notice.notice, Some(Notice::Pinned(Some("4".into()))));
        let joined = parse(&json!({
            "id": "6", "channel_id": "1", "type": 7, "content": "",
            "author": {"id": "2", "username": "bob"},
        }))
        .unwrap();
        assert_eq!(joined.notice, Some(Notice::Joined));
    }

    #[test]
    fn links_threads_and_shows_a_threads_first_message() {
        let started = parse(&json!({"id": "500", "type": 0, "channel_id": "11",
            "author": {"id": "2", "username": "bob"}, "content": "Who's in?",
            "thread": {"id": "500", "type": 11, "name": "Saturday", "message_count": 4,
                       "thread_metadata": {"archived": true}}}))
        .unwrap();
        assert_eq!(
            started.thread,
            Some(ThreadLink {
                id: "500".into(),
                name: "Saturday".into(),
                messages: 4,
                archived: true,
            })
        );
        let notice = parse(&json!({"id": "600", "type": 18, "channel_id": "11",
            "author": {"id": "2", "username": "bob"}, "content": "Sunday",
            "message_reference": {"channel_id": "700", "guild_id": "10"}}))
        .unwrap();
        assert_eq!(notice.system.as_deref(), Some("started a thread: Sunday."));
        assert_eq!(
            notice.thread.map(|thread| thread.id).as_deref(),
            Some("700")
        );
        // In the thread, its first message is the one it was started from.
        let first = parse(&json!({"id": "501", "type": 21, "channel_id": "500",
            "author": {"id": "2", "username": "bob"}, "content": "",
            "message_reference": {"channel_id": "11", "message_id": "500"},
            "referenced_message": {"id": "500", "type": 0, "channel_id": "11",
                "author": {"id": "2", "username": "bob"}, "content": "Who's in?",
                "thread": {"id": "500", "name": "Saturday"}}}))
        .unwrap();
        assert_eq!(
            (first.id.as_str(), first.channel_id.as_str()),
            ("501", "500")
        );
        assert_eq!(first.content, "Who's in?");
        assert!(first.thread.is_none() && !first.editable && first.undeletable);
        assert!(
            parse(&json!({"id": "502", "type": 21, "channel_id": "500",
            "author": {"id": "2"}, "referenced_message": null}))
            .is_none()
        );
        // A forum post's first message has the post's id, and links to no
        // thread but the one it's in.
        let post = parse(&json!({"id": "800", "type": 0, "channel_id": "800",
            "author": {"id": "2", "username": "bob"}, "content": "An idea",
            "thread": {"id": "800", "name": "Idea"}}))
        .unwrap();
        assert!(post.thread.is_none());
    }

    #[test]
    fn knows_who_may_delete_an_apps_answer_and_a_call() {
        let answer = |owners: Value| {
            parse(&json!({"id": "900", "type": 20, "channel_id": "11",
                "author": {"id": "5", "username": "app", "bot": true}, "content": "Pong",
                "interaction_metadata": {"id": "901", "type": 2,
                    "user": {"id": "1", "username": "me"},
                    "authorizing_integration_owners": owners}}))
            .unwrap()
        };
        let mine = answer(json!({"1": "1"}));
        assert_eq!(mine.invoker.as_deref(), Some("1"));
        assert!(!mine.undeletable);
        assert_eq!(
            answer(json!({"0": "10", "1": "1"})).invoker,
            None,
            "installed in the server too"
        );
        assert_eq!(answer(json!({"0": "10"})).invoker, None);
        let call = parse(&json!({"id": "902", "type": 3, "channel_id": "11",
            "author": {"id": "1"}, "call": {"participants": ["1"]}}))
        .unwrap();
        assert!(call.undeletable && call.invoker.is_none());
    }

    #[test]
    fn says_how_long_calls_lasted() {
        // Two minutes after Discord's epoch, as a snowflake.
        let id = snowflake_at(DISCORD_EPOCH_MS + 120_000).to_string();
        let call = |ended: Value| {
            parse(
                &json!({"id": id, "type": 3, "author": {"id": "1"}, "call": {
                    "participants": ["1"], "ended_timestamp": ended,
                }}),
            )
            .and_then(|message| message.system)
        };
        assert_eq!(call(Value::Null).as_deref(), Some("started a call."));
        assert_eq!(
            call(json!("2015-01-01T00:07:10+00:00")).as_deref(),
            Some("started a call that lasted 5 minutes.")
        );
        assert_eq!(lasted(10_000), "a few seconds");
        assert_eq!(lasted(50 * 60_000), "an hour");
        assert_eq!(lasted(150 * 60_000), "3 hours");
    }

    #[test]
    fn parses_stickers_and_spoilers() {
        let message = parse(&json!({
            "id": "1300000000000000000",
            "type": 0,
            "content": "look ||https://example.com/cat.gif||",
            "author": {"id": "1", "username": "me"},
            "attachments": [
                {"filename": "SPOILER_cat.png", "url": "https://cdn/c.png", "content_type": "image/png"},
                {"filename": "dog.png", "flags": 8, "url": "https://cdn/d.png", "content_type": "image/png"},
                {"filename": "plain.png", "url": "https://cdn/p.png", "content_type": "image/png"},
            ],
            "embeds": [{"type": "image", "url": "https://example.com/cat.gif",
                "thumbnail": {"url": "https://example.com/cat.gif", "proxy_url": "https://media/cat.gif", "width": 100, "height": 100}}],
            "sticker_items": [
                {"id": "7", "name": "Wave", "format_type": 1},
                {"id": "8", "name": "Dance", "format_type": 4},
                {"id": "9", "name": "Spin", "format_type": 3},
            ],
        }))
        .unwrap();
        let spoilers: Vec<bool> = message.attachments.iter().map(|a| a.spoiler).collect();
        assert_eq!(&spoilers[..3], [true, true, false]);
        assert!(spoilers[3], "a spoiled link's preview is a spoiler too");
        let stickers: Vec<(&str, &str)> = message
            .attachments
            .iter()
            .filter(|attachment| attachment.shown == Shown::Sticker)
            .map(|attachment| (attachment.name.as_str(), &*attachment.url))
            .collect();
        assert_eq!(
            stickers,
            [
                ("Wave", "https://cdn.discordapp.com/stickers/7.png?size=160"),
                (
                    "Dance",
                    "https://media.discordapp.net/stickers/8.gif?size=160"
                ),
                ("Spin", "https://cdn.discordapp.com/stickers/9.json"),
            ]
        );
        assert_eq!(message.content, "look ||https://example.com/cat.gif||");
    }

    #[test]
    fn a_message_of_components_shows_its_files_only_through_them() {
        let file = |name: &str| {
            json!({"url": format!("https://cdn/{name}"), "proxy_url": format!("https://media/{name}"),
                   "width": 10, "height": 10, "content_type": "image/png"})
        };
        let message = parse(&json!({
            "id": "1", "channel_id": "2", "author": {"id": "3", "username": "discord"},
            "content": "", "flags": 1 << 15,
            "attachments": [{"filename": "a.png", "size": 1, "url": "https://cdn/a.png",
                             "proxy_url": "https://media/a.png", "content_type": "image/png"}],
            "components": [{"type": 17, "components": [
                {"type": 10, "content": "Hi"},
                {"type": 12, "items": [{"media": file("a.png")}]},
            ]}],
        }))
        .unwrap();
        assert_eq!(message.components.len(), 1);
        let shown: Vec<_> = message.attachments.iter().map(|a| a.shown).collect();
        assert_eq!(shown, [Shown::Component(0)]);
        // An older message's buttons show under its own files.
        let message = parse(&json!({
            "id": "1", "channel_id": "2", "author": {"id": "3", "username": "bot"},
            "content": "Done",
            "attachments": [{"filename": "a.txt", "size": 1, "url": "https://cdn/a.txt"}],
            "components": [{"type": 1, "components": [
                {"type": 2, "style": 5, "label": "Open", "url": "https://example.com"}]}],
        }))
        .unwrap();
        assert_eq!(message.attachments.len(), 1);
        assert_eq!(message.components[0].rows.len(), 1);
    }

    #[test]
    fn parses_replies_attachments_and_reactions() {
        let message = parse(&json!({
            "id": "1300000000000000000",
            "channel_id": "11",
            "type": 19,
            "content": "hi <@2>",
            "author": {"id": "1", "username": "me", "global_name": "Me"},
            "member": {"nick": "Captain"},
            "edited_timestamp": "2026-10-06T10:00:00.000000+00:00",
            "referenced_message": {"id": "9", "author": {"id": "2", "username": "ann"}, "content": "hello"},
            "attachments": [{"filename": "cat.png", "size": 10, "url": "https://cdn/cat.png", "content_type": "image/png",
                "proxy_url": "https://media/cat.png?ex=1", "width": 1440, "height": 540},
                {"filename": "clip.mp4", "size": 99, "url": "https://cdn/clip.mp4", "content_type": "video/mp4",
                "proxy_url": "https://media/clip.mp4", "width": 1920, "height": 1080}],
            "reactions": [
                {"emoji": {"id": null, "name": "👍"}, "count": 2, "me": true},
                {"emoji": {"id": "5", "name": "pog"}, "count": 1, "me": false},
            ],
        }))
        .unwrap();
        assert_eq!(message.author, "Captain");
        assert!(message.edited && message.editable);
        assert_eq!(
            message.reply,
            Some(Reply::Message {
                id: "9".into(),
                author_id: "2".into(),
                author: "ann".into(),
                content: "hello".into()
            })
        );
        assert!(message.attachments[0].image);
        assert_eq!(
            message.attachments[0].preview_url.as_deref(),
            Some("https://media/cat.png?ex=1&animated=true&format=webp&width=360&height=135")
        );
        assert_eq!(
            message.attachments[0].view_url.as_deref(),
            Some("https://media/cat.png?ex=1&animated=true&format=webp&width=1440&height=540"),
            "a small image isn't enlarged"
        );
        // A video plays from the file, its poster from the media proxy.
        let video = &message.attachments[1];
        assert!(!video.image);
        assert_eq!(video.video_url.as_deref(), Some("https://cdn/clip.mp4"));
        assert_eq!(
            video.preview_url.as_deref(),
            Some("https://media/clip.mp4?format=webp&width=360&height=203")
        );
        assert_eq!(video.dimensions, Some((1920, 1080)));
        assert_eq!(message.attachments[0].video_url, None);
        assert_eq!(message.reactions[1].key, ":pog:");
        assert_eq!(message.reactions[1].emoji, "pog:5");
        assert_eq!(message.timestamp(), 1_730_014_552);

        let mut message = message;
        message.react("👍", "👍", true, false);
        assert_eq!(
            (message.reactions[0].count, message.reactions[0].me),
            (1, false)
        );
        message.react("pog", "pog:5", false, false);
        message.react(":pog:", "pog:5", false, false);
        assert_eq!(message.reactions.len(), 1);

        assert!(parse(&json!({"id": "1", "type": 8, "author": {"id": "1"}})).is_none());
        let deleted = parse(
            &json!({"id": "1", "type": 19, "author": {"id": "1"}, "referenced_message": null}),
        );
        assert_eq!(deleted.unwrap().reply, Some(Reply::Deleted));
    }

    #[test]
    fn shows_what_a_forward_carries() {
        let message = parse(&json!({
            "id": "1300000000000000000",
            "channel_id": "11",
            "type": 0,
            "content": "",
            "author": {"id": "1", "username": "me"},
            "attachments": [],
            "embeds": [],
            "message_reference": {"type": 1, "channel_id": "12", "message_id": "9", "guild_id": "10"},
            "message_snapshots": [{"message": {
                "type": 0,
                "content": "look",
                "flags": 0,
                "attachments": [{"filename": "clip.mp4", "size": 99, "url": "https://cdn/clip.mp4",
                    "content_type": "video/mp4", "proxy_url": "https://media/clip.mp4", "width": 640, "height": 360}],
                "embeds": [],
                "sticker_items": [{"id": "7", "name": "wave", "format_type": 1}],
            }}],
        }))
        .unwrap();
        assert!(message.forwarded && !message.editable);
        assert_eq!(message.content, "look");
        assert_eq!(
            message.attachments[0].video_url.as_deref(),
            Some("https://cdn/clip.mp4")
        );
        assert_eq!(message.attachments[1].shown, Shown::Sticker);

        // A reply's reference has the default type, so it shows its own content.
        let reply = parse(&json!({
            "id": "1", "type": 19, "content": "mine", "author": {"id": "1"},
            "message_reference": {"type": 0, "message_id": "9"},
            "referenced_message": {"id": "9", "author": {"id": "2"}, "content": "theirs"},
        }))
        .unwrap();
        assert!(!reply.forwarded);
        assert_eq!(reply.content, "mine");
    }

    #[test]
    fn forwards_what_discord_forwards() {
        let forwardable = |extra: Value| {
            let mut value = json!({"id": "1", "type": 0, "content": "hi", "author": {"id": "1"}});
            value
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            parse(&value).unwrap().forwardable
        };
        assert!(forwardable(json!({})));
        assert!(forwardable(json!({"type": 19, "referenced_message": null})));
        // A forward, a voice message and an @silent one forward again.
        assert!(forwardable(
            json!({"flags": (1 << 14) | (1 << 13) | (1 << 12)})
        ));
        // An ephemeral one doesn't, nor a poll, nor a notice.
        assert!(!forwardable(json!({"flags": 1 << 6})));
        assert!(!forwardable(json!({"poll": {"question": {"text": "?"}}})));
        assert!(!forwardable(json!({"type": 6})));
    }

    #[test]
    fn reads_a_poll_and_counts_votes_on_it() {
        let mut message = parse(&json!({
            "id": "1", "type": 0, "content": "", "author": {"id": "2"},
            "poll": {
                "question": {"text": "Lunch?"},
                "answers": [
                    {"answer_id": 1, "poll_media": {"text": "Pizza", "emoji": {"id": null, "name": "🍕"}}},
                    {"answer_id": 2, "poll_media": {"text": "Soup", "emoji": {"id": "77", "name": ""}}},
                    {"answer_id": 3, "poll_media": {"text": "Salad"}},
                ],
                "expiry": "2026-10-20T12:00:00.000000+00:00",
                "allow_multiselect": true,
                "layout_type": 1,
                "results": {"is_finalized": false, "answer_counts": [
                    {"id": 1, "count": 3, "me_voted": true},
                    {"id": 3, "count": 1, "me_voted": false},
                ]},
            },
        }))
        .unwrap();
        assert!(!message.editable, "a poll stays as it was sent");
        let poll = message.poll.as_ref().unwrap();
        assert_eq!(poll.question, "Lunch?");
        assert!(poll.multiselect && poll.counted && !poll.finalized);
        let expiry = iso_ms("2026-10-20T12:00:00Z").unwrap();
        assert_eq!(poll.expiry, Some(expiry));
        assert!(!poll.closed(expiry - 1));
        assert!(poll.closed(expiry));
        let answers: Vec<_> = poll
            .answers
            .iter()
            .map(|answer| {
                (
                    answer.id,
                    answer.text.as_str(),
                    answer.emoji.clone(),
                    answer.votes,
                    answer.me,
                )
            })
            .collect();
        assert_eq!(
            answers,
            [
                (1, "Pizza", Some(("🍕".to_owned(), None)), 3, true),
                (
                    2,
                    "Soup",
                    Some((":77:".to_owned(), Some("77".to_owned()))),
                    0,
                    false
                ),
                (3, "Salad", None, 1, false),
            ]
        );
        message.vote(2, false, true);
        message.vote(1, true, false);
        message.vote(9, false, true);
        let poll = message.poll.as_ref().unwrap();
        assert_eq!((poll.answers[0].votes, poll.answers[0].me), (2, false));
        assert_eq!(poll.answers[1].votes, 1);
    }

    #[test]
    fn tells_of_a_closed_poll_and_its_winner() {
        let notice = |fields: Value| {
            parse(&json!({
                "id": "5", "type": 46, "content": "", "author": {"id": "2"},
                "message_reference": {"message_id": "1"},
                "embeds": [{"type": "poll_result", "fields": fields}],
            }))
            .unwrap()
            .system
            .unwrap()
        };
        let field =
            |name: &str, value: &str| json!({"name": name, "value": value, "inline": false});
        assert_eq!(
            notice(json!([
                field("poll_question_text", "Lunch?"),
                field("victor_answer_votes", "3"),
                field("total_votes", "4"),
                field("victor_answer_id", "1"),
                field("victor_answer_text", "Pizza"),
                field("victor_answer_emoji_name", "🍕"),
            ])),
            "'s poll \u{201c}Lunch?\u{201d} has closed. Winning answer: 🍕 Pizza, 3 votes (75%)."
        );
        assert_eq!(
            notice(json!([
                field("poll_question_text", "Lunch?"),
                field("total_votes", "0")
            ])),
            "'s poll \u{201c}Lunch?\u{201d} has closed. There was no winner."
        );
    }

    #[test]
    fn learns_the_author_a_reply_quotes() {
        let mut names = Names::default();
        names.learn_message(
            None,
            &json!({
                "id": "1", "type": 19, "author": {"id": "1", "username": "me"},
                "referenced_message": {
                    "id": "9", "author": {"id": "2", "username": "ann", "avatar": "abc"},
                    "content": "hello",
                },
            }),
        );
        assert!(
            names
                .avatar(None, "2")
                .ends_with("/avatars/2/abc.png?size=128")
        );
    }

    #[test]
    fn prefers_server_avatars() {
        let mut names = Names::default();
        names.learn_user(&json!({"id": "2", "username": "ann", "avatar": "abc"}));
        names.learn_member("10", &json!({"user_id": "2", "avatar": "def"}));
        assert!(
            names
                .avatar(None, "2")
                .ends_with("/avatars/2/abc.png?size=128")
        );
        assert!(
            names
                .avatar(Some("10"), "2")
                .ends_with("/guilds/10/users/2/avatars/def.png?size=128")
        );
        assert!(
            names
                .avatar(Some("11"), "2")
                .ends_with("/avatars/2/abc.png?size=128")
        );
        // A mention without the avatar keeps it; null removes it.
        names.learn_user(&json!({"id": "2", "username": "ann"}));
        assert!(names.avatar(None, "2").contains("/avatars/2/"));
        names.learn_user(&json!({"id": "2", "username": "ann", "avatar": null}));
        assert!(names.avatar(None, "2").contains("/embed/avatars/"));
    }

    #[test]
    fn reads_iso_times() {
        assert_eq!(iso_ms("2026-10-06T14:05:00+00:00"), Some(1_791_295_500_000));
        assert_eq!(iso_ms("2026-10-06T14:05:00.25Z"), Some(1_791_295_500_250));
        assert_eq!(
            iso_ms("2026-10-06T16:05:00.000000+02:00"),
            Some(1_791_295_500_000)
        );
        assert_eq!(iso_ms("soon"), None);
        assert_eq!(
            snowflake_ms(&snowflake_at(1_791_295_500_000).to_string()),
            1_791_295_500_000
        );
    }

    #[test]
    fn renders_mentions_emoji_and_times() {
        let mut names = Names::default();
        names.learn_message(
            Some("10"),
            &json!({
                "author": {"id": "1", "username": "me"},
                "mentions": [{"id": "2", "username": "ann", "global_name": "Ann", "member": {"nick": "Annie"}}],
            }),
        );
        let server = Server {
            id: "10".into(),
            role_names: HashMap::from([("30".into(), "Mods".into())]).into(),
            text_channels: vec![ready::Channel {
                id: "11".into(),
                name: "general".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(
            names.render(
                Some(&server),
                "<@2> <@!1> <@&30> in <#11> <:pog:5> <a:dance:6> <t:1791295500:R> <@9> a < b"
            ),
            format!(
                "@Annie @me @Mods in #general :pog: :dance: {} @Discord user a < b",
                date_time(1_791_295_500)
            )
        );
        assert_eq!(
            custom_emoji("<:pog:5> <a:dance:6> <:pog:5> <@2> <:bad:x> `<:x:>`"),
            [
                CustomEmoji {
                    name: "pog".into(),
                    id: "5".into(),
                    animated: false
                },
                CustomEmoji {
                    name: "dance".into(),
                    id: "6".into(),
                    animated: true
                },
            ]
        );
        assert!(names.knows_member("10", "2") && !names.knows_member("10", "1"));
        let mention = |text: &str, target, at: &[usize]| Mention {
            text: text.into(),
            target,
            at: at.to_vec(),
        };
        assert_eq!(
            names.render_mentions(Some(&server), "<@2> and <@!2>, <@&30>, <#11> <@x"),
            (
                "@Annie and @Annie, @Mods, #general <@x".to_owned(),
                vec![
                    mention("@Annie", MentionTarget::User("2".into()), &[0, 11]),
                    mention("@Mods", MentionTarget::Role("30".into()), &[19])
                ]
            )
        );
        assert_eq!(
            names.render_mentions(
                Some(&server),
                "@here <@2>@everyone x@everyones \\@here @heresy"
            ),
            (
                "@here @Annie@everyone x@everyones \\@here @heresy".to_owned(),
                vec![
                    mention("@here", MentionTarget::Everyone, &[0, 41]),
                    mention("@Annie", MentionTarget::User("2".into()), &[6]),
                    mention("@everyone", MentionTarget::Everyone, &[12, 23]),
                ]
            )
        );
        names.learn_member("10", &json!({"user_id": "2", "roles": ["30"]}));
        names.learn_member("10", &json!({"user_id": "3", "roles": ["31"]}));
        assert_eq!(names.members_with_role("10", "30"), ["2"]);
    }

    #[test]
    fn the_version_changes_only_with_what_names_show() {
        let mut names = Names::default();
        let ann = json!({"id": "2", "username": "ann", "avatar": "abc"});
        names.learn_user(&ann);
        let learned = names.version();
        assert_eq!(learned, 0, "someone new changes nothing shown");
        names.learn_user(&ann);
        names.learn_member("10", &json!({"user": ann, "nick": null, "roles": []}));
        let member = names.version();
        assert!(
            member > learned,
            "a server profile, though without a nickname"
        );
        names.learn_member("10", &json!({"user": ann, "nick": null, "roles": []}));
        assert_eq!(names.version(), member, "the same again");
        names.learn_member("10", &json!({"user_id": "2", "nick": "Annie", "roles": []}));
        assert!(names.version() > member, "a nickname");
    }

    #[test]
    fn forgets_those_learned_longest_ago_once_there_are_too_many() {
        let mut names = Names::default();
        let person = |n: usize| json!({"id": n.to_string(), "username": format!("p{n}")});
        names.learn_member(
            "10",
            &json!({"user": person(0), "nick": "Zero", "roles": []}),
        );
        names.learn_user(&person(1));
        for n in 2..=KEPT_PEOPLE {
            names.learn_user(&person(n));
            if n == KEPT_PEOPLE / 2 {
                // Shown, as in a member list, so kept.
                assert_eq!(names.name("10", "0"), Some("Zero"));
            }
        }
        let version = names.version();
        names.learn_user(&person(KEPT_PEOPLE + 1));
        assert_eq!(names.version(), version, "what was shown with them stays");
        assert!(names.seen.len() <= KEPT_PEOPLE * 3 / 4 + 1);
        assert_eq!(names.user_name("1"), None, "learned first");
        assert_eq!(names.name("10", "0"), Some("Zero"));
        assert!(names.knows_member("10", "0"));
        assert!(names.user_name(&KEPT_PEOPLE.to_string()).is_some());
        let saved = serde_json::to_vec(&names.saving()).unwrap();
        let restored = Names::restore(serde_json::from_slice(&saved).unwrap());
        assert_eq!(restored.users.len(), names.users.len());
    }

    #[test]
    fn knows_bots_from_any_full_user_object_and_keeps_them() {
        let mut names = Names::default();
        names.learn_user(&json!({"id": "2", "username": "helper", "bot": true}));
        names.learn_user(&json!({"id": "3", "username": "ann"}));
        // A partial user object says nothing of it.
        names.learn_member("10", &json!({"user": {"id": "2"}, "roles": []}));
        assert!(names.is_bot("2"));
        assert!(!names.is_bot("3"));
        let saved = serde_json::to_vec(&names.saving()).unwrap();
        let restored = Names::restore(serde_json::from_slice(&saved).unwrap());
        assert!(restored.is_bot("2"));
        names.learn_user(&json!({"id": "2", "username": "helper", "bot": false}));
        assert!(!names.is_bot("2"));
    }

    #[test]
    fn learns_roles_for_name_colours() {
        let mut names = Names::default();
        names.learn_member("10", &json!({"user_id": "2", "roles": ["30", "31"]}));
        let server = Server {
            id: "10".into(),
            role_colors: HashMap::from([
                ("30".into(), (1, 0x3498db)),
                ("31".into(), (2, 0xe91e63)),
            ])
            .into(),
            ..Default::default()
        };
        assert_eq!(
            server.color(names.roles("10", "2")),
            Some(0xe91e63),
            "the highest role's"
        );
        names.learn_member("10", &json!({"user_id": "2", "nick": "Annie"}));
        assert_eq!(
            names.roles("10", "2"),
            ["30", "31"],
            "kept when a member object leaves them out"
        );
        assert!(names.roles("11", "2").is_empty());
    }

    #[test]
    fn mentions_call_on_this_account_as_discord_highlights_them() {
        let me = "1";
        let message = |extra: Value| {
            let mut value = json!({"id": "500", "channel_id": "11", "type": 0,
                "author": {"id": "2", "username": "bob"}, "content": "hi"});
            value
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            parse(&value).unwrap()
        };
        let server = Server {
            my_roles: vec!["30".into()],
            ..Default::default()
        };
        let pings = |extra: Value| message(extra).mentions_me(me, Some(&server));
        assert!(!pings(json!({})));
        assert!(pings(
            json!({"mentions": [{"id": "1", "username": "alice"}]})
        ));
        assert!(!pings(
            json!({"mentions": [{"id": "3", "username": "carol"}]})
        ));
        assert!(pings(json!({"mention_roles": ["30"]})), "Alice's role");
        assert!(!pings(json!({"mention_roles": ["31"]})));
        assert!(pings(json!({"mention_everyone": true})));
        let reply = |author: &str| {
            json!({"type": 19, "referenced_message": {"id": "400",
                "author": {"id": author, "username": "someone"}, "content": "plan"}})
        };
        assert!(pings(reply("1")), "a reply to Alice, pinging her or not");
        assert!(!pings(reply("3")));
        // A reply to her names her only when its text does: a ping lists
        // her in `mentions` all the same.
        let names = |extra: Value| message(extra).names_me(me, Some(&server));
        let ping = |content: &str| {
            let mut value = reply("1");
            value["mentions"] = json!([{"id": "1", "username": "alice"}]);
            value["content"] = json!(content);
            value
        };
        assert!(!names(reply("1")));
        assert!(!names(ping("sure")), "a ping reply");
        assert!(names(ping("sure <@1>")));
        assert!(names(ping("sure <@!1>")));
        assert!(names(
            json!({"mentions": [{"id": "1", "username": "alice"}]})
        ));
        assert!(message(ping("sure")).replies_to(me));
        assert!(!message(reply("3")).replies_to(me));

        // Her settings can quiet @everyone and roles, but not her name.
        let quiet = Server {
            suppress_everyone: true,
            suppress_roles: true,
            ..server.clone()
        };
        let quieted = |extra: Value| message(extra).mentions_me(me, Some(&quiet));
        assert!(!quieted(json!({"mention_everyone": true})));
        assert!(!quieted(json!({"mention_roles": ["30"]})));
        assert!(quieted(
            json!({"mentions": [{"id": "1", "username": "alice"}]})
        ));
        // Her own messages don't call on her.
        let own = message(json!({"author": {"id": "1", "username": "alice"},
            "mention_everyone": true}));
        assert!(!own.mentions_me(me, Some(&server)));
        // A DM has no server: @everyone means nothing there.
        assert!(!message(json!({"mention_everyone": true})).mentions_me(me, None));
    }
}
