//! The rail's rows, flat as the Slint list has them: unread DMs, the line
//! under them, then the servers with each open folder's after its tile.
//! A server in a folder follows the folder's first one, as the core lists
//! them; `refresh.rs`'s `rail_rows` flattened the same way.

use std::hash::{DefaultHasher, Hash, Hasher};

use anomi_theme::metrics;
use anomi_vm::rail::{Dm, Folder, Rail, Server};
use iced::Color;

/// Under an open folder's tile, a strip of its colour joins its servers to it.
const STRIP_END: f32 = 4.0;
/// The gap under a row that isn't an open folder.
const GAP: f32 = metrics::RAIL_GAP;
/// The line between unread DMs and servers: its height with its gap.
const DIVIDER: f32 = 1.0;

pub enum Row<'a> {
    Divider,
    Dm(&'a Dm),
    Folder {
        folder: &'a Folder,
        servers: &'a [Server],
    },
    Server {
        server: &'a Server,
        /// In an open folder: its colour, and whether it's the last.
        inside: Option<(Color, bool)>,
    },
}

/// The rows of `rail`, in order.
pub fn rows(rail: &Rail) -> Vec<Row<'_>> {
    let mut rows = Vec::with_capacity(rail.dms.len() + rail.servers.len() + 1);
    rows.extend(rail.dms.iter().map(Row::Dm));
    if !rail.dms.is_empty() {
        rows.push(Row::Divider);
    }
    let mut servers = rail.servers.iter().peekable();
    while let Some(server) = servers.next() {
        let folder = server
            .folder
            .as_ref()
            .and_then(|id| rail.folders.iter().find(|folder| &folder.id == id));
        let Some(folder) = folder else {
            rows.push(Row::Server {
                server,
                inside: None,
            });
            continue;
        };
        // A folder's servers are contiguous, so they are a slice of the rail's.
        let start = rail.servers.len() - servers.len() - 1;
        let mut end = start + 1;
        while servers
            .next_if(|next| next.folder == server.folder)
            .is_some()
        {
            end += 1;
        }
        let inside = &rail.servers[start..end];
        let last = inside.len() - 1;
        let open = folder.open;
        let tint = folder.tint;
        rows.push(Row::Folder {
            folder,
            servers: inside,
        });
        if open {
            for (index, server) in inside.iter().enumerate() {
                rows.push(Row::Server {
                    server,
                    inside: Some((tint, index == last)),
                });
            }
        }
    }
    rows
}

impl Row<'_> {
    /// What names it across rebuilds, so a row keeps its state.
    pub fn key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        match self {
            Row::Divider => 0u8.hash(&mut hasher),
            Row::Dm(dm) => (1u8, &dm.id).hash(&mut hasher),
            Row::Folder { folder, .. } => (2u8, &folder.id).hash(&mut hasher),
            Row::Server { server, .. } => (3u8, &server.id).hash(&mut hasher),
        }
        hasher.finish()
    }

    /// Its height with the gap under it.
    pub fn height(&self) -> f32 {
        match self {
            Row::Divider => DIVIDER + GAP,
            Row::Dm(_) => metrics::RAIL_TILE + GAP,
            Row::Folder { folder, .. } if folder.open => metrics::FOLDER_TILE + STRIP_END,
            Row::Folder { .. } => metrics::FOLDER_TILE + GAP,
            Row::Server {
                inside: Some((_, true)),
                ..
            } => metrics::RAIL_TILE + STRIP_END + GAP,
            Row::Server { .. } => metrics::RAIL_TILE + GAP,
        }
    }

    /// The strip behind it, if it's part of an open folder: its colour, its
    /// height, and whether it starts or ends rounded.
    pub fn strip(&self) -> Option<Strip> {
        match self {
            Row::Folder { folder, .. } if folder.open => Some(Strip {
                tint: folder.tint,
                height: self.height(),
                top: true,
                bottom: false,
            }),
            Row::Server {
                inside: Some((tint, last)),
                ..
            } => Some(Strip {
                tint: *tint,
                height: if *last {
                    metrics::RAIL_TILE + STRIP_END
                } else {
                    self.height()
                },
                top: false,
                bottom: *last,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Strip {
    pub tint: Color,
    pub height: f32,
    pub top: bool,
    pub bottom: bool,
}

#[cfg(test)]
mod tests {
    use anomi_vm::fixtures::rail as samples;

    use super::*;

    fn kinds(rail: &Rail) -> String {
        rows(rail)
            .iter()
            .map(|row| match row {
                Row::Divider => '-',
                Row::Dm(_) => 'd',
                Row::Folder { folder, .. } if folder.open => 'F',
                Row::Folder { .. } => 'f',
                Row::Server {
                    inside: Some(_), ..
                } => 'n',
                Row::Server { .. } => 's',
            })
            .collect()
    }

    #[test]
    fn dms_come_first_with_a_line_only_when_there_are_some() {
        assert_eq!(kinds(&samples::rail()), "sss");
        assert_eq!(kinds(&samples::with_dms()), "d-sss");
    }

    #[test]
    fn an_open_folder_shows_its_servers_after_it_and_a_closed_one_does_not() {
        // Server, the open folder with F2 and F3, the closed one, Epsilon.
        assert_eq!(kinds(&samples::folders()), "d-sFnnfs");
        let mut rail = samples::folders();
        rail.folders[0].open = false;
        assert_eq!(kinds(&rail), "d-sffs");
        assert_eq!(
            rows(&rail)
                .iter()
                .filter(|row| row.strip().is_some())
                .count(),
            0
        );
    }

    #[test]
    fn a_closed_folder_keeps_its_servers_for_its_tile() {
        let rail = samples::folders();
        let rows = rows(&rail);
        let Some(Row::Folder { servers, .. }) = rows
            .iter()
            .find(|row| matches!(row, Row::Folder { folder, .. } if folder.id == "f2"))
        else {
            panic!("no closed folder");
        };
        assert_eq!(servers.len(), 4);
    }

    #[test]
    fn the_strip_runs_from_the_folder_to_its_last_server_and_ends_rounded() {
        let rail = samples::folders();
        let rows = rows(&rail);
        let strips: Vec<_> = rows.iter().filter_map(Row::strip).collect();
        assert_eq!(strips.len(), 3);
        assert!(strips[0].top && !strips[0].bottom);
        assert!(!strips[1].top && !strips[1].bottom);
        assert!(!strips[2].top && strips[2].bottom);
        // The last stops 4 under its tile, the others fill their rows.
        assert_eq!(strips[2].height, metrics::RAIL_TILE + STRIP_END);
        assert_eq!(strips[1].height, metrics::RAIL_TILE + GAP);
    }

    #[test]
    fn rows_are_keyed_by_what_they_are() {
        let rail = samples::folders();
        let keys: Vec<u64> = rows(&rail).iter().map(Row::key).collect();
        let mut unique = keys.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), keys.len());
        // Opening the folder changes no other row's key.
        let mut closed = rail.clone();
        closed.folders[0].open = false;
        let others: Vec<u64> = rows(&closed).iter().map(Row::key).collect();
        assert!(others.iter().all(|key| keys.contains(key)));
    }

    #[test]
    fn a_server_naming_an_unknown_folder_is_a_plain_server() {
        let mut rail = samples::folders();
        rail.folders.clear();
        assert_eq!(kinds(&rail), "d-ssssssss");
    }
}
