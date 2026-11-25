//! One message's content in the timeline: the gutter (the author's avatar,
//! a notice's icon, the time beside a grouped message under the pointer, a
//! reply's spine), the line above (the reply it quotes, the command it
//! answers), the author line, the text in its blocks, `(edited)`, picture
//! previews (GIFs playing), reaction chips with who reacted in their
//! tooltips, and the card of the thread a message started.

use iced::{
    Alignment, Element, Length, Padding, Point,
    widget::{Space, button, column, container, mouse_area, row, span, stack, text},
};
use std::sync::Arc;

use yapfast_core::chat::{self, ChatMessage, NoticeKind, Placement};
use yapfast_core::themes::layout::MessageStyle;

use crate::{
    app::{App, Message},
    controls::{self, placed},
    fonts::{ITALIC, MEDIUM, REGULAR, SEMIBOLD, shown_name, style_font},
    looks::{self, Chip},
    markdown::{FIRST_PART, Look},
    media::Gallery,
    pictures::{Face, Icon},
    piece::Piece,
    style::{CHIP, hex},
    timeline::{AUTHOR_GAP, AUTHOR_LINE, CONTENT_PAD, END_PAD, LINE_HEIGHT, RowLook},
    widgets::{Fold, RightPress},
};

pub mod attachments;
mod beside;
mod embed;
pub mod hover_bar;
mod reaction_chips;

pub use attachments::files;
#[cfg(test)]
pub use reaction_chips::CHIP_HEIGHT;
pub use reaction_chips::{emoji_shortcode, reacts};

/// The line above a reply or a command's answer, and its avatar.
const HEADER_LINE: f32 = 22.0;
const HEADER_AVATAR: f32 = 16.0;
/// A custom emoji's picture on a reply's quote line.
const REPLY_EMOJI: u32 = 16;
/// An embed at its widest, its bar, its thumbnail at its widest, and the
/// pieces its text is in the selection (left out of copies).
const EMBED_WIDTH: f32 = 432.0;
const EMBED_BAR: f32 = 4.0;
const THUMBNAIL: f32 = 80.0;
/// Past any message's body's parts (Discord's 4000 characters make far
/// fewer), each embed 16 of them, before a bot's cards' (`components.rs`).
pub const EMBED_PARTS: crate::piece::PartNo = 16384;
/// A thread's card under its message at its widest.
const THREAD_WIDTH: f32 = 480.0;
/// A file's card at its widest, and its icon.
const FILE_CARD_WIDTH: f32 = 432.0;
const FILE_ICON: u32 = 30;
/// An embed's padding across, around its text and pictures.
const EMBED_PADDING: f32 = 4.0 + 12.0 + 16.0;
/// A custom emoji's picture on a reaction's chip.
const REACTION_EMOJI: u32 = 18;
/// A picture's corners in a message.
pub const PICTURE_RADIUS: f32 = 8.0;
/// A message's sticker, as Discord draws it whatever size its picture is.
pub const STICKER: f32 = 160.0;
/// The play mark over a video's poster.
const PLAY_BADGE: f32 = 48.0;
/// The reply spine's square.
const SPINE: u32 = 33;
/// The hover bar's buttons.
const BAR_BUTTON: f32 = 32.0;
/// A message's hover bar's height: its buttons and its 3 px padding.
pub const BAR_HEIGHT: f32 = BAR_BUTTON + 6.0;
/// The room an author line keeps for its time and badges after the name.
const NAME_TIME_ROOM: f32 = 64.0;
/// Discord's flag on an answer only this account sees.
pub const EPHEMERAL: u64 = 1 << 6;
/// Discord's flag on an app's answer still to come.
pub const LOADING: u64 = 1 << 7;
/// How long Discord's client waits for an answer still to come before it
/// says the app didn't respond: 15 minutes from the message's time.
const THINKING_SECS: u64 = 15 * 60;

/// A message's avatar or author was clicked: the card opens beside the
/// avatar.
pub fn open_author(app: &mut App, index: usize) -> iced::Task<Message> {
    let Some(message) = crate::timeline::row_message(app, index) else {
        return iced::Task::none();
    };
    let (lane, _) = app.ctx().lane_place(index);
    // The avatar's top: moved down under a reply's line.
    let header = message.reply.is_some() || message.used.is_some();
    let avatar = crate::timeline::avatar(&app.theme);
    let gutter = crate::timeline::gutter(&app.theme);
    let y = app.ctx().row_body_top(index) + CONTENT_PAD + if header { 24.0 } else { 2.0 };
    let x = lane.left + (gutter - avatar) / 2.0;
    let width = crate::profile::WIDTH;
    let beside = if x + avatar + 8.0 + width <= app.window.width - 8.0 {
        x + avatar + 8.0
    } else {
        (x - width - 8.0).max(8.0)
    };
    // A Discord author's sender is `discord:` and their id; the
    // profile is asked for by the id, as the member list's is.
    let sender = &message.sender;
    let user = yapfast_core::discord::sender_user(sender).unwrap_or(sender);
    app.open_card(user.to_owned(), Point::new(beside, y - 8.0))
}

/// What `revealed` holds for a blocked person's message shown.
pub fn blocked_key(item: &str) -> String {
    format!("{item}/blocked")
}

/// What `revealed` holds for a long message shown whole.
pub fn fold_key(item: &str) -> String {
    format!("{item}/more")
}

/// What `revealed` holds for a message's attachment shown past its
/// spoiler's cover.
pub fn reveal_key(item: &str, index: usize) -> String {
    format!("{item}/a{index}")
}

