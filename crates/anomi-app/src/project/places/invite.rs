//! The invite dialog from the core's `invite` (`show_invite`,
//! `src/ui/refresh.rs:2717`), as Discord's: where it lands people, the link
//! with how long it lasts and how often it works, and whom to send the
//! invite to ([`people`]), each with how their invite stands. Sends are the
//! window's: [`send`] asks the backend, and [`heard`] takes Discord's answer
//! before the core does. The rows are built again only when the people, the
//! search, a send or the pictures change, and only while the dialog shows.

mod people;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use yapfast_core::application::{AppState, INVITE_AGES, INVITE_USES, InviteDialog};
use yapfast_core::chat::{Command, DiscordCommand, Event};
use yapfast_lang::{tr, trn};

use anomi_vm::icon::Icon;
use anomi_vm::places::invite::{Invite, Page, Sent};
use anomi_vm::shell::{Layer, Shell};

use super::{follow, set};
use crate::pictures::Requests;
use crate::project::rows;

/// Whom the dialog offers, and how their invites stand.
#[derive(Default)]
pub(crate) struct Shown {
    people: people::People,
    /// How each invite stands, by user id, while the dialog is open.
    sent: HashMap<String, Sent>,
    /// Counts the changes to `sent`, which the rows key on.
    sends: u64,
    /// The rows' key: the gathering's, the search, the sends', the
    /// pictures'.
    rows: Option<(u64, String, u64, u64)>,
}

pub(super) fn show(
    state: &AppState,
    shell: &mut Shell,
    invite: &mut Invite,
    shown: &mut Shown,
    focus: &mut Option<&'static str>,
) {
    let dialog = state.invite.as_ref();
    let opened = follow(shell, Layer::Invite, dialog.is_some());
    let Some(dialog) = dialog else {
        return;
    };
    if opened {
        // The search has the keys, and the dialog starts afresh.
        *focus = Some(anomi_screens::places::invite::SEARCH);
        *shown = Shown::default();
        invite.page = Page::People;
        invite.query.clear();
        invite.folded = false;
        invite.age = index(&INVITE_AGES, dialog.max_age);
        invite.uses = index(&INVITE_USES, dialog.max_uses);
    }
    let channel = state.rooms.iter().find(|room| room.id == dialog.room);
    set(
        &mut invite.title,
        &tr!("Invite friends to {}", dialog.server),
    );
    set(&mut invite.channel, &dialog.channel);
    invite.icon = Some(channel.map_or(
        if dialog.voice {
            Icon::Volume
        } else {
            Icon::Hash
        },
        |room| rows::channel_icon(room),
    ));
    invite.private = channel.is_some_and(|room| room.mark.private);
    let link = dialog.link.as_deref().unwrap_or_default();
    invite.copied = invite.copied && !opened && invite.link == link;
    set(&mut invite.link, link);
    let note = dialog
        .link
        .as_ref()
        .map_or_else(String::new, |_| note(dialog));
    set(&mut invite.note, &note);
    invite.making = dialog.making;
    set(
        &mut invite.error,
        dialog.error.as_deref().unwrap_or_default(),
    );
    shown.people.gather(state, dialog);
    let key = (
        shown.people.version,
        invite.query.clone(),
        shown.sends,
        state.picture_changes,
    );
    if shown.rows.as_ref() != Some(&key) {
        let sent = |user: &str| shown.sent.get(user).cloned().unwrap_or_default();
        (invite.members, invite.friends) = shown.people.rows(state, &invite.query, &sent);
        shown.rows = Some(key);
    }
}

/// The dialog played its way out: its rows go, with what they knew.
pub(super) fn gone(invite: &mut Invite, shown: &mut Shown) {
    invite.members = Vec::new();
    invite.friends = Vec::new();
    *shown = Shown::default();
}

/// The rows in view, by user id: their pictures are wanted.
pub(crate) fn seen(state: &AppState, shown: &Shown, ids: &[String], requests: &mut Requests) {
    requests.want(state, shown.people.avatars(ids));
}

/// What sends someone the invite, unless the dialog doesn't offer them or
/// theirs went already: one that works once and lasts a day, to the
/// dialog's channel, in their DM, as the core's Invite to server sends it
/// (`DiscordCommand::SendInvite`).
pub(crate) fn send(state: &AppState, shown: &mut Shown, user: String) -> Option<Command> {
    let dialog = state.invite.as_ref()?;
    if !shown.people.offers(&user) || shown.sent.contains_key(&user) {
        return None;
    }
    shown.sent.insert(user.clone(), Sent::Sending);
    shown.sends += 1;
    Some(Command::Discord(DiscordCommand::SendInvite {
        user,
        room: dialog.room.clone(),
    }))
}

/// Discord's answer to an invite this dialog sent: its row says Sent, or
/// why not. The core then says a refusal in its notice too.
pub(crate) fn heard(state: &AppState, shown: &mut Shown, event: &Event) {
    let Event::InviteSent { user, room, result } = event else {
        return;
    };
    let ours = state
        .invite
        .as_ref()
        .is_some_and(|dialog| dialog.room == *room);
    let Some(sent) = shown.sent.get_mut(user).filter(|_| ours) else {
        return;
    };
    *sent = match result {
        Ok(()) => Sent::Sent,
        Err(why) => Sent::Failed(why.clone()),
    };
    shown.sends += 1;
}

/// Where a value is in a list of choices, the first for one not offered.
fn index(list: &[u32], value: u32) -> usize {
    list.iter().position(|known| *known == value).unwrap_or(0)
}

/// How long the link lasts, and how often it works when that's limited,
/// said under it.
pub(super) fn note(dialog: &InviteDialog) -> String {
    let lasts = match dialog.max_age {
        0 => tr!("Your invite link never expires.").to_owned(),
        1800 => tr!("Your invite link expires in 30 minutes.").to_owned(),
        age if age % 86_400 == 0 => trn!(
            "Your invite link expires in {n} day.",
            "Your invite link expires in {n} days.",
            age / 86_400
        ),
        age => trn!(
            "Your invite link expires in {n} hour.",
            "Your invite link expires in {n} hours.",
            age / 3600
        ),
    };
    match dialog.max_uses {
        0 => lasts,
        1 => format!("{lasts} {}", tr!("It works once.")),
        uses => format!(
            "{lasts} {}",
            trn!("It works {n} time.", "It works {n} times.", uses)
        ),
    }
}
