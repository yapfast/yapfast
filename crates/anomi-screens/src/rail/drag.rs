//! Dragging servers and folders on the rail, as Discord's server list
//! lets one, shown as icons make room on a phone's home screen rather than
//! by Discord's line (`discord-rail-drag-*.png`): the list makes room
//! ([`List::live`](anomi_widgets::list::List::live)), the rows are the
//! order it would leave ([`anomi_vm::rail::arrange::preview`]) with a gap
//! where it would land, its copy is lifted off the rail at the pointer, and
//! a server held over the middle of a server alone or a closed folder
//! shows the folder it would make or go into.
//!
//! The unread DMs, the line under them, the home tile and the "+" don't
//! move; a row of the DMs aims before the first server.

use std::collections::HashMap;

use iced::widget::{container, space};
use iced::{Background, Border, Length, alignment};

use anomi_theme::metrics;
use anomi_theme::prelude::*;
use anomi_vm::rail::{Drag, Folder, Intent, Place, Rail, Server, arrange};
use anomi_widgets::list::{self, Fill, Ground, Zone};
use anomi_widgets::rail_item::rail_item;

use super::face;
use super::rows::{Moved, Row, folder_rows};

/// The copy is this much larger than the tile, lifted off the rail; over a
/// server or folder it would go into, this much smaller, so the folder
/// forming under it shows.
const LIFT: f32 = 1.12;
const INTO: f32 = 0.6;
/// The gap's slot: the accent's soft ground, outlined.
const SLOT_EDGE: f32 = 2.0;
const SLOT_EDGE_ALPHA: f32 = 0.6;

/// The rows while `drag` goes on: the order it would leave, with a gap for
/// it, and what it's aimed onto marked.
pub fn previewed<'a>(rail: &'a Rail, drag: &'a Drag, rows: &mut Vec<Row<'a>>) {
    let servers: HashMap<&str, &Server> = rail
        .servers
        .iter()
        .map(|server| (server.id.as_str(), server))
        .collect();
    let folders: HashMap<&str, &Folder> = rail
        .folders
        .iter()
        .map(|folder| (folder.id.as_str(), folder))
        .collect();
    let onto = drag.onto.as_deref();
    let row = |server: &'a Server, inside| {
        if server.id == drag.id {
            Row::Gap {
                moved: Moved::Server(server),
                inside,
            }
        } else {
            Row::Server {
                server,
                inside,
                aimed: inside.is_none() && onto == Some(server.id.as_str()),
            }
        }
    };
    for group in arrange::preview(rail) {
        let inside: Vec<&Server> = group
            .servers
            .iter()
            .filter_map(|id| servers.get(id.as_str()).copied())
            .collect();
        match group.folder.as_deref().and_then(|id| folders.get(id)) {
            Some(folder) if folder.id == drag.id => rows.push(Row::Gap {
                moved: Moved::Folder(folder, inside),
                inside: None,
            }),
            Some(folder) => {
                let aimed = onto == Some(folder.id.as_str());
                folder_rows(folder, inside, aimed, rows, row);
            }
            None => rows.extend(inside.into_iter().map(|server| row(server, None))),
        }
    }
}

/// Whether row `index` may be picked up: a Discord server or a folder.
pub fn may_drag(rows: &[Row<'_>], index: usize) -> bool {
    match &rows[index] {
        Row::Server { server, .. } => server.movable,
        Row::Folder { .. } | Row::Gap { .. } => true,
        Row::Dm(_) | Row::Divider => false,
    }
}

/// Whether the row at `to` takes the row from `from` onto itself: a server
/// onto a server alone (a folder of the two) or onto a closed folder.
pub fn onto(rows: &[Row<'_>], from: usize, to: usize) -> bool {
    let server = matches!(
        rows[from],
        Row::Server { .. }
            | Row::Gap {
                moved: Moved::Server(_),
                ..
            }
    );
    server
        && match &rows[to] {
            Row::Server {
                server,
                inside: None,
                ..
            } => server.movable,
            Row::Folder { folder, .. } => !folder.open,
            _ => false,
        }
}

/// The id of the server or folder of a row, if it has one that moves.
fn id<'a>(row: &Row<'a>) -> Option<&'a str> {
    match row {
        Row::Server { server, .. }
        | Row::Gap {
            moved: Moved::Server(server),
            ..
        } => Some(&server.id),
        Row::Folder { folder, .. }
        | Row::Gap {
            moved: Moved::Folder(folder, _),
            ..
        } => Some(&folder.id),
        Row::Dm(_) | Row::Divider => None,
    }
}

