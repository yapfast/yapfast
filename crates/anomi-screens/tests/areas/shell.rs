//! The window around the areas: task N01's tests, driving
//! `anomi_screens::window` over the shell's fixtures in the simulator
//! (`super::support`): the panels' edges (let go behind another window
//! too), the layers' Escape and outside presses, the notes, and the
//! conversation's own bar.

use iced::keyboard::key::Named;
use iced::{Event, Point, Size, mouse};

use anomi_theme::{Motion, Theme};
use anomi_vm::fixtures::{conversation, shell as samples};
use anomi_vm::menu::{Choice, Intent as MenuIntent};
use anomi_vm::shell::{Intent as ShellIntent, Layer, Panel};
use anomi_vm::{Intent, Vm};

use super::support::{SIZE, asked, settings, simulate};

type Simulator<'a> = iced_test::Simulator<'a, Intent, Theme, anomi_theme::Renderer>;

fn shell(intent: ShellIntent) -> Intent {
    Intent::Shell(intent)
}

fn press(ui: &mut Simulator<'_>, at: Point) {
    ui.point_at(at);
    ui.simulate([
        Event::Mouse(mouse::Event::CursorMoved { position: at }),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
    ]);
}

fn move_to(ui: &mut Simulator<'_>, at: Point) {
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
}

fn release(ui: &mut Simulator<'_>) {
    ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(
        mouse::Button::Left,
    ))]);
}

/// The sidebar's edge: the rail's 72 px and the sidebar's 304.
const SIDEBAR_EDGE: f32 = 72.0 + 304.0;

#[test]
fn dragging_the_sidebars_edge_resizes_it_within_its_bounds() {
    let vm = samples::dm_home();
    let mut ui = simulate(&vm);
    press(&mut ui, Point::new(SIDEBAR_EDGE, 400.0));
    move_to(&mut ui, Point::new(SIDEBAR_EDGE + 50.0, 400.0));
    move_to(&mut ui, Point::new(SIDEBAR_EDGE + 400.0, 400.0));
    release(&mut ui);
    assert_eq!(
        asked(ui),
        [
            shell(ShellIntent::Resize(Panel::Sidebar, 354.0)),
            shell(ShellIntent::Resize(Panel::Sidebar, 380.0)),
            shell(ShellIntent::Resized(Panel::Sidebar, 380.0)),
        ]
    );
}

#[test]
fn the_sidebar_keeps_its_least_width() {
    let vm = samples::server_home();
    let mut ui = simulate(&vm);
    press(&mut ui, Point::new(SIDEBAR_EDGE - 2.0, 300.0));
    move_to(&mut ui, Point::new(20.0, 300.0));
    release(&mut ui);
    assert_eq!(
        asked(ui),
        [
            shell(ShellIntent::Resize(Panel::Sidebar, 180.0)),
            shell(ShellIntent::Resized(Panel::Sidebar, 180.0)),
        ]
    );
}

#[test]
fn going_behind_another_window_lets_a_dragged_edge_go() {
    let vm = samples::dm_home();
    let mut ui = simulate(&vm);
    press(&mut ui, Point::new(SIDEBAR_EDGE, 400.0));
    move_to(&mut ui, Point::new(SIDEBAR_EDGE + 50.0, 400.0));
    ui.simulate([Event::Window(iced::window::Event::Unfocused)]);
    // No release may come: the pointer going on drags nothing.
    move_to(&mut ui, Point::new(SIDEBAR_EDGE + 400.0, 400.0));
    assert_eq!(
        asked(ui),
        [
            shell(ShellIntent::Resize(Panel::Sidebar, 354.0)),
            shell(ShellIntent::Resized(Panel::Sidebar, 354.0)),
        ]
    );
}

#[test]
fn a_double_click_on_an_edge_resets_it_and_its_release_asks_nothing() {
    let vm = samples::dm_home();
    let mut ui = simulate(&vm);
    ui.point_at(Point::new(SIDEBAR_EDGE + 1.0, 500.0));
    ui.simulate(iced_test::simulator::click());
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [shell(ShellIntent::ResetSize(Panel::Sidebar))]);
}

#[test]
fn a_click_on_an_edge_that_drags_nothing_leaves_its_size_unchosen() {
    let vm = samples::server_home();
    let mut ui = simulate(&vm);
    ui.point_at(Point::new(1400.0 - 265.0, 400.0));
    ui.simulate(iced_test::simulator::click());
    let intents = asked(ui);
    assert!(intents.is_empty(), "{intents:?}");
}

#[test]
fn dragging_the_member_lists_edge_widens_it_leftwards() {
    let vm = samples::server_home();
    let mut ui = simulate(&vm);
    // Its left edge: the window less its 265 px.
    let edge = 1400.0 - 265.0;
    press(&mut ui, Point::new(edge, 400.0));
    move_to(&mut ui, Point::new(edge - 50.0, 400.0));
    release(&mut ui);
    assert_eq!(
        asked(ui),
        [
            shell(ShellIntent::Resize(Panel::Members, 315.0)),
            shell(ShellIntent::Resized(Panel::Members, 315.0)),
        ]
    );
}

