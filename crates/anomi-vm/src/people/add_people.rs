//! Adding people to a DM: what it shows and what it asks for. Task P13
//! (docs/iced/tasks/people.md). The Slint side is `AddPeoplePopover`
//! (`ui/header.slint:539`), filled by `src/ui/refresh.rs` from
//! `State::addable_friends` and answered by `src/ui/callbacks.rs:1804`
//! (`Action::AddPeopleQuery`, `ChoosePerson`, `AddPeople`,
//! `CloseAddPeople`).

use crate::kind::{Avatar, Presence};

/// A friend the picker offers (`PickRow`, `ui/header.slint:56`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Person {
    /// The user id, which `Intent::Choose` carries.
    pub id: String,
    pub name: String,
    pub avatar: Avatar,
    /// `None` and `Offline` draw no dot.
    pub presence: Presence,
    /// Picked already: its box is ticked.
    pub chosen: bool,
}

/// Adding people to a DM: what its popover draws. The window holds it as
/// `vm.people.add_people`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AddPeople {
    /// What the search field holds; the app keeps it as typed.
    pub query: String,
    /// The friends the query leaves, not in the DM yet. Chosen ones the
    /// query hides are not here, but they count in `chosen`.
    pub people: Vec<Person>,
    /// How many more the group can take, before any is chosen
    /// (`State::group_room`).
    pub room: usize,
    /// How many are chosen, in view or not.
    pub chosen: usize,
    /// The DM is one-to-one, so adding makes a new group.
    pub one_to_one: bool,
    /// The request is on its way: nothing can be chosen or sent.
    pub adding: bool,
    /// Why adding failed; empty when it did not.
    pub note: String,
}

impl AddPeople {
    /// How many more may be chosen (`room - chosen`).
    pub fn left(&self) -> usize {
        self.room.saturating_sub(self.chosen)
    }

    /// Whether `person` can be clicked: nothing is on its way, and either
    /// it is chosen (a click unchooses) or there is room
    /// (`can-choose`, `:581`).
    pub fn may_choose(&self, person: &Person) -> bool {
        !self.adding && (person.chosen || self.left() > 0)
    }

    /// Whether the button and Enter may add: someone is chosen and none is
    /// on its way.
    pub fn may_add(&self) -> bool {
        self.chosen > 0 && !self.adding
    }
}

/// Adding people to a DM: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The search field changed: the app keeps what was typed and filters
    /// (`Action::AddPeopleQuery`).
    Query(String),
    /// A row was clicked: choose this user id, or unchoose it
    /// (`Action::ChoosePerson`). Never asked of a row that may not be.
    Choose(String),
    /// The button, or Enter in the field: add those chosen, or make a group
    /// (`Action::AddPeople`). Never with none chosen or one on its way.
    Add,
    /// The close button (Escape and a press outside are the shell's).
    Close,
}
