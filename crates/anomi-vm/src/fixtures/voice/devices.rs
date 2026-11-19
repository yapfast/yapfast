//! Samples of the device menus for tests and shots, after the fake
//! Discord's data: task V09's. The two menus as `voice/mic_device_menu.png`
//! and `voice/headphones_device_menu.png` have them, with a submenu open,
//! and the windows they open over: the Lounge's call with its dock, the
//! menu above the caret of the toggle it came from.

use iced::{Point, Rectangle, Size};
use yapfast_lang::tr;

use crate::Vm;
use crate::menu::Spot;
use crate::settings::voice::Profile;
use crate::shell::Layer;
use crate::voice::devices::{Devices, Intent, Kind, Labels, Part};

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

/// The rows' names, the system's default named as the projection does.
fn named(names: &[&str]) -> Vec<String> {
    names
        .iter()
        .map(|name| match *name {
            DEFAULT => tr!("System: {}", "BlackShark V3 X Monitor"),
            name => name.into(),
        })
        .collect()
}

/// A menu over `names`, the first in use.
fn menu(kind: Kind, names: &[&str]) -> Devices {
    Devices::new(kind, labels(kind), &named(names), Some(0))
}

/// The input profile's row, as the projection names it.
fn with_profile(menu: Devices, chosen: Profile) -> Devices {
    let names = [
        tr!("Voice Isolation"),
        tr!("Studio"),
        yapfast_lang::text_in("InputProfile", "Custom"),
    ]
    .map(String::from);
    menu.profile(tr!("Input Profile"), names, chosen)
}

/// The input menu with Discord linked, the profile Custom
/// (`voice/mic_device_menu.png`).
pub fn microphone() -> Devices {
    with_profile(
        menu(
            Kind::Input,
            &[DEFAULT, "Built-in Audio Analog Stereo", "Webcam Microphone"],
        ),
        Profile::Custom,
    )
}

/// The output menu (`voice/headphones_device_menu.png`): no profile.
pub fn speakers() -> Devices {
    menu(Kind::Output, &[DEFAULT, "Built-in Audio Analog Stereo"])
}

/// The output menu with its devices open beside it: names long enough to
/// be cut, the one in use on two lines under the head.
pub fn listing() -> Devices {
    let names = named(&[
        DEFAULT,
        "BlackShark V3 X Analog Stereo",
        "Ryzen HD Audio Controller Analog Stereo",
        "GA102 High Definition Audio Controller Digital Stereo (HDMI)",
        "yapfast_frameworks Audio/Sink sink",
    ]);
    let mut menu = Devices::new(Kind::Output, labels(Kind::Output), &names, Some(1));
    menu.apply(&Intent::Hover(Some(Part::Device)));
    menu
}

/// The input menu with its profiles open beside it, Voice Isolation in use.
pub fn profiles() -> Devices {
    let mut menu = with_profile(
        menu(Kind::Input, &[DEFAULT, "Webcam Microphone"]),
        Profile::Isolation,
    );
    menu.apply(&Intent::Hover(Some(Part::Profile)));
    menu
}

/// No device at all, the submenu open saying so.
pub fn nothing_found() -> Devices {
    let mut menu = with_profile(
        Devices::new(Kind::Input, labels(Kind::Input), &[], None),
        Profile::Studio,
    );
    menu.apply(&Intent::Hover(Some(Part::Device)));
    menu
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
