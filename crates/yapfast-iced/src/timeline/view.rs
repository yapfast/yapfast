//! The timeline drawn: its rows over their tints, the dividers, the
//! history's edge and Discord's welcome, the conversation's start, and
//! the bars that jump to the first unread message and to the present.

use crate::widgets::scroll_view;
use std::sync::Arc;

use iced::{
    Alignment, Color, Element, Length, Padding,
    widget::{Space, button, column, container, mouse_area, row, stack, text},
};
use yapfast_core::chat::ChatMessage;

use crate::{
    app::{Message, key},
    controls::bar,
    fonts::{BOLD, MEDIUM, REGULAR, SEMIBOLD},
    pictures::{Face, Icon},
    style::{corners, rounded, scrollbar},
    widgets::{Corner, RightPress, Rows, Settle, Watch},
};

use super::{
    ASIDE, DIVIDER, EDGE, Edge, Event, Intro, KEYED_EDGE, MENTION_EDGE, MENTION_GOLD, MENTIONED,
    MENTIONED_HOVER, PRESENT_BAR, REPLY_EDGE, REPLY_TARGET, REPLY_TARGET_HOVER, RowLook, TAIL,
    TIMELINE, UNREAD_BAR, top_space,
};
use yapfast_core::i18n::{self, tr};

/// What the unread bar says: how many new messages (more when the window
/// is older ones), since when.
pub fn unread_label(unread: &[Arc<ChatMessage>], more: bool) -> String {
    let count = unread.iter().filter(|message| !message.own).count();
    let count = if more {
        format!("{count}+")
    } else {
        count.to_string()
    };
    let new = if count == "1" {
        i18n::fill(tr("{} new message"), &[&count])
    } else {
        i18n::fill(tr("{} new messages"), &[&count])
    };
    let Some(first) = unread.first().filter(|first| first.time.contains(':')) else {
        return new;
    };
    let on = yapfast_core::clock::day(first.timestamp);
    if on.is_empty() || on == yapfast_core::clock::day(yapfast_core::clock::now_ms() / 1000) {
        i18n::fill(tr("{0} since {1}"), &[&new, &first.time])
    } else {
        i18n::fill(tr("{0} since {1} on {2}"), &[&new, &first.time, &on])
    }
}

pub(crate) fn timeline<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    if ctx.lanes.timeline.messages.is_empty() {
        return conversation_start(ctx);
    }
    let (first, last) = ctx.lanes.timeline.in_view(crate::sync::MARGIN);
    // While someone reads earlier ones, the row at the view's top: it
    // and those after it keep their tops as they're measured.
    let pivot = (!ctx.lanes.timeline.at_end).then(|| {
        let top = ctx.lanes.timeline.view_top();
        (first..last)
            .position(|index| ctx.lanes.timeline.tops[index + 1] > top)
            .unwrap_or(last - first)
    });
    let mut rows = Rows::new(ctx.lanes.timeline.total_height()).measured(
        |heights| Message::Timeline(Event::Measured(heights)),
        ctx.lanes.timeline.found.clone(),
        pivot,
    );
    // A short history is held down, its newest message over the composer.
    let lead = (ctx.lanes.timeline.height - ctx.lanes.timeline.total_height()).max(0.0);
    for index in first..last {
        rows.push(
            key(&ctx.lanes.timeline.messages[index].item),
            ctx.lanes.timeline.tops[index],
            ctx.lanes.timeline.heights[index],
            timeline_row(ctx, index),
        );
    }
    let list = scroll_view(column![Space::new().height(lead), rows])
        .id(TIMELINE)
        .anchor_bottom()
        .on_scroll(|viewport| Message::Timeline(Event::Scrolled(viewport)))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(scrollbar(ctx.look.theme));
    // The scrollable reports its viewport as it scrolls, not as the window
    // grows round a list too short to scroll: the room that holds a short
    // history down is worked out from the height the list really has.
    let list = iced::widget::sensor(list)
        .on_show(|size| Message::Timeline(Event::Viewport(size)))
        .on_resize(|size| Message::Timeline(Event::Viewport(size)));
    // Its scroll reports build the view again only when they matter.
    let list = Settle::new(list, |message| match message {
        Message::Timeline(Event::Scrolled(viewport)) => Some(viewport),
        _ => None,
    });
    // A press on no text clears the selection; letting go ends a drag.
    let list = Watch::new(
        list,
        ctx.lanes
            .timeline
            .selection
            .is_some()
            .then_some(Message::SelectClear),
        ctx.lanes.timeline.selecting.then_some(Message::SelectEnd),
    );
    let mut layers = stack![list].width(Length::Fill).height(Length::Fill);
    // Discord's banner runs across the top, 7 px in from the left and 17
    // from the scrollbar's side.
    if let Some(bar) = unread_bar(ctx) {
        layers = layers.push(container(bar).width(Length::Fill).padding(Padding {
            left: 7.0,
            right: 17.0,
            ..Padding::ZERO
        }));
    }
    if ctx.state.detached {
        layers = layers.push(
            container(present_bar(ctx))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(Padding::ZERO.bottom(8))
                .align_x(Alignment::Center)
                .align_y(Alignment::End),
        );
    }
    // A press notes where the list is in the window, for the menus and
    // cards its rows open: what's over the list comes and goes.
    crate::widgets::Spot::new(layers, &ctx.lanes.timeline.place).into()
}

