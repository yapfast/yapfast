//! One message's content in the timeline: the gutter (the author's avatar,
//! a notice's icon, the time beside a grouped message under the pointer, a
//! reply's spine), the line above (the reply it quotes, the command it
//! answers), the author line, the text in its blocks, `(edited)`, picture
//! previews (GIFs playing), and reaction chips with who reacted in their
//! tooltips.

use iced::{
    Alignment, Element, Length, Padding, Point,
    widget::{Space, button, column, container, mouse_area, row, span, stack, text},
};
use yapfast_core::chat::{self, ChatMessage, NoticeKind, Placement};

use crate::{
    app::{App, Message},
    controls::{self, placed},
    fonts::{BOLD, ITALIC, MEDIUM, SEMIBOLD, shown_name, style_font},
    looks::{self, Chip},
    markdown::{FIRST_PART, Look},
    media::Gallery,
    pictures::{Face, Icon},
    piece::Piece,
    style::{CHIP, hex},
    timeline::{
        AUTHOR_GAP, AUTHOR_LINE, AVATAR, CONTENT_PAD, END_PAD, GUTTER, LINE_HEIGHT, RowLook,
    },
    widgets::{Fold, RightPress},
};

pub mod attachments;
mod embed;
pub mod hover_bar;
mod reaction_chips;

