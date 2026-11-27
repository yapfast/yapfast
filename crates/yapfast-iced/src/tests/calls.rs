//! The call: the dock's noise toggle, the device menus' volumes and a
//! device gone away, the share menu in a short window, its settings' rows
//! with their choices beside them, the application remembered for the
//! stream's audio, the microphone's level only while its menu shows, the
//! dock's channel line, the call's hint closed for good, a voice channel's
//! Join, someone's volumes and menu from their tile, a camera's tile
//! before its first frame, the corner picture's grip resizing it, and the
//! call's own order under its channel.

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

/// Discord's client keeps the user's own mute apart from deafening: a
/// call joined deafened undeafens to the microphone its other preference
/// asks for, and deafening in a call leaves the mute as it was.
#[test]
fn undeafening_leaves_the_mute_as_it_was() {
    let call = |app: &App| {
        let voice = &app.client.state().voice;
        (voice.muted, voice.deafened)
    };
    let deafen = |app: &mut App| {
        let _ = app.update(crate::app::Message::Account(crate::account::Event::Deafen));
        let _ = app.sync();
    };
    for unmute in [true, false] {
        let (mut app, _folder) = preview();
        app.layout.deafen_on_join = true;
        app.layout.unmute_on_join = Some(unmute);
        app.client
            .dispatch(Action::Voice(voice::Command::Join("lounge".into())));
        // Once for the join, which deafens, and once for the deafening.
        let _ = app.sync();
        let _ = app.sync();
        assert_eq!(call(&app), (true, true), "joined deafened, and so muted");
        deafen(&mut app);
        assert_eq!(call(&app), (!unmute, false));
        deafen(&mut app);
        assert_eq!(call(&app), (true, true));
        deafen(&mut app);
        assert_eq!(call(&app), (!unmute, false), "and so each time");
    }
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
    let _ = crate::call::update(&mut app, Event::Noise);
    assert_eq!(app.layout.voice.noise_suppression, Suppressor::Off);
    assert_eq!(stored(), Suppressor::Off, "saved");
    let _ = crate::call::update(&mut app, Event::Noise);
    assert_eq!(
        app.layout.voice.noise_suppression,
        Suppressor::DeepFilterNet
    );
    // RNNoise, picked in the input menu, is the one it brings back.
    let _ = crate::call::update(&mut app, Event::Suppressor(Suppressor::RNNoise));
    let _ = crate::call::update(&mut app, Event::Noise);
    assert_eq!(app.layout.voice.noise_suppression, Suppressor::Off);
    let _ = crate::call::update(&mut app, Event::Noise);
    assert_eq!(app.layout.voice.noise_suppression, Suppressor::RNNoise);
    assert_eq!(stored(), Suppressor::RNNoise);
    // Off picked there after RNNoise brings RNNoise back too.
    let _ = crate::call::update(&mut app, Event::Suppressor(Suppressor::Off));
    let _ = crate::call::update(&mut app, Event::Noise);
    assert_eq!(app.layout.voice.noise_suppression, Suppressor::RNNoise);
    let _ = crate::call::update(&mut app, Event::Suppressor(Suppressor::DeepFilterNet));
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
    // Other tests' windows play their volumes as they open.
    let _volumes = crate::layout::tests::hold_volumes();
    let stored = || crate::layout::Layout::read(folder.path()).voice;
    let _ = crate::call::update(&mut app, Event::DeviceVolume(true, 200, false));
    let _ = crate::call::update(&mut app, Event::DeviceVolume(false, 50, false));
    assert!((yapfast_core::volume::microphone() - 1.995).abs() < 1e-3);
    assert!((yapfast_core::volume::speakers() - 0.143).abs() < 1e-3);
    assert_eq!(
        stored().microphone_volume,
        None,
        "a drag's steps aren't saved"
    );
    let _ = crate::call::update(&mut app, Event::DeviceVolume(true, 200, true));
    let _ = crate::call::update(&mut app, Event::DeviceVolume(false, 50, true));
    assert_eq!(stored().microphone_volume, Some(200));
    assert_eq!(stored().speaker_volume, Some(50));
    // A new window plays at the volumes kept, and the defaults at none.
    crate::layout::Voice::default().play_volumes();
    assert_eq!(yapfast_core::volume::microphone(), 1.0);
    stored().play_volumes();
    assert!((yapfast_core::volume::speakers() - 0.143).abs() < 1e-3);
    crate::layout::Voice::default().play_volumes();
}

/// What a listing of the screens and windows says, as it comes.
fn listed(count: u64) -> Event {
    use crate::call::{Listing, ShareEvent};
    let listed = vec![Shareable {
        source: Source::Display(1),
        title: "Entire screen".into(),
        program: None,
    }];
    Event::Share(ShareEvent::Listed(
        count,
        std::sync::Arc::new(Listing(listed)),
    ))
}

#[test]
fn the_share_menu_waits_for_its_own_listing_and_a_cancel_drops_it() {
    use iced::keyboard::{Key, key};
    let (mut app, _folder, _) = in_call();
    // Escape while it's listed cancels it: what's listed after opens nothing.
    let _ = crate::call::update(&mut app, Event::Open(Popup::Share));
    let first = app.call.share_listing.expect("listing");
    let _ = app.update(crate::app::Message::Key(
        Key::Named(key::Named::Escape),
        Default::default(),
    ));
    assert_eq!(app.call.share_listing, None, "Escape cancels it");
    let _ = crate::call::update(&mut app, listed(first));
    assert_eq!(app.call.popup, None, "nothing opens after the cancel");
    // A second click on Go Live cancels it too, and a third lists anew,
    // which the first listing's late answer doesn't open.
    let _ = crate::call::update(&mut app, Event::Open(Popup::Share));
    let _ = crate::call::update(&mut app, Event::Open(Popup::Share));
    assert_eq!(app.call.share_listing, None);
    let _ = crate::call::update(&mut app, Event::Open(Popup::Share));
    let third = app.call.share_listing.expect("listing again");
    let _ = crate::call::update(&mut app, listed(first));
    assert_eq!(app.call.popup, None, "an older listing's answer");
    // The input menu opened meanwhile stays.
    let _ = crate::call::update(&mut app, Event::Open(Popup::Input));
    let _ = crate::call::update(&mut app, listed(third));
    assert_eq!(app.call.popup, Some(Popup::Input));
    // Out of the call, its listing opens nothing.
    let _ = crate::call::update(&mut app, Event::Open(Popup::Share));
    let fourth = app.call.share_listing.expect("listing");
    app.client
        .dispatch(Action::Voice(yapfast_core::voice::Command::Leave));
    let _ = app.sync();
    let _ = crate::call::update(&mut app, listed(fourth));
    assert_eq!(app.call.popup, None, "left the call meanwhile");
    // Its own answer, in the call, opens it.
    let (mut app, _folder, _) = in_call();
    let _ = crate::call::update(&mut app, Event::Open(Popup::Share));
    let count = app.call.share_listing.expect("listing");
    let _ = crate::call::update(&mut app, listed(count));
    assert_eq!(app.call.popup, Some(Popup::Share));
}

#[test]
fn the_share_menu_fits_a_short_window_and_keeps_its_button() {
    use crate::call::{Listing, ShareEvent};
    let (mut app, _folder, _) = in_call();
    app.shell.frame = Size::new(800.0, 560.0);
    crate::window_bar::fit_window(&mut app);
    // More than the window is tall: a screen, twelve windows, four cameras.
    let mut listed = vec![Shareable {
        source: Source::Display(1),
        title: "Entire screen".into(),
        program: None,
    }];
    listed.extend((0..12).map(|index| Shareable {
        source: Source::Window(index),
        title: format!("Window {index}"),
        program: None,
    }));
    listed.extend((0..4).map(|index| Shareable {
        source: Source::Camera(format!("camera{index}")),
        title: format!("Camera {index}"),
        program: None,
    }));
    // Opened from the share button near the window's foot.
    app.call.spot.set(Rectangle::new(
        Point::new(500.0, 500.0),
        Size::new(52.0, 52.0),
    ));
    let _ = crate::call::update(&mut app, Event::Open(Popup::Share));
    let count = app.call.share_listing.expect("listing");
    let _ = crate::call::update(
        &mut app,
        Event::Share(ShareEvent::Listed(
            count,
            std::sync::Arc::new(Listing(listed)),
        )),
    );
    assert_eq!(app.call.popup, Some(Popup::Share));
    let mut go = None;
    interact(&mut app, |ui| {
        let at = last(ui, "Go Live");
        go = Some(at);
        assert!(at.y < 560.0 - 8.0, "Go Live is in view, at {at:?}");
        // The settings' rows stay in view with it, over the sources,
        // which scroll.
        assert!(last(ui, "Audio").y < at.y && last(ui, "Higher quality").y < at.y);
        let screen = ui.find("Entire screen").expect("the screen");
        ui.point_at(screen.visible_bounds().expect("in view").center());
        ui.simulate(crate::widgets::swipe(-120.0));
        ui.click("Window 1").expect("a window to pick");
    });
    assert_eq!(
        app.call.share.as_ref().and_then(|share| share.pick),
        Some(2)
    );

    // Window 1 and the screen have the same numeric ID. A picture is applied
    // only to its source kind and only to the listing that requested it.
    for preview_count in [count + 1, count] {
        let _ = crate::call::update(
            &mut app,
            Event::Share(ShareEvent::Picture(
                preview_count,
                Source::Window(1),
                iced::advanced::image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]),
            )),
        );
        let share = app.call.share.as_ref().expect("share menu");
        assert!(share.screens[0].1.is_none());
        assert_eq!(share.windows[1].1.is_some(), preview_count == count);
    }
}

/// The share menu open over a window `size` large from the share button
/// at `spot`, with `listed` to share; `applications` as on Windows, where
/// the stream's audio can be one application's.
fn share_menu(
    size: Size,
    spot: Point,
    listed: Vec<Shareable>,
    applications: bool,
) -> (App, tempfile::TempDir) {
    let (mut app, folder, _) = in_call();
    open_share_menu(&mut app, size, spot, listed, applications);
    (app, folder)
}

fn open_share_menu(
    app: &mut App,
    size: Size,
    spot: Point,
    listed: Vec<Shareable>,
    applications: bool,
) {
    app.shell.frame = size;
    crate::window_bar::fit_window(app);
    app.call
        .spot
        .set(Rectangle::new(spot, Size::new(52.0, 52.0)));
    let _ = crate::call::update(app, Event::Close);
    let _ = crate::call::update(app, Event::Open(Popup::Share));
    let count = app.call.share_listing.expect("listing");
    let listing = crate::call::Listing(listed);
    let _ = crate::call::popups::share::list(app, count, &listing, applications);
    assert_eq!(app.call.popup, Some(Popup::Share));
}

/// A screen, `windows` windows, each of a program of its own
/// (`app<index>.exe`), and a camera.
fn sources(windows: u64) -> Vec<Shareable> {
    let mut listed = vec![Shareable {
        source: Source::Display(1),
        title: "Entire screen".into(),
        program: None,
    }];
    listed.extend((0..windows).map(|index| Shareable {
        source: Source::Window(index),
        title: format!("Window {index}"),
        program: Some(format!("app{index}.exe")),
    }));
    listed.push(Shareable {
        source: Source::Camera("camera".into()),
        title: "Camera".into(),
        program: None,
    });
    listed
}