/// A row's tint: the edit's, the jump's light, a mention's, the pointer's,
/// or a theme's stripe.
pub(crate) fn row_ground(ctx: crate::ctx::Ctx<'_>, index: usize, look: &RowLook) -> Option<Color> {
    let theme = ctx.look.theme;
    let message = ctx.row_message(index)?;
    let pointed = ctx.pointed.message.as_deref() == Some(message.item.as_str());
    let jumped = ctx.lanes.timeline.jump_lit.as_deref() == Some(message.item.as_str());
    let edited = ctx.composer.editing.as_deref() == Some(message.item.as_str());
    // As Slint's: in bubbles the jump's light and a mention's tint lie on
    // the bubble (see `message::message_content`), not across the row.
    let bubble = bubbled(ctx, message);
    if edited {
        // Discord's edit tint (#29292d on its dark theme).
        Some(crate::theme::mix(theme.active, theme.chat, 0.84))
    } else if bubble {
        None
    } else if jumped {
        Some(Color {
            a: 0.14,
            ..theme.accent
        })
    } else if index < ASIDE && reply_target(ctx, message) {
        Some(if pointed {
            REPLY_TARGET_HOVER
        } else {
            REPLY_TARGET
        })
    } else if look.highlighted {
        Some(if pointed {
            theme.token_colour("mentioned-hover", MENTIONED_HOVER)
        } else {
            theme.token_colour("mentioned", MENTIONED)
        })
    } else if pointed && !crate::theme_effects::message_row_active(ctx) && !bubbled(ctx, message) {
        // Slint lays no band under a bubble's row; the bubble is the
        // message.
        Some(theme.row_hover)
    } else {
        None
    }
    .or_else(|| {
        theme
            .surface_stripe("row")
            .filter(|stripe| stripe.messages && index % 2 == 1)
            .map(|stripe| stripe.colour)
    })
}

/// Whether a reply being written answers this message.
fn reply_target(ctx: crate::ctx::Ctx<'_>, message: &yapfast_core::chat::ChatMessage) -> bool {
    matches!(
        ctx.state.compose(),
        Some(yapfast_core::application::Compose::Reply { item, .. })
            if item.as_str() == message.item.as_str()
    )
}

