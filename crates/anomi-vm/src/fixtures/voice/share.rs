//! Samples of the share menu for tests and shots, after the fake Discord's
//! data: task V10's. The menu's own states, and the Lounge's window with it
//! open over the call controls' share button.

use iced::{Point, Rectangle, Size};

use crate::Vm;
use crate::fixtures::pictures;
use crate::menu::Spot;
use crate::shell::Layer;
use crate::voice::share::{Screen, Share, Sound};

use super::super::shell;
use super::controls;

/// Where the system's dialog picks, and nothing is listed: sound and quality
/// only, as `voice/share_menu.png` shows it.
pub fn dialog() -> Share {
    Share {
        dialog: true,
        ..Share::default()
    }
}

/// Two screens with their pictures, a few windows and a camera.
pub fn listed() -> Share {
    Share {
        screens: vec![
            Screen {
                title: "Screen 1".into(),
                picture: Some(pictures::sunset(160, 90)),
            },
            Screen {
                title: "Screen 2".into(),
                picture: None,
            },
        ],
        windows: vec!["Terminal".into(), "Browser".into(), "Notes".into()],
        cameras: vec!["Webcam".into()],
        dialog: true,
        ..Share::default()
    }
}

/// One screen alone, which sits in the middle.
pub fn one_screen() -> Share {
    Share {
        screens: vec![Screen {
            title: "Screen 1".into(),
            picture: Some(pictures::sunset(160, 90)),
        }],
        ..Share::default()
    }
}

/// Windows whose application's sound alone can be shared, the list open.
pub fn apps() -> Share {
    Share {
        windows: vec!["Terminal".into(), "Browser".into()],
        sound_apps: vec!["Music".into(), "Video".into()],
        apps_open: true,
        sound: Sound::App(1),
        ..Share::default()
    }
}

/// While live: what to share instead, and its sound; nothing picked yet.
pub fn switching() -> Share {
    Share {
        switching: true,
        source: None,
        ..listed()
    }
}

/// The Lounge's window with `share` open over the controls' share button, as
/// the golden `voice/share_menu.png` has it.
pub fn window(share: Share) -> Vm {
    let mut vm = shell::open_at(
        controls::window(controls::joined()),
        Layer::Share,
        Spot::above(Rectangle::new(
            Point::new(933.0, 796.0),
            Size::new(44.0, 40.0),
        )),
    );
    vm.voice.share = share;
    vm
}
