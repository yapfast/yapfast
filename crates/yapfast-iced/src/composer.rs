//! The composer under the timeline, as the Slint client's: the message
//! request's answer in its place, the reply's bar over it (with whether the
//! reply mentions its author), the field (`field::Field`) between the + and
//! the emoji button, suggestions over it (`suggest.rs`), and the hint under
//! it. The draft is the core's, per channel (`Action::SetDraft`), which
//! tells the room this account is typing; the field follows it when the
//! core changes it, as when a sent message empties it. A message of one's
//! own is edited in its row (`edit.rs`), in a field of its own beside the
//! draft.

use iced::{
    Alignment, Border, Element, Length, Padding, Task,
    widget::{Id, Space, button, column, container, operation, row, text, tooltip},
};
use yapfast_core::{
    application::{Action, Compose},
    network::Network,
};

use crate::{
    app::{App, Message, NOBODY},
    controls::{self, lit_style},
    field::{self, Field, MenuEdit},
    fonts::BOLD,
    menus::{Menu, MenuAct, MenuRow},
    picker::{Tab, Target},
    pictures::Icon,
    popover::{Popover, Side},
    style::{corners, faded, rounded},
};

/// The composer's field and the inline edit's, for the app's focus tasks.
pub const COMPOSER: &str = "composer";
pub const EDIT: &str = "edit";
/// The reply bar's height.
pub const BAR: f32 = 36.0;
/// The composer's least height (`ClientStyle.composer-height`), the room
/// over and under its text, beside it, and under its buttons.
pub const HEIGHT: f32 = 58.0;
pub const PAD: f32 = 17.0;
pub const GUTTER: f32 = 8.0;
pub const BUTTON_INSET: f32 = 13.0;
/// The hint's line under the composer.
const HINT: f32 = 22.0;

/// Which field is written in: the room's draft, or a message's edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Writing {
    #[default]
    Draft,
    Edit,
}

/// What the composer's parts send.
#[derive(Clone, Debug)]
pub enum Event {
    /// A field's keys and text.
    Field(Writing, field::Event),
    /// The clipboard's text, for a field's paste.
    Pasted(Writing, Option<String>),
    /// A suggestion clicked, by its place.
    Suggestion(usize),
    CancelReply,
    TogglePing,
    /// The message request's Accept (true) or Ignore.
    Answer(bool),
    /// The +.
    Plus,
    /// The edit's hint: cancel or save; its emoji button.
    CancelEdit,
    SaveEdit,
    EditEmoji,
    /// A message of one's own that didn't go: send it again, or not; an
    /// answer only this account sees, dismissed.
    Retry(String),
    CancelSend(String),
    Dismiss(String),
    /// Files to go with the message.
    Uploads(crate::uploads::Event),
    /// An app command's pill clicked: its option, or a file option's file;
    /// the file chosen.
    CommandOption(usize),
    CommandFile(usize),
    CommandFileChosen(usize, String),
}

/// What the composer shows for the open room, from the core's state.
pub struct Look {
    pub enabled: bool,
    /// Discord's one bar in place of the composer, where this account may
    /// not write.
    pub locked: bool,
    /// The line under it: why it can't send, or nothing.
    pub hint: String,
    /// A message request's title, answered in place of the composer.
    pub request: Option<String>,
    pub attach: bool,
    /// The + opens a menu (threads or polls can start here).
    pub plus: bool,
}

