//! The message menu: task C12's tests, driving `anomi_chat::menu::view` over
//! its fixtures in the simulator (`super::support`).

use iced::Size;
use iced_test::Simulator;

use anomi_chat::menu::{build, view};
use anomi_vm::Vm;
use anomi_vm::chat::menu::{Intent, Menu};
use anomi_vm::fixtures::chat::menu as sample;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(700.0, 700.0);

fn menu_of(vm: fn() -> Vm) -> Menu {
    let mut menu = vm().chat.menu;
    build(&mut menu);
    menu
}

fn texts(menu: &Menu) -> Vec<&str> {
    menu.rows.iter().map(|row| row.text.as_str()).collect()
}

fn simulate(menu: &Menu) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(menu, front()))
}

fn click(menu: &Menu, text: &str) -> Vec<Intent> {
    let mut ui = simulate(menu);
    ui.click(text).unwrap_or_else(|_| panic!("a row {text}"));
    asked(ui)
}

#[test]
fn the_rows_follow_discords_order() {
    let menu = menu_of(sample::message_context);
    assert_eq!(
        texts(&menu),
        [
            "Add reaction",
            "Reply",
            "Forward",
            "Create Thread",
            "Copy text",
            "Pin message",
            "Apps",
            "Mark Unread",
            "Copy Message Link",
            "Copy Message ID"
        ]
    );
}

#[test]
fn the_id_has_a_line_over_it_and_delete_is_red() {
    let menu = menu_of(sample::own);
    let id = menu.rows.last().unwrap();
    assert!(id.separated && !id.danger);
    let delete = menu.rows.iter().find(|row| row.text == "Delete message");
    assert!(delete.is_some_and(|row| row.danger && row.separated));
    assert_eq!(texts(&menu)[..2], ["Add reaction", "View Reactions"]);
    assert!(texts(&menu).contains(&"Edit message"));
}

#[test]
fn a_picture_offers_saving_and_its_link_under_a_line() {
    let menu = menu_of(sample::picture);
    let rows = texts(&menu);
    let at = rows.iter().position(|text| *text == "Copy Image").unwrap();
    assert!(menu.rows[at].separated);
    assert_eq!(rows[at..at + 3], ["Copy Image", "Save Image", "Copy Link"]);
    assert!(rows.contains(&"Open Link"));
    assert!(!rows.contains(&"Apps"));
}

#[test]
fn a_video_saves_as_a_video_and_is_not_copied_as_a_picture() {
    let menu = menu_of(sample::video);
    let rows = texts(&menu);
    assert!(rows.contains(&"Save Video") && !rows.contains(&"Copy Image"));
}

#[test]
fn a_message_on_its_way_has_no_pin_or_thread() {
    let menu = menu_of(sample::sending);
    let rows = texts(&menu);
    assert!(!rows.contains(&"Pin message") && !rows.contains(&"Create Thread"));
    assert!(rows.contains(&"Reply"));
}

#[test]
fn a_thread_poll_and_pin_read_as_their_opposites() {
    let menu = menu_of(sample::poll);
    let rows = texts(&menu);
    assert!(rows.contains(&"View Thread") && !rows.contains(&"Create Thread"));
    assert!(rows.contains(&"Unpin message") && rows.contains(&"End Poll Now"));
    assert!(rows.contains(&"Copy selection"));
}

#[test]
fn a_menu_without_the_composer_leaves_out_what_needs_it() {
    let mut menu = sample::own().chat.menu;
    menu.facts.composer_enabled = false;
    build(&mut menu);
    let rows = texts(&menu);
    assert!(!rows.contains(&"Reply") && !rows.contains(&"Edit message"));
    assert!(!rows.contains(&"Delete message"));
}

#[test]
fn choosing_a_row_asks_for_it() {
    let menu = menu_of(sample::message_context);
    assert_eq!(click(&menu, "Reply"), [Intent::Reply]);
    assert_eq!(click(&menu, "Forward"), [Intent::Forward]);
    assert_eq!(click(&menu, "Add reaction"), [Intent::AddReaction]);
    assert_eq!(click(&menu, "Pin message"), [Intent::Pin]);
    assert_eq!(click(&menu, "Create Thread"), [Intent::StartThread]);
    assert_eq!(click(&menu, "Mark Unread"), [Intent::MarkUnread]);
    assert_eq!(
        click(&menu, "Copy Message ID"),
        [Intent::Copy(menu.facts.id.clone())]
    );
}

#[test]
fn deleting_asks_for_it() {
    let menu = menu_of(sample::own);
    assert_eq!(click(&menu, "Delete message"), [Intent::Delete]);
}

#[test]
fn a_quick_reaction_reacts_by_its_key() {
    let menu = menu_of(sample::message_context);
    assert_eq!(menu.reactions.len(), 4);
    let mut ui = simulate(&menu);
    // The tiles are the first thing in the card: 6 px padding, 44 px tiles.
    let at = iced::Point::new(1.0 + 6.0 + 22.0, 6.0 + 22.0);
    ui.point_at(at);
    // The pointer arriving on a tile leaves the Apps submenu.
    ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: at,
    })]);
    assert_eq!(ui.drain().collect::<Vec<_>>(), [Intent::Apps(false)]);
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [Intent::React("👍".into())]);
}

#[test]
fn the_pointer_on_apps_opens_its_submenu_and_a_command_runs() {
    let mut menu = menu_of(sample::message_context);
    // Closed, the submenu's commands are not drawn.
    assert!(simulate(&menu).find("Quote").is_err());
    let mut ui = simulate(&menu);
    ui.click("Apps").expect("the Apps row");
    assert_eq!(asked(ui), [Intent::Apps(true)]);
    menu.apps_open = true;
    let mut ui = simulate(&menu);
    ui.click("Quote").expect("the app's command");
    assert_eq!(asked(ui), [Intent::RunApp("quote".into())]);
    // Another row closes it by being pointed at.
    let apps = menu.rows.iter().find(|row| row.text == "Apps").unwrap();
    assert_eq!(apps.submenu.len(), 1);
    assert!(apps.submenu[0].plain && apps.submenu[0].subtitle == "Status Bot");
}

#[test]
fn the_submenu_opening_on_the_left_moves_nothing() {
    let mut menu = menu_of(sample::message_context);
    menu.flip = true;
    let closed = simulate(&menu).find("Apps").expect("the Apps row").bounds();
    menu.apps_open = true;
    let mut ui = simulate(&menu);
    assert_eq!(ui.find("Apps").expect("the Apps row").bounds(), closed);
    // Left of the menu, level with its row.
    let quote = ui.find("Quote").expect("the app's command").bounds();
    assert!(quote.x + quote.width < closed.x);
    assert!((quote.y - closed.y).abs() < 8.0, "{quote:?} by {closed:?}");
}