/// One row: the history's edge above the first, a divider above a new
/// day or the first unread message, then the message on its tint.
pub fn timeline_row<'a>(ctx: crate::ctx::Ctx<'a>, index: usize) -> Element<'a, Message> {
    let (Some(message), Some(look)) = (ctx.row_message(index), ctx.row_look(index)) else {
        return Space::new().into();
    };
    let theme = ctx.look.theme;
    let spacing = theme.spacing();
    let message_gap = super::rows::row_gap(spacing);
    let aside = index >= ASIDE;
    let edge = if aside {
        index == ASIDE && ctx.lanes.aside.is_some_and(|aside| aside.ready)
    } else {
        index == 0 && ctx.lanes.timeline.edge.ready
    };
    let head = edge || look.divided();
    let mut whole = column![].width(Length::Fill);
    if head {
        let mut above = column![].width(Length::Fill);
        if edge && aside {
            above = above.push(crate::thread::aside_edge(ctx));
        } else if edge {
            above = above.push(history_edge(ctx));
        }
        if look.divided() {
            above = above.push(divider(ctx, look));
        }
        whole = whole.push(
            container(above).padding(
                Padding::ZERO
                    .top(top_space(look, spacing) - if look.divided() { DIVIDER } else { 0.0 }),
            ),
        );
    }
    let pointed = ctx.pointed.message.as_deref() == Some(message.item.as_str());
    // The message a reply being written answers is blurple, not gold.
    let target = !aside && reply_target(ctx, message);
    // A card carries its own tint, mark and edge, leaning with it, as
    // Slint's; the row under it stays bare.
    let carded = spacing.cards_gap.is_some() && message.notice.is_none();
    let ground = if carded {
        None
    } else {
        row_ground(ctx, index, look)
    };
    // The tint starts under the gap that sets a group apart from the one
    // above it, as Discord's does.
    let gap = if !head && look.first {
        top_space(look, spacing)
    } else {
        0.0
    };
    let mut message_content = container(crate::message::message_content(ctx, index, look))
        .width(Length::Fill)
        .padding(Padding {
            top: if head || look.first {
                0.0
            } else {
                top_space(look, spacing)
            },
            bottom: message_gap,
            ..Padding::ZERO
        });
    // A theme's row inset (Slint's `row-inset-x`/`-y`): the band and its
    // edges sit that far in from the row, clear of a pane's rim
    // (Bubblegum's), over the message rather than its gap above.
    let inset = theme.surface_inset("row");
    let band = |child: Element<'a, Message>| -> Element<'a, Message> {
        match inset {
            Some((across, down)) => container(child)
                .padding(Padding {
                    top: if head { 0.0 } else { top_space(look, spacing) } + down,
                    bottom: message_gap + down,
                    left: across,
                    right: across,
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => child,
        }
    };
    let mut ground_layer = None;
    if let Some(ground) = ground {
        let mut border = theme.surface_border("row", 0.0, 0.0, Color::TRANSPARENT);
        border.radius = theme.row_radius(0.0).into();
        let shadow = theme.surface_shadow("row", iced::Shadow::default());
        let style = move |_: &_| container::Style {
            background: Some(ground.into()),
            border,
            shadow,
            ..Default::default()
        };
        if inset.is_some() {
            ground_layer = Some(band(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(style)
                    .into(),
            ));
        } else {
            message_content = message_content.style(style);
        }
    }
    let message_content = container(message_content)
        .width(Length::Fill)
        .padding(Padding::ZERO.top(gap));
    let mut content = stack![message_content];
    if let Some(ground) = ground_layer {
        content = content.push_under(ground);
    }
    // The edges run down the message, from under the gap.
    let edged = |ink: Color, width: f32| -> Element<'a, Message> {
        if inset.is_some() {
            band(bar(ink, width, Length::Fill).into())
        } else {
            container(bar(ink, width, Length::Fill))
                .height(Length::Fill)
                .padding(Padding::ZERO.top(gap))
                .into()
        }
    };
    if !carded && !bubbled(ctx, message) {
        if target {
            content = content.push(edged(REPLY_EDGE, MENTION_EDGE));
        } else if look.highlighted {
            content = content.push(edged(MENTION_GOLD, MENTION_EDGE));
        }
    }
    if ctx.lanes.keyed == Some(message.item.as_str()) && !carded {
        content = content.push(edged(theme.accent, KEYED_EDGE));
    }
    // The hover bar floats over the row's top edge, so it never moves
    // the timeline. It stays while what it opened is open.
    // A bubble carries its own bar beside it (see `message::message_content`).
    if (pointed || crate::message::hover_bar::kept(ctx, &message.item))
        && !bubbled(ctx, message)
        && let Some(actions) = crate::message::hover_bar::hover_bar(ctx, index)
    {
        let body = if head { 0.0 } else { top_space(look, spacing) };
        // As Slint's: in a log at its text column's start, over the line
        // above, clear of the hovered one; on a first row with nothing over
        // it, inside the row.
        let log = crate::message::irc_column_left(ctx, message);
        let top = if index == 0 && !head {
            body + 4.0
        } else if log.is_some() {
            body + 4.0 - crate::message::BAR_HEIGHT
        } else {
            body - 16.0
        };
        // Over a card's right edge, as Slint's, 8 px in from it.
        let corner = Corner::new(actions, if carded { 8.0 } else { 16.0 }, top);
        content = content.push(match log {
            Some(left) => corner.at_most(left),
            None => corner,
        });
    }
    // A right click opens the message's menu where the pointer is: the
    // press is in the list's space, which the lane's corner, as the press
    // itself notes it, makes the window's.
    let item = look.item.clone();
    let content = RightPress::new(content, move |at| {
        let (lane, _) = ctx.lane_place(index);
        let origin = iced::Vector::new(lane.left, lane.top);
        Message::Timeline(Event::RowMenu(item.clone(), at + origin))
    });
    let content = if carded {
        content.into()
    } else {
        // As Slint's: a theme's row highlighter marks a plain row, not a
        // bubble's (Bubblegum's band ran the row's width under its pill).
        crate::theme_effects::message_row(content, ctx, pointed && !bubbled(ctx, message))
    };
    whole = whole.push(
        mouse_area(content)
            .on_enter(Message::Timeline(Event::PointRow(look.item.clone(), true)))
            .on_exit(Message::Timeline(Event::PointRow(look.item.clone(), false))),
    );
    if look.last {
        whole = whole.push(Space::new().height((TAIL - message_gap).max(0.0)));
    }
    // A window of older messages ends with what's after it: newer
    // messages loading, or a way to load them.
    if !aside
        && index + 1 == ctx.lanes.timeline.messages.len()
        && ctx.lanes.timeline.edge.foot() > 0.0
    {
        let edge = &ctx.lanes.timeline.edge;
        whole = whole.push(
            container(edge_button(
                ctx,
                edge.loading_later,
                edge.later_failed,
                true,
                Message::Timeline(Event::Later),
            ))
            .height(edge.foot())
            .padding(Padding {
                top: 4.0,
                bottom: 8.0,
                ..Padding::ZERO
            })
            .center_x(Length::Fill)
            .align_y(Alignment::Center),
        );
    }
    whole.into()
}

