//! Who wrote a message: their picture, or the time, in the gutter before
//! the text, and the line with their name, their server's tag, "APP" for a
//! bot and when. The Slint side: the gutter and the author's line of
//! `MessageContent` (`ui/messages.slint:3086`-`:3133`, `:3200`-`:3262`) and
//! `BotTag` (`:2031`). A click on the picture or the name opens the
//! author's card beside it, a right click their menu; the name is
//! underlined under the pointer, as a link is.

mod underline;

use iced::font::Weight;
use std::sync::LazyLock;

use iced::widget::{Column, column, container, mouse_area, row, space, svg, text};
use iced::{Alignment, Background, Border, Color, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::message::{Intent, Message, Who};
use anomi_vm::chat::{self};
use anomi_vm::menu::Spot;
use anomi_widgets::avatar::portrait;
use anomi_widgets::menu::opener;
use anomi_widgets::name::{Ink, styled_name};
use anomi_widgets::selection;
use anomi_widgets::tag::server_tag;
use yapfast_lang::tr;

use super::notice;
use underline::underline;

/// The gutter: the picture's column, or in the compact display the time's
/// (`gutter`, `ClientStyle.compact-gutter`).
pub const GUTTER: f32 = 40.0;
pub const COMPACT_GUTTER: f32 = 60.0;
/// The picture: 2 px down, or 24 under the line over the message.
const AVATAR_TOP: f32 = 2.0;
const AVATAR_UNDER_HEADER: f32 = 24.0;
/// The reply's spine: its corner 19 px in and 11 down, reaching 33 px
/// right and 13 down to the quote, 2 px wide, rounded 6 (`:3090`-`:3093`).
const SPINE_X: f32 = 19.0;
const SPINE_Y: f32 = 11.0;
const SPINE_WIDTH: f32 = 33.0;
const SPINE_HEIGHT: f32 = 13.0;
/// The time in the gutter: 12 px medium, 4 px down, or level with the
/// first line of text in the compact display (`:3126`-`:3131`).
const TIME: f32 = 12.0;
const TIME_TOP: f32 = 4.0;
/// A line of message text is 1.3 times its size; a reply's or a command's
/// line is 22 px.
pub const LINE: f32 = 1.3;
pub const HEADER: f32 = 22.0;
/// The name's line: its parts 8 px apart, 2 px under it; the tag at 0.72 of
/// the text's size; the time 4 px smaller than the text (`:3201`-`:3252`).
const PARTS_GAP: f32 = 8.0;
const LINE_UNDER: f32 = 2.0;
const TAG_SCALE: f32 = 0.72;
const TIME_SMALLER: f32 = 4.0;
/// "APP": white on Discord's blurple, 16 px tall, 4 px round, 10 px bold
/// text 4 px in (`BotTag`).
const BOT: Color = Color::from_rgb8(0x58, 0x65, 0xf2);
const BOT_HEIGHT: f32 = 16.0;
const BOT_RADIUS: f32 = 4.0;
const BOT_TEXT: f32 = 10.0;
const BOT_PAD: f32 = 4.0;

/// Between the gutter and the text (`spacing`, `:3085`).
const GAP: f32 = 16.0;
const COMPACT_GAP: f32 = 8.0;

/// The room before the text: the gutter and the space after it, which the
/// reply's spine reaches into.
pub fn gutter_width(message: &Message) -> f32 {
    if message.compact {
        COMPACT_GUTTER + COMPACT_GAP
    } else {
        GUTTER + GAP
    }
}

fn ask(intent: Intent) -> chat::Intent {
    chat::Intent::from(intent)
}

/// The gutter of `message`: a notice's icon, the author's picture over the
/// first of a group (under the reply's spine), or its time while the
/// pointer is on it, and every message's in the compact display.
pub fn gutter<'a>(message: &'a Message, motion: Motion) -> Element<'a, chat::Intent> {
    let compact = message.compact;
    let room = gutter_width(message);
    let width = if compact { COMPACT_GUTTER } else { GUTTER };
    let header = message.header.shown();
    if let Some(kind) = message.notice {
        return notice::icon_of(kind, message.size.px(), compact, room);
    }
    if message.headed() && !compact {
        let mut parts: Column<Element<'a, chat::Intent>> = column![];
        parts = if header {
            parts.push(spine()).push(
                space()
                    .height(AVATAR_UNDER_HEADER - SPINE_Y - SPINE_HEIGHT)
                    .boxed(),
            )
        } else {
            parts.push(space().height(AVATAR_TOP).boxed())
        };
        return parts.push(picture(message, motion)).width(room).boxed();
    }
    if !compact && !(message.lit && !message.first) {
        return space().width(room).boxed();
    }
    let size = message.size.px();
    let top = if compact {
        let under = if header { HEADER } else { 0.0 };
        under + (size * LINE - TIME * LINE) / 2.0 + 1.0
    } else {
        TIME_TOP
    };
    let time = text(message.short_time.as_str())
        .size(TIME)
        .font(typography::MEDIUM)
        .wrapping(text::Wrapping::None)
        .style(catalog::text::muted);
    let time = container(time)
        .width(width)
        .align_x(Alignment::End)
        .padding(Padding::default().top(top));
    container(time).width(room).boxed()
}

