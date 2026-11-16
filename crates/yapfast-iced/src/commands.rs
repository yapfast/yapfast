//! Apps' commands in the composer, as the Slint client's `CommandList` and
//! `CommandPills` (`ui/app_commands.slint`): `/` opens the list of the
//! commands that fit the channel over the composer, or says they're coming
//! or that none match; with a command picked, its options show as pills
//! over the text, which writes the focused one, and the list offers the
//! values that option may take. Tab and Shift+Tab move between the pills,
//! Enter runs the command (or, on a file option without one, picks the
//! file), Escape stops writing it, and Backspace in the empty first option
//! puts the command's name back. What they show and do is the core's
//! (`application/slash.rs`): the draft carries the `/` query and each
//! option's text, `Action::Send` runs the command.

use iced::{
    Alignment, Border, Element, Length, Padding, Task,
    widget::{button, column, container, row, text},
};
use yapfast_core::application::{Action, CommandRow};

use crate::{
    app::{App, Message},
    composer::Event,
    controls::lit_style,
    fonts::{BOLD, SEMIBOLD},
    pictures::Icon,
    style::rounded,
};

/// A row of the list, and how many show at once.
const ROW: f32 = 44.0;
const SHOWN: usize = 8;
/// A pill's height.
const PILL: f32 = 26.0;

impl App {
    /// Enter with a command being written: a file option without its file
    /// asks for one; otherwise the core runs it (`Action::Send`).
    pub fn command_enter(&mut self) -> Option<Task<Message>> {
        let command = self.client.state().command_draft()?;
        let focused = command.focused;
        let option = command.options.get(focused);
        if option.is_some_and(|option| option.is_attachment() && option.file.is_none()) {
            return Some(self.command_file(focused));
        }
        None
    }

    /// Tab or Shift+Tab: the next or the last option.
    pub fn command_tab(&mut self, back: bool) -> Task<Message> {
        let Some(command) = self.client.state().command_draft() else {
            return Task::none();
        };
        let count = command.options.len();
        if count == 0 {
            return Task::none();
        }
        let next = if back {
            (command.focused + count - 1) % count
        } else {
            (command.focused + 1) % count
        };
        self.focus_option(next)
    }

    /// The composer writes option `index`, the caret at its text's end.
    pub fn focus_option(&mut self, index: usize) -> Task<Message> {
        self.composer.suggest.set_index(0);
        self.client.dispatch(Action::FocusCommandOption(index));
        self.sync()
    }

    /// A file option's file, from the same dialog as the composer's files.
    pub fn command_file(&mut self, index: usize) -> Task<Message> {
        Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Choose a file")
                    .pick_file()
                    .await
                    .map(|file| file.path().to_string_lossy().into_owned())
            },
            move |path| match path {
                Some(path) => Message::Write(Event::CommandFileChosen(index, path)),
                None => Message::Nothing,
            },
        )
    }

    /// Escape or Backspace at the first option's start: the list closes,
    /// then the command stops being written (with `back`, its name goes
    /// back in the composer). Whether there was a command or a list.
    pub fn close_command(&mut self, back: bool) -> bool {
        let state = self.client.state();
        if state.command_draft().is_none() && state.slash_query().is_none() {
            return false;
        }
        if back
            && state
                .command_draft()
                .is_none_or(|command| command.focused != 0)
        {
            return false;
        }
        self.client.dispatch(Action::CloseCommand(back));
        true
    }

    /// The pictures the list shows: apps' icons, people's avatars.
    pub fn command_pictures(&self) -> Vec<String> {
        self.ctx()
            .command_list()
            .map(|(rows, _)| rows.into_iter().filter_map(|row| row.icon).collect())
            .unwrap_or_default()
    }
}

