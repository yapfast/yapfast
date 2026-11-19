//! Samples of the share menu for tests and shots, after the fake Discord's
//! data: task V10's. The menu's own states, and the Lounge's window with it
//! open over the call controls' share button.

use iced::{Point, Rectangle, Size};

use crate::Vm;
use crate::fixtures::pictures;
use crate::menu::Spot;
use crate::shell::Layer;
use crate::voice::controls::MENU_SHIFT;
use crate::voice::share::{Live, Part, Resolution, Screen, Share, Sound};

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

/// While live, after "Change Windows": what to share instead, nothing
/// picked yet; the stream goes out at 720p and 60 fps, the higher quality
/// on.
pub fn switching() -> Share {
    Share {
        switching: true,
        source: None,
        resolution: Resolution::P720,
        fps: 60,
        high_quality: true,
        ..listed()
    }
}

/// While live, the stream's own menu, as the share button opens it: 720p
/// at 60 fps, the higher quality on, all applications heard.
pub fn live() -> Share {
    Share {
        live: Some(Live::new(true)),
        ..switching()
    }
}

/// The stream's menu with its quality's submenu open beside it, as
/// Discord's client shows it.
pub fn live_quality() -> Share {
    Share {
        live: Some(Live {
            open: Some(Part::Quality),
            current: Some(Part::Quality),
            ..Live::new(true)
        }),
        ..live()
    }
}

/// The stream's menu while the stream starts: "Change Windows" waits.
pub fn live_starting() -> Share {
    Share {
        live: Some(Live::new(false)),
        ..live()
    }
}

/// The Lounge's window with `share` open over the controls' share button, as
/// the golden `voice/share_menu.png` has it.
pub fn window(share: Share) -> Vm {
    let mut vm = shell::open_at(
        controls::window(controls::joined()),
        Layer::Share,
        Spot::above(Rectangle::new(
            Point::new(933.0 - MENU_SHIFT, 794.0),
            Size::new(44.0, 44.0),
        )),
    );
    vm.voice.share = share;
    vm
}
