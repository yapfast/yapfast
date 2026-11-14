//! The call, as the Slint client's tests of it: the dock's noise toggle,
//! the device menus' volumes and a device gone away, the share menu in a
//! short window, the microphone's level only while its menu shows, the
//! dock's channel line, a voice channel's Join, and someone's volumes and
//! menu from their tile.

use iced::{Point, Rectangle, Size};
use yapfast_core::{
    application::{Action, Navigation},
    noise::Suppressor,
    stream_worker::{Shareable, Source},
    voice,
};

use super::{interact, last, preview, shows};
use crate::{
    app::App,
    call::{Event, Popup},
};

/// In the preview's call in Lounge, its channel open: a click on it joins,
/// as Discord's does, and a second shows it.
fn in_call() -> (App, tempfile::TempDir, String) {
    let (mut app, folder) = preview();
    let room = "lounge".to_owned();
    app.client.dispatch(Action::SelectRoom(room.clone()));
    let _ = app.sync();
    assert!(app.client.state().voice.active(), "the preview joins");
    assert_ne!(app.client.state().selected.as_deref(), Some("lounge"));
    app.client.dispatch(Action::SelectRoom(room.clone()));
    let _ = app.sync();
    (app, folder, room)
}

#[test]
fn the_docks_noise_toggle_turns_back_on_the_kind_last_used() {
    let (mut app, folder) = preview();
    let stored = || {
        crate::layout::Layout::read(folder.path())
            .voice
            .noise_suppression
    };
    // DeepFilterNet, as it comes, goes off and comes back.
    let _ = app.call_event(Event::Noise);
    assert_eq!(app.layout.voice.noise_suppression, Suppressor::Off);
    assert_eq!(stored(), Suppressor::Off, "saved");
    let _ = app.call_event(Event::Noise);
    assert_eq!(
        app.layout.voice.noise_suppression,
        Suppressor::DeepFilterNet
    );
    // RNNoise, picked in the input menu, is the one it brings back.
    let _ = app.call_event(Event::Suppressor(Suppressor::RNNoise));
    let _ = app.call_event(Event::Noise);
    assert_eq!(app.layout.voice.noise_suppression, Suppressor::Off);
    let _ = app.call_event(Event::Noise);
    assert_eq!(app.layout.voice.noise_suppression, Suppressor::RNNoise);
    assert_eq!(stored(), Suppressor::RNNoise);
    // Off picked there after RNNoise brings RNNoise back too.
    let _ = app.call_event(Event::Suppressor(Suppressor::Off));
    let _ = app.call_event(Event::Noise);
    assert_eq!(app.layout.voice.noise_suppression, Suppressor::RNNoise);
    let _ = app.call_event(Event::Suppressor(Suppressor::DeepFilterNet));
}

#[test]
fn a_chosen_device_that_went_away_shows_as_not_connected() {
    use crate::call::device_rows;
    let device = |id: &str, name: &str| voice::Device {
        id: id.into(),
        name: name.into(),
    };
    let both = [
        device("{speakers}", "Speakers"),
        device("{headphones}", "Headphones"),
    ];
    let (rows, index) = device_rows(&both, Some("{headphones}"));
    assert_eq!(rows, ["Speakers", "Headphones"]);
    assert_eq!(index, Some(1));
    // A device known by its name, as on PulseAudio, has it in its id.
    let (rows, index) = device_rows(&both[..1], Some("name:USB headset"));
    assert_eq!(rows, ["Speakers", "USB headset (not connected)"]);
    assert_eq!(index, Some(1));
    // The default entry's id names the default it had; it stays the default.
    let pulse = [
        device("name:default: Headset", "default: Headset"),
        device("name:Headset", "Headset"),
    ];
    let (rows, index) = device_rows(&pulse, Some("name:default: Built-in"));
    assert_eq!(rows, ["Default (Headset)", "Headset"]);
    assert_eq!(index, Some(0));
    assert_eq!(device_rows(&[], None), (vec![], None), "nothing to choose");
    assert_eq!(device_rows(&both, None).1, Some(0), "the default, first");
}

#[test]
fn call_volumes_play_while_dragged_and_are_remembered() {
    let (mut app, folder) = preview();
    let stored = || crate::layout::Layout::read(folder.path()).voice;
    let _ = app.call_event(Event::DeviceVolume(true, 200, false));
    let _ = app.call_event(Event::DeviceVolume(false, 50, false));
    assert!((yapfast_core::volume::microphone() - 1.995).abs() < 1e-3);
    assert!((yapfast_core::volume::speakers() - 0.143).abs() < 1e-3);
    assert_eq!(
        stored().microphone_volume,
        None,
        "a drag's steps aren't saved"
    );
    let _ = app.call_event(Event::DeviceVolume(true, 200, true));
    let _ = app.call_event(Event::DeviceVolume(false, 50, true));
    assert_eq!(stored().microphone_volume, Some(200));
    assert_eq!(stored().speaker_volume, Some(50));
    // A new window plays at the volumes kept, and the defaults at none.
    crate::layout::Voice::default().play_volumes();
    assert_eq!(yapfast_core::volume::microphone(), 1.0);
    stored().play_volumes();
    assert!((yapfast_core::volume::speakers() - 0.143).abs() < 1e-3);
    crate::layout::Voice::default().play_volumes();
}

