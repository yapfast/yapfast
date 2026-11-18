//! The call's intents: a tile shown large and the grid again, hiding the
//! others, the corner let go, the device menus opening and closing with the
//! account card's carets, and the participant card's volume.

use anomi_vm::menu::Spot;
use anomi_vm::shell::Layer;
use anomi_vm::voice::devices::{self as menu, Kind};
use anomi_vm::voice::{Intent, participant_menu, pip, stage, tiles};

use super::handle;
use crate::project::voice::tests::in_call;

#[test]
fn a_tile_shows_large_and_a_click_beside_it_shows_them_all() {
    let (mut app, folder) = in_call("focus");
    let bob = tiles::Key::Person("bob".into());
    drop(handle(&mut app, Intent::Tiles(tiles::Intent::Focus(bob))));
    assert_eq!(app.vm.voice.stage.focused(), Some(1));
    drop(handle(
        &mut app,
        Intent::Tiles(tiles::Intent::HideOthers(true)),
    ));
    assert!(app.vm.voice.stage.others_hidden());
    drop(handle(&mut app, Intent::Stage(stage::Intent::ShowAll)));
    assert_eq!(app.vm.voice.stage.focused(), None);
    assert!(!app.vm.voice.stage.others_hidden(), "ends with it");
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn full_screen_hides_the_bar_until_escape() {
    let (mut app, folder) = in_call("full");
    let bob = tiles::Key::Person("bob".into());
    drop(handle(&mut app, Intent::Tiles(tiles::Intent::Focus(bob))));
    drop(handle(
        &mut app,
        Intent::Tiles(tiles::Intent::FullScreen(true)),
    ));
    assert!(app.vm.shell.is_open(Layer::FullStage) && app.vm.chrome.full_screen);
    assert!(app.vm.voice.stage.full());
    drop(super::close(&mut app, Layer::FullStage));
    assert!(!app.vm.shell.is_open(Layer::FullStage) && !app.vm.chrome.full_screen);
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn the_corner_goes_where_it_was_let_go() {
    let (mut app, folder) = in_call("corner");
    app.size = iced::Size::new(1400.0, 860.0);
    assert!(!app.vm.shell.pip.left && !app.vm.shell.pip.bottom);
    let far = iced::Vector::new(-900.0, 500.0);
    drop(handle(&mut app, Intent::Pip(pip::Intent::Drag(far))));
    assert_eq!(app.vm.voice.pip.drag, far);
    drop(handle(&mut app, Intent::Pip(pip::Intent::Dropped)));
    assert!(app.vm.shell.pip.left && app.vm.shell.pip.bottom);
    assert_eq!(app.vm.voice.pip.drag, iced::Vector::ZERO);
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_caret_opens_its_device_menu_and_closing_it_puts_the_caret_back() {
    let (mut app, folder) = in_call("devices");
    drop(super::open_devices(&mut app, Kind::Input, Spot::default()));
    assert!(app.vm.shell.is_open(Layer::Devices) && app.vm.account.input_open);
    assert!(app.vm.voice.devices.input());
    assert!(app.vm.voice.devices.noise.is_some(), "a Discord call's");
    drop(handle(&mut app, Intent::Devices(menu::Intent::ToggleList)));
    assert!(app.vm.voice.devices.listing);
    drop(handle(&mut app, Intent::Devices(menu::Intent::Volume(150))));
    drop(handle(&mut app, Intent::Devices(menu::Intent::Commit(150))));
    assert_eq!(app.vm.voice.devices.volume, 150);
    assert_eq!(app.layout.microphone_volume, Some(150));
    drop(super::close(&mut app, Layer::Devices));
    assert!(!app.vm.shell.is_open(Layer::Devices) && !app.vm.account.input_open);
    assert!(app.shown.voice.meter.is_none());
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_participants_volume_is_heard_at_once_and_kept_when_let_go() {
    let (mut app, folder) = in_call("volume");
    let volume = |volume, last| {
        Intent::ParticipantMenu(participant_menu::Intent::Volume {
            person: "bob".into(),
            stream: false,
            volume,
            last,
        })
    };
    drop(handle(&mut app, volume(50, false)));
    assert_eq!(app.vm.voice.participant_menu.voice_volume, 50);
    assert!(
        !app.layout.volumes.contains_key("bob"),
        "not while dragging"
    );
    drop(handle(&mut app, volume(50, true)));
    assert!(app.layout.volumes.contains_key("bob"));
    drop(handle(&mut app, volume(100, true)));
    assert!(
        !app.layout.volumes.contains_key("bob"),
        "the default isn't kept"
    );
    yapfast_core::volume::set(yapfast_core::volume::Kind::Voice, "bob", 1.0);
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_microphone_that_cant_be_read_says_so_in_its_own_menu() {
    let (mut app, folder) = in_call("unread");
    drop(super::open_devices(&mut app, Kind::Input, Spot::default()));
    let other = anomi_vm::kind::Level::default();
    drop(super::due(&mut app, super::Due::Unread(other)));
    assert!(app.vm.voice.devices.note().is_none(), "another meter's");
    let level = app.vm.voice.devices.level.clone();
    drop(super::due(&mut app, super::Due::Unread(level)));
    assert!(app.vm.voice.devices.note().is_some());
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn the_screens_pictures_fill_their_cards_while_their_menu_shows() {
    use yapfast_core::stream_worker::{Shareable, Source};

    let (mut app, folder) = in_call("glance");
    let screen = |id: u64, title: &str| Shareable {
        source: Source::Display(id),
        title: title.into(),
    };
    drop(super::share::open(&mut app, Spot::default()));
    let listed = vec![screen(7, "Screen 1"), screen(9, "Screen 2")];
    let listed = super::share::Listed(1, crate::message::Once::new(listed));
    drop(super::share::listed(&mut app, listed));
    assert!(app.vm.shell.is_open(Layer::Share));
    let picture = iced::widget::image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]);
    let glanced = |asked, id| {
        super::Due::Glanced(super::glance::Glanced {
            asked,
            id,
            picture: picture.clone(),
        })
    };
    drop(super::due(&mut app, glanced(1, 9)));
    let screens = &app.vm.voice.share.screens;
    assert!(screens[0].picture.is_none() && screens[1].picture.is_some());
    drop(super::close(&mut app, Layer::Share));
    drop(super::due(&mut app, glanced(1, 7)));
    assert!(
        app.vm.voice.share.screens[0].picture.is_none(),
        "taken only while it shows"
    );
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}
