//! The call: the dock's noise toggle, the device menus' volumes and a
//! device gone away, the share menu in a short window, the microphone's
//! level only while its menu shows, the dock's channel line, a voice
//! channel's Join, someone's volumes and menu from their tile, a camera's
//! tile before its first frame, and the call's own order under its
//! channel.

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
        assert!(ui.find("Entire screen").is_ok());
        ui.click("Window 1").expect("a window to pick");
    });
    assert_eq!(app.call.share.as_ref().map(|share| share.pick), Some(2));
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
            width: 1920,
            height: 1080,
            fps: 60,
            target_kbps: 4000,
            sent_kbps: 3812,
            keyframe_requests: 2,
            resent: 5,
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
        "NVENC on the GPU",
        "H264",
        "1920×1080 at 60 fps",
        "3.8 Mbit/s",
        "4.0 Mbit/s",
    ] {
        assert!(shows(&app, words), "{words}");
    }
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
        server_deafened: false,
        streaming: false,
        video: false,
        elsewhere: false,
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
fn switching_with_only_the_systems_dialog_opens_it_with_no_menu() {
    use crate::call::popups::Share;
    let share = |switching, dialog, cameras: &[&str]| Share {
        screens: vec![],
        windows: vec![],
        cameras: cameras.iter().map(|&camera| camera.into()).collect(),
        sources: vec![],
        dialog,
        pick: 0,
        quality: Default::default(),
        switching,
    };
    assert!(share(true, true, &[]).dialog_alone());
    assert!(
        !share(false, true, &[]).dialog_alone(),
        "Go Live's menu has the quality"
    );
    assert!(
        !share(true, true, &["Webcam"]).dialog_alone(),
        "a camera to pick"
    );
    assert!(!share(true, false, &[]).dialog_alone());
    // Listed so while sharing, nothing opens and nothing waits.
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
    if yapfast_core::stream_worker::portal_picks() {
        assert_eq!(app.call.popup, None, "the dialog, no menu");
        assert!(app.call.share.is_none());
    } else {
        assert_eq!(app.call.popup, Some(Popup::Share));
    }
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
