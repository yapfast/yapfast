//! Dragging in the channel list, through the virtual list's `reorder`
//! (F07): a channel or a category to move it among the server's
//! (`start-arranging`, `arrange`, `stop-arranging`), or someone from under
//! a voice channel onto another (`start-moving`, `move-person`,
//! `stop-moving`), as `RoomList` has them (`ui/rail.slint:1200`-`:1300`).
//! Which may be picked up the rows say; where they may drop, the app, once
//! told (`Channels::arranging`, `Channels::moving`). The faint copy that
//! follows the pointer is the channel's icon and name, the category's
//! chevron and name, or the person's avatar and name, on the ground Slint
//! gives each (`:1611`-`:1651`).

use iced::widget::{container, row, svg, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::sidebar::channels::{Arrange, Channels, Intent, Move};
use anomi_widgets::avatar::portrait;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::list::{Drag, Ground};

use super::items::{self, Item, place, room_of};

/// A category's copy: 36 px tall, 4 px corners (`:1629`, `:1630`); a
/// person's 40 px in, 30 tall (`:1614`, `:1615`).
const CATEGORY: f32 = 36.0;
const PERSON_IN: f32 = 40.0;
const PERSON_RADIUS: f32 = 4.0;
/// The copies' text: a category's 12 px bold, a channel's 16 px, a
/// person's 15 px (`:1639`, `:1647`, `:1621`).
const CATEGORY_TEXT: f32 = typography::size::CAPTION;
const CHANNEL_TEXT: f32 = typography::size::TITLE;
const PERSON_TEXT: f32 = 15.0;
const CHEVRON: f32 = 12.0;
const AVATAR: f32 = 22.0;
/// A category's chevron and name, 2 px in and 2 apart (`:1633`); a
/// person's avatar 4 px in (`:1618`).
const CATEGORY_IN: f32 = 2.0;
const CATEGORY_GAP: f32 = 2.0;
const PERSON_LEFT: f32 = 4.0;

/// Whether the item may be picked up: a heading or a channel with a place
/// this account may drag, or someone it may move, while nothing else is.
pub fn may_drag(channels: &Channels, item: Item) -> bool {
    let rows = &channels.rows;
    match item {
        Item::Heading(r) => {
            let row = &rows[r];
            row.heading_place > 0 && row.heading_draggable && channels.moving.is_none()
        }
        Item::Channel(r) => {
            let row = &rows[r];
            !row.thread() && row.place > 0 && row.draggable && channels.moving.is_none()
        }
        Item::Person(r, p) => {
            let person = &rows[r].people[p];
            person.movable && !person.user.is_empty() && channels.arranging.is_none()
        }
        _ => false,
    }
}

/// Whether what's dragged from `from` may drop on `to`, as the app said.
pub fn may_drop(channels: &Channels, from: Item, to: Item) -> bool {
    let rows = &channels.rows;
    match from {
        Item::Person(..) => channels
            .moving
            .as_ref()
            .is_some_and(|moving| room_of(rows, to).is_some_and(|room| moving.drops_on(room))),
        _ => channels.arranging.as_ref().is_some_and(|arranging| {
            place(rows, to).is_some_and(|place| arranging.drops_on(place))
        }),
    }
}

/// What a drag tells, as the intent the app hears.
pub fn intent(channels: &Channels, items: &[Item], drag: Drag) -> Intent {
    let rows = &channels.rows;
    // The channels don't make room: an aim is the row the pointer is over.
    let drag = match drag {
        Drag::Aimed { from, to } => Drag::Over {
            from,
            to: to.map(|(at, _)| at),
        },
        drag => drag,
    };
    let from = drag.from();
    let to = |to: usize| items[to];
    match items[from] {
        Item::Person(r, p) => Intent::Move(match drag {
            Drag::Started { .. } => Move::Start {
                channel: rows[r].id.clone(),
                user: rows[r].people[p].user.clone(),
            },
            Drag::Over { to: over, .. } => {
                Move::Over(over.and_then(|at| room_of(rows, to(at))).map(str::to_owned))
            }
            Drag::Dropped { to: at, .. } => {
                room_of(rows, to(at)).map_or(Move::Stop, |room| Move::Drop(room.to_owned()))
            }
            Drag::Cancelled { .. } | Drag::Aimed { .. } => Move::Stop,
        }),
        dragged => Intent::Arrange(match drag {
            Drag::Started { .. } => place(rows, dragged).map_or(Arrange::Stop, Arrange::Start),
            Drag::Over { to: over, .. } => Arrange::Over(over.and_then(|at| place(rows, to(at)))),
            Drag::Dropped { to: at, .. } => {
                place(rows, to(at)).map_or(Arrange::Stop, Arrange::Drop)
            }
            Drag::Cancelled { .. } | Drag::Aimed { .. } => Arrange::Stop,
        }),
    }
}

/// The ground under the copy of what's dragged.
pub fn ground(item: Item) -> Ground {
    match item {
        Item::Heading(_) => Ground {
            radius: PERSON_RADIUS,
            height: Some(CATEGORY),
            ..Ground::default()
        },
        Item::Person(..) => Ground {
            left: PERSON_IN,
            right: metrics::ROW_INSET,
            radius: PERSON_RADIUS,
            height: Some(items::PERSON),
            ..Ground::default()
        },
        _ => Ground {
            height: Some(metrics::CHANNEL_ROW_HEIGHT),
            ..Ground::default()
        },
    }
}

/// The copy of what's dragged, laid out in its ground.
pub fn copy(channels: &Channels, item: Item) -> Element<'_, Intent> {
    let rows = &channels.rows;
    let secondary = |theme: &Theme, _| svg::Style {
        color: Some(theme.palette().secondary),
    };
    let content = match item {
        Item::Heading(r) => {
            let glyph = if rows[r].collapsed {
                Icon::ChevronRight
            } else {
                Icon::ChevronDown
            };
            row![
                icon(glyph, CHEVRON).style(secondary),
                text(rows[r].heading.as_str())
                    .size(CATEGORY_TEXT)
                    .font(typography::BOLD)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.palette().secondary),
                        ..text::Style::default()
                    })
                    .wrapping(text::Wrapping::None)
                    .ellipsis(text::Ellipsis::End)
                    .width(Fill),
            ]
            .spacing(CATEGORY_GAP)
            .padding(Padding::ZERO.left(CATEGORY_IN))
        }
        Item::Person(r, p) => {
            let person = &rows[r].people[p];
            row![
                portrait(&person.avatar, AVATAR).view(),
                name(&person.name, PERSON_TEXT),
            ]
            .spacing(metrics::SPACE_S)
            .padding(Padding::ZERO.left(PERSON_LEFT).right(metrics::SPACE_S))
        }
        Item::Channel(r) => row![
            icon(rows[r].glyph(), metrics::ICON_ROW).style(secondary),
            name(&rows[r].name, CHANNEL_TEXT),
        ]
        .spacing(metrics::SPACE_S)
        .padding([0.0, metrics::SPACE_S]),
        _ => row![],
    };
    container(content.align_y(Alignment::Center))
        .center_y(Fill)
        .width(Fill)
        .boxed()
}

