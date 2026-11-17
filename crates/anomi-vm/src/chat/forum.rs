//! A forum: what it shows and what it asks for. Task C26's
//! (docs/iced/tasks/chat.md). The Slint side: `ForumPanel`, `PostRow`,
//! `PostTag` (`ui/forum.slint:473`, `:17`, `:10`); the glue is `show_forum`
//! (`src/ui/refresh.rs:1995`) and `post_row` (`:6004`), whose formatting the
//! projection keeps: the first three tags and "+2" for the rest, the reply
//! count and the time as text, "OLDER POSTS" on the first older post.

use iced::Color;

use crate::chat::new_post::NewPost;
use crate::kind::Expected;

/// An emoji on a tag or a reaction: a Unicode one's text, or a custom one's
/// picture, whose room is kept while it is on its way.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Mark {
    #[default]
    None,
    Unicode(String),
    Custom(Expected),
}

/// A forum's tag as a post's card shows it (`PostTag`, `:10`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tag {
    pub name: String,
    pub mark: Mark,
}

/// The reaction most given, or the forum's own for none (an empty count).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reaction {
    pub mark: Mark,
    pub count: String,
}

/// An entry of the list: a heading, or a post's card, which may follow a
/// heading (`PostRow`, `:17`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Post {
    /// The post's room id; empty for a heading alone.
    pub room: String,
    /// A heading, such as "OLDER POSTS", above the post if it has a room.
    pub heading: String,
    pub name: String,
    /// Pinned to the top; made since the forum was last read and never
    /// opened; opened, with messages to read.
    pub pinned: bool,
    pub new: bool,
    pub unread: bool,
    /// Its first tags, and how many more it has, as "+2".
    pub tags: Vec<Tag>,
    pub more_tags: String,
    /// Who wrote its first message, in their role colour if they have one,
    /// and its text; or what to say for none, such as that it was deleted.
    pub person: String,
    pub color: Option<Color>,
    pub text: String,
    pub reaction: Option<Reaction>,
    /// Its replies, as "3", and when it was last active or made.
    pub replies: String,
    pub time: String,
    /// Its first picture, as a thumbnail, and whether it is a spoiler.
    pub picture: Expected,
    pub spoiler: bool,
}

impl Post {
    /// The list's key for this entry: its room, or its heading alone.
    pub fn key(&self) -> u64 {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        (&self.room, &self.heading).hash(&mut hasher);
        hasher.finish()
    }
}

/// A forum: what its screen draws. The window holds it as `vm.chat.forum`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Forum {
    pub rows: Vec<Post>,
    /// Said instead of the list: while it loads, or why it failed.
    pub note: String,
    /// There are no posts, or a search found none.
    pub empty: bool,
    pub searching: bool,
    /// The titles can be searched, as with Read Message History.
    pub searchable: bool,
    pub query: String,
    /// More cards are still to come.
    pub more: bool,
    /// A page of older ones is on its way.
    pub loading: bool,
    /// This account may post here.
    pub can_post: bool,
    /// The form for a post is open, in place of the list.
    pub composing: Option<NewPost>,
}

/// A forum: what a click or a key on it asks for. Keep `#[non_exhaustive]`:
/// the app matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The search field's text after an edit (`posts-query`).
    Query(String),
    /// Enter in the search field (`Action::SearchPosts`).
    Search(String),
    /// The cross, which empties the query and shows the posts again.
    Clear,
    /// A card clicked: beside the forum, or in its place with Shift
    /// (`open(room, beside)`, `Action::OpenAside` / `Action::Open`).
    Open { room: String, beside: bool },
    /// The list moved. The app asks for older posts (`Action::MorePosts`)
    /// when it ended near the end by a scroll of the person's, a list too
    /// short to scroll asking for none, and more is wanted and none loads.
    Scrolled { near_end: bool, by_hand: bool },
    /// "New Post" (`new-post`).
    NewPost,
}