/// A new day's date, and above the first unread message Discord's
/// new-messages marker, in the middle of one line: a red line across the row
/// (the date in red when both are here) ending in a red "NEW" tag with a
/// notched left end. Measured: the line is 1 px, from 15 px to 31 px short of
/// the row's right edge, and the tag 13 px tall with 10 px bold capitals.
/// The red is the theme's danger, Discord's own (#d22d39) on Bonfire.
pub(super) fn divider<'a>(ctx: crate::ctx::Ctx<'a>, look: &'a RowLook) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let new = look.unread_start;
    let ink = if new { theme.danger } else { theme.line };
    let line = || {
        container(crate::theme_art::rule(
            ctx,
            yapfast_core::themes::layout::BorderPlace::Divider,
            ink,
            1.0,
        ))
        .width(Length::Fill)
        .center_y(Length::Fill)
    };
    let mut middle = row![line()].align_y(Alignment::Center);
    if let Some(day) = &look.day {
        let day = ctx.themes.label(
            &theme.id,
            yapfast_core::themes::chrome::Label::Day,
            day.as_str(),
            &[("date", day.as_str())],
        );
        middle = middle
            .push(
                container(text(day).size(12).font(SEMIBOLD).color(if new {
                    theme.danger
                } else {
                    theme.muted
                }))
                .padding([0, 10]),
            )
            .push(line());
    }
    if new {
        middle = middle.push(new_tag(ctx));
    }
    container(middle)
        .padding(Padding {
            left: if new { 15.0 } else { 16.0 },
            right: if new { 30.0 } else { 16.0 },
            bottom: 6.0,
            top: 0.0,
        })
        .height(DIVIDER)
        .into()
}

/// The "NEW" tag at the marker's right end: a red pill with a point at its
/// left end that narrows to the line's height.
fn new_tag<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (red, ink) = (theme.danger, theme.on_danger);
    let label = ctx.themes.label(
        &theme.id,
        yapfast_core::themes::chrome::Label::New,
        tr("NEW"),
        &[],
    );
    let pill = container(text(label).size(10).font(BOLD).color(ink))
        .padding([0, 4])
        .height(13)
        .align_y(Alignment::Center)
        .style(move |_| rounded(red, 3));
    stack![
        container(pill).padding(Padding::ZERO.left(6)),
        ctx.look.glyph(Icon::NoticeNotch, 13, red)
    ]
    .into()
}

/// The history's edge: Discord's welcome at a server channel's start, a
/// line at another's, else the Earlier messages button, loading or failed.
pub(super) fn history_edge<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    if ctx.lanes.timeline.edge.welcome() {
        return welcome(ctx);
    }
    edge_line(
        ctx,
        &ctx.lanes.timeline.edge,
        Message::Timeline(Event::Earlier),
    )
}