impl<'a> crate::ctx::Ctx<'a> {
    /// The list over the composer, lit at `lit`; the eight rows around the
    /// lit one show.
    pub fn command_list_view(
        self,
        rows: &[CommandRow],
        heading: String,
        lit: usize,
    ) -> iced::widget::Column<'a, Message> {
        let theme = self.look.theme;
        let commands = self.state.command_draft().is_none();
        let mut list = column![
            container(text(heading).size(11).font(BOLD).color(theme.muted))
                .padding([0, 6])
                .height(24)
                .align_y(Alignment::Center)
        ]
        .padding(6);
        let first = (lit + 1).saturating_sub(SHOWN);
        for (index, choice) in rows.iter().enumerate().skip(first).take(SHOWN) {
            let mut face = row![].spacing(10).align_y(Alignment::Center);
            if let Some(icon) = &choice.icon {
                face = face.push(self.avatar(Some(icon), &choice.name, 24.0, 12.0, 1.0));
            } else if commands {
                let ground = theme.accent_soft;
                face = face.push(
                    container(text("/").size(13).font(BOLD).color(theme.accent))
                        .center(24)
                        .style(move |_| rounded(ground, 12)),
                );
            }
            let mut words = column![
                text(format!(
                    "{}{}",
                    if commands { "/" } else { "" },
                    choice.name
                ))
                .size(14)
                .font(SEMIBOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None)
            ];
            if !choice.description.is_empty() {
                words = words.push(
                    text(choice.description.clone())
                        .size(12)
                        .color(theme.muted)
                        .wrapping(text::Wrapping::None),
                );
            }
            face = face.push(container(words).width(Length::Fill).clip(true));
            if !choice.app.is_empty() {
                face = face.push(
                    container(
                        text(choice.app.clone())
                            .size(12)
                            .font(SEMIBOLD)
                            .color(theme.muted)
                            .wrapping(text::Wrapping::None),
                    )
                    .max_width(160)
                    .clip(true),
                );
            }
            let on = index == lit;
            let (active, hover) = (theme.active, theme.hover);
            list = list.push(
                button(face)
                    .padding(Padding::ZERO.left(8).right(10))
                    .width(Length::Fill)
                    .height(ROW)
                    .on_press(Message::Write(Event::Suggestion(index)))
                    .style(lit_style(
                        on.then_some(active),
                        if on { active } else { hover },
                        6.0,
                    )),
            );
        }
        list
    }
}

impl<'a> crate::ctx::Ctx<'a> {
    /// The list's rows and heading, while it's open: the commands of the
    /// `/` picker (perhaps none, with why), or the focused option's values.
    pub fn command_list(self) -> Option<(Vec<CommandRow>, String)> {
        let rows = self.app.command_rows();
        let heading = self.app.command_heading(&rows);
        let open = if self.state.command_draft().is_some() {
            !rows.is_empty()
        } else {
            !heading.is_empty()
        };
        open.then_some((rows, heading))
    }
}

impl<'a> crate::ctx::Ctx<'a> {
    /// The command's name and its options' pills over the composer's
    /// text, wrapping onto as many rows as they take; an empty row without
    /// a command.
    pub fn command_pills(self) -> Element<'a, Message> {
        let state = self.state;
        let Some(command) = state.command_draft() else {
            return container(row![].wrap()).into();
        };
        let theme = self.look.theme;
        let errors = state.option_errors(command);
        let soft = theme.accent_soft;
        let mut pills = row![
            container(
                text(format!("/{}", command.name))
                    .size(14)
                    .font(BOLD)
                    .color(theme.accent)
                    .wrapping(text::Wrapping::None)
            )
            .padding([0, 6])
            .height(PILL)
            .align_y(Alignment::Center)
            .style(move |_| rounded(soft, 4))
        ]
        .spacing(6);
        for (index, (option, error)) in command.options.iter().zip(errors).enumerate() {
            let focused = index == command.focused;
            let value = option.shown();
            let mut face = row![].spacing(4).align_y(Alignment::Center);
            if option.is_attachment() {
                face = face.push(self.look.glyph(Icon::File, 14, theme.muted));
            }
            face = face.push(
                text(format!("{}:", option.name))
                    .size(13)
                    .font(SEMIBOLD)
                    .color(theme.muted),
            );
            if !value.is_empty() {
                face = face.push(text(value.clone()).size(13).color(theme.text));
            }
            if let Some(error) = &error {
                face = face.push(text(error.clone()).size(12).color(theme.danger_text));
            }
            let edge = if error.is_some() {
                theme.danger
            } else if focused {
                theme.accent
            } else {
                theme.line
            };
            let ground = if focused { theme.active } else { theme.raised };
            let faint = !option.required && value.is_empty() && !focused;
            let alpha = if faint { 0.6 } else { 1.0 };
            let message = if option.is_attachment() {
                Event::CommandFile(index)
            } else {
                Event::CommandOption(index)
            };
            pills = pills.push(
                button(
                    container(face)
                        .height(Length::Fill)
                        .align_y(Alignment::Center),
                )
                .padding([0, 6])
                .height(PILL)
                .on_press(Message::Write(message))
                .style(move |_, _| button::Style {
                    background: Some(crate::style::faded(ground, alpha).into()),
                    border: Border {
                        color: crate::style::faded(edge, alpha),
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                }),
            );
        }
        container(pills.wrap().vertical_spacing(6))
            .padding(Padding::ZERO.left(6).top(crate::composer::PAD - 4.0))
            .width(Length::Fill)
            .into()
    }

    /// What the empty field says while a command is written.
    pub fn command_placeholder(self) -> Option<String> {
        let command = self.state.command_draft()?;
        Some(match command.options.get(command.focused) {
            None => format!("Press Enter to run /{}", command.name),
            Some(option) if option.is_attachment() => {
                format!("Press Enter to choose a file for {}", option.name)
            }
            Some(option) => option.description.clone(),
        })
    }
}
