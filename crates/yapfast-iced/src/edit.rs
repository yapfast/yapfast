//! Editing a message of one's own in its row, as Discord's client and the
//! Slint client's `InlineEditor` do: the text becomes a field there, with
//! "escape to cancel • enter to save" under it and the emoji button beside
//! it. The field is its own text (`App::edit`, the core's `Editing`), apart
//! from the room's draft and a reply being written, which stay as they
//! were. Enter saves (the core asks first when the text is empty, and
//! closes an edit that changed nothing), Escape or the hint's cancel closes
//! it. It takes `@` and `:name` suggestions and the emoji picker.

use iced::{
    Alignment, Border, Element, Length, Padding, Task,
    widget::{Id, button, column, container, operation, row, scrollable::AbsoluteOffset, text},
};
use yapfast_core::application::Action;

use crate::{
    app::{App, Message, NOBODY},
    composer::{BUTTON_INSET, EDIT, Event, PAD, Writing},
    controls::ink_style,
    field::{self, Field},
    fonts::MEDIUM,
    picker::Target,
    pictures::Icon,
    popover::{Popover, Side},
    style::rounded,
    timeline::TIMELINE,
};

/// Room kept under a row as its edit opens, so the field is in view as it
/// grows from the message's text.
const EDIT_ROOM: f32 = 64.0;

impl App {
    /// Edits a message of one's own in its row: its text in the edit's
    /// field, which takes the keys, and the row scrolled into view.
    pub fn begin_edit(&mut self, item: String) -> Task<Message> {
        self.client.dispatch(Action::Edit(item));
        self.take_edit()
    }

    /// The edit the core began, if it did: its text in the field, its row
    /// in view, the keys in the field.
    pub fn take_edit(&mut self) -> Task<Message> {
        let Some(edit) = self.client.state().edit() else {
            return Task::none();
        };
        let (item, text) = (edit.item.clone(), edit.text.clone());
        self.editing = Some(item.clone());
        self.edit.reset(&text);
        self.suggest.clear(&mut self.client);
        let reveal = self
            .messages
            .iter()
            .position(|message| message.item == item)
            .map_or_else(Task::none, |index| self.reveal_row(index));
        Task::batch([self.sync(), reveal, operation::focus(EDIT)])
    }

    /// Enter in the edit, or the hint's save: the core saves it, or asks
    /// whether to delete a message emptied.
    pub fn save_edit(&mut self) -> Task<Message> {
        self.client.dispatch(Action::SetEditText(self.edit.text()));
        self.client.dispatch(Action::SaveEdit);
        self.suggest.clear(&mut self.client);
        let focus = if self.client.state().confirm_delete.is_some() {
            self.confirm_focus = true;
            operation::focus(NOBODY)
        } else {
            operation::focus(crate::composer::COMPOSER)
        };
        Task::batch([self.sync(), focus])
    }

    /// Scrolls row `index` into view, with room under it for its edit.
    pub(crate) fn reveal_row(&mut self, index: usize) -> Task<Message> {
        let total = self.total_height();
        let view_top = self.view_top();
        let view_bottom = view_top + self.timeline_height;
        let (top, bottom) = (self.tops[index], self.tops[index + 1] + EDIT_ROOM);
        self.from_end = if bottom > view_bottom {
            (total - bottom).max(0.0)
        } else if top < view_top {
            (total - self.timeline_height - top).max(0.0)
        } else {
            return Task::none();
        };
        self.at_end = self.from_end <= 2.0;
        operation::scroll_to(
            TIMELINE,
            AbsoluteOffset {
                x: None,
                y: Some(self.from_end),
            },
        )
    }

    /// The field in place of a message's text while it's edited.
    pub fn inline_edit(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let field = Field::new(
            &self.edit,
            Id::new(EDIT),
            String::new(),
            |event| Message::Write(Event::Field(Writing::Edit, event)),
            field::Colors {
                ground: iced::Color::TRANSPARENT,
                text: theme.text,
                placeholder: theme.placeholder,
                selection: theme.selection,
            },
        )
        .padding(Padding {
            top: PAD,
            right: 6.0,
            bottom: PAD,
            left: 6.0,
        })
        .suggesting(self.suggesting(Writing::Edit));
        let tint = match &self.picker {
            Some(picker) if picker.target == Target::Edit => theme.text,
            _ => theme.secondary,
        };
        let emoji = self.composer_button(
            Icon::Smile,
            "Add an emoji",
            18,
            tint,
            Some(Message::Write(Event::EditEmoji)),
        );
        let line = row![
            field,
            container(emoji).padding(Padding::ZERO.bottom(BUTTON_INSET))
        ]
        .spacing(6)
        .align_y(Alignment::End)
        .padding(Padding::ZERO.left(10).right(12));
        let (ground, edge) = (theme.composer, theme.outline_field);
        let frame = container(line)
            .width(Length::Fill)
            .height(Length::Shrink)
            .style(move |_| container::Style {
                border: Border {
                    color: edge,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..rounded(ground, 8)
            });
        let frame = Popover::new(frame, self.suggestions_view(Writing::Edit), Side::Above);
        // Discord's hint: 12 px in the text's colour, its two actions links.
        let words = |words: &'static str| text(words).size(12).font(MEDIUM).color(theme.text);
        let link = |words: &'static str, event: Event| {
            button(text(words).size(12).font(MEDIUM))
                .padding(0)
                .on_press(Message::Write(event))
                .style(ink_style(theme.link, theme.text))
        };
        let hint = row![
            words("escape to"),
            link("cancel", Event::CancelEdit),
            words("\u{2022} enter to"),
            link("save", Event::SaveEdit),
        ]
        .spacing(3);
        column![container(frame).width(Length::Fill), hint]
            .spacing(5)
            .padding(Padding::ZERO.bottom(4))
            .into()
    }
}

impl App {
    /// The edit's field's top in the window, roughly: its row's foot less
    /// the hint, the field and what follows the text.
    pub fn edit_top(&self) -> Option<f32> {
        let index = self
            .messages
            .iter()
            .position(|message| Some(&message.item) == self.editing.as_ref())?;
        let foot = crate::header::HEIGHT + 1.0 + self.tops[index + 1] - self.view_top();
        let reactions = if self.messages[index].reactions.is_empty() {
            0.0
        } else {
            30.0
        };
        Some(foot - 4.0 - 17.0 - 5.0 - 4.0 - reactions - self.edit.height())
    }
}