#[test]
fn dragging_a_dm_calls_foot_makes_it_taller() {
    let mut vm = samples::dm_with(conversation::dm_call_here);
    // A call dragged before to 300 px, whatever the composer under it
    // leaves; how it shares the room undragged is `call`'s own test.
    vm.shell.call_height = Some(300.0);
    let mut ui = simulate(&vm);
    // Under the 50 px header, 3 px into its 6 px foot.
    let foot = 50.0 + 300.0 - 3.0;
    press(&mut ui, Point::new(700.0, foot));
    move_to(&mut ui, Point::new(700.0, foot + 50.0));
    release(&mut ui);
    assert_eq!(
        asked(ui),
        [
            shell(ShellIntent::Resize(Panel::Call, 350.0)),
            shell(ShellIntent::Resized(Panel::Call, 350.0)),
        ]
    );
}

#[test]
fn in_a_call_its_line_sits_over_the_stage() {
    let vm = samples::channel_with(conversation::voice);
    let mut ui = simulate(&vm);
    let line = ui
        .find(vm.conversation.description.as_str())
        .expect("the call's line")
        .visible_bounds()
        .expect("shown");
    // 16 px under the 50 px header.
    assert_eq!(line.y, 50.0 + 16.0);
}

#[test]
fn escape_closes_the_top_layer_only() {
    let vm = samples::open(samples::new_dm(), Layer::Switcher);
    let mut ui = simulate(&vm);
    ui.tap_key(Named::Escape);
    assert_eq!(asked(ui), [shell(ShellIntent::Close(Layer::Switcher))]);
}

#[test]
fn a_press_outside_a_dialog_closes_it_and_reaches_nothing_under_it() {
    let vm = samples::new_dm();
    let mut ui = simulate(&vm);
    ui.click("Person 4")
        .expect("Person 4's row, under the dialog");
    assert_eq!(asked(ui), [shell(ShellIntent::Close(Layer::NewDm))]);
}

/// A participant's card and the menu's rows are two layers, the rows' on
/// top: a press on the card's slider is the slider's, and closes neither.
#[test]
fn a_press_on_a_participants_volume_is_its_sliders() {
    use anomi_vm::fixtures::voice::participant_menu as fixture;
    use anomi_vm::voice::participant_menu::{NAME_HEIGHT, PADDING, SLIDER_HEIGHT, SPACING};

    let vm = fixture::carol();
    let card = (vm.shell.layers.iter())
        .find(|opened| opened.layer == Layer::ParticipantMenu)
        .and_then(|opened| opened.at)
        .expect("the card's place")
        .anchor
        .position();
    // On the slider's track, in the lower part of its row.
    let track = Point::new(
        card.x + PADDING + 60.0,
        card.y + PADDING + NAME_HEIGHT + SPACING + SLIDER_HEIGHT - 8.0,
    );
    let mut ui = simulate(&vm);
    press(&mut ui, track);
    move_to(&mut ui, Point::new(track.x + 30.0, track.y));
    release(&mut ui);
    let said: Vec<String> = (asked(ui).iter())
        .map(|intent| format!("{intent:?}"))
        .collect();
    assert!(
        said.iter().any(|intent| intent.contains("Volume")),
        "the slider says its volume: {said:?}"
    );
    assert!(
        !said.iter().any(|intent| intent.contains("Close")),
        "nothing closes: {said:?}"
    );
}

/// A press on neither the card nor the rows closes the menu, and the card
/// goes with it.
#[test]
fn a_press_beside_a_participants_card_closes_it() {
    use anomi_vm::fixtures::voice::participant_menu as fixture;

    let vm = fixture::carol();
    let mut ui = simulate(&vm);
    press(&mut ui, Point::new(20.0, 20.0));
    release(&mut ui);
    assert_eq!(asked(ui), [Intent::Menu(MenuIntent::Close)]);
}

#[test]
fn a_closed_layer_says_when_its_way_out_has_ended() {
    let mut vm = samples::new_dm();
    vm.shell.close(Layer::NewDm);
    // Held still, its way out ends at the first frame.
    let window = anomi_screens::window(&vm, Motion::default());
    let mut ui = Simulator::with_size(settings(), SIZE, window);
    ui.draw(&Theme::default());
    // Drawn, the rail and the DM list also tell which rows are in view.
    let asked: Vec<Intent> = asked(ui)
        .into_iter()
        .filter(|intent| {
            !matches!(
                intent,
                Intent::Rail(anomi_vm::rail::Intent::Seen(_))
                    | Intent::Sidebar(anomi_vm::sidebar::Intent::Dms(
                        anomi_vm::sidebar::dms::Intent::Seen(_)
                    ))
            )
        })
        .collect();
    assert_eq!(asked, [shell(ShellIntent::Gone(Layer::NewDm))]);
}

