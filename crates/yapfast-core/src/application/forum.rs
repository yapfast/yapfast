//! A Discord forum, which shows its posts in place of messages, as Discord's
//! client shows one: its active posts, pinned ones first, then its older,
//! archived ones a page at a time, or the posts a search of their titles
//! finds. Each post shows as a card with its first message. The active posts
//! are the server's, as its snapshot shares them; their first messages are
//! asked for ten at a time, for the cards near the top and then for more as
//! the list scrolls, as Discord's client asks for those it shows. Archived
//! pages and a search's results bring their own. All of it is kept only
//! while the forum shows.

use std::collections::HashMap;

use super::*;
use crate::chat::{ForumTag, ListedThread, PostStarter, PostStarters};
use threads::ThreadList;

/// How many cards want their first messages at first, and how many more
/// each time the list scrolls near its end: a page.
const CARDS: usize = 25;
/// How many posts' first messages one request asks for, as Discord's client
/// asks for them.
const STARTERS_PER_ASK: usize = 10;

/// The forum on screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForumView {
    /// Its archived posts and a search, as a channel's list of threads keeps
    /// them.
    pub list: ThreadList,
    /// Posts' first messages, by room id; none where it was deleted.
    pub starters: HashMap<String, Option<PostStarter>>,
    /// Posts whose first messages are on their way.
    asking: Vec<String>,
    /// Asking failed: it waits for the list to scroll before asking again.
    stalled: bool,
    /// How many cards, from the top, want their first messages.
    shown: usize,
    /// The last message read here when it opened, a snowflake: posts made
    /// after it, which this account never opened, are new.
    read: u64,
}

/// A forum's posts as it shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ForumPosts {
    /// Its active posts, pinned ones first, then by their latest message,
    /// or by when they were made, as the forum sorts them.
    pub active: Vec<ForumPost>,
    /// Older, archived ones, by their latest message.
    pub older: Vec<ForumPost>,
    /// What a search of their titles found, while one shows: `None` while
    /// it's on its way, or why it failed.
    pub found: Option<Option<Result<Vec<ForumPost>, String>>>,
}

/// A post as its card shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ForumPost {
    pub thread: ListedThread,
    /// Its tags, in the forum's order.
    pub tags: Vec<ForumTag>,
    /// Its first message: `None` until it comes, `Some(None)` when it was
    /// deleted.
    pub starter: Option<Option<PostStarter>>,
    /// Made since the forum was last read, and never opened.
    pub new: bool,
}

/// What a forum says of its posts, besides them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForumInfo {
    pub tags: Vec<ForumTag>,
    pub by_creation: bool,
    /// A post must take one of its tags.
    pub require_tag: bool,
    /// The emoji a post without reactions offers.
    pub reaction: Option<chat::ShownEmoji>,
}

impl ForumView {
    /// A page of posts, or first messages, is on its way.
    pub fn loading(&self) -> bool {
        self.list.loading || !self.asking.is_empty()
    }

    /// Scrolling further wants more: of `cards`, some beyond those that
    /// have asked for their first messages, or older posts still to load.
    pub fn wants_more(&self, cards: usize) -> bool {
        self.list.search.is_none() && (self.list.more || cards > self.shown)
    }
}

impl AppState {
    /// The Discord forum on screen.
    pub fn forum(&self) -> Option<&ForumView> {
        self.forum
            .as_ref()
            .filter(|forum| self.selected.as_ref() == Some(&forum.list.room))
    }
}

impl Application {
    /// Shows a forum, by room id, in place of messages: its posts, and the
    /// first messages of those on top. Discord sends its active posts, and
    /// any made while it shows, with the server's threads.
    pub(super) fn show_forum(&mut self, room: String) -> Vec<Effect> {
        let archived = self.may(&room, chat::Permission::ReadHistory);
        let (list, mut effects) = ThreadList::open(room.clone(), archived);
        self.state.forum = Some(ForumView {
            list,
            shown: CARDS,
            read: self.forum_read(&room),
            ..ForumView::default()
        });
        effects.push(Effect::Backend(Command::Discord(
            DiscordCommand::ViewForum { room },
        )));
        effects.extend(self.ask_starters());
        effects
    }

    /// Forgets the forum, as another room shows.
    pub(super) fn close_forum(&mut self) -> Vec<Effect> {
        match self.state.forum.take() {
            Some(_) => vec![Effect::Backend(Command::Discord(
                DiscordCommand::CloseThreads,
            ))],
            None => vec![],
        }
    }

    /// Searches the posts' titles, as Enter in the forum's search does; an
    /// empty query shows the posts again.
    pub(super) fn search_posts(&mut self, query: String) -> Vec<Effect> {
        match self.state.forum.as_mut() {
            Some(forum) => forum.list.search(query),
            None => vec![],
        }
    }

    /// Wants another page of cards, as the list does when scrolled near its
    /// end: their first messages, and older posts once the active ones run
    /// out.
    pub(super) fn more_posts(&mut self) -> Vec<Effect> {
        let active = self.forum_posts().map_or(0, |posts| posts.active.len());
        let Some(forum) = self.state.forum.as_mut() else {
            return vec![];
        };
        forum.shown += CARDS;
        forum.stalled = false;
        let mut effects = vec![];
        if forum.shown > active {
            effects.extend(forum.list.more());
        }
        effects.extend(self.ask_starters());
        effects
    }

