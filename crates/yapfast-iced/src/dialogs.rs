//! Dialogs over the dimmed window, drawn in the kit's frame
//! ([`crate::dialog`]): Delete's confirmation and the question before
//! following a masked link.

use iced::{
    Element, Length, Task,
    keyboard::key,
    widget::{Space, column, container, row, text},
};
use yapfast_core::application::Action;

use crate::{
    app::{App, Message},
    controls::{Kind, action},
    ctx::Ctx,
    dialog::{CONFIRM_WIDTH, dialog},
    fonts::{BOLD, MEDIUM},
};

/// The dialogs' answers.
#[derive(Clone, Debug)]
pub enum Event {
    /// The delete confirmation's answers.
    ConfirmDelete,
    CancelDelete,
    /// The question before a masked link: visit it, or go back.
    Leave(bool),
}

/// What each answer does.
pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::ConfirmDelete => {
            app.client.dispatch(Action::ConfirmDelete);
            Task::batch([app.sync(), crate::composer::refocus(app)])
        }
        Event::CancelDelete => {
            app.client.dispatch(Action::CancelDelete);
            Task::batch([app.sync(), crate::composer::refocus(app)])
        }
        Event::Leave(visit) => leave(app, visit),
    }
}

/// The confirmation's keys: Escape cancels, Enter does what the focused
/// button does, Tab and the side arrows go round the two buttons.
pub fn confirm_key(app: &mut App, named: Option<key::Named>) -> Task<Message> {
    match named {
        Some(key::Named::Escape) => app.update(Message::Dialog(Event::CancelDelete)),
        Some(key::Named::Enter) => {
            if app.confirm_focus {
                app.update(Message::Dialog(Event::ConfirmDelete))
            } else {
                app.update(Message::Dialog(Event::CancelDelete))
            }
        }
        Some(key::Named::Tab | key::Named::ArrowLeft | key::Named::ArrowRight) => {
            app.confirm_focus = !app.confirm_focus;
            Task::none()
        }
        _ => Task::none(),
    }
}

/// Delete's confirmation over the dimmed window: the question, the
/// message as it shows, Cancel and Delete (focused when `focus`, as it is
/// first). A click on the dimmed window cancels; nothing under it takes a
/// key.
pub fn delete_view(focus: bool, cx: Ctx<'_>) -> Option<Element<'_, Message>> {
    let state = cx.state;
    let item = state.confirm_delete.as_ref()?;
    let message = state.message(item);
    let theme = cx.look.theme;
    let line = theme.line;
    let mut body = column![
        text("Delete Message").size(20).font(BOLD).color(theme.text),
        text("Are you sure you want to delete this message?")
            .size(15)
            .color(theme.secondary),
    ]
    .spacing(12)
    .padding(16);
    if let Some(message) = message {
        body = body.push(
            container(
                column![
                    text(message.author.clone()).font(MEDIUM).color(theme.text),
                    text(message.body.clone())
                        .color(theme.text)
                        .wrapping(text::Wrapping::WordOrGlyph),
                ]
                .spacing(2),
            )
            .padding(12)
            .width(Length::Fill)
            .max_height(160)
            .clip(true)
            .style(move |_| container::Style {
                border: crate::style::edge(line, 6.0),
                ..Default::default()
            }),
        );
    }
    body = body.push(
        row![
            Space::new().width(Length::Fill),
            action(
                theme,
                "Cancel",
                Kind::Ghost,
                Some(Message::Dialog(Event::CancelDelete))
            )
            .ring(!focus),
            action(
                theme,
                "Delete",
                Kind::Danger,
                Some(Message::Dialog(Event::ConfirmDelete))
            )
            .ring(focus),
        ]
        .spacing(8),
    );
    Some(dialog(
        theme,
        body.into(),
        CONFIRM_WIDTH,
        Message::Dialog(Event::CancelDelete),
    ))
}

/// The question before following a masked link (one whose words aren't
/// where it goes), as the Slint client's `LeavingDialog`: where it goes,
/// Go back and Visit site. Escape goes back, Enter visits.
pub fn leaving_view<'a>(link: Option<&'a str>, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let link = link?;
    let theme = cx.look.theme;
    let sunken = theme.sunken;
    let body = column![
        text("Leaving Yapfast")
            .size(18)
            .font(BOLD)
            .color(theme.text),
        text("This link goes to").size(15).color(theme.secondary),
        text(yapfast_core::links::host(link))
            .size(17)
            .font(BOLD)
            .color(theme.text),
        container(
            text(link)
                .size(14)
                .color(theme.secondary)
                .wrapping(text::Wrapping::Glyph),
        )
        .padding([8, 10])
        .width(Length::Fill)
        .style(move |_| crate::style::rounded(sunken, 6)),
        row![
            Space::new().width(Length::Fill),
            action(
                theme,
                "Go back",
                Kind::Ghost,
                Some(Message::Dialog(Event::Leave(false)))
            ),
            action(
                theme,
                "Visit site",
                Kind::Primary,
                Some(Message::Dialog(Event::Leave(true)))
            ),
        ]
        .spacing(8),
    ]
    .spacing(12)
    .padding(20);
    Some(dialog(
        theme,
        body.into(),
        CONFIRM_WIDTH,
        Message::Dialog(Event::Leave(false)),
    ))
}

/// The question's answer: the link opens, or not.
pub fn leave(app: &mut App, visit: bool) -> Task<Message> {
    if let Some(link) = app.leaving.take()
        && visit
    {
        crate::browser::open(&link);
    }
    Task::none()
}

/// An emoji to pick in a form, as the Slint client's custom status and
/// poll answers have it: 40 px on a sunken ground, the emoji chosen
/// (`key`) or a smile with a plus, and with one, a small badge on its
/// corner that takes it off.
pub fn emoji_slot<'a>(
    ctx: crate::ctx::Ctx<'a>,
    key: Option<&'a str>,
    pick: Message,
    remove: Message,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let face: Element<'a, Message> = match key {
        Some(key) => crate::suggest::emoji_face(ctx, key, 22),
        None => ctx
            .look
            .glyph(crate::pictures::Icon::SmilePlus, 20, theme.secondary)
            .into(),
    };
    let pick = iced::widget::button(container(face).center(40))
        .padding(0)
        .width(40)
        .height(40)
        .on_press(pick)
        .style(crate::controls::lit_style(
            Some(theme.sunken),
            theme.hover,
            6.0,
        ));
    if key.is_none() {
        return pick.into();
    }
    let (faint, danger, raised) = (theme.faint, theme.danger, theme.raised);
    let remove = iced::widget::button(
        container(ctx.look.glyph(crate::pictures::Icon::X, 10, theme.text)).center(14),
    )
    .padding(0)
    .width(18)
    .height(18)
    .on_press(remove)
    .style(move |_, status| iced::widget::button::Style {
        background: Some(
            if crate::controls::lights(status) {
                danger
            } else {
                faint
            }
            .into(),
        ),
        border: iced::Border {
            color: raised,
            width: 2.0,
            radius: 9.0.into(),
        },
        ..Default::default()
    });
    iced::widget::stack![pick, crate::controls::placed(remove, 28.0, 0.0)].into()
}
