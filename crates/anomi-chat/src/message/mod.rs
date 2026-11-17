//! A message, as the timeline and a thread beside it draw it. The Slint
//! side: `MessageContent` and `MessageBody` (`ui/messages.slint:3015`,
//! `:2948`): the gutter with the author's picture, a notice's icon or the
//! time; the line over the message (a reply or a command); the author's
//! line; a forward's mark; the body, folding to twelve lines while long;
//! and under it the parts other tasks draw (attachments, embeds, a bot's
//! components, a poll, reactions) among its own lines (a thread, its
//! delivery, an answer only this account sees). It lies on its ground
//! (lit, or marigold where it calls on this account) with its hover bar
//! over it (`ui/yapfast.slint:3804`-`:4039`); a right click opens its menu.
//! See `anomi_vm::chat::message` for what each part shows.

mod author;
mod hover_bar;
mod notice;
mod reply;

use iced::widget::{Column, button, column, row, text};
use iced::{Color, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, typography};
use anomi_vm::chat::message::{Edge, Fold, Intent, Message};
use anomi_vm::chat::{self};
use anomi_vm::menu::Spot;
use anomi_widgets::menu::opener;
use anomi_widgets::paragraph::{self, paragraph};
use yapfast_lang::tr;

use crate::{attachments, components, embeds, inline_edit, poll, reactions};

/// Before the gutter, and after the text for the hover bar's room; 2 px
/// over and under (`start-padding`, `end-padding`, `:3033`, `:3085`).
const START: f32 = 16.0;
pub const END: f32 = 48.0;
/// The row's end beside a channel, in the thread's panel.
pub const ASIDE_END: f32 = 16.0;
const UNDER: f32 = 2.0;
/// The space over a row that starts a group, and over any other, which the
/// ground reaches up over; the pixel under every row
/// (`ClientStyle.message-group-gap`, `ui/yapfast.slint:3855`-`:3857`).
const GROUP_GAP: f32 = 16.0;
const COMPACT_GROUP_GAP: f32 = 8.0;
const ROW_GAP: f32 = 1.0;
/// A compact first line's name and the text beside it (`spacing`, `:3201`).
const INLINE_GAP: f32 = 8.0;
/// Show more and Show less: 7/8 of the text, semibold (`:2998`-`:3001`).
const TOGGLE_SCALE: f32 = 0.875;

fn ask(intent: Intent) -> chat::Intent {
    chat::Intent::from(intent)
}

/// A message, with the parts other tasks draw. The calls to them are F01's
/// wiring: keep them as the view grows.
pub fn view(message: &Message, motion: Motion) -> impl View<chat::Intent> + '_ {
    ending(message, END, motion)
}

/// [`view`] ending `end` from the row's right edge rather than 48 px: 16 in
/// the thread beside a channel (`MessageContent { aside: true; end-padding:
/// 16px }`, `ui/thread.slint:240`).
pub fn ending(message: &Message, end: f32, motion: Motion) -> impl View<chat::Intent> + '_ {
    let key = message.key;
    let size = message.size.px();
    let above = above(message);
    // The compact display sets a group's first text beside the name; one
    // with blocks or a forward keeps its own lines.
    let inline = message.compact
        && message.headed()
        && !message.body.is_empty()
        && message.body.blocks().len() <= 1
        && !message.forwarded
        && !message.editing.open();
    let mut parts: Column<Element<'_, chat::Intent>> = column![].width(Fill);
    if let Some(bar) = message.bar.as_ref().filter(|_| message.acts()) {
        let before = START + author::gutter_width(message);
        parts = parts.push(hover_bar::view(message, bar, above, [before, end], motion));
    }
    if let Some(header) = reply::view(message) {
        parts = parts.push(menu(message, header));
    }
    // The author's name has their menu: a right click on it opens no other.
    if message.headed() {
        let line = author::line(message, motion);
        parts = parts.push(if inline {
            row![line, menu(message, body(message, motion))]
                .spacing(INLINE_GAP)
                .boxed()
        } else {
            line
        });
    }
    let mut rest: Column<Element<'_, chat::Intent>> = column![].width(Fill);
    if message.forwarded {
        rest = rest.push(notice::forwarded());
    }
    rest = rest.push(
        inline_edit::view(&message.editing, motion)
            .map(chat::Intent::from)
            .boxed(),
    );
    // The editor stands in the body's place, and its "(edited)" hides.
    if !message.body.is_empty() && !inline && !message.editing.open() {
        rest = rest.push(body(message, motion));
    }
    if let Some(thinking) = message.thinking {
        rest = rest.push(notice::thinking(message, thinking));
    }
    if message.edited && !message.deleted && !message.editing.open() {
        rest = rest.push(notice::edited());
    }
    rest = rest.push(
        attachments::view(&message.attachments, motion)
            .map(chat::Intent::from)
            .boxed(),
    );
    if let Some(note) = &message.hidden_card {
        rest = rest.push(notice::hidden(note, size));
    }
    rest = rest
        .push(
            poll::view(&message.poll, motion)
                .map(chat::Intent::from)
                .boxed(),
        )
        .push(
            embeds::view(&message.embeds, motion)
                .map(chat::Intent::from)
                .boxed(),
        )
        .push(
            components::view(&message.components, motion)
                .map(chat::Intent::from)
                .boxed(),
        );
    if let Some(why) = &message.failed {
        rest = rest.push(notice::failed(why));
    }
    if let Some(thread) = &message.thread {
        rest = rest.push(notice::thread(key, thread));
    }
    rest = rest.push(
        reactions::view(&message.reactions, motion)
            .map(chat::Intent::from)
            .boxed(),
    );
    if let Some(delivery) = message.delivery.as_ref().filter(|delivery| !delivery.quiet) {
        rest = rest.push(notice::delivery(key, delivery, motion));
    }
    if message.ephemeral {
        rest = rest.push(notice::ephemeral(key, message.preview));
    }
    parts = parts.push(menu(message, rest.boxed()));
    // The author's picture has their menu too.
    let gutter = author::gutter(message, motion);
    let pictured = message.headed() && !message.compact && message.author.known;
    let gutter = if pictured {
        gutter
    } else {
        menu(message, gutter)
    };
    let content = row![gutter, parts].padding(Padding {
        top: UNDER,
        right: if message.editing.open() {
            inline_edit::END
        } else {
            end
        },
        bottom: UNDER,
        left: START,
    });
    let ground = hover_bar::Ground {
        lit: message.lit,
        highlighted: message.highlighted,
        above,
        below: ROW_GAP,
    };
    let enter = (!message.lit).then(|| ask(Intent::Point(key)));
    hover_bar::frame(content, ground, enter, motion)
}

