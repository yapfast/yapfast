//! The profile card: task P07's tests, driving
//! `anomi_people::profile::card` over its fixtures in the simulator
//! (`super::support`). The popout is 300 px wide: its banner 105 px, the
//! 80 px avatar in a 6 px ring 16 px in (its middle at 56, 105), the round
//! buttons 32 px over the banner's top right, 10 px in and 8 apart (the
//! menu's middle at 274, 26, the friendship's at 234, 26).

use iced::{Point, Size, keyboard};
use iced_test::Simulator;

use anomi_people::profile::card::{EDIT, FULL, card, panel, view};
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::people::profile as samples;
use anomi_vm::people::profile::{Card, Intent, Place, Tab};
use anomi_vm::people::{self};

use super::support::{asked, front, settings};

const POPOUT: Size = Size::new(300.0, 860.0);
const SIDE: Size = Size::new(400.0, 860.0);
const AVATAR: Point = Point::new(56.0, 105.0);
const MORE: Point = Point::new(274.0, 26.0);
const FRIEND: Point = Point::new(234.0, 26.0);

type Ui<'a> = Simulator<'a, people::Intent, Theme, Renderer>;

fn at(shown: &Card, place: Place, size: Size) -> Ui<'_> {
    Simulator::with_size(settings(), size, card(shown, place, front()))
}

/// What the card asked for, but the pointer coming and going.
fn said(ui: Ui<'_>) -> Vec<Intent> {
    asked(ui)
        .into_iter()
        .filter_map(|intent| match intent {
            people::Intent::Profile(Intent::Pointed(..)) => None,
            people::Intent::Profile(intent) => Some(intent),
            _ => None,
        })
        .collect()
}

fn click(mut ui: Ui<'_>, point: Point) -> Vec<Intent> {
    ui.point_at(point);
    ui.simulate(iced_test::simulator::click());
    said(ui)
}

fn click_text<'a>(mut ui: Ui<'a>, words: &str) -> Vec<Intent> {
    ui.click(words).expect(words);
    said(ui)
}

#[test]
fn the_picture_opens_the_full_profile() {
    let bob = samples::bob();
    assert_eq!(
        click(at(&bob, Place::Popout, POPOUT), AVATAR),
        [Intent::Enlarge(Place::Popout)]
    );
    let side = samples::side();
    assert_eq!(
        click(at(&side, Place::Side, SIDE), Point::new(92.0, 140.0)),
        []
    );
}

