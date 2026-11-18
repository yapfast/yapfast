//! Active Now beside the friends: friends together in a server's voice
//! channel, at the same game or party, or each on their own, a card each in
//! Discord's order (`src/ui/active_now.rs`). Made only while the Friends
//! screen shows; a card keeps its id (the voice room, or `person:` and the
//! first friend's id) while it stays, so it keeps its state on screen.

use std::collections::HashMap;

use yapfast_core::application::AppState;
use yapfast_core::chat::ChatRoom;

use anomi_vm::people::active_now::{ActiveNow, Card};

use crate::pictures::{self, Requests, Watch};

use super::lines;

/// What the column was made from.
#[derive(Default)]
pub(crate) struct Shown {
    key: Option<(u64, u64)>,
    pictures: Watch<()>,
}

/// Shows the cards while the Friends screen shows them.
pub(super) fn show(
    state: &AppState,
    open: bool,
    active: &mut ActiveNow,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    if !open {
        if shown.key.take().is_some() {
            *active = ActiveNow::default();
            shown.pictures = Watch::default();
        }
        return;
    }
    let key = (state.people_changes, state.list_changes);
    if shown.key == Some(key) && shown.pictures.moved(state).is_empty() {
        return;
    }
    let mut watch = std::mem::take(&mut shown.pictures);
    active.cards = pictures::noting(state, &mut watch, || cards(state));
    requests.want(state, watch.addresses());
    shown.pictures = watch;
    shown.key = Some(key);
    active.now = lines::now();
}

/// A second passed: the cards' times count on.
pub(crate) fn tick(active: &mut ActiveNow) {
    active.now = lines::now();
}

#[cfg(not(feature = "discord"))]
fn cards(_state: &AppState) -> Vec<Card> {
    Vec::new()
}

#[cfg(feature = "discord")]
fn cards(state: &AppState) -> Vec<Card> {
    let rooms: HashMap<&str, &ChatRoom> = state
        .rooms
        .iter()
        .map(|room| (room.id.as_str(), &**room))
        .collect();
    made::active_now(state, &rooms)
}

/// "Ann", "Ann and Bob", "Ann, Bob, and 3 others".
fn names(names: &[&str]) -> String {
    use yapfast_lang::{tr, trn};
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [one, two] => tr!("{} and {}", one, two),
        [one, two, rest @ ..] => trn!(
            "{}, {}, and {n} other",
            "{}, {}, and {n} others",
            rest.len(),
            one,
            two
        ),
    }
}

#[cfg(feature = "discord")]
mod made;

#[cfg(test)]
mod tests {
    use super::names;

    #[test]
    fn names_shorten_as_discords_active_now_does() {
        assert_eq!(names(&["A"]), "A");
        assert_eq!(names(&["A", "B"]), "A and B");
        assert_eq!(names(&["A", "B", "C"]), "A, B, and 1 other");
        assert_eq!(names(&["A", "B", "C", "D", "E"]), "A, B, and 3 others");
    }
}
