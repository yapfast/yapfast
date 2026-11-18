//! The dialogs about a person (a note, a nickname, a call, unfriending,
//! blocking, and in a server a nickname, a timeout, a kick or a ban:
//! `show_person_dialog`, `src/ui/refresh.rs:3377`) and the friends picker
//! that adds people to a DM (`:2379`).

use yapfast_core::application::{AppState, BAN_HISTORY, PersonAsk, Relation, TIMEOUT_TIMES};
use yapfast_lang::{tr, trn};

use anomi_vm::people::add_people::{AddPeople, Person as Pick};
use anomi_vm::people::person::{self, Person};
use anomi_vm::shell::{Layer, Shell};

use crate::pictures::Requests;
use crate::project::rows;

/// The dialog about a person, open while the core asks it. Its field starts
/// from the note or nickname there is once that's here, and keeps what's
/// typed after.
pub(super) fn show_person(
    state: &AppState,
    shell: &mut Shell,
    dialog: &mut Person,
    focus: &mut Option<&'static str>,
) {
    let Some(asked) = &state.person_dialog else {
        shell.close(Layer::Person);
        return;
    };
    let opening = !shell.is_open(Layer::Person);
    if opening {
        shell.open(Layer::Person);
    }
    if let Some(text) = &asked.text
        && (opening || dialog.loading)
    {
        dialog.text_now.clone_from(text);
    } else if opening {
        dialog.text_now.clear();
    }
    let name = asked.name.as_str();
    let (title, text, field, confirm, danger) = words(state, asked.ask, &asked.user, name);
    let (heading, options): (&str, Vec<String>) = match asked.ask {
        PersonAsk::Timeout => (tr!("DURATION"), TIMEOUT_TIMES.map(span).to_vec()),
        PersonAsk::Ban => (
            tr!("DELETE THEIR MESSAGES FROM"),
            // As Discord's: hours up to a day, then days.
            BAN_HISTORY
                .map(|seconds| match seconds / 3600 {
                    0 => tr!("Don't delete any").to_owned(),
                    1 => tr!("The last hour").to_owned(),
                    hours if hours <= 24 => trn!("The last {n} hour", "The last {n} hours", hours),
                    hours => trn!("The last {n} day", "The last {n} days", hours / 24),
                })
                .to_vec(),
        ),
        _ => ("", Vec::new()),
    };
    *dialog = Person {
        title,
        text,
        options_heading: heading.to_owned(),
        options: person::choices(options, asked.choice),
        field: field.to_owned(),
        text_now: std::mem::take(&mut dialog.text_now),
        tick: (asked.ask == PersonAsk::Call)
            .then(|| person::tick(tr!("Don't ask again"), state.skip_call_prompt)),
        confirm: confirm.to_owned(),
        danger,
        loading: asked.text.is_none(),
        saving: asked.saving,
        error: asked.error.clone().unwrap_or_default(),
    };
    // Its field takes the keys as it opens, else its button.
    if opening {
        *focus = Some(if dialog.has_field() {
            FIELD
        } else {
            anomi_people::person::CONFIRM
        });
    }
}

/// The dialog's field (`anomi_people::person::field_id`) and the picker's
/// (`anomi_people::add_people::query_id`).
const FIELD: &str = "person-field";
const QUERY: &str = "add-people-query";