/// Row `index`'s message, beside its gutter.
pub fn message_content<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    look: &'a RowLook,
) -> Element<'a, Message> {
    let Some(message) = ctx.row_message(index) else {
        return Space::new().into();
    };
    if (message.blocked || message.ignored)
        && !ctx
            .lanes
            .timeline
            .revealed
            .contains(&blocked_key(&message.item))
    {
        return hidden_row(ctx, look, message.blocked);
    }
    let theme = ctx.look.theme;
    let package_layout = ctx.themes.package_layout(&theme.id);
    let message_style = package_layout
        .and_then(|layout| layout.messages)
        .unwrap_or(MessageStyle::Flat);
    let header = message.reply.is_some() || message.used.is_some();
    // A bubble hugs its words, as the Slint build's bubbles do, up to the
    // theme's share of the lane; anything that fills (a code block, an
    // embed) still takes that share.
    let hugs = message_style == MessageStyle::Bubbles
        && message.notice.is_none()
        && !editing_row(ctx, message);
    let mut content = column![].width(if hugs { Length::Shrink } else { Length::Fill });
    if let Some(reply) = &message.reply {
        content = content.push(reply_line(ctx, look, reply));
    } else if let Some(used) = &message.used {
        content = content.push(used_line(ctx, index, used));
    }
    if look.first
        && message.notice.is_none()
        && message_style != MessageStyle::Irc
        && !(message_style == MessageStyle::Bubbles && message.own)
    {
        content = content.push(
            container(timed_out_mark(
                ctx,
                message,
                author_line(ctx, index, message),
            ))
            .padding(Padding::ZERO.bottom(AUTHOR_GAP)),
        );
    }
    if message.forwarded {
        content = content.push(
            row![
                ctx.look.glyph(Icon::Forward, 16, theme.muted),
                text("Forwarded")
                    .size(theme.type_size("body", 14.0))
                    .font(theme.font("body", ITALIC))
                    .color(theme.muted)
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }
    let ink = if message.deleted || message.delivery != chat::Delivery::default() {
        theme.muted
    } else if message.notice.is_some() {
        theme.secondary
    } else if message_style == MessageStyle::Bubbles && message.own {
        package_layout
            .and_then(|layout| layout.bubbles.own_text)
            .map(crate::themes::argb)
            .unwrap_or(theme.on_accent)
    } else {
        theme.text
    };
    let editing = ctx.composer.editing.as_deref() == Some(message.item.as_str());
    if editing {
        content = content.push(crate::edit::inline_edit(ctx));
    } else if !look.body.parts.is_empty() && !look.body.plain().is_empty() {
        let body = ctx.body_view(
            &look.body,
            index,
            FIRST_PART,
            Look {
                size: text_size(ctx),
                line_height: LINE_HEIGHT,
                ink,
            },
        );
        content = content.push(if ctx.layout.collapse_long {
            folding(ctx, index, body)
        } else {
            body
        });
    }
    if message.flags & LOADING != 0 {
        content = content.push(thinking(ctx, message));
    }
    if message.edited && message.notice.is_none() && !editing {
        content = content.push(
            text("(edited)")
                .size(theme.type_size("small", 10.0))
                .font(theme.font("small", REGULAR))
                .color(theme.muted),
        );
    }
    let gallery = Gallery::of(
        &message.attachments,
        Placement::Message,
        content_width_of(ctx, index),
    );
    if !gallery.is_empty() {
        content = content.push(
            container(crate::message::attachments::gallery(
                ctx, index, message, &gallery,
            ))
            // Discord leaves 3px under a picture, measured against its next
            // message.
            .padding(Padding::ZERO.top(4).bottom(3)),
        );
    }
    if let Some(stickers) = crate::message::attachments::stickers(ctx, message) {
        content = content.push(container(stickers).padding(Padding::ZERO.top(4)));
    }
    for (at, (index, attachment)) in files(message, Placement::Message).enumerate() {
        // A voice message's recording is its first attachment.
        if at == 0
            && let Some(voice) = &message.voice
        {
            content = content.push(crate::playback::voice_card(ctx, message, voice));
            continue;
        }
        content = content.push(
            container(crate::message::attachments::file_card(
                ctx,
                &message.item,
                index,
                attachment,
            ))
            .padding(Padding::ZERO.top(4)),
        );
    }
    if let Some(poll) = &message.poll {
        content = content.push(crate::polls::view(ctx, message, poll));
    }
    if ctx.layout.prefs.link_previews.unwrap_or(true) {
        for (place, embed) in message.embeds.iter().enumerate() {
            if let Some(laid) = look.body.embeds.get(place) {
                content = content.push(crate::message::embed::embed(
                    ctx, index, message, place, embed, laid,
                ));
            }
        }
    } else if let Some(hidden) = embed::hidden_card(message) {
        // A bot's card can be the whole message: say it's there.
        content = content.push(
            text(hidden)
                .size(text_size(ctx))
                .color(ctx.look.theme.muted),
        );
    }
    for (place, card) in message.cards.iter().enumerate() {
        if let Some(bodies) = look.body.cards.get(place) {
            content = content.push(crate::components::app_card(
                ctx, index, message, card, bodies,
            ));
        }
    }
    if let Some(failed) = crate::components::bot_failed(ctx, message) {
        content = content.push(failed);
    }
    // In bubbles the reactions sit under the bubble, on its side, as
    // Slint's; elsewhere they end the message.
    let bubbled = message_style == MessageStyle::Bubbles && message.notice.is_none();
    let mut under = None;
    if !message.reactions.is_empty() {
        let chips = container(crate::message::reaction_chips::reactions(
            ctx, index, message,
        ))
        .padding(Padding::ZERO.top(4));
        if bubbled {
            under = Some(chips);
        } else {
            content = content.push(chips);
        }
    }
    if let Some(delivery) = delivery_line(ctx, message) {
        content = content.push(delivery);
    }
    if message.flags & EPHEMERAL != 0 {
        content = content.push(ephemeral_line(ctx, message));
    }
    // Last, under the reactions, as in Discord's client.
    if let Some(thread) = &message.thread {
        content = content.push(thread_card(
            ctx,
            ctx.row_item(index),
            thread,
            message.notice.is_some(),
        ));
    }
    let avatars = package_layout.and_then(|layout| layout.chat_avatars) != Some(false);
    let show_gutter = message.notice.is_some()
        || (avatars
            && message_style != MessageStyle::Irc
            && !(message_style == MessageStyle::Bubbles && message.own));
    let gutter = show_gutter.then(|| gutter(ctx, index, message, look, header));
    let avatar = crate::timeline::avatar(theme);
    let gutter_width = if show_gutter {
        crate::timeline::gutter(theme)
    } else {
        0.0
    };
    let lane: Element<'a, Message> = match message_style {
        MessageStyle::Irc if message.notice.is_none() => {
            let time_width = package_layout
                .and_then(|layout| layout.irc.time_width)
                .unwrap_or(64.0);
            let nick_width = package_layout
                .and_then(|layout| layout.irc.nick_width)
                .unwrap_or(112.0);
            let separator = package_layout
                .and_then(|layout| layout.irc.separator.clone())
                .unwrap_or_default();
            let rule = package_layout.and_then(|layout| layout.irc.rule) == Some(true);
            let mut line = row![].spacing(6).align_y(Alignment::Start);
            if time_width > 0.0 {
                line = line.push(
                    container(
                        text(ctx.themes.marked_time(&theme.id, &look.time))
                            .size(theme.type_size("time", 12.0))
                            .font(theme.font("time", MEDIUM))
                            .color(theme.muted),
                    )
                    .width(time_width)
                    .align_x(Alignment::End),
                );
            }
            // As Slint's irc log: the nick on a group's first line only,
            // its column blank under it.
            let nick: Element<'a, Message> = if look.first {
                // The person's own name style, as the other layouts keep it
                // unless the theme may restyle names.
                let style = looks::name_look(
                    ctx.state.cosmetics.style(message.style.as_deref()),
                    &message.author,
                    ctx.in_server,
                    theme.dark,
                );
                ctx.person_name(
                    ctx.themes.marked(
                        &theme.id,
                        yapfast_core::themes::chrome::Place::Author,
                        message.author.as_str(),
                    ),
                    crate::people::NameInk::new(
                        14.0,
                        500,
                        message.color.map_or(theme.text, crate::style::hex),
                    )
                    .style(style.as_ref())
                    .key(message.sender.as_str()),
                )
            } else {
                Space::new().into()
            };
            line = line.push(
                container(nick)
                    .width(nick_width)
                    .clip(true)
                    .align_x(Alignment::End),
            );
            if rule {
                let separator = package_layout
                    .and_then(|layout| layout.irc.separator.clone())
                    .unwrap_or_else(|| "│".into());
                if !separator.is_empty() {
                    line = line.push(crate::theme_art::irc_rule(
                        separator,
                        text_size(ctx),
                        LINE_HEIGHT,
                        theme.font("body", REGULAR),
                        theme.faint,
                        content.into(),
                    ));
                } else {
                    line = line.push(content);
                }
            } else {
                if !separator.is_empty() {
                    line = line.push(text(separator).size(text_size(ctx)).color(theme.faint));
                }
                line = line.push(content);
            }
            // Slint's log starts the chat's padding in, not a gutter's.
            let lead = theme.spacing().chat_padding.unwrap_or(END_PAD);
            container(line)
                .width(Length::Fill)
                .padding(Padding {
                    top: CONTENT_PAD,
                    bottom: CONTENT_PAD,
                    left: lead,
                    right: if editing { 21.0 } else { END_PAD },
                })
                .into()
        }
        MessageStyle::Bubbles if message.notice.is_none() => {
            let stock = yapfast_core::themes::layout::Bubbles::default();
            let bubbles = package_layout.map_or(&stock, |layout| &layout.bubbles);
            let (across, down) = bubbles
                .padding
                .unwrap_or(yapfast_core::themes::layout::BUBBLE_PADDING);
            let background = if message.own {
                bubbles
                    .own
                    .as_ref()
                    .map(crate::themes::paint)
                    .unwrap_or_else(|| theme.accent.into())
            } else {
                bubbles
                    .other
                    .as_ref()
                    .map(crate::themes::paint)
                    .unwrap_or_else(|| theme.raised.into())
            };
            let radius = bubbles
                .radius
                .unwrap_or_else(|| theme.surface_radius("card", 12.0));
            let gel = if message.own {
                &bubbles.own_gel
            } else {
                &bubbles.other_gel
            };
            let padded = container(content).padding([down, across]);
            // A mention's tint lies on the bubble under its words, as
            // Slint's `mark`, and the jump's light: never a band across the
            // row.
            let pointed_mark = ctx.pointed.message.as_deref() == Some(message.item.as_str());
            let jumped = ctx.lanes.timeline.jump_lit.as_deref() == Some(message.item.as_str());
            let mark = if look.highlighted {
                Some(if pointed_mark {
                    theme.token_colour("mentioned-hover", crate::timeline::MENTIONED_HOVER)
                } else {
                    theme.token_colour("mentioned", crate::timeline::MENTIONED)
                })
            } else if jumped {
                Some(iced::Color {
                    a: 0.14,
                    ..theme.accent
                })
            } else {
                None
            };
            let padded: Element<'a, Message> = match mark {
                Some(mark) => iced::widget::Stack::with_children([padded.into()])
                    .push_under(
                        container(Space::new())
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .style(move |_| container::Style {
                                background: Some(mark.into()),
                                border: iced::Border {
                                    radius: radius.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }),
                    )
                    .into(),
                None => padded.into(),
            };
            let mut bubble: Element<'a, Message> = if gel.is_empty() {
                let ground = move |_: &_| container::Style {
                    background: Some(background),
                    border: iced::Border {
                        radius: radius.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                };
                // The theme's pictures on the bubble lie over its ground,
                // under its words; the words keep the bubble's size.
                match crate::theme_art::bubble_layers(ctx, message.own, radius) {
                    Some(art) => iced::widget::Stack::with_children([padded])
                        .push_under(art)
                        .push_under(
                            container(Space::new())
                                .width(Length::Fill)
                                .height(Length::Fill)
                                .style(ground),
                        )
                        .into(),
                    None => container(padded).style(ground).into(),
                }
            } else {
                crate::theme_art::gel_bubble(padded, background, radius, gel.clone())
            };
            let pointed_here = ctx.pointed.message.as_deref() == Some(message.item.as_str());
            // Under the pointer the bubble takes Slint's tint: the
            // overlay colour at 8% over it.
            if pointed_here {
                let tint = theme.wash(0.08);
                bubble = iced::widget::Stack::with_children([bubble])
                    .push(
                        container(Space::new())
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .style(move |_| container::Style {
                                background: Some(tint.into()),
                                border: iced::Border {
                                    radius: radius.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }),
                    )
                    .into();
            }
            if let Some(share) = bubbles.max_width {
                // A share of the lane the bubble is in: the theme's chat
                // width, or the list's own while the window is narrower.
                let lane = package_layout
                    .and_then(|layout| layout.chat_max_width)
                    .unwrap_or(960.0)
                    .min(ctx.lane_place(index).0.width.max(1.0));
                bubble = container(bubble)
                    .max_width((lane * share / 100.0).max(120.0))
                    .into();
            }
            let bubble = crate::theme_art::outlined(
                bubble,
                ctx,
                yapfast_core::themes::layout::BorderPlace::Message,
                "card",
                radius,
                0.0,
                theme.line,
            );
            // As Slint: the hover bar beside the bubble, not at the row's
            // end (the timeline leaves it out for bubbles).
            let pointed = ctx.pointed.message.as_deref() == Some(message.item.as_str());
            let bubble = match (pointed || hover_bar::kept(ctx, &message.item))
                .then(|| hover_bar::hover_bar(ctx, index))
                .flatten()
            {
                Some(bar) => beside::Beside::new(bubble, bar, message.own).into(),
                None => bubble,
            };
            let bubble: Element<'a, Message> = match under {
                Some(chips) => column![bubble, chips]
                    .align_x(if message.own {
                        Alignment::End
                    } else {
                        Alignment::Start
                    })
                    .into(),
                None => bubble,
            };
            let aligned = if message.own {
                row![Space::new().width(Length::Fill), bubble]
            } else if let Some(gutter) = gutter {
                row![gutter, bubble].spacing(16)
            } else {
                row![bubble, Space::new().width(Length::Fill)]
            };
            container(aligned)
                .width(Length::Fill)
                .padding(Padding {
                    top: CONTENT_PAD,
                    bottom: CONTENT_PAD,
                    left: (gutter_width - avatar).max(0.0) / 2.0,
                    right: if editing { 21.0 } else { END_PAD },
                })
                .into()
        }
        MessageStyle::Cards if message.notice.is_none() => {
            let stock = yapfast_core::themes::layout::Cards::default();
            let cards = package_layout.map_or(&stock, |layout| &layout.cards);
            let ground = cards
                .ground
                .map(crate::themes::argb)
                .unwrap_or(theme.raised);
            let padding = cards.padding.unwrap_or(12.0);
            // As in Slint, one card holds a whole group: its first row
            // has the card's top corners and the room above it, its last
            // the bottom corners and the room below, the rows between run
            // on without a seam.
            let first = look.first;
            let end = look.group_end;
            // A card takes the `message` shape, as in Slint: its corners
            // and its border (the theme's `card` shape dresses profile
            // cards, not these).
            let corner = theme.surface_radius("message", 4.0);
            let mut border = theme.surface_border("message", 0.0, 0.0, iced::Color::TRANSPARENT);
            border.radius = iced::border::Radius {
                top_left: if first { corner } else { 0.0 },
                top_right: if first { corner } else { 0.0 },
                bottom_right: if end { corner } else { 0.0 },
                bottom_left: if end { corner } else { 0.0 },
            };
            let outline_fallback = border;
            // A dashed or dotted card border draws instead of the solid one.
            if package_layout.is_some_and(|layout| {
                layout
                    .borders
                    .contains_key(&yapfast_core::themes::layout::BorderPlace::Card)
            }) {
                border.width = 0.0;
            }
            let shadow = cards.shadow.as_ref().map_or_else(
                || theme.surface_shadow("card", iced::Shadow::default()),
                |shadow| iced::Shadow {
                    color: crate::themes::argb(shadow.colour),
                    offset: iced::Vector::new(shadow.x, shadow.y),
                    blur_radius: shadow.blur,
                },
            );
            let mut card_content = row![];
            if let Some(gutter) = gutter {
                card_content = card_content.push(gutter);
            }
            card_content = card_content.push(content);
            // As Slint: the group's card leans by its seed, within the
            // theme's tilt; its words stay straight.
            let lean = cards.tilt.map_or(0.0, |tilt| {
                crate::themes::seeded_lean(crate::themes::slint_seed(&look.group), tilt)
            });
            let card = container(card_content.spacing(if show_gutter { 16 } else { 0 }))
                .padding(Padding {
                    // A row within the group keeps the gap above it inside
                    // the card too.
                    top: if first {
                        padding
                    } else if look.divided() {
                        CONTENT_PAD
                    } else {
                        CONTENT_PAD + theme.spacing().message_gap.unwrap_or(1.0)
                    },
                    // Before the group's end the row's gap is the card's
                    // own (the timeline leaves none under it), so no seam
                    // shows between its rows.
                    bottom: if end {
                        padding
                    } else {
                        CONTENT_PAD + theme.spacing().message_gap.unwrap_or(1.0)
                    },
                    left: padding,
                    right: padding,
                })
                .width(Length::Fill);
            // The row's tint, a theme's highlighter, a mention's edge and the
            // keyboard's lie on the card, as Slint's, not across the row.
            // Lit while pointed at, or while what its bar opened is open, as
            // Slint keeps the active message lit.
            let pointed = ctx.pointed.message.as_deref() == Some(message.item.as_str())
                || crate::message::hover_bar::kept(ctx, &message.item);
            let highlighter = ctx
                .themes
                .event_effect(
                    &theme.id,
                    yapfast_core::themes::package::art::EventSlot::Row,
                )
                .filter(|_| pointed)
                .map(|event| crate::themes::argb(event.colour));
            let tint = crate::timeline::view::row_ground(ctx, index, look)
                .filter(|tint| highlighter.is_none() || *tint != theme.row_hover);
            let edge = if ctx.lanes.keyed == Some(message.item.as_str()) {
                Some((theme.accent, 3.0))
            } else if look.highlighted {
                Some((theme.accent, 2.0))
            } else {
                None
            };
            let marks = crate::theme_art::CardMarks {
                tint,
                highlighter,
                edge,
            };
            // Where the card's top is above this row: the rows of a group
            // turn about one point, so they line up as one card.
            let pivot = if first {
                0.0
            } else {
                let mut start = index;
                while start > 0
                    && start != crate::timeline::ASIDE
                    && ctx.row_look(start).is_some_and(|look| !look.first)
                {
                    start -= 1;
                }
                (ctx.row_body_top(index) - ctx.row_body_top(start)).max(0.0)
            };
            let card: Element<'a, Message> = if lean != 0.0 {
                crate::theme_art::card_ground(
                    card.into(),
                    Some(ground),
                    shadow,
                    lean,
                    pivot,
                    // A row after the first reaches up under the one
                    // before, so their turned edges leave no seam.
                    if first { 0.0 } else { 2.0 },
                    marks,
                )
            } else {
                // The group's card casts one shadow: a row before its end
                // keeps only the shadow beside it, none across the seam.
                let mut shadow = shadow;
                if !end {
                    shadow.offset.y = 0.0;
                }
                container(crate::theme_art::card_ground(
                    card.into(),
                    None,
                    iced::Shadow::default(),
                    0.0,
                    0.0,
                    0.0,
                    marks,
                ))
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(ground.into()),
                    border: iced::Border {
                        width: 0.0,
                        ..border
                    },
                    shadow,
                    ..Default::default()
                })
                .into()
            };
            let card = crate::theme_art::card_edges(card, border, corner, first, end);
            let card = crate::theme_art::outlined(
                card,
                ctx,
                yapfast_core::themes::layout::BorderPlace::Card,
                "message",
                corner,
                outline_fallback.width,
                outline_fallback.color,
            );
            // The theme's pictures on the card's edges: on its top edge on
            // the group's first row, its bottom edge on its last. A place
            // with several picks one a card.
            let mut card = iced::widget::Stack::with_children([card]);
            for (bottom, tiles) in crate::themes::card_tiles(cards) {
                if (bottom && !end) || (!bottom && !first) || tiles.is_empty() {
                    continue;
                }
                let pick = message.item.bytes().fold(0usize, |hash, byte| {
                    hash.wrapping_mul(31).wrapping_add(byte.into())
                }) % tiles.len();
                if let Some(art) = crate::theme_art::card_art(ctx, &tiles[pick], lean) {
                    card = card.push(art);
                }
                // Tape cut out of the picture lies over it, across the
                // torn edge: half on the board, pinning the card down.
                if let Some(tape) = ctx.themes.card_tape(&theme.id, &tiles[pick])
                    && let Some(art) = crate::theme_art::card_art(ctx, tape, lean)
                {
                    card = card.push(art);
                }
            }
            // The card spans the lane, as Slint's: the gap between cards
            // is the rows' (`top_space`), and a lane at the theme's widest
            // is the card's width.
            let side = if package_layout
                .and_then(|layout| layout.chat_max_width)
                .is_some()
            {
                0.0
            } else {
                16.0
            };
            container(card)
                .width(Length::Fill)
                .padding(Padding {
                    top: 0.0,
                    bottom: 0.0,
                    left: side,
                    right: if editing { 21.0 } else { side },
                })
                .into()
        }
        _ => {
            let mut flat = row![];
            if let Some(gutter) = gutter {
                flat = flat.push(gutter);
            }
            flat = flat.push(content);
            container(flat.spacing(if show_gutter { 16 } else { 0 }))
                .width(Length::Fill)
                .padding(Padding {
                    top: CONTENT_PAD,
                    bottom: CONTENT_PAD,
                    left: (gutter_width - avatar).max(0.0) / 2.0,
                    right: if editing { 21.0 } else { END_PAD },
                })
                .into()
        }
    };
    if !header || message_style != MessageStyle::Flat {
        return lane;
    }
    // The spine from the line above down to the avatar, over the
    // gutter's edge and the space after it.
    let spine = ctx.look.glyph(Icon::ReplySpine, SPINE, theme.reply_spine);
    stack![
        lane,
        placed(
            spine,
            (gutter_width - avatar) / 2.0 + avatar / 2.0 - 1.0,
            CONTENT_PAD + 11.0,
        )
    ]
    .into()
}

