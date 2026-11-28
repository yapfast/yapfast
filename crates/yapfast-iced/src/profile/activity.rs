//! One of someone's activities on their profile, as Discord's client lays
//! its card out: a heading, the picture with the small one at its corner,
//! the title and up to two lines, then small lines with an icon (the
//! party, an episode, how long it's run) or a song's bar.

use iced::{
    Alignment, Color, Element, Length, Point, Rectangle, Size,
    widget::{Space, button, column, container, row, stack, text},
};

use super::look::{ActivityLook, Inks, clock, timer};
use crate::{
    app::Message,
    controls::{Kind, action, ink_style},
    elide::elide,
    fonts::{MEDIUM, SEMIBOLD},
    looks::Status,
    people::StatusInk,
    pictures::Icon,
    style::rounded,
    widgets::Pic,
};

/// The picture and the small one at its corner: 60 and 24 on a card, 100
/// and 32 on the full profile, as Discord's `size-60` and `size-100`.
/// Whether the theme dresses activity cards (its content follows the
/// theme): its accent fills a song's bar and its card shape rounds the art.
fn activity_themed(ctx: crate::ctx::Ctx<'_>) -> bool {
    ctx.themes
        .content(
            &ctx.look.theme.id,
            ctx.layout.theme_content.contains(&ctx.look.theme.id),
        )
        .is_some_and(|c| c.activity == yapfast_core::themes::content::Follow::Theme)
}

fn sizes(big: bool) -> (f32, f32) {
    if big { (100.0, 32.0) } else { (60.0, 24.0) }
}

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

/// An activity's card; its times move as the clock does. `big` is the
/// full profile's, with the larger picture; `bot` leaves the picture out,
/// as Discord does on a bot's card.
pub fn activity_card_view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    activity: &'a ActivityLook,
    inks: &Inks,
    menu: Option<bool>,
    big: bool,
    bot: bool,
) -> Element<'a, Message> {
    card_view(ctx, activity, inks, menu, big, bot)
}

/// The account menu uses the same card as a profile, without its menu:
/// the time runs in green beside the kind's icon there too.
pub fn account_activity_view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    activity: &'a ActivityLook,
    inks: &Inks,
) -> Element<'a, Message> {
    card_view(ctx, activity, inks, None, false, false)
}

fn activity_heading<'a>(
    heading: &'a str,
    i18n: &'a crate::i18n::I18n,
) -> std::borrow::Cow<'a, str> {
    for (prefix, template) in [
        (
            "Listening to ",
            crate::i18n::tr_mark!("Listening to {name}"),
        ),
        ("Watching ", crate::i18n::tr_mark!("Watching {name}")),
        ("Streaming ", crate::i18n::tr_mark!("Streaming {name}")),
        (
            "Competing in ",
            crate::i18n::tr_mark!("Competing in {name}"),
        ),
    ] {
        if let Some(name) = heading.strip_prefix(prefix) {
            return i18n.text(template).replace("{name}", name).into();
        }
    }
    i18n.text(heading).into()
}