/// The pointer moved onto the text reading `text`.
fn point_at(app: &mut App, text: &str) {
    interact(app, |ui| {
        let at = last(ui, text);
        ui.point_at(at);
        ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
            position: at,
        })]);
    });
}

/// Where the texts reading each of `texts` are, in view.
fn places(app: &App, texts: &[&str]) -> Vec<Point> {
    use iced_test::Simulator;
    let mut ui = Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
    texts.iter().map(|text| last(&mut ui, text)).collect()
}

/// A click on the last text reading `text`: a choice beside the menu,
/// which is drawn after it.
fn click_last(app: &mut App, text: &str) {
    interact(app, |ui| {
        let at = last(ui, text);
        ui.point_at(at);
        ui.simulate(iced_test::simulator::click());
    });
}

fn side(app: &App) -> Option<crate::call::popups::share::Setting> {
    app.call.share.as_ref().and_then(|share| share.side)
}

#[test]
fn the_share_menus_settings_are_rows_that_open_their_choices_beside_them() {
    use crate::call::{ShareEvent, popups::share::Setting};
    use iced::keyboard::{Key, key};
    let spot = Point::new(300.0, 640.0);
    let (mut app, _folder) = share_menu(Size::new(1200.0, 720.0), spot, sources(3), false);
    // Each setting is one row that names what's chosen, and none of the
    // choices show.
    for shown in [
        "Audio",
        "All applications",
        "Resolution",
        "1080p",
        "Frame rate",
        "30 fps",
        "Higher quality",
        "Go Live",
    ] {
        assert!(shows(&app, shown), "{shown}");
    }
    for hidden in ["No audio", "720p", "Source", "15 fps", "60 fps"] {
        assert!(!super::present(&app, hidden), "{hidden}");
    }
    // The pointer on a row opens its choices to the right of the menu,
    // the first level with the row, and nothing in the menu moves.
    let parts = [
        "Entire screen",
        "Window 1",
        "Audio",
        "Resolution",
        "Frame rate",
        "Higher quality",
        "Go Live",
    ];
    let closed = places(&app, &parts);
    let right = spot.x - 6.0 + 496.0;
    point_at(&mut app, "Resolution");
    assert_eq!(side(&app), Some(Setting::Resolution));
    assert_eq!(places(&app, &parts), closed, "the menu stays as it was");
    let choices = places(&app, &["720p", "Source", "Resolution"]);
    assert!(choices[0].x > right, "beside the menu: {choices:?}");
    assert!((choices[0].y - choices[2].y).abs() < 3.0, "{choices:?}");
    assert!(choices[1].y > choices[0].y);
    // Another row's take their place: one list at a time.
    point_at(&mut app, "Frame rate");
    assert_eq!(side(&app), Some(Setting::FrameRate));
    assert!(!super::present(&app, "720p"));
    assert_eq!(places(&app, &parts), closed);
    let choices = places(&app, &["15 fps", "Frame rate"]);
    assert!(choices[0].x > right && (choices[0].y - choices[1].y).abs() < 3.0);
    // A choice is taken and its list closes; the row names it.
    interact(&mut app, |ui| {
        ui.click("60 fps").expect("a frame rate");
    });
    let share = app.call.share.as_ref().expect("the menu stays");
    assert_eq!((share.quality.fps, share.side), (60, None));
    assert!(shows(&app, "60 fps") && !super::present(&app, "15 fps"));
    assert_eq!(places(&app, &parts), closed);
    // A click on a row opens its choices too.
    interact(&mut app, |ui| {
        ui.click("Resolution").expect("the row");
    });
    assert_eq!(side(&app), Some(Setting::Resolution));
    interact(&mut app, |ui| {
        ui.click("Source").expect("a resolution");
    });
    let share = app.call.share.as_ref().expect("the menu stays");
    assert_eq!(share.quality.resolution, voice::Resolution::Source);
    assert_eq!(share.side, None);
    // The audio's choices: here, without an application's.
    point_at(&mut app, "Audio");
    assert_eq!(side(&app), Some(Setting::Audio));
    assert!(shows(&app, "No audio") && shows(&app, "Excludes Yapfast's own audio"));
    assert!(!super::present(&app, "Changes apply immediately"));
    // The pointer on a row without choices closes them, as picking what
    // to share does.
    point_at(&mut app, "Higher quality");
    assert_eq!(side(&app), None);
    let _ = crate::call::update(
        &mut app,
        Event::Share(ShareEvent::Side(Some(Setting::Audio))),
    );
    interact(&mut app, |ui| {
        ui.click("Window 1").expect("a window to share");
    });
    let share = app.call.share.as_ref().expect("the menu stays");
    assert_eq!((share.pick, share.side), (Some(2), None));
    // Escape closes the choices, then the menu.
    let _ = crate::call::update(
        &mut app,
        Event::Share(ShareEvent::Side(Some(Setting::Audio))),
    );
    let escape = crate::app::Message::Key(Key::Named(key::Named::Escape), Default::default());
    let _ = app.update(escape.clone());
    assert_eq!(side(&app), None);
    assert_eq!(app.call.popup, Some(Popup::Share));
    let _ = app.update(escape);
    assert_eq!(app.call.popup, None);

    // Without room on the right the choices open on the left, the menu
    // where it was.
    let spot = Point::new(900.0, 640.0);
    open_share_menu(&mut app, Size::new(1200.0, 720.0), spot, sources(3), false);
    let closed = places(&app, &parts);
    point_at(&mut app, "Frame rate");
    assert_eq!(places(&app, &parts), closed);
    let left = 1200.0 - 8.0 - 496.0;
    let choices = places(&app, &["15 fps", "Frame rate"]);
    assert!(
        choices[0].x < left,
        "left of the menu: {choices:?} {closed:?}"
    );
    assert!((choices[0].y - choices[1].y).abs() < 3.0);
}

/// A theme that dashes the menus' outlines lays the outline over the
/// panel: a setting's choices still open level with its row.
#[test]
fn the_share_menus_choices_open_beside_their_row_under_a_themes_outline() {
    use crate::call::{ShareEvent, popups::share::Setting};
    let (mut app, _folder) = super::themed(
        serde_json::json!({ "shape": { "borders": { "menu": "dashed" } } }),
        &[],
    );
    assert!(app.ctx().look.theme.outline("menu").is_some());
    app.client.dispatch(Action::SelectRoom("lounge".into()));
    let _ = app.sync();
    assert!(app.client.state().voice.active(), "the preview joins");
    let spot = Point::new(300.0, 640.0);
    open_share_menu(&mut app, Size::new(1200.0, 720.0), spot, sources(3), false);
    let _ = crate::call::update(
        &mut app,
        Event::Share(ShareEvent::Side(Some(Setting::FrameRate))),
    );
    let choices = places(&app, &["15 fps", "Frame rate"]);
    assert!(choices[0].x > spot.x - 6.0 + 496.0, "{choices:?}");
    assert!((choices[0].y - choices[1].y).abs() < 3.0, "{choices:?}");
}

#[test]
fn the_share_menus_audio_is_an_application_picked_beside_its_row() {
    use crate::call::{ShareEvent, popups::share::Setting};
    let size = Size::new(1200.0, 720.0);
    let spot = Point::new(300.0, 640.0);
    // Where the audio can be one application's, its windows follow the
    // two choices there always are.
    let (mut app, _folder) = share_menu(size, spot, sources(3), true);
    point_at(&mut app, "Audio");
    click_last(&mut app, "Window 1");
    assert_eq!(
        app.call.share.as_ref().and_then(|share| share.pick),
        Some(0),
        "its row beside the menu, not its card"
    );
    assert_eq!(
        app.client.state().voice.stream_audio,
        voice::StreamAudio::Window(1)
    );
    assert_eq!(side(&app), None);
    let named = places(&app, &["Audio", "Window 1"]);
    assert!((named[0].y - named[1].y).abs() < 3.0, "its row names it");
    // More windows than fit: the list scrolls inside the window, and the
    // menu stays where it was.
    let parts = ["Entire screen", "Audio", "Go Live"];
    open_share_menu(&mut app, size, spot, sources(40), true);
    let closed = places(&app, &parts);
    let _ = crate::call::update(
        &mut app,
        Event::Share(ShareEvent::Side(Some(Setting::Audio))),
    );
    assert_eq!(places(&app, &parts), closed);
    assert!(shows(&app, "No audio"));
    assert!(!shows(&app, "Window 39"), "the last is scrolled to");
}

#[test]
fn an_application_chosen_for_the_audio_is_found_again_or_everything_is_shared() {
    let size = Size::new(1200.0, 720.0);
    let spot = Point::new(300.0, 640.0);
    let remembered = |app: &mut App, audio| {
        super::change(app, |state| {
            state.voice.stream_audio = audio;
            state.voice.stream_app = Some("app2.exe".into());
        });
    };
    let audio = |app: &App| app.client.state().voice.stream_audio;
    let named = |app: &App, title: &str| {
        let named = places(app, &["Audio", title]);
        (named[0].y - named[1].y).abs() < 3.0
    };
    // As a new start has it: the application by its program, everything
    // shared until a window of it is listed.
    let (mut app, _folder, _) = in_call();
    remembered(&mut app, voice::StreamAudio::All);
    open_share_menu(&mut app, size, spot, sources(4), true);
    assert_eq!(audio(&app), voice::StreamAudio::Window(2));
    assert!(named(&app, "Window 2"));
    // Not running: everything is shared, and it's still the one looked
    // for the next time.
    remembered(&mut app, voice::StreamAudio::All);
    open_share_menu(&mut app, size, spot, sources(2), true);
    assert_eq!(audio(&app), voice::StreamAudio::All);
    assert!(named(&app, "All applications"));
    assert_eq!(
        app.client.state().voice.stream_app.as_deref(),
        Some("app2.exe")
    );
    // Its window closed since it was chosen: another of its windows is
    // taken, and without one the menu says it's gone.
    remembered(&mut app, voice::StreamAudio::Window(77));
    let mut listed = sources(2);
    listed.push(Shareable {
        source: Source::Window(9),
        title: "Second window".into(),
        program: Some("app2.exe".into()),
    });
    open_share_menu(&mut app, size, spot, listed, true);
    assert_eq!(audio(&app), voice::StreamAudio::Window(9));
    remembered(&mut app, voice::StreamAudio::Window(77));
    open_share_menu(&mut app, size, spot, sources(2), true);
    assert_eq!(audio(&app), voice::StreamAudio::Window(77));
    assert!(named(&app, "Unavailable application"));
    // A window of it chosen and still open stays the choice.
    remembered(&mut app, voice::StreamAudio::Window(9));
    let mut listed = sources(3);
    listed.push(Shareable {
        source: Source::Window(9),
        title: "Second window".into(),
        program: Some("app2.exe".into()),
    });
    open_share_menu(&mut app, size, spot, listed, true);
    assert_eq!(audio(&app), voice::StreamAudio::Window(9));
    // A live stream's audio isn't changed by opening the menu.
    remembered(&mut app, voice::StreamAudio::All);
    super::change(&mut app, |state| {
        state.voice.sharing = Some(voice::Sharing {
            phase: voice::SharePhase::Live,
            frames: std::sync::Arc::new(yapfast_core::frames::Frames::create(16, 16).unwrap()),
            viewers: vec![],
        });
    });
    open_share_menu(&mut app, size, spot, sources(4), true);
    assert_eq!(audio(&app), voice::StreamAudio::All);
}

