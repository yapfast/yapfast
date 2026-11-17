//! A new forum post: what it shows and what it asks for. Task C27
//! (docs/iced/tasks/chat.md). The Slint side: `NewPostForm`
//! (`ui/forum.slint:212`); the glue is `show_new_post`
//! (`src/ui/refresh.rs:2065`) and the `on_post_*`, `on_toggle_post_tag`,
//! `on_create_post` handlers (`src/ui/callbacks.rs:1723`-`:1751`), whose
//! formatting the projection keeps: the tags chosen in the order they were,
//! the forum's tags the post may take, the guidelines as the channel's topic.
//!
//! What Slint kept in the form's own properties (the menu of tags open, the
//! guidelines folded, which field has the keys) is held here, so the app
//! owns it and a shot can show it.

use crate::chat::composer::Staged;
use crate::chat::forum::Mark;
use crate::text::{Link, Text};

/// One of the forum's tags as the form offers it (`PostTagChoice`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choice {
    /// The tag's id, which a toggle names.
    pub id: String,
    pub name: String,
    pub mark: Mark,
    /// The post has taken it.
    pub chosen: bool,
}

/// The two fields that take the keys, as the form lights their frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Title,
    Body,
}

/// A new forum post: what its screen draws. The window holds it as
/// `vm.chat.new_post`, and the forum as `composing` while it is open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewPost {
    /// Whose draft the message is, as the post's room hashed: a new key
    /// shows the message afresh.
    pub key: u64,
    pub title: String,
    pub body: String,
    /// The field with the keys: its frame lightens.
    pub focus: Option<Part>,
    /// The tags chosen, in the order they were.
    pub chosen: Vec<Choice>,
    /// Every tag the post may take, ticked where chosen.
    pub choices: Vec<Choice>,
    /// The forum requires a tag: the heading says "(REQUIRED)".
    pub tag_required: bool,
    /// Fewer than five tags are chosen: "Add tags" and the unticked rows
    /// take one more.
    pub more_tags: bool,
    /// The menu of tags shows under "Add tags" (`tags-menu`).
    pub tags_open: bool,
    /// The forum's guidelines, its topic; `None` where it has none.
    pub guidelines: Option<Text>,
    /// The guidelines card shows its text rather than only its heading
    /// (`show-guidelines`, open to begin with).
    pub guidelines_open: bool,
    /// Files to go with the post, in the order they go.
    pub staged: Vec<Staged>,
    pub can_attach: bool,
    pub can_spoil: bool,
    /// The post is on its way: the form takes nothing and Publish reads
    /// "Posting...".
    pub posting: bool,
    /// Why it was not made; empty for no error.
    pub error: String,
    /// Closing was asked after writing: the form asks before it goes.
    pub leaving: bool,
}

/// A new forum post: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The title's text after an edit (`post-title-edited`,
    /// `Action::WriteNewPost`).
    Title(String),
    /// Enter in the title: the message takes the keys (the app runs
    /// `anomi_chat::forum::new_post::focus_body`).
    ToBody,
    /// The message's text after an edit (`post-body-edited`).
    Body(String),
    /// A field took the keys or lost them.
    Focus(Part, bool),
    /// Ctrl+V in the message: the app looks at the clipboard for files first.
    Paste,
    /// A chip's cross, or a row of the menu: take the tag if it is not
    /// chosen, leave it if it is (`toggle-tag`, `Action::TogglePostTag`).
    ToggleTag(String),
    /// "Add tags" opens the menu; a press outside it, or Escape, closes it.
    Tags(bool),
    /// The chevron of the guidelines card folds or unfolds it.
    ToggleGuidelines,
    /// A link in the guidelines.
    OpenLink(Link),
    /// The plus: the file picker.
    Attach,
    /// Take the staged file at this index out.
    Unstage(usize),
    /// Cover or uncover the staged file at this index.
    ToggleSpoiler(usize),
    /// Publish, or Enter in the message (`create-post`).
    Create,
    /// The cross of the form's bar (`close`); after writing, the form asks
    /// first ([`NewPost::leaving`]).
    Close,
    /// "Exit" in the question (`leave`).
    Leave,
    /// "Back to editing", Escape or a press outside the question (`stay`).
    Stay,
    /// What the staged files' tiles send that the form does not use. Never
    /// asked for.
    Idle,
}
