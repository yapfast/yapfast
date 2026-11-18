//! The DM list: task N03's tests, driving the whole window over the shell's
//! fixtures in the simulator (`super::support`). The window is the golden
//! one (1400 by 860): the sidebar's rows are 32 tall (Friends at y 72,
//! Requests at 106), the heading at 150 with its buttons at x 328 and 356,
//! and the DMs 42 tall and 2 apart from y 172, Bob's centre at 193.

use iced::{Event, Point, mouse};

use anomi_vm::fixtures::shell;
use anomi_vm::fixtures::sidebar::dms as samples;
use anomi_vm::sidebar::Place;
use anomi_vm::sidebar::dms::{Dms, Empty};
use anomi_vm::{Intent, Vm, sidebar};

use super::support::simulate;

/// What the list asked for, but where the pointer is (`Pointed`), which
/// only moves the pointed row's pictures: see
/// `the_pointer_on_a_dm_names_it_and_leaving_names_none`.
fn asked(
    ui: iced_test::Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer>,
) -> Vec<Intent> {
    super::support::asked(ui)
        .into_iter()
        .filter(|intent| {
            !matches!(
                intent,
                Intent::Sidebar(sidebar::Intent::Dms(sidebar::dms::Intent::Pointed(_)))
            )
        })
        .collect()
}

fn dms_intent(intent: sidebar::dms::Intent) -> Intent {
    Intent::Sidebar(sidebar::Intent::Dms(intent))
}

/// The home window with `dms` in its list.
fn home(dms: Dms) -> Vm {
    let mut vm = shell::dm_home();
    vm.sidebar.place = Place::Dms(dms);
    vm.sidebar.version += 1;
    vm
}

/// The centre of the DM at `at` in the sample lists.
fn row(at: usize) -> Point {
    Point::new(150.0, 193.0 + 44.0 * at as f32)
}

