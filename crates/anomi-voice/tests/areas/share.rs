//! The share menu: task V10's tests, driving `anomi_voice::share::view` over
//! its fixtures in the simulator (`super::support`).

use iced::Size;
use iced::keyboard::key::Named;
use iced_test::{Simulator, selector};

use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::voice::share as samples;
use anomi_vm::menu::Key;
use anomi_vm::voice::share::{Intent, Resolution, Share, Sound};
use anomi_voice::share::{self, built};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(336.0, 1000.0);
/// Room for the stream's menu and a submenu beside it.
const LIVE: Size = Size::new(900.0, 700.0);

fn menu(share: Share) -> Share {
    built(share)
}

fn simulate(vm: &Share) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, share::view(vm, front()))
}

/// Clicks `text` and returns what the menu asked for.
fn click(vm: &Share, text: &str) -> Vec<Intent> {
    let mut ui = simulate(vm);
    ui.click(text).unwrap();
    asked(ui)
}

/// Presses the button under the choices.
fn go(vm: &Share) -> Vec<Intent> {
    let mut ui = simulate(vm);
    let _ = ui.click(selector::id(share::GO));
    asked(ui)
}

#[test]
fn with_nothing_listed_it_picks_sound_and_quality_only() {
    let vm = menu(samples::dialog());
    let mut ui = simulate(&vm);
    for shown in ["SOUND", "RESOLUTION", "FRAME RATE", "ENCODING"] {
        assert!(ui.find(shown).is_ok(), "{shown}");
    }
    for hidden in ["SCREENS", "WINDOWS", "CAMERAS", "SCREEN", "One application"] {
        assert!(ui.find(hidden).is_err(), "{hidden}");
    }
}

#[test]
fn sources_are_listed_in_order_and_counted_from_the_screens() {
    let vm = menu(samples::listed());
    let mut ui = simulate(&vm);
    for shown in [
        "SCREENS",
        "WINDOWS",
        "SCREEN",
        "CAMERAS",
        "Screen or window",
    ] {
        assert!(ui.find(shown).is_ok(), "{shown}");
    }
    assert_eq!(click(&vm, "Screen 2"), [Intent::Source(1)]);
    assert_eq!(click(&vm, "Terminal"), [Intent::Source(2)]);
    assert_eq!(click(&vm, "Screen or window"), [Intent::Source(5)]);
    assert_eq!(click(&vm, "Webcam"), [Intent::Source(6)]);
}

#[test]
fn the_dialog_is_a_choice_only_beside_cameras() {
    assert!(menu(samples::dialog()).rows.dialog.is_none());
    assert!(menu(samples::listed()).rows.dialog.is_some());
    let no_cameras = Share {
        cameras: Vec::new(),
        ..samples::listed()
    };
    assert!(menu(no_cameras).rows.dialog.is_none());
}

#[test]
fn a_screen_alone_is_still_chosen_by_its_card() {
    let vm = menu(samples::one_screen());
    assert_eq!(click(&vm, "Screen 1"), [Intent::Source(0)]);
}

#[test]
fn the_choices_ask_for_themselves() {
    let vm = menu(samples::dialog());
    assert_eq!(click(&vm, "720p"), [Intent::Resolution(Resolution::P720)]);
    assert_eq!(
        click(&vm, "Source"),
        [Intent::Resolution(Resolution::Source)]
    );
    assert_eq!(click(&vm, "60 fps"), [Intent::Fps(60)]);
    assert_eq!(click(&vm, "No sound"), [Intent::Sound(Sound::None)]);
    assert_eq!(click(&vm, "Higher quality"), [Intent::HighQuality(true)]);
}

#[test]
fn a_ticked_quality_asks_to_untick() {
    let vm = menu(Share {
        high_quality: true,
        ..samples::dialog()
    });
    assert_eq!(click(&vm, "Higher quality"), [Intent::HighQuality(false)]);
}

#[test]
fn go_live_carries_the_picks() {
    let vm = menu(Share {
        source: Some(2),
        resolution: Resolution::P720,
        fps: 60,
        high_quality: true,
        ..samples::listed()
    });
    assert_eq!(
        go(&vm),
        [Intent::GoLive {
            source: 2,
            resolution: Resolution::P720,
            fps: 60,
            high_quality: true,
        }]
    );
}

#[test]
fn one_application_opens_its_list_and_names_the_choice() {
    let closed = menu(Share {
        apps_open: false,
        sound: Sound::All,
        ..samples::apps()
    });
    assert_eq!(closed.rows.sound[2].subtitle, "Chosen by its window");
    assert_eq!(menu(samples::apps()).rows.sound[2].subtitle, "Video");
    assert_eq!(click(&closed, "One application"), [Intent::Apps(true)]);

    let open = menu(samples::apps());
    assert_eq!(click(&open, "One application"), [Intent::Apps(false)]);
    assert_eq!(
        click(&open, "Music"),
        [Intent::Sound(Sound::App(0))],
        "the list's rows choose the application"
    );
}

#[test]
fn without_windows_to_choose_from_there_is_no_application_row() {
    let vm = menu(samples::dialog());
    assert_eq!(vm.rows.sound.len(), 2);
    assert!(menu(samples::apps()).rows.sound.len() == 3);
}

