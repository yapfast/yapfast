//! Samples of the About settings page, with updates for tests and shots,
//! after the fake Discord's data: task S10's.

use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::settings::frame::settings;
use crate::settings::about::{About, Install, Status};
use crate::settings::frame::{Build, Page, Tone};

/// The golden screenshot's: a development build of a fork, with no releases.
pub fn about() -> About {
    About {
        build: Build {
            headline: "Anomi acd914cd".into(),
            channel: tr!("Development build").into(),
            details: "2026-11-16".into(),
            summary: "Anomi acd914cd · Development build · 2026-11-16".into(),
            tone: Tone::Development,
        },
        product: "Anomi".into(),
        original: tr!("Original: {}", "Yapfast"),
        ..About::default()
    }
}

/// A release build that checks: the switches and the buttons show.
pub fn releases() -> About {
    About {
        build: Build {
            headline: "Yapfast 1.4.0".into(),
            channel: tr!("Stable").into(),
            details: "2026-10-02".into(),
            summary: "Yapfast 1.4.0 · Stable · 2026-10-02".into(),
            tone: Tone::Release,
        },
        product: "Yapfast".into(),
        original: tr!("Original: {}", "Yapfast"),
        releases: true,
        check: true,
        fresh_nightlies: true,
        status: Status::Current,
        ..About::default()
    }
}

/// A newer release with notes, ready to install.
pub fn available() -> About {
    About {
        status: Status::Available,
        latest: "1.5.0".into(),
        can_install: true,
        install_note: tr!("{} MB. Yapfast restarts when it's done.", 48),
        notes: "Calls in DMs.\nFaster scrolling.".into(),
        ..releases()
    }
}

fn window(about: About) -> Vm {
    let mut vm = settings(Page::About);
    vm.settings.frame.update = (about.status == Status::Available).then(|| about.latest.clone());
    vm.settings.about = about;
    vm
}

/// The window with the settings open on About.
pub fn page() -> Vm {
    window(about())
}

pub fn page_releases() -> Vm {
    window(releases())
}

pub fn page_available() -> Vm {
    window(available())
}

/// An update downloading.
pub fn page_installing() -> Vm {
    window(About {
        installing: Some(Install {
            text: tr!("Downloading {}%", 40),
            progress: Some(0.4),
            can_cancel: true,
        }),
        ..available()
    })
}

/// The check failed.
pub fn page_failed() -> Vm {
    window(About {
        status: Status::Failed,
        error: "no connection".into(),
        ..releases()
    })
}
