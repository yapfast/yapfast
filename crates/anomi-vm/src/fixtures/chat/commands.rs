//! Samples of the composer's slash commands for tests and shots, after the
//! fake Discord's data (Status Bot's commands and Notes' one): task C16's.
//! `listing` is `composer/slash_suggestions.png`, the rest are the states
//! of the list and the pills the golden does not show.

use yapfast_lang::tr;

use crate::Vm;
use crate::chat::commands::{Choice, Commands, Pill, Written};
use crate::chat::composer::Composer;

use super::super::pictures;
use super::composer as frame;

fn command(app: &str, name: &str, description: &str) -> Choice {
    Choice {
        app: app.into(),
        icon: None,
        name: name.into(),
        description: description.into(),
    }
}

/// `/` typed in #general: Status Bot's commands and Notes', as the fake
/// lists them.
pub fn listing() -> Commands {
    let bot = "Status Bot";
    Commands {
        shown: true,
        heading: tr!("COMMANDS").into(),
        choices: vec![
            command(bot, "ping", "Checks that Status Bot is up."),
            command(bot, "echo", "Says it back."),
            command(bot, "pick", "Picks a colour."),
            command(bot, "find", "Finds a deploy."),
            command(bot, "upload", "Attaches a build log."),
            command(bot, "config set", "Changes a setting."),
            command(bot, "config get", "Shows a setting."),
            command("Notes", "note", "Keeps a note."),
        ],
        commands: true,
        ..Commands::default()
    }
}

/// Twelve commands, the chosen one past the eighth: the list scrolls.
pub fn long() -> Commands {
    let mut commands = listing();
    for number in 1..=4 {
        commands
            .choices
            .push(command("Notes", &format!("note {number}"), "Keeps a note."));
    }
    commands.chosen = 10;
    commands
}

/// The room's commands are being asked for: a heading and no rows.
pub fn loading() -> Commands {
    Commands {
        shown: true,
        heading: tr!("Loading commands…").into(),
        commands: true,
        ..Commands::default()
    }
}

/// Nothing matches what was typed.
pub fn none() -> Commands {
    Commands {
        heading: tr!("No commands match").into(),
        ..loading()
    }
}

/// An app with its icon, and a command without a picture beside it.
pub fn with_icons() -> Commands {
    let mut commands = listing();
    for choice in commands.choices.iter_mut().take(2) {
        choice.icon = Some(pictures::sunset(48, 48));
    }
    commands.chosen = 1;
    commands
}

/// `/pick` written: the values its first option may take, with a person's
/// avatar for each and no app.
pub fn values() -> Commands {
    let person = |name: &str| Choice {
        icon: Some(pictures::sunset(48, 48)),
        name: name.into(),
        ..Choice::default()
    };
    Commands {
        shown: true,
        heading: "COLOUR".into(),
        choices: vec![person("red"), person("green"), person("blue")],
        written: Some(pick()),
        ..Commands::default()
    }
}

fn pill(name: &str, value: &str, required: bool) -> Pill {
    Pill {
        name: name.into(),
        description: format!("The {name}."),
        value: value.into(),
        required,
        ..Pill::default()
    }
}

/// `/pick` with its one option still empty and focused.
pub fn pick() -> Written {
    Written {
        name: "pick".into(),
        options: vec![pill("colour", "", true)],
        focused: 0,
    }
}

/// `/config set` with a value in its first option, an optional one empty,
/// a wrong one, and a file option.
pub fn written() -> Commands {
    let wrong = Pill {
        error: tr!("Must be a number").into(),
        ..pill("limit", "many", true)
    };
    let file = Pill {
        file: true,
        ..pill("log", "build.log", false)
    };
    Commands {
        written: Some(Written {
            name: "config set".into(),
            options: vec![
                pill("key", "theme", true),
                pill("note", "", false),
                wrong,
                file,
            ],
            focused: 1,
        }),
        ..Commands::default()
    }
}

/// A command run with no options: just its name.
pub fn bare() -> Commands {
    Commands {
        written: Some(Written {
            name: "ping".into(),
            ..Written::default()
        }),
        ..Commands::default()
    }
}

/// `composer/slash_suggestions.png`: `/` in the reply's composer, the staged
/// file under the list.
pub fn slash_suggestions() -> Vm {
    frame::in_channel(Composer {
        draft: "/".into(),
        focused: true,
        suggesting: true,
        picking: true,
        commands: listing(),
        ..frame::staged_and_mentioning()
    })
}
