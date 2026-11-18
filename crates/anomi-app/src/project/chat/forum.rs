//! A Discord forum from the core's state: its posts in place of messages
//! (`show_forum`, `src/ui/refresh.rs:1995`, and `post_row`, `:5755`), the
//! form for a new post ([`new_post`]), and the bar over a post open as the
//! conversation (`forum-post`, `forum-post-item`, `forum-post-follow`,
//! `:2219`-`:2245`).
//!
//! The core sorts and filters the posts (pinned ones first, then by their
//! latest message or by when they were made, as the forum sorts them; or
//! what a search of their titles found). The list is built again only once
//! they changed or a minute passed for their times; a picture of a card's
//! (its thumbnail, a tag's or its reaction's emoji) builds that card alone.
//! The screen's list is virtual, and the pictures of the cards in view are
//! the ones asked for ([`seen`]).

pub(crate) mod new_post;
#[cfg(test)]
mod tests;

use std::hash::{DefaultHasher, Hash, Hasher};

use yapfast_core::application::{Application, ForumInfo, ForumPost};
use yapfast_core::chat::{Permission, ShownEmoji};
use yapfast_core::network::Network;
use yapfast_lang::tr;

use anomi_vm::chat::forum::{Forum, Mark, Post, Reaction, Tag};
use anomi_vm::conversation::{Conversation, ForumPost as PostBar};

use crate::pictures::{Requests, Watch};
use crate::project::chat::threads::ago;
use crate::project::rows;

/// How many tags a card shows before "+2".
const TAGS: usize = 3;
/// The cards either side of those in view whose pictures are asked for.
const MARGIN: usize = 6;

/// What the list showed last.
#[derive(Default)]
pub(crate) struct Shown {
    key: Option<u64>,
    pictures: Watch<usize>,
    /// The first and last card in view, as the list last told.
    view: Option<(usize, usize)>,
    pub(crate) post: new_post::Shown,
}

pub(crate) fn show(
    core: &Application,
    forum: &mut Forum,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let state = &core.state;
    let Some(view) = state.forum() else {
        // Its search goes with it, as Slint's `posts-query`.
        if shown.key.is_some() {
            *forum = Forum::default();
            *shown = Shown::default();
        }
        return;
    };
    let room = &view.list.room;
    forum.searchable = core.may(room, Permission::ReadHistory);
    forum.can_post = core.can_post();
    let posts = core.forum_posts().unwrap_or_default();
    let info = core.forum_info(room).unwrap_or_default();
    let cards = posts.active.len() + posts.older.len();
    forum.more = view.wants_more(cards);
    forum.loading = view.loading();
    forum.searching = posts.found.is_some();
    let (note, empty) = match &posts.found {
        Some(None) => (tr!("Searching...").to_owned(), false),
        Some(Some(Err(error))) => (error.clone(), false),
        Some(Some(Ok(found))) => (String::new(), found.is_empty()),
        None if cards == 0 && view.list.loading => (tr!("Loading posts...").to_owned(), false),
        None if cards == 0 => (
            view.list.error.clone().unwrap_or_default(),
            view.list.error.is_none(),
        ),
        None => (String::new(), false),
    };
    if forum.note != note {
        forum.note = note;
    }
    forum.empty = empty;
    let now = yapfast_core::clock::now_ms();
    let key = {
        let mut hasher = DefaultHasher::new();
        (&posts, room, info.by_creation, &info.reaction, now / 60_000).hash(&mut hasher);
        hasher.finish()
    };
    let listed: Vec<&ForumPost> = match &posts.found {
        Some(Some(Ok(found))) => found.iter().collect(),
        _ => posts.active.iter().chain(&posts.older).collect(),
    };
    let older = match &posts.found {
        Some(Some(Ok(_))) => None,
        _ => (!posts.older.is_empty()).then_some(posts.active.len()),
    };
    let moved = shown.pictures.moved(state);
    if shown.key != Some(key) {
        shown.key = Some(key);
        shown.pictures.clear();
        forum.rows = (listed.iter().enumerate())
            .map(|(index, post)| row(core, &info, post, older == Some(index), now))
            .collect();
        for (index, post) in listed.iter().enumerate() {
            self::note(state, post, index, &info, &mut shown.pictures);
        }
    } else {
        for index in moved {
            if let (Some(post), Some(shown)) = (listed.get(index), forum.rows.get_mut(index)) {
                *shown = row(core, &info, post, older == Some(index), now);
            }
        }
    }
    want(core, &listed, &info, shown.view, requests);
}

/// The list told which cards are in view: their pictures are asked for.
pub(crate) fn seen(
    core: &Application,
    shown: &mut Shown,
    first: usize,
    last: usize,
    requests: &mut Requests,
) {
    shown.view = Some((first, last));
    let Some(posts) = core.forum_posts() else {
        return;
    };
    let room = core.state.forum().map(|view| view.list.room.clone());
    let info = room
        .and_then(|room| core.forum_info(&room))
        .unwrap_or_default();
    let listed: Vec<&ForumPost> = match &posts.found {
        Some(Some(Ok(found))) => found.iter().collect(),
        _ => posts.active.iter().chain(&posts.older).collect(),
    };
    want(core, &listed, &info, shown.view, requests);
}

/// Wants the pictures of the cards in view, with a margin; the first
/// cards' before the list told.
fn want(
    core: &Application,
    listed: &[&ForumPost],
    info: &ForumInfo,
    view: Option<(usize, usize)>,
    requests: &mut Requests,
) {
    let (first, last) = view.unwrap_or((0, 2 * MARGIN));
    let from = first.saturating_sub(MARGIN);
    let to = (last + MARGIN + 1).min(listed.len());
    let mut wanted = Vec::new();
    for post in listed.get(from..to).unwrap_or_default() {
        addresses(post, info, &mut |address| wanted.push(address));
    }
    requests.want(&core.state, wanted);
}