#[test]
fn the_share_menu_fits_a_short_window_and_keeps_its_button() {
    use crate::call::{Listing, ShareEvent};
    let (mut app, _folder, _) = in_call();
    app.shell.frame = Size::new(800.0, 560.0);
    app.fit_window();
    // More than the window is tall: a screen, twelve windows, four cameras.
    let mut listed = vec![Shareable {
        source: Source::Display(1),
        title: "Entire screen".into(),
    }];
    listed.extend((0..12).map(|index| Shareable {
        source: Source::Window(index),
        title: format!("Window {index}"),
    }));
    listed.extend((0..4).map(|index| Shareable {
        source: Source::Camera(format!("camera{index}")),
        title: format!("Camera {index}"),
    }));
    // Opened from the share button near the window's foot.
    app.call.spot.set(Rectangle::new(
        Point::new(500.0, 500.0),
        Size::new(52.0, 52.0),
    ));
    let _ = app.call_event(Event::Share(ShareEvent::Listed(std::sync::Arc::new(
        Listing(listed),
    ))));
    assert_eq!(app.call.popup, Some(Popup::Share));
    let mut go = None;
    interact(&mut app, |ui| {
        let at = last(ui, "Go Live");
        go = Some(at);
        assert!(at.y < 560.0 - 8.0, "Go Live is in view, at {at:?}");
        assert!(ui.find("Entire screen").is_ok());
        ui.click("Window 1").expect("a window to pick");
    });
    assert_eq!(app.call.share.as_ref().map(|share| share.pick), Some(2));
}

#[test]
fn the_input_menu_measures_the_microphone_only_while_it_shows() {
    let (mut app, _folder) = preview();
    assert!(!app.metering());
    let _ = app.call_event(Event::Open(Popup::Input));
    assert!(app.metering(), "the level shows");
    assert!(shows(&app, "INPUT LEVEL"));
    assert!(shows(&app, "Input Device"));
    let _ = app.call_event(Event::Close);
    assert!(!app.metering());
    let _ = app.call_event(Event::Open(Popup::Output));
    assert!(!app.metering(), "the speakers' menu has no level");
    assert!(shows(&app, "Output Volume"));
}

#[test]
fn clicking_the_docks_channel_line_opens_the_call() {
    let (mut app, _folder, room) = in_call();
    app.client
        .dispatch(Action::Navigate(Navigation::DirectMessages));
    let _ = app.sync();
    assert_ne!(app.client.state().selected.as_deref(), Some(room.as_str()));
    assert!(shows(&app, "Preview voice"));
    interact(&mut app, |ui| {
        let line = super::starting(ui, "Lounge").expect("the dock's line");
        ui.point_at(line);
        ui.simulate(iced_test::simulator::click());
    });
    assert_eq!(app.client.state().selected.as_deref(), Some(room.as_str()));
    assert!(shows(&app, &app.voice_description()), "the call shows");
}

#[test]
fn a_voice_channel_this_account_isnt_in_offers_to_join() {
    let (mut app, _folder, _) = in_call();
    let _ = app.call_event(Event::Leave);
    let _ = app.sync();
    assert!(!app.client.state().voice.active());
    assert!(shows(&app, "Join voice"));
    let description = app.voice_description();
    assert!(description.contains("Join and hang out."), "{description}");
    assert!(shows(&app, &description));
}

#[test]
fn someones_tile_right_clicked_sets_their_volume_and_keeps_it() {
    let (mut app, folder, room) = in_call();
    super::change(&mut app, |state| {
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.user = "Bob".into();
        bob.local = false;
        state.voice.participants.push(bob);
    });
    // One's own tile has nothing to set in the preview.
    let you = app.client.state().voice.participants[0].person.clone();
    let _ = app.call_event(Event::Person(room.clone(), you, Point::new(400.0, 300.0)));
    assert!(app.menu.is_none());
    let _ = app.call_event(Event::Person(room, "bob".into(), Point::new(400.0, 300.0)));
    let volumes = app.menu.as_ref().and_then(|menu| menu.volumes.as_ref());
    assert_eq!(volumes.map(|volumes| volumes.name.as_str()), Some("Bob"));
    assert!(shows(&app, "User volume"));
    assert!(!shows(&app, "Stream volume"), "Bob isn't streaming");
    let gain = || yapfast_core::volume::get(yapfast_core::volume::Kind::Voice, "bob");
    let _ = app.call_event(Event::Volume(false, 50, false));
    let halved = gain();
    assert!(halved < 1.0, "plays quieter while dragged: {halved}");
    let stored = || crate::layout::Layout::read(folder.path()).voice.volumes;
    assert!(stored().is_empty(), "a drag's steps aren't saved");
    let _ = app.call_event(Event::Volume(false, 50, true));
    assert_eq!(stored().get("bob"), Some(&halved));
    // Back at its default it's forgotten.
    let default = yapfast_core::volume::to_slider(yapfast_core::volume::Kind::Voice.default_gain());
    let _ = app.call_event(Event::Volume(false, default.round() as u32, true));
    assert!(stored().is_empty());
}
