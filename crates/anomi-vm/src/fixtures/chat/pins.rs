//! Samples of the pins popover for tests and shots, after the fake
//! Discord's data: task C22's. One pinned message, Bob's welcome in #general.

use crate::Vm;
use crate::chat::pins::Pins;
use crate::fixtures::chat::search as found;
use crate::fixtures::{conversation, shell, text};
use crate::menu::{Align, Spot};
use crate::shell::Layer;
use iced::{Point, Rectangle, Size};
use yapfast_lang::tr;

/// The fake's one pin.
pub fn one() -> Pins {
    Pins {
        note: String::new(),
        pins: vec![welcome()],
    }
}

/// The fake's pin: the first message in #general.
fn welcome() -> crate::chat::search::Found {
    crate::chat::search::Found {
        when: "01.10.2026 16:00".into(),
        body: Some(text::welcome()),
        ..found::found(0)
    }
}

/// Four pins: the welcome, one with pictures, a poll and one with files.
pub fn several() -> Pins {
    Pins {
        note: String::new(),
        pins: vec![
            welcome(),
            found::pictures_found(),
            found::poll_found(),
            found::files_found(),
        ],
    }
}

/// While they load.
pub fn loading() -> Pins {
    Pins {
        note: tr!("Loading pinned messages...").into(),
        pins: Vec::new(),
    }
}

/// No message is pinned.
pub fn none() -> Pins {
    Pins {
        note: tr!("This channel doesn't have any pinned messages yet.").into(),
        pins: Vec::new(),
    }
}

/// The pins button of #general's header, at 1400 by 860, as the header
/// reports it: the popover keeps to its right edge.
fn button() -> Spot {
    let bounds = Rectangle::new(Point::new(1059.0, 9.0), Size::new(32.0, 32.0));
    Spot::below(bounds).align(Align::End)
}

/// The window on #general with `pins` open under the header's button.
pub fn open_with(pins: Pins) -> Vm {
    let mut vm = shell::channel_with(conversation::searching);
    vm.chat.pins = pins;
    shell::open_at(vm, Layer::Pins, button())
}

/// The window with the one pin open, as the golden `header/pins.png`.
pub fn open() -> Vm {
    open_with(one())
}
