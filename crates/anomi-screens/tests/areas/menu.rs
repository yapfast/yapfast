//! The context menu open over the window, with its rows: task F06's tests,
//! driving the window over its fixtures in the simulator (`super::support`),
//! with `vm.menu` in the layer N01 gives it. The layer stack, the opener,
//! the hand-laid row and the tooltip have their own tests in
//! `anomi-widgets` (`layer/tests.rs`, `menu/tests.rs`, `tip/tests.rs`).

use iced::keyboard::key::Named;
use iced::mouse;
use iced::{Event, Point};

use anomi_theme::{Element, Motion};
use anomi_vm::fixtures::menu as samples;
use anomi_vm::menu::{Choice, Intent as MenuIntent};
use anomi_vm::{Intent, Vm};

use super::support::{SIZE, front, settings};

type Simulator<'a> = iced_test::Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

/// The window, its menu over it.
fn window(vm: &Vm, motion: Motion) -> Element<'_, Intent> {
    use iced::Widget as _;
    anomi_screens::window(vm, motion).boxed()
}

fn simulate(vm: &Vm) -> Simulator<'_> {
    Simulator::with_size(settings(), SIZE, window(vm, front()))
}

fn choose(target: &str, id: &str) -> Intent {
    Intent::from(Choice::new(target, id))
}

const CLOSE: Intent = Intent::Menu(MenuIntent::Close);

/// Moves the pointer onto a row by its text, as hovering does.
fn hover(ui: &mut Simulator<'_>, text: &str) {
    let row = ui.find(text).expect(text);
    let at = row.visible_bounds().expect("shown").center();
    ui.point_at(at);
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
}

#[test]
fn choosing_a_row_sends_its_intent_and_closes_the_menu() {
    let vm = samples::server_header();
    let mut ui = simulate(&vm);
    ui.click("Invite to server").expect("the invite row");
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [choose("header:discord:g1", "invite"), CLOSE]
    );
}

#[test]
fn a_box_to_tick_sends_its_intent_and_stays_open() {
    let vm = samples::server_header();
    let mut ui = simulate(&vm);
    ui.click("Hide muted channels").expect("the checkbox row");
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [choose("header:discord:g1", "hide-muted")]
    );
}

#[test]
fn a_row_with_a_submenu_opens_it_on_click_and_its_rows_choose() {
    let vm = samples::server_header();
    let mut ui = simulate(&vm);
    assert!(ui.find("Suppress all role @mentions").is_err());
    ui.click("Notification settings")
        .expect("the notifications row");
    assert_eq!(ui.drain().count(), 0);
    ui.click("Nothing").expect("a level in the submenu");
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [choose("header:discord:g1", "level-nothing"), CLOSE]
    );
}

#[test]
fn hovering_a_row_opens_its_submenu() {
    let vm = samples::message_context();
    let mut ui = simulate(&vm);
    hover(&mut ui, "Apps");
    ui.click("Quote").expect("the app command");
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [choose("d:m1", "app:quote"), CLOSE]
    );
}

#[test]
fn a_mute_sends_itself_when_clicked_and_offers_times_on_hover() {
    let vm = samples::notifications();
    let mut ui = simulate(&vm);
    hover(&mut ui, "Mute channel");
    assert!(ui.find("Until I turn it back on").is_ok());
    ui.click("Mute channel").expect("the mute row");
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [choose("notifications:discord:c1", "mute-always"), CLOSE]
    );
}

#[test]
fn the_keys_move_through_the_rows_and_choose() {
    let vm = samples::message_context();
    let mut ui = simulate(&vm);
    ui.tap_key(Named::ArrowDown);
    ui.tap_key(Named::ArrowDown);
    ui.tap_key(Named::Enter);
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [choose("d:m1", "reply"), CLOSE]
    );
}

#[test]
fn up_wraps_to_the_last_row() {
    let vm = samples::message_context();
    let mut ui = simulate(&vm);
    ui.tap_key(Named::ArrowUp);
    ui.tap_key(Named::Space);
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        [choose("d:m1", "copy-id"), CLOSE]
    );
}

