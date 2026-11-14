//! The composer under the timeline, as the Slint client's: the field
//! (`field::Field`) between the attach and emoji buttons, the reply or edit
//! bar over it, mention suggestions over that, and the emoji picker the
//! emoji button opens. The draft is the core's, per channel
//! (`Action::SetDraft`); the field follows it when the core changes it, as
//! when a sent message empties it.

use iced::{
    Alignment, Border, Element, Length, Padding, Point, Size, Task,
    widget::{Id, button, column, container, operation, row, scrollable::AbsoluteOffset, text},
};
use yapfast_core::{
    application::{Action, Compose},
    mention,
};

use crate::{
    app::{App, Message, NOBODY},
    field::{self, Field, MenuEdit},
    fonts::{BOLD, SEMIBOLD},
    menus::{Menu, MenuAct, MenuRow},
    picker::{self, Picker, Target},
    pictures::Icon,
    rail, sidebar,
    style::{
        EDGE, FIELD, LIT, MENU, MUTED, PANEL, SECONDARY, SECONDARY_INK, SELECTION, TEXT, corners,
        faded, hex, raised_shadow, rounded,
    },
    widgets::{Anchor, Pic, Place},
};

/// The composer's field, for the app's focus tasks.
pub const COMPOSER: &str = "composer";
/// The reply and edit bars' height.
const BAR: f32 = 36.0;
/// A mention suggestion under the pointer.
const POINTED: u32 = 0x222329;