#[test]
fn the_menu_opens_under_its_button() {
    let bob = samples::bob();
    let asked = click(at(&bob, Place::Popout, POPOUT), MORE);
    let [Intent::More(id, spot)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    assert_eq!(id, &bob.id);
    assert!(spot.anchor.contains(MORE));
}

#[test]
fn the_friendship_button_befriends_opens_the_menu_or_waits() {
    // A friend's opens their menu, as Discord's does.
    let bob = samples::bob();
    let asked = click(at(&bob, Place::Popout, POPOUT), FRIEND);
    assert!(matches!(asked.as_slice(), [Intent::More(..)]), "{asked:?}");
    let offered = samples::themed();
    assert_eq!(
        click(at(&offered, Place::Popout, POPOUT), FRIEND),
        [Intent::Befriend(offered.id.clone())]
    );
    let sent = samples::loading();
    assert_eq!(click(at(&sent, Place::Popout, POPOUT), FRIEND), []);
}

#[test]
fn each_part_of_the_mutual_line_opens_its_tab() {
    let bob = samples::bob();
    assert_eq!(
        click_text(at(&bob, Place::Popout, POPOUT), "1 Mutual Server"),
        [Intent::EnlargeOn(Place::Popout, Tab::Servers)]
    );
    assert_eq!(
        click_text(at(&bob, Place::Popout, POPOUT), "1 Mutual Friend"),
        [Intent::EnlargeOn(Place::Popout, Tab::Friends)]
    );
    // One's own card says nothing of what's shared.
    let own = samples::own();
    assert!(
        at(&own, Place::Popout, POPOUT)
            .find("1 Mutual Server")
            .is_err()
    );
}

#[test]
fn the_box_messages_them_on_enter() {
    let bob = samples::bob();
    let mut ui = at(&bob, Place::Popout, POPOUT);
    let chip = ui.find("Mods").expect("the role's chip").bounds();
    // The box is the section 12 px under the role's chip, 36 px tall.
    ui.point_at(Point::new(150.0, chip.y + chip.height + 12.0 + 18.0));
    ui.simulate(iced_test::simulator::click());
    ui.typewrite("hi");
    ui.tap_key(keyboard::key::Named::Enter);
    let asked = said(ui);
    assert!(
        asked.contains(&Intent::Draft(Place::Popout, "h".into())),
        "{asked:?}"
    );
    assert!(asked.contains(&Intent::Send(Place::Popout)), "{asked:?}");
}

#[test]
fn ones_own_card_edits_the_profile() {
    let own = samples::own();
    let mut ui = at(&own, Place::Popout, POPOUT);
    ui.click(iced::widget::Id::new(EDIT)).expect("Edit Profile");
    assert_eq!(said(ui), [Intent::EditProfile]);
    assert_eq!(
        click_text(at(&own, Place::Side, SIDE), "Edit Profile"),
        [Intent::EditProfile]
    );
    // The editor's preview opens nothing.
    assert!(
        at(&own, Place::Preview, POPOUT)
            .find(iced::widget::Id::new(EDIT))
            .is_err()
    );
    assert_eq!(click(at(&own, Place::Preview, POPOUT), AVATAR), []);
}

#[test]
fn the_panel_opens_the_full_profile_from_its_foot() {
    let profile = samples::at(Place::Panel, samples::bob_docked());
    let mut ui: Ui<'_> = Simulator::with_size(
        settings(),
        Size::new(304.0, 844.0),
        panel(&profile, front()),
    );
    ui.click(iced::widget::Id::new(FULL))
        .expect("the panel's button");
    assert_eq!(said(ui), [Intent::Enlarge(Place::Panel)]);
    // Beside the DM it has the dates and no box.
    let mut ui: Ui<'_> = Simulator::with_size(
        settings(),
        Size::new(304.0, 844.0),
        panel(&profile, front()),
    );
    ui.find("Member Since").expect("the dates' title");
    assert!(ui.find("Message @Bob").is_err());
}

#[test]
fn the_full_profiles_side_has_its_actions_connections_and_note() {
    let side = samples::side();
    assert_eq!(
        click_text(at(&side, Place::Side, SIDE), "Message"),
        [Intent::Message(side.id.clone())]
    );
    assert_eq!(
        click_text(at(&side, Place::Side, SIDE), "bobthebuilder"),
        [Intent::OpenLink("https://steamcommunity.com/id/bob".into())]
    );
    // An account with nowhere to go opens nothing.
    assert_eq!(click_text(at(&side, Place::Side, SIDE), "bob"), []);
    let mut ui = at(&side, Place::Side, SIDE);
    for title in [
        "Friends Since",
        "Roles",
        "Connections",
        "Note (only visible to you)",
    ] {
        ui.find(title).expect(title);
    }
    // Nine badges: six on the name's line, then "+3", which shows them all.
    assert_eq!(
        click_text(at(&side, Place::Side, SIDE), "+3"),
        [Intent::AllBadges(Place::Side)]
    );
}

#[test]
fn the_bubble_opens_under_the_pointer() {
    let carol = samples::carol();
    let mut ui = at(&carol, Place::Popout, POPOUT);
    ui.point_at(Point::new(150.0, 122.0));
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: Point::new(150.0, 122.0),
    })]);
    assert!(said(ui).contains(&Intent::Bubble(Place::Popout, true)));
}

#[test]
fn the_popout_is_the_window_profile_card() {
    let vm = samples::popout_carol();
    let mut ui: Ui<'_> =
        Simulator::with_size(settings(), POPOUT, view(&vm.people.profile, front()));
    ui.find("Carol").expect("her name");
    ui.find("1 Mutual Server").expect("what they share");
}

#[test]
fn a_long_bio_is_cut_on_the_popout_with_a_way_to_all_of_it() {
    let mut bob = samples::bob();
    bob.bio = samples::side().bio.map(|_| long_bio());
    assert_eq!(
        click_text(at(&bob, Place::Popout, POPOUT), "View full bio"),
        [Intent::Enlarge(Place::Popout)]
    );
    assert!(
        at(&bob, Place::Panel, Size::new(304.0, 860.0))
            .find("View full bio")
            .is_err()
    );
}

fn long_bio() -> anomi_vm::text::Text {
    anomi_vm::text::plain(&"A line of a long bio that goes on. ".repeat(12))
}