/// A collapsed message from a blocked or ignored account.
fn hidden_row<'a>(ctx: crate::ctx::Ctx<'a>, look: &RowLook, blocked: bool) -> Element<'a, Message> {
    let (author, message, show) = if blocked {
        ("Blocked user", "Blocked message.", "Show it")
    } else {
        ("Ignored user", "Ignored message.", "Show")
    };
    let theme = ctx.look.theme;
    let avatar = crate::timeline::avatar(theme);
    let gutter_width = crate::timeline::gutter(theme);
    let mut content = column![].width(Length::Fill);
    let mut gutter =
        stack![
            Space::new()
                .width(avatar)
                .height(if look.first { avatar } else { 0.0 })
        ];
    if look.first {
        content = content.push(
            container(
                text(author)
                    .size(text_size(ctx))
                    .font(MEDIUM)
                    .color(theme.text)
                    .height(AUTHOR_LINE),
            )
            .padding(Padding::ZERO.bottom(AUTHOR_GAP)),
        );
        gutter = gutter.push(placed(
            ctx.avatar(None, "?", avatar, avatar / 2.0, 1.0),
            0.0,
            2.0,
        ));
    }
    content = content.push(
        row![
            text(message)
                .size(text_size(ctx))
                .font(ITALIC)
                .color(theme.text),
            button(text(show).size(text_size(ctx)))
                .padding(0)
                .on_press(Message::Timeline(crate::timeline::Event::ShowBlocked(
                    look.item.clone()
                )))
                .style(controls::ink_style(theme.link, theme.text)),
        ]
        .spacing(4)
        .height(LINE_HEIGHT)
        .align_y(Alignment::Center),
    );
    container(row![container(gutter).width(avatar), content].spacing(16))
        .width(Length::Fill)
        .padding(Padding {
            top: CONTENT_PAD,
            bottom: CONTENT_PAD,
            left: (gutter_width - avatar) / 2.0,
            right: END_PAD,
        })
        .into()
}

