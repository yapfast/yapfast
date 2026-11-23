//! One of someone's activities on their profile: its heading, its pictures,
//! its lines, and how long it's run or how far a song has got.

use iced::{
    Alignment, Color, Element, Length, Point, Rectangle, Size,
    widget::{Space, button, column, container, row, stack, text},
};

use super::look::{ActivityLook, Inks, clock};
use crate::{
    app::Message,
    controls::{Kind, action, ink_style},
    fonts::{BOLD, SEMIBOLD},
    looks::Status,
    people::StatusInk,
    style::rounded,
    widgets::Pic,
};

/// The large picture, the small one on its ring, and where the ring sits.
const LARGE: f32 = 64.0;
const SMALL: f32 = 22.0;
const RING: f32 = 26.0;
const RING_AT: f32 = 44.0;

/// Now, in Unix seconds, as the activities' times count.
fn now_seconds() -> u64 {
    yapfast_core::clock::now_ms() / 1000
}

/// A song's bar: its `track` across `bounds`, `share` of it filled with
/// `ink`, both round at their ends.
fn progress(
    renderer: &mut crate::widgets::Renderer,
    bounds: Rectangle,
    share: f32,
    track: Color,
    ink: Color,
) {
    use iced::advanced::{Renderer as _, renderer::Quad};
    let round = crate::style::round(bounds.height / 2.0);
    renderer.fill_quad(
        Quad {
            bounds,
            border: round,
            ..Default::default()
        },
        track,
    );
    let width = bounds.width * share.clamp(0.0, 1.0);
    if width > 0.0 {
        renderer.fill_quad(
            Quad {
                bounds: Rectangle { width, ..bounds },
                border: round,
                ..Default::default()
            },
            ink,
        );
    }
}

/// An activity's card; its times move as the clock does.
pub fn activity_card_view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    activity: &'a ActivityLook,
    inks: &Inks,
    menu: Option<bool>,
) -> Element<'a, Message> {
    card_view(ctx, activity, inks, menu, false)
}

/// The account menu uses the same artwork and rich text as a profile,
/// with a compact, green elapsed clock beside a game's controller.
pub fn account_activity_view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    activity: &'a ActivityLook,
    inks: &Inks,
) -> Element<'a, Message> {
    card_view(ctx, activity, inks, None, true)
}

