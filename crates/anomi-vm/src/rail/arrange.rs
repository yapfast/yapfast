//! Moving a server or a folder on the rail. The server list is a list of
//! [`Group`]s, as Discord's settings keep it: a server alone, or a folder's
//! servers. A drag shows the move before it's made ([`preview`]) and a drop
//! makes it ([`dropped`]). As in Discord's client, a server dropped onto a
//! server alone makes a folder of the two, one dropped onto a folder goes
//! in at its end, a folder never goes into a folder, and a folder a server
//! left holding one server is that server alone again.

use super::{Drag, Rail};

/// The id a folder made by a drop has until the app's core gives it one.
pub const NEW_FOLDER: &str = "";

/// A server alone (`folder: None`) or a folder's servers, by id.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub folder: Option<String>,
    pub servers: Vec<String>,
}

/// Where a dragged server or folder goes, by the server or folder it meets.
#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    /// Before this server or folder; for a server in a folder, in it.
    Before(String),
    /// After it, likewise.
    After(String),
    /// Onto a server alone, making a folder of the two; into a folder, at
    /// its end; after a server in a folder.
    Onto(String),
}

impl Place {
    fn target(&self) -> &str {
        match self {
            Place::Before(id) | Place::After(id) | Place::Onto(id) => id,
        }
    }
}

/// The rail's servers as groups, in order: the servers of a folder the
/// rail knows follow each other, as the core lists them.
pub fn groups(rail: &Rail) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for server in &rail.servers {
        let folder = server
            .folder
            .as_ref()
            .filter(|id| rail.folders.iter().any(|folder| &folder.id == *id));
        match (groups.last_mut(), folder) {
            (Some(last), Some(id)) if last.folder.as_ref() == Some(id) => {
                last.servers.push(server.id.clone());
            }
            _ => groups.push(Group {
                folder: folder.cloned(),
                servers: vec![server.id.clone()],
            }),
        }
    }
    groups
}

/// Where `id` is: its group, and for a server its place in it.
fn find(groups: &[Group], id: &str) -> Option<(usize, Option<usize>)> {
    groups.iter().enumerate().find_map(|(at, group)| {
        if group.folder.as_deref() == Some(id) {
            return Some((at, None));
        }
        let place = group.servers.iter().position(|server| server == id)?;
        Some((at, Some(place)))
    })
}

/// `groups` with the server or folder `id` moved to `place`; a folder it
/// left keeps its last server. `None` where it can't go, or where it is.
pub fn moved(groups: &[Group], id: &str, place: &Place) -> Option<Vec<Group>> {
    if place.target() == id {
        return None;
    }
    let (from, at) = find(groups, id)?;
    let mut out = groups.to_vec();
    let folder = match at {
        None => Some(out.remove(from)),
        Some(at) => {
            out[from].servers.remove(at);
            if out[from].servers.is_empty() {
                out.remove(from);
            }
            None
        }
    };
    // A folder's own server isn't anywhere now.
    let (to, inside) = find(&out, place.target())?;
    let after = usize::from(matches!(place, Place::After(_)));
    match (place, folder) {
        (Place::Onto(_), Some(_)) => return None,
        (_, Some(folder)) => out.insert(to + after, folder),
        (Place::Onto(_), None) => match inside {
            None => out[to].servers.push(id.to_owned()),
            Some(at) if out[to].folder.is_some() => out[to].servers.insert(at + 1, id.to_owned()),
            Some(_) => {
                out[to].folder = Some(NEW_FOLDER.to_owned());
                out[to].servers.push(id.to_owned());
            }
        },
        (_, None) => match inside {
            Some(at) if out[to].folder.is_some() => {
                out[to].servers.insert(at + after, id.to_owned());
            }
            _ => out.insert(
                to + after,
                Group {
                    folder: None,
                    servers: vec![id.to_owned()],
                },
            ),
        },
    }
    (out != groups).then_some(out)
}

/// `groups` with the folder `id` first, or last, its servers with it: the
/// folder menu's Move to the top or the bottom of the list. `None` where
/// it is already, or for no folder of theirs.
pub fn to_end(groups: &[Group], id: &str, first: bool) -> Option<Vec<Group>> {
    let from = groups
        .iter()
        .position(|group| group.folder.as_deref() == Some(id))?;
    let mut out = groups.to_vec();
    let folder = out.remove(from);
    if first {
        out.insert(0, folder);
    } else {
        out.push(folder);
    }
    (out != groups).then_some(out)
}

/// The groups the rail shows while `rail.drag` goes on: what's dragged in
/// its gap. Where it's aimed onto shows on the server or folder, not here.
pub fn preview(rail: &Rail) -> Vec<Group> {
    let groups = groups(rail);
    match &rail.drag {
        Some(Drag {
            id,
            gap: Some(place),
            ..
        }) => moved(&groups, id, place).unwrap_or(groups),
        _ => groups,
    }
}

/// The rail's groups once `drag` drops where it's aimed, if that moves
/// anything: a folder the server left holding one server is that server.
pub fn dropped(rail: &Rail, drag: &Drag) -> Option<Vec<Group>> {
    let place = drag.target()?;
    let groups = groups(rail);
    let left = find(&groups, &drag.id)
        .and_then(|(group, server)| server.and(groups[group].folder.clone()));
    let mut out = moved(&groups, &drag.id, &place)?;
    if let Some(left) = left
        && let Some(group) = out
            .iter_mut()
            .find(|group| group.folder.as_deref() == Some(left.as_str()))
        && group.servers.len() == 1
    {
        group.folder = None;
    }
    Some(out)
}