#[test]
fn the_input_menu_measures_the_microphone_only_while_it_shows() {
    let (mut app, _folder) = preview();
    assert!(!crate::call::metering(&app));
    let _ = crate::call::update(&mut app, Event::Open(Popup::Input));
    assert!(crate::call::metering(&app), "the level shows");
    assert!(shows(&app, "INPUT LEVEL"));
    assert!(shows(&app, "Input Device"));
    let _ = crate::call::update(&mut app, Event::Close);
    assert!(!crate::call::metering(&app));
    let _ = crate::call::update(&mut app, Event::Open(Popup::Output));
    assert!(
        !crate::call::metering(&app),
        "the speakers' menu has no level"
    );
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
    assert!(
        shows(&app, &crate::call::stage::voice_description(&app)),
        "the call shows"
    );
}

#[test]
fn the_calls_stays_connected_hint_closes_for_good() {
    let (mut app, folder, _) = in_call();
    super::change(&mut app, |state| {
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.user = "Bob".into();
        bob.local = false;
        state.voice.participants.push(bob);
    });
    let hint = "Voice stays connected while you browse other conversations.";
    assert_eq!(crate::call::stage::voice_description(&app), hint);
    assert!(shows(&app, hint));
    // Its close button sits just after it.
    interact(&mut app, |ui| {
        let words = ui.find(hint).expect("the hint");
        let bounds = words.visible_bounds().expect("in view");
        ui.point_at(Point::new(
            bounds.x + bounds.width + 14.0,
            bounds.center_y(),
        ));
        ui.simulate(iced_test::simulator::click());
    });
    assert!(app.layout.hide_voice_hint);
    assert!(!super::present(&app, hint));
    assert!(
        crate::layout::Layout::read(folder.path()).hide_voice_hint,
        "kept for the next start"
    );
    // The call's other lines still show.
    super::change(&mut app, |state| {
        state.voice.participants.truncate(1);
    });
    let alone = crate::call::stage::voice_description(&app);
    assert!(alone.starts_with("You're here alone."), "{alone}");
    assert!(shows(&app, &alone));
}

#[test]
fn a_voice_channel_this_account_isnt_in_offers_to_join() {
    let (mut app, _folder, _) = in_call();
    let _ = crate::call::update(&mut app, Event::Leave);
    let _ = app.sync();
    assert!(!app.client.state().voice.active());
    assert!(shows(&app, "Join voice"));
    let description = crate::call::stage::voice_description(&app);
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
    let _ = crate::call::update(
        &mut app,
        Event::Person(room.clone(), you, Point::new(400.0, 300.0)),
    );
    assert!(app.win.menu.is_none());
    let _ = crate::call::update(
        &mut app,
        Event::Person(room, "bob".into(), Point::new(400.0, 300.0)),
    );
    let volumes = app.win.menu.as_ref().and_then(|menu| menu.volumes.as_ref());
    assert_eq!(volumes.map(|volumes| volumes.name.as_str()), Some("Bob"));
    assert!(shows(&app, "User volume"));
    assert!(!shows(&app, "Stream volume"), "Bob isn't streaming");
    let gain = || yapfast_core::volume::get(yapfast_core::volume::Kind::Voice, "bob");
    let _ = crate::call::update(&mut app, Event::Volume(false, 50, false));
    let halved = gain();
    assert!(halved < 1.0, "plays quieter while dragged: {halved}");
    let stored = || crate::layout::Layout::read(folder.path()).voice.volumes;
    assert!(stored().is_empty(), "a drag's steps aren't saved");
    let _ = crate::call::update(&mut app, Event::Volume(false, 50, true));
    assert_eq!(stored().get("bob"), Some(&halved));
    // Back at its default it's forgotten.
    let default = yapfast_core::volume::to_slider(yapfast_core::volume::Kind::Voice.default_gain());
    let _ = crate::call::update(&mut app, Event::Volume(false, default.round() as u32, true));
    assert!(stored().is_empty());
}

