//! People from the core's state: the member list with its Activity panel,
//! the profile cards and the full profile, Friends with its requests, Add
//! Friend and Active Now, the dialogs about a person, the cards of roles,
//! server tags and mutual servers, and the friends picker of a DM. Task I16
//! (docs/iced/tasks/integration.md); the Slint side is `src/ui/profile.rs`,
//! `src/ui/active_now.rs` and `show_members`, `show_profile`,
//! `show_person_dialog` and the friend half of `show_sidebar` in
//! `src/ui/refresh.rs`.
//!
//! Each part is made only while it shows, and again only when what it reads
//! changed: the member list from the core's shared list, member by member;
//! a profile from its `Arc<ProfileView>`; Friends from the core's count of
//! people changes.

mod active_now;
mod activity;
pub(crate) mod card;
pub(crate) mod cards;
mod dialogs;
mod friends;
pub(crate) mod full;
pub(crate) mod lines;
mod members;
pub(crate) mod profile;
#[cfg(test)]
pub(crate) mod tests;

pub(crate) use cards::ShownTag;

use yapfast_core::application::Application;

use anomi_vm::Vm;
use anomi_vm::conversation::Body;
use anomi_vm::people::active_now::Kind;
use anomi_vm::shell::Layer;

use crate::layout::Layout;
use crate::pictures::Requests;
use crate::project::Reach;

/// What people's parts were made from.
#[derive(Default)]
pub struct Shown {
    pub(crate) members: members::Shown,
    activity: activity::Shown,
    pub(crate) profile: profile::Shown,
    friends: friends::Shown,
    active_now: active_now::Shown,
    /// The server tag whose card is open.
    pub(crate) tag: Option<ShownTag>,
    /// The friends picker showed at the last refresh.
    picker: bool,
    /// The window's height, which a role's card keeps within and the full
    /// profile's frame needs room in; 0 until the window says it.
    pub(crate) height: f32,
}

/// Shows people's parts as far as `reach` can have changed them.
pub(super) fn show(
    core: &Application,
    layout: &Layout,
    vm: &mut Vm,
    shown: &mut Shown,
    requests: &mut Requests,
    reach: Reach,
    focus: &mut Option<&'static str>,
) {
    use Reach as R;
    let state = &core.state;
    let listed = vm.conversation.beside.members;
    if reach.touches(R::STATE | R::PICTURES | R::EMOJI | R::MEMBERS) {
        let people = &mut vm.people;
        members::show(
            core,
            layout,
            listed,
            &mut people.members,
            &mut shown.members,
            requests,
        );
    }
    if reach.touches(R::STATE | R::PICTURES | R::MEMBERS | R::MINUTE) {
        let activity = &mut vm.people.activity;
        activity::show(state, listed, activity, &mut shown.activity, requests);
    }
    if reach.touches(R::STATE | R::PICTURES | R::EMOJI) {
        profile::show(core, shown.height, vm, &mut shown.profile, requests);
        let body = vm.conversation.body;
        let people = &mut vm.people;
        let friends_open = body == Body::Friends;
        friends::show_friends(
            state,
            friends_open,
            &mut people.friends,
            &mut shown.friends,
            requests,
        );
        let active = &mut people.active_now;
        active_now::show(state, friends_open, active, &mut shown.active_now, requests);
        let requests_open = body == Body::Requests;
        let asked = &mut people.requests;
        friends::show_requests(
            state,
            layout,
            requests_open,
            asked,
            &mut shown.friends,
            requests,
        );
        friends::show_add_friend(state, &mut vm.shell, &mut people.add_friend, focus);
        dialogs::show_person(state, &mut vm.shell, &mut people.person, focus);
        cards::show(
            state,
            shown.height,
            shown.tag.as_ref(),
            &mut vm.shell,
            &mut people.cards,
        );
        requests.want(state, cards::wanted(state));
        if let Some(icon) = shown
            .tag
            .as_ref()
            .and_then(|tag| cards::tag_wanted(state, tag))
        {
            requests.want(state, [icon]);
        }
    } else if reach.touches(R::MINUTE) {
        active_now::tick(&mut vm.people.active_now);
    }
    if reach.touches(R::STATE | R::PICTURES | R::PLACES) {
        let open = state.add_people.is_some();
        let opened = open && !shown.picker;
        shown.picker = open;
        dialogs::show_add_people(state, opened, &mut vm.people.add_people, requests, focus);
    }
}

/// The member list's rows in view changed, by place: their pictures are
/// asked for.
pub fn members_seen(
    state: &yapfast_core::application::AppState,
    shown: &mut Shown,
    first: usize,
    last: usize,
    requests: &mut Requests,
) {
    members::seen(state, &mut shown.members, first, last, requests);
}

/// The pointer came onto a member's row, or left the list's rows.
pub fn members_pointed(shown: &mut Shown, user: Option<String>) {
    shown.members.pointed = user;
}

/// Whether a card on screen counts seconds: an activity on a profile card,
/// or Active Now's rows.
pub fn seconds(vm: &Vm, shown: &Shown) -> bool {
    shown.profile.ticking
        || (vm.conversation.body == Body::Friends
            && vm.people.active_now.cards.iter().any(|card| {
                card.rows
                    .iter()
                    .any(|row| row.kind == Kind::Detail && row.start > 0)
            }))
}

/// Whether something shows minutes: the Activity panel's times, Active
/// Now's cards'.
pub fn minutes(vm: &Vm) -> bool {
    activity::timed(&vm.people.activity)
        || (vm.conversation.body == Body::Friends && vm.people.active_now.wants_clock())
}

/// A second passed: the cards that count seconds say their times again.
pub fn tick(vm: &mut Vm) {
    profile::tick(vm);
    if vm.conversation.body == Body::Friends {
        active_now::tick(&mut vm.people.active_now);
    }
}

/// A layer about people played its way out: what it held goes.
pub fn gone(
    state: &yapfast_core::application::AppState,
    layer: Layer,
    vm: &mut Vm,
    shown: &mut Shown,
) {
    match layer {
        Layer::Profile | Layer::FullProfile => profile::gone(state, layer, vm, &mut shown.profile),
        Layer::Card => {
            shown.tag = None;
            vm.people.cards.server = None;
            vm.people.cards.tag = None;
        }
        Layer::RoleCard if state.role.is_none() => vm.people.cards.role = None,
        Layer::Person if state.person_dialog.is_none() => vm.people.person = Default::default(),
        _ => {}
    }
}
