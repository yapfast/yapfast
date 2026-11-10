//! Messages only this account sees, which an app sent as its answer
//! (EPHEMERAL, `1 << 6`): "Only you can see this · Dismiss message". A stub
//! that phase 1d of the bot commands fills in (see bots-gap-analysis.md):
//! dismissing one asks Discord nothing, and none is kept on disk.

use super::*;

impl Application {
    /// [`Action::DismissEphemeral`]: hides the ephemeral message `item`.
    pub(super) fn dismiss_ephemeral(&mut self, _item: String) -> Vec<Effect> {
        vec![]
    }
}