/// A dragged copy's name, in the text colour.
fn name(name: &str, size: f32) -> impl View<Intent> + '_ {
    text(name)
        .size(size)
        .font(typography::MEDIUM)
        .style(|theme: &Theme| text::Style {
            color: Some(theme.palette().text),
            ..text::Style::default()
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Fill)
}

#[cfg(test)]
mod tests {
    use anomi_vm::fixtures::sidebar::channels as samples;

    use super::*;

    #[test]
    fn what_may_be_picked_up() {
        let list = samples::channels();
        assert!(may_drag(&list, Item::Heading(0)));
        assert!(may_drag(&list, Item::Channel(3)), "the Lounge");
        assert!(!may_drag(&list, Item::Channel(1)), "a thread");
        assert!(may_drag(&list, Item::Person(3, 1)), "Carol");
        let arranging = samples::arranging();
        assert!(
            !may_drag(&arranging, Item::Person(3, 1)),
            "not while arranging"
        );
        let moving = samples::moving();
        assert!(!may_drag(&moving, Item::Channel(3)), "not while moving");
    }

    #[test]
    fn it_drops_where_the_app_said() {
        let list = samples::arranging();
        let lounge = Item::Channel(3);
        assert!(may_drop(&list, lounge, Item::Channel(4)), "Create Room");
        assert!(may_drop(&list, lounge, Item::Heading(0)), "the category");
        assert!(!may_drop(&list, lounge, Item::Channel(2)), "ideas");
        assert!(
            !may_drop(&samples::channels(), lounge, Item::Channel(4)),
            "unasked"
        );
        let moving = samples::moving();
        let carol = Item::Person(3, 1);
        assert!(may_drop(&moving, carol, Item::Channel(4)));
        assert!(
            !may_drop(&moving, carol, Item::Person(3, 0)),
            "her own channel"
        );
    }

    #[test]
    fn a_drag_tells_the_app_by_places_and_rooms() {
        let list = samples::channels();
        let all = items::items(&list);
        let at = |item| all.iter().position(|known| *known == item).unwrap();
        let (lounge, room) = (at(Item::Channel(3)), at(Item::Channel(4)));
        let started = intent(&list, &all, Drag::Started { from: lounge });
        assert_eq!(started, Intent::Arrange(Arrange::Start(4)));
        let over = Drag::Over {
            from: lounge,
            to: Some(at(Item::Person(3, 2))),
        };
        assert_eq!(
            intent(&list, &all, over),
            Intent::Arrange(Arrange::Over(Some(4)))
        );
        let carol = at(Item::Person(3, 1));
        let dropped = intent(
            &list,
            &all,
            Drag::Dropped {
                from: carol,
                to: room,
            },
        );
        assert_eq!(
            dropped,
            Intent::Move(Move::Drop(samples::CREATE_ROOM.into()))
        );
        let back = intent(&list, &all, Drag::Cancelled { from: carol });
        assert_eq!(back, Intent::Move(Move::Stop));
    }
}
