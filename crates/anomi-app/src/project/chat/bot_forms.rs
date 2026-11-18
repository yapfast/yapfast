//! A bot's form and the select menu open (`show_bot_interactions`,
//! `src/ui/refresh.rs:785`), from the core's `interaction_modal` and
//! `select_menu`: each one's layer is open while the core holds it, the
//! form's first text field takes the keys as it opens, a menu opens under
//! the box that opened it and lists the choices its search finds. A
//! keystroke in the form or the menu's search reaches these alone
//! (`Reach::FORM`). Text to read is parsed once for each value it takes.

use yapfast_core::application::{AppState, InteractionModal, ModalField, SelectMenu};
use yapfast_lang::tr;

use anomi_vm::chat::bot_forms::{BotForms, Choice, Field, File, Form, Kind, Select};
use anomi_vm::menu::Spot;
use anomi_vm::shell::{Layer, Shell};
use anomi_vm::text::{self, Source, Text};

use super::message::body_sections;
use crate::pictures::{self, Requests};

/// What the form and the menu were shown with.
#[derive(Default)]
pub(crate) struct Shown {
    /// Where the open menu's box is, which it opens under.
    pub(crate) at: Option<Spot>,
    /// The form's texts to read as parsed, by the value each was parsed from.
    texts: Vec<(String, Text)>,
}

/// Shows the form and the menu the core holds, opening or closing their
/// layers as it does.
pub(crate) fn show(
    state: &AppState,
    shell: &mut Shell,
    forms: &mut BotForms,
    shown: &mut Shown,
    requests: &mut Requests,
    focus: &mut Option<&'static str>,
) {
    let modal = state.interaction_modal.as_ref();
    match (modal, shell.is_open(Layer::BotModal)) {
        (Some(_), false) => {
            shell.open(Layer::BotModal);
            *focus = Some(anomi_chat::bot_forms::FIRST);
        }
        (None, true) => shell.close(Layer::BotModal),
        _ => {}
    }
    if let Some(modal) = modal {
        let form = form(modal, &mut shown.texts);
        if forms.form != form {
            forms.form = form;
        }
    } else {
        shown.texts.clear();
    }
    let menu = state.select_menu.as_ref();
    match (menu, shell.is_open(Layer::BotSelect)) {
        (Some(menu), false) => {
            shell.open_at(Layer::BotSelect, shown.at.unwrap_or_default());
            if searchable(menu) {
                *focus = Some(anomi_chat::bot_forms::SEARCH);
            }
        }
        (None, true) => shell.close(Layer::BotSelect),
        _ => {}
    }
    if let Some(menu) = menu {
        let anchor = shown.at.map_or(0.0, |at| at.anchor.width);
        let select = select(state, menu, anchor, requests);
        if forms.select != select {
            forms.select = select;
        }
    }
}

fn form(modal: &InteractionModal, texts: &mut Vec<(String, Text)>) -> Form {
    let mut parsed = Vec::new();
    let fields = modal
        .fields
        .iter()
        .map(|field| {
            let kind = Kind::of(u8::try_from(field.kind).unwrap_or_default());
            let text = if kind == Kind::Text {
                let text = texts
                    .iter()
                    .find(|(value, _)| *value == field.value)
                    .map_or_else(|| markdown(&field.value), |(_, text)| text.clone());
                parsed.push((field.value.clone(), text.clone()));
                text
            } else {
                Text::default()
            };
            Field {
                kind,
                label: field.label.clone(),
                description: field.description.clone(),
                value: field.value.clone(),
                placeholder: field.placeholder.clone(),
                required: field.required,
                paragraph: field.paragraph,
                summary: summary(field),
                text,
                choices: if matches!(kind, Kind::Radio | Kind::Checks) {
                    choices(field)
                } else {
                    Vec::new()
                },
                checked: field.checked,
                files: field
                    .files
                    .iter()
                    .map(|path| {
                        let name = path
                            .file_name()
                            .unwrap_or(path.as_os_str())
                            .to_string_lossy()
                            .into_owned();
                        File {
                            remove_label: tr!("Remove {}", name),
                            name,
                        }
                    })
                    .collect(),
                max_files: field.max_values,
            }
        })
        .collect();
    *texts = parsed;
    Form {
        heading: modal.title.clone(),
        fields,
        supported: modal.supported,
        saving: modal.saving(),
        error: modal.error.clone().unwrap_or_default(),
    }
}

/// A form's Markdown to read, its blocks laid out.
fn markdown(markdown: &str) -> Text {
    let sections = body_sections(markdown);
    text::markdown(Source {
        sections: sections.as_deref(),
        ..Source::new(markdown)
    })
}

/// A select field's picks, by their labels.
fn summary(field: &ModalField) -> String {
    let labels = field.values.iter().map(|value| {
        let option = field.options.iter().find(|option| &option.value == value);
        option.map_or(value.as_str(), |option| option.label.as_str())
    });
    labels.collect::<Vec<_>>().join(", ")
}

/// A radio or checkbox group's options.
fn choices(field: &ModalField) -> Vec<Choice> {
    let options = field.options.iter().enumerate();
    options
        .map(|(index, option)| Choice {
            index,
            label: option.label.clone(),
            value: option.value.clone(),
            description: option.description.clone(),
            selected: field.values.contains(&option.value),
            emoji: option.emoji.clone(),
            picture: None,
        })
        .collect()
}

/// Members, roles and channels: typing finds more.
fn searchable(menu: &SelectMenu) -> bool {
    matches!(menu.kind, 5..=8)
}

/// The menu open, its choices those its search finds; a message's shows
/// its server's emoji as the message does.
fn select(state: &AppState, menu: &SelectMenu, anchor: f32, requests: &mut Requests) -> Select {
    let query = menu.query.to_lowercase();
    let emoji = state
        .message(&menu.item)
        .map(|message| message.emoji.as_slice())
        .unwrap_or_default();
    let choices = menu
        .choices
        .iter()
        .enumerate()
        .filter(|(_, choice)| {
            query.is_empty()
                || choice.label.to_lowercase().contains(&query)
                || choice.description.to_lowercase().contains(&query)
        })
        .map(|(index, choice)| {
            let custom = emoji.iter().find(|emoji| emoji.text == choice.emoji);
            if let Some(custom) = custom {
                requests.want(state, [custom.picture.as_str()]);
            }
            Choice {
                index,
                label: choice.label.clone(),
                value: choice.value.clone(),
                description: choice.description.clone(),
                selected: choice.selected,
                emoji: if custom.is_some() {
                    String::new()
                } else {
                    choice.emoji.clone()
                },
                picture: custom.and_then(|custom| pictures::find(state, Some(&custom.picture))),
            }
        })
        .collect();
    Select {
        heading: menu.label.clone(),
        query: menu.query.clone(),
        min: menu.min,
        max: menu.max,
        searchable: searchable(menu),
        choices,
        anchor,
    }
}
