//! The profile cards from the core's `profile` and `panel_profile`
//! (`show_profile`, `src/ui/refresh.rs:4919`): the popout over the window,
//! or the full profile when the core shows it large, and the card docked
//! beside a one-to-one DM, all made by [`super::card::card`].
//!
//! The core shares each profile as an `Arc<ProfileView>`: a card is made
//! again only when that `Arc`, or what else the card reads, changed. What
//! the person does on a card (`Touch`) stays while the same person shows.
//! The popout and the full profile open and close here as the core shows
//! and lets go of the profile; the intent that asked opens the popout at
//! its spot first.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use yapfast_core::application::{AppState, Application, Cosmetics, ProfileView};

use anomi_vm::Vm;
use anomi_vm::people::profile::Card;
use anomi_vm::shell::Layer;

use super::{card, full, lines};
use crate::pictures::{self, Requests, Watch};

/// What the cards were made from.
#[derive(Default)]
pub(crate) struct Shown {
    popout: Option<Made>,
    panel: Option<Made>,
    /// A card on screen counts an activity's time each second.
    pub(crate) ticking: bool,
}

/// A card made from `view`, with what else it read.
struct Made {
    view: Arc<ProfileView>,
    key: Key,
    pictures: Watch<()>,
    /// Its profile effect's pictures were asked for, as it was about to play.
    effect: bool,
}

/// What a card reads besides its profile.
#[derive(Clone, Copy, PartialEq)]
struct Key {
    people: u64,
    lists: u64,
    cosmetics: Cosmetics,
    effects: (usize, usize),
    status: u64,
    activities: usize,
    /// The window leaves the full profile room for its frame above it.
    framed: bool,
}

/// The least height of a window whose full profile shows a frame: the
/// modal's 560 and 120 for the frame's staples (`profile-frame`,
/// `ui/yapfast.slint:5074`).
const FRAMED: f32 = 560.0 + 120.0;

impl Key {
    fn of(state: &AppState, height: f32) -> Self {
        let mut hasher = DefaultHasher::new();
        if let Some(status) = &state.custom_status {
            (&status.text, &status.emoji, &status.emoji_id).hash(&mut hasher);
        }
        Self {
            people: state.people_changes,
            lists: state.list_changes,
            cosmetics: state.cosmetics,
            effects: (
                state.profile_effects.len(),
                state
                    .profile_effects
                    .last()
                    .map_or(0, |effect| Arc::as_ptr(effect).addr()),
            ),
            status: hasher.finish(),
            activities: Arc::as_ptr(&state.discord_activities).cast::<()>().addr(),
            framed: height == 0.0 || height >= FRAMED,
        }
    }
}

impl Made {
    /// Whether `view` would show as this was made.
    fn same(&mut self, state: &AppState, view: &Arc<ProfileView>, key: Key) -> bool {
        Arc::ptr_eq(&self.view, view) && self.key == key && self.pictures.moved(state).is_empty()
    }
}

/// Makes `view`'s card, noting and asking for the pictures it shows.
fn make(
    core: &Application,
    view: &Arc<ProfileView>,
    key: Key,
    effect: bool,
    panel: bool,
    requests: &mut Requests,
) -> (Card, Made) {
    let state = &core.state;
    let mut watch = Watch::default();
    let card = pictures::noting(state, &mut watch, || {
        let mut made = card::card(core, view);
        if panel {
            card::follow_room(state, &mut made, view);
        }
        made
    });
    note_effect(state, view, effect, &mut watch);
    requests.want(state, watch.addresses());
    let made = Made {
        view: view.clone(),
        key,
        pictures: watch,
        effect,
    };
    (card, made)
}

/// Notes and asks for the pictures of `view`'s profile effect: its still
/// always, as the card shows it at rest, and its layers once it is about
/// to play (`all`).
fn note_effect(state: &AppState, view: &ProfileView, all: bool, watch: &mut Watch<()>) {
    if let Some(effect) = card::effect_of(state, &view.profile) {
        let still = effect.still.as_deref();
        for address in effect
            .pictures()
            .filter(|&address| all || Some(address) == still)
        {
            watch.note(state, address, ());
        }
    }
}

