//! The Voice and video settings page: task S03's tests, driving
//! `anomi_settings::voice::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced_test::simulator::Simulator;

use anomi_settings::voice::view;
use anomi_vm::fixtures::settings::voice::{busy, page, plain, voice};
use anomi_vm::settings::voice::{Intent, Voice};

use super::support::{SIZE, asked, front, settings};

/// The page runs past the window's height, where the frame scrolls it: the
/// simulator gets room for all of it.
fn simulate(page: &Voice) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    let tall = iced::Size::new(SIZE.width, 2400.0);
    Simulator::with_size(settings(), tall, view(page, front()).boxed())
}

fn clicked(page: &Voice, words: &str) -> Vec<Intent> {
    let mut ui = simulate(page);
    ui.click(words)
        .unwrap_or_else(|_| panic!("{words} is on the page"));
    asked(ui)
}

#[test]
fn the_microphone_test_and_the_refresh_link_ask() {
    let page = voice();
    assert_eq!(
        clicked(&page, "Test microphone"),
        [Intent::TestMicrophone(true)]
    );
    assert_eq!(clicked(&page, "Refresh devices"), [Intent::Refresh]);
}

#[test]
fn a_running_test_offers_to_stop_and_a_call_going_on_refuses_it() {
    let page = Voice {
        microphone_testing: true,
        ..voice()
    };
    assert_eq!(
        clicked(&page, "Stop testing"),
        [Intent::TestMicrophone(false)]
    );
    let calling = Voice {
        calling: true,
        ..voice()
    };
    let mut ui = simulate(&calling);
    let _ = ui.click("Test microphone");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_camera_test_needs_a_camera() {
    let none = voice();
    let mut ui = simulate(&none);
    let _ = ui.click("Test video");
    assert!(asked(ui).is_empty());
    let page = Voice {
        cameras: vec!["Integrated camera".into()],
        ..voice()
    };
    assert_eq!(clicked(&page, "Test video"), [Intent::TestCamera(true)]);
}

#[test]
fn the_switches_and_choices_ask_for_their_values() {
    let page = voice();
    assert_eq!(
        clicked(&page, "Turn on your microphone when you join a call"),
        [Intent::UnmuteOnJoin(false)]
    );
    assert_eq!(clicked(&page, "RNNoise"), [Intent::NoiseSuppression(1)]);
    assert_eq!(clicked(&page, "Push to talk"), [Intent::InputMode(1)]);
    assert_eq!(clicked(&page, "30 min"), [Intent::AloneWait(3)]);
}

#[test]
fn push_to_talk_shows_its_key_and_records_it() {
    let page = busy().settings.voice;
    assert_eq!(
        clicked(&page, "Edit keybind"),
        [Intent::RecordTalkKey(true)]
    );
    let recording = Voice {
        talk_recording: true,
        ..page
    };
    assert_eq!(
        clicked(&recording, "Stop recording"),
        [Intent::RecordTalkKey(false)]
    );
}

#[test]
fn entrance_sounds_open_a_list_and_a_choice_closes_it() {
    let page = busy().settings.voice;
    assert_eq!(clicked(&page, "Choose"), [Intent::EntranceList(true)]);
    assert_eq!(clicked(&page, "Remove"), [Intent::Entrance(String::new())]);
    let open = anomi_vm::fixtures::settings::voice::gate().settings.voice;
    assert_eq!(clicked(&open, "Quack"), [Intent::Entrance("2".into())]);
    // Without Nitro the button is off.
    let poor = Voice {
        nitro: false,
        ..page
    };
    let mut ui = simulate(&poor);
    let _ = ui.click("Choose");
    assert!(asked(ui).is_empty());
}

#[test]
fn what_discord_adds_shows_only_when_it_is_linked() {
    let local = plain().settings.voice;
    let mut ui = simulate(&local);
    assert!(ui.find("Test microphone").is_err());
    assert!(ui.find("ENTRANCE SOUNDS").is_err());
    assert!(ui.find("INPUT MODE").is_ok());
    let linked = page().settings.voice;
    let mut ui = simulate(&linked);
    assert!(ui.find("MICROPHONE TEST").is_ok());
}

#[test]
fn the_rest_of_the_calls_settings_ask_for_their_values() {
    let open = anomi_vm::fixtures::settings::voice::gate().settings.voice;
    assert_eq!(clicked(&open, "Close"), [Intent::EntranceList(false)]);
    assert_eq!(clicked(&open, "Light"), [Intent::NoiseStrength(2)]);
    assert_eq!(
        clicked(&open, "Take out your room's echo in Discord calls"),
        [Intent::Dereverb(true)]
    );
    assert_eq!(
        clicked(
            &open,
            "Let Windows process your microphone in Discord calls"
        ),
        [Intent::WindowsEffects(true)]
    );
}

#[test]
fn letting_go_of_the_soundboard_slider_settles_it() {
    let page = voice();
    let mut ui = simulate(&page);
    let at = ui.find("Soundboard volume").expect("its label").bounds();
    // The track runs under the label, across the page.
    ui.point_at(iced::Point::new(SIZE.width / 4.0, at.y + at.height + 14.0));
    ui.simulate(iced_test::simulator::click());
    let asked = asked(ui);
    assert!(
        matches!(
            &asked[..],
            [Intent::SoundboardVolume(a), Intent::SoundboardVolumeSettled(b)] if a == b && *a < 50
        ),
        "{asked:?}"
    );
}