/// The history's edge of a timeline whose start has no welcome: where
/// it starts, or Earlier messages (`earlier`), loading, or failed.
pub fn edge_line<'a>(
    ctx: crate::ctx::Ctx<'a>,
    edge: &Edge,
    earlier: Message,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let shown: Element<'a, Message> = if edge.start {
        let words = if edge.direct {
            i18n::fill(
                tr("This is the beginning of your messages with {}."),
                &[&edge.name],
            )
        } else if edge.thread {
            i18n::fill(tr("This is the start of the {} thread."), &[&edge.name])
        } else {
            // A theme's own words for a channel's start, as Slint's.
            ctx.themes.label(
                &theme.id,
                yapfast_core::themes::chrome::Label::Start,
                i18n::fill(tr("This is the start of #{}."), &[&edge.name]),
                &[("channel", edge.name.as_str())],
            )
        };
        text(words)
            .size(theme.type_size("body", 14.0))
            .font(theme.font("body", REGULAR))
            .color(theme.muted)
            .into()
    } else {
        edge_button(ctx, edge.loading, edge.failed, false, earlier)
    };
    container(shown)
        .width(Length::Fill)
        .height(EDGE)
        .padding(Padding {
            top: 4.0,
            bottom: 8.0,
            ..Padding::ZERO
        })
        .center_x(Length::Fill)
        .align_y(Alignment::Center)
        .into()
}

/// Earlier messages, or with `newer` Newer messages, as a pill that
/// loads them (`load`): loading, or failed and offering a retry.
pub(super) fn edge_button<'a>(
    ctx: crate::ctx::Ctx<'a>,
    loading: bool,
    failed: bool,
    newer: bool,
    load: Message,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (loading_words, failed_words) = if newer {
        (
            tr("Loading newer messages..."),
            tr("Couldn't load newer messages. Retry"),
        )
    } else {
        (
            tr("Loading earlier messages..."),
            tr("Couldn't load earlier messages. Retry"),
        )
    };
    let (icon, ink, words) = if loading {
        (Icon::History, theme.secondary, loading_words.to_owned())
    } else if failed {
        (Icon::TriangleAlert, theme.danger, failed_words.to_owned())
    } else if newer {
        (
            Icon::History,
            theme.secondary,
            tr("Newer messages").to_owned(),
        )
    } else {
        (
            Icon::History,
            theme.secondary,
            tr("Earlier messages").to_owned(),
        )
    };
    let face = row![
        ctx.look.glyph(icon, 16, ink),
        text(words)
            .size(theme.type_size("body", 14.0))
            .font(theme.font("body", MEDIUM))
            .color(theme.secondary)
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let enabled = !loading && !ctx.state.preview;
    let (rest, lit) = (theme.raised, theme.active);
    button(
        container(face)
            .padding(Padding {
                left: 12.0,
                right: 14.0,
                ..Padding::ZERO
            })
            .height(30)
            .align_y(Alignment::Center),
    )
    .padding(0)
    .on_press_maybe(enabled.then_some(load))
    .style(crate::controls::lit_style(Some(rest), lit, 15.0))
    .into()
}

/// Discord's welcome: a server's in two lines over its line of text, a
/// channel's under a hash in a circle.
pub(super) fn welcome<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let edge = &ctx.lanes.timeline.edge;
    if edge.intro == Intro::Direct {
        return direct_welcome(ctx);
    }
    let theme = ctx.look.theme;
    let title = |words: String, weight| {
        text(theme.heading(&words).into_owned())
            .size(theme.title_size(32.0))
            .font(theme.font("headings", weight))
            .color(theme.text)
            .height(40)
            .wrapping(text::Wrapping::None)
    };
    let mut block = column![]
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .padding(Padding {
            top: 47.0,
            bottom: 12.0,
            ..Padding::ZERO
        });
    let note = match &edge.intro {
        Intro::Server(name) => {
            block = block
                .push(title(tr("Welcome to").to_owned(), SEMIBOLD))
                .push(title(name.clone(), SEMIBOLD));
            text(tr("This is the beginning of this server."))
                .size(theme.type_size("body", 14.0))
                .font(theme.font("body", crate::fonts::REGULAR))
        }
        _ => {
            let field = theme.field;
            block = block
                .push(
                    container(ctx.look.glyph(Icon::Hash, 40, theme.text))
                        .center(68)
                        .style(move |_| rounded(field, 34)),
                )
                .push(Space::new().height(8))
                .push(title(
                    ctx.themes.label(
                        &theme.id,
                        yapfast_core::themes::chrome::Label::Welcome,
                        i18n::fill(tr("Welcome to #{}!"), &[&edge.name]),
                        &[("channel", edge.name.as_str())],
                    ),
                    BOLD,
                ));
            text(ctx.themes.label(
                &theme.id,
                yapfast_core::themes::chrome::Label::Start,
                i18n::fill(tr("This is the start of the #{} channel."), &[&edge.name]),
                &[("channel", edge.name.as_str())],
            ))
            .size(theme.type_size("body", 16.0))
            .font(theme.font("body", crate::fonts::REGULAR))
        }
    };
    block
        .push(Space::new().height(8))
        .push(note.color(theme.secondary).height(20))
        .height(ctx.lanes.timeline.edge.height())
        .into()
}

