//! Samples of the account card. Task N06's.

use crate::account::Account;
use crate::kind::Presence;

use super::avatar;
use super::pictures::pictured;

pub fn account() -> Account {
    Account {
        name: "Alice".into(),
        line: "Testing Yapfast".into(),
        avatar: pictured(avatar("A", 0x7a5fc9), "Alice"),
        presence: Presence::Online,
        muted: false,
        deafened: false,
    }
}
