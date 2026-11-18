//! The slash commands over the composer (`show_app_commands`,
//! `src/ui/app_commands.rs`): the `/` picker's rows, or the values the
//! focused option of the command being written may take, with the heading
//! over them; and the command's pills over the field's text. What they
//! say is the core's (`command_rows`, `command_heading`, `command_draft`).

use yapfast_core::application::Application;

use anomi_vm::chat::commands::{Choice, Commands, Pill, Written};
use anomi_vm::chat::composer::Composer;

use super::{Field, Words};
use crate::pictures::{self, Requests};

pub(super) fn show(
    core: &Application,
    words: &Words,
    composer: &mut Composer,
    requests: &mut Requests,
) {
    let state = &core.state;
    let command = state.command_draft();
    if command.is_none() && state.slash_query().is_none() {
        if composer.commands != Commands::default() {
            composer.commands = Commands::default();
        }
        settle(composer, words);
        return;
    }
    let rows = core.command_rows();
    let heading = core.command_heading(&rows);
    requests.want(state, rows.iter().filter_map(|row| row.icon.as_deref()));
    let choices: Vec<Choice> = rows
        .into_iter()
        .map(|row| Choice {
            icon: pictures::find(state, row.icon.as_deref()),
            app: row.app,
            name: row.name,
            description: row.description,
        })
        .collect();
    let written = command.map(|command| Written {
        name: command.name.clone(),
        options: command
            .options
            .iter()
            .zip(state.option_errors(command))
            .map(|(option, error)| Pill {
                name: option.name.clone(),
                description: option.description.clone(),
                value: option.shown(),
                required: option.required,
                error: error.unwrap_or_default(),
                file: option.is_attachment(),
            })
            .collect(),
        focused: command.focused,
    });
    // The picker shows while `/` is typed, even with nothing to list yet;
    // an option's values only when there are some. Not while the message
    // edited in place was typed in last.
    let shown = words.field == Field::Channel
        && if command.is_some() {
            !choices.is_empty()
        } else {
            !heading.is_empty()
        };
    let chosen = if words.command < choices.len() {
        words.command
    } else {
        0
    };
    let commands = Commands {
        shown,
        heading,
        commands: command.is_none(),
        chosen,
        choices,
        written,
    };
    if composer.commands != commands {
        composer.commands = commands;
    }
    settle(composer, words);
}

/// What the field's keys go to: the `/` picker while it shows, and its
/// rows while it has some.
fn settle(composer: &mut Composer, words: &Words) {
    let commands = &composer.commands;
    let picking = commands.shown;
    let suggesting = picking && !commands.choices.is_empty();
    if composer.picking != picking {
        composer.picking = picking;
    }
    let suggesting = suggesting && words.field == Field::Channel;
    if composer.suggesting != suggesting {
        composer.suggesting = suggesting;
    }
}