/// The first server or folder at or after `index` that isn't the gap.
fn next_entry<'a>(rows: &[Row<'a>], index: usize) -> Option<&'a str> {
    rows[index..]
        .iter()
        .filter(|row| !matches!(row, Row::Gap { .. }))
        .find_map(id)
}

/// Where aiming at the row at `to` in `zone` puts what's dragged, from
/// `from`; `None` leaves the gap where it is.
fn place(rows: &[Row<'_>], from: usize, to: usize, zone: Zone) -> Option<Option<Place>> {
    if zone == Zone::Past {
        // After the rail's last server or folder, out of an open folder.
        let last = rows.iter().rev().find(|row| {
            matches!(
                row,
                Row::Folder { .. }
                    | Row::Server { inside: None, .. }
                    | Row::Gap { inside: None, .. }
            )
        });
        return match last {
            Some(Row::Gap { .. }) | None => None,
            Some(row) => Some(Some(Place::After(id(row)?.to_owned()))),
        };
    }
    let folder_dragged = matches!(
        rows[from],
        Row::Folder { .. }
            | Row::Gap {
                moved: Moved::Folder(..),
                ..
            }
    );
    let at = |id: &str| match zone {
        Zone::Above => Place::Before(id.to_owned()),
        Zone::Below | Zone::Past => Place::After(id.to_owned()),
        Zone::Onto => Place::Onto(id.to_owned()),
    };
    Some(Some(match &rows[to] {
        // A folder passes an open folder whole: at its tile going up, at
        // the row after it going down.
        Row::Server {
            inside: Some(_), ..
        } if folder_dragged => return None,
        Row::Folder { folder, .. } if folder.open && zone != Zone::Above => {
            if folder_dragged {
                return None;
            }
            // Below an open folder's tile is its first place.
            match rows[to + 1..]
                .iter()
                .find(|row| !matches!(row, Row::Gap { .. }))
            {
                Some(Row::Server {
                    server,
                    inside: Some(_),
                    ..
                }) => Place::Before(server.id.clone()),
                _ => Place::Onto(folder.id.clone()),
            }
        }
        Row::Server {
            server,
            inside: Some(_),
            ..
        } if zone == Zone::Onto => Place::After(server.id.clone()),
        Row::Dm(_) | Row::Divider => Place::Before(next_entry(rows, to)?.to_owned()),
        row => at(id(row)?),
    }))
}

/// What a drag of the rail's rows tells, as the rail's intent; `now` is
/// the drag going on.
pub fn intent(rows: &[Row<'_>], now: Option<&Drag>, drag: list::Drag) -> Intent {
    match drag {
        list::Drag::Started { from } => match id(&rows[from]) {
            Some(id) => Intent::Lift(id.to_owned()),
            None => Intent::PutBack,
        },
        list::Drag::Aimed { from, to } => Intent::Aim(match to {
            None => None,
            Some((to, zone)) => match place(rows, from, to, zone) {
                Some(place) => place,
                None => now.and_then(|drag| drag.target()),
            },
        }),
        list::Drag::Dropped { .. } => Intent::Drop,
        list::Drag::Over { .. } | list::Drag::Cancelled { .. } => Intent::PutBack,
    }
}

/// The copy lies on no ground of the list's: its tile is lifted itself.
pub fn ground(_index: usize) -> Ground {
    Ground {
        fill: Fill::Clear,
        left: 0.0,
        right: 0.0,
        radius: 0.0,
        height: None,
        alpha: 1.0,
    }
}

/// The copy that follows the pointer: the dragged server's or folder's
/// tile, lifted, a folder closed; smaller while `drag` would go onto
/// another.
pub fn copy<'a>(row: &Row<'a>, drag: Option<&Drag>, motion: Motion) -> Element<'a, Intent> {
    let scale = if drag.is_some_and(|drag| drag.onto.is_some()) {
        INTO
    } else {
        LIFT
    };
    let item = match row {
        Row::Server { server, .. }
        | Row::Gap {
            moved: Moved::Server(server),
            ..
        } => rail_item(face(&server.avatar), motion)
            .badge(&server.tile.badge)
            .discord(server.discord),
        Row::Folder {
            folder, servers, ..
        }
        | Row::Gap {
            moved: Moved::Folder(folder, servers),
            ..
        } => rail_item(super::folder_face(folder, servers, false), motion)
            .badge(&folder.tile.badge)
            .discord(folder.discord),
        Row::Dm(_) | Row::Divider => return space().boxed(),
    };
    container(item.lifted(scale))
        .width(metrics::RAIL_WIDTH)
        .height(Length::Fixed(row.height()))
        .align_y(alignment::Vertical::Top)
        .boxed()
}

