//! The device menus and the input level: task V09's tests, driving
//! `anomi_voice::devices::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced_test::Simulator;

use anomi_theme::{Motion, Renderer, Theme};
use anomi_vm::fixtures::voice::devices as samples;
use anomi_vm::voice::devices::{Devices, Intent, Noise};
use anomi_voice::devices;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(420.0, 560.0);

fn simulate(vm: &Devices) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, devices::view(vm, front()))
}

#[test]
fn the_input_menu_has_everything_of_a_discord_call() {
    let vm = samples::microphone();
    let mut ui = simulate(&vm);
    for text in [
        "Input Device",
        "Input Volume",
        "100%",
        "INPUT LEVEL",
        "NOISE SUPPRESSION",
        "DeepFilterNet",
        "RNNoise",
        "Off",
        "Voice Settings",
    ] {
        assert!(ui.find(text).is_ok(), "{text}");
    }
    assert!(ui.find("Muted. Nothing you say is sent.").is_err());
}

#[test]
fn the_output_menu_has_neither_level_nor_noise_suppression() {
    let vm = samples::speakers();
    let mut ui = simulate(&vm);
    assert!(ui.find("Output Device").is_ok());
    assert!(ui.find("Output Volume").is_ok());
    assert!(ui.find("Voice Settings").is_ok());
    assert!(ui.find("INPUT LEVEL").is_err());
    assert!(ui.find("NOISE SUPPRESSION").is_err());
    assert!(ui.find("Input Volume").is_err());
}

#[test]
fn a_call_that_is_not_discords_has_no_noise_choice() {
    let vm = samples::without_noise();
    let mut ui = simulate(&vm);
    assert!(ui.find("INPUT LEVEL").is_ok());
    assert!(ui.find("NOISE SUPPRESSION").is_err());
}

#[test]
fn the_head_opens_the_list_and_the_list_is_hidden_until_then() {
    let vm = samples::microphone();
    let mut ui = simulate(&vm);
    assert!(ui.find("Webcam Microphone").is_err());
    ui.click("Input Device").unwrap();
    assert_eq!(asked(ui), [Intent::ToggleList]);
}

#[test]
fn an_open_list_names_every_device_and_choosing_one_asks_for_it() {
    let vm = samples::listing();
    let mut ui = simulate(&vm);
    assert!(ui.find("Webcam Microphone").is_ok());
    // Five show before it scrolls: the sixth waits below.
    assert!(ui.click("Bluetooth Headset").is_err());
    ui.click("USB Condenser Microphone").unwrap();
    assert_eq!(asked(ui), [Intent::Select(3)]);
}

#[test]
fn nothing_found_says_so_in_the_list() {
    let mut vm = samples::nothing_found();
    vm.listing = true;
    let mut ui = simulate(&vm);
    assert!(
        ui.find("No microphones found. Connect one, then reopen this menu.")
            .is_ok()
    );
}

#[test]
fn a_noise_option_asks_for_itself() {
    let vm = samples::microphone();
    let mut ui = simulate(&vm);
    ui.click("RNNoise").unwrap();
    ui.click("Off").unwrap();
    assert_eq!(
        asked(ui),
        [Intent::Noise(Noise::RNNoise), Intent::Noise(Noise::Off)]
    );
}

#[test]
fn voice_settings_asks_for_them() {
    let vm = samples::speakers();
    let mut ui = simulate(&vm);
    ui.click("Voice Settings").unwrap();
    assert_eq!(asked(ui), [Intent::Settings]);
}

#[test]
fn a_muted_microphone_says_nothing_is_sent_and_a_note_says_why_there_is_no_level() {
    let vm = samples::muted();
    let mut ui = simulate(&vm);
    assert!(ui.find("Muted. Nothing you say is sent.").is_ok());

    let vm = samples::unreadable();
    let mut ui = simulate(&vm);
    assert!(
        ui.find(
            "Yapfast can't read this microphone. Check that it's connected, or choose another."
        )
        .is_ok()
    );
    assert!(ui.find("Muted. Nothing you say is sent.").is_err());
}

#[test]
fn the_volume_follows_the_view_model() {
    let vm = Devices {
        volume: 35,
        ..samples::speakers()
    };
    let mut ui = simulate(&vm);
    assert!(ui.find("35%").is_ok());
}

#[test]
fn a_window_behind_another_draws_the_open_list_at_once() {
    // Nothing plays: the list is open at its full height on the first frame.
    let vm = samples::listing();
    let mut ui = Simulator::with_size(settings(), SIZE, devices::view(&vm, Motion::default()));
    assert!(ui.find("Bluetooth Headset").is_ok());
}