#[test]
fn the_signal_opens_the_connections_details_with_their_tabs() {
    use crate::call::{LinkEvent, LinkTab};
    let (mut app, _folder, _) = in_call();
    assert!(
        !crate::call::details::link_measured(app.ctx()),
        "the preview doesn't measure"
    );
    let stats = voice::LinkStats {
        pings: vec![40, 80],
        loss: Some(1.5),
        privacy_code: Some("123456789012345678901234567890".into()),
        stream: Some(voice::StreamStats {
            engine: "h264_nvenc".into(),
            codec: "H264".into(),
            captured: "2560×1440 screen".into(),
            width: 1920,
            height: 1080,
            fps: 60,
            target_kbps: 4000,
            sent_kbps: 3812,
            keyframe_requests: 2,
            packets: 1000,
            resent: 5,
        }),
        watching: Some(voice::WatchStats {
            decoder: "dav1d".into(),
            codec: "AV1".into(),
            width: 1280,
            height: 720,
            fps: 30,
            received_kbps: 2500,
            packets: 4000,
            lost: 8,
            repaired: 31,
            keyframe_requests: 1,
            dropped: 0,
        }),
    };
    app.client.app.state.voice.link = Some(voice::Link::new(move || stats.clone()));
    let _ = app.sync();
    assert!(crate::call::details::link_measured(app.ctx()));
    // The pointer on the signal reads the ping for its tooltip.
    let _ = crate::call::update(&mut app, Event::Link(LinkEvent::Pointed(true)));
    assert!(app.call.details.stats.is_some());
    let _ = crate::call::update(&mut app, Event::Open(Popup::Link));
    for words in [
        "Average ping:",
        "60 ms",
        "Last ping:",
        "80 ms",
        "1.5%",
        "100 ms",
        "50 ms",
        "Connection",
        "Secure connection",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    assert!(!shows(&app, "Privacy"), "alone, no code to compare");
    // Someone else joins to compare it with.
    let voice = &mut app.client.app.state.voice;
    let mut other = voice.participants[0].clone();
    other.id = "other".into();
    voice.participants.push(other);
    let _ = app.sync();
    interact(&mut app, |ui| {
        ui.click("Privacy").expect("the privacy tab");
    });
    assert_eq!(app.call.details.tab, LinkTab::Privacy);
    assert!(shows(&app, "Voice Privacy Code"));
    assert!(shows(&app, "67890"));
    let _ = crate::call::update(&mut app, Event::Link(LinkEvent::Copy));
    assert!(shows(&app, "Copied"));
    let _ = crate::call::update(&mut app, Event::Link(LinkEvent::CopiedDone));
    assert!(shows(&app, "Copy"));
    let _ = crate::call::update(&mut app, Event::Link(LinkEvent::Tab(LinkTab::Stream)));
    for words in [
        "Sharing",
        "NVENC on the GPU",
        "H264",
        "2560×1440 screen",
        "1920×1080 at 60 fps",
        "3.8 Mbit/s",
        "4.0 Mbit/s",
        "5 (0.5%)",
        "Watching",
        "dav1d on the CPU",
        "1280×720 at 30 fps",
        "2.5 Mbit/s",
        "8 (0.2%)",
        "Copy diagnostics",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    let _ = crate::call::update(&mut app, Event::Link(LinkEvent::Diagnostics));
    assert!(shows(&app, "Copied"));
    let _ = crate::call::update(&mut app, Event::Link(LinkEvent::CopiedDone));
    // Closed with the pointer gone, nothing is copied out of the call.
    let _ = crate::call::update(&mut app, Event::Link(LinkEvent::Pointed(false)));
    let _ = crate::call::update(&mut app, Event::Close);
    assert!(app.call.details.stats.is_none());
    assert_eq!(app.call.details.tab, LinkTab::Connection);
}

#[test]
fn the_banners_show_over_a_voice_channels_call_too() {
    let (mut app, _folder, _) = in_call();
    super::change(&mut app, |state| {
        state.notice = Some("Discord asked Yapfast to slow down.".into());
    });
    assert!(crate::call::stage::voice_panel(&app), "the call's view");
    assert!(shows(&app, "Discord asked Yapfast to slow down."));
}

#[test]
fn cameras_are_made_at_the_windows_pixels_on_a_scaled_display() {
    let (mut app, _folder, _) = in_call();
    let feed = yapfast_core::picture::Feed::new();
    let camera = feed.clone();
    super::change(&mut app, move |state| {
        state.voice.participants[0].video = Some(camera);
    });
    let (width, height) = feed.shown_at().expect("made for its tile");
    let id = app.shell.window.expect("the window");
    let _ = app.update(crate::app::Message::Shell(crate::shell::Event::Scaled(
        id, 2.0,
    )));
    let (wide, tall) = feed.shown_at().expect("made for its tile");
    assert!(
        wide.abs_diff(width * 2) <= 1 && tall.abs_diff(height * 2) <= 1,
        "{width}x{height} at 100%, {wide}x{tall} at 200%"
    );
}

#[test]
fn a_camera_without_a_frame_yet_shows_their_picture_and_name() {
    let (mut app, _folder, _) = in_call();
    // Bob's camera is on, but no frame has come.
    super::change(&mut app, |state| {
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.user = "Bob".into();
        bob.local = false;
        bob.video = Some(yapfast_core::picture::Feed::new());
        state.voice.participants.push(bob);
    });
    assert!(crate::call::stage::voice_panel(&app), "the call's view");
    // His tile has his name at its foot, over his picture, until the
    // first frame.
    assert!(shows(&app, "Bob"), "his tile's name");
}

/// Discord's picture in picture: its grip resizes it horizontally only,
/// from 200 px up to the window's room, and settles on the nearest 40 px
/// when let go.
#[test]
fn the_corner_pictures_grip_resizes_it_and_the_width_is_kept() {
    use crate::app::Message;
    let (mut app, folder, _) = in_call();
    // Bob's camera stays in the corner once the call is off screen.
    super::change(&mut app, |state| {
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.local = false;
        bob.video = Some(yapfast_core::picture::Feed::new());
        state.voice.participants.push(bob);
    });
    app.client.dispatch(Action::SelectRoom("general".into()));
    let _ = app.sync();
    assert!(
        crate::call::corner::corner_view(&app).is_some(),
        "in the corner"
    );
    let width = |app: &App| crate::call::corner::corner_size(app).width;
    let size = crate::call::corner::corner_size(&app);
    assert_eq!((size.width, size.height), (320.0, 180.0), "as it comes");
    // Its grip, 90 px towards the window's middle: 90 px wider while
    // dragged, whatever the pointer does up or down, then 400 px, the
    // nearest 40, once let go.
    let grip = crate::call::corner::grip(&app).center();
    app.pressed.set(grip);
    let _ = app.update(Message::Call(Event::Grab));
    assert!(app.call.resize.is_some(), "its grip resizes it");
    for down in [0.0, 120.0, -60.0] {
        let _ = app.update(Message::Call(Event::DragTo(Point::new(
            grip.x - 90.0,
            grip.y + down,
        ))));
        assert_eq!(width(&app), 410.0, "across only, {down} px down");
    }
    let _ = app.update(Message::Call(Event::Drop));
    assert!(app.call.resize.is_none() && app.call.grab.is_none());
    assert_eq!(width(&app), 400.0, "settled on 40 px");
    assert_eq!(
        crate::layout::Layout::read(folder.path()).corner_width,
        Some(400.0),
        "kept"
    );
    // Up and down alone change nothing.
    let grip = crate::call::corner::grip(&app).center();
    app.pressed.set(grip);
    let _ = app.update(Message::Call(Event::Grab));
    let _ = app.update(Message::Call(Event::DragTo(Point::new(
        grip.x,
        grip.y + 200.0,
    ))));
    let _ = app.update(Message::Call(Event::Drop));
    assert_eq!(width(&app), 400.0, "the same");
    // Towards the window's edge it shrinks, to 200 px at least.
    let grip = crate::call::corner::grip(&app).center();
    app.pressed.set(grip);
    let _ = app.update(Message::Call(Event::Grab));
    let _ = app.update(Message::Call(Event::DragTo(Point::new(
        grip.x + 500.0,
        grip.y,
    ))));
    assert_eq!(width(&app), 200.0, "no narrower");
    let _ = app.update(Message::Call(Event::Drop));
    assert_eq!(width(&app), 200.0);
    // And as wide as the window has room for, no cap under it, in a window
    // with plenty of room.
    app.shell.frame = Size::new(3000.0, 2000.0);
    crate::window_bar::fit_window(&mut app);
    let room = crate::call::corner::room(&app);
    let grip = crate::call::corner::grip(&app).center();
    app.pressed.set(grip);
    let _ = app.update(Message::Call(Event::Grab));
    let _ = app.update(Message::Call(Event::DragTo(Point::new(
        grip.x - 5000.0,
        grip.y,
    ))));
    assert!(width(&app) > 960.0, "no 960 px cap: {}", width(&app));
    assert!(
        (width(&app) - room).abs() < 1.0,
        "the window's room, {room}: {}",
        width(&app)
    );
    let _ = app.update(Message::Call(Event::Drop));
    assert!(width(&app) > 960.0 && width(&app) % 40.0 == 0.0);
    // A smaller window narrows it to the room it settles on.
    app.shell.frame = Size::new(1000.0, 700.0);
    crate::window_bar::fit_window(&mut app);
    assert!(width(&app) < 960.0 && width(&app) % 40.0 == 0.0);
    // A click on its grip opens nothing, where one on the picture returns
    // to the call.
    let selected = app.client.state().selected.clone();
    app.pressed.set(crate::call::corner::grip(&app).center());
    let _ = app.update(Message::Call(Event::Grab));
    let _ = app.update(Message::Call(Event::Drop));
    assert_eq!(app.client.state().selected, selected, "no call opened");
    // Pressed on the picture instead, it's moved, not resized.
    let grip = crate::call::corner::grip(&app).center();
    app.pressed.set(Point::new(grip.x + 100.0, grip.y - 100.0));
    let _ = app.update(Message::Call(Event::Grab));
    assert!(
        app.call.grab.is_some() && app.call.resize.is_none(),
        "moved"
    );
    let _ = app.update(Message::Call(Event::Drop));
    // Its edges fall on the window's own pixels, at any zoom, so its
    // picture and the shade over it end on the same row.
    app.shell.zoom = 1.15;
    let size = crate::call::corner::corner_size(&app);
    let pixels = app.device_scale();
    for length in [size.width, size.height] {
        assert!(
            ((length * pixels).round() - length * pixels).abs() < 0.01,
            "{length} is a whole window pixel: {}",
            length * pixels
        );
    }
}

/// Sat top left, the corner picture's grip is its bottom right, over the
/// stop button's corner: a press there resizes, as the pointer says, and
/// doesn't stop the share; the button's middle still stops it.
#[test]
fn the_corner_pictures_grip_over_its_stop_button_resizes() {
    use crate::app::Message;
    let (mut app, _folder, _) = in_call();
    super::change(&mut app, |state| {
        state.voice.sharing = Some(voice::Sharing {
            phase: voice::SharePhase::Live,
            frames: std::sync::Arc::new(yapfast_core::frames::Frames::create(16, 16).unwrap()),
            viewers: vec![],
        });
    });
    app.client.dispatch(Action::SelectRoom("general".into()));
    let _ = app.sync();
    assert!(
        crate::call::corner::corner_view(&app).is_some(),
        "your screen in the corner"
    );
    app.call.left = true;
    app.call.bottom = false;
    let grip = crate::call::corner::grip(&app);
    let size = crate::call::corner::corner_size(&app);
    // The button: 32 px, 8 px in from the picture's bottom right, where
    // the grip ends.
    let corner = Point::new(grip.x + grip.width, grip.y + grip.height);
    let overlap = Point::new(corner.x - 15.0, corner.y - 15.0);
    let button = Point::new(corner.x - 24.0, corner.y - 24.0);
    assert!(grip.contains(overlap) && !grip.contains(button));
    assert!(size.width > 64.0);
    let press = |app: &App, at: Point| -> Vec<Message> {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        ui.point_at(at);
        let _ = ui.simulate(iced_test::simulator::click());
        ui.into_messages().collect()
    };
    let stops = |messages: &[Message]| {
        messages
            .iter()
            .any(|message| matches!(message, Message::Call(Event::StopSharing)))
    };
    let grabs = |messages: &[Message]| {
        messages
            .iter()
            .any(|message| matches!(message, Message::Call(Event::Grab)))
    };
    let messages = press(&app, button);
    assert!(stops(&messages), "the button's middle stops the share");
    let messages = press(&app, overlap);
    assert!(
        grabs(&messages) && !stops(&messages),
        "the grip takes it, {} messages",
        messages.len()
    );
    for message in messages {
        let _ = app.update(message);
    }
    assert!(app.call.resize.is_some(), "resized, not stopped");
    assert!(app.client.state().voice.sharing.is_some());
}

#[test]
fn the_call_this_account_is_in_lists_it_first_under_its_channel() {
    let (mut app, _folder, room) = in_call();
    let person = |id: &str, name: &str| yapfast_core::chat::VoicePerson {
        id: id.into(),
        name: name.into(),
        avatar: None,
        muted: false,
        deafened: false,
        server_muted: false,
        speaking_blocked: false,
        server_deafened: false,
        streaming: false,
        video: false,
        elsewhere: false,
        style: None,
        tag: None,
    };
    // Discord lists a channel's people by name; this account is among them.
    let you = app.client.state().voice.participants[0].person.clone();
    super::change(&mut app, |state| {
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.local = false;
        state.voice.participants.push(bob);
        let lounge = state
            .rooms
            .iter_mut()
            .find(|listed| listed.id == room)
            .expect("Lounge");
        std::sync::Arc::make_mut(lounge).people = vec![
            person("ann", "Ann Listed"),
            person("bob", "Bob Listed"),
            person(&you, "Zed Listed"),
        ];
    });
    let mut at = Vec::new();
    interact(&mut app, |ui| {
        for name in ["Zed Listed", "Ann Listed", "Bob Listed"] {
            at.push(last(ui, name).y);
        }
    });
    // This account first, then the call's others in its order, then
    // anyone the call hasn't got yet.
    assert!(at[0] < at[2] && at[2] < at[1], "{at:?}");
}

#[test]
fn this_accounts_own_row_marks_its_mute_deafening_camera_and_stream() {
    use crate::sidebar::voice::Marks;
    let (app, _folder, _) = in_call();
    let mut me = app.client.state().voice.participants[0].clone();
    assert!(me.local);
    // Discord's listing, not yet caught up with any of it.
    let listed = yapfast_core::chat::VoicePerson {
        id: me.person.clone(),
        name: me.user.clone(),
        ..Default::default()
    };
    me.muted = true;
    me.deafened = true;
    me.streaming = true;
    me.video = Some(yapfast_core::picture::Feed::new());
    let own = Marks::of(&listed, Some(&me));
    assert!(own.muted && own.deafened && own.streaming && own.video);
    // Unmuting in the call clears the mark, whatever the listing says.
    me.muted = false;
    me.deafened = false;
    let stale = yapfast_core::chat::VoicePerson {
        muted: true,
        deafened: true,
        ..listed.clone()
    };
    let own = Marks::of(&stale, Some(&me));
    assert!(!own.muted && !own.deafened);
    // Joined on this client, so not dimmed as another client's.
    let elsewhere = yapfast_core::chat::VoicePerson {
        elsewhere: true,
        ..listed
    };
    assert!(!Marks::of(&elsewhere, Some(&me)).elsewhere);
    // Someone else's row keeps what Discord lists.
    me.local = false;
    assert_eq!(Marks::of(&stale, Some(&me)), Marks::of(&stale, None));
    assert!(Marks::of(&elsewhere, None).elsewhere);
}

#[test]
fn the_large_tile_full_screen_puts_the_window_full_screen() {
    use crate::app::Message;
    use iced::keyboard::{Key, key};
    let (mut app, _folder, _) = in_call();
    let id = app.shell.window.expect("a window");
    let _ = app.update(Message::Call(Event::Focus(Some(0))));
    assert_eq!(app.call.screen, None, "large isn't full screen");
    let _ = app.update(Message::Call(Event::Full(true)));
    assert_eq!(
        app.call.screen,
        Some((id, false)),
        "the window goes full screen"
    );
    // Escape leaves it, and the window goes back.
    let _ = app.update(Message::Key(
        Key::Named(key::Named::Escape),
        Default::default(),
    ));
    assert!(!app.call.full);
    assert_eq!(app.call.screen, None);
    // A window the desktop made full screen is left so.
    app.shell.full_screen = true;
    let _ = app.update(Message::Call(Event::Full(true)));
    assert_eq!(app.call.screen, Some((id, true)));
    // Leaving the call leaves full screen too.
    let _ = app.update(Message::Call(Event::Leave));
    assert!(!crate::call::stage::full_shown(&app));
    assert_eq!(app.call.screen, None);
}

#[test]
fn a_window_closed_with_the_large_tile_full_screen_opens_again_as_a_window() {
    use crate::app::Message;
    let (mut app, _folder, _) = in_call();
    let id = app.shell.window.expect("a window");
    let _ = app.update(Message::Call(Event::Focus(Some(0))));
    let _ = app.update(Message::Call(Event::Full(true)));
    // As the window said, once it was.
    app.shell.full_screen = true;
    // Closed, as to the tray.
    let _ = app.update(Message::Shell(crate::shell::Event::Closed(id)));
    assert!(!app.call.full, "the tile is back in the call");
    assert_eq!(app.call.screen, None);
    assert!(!app.shell.full_screen, "the next window isn't full screen");
}

#[test]
fn stream_audio_can_change_while_sharing_without_picking_a_new_source() {
    let (mut app, _folder, _) = in_call();
    super::change(&mut app, |state| {
        state.voice.sharing = Some(voice::Sharing {
            phase: voice::SharePhase::Live,
            frames: std::sync::Arc::new(yapfast_core::frames::Frames::create(16, 16).unwrap()),
            viewers: vec![],
        });
    });
    let _ = crate::call::update(&mut app, Event::Open(Popup::Share));
    let count = app.call.share_listing.expect("listing");
    let _ = crate::call::update(
        &mut app,
        Event::Share(crate::call::ShareEvent::Listed(
            count,
            std::sync::Arc::new(crate::call::Listing(vec![])),
        )),
    );
    assert_eq!(app.call.popup, Some(Popup::Share));
    assert_eq!(app.call.share.as_ref().and_then(|share| share.pick), None);
    // A live stream keeps its quality: only the audio has a row, whose
    // choices say they apply at once.
    assert!(shows(&app, "Audio") && shows(&app, "Switch"));
    for hidden in ["Resolution", "Frame rate", "Higher quality"] {
        assert!(!super::present(&app, hidden), "{hidden}");
    }
    point_at(&mut app, "Audio");
    assert!(shows(&app, "Changes apply immediately"));
    interact(&mut app, |ui| {
        ui.click("No audio").expect("a choice");
    });
    assert_eq!(
        app.client.state().voice.stream_audio,
        voice::StreamAudio::Off
    );
    assert_eq!(app.call.popup, Some(Popup::Share));
    assert_eq!(side(&app), None);
    assert!(shows(&app, "No audio"), "the row names it");
}

#[test]
fn someones_voice_menu_keeps_the_cores_order_with_an_icon_on_each_row() {
    use yapfast_core::application::{MenuChoice, member_server};
    let (mut app, _folder) = preview();
    app.client.app = member_server(&["30"]);
    app.layout.developer_mode = true;
    let _ = app.sync();
    let lounge = "discord:10:12";
    let choices = app.client.app.voice_person_choices(lounge, "4");
    assert!(choices.contains(&MenuChoice::Profile) && choices.contains(&MenuChoice::CopyId));
    let _ = crate::call::update(
        &mut app,
        Event::Person(lounge.into(), "4".into(), Point::new(400.0, 300.0)),
    );
    let menu = app.win.menu.as_ref().expect("Dave's voice menu");
    let shown: Vec<_> = menu.rows.iter().map(|row| row.label.as_ref()).collect();
    let listed: Vec<_> = choices
        .iter()
        .map(|&choice| crate::place_menus::label(choice))
        .collect();
    assert_eq!(shown, listed, "the core's order");
    for row in &menu.rows {
        assert!(row.icon.is_some(), "{} has its icon", row.label);
    }
}

#[test]
fn hiding_members_collapses_the_strip_stops_hidden_cameras_and_toggles_back() {
    use crate::call::stage::MEMBERS_TOGGLE;
    use iced::widget::Id;
    use iced_test::selector::Candidate;

    let (mut app, _folder, _) = in_call();
    let bob_feed = yapfast_core::picture::Feed::new();
    let bob_camera = bob_feed.clone();
    super::change(&mut app, move |state| {
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.user = "Bob".into();
        bob.local = false;
        bob.video = Some(bob_camera);
        state.voice.participants.push(bob);
    });
    app.shell.frame = Size::new(1400.0, 760.0);
    crate::window_bar::fit_window(&mut app);
    crate::call::stage::place_videos(&mut app);

    let on_stage = |app: &App, name: &str| {
        let left = crate::conversation::chat_left(app);
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
        let mut found = false;
        let _ = ui.find(|candidate: Candidate<'_>| {
            if let Candidate::Text {
                content,
                visible_bounds: Some(bounds),
                ..
            } = candidate
                && content == name
                && bounds.x >= left
            {
                found = true;
            }
            None::<()>
        });
        found
    };

    // Grid mode has no strip to hide.
    interact(&mut app, |ui| {
        assert!(ui.find(Id::new(MEMBERS_TOGGLE)).is_err());
    });

    // Focus the local tile: Bob sits in the bottom strip and his camera decodes.
    let _ = crate::call::update(&mut app, Event::Focus(Some(0)));
    let (_, _, before_tiles, _) = crate::call::stage::stage(&app);
    assert!(before_tiles[1].width > 0.0 && before_tiles[1].height > 0.0);
    assert!(bob_feed.shown_at().is_some(), "strip camera decodes");
    assert!(on_stage(&app, "Bob"), "Bob shows in the strip");

    let mut shown_toggle = None;
    interact(&mut app, |ui| {
        let target = ui
            .find(Id::new(MEMBERS_TOGGLE))
            .expect("Hide Members button");
        let bounds = target.visible_bounds().expect("in view");
        assert_eq!(bounds.size(), Size::new(60.0, 32.0));
        let tip_area = Rectangle::new(
            Point::new(bounds.x - 40.0, bounds.y - 44.0),
            Size::new(140.0, 38.0),
        );
        assert!(
            super::hover_draws(ui, bounds.center(), tip_area),
            "Hide Members tooltip"
        );
        shown_toggle = Some(bounds);
    });
    interact(&mut app, |ui| {
        ui.click(Id::new(MEMBERS_TOGGLE))
            .expect("click Hide Members");
    });

    assert!(crate::call::members_hidden(&app));
    let (_, _, after_tiles, _) = crate::call::stage::stage(&app);
    assert_eq!(after_tiles[1].size(), Size::ZERO);
    assert_eq!(
        bob_feed.shown_at(),
        None,
        "hidden participant camera stops decoding"
    );
    assert!(after_tiles[0].height > before_tiles[0].height);
    assert!(after_tiles[0].width > before_tiles[0].width);
    assert!(!on_stage(&app, "Bob"), "Bob is hidden from the stage");

    interact(&mut app, |ui| {
        let target = ui
            .find(Id::new(MEMBERS_TOGGLE))
            .expect("Show Members button");
        let bounds = target.visible_bounds().expect("in view");
        assert_eq!(bounds.size(), Size::new(60.0, 32.0));
        assert!(
            bounds.y > shown_toggle.unwrap().y,
            "Show Members sits just above the controls"
        );
        let tip_area = Rectangle::new(
            Point::new(bounds.x - 40.0, bounds.y - 44.0),
            Size::new(140.0, 38.0),
        );
        assert!(
            super::hover_draws(ui, bounds.center(), tip_area),
            "Show Members tooltip"
        );
    });
    interact(&mut app, |ui| {
        ui.click(Id::new(MEMBERS_TOGGLE))
            .expect("click Show Members");
    });

    assert!(!crate::call::members_hidden(&app));
    let (_, _, restored_tiles, _) = crate::call::stage::stage(&app);
    assert_eq!(restored_tiles, before_tiles);
    assert!(
        bob_feed.shown_at().is_some(),
        "participant camera resumes decoding"
    );
    assert!(on_stage(&app, "Bob"), "Bob returns to the strip");

    // The channel keeps its choice after the call is left, as Discord's
    // client keeps it while it runs.
    let _ = crate::call::update(&mut app, Event::ToggleMembers);
    assert!(crate::call::members_hidden(&app));
    let room = app.client.state().voice.room.clone().expect("in a call");
    let _ = crate::call::update(&mut app, Event::Leave);
    assert!(app.call.members_hidden.contains(&room));
}

/// The picture's own window: with the members hidden, the picture takes the
/// whole window; shown, it stands in on the call's ground with the call's
/// members in a strip under it, the controls under them, and the chevron over
/// its top. What the window shows large is left out of the strip.
#[test]
fn the_pictures_window_puts_the_members_strip_between_the_picture_and_the_controls() {
    use crate::call::stage::MEMBERS_TOGGLE;
    use iced::widget::Id;
    use iced_test::selector::Candidate;

    let (mut app, _folder, _) = in_call();
    super::change(&mut app, |state| {
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.user = "Bob".into();
        bob.local = false;
        bob.video = None;
        bob.streaming = false;
        state.voice.participants.push(bob);
        let mut cy = state.voice.participants[0].clone();
        cy.id = "preview-cy".into();
        cy.person = "cy".into();
        cy.user = "Cy".into();
        cy.local = false;
        state.voice.participants.push(cy);
    });
    app.shell.frame = Size::new(1400.0, 760.0);
    crate::window_bar::fit_window(&mut app);
    crate::call::stage::place_videos(&mut app);
    // The window of its own, on the local camera.
    let _ = crate::call::update(&mut app, Event::PopOut("preview-you".into()));
    let window = Size::new(960.0, 540.0);
    // Where something of the window's own shows, or `None` while it isn't in
    // it: the chevron, or a member's name in the strip.
    let shown = |app: &App, looking: Looking<'_>| {
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            window,
            crate::call::popout::view(app),
        );
        match looking {
            Looking::Toggle => ui
                .find(Id::new(MEMBERS_TOGGLE))
                .ok()
                .and_then(|target| target.visible_bounds()),
            Looking::Name(name) => {
                let mut found = None;
                let _ = ui.find(|candidate: Candidate<'_>| {
                    if let Candidate::Text {
                        content,
                        visible_bounds: Some(bounds),
                        ..
                    } = candidate
                        && content == name
                    {
                        found = Some(bounds);
                    }
                    None::<()>
                });
                found
            }
        }
    };
    enum Looking<'a> {
        Toggle,
        Name(&'a str),
    }

    let toggle = shown(&app, Looking::Toggle).expect("the chevron");
    assert_eq!(toggle.size(), Size::new(60.0, 32.0));
    assert_eq!(
        toggle.x + toggle.width / 2.0,
        window.width / 2.0,
        "it is centred"
    );
    // The strip lies between the picture and the controls under it, holding
    // the others: what the window shows large is no tile of its own.
    let controls = window.height - crate::call::stage::CONTROLS_HEIGHT;
    let bob = shown(&app, Looking::Name("Bob")).expect("Bob in the strip");
    assert!(
        bob.y > toggle.y + toggle.height,
        "the chevron is over the strip, not over the picture: {toggle:?} {bob:?}"
    );
    assert!(
        bob.y > window.height / 2.0 && bob.y < controls,
        "Bob sits in the strip under the picture and over the controls: {bob:?}"
    );
    assert!(
        bob.x > 200.0,
        "the strip is centred, not glued to the window's edge: {bob:?}"
    );
    assert!(
        shown(&app, Looking::Name("You")).is_none(),
        "the picture isn't in its own strip"
    );

    // The picture stands in on the call's ground, not on a black box of its
    // own: what it hasn't covered yet is the theme's stage, never black.
    let (width, _, pixels) = {
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            window,
            crate::call::popout::view(&app),
        );
        super::drawn(&mut ui)
    };
    let stage = crate::theme::rgb(app.theme.stage);
    let expected = [
        ((stage >> 16) & 0xff) as u8,
        ((stage >> 8) & 0xff) as u8,
        (stage & 0xff) as u8,
    ];
    // The middle of the picture's room, over the strip and its chevron.
    let index = ((100 * 2 * width as usize) + (window.width as usize / 2) * 2) * 4;
    let pixel = &pixels[index..index + 3];
    assert!(
        pixel
            .iter()
            .zip(expected)
            .all(|(drawn, want)| drawn.abs_diff(want) <= 2),
        "the picture's room is the stage's ground {expected:?}, drawn {pixel:?}"
    );

    let messages: Vec<crate::app::Message> = {
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            window,
            crate::call::popout::view(&app),
        );
        ui.click(Id::new(MEMBERS_TOGGLE))
            .expect("click the chevron");
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = app.update(message);
    }
    assert!(crate::call::members_hidden(&app), "the chevron hides them");

    // The block slides: it starts where it was, nothing of it showing, and a
    // fifth of a second brings it whole — opening, or closing.
    let half = |opening| crate::call::Reveal {
        start: std::time::Instant::now() - std::time::Duration::from_millis(80),
        opening,
    };
    let done = |opening| crate::call::Reveal {
        start: std::time::Instant::now(),
        opening,
    };
    app.call.popout_reveal = Some(half(true));
    let sliding = crate::call::popout_revealed(&app);
    assert!(
        sliding > 0.0 && sliding < 1.0,
        "the block is on its way in: {sliding}"
    );
    app.call.popout_reveal = Some(half(false));
    let closing = crate::call::popout_revealed(&app);
    assert!(
        (sliding + closing - 1.0).abs() < 0.02,
        "out the way it came in: in {sliding}, out {closing}"
    );
    app.call.popout_reveal = Some(done(true));
    assert!(
        crate::call::popout_revealed(&app) < 0.01,
        "a fresh slide is shut"
    );
    app.call.popout_reveal = Some(done(false));
    assert!(
        crate::call::popout_revealed(&app) > 0.99,
        "a fresh slide out leaves the picture whole"
    );

    // Settled, the picture has the whole window again, and the chevron comes
    // over the controls under the pointer.
    app.call.popout_reveal = None;
    assert!(
        shown(&app, Looking::Name("Bob")).is_none(),
        "no strip without the members"
    );
    {
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            window,
            crate::call::popout::view(&app),
        );
        let at = Point::new(window.width / 2.0, window.height / 2.0);
        ui.point_at(at);
        ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
            position: at,
        })]);
        let foot = ui
            .find(Id::new(MEMBERS_TOGGLE))
            .expect("the chevron over the controls")
            .visible_bounds()
            .expect("in view");
        assert_eq!(foot.size(), Size::new(60.0, 32.0));
        assert!(
            foot.y + foot.height < window.height - 40.0 && foot.y > window.height / 2.0,
            "the chevron sits over the controls at the picture's foot: {foot:?}"
        );
    }
}