/// After "Change Windows" the menu lists what to share instead: the
/// stream's menu has the quality and the sound.
#[test]
fn switching_lists_the_sources_alone_and_waits_for_a_pick() {
    let vm = menu(samples::switching());
    let mut ui = simulate(&vm);
    for shown in ["SCREENS", "WINDOWS", "CAMERAS", "Terminal"] {
        assert!(ui.find(shown).is_ok(), "{shown}");
    }
    for hidden in ["SOUND", "RESOLUTION", "FRAME RATE", "ENCODING"] {
        assert!(ui.find(hidden).is_err(), "{hidden}");
    }
    drop(ui);
    // Before going live the sound comes first, then the quality.
    let listed = menu(samples::listed());
    let mut ui = simulate(&listed);
    let sound = ui.find("SOUND").unwrap().bounds().y;
    assert!(sound < ui.find("RESOLUTION").unwrap().bounds().y);
    drop(ui);
    assert!(!vm.can_go());
    assert_eq!(vm.go(), None);
    // Disabled: pressing it asks for nothing.
    assert!(go(&vm).is_empty());
}

/// The stream's menu in a window with room for its submenus.
fn live(vm: &Share) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), LIVE, share::view(vm, front()))
}

#[test]
fn while_live_the_menu_is_the_streams_own() {
    let vm = menu(samples::live());
    let mut ui = live(&vm);
    for shown in [
        "Stream Quality",
        "Sound",
        "Change Windows",
        "Stop Streaming",
    ] {
        assert!(ui.find(shown).is_ok(), "{shown}");
    }
    // Its submenus wait for their rows; the sources wait for Change Windows.
    for hidden in ["FRAME RATE", "60 fps", "SOUND", "WINDOWS", "Switch"] {
        assert!(ui.find(hidden).is_err(), "{hidden}");
    }
    let stream = vm.live.as_ref().unwrap();
    assert_eq!(stream.quality.subtitle, "720p · 60 fps");
    assert_eq!(stream.sound.subtitle, "All applications");
    assert!(stream.stop.danger);
}

#[test]
fn its_quality_has_discords_groups_beside_it_and_marks_the_stream() {
    let vm = menu(samples::live_quality());
    let mut ui = live(&vm);
    let head = ui.find("Stream Quality").unwrap().bounds();
    let rates = ui.find("FRAME RATE").unwrap().bounds();
    assert!(rates.x > head.x + head.width, "beside the card: {rates:?}");
    assert!(rates.y < ui.find("RESOLUTION").unwrap().bounds().y);
    for shown in ["15 fps", "30 fps", "1080p", "Source", "Higher quality"] {
        assert!(ui.find(shown).is_ok(), "{shown}");
    }
    drop(ui);
    let rows = &vm.live.as_ref().unwrap().quality.submenu;
    let marked: Vec<&str> = (rows.iter())
        .filter(|row| row.radio && row.checked)
        .map(|row| row.text.as_str())
        .collect();
    assert_eq!(marked, ["60 fps", "720p"]);
    let mut ui = live(&vm);
    ui.click("1080p").unwrap();
    assert_eq!(asked(ui), [Intent::Resolution(Resolution::P1080)]);
    let mut ui = live(&vm);
    ui.click("15 fps").unwrap();
    assert_eq!(asked(ui), [Intent::Fps(15)]);
    let mut ui = live(&vm);
    ui.click("FRAME RATE").unwrap();
    assert!(asked(ui).is_empty(), "a group's title chooses nothing");
}

#[test]
fn its_last_rows_change_what_is_shared_and_stop() {
    let vm = menu(samples::live());
    let mut ui = live(&vm);
    ui.click("Change Windows").unwrap();
    assert_eq!(asked(ui).last(), Some(&Intent::Change));
    let mut ui = live(&vm);
    ui.click("Stop Streaming").unwrap();
    assert_eq!(asked(ui).last(), Some(&Intent::Stop));
    // Before it's live there is nothing to change yet.
    let starting = menu(samples::live_starting());
    let mut ui = live(&starting);
    ui.click("Change Windows").unwrap();
    assert!(!asked(ui).contains(&Intent::Change));
}

#[test]
fn its_keys_go_to_the_menu_and_escape_only_while_a_submenu_shows() {
    let vm = menu(samples::live());
    let mut ui = live(&vm);
    for key in [Named::ArrowDown, Named::ArrowRight, Named::Escape] {
        ui.tap_key(key);
    }
    assert_eq!(
        asked(ui),
        [Intent::Key(Key::Down), Intent::Key(Key::Right)],
        "Escape is the layer's"
    );
    let open = menu(samples::live_quality());
    let mut ui = live(&open);
    ui.tap_key(Named::Escape);
    assert_eq!(asked(ui), [Intent::Key(Key::Escape)]);
}

#[test]
fn switching_to_a_pick_sends_it() {
    let vm = menu(Share {
        source: Some(1),
        ..samples::switching()
    });
    assert!(vm.can_go());
    assert_eq!(go(&vm), [Intent::Switch(1)]);
}

#[test]
fn the_wider_menu_is_for_the_screens() {
    assert!(samples::listed().wide());
    assert!(!samples::dialog().wide());
}

#[test]
fn go_live_stays_clickable_in_a_short_window() {
    let vm = menu(samples::listed());
    let mut ui = Simulator::with_size(
        settings(),
        Size::new(336.0, 400.0),
        share::view(&vm, front()),
    );
    let _ = ui.click(selector::id(share::GO));
    assert_eq!(asked(ui), vm.go().into_iter().collect::<Vec<_>>());
}
