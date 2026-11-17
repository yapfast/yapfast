//! The account card at the foot of the rail and the sidebar, and the popover
//! it opens above itself. The Slint side: the account card of
//! `ui/yapfast.slint:3014` and `AccountPopover`
//! (`ui/yapfast/account-popover.slint`), filled by `show_account` in
//! `src/ui/refresh.rs`. Task N06 (docs/iced/tasks/shell.md).
//!
//! - [`view`] is the card: the nameplate behind it, the avatar with its
//!   presence, the name (the username while the pointer is on it), the
//!   status line, then the microphone and headphones with their carets,
//!   the reconnect button while offline, and the cog.
//! - [`popover`] is the layer the shell draws for `Layer::Account`
//!   (`shell/layers/content.rs`), which places it above the card from the
//!   spot the card's `Popover` intent carried.

mod anchored;
mod card;
mod pointed;
mod popover;
#[cfg(feature = "shots")]
mod shots;

use anomi_theme::prelude::*;
use anomi_vm::account::{Account, Intent};

pub use popover::popover;
#[cfg(feature = "shots")]
pub use shots::shots;

/// The account card, `ACCOUNT_HEIGHT` tall, as wide as it is given.
pub fn view(account: &Account, motion: Motion) -> impl View<Intent> + '_ {
    card::view(account, motion)
}