fn click_at(vm: &Vm, at: Point) -> Vec<Intent> {
    let mut ui = simulate(vm);
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

fn right_click_at(vm: &Vm, at: Point) -> Vec<Intent> {
    let mut ui = simulate(vm);
    ui.point_at(at);
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
    asked(ui)
}

#[test]
fn clicking_a_dm_asks_to_open_it() {
    let vm = shell::dm_home();
    let mut ui = simulate(&vm);
    ui.click("Person 4").expect("Person 4's row");
    assert_eq!(
        asked(ui),
        [dms_intent(sidebar::dms::Intent::Open("discord:p4".into()))]
    );
}

#[test]
fn clicking_the_row_beside_its_name_opens_it_too() {
    let vm = shell::dm_home();
    assert_eq!(
        click_at(&vm, Point::new(330.0, row(1).y)),
        [dms_intent(sidebar::dms::Intent::Open("discord:d2".into()))]
    );
}

#[test]
fn clicking_friends_asks_for_the_friends_screen() {
    let vm = shell::dm_home();
    let mut ui = simulate(&vm);
    // No language is chosen in tests, so the text is the code's own.
    ui.click("Friends").expect("the Friends row");
    assert_eq!(asked(ui), [dms_intent(sidebar::dms::Intent::Friends)]);
}

#[test]
fn clicking_requests_asks_for_the_requests_screen() {
    let vm = shell::dm_home();
    assert_eq!(
        click_at(&vm, Point::new(150.0, 106.0)),
        [dms_intent(sidebar::dms::Intent::Requests)]
    );
}

#[test]
fn the_headings_buttons_add_a_friend_and_start_a_dm() {
    let vm = shell::dm_home();
    assert_eq!(
        click_at(&vm, Point::new(328.0, 150.0)),
        [dms_intent(sidebar::dms::Intent::AddFriend)]
    );
    assert_eq!(
        click_at(&vm, Point::new(356.0, 150.0)),
        [dms_intent(sidebar::dms::Intent::NewMessage)]
    );
}

#[test]
fn without_discord_there_are_no_links_and_no_friend_to_add() {
    let vm = home(Dms {
        no_discord: true,
        ..samples::dms()
    });
    let mut ui = simulate(&vm);
    assert!(ui.find("Friends").is_err());
    assert!(ui.find("Requests").is_err());
    // The heading stays, with only the new-message button at its end.
    assert!(ui.find("Direct Messages").is_ok());
    assert_eq!(
        click_at(&vm, Point::new(356.0, 70.0)),
        [dms_intent(sidebar::dms::Intent::NewMessage)]
    );
}

#[test]
fn a_preview_lists_the_dms_under_neither_links_nor_heading() {
    let vm = home(Dms {
        preview: true,
        ..samples::dms()
    });
    let mut ui = simulate(&vm);
    assert!(ui.find("Friends").is_err());
    assert!(ui.find("Direct Messages").is_err());
    assert!(ui.find("Bob").is_ok());
}

#[test]
fn an_empty_dm_offers_a_wave_when_pointed_at() {
    let vm = home(samples::states());
    let mut ui = simulate(&vm);
    let newcomer = ui.find("Newcomer").expect("the row");
    let at = newcomer.visible_bounds().expect("shown").center();
    // Its button sits at the row's end, in the 42 px row.
    let wave = Point::new(340.0, at.y);
    let mut ui = simulate(&vm);
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
    ui.point_at(wave);
    ui.simulate(iced_test::simulator::click());
    assert_eq!(
        asked(ui),
        [dms_intent(sidebar::dms::Intent::Wave("discord:u8".into()))]
    );
}

#[test]
fn a_voice_spot_goes_to_the_channel_and_not_to_the_dm() {
    let vm = home(samples::states());
    let mut ui = simulate(&vm);
    ui.click("Lounge \u{2014} Server").expect("the voice spot");
    assert_eq!(
        asked(ui),
        [dms_intent(sidebar::dms::Intent::OpenVoice(
            "discord:v1".into()
        ))]
    );
}

#[test]
fn a_right_click_asks_for_the_dms_menu_at_the_pointer() {
    let vm = shell::dm_home();
    let at = row(2);
    let intents = right_click_at(&vm, at);
    let [Intent::Sidebar(sidebar::Intent::Dms(sidebar::dms::Intent::Menu(id, spot)))] =
        intents.as_slice()
    else {
        panic!("{intents:?}")
    };
    assert_eq!(id, "discord:p5");
    assert!((spot.anchor.x - at.x).abs() < 1.0 && (spot.anchor.y - at.y).abs() < 1.0);
}

#[test]
fn a_long_list_builds_only_what_is_near_the_view() {
    let vm = home(samples::many(2_000));
    let mut ui = simulate(&vm);
    assert!(ui.find("Contact 0").is_ok());
    assert!(ui.find("Contact 1999").is_err());
}

#[test]
fn a_list_with_no_rows_says_why() {
    let vm = home(samples::none(Empty::NoMatch));
    let mut ui = simulate(&vm);
    assert!(ui.find("No matching conversations.").is_ok());
    let vm = home(samples::none(Empty::NoDms));
    let mut ui = simulate(&vm);
    assert!(
        ui.find("No direct messages yet. Existing DMs appear here after syncing.")
            .is_ok()
    );
    // Friends and the heading still show above it.
    assert!(ui.find("Friends").is_ok());
}

#[test]
fn the_pointer_on_a_dm_names_it_and_leaving_names_none() {
    let vm = shell::dm_home();
    let mut ui = simulate(&vm);
    let at = ui
        .find("Person 4")
        .expect("the row")
        .visible_bounds()
        .expect("shown")
        .center();
    for position in [at, Point::new(700.0, 5.0)] {
        ui.point_at(position);
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position })]);
    }
    let pointed =
        |room: Option<&str>| dms_intent(sidebar::dms::Intent::Pointed(room.map(str::to_owned)));
    assert_eq!(
        super::support::asked(ui),
        [pointed(Some("discord:p4")), pointed(None)]
    );
}