impl App {
    /// What the composer shows for the open room, as the Slint client's
    /// `show_composer` works it out.
    pub fn composer_look(&self) -> Look {
        let state = self.client.state();
        let room = state.selected_room();
        let discord = room.is_some_and(|room| room.network == Network::Discord);
        let enabled = state.room_ready
            && !state.busy
            && state.shows_text()
            && room.is_some_and(|room| {
                if discord {
                    state.writable(room)
                } else {
                    state.preview || room.encrypted
                }
            });
        let timed_out = room
            .and_then(|room| room.timed_out_until)
            .filter(|_| discord)
            .map(|until| {
                let at = chrono::DateTime::from_timestamp((until / 1000) as i64, 0)
                    .map(|at| {
                        at.with_timezone(&chrono::Local)
                            .format("%-d %B %Y %H:%M")
                            .to_string()
                    })
                    .unwrap_or_default();
                format!("You're timed out in this server until {at}.")
            });
        let request = room.filter(|room| room.request).map(|room| {
            if room.category == yapfast_core::discord::SPAM {
                format!("Discord thinks {} may be spam", room.name)
            } else {
                format!("{} wants to message you", room.name)
            }
        });
        let locked = timed_out.is_none()
            && !state.preview
            && discord
            && room.is_some_and(|room| !room.request && (room.read_only || room.locked));
        let hint = if let Some(timed_out) = timed_out {
            timed_out
        } else if room.is_none() {
            "Choose a DM or server channel to write a message.".to_owned()
        } else if state.preview || locked || request.is_some() {
            String::new()
        } else if discord && !state.discord_online {
            "Discord is disconnected. Yapfast is reconnecting.".to_owned()
        } else if !discord && !state.trusted {
            "Sending is locked until this device is verified.".to_owned()
        } else {
            String::new()
        };
        Look {
            enabled,
            locked,
            hint,
            request,
            attach: state.can_attach(),
            plus: self.client.app.can_start_threads() || self.client.app.can_create_polls(),
        }
    }