#[cfg(all(test, feature = "fixtures"))]
mod tests {
    use super::*;
    use crate::fixtures::rail as samples;

    /// The groups as text: a server alone by its number, a folder's in
    /// brackets after its id (`+` for a new one).
    fn text(groups: &[Group]) -> String {
        let number = |id: &String| id.trim_start_matches("discord:g").to_owned();
        groups
            .iter()
            .map(|group| match &group.folder {
                None => number(&group.servers[0]),
                Some(id) => {
                    let id = if id.is_empty() { "+" } else { id };
                    let servers: Vec<String> = group.servers.iter().map(number).collect();
                    format!("{id}[{}]", servers.join(" "))
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn drop(id: &str, place: Place) -> String {
        let rail = samples::folders();
        let drag = Drag {
            id: id.into(),
            ..Drag::default()
        };
        let mut drag = drag;
        drag.aim(Some(place));
        dropped(&rail, &drag).map_or("unchanged".into(), |groups| text(&groups))
    }

    fn server(n: u8) -> String {
        format!("discord:g{n}")
    }

    #[test]
    fn the_list_is_the_servers_alone_and_the_folders_in_order() {
        assert_eq!(
            text(&groups(&samples::folders())),
            "1 f1[2 3] f2[4 5 6 7] 8"
        );
    }

    #[test]
    fn a_server_goes_before_or_after_another_or_a_folder() {
        assert_eq!(
            drop(&server(8), Place::Before(server(1))),
            "8 1 f1[2 3] f2[4 5 6 7]"
        );
        assert_eq!(
            drop(&server(1), Place::After("f2".into())),
            "f1[2 3] f2[4 5 6 7] 1 8"
        );
        assert_eq!(drop(&server(1), Place::Before(server(1))), "unchanged");
    }

    #[test]
    fn a_server_onto_a_server_alone_makes_a_folder_and_onto_a_folder_goes_in() {
        assert_eq!(
            drop(&server(8), Place::Onto(server(1))),
            "+[1 8] f1[2 3] f2[4 5 6 7]"
        );
        assert_eq!(
            drop(&server(1), Place::Onto("f2".into())),
            "f1[2 3] f2[4 5 6 7 1] 8"
        );
    }

    #[test]
    fn a_server_moves_within_a_folder_into_one_beside_a_server_and_out() {
        assert_eq!(
            drop(&server(7), Place::Before(server(4))),
            "1 f1[2 3] f2[7 4 5 6] 8"
        );
        assert_eq!(
            drop(&server(1), Place::After(server(2))),
            "f1[2 1 3] f2[4 5 6 7] 8"
        );
        assert_eq!(
            drop(&server(5), Place::Before(server(8))),
            "1 f1[2 3] f2[4 6 7] 5 8"
        );
    }

    #[test]
    fn a_folder_left_with_one_server_is_that_server_alone() {
        assert_eq!(
            drop(&server(2), Place::After(server(8))),
            "1 3 f2[4 5 6 7] 8 2"
        );
    }

    #[test]
    fn a_folder_moves_whole_never_into_a_folder() {
        assert_eq!(
            drop("f2", Place::Before(server(1))),
            "f2[4 5 6 7] 1 f1[2 3] 8"
        );
        assert_eq!(
            drop("f2", Place::Before(server(3))),
            "1 f2[4 5 6 7] f1[2 3] 8"
        );
        assert_eq!(drop("f2", Place::Onto(server(1))), "unchanged");
        assert_eq!(drop("f1", Place::After(server(2))), "unchanged");
    }

    #[test]
    fn a_folders_menu_moves_it_whole_to_the_top_or_the_bottom() {
        let groups = groups(&samples::folders());
        let moved = |id: &str, first| to_end(&groups, id, first).map(|out| text(&out));
        assert_eq!(
            moved("f2", true).as_deref(),
            Some("f2[4 5 6 7] 1 f1[2 3] 8")
        );
        assert_eq!(
            moved("f1", false).as_deref(),
            Some("1 f2[4 5 6 7] 8 f1[2 3]")
        );
        let first = to_end(&groups, "f2", true).unwrap();
        assert_eq!(to_end(&first, "f2", true), None, "where it is already");
        assert_eq!(
            moved(server(1).as_str(), true),
            None,
            "a server isn't a folder"
        );
        assert_eq!(moved("nowhere", false), None);
    }

    #[test]
    fn the_preview_moves_the_gap_and_keeps_the_folder_it_left() {
        let mut rail = samples::folders();
        let mut drag = Drag {
            id: server(2),
            ..Drag::default()
        };
        drag.aim(Some(Place::After(server(8))));
        rail.drag = Some(drag.clone());
        assert_eq!(text(&preview(&rail)), "1 f1[3] f2[4 5 6 7] 8 2");
        // Aimed onto a server, the gap stays where it was.
        drag.aim(Some(Place::Onto(server(1))));
        assert_eq!(drag.gap, Some(Place::After(server(8))));
        assert_eq!(drag.target(), Some(Place::Onto(server(1))));
        drag.aim(None);
        rail.drag = Some(drag);
        assert_eq!(
            text(&preview(&rail)),
            "1 f1[2 3] f2[4 5 6 7] 8",
            "where it was"
        );
    }
}
