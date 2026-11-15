//! An app's form (modal), as the Slint client's `BotModalDialog`
//! (`ui/interactions.slint`): its title, then its parts in order (text to
//! read, a line or a paragraph to write, a select menu, radio buttons,
//! checkboxes, a checkbox of its own, files to upload, or a note for a
//! part Yapfast can't fill), why it failed, and Cancel and Submit. What it
//! holds and sends is the core's (`SetModalField`, `OpenModalSelect`,
//! `AddModalFiles`, `RemoveModalFile`, `SubmitModal`, `CloseModal`); the
//! window keeps only the paragraphs' editors and the text laid out, which
//! go as it closes.

use crate::widgets::scroll_view;
use std::path::PathBuf;

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Task,
    widget::{Space, button, column, container, row, text, text_editor},
};
use yapfast_core::application::{Action, ModalField};

use crate::{
    app::{App, Message},
    controls::{self, Kind},
    dialogs::dialog,
    elide::elide,
    fonts::{BOLD, MEDIUM},
    markdown::{Body, FIRST_PART, Look},
    pictures::Icon,
    popover::{Popover, Side, Width},
    style::{rounded, scrollbar},
    theme::rgb,
};

/// The parts' kinds, as Discord numbers its components and the core keeps
/// them; select menus are 3 and 5 to 8.
const UNSUPPORTED: u64 = 0;
const TEXT_INPUT: u64 = 4;
const TEXT_DISPLAY: u64 = 10;
const FILE_UPLOAD: u64 = 19;
const RADIO_GROUP: u64 = 21;
const CHECKBOX_GROUP: u64 = 22;
const CHECKBOX: u64 = 23;

/// The form's width, a paragraph's height, a file's row and the upload
/// box's.
const WIDTH: f32 = 520.0;
const PARAGRAPH: f32 = 100.0;
const FILE_ROW: f32 = 40.0;
const UPLOAD: f32 = 56.0;

/// What the window keeps of the form while it shows: by the field's
/// place, a paragraph's editor and text to read laid out.
#[derive(Default)]
pub struct Shown {
    paragraphs: Vec<Option<text_editor::Content>>,
    texts: Vec<Option<(String, Body)>>,
}

#[derive(Clone, Debug)]
pub enum Event {
    Edited(usize, String),
    Paragraph(usize, text_editor::Action),
    OpenSelect(usize),
    ChooseFiles(usize),
    Chosen(usize, Vec<PathBuf>),
    RemoveFile(usize, usize),
    Submit,
    Close,
}