    pub fn composer_view(&self) -> Element<'_, Message> {
        let look = self.composer_look();
        let mut compose = column![];
        if let Some(title) = &look.request {
            compose = compose.push(self.request_bar(title));
        } else {
            if let Some(Compose::Reply {
                author, body, ping, ..
            }) = self.client.state().compose()
            {
                let label = format!(
                    "Replying to {author}: {}",
                    body.lines().next().unwrap_or_default()
                );
                compose = compose.push(self.reply_bar(label, *ping));
            }
            compose = compose.push(if look.locked {
                self.locked_bar()
            } else {
                self.composer_box(&look)
            });
        }
        let foot: Element<'_, Message> = if look.hint.is_empty() {
            Space::new().height(GUTTER).into()
        } else {
            container(text(look.hint).size(13).color(self.theme.muted))
                .height(HINT)
                .padding(Padding::ZERO.left(4))
                .align_y(Alignment::Center)
                .into()
        };
        container(compose.push(foot))
            .padding(Padding::ZERO.left(GUTTER).right(GUTTER))
            .into()
    }

    /// The box with the field and its buttons, and the suggestions over it.
    fn composer_box(&self, look: &Look) -> Element<'_, Message> {
        let theme = &self.theme;
        let header = &self.header;
        let replying = self.client.state().compose().is_some();
        // Discord's words: `Message @sam` in a DM, `Message #general` in a
        // channel, and a group's or thread's own name as it is.
        let name = &header.name;
        let placeholder = if replying {
            "Write a reply".to_owned()
        } else {
            match (header.direct, header.one_to_one, header.thread) {
                (true, true, _) => format!("Message @{name}"),
                (false, _, false) => format!("Message #{name}"),
                _ => format!("Message {name}"),
            }
        };
        let commanding = self.client.state().command_draft().is_some();
        let placeholder = self.command_placeholder().unwrap_or(placeholder);
        let field = Field::new(
            &self.draft,
            Id::new(COMPOSER),
            placeholder,
            |event| Message::Write(Event::Field(Writing::Draft, event)),
            field::Colors {
                ground: iced::Color::TRANSPARENT,
                text: theme.text,
                placeholder: theme.placeholder,
                selection: theme.selection,
            },
        )
        .padding(Padding {
            // Under the pills, 10 px from them, as Slint's.
            top: if commanding { 10.0 } else { PAD },
            right: 6.0,
            bottom: PAD,
            left: 6.0,
        })
        .enabled(look.enabled)
        .pastes()
        .tabs(commanding)
        .suggesting(self.suggesting(Writing::Draft));
        // Always in a column after the same kind of widget, so the field
        // keeps its state (the keys) as the pills come and go.
        let pills = self.command_pills();
        let field = column![pills, field];
        // A tab's button is lit while its tab shows, the emoji's also while
        // the emoji picker alone is open over the composer.
        let shown = self
            .picker
            .as_ref()
            .filter(|picker| picker.target == Target::Composer)
            .map(|picker| picker.tab);
        let tint = |tab: Tab| {
            if shown == Some(tab) {
                theme.text
            } else {
                theme.secondary
            }
        };
        let plus = self.composer_button(
            Icon::Add,
            if look.plus {
                "More options"
            } else {
                "Attach files"
            },
            27,
            theme.secondary,
            (look.attach || look.plus).then_some(Message::Write(Event::Plus)),
        );
        let opens = |tab: Tab| {
            look.enabled
                .then_some(Message::OpenPicker(Target::Composer, tab))
        };
        // As Discord's: the GIFs, the stickers, then the emoji, in one
        // picker, in a Discord room.
        let mut buttons = row![].spacing(6).align_y(Alignment::End);
        let state = self.client.state();
        if state.can_pick_gifs() {
            buttons = buttons.push(lift(self.composer_button(
                Icon::Gif,
                "Open GIF picker",
                22,
                tint(Tab::Gifs),
                opens(Tab::Gifs),
            )));
        }
        if state.can_pick_stickers() {
            buttons = buttons.push(lift(self.composer_button(
                Icon::Sticker,
                "Open sticker picker",
                22,
                tint(Tab::Stickers),
                opens(Tab::Stickers),
            )));
        }
        let emoji = buttons.push(lift(self.composer_button(
            Icon::Smile,
            "Add an emoji",
            22,
            tint(Tab::Emoji),
            opens(Tab::Emoji),
        )));
        let line = row![lift(plus), field, emoji]
            .spacing(6)
            .align_y(Alignment::End)
            .padding(Padding::ZERO.left(10).right(8));
        let ground = if look.enabled {
            theme.composer
        } else {
            theme.field
        };
        // Discord's outline: faint, and a notch lighter with the keys.
        let edge = if self.composer_focused {
            faded(theme.muted, 0.55)
        } else {
            theme.outline_field
        };
        let radius = if replying {
            corners([0.0, 0.0, 10.0, 10.0])
        } else {
            10.0.into()
        };
        let mut inside = column![];
        if let Some(tray) = self.tray() {
            inside = inside.push(tray);
        }
        let frame = container(inside.push(line))
            .width(Length::Fill)
            .style(move |_| container::Style {
                border: Border {
                    color: edge,
                    width: 1.0,
                    radius,
                },
                ..rounded(ground, radius)
            });
        Popover::new(frame, self.suggestions_view(Writing::Draft), Side::Above).into()
    }

    /// An icon button tinted as the Slint client's `IconButton` with `tint`,
    /// greyed without a message.
    pub fn composer_button(
        &self,
        icon: Icon,
        label: &'static str,
        glyph: u32,
        tint: iced::Color,
        message: Option<Message>,
    ) -> Element<'static, Message> {
        let tint = if message.is_some() {
            tint
        } else {
            faded(tint, 0.45)
        };
        let face = self.glyph(icon, glyph, tint).boxed(32);
        let face = button(face)
            .padding(0)
            .width(32)
            .height(32)
            .on_press_maybe(message)
            .style(lit_style(None, self.theme.wash(0.1), 6.0));
        tooltip(
            face,
            controls::tip(&self.theme, label),
            tooltip::Position::Top,
        )
        .gap(6)
        .into()
    }

    /// The bar over the field while replying: who to, the mention's toggle,
    /// and the cancel.
    fn reply_bar(&self, label: String, ping: bool) -> Element<'_, Message> {
        let theme = &self.theme;
        let ground = theme.sidebar;
        let ink = if ping { theme.accent } else { theme.muted };
        let toggle = button(
            text(if ping { "@ On" } else { "@ Off" })
                .size(13)
                .font(BOLD)
                .color(ink),
        )
        .padding([0, 8])
        .height(28)
        .on_press(Message::Write(Event::TogglePing))
        .style(lit_style(None, theme.hover, 6.0));
        let toggle = tooltip(
            container(toggle).height(28).align_y(Alignment::Center),
            controls::tip(theme, "Mention the person you reply to"),
            tooltip::Position::Top,
        )
        .gap(6);
        container(
            row![
                self.glyph(Icon::Reply, 16, theme.muted),
                text(label)
                    .size(14)
                    .color(theme.secondary)
                    .wrapping(text::Wrapping::None)
                    .width(Length::Fill),
                toggle,
                self.icon_button(
                    Icon::X,
                    "Cancel reply",
                    28.0,
                    16,
                    Message::Write(Event::CancelReply),
                    false
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(Padding::ZERO.left(14).right(4))
        .height(BAR)
        .clip(true)
        .align_y(Alignment::Center)
        .style(move |_| rounded(ground, corners([10.0, 10.0, 0.0, 0.0])))
        .into()
    }

    /// Discord's bar in place of the composer where this account may not
    /// write.
    fn locked_bar(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let (ground, edge) = (theme.field, theme.outline_field);
        container(
            text("You do not have permission to send messages in this channel.")
                .size(16)
                .color(theme.muted)
                .wrapping(text::Wrapping::None),
        )
        .padding(Padding::ZERO.left(16).right(16))
        .width(Length::Fill)
        .height(HEIGHT)
        .clip(true)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            border: Border {
                color: edge,
                width: 1.0,
                radius: 10.0.into(),
            },
            ..rounded(ground, 10)
        })
        .into()
    }

    /// A message request's answer, as the Slint client's `RequestBar`.
    fn request_bar(&self, title: &str) -> Element<'_, Message> {
        let theme = &self.theme;
        let ground = theme.field;
        let words = column![
            text(title.to_owned())
                .size(15)
                .font(BOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None),
            text("Accept to reply. Ignoring removes it without telling them.")
                .size(14)
                .color(theme.secondary)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(2)
        .width(Length::Fill)
        .clip(true);
        container(
            row![
                words,
                controls::action(
                    theme,
                    "Ignore",
                    controls::Kind::Plain,
                    Some(Message::Write(Event::Answer(false)))
                ),
                controls::action(
                    theme,
                    "Accept",
                    controls::Kind::Primary,
                    Some(Message::Write(Event::Answer(true)))
                ),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 12.0,
            right: 12.0,
            bottom: 12.0,
            left: 16.0,
        })
        .width(Length::Fill)
        .style(move |_| rounded(ground, 10))
        .into()
    }

    /// The composer box's top in the window, where what opens over it sits.
    /// The top of the composer's buttons, which stay on its last line:
    /// the expression picker opens 8 px over them.
    pub fn composer_buttons_top(&self) -> f32 {
        self.window.height - self.composer_foot() - BUTTON_INSET - 32.0
    }

    /// The room under the composer's box: its hint, or the gutter.
    fn composer_foot(&self) -> f32 {
        if self.composer_look().hint.is_empty() {
            GUTTER
        } else {
            HINT
        }
    }

    pub fn composer_top(&self) -> f32 {
        let foot = self.composer_foot();
        let tray = if self.client.state().staged().is_empty() {
            0.0
        } else {
            crate::uploads::TRAY
        };
        self.window.height - foot - self.draft.height().max(HEIGHT) - tray
    }

    pub fn write_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Field(which, event) => self.field_event(which, event),
            Event::Pasted(which, pasted) => {
                if pasted.is_some() {
                    let draft = self.writing(which);
                    draft.act(MenuEdit::Paste, pasted);
                    let (text, caret) = (draft.text(), draft.caret());
                    self.edited(which, text, caret);
                }
                Task::none()
            }
            Event::Suggestion(index) => {
                let which = self.suggest.field;
                self.suggest.set_index(index);
                Task::batch([self.choose_suggestion(which), self.refocus()])
            }
            Event::CancelReply => {
                self.client.dispatch(Action::CancelCompose);
                Task::batch([self.sync(), self.refocus()])
            }
            Event::TogglePing => {
                self.client
                    .dispatch(Action::ToggleReplyPing { aside: false });
                Task::batch([self.sync(), self.refocus()])
            }
            Event::Answer(accept) => {
                self.client.dispatch(Action::AnswerRequest(accept));
                Task::batch([self.sync(), self.refocus()])
            }
            Event::Plus => self.plus(),
            Event::CancelEdit => {
                self.client.dispatch(Action::CancelEdit);
                Task::batch([self.sync(), operation::focus(COMPOSER)])
            }
            Event::SaveEdit => self.save_edit(),
            Event::EditEmoji => self.open_picker(Target::Edit, Tab::Emoji),
            Event::Retry(item) => {
                self.client.dispatch(Action::Retry(item));
                self.sync()
            }
            Event::CancelSend(item) => {
                self.client.dispatch(Action::CancelSend(item));
                self.sync()
            }
            Event::Dismiss(item) => {
                self.client.dispatch(Action::DismissEphemeral(item));
                self.sync()
            }
            Event::Uploads(event) => self.uploads_event(event),
            Event::CommandOption(index) => Task::batch([self.focus_option(index), self.refocus()]),
            Event::CommandFile(index) => self.command_file(index),
            Event::CommandFileChosen(index, value) => {
                self.client
                    .dispatch(Action::SetCommandOption { index, value });
                self.sync()
            }
        }
    }

    /// The draft's or the edit's text field.
    pub fn writing(&self, which: Writing) -> &field::Draft {
        match which {
            Writing::Draft => &self.draft,
            Writing::Edit => &self.edit,
        }
    }

    /// The keys go back to the field being written in: the edit while one
    /// is open, else the composer.
    pub fn refocus(&self) -> Task<Message> {
        operation::focus(if self.editing.is_some() {
            EDIT
        } else {
            COMPOSER
        })
    }

    fn field_event(&mut self, which: Writing, event: field::Event) -> Task<Message> {
        match event {
            field::Event::Edited(text, caret) => {
                self.edited(which, text, caret);
                Task::none()
            }
            field::Event::Moved(caret) => {
                let text = self.writing(which).text();
                self.caret_moved(which, &text, caret);
                Task::none()
            }
            field::Event::Send => match which {
                Writing::Draft => self.send(),
                Writing::Edit => self.save_edit(),
            },
            field::Event::Step(step) => {
                self.suggest.step(step);
                Task::none()
            }
            field::Event::Choose => self.choose_suggestion(which),
            field::Event::Escape => {
                if self.dismiss_suggestions() {
                    return Task::none();
                }
                // Escape in the edit's field ends the edit.
                if which == Writing::Edit {
                    self.client.dispatch(Action::CancelEdit);
                    return Task::batch([self.sync(), operation::focus(COMPOSER)]);
                }
                self.escape()
            }
            field::Event::Emoji => self.open_picker(
                match which {
                    Writing::Draft => Target::Composer,
                    Writing::Edit => Target::Edit,
                },
                Tab::Emoji,
            ),
            // Ctrl+G opens the GIFs, as Discord's; again, it closes them.
            field::Event::Gifs if self.client.state().can_pick_gifs() => {
                self.open_picker(Target::Composer, Tab::Gifs)
            }
            field::Event::Gifs => Task::none(),
            field::Event::Menu(at) => {
                let row = MenuRow::new;
                let mut rows = Vec::new();
                if self.writing(which).selection().is_some() {
                    rows.push(row(
                        MenuAct::Field(which, MenuEdit::Cut),
                        "Cut",
                        Icon::Scissors,
                    ));
                    rows.push(row(
                        MenuAct::Field(which, MenuEdit::Copy),
                        "Copy",
                        Icon::Copy,
                    ));
                }
                rows.push(row(
                    MenuAct::Field(which, MenuEdit::Paste),
                    "Paste",
                    Icon::Paste,
                ));
                rows.push(row(
                    MenuAct::Field(which, MenuEdit::SelectAll),
                    "Select All",
                    Icon::TextSelect,
                ));
                self.menu = Some(Menu::new(at, rows).width(crate::menus::FIELD_WIDTH));
                operation::focus(NOBODY)
            }
            // Up in an empty composer edits the last message of one's own,
            // as the Slint client's `edit-last`.
            field::Event::UpFromEmpty => {
                if which == Writing::Edit {
                    return Task::none();
                }
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
            field::Event::UpToMessages => self.step_into_messages(),
            field::Event::Paste => self.paste(which),
            field::Event::Focus(focused) => {
                if which == Writing::Draft {
                    self.composer_focused = focused;
                }
                if focused {
                    self.leave_messages();
                }
                Task::none()
            }
            field::Event::Tab(back) => self.command_tab(back),
            field::Event::BackFromEmpty => {
                if self.close_command(true) {
                    return self.sync();
                }
                Task::none()
            }
        }
    }

    /// Ctrl+V and the menu's Paste: in the composer, files copied in a
    /// file manager or a picture go with the message, as the Slint client's
    /// `paste` finds them, and otherwise the text is pasted; the edit's
    /// field takes text alone.
    fn paste(&mut self, which: Writing) -> Task<Message> {
        use crate::uploads::Event as Uploads;
        if which == Writing::Draft
            && self.client.state().can_attach()
            && let Some(read) = crate::paste_reader()
        {
            return Task::perform(
                async move { tokio::task::spawn_blocking(read).await.ok().flatten() },
                |files| Message::Write(Event::Uploads(Uploads::Clipboard(files))),
            );
        }
        iced::clipboard::read().map(move |text| Message::Write(Event::Pasted(which, text)))
    }

    /// The + opens Discord's menu where threads or polls can start, and the
    /// file dialog elsewhere.
    fn plus(&mut self) -> Task<Message> {
        let look = self.composer_look();
        if !look.plus {
            return self.attach();
        }
        let mut rows = vec![MenuRow {
            disabled: !look.attach,
            subtitle: None,
            check: crate::controls::Check::None,
            ..MenuRow::new(MenuAct::Upload, "Upload a File", Icon::File)
        }];
        // The thread's and the poll's forms come with the settings and
        // forms milestone: greyed until then.
        if self.client.app.can_start_threads() {
            rows.push(MenuRow {
                disabled: true,
                subtitle: None,
                check: crate::controls::Check::None,
                ..MenuRow::new(MenuAct::NewThread, "Create Thread", Icon::Thread)
            });
        }
        if self.client.app.can_create_polls() {
            rows.push(MenuRow {
                disabled: true,
                subtitle: None,
                check: crate::controls::Check::None,
                ..MenuRow::new(MenuAct::NewPoll, "Create Poll", Icon::ChartBar)
            });
        }
        let at = iced::Point::new(
            crate::rail::WIDTH + crate::sidebar::WIDTH + GUTTER + 10.0,
            self.composer_top() - 8.0,
        );
        self.menu = Some(Menu::new(at, rows).anchor(crate::widgets::Anchor::Bottom));
        operation::focus(NOBODY)
    }

    /// Enter: the draft sent. The core empties the draft once the message is
    /// sent; `sync` follows it.
    fn send(&mut self) -> Task<Message> {
        if self.client.state().selected.is_none() {
            return Task::none();
        }
        if let Some(task) = self.command_enter() {
            return task;
        }
        let commanding = self.client.state().command_draft().is_some();
        if !commanding {
            self.client.dispatch(Action::SetDraft(self.draft.text()));
        }
        // Sending goes to the newest message, as Discord does, so the sent
        // one shows; from a window of older messages, once the newest have
        // replaced it.
        let mut to_end = Task::none();
        if self.client.state().can_send() {
            if self.client.state().detached {
                self.latest_due = true;
            } else {
                to_end = self.go_to_end();
            }
        }
        self.client.dispatch(Action::Send);
        self.suggest.clear(&mut self.client);
        Task::batch([to_end, self.sync()])
    }

    /// A field's menu chose Cut, Copy, Paste or Select All.
    pub fn field_menu(&mut self, which: Writing, edit: MenuEdit) -> Task<Message> {
        self.menu = None;
        let focus = operation::focus(match which {
            Writing::Draft => COMPOSER,
            Writing::Edit => EDIT,
        });
        if edit == MenuEdit::Paste {
            return Task::batch([focus, self.paste(which)]);
        }
        let draft = self.writing(which);
        let copied = draft.act(edit, None);
        if edit == MenuEdit::Cut {
            let (text, caret) = (draft.text(), draft.caret());
            self.edited(which, text, caret);
        }
        match copied {
            Some(copied) => Task::batch([focus, iced::clipboard::write(copied)]),
            None => focus,
        }
    }

    /// A field's text changed: the core's draft or edit follows, as the
    /// Slint composer's `edited` does, and so do the suggestions. A draft
    /// changed tells the room this account is typing (the core's).
    pub fn edited(&mut self, which: Writing, text: String, caret: usize) {
        self.keep_text(which, text.clone());
        let draft = self.writing(which);
        if draft.text() != text {
            let (text, caret) = (draft.text(), draft.caret());
            self.caret_moved(which, &text, caret);
        } else {
            self.caret_moved(which, &text, caret);
        }
    }

    /// The core's draft or edit takes a field's text.
    fn keep_text(&mut self, which: Writing, text: String) {
        match which {
            Writing::Edit => self.client.dispatch(Action::SetEditText(text)),
            Writing::Draft if self.client.state().selected.is_some() => {
                self.client.dispatch(Action::SetDraft(text.clone()));
                self.draft_known = self.client.state().draft().to_owned();
                // The core took the text as something else, as a command's
                // name and a space picks it: the field shows what it kept.
                if self.draft_known != text {
                    self.draft.reset(&self.draft_known);
                }
            }
            Writing::Draft => {}
        }
    }

    /// Puts `text` in a field with the caret at `caret`, and in the core's
    /// draft or edit with it.
    pub fn replace_text(&mut self, which: Writing, text: String, caret: usize) {
        self.writing(which).replace(&text, caret);
        self.keep_text(which, text);
    }

    /// An emoji's text goes in at the field's caret, as the Slint client's
    /// `insert-emoji` puts it.
    pub fn insert(&mut self, which: Writing, inserted: &str) {
        let draft = self.writing(which);
        let text = draft.text();
        let at = draft.caret().min(text.len());
        let text = format!("{}{inserted}{}", &text[..at], &text[at..]);
        let caret = at + inserted.len();
        self.replace_text(which, text.clone(), caret);
        self.caret_moved(which, &text, caret);
    }
}

/// A composer button, kept off the box's foot by the Slint client's inset,
/// so it stays on the last line as the field grows.
fn lift<'a>(button: Element<'a, Message>) -> Element<'a, Message> {
    container(button)
        .padding(Padding::ZERO.bottom(BUTTON_INSET))
        .into()
}
