//! The lines across the top of the conversation: the preview's note that
//! its data is sample data, and what the core says needs saying
//! (`AppState::notice`), until dismissed.

use iced::{
    Alignment, Element, Length,
    widget::{container, row, text},
};
use yapfast_core::application::Action;

use crate::{
    app::{App, Message},
    controls::{Kind, action},
    fonts::MEDIUM,
    pictures::Icon,
};

pub fn dismiss_notice(app: &mut App) -> iced::Task<Message> {
    app.client.dispatch(Action::DismissNotice);
    app.sync()
}

/// The preview's note, that the GPU failed to start, Matrix's verification
/// and the core's notice, stacked over the conversation or the call.
pub fn view<'a>(ctx: crate::ctx::Ctx<'a>) -> impl Iterator<Item = Element<'a, Message>> {
    [
        preview_banner(ctx),
        gpu_banner(ctx),
        crate::signin::verify_banner(ctx),
        notice_banner(ctx),
        rail_undo_banner(ctx),
    ]
    .into_iter()
    .flatten()
}

/// That the GPU failed to start, so Yapfast draws on the CPU, until
/// dismissed.
pub fn gpu_banner<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    ctx.gpu_notice.then(|| {
        banner(
            ctx,
            crate::graphics::NOTICE,
            Message::Shell(crate::shell::Event::DismissGpuNotice),
        )
    })
}

/// The core's notice, while there's one.
pub fn notice_banner<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let notice = ctx.state.notice.as_deref()?;
    Some(banner(ctx, notice, Message::DismissNotice))
}

/// After a drop on the rail, for six seconds: Undo, which puts the server
/// list back as it was, and Dismiss.
pub fn rail_undo_banner<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    ctx.state.rail_undo.as_ref()?;
    let theme = ctx.look.theme;
    let raised = theme.raised;
    Some(
        container(
            row![
                ctx.look.glyph(Icon::Folder, 18, theme.accent),
                text("Your server list changed.")
                    .size(theme.type_size("body", 15.0))
                    .font(theme.font("body", crate::fonts::REGULAR))
                    .color(theme.text)
                    .width(Length::Fill),
                action(
                    theme,
                    "Undo",
                    Kind::Ghost,
                    Some(Message::Rail(crate::rail::Event::Undo))
                )
                .small(),
                action(
                    theme,
                    "Dismiss",
                    Kind::Ghost,
                    Some(Message::Rail(crate::rail::Event::UndoGone(None)))
                )
                .small(),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding(iced::Padding {
            top: 6.0,
            bottom: 6.0,
            left: 16.0,
            right: 8.0,
        })
        .width(Length::Fill)
        .style(move |_| crate::style::fill(raised))
        .into(),
    )
}

/// The preview's note, which stays.
pub fn preview_banner<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    if !ctx.state.preview {
        return None;
    }
    let theme = ctx.look.theme;
    let raised = banner_ground(ctx);
    let note = container(
        row![
            ctx.look.glyph(Icon::TriangleAlert, 16, theme.accent),
            text("Local preview: sample data. Nothing leaves this computer.")
                .size(14)
                .font(MEDIUM)
                .color(theme.secondary)
                .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([7, 16])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(raised),
        ..Default::default()
    });
    Some(note.into())
}

/// A notice with its Dismiss button (`dismissed`).
pub fn banner<'a>(
    ctx: crate::ctx::Ctx<'a>,
    words: &'a str,
    dismissed: Message,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let raised = banner_ground(ctx);
    let note = container(
        row![
            ctx.look.glyph(Icon::TriangleAlert, 18, theme.accent),
            text(words).size(15).color(theme.text).width(Length::Fill),
            action(theme, "Dismiss", Kind::Ghost, Some(dismissed)).small(),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding(iced::Padding {
        top: 6.0,
        bottom: 6.0,
        left: 16.0,
        right: 8.0,
    })
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(raised),
        ..Default::default()
    });
    note.into()
}

/// A banner's ground: the raised paint, a theme's gradient included, as
/// Slint's `Banner` (`Theme.raised-fill`).
fn banner_ground(ctx: crate::ctx::Ctx<'_>) -> iced::Background {
    ctx.look.theme.surface_paint("raised")
}