/// The author's picture, with their decoration, playing while the message
/// is lit; opening their card or menu when the app can show them.
fn picture<'a>(message: &'a Message, motion: Motion) -> Element<'a, chat::Intent> {
    let author = &message.author;
    let playing = if message.lit {
        motion
    } else {
        Motion::default()
    };
    let face = portrait(&author.avatar, metrics::AVATAR_MESSAGE)
        .decoration(author.decoration.as_ref())
        .motion(playing)
        .view();
    if !author.known {
        return face.boxed();
    }
    let key = message.key;
    let pointing = mouse_area(face).interaction(iced::mouse::Interaction::Pointer);
    opener(pointing)
        .on_press(move |at| ask(Intent::Card(key, Who::Author, Spot::beside(at))))
        .on_right_press(move |at| ask(Intent::AuthorMenu(key, Spot::pointer(at))))
        .boxed()
}

/// The corner from the picture's top up to the quoted message: the top and
/// left of a rounded box (`:3090`-`:3093`), drawn as its path, the colour
/// the theme's.
fn spine<'a>() -> Element<'a, chat::Intent> {
    static CORNER: LazyLock<svg::Handle> = LazyLock::new(|| {
        svg::Handle::from_memory(
            &b"<svg xmlns='http://www.w3.org/2000/svg' width='33' height='13' viewBox='0 0 33 13'>\
<path d='M1 13V7a6 6 0 0 1 6-6h26' fill='none' stroke='#000' stroke-width='2'/></svg>"[..],
        )
    });
    let corner = svg(CORNER.clone())
        .width(SPINE_WIDTH)
        .height(SPINE_HEIGHT)
        .style(|theme: &Theme, _status| svg::Style {
            color: Some(theme.palette().reply_spine),
        });
    container(corner)
        .padding(Padding::default().left(SPINE_X).top(SPINE_Y))
        .boxed()
}

/// The first line of a group: the name in its role's colour and style, the
/// tag, "APP" and when (not in the compact display, whose gutter has it).
pub fn line<'a>(message: &'a Message, motion: Motion) -> Element<'a, chat::Intent> {
    let author = &message.author;
    let key = message.key;
    let size = message.size.px();
    let ink = author
        .color
        .map_or(Ink::Token(|palette| palette.text), Ink::Fixed);
    let name = selection::whole(
        styled_name(&author.name, author.style)
            .size(size)
            .weight(Weight::Medium)
            .ink(ink),
    );
    let name: Element<'a, chat::Intent> = if author.known {
        let color = author.color;
        let lined = underline(name, size, move |palette| color.unwrap_or(palette.text));
        opener(lined)
            .on_press(move |at| ask(Intent::Card(key, Who::Author, Spot::beside(at))))
            .on_right_press(move |at| ask(Intent::AuthorMenu(key, Spot::pointer(at))))
            .boxed()
    } else {
        name.boxed()
    };
    let mut parts = row![name].spacing(PARTS_GAP).align_y(Alignment::Center);
    if let Some(tag) = &author.tag {
        let chip = server_tag(tag, motion).size(size * TAG_SCALE);
        parts = parts.push(chip.on_open(move |at| ask(Intent::TagCard(key, Spot::below(at)))));
    }
    if author.bot {
        parts = parts.push(bot_tag());
    }
    if !message.compact {
        let time = text(message.time.as_str())
            .size(size - TIME_SMALLER)
            .wrapping(text::Wrapping::None)
            .style(catalog::text::muted);
        parts = parts.push(selection::whole(time).boxed());
    }
    let under = if message.compact { 0.0 } else { LINE_UNDER };
    container(parts)
        .padding(Padding::default().bottom(under))
        .boxed()
}

/// Discord's tag after a bot's or an app's name.
pub fn bot_tag<'a>() -> Element<'a, chat::Intent> {
    let label = text(tr!("APP"))
        .size(BOT_TEXT)
        .font(typography::BOLD)
        .color(Color::WHITE);
    container(label)
        .padding([0.0, BOT_PAD])
        .height(BOT_HEIGHT)
        .align_y(Alignment::Center)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(BOT)),
            border: Border::default().rounded(BOT_RADIUS),
            ..container::Style::default()
        })
        .boxed()
}
