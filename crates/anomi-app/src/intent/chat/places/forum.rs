//! A forum (`ForumPanel`: `search`, `more-posts`, `open`, `new-post`;
//! `src/ui/callbacks.rs:1749`-`:1795`), the form for a new post
//! (`NewPostForm`: `:1650`-`:1653`, `:1723`-`:1751`) and the bar over a post
//! open as the conversation (React to post, Follow, Copy link,
//! `ui/yapfast.slint:3554`).
//!
//! What Slint kept in the form's own properties (which field has the keys,
//! the menu of tags, the guidelines folded) changes the view-model alone.

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::chat::forum::Intent;
use anomi_vm::chat::new_post::{Intent as PostIntent, NewPost};
use anomi_vm::menu::{Align, Spot};
use anomi_vm::shell::Intent as ShellIntent;

use crate::app::App;
use crate::intent::chat::{composer, open_link, pickers};
use crate::message::Message;

pub(super) fn forum(app: &mut App, intent: Intent) -> Task<Message> {
    let forum = &mut app.vm.chat.forum;
    match intent {
        Intent::Query(query) => {
            forum.query = query;
            Task::none()
        }
        Intent::Search(query) => app.dispatch(Action::SearchPosts(query)),
        Intent::Clear => {
            forum.query.clear();
            app.dispatch(Action::SearchPosts(String::new()))
        }
        Intent::Open { room, beside } => app.dispatch(if beside {
            Action::OpenAside(room)
        } else {
            Action::Open(room)
        }),
        // The cards in view want their pictures; a scroll of the person's
        // near the end wants more, while more is wanted and none comes.
        Intent::Scrolled {
            near_end,
            by_hand,
            first,
            last,
        } => {
            let (shown, pictures) = (&mut app.shown.forum, &mut app.pictures);
            crate::project::chat::forum::seen(&app.core, shown, first, last, pictures);
            let forum = &app.vm.chat.forum;
            if near_end && by_hand && forum.more && !forum.loading {
                return app.dispatch(Action::MorePosts);
            }
            Task::none()
        }
        Intent::NewPost => app.dispatch(Action::NewPost),
        intent => {
            tracing::debug!(?intent, "not the forum's");
            Task::none()
        }
    }
}

pub(super) fn new_post(app: &mut App, intent: PostIntent) -> Task<Message> {
    let write = |title: Option<String>, body: Option<String>| Action::WriteNewPost { title, body };
    match intent {
        PostIntent::Title(title) => app.dispatch(write(Some(title), None)),
        PostIntent::ToBody => anomi_chat::forum::new_post::focus_body(),
        PostIntent::Body(body) => app.dispatch(write(None, Some(body))),
        PostIntent::Focus(part, focused) => {
            form(app, |form| {
                if focused {
                    form.focus = Some(part);
                } else if form.focus == Some(part) {
                    form.focus = None;
                }
            });
            Task::none()
        }
        PostIntent::ToggleTag(tag) => app.dispatch(Action::TogglePostTag(tag)),
        PostIntent::Tags(open) => {
            form(app, |form| form.tags_open = open);
            Task::none()
        }
        PostIntent::ToggleGuidelines => {
            form(app, |form| form.guidelines_open = !form.guidelines_open);
            Task::none()
        }
        PostIntent::OpenLink(link) => open_link(app, link),
        // The post's files are the forum's, as the channel composer's.
        PostIntent::Attach => {
            composer::handle(app, anomi_vm::chat::composer::Intent::Attach.into())
        }
        PostIntent::Unstage(index) => app.dispatch(Action::Unstage(index)),
        PostIntent::ToggleSpoiler(index) => app.dispatch(Action::ToggleSpoiler(index)),
        PostIntent::Create => app.dispatch(Action::CreatePost),
        PostIntent::Close => app.dispatch(Action::CloseNewPost),
        PostIntent::Leave => app.dispatch(Action::LeaveNewPost),
        PostIntent::Stay => app.dispatch(Action::StayInNewPost),
        intent => {
            tracing::debug!(?intent, "nothing the new post asks");
            Task::none()
        }
    }
}

/// Changes the form open, if it is.
fn form(app: &mut App, change: impl FnOnce(&mut NewPost)) {
    if let Some(form) = &mut app.vm.chat.forum.composing {
        change(form);
    }
}

/// The post's bar: its reaction picker under React to post, at its end
/// (`open-picker(root.forum-post-item)`); Follow or Following, which join
/// or leave the thread; Copy link. Each as the thread's menu does them.
pub(super) fn post_bar(app: &mut App, intent: ShellIntent) -> Task<Message> {
    let Some((room, item)) = crate::project::chat::forum::post_item(&app.core) else {
        return Task::none();
    };
    let room = room.to_owned();
    match intent {
        ShellIntent::ReactToPost(rect) => match item {
            Some(item) => pickers::open::for_item(app, item, Spot::below(rect).align(Align::End)),
            None => Task::none(),
        },
        ShellIntent::FollowPost(follow) => {
            let id = if follow {
                "join-thread"
            } else {
                "leave-thread"
            };
            crate::intent::menus::choose_on(app, room, id)
        }
        ShellIntent::CopyPostLink => crate::intent::menus::choose_on(app, room, "copy-link"),
        intent => {
            tracing::debug!(?intent, "not the post's bar's");
            Task::none()
        }
    }
}