/// An app's answer still to come, as Discord's client words it: "{app} is
/// thinking…", and once it has waited too long, that it didn't respond.
fn thinking<'a>(ctx: crate::ctx::Ctx<'a>, message: &ChatMessage) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let timed_out = message.timestamp != 0
        && yapfast_core::clock::now_ms() / 1000 >= message.timestamp + THINKING_SECS;
    if timed_out {
        text("The application did not respond")
            .size(text_size(ctx))
            .color(theme.danger)
            .into()
    } else {
        text(format!("{} is thinking…", message.author))
            .size(text_size(ctx))
            .font(ITALIC)
            .color(theme.muted)
            .into()
    }
}

/// What's left of a message: its author's avatar, a notice's icon, the
/// clock time of a grouped message under the pointer, and the spine
/// that leads a reply's line to its avatar.
fn gutter<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    message: &'a ChatMessage,
    look: &RowLook,
    header: bool,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let avatar = crate::timeline::avatar(theme);
    let mut layers =
        stack![
            Space::new()
                .width(avatar)
                .height(if look.first && message.notice.is_none() {
                    if header { avatar + 24.0 } else { avatar }
                } else {
                    0.0
                })
        ];
    if let Some(notice) = &message.notice {
        let (icon, ink) = match notice.kind {
            NoticeKind::Pinned(_) => (Icon::Pin, theme.muted),
            NoticeKind::Joined => (Icon::UserPlus, theme.live),
            NoticeKind::ThreadStarted => (Icon::Thread, theme.muted),
            NoticeKind::Call => (Icon::Phone, theme.live),
            NoticeKind::PollResult => (Icon::ChartBar, theme.muted),
        };
        layers = layers.push(placed(
            ctx.look.glyph(icon, 16, ink),
            12.0,
            (text_size(ctx) * 1.375 - 16.0) / 2.0 + 1.0,
        ));
    } else if look.first {
        let decoration = ctx
            .state
            .cosmetics
            .decoration(message.decoration.as_deref());
        let pointed = ctx.pointed.message.as_deref() == Some(message.item.as_str());
        let face = ctx.face(
            Face::new(message.avatar.as_deref(), &message.author, avatar)
                .decoration(decoration, pointed)
                .illuminated(pointed),
        );
        let face = mouse_area(face)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_press(Message::Timeline(crate::timeline::Event::OpenAuthor(
                look.item.clone(),
            )));
        layers = layers.push(placed(
            person_menu(ctx, index, message, face),
            0.0,
            if header { 24.0 } else { 2.0 },
        ));
    } else if ctx.pointed.message.as_deref() == Some(message.item.as_str()) {
        // Right-aligned to the avatar's edge.
        let clock = sent_time(ctx, index, message)
            .split_whitespace()
            .find(|word| word.contains(':'))
            .unwrap_or_default();
        layers = layers.push(placed(
            container(
                text(clock)
                    .size(theme.type_size("time", 12.0))
                    .font(theme.font("time", MEDIUM))
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            )
            .width(avatar)
            .align_x(Alignment::End),
            0.0,
            4.0,
        ));
    }
    container(layers).width(avatar).into()
}

