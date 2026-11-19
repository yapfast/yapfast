//! The window going behind another, on the real core: what hangs off it
//! (the window's menu and its submenu, a header's popover, a device menu
//! with its submenu) closes through its own close, so the core lets go what
//! it showed, and a dialog with text typed in it stays.

use std::sync::Arc;

use yapfast_core::application::{Action, NewDm};
use yapfast_core::network::Network;

use anomi_vm::header::Intent as Header;
use anomi_vm::menu::{Menu, Row, Spot};
use anomi_vm::shell::Layer;
use anomi_vm::voice::devices::{Intent as Devices, Kind, Part};

use super::start;
use crate::app::App;
use crate::message::Message;
use crate::project::header::pins_open;

fn ask(app: &mut App, intent: impl Into<anomi_vm::Intent>) {
    drop(crate::intent::handle(app, intent.into()));
}

/// The window goes behind another, as its `Unfocused` says.
fn behind(app: &mut App) {
    drop(app.update(Message::Focused(false)));
}

#[test]
fn the_windows_menu_and_a_popover_close_and_the_core_lets_go() {
    let mut window = start("unfocus-menus", true);
    let app = &mut window.app;
    // Sam's DM with Discord's abilities: its pins open under their button.
    for room in app.core.state.rooms.iter_mut() {
        if room.id == "sam" {
            let room = Arc::make_mut(room);
            room.capabilities = Network::Discord.capabilities();
            room.person = Some("sam".into());
        }
    }
    app.core.state.list_changes += 1;
    app.core.state.discord_online = true;
    drop(app.dispatch(Action::Open("sam".into())));
    let spot = Spot::pointer(iced::Point::new(900.0, 40.0));
    ask(app, Header::Pins(spot));
    assert!(pins_open(&app.core.state) && app.vm.shell.is_open(Layer::Pins));
    // The window's menu over it, its submenu showing.
    let close = anomi_vm::Intent::from(anomi_vm::menu::Intent::Close);
    let mute = Row::new("For 15 Minutes", close);
    let rows = vec![Row::opens("Mute", vec![mute])];
    let menu = Menu {
        submenu: Some(0),
        ..Menu::new(0, spot, rows)
    };
    crate::intent::menus::show(app, menu);
    assert!(app.vm.menu.open);

    behind(app);
    assert!(!app.vm.menu.open, "the menu plays its way out");
    assert!(!app.vm.shell.is_open(Layer::Pins));
    assert!(!pins_open(&app.core.state), "the core let the pins go");
    // Back in front, nothing opens again by itself.
    drop(app.update(Message::Focused(true)));
    assert!(!app.vm.menu.open && !app.vm.shell.is_open(Layer::Pins));
}

#[test]
fn a_dialog_with_text_typed_in_it_stays() {
    let mut window = start("unfocus-dialog", true);
    let app = &mut window.app;
    app.core.state.new_dm = Some(NewDm::default());
    app.refresh();
    ask(app, anomi_vm::new_dm::Intent::Query("bo".into()));
    // The topic in full hangs over the window meanwhile.
    app.vm.shell.open_at(Layer::Topic, Spot::default());

    behind(app);
    assert!(!app.vm.shell.is_open(Layer::Topic));
    assert!(app.vm.shell.is_open(Layer::NewDm));
    let dialog = app.core.state.new_dm.as_ref().expect("still open");
    assert_eq!(dialog.query, "bo");
    assert_eq!(app.vm.new_dm.query, "bo");
}

#[test]
fn a_device_menu_closes_with_its_submenu() {
    let (mut app, folder) = crate::project::voice::tests::in_call("unfocus-devices");
    drop(app.update(Message::Focused(true)));
    drop(crate::intent::voice::open_devices(
        &mut app,
        Kind::Input,
        Spot::default(),
    ));
    let device = anomi_vm::voice::Intent::Devices(Devices::Hover(Some(Part::Device)));
    ask(&mut app, device);
    assert_eq!(app.vm.voice.devices.open, Some(Part::Device), "its submenu");

    behind(&mut app);
    assert!(!app.vm.shell.is_open(Layer::Devices));
    assert!(!app.vm.account.input_open, "the caret is put back");
    assert!(app.shown.voice.devices.is_none());
    // In front again, the caret opens it with no submenu.
    drop(app.update(Message::Focused(true)));
    drop(crate::intent::voice::open_devices(
        &mut app,
        Kind::Input,
        Spot::default(),
    ));
    assert_eq!(app.vm.voice.devices.open, None);
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}