#[test]
fn a_popover_closes_on_a_press_outside_it() {
    let vm = samples::pins();
    let mut ui = simulate(&vm);
    ui.point_at(Point::new(200.0, 600.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [shell(ShellIntent::Close(Layer::Pins))]);
}

#[test]
fn got_it_dismisses_a_refused_drop() {
    let vm = samples::drop_refused();
    let mut ui = simulate(&vm);
    // Got It draws its own words: it sits under the help, 8 px below it and
    // 8 px into its row, 36 px tall.
    let help = ui
        .find("Max file size is 20.00 MB please.")
        .expect("the help")
        .visible_bounds()
        .expect("shown");
    ui.point_at(Point::new(
        help.center_x(),
        help.y + help.height + 16.0 + 18.0,
    ));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [shell(ShellIntent::DismissDrop)]);
}

#[test]
fn a_click_anywhere_dismisses_a_refused_drop() {
    let vm = samples::drop_refused();
    let mut ui = simulate(&vm);
    ui.point_at(Point::new(100.0, 100.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [shell(ShellIntent::DismissDrop)]);
}

#[test]
fn a_drop_refused_over_the_thread_beside_stays_over_it() {
    let vm = samples::drop_refused_beside();
    let mut ui = simulate(&vm);
    let reason = ui
        .find("Your files are too powerful")
        .expect("the reason")
        .visible_bounds()
        .expect("shown");
    // In the middle of the thread's 420 px at the window's right.
    assert_eq!(reason.center_x().round(), 1400.0 - 420.0 / 2.0);
}

#[test]
fn a_popup_over_the_bar_takes_the_press_there() {
    // A short window: the message's menu flips above the pointer and
    // stops 8 px from the top, over the 32 px bar.
    let vm = samples::popup_over_bar();
    let window = anomi_screens::window(&vm, super::support::front());
    let mut ui = Simulator::with_size(settings(), Size::new(1000.0, 640.0), window);
    let row = ui
        .find("Add reaction")
        .expect("the menu's first row")
        .visible_bounds()
        .expect("shown");
    assert!(row.y < 32.0, "{row:?}");
    ui.point_at(Point::new(row.center_x(), row.y + 1.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(
        asked(ui),
        [
            Intent::from(Choice::new("d:m1", "react")),
            Intent::Menu(MenuIntent::Close)
        ]
    );
}

#[test]
fn the_notice_dismisses() {
    let vm = samples::notice();
    let mut ui = simulate(&vm);
    // The banner's button draws its own words: it's found by its id
    // (`anomi_widgets::cover::banner`).
    ui.click(iced_test::selector::id("banner-dismiss"))
        .expect("the notice's Dismiss");
    assert_eq!(asked(ui), [shell(ShellIntent::DismissNotice)]);
}

#[test]
fn the_zoom_note_takes_nothing_from_the_window() {
    let vm = samples::zoomed();
    let mut ui = simulate(&vm);
    assert!(ui.find("110%").is_ok());
    ui.click("general").expect("#general's row");
    assert_eq!(asked(ui).len(), 1);
}

#[test]
fn a_forum_posts_bar_follows_reacts_and_copies() {
    let vm: Vm = samples::channel_with(conversation::forum_post);
    let mut ui = simulate(&vm);
    ui.click("Following").expect("Following");
    ui.click("React to post").expect("React to post");
    // Copy link, an icon 16 px in from the conversation's right, beside the
    // member list: 1400 - 265 - 16 - 16, in the 44 px bar under the header.
    ui.point_at(Point::new(1103.0, 72.0));
    ui.simulate(iced_test::simulator::click());
    let intents = asked(ui);
    assert_eq!(intents[0], shell(ShellIntent::FollowPost(false)));
    // The pill 7 px down the bar under the 50 px header, on whole pixels.
    assert!(
        matches!(intents[1], Intent::Shell(ShellIntent::ReactToPost(bounds)) if bounds.y == 57.0 && bounds.height == 31.0 && bounds.x > 376.0),
        "{intents:?}"
    );
    assert_eq!(intents[2..], [shell(ShellIntent::CopyPostLink)]);
}

#[test]
fn signed_out_the_window_shows_no_lists() {
    let vm = samples::signed_out();
    let mut ui = simulate(&vm);
    assert!(ui.find("Find a conversation").is_err());
}

#[test]
fn the_lists_take_clicks_beside_a_docked_panel() {
    let vm = samples::dragged();
    let mut ui = simulate(&vm);
    ui.click("general").expect("#general's row");
    assert_eq!(asked(ui).len(), 1);
}