impl App {
    pub fn composer_view(&self) -> Element<'_, Message> {
        let (name, direct) = &self.header;
        let placeholder = if *direct {
            format!("Message @{name}")
        } else {
            format!("Message #{name}")
        };
        let field = Field::new(
            &self.draft,
            Id::new(COMPOSER),
            placeholder,
            Message::Composer,
            field::Colors {
                ground: iced::Color::TRANSPARENT,
                text: hex(TEXT),
                placeholder: hex(MUTED),
                selection: SELECTION,
            },
        )
        .suggesting(self.mentions_shown());
        let emoji = self.icon_button(
            Icon::Smile,
            "Add an emoji",
            32.0,
            22,
            Message::OpenPicker(Target::Composer),
            false,
        );
        let attach = container(Pic::new(
            self.icon(Icon::Plus, 22, SECONDARY),
            Size::new(22.0, 22.0),
        ))
        .center(32);
        let field = container(
            row![
                container(attach).height(48).align_y(Alignment::Center),
                field,
                container(emoji).height(48).align_y(Alignment::Center)
            ]
            .align_y(Alignment::End)
            .padding([0, 6]),
        )
        .width(Length::Fill)
        .style(|_| rounded(hex(FIELD), 8));
        let mut compose = column![];
        if self.editing.is_some() {
            compose = compose.push(self.bar(
                Icon::Pencil,
                "Editing message · escape to cancel · enter to save".to_owned(),
                "Cancel edit",
                Message::CancelEdit,
            ));
        } else if let Some(Compose::Reply { author, body, .. }) = self.client.state().compose() {
            let label = format!(
                "Replying to {author}: {}",
                body.lines().next().unwrap_or_default()
            );
            compose =
                compose.push(self.bar(Icon::Reply, label, "Cancel reply", Message::CancelReply));
        }
        container(compose.push(field))
            .padding(Padding {
                top: 0.0,
                right: 8.0,
                bottom: 8.0,
                left: 8.0,
            })
            .into()
    }

    /// The bar over the field while replying or editing, with its cancel.
    fn bar(
        &self,
        icon: Icon,
        label: String,
        cancel: &'static str,
        message: Message,
    ) -> Element<'_, Message> {
        container(
            row![
                Pic::new(self.icon(icon, 16, 0x8d919c), Size::new(16.0, 16.0)),
                text(label)
                    .size(14)
                    .color(hex(SECONDARY_INK))
                    .wrapping(text::Wrapping::None)
                    .width(Length::Fill),
                self.icon_button(Icon::X, cancel, 28.0, 16, message, false),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 0.0,
            right: 4.0,
            bottom: 0.0,
            left: 14.0,
        })
        .height(BAR)
        .clip(true)
        .align_y(Alignment::Center)
        .style(|_| rounded(hex(PANEL), corners([10.0, 10.0, 0.0, 0.0])))
        .into()
    }

    pub fn composer_event(&mut self, event: field::Event) -> Task<Message> {
        match event {
            field::Event::Edited(text, caret) => {
                self.edited(text, caret);
                Task::none()
            }
            field::Event::Moved(caret) => {
                let text = self.draft.text();
                self.caret_moved(&text, caret);
                Task::none()
            }
            field::Event::Send => self.send(),
            field::Event::Step(step) => {
                let count = self.client.state().mention_suggestions().len();
                if count > 0 {
                    self.mention_index =
                        (self.mention_index as i32 + step).rem_euclid(count as i32) as usize;
                }
                Task::none()
            }
            field::Event::Choose => self.choose_mention(),
            field::Event::Escape => {
                if self.client.state().mention_query.is_some() {
                    self.mention_dismissed = self.mention_at;
                    self.mention_at = None;
                    self.client.dispatch(Action::MentionQuery(None));
                    return Task::none();
                }
                // Escape in the edit's field ends the edit, whatever else
                // is open below it.
                if self.editing.is_some() && self.menu.is_none() && self.picker.is_none() {
                    self.client.dispatch(Action::CancelEdit);
                    return Task::batch([self.sync(), operation::focus(COMPOSER)]);
                }
                self.escape()
            }
            field::Event::Emoji => self.open_picker(Target::Composer),
            field::Event::Menu(at) => {
                let row = |act, label, icon| MenuRow {
                    act,
                    label,
                    icon,
                    danger: false,
                    separated: false,
                };
                let mut rows = Vec::new();
                if self.draft.selection().is_some() {
                    rows.push(row(MenuAct::Field(MenuEdit::Cut), "Cut", Icon::Scissors));
                    rows.push(row(MenuAct::Field(MenuEdit::Copy), "Copy", Icon::Copy));
                }
                rows.push(row(MenuAct::Field(MenuEdit::Paste), "Paste", Icon::Paste));
                rows.push(row(
                    MenuAct::Field(MenuEdit::SelectAll),
                    "Select All",
                    Icon::TextSelect,
                ));
                self.menu = Some(Menu {
                    at,
                    rows,
                    lit: None,
                    item: None,
                    reactions: false,
                });
                operation::focus(NOBODY)
            }
            // Up in an empty composer edits the last message of one's own,
            // as the Slint client's `edit-last`.
            field::Event::UpFromEmpty => {
                let last = self
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.can_edit && !message.deleted)
                    .map(|message| message.item.clone());
                match last {
                    Some(item) => self.begin_edit(item),
                    None => Task::none(),
                }
            }
        }
    }

    /// Enter: the edit is saved, or the draft sent. The core empties the
    /// draft once the message is sent; `sync` follows it.
    fn send(&mut self) -> Task<Message> {
        if self.client.state().selected.is_none() {
            return Task::none();
        }
        if self.editing.is_some() {
            self.client.dispatch(Action::SetEditText(self.draft.text()));
            self.client.dispatch(Action::SaveEdit);
        } else {
            self.client.dispatch(Action::SetDraft(self.draft.text()));
            self.client.dispatch(Action::Send);
        }
        self.mention_at = None;
        self.sync()
    }

    /// The composer's menu chose Cut, Copy, Paste or Select All.
    pub fn composer_menu(&mut self, edit: MenuEdit) -> Task<Message> {
        self.menu = None;
        let focus = operation::focus(COMPOSER);
        if edit == MenuEdit::Paste {
            return Task::batch([focus, iced::clipboard::read().map(Message::Pasted)]);
        }
        let copied = self.draft.act(edit, None);
        if edit == MenuEdit::Cut {
            self.edited(self.draft.text(), self.draft.caret());
        }
        match copied {
            Some(copied) => Task::batch([focus, iced::clipboard::write(copied)]),
            None => focus,
        }
    }

    /// The composer's text changed: the core's draft follows, as the Slint
    /// composer's `edited` does, and so do the mention suggestions.
    pub fn edited(&mut self, text: String, caret: usize) {
        if self.editing.is_some() {
            self.client.dispatch(Action::SetEditText(text.clone()));
        } else if self.client.state().selected.is_some() {
            self.client.dispatch(Action::SetDraft(text.clone()));
            self.draft_known = self.client.state().draft().to_owned();
        }
        self.caret_moved(&text, caret);
    }

    /// The `@` or `#` word at the caret, as the Slint client's
    /// `composer_caret` finds it, unless Escape closed its suggestions.
    fn caret_moved(&mut self, text: &str, caret: usize) {
        let found = mention::query(text, caret);
        if found.is_none() {
            self.mention_dismissed = None;
        }
        let found = found.filter(|(at, ..)| self.mention_dismissed != Some(*at));
        self.mention_at = found.map(|(at, ..)| at);
        let query = found.map(|(_, sigil, query)| (sigil, query.to_owned()));
        if self.client.state().mention_query != query {
            self.mention_index = 0;
            self.client.dispatch(Action::MentionQuery(query));
        }
    }

    /// The lit suggestion goes in place of the word typed, with a space
    /// after it, as the Slint client's `mention-chosen` writes it.
    pub fn choose_mention(&mut self) -> Task<Message> {
        let state = self.client.state();
        let (Some(chosen), Some(at), Some((sigil, query))) = (
            state
                .mention_suggestions()
                .into_iter()
                .nth(self.mention_index),
            self.mention_at,
            state.mention_query.clone(),
        ) else {
            return Task::none();
        };
        let text = self.draft.text();
        let end = at + 1 + query.len();
        if text.get(at..end) != Some(format!("{sigil}{query}").as_str()) {
            return Task::none();
        }
        let inserted = format!("{}{} ", chosen.sigil(), chosen.name);
        let text = format!("{}{inserted}{}", &text[..at], &text[end..]);
        let caret = at + inserted.len();
        self.mention_at = None;
        self.client.dispatch(Action::MentionQuery(None));
        self.draft.replace(&text, caret);
        if self.editing.is_some() {
            self.client.dispatch(Action::SetEditText(text));
        } else {
            self.client.dispatch(Action::SetDraft(text));
            self.draft_known = self.client.state().draft().to_owned();
        }
        Task::none()
    }

    /// Edits a message of one's own in the composer: its text there, the
    /// draft kept by the core meanwhile and back when the edit ends. The
    /// Slint client edits in place in the message (M1).
    pub fn begin_edit(&mut self, item: String) -> Task<Message> {
        self.client.dispatch(Action::Edit(item.clone()));
        let Some(edit) = self.client.state().edit() else {
            return Task::none();
        };
        let text = edit.text.clone();
        self.editing = Some(item);
        self.mention_at = None;
        self.draft.reset(&text);
        Task::batch([self.sync(), operation::focus(COMPOSER)])
    }

    pub fn mentions_shown(&self) -> bool {
        self.mention_at.is_some() && !self.client.state().mention_suggestions().is_empty()
    }

    /// Mention suggestions just above the composer, as wide as it, as the
    /// Slint client's `MentionList`: members, then roles, each under its
    /// heading; Up and Down light one, Tab, Enter or a click takes it.
    pub fn mentions_view(&self) -> Option<Element<'_, Message>> {
        self.mention_at?;
        let state = self.client.state();
        let suggestions = state.mention_suggestions();
        if suggestions.is_empty() {
            return None;
        }
        let mut list = column![].padding(6);
        for (index, candidate) in suggestions.iter().enumerate() {
            if index == 0 || suggestions[index - 1].group != candidate.group {
                let heading = match (candidate.group, candidate.channel) {
                    (false, _) => "MEMBERS",
                    (true, false) => "ROLES",
                    (true, true) => "CHANNELS",
                };
                list = list.push(
                    container(text(heading).size(11).font(BOLD).color(hex(MUTED)))
                        .padding([0, 6])
                        .height(24)
                        .align_y(Alignment::Center),
                );
            }
            let color = candidate.color.map(hex);
            let mark: Element<'_, Message> = if candidate.group {
                let ink = color.unwrap_or(hex(SECONDARY_INK));
                let ground = faded(color.unwrap_or(hex(MUTED)), 0.2);
                container(
                    text(candidate.sigil().to_string())
                        .size(13)
                        .font(BOLD)
                        .color(ink),
                )
                .center(24)
                .style(move |_| rounded(ground, 12))
                .into()
            } else {
                self.avatar(
                    candidate.avatar.as_deref(),
                    &candidate.name,
                    24.0,
                    12.0,
                    1.0,
                )
            };
            let detail = match candidate.mention.as_str() {
                "@everyone" => "Everyone in this channel".to_owned(),
                "@here" => "Everyone online here".to_owned(),
                _ => candidate.username.clone().unwrap_or_default(),
            };
            let lit = index == self.mention_index;
            list = list.push(
                button(
                    row![
                        mark,
                        text(candidate.name.clone())
                            .size(14)
                            .font(SEMIBOLD)
                            .color(color.unwrap_or(hex(TEXT)))
                            .wrapping(text::Wrapping::None),
                        text(detail)
                            .size(13)
                            .color(hex(MUTED))
                            .wrapping(text::Wrapping::None),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding(Padding {
                    top: 0.0,
                    right: 10.0,
                    bottom: 0.0,
                    left: 8.0,
                })
                .width(Length::Fill)
                .height(36)
                .on_press(Message::Mention(index))
                .style(move |_, status| button::Style {
                    background: if lit {
                        Some(hex(LIT).into())
                    } else {
                        matches!(status, button::Status::Hovered | button::Status::Pressed)
                            .then(|| hex(POINTED).into())
                    },
                    border: Border::default().rounded(6),
                    ..Default::default()
                }),
            );
        }
        let bar = if state.compose().is_some() { BAR } else { 0.0 };
        let left = rail::WIDTH + sidebar::WIDTH + 8.0;
        let width = self.chat_right() - left - 8.0;
        let bottom = self.window.height - 8.0 - self.draft.height() - bar - 8.0;
        let frame = container(list).width(width).style(|_| container::Style {
            background: Some(hex(MENU).into()),
            border: Border {
                color: hex(EDGE),
                width: 1.0,
                radius: 8.0.into(),
            },
            shadow: raised_shadow(),
            ..Default::default()
        });
        Some(
            Place::new(frame, Point::new(left, bottom), 0.0)
                .anchor(Anchor::Bottom)
                .into(),
        )
    }

    /// Opens the picker for the composer or a reaction; the same again
    /// closes it.
    pub fn open_picker(&mut self, target: Target) -> Task<Message> {
        self.menu = None;
        if self
            .picker
            .as_ref()
            .is_some_and(|picker| picker.target == target)
        {
            return self.close_picker();
        }
        self.picker = Some(Picker::new(target));
        operation::focus(picker::FIELD)
    }

    /// The picker closes and lets go of its pictures, and the heap they
    /// took goes back, as the Slint client's `let_go_after_picker` does.
    pub fn close_picker(&mut self) -> Task<Message> {
        self.picker = None;
        crate::control::trim();
        operation::focus(COMPOSER)
    }

    pub fn picker_event(&mut self, event: picker::Event) -> Task<Message> {
        let Some(picker) = &mut self.picker else {
            return Task::none();
        };
        match picker.update(event) {
            picker::Out::None => Task::none(),
            picker::Out::Scroll(offset) => operation::scroll_to(
                picker::GRID,
                AbsoluteOffset {
                    x: None,
                    y: Some(offset),
                },
            ),
            picker::Out::Close => self.close_picker(),
            picker::Out::Pick(key) => {
                let target = picker.target.clone();
                let close = self.close_picker();
                match target {
                    Target::Composer => {
                        // At the caret the composer had, as the Slint
                        // client's `insert-emoji` puts it.
                        let text = self.draft.text();
                        let at = self.draft.caret().min(text.len());
                        let text = format!("{}{key}{}", &text[..at], &text[at..]);
                        self.draft.replace(&text, at + key.len());
                        self.edited(text, at + key.len());
                        close
                    }
                    Target::React(item, _) => {
                        self.client.dispatch(Action::React {
                            item,
                            key: key.to_owned(),
                        });
                        Task::batch([close, self.sync()])
                    }
                }
            }
        }
    }
}