/// A one-to-one DM's opening, as Discord's: the other person's avatar,
/// name and handle, what this is, and what they share with this account,
/// with what to do about them as Discord's buttons there: Add Friend,
/// Accept or Ignore their request, Remove Friend, Block, or Unblock.
fn direct_welcome<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let edge = &ctx.lanes.timeline.edge;
    let theme = ctx.look.theme;
    let bright = if theme.bonfire {
        crate::style::hex(0xfbfbfb)
    } else {
        theme.text
    };
    // The profile beside the DM, once it's for this person, has the handle
    // and what is shared.
    let person = ctx
        .state
        .selected_room()
        .and_then(|room| room.person.as_deref());
    let look = ctx
        .profiles
        .panel
        .as_ref()
        .map(|shown| &shown.look)
        .filter(|look| Some(look.profile().id.as_str()) == person);
    let mut block = column![
        ctx.face(Face::new(ctx.header.avatar.as_deref(), &edge.name, 80.0)),
        Space::new().height(8),
        text(edge.name.as_str())
            .size(32)
            .font(BOLD)
            .color(bright)
            .height(40)
            .wrapping(text::Wrapping::None),
    ]
    .width(Length::Fill)
    .padding(Padding {
        top: 16.0,
        right: 16.0,
        bottom: 12.0,
        left: 16.0,
    });
    if let Some(look) = look {
        block = block.push(Space::new().height(8)).push(
            text(look.profile().handle.as_str())
                .size(24)
                .font(MEDIUM)
                .color(bright)
                .height(32)
                .wrapping(text::Wrapping::None),
        );
    }
    // One sentence, which wraps as one where the timeline is narrow.
    let spans: Vec<iced::advanced::text::Span<'a, (), iced::Font>> = vec![
        iced::widget::span("This is the beginning of your direct message history with ")
            .color(theme.text),
        iced::widget::span(edge.name.as_str())
            .font(BOLD)
            .color(bright),
        iced::widget::span(".").color(theme.text),
    ];
    block = block
        .push(Space::new().height(18))
        .push(iced::widget::rich_text(spans).size(16).line_height(1.5));
    let mut line = row![].spacing(12).height(32).align_y(Alignment::Center);
    let dot = if theme.bonfire {
        crate::style::hex(0x47474d)
    } else {
        theme.faint
    };
    if let Some(look) = look
        && let Some(server) = look.profile().servers.first()
    {
        line = line
            .push(
                row![
                    ctx.avatar(server.icon.as_deref(), &server.name, 24.0, 6.0, 1.0),
                    text(look.mutual_servers.as_str())
                        .size(14)
                        .color(theme.secondary),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .push(
                container(Space::new())
                    .width(4)
                    .height(4)
                    .style(move |_| rounded(dot, 2)),
            );
    }
    let button_face =
        |words: &'static str, rest: Color, lit: Color, ink: Color, press: Option<Message>| {
            button(
                container(text(words).size(14).font(SEMIBOLD).color(ink))
                    .padding([0, 16])
                    .height(32)
                    .align_y(Alignment::Center),
            )
            .padding(0)
            .on_press_maybe(press)
            .style(crate::controls::lit_style(Some(rest), lit, 8.0))
        };
    let rest = if theme.bonfire {
        crate::style::hex(0x28282d)
    } else {
        theme.raised
    };
    let mut actions = row![].spacing(8);
    if let Some(person) = person {
        use crate::profile::{Event as Profile, Relate};
        use yapfast_core::application::Relation;
        let relate = |relate| Some(Message::Profile(Profile::Relate(person.to_owned(), relate)));
        let primary = |words, press| {
            button_face(
                words,
                theme.accent,
                theme.accent_hover,
                theme.on_accent,
                press,
            )
        };
        let secondary = |words, press| button_face(words, rest, theme.active, theme.text, press);
        // Discord's buttons by how this account stands with them: a
        // blocked person only unblocks; a bot can't be befriended.
        if ctx.state.is_blocked(person) {
            actions = actions.push(secondary("Unblock", relate(Relate::Unblock)));
        } else {
            match ctx.state.relation(person) {
                Relation::Stranger => {
                    actions = actions.push(primary("Add Friend", relate(Relate::Befriend)));
                }
                Relation::Incoming => {
                    actions = actions
                        .push(primary("Accept Request", relate(Relate::Befriend)))
                        .push(secondary("Ignore Request", relate(Relate::Ignore)));
                }
                Relation::Outgoing => {
                    actions = actions.push(primary("Friend Request Sent", None));
                }
                Relation::Friend => {
                    actions = actions.push(secondary("Remove Friend", relate(Relate::Unfriend)));
                }
                Relation::Bot | Relation::Me => {}
            }
            actions = actions.push(secondary("Block", relate(Relate::Block)));
        }
    }
    // Its own height, taller where the sentence wraps: the row's measure
    // takes it up in place of the estimate.
    block
        .push(Space::new().height(6))
        .push(line.push(actions))
        .height(Length::Shrink)
        .into()
}

