//! Apps' commands in the composer: `/` opens the list of the commands that
//! fit the channel over the composer, or says they're coming or that none
//! match; with a command picked, its options show as pills over the text,
//! which writes the focused one, and the list offers the values that option
//! may take. Tab and Shift+Tab move between the pills, Enter runs the
//! command (or, on a file option without one, picks the file), Escape stops
//! writing it, and Backspace in the empty first option puts the command's
//! name back. What they show and do is the core's (`application/slash.rs`):
//! the draft carries the `/` query and each option's text, `Action::Send`
//! runs the command.

use iced::{
    Alignment, Element, Length, Padding, Task,
    widget::{button, column, container, operation, row, scrollable::AbsoluteOffset, text},
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
/// The list's scrollable, which the arrows move to keep the lit row in
/// view.
const LIST: &str = "command-list";
/// A pill's height.
const PILL: f32 = 26.0;

/// Enter with a command being written: a file option without its file
/// asks for one; otherwise the core runs it (`Action::Send`).
pub fn command_enter(app: &App) -> Option<Task<Message>> {
    let command = app.client.state().command_draft()?;
    let focused = command.focused;
    let option = command.options.get(focused);
    if option.is_some_and(|option| option.is_attachment() && option.file.is_none()) {
        return Some(command_file(focused));
    }
    None
}

/// Tab or Shift+Tab: the next or the last option.
pub fn command_tab(app: &mut App, back: bool) -> Task<Message> {
    let Some(command) = app.client.state().command_draft() else {
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
    focus_option(app, next)
}

/// The composer writes option `index`, the caret at its text's end.
pub fn focus_option(app: &mut App, index: usize) -> Task<Message> {
    app.composer.suggest.set_index(0);
    app.client.dispatch(Action::FocusCommandOption(index));
    app.sync()
}

/// A file option's file, from the same dialog as the composer's files.
pub fn command_file(index: usize) -> Task<Message> {
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
pub fn close_command(app: &mut App, back: bool) -> bool {
    let state = app.client.state();
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
    app.client.dispatch(Action::CloseCommand(back));
    true
}

/// The pictures the list shows: apps' icons, people's avatars.
pub fn command_pictures(app: &App) -> Vec<String> {
    command_list(app.ctx())
        .map(|(rows, _)| rows.into_iter().filter_map(|row| row.icon).collect())
        .unwrap_or_default()
}

/// The list over the composer, lit at `lit`: eight rows in view, the
/// rest a wheel or the arrows away. Only the rows near the view are
/// built, as a server's apps may bring hundreds of commands.
pub fn command_list_view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    rows: &[CommandRow],
    heading: String,
    lit: usize,
) -> iced::widget::Column<'a, Message> {
    let theme = ctx.look.theme;
    let commands = ctx.state.command_draft().is_none();
    let list = column![
        container(text(heading).size(11).font(BOLD).color(theme.muted))
            .padding([0, 6])
            .height(24)
            .align_y(Alignment::Center)
    ]
    .padding(6);
    if rows.is_empty() {
        return list;
    }
    let total = rows.len() as f32 * ROW;
    let view = view_height(rows.len());
    let scroll = crate::sync::Scroll {
        offset: ctx.composer.suggest.command_offset.min(total - view),
        height: view,
    };
    let (first, last) = scroll.range(rows.len(), ROW, 0.0);
    let mut built = crate::widgets::Rows::new(total);
    for (index, choice) in rows.iter().enumerate().take(last).skip(first) {
        built.push(
            index as u64,
            index as f32 * ROW,
            ROW,
            command_row(ctx, index, choice, commands, index == lit),
        );
    }
    list.push(
        crate::widgets::scroll_view(built)
            .id(LIST)
            .height(view)
            .on_scroll(|viewport| {
                Message::Write(Event::CommandsScrolled(viewport.absolute_offset().y))
            })
            .style(crate::style::scrollbar(theme)),
    )
}

/// The list's height in view for `count` rows.
fn view_height(count: usize) -> f32 {
    count.min(SHOWN) as f32 * ROW
}

/// The arrows moved the light to `lit` of `count`: the list scrolls as
/// little as keeps it in view.
pub fn keep_lit_in_view(app: &mut App, lit: usize, count: usize) -> Task<Message> {
    let view = view_height(count);
    let total = count as f32 * ROW;
    let offset = app
        .composer
        .suggest
        .command_offset
        .min(total - view)
        .max(0.0);
    let top = lit as f32 * ROW;
    let next = if top < offset {
        top
    } else if top + ROW > offset + view {
        top + ROW - view
    } else {
        return Task::none();
    };
    app.composer.suggest.command_offset = next;
    operation::scroll_to(
        LIST,
        AbsoluteOffset {
            x: None,
            y: Some(next),
        },
    )
}

/// The list back at its top, as a command picked gives it over to the
/// values of its options: its rows are built from the top again, and
/// the list itself, still scrolled, is put there with them.
pub fn list_to_top(app: &mut App) -> Task<Message> {
    app.composer.suggest.command_offset = 0.0;
    operation::scroll_to(
        LIST,
        AbsoluteOffset {
            x: None,
            y: Some(0.0),
        },
    )
}

/// A command, or a value of the focused option, in the list.
fn command_row<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    choice: &CommandRow,
    commands: bool,
    on: bool,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let mut face = row![].spacing(10).align_y(Alignment::Center);
    if let Some(icon) = &choice.icon {
        face = face.push(ctx.avatar(Some(icon), &choice.name, 24.0, 12.0, 1.0));
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
    let (active, hover) = (theme.active, theme.hover);
    button(face)
        .padding(Padding::ZERO.left(8).right(10))
        .width(Length::Fill)
        .height(ROW)
        .on_press(Message::Write(Event::Suggestion(index)))
        .style(lit_style(
            on.then_some(active),
            if on { active } else { hover },
            6.0,
        ))
        .into()
}

/// The list's rows and heading, while it's open: the commands of the
/// `/` picker (perhaps none, with why), or the focused option's values.
pub fn command_list(ctx: crate::ctx::Ctx<'_>) -> Option<(Vec<CommandRow>, String)> {
    let rows = ctx.app.command_rows();
    let heading = ctx.app.command_heading(&rows);
    let open = if ctx.state.command_draft().is_some() {
        !rows.is_empty()
    } else {
        !heading.is_empty()
    };
    open.then_some((rows, heading))
}

/// The command's name and its options' pills over the composer's
/// text, wrapping onto as many rows as they take; an empty row without
/// a command.
pub fn command_pills<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let state = ctx.state;
    let Some(command) = state.command_draft() else {
        return container(row![].wrap()).into();
    };
    let theme = ctx.look.theme;
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
            face = face.push(ctx.look.glyph(Icon::File, 14, theme.muted));
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
                border: crate::style::edge(crate::style::faded(edge, alpha), 4.0),
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
pub fn command_placeholder(ctx: crate::ctx::Ctx<'_>) -> Option<String> {
    let command = ctx.state.command_draft()?;
    Some(match command.options.get(command.focused) {
        None => format!("Press Enter to run /{}", command.name),
        Some(option) if option.is_attachment() => {
            format!("Press Enter to choose a file for {}", option.name)
        }
        Some(option) => option.description.clone(),
    })
}
