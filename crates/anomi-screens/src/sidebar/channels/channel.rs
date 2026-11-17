//! A channel or a thread in the list: `ChannelRow` as `RoomList` lays a
//! channel out (`ui/rail.slint:1399`-`:1482`). Its icon (none for a thread,
//! which its line from the channel marks), its name, how long a busy voice
//! channel's call has gone on, its mentions, and under the pointer the
//! voice channel's chat button and the card of who is in it. A thread sits
//! 28 px further in, past its line (`:1401`).

use iced::widget::{Row, canvas, column, container, row, space, text};
use iced::{Alignment, Fill, Padding, Point, Rectangle, mouse};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, Renderer, catalog, metrics, typography};
use anomi_vm::menu::Spot;
use anomi_vm::sidebar::channels::{ChannelRow, Channels, Intent};
use anomi_widgets::badge::badge;
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::Icon;
use yapfast_lang::tr;

use super::card;
use super::items::Room;
use super::pointed::{Look, pointed};

/// A muted channel at rest is half as strong (`:596`); a row a drag can't
/// drop among, 0.3 (`:1452`).
pub const MUTED: f32 = 0.5;
pub const DIMMED: f32 = 0.3;
/// How far in a thread sits, past its line (`:1402`), and its line's own
/// stroke (`:1407`).
const THREAD: f32 = 28.0;
const LINE: f32 = 1.5;
/// The chat button: 24 px, its icon 16, with 6 px corners (`:705`).
const CHAT: f32 = 24.0;
const CHAT_GLYPH: f32 = 16.0;
/// The call's length beside its name, 12 px (`:690`).
const LASTED: f32 = typography::size::CAPTION;

pub fn view<'a>(
    channels: &'a Channels,
    r: usize,
    room: Room,
    motion: Motion,
) -> Element<'a, Intent> {
    let row_vm = &channels.rows[r];
    let thread = row_vm.thread();
    let look = Look {
        selected: row_vm.selected,
        unread: row_vm.unread,
        locked: row_vm.locked,
        rest: if row_vm.muted { MUTED } else { 1.0 },
        dim: dim(channels, row_vm),
        mark: Some(if thread {
            metrics::ROW_INSET + THREAD
        } else {
            metrics::ROW_INSET
        }),
        aimed: aimed(channels, row_vm),
        ..Look::default()
    };
    let id = row_vm.id.as_str();
    let mut row = pointed(body(row_vm), metrics::CHANNEL_ROW_HEIGHT, look, motion)
        .on_press(Intent::Open(id.to_owned()))
        .on_menu(move |at| Intent::Menu(id.to_owned(), Spot::pointer(at)));
    if !thread {
        row = row.leading(row_vm.glyph(), glyph_tint);
    }
    if row_vm.chat {
        row = row.trailing(
            icon_button(Icon::MessageSquareText, tr!("Open chat"), motion)
                .size(CHAT)
                .glyph(CHAT_GLYPH)
                .radius(metrics::RADIUS_ROW)
                .on_press(Intent::OpenChat(id.to_owned()))
                .view(),
        );
    }
    if row_vm.voice() && !row_vm.people.is_empty() {
        row = row.card(card::view(row_vm));
    }
    let height = room.above + metrics::CHANNEL_ROW_HEIGHT + room.under;
    let placed = column![space().height(room.above), row, space().height(room.under)];
    if thread {
        let line = canvas(ThreadLine {
            continues: row_vm.thread_continues,
        })
        .width(THREAD)
        .height(height);
        row![
            space().width(metrics::ROW_INSET),
            line,
            placed.width(Fill),
            space().width(metrics::ROW_INSET)
        ]
        .height(height)
        .boxed()
    } else {
        container(placed)
            .padding(
                Padding::ZERO
                    .left(metrics::ROW_INSET)
                    .right(metrics::ROW_INSET),
            )
            .height(height)
            .boxed()
    }
}