#[test]
fn escape_closes_the_submenu_first_then_the_menu() {
    let vm = samples::message_context();
    let mut ui = simulate(&vm);
    ui.tap_key(Named::End);
    for _ in 0..3 {
        ui.tap_key(Named::ArrowUp);
    }
    // Apps: Right opens its submenu at its first row.
    ui.tap_key(Named::ArrowRight);
    assert!(ui.find("Quote").is_ok());
    ui.tap_key(Named::Escape);
    assert!(ui.find("Quote").is_err());
    assert_eq!(ui.drain().count(), 0);
    ui.tap_key(Named::Escape);
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), [CLOSE]);
}

#[test]
fn a_click_outside_closes_it_and_reaches_nothing_under_it() {
    let vm = samples::message_context();
    let mut ui = simulate(&vm);
    // #general's row in the channel list, under the clear backdrop.
    ui.point_at(Point::new(150.0, 166.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(ui.into_messages().collect::<Vec<_>>(), [CLOSE]);
}

#[test]
fn a_disabled_row_sends_nothing() {
    let mut vm = samples::server_header();
    vm.menu.rows[0].disabled = true;
    let mut ui = simulate(&vm);
    ui.click("Invite to server").expect("the invite row");
    assert_eq!(ui.into_messages().count(), 0);
}

#[test]
fn a_closing_menu_takes_no_clicks_and_says_when_it_has_gone() {
    let mut vm = samples::server_header();
    vm.menu.open = false;
    // Held still, its way out ends at the first frame.
    let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, Motion::default()));
    assert!(ui.click("Invite to server").is_ok());
    assert!(
        !ui.drain()
            .any(|intent| intent == choose("header:discord:g1", "invite"))
    );
    ui.draw(&anomi_theme::Theme::default());
    // The window's other parts tell what they show as they are drawn (the
    // member list its rows): the menu's own word is the one asked of it.
    assert_eq!(
        ui.into_messages()
            .filter(|intent| matches!(intent, Intent::Menu(_)))
            .collect::<Vec<_>>(),
        [Intent::Menu(MenuIntent::Gone)]
    );
}

/// Draws the golden shots' menus over the window, at 2x, into the folder
/// `ANOMI_MENU_SHOTS` names, for a reviewer to hold beside
/// `~/.cache/anomi-iced/golden/menu/`: `ANOMI_MENU_SHOTS=dir cargo test -p
/// anomi-screens --test areas draws_the_golden_menus -- --ignored`. F06
/// owns no screen module, so no `shots()` of its own: the window draws
/// `vm.menu`, and a shots list naming `fixtures::menu`'s samples draws
/// them through `anomi-shots`, without the hovered submenus.
#[test]
#[ignore = "writes pictures for a reviewer"]
fn draws_the_golden_menus() {
    let Ok(dir) = std::env::var("ANOMI_MENU_SHOTS") else {
        return;
    };
    let shots = [
        ("server_header", samples::server_header(), None, "bonfire"),
        ("notifications", samples::notifications(), None, "bonfire"),
        (
            "message_context",
            samples::message_context(),
            None,
            "bonfire",
        ),
        (
            "apps_submenu",
            samples::message_context(),
            Some("Apps"),
            "bonfire",
        ),
        (
            "server_header_latte",
            samples::server_header(),
            Some("Notification settings"),
            "catppuccin-latte",
        ),
    ];
    for (name, vm, hovered, theme) in shots {
        // Held still, as anomi-shots draws: everything has arrived.
        let mut ui = Simulator::with_size(settings(), SIZE, window(&vm, Motion::default()));
        if let Some(text) = hovered {
            hover(&mut ui, text);
        }
        let theme = anomi_theme::Theme::built_in(Some(theme));
        let path = format!("{dir}/{name}.png");
        // The simulator names the file after its renderer.
        let _ = std::fs::remove_file(format!("{dir}/{name}-tiny-skia.png"));
        let snapshot = ui.snapshot(&theme).expect("a snapshot");
        assert!(snapshot.matches_image(&path).expect("written"));
    }
}