/// Messages on their way: rows the shape of messages, held to the bottom
/// as the messages will be, each a round face, a name and a line or more.
pub fn messages_skeleton<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    const ROWS: [(f32, &[f32]); 5] = [
        (92.0, &[380.0, 220.0]),
        (120.0, &[300.0]),
        (76.0, &[420.0, 340.0, 160.0]),
        (104.0, &[260.0]),
        (88.0, &[360.0, 200.0]),
    ];
    let ground = ctx.look.theme.raised;
    let bone = move |width: f32, height: f32, radius: f32| {
        container(Space::new())
            .width(width)
            .height(height)
            .style(move |_| rounded(ground, radius))
    };
    let mut rows = column![].spacing(22).padding(16).width(Length::Fill);
    for (name, lines) in ROWS {
        let mut words = column![bone(name, 12.0, 6.0)]
            .spacing(8)
            .padding(Padding::ZERO.top(4));
        for &line in lines {
            words = words.push(bone(line, 12.0, 6.0));
        }
        rows = rows.push(row![bone(40.0, 40.0, 20.0), words].spacing(16));
    }
    container(rows)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::End)
        .clip(true)
        .into()
}

/// An empty conversation's opening; while it loads, a skeleton of rows.
pub(super) fn conversation_start<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let edge = &ctx.lanes.timeline.edge;
    if ctx.state.selected.is_some() && !edge.ready {
        return messages_skeleton(ctx);
    }
    let theme = ctx.look.theme;
    let mut start = column![].spacing(8).padding(24).width(Length::Fill);
    if ctx.state.selected.is_some() && edge.ready {
        if edge.direct {
            start = start.push(ctx.face(Face::new(
                ctx.header.avatar.as_deref(),
                &ctx.header.name,
                72.0,
            )));
        } else {
            let raised = theme.raised;
            let icon = ctx.header.icon.unwrap_or(Icon::Hash);
            start = start.push(
                container(ctx.look.glyph(icon, 36, theme.text))
                    .center(68)
                    .style(move |_| rounded(raised, 34)),
            );
        }
    }
    let (title, size) = if !edge.ready {
        (tr("Loading conversation...").to_owned(), 18)
    } else if edge.direct || edge.thread {
        (edge.name.clone(), 28)
    } else {
        (i18n::fill(tr("Welcome to #{}"), &[&edge.name]), 28)
    };
    start = start.push(
        text(theme.heading(&title).into_owned())
            .size(theme.title_size(size as f32))
            .font(theme.font("headings", BOLD))
            .color(theme.text),
    );
    if edge.ready {
        let words = if edge.direct {
            i18n::fill(
                tr("This is the beginning of your messages with {}."),
                &[&edge.name],
            )
        } else if edge.thread {
            i18n::fill(
                tr("This is the start of the {} thread. No messages here yet."),
                &[&edge.name],
            )
        } else {
            i18n::fill(
                tr("This is the start of #{}. No messages here yet."),
                &[&edge.name],
            )
        };
        start = start.push(
            text(words)
                .size(theme.type_size("body", 16.0))
                .font(theme.font("body", crate::fonts::REGULAR))
                .color(theme.secondary),
        );
    }
    container(start)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::End)
        .into()
}

/// Whether the first unread message is older than every loaded one, by
/// Discord's snowflakes.
fn unread_earlier(
    state: &yapfast_core::application::AppState,
    messages: &[Arc<ChatMessage>],
) -> bool {
    let order = yapfast_core::discord::item_order;
    state
        .first_unread
        .as_deref()
        .and_then(order)
        .zip(messages.first().and_then(|first| order(&first.item)))
        .is_some_and(|(unread, first)| unread < first)
}