pub use attachments::files;
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
    let y = app.ctx().row_body_top(index) + CONTENT_PAD + if header { 24.0 } else { 2.0 };
    let x = lane.left + (GUTTER - AVATAR) / 2.0;
    let width = crate::profile::WIDTH;
    let beside = if x + AVATAR + 8.0 + width <= app.window.width - 8.0 {
        x + AVATAR + 8.0
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
    if message.blocked
        && !ctx
            .lanes
            .timeline
            .revealed
            .contains(&blocked_key(&message.item))
    {
        return blocked_row(ctx, look);
    }
    let theme = ctx.look.theme;
    let header = message.reply.is_some() || message.used.is_some();
    let mut content = column![].width(Length::Fill);
    if let Some(reply) = &message.reply {
        content = content.push(reply_line(ctx, look, reply));
    } else if let Some(used) = &message.used {
        content = content.push(used_line(ctx, index, used));
    }
    if look.first && message.notice.is_none() {
        content = content.push(
            container(author_line(ctx, index, message)).padding(Padding::ZERO.bottom(AUTHOR_GAP)),
        );
    }
    if message.forwarded {
        content = content.push(
            row![
                ctx.look.glyph(Icon::Forward, 16, theme.muted),
                text("Forwarded").size(14).font(ITALIC).color(theme.muted)
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }
    let ink = if message.deleted || message.delivery != chat::Delivery::default() {
        theme.muted
    } else if message.notice.is_some() {
        theme.secondary
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
        content = content.push(text("(edited)").size(10).color(theme.muted));
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
    if let Some(thread) = &message.thread {
        content = content.push(thread_chip(ctx, thread));
    }
    if !message.reactions.is_empty() {
        content = content.push(
            container(crate::message::reaction_chips::reactions(
                ctx, index, message,
            ))
            .padding(Padding::ZERO.top(4)),
        );
    }
    if let Some(delivery) = delivery_line(ctx, message) {
        content = content.push(delivery);
    }
    if message.flags & EPHEMERAL != 0 {
        content = content.push(ephemeral_line(ctx, message));
    }
    let gutter = gutter(ctx, index, message, look, header);
    let shown = container(row![gutter, content].spacing(16))
        .width(Length::Fill)
        .padding(Padding {
            top: CONTENT_PAD,
            bottom: CONTENT_PAD,
            left: (GUTTER - AVATAR) / 2.0,
            right: if editing { 21.0 } else { END_PAD },
        });
    if !header {
        return shown.into();
    }
    // The spine from the line above down to the avatar, over the
    // gutter's edge and the space after it.
    let spine = ctx.look.glyph(Icon::ReplySpine, SPINE, theme.reply_spine);
    stack![
        shown,
        placed(spine, (GUTTER - AVATAR) / 2.0 + 19.0, CONTENT_PAD + 11.0)
    ]
    .into()
}

/// A message from someone this account blocked, as Discord shows it
/// until asked: "Blocked user" by a blank face, and "Blocked message."
/// with Show it.
fn blocked_row<'a>(ctx: crate::ctx::Ctx<'a>, look: &RowLook) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let mut content = column![].width(Length::Fill);
    let mut gutter =
        stack![
            Space::new()
                .width(AVATAR)
                .height(if look.first { AVATAR } else { 0.0 })
        ];
    if look.first {
        content = content.push(
            container(
                text("Blocked user")
                    .size(text_size(ctx))
                    .font(MEDIUM)
                    .color(theme.text)
                    .height(AUTHOR_LINE),
            )
            .padding(Padding::ZERO.bottom(AUTHOR_GAP)),
        );
        gutter = gutter.push(placed(
            ctx.avatar(None, "?", AVATAR, AVATAR / 2.0, 1.0),
            0.0,
            2.0,
        ));
    }
    content = content.push(
        row![
            text("Blocked message.")
                .size(text_size(ctx))
                .font(ITALIC)
                .color(theme.text),
            button(text("Show it").size(text_size(ctx)))
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
    container(row![container(gutter).width(AVATAR), content].spacing(16))
        .width(Length::Fill)
        .padding(Padding {
            top: CONTENT_PAD,
            bottom: CONTENT_PAD,
            left: (GUTTER - AVATAR) / 2.0,
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
    let mut layers =
        stack![
            Space::new()
                .width(AVATAR)
                .height(if look.first && message.notice.is_none() {
                    if header { 64.0 } else { AVATAR }
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
        let face = ctx.face(
            Face::new(message.avatar.as_deref(), &message.author, AVATAR).decoration(
                decoration,
                ctx.pointed.message.as_deref() == Some(message.item.as_str()),
            ),
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
                    .size(12)
                    .font(MEDIUM)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            )
            .width(AVATAR)
            .align_x(Alignment::End),
            0.0,
            4.0,
        ));
    }
    container(layers).width(AVATAR).into()
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
                    .size(14)
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
                size: 13.0,
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
        .size(14)
        .font(MEDIUM)
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
    let command = text(used.name.as_str()).size(14).font(MEDIUM);
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
        text("used").size(14).color(theme.muted),
        command,
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    if let Some(target) = used.target.as_deref().filter(|target| !target.is_empty()) {
        line = line.push(text(format!("on @{target}")).size(14).color(theme.muted));
    }
    container(line)
        .height(HEADER_LINE)
        .align_y(Alignment::Center)
        .clip(true)
        .into()
}

/// Messages' text size, as Messages' settings chose it.
pub fn text_size(ctx: crate::ctx::Ctx<'_>) -> f32 {
    crate::settings::message_size(ctx.layout.prefs.text_size)
}

/// When the message at `index` was sent, on the clock Messages'
/// settings chose.
fn sent_time<'a>(ctx: crate::ctx::Ctx<'a>, index: usize, message: &'a ChatMessage) -> &'a str {
    ctx.row_look(index)
        .map_or(message.time.as_str(), |look| &*look.time)
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
    let style = looks::name_look(
        cosmetics.style(message.style.as_deref()),
        &message.author,
        ctx.in_server,
        theme.dark,
    );
    let color = message.color.map_or(theme.channel_white, hex);
    // Out of a server a whole style shows; its first colour stands in
    // for the effect in a timeline row.
    let color = match &style {
        Some(style) if !ctx.in_server => style.colors.first().map_or(color, |rgb| hex(*rgb)),
        _ => color,
    };
    let name = Piece::new(
        vec![
            span(
                shown_name(
                    &message.author,
                    style.as_ref().and_then(crate::looks::NameLook::font),
                )
                .into_owned(),
            )
            .font(style_font(
                style.as_ref().and_then(crate::looks::NameLook::font),
                500,
            )),
        ],
        index,
        0,
        Message::Link,
        Message::Select,
    )
    .item(Some(ctx.row_item(index)))
    .size(text_size(ctx), AUTHOR_LINE)
    .color(color)
    .starts(false)
    .selection(
        ctx.selected(index, 0),
        ctx.lanes.timeline.selecting,
        theme.selection,
    );
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
    let time = Piece::new(
        vec![span(sent_time(ctx, index, message))],
        index,
        1,
        Message::Link,
        Message::Select,
    )
    .item(Some(ctx.row_item(index)))
    .size(text_size(ctx) - 4.0, 16.0)
    // Discord's dark theme draws the time in its channel list's grey.
    .color(if theme.bonfire {
        theme.channel_read
    } else {
        theme.muted
    })
    .selection(
        ctx.selected(index, 1),
        ctx.lanes.timeline.selecting,
        theme.selection,
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

/// The thread a message started: its name, how many messages it has and
/// whether it's archived, and a click that opens it beside the channel.
fn thread_chip<'a>(ctx: crate::ctx::Ctx<'a>, thread: &'a chat::ThreadLink) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let count = match thread.messages {
        None => None,
        Some(0) => Some("No messages yet".to_owned()),
        Some(1) => Some("1 message".to_owned()),
        Some(count) => Some(format!("{count} messages")),
    };
    let detail = match (count, thread.archived) {
        (Some(count), true) => format!("{count}, archived"),
        (Some(count), false) => count,
        (None, true) => "Archived".into(),
        (None, false) => String::new(),
    };
    // The name as long as it is, the count after it, the chevron at
    // the end.
    let mut words = row![
        crate::elide::elide(thread.name.as_str())
            .size(text_size(ctx))
            .font(BOLD)
            .color(theme.text),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if !detail.is_empty() {
        words = words.push(text(detail).size(14).color(theme.link));
    }
    let face = row![
        ctx.look.glyph(Icon::Thread, 18, theme.muted),
        container(words).width(Length::Fill),
        ctx.look.glyph(Icon::ChevronRight, 16, theme.muted),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let (rest, lit, edge) = (theme.sidebar, theme.hover, theme.edge);
    let chip = button(face)
        .padding([8, 12])
        .on_press(Message::Thread(crate::thread::Event::Open(
            thread.room.clone(),
        )))
        .style(controls::outlined_lit_style(rest, lit, edge, edge, 8.0));
    container(container(chip).max_width(EMBED_WIDTH))
        .padding(Padding::ZERO.top(4))
        .into()
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
    let mut line =
        row![
            text(words)
                .size(13)
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
    let dismiss = button(text("Dismiss message").size(12))
        .padding(0)
        .on_press_maybe(
            (!preview)
                .then(|| Message::Write(crate::composer::Event::Dismiss(message.item.clone()))),
        )
        .style(controls::ink_style(
            if preview { theme.muted } else { theme.link },
            theme.text,
        ));
    row![
        ctx.look.glyph(Icon::Eye, 16, theme.muted),
        text("Only you can see this ·").size(12).color(theme.muted),
        dismiss,
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

/// The room row `row`'s content has across: its timeline's width but
/// the gutter and the room kept for its actions.
pub fn content_width_of(ctx: crate::ctx::Ctx<'_>, row: usize) -> f32 {
    let (lane, _) = ctx.lane_place(row);
    (lane.width - GUTTER - END_PAD).max(0.0)
}
