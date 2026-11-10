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
    /// [`Action::SetCommandOption`], [`Action::FocusCommandOption`],
    /// [`Action::CloseCommand`] and [`Action::RunAppCommand`].
    pub(super) fn command_action(&mut self, action: Action) -> Vec<Effect> {
        match action {
            // Phase 1b: application/slash.rs.
            Action::SlashQuery(query) => self.slash_query_action(query),
            Action::PickCommand(index) => self.pick_command(index),
            Action::SetCommandOption { index, value } => self.set_command_option(index, value),
            Action::FocusCommandOption(index) => self.focus_option(index),
            Action::CloseCommand(back) => self.close_command(back),
            Action::RunAppCommand {
                command: _,
                target: None,
            } => match self.state.selected.clone() {
                Some(room) => self.run_written_command(&room),
                None => vec![],
            },
            Action::RunAppCommand { .. } => vec![],
            _ => vec![],
        }
    }

    /// [`Event::InteractionCreated`], [`Event::CommandIndex`],
    /// [`Event::CommandIndexChanged`] and [`Event::AutocompleteChoices`].
    pub(super) fn command_event(&mut self, event: Event) -> Vec<Effect> {
        match event {
            // Phase 1b: application/slash.rs.
            Event::CommandIndex { room, index } => self.slash_index(room, &index),
            Event::CommandIndexChanged { guild } => {
                self.slash_index_changed(guild.as_deref());
                vec![]
            }
            Event::AutocompleteChoices { nonce, choices } => {
                self.slash_autocomplete(&nonce, &choices)
            }
            Event::InteractionCreated { .. } => vec![],
            _ => vec![],
        }
    }
}