/// What a right click on a message's author (their avatar or name) opens:
/// their menu, where the pointer is, rather than the message's.
fn person_menu<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    message: &'a ChatMessage,
    on: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let sender = message.sender.as_str();
    if sender.is_empty() {
        return on.into();
    }
    let user = yapfast_core::discord::sender_user(sender).unwrap_or(sender);
    // The press is in the list's space: the lane's corner, as the press
    // itself notes it, makes it the window's.
    RightPress::new(on, move |at| {
        let (lane, _) = ctx.lane_place(index);
        let origin = iced::Vector::new(lane.left, lane.top);
        Message::PersonMenu(user.to_owned(), at + origin)
    })
    .into()
}

/// The quote a reply leads with: its author's avatar and "@name" in
/// their colour, its text on one line; a click jumps to it.
fn reply_line<'a>(
    ctx: crate::ctx::Ctx<'a>,
    look: &'a RowLook,
    reply: &'a chat::Reply,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let mut line = row![].spacing(6).align_y(Alignment::Center);
    if !reply.author.is_empty() {
        line = line
            .push(ctx.face(Face::new(
                reply.avatar.as_deref(),
                &reply.author,
                HEADER_AVATAR,
            )))
            .push(
                text(format!("@{}", reply.author))
                    .size(theme.type_size("names", 14.0))
                    .font(theme.font("names", MEDIUM))
                    .color(reply.color.map_or(theme.secondary, hex))
                    .wrapping(text::Wrapping::None),
            );
        // The tag the author wears, a little smaller than a message's.
        if let Some(tag) = ctx.state.cosmetics.tag(reply.tag.as_ref()) {
            let size = 12.0 * 0.9;
            line = line.push(crate::tag_card::opens(
                ctx.tag_chip(
                    &tag.text,
                    tag.badge.as_deref(),
                    Chip {
                        size,
                        font: SEMIBOLD,
                        height: size * 1.3333,
                        pad: 4.0,
                        radius: 4.0,
                        fill: CHIP,
                        ink: theme.text,
                    },
                    1.0,
                ),
                tag,
            ));
        }
    }
    line = line.push(
        container(ctx.inline_emoji_line(
            &[],
            &look.body.reply,
            &reply.emoji,
            Look {
                size: theme.type_size("small", 13.0),
                line_height: 18.0,
                ink: theme.text,
            },
            REPLY_EMOJI,
            true,
        ))
        .width(Length::Fill)
        .clip(true),
    );
    let line = container(line)
        .height(HEADER_LINE)
        .align_y(Alignment::Center);
    match &reply.item {
        Some(item) => button(line)
            .padding(0)
            .width(Length::Fill)
            .on_press(Message::Link(
                format!("{}{item}", crate::links::JUMP_LINK),
                Point::ORIGIN,
            ))
            .style(|_, _| button::Style::default())
            .into(),
        None => line.into(),
    }
}

/// An app's answer's line: "Ann used /ping". The person opens their card;
/// a slash command, where one may write, is written in the composer for
/// its list to find.
fn used_line<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    used: &'a chat::UsedCommand,
) -> Element<'a, Message> {
    use crate::timeline::Event;
    let theme = ctx.look.theme;
    let user = text(used.user.as_str())
        .size(theme.type_size("names", 14.0))
        .font(theme.font("names", MEDIUM))
        .color(used.color.map_or(theme.secondary, hex));
    let user: Element<'a, Message> = if used.sender.is_empty() {
        user.into()
    } else {
        let sender = &used.sender;
        let id = yapfast_core::discord::sender_user(sender).unwrap_or(sender);
        mouse_area(user)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_press(Message::Timeline(Event::OpenPerson(id.to_owned())))
            .into()
    };
    let command = text(used.name.as_str())
        .size(theme.type_size("body", 14.0))
        .font(theme.font("body", MEDIUM));
    let command: Element<'a, Message> = match used.slash.as_deref() {
        // Written in the channel's composer, so not from the thread beside.
        Some(slash)
            if !slash.is_empty()
                && index < crate::timeline::ASIDE
                && crate::composer::composer_look(ctx).enabled =>
        {
            button(command)
                .padding(0)
                .on_press(Message::Timeline(Event::UseCommand(slash.to_owned())))
                .style(controls::ink_style(theme.link, theme.text))
                .into()
        }
        _ => command.color(theme.link).into(),
    };
    let mut line = row![
        ctx.face(Face::new(used.avatar.as_deref(), &used.user, HEADER_AVATAR)),
        user,
        text("used")
            .size(theme.type_size("small", 14.0))
            .font(theme.font("small", REGULAR))
            .color(theme.muted),
        command,
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    if let Some(target) = used.target.as_deref().filter(|target| !target.is_empty()) {
        line = line.push(
            text(format!("on @{target}"))
                .size(theme.type_size("small", 14.0))
                .font(theme.font("small", REGULAR))
                .color(theme.muted),
        );
    }
    container(line)
        .height(HEADER_LINE)
        .align_y(Alignment::Center)
        .clip(true)
        .into()
}