/// Notes the pictures the card at `index` shows.
fn note(
    state: &yapfast_core::application::AppState,
    post: &ForumPost,
    index: usize,
    info: &ForumInfo,
    pictures: &mut Watch<usize>,
) {
    addresses(post, info, &mut |address| {
        pictures.note(state, address, index)
    });
}

/// Every picture a card shows: its thumbnail, its tags' and its reaction's
/// custom emoji, or the forum's own reaction.
fn addresses<'a>(post: &'a ForumPost, info: &'a ForumInfo, each: &mut dyn FnMut(&'a str)) {
    let first = post.starter.as_ref().and_then(Option::as_ref);
    let emoji = |emoji: Option<&'a ShownEmoji>| emoji.and_then(|emoji| emoji.picture.as_deref());
    let tags = post
        .tags
        .iter()
        .take(TAGS)
        .map(|tag| emoji(tag.emoji.as_ref()));
    let reaction = match first.and_then(|first| first.reaction.as_ref()) {
        Some((shown, _)) => emoji(Some(shown)),
        None if first.is_some() => emoji(info.reaction.as_ref()),
        None => None,
    };
    let picture = first.and_then(|first| first.picture.as_deref());
    for address in tags.chain([reaction, picture]).flatten() {
        each(address);
    }
}

/// An emoji on a tag or a reaction: a Unicode one's text, or a custom
/// one's picture (`post_tag`).
fn mark(state: &yapfast_core::application::AppState, emoji: Option<&ShownEmoji>) -> Mark {
    match emoji {
        None => Mark::None,
        Some(ShownEmoji {
            picture: Some(address),
            ..
        }) => Mark::Custom(rows::expected(state, Some(address))),
        Some(emoji) => Mark::Unicode(emoji.text.clone()),
    }
}

/// A post as its card shows it, as Discord's client does: its first three
/// tags, its first message's author and text, its top reaction or the
/// forum's own, its replies, and when it was last active, or made when the
/// forum sorts by that.
fn row(core: &Application, info: &ForumInfo, post: &ForumPost, older: bool, now: u64) -> Post {
    let state = &core.state;
    let thread = &post.thread;
    let starter = post.starter.as_ref();
    let first = starter.and_then(Option::as_ref);
    let (person, text) = match starter {
        Some(Some(first)) => (Some(&first.author), first.text.clone()),
        Some(None) => (None, tr!("Original message was deleted").to_owned()),
        None => (None, String::new()),
    };
    let reaction = match first.and_then(|first| first.reaction.as_ref()) {
        Some((emoji, count)) => Some(Reaction {
            mark: mark(state, Some(emoji)),
            count: count.to_string(),
        }),
        // A post without reactions offers the forum's own.
        None if first.is_some() => info.reaction.as_ref().map(|emoji| Reaction {
            mark: mark(state, Some(emoji)),
            count: String::new(),
        }),
        None => None,
    };
    let at = if info.by_creation {
        thread.created_ms
    } else {
        thread.active_ms
    };
    Post {
        room: thread.room.clone(),
        heading: if older {
            tr!("OLDER POSTS").to_owned()
        } else {
            String::new()
        },
        name: thread.name.clone(),
        pinned: thread.pinned,
        new: post.new,
        unread: thread.unread,
        tags: (post.tags.iter().take(TAGS))
            .map(|tag| Tag {
                name: tag.name.clone(),
                mark: mark(state, tag.emoji.as_ref()),
            })
            .collect(),
        more_tags: match post.tags.len().saturating_sub(TAGS) {
            0 => String::new(),
            more => format!("+{more}"),
        },
        person: person.map_or_else(String::new, |person| person.name.clone()),
        color: person.and_then(|person| person.color).map(rows::rgb),
        text,
        reaction,
        replies: thread.replies.to_string(),
        time: ago(at, now),
        picture: rows::expected(state, first.and_then(|first| first.picture.as_deref())),
        spoiler: first.is_some_and(|first| first.spoiler),
    }
}

/// The bar over a forum post open as the conversation: a Discord thread
/// whose channel is a forum. Its first message is the post's, whose id is
/// the thread's, and can take a reaction once it's loaded; following it
/// shows where it can be joined or left now (`thread_followed`).
pub(crate) fn post_bar(core: &Application, conversation: &mut Conversation) {
    let bar = post_item(core).map(|(room, item)| PostBar {
        reacts: item.is_some(),
        following: core.thread_followed(room),
    });
    if conversation.forum_post != bar {
        conversation.forum_post = bar;
    }
}

/// The post open as the conversation, and its first message's item while
/// it's loaded (`forum-post-item`).
pub(crate) fn post_item(core: &Application) -> Option<(&str, Option<String>)> {
    let state = &core.state;
    let room = state.selected_room()?;
    let parent = room.parent.as_deref()?;
    let forum = state
        .rooms
        .iter()
        .any(|listed| listed.id == parent && listed.forum);
    if room.network != Network::Discord || !forum || room.voice {
        return None;
    }
    let item = room.id.rsplit(':').next().map(|id| format!("d:{id}"));
    // Messages run oldest first, so the post is the first.
    let item = item.filter(|item| {
        state
            .messages
            .first()
            .is_some_and(|first| &first.item == item)
    });
    Some((room.id.as_str(), item))
}
