//! The "Create a Poll" dialog from the core's `poll_draft`
//! (`show_poll_draft`, `src/ui/polls.rs`): its layer is open while the core
//! makes a poll, its question takes the keys as it opens, and each answer
//! shows the emoji the picker gave it (I12's `for_poll`), a server's with
//! its picture, asked for while it shows.

use yapfast_core::application::{AppState, PollDraft, QUESTION_LIMIT};

use anomi_vm::chat::poll::{Dialog, Draft};
use anomi_vm::people::status::Emoji;
use anomi_vm::shell::{Layer, Shell};

use crate::pictures::{self, Requests};
use crate::project::account::status;

/// Shows the poll being made, opening or closing its layer as the core
/// starts or lets go of it.
pub(crate) fn show(
    state: &AppState,
    shell: &mut Shell,
    dialog: &mut Dialog,
    requests: &mut Requests,
    focus: &mut Option<&'static str>,
) {
    let draft = state.poll_draft.as_ref();
    match (draft, shell.is_open(Layer::Poll)) {
        (Some(_), false) => {
            shell.open(Layer::Poll);
            *focus = Some(anomi_chat::poll::QUESTION);
        }
        (None, true) => shell.close(Layer::Poll),
        _ => {}
    }
    let Some(draft) = draft else {
        return;
    };
    let made = made(state, draft, requests);
    if *dialog != made {
        *dialog = made;
    }
}

fn made(state: &AppState, draft: &PollDraft, requests: &mut Requests) -> Dialog {
    Dialog {
        question: draft.question.clone(),
        question_count: format!("{} / {QUESTION_LIMIT}", draft.question.chars().count()),
        answers: draft
            .answers
            .iter()
            .map(|answer| Draft {
                id: u64::from(answer.id),
                text: answer.text.clone(),
                emoji: answer
                    .emoji
                    .as_deref()
                    .map(|key| emoji(state, key, requests)),
            })
            .collect(),
        duration: draft.duration,
        multiselect: draft.multiselect,
        can_add: draft.can_add(),
        can_post: draft.can_post(),
        posting: draft.posting,
        problem: draft
            .error
            .as_deref()
            .or(draft.problem())
            .unwrap_or_default()
            .to_owned(),
    }
}

/// An answer's emoji as the picker gave it: Unicode's own, or a server's
/// `name:id` with its picture, asked for while it hasn't come.
fn emoji(state: &AppState, key: &str, requests: &mut Requests) -> Emoji {
    let Some(address) = status::custom_picture(key) else {
        return status::unicode(key);
    };
    requests.want(state, [address.as_str()]);
    let name = key.rsplit_once(':').map_or(key, |(name, _)| name);
    Emoji::custom(
        key,
        &format!(":{name}:"),
        pictures::find(state, Some(&address)),
    )
}

/// The custom emoji pictures the poll being made shows, which the emoji
/// picker keeps as it lets go of its own.
pub(crate) fn pictures(state: &AppState) -> Vec<String> {
    let answers = state.poll_draft.iter().flat_map(|draft| &draft.answers);
    answers
        .filter_map(|answer| status::custom_picture(answer.emoji.as_deref()?))
        .collect()
}
