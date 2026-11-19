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

/// While live, a frame rate, a resolution or the higher quality chosen in
/// the share menu changes the stream at once and the menu stays open;
/// before, it waits for Go Live.
#[test]
fn the_streams_quality_changes_while_live() {
    use anomi_vm::voice::share::{self, Resolution, Share};

    let (mut app, folder) = in_call("quality");
    // The preview applies a voice command to the state as it's dispatched.
    app.core.state.preview = true;
    app.vm.voice.share = anomi_voice::share::built(Share::default());
    drop(handle(&mut app, Intent::Share(share::Intent::Fps(60))));
    assert_eq!(app.vm.voice.share.fps, 60);
    assert_eq!(
        app.core.state.voice.quality.fps, 30,
        "not before going live"
    );
    app.vm.voice.share = anomi_voice::share::built(Share {
        switching: true,
        ..Share::default()
    });
    app.vm.shell.open_at(Layer::Share, Spot::default());
    drop(handle(&mut app, Intent::Share(share::Intent::Fps(60))));
    drop(handle(
        &mut app,
        Intent::Share(share::Intent::Resolution(Resolution::P720)),
    ));
    drop(handle(
        &mut app,
        Intent::Share(share::Intent::HighQuality(true)),
    ));
    let quality = app.core.state.voice.quality;
    assert_eq!(quality.fps, 60);
    assert_eq!(quality.resolution, yapfast_core::voice::Resolution::P720);
    assert!(quality.high_quality);
    assert!(app.vm.shell.is_open(Layer::Share), "the menu stays open");
    let share = &app.vm.voice.share;
    assert!(
        share
            .rows
            .rates
            .iter()
            .any(|row| row.checked && row.intent == Some(share::Intent::Fps(60)))
    );
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

/// While live the menu opens with nothing picked, so its Switch button
/// waits for a pick, as Slint's did, and the stream's quality checked.
#[test]
fn the_share_menu_picks_nothing_while_live() {
    use yapfast_core::stream_worker::{Shareable, Source};
    use yapfast_core::voice::{SharePhase, Sharing};

    let (mut app, folder) = in_call("switch");
    let frames = yapfast_core::frames::Frames::create(4, 2).expect("frames");
    app.core.state.voice.sharing = Some(Sharing {
        phase: SharePhase::Live,
        frames: std::sync::Arc::new(frames),
        viewers: Vec::new(),
    });
    app.core.state.voice.quality = yapfast_core::voice::StreamQuality {
        resolution: yapfast_core::voice::Resolution::P720,
        fps: 15,
        high_quality: true,
    };
    drop(super::share::open(&mut app, Spot::default()));
    let screen = Shareable {
        source: Source::Display(7),
        title: "Screen 1".into(),
    };
    let listed = super::share::Listed(1, crate::message::Once::new(vec![screen]));
    drop(super::share::listed(&mut app, listed));
    let share = &app.vm.voice.share;
    assert!(share.switching && share.source.is_none() && !share.can_go());
    let checked = |rows: &[anomi_vm::menu::Row<_>]| -> Vec<String> {
        let checked = rows.iter().filter(|row| row.checked);
        checked.map(|row| row.text.clone()).collect()
    };
    assert_eq!(checked(&share.rows.resolutions), ["720p"]);
    assert_eq!(checked(&share.rows.rates), ["15 fps"]);
    assert!(share.high_quality);
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}