/// What the row shows after its icon: its name, the call's length and
/// mentions. A thread has no icon, and starts 8 px in.
fn body(row_vm: &ChannelRow) -> Element<'_, Intent> {
    let mut content: Row<Element<'_, Intent>> = Row::new()
        .spacing(metrics::SPACE_S)
        .align_y(Alignment::Center);
    let name = text(row_vm.name.as_str())
        .size(typography::size::BODY)
        .font(typography::MEDIUM)
        .style(catalog::text::inherit)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Fill);
    content = content.push(name.boxed());
    if !row_vm.lasted.is_empty() {
        let lasted = text(row_vm.lasted.as_str())
            .size(LASTED)
            .style(catalog::text::muted)
            .wrapping(text::Wrapping::None);
        content = content.push(lasted.boxed());
    }
    if !row_vm.badge.is_empty() {
        content = content.push(badge(&row_vm.badge).boxed());
    }
    let left = if row_vm.thread() {
        metrics::SPACE_S
    } else {
        0.0
    };
    container(content)
        .padding(Padding::ZERO.left(left).right(metrics::SPACE_S))
        .center_y(metrics::CHANNEL_ROW_HEIGHT)
        .width(Fill)
        .boxed()
}

/// The icon's colour: the text colour chosen, secondary unread, faint else
/// (`:612`, Discord's look).
pub fn glyph_tint(palette: &Palette, look: &Look) -> iced::Color {
    if look.selected {
        palette.text
    } else if look.unread {
        palette.secondary
    } else {
        palette.faint
    }
}

/// How strong the row is while something drags: a channel or category
/// dragged dims the rows it can't drop among (`dimmed`, `:1291`), someone
/// dragged the channels they can't move to (`unfit`, `:1299`).
pub fn dim(channels: &Channels, row_vm: &ChannelRow) -> f32 {
    let arranged = channels.arranging.as_ref().is_some_and(|arranging| {
        let place = arranging.place;
        let dragged = channels
            .rows
            .iter()
            .find(|row| row.heading_place == place || row.place == place);
        match dragged {
            Some(dragged) if dragged.heading_place == place => true,
            Some(dragged) => dragged.voice() != row_vm.voice(),
            None => false,
        }
    });
    let unfit = channels
        .moving
        .as_ref()
        .is_some_and(|moving| !moving.drops_on(&row_vm.id));
    if arranged || unfit { DIMMED } else { 1.0 }
}

/// Someone dragged would drop here (`move-aimed`, `:1296`).
fn aimed(channels: &Channels, row_vm: &ChannelRow) -> bool {
    !row_vm.thread()
        && channels.moving.as_ref().is_some_and(|moving| {
            moving.over.as_deref() == Some(row_vm.id.as_str()) && moving.drops_on(&row_vm.id)
        })
}

/// A thread's line from its channel's icon: down, and round into the
/// thread, running on to the next of the channel's threads (`:1403`-`:1408`).
/// Its box is the item's, whose top is 2 px above the thread's row, where
/// Slint's path starts.
struct ThreadLine {
    continues: bool,
}

impl canvas::Program<Intent, Theme, Renderer> for ThreadLine {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry<Renderer>> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        // `M 18 0 L 18 13 Q 18 19 24 19 L 28 19`, and on down when it
        // continues.
        let path = canvas::Path::new(|path| {
            path.move_to(Point::new(18.0, 0.0));
            if self.continues {
                path.line_to(Point::new(18.0, bounds.height));
                path.move_to(Point::new(18.0, 13.0));
            } else {
                path.line_to(Point::new(18.0, 13.0));
            }
            path.quadratic_curve_to(Point::new(18.0, 19.0), Point::new(24.0, 19.0));
            path.line_to(Point::new(28.0, 19.0));
        });
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_color(theme.palette().faint)
                .with_width(LINE),
        );
        vec![frame.into_geometry()]
    }
}