/// `content` opening the message's menu at the pointer on a right click,
/// while it takes actions.
fn menu<'a>(message: &Message, content: Element<'a, chat::Intent>) -> Element<'a, chat::Intent> {
    if !message.acts() {
        return content;
    }
    let key = message.key;
    opener(content)
        .on_right_press(move |at| ask(Intent::Menu(key, Spot::pointer(at))))
        .boxed()
}

/// The space between the message and its row's top, which its ground
/// reaches up over: none under a divider, where the ground starts at the
/// message.
fn above(message: &Message) -> f32 {
    match message.edge {
        Edge::Divider => 0.0,
        Edge::Message | Edge::Top if !message.first => ROW_GAP,
        Edge::Message | Edge::Top if message.compact => COMPACT_GROUP_GAP,
        Edge::Message | Edge::Top => GROUP_GAP,
    }
}

/// The body: dimmed while deleted or on its way, a notice's dimmer; folded
/// to twelve lines while long, with Show more under it.
fn body(message: &Message, motion: Motion) -> Element<'_, chat::Intent> {
    let key = message.key;
    let size = message.size.px();
    let colour: fn(&Palette) -> Color = if message.dimmed() {
        |palette| palette.muted
    } else if message.notice.is_some() {
        |palette| palette.secondary
    } else {
        |palette| palette.text
    };
    let text = paragraph(&message.body)
        .size(size)
        .color(colour)
        .motion(motion)
        .on_link(move |link| ask(Intent::Link(key, link)));
    match message.fold {
        Fold::Never => text.boxed(),
        Fold::Folded | Fold::Open => {
            let open = message.fold == Fold::Open;
            let fold = paragraph::Fold {
                open,
                ..paragraph::Fold::MESSAGE
            };
            text.fold(fold, toggle(key, open, size)).boxed()
        }
    }
}

/// Show more, or Show less once open: the link's colour, the text's under
/// the pointer.
fn toggle<'a>(key: u64, open: bool, size: f32) -> Element<'a, chat::Intent> {
    let label = if open {
        tr!("Show less")
    } else {
        tr!("Show more")
    };
    button(
        text(label)
            .size(size * TOGGLE_SCALE)
            .font(typography::SEMIBOLD),
    )
    .padding(0)
    .on_press(ask(Intent::Fold(key, !open)))
    .style(|theme: &Theme, status| {
        let palette = theme.palette();
        button::Style {
            text_color: if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                palette.text
            } else {
                palette.link
            },
            ..button::Style::default()
        }
    })
    .boxed()
}

/// Its shots for `anomi-shots`: the golden screenshots it matches, one of
/// every kind of message, the compact display, and the hover bar with
/// Shift held and asking before a deletion; each the whole window from a
/// fixture.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::Vm;
    use anomi_vm::chat::timeline::Timeline;
    use anomi_vm::fixtures::chat::message as fixtures;
    use anomi_vm::fixtures::shell;
    use anomi_vm::sidebar::Place;

    use crate::shots::{GOLDEN, Shot};

    /// `vm` with `timeline` open and its place chosen in the sidebar.
    fn open(mut vm: Vm, timeline: Timeline) -> Vm {
        let room = timeline.room.as_ref().map_or("", |room| room.id.as_str());
        match &mut vm.sidebar.place {
            Place::Dms(dms) => {
                for row in &mut dms.rows {
                    row.selected = row.id == room;
                }
            }
            Place::Server(server) => {
                for row in &mut server.channels.rows {
                    row.selected = row.id == room;
                }
            }
        }
        vm.chat.timeline = timeline;
        vm
    }
    fn channel(timeline: fn() -> Timeline) -> Vm {
        open(shell::server_home(), timeline())
    }
    fn dm(timeline: fn() -> Timeline) -> Vm {
        open(shell::dm_home(), timeline())
    }
    let shot = |area, name, vm| Shot {
        area,
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("chan", "history_plain_a", || channel(fixtures::history_lit)),
        shot("chan", "gallery_hoverbar", || channel(fixtures::gallery)),
        shot("chan", "spoiler_reply_blocked_linkembed", || {
            channel(fixtures::blocked_showcase)
        }),
        shot("dmcall", "connected_bob_dm", || dm(fixtures::call_in_dm)),
        shot("message", "kinds", || channel(fixtures::kinds)),
        shot("message", "kinds_compact", || {
            channel(fixtures::kinds_compact)
        }),
        shot("message", "shift_held", || channel(fixtures::shift_held)),
        shot("message", "confirm_delete", || {
            channel(fixtures::confirming)
        }),
    ]
}
