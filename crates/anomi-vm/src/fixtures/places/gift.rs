//! Samples of a gift's dialog for tests and shots, after the fake Discord's
//! data: task N12's.

use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::shell;
use crate::places::gift::{Found, Gift};
use crate::shell::Layer;

/// A gift to redeem.
pub fn waiting() -> Gift {
    Gift {
        title: tr!("You've been sent a gift").into(),
        found: Some(Found {
            name: "Discord Nitro".into(),
            detail: format!("1 month · {}", tr!("from {}", "Bob")),
        }),
        confirm: tr!("Redeem Gift").into(),
        ..Gift::default()
    }
}

/// Redeeming it.
pub fn redeeming() -> Gift {
    Gift {
        busy: true,
        status: tr!("Redeeming...").into(),
        ..waiting()
    }
}

/// Redeemed: nothing more to do.
pub fn redeemed() -> Gift {
    Gift {
        confirm: String::new(),
        status: tr!("Redeemed. Enjoy!").into(),
        ..waiting()
    }
}

/// A gift only Discord can redeem: the button opens it in the browser.
pub fn in_discord() -> Gift {
    Gift {
        confirm: tr!("Open in Browser").into(),
        browser: true,
        status: tr!(
            "This gift can only be redeemed in Discord. Open it in the browser to redeem it there."
        )
        .into(),
        ..waiting()
    }
}

/// Looking it up: nothing found yet.
pub fn looking() -> Gift {
    Gift {
        title: tr!("You've been sent a gift").into(),
        busy: true,
        status: tr!("Looking up the gift...").into(),
        ..Gift::default()
    }
}

/// It failed.
pub fn failed() -> Gift {
    Gift {
        title: tr!("You've been sent a gift").into(),
        error: "This gift does not exist.".into(),
        ..Gift::default()
    }
}

/// The window on the first server with the dialog open on `gift`.
pub fn over(gift: Gift) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::Gift);
    vm.places.gift = gift;
    vm
}
