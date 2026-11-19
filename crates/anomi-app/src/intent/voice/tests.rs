//! The call's intents: a tile shown large and the grid again, hiding the
//! others, the corner let go, the device menus opening and closing with the
//! account card's carets, and the participant card's volume.

use anomi_vm::menu::Spot;
use anomi_vm::settings::voice::Profile;
use anomi_vm::shell::Layer;
use anomi_vm::voice::devices::{self as menu, Kind, Part};
use anomi_vm::voice::{Intent, participant_menu, pip, stage, tiles};
use yapfast_core::noise::{Strength, Suppressor};

use super::handle;
use anomi_vm::menu::Key;
use menu::Intent::Hover;

/// A device menu's intent.
fn devices(intent: menu::Intent) -> Intent {
    Intent::Devices(intent)
}

/// A call with Discord linked and two speakers listed.
fn linked(name: &str) -> (crate::app::App, std::path::PathBuf) {
    let (mut app, folder) = in_call(name);
    app.core.state.discord_linked = true;
    let speaker = |id: &str| yapfast_core::voice::Device {
        id: id.into(),
        name: id.into(),
    };
    app.core.state.voice.speakers = std::sync::Arc::from([speaker("one"), speaker("two")]);
    (app, folder)
}
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

/// Bob's stream is offered in his tile, shown large over the window: the
/// core hears his preview is in sight, and one wait is set for when his
/// client uploads the next. Off screen, behind another window, in the tray
/// or while he's watched it isn't in sight, and no wait runs.
#[test]
fn a_preview_in_sight_waits_for_the_next_upload_and_none_behind() {
    use yapfast_core::application::STREAM_PREVIEW_EVERY_MS as EVERY;
    use yapfast_core::chat::Event;
    let (mut app, folder) = in_call("previews");
    let mut voice = app.core.state.voice.clone();
    voice.room = Some(yapfast_core::discord::room_id("10", "11"));
    let mut people = voice.participants.to_vec();
    people[1].streaming = true;
    voice.participants = people.into();
    // Bob is live as the call connects: the core asks for his preview.
    let asked = yapfast_core::clock::now_ms();
    drop(app.core.apply(Event::Voice(voice)));
    app.vm.chrome.focused = true;
    app.motion.front = true;
    let after = |app: &mut crate::app::App| {
        drop(super::after(app));
        let previews = &app.shown.voice.previews;
        (previews.in_sight.clone(), previews.waiting())
    };
    app.show(crate::project::Reach::VOICE);
    assert_eq!(after(&mut app), (vec![], None), "the call is off screen");
    let bob = tiles::Key::Person("bob".into());
    drop(handle(&mut app, Intent::Tiles(tiles::Intent::Focus(bob))));
    let full = Intent::Tiles(tiles::Intent::FullScreen(true));
    drop(handle(&mut app, full));
    let placed = stage::Placement {
        focused: Some(1),
        focus_width: 800.0,
        focus_height: 450.0,
        ..Default::default()
    };
    drop(handle(
        &mut app,
        Intent::Stage(stage::Intent::Placed(placed)),
    ));
    let (seen, due) = after(&mut app);
    assert_eq!(seen, ["bob"]);
    let due = due.expect("a wait for his next upload");
    assert_eq!(app.core.stream_previews_due(), Some(due));
    assert!((asked + EVERY..=yapfast_core::clock::now_ms() + EVERY).contains(&due));

    // Behind another window nothing is in sight and no wait runs; in front
    // again, the same wait.
    drop(crate::window::focused(&mut app, false));
    assert_eq!(after(&mut app), (vec![], None));
    assert_eq!(app.core.stream_previews_due(), None);
    drop(crate::window::focused(&mut app, true));
    assert_eq!(after(&mut app), (vec!["bob".into()], Some(due)));
    // A wait that ends finds it set again since: it does nothing.
    drop(super::previews::ended(&mut app, due - 1));
    assert_eq!(after(&mut app).1, Some(due));
    // In the tray, the same.
    let shown = &mut app.shown;
    crate::project::voice::hide(&app.core, &mut app.vm, &mut shown.voice, &mut shown.media);
    assert_eq!(after(&mut app), (vec![], None));
    // Watched, his tile shows his stream: no preview, no wait.
    app.show(crate::project::Reach::VOICE);
    assert_eq!(after(&mut app).1, Some(due));
    app.core.state.voice.watching = Some(Box::new(yapfast_core::voice::Watching {
        user: "bob".into(),
        name: "Bob".into(),
        opening: true,
        frames: None,
        viewers: vec![],
    }));
    app.show(crate::project::Reach::VOICE);
    assert_eq!(after(&mut app), (vec![], None));
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_caret_opens_its_device_menu_and_closing_it_puts_the_caret_back() {
    let (mut app, folder) = linked("devices");
    drop(super::open_devices(&mut app, Kind::Input, Spot::default()));
    assert!(app.vm.shell.is_open(Layer::Devices) && app.vm.account.input_open);
    assert!(app.vm.voice.devices.input());
    assert!(app.vm.voice.devices.profile.is_some(), "Discord is linked");
    drop(handle(&mut app, devices(Hover(Some(Part::Device)))));
    assert_eq!(app.vm.voice.devices.open, Some(Part::Device));
    drop(handle(&mut app, devices(Hover(None))));
    assert_eq!(app.vm.voice.devices.open, None, "the volume closes it");
    drop(handle(&mut app, devices(menu::Intent::Volume(150))));
    drop(handle(&mut app, devices(menu::Intent::Commit(150))));
    assert_eq!(app.vm.voice.devices.volume, 150);
    assert_eq!(app.layout.microphone_volume, Some(150));
    drop(super::close(&mut app, Layer::Devices));
    assert!(!app.vm.shell.is_open(Layer::Devices) && !app.vm.account.input_open);
    assert!(app.shown.voice.devices.is_none());
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn the_keys_choose_a_device_and_the_menu_closes() {
    let (mut app, folder) = linked("device-keys");
    drop(super::open_devices(&mut app, Kind::Output, Spot::default()));
    assert!(app.vm.voice.devices.profile.is_none(), "the output menu's");
    for key in [Key::Down, Key::Right, Key::Down] {
        drop(handle(&mut app, devices(menu::Intent::Key(key))));
    }
    let menu = &app.vm.voice.devices;
    assert_eq!((menu.open, menu.sub), (Some(Part::Device), Some(1)));
    assert_eq!(menu.device.submenu[1].text, "two");
    drop(handle(&mut app, devices(menu::Intent::Key(Key::Enter))));
    assert!(!app.vm.shell.is_open(Layer::Devices) && !app.vm.account.output_open);
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn an_input_profile_sets_the_switches_as_voice_settings_do() {
    let (mut app, folder) = linked("profile");
    drop(super::open_devices(&mut app, Kind::Input, Spot::default()));
    drop(handle(
        &mut app,
        devices(menu::Intent::Profile(Profile::Studio)),
    ));
    assert!(!app.vm.shell.is_open(Layer::Devices), "choosing closes it");
    let layout = &app.layout;
    assert_eq!(layout.level_voice, Some(false));
    assert_eq!(layout.noise_suppression, Suppressor::Off);
    assert!(!layout.reduce_room_echo);
    assert_eq!(app.vm.settings.voice.profile(), Profile::Studio);
    drop(super::open_devices(&mut app, Kind::Input, Spot::default()));
    let row = app
        .vm
        .voice
        .devices
        .profile
        .as_ref()
        .map(|row| &row.subtitle);
    assert_eq!(row.map(String::as_str), Some("Studio"));
    drop(handle(
        &mut app,
        devices(menu::Intent::Profile(Profile::Isolation)),
    ));
    let layout = &app.layout;
    assert_eq!(layout.level_voice, Some(true));
    assert_eq!(layout.noise_suppression, Suppressor::DeepFilterNet);
    assert_eq!(layout.noise_strength, Strength::Full);
    assert!(layout.reduce_room_echo);
    // A switch of its own makes it Custom, which opens Voice & video.
    drop(handle(
        &mut app,
        Intent::Devices(menu::Intent::Profile(Profile::Custom)),
    ));
    assert!(app.vm.shell.is_open(Layer::Settings));
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
