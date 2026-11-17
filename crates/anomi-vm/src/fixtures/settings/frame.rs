//! Samples of the settings frame for tests and shots, after the fake
//! Discord's data: task S01's.

use crate::Vm;
use crate::fixtures::pictures::pictured;
use crate::fixtures::{avatar, shell};
use crate::settings::frame::{Build, Frame, Page, Tone, Unsaved};
use crate::shell::Layer;

/// Alice's frame on the page `page`, with Discord linked and a development
/// build under the sections.
pub fn frame(page: Page) -> Frame {
    Frame {
        page,
        name: "Alice".into(),
        avatar: pictured(avatar("A", 0x7a5fc9), "Alice"),
        close: yapfast_lang::tr!("Close {}", yapfast_lang::tr!("Settings")),
        may_edit_profile: true,
        with_discord: true,
        build: Some(Build {
            headline: "Anomi acd138".into(),
            channel: yapfast_lang::tr!("Development build").into(),
            details: "2026-11-16".into(),
            summary: "Anomi acd138 · Development build · 2026-11-16".into(),
            tone: Tone::Development,
        }),
        ..Frame::default()
    }
}

/// A frame as `frame` makes it with a bar for changes not saved.
pub fn unsaved() -> Frame {
    Frame {
        unsaved: Some(Unsaved::default()),
        ..frame(Page::Account)
    }
}

/// The window with the settings open on `page`.
pub fn settings(page: Page) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::Settings);
    vm.settings.frame = frame(page);
    vm
}

/// The golden screenshot's: My Account.
pub fn account() -> Vm {
    let mut vm = settings(Page::Account);
    vm.settings.account = super::account::account();
    vm
}
