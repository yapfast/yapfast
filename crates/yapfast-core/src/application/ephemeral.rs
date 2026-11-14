//! Messages only this account sees, which an app sent as its answer
//! (EPHEMERAL, `1 << 6`): "Only you can see this · Dismiss message".
//! Dismissing one asks Discord nothing, as there's no endpoint for it: the
//! channel lets go of it, and none is ever kept on disk
//! (`discord_text/open_channel.rs`).

use super::*;

/// The message flag of an answer only this account sees.
const EPHEMERAL: u64 = 1 << 6;

impl Application {
    /// [`Action::DismissEphemeral`]: hides the ephemeral message `item`.
    pub(super) fn dismiss_ephemeral(&mut self, item: String) -> Vec<Effect> {
        if self.state.preview
            || !self
                .state
                .message(&item)
                .is_some_and(|message| message.flags & EPHEMERAL != 0)
        {
            return vec![];
        }
        let Some(room) = self.state.room_of(&item).cloned() else {
            return vec![];
        };
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::DismissEphemeral { room, item },
        ))]
    }
}
