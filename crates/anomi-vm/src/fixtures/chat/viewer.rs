//! Samples of the image viewer for tests and shots, after the fake Discord's
//! data: task C10's. `viewer(..)` are its view-model alone, the rest the whole
//! window with the viewer open over a DM.

use crate::Vm;
use crate::chat::viewer::{Note, Viewer};
use crate::fixtures::{pictures, shell};
use crate::menu::{Menu, Row, Spot};
use crate::shell::Layer;
use yapfast_lang::tr;

/// The second of a message's three pictures, in full.
pub fn several_viewer() -> Viewer {
    Viewer {
        item: "1400000000000009001".into(),
        index: 1,
        name: "sunset.png".into(),
        picture: Some(pictures::sunset(1600, 1000)),
        size: (1600, 1000),
        place: Some((2, 3)),
        save_tip: tr!("Save {}", "sunset.png"),
        link: Some("https://cdn.discordapp.com/attachments/1/2/sunset.png".into()),
        ..Viewer::default()
    }
}

/// A message's only picture: no position, no arrows.
pub fn single_viewer() -> Viewer {
    Viewer {
        place: None,
        name: "photo.png".into(),
        save_tip: tr!("Save {}", "photo.png"),
        ..several_viewer()
    }
}

/// The preview of a picture whose full image is on its way.
pub fn loading_viewer() -> Viewer {
    Viewer {
        picture: Some(pictures::sunset(480, 300)),
        note: Note::Loading,
        ..several_viewer()
    }
}

/// A picture that couldn't be fetched, its preview still shown.
pub fn failed_viewer() -> Viewer {
    Viewer {
        note: Note::Failed("Couldn't load the image: 404".into()),
        ..loading_viewer()
    }
}

/// A link's preview: it has an address and no file to save.
pub fn preview_viewer() -> Viewer {
    Viewer {
        preview: true,
        place: None,
        ..several_viewer()
    }
}

/// A picture already zoomed in.
pub fn zoomed_viewer() -> Viewer {
    Viewer {
        zoom: 2.0,
        ..several_viewer()
    }
}

/// The window with `viewer` open over it.
pub fn open(viewer: Viewer) -> Vm {
    let mut vm = shell::dm_home();
    vm.chat.viewer = viewer;
    shell::open(vm, Layer::Viewer)
}

pub fn several() -> Vm {
    open(several_viewer())
}

pub fn single() -> Vm {
    open(single_viewer())
}

pub fn loading() -> Vm {
    open(loading_viewer())
}

pub fn failed() -> Vm {
    open(failed_viewer())
}

pub fn zoomed() -> Vm {
    open(zoomed_viewer())
}

/// `vm` with the window's context menu open at (`x`, `y`) over the viewer,
/// its rows as `rows` makes them (`anomi_chat::viewer::menu::rows`, which
/// carries the language).
pub fn with_menu(mut vm: Vm, x: f32, y: f32, rows: fn(&Viewer, bool) -> Vec<Row>) -> Vm {
    let at = Spot::pointer(iced::Point::new(x, y));
    let rows = rows(&vm.chat.viewer, false);
    vm.menu = Menu::new(1, at, rows);
    vm
}
