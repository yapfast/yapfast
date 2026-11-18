//! The form for a post in the forum on screen (`show_new_post`,
//! `src/ui/refresh.rs:2065`): its title and message, which the core keeps
//! while other rooms show; the tags chosen, in the order they were, and
//! every tag the post may take; the forum's guidelines (its topic); the
//! files to go with it, which are the forum's staged ones, as the channel
//! composer's. The fields are the window's while it's open: they're set
//! only when the core's differ, so the caret stays. What Slint kept in the
//! form's own properties (which field has the keys, the menu of tags open,
//! the guidelines folded) is the app's, kept here across shows.

use std::hash::{DefaultHasher, Hash, Hasher};

use yapfast_core::application::{Application, POST_TAGS};
use yapfast_core::chat::ForumTag;

use anomi_vm::chat::new_post::{Choice, NewPost};

use super::mark;
use crate::pictures::Requests;
use crate::project::chat::composer::{Previews, fill_staged};
use crate::project::header::topic;

/// What the form showed last.
#[derive(Default)]
pub(crate) struct Shown {
    tags: Option<u64>,
    guidelines: Option<topic::Shown>,
}

/// The title's field, which takes the keys as the form opens.
const TITLE: &str = "new-post-title";

pub(crate) fn show(
    core: &Application,
    composing: &mut Option<NewPost>,
    shown: &mut Shown,
    previews: &Previews,
    requests: &mut Requests,
    focus: &mut Option<&'static str>,
) {
    let state = &core.state;
    let Some(post) = state.new_post() else {
        if composing.take().is_some() {
            *shown = Shown::default();
        }
        return;
    };
    let form = composing.get_or_insert_with(|| {
        *focus = Some(TITLE);
        NewPost {
            guidelines_open: true,
            ..NewPost::default()
        }
    });
    let key = {
        let mut hasher = DefaultHasher::new();
        post.room.hash(&mut hasher);
        hasher.finish().max(1)
    };
    form.key = key;
    if form.title != post.title {
        post.title.clone_into(&mut form.title);
    }
    if form.body != post.body {
        post.body.clone_into(&mut form.body);
    }
    let info = core.forum_info(&post.room).unwrap_or_default();
    form.tag_required = info.require_tag;
    form.more_tags = post.tags.len() < POST_TAGS;
    let guidelines = (state.topic.as_ref())
        .filter(|topic| !topic.body.trim().is_empty())
        .map(|topic| topic::text(state, topic, &mut shown.guidelines, requests).0)
        .and_then(|text| text);
    if form.guidelines != guidelines {
        form.guidelines = guidelines;
    }
    fill_staged(state.staged(), &mut form.staged, previews);
    form.can_attach = state.can_attach();
    form.can_spoil = state.can_mark_spoilers();
    form.posting = post.posting;
    let error = post.error.as_deref().unwrap_or_default();
    if form.error != error {
        error.clone_into(&mut form.error);
    }
    form.leaving = post.leaving;
    let tags = core.post_tags(&post.room);
    let tags_key = {
        let mut hasher = DefaultHasher::new();
        (&tags, &post.tags, state.picture_changes).hash(&mut hasher);
        hasher.finish()
    };
    if shown.tags == Some(tags_key) {
        return;
    }
    shown.tags = Some(tags_key);
    let choice = |tag: &ForumTag| Choice {
        id: tag.id.clone(),
        name: tag.name.clone(),
        mark: mark(state, tag.emoji.as_ref()),
        chosen: post.tags.contains(&tag.id),
    };
    form.chosen = (post.tags.iter())
        .filter_map(|id| tags.iter().find(|tag| &tag.id == id))
        .map(choice)
        .collect();
    form.choices = tags.iter().map(choice).collect();
    let pictures = tags
        .iter()
        .filter_map(|tag| tag.emoji.as_ref()?.picture.as_deref());
    requests.want(state, pictures);
}