/// A call tile's name takes its person's display name style's font, as
/// the call's voice channel lists them by user id; anyone else's is bold.
#[test]
fn a_call_tiles_name_takes_its_persons_style_font() {
    use std::sync::Arc;
    use yapfast_core::chat::{NameStyle, VoicePerson};
    let (mut app, _folder) = preview();
    super::change(&mut app, |state| {
        state.voice.room = Some("party-voice".into());
        let party = state
            .rooms
            .iter_mut()
            .find(|room| room.id == "party-voice")
            .expect("the preview's Party");
        Arc::make_mut(party).people = vec![VoicePerson {
            id: "riley".into(),
            name: "Riley".into(),
            style: Some(Arc::new(NameStyle::new(12, 2, &[0xff00ff]))),
            ..Default::default()
        }];
    });
    let riley = crate::call::stage::call_name_font(&app, "riley");
    assert_eq!(riley.family, iced::font::Family::Name("Tempo"));
    let other = crate::call::stage::call_name_font(&app, "sam");
    assert_eq!(other, crate::fonts::BOLD);
}

/// Up to nine people fit the stage under the preview's line and over the
/// controls: every one's name shows whole, none cut off at the stage's foot
/// or behind the controls, in a small window and a large one.
#[test]
fn everyone_in_a_call_of_nine_shows_by_name_over_the_controls() {
    let names = ["Ada", "Bea", "Cy", "Dot", "Eli", "Fay", "Gus", "Hal", "Ivy"];
    for (width, height) in [(900.0, 640.0), (1120.0, 760.0), (1440.0, 900.0)] {
        let (mut app, _folder, _) = in_call();
        app.shell.frame = Size::new(width, height);
        crate::window_bar::fit_window(&mut app);
        super::change(&mut app, |state| {
            let voice = &mut state.voice;
            let mut template = voice.participants[0].clone();
            template.local = false;
            template.video = None;
            template.streaming = false;
            voice.participants.truncate(1);
            for (index, name) in names.iter().enumerate().skip(1) {
                let mut person = template.clone();
                person.id = format!("person-{index}");
                person.person = person.id.clone();
                person.user = (*name).to_owned();
                voice.participants.push(person);
            }
        });
        let local = app.client.state().voice.participants[0].user.clone();
        for name in names.iter().skip(1).chain([&local.as_str()]) {
            assert!(shows(&app, name), "{name} in {width}x{height}");
        }
    }
}

