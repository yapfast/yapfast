//! Samples of the Voice and video settings page for tests and shots, after
//! the fake Discord's data: task S03's.

use crate::Vm;
use crate::fixtures::pictures::sunset;
use crate::fixtures::settings::frame::settings;
use crate::settings::frame::Page;
use crate::settings::voice::{Entrance, Group, Voice};

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// The golden screenshot's: Discord linked, a microphone and speakers, no
/// camera.
pub fn voice() -> Voice {
    Voice {
        microphones: names(&["Default (BlackShark V3 X Mono)", "Webcam microphone"]),
        speakers: names(&["Default (BlackShark V3 X Analog)", "Monitor speakers"]),
        discord: true,
        level_voice: true,
        talk_note: "The key works in every app.".to_owned(),
        ..Voice::default()
    }
}

/// The window with the settings open on Voice and video.
pub fn page() -> Vm {
    let mut vm = settings(Page::Voice);
    vm.settings.voice = voice();
    vm
}

/// With a camera filming, a microphone test running, push to talk with its
/// keys and a server's entrance sound.
pub fn busy() -> Vm {
    let mut vm = page();
    let page = &mut vm.settings.voice;
    page.cameras = names(&["Integrated camera", "Phone camera"]);
    page.camera_testing = true;
    page.camera_preview = Some(sunset(320, 180));
    page.microphone_testing = true;
    page.gate_level = -38.0;
    page.gate_sending = true;
    page.input_mode = 1;
    page.talk_key = "Ctrl + Space".to_owned();
    page.talk_delay = 1250;
    page.noise_suppression = 1;
    page.noise_strength = 1;
    page.entrance_servers = names(&["All servers", "Server"]);
    page.entrance_server = 1;
    page.entrance_sound = "Airhorn".to_owned();
    page.nitro = true;
    vm
}

/// Voice activity with the gate's slider, Windows' effects, and the list of
/// entrance sounds open.
pub fn gate() -> Vm {
    let mut vm = busy();
    let page = &mut vm.settings.voice;
    page.camera_testing = false;
    page.microphone_testing = false;
    page.input_mode = 0;
    page.windows_effects_choice = true;
    page.entrance_open = true;
    page.entrance_groups = vec![
        Group {
            heading: "Favourites".to_owned(),
            sounds: vec![sound("1", "Airhorn"), sound("2", "Quack")],
        },
        Group {
            heading: "Server".to_owned(),
            sounds: vec![sound("3", "Cricket"), sound("4", "Applause")],
        },
    ];
    vm
}

fn sound(id: &str, name: &str) -> Entrance {
    Entrance {
        id: id.to_owned(),
        name: name.to_owned(),
    }
}

/// Without Discord: the page of the local calls only, a notice for each
/// missing device.
pub fn plain() -> Vm {
    let mut vm = page();
    vm.settings.voice = Voice::default();
    vm
}