/// Messages' text size, as Messages' settings chose it.
pub fn text_size(ctx: crate::ctx::Ctx<'_>) -> f32 {
    ctx.look.theme.type_size(
        "body",
        crate::settings::message_size(ctx.layout.prefs.text_size),
    )
}

/// When the message at `index` was sent, on the clock Messages'
/// settings chose.
fn sent_time<'a>(ctx: crate::ctx::Ctx<'a>, index: usize, message: &'a ChatMessage) -> &'a str {
    ctx.row_look(index)
        .map_or(message.time.as_str(), |look| &*look.time)
}

/// What the timeout clock's tooltip says, as Discord's client words it.
pub const TIMED_OUT_TIP: &str =
    "This is only visible to moderators and indicates this user is timed out.";

/// A message's author line with Discord's timeout clock before the name,
/// while the author is timed out and this account may time them out (the
/// core leaves [`ChatMessage::author_timed_out_until`] empty otherwise).
/// Its end is checked when the row is drawn, as Discord's client checks it
/// when it renders the message: nothing wakes for it.
fn timed_out_mark<'a>(
    ctx: crate::ctx::Ctx<'a>,
    message: &ChatMessage,
    line: Element<'a, Message>,
) -> Element<'a, Message> {
    let now = yapfast_core::clock::now_ms();
    if !message
        .author_timed_out_until
        .is_some_and(|until| until > now)
    {
        return line;
    }
    let theme = ctx.look.theme;
    // The member list's clock, in its colour.
    let clock = iced::widget::tooltip(
        ctx.look.glyph(Icon::Clock, 14, theme.danger),
        controls::tip(theme, TIMED_OUT_TIP),
        iced::widget::tooltip::Position::Top,
    )
    .gap(6);
    row![clock, line]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
}

/// A message's author line: the name in its style's font (only the font, in
/// a server) and role colour, the server tag chip, APP for a bot, the time.
fn author_line<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    message: &'a ChatMessage,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let cosmetics = ctx.state.cosmetics;
    let mut style = looks::name_look(
        cosmetics.style(message.style.as_deref()),
        &message.author,
        ctx.in_server,
        theme.dark,
    );
    let content = ctx
        .themes
        .content(&theme.id, ctx.layout.theme_content.contains(&theme.id));
    if content
        .is_some_and(|content| content.names.styles == yapfast_core::themes::content::Follow::Theme)
    {
        style = None;
    }
    let color = message.color.map_or(theme.channel_white, hex);
    // Out of a server a whole style shows; its first colour stands in
    // for the effect in a timeline row.
    let mut color = match &style {
        Some(style) if !ctx.in_server => style.colors.first().map_or(color, |rgb| hex(*rgb)),
        _ => color,
    };
    if let Some(names) = content.map(|content| &content.names) {
        color = crate::people::themed_name_colour(names, Some(&message.sender), color, theme);
    }
    let author = ctx.themes.marked(
        &theme.id,
        yapfast_core::themes::chrome::Place::Author,
        &message.author,
    );
    let shown_author = shown_name(
        &author,
        style.as_ref().and_then(crate::looks::NameLook::font),
    )
    .into_owned();
    let hover = crate::theme_effects::name_hover_effect(ctx, &shown_author);
    // A name longer than its room (the bubble's share, or the lane, less
    // its time) ends in "…" rather than widening its bubble past it.
    let room = (content_width_of(ctx, index) - NAME_TIME_ROOM).max(48.0);
    let name_size = theme.type_size("names", text_size(ctx));
    let long = shown_author.chars().count() as f32 * name_size * 0.62 > room;
    let name_font = {
        let stock = style_font(style.as_ref().and_then(crate::looks::NameLook::font), 500);
        if style.is_some() {
            stock
        } else {
            theme.font("names", stock)
        }
    };
    let elided: Option<Element<'a, Message>> = long.then(|| {
        let mut words = crate::elide::elide(shown_author.clone())
            .size(name_size)
            .font(name_font)
            .color(color)
            .line_height(AUTHOR_LINE)
            .light(crate::theme_effects::text_light(
                ctx,
                yapfast_core::themes::package::art::TextRole::Names,
            ))
            .shrink();
        if let Some(effect) = hover {
            words = words.hover_effect(effect);
        }
        container(words).max_width(room).into()
    });
    // A gradient role colour runs across the name, letter by letter, as
    // Discord draws it, unless the theme inks names itself; otherwise the
    // one colour.
    let spans = match message
        .color_end
        .filter(|_| message.color.is_some() && content.is_none())
    {
        Some(end) => crate::looks::gradient_letters(
            &shown_author,
            [color, hex(end.0)],
            end.1.map(hex),
            name_size * 0.6,
        )
        .into_iter()
        .map(|(letter, ink)| span(letter).font(name_font).color(ink))
        .collect(),
        None => vec![span(shown_author).font(name_font)],
    };
    let mut name = Piece::new(spans, index, 0, Message::Link, Message::Select)
        .item(Some(ctx.row_item(index)))
        .size(theme.type_size("names", text_size(ctx)), AUTHOR_LINE)
        .color(color)
        .light(crate::theme_effects::text_light(
            ctx,
            yapfast_core::themes::package::art::TextRole::Names,
        ))
        .starts(false)
        .selection(
            ctx.selected(index, 0),
            ctx.lanes.timeline.selecting,
            theme.token_colour("selection", theme.selection),
        );
    if let Some(effect) = hover {
        name = name.hover_effect(effect);
    }
    // A theme's ransom-note names, for someone with no name style of
    // their own (as Slint).
    let name: Element<'a, Message> = match style
        .is_none()
        .then(|| {
            // Scraps the author line's height (Slint's take its height,
            // a little taller than this line).
            crate::theme_chrome::name_ransom_in(
                ctx,
                &author,
                theme.type_size("names", text_size(ctx)) * 0.85,
                Some(AUTHOR_LINE + 2.0),
            )
        })
        .flatten()
    {
        Some(ransom) => ransom,
        None => elided.unwrap_or_else(|| name.into()),
    };
    let name = mouse_area(name)
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(Message::Timeline(crate::timeline::Event::OpenAuthor(
            ctx.row_item(index),
        )));
    // Underlined under the pointer, as a link is; drawn, with no message.
    let name: Element<'a, Message> = if message.sender.is_empty() {
        name.into()
    } else {
        let under = column![
            // Under the name's letters, and within the line at any size.
            Space::new()
                .height(((AUTHOR_LINE + text_size(ctx)) / 2.0 + 1.0).min(AUTHOR_LINE - 1.0)),
            controls::bar(color, Length::Fill, 1),
        ];
        crate::widgets::Hover::new(name, under).into()
    };
    let mut line = row![person_menu(ctx, index, message, name)]
        .spacing(8)
        .align_y(Alignment::Center);
    if let Some(tag) = cosmetics.tag(message.tag.as_ref()) {
        let size = text_size(ctx) * 0.72;
        let chip = ctx.tag_chip(
            &tag.text,
            tag.badge.as_deref(),
            Chip {
                size,
                font: SEMIBOLD,
                height: size * 1.3333,
                pad: 4.0,
                radius: 4.0,
                fill: CHIP,
                ink: theme.text,
            },
            1.0,
        );
        line = line.push(crate::tag_card::opens(chip, tag));
    }
    // After the server tag, as Discord's client puts it, in its green
    // (#23a559), which is the theme's live colour on Bonfire.
    if message.new_member {
        line = line.push(ctx.look.glyph(Icon::StarFilled, 14, theme.live));
    }
    // OP gives way to the APP tag, as in Discord's client.
    if message.bot {
        line = line.push(ctx.bot_tag(1.0));
    } else if ctx
        .post_author
        .is_some_and(|author| author == message.sender)
    {
        line = line.push(ctx.blurple_tag("OP", 1.0));
    }
    let sent = ctx
        .themes
        .marked_time(&theme.id, sent_time(ctx, index, message));
    let time = Piece::new(
        vec![span(sent).font(theme.font("time", REGULAR))],
        index,
        1,
        Message::Link,
        Message::Select,
    )
    .item(Some(ctx.row_item(index)))
    .size(theme.type_size("time", text_size(ctx) - 4.0), 16.0)
    // Discord's dark theme draws the time in its channel list's grey.
    .color(if theme.bonfire {
        theme.channel_read
    } else {
        theme.muted
    })
    .selection(
        ctx.selected(index, 1),
        ctx.lanes.timeline.selecting,
        theme.token_colour("selection", theme.selection),
    );
    line.push(time).height(AUTHOR_LINE).into()
}