/// The picture's window keeps itself over the others from the pin at its
/// corner, and goes over the whole screen from its own button beside it,
/// which changes its icon with what it does.
#[test]
fn the_pictures_window_keeps_on_top_and_goes_full_screen() {
    let (mut app, _folder, _) = in_call();
    let _ = crate::call::update(&mut app, Event::PopOut("preview-you".into()));
    // The whole window, so the picture's button sits beside the pin.
    let _ = crate::call::update(&mut app, Event::ToggleMembers);
    app.call.popout_reveal = None;
    assert!(crate::call::members_hidden(&app));
    assert!(app.call.popout.as_ref().expect("a window").on_top);

    let window = Size::new(960.0, 540.0);
    let press = |app: &mut App, at: Point| {
        let messages: Vec<crate::app::Message> = {
            let mut ui = iced_test::Simulator::with_size(
                iced::Settings::default(),
                window,
                crate::call::popout::view(app),
            );
            ui.point_at(at);
            let _ = ui.simulate([
                iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )),
            ]);
            ui.into_messages().collect()
        };
        for message in messages {
            let _ = app.update(message);
        }
    };
    // Over the whole window the two round buttons sit in its corner, the pin
    // first and the picture's own at the corner itself.
    let round = crate::call::popout::BUTTON;
    let corner = crate::call::popout::CORNER;
    let foot = window.height - corner - round / 2.0;
    let pin = Point::new(window.width - corner - round - corner - round / 2.0, foot);
    let over = Point::new(window.width - corner - round / 2.0, foot);
    press(&mut app, pin);
    assert!(!app.call.popout.as_ref().expect("a window").on_top);
    press(&mut app, pin);
    assert!(app.call.popout.as_ref().expect("a window").on_top);
    assert!(
        !app.call.popout.as_ref().expect("a window").full_screen,
        "the pin is the first of the two"
    );

    press(&mut app, over);
    assert!(app.call.popout.as_ref().expect("a window").full_screen);
    press(&mut app, over);
    assert!(!app.call.popout.as_ref().expect("a window").full_screen);

    // With the members shown the picture stands in from the window's edge and
    // its own button goes with it, at its bottom right corner: the pin stays
    // at the window's own.
    let _ = crate::call::update(&mut app, Event::ToggleMembers);
    app.call.popout_reveal = None;
    assert!(!crate::call::members_hidden(&app), "the members in");
    let margin = crate::call::popout::MARGIN;
    let block = crate::call::stage::members_block_height(&app, true);
    let room = Size::new(
        window.width - 2.0 * margin,
        window.height - block - 2.0 * margin,
    );
    let own = Point::new(
        margin + room.width - corner - round / 2.0,
        margin + room.height - corner - round / 2.0,
    );
    press(&mut app, own);
    assert!(
        app.call.popout.as_ref().expect("a window").full_screen,
        "the picture's own button, at its corner"
    );
    press(&mut app, own);
    assert!(!app.call.popout.as_ref().expect("a window").full_screen);
}

/// The picture's own window takes what it shows with it: its frames are made
/// for that window, and its tile leaves the call for as long as it shows it
/// there.
#[test]
fn a_picture_in_its_own_window_takes_its_frames_with_it() {
    let (mut app, _folder, _) = in_call();
    let feed = yapfast_core::picture::Feed::new();
    let camera = feed.clone();
    super::change(&mut app, move |state| {
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.user = "Bob".into();
        bob.local = false;
        bob.streaming = false;
        bob.video = Some(camera);
        state.voice.participants.push(bob);
        state.voice.sharing = Some(voice::Sharing {
            phase: voice::SharePhase::Live,
            frames: std::sync::Arc::new(yapfast_core::frames::Frames::create(16, 16).unwrap()),
            viewers: vec![],
        });
    });
    app.shell.frame = Size::new(1400.0, 760.0);
    crate::window_bar::fit_window(&mut app);
    crate::call::stage::place_videos(&mut app);
    assert!(
        feed.shown_at().is_some(),
        "his camera decodes on the stage while it shows there"
    );
    let shares_tile = |app: &App| {
        crate::call::stage::stage(app)
            .0
            .contains(&crate::call::stage::Slot::OwnScreen)
    };
    assert!(shares_tile(&app), "the share has its tile");

    // His camera goes to the window of its own, at that window's size, and
    // no frame of it is asked for by the stage.
    let _ = crate::call::update(&mut app, Event::PopOut("preview-bob".into()));
    assert_eq!(
        feed.shown_at(),
        Some((
            (crate::call::popout::WIDE * app.device_scale()).round() as u32,
            (crate::call::popout::WIDE * 9.0 / 16.0 * app.device_scale()).round() as u32
        )),
        "made for the window it shows in"
    );

    // The share leaves the call whole: its tile is gone while it shows there.
    let _ = crate::call::update(
        &mut app,
        Event::PopOut(crate::call::stage::OWN_SCREEN.into()),
    );
    assert!(!shares_tile(&app), "nothing of the share stays in the call");
    app.call.popout = None;
    crate::call::stage::place_videos(&mut app);
    assert!(shares_tile(&app), "back in the call");
    assert!(feed.shown_at().is_some(), "his camera back on the stage");
}