impl App {
    pub fn modal_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Edited(index, value) => {
                self.client.dispatch(Action::SetModalField { index, value })
            }
            Event::Paragraph(index, action) => {
                let Some(Some(content)) = self.modal.paragraphs.get_mut(index) else {
                    return Task::none();
                };
                let edits = action.is_edit();
                content.perform(action);
                if edits {
                    let value = content.text();
                    self.client.dispatch(Action::SetModalField { index, value });
                } else {
                    return Task::none();
                }
            }
            Event::OpenSelect(index) => self.client.dispatch(Action::OpenModalSelect(index)),
            Event::ChooseFiles(index) => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Choose files")
                            .pick_files()
                            .await
                            .map(|files| {
                                files
                                    .iter()
                                    .map(|file| file.path().to_owned())
                                    .collect::<Vec<_>>()
                            })
                    },
                    move |files| match files {
                        Some(files) => Message::Modal(Event::Chosen(index, files)),
                        None => Message::Nothing,
                    },
                );
            }
            Event::Chosen(index, files) => {
                self.client.dispatch(Action::AddModalFiles { index, files })
            }
            Event::RemoveFile(index, file) => self
                .client
                .dispatch(Action::RemoveModalFile { index, file }),
            Event::Submit => self.client.dispatch(Action::SubmitModal),
            Event::Close => self.client.dispatch(Action::CloseModal),
        }
        self.sync()
    }

    /// Keeps a paragraph's editor and text to read in step with the core's
    /// fields, laying text out again only when it changed; lets go of them
    /// as the form closes.
    pub fn modal_sync(&mut self) {
        let Some(modal) = self.client.state().interaction_modal.as_ref() else {
            if !self.modal.paragraphs.is_empty() || !self.modal.texts.is_empty() {
                self.modal = Shown::default();
            }
            return;
        };
        let fields = &modal.fields;
        self.modal.paragraphs.resize_with(fields.len(), || None);
        self.modal.texts.resize_with(fields.len(), || None);
        let link = rgb(self.theme.mention);
        for (index, field) in fields.iter().enumerate() {
            let paragraph = &mut self.modal.paragraphs[index];
            if field.kind == TEXT_INPUT && field.paragraph {
                match paragraph {
                    Some(content) if content.text() == field.value => {}
                    _ => *paragraph = Some(text_editor::Content::with_text(&field.value)),
                }
            } else {
                *paragraph = None;
            }
            let shown = &mut self.modal.texts[index];
            if field.kind == TEXT_DISPLAY {
                if shown
                    .as_ref()
                    .is_none_or(|(value, _)| *value != field.value)
                {
                    let body = crate::markdown::parse(&field.value, &[], link, "modal", |_| true);
                    *shown = Some((field.value.clone(), body));
                }
            } else {
                *shown = None;
            }
        }
    }

    /// The form over the dimmed window, while one is open.
    pub fn modal_view(&self) -> Option<Element<'_, Message>> {
        let modal = self.client.state().interaction_modal.as_ref()?;
        let theme = &self.theme;
        let saving = modal.saving();
        let close = (!saving).then_some(Message::Modal(Event::Close));
        let head = row![
            container(
                text(modal.title.as_str())
                    .size(18)
                    .font(BOLD)
                    .color(theme.text)
            )
            .width(Length::Fill),
            match close.clone() {
                Some(close) => self.icon_button(Icon::X, "Close", 28.0, 18, close, true),
                None => Space::new().width(28).into(),
            },
        ]
        .align_y(Alignment::Center);
        let mut fields = column![].spacing(16);
        for (index, field) in modal.fields.iter().enumerate() {
            fields = fields.push(self.modal_field(index, field, saving));
        }
        if !modal.supported {
            fields = fields.push(crate::form::error(
                theme,
                "This form has a part Yapfast can't fill yet. Use it in Discord.",
            ));
        }
        let mut body = column![
            head,
            scroll_view(fields)
                .height(Length::Shrink)
                .style(scrollbar(theme)),
        ]
        .spacing(20)
        .padding(20);
        if let Some(error) = &modal.error {
            body = body.push(crate::form::error(theme, error.as_str()));
        }
        let label = if saving { "Submitting…" } else { "Submit" };
        body = body.push(
            row![
                Space::new().width(Length::Fill),
                controls::action(theme, "Cancel", Kind::Ghost, close.clone()),
                controls::action(
                    theme,
                    label,
                    Kind::Primary,
                    (modal.supported && !saving).then_some(Message::Modal(Event::Submit)),
                ),
            ]
            .spacing(8),
        );
        let width = WIDTH.min(self.window.width - 40.0);
        let body = container(body).max_height(self.window.height - 48.0);
        Some(dialog(
            theme,
            body.into(),
            width,
            close.unwrap_or(Message::Nothing),
        ))
    }

    /// One part of the form, by its kind.
    fn modal_field<'a>(
        &'a self,
        index: usize,
        field: &'a ModalField,
        saving: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut part = column![].spacing(8);
        // A checkbox's label is its row's.
        if !field.label.is_empty() && field.kind != CHECKBOX {
            let mut label = column![
                text(required(&field.label, field.required))
                    .size(14)
                    .font(BOLD)
                    .color(theme.text)
            ]
            .spacing(2);
            if !field.description.is_empty() {
                label = label.push(
                    text(field.description.as_str())
                        .size(13)
                        .color(theme.secondary),
                );
            }
            part = part.push(label);
        }
        let edited = move |value: String| Message::Modal(Event::Edited(index, value));
        let shown: Element<'a, Message> = match field.kind {
            TEXT_DISPLAY => match self.modal.texts.get(index).and_then(Option::as_ref) {
                Some((_, body)) => self.body_view(
                    body,
                    usize::MAX,
                    FIRST_PART,
                    Look {
                        size: 15.0,
                        line_height: 20.0,
                        ink: theme.text,
                    },
                ),
                None => Space::new().into(),
            },
            UNSUPPORTED => text("Yapfast can't fill this part yet.")
                .size(14)
                .color(theme.secondary)
                .into(),
            3 | 5..=8 => {
                let summary: String = field
                    .values
                    .iter()
                    .map(|value| {
                        field
                            .options
                            .iter()
                            .find(|option| &option.value == value)
                            .map_or(value.as_str(), |option| option.label.as_str())
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let picked = !summary.is_empty();
                let shown = if picked {
                    summary
                } else if !field.placeholder.is_empty() {
                    field.placeholder.clone()
                } else {
                    "Make a selection".to_owned()
                };
                let boxed = self.select_box(
                    shown,
                    picked,
                    !saving,
                    (!saving).then_some(Message::Modal(Event::OpenSelect(index))),
                );
                let open = self
                    .client
                    .state()
                    .select_menu
                    .as_ref()
                    .is_some_and(|select| select.modal_field == Some(index));
                Popover::new(boxed, open.then(|| self.dropdown()), Side::Below)
                    .width(Width::Same)
                    .gap(4.0)
                    .outside(Message::BotSelect(crate::bot_select::Event::Close))
                    .into()
            }
            TEXT_INPUT if field.paragraph => {
                match self.modal.paragraphs.get(index).and_then(Option::as_ref) {
                    Some(content) => {
                        let (ground, ink, selection, accent) =
                            (theme.sunken, theme.text, theme.selection, theme.accent);
                        let editor = text_editor(content)
                            .height(PARAGRAPH)
                            .size(15)
                            .padding([8, 10])
                            .style(move |_, status| text_editor::Style {
                                background: ground.into(),
                                border: Border {
                                    color: Color { a: 0.8, ..accent },
                                    width: if matches!(status, text_editor::Status::Focused { .. })
                                    {
                                        1.0
                                    } else {
                                        0.0
                                    },
                                    radius: 6.0.into(),
                                },
                                placeholder: ink,
                                value: ink,
                                selection,
                            });
                        if saving {
                            editor.into()
                        } else {
                            editor
                                .on_action(move |action| {
                                    Message::Modal(Event::Paragraph(index, action))
                                })
                                .into()
                        }
                    }
                    None => Space::new().into(),
                }
            }
            TEXT_INPUT => {
                let input =
                    controls::field(theme, "", &field.placeholder, &field.value, false, !saving)
                        .id(iced::widget::Id::unique());
                if saving {
                    input.into()
                } else {
                    input.on_input(edited).into()
                }
            }
            RADIO_GROUP | CHECKBOX_GROUP => {
                let round = field.kind == RADIO_GROUP;
                let mut choices = column![].spacing(2);
                for option in &field.options {
                    choices = choices.push(self.mark_row(
                        option.label.clone(),
                        &option.description,
                        field.values.contains(&option.value),
                        round,
                        (!saving).then(|| edited(option.value.clone())),
                    ));
                }
                choices.into()
            }
            CHECKBOX => self.mark_row(
                required(&field.label, field.required),
                &field.description,
                field.checked,
                false,
                (!saving).then(|| edited(String::new())),
            ),
            FILE_UPLOAD => self.upload_field(index, field, saving),
            _ => Space::new().into(),
        };
        part.push(shown).into()
    }

    /// A file upload: the files chosen, each with Remove, and the box that
    /// chooses more while there's room.
    fn upload_field<'a>(
        &'a self,
        index: usize,
        field: &'a ModalField,
        saving: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut files = column![].spacing(6);
        for (place, path) in field.files.iter().enumerate() {
            let name = path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned();
            let ground = theme.sunken;
            let remove: Element<'_, Message> = if saving {
                Space::new().width(28).into()
            } else {
                self.icon_button(
                    Icon::X,
                    "Remove",
                    28.0,
                    16,
                    Message::Modal(Event::RemoveFile(index, place)),
                    true,
                )
            };
            files = files.push(
                container(
                    row![
                        self.glyph(Icon::File, 18, theme.secondary),
                        container(elide(name).size(14).color(theme.text)).width(Length::Fill),
                        remove,
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding(Padding::ZERO.left(10).right(4))
                .height(FILE_ROW)
                .align_y(Alignment::Center)
                .style(move |_| rounded(ground, 6)),
            );
        }
        if field.files.len() < field.max_values {
            let (ground, edge, accent) = (theme.sunken, theme.edge, theme.accent);
            let face = row![
                self.glyph(Icon::CirclePlus, 20, theme.secondary),
                text("Choose files").size(14).font(MEDIUM).color(theme.text),
                text(format!("Up to {}", field.max_values))
                    .size(13)
                    .color(theme.secondary),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            files = files.push(
                button(
                    container(face)
                        .center_x(Length::Fill)
                        .height(UPLOAD)
                        .align_y(Alignment::Center),
                )
                .padding(0)
                .width(Length::Fill)
                .on_press_maybe((!saving).then_some(Message::Modal(Event::ChooseFiles(index))))
                .style(move |_, status| button::Style {
                    background: Some(ground.into()),
                    border: Border {
                        color: if controls::lights(status) {
                            accent
                        } else {
                            edge
                        },
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..button::Style::default()
                }),
            );
        }
        files.into()
    }

    /// An option of a radio or checkbox group, or a checkbox's own row, as
    /// the Slint client's `MarkRow`: its mark, label and description.
    fn mark_row<'a>(
        &'a self,
        label: String,
        description: &'a str,
        on: bool,
        round: bool,
        message: Option<Message>,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let (accent, edge) = (theme.accent, theme.secondary);
        let inner: Element<'_, Message> = match (on, round) {
            (true, true) => container(Space::new())
                .width(10)
                .height(10)
                .style(move |_| rounded(accent, 5))
                .into(),
            (true, false) => self.glyph(Icon::Check, 14, theme.on_accent).into(),
            (false, _) => Space::new().into(),
        };
        let mark = container(inner)
            .center(20)
            .style(move |_| container::Style {
                background: (on && !round).then(|| accent.into()),
                border: Border {
                    color: if on { accent } else { edge },
                    width: 2.0,
                    radius: if round { 10.0 } else { 5.0 }.into(),
                },
                ..Default::default()
            });
        let mut words = column![text(label).size(15).color(theme.text)];
        if !description.is_empty() {
            words = words.push(text(description).size(13).color(theme.secondary));
        }
        button(
            row![
                container(mark).padding(Padding::ZERO.top(1)),
                container(words).width(Length::Fill)
            ]
            .spacing(10),
        )
        .padding(6)
        .width(Length::Fill)
        .on_press_maybe(message)
        .style(controls::lit_style(None, theme.hover, 6.0))
        .into()
    }
}

/// A label, with a star when its part must be filled.
fn required(label: &str, required: bool) -> String {
    if required {
        format!("{label} *")
    } else {
        label.to_owned()
    }
}
