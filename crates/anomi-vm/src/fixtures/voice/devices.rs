//! Samples of the device menus and the input level for tests and shots, after
//! the fake Discord's data: task V09's. The two menus as `voice/
//! mic_device_menu.png` and `voice/headphones_device_menu.png` have them,
//! their other states, and the windows they open over: the Lounge's call
//! with its dock, the menu above the caret of the toggle it came from.

use iced::{Point, Rectangle, Size};
use yapfast_lang::tr;

use crate::Vm;
use crate::menu::Spot;
use crate::shell::Layer;
use crate::voice::devices::{Devices, Kind, Labels, Noise};

use super::super::shell;

/// The words of a menu, translated as the projection does.
pub fn labels(kind: Kind) -> Labels {
    let (title, empty) = match kind {
        Kind::Input => (
            tr!("Input Device"),
            tr!("No microphones found. Connect one, then reopen this menu."),
        ),
        Kind::Output => (
            tr!("Output Device"),
            tr!("No speakers found. Connect one, then reopen this menu."),
        ),
    };
    Labels {
        title: title.into(),
        empty: empty.into(),
        settings: tr!("Voice Settings").into(),
        none: tr!("None found").into(),
        default: tr!("System default").into(),
    }
}

/// Stands for the system's default, which [`menu`] names as the projection
/// does (`src/ui.rs:3523`).
const DEFAULT: &str = "\u{0}default";

/// A menu over `names`, the first in use.
fn menu(kind: Kind, names: &[&str]) -> Devices {
    let names: Vec<String> = names
        .iter()
        .map(|name| match *name {
            DEFAULT => tr!("Default ({})", "BlackShark V3 X Monitor"),
            name => name.into(),
        })
        .collect();
    Devices::new(kind, labels(kind), &names, Some(0))
}

/// The input menu of a Discord call, closed list, a still microphone and
/// DeepFilterNet in use (`voice/mic_device_menu.png`).
pub fn microphone() -> Devices {
    Devices {
        noise: Some(Noise::DeepFilterNet),
        ..menu(
            Kind::Input,
            &[DEFAULT, "Built-in Audio Analog Stereo", "Webcam Microphone"],
        )
    }
}

/// The output menu (`voice/headphones_device_menu.png`): no level, no noise
/// choice.
pub fn speakers() -> Devices {
    menu(Kind::Output, &[DEFAULT, "Built-in Audio Analog Stereo"])
}

/// Someone talking: the meter two thirds full.
pub fn talking() -> Devices {
    Devices {
        level: crate::kind::Level::new(0.62),
        ..microphone()
    }
}

/// Muted and talking: the faded meter and its line under it.
pub fn muted() -> Devices {
    Devices {
        muted: true,
        ..talking()
    }
}

/// A microphone that can't be read.
pub fn unreadable() -> Devices {
    Devices {
        note: tr!(
            "Yapfast can't read this microphone. Check that it's connected, or choose another."
        )
        .into(),
        ..microphone()
    }
}

/// A call that isn't Discord's: no noise suppression to choose.
pub fn without_noise() -> Devices {
    Devices {
        noise: None,
        ..microphone()
    }
}

/// The device list open, six devices so it scrolls.
pub fn listing() -> Devices {
    let mut menu = menu(
        Kind::Input,
        &[
            DEFAULT,
            "Built-in Audio Analog Stereo",
            "Webcam Microphone",
            "USB Condenser Microphone",
            "Monitor of Speakers",
            "Bluetooth Headset",
        ],
    );
    menu.noise = Some(Noise::RNNoise);
    menu.choose(Some(2));
    menu.listing = true;
    menu
}

/// No device at all.
pub fn nothing_found() -> Devices {
    Devices {
        noise: Some(Noise::Off),
        listing: true,
        ..Devices::new(Kind::Input, labels(Kind::Input), &[], None)
    }
}

/// The caret's toggle in the account card: the microphone's.
const MICROPHONE_AT: Point = Point::new(202.0, 807.0);
/// The headphones'.
const HEADPHONES_AT: Point = Point::new(259.0, 807.0);

/// The Lounge's call with a menu open over the account card's toggle.
pub fn window(devices: Devices) -> Vm {
    let at = match devices.kind {
        Kind::Input => MICROPHONE_AT,
        Kind::Output => HEADPHONES_AT,
    };
    let mut vm = shell::open_at(
        super::stage::window(super::stage::lounge()),
        Layer::Devices,
        Spot::above(Rectangle::new(at, Size::new(52.0, 32.0))),
    );
    match devices.kind {
        Kind::Input => vm.account.input_open = true,
        Kind::Output => vm.account.output_open = true,
    }
    vm.voice.devices = devices;
    vm
}
