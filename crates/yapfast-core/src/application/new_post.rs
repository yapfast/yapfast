//! Posting in a Discord forum, as the form at the top of Discord's forum
//! does: a title, up to five of the forum's tags, and a first message of
//! text, files or both. The form keeps what was written while other rooms
//! show, and shows again with its forum; the files are the forum's staged
//! ones. Once Discord makes the post, it opens beside the forum.

use super::*;
use crate::chat::ForumTag;

/// The longest title a post may have, in characters, as Discord's client
/// allows.
pub const POST_TITLE_LIMIT: usize = 100;
/// The most tags a post may have.
pub const POST_TAGS: usize = 5;

/// The form for a post in a forum.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewPost {
    /// The forum's room id.
    pub room: String,
    pub title: String,
    pub body: String,
    /// The tags chosen, by id, in the order they were.
    pub tags: Vec<String>,
    pub posting: bool,
    pub error: Option<String>,
    /// Closing it asks first, as something was written.
    pub leaving: bool,
    /// The files that went with the post, which leave the form once made.
    sent: Vec<PathBuf>,
}

impl AppState {
    /// The form for a post in the forum on screen, while it's open.
    pub fn new_post(&self) -> Option<&NewPost> {
        let forum = self.forum()?;
        self.new_post
            .as_ref()
            .filter(|post| post.room == forum.list.room)
    }
}

impl Application {
    /// Whether this account may post in the forum on screen, as Discord's
    /// client decides: with Send Messages there, while Discord is connected.
    pub fn can_post(&self) -> bool {
        self.state.forum().is_some()
            && self
                .state
                .selected_room()
                .is_some_and(|room| self.state.writable(room))
    }

    /// The tags a post in a forum, by room id, may take, in its order: the
    /// moderated ones only with Manage Threads.
    pub fn post_tags(&self, room: &str) -> Vec<ForumTag> {
        let manages = self.may(room, chat::Permission::ManageThreads);
        let mut tags = self.forum_info(room).unwrap_or_default().tags;
        tags.retain(|tag| manages || !tag.moderated);
        tags
    }

    /// Opens the form at the top of the forum on screen, as its New Post
    /// button does, with what was written there before.
    pub(super) fn open_new_post(&mut self) {
        if !self.can_post() {
            return;
        }
        let Some(room) = self.state.selected.clone() else {
            return;
        };
        match &mut self.state.new_post {
            Some(post) if post.room == room => post.leaving = false,
            post => {
                *post = Some(NewPost {
                    room,
                    ..NewPost::default()
                })
            }
        }
    }

    /// Writes the form's title, as a single line of at most
    /// [`POST_TITLE_LIMIT`] characters, or its message.
    pub(super) fn write_new_post(&mut self, title: Option<String>, body: Option<String>) {
        let Some(post) = self.state.new_post.as_mut().filter(|post| !post.posting) else {
            return;
        };
        if let Some(title) = title {
            post.title = title
                .replace(['\n', '\r'], " ")
                .chars()
                .take(POST_TITLE_LIMIT)
                .collect();
        }
        if let Some(body) = body {
            post.body = body;
        }
        post.error = None;
    }

    /// Gives the post a tag, by id, or takes it away; at most
    /// [`POST_TAGS`], of those it may take.
    pub(super) fn toggle_post_tag(&mut self, tag: String) {
        let Some(room) = self.state.new_post().map(|post| post.room.clone()) else {
            return;
        };
        let allowed = self.post_tags(&room).iter().any(|known| known.id == tag);
        let Some(post) = self.state.new_post.as_mut().filter(|post| !post.posting) else {
            return;
        };
        if let Some(index) = post.tags.iter().position(|chosen| *chosen == tag) {
            post.tags.remove(index);
        } else if allowed && post.tags.len() < POST_TAGS {
            post.tags.push(tag);
        }
        post.error = None;
    }

    /// Makes the post the form describes, as its Post button does, unless
    /// it lacks a title, a message, or a tag the forum requires.
    pub(super) fn create_post(&mut self) -> Vec<Effect> {
        let Some(room) = self
            .state
            .new_post()
            .filter(|post| !post.posting)
            .map(|post| post.room.clone())
        else {
            return vec![];
        };
        let require_tag = self.forum_info(&room).is_some_and(|info| info.require_tag);
        let files: Vec<chat::OutgoingFile> = self
            .state
            .staged
            .get(&room)
            .into_iter()
            .flatten()
            .map(|file| chat::OutgoingFile {
                path: file.path.clone(),
                spoiler: file.spoiler,
                clip: false,
            })
            .collect();
        let Some(post) = self.state.new_post.as_mut() else {
            return vec![];
        };
        let title = post.title.trim().to_owned();
        let problem = if title.is_empty() {
            Some("A title is required")
        } else if post.body.trim().is_empty() && files.is_empty() {
            Some("A message is required")
        } else if require_tag && post.tags.is_empty() {
            Some("A tag is required to create a post in this channel")
        } else {
            None
        };
        if let Some(problem) = problem {
            post.error = Some(problem.into());
            return vec![];
        }
        post.posting = true;
        post.error = None;
        post.sent = files.iter().map(|file| file.path.clone()).collect();
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::PostInForum {
                room,
                title,
                tags: post.tags.clone(),
                body: post.body.trim_end().to_owned(),
                files,
            },
        ))]
    }

    /// Closes the form, as its close button does, asking first when
    /// something was written in it; with `sure`, closes it anyway, and its
    /// files go.
    pub(super) fn close_new_post(&mut self, sure: bool) {
        let Some(post) = self.state.new_post.as_mut().filter(|post| !post.posting) else {
            return;
        };
        let staged = self
            .state
            .staged
            .get(&post.room)
            .is_some_and(|files| !files.is_empty());
        let written = !post.title.trim().is_empty() || !post.body.trim().is_empty();
        if (written || staged) && !sure {
            post.leaving = true;
        } else {
            self.state.staged.remove(&post.room);
            self.state.new_post = None;
        }
    }

    /// The post was made: it opens beside its forum, now if it lists, as
    /// Discord tells of it before answering, else once it does.
    pub(super) fn forum_posted(
        &mut self,
        room: String,
        result: Result<String, String>,
    ) -> Vec<Effect> {
        let Some(post) = self
            .state
            .new_post
            .as_mut()
            .filter(|post| post.room == room && post.posting)
        else {
            return vec![];
        };
        match result {
            Ok(made) => {
                let sent = std::mem::take(&mut post.sent);
                self.state.new_post = None;
                if let Some(staged) = self.state.staged.get_mut(&room) {
                    staged.retain(|file| !sent.contains(&file.path));
                }
                if self.state.selected.as_ref() != Some(&room) {
                    return vec![];
                }
                if self.state.rooms.iter().any(|listed| listed.id == made) {
                    return self.open_aside(made);
                }
                self.pending_aside = Some(made);
                vec![]
            }
            Err(error) => {
                post.posting = false;
                post.error = Some(error);
                vec![]
            }
        }
    }
}