/// The gap: a slot of the dragged tile's size, where it would land.
pub fn gap<'a>(moved: &Moved<'a>) -> Element<'a, Intent> {
    let (side, radius) = match moved {
        Moved::Server(_) => (metrics::RAIL_TILE, metrics::RADIUS_TILE),
        Moved::Folder(..) => (metrics::FOLDER_TILE, metrics::RADIUS_TILE + 4.0),
    };
    container(
        container(space())
            .width(side)
            .height(side)
            .style(move |theme: &Theme| {
                let palette = theme.palette();
                container::Style {
                    background: Some(Background::Color(palette.accent_soft)),
                    border: Border {
                        color: palette.accent.scale_alpha(SLOT_EDGE_ALPHA),
                        width: SLOT_EDGE,
                        radius: radius.into(),
                    },
                    ..container::Style::default()
                }
            }),
    )
    .width(metrics::RAIL_WIDTH)
    .align_x(alignment::Horizontal::Center)
    .boxed()
}

#[cfg(test)]
mod tests {
    use anomi_vm::fixtures::rail as samples;

    use super::super::rows::rows;
    use super::*;

    /// Where aiming at row `row` puts Epsilon, the last server.
    fn aimed(rail: &Rail, row: usize, zone: Zone) -> Option<Place> {
        let rows = rows(rail);
        place(&rows, rows.len() - 1, row, zone).flatten()
    }

    #[test]
    fn a_server_aims_beside_rows_onto_servers_alone_and_closed_folders() {
        // d - Server f1 F2 F3 f2 Epsilon
        let rail = samples::folders();
        let rows = rows(&rail);
        let epsilon = rows.len() - 1;
        assert!(onto(&rows, epsilon, 2), "a server alone");
        assert!(!onto(&rows, epsilon, 3), "an open folder");
        assert!(!onto(&rows, epsilon, 4), "a server in a folder");
        assert!(onto(&rows, epsilon, 6), "a closed folder");
        assert!(!onto(&rows, 3, 2), "a folder onto anything");
        assert_eq!(
            aimed(&rail, 2, Zone::Onto),
            Some(Place::Onto("discord:g1".into()))
        );
        assert_eq!(
            aimed(&rail, 3, Zone::Below),
            Some(Place::Before("discord:g2".into())),
            "below an open folder's tile: its first place"
        );
        assert_eq!(
            aimed(&rail, 3, Zone::Above),
            Some(Place::Before("f1".into()))
        );
        assert_eq!(
            aimed(&rail, 5, Zone::Below),
            Some(Place::After("discord:g3".into()))
        );
        assert_eq!(
            aimed(&rail, 0, Zone::Below),
            Some(Place::Before("discord:g1".into())),
            "the DMs aim before the first server"
        );
        assert!(!may_drag(&rows, 0) && !may_drag(&rows, 1));
        assert!(may_drag(&rows, 2) && may_drag(&rows, 3));
        // A folder passes an open folder whole: its servers keep the gap.
        assert_eq!(place(&rows, 6, 4, Zone::Below), None);
        assert_eq!(
            place(&rows, 6, 3, Zone::Above),
            Some(Some(Place::Before("f1".into())))
        );
    }

    #[test]
    fn the_dragged_one_is_a_gap_where_it_would_land() {
        let mut rail = samples::folders();
        let mut drag = Drag {
            id: "discord:g1".into(),
            ..Drag::default()
        };
        drag.aim(Some(Place::After("discord:g2".into())));
        rail.drag = Some(drag);
        let rows = rows(&rail);
        let kinds: Vec<&str> = rows
            .iter()
            .map(|row| match row {
                Row::Gap {
                    inside: Some(_), ..
                } => "gap in",
                Row::Gap { .. } => "gap",
                Row::Server {
                    inside: Some(_), ..
                } => "in",
                Row::Server { .. } => "server",
                Row::Folder { .. } => "folder",
                Row::Dm(_) | Row::Divider => "-",
            })
            .collect();
        assert_eq!(
            kinds,
            ["-", "-", "folder", "in", "gap in", "in", "folder", "server"]
        );
        // The open folder dragged shows closed, its servers in its gap.
        rail.drag = Some(Drag {
            id: "f1".into(),
            ..Drag::default()
        });
        let rows = super::super::rows::rows(&rail);
        assert!(
            matches!(&rows[3], Row::Gap { moved: Moved::Folder(_, servers), .. } if servers.len() == 2)
        );
        assert!(matches!(&rows[4], Row::Folder { .. }));
    }
}
