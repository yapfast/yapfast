//! The share menu: task V10's tests, driving `anomi_voice::share::view` over
//! its fixtures in the simulator (`super::support`).

use iced::Size;
use iced_test::{Simulator, selector};

use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::voice::share as samples;
use anomi_vm::voice::share::{Intent, Resolution, Share, Sound};
use anomi_voice::share::{self, built};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(336.0, 1000.0);

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

#[test]
fn switching_keeps_the_quality_and_waits_for_a_pick() {
    let vm = menu(samples::switching());
    let mut ui = simulate(&vm);
    // While live, these change the stream at once.
    for shown in [
        "SOUND",
        "RESOLUTION",
        "FRAME RATE",
        "ENCODING",
        "Higher quality",
    ] {
        assert!(ui.find(shown).is_ok(), "{shown}");
    }
    // The quality comes first while live, under the sound before.
    let top = |ui: &mut Simulator<'_, Intent, Theme, Renderer>, words: &str| {
        ui.find(words).unwrap().bounds().y
    };
    assert!(top(&mut ui, "RESOLUTION") < top(&mut ui, "SOUND"));
    drop(ui);
    let listed = menu(samples::listed());
    let mut ui = simulate(&listed);
    assert!(top(&mut ui, "SOUND") < top(&mut ui, "RESOLUTION"));
    drop(ui);
    assert_eq!(click(&vm, "60 fps"), [Intent::Fps(60)]);
    assert_eq!(click(&vm, "720p"), [Intent::Resolution(Resolution::P720)]);
    assert!(!vm.can_go());
    assert_eq!(vm.go(), None);
    // Disabled: pressing it asks for nothing.
    assert!(go(&vm).is_empty());
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