/// Shows the popout or the full profile, and the panel beside a DM.
pub(super) fn show(
    core: &Application,
    height: f32,
    vm: &mut Vm,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let state = &core.state;
    let key = Key::of(state, height);
    match &state.profile {
        Some(view) => popout(core, view, key, vm, shown, requests),
        None => {
            shown.popout = None;
            vm.shell.close(Layer::Profile);
            vm.shell.close(Layer::FullProfile);
        }
    }
    match &state.panel_profile {
        Some(view) => panel(core, view, key, vm, shown, requests),
        None => {
            shown.panel = None;
            vm.people.profile.panel = None;
        }
    }
    let popout = vm.shell.is_open(Layer::Profile);
    shown.ticking = vm.people.profile.wants_clock(popout);
}

fn popout(
    core: &Application,
    view: &Arc<ProfileView>,
    key: Key,
    vm: &mut Vm,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let state = &core.state;
    let people = &mut vm.people;
    let was = shown.popout.as_mut();
    let (same_person, was_large) = was.as_ref().map_or((false, false), |made| {
        (made.view.profile.id == view.profile.id, made.view.large)
    });
    let unchanged = was.is_some_and(|made| made.same(state, view, key));
    if !unchanged {
        let effect = same_person && shown.popout.as_ref().is_some_and(|made| made.effect);
        let (mut made_card, made) = make(core, view, key, effect, false, requests);
        if same_person {
            made_card.touch = if was_large {
                std::mem::take(&mut people.full_profile.card.touch)
            } else {
                std::mem::take(&mut people.profile.card.touch)
            };
        }
        if view.large {
            // The tab shown stays, even as a tab before it arrives.
            let chosen = if same_person && was_large {
                Some(people.full_profile.page())
            } else {
                people.full_profile.chosen.filter(|_| same_person)
            };
            people.full_profile = full::full(state, view, made_card, chosen);
            // No room above it, the frame stays off (`show-frame`).
            if !key.framed {
                people.full_profile.card.frame.clear();
            }
        } else {
            people.profile.card = made_card;
        }
        shown.popout = Some(made);
    }
    if view.large {
        vm.shell.close(Layer::Profile);
        if !vm.shell.is_open(Layer::FullProfile) {
            vm.shell.open(Layer::FullProfile);
        }
    } else {
        vm.shell.close(Layer::FullProfile);
        if !vm.shell.is_open(Layer::Profile) {
            vm.shell.open(Layer::Profile);
        }
    }
}

fn panel(
    core: &Application,
    view: &Arc<ProfileView>,
    key: Key,
    vm: &mut Vm,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let state = &core.state;
    let was = shown.panel.as_mut();
    let same_person = was
        .as_ref()
        .is_some_and(|made| made.view.profile.id == view.profile.id);
    if was.is_some_and(|made| made.same(state, view, key)) && vm.people.profile.panel.is_some() {
        return;
    }
    let effect = same_person && shown.panel.as_ref().is_some_and(|made| made.effect);
    let (mut made_card, made) = make(core, view, key, effect, true, requests);
    if same_person && let Some(old) = vm.people.profile.panel.take() {
        made_card.touch = old.touch;
    }
    vm.people.profile.panel = Some(made_card);
    shown.panel = Some(made);
}

/// A card's profile effect is about to play: its pictures are asked for,
/// and the card is made again as they come.
pub(crate) fn effect_wanted(
    state: &AppState,
    panel: bool,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let made = if panel {
        &mut shown.panel
    } else {
        &mut shown.popout
    };
    if let Some(made) = made
        && !made.effect
    {
        made.effect = true;
        note_effect(state, &made.view, true, &mut made.pictures);
        requests.want(state, made.pictures.addresses());
    }
}

/// A second passed: the cards that count an activity's time say it again.
pub(crate) fn tick(vm: &mut Vm) {
    let now = lines::now();
    let popout = vm.shell.is_open(Layer::Profile);
    let profile = &mut vm.people.profile;
    if popout
        && profile
            .card
            .wants_clock(anomi_vm::people::profile::Place::Popout)
    {
        profile.card.now = now;
    }
    if let Some(panel) = &mut profile.panel {
        panel.now = now;
    }
}

/// The popout or the full profile played its way out: what it showed goes,
/// unless the core shows a profile again.
pub(crate) fn gone(state: &AppState, layer: Layer, vm: &mut Vm, shown: &mut Shown) {
    if state.profile.is_some() {
        return;
    }
    shown.popout = None;
    match layer {
        Layer::FullProfile => vm.people.full_profile = Default::default(),
        _ => vm.people.profile.card = Card::default(),
    }
}
