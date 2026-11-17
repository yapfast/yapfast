//! Samples of a bot's forms and select menus for tests and shots, after the
//! fake Discord's data (`show_bot_interactions`: `src/ui/refresh.rs:784`):
//! task C20's.

use iced::{Point, Rectangle, Size};

use crate::Vm;
use crate::chat::bot_forms::{BotForms, Choice, Field, Form, Kind, Select};
use crate::fixtures::{pictures, shell, text};
use crate::menu::Spot;
use crate::shell::Layer;

fn choice(index: usize, label: &str, description: &str, selected: bool) -> Choice {
    Choice {
        index,
        label: label.into(),
        value: label.to_lowercase(),
        description: description.into(),
        selected,
        ..Choice::default()
    }
}

/// A one-line input, a paragraph, a select and the rest: every kind of field.
pub fn form() -> Form {
    Form {
        heading: "Feedback".into(),
        supported: true,
        fields: vec![
            Field {
                kind: Kind::Text,
                text: text::ship_it(),
                ..Field::default()
            },
            Field {
                kind: Kind::Input,
                label: "Subject".into(),
                description: "A few words".into(),
                placeholder: "What is it about?".into(),
                required: true,
                ..Field::default()
            },
            Field {
                kind: Kind::Input,
                label: "Details".into(),
                value: "The sidebar jumps when a call starts.".into(),
                paragraph: true,
                ..Field::default()
            },
            Field {
                kind: Kind::Select,
                label: "Area".into(),
                placeholder: "Make a selection".into(),
                summary: "Voice, Chat".into(),
                ..Field::default()
            },
            Field {
                kind: Kind::Radio,
                label: "How bad is it?".into(),
                choices: vec![
                    choice(0, "Annoying", "", false),
                    choice(1, "Blocks me", "I can't continue", true),
                ],
                ..Field::default()
            },
            Field {
                kind: Kind::Checks,
                label: "Send along".into(),
                choices: vec![
                    choice(0, "Logs", "The last hour", true),
                    choice(1, "Screenshot", "", false),
                ],
                ..Field::default()
            },
            Field {
                kind: Kind::Check,
                label: "Contact me".into(),
                description: "Only about this".into(),
                required: true,
                checked: true,
                ..Field::default()
            },
            Field {
                kind: Kind::Files,
                label: "Attachments".into(),
                files: vec!["crash.log".into()],
                max_files: 3,
                ..Field::default()
            },
        ],
        ..Form::default()
    }
}

/// A form with a part Yapfast can't fill.
pub fn unsupported() -> Form {
    Form {
        supported: false,
        fields: vec![
            Field {
                kind: Kind::Input,
                label: "Subject".into(),
                ..Field::default()
            },
            Field {
                kind: Kind::Unsupported,
                label: "Colour".into(),
                ..Field::default()
            },
        ],
        ..form()
    }
}

/// Submitted, and waiting for Discord.
pub fn saving() -> Form {
    Form {
        saving: true,
        ..form()
    }
}

/// Discord refused it.
pub fn refused() -> Form {
    Form {
        error: "That subject is too short.".into(),
        ..form()
    }
}

/// A menu of several picks, with emoji and descriptions.
pub fn several() -> Select {
    let mut choices = vec![
        choice(0, "Voice", "Calls and rooms", true),
        choice(1, "Chat", "The timeline and the composer", true),
        choice(2, "Settings", "", false),
        choice(3, "Everything else", "Anything not above", false),
    ];
    choices[0].emoji = "🎙".into();
    choices[1].picture = Some(pictures::badge("chat", 36));
    choices[2].emoji = "⚙".into();
    Select {
        heading: "Area".into(),
        min: 1,
        max: 3,
        anchor: 480.0,
        choices,
        ..Select::default()
    }
}

/// A menu of one pick: the choice made shows a check.
pub fn single() -> Select {
    Select {
        max: 1,
        choices: vec![
            choice(0, "Low", "", false),
            choice(1, "Normal", "", true),
            choice(2, "High", "", false),
        ],
        ..several()
    }
}

/// A menu of people, searchable.
pub fn people() -> Select {
    Select {
        searchable: true,
        choices: vec![choice(0, "Alice", "", false), choice(1, "Bob", "", true)],
        ..several()
    }
}

/// A search that found nothing.
pub fn nothing() -> Select {
    Select {
        searchable: true,
        query: "zzz".into(),
        choices: Vec::new(),
        ..several()
    }
}

fn open_form(form: Form) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::BotModal);
    vm.chat.bot_forms = BotForms {
        form,
        ..BotForms::default()
    };
    vm
}

/// The form open over the channel.
pub fn asked() -> Vm {
    open_form(form())
}

pub fn asked_unsupported() -> Vm {
    open_form(unsupported())
}

pub fn asked_saving() -> Vm {
    open_form(saving())
}

pub fn asked_refused() -> Vm {
    open_form(refused())
}

/// Where the select's box is, in the form's middle.
pub fn box_at() -> Spot {
    Spot::below(Rectangle::new(
        Point::new(460.0, 381.0),
        Size::new(480.0, 40.0),
    ))
}

/// A select menu open under its box, over the form.
pub fn picking(select: Select) -> Vm {
    let mut vm = asked();
    vm.chat.bot_forms.select = select;
    vm.shell.open_at(Layer::BotSelect, box_at());
    vm
}
