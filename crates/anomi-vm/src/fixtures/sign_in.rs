//! Samples of the sign-in screen and linking Discord for tests and shots,
//! after the fake Discord's data: task N09's. Each is the whole window with
//! nobody signed in.

use crate::Vm;
use crate::sign_in::{Code, Link, SignIn, Verify};

use super::shell;

/// The sign-in screen with `sign_in` filled in.
fn with(sign_in: SignIn) -> Vm {
    Vm {
        sign_in,
        ..shell::signed_out()
    }
}

/// What the fake's pairing link looks like: long enough to need a real code.
fn address() -> &'static str {
    "https://discord.com/ra/Yb3Fq9ZkLw0cT7mPpXr2NsVhAeUiOjDg"
}

/// Linking Discord, before anything was asked: the warning, Show the code.
pub fn idle() -> Vm {
    with(SignIn::default())
}

/// A code is on its way.
pub fn starting() -> Vm {
    with(SignIn {
        link: Link::Starting,
        ..SignIn::default()
    })
}

/// The code to scan.
pub fn code() -> Vm {
    with(SignIn {
        link: Link::Code(Code::new(address())),
        ..SignIn::default()
    })
}

/// The phone scanned it, as Alice.
pub fn scanned() -> Vm {
    with(SignIn {
        link: Link::Scanned("Alice".into()),
        ..SignIn::default()
    })
}

/// Linking failed.
pub fn failed() -> Vm {
    with(SignIn {
        link: Link::Failed("Linking failed. Try again.".into()),
        ..SignIn::default()
    })
}

/// Discord's captcha waits in the browser.
pub fn captcha() -> Vm {
    with(SignIn {
        link: Link::Captcha,
        ..SignIn::default()
    })
}

/// The token field is open, with a token typed.
pub fn token() -> Vm {
    with(SignIn {
        token_open: true,
        token: "mfa.sample-token".into(),
        ..SignIn::default()
    })
}

/// Discord is checking a token: the panel's own line, the preview on (the
/// core sets only the link's phase).
pub fn checking() -> Vm {
    with(SignIn {
        link: Link::Checking,
        ..SignIn::default()
    })
}

/// Connecting to Discord: busy, with the line the core's `start` sets.
pub fn connecting() -> Vm {
    with(SignIn {
        busy: true,
        notice: "Connecting to Discord...".into(),
        ..SignIn::default()
    })
}

/// The graphics' notice over the card.
pub fn graphics_notice() -> Vm {
    with(SignIn {
        graphics_notice: Some(
            "Graphics acceleration is unavailable, so Yapfast draws in software.".into(),
        ),
        ..SignIn::default()
    })
}

/// The Matrix sign-in, empty.
pub fn matrix() -> Vm {
    with(SignIn {
        matrix_build: true,
        homeserver: "matrix.org".into(),
        ..SignIn::default()
    })
}

/// The Matrix sign-in refused, with a saved session to retry.
pub fn matrix_refused() -> Vm {
    with(SignIn {
        matrix_build: true,
        homeserver: "matrix.org".into(),
        username: "@alice:matrix.org".into(),
        password: "secret".into(),
        notice: "Wrong user name or password.".into(),
        restore_available: true,
        ..SignIn::default()
    })
}

/// The Matrix sign-in while it connects.
pub fn matrix_busy() -> Vm {
    with(SignIn {
        matrix_build: true,
        busy: true,
        homeserver: "matrix.org".into(),
        username: "@alice:matrix.org".into(),
        ..SignIn::default()
    })
}

/// The direct messages of an unverified device: the banner is on.
pub fn verify() -> Vm {
    let mut vm = shell::dm_home();
    vm.sign_in.verify = Verify {
        unverified: true,
        ..Verify::default()
    };
    vm
}
