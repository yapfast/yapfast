//! Samples of the Account settings page for tests and shots, after the fake
//! Discord's data: task S02's.

use crate::fixtures::account as card;
use crate::settings::account::Account;

/// Alice, as the golden screenshot shows her: Discord linked, her custom
/// status set, and the login in the system's store.
pub fn account() -> Account {
    let from = card::account();
    Account {
        name: from.name,
        avatar: from.avatar,
        decoration: from.decoration,
        status: from.status,
        storage: yapfast_lang::tr!("Discord token is stored in the system credential store.")
            .into(),
        online: true,
        linked: true,
        has_custom_status: true,
        ..Account::default()
    }
}

/// Offline: the card says sync is paused and Reconnect shows.
pub fn offline() -> Account {
    Account {
        status: card::offline().status,
        online: false,
        ..account()
    }
}

/// The local preview: nothing linked, "Back to sign in".
pub fn preview() -> Account {
    Account {
        status: crate::kind::Status {
            text: yapfast_lang::tr!("Local only").into(),
            ..Default::default()
        },
        preview: true,
        online: false,
        linked: false,
        has_custom_status: false,
        storage: String::new(),
        ..account()
    }
}