    /// Takes a page of posts for the forum, if it's the one it asked for,
    /// with their first messages.
    pub(super) fn forum_page_came(
        &mut self,
        room: String,
        query: Option<String>,
        offset: u32,
        result: Result<(Vec<ListedThread>, bool), String>,
        starters: PostStarters,
    ) -> Vec<Effect> {
        let Some(forum) = self
            .state
            .forum
            .as_mut()
            .filter(|forum| forum.list.room == room)
        else {
            return vec![];
        };
        forum.starters.extend(starters);
        forum.list.came(query, offset, result);
        self.ask_starters()
    }

    /// Takes first messages for the forum's cards, and asks for the next
    /// ones they want.
    pub(super) fn posts_came(
        &mut self,
        room: String,
        result: Result<PostStarters, String>,
    ) -> Vec<Effect> {
        let Some(forum) = self
            .state
            .forum
            .as_mut()
            .filter(|forum| forum.list.room == room)
        else {
            return vec![];
        };
        let asked = std::mem::take(&mut forum.asking);
        match result {
            Ok(starters) => {
                // One Discord didn't answer for was deleted.
                for post in asked {
                    forum.starters.entry(post).or_insert(None);
                }
                forum.starters.extend(starters);
            }
            Err(error) => {
                tracing::debug!(%error, "a forum's first messages didn't come");
                forum.stalled = true;
            }
        }
        self.ask_starters()
    }

    /// Asks for the first messages the cards on top want, unless some are
    /// on their way: the posts the server's snapshot knows, as archived
    /// pages and searches bring their own.
    pub(super) fn ask_starters(&mut self) -> Vec<Effect> {
        let wanted: Vec<String> = match self.state.forum.as_ref() {
            Some(forum) if forum.asking.is_empty() && !forum.stalled => {
                let posts = self.forum_posts().unwrap_or_default();
                posts
                    .active
                    .iter()
                    .chain(&posts.older)
                    .take(forum.shown)
                    .filter(|post| post.starter.is_none())
                    .take(STARTERS_PER_ASK)
                    .map(|post| post.thread.room.clone())
                    .collect()
            }
            _ => return vec![],
        };
        let Some(forum) = self.state.forum.as_mut().filter(|_| !wanted.is_empty()) else {
            return vec![];
        };
        forum.asking.clone_from(&wanted);
        vec![Effect::Backend(Command::Discord(DiscordCommand::Posts {
            room: forum.list.room.clone(),
            posts: wanted,
        }))]
    }

    /// The forum's posts as it shows them, while one shows.
    pub fn forum_posts(&self) -> Option<ForumPosts> {
        let forum = self.state.forum()?;
        let info = self.forum_info(&forum.list.room).unwrap_or_default();
        let sections = self.sections(&forum.list);
        let post = |thread: &ListedThread| ForumPost {
            tags: info
                .tags
                .iter()
                .filter(|tag| thread.tags.contains(&tag.id))
                .cloned()
                .collect(),
            starter: forum.starters.get(&thread.room).cloned(),
            new: !thread.archived && !thread.opened && created(thread) > forum.read,
            thread: thread.clone(),
        };
        let mut active: Vec<_> = sections
            .joined
            .iter()
            .chain(&sections.other)
            .map(post)
            .collect();
        let order = |post: &ForumPost| match info.by_creation {
            true => created(&post.thread),
            false => post.thread.last,
        };
        active.sort_by_key(|post| (!post.thread.pinned, std::cmp::Reverse(order(post))));
        let found = forum.list.search.as_ref().map(|search| {
            search
                .found
                .as_ref()
                .map(|found| found.clone().map(|found| found.iter().map(post).collect()))
        });
        Some(ForumPosts {
            active,
            older: sections.older.iter().map(post).collect(),
            found,
        })
    }

    /// What the forum, by room id, says of its posts.
    pub fn forum_info(&self, room: &str) -> Option<ForumInfo> {
        #[cfg(feature = "discord")]
        if let Some((server, Some(channel), _)) = self.discord_place(room)
            && let Some(forum) = server
                .server
                .channel(channel)
                .and_then(|channel| channel.forum.as_deref())
        {
            return Some(ForumInfo {
                tags: discord::forum_tags(forum),
                by_creation: forum.by_creation,
                require_tag: forum.require_tag,
                reaction: forum.reaction.as_ref().map(discord::shown_emoji),
            });
        }
        let _ = room;
        None
    }

    /// The last message read in a forum, by room id, as a snowflake.
    fn forum_read(&self, room: &str) -> u64 {
        #[cfg(feature = "discord")]
        if let Some((server, Some(channel), _)) = self.discord_place(room)
            && let Some(channel) = server.channel(channel)
        {
            return channel.marks.last_read;
        }
        let _ = room;
        0
    }
}

/// When a post was made, as a snowflake: its room id's.
fn created(thread: &ListedThread) -> u64 {
    thread
        .room
        .rsplit(':')
        .next()
        .and_then(|id| id.parse().ok())
        .unwrap_or_default()
}