/// A body that folds, when long messages fold: once it's a few lines past
/// twelve, so Show more never uncovers only a line or two, it's cut to
/// twelve with Show more under it.
fn folding<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    let item = ctx.row_item(index);
    let whole = ctx.lanes.timeline.revealed.contains(&fold_key(&item));
    let theme = ctx.look.theme;
    let more = button(
        text(if whole { "Show less" } else { "Show more" })
            .size(text_size(ctx) * 0.875)
            .font(SEMIBOLD),
    )
    .padding(0)
    .on_press(Message::Timeline(crate::timeline::Event::Unfold(item)))
    .style(controls::ink_style(theme.link, theme.text));
    Fold::new(
        body,
        more,
        !whole,
        (LINE_HEIGHT * 15.0, LINE_HEIGHT * 12.0, 3.0),
    )
    .into()
}

/// Before this thread (by its snowflake, July 2022), Discord counted a
/// thread's messages only up to 50.
const COUNTED_TO_50: u64 = 992_549_565_104_128_000;

/// The card under a message that leads to a thread, as Discord's client
/// draws it: the thread's name and how many messages it has, its newest
/// message under them with how long ago the thread was active. A click
/// opens the thread beside the channel; under the pointer the count is
/// underlined.
fn thread_card<'a>(
    ctx: crate::ctx::Ctx<'a>,
    item: Arc<str>,
    thread: &'a chat::ThreadLink,
    notice: bool,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let lit = ctx.pointed.thread.as_ref() == Some(&item);
    let id = yapfast_core::discord::parse_room(&thread.room)
        .and_then(|(_, id)| id.parse().ok())
        .unwrap_or(u64::MAX);
    let count = text(thread_count(thread.messages, id))
        .size(14)
        .line_height(text::LineHeight::Absolute(18.into()))
        .font(SEMIBOLD)
        .color(theme.link)
        .wrapping(text::Wrapping::None);
    let count: Element<'a, Message> = if lit {
        column![count, controls::bar(theme.link, Length::Fill, 1)]
            .width(Length::Shrink)
            .into()
    } else {
        count.into()
    };
    let top = row![
        crate::elide::elide(thread.name.as_str())
            .size(14)
            .line_height(18.0)
            .font(theme.font("body", SEMIBOLD))
            .color(theme.text),
        count,
    ]
    .spacing(8)
    .width(Length::Shrink)
    .height(18);
    let words = |words: &'a str, ink| {
        crate::elide::elide(words)
            .size(14)
            .line_height(18.0)
            .color(ink)
    };
    let mut bottom = row![].spacing(4).align_y(Alignment::Center);
    match &thread.recent {
        _ if thread.archived => {
            bottom = bottom.push(words(
                "There are no recent messages in this thread.",
                theme.text,
            ));
        }
        _ if thread.messages.unwrap_or_default() == 0 => {
            bottom = bottom.push(words(
                "There are no messages in this thread yet.",
                theme.text,
            ));
        }
        None => {
            bottom = bottom.push(words(
                "There are no recent messages in this thread.",
                theme.text,
            ));
        }
        Some(recent) => {
            let author = &recent.author;
            bottom = bottom
                .push(
                    container(ctx.face(Face::new(
                        Some(author.avatar.as_str()),
                        &author.name,
                        HEADER_AVATAR,
                    )))
                    .padding(Padding::ZERO.right(4)),
                )
                .push(
                    text(author.name.as_str())
                        .size(14)
                        .line_height(text::LineHeight::Absolute(18.into()))
                        .color(author.color.map_or(theme.text, hex).scale_alpha(0.64))
                        .wrapping(text::Wrapping::None),
                );
            if let Some(tag) = ctx.state.cosmetics.tag(recent.tag.as_ref()) {
                let size = 12.0 * 0.9;
                bottom = bottom.push(ctx.tag_chip(
                    &tag.text,
                    tag.badge.as_deref(),
                    Chip {
                        size,
                        font: SEMIBOLD,
                        height: size * 1.3333,
                        pad: 4.0,
                        radius: 4.0,
                        fill: CHIP,
                        ink: theme.text,
                    },
                    1.0,
                ));
            }
            bottom = bottom.push(words(recent.text.as_str(), theme.text));
        }
    }
    if !thread.archived && thread.messages.unwrap_or_default() > 0 {
        let ago = thread_ago(thread.active_ms, ctx.minute * 1000);
        bottom = bottom.push(
            container(
                text(ago)
                    .size(14)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            )
            .padding(Padding::ZERO.left(4)),
        );
    }
    // As wide as its lines, up to its widest: the name and the newest
    // message are cut to the room the count and the time leave them. Their
    // text would have it fill its room.
    // A row's parts widen it as they're pushed: shrunk after them.
    let bottom = container(bottom.width(Length::Shrink))
        .height(18)
        .align_y(Alignment::Center);
    let face = column![top, bottom].spacing(2).width(Length::Shrink);
    // Discord's dark card is a step above the chat in a line a step above
    // that.
    let (ground, edge) = if theme.bonfire {
        (hex(0x242429), hex(0x323237))
    } else {
        (theme.raised, theme.edge)
    };
    let card = button(face)
        .padding(8)
        .width(Length::Shrink)
        .on_press(Message::Thread(crate::thread::Event::Open(
            thread.room.clone(),
        )))
        .style(controls::outlined_lit_style(
            ground, ground, edge, edge, 4.0,
        ));
    let card = mouse_area(
        container(card)
            .width(Length::Shrink)
            .max_width(THREAD_WIDTH),
    )
    .on_enter(Message::Timeline(crate::timeline::Event::PointThread(
        item.clone(),
        true,
    )))
    .on_exit(Message::Timeline(crate::timeline::Event::PointThread(
        item, false,
    )));
    container(card)
        .width(Length::Shrink)
        .padding(Padding::ZERO.top(if notice { 0 } else { 8 }))
        .into()
}