fn card_view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    activity: &'a ActivityLook,
    inks: &Inks,
    menu: Option<bool>,
    compact: bool,
) -> Element<'a, Message> {
    let mut head = row![
        text(activity.heading.as_str())
            .size(12)
            .font(BOLD)
            .color(inks.ink3)
            .wrapping(text::Wrapping::None)
            .width(Length::Fill)
    ]
    .height(16)
    .align_y(Alignment::Center);
    // Discord's more menu, on anyone's card but a bot's; on one's own
    // game it offers its privacy too.
    if let Some(own) = menu {
        let face = ctx
            .look
            .glyph(crate::pictures::Icon::Ellipsis, 14, inks.ink2);
        head = head.push(ctx.look.face_button(
            face,
            20.0,
            "More",
            Message::Profile(super::Event::ActivityMenu(own && activity.game)),
        ));
    }
    let mut card = column![head].spacing(8);
    let mut body = row![].spacing(12).align_y(Alignment::Center);
    if let Some(large) = &activity.large {
        body = body.push(activity_art(ctx, large, activity.small.as_deref(), inks));
    }
    let mut lines = column![].spacing(2).width(Length::Fill);
    let title: Element<'a, Message> = match &activity.link {
        // A song's title opens its page, as in Discord.
        Some(link) => button(
            text(activity.name.as_str())
                .size(14)
                .font(BOLD)
                .wrapping(text::Wrapping::None),
        )
        .padding(0)
        .on_press(Message::Link(link.clone(), Point::ORIGIN))
        .style(ink_style(inks.ink, ctx.look.theme.link))
        .into(),
        None => {
            let status = Status {
                words: activity.name.clone(),
                emoji: activity.name_emoji.clone(),
                ..Status::default()
            };
            ctx.status_line(status, StatusInk::words(14.0, SEMIBOLD, inks.ink))
        }
    };
    lines = lines.push(title);
    for line in [&activity.details, &activity.state] {
        if !line.is_empty() {
            lines = lines.push(
                text(line.as_str())
                    .size(13)
                    .color(inks.ink2)
                    .wrapping(text::Wrapping::None),
            );
        }
    }
    // Its times move by themselves, each second: no message, no view.
    let second = std::time::Duration::from_secs(1);
    match (activity.start, activity.end) {
        // A song, with its start and end, shows how far it's got on a
        // bar, with how long it is either side.
        (Some(start), Some(end)) if end > start => {
            let length = end - start;
            let played = move || now_seconds().saturating_sub(start).min(length);
            let track = if inks.themed {
                Color { a: 0.2, ..inks.ink }
            } else {
                ctx.look.theme.raised
            };
            let activity_themed = ctx
                .themes
                .content(
                    &ctx.look.theme.id,
                    ctx.layout.theme_content.contains(&ctx.look.theme.id),
                )
                .is_some_and(|c| c.activity == yapfast_core::themes::content::Follow::Theme);
            let ink = if activity_themed {
                ctx.look.theme.accent
            } else {
                inks.ink
            };
            let filled = crate::widgets::gauge(second, move |renderer, bounds| {
                let share = played() as f32 / length.max(1) as f32;
                progress(renderer, bounds, share, track, ink);
            })
            .height(3);
            lines = lines.push(
                row![
                    crate::widgets::live_text(second, move || clock(played()))
                        .size(12)
                        .color(inks.ink2),
                    filled,
                    text(clock(length)).size(12).color(inks.ink2),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        (Some(start), _) => {
            if compact {
                let ink = ctx.look.theme.live;
                let mut timer = row![].spacing(4).align_y(Alignment::Center);
                if activity.game {
                    timer = timer.push(ctx.look.glyph(crate::pictures::Icon::Gamepad, 14, ink));
                }
                lines = lines.push(
                    timer.push(
                        crate::widgets::live_text(second, move || {
                            clock(now_seconds().saturating_sub(start))
                        })
                        .size(12)
                        .color(ink),
                    ),
                );
            } else {
                lines = lines.push(
                    crate::widgets::live_text(second, move || {
                        format!("{} elapsed", clock(now_seconds().saturating_sub(start)))
                    })
                    .size(12)
                    .color(inks.ink3),
                );
            }
        }
        (None, Some(end)) => {
            lines = lines.push(
                crate::widgets::live_text(second, move || {
                    format!("{} left", clock(end.saturating_sub(now_seconds())))
                })
                .size(12)
                .color(inks.ink3),
            );
        }
        (None, None) => {}
    }
    body = body.push(lines);
    card = card.push(body);
    if let Some(url) = &activity.url {
        card = card.push(
            action(
                ctx.look.theme,
                "Watch stream",
                Kind::Plain,
                Some(Message::Link(url.clone(), Point::ORIGIN)),
            )
            .small(),
        );
    }
    card.into()
}

/// The large picture, with the small one on a ring of the card's ground
/// at its corner.
fn activity_art<'a>(
    ctx: crate::ctx::Ctx<'a>,
    large: &str,
    small: Option<&str>,
    inks: &Inks,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let raised = theme.raised;
    let activity_themed = ctx
        .themes
        .content(&theme.id, ctx.layout.theme_content.contains(&theme.id))
        .is_some_and(|c| c.activity == yapfast_core::themes::content::Follow::Theme);
    let tile_border = if activity_themed {
        theme.surface_border("card", LARGE / 6.0, 0.0, Color::TRANSPARENT)
    } else {
        crate::style::round(LARGE / 6.0)
    };
    let tile_radius = if activity_themed {
        theme.surface_radius("card", LARGE / 6.0)
    } else {
        LARGE / 6.0
    };
    let square = Size::new(LARGE, LARGE);
    let mut pictures =
        stack![
            container(Space::new())
                .width(LARGE)
                .height(LARGE)
                .style(move |_| iced::widget::container::Style {
                    background: Some(raised.into()),
                    border: tile_border,
                    ..Default::default()
                })
        ];
    if let Some(picture) = ctx.picture(large, LARGE as u32, LARGE as u32, true) {
        pictures = pictures.push(Pic::new(picture, square).radius(tile_radius));
    }
    if let Some(small) = small {
        let ground = inks.ground;
        pictures = pictures.push(crate::controls::placed(
            container(Space::new())
                .width(RING)
                .height(RING)
                .style(move |_| rounded(ground, RING / 2.0)),
            RING_AT,
            RING_AT,
        ));
        if let Some(picture) = ctx.picture(small, SMALL as u32, SMALL as u32, true) {
            let inset = RING_AT + (RING - SMALL) / 2.0;
            pictures = pictures.push(
                Pic::new(picture, Size::new(RING_AT + RING, RING_AT + RING))
                    .at(Rectangle::new(
                        Point::new(inset, inset),
                        Size::new(SMALL, SMALL),
                    ))
                    .radius(SMALL / 2.0),
            );
        }
    }
    container(pictures).width(LARGE).height(LARGE).into()
}