fn card_view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    activity: &'a ActivityLook,
    inks: &Inks,
    menu: Option<bool>,
    big: bool,
    bot: bool,
) -> Element<'a, Message> {
    // The heading ends with "…" short of the more button, never under it.
    let mut head = row![
        container(
            elide(activity_heading(&activity.heading, ctx.i18n))
                .size(12)
                .font(MEDIUM)
                .color(inks.ink)
        )
        .width(Length::Fill)
    ]
    .spacing(4)
    .height(16)
    .align_y(Alignment::Center);
    // Discord's more menu, on anyone's card but a bot's; on one's own
    // game it offers its privacy too.
    if let Some(own) = menu {
        let face = ctx.look.glyph(Icon::Ellipsis, 14, inks.ink2);
        head = head.push(ctx.look.face_button(
            face,
            20.0,
            "More",
            Message::Profile(super::Event::ActivityMenu(own && activity.game)),
        ));
    }
    let mut card = column![head].spacing(8);
    let mut body = row![]
        .spacing(if big { 12 } else { 8 })
        .align_y(Alignment::Center);
    if !bot {
        body = body.push(activity_art(
            ctx,
            activity.large.as_deref(),
            activity.small.as_deref(),
            inks,
            big,
        ));
    }
    let mut lines = column![].spacing(4).width(Length::Fill);
    let title: Element<'a, Message> = match &activity.link {
        // A song's title opens its page, as in Discord.
        Some(link) => button(
            elide(activity.name.as_str())
                .size(14)
                .font(SEMIBOLD)
                .color(inks.ink),
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
    for line in &activity.lines {
        lines = lines.push(elide(line.as_str()).size(12).color(inks.ink2));
    }
    // The small lines, each with its icon: the party, the episode, then
    // how long it's run in the online green.
    let green = ctx.look.theme.live;
    let small =
        |icon: Icon, color: Color| ctx.look.icons.coloured(icon, 12, crate::theme::rgb(color));
    let mut chips = row![].spacing(8).align_y(Alignment::Center);
    let mut any = false;
    for (icon, words) in [
        (Icon::Users, &activity.party),
        (Icon::Tv, &activity.episode),
    ] {
        if let Some(words) = words {
            any = true;
            chips = chips.push(
                row![
                    Pic::square(small(icon, inks.ink2), 12.0),
                    elide(words.as_str()).size(12).color(inks.ink2)
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            );
        }
    }
    // Its times move by themselves, each second: no message, no view.
    let second = std::time::Duration::from_secs(1);
    if let Some(since) = activity.since {
        any = true;
        let until = activity.until;
        let icon = Icon::activity(activity.icon).unwrap_or(Icon::Gamepad);
        chips = chips.push(
            row![
                Pic::square(small(icon, green), 12.0),
                crate::widgets::live_text(second, move || {
                    let now = until.map_or(now_seconds(), |until| now_seconds().min(until));
                    timer(now.saturating_sub(since))
                })
                .size(12)
                .color(green),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }
    if any {
        lines = lines.push(chips);
    }
    // A song or show with its start and end shows how far it's got on a
    // bar, with how long it is either side.
    if let Some((start, end)) = activity.bar {
        let length = end - start;
        let played = move || now_seconds().saturating_sub(start).min(length);
        let track = Color {
            a: if inks.themed { 0.2 } else { 0.12 },
            ..inks.ink
        };
        let ink = if activity_themed(ctx) {
            ctx.look.theme.accent
        } else {
            inks.ink
        };
        let filled = crate::widgets::gauge(second, move |renderer, bounds| {
            let share = played() as f32 / length.max(1) as f32;
            progress(renderer, bounds, share, track, ink);
        })
        .height(2);
        lines = lines.push(
            row![
                crate::widgets::live_text(second, move || clock(played()))
                    .size(12)
                    .font(iced::Font::MONOSPACE)
                    .color(inks.ink2),
                filled,
                text(clock(length))
                    .size(12)
                    .font(iced::Font::MONOSPACE)
                    .color(inks.ink2),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    body = body.push(lines);
    card = card.push(body);
    if let Some(url) = &activity.url
        && menu != Some(true)
    {
        card = card.push(
            action(
                ctx.look.theme,
                "Watch",
                Kind::Plain,
                Some(Message::Link(url.clone(), Point::ORIGIN)),
            )
            .small(),
        );
    }
    // Its Rich Presence buttons, each opening its address once asked.
    for (label, press) in &activity.buttons {
        card = card.push(
            action(
                ctx.look.theme,
                label.as_str(),
                Kind::Plain,
                Some(Message::Profile(super::Event::ActivityButton(
                    press.clone(),
                ))),
            )
            .small(),
        );
    }
    card.into()
}

/// The picture, or a plain square when there's none, with the small one
/// on a ring of the card's ground at its lower corner, overhanging it by
/// 4 px as Discord's does.
fn activity_art<'a>(
    ctx: crate::ctx::Ctx<'a>,
    large: Option<&str>,
    small: Option<&str>,
    inks: &Inks,
    big: bool,
) -> Element<'a, Message> {
    let (side, small_side) = sizes(big);
    let ring = small_side + 4.0;
    let ring_at = side + 4.0 - ring;
    // The square under the picture, and all there is without one: the
    // field's shade, which stands apart from the card's tile.
    let under = inks.field;
    let themed = activity_themed(ctx);
    let radius = if themed {
        ctx.look.theme.surface_radius("card", 8.0)
    } else {
        8.0
    };
    let border = if themed {
        ctx.look
            .theme
            .surface_border("card", 8.0, 0.0, Color::TRANSPARENT)
    } else {
        crate::style::round(radius)
    };
    let mut pictures = stack![
        container(Space::new())
            .width(side)
            .height(side)
            .style(move |_| iced::widget::container::Style {
                background: Some(under.into()),
                border,
                ..Default::default()
            })
    ];
    if let Some(picture) =
        large.and_then(|large| ctx.picture(large, side as u32, side as u32, true))
    {
        pictures = pictures.push(Pic::new(picture, Size::new(side, side)).radius(radius));
    }
    let mut outer = side;
    if let Some(small) = small {
        outer = ring_at + ring;
        let ground = inks.ground;
        pictures = pictures.push(crate::controls::placed(
            container(Space::new())
                .width(ring)
                .height(ring)
                .style(move |_| rounded(ground, ring / 2.0)),
            ring_at,
            ring_at,
        ));
        if let Some(picture) = ctx.picture(small, small_side as u32, small_side as u32, true) {
            let inset = ring_at + 2.0;
            pictures = pictures.push(
                Pic::new(picture, Size::new(outer, outer))
                    .at(Rectangle::new(
                        Point::new(inset, inset),
                        Size::new(small_side, small_side),
                    ))
                    .radius(small_side / 2.0),
            );
        }
    }
    container(pictures).width(outer).height(outer).into()
}