/// Another person's share in a window of its own shows there: their tile
/// stays in the call without it, and no tile draws their stream, which the
/// window has alone.
#[test]
fn another_persons_share_in_its_own_window_leaves_the_call() {
    let (mut app, _folder, _) = in_call();
    let watched = std::sync::Arc::new(yapfast_core::frames::Frames::create(16, 16).unwrap());
    super::change(&mut app, move |state| {
        let mut ana = state.voice.participants[0].clone();
        ana.id = "streaming-ana".into();
        ana.person = "ana".into();
        ana.user = "Ana".into();
        ana.local = false;
        ana.streaming = true;
        ana.video = None;
        state.voice.participants.push(ana);
        state.voice.watching = Some(Box::new(voice::Watching {
            user: "streaming-ana".into(),
            name: "Ana".into(),
            opening: false,
            frames: Some(watched),
            viewers: Vec::new(),
        }));
    });
    app.shell.frame = Size::new(1400.0, 760.0);
    crate::window_bar::fit_window(&mut app);
    crate::call::stage::place_videos(&mut app);
    let slots = |app: &App| crate::call::stage::stage(app).0;
    assert!(!slots(&app).contains(&crate::call::stage::Slot::Stream(1)));
    assert!(!crate::call::stage::popped_out(
        &app,
        "stream:streaming-ana"
    ));

    // Out into a window of its own, and there and only there: her tile keeps
    // her picture, and no window of the call has the stream.
    let _ = crate::call::update(&mut app, Event::PopOut("stream:streaming-ana".into()));
    assert!(crate::call::stage::popped_out(&app, "stream:streaming-ana"));
    assert_eq!(
        slots(&app),
        [
            crate::call::stage::Slot::Person(0),
            crate::call::stage::Slot::Person(1)
        ],
        "her tile stays, her stream has none"
    );
    let voice = &app.client.state().voice;
    let stage = crate::call::stage::Stage::of(voice, Some("stream:streaming-ana"));
    assert!(
        !stage.stream_in_tile(&voice.participants[1]),
        "nothing of her stream is drawn in the call"
    );
}

/// The picture's own window: its strip is the window's own switcher. A tile
/// pressed shows that picture in the window itself, which stays where it
/// stands and as it is, and the call's stage behind it is left alone.
#[test]
fn a_press_in_the_pictures_window_shows_that_picture_there() {
    use iced::mouse;

    let (mut app, _folder, _) = in_call();
    let camera = yapfast_core::picture::Feed::new();
    super::change(&mut app, move |state| {
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.user = "Bob".into();
        bob.local = false;
        bob.video = Some(camera);
        state.voice.participants.push(bob);
    });
    app.shell.frame = Size::new(1400.0, 760.0);
    crate::window_bar::fit_window(&mut app);
    crate::call::stage::place_videos(&mut app);
    let _ = crate::call::update(&mut app, Event::PopOut("preview-you".into()));
    let window = Size::new(960.0, 540.0);
    let id = app.call.popout.as_ref().expect("a window").id;
    let bob = {
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            window,
            crate::call::popout::view(&app),
        );
        super::last(&mut ui, "Bob")
    };
    let messages: Vec<crate::app::Message> = {
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            window,
            crate::call::popout::view(&app),
        );
        ui.point_at(bob);
        let _ = ui.simulate([
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ]);
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = app.update(message);
    }
    let popout = app.call.popout.as_ref().expect("a window");
    assert_eq!(popout.id, id, "the same window, not another");
    assert_eq!(popout.key, "preview-bob", "Bob's picture in it");
    assert_eq!(
        app.title(id),
        "Yapfast",
        "the window is Yapfast's own, named so"
    );
    assert!(app.call.focus.is_none(), "the call's stage left alone");
}

/// A tile of the picture's own window pressed with the right button opens
/// that person's menu there, in that window, over the picture; Yapfast's own
/// window shows none of it.
#[test]
fn the_pictures_window_opens_a_members_menu_over_its_picture() {
    use iced::mouse;

    let (mut app, _folder, _) = in_call();
    let camera = yapfast_core::picture::Feed::new();
    super::change(&mut app, move |state| {
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.user = "Bob".into();
        bob.local = false;
        bob.video = Some(camera);
        state.voice.participants.push(bob);
    });
    app.shell.frame = Size::new(1400.0, 760.0);
    crate::window_bar::fit_window(&mut app);
    crate::call::stage::place_videos(&mut app);
    let _ = crate::call::update(&mut app, Event::PopOut("preview-you".into()));
    let window = Size::new(960.0, 540.0);
    let id = app.call.popout.as_ref().expect("a window").id;
    let bob = {
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            window,
            crate::call::popout::view(&app),
        );
        super::last(&mut ui, "Bob")
    };
    let messages: Vec<crate::app::Message> = {
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            window,
            crate::call::popout::view(&app),
        );
        ui.point_at(bob);
        let _ = ui.simulate([iced::Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Right,
        ))]);
        ui.into_messages().collect()
    };
    assert!(!messages.is_empty(), "the press says something");
    for message in messages {
        let _ = app.update(message);
    }
    let menu = app.win.menu.as_ref().expect("a menu");
    assert_eq!(
        menu.window.map(|(window, _)| window),
        Some(id),
        "opened in the window's own"
    );
    // Its volumes over its rows: what a call in a server lets them do may
    // leave it none, as the fake has no server to ask.
    let label = menu
        .rows
        .first()
        .map_or_else(|| "User volume".to_owned(), |row| row.label.to_string());
    let mut ui = iced_test::Simulator::with_size(
        iced::Settings::default(),
        window,
        crate::call::popout::view(&app),
    );
    assert!(ui.find(label.as_str()).is_ok(), "{label:?} in the window");
    assert!(
        !super::present(&app, &label),
        "{label:?} nowhere in Yapfast's own window"
    );
}

/// A picture in a window of its own takes the call out of Yapfast's window:
/// no panel, no stage, no controls and no corner picture, and closing that
/// window brings them all back.
#[test]
fn the_call_leaves_yapfasts_window_while_a_picture_has_one() {
    let (mut app, _folder, _) = in_call();
    app.shell.frame = Size::new(1400.0, 760.0);
    crate::window_bar::fit_window(&mut app);
    crate::call::stage::place_videos(&mut app);
    assert!(
        crate::call::stage::voice_panel(&app),
        "the call is the view"
    );
    assert!(crate::call::stage::stage_shown(&app));

    let _ = crate::call::update(&mut app, Event::PopOut("preview-you".into()));
    assert!(crate::call::in_window(&app), "a window of its own");
    assert!(
        !crate::call::stage::voice_panel(&app),
        "nothing of the call in Yapfast's window"
    );
    assert!(!crate::call::stage::stage_shown(&app));
    assert_eq!(
        crate::call::stage::corner(&app),
        crate::call::stage::Pip::None,
        "no corner picture either"
    );
    assert!(!crate::call::stage::full_shown(&app), "and no full screen");

    // Closed, the call is Yapfast's own again.
    crate::call::popout::closed(&mut app);
    assert!(!crate::call::in_window(&app));
    assert!(crate::call::stage::voice_panel(&app));
    assert!(crate::call::stage::stage_shown(&app));
}

/// A tile of the call's own stage sends its picture out too, from the button
/// beside its full screen one: their share, their stream or their camera.
#[test]
fn a_tile_of_the_call_sends_its_picture_into_a_window() {
    use crate::call::stage::Slot;

    let (mut app, _folder, _) = in_call();
    let frames = std::sync::Arc::new(yapfast_core::frames::Frames::create(16, 16).unwrap());
    let camera = yapfast_core::picture::Feed::new();
    super::change(&mut app, move |state| {
        state.voice.sharing = Some(voice::Sharing {
            phase: voice::SharePhase::Live,
            frames,
            viewers: Vec::new(),
        });
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.user = "Bob".into();
        bob.local = false;
        bob.streaming = false;
        bob.video = Some(camera);
        state.voice.participants.push(bob);
    });
    crate::call::stage::place_videos(&mut app);
    let _ = crate::call::update(&mut app, Event::PopoutSlot(Slot::OwnScreen));
    let popout = app.call.popout.as_ref().expect("a window");
    assert_eq!(popout.key, crate::call::stage::OWN_SCREEN, "this share");
    // Another tile takes the window's place, the window as it is.
    let id = popout.id;
    let _ = crate::call::update(&mut app, Event::PopoutSlot(Slot::Person(1)));
    let popout = app.call.popout.as_ref().expect("a window");
    assert_eq!(popout.id, id, "the same window");
    assert_eq!(popout.key, "preview-bob", "Bob's camera");
}

/// The window's own buttons over the whole window are under the pointer, as
/// the controls are: none while it is away, both while it is on the window.
#[test]
fn the_whole_windows_buttons_show_under_the_pointer() {
    use crate::call::popout::{BUTTON, CORNER};

    let (mut app, _folder, _) = in_call();
    let _ = crate::call::update(&mut app, Event::PopOut("preview-you".into()));
    let _ = crate::call::update(&mut app, Event::ToggleMembers);
    app.call.popout_reveal = None;
    let window = Size::new(960.0, 540.0);
    let pin = Point::new(
        window.width - CORNER - BUTTON - CORNER - BUTTON / 2.0,
        window.height - CORNER - BUTTON / 2.0,
    );
    // The pin's middle, as drawn with the pointer at `cursor`.
    let at = |app: &App, cursor: Point| {
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            window,
            crate::call::popout::view(app),
        );
        ui.point_at(cursor);
        let _ = ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
            position: cursor,
        })]);
        let (width, _, pixels) = super::drawn(&mut ui);
        let index =
            ((pin.y.round() as usize * 2) * width as usize + pin.x.round() as usize * 2) * 4;
        pixels[index..index + 3].to_vec()
    };
    let away = at(&app, Point::new(-40.0, -40.0));
    let on = at(&app, Point::new(480.0, 400.0));
    assert!(
        away.iter().all(|channel| *channel < 12),
        "nothing over the picture with the pointer away: {away:?}"
    );
    assert_ne!(away, on, "the button comes with the pointer");
}

/// The strip under a picture's window is a stage of its own: the pictures in
/// it are asked for and drawn there, at a tile's size, while the call's stage
/// behind Yapfast's window asks for none.
#[test]
fn the_pictures_window_asks_for_the_pictures_in_its_strip() {
    let (mut app, _folder, _) = in_call();
    let feed = yapfast_core::picture::Feed::new();
    let camera = feed.clone();
    super::change(&mut app, move |state| {
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.user = "Bob".into();
        bob.local = false;
        bob.video = Some(camera);
        state.voice.participants.push(bob);
    });
    app.shell.frame = Size::new(1400.0, 760.0);
    crate::window_bar::fit_window(&mut app);
    crate::call::stage::place_videos(&mut app);
    assert!(feed.shown_at().is_some(), "his camera on the stage");

    // The picture out, and his camera is a tile of the strip under it.
    let _ = crate::call::update(&mut app, Event::PopOut("preview-you".into()));
    assert!(crate::call::in_window(&app));
    assert!(
        !crate::call::stage::stage_shown(&app),
        "no call in Yapfast's window"
    );
    assert!(
        feed.shown_at().is_some(),
        "his camera is still drawn, in the window's strip"
    );

    // The window gone, and it is the stage's again.
    crate::call::popout::closed(&mut app);
    assert!(feed.shown_at().is_some());
}