/// Until the newest message is read, Discord's banner across the top: how
/// much is new and since when, which jumps to the first on a click, and Mark
/// as read at its right end. Measured: 34 px tall, square at the top and
/// rounded 8 px at the bottom, 14 px semibold text 21 px in from the left.
pub(super) fn unread_bar<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let state = ctx.state;
    if state.caught_up {
        return None;
    }
    let messages = &ctx.lanes.timeline.messages;
    let label = match ctx.lanes.timeline.unread_index(ctx.state) {
        Some(index) => unread_label(&messages[index..], state.detached),
        // Sending from a window of older messages shows the newest, which
        // can leave the divider above what's loaded: the banner back to it
        // stays until the conversation is left, as the divider would.
        None if unread_earlier(state, messages) => unread_label(messages, true),
        None => return None,
    };
    let theme = ctx.look.theme;
    let ink = theme.on_accent;
    let jump = button(
        container(
            text(label)
                .size(14)
                .font(SEMIBOLD)
                .color(ink)
                .wrapping(text::Wrapping::None),
        )
        .padding(Padding::ZERO.left(21))
        .height(UNREAD_BAR)
        .width(Length::Fill)
        .align_y(Alignment::Center),
    )
    .padding(0)
    .width(Length::Fill)
    .on_press(Message::Timeline(Event::JumpUnread))
    .style(|_, _| button::Style::default());
    let mark = button(
        row![
            text("Mark as read")
                .size(14)
                .font(SEMIBOLD)
                .color(ink)
                .wrapping(text::Wrapping::None),
            ctx.look.glyph(Icon::MarkRead, 19, ink),
        ]
        .spacing(7)
        .align_y(Alignment::Center),
    )
    .padding(Padding::ZERO.right(23).left(12))
    .height(UNREAD_BAR)
    .on_press(Message::Timeline(Event::MarkRead))
    .style(|_, _| button::Style::default());
    let ground = theme.accent;
    Some(
        container(row![jump, mark].align_y(Alignment::Center))
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border: crate::style::round(corners([0.0, 0.0, 8.0, 8.0])),
                ..container::Style::default()
            })
            .into(),
    )
}

/// A window of older messages says so over its end, with the way back:
/// Discord's floating pill over the composer. Measured: 50 px tall in a 1 px
/// border with a 16 px radius, its button 30 px tall with an 8 px radius;
/// Bonfire draws it in Discord's inks, other themes in their raised ground.
pub(super) fn present_bar<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (ground, edge, words) = if theme.bonfire {
        (
            crate::style::hex(0x2c2d32),
            crate::style::hex(0x414248),
            crate::style::hex(0xb3b4b6),
        )
    } else {
        (theme.raised, theme.edge, theme.secondary)
    };
    let (rest, lit) = (theme.accent, theme.accent_hover);
    let on_accent = theme.on_accent;
    let go = button(
        text(tr("Jump to present"))
            .size(14)
            .font(SEMIBOLD)
            .color(on_accent)
            .wrapping(text::Wrapping::None),
    )
    .padding([0, 11])
    .height(30)
    .on_press(Message::Timeline(Event::JumpPresent))
    .style(crate::controls::lit_style(Some(rest), lit, 8.0));
    let face = row![
        text(tr("You're viewing older messages"))
            .size(theme.type_size("body", 14.0))
            .font(theme.font("body", MEDIUM))
            .color(words)
            .wrapping(text::Wrapping::None),
        go,
    ]
    .spacing(15)
    .align_y(Alignment::Center);
    let border = theme.surface_border("toast", 16.0, 1.0, edge);
    let shadow = theme.surface_shadow("toast", crate::style::pill_shadow());
    container(face)
        .padding(Padding {
            left: 17.0,
            right: 13.0,
            top: 9.0,
            bottom: 9.0,
        })
        .height(PRESENT_BAR)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(ground.into()),
            border,
            shadow,
            ..container::Style::default()
        })
        .into()
}

/// Whether the message is drawn as a theme's bubble, which takes no row
/// band under the pointer, as in Slint.
fn bubbled(ctx: crate::ctx::Ctx<'_>, message: &yapfast_core::chat::ChatMessage) -> bool {
    message.notice.is_none()
        && ctx
            .themes
            .package_layout(&ctx.look.theme.id)
            .and_then(|layout| layout.messages)
            == Some(yapfast_core::themes::layout::MessageStyle::Bubbles)
}