/// The dialog's title, what it explains, its field's name, its button and
/// whether it's hard to undo.
fn words(
    state: &AppState,
    ask: PersonAsk,
    user: &str,
    name: &str,
) -> (String, String, &'static str, &'static str, bool) {
    match ask {
        PersonAsk::Note => (
            tr!("Note on {}", name),
            tr!("Only you see your note, in Discord too.").into(),
            tr!("Note"),
            tr!("Save"),
            false,
        ),
        PersonAsk::Call => (
            tr!("Ready to start a call?").into(),
            tr!("Enjoy this moment. Then jump in when you're ready.").into(),
            "",
            tr!("Start Call"),
            false,
        ),
        PersonAsk::Nickname => (
            tr!("Nickname for {}", name),
            tr!("Only you see the nickname you give a friend. Leave it empty to remove it.").into(),
            tr!("Nickname"),
            tr!("Save"),
            false,
        ),
        PersonAsk::Unfriend => (
            tr!("Remove {} as a friend?", name),
            tr!(
                "You'll need to send {} a friend request to be friends again.",
                name
            ),
            "",
            tr!("Remove friend"),
            true,
        ),
        PersonAsk::Block => (
            tr!("Block {}?", name),
            tr!(
                "Blocking {} also removes them as a friend. Their messages show collapsed, and they can't message you.",
                name
            ),
            "",
            tr!("Block"),
            true,
        ),
        PersonAsk::ServerNickname if state.relation(user) == Relation::Me => (
            tr!("Change your nickname").into(),
            tr!("Everyone in this server sees it. Leave it empty to go by your name.").into(),
            tr!("Nickname"),
            tr!("Save"),
            false,
        ),
        PersonAsk::ServerNickname => (
            tr!("Change {}'s nickname", name),
            tr!("Everyone in this server sees it. Leave it empty to reset it to their name.")
                .into(),
            tr!("Nickname"),
            tr!("Save"),
            false,
        ),
        PersonAsk::Timeout => (
            tr!("Time out {}", name),
            tr!(
                "Until it ends, {} can read the server but can't send messages, react, or join voice. The reason goes in the server's audit log.",
                name
            ),
            tr!("Reason (optional)"),
            tr!("Time out"),
            true,
        ),
        PersonAsk::Kick => (
            tr!("Kick {}?", name),
            tr!(
                "{} can join again with a new invite. The reason goes in the server's audit log.",
                name
            ),
            tr!("Reason (optional)"),
            tr!("Kick"),
            true,
        ),
        PersonAsk::Ban => (
            tr!("Ban {}?", name),
            tr!(
                "{} can't join again until unbanned. The reason goes in the server's audit log.",
                name
            ),
            tr!("Reason (optional)"),
            tr!("Ban"),
            true,
        ),
    }
}

/// A length of time as the timeout's choices say it: "5 minutes".
fn span(seconds: u32) -> String {
    match seconds {
        seconds if seconds % (7 * 86400) == 0 => {
            trn!("{n} week", "{n} weeks", seconds / (7 * 86400))
        }
        seconds if seconds % 86400 == 0 => trn!("{n} day", "{n} days", seconds / 86400),
        seconds if seconds % 3600 == 0 => trn!("{n} hour", "{n} hours", seconds / 3600),
        seconds if seconds % 60 == 0 && seconds > 60 => {
            trn!("{n} minute", "{n} minutes", seconds / 60)
        }
        seconds => trn!("{n} second", "{n} seconds", seconds),
    }
}

/// The friends picker of the open DM, while the core has it: the friends
/// it offers, those chosen ticked; its field empties as it opens. The
/// header's button opens and closes its layer.
pub(super) fn show_add_people(
    state: &AppState,
    opened: bool,
    add: &mut AddPeople,
    requests: &mut Requests,
    focus: &mut Option<&'static str>,
) {
    let picker = state
        .add_people
        .as_ref()
        .filter(|picker| Some(&picker.room) == state.selected.as_ref());
    let Some(picker) = picker else {
        if !add.people.is_empty() {
            *add = AddPeople::default();
        }
        return;
    };
    if opened {
        add.query.clear();
        *focus = Some(QUERY);
    }
    let friends = state.addable_friends();
    requests.want(
        state,
        friends.iter().filter_map(|friend| friend.avatar.as_deref()),
    );
    let people: Vec<Pick> = friends
        .into_iter()
        .map(|friend| Pick {
            id: friend.id.clone(),
            name: friend.name.clone(),
            avatar: rows::avatar(state, &friend.name, &friend.name, friend.avatar.as_deref()),
            presence: rows::presence(friend.presence),
            chosen: picker.chosen.contains(&friend.id),
        })
        .collect();
    *add = AddPeople {
        query: std::mem::take(&mut add.query),
        people,
        room: state.group_room(),
        chosen: picker.chosen.len(),
        one_to_one: state.one_to_one().is_some(),
        adding: picker.adding,
        note: picker.error.clone().unwrap_or_default(),
    };
}