/// The wheel over the picture brings it closer about the pointer, and down
/// lets it back to its room: never off it.
#[test]
fn the_wheel_brings_the_pictures_window_closer_about_the_pointer() {
    let (mut app, _folder, _) = in_call();
    super::change(&mut app, move |state| {
        state.voice.sharing = Some(voice::Sharing {
            phase: voice::SharePhase::Live,
            frames: std::sync::Arc::new(yapfast_core::frames::Frames::create(16, 16).unwrap()),
            viewers: Vec::new(),
        });
    });
    crate::call::stage::place_videos(&mut app);
    let _ = crate::call::update(
        &mut app,
        Event::PopOut(crate::call::stage::OWN_SCREEN.into()),
    );
    // Its frames come at the share's shape, so it fills the window's room.
    app.video
        .showed(crate::call::stage::OWN_SCREEN, (1600, 900));
    let _ = crate::call::update(&mut app, Event::ToggleMembers);
    app.call.popout_reveal = None;
    assert!(crate::call::members_hidden(&app), "the picture whole");

    let window = Size::new(960.0, 540.0);
    let turn = |app: &mut App, notches: f32, at: Point| {
        let messages: Vec<crate::app::Message> = {
            let mut ui = iced_test::Simulator::with_size(
                iced::Settings::default(),
                window,
                crate::call::popout::view(app),
            );
            ui.point_at(at);
            let _ = ui.simulate([iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                delta: iced::mouse::ScrollDelta::Lines { x: 0.0, y: notches },
            })]);
            ui.into_messages().collect()
        };
        assert!(!messages.is_empty(), "the wheel over the picture is heard");
        for message in messages {
            let _ = app.update(message);
        }
    };
    // A notch up with the pointer in the picture's top left quarter: it comes
    // closer, keeping that quarter's place under the pointer, and its middle
    // moves up and left with it.
    let at = Point::new(240.0, 135.0);
    turn(&mut app, 1.0, at);
    let zoom = app.call.zoom;
    assert!(zoom.by > 1.0, "closer: {}", zoom.by);
    assert!(
        (zoom.pan.x - 36.0).abs() < 0.1 && (zoom.pan.y - 20.25).abs() < 0.1,
        "the pointer's place on the picture stays there: {:?}",
        zoom.pan
    );

    // Every notch adds, and none takes it further than the room shows it
    // whole: its middle stays where the picture still covers the room.
    for _ in 0..20 {
        turn(&mut app, 1.0, at);
    }
    let zoom = app.call.zoom;
    assert!((zoom.by - 4.0).abs() < 0.01, "as close as it comes");
    assert!(
        zoom.pan.x.abs() <= (960.0 * zoom.by - 960.0) / 2.0 + 0.1
            && zoom.pan.y.abs() <= (540.0 * zoom.by - 540.0) / 2.0 + 0.1,
        "never off its room: {:?}",
        zoom.pan
    );

    // Down lets it back, and at its room's size it stops.
    for _ in 0..40 {
        turn(&mut app, -1.0, at);
    }
    let zoom = app.call.zoom;
    assert_eq!((zoom.by, zoom.pan), (1.0, iced::Vector::ZERO), "as it came");

    // The window gone, and the next picture starts as it comes too.
    turn(&mut app, 1.0, at);
    assert!(app.call.zoom.by > 1.0);
    crate::call::popout::closed(&mut app);
    assert_eq!(app.call.zoom.by, 1.0, "fitted again");
}

/// Only a picture shown large is the wheel's: a tile of the call's grid comes
/// as it is and the stage scrolls, a large tile comes closer, over the whole
/// window too, and comes as it was again as the tile changes.
#[test]
fn only_a_picture_shown_large_takes_the_wheel() {
    use crate::call::stage::Slot;

    let (mut app, _folder, _) = in_call();
    super::change(&mut app, move |state| {
        state.voice.sharing = Some(voice::Sharing {
            phase: voice::SharePhase::Live,
            frames: std::sync::Arc::new(yapfast_core::frames::Frames::create(16, 16).unwrap()),
            viewers: Vec::new(),
        });
        let mut bob = state.voice.participants[0].clone();
        bob.id = "preview-bob".into();
        bob.person = "bob".into();
        bob.user = "Bob".into();
        bob.local = false;
        state.voice.participants.push(bob);
    });
    app.shell.frame = Size::new(1400.0, 760.0);
    crate::window_bar::fit_window(&mut app);
    crate::call::stage::place_videos(&mut app);
    app.video
        .showed(crate::call::stage::OWN_SCREEN, (1600, 900));
    assert!(crate::call::stage::stage_shown(&app), "the call shows");
    let place = crate::call::stage::stage(&app)
        .0
        .iter()
        .position(|slot| *slot == Slot::OwnScreen)
        .expect("the share's tile");
    let centre = |app: &App| {
        let (_, focused, tiles, _) = crate::call::stage::stage(app);
        let place = focused.unwrap_or(place);
        tiles[place].center()
    };
    let turn = |app: &mut App| {
        let at = centre(app);
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            app.window,
            crate::call::stage::stage_view(app),
        );
        ui.point_at(at);
        let _ = ui.simulate([iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
            delta: iced::mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
        })]);
        for message in ui.into_messages() {
            let _ = app.update(message);
        }
    };
    // The grid: a tile comes as it is, and the wheel scrolls it.
    super::interact(&mut app, |_| {});
    turn(&mut app);
    assert_eq!(
        app.call.zoom.by, 1.0,
        "a tile of the grid comes as it is: the stage scrolls"
    );

    // Large, and it is the wheel's.
    let _ = crate::call::update(&mut app, Event::Focus(Some(place)));
    super::interact(&mut app, |_| {});
    turn(&mut app);
    assert!(app.call.zoom.by > 1.0, "shown large it comes closer");
    // Over the whole window too, fitted again as the tile goes.
    let _ = crate::call::update(&mut app, Event::Full(true));
    assert_eq!(app.call.zoom.by, 1.0, "as it comes again");
    super::interact(&mut app, |_| {});
    turn(&mut app);
    assert!(app.call.zoom.by > 1.0, "shown whole it comes closer");
    let _ = crate::call::update(&mut app, Event::Full(false));
    assert_eq!(app.call.zoom.by, 1.0, "as it comes again");
    let _ = crate::call::update(&mut app, Event::Focus(None));
    assert_eq!(app.call.zoom.by, 1.0, "and another tile comes as it is");
}

/// A picture brought closer is the same box on its ground with its frame
/// larger within it: the bars beside a share of another shape stay, and what
/// stood at the picture's edge has moved out of the box.
#[test]
fn a_picture_brought_closer_stays_in_its_box() {
    let (mut app, _folder, _) = in_call();
    super::change(&mut app, move |state| {
        state.voice.sharing = Some(voice::Sharing {
            phase: voice::SharePhase::Live,
            frames: std::sync::Arc::new(yapfast_core::frames::Frames::create(16, 16).unwrap()),
            viewers: Vec::new(),
        });
    });
    crate::call::stage::place_videos(&mut app);
    let _ = crate::call::update(
        &mut app,
        Event::PopOut(crate::call::stage::OWN_SCREEN.into()),
    );
    // A 4:3 share, its middle half white and the rest black, in a 16:9
    // window: 720 of its 960 px across, bars beside it.
    app.video
        .showed_striped(crate::call::stage::OWN_SCREEN, (1600, 1200));
    let _ = crate::call::update(&mut app, Event::ToggleMembers);
    app.call.popout_reveal = None;
    assert!(crate::call::members_hidden(&app), "the picture whole");

    let window = Size::new(960.0, 540.0);
    let drawn = |app: &App| {
        let (width, _height, pixels) = {
            let mut ui = iced_test::Simulator::with_size(
                iced::Settings::default(),
                window,
                crate::call::popout::view(app),
            );
            super::drawn(&mut ui)
        };
        // Twice the window's size, and one colour in RGB.
        let at = |x: f32, y: f32| {
            let index = ((y as u32 * 2) * width + x as u32 * 2) as usize * 4;
            u16::from(pixels[index]) + u16::from(pixels[index + 1]) + u16::from(pixels[index + 2])
        };
        // The bar at the left, the picture a quarter in from its box, and its
        // middle, all on the window's middle row.
        (at(2.0, 270.0), at(250.0, 270.0), at(400.0, 270.0))
    };
    // Fitted, the white middle half stands 300 px in, and the bar is the
    // ground's.
    let (bar, left, middle) = drawn(&app);
    assert!(bar < 120, "a bar beside the picture: {bar}");
    assert!(left < 120, "the picture's black quarter: {left}");
    assert!(middle > 700, "its white half: {middle}");

    // Closer, and the white half reaches where its black quarter stood: the
    // frame within its box is larger, and the box and its bar are as they
    // were.
    let _ = drawn(&app);
    let _ = crate::call::update(&mut app, Event::ZoomTo(2.0));
    let (bar, left, middle) = drawn(&app);
    assert!(bar < 120, "the bar stays beside it: {bar}");
    assert!(left > 700, "its white half is larger: {left}");
    assert!(middle > 700, "the picture in its box: {middle}");
}

/// A picture brought closer stays in its room: it's cut at its edge, and the
/// ground about it, where the window stands it in with the members shown,
/// shows as it did.
#[test]
fn a_picture_brought_closer_stays_in_its_room() {
    let (mut app, _folder, _) = in_call();
    super::change(&mut app, move |state| {
        state.voice.sharing = Some(voice::Sharing {
            phase: voice::SharePhase::Live,
            frames: std::sync::Arc::new(yapfast_core::frames::Frames::create(16, 16).unwrap()),
            viewers: Vec::new(),
        });
    });
    crate::call::stage::place_videos(&mut app);
    let _ = crate::call::update(
        &mut app,
        Event::PopOut(crate::call::stage::OWN_SCREEN.into()),
    );
    app.video
        .showed(crate::call::stage::OWN_SCREEN, (1600, 900));
    app.call.popout_reveal = None;
    assert!(!crate::call::members_hidden(&app), "the members shown");

    let window = Size::new(960.0, 540.0);
    let drawn = |app: &App| {
        let (width, _height, pixels) = {
            let mut ui = iced_test::Simulator::with_size(
                iced::Settings::default(),
                window,
                crate::call::popout::view(app),
            );
            super::drawn(&mut ui)
        };
        // Twice the window's size, and one colour in RGB.
        let at = |x: f32, y: f32| {
            let index = ((y as u32 * 2) * width + x as u32 * 2) as usize * 4;
            u16::from(pixels[index]) + u16::from(pixels[index + 1]) + u16::from(pixels[index + 2])
        };
        let middle = {
            let room = app.call.popout_seen.get().expect("its room");
            crate::call::popout::MARGIN + room.height / 2.0
        };
        (at(4.0, middle), at(window.width / 2.0, middle))
    };
    // As it comes: the picture fills its room and its margins are the
    // ground's.
    let (margin, middle) = drawn(&app);
    assert!(margin < 120, "the margin is the ground's: {margin}");
    assert!(middle > 700, "the picture in its room: {middle}");

    // Brought closer it covers its room whole, and no more.
    let _ = drawn(&app);
    let _ = crate::call::update(&mut app, Event::ZoomTo(3.0));
    let (margin, middle) = drawn(&app);
    assert!(margin < 120, "still the ground's: {margin}");
    assert!(middle > 700, "its room covered: {middle}");
}