/// A thread's count on its card, as Discord's client words it: "See
/// Thread" before any message, and "50+" for an old thread Discord stopped
/// counting.
fn thread_count(messages: Option<u64>, id: u64) -> String {
    match messages.unwrap_or_default() {
        0 => "See Thread \u{203a}".into(),
        50.. if id <= COUNTED_TO_50 => "50+ Messages \u{203a}".into(),
        100_000.. => "100k+ Messages \u{203a}".into(),
        1 => "1 Message \u{203a}".into(),
        count => format!("{count} Messages \u{203a}"),
    }
}

/// How long ago a thread was last active, as its card says it: "1m ago"
/// for anything under two minutes, the date past a year of 30-day months.
fn thread_ago(at_ms: u64, now_ms: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    let seconds = now_ms.saturating_sub(at_ms) / 1000;
    match seconds {
        0..MINUTE => "1m ago".into(),
        MINUTE..HOUR => format!("{}m ago", seconds / MINUTE),
        HOUR..DAY => format!("{}h ago", seconds / HOUR),
        _ if seconds < 30 * DAY => format!("{}d ago", seconds / DAY),
        _ if seconds < 360 * DAY => ">30d ago".into(),
        _ => yapfast_core::clock::day(at_ms / 1000),
    }
}

/// How a message of one's own is going, under it: "Sending..." with Cancel
/// while its files go, "Not sent yet" while the connection is down, or "Not
/// sent" with Retry.
fn delivery_line<'a>(
    ctx: crate::ctx::Ctx<'a>,
    message: &ChatMessage,
) -> Option<Element<'a, Message>> {
    use crate::composer::Event;
    let theme = ctx.look.theme;
    let (words, retry, cancel) = match message.delivery {
        chat::Delivery::Sent => return None,
        chat::Delivery::Sending if message.attachments.is_empty() => return None,
        chat::Delivery::Sending => ("Sending...", false, true),
        chat::Delivery::Waiting => (
            "Not sent yet. Yapfast sends it when the connection returns.",
            false,
            true,
        ),
        // An app's command, whose row says why.
        chat::Delivery::Failed if message.item.starts_with("c:") => ("Not sent.", true, true),
        chat::Delivery::Failed => ("Not sent. The server refused this message.", true, true),
    };
    let failed = message.delivery == chat::Delivery::Failed;
    let mut line = row![
        text(words)
            .size(theme.type_size("small", 13.0))
            .font(theme.font("small", REGULAR))
            .color(if failed { theme.danger } else { theme.muted })
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let item = &message.item;
    if retry {
        line = line.push(
            controls::action(
                theme,
                "Retry",
                controls::Kind::Ghost,
                Some(Message::Write(Event::Retry(item.clone()))),
            )
            .small(),
        );
    }
    if cancel {
        line = line.push(
            controls::action(
                theme,
                "Cancel",
                controls::Kind::Ghost,
                Some(Message::Write(Event::CancelSend(item.clone()))),
            )
            .small(),
        );
    }
    Some(line.into())
}

/// As Discord's client marks an answer only this account sees.
fn ephemeral_line<'a>(ctx: crate::ctx::Ctx<'a>, message: &ChatMessage) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let preview = ctx.state.preview;
    let dismiss = button(
        text("Dismiss message")
            .size(theme.type_size("small", 12.0))
            .font(theme.font("small", REGULAR)),
    )
    .padding(0)
    .on_press_maybe(
        (!preview).then(|| Message::Write(crate::composer::Event::Dismiss(message.item.clone()))),
    )
    .style(controls::ink_style(
        if preview { theme.muted } else { theme.link },
        theme.text,
    ));
    row![
        ctx.look.glyph(Icon::Eye, 16, theme.muted),
        text("Only you can see this ·")
            .size(theme.type_size("small", 12.0))
            .font(theme.font("small", REGULAR))
            .color(theme.muted),
        dismiss,
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

/// Where a log's (`irc`) text column starts in its row, as Slint's
/// `column-left`: the chat's padding, the time, the nick and the separator,
/// 6 px apart. `None` outside a log, or for a notice.
pub fn irc_column_left(ctx: crate::ctx::Ctx<'_>, message: &ChatMessage) -> Option<f32> {
    if message.notice.is_some() {
        return None;
    }
    irc_left(
        ctx.themes.package_layout(&ctx.look.theme.id)?,
        ctx.look.theme,
        text_size(ctx),
    )
}

/// [`irc_column_left`] for a log's messages, from the theme's layout and
/// the messages' text size; `None` outside a log.
pub fn irc_left(
    layout: &yapfast_core::themes::layout::Layout,
    theme: &crate::theme::Theme,
    text_size: f32,
) -> Option<f32> {
    if layout.messages != Some(MessageStyle::Irc) {
        return None;
    }
    let time = layout.irc.time_width.unwrap_or(64.0);
    let nick = layout.irc.nick_width.unwrap_or(112.0);
    let rule = layout.irc.rule == Some(true);
    let separator = layout
        .irc
        .separator
        .clone()
        .unwrap_or_else(|| if rule { "│".into() } else { String::new() });
    let lead = theme.spacing().chat_padding.unwrap_or(END_PAD);
    // A separator is a glyph or two of the log's monospaced face.
    let sep = separator.chars().count() as f32 * text_size * 0.6;
    let gaps = if time > 0.0 { 6.0 } else { 0.0 } + 6.0 + if sep > 0.0 { 6.0 } else { 0.0 };
    Some(lead + if time > 0.0 { time } else { 0.0 } + nick + sep + gaps)
}

/// The room row `row`'s content has across: its timeline's width but
/// the gutter and the room kept for its actions; in a log, from its text
/// column on.
pub fn content_width_of(ctx: crate::ctx::Ctx<'_>, row: usize) -> f32 {
    let (lane, _) = ctx.lane_place(row);
    if let Some(left) = ctx
        .row_message(row)
        .and_then(|message| irc_column_left(ctx, message))
    {
        return (lane.width - left - END_PAD).max(0.0);
    }
    let width = (lane.width - crate::timeline::gutter(ctx.look.theme) - END_PAD).max(0.0);
    // In a theme's bubbles, what's sized from the room (pictures, embeds)
    // fits inside the bubble: its share of the lane, less its padding. A
    // picture wider than that widened the bubble's words past it.
    let layout = ctx.themes.package_layout(&ctx.look.theme.id);
    match layout.filter(|layout| layout.messages == Some(MessageStyle::Bubbles)) {
        Some(layout) => {
            let (across, _) = layout
                .bubbles
                .padding
                .unwrap_or(yapfast_core::themes::layout::BUBBLE_PADDING);
            let share = layout.bubbles.max_width.map_or(width, |share| {
                let lane = layout
                    .chat_max_width
                    .unwrap_or(960.0)
                    .min(lane.width.max(1.0));
                (lane * share / 100.0).max(120.0)
            });
            (width.min(share) - 2.0 * across).max(0.0)
        }
        None => width,
    }
}

/// Whether `message` is being edited in place, where the field needs the
/// whole lane.
fn editing_row(ctx: crate::ctx::Ctx<'_>, message: &yapfast_core::chat::ChatMessage) -> bool {
    ctx.composer.editing.as_deref() == Some(message.item.as_str())
}
