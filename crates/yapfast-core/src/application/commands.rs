//! Apps' commands: the `/` picker in the composer, the options of the
//! command being written, and the commands a message's or a person's menu
//! offers. A stub that phase 1 of the bot commands fills in (see
//! bots-gap-analysis.md, phases 1a-1c and 2): its actions and events come
//! here and do nothing yet.
//!
//! The rules it keeps: the index is fetched only once the person types `/`
//! ([`DiscordCommand::CommandIndex`]), a server's is kept until
//! [`Event::CommandIndexChanged`], autocomplete is asked only on a
//! keystroke, and a command runs only on Enter or a menu choice.

use super::*;

impl Application {
    /// [`Action::SlashQuery`], [`Action::PickCommand`],
    /// [`Action::SetCommandOption`] and [`Action::RunAppCommand`].
    pub(super) fn command_action(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::SlashQuery(_)
            | Action::PickCommand(_)
            | Action::SetCommandOption { .. }
            | Action::RunAppCommand { .. } => vec![],
            _ => vec![],
        }
    }

    /// [`Event::InteractionCreated`], [`Event::CommandIndex`],
    /// [`Event::CommandIndexChanged`] and [`Event::AutocompleteChoices`].
    pub(super) fn command_event(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::InteractionCreated { .. }
            | Event::CommandIndex { .. }
            | Event::CommandIndexChanged { .. }
            | Event::AutocompleteChoices { .. } => vec![],
            _ => vec![],
        }
    }
}
