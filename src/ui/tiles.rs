//! Which of the call's tiles are on screen, and how large, so a camera's
//! pictures are made only for tiles in sight, at their size; which tile
//! shows large; and what the corner shows while the call is off screen.

use yapfast_core::voice::Participant;

/// The call grid's layout, in logical pixels, as `ParticipantGrid` reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Tiles {
    pub columns: u32,
    pub width: f32,
    pub height: f32,
    pub gap: f32,
    /// The first row's top, and how far the grid is scrolled down.
    pub top: f32,
    pub scrolled: f32,
    /// The grid's height on screen.
    pub shown: f32,
    /// The tile shown large, and its size; the rows hold the others.
    pub focused: Option<usize>,
    pub focus_width: f32,
    pub focus_height: f32,
}

impl Tiles {
    /// The size in pixels the tile at `index` shows at, if any of it is in
    /// sight. `index` counts the tiles before it, such as a shared screen.
    /// `wall` lists the tiles sharing the room, which leave the rows; only
    /// streams do, which a camera's tile never is.
    pub(super) fn shown_at(&self, index: usize, scale: f32, wall: &[usize]) -> Option<(u32, u32)> {
        let pixels = |side: f32| ((side * scale).round() as u32).max(2);
        let index = match self.focused {
            Some(focused) if index == focused => {
                return (self.focus_width > 0.0 && self.focus_height > 0.0)
                    .then(|| (pixels(self.focus_width), pixels(self.focus_height)));
            }
            Some(focused) if index > focused => index - 1,
            _ => index - wall.iter().filter(|tile| **tile < index).count(),
        };
        if self.columns == 0 || self.width <= 0.0 || self.height <= 0.0 {
            return None;
        }
        let row = (index / self.columns as usize) as f32;
        let top = self.top + row * (self.height + self.gap) - self.scrolled;
        let in_sight = top < self.shown && top + self.height > 0.0;
        in_sight.then(|| (pixels(self.width), pixels(self.height)))
    }
}

/// The tile the call shows large, as a click on it chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Focus {
    /// Someone's tile, by participant id: their camera, or their picture.
    Person(String),
    /// Someone's stream: in a tile of its own while they film too, or else
    /// in theirs.
    Stream(String),
    /// This account's shared screen.
    OwnScreen,
}

/// What shows in a corner while the call is off screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pip {
    None,
    Watched,
    Shared,
    /// The camera of the participant at this index.
    Camera(usize),
}

/// What shows large as the streams showing in the call change from `before`
/// to `after`, by whose they are: a stream alone does, and one joining others
/// leaves them all side by side.
pub(super) fn follow_streams(focus: &mut Option<Focus>, before: &[String], after: &[String]) {
    if let [alone] = after {
        *focus = Some(Focus::Stream(alone.clone()));
    } else if after.iter().any(|user| !before.contains(user))
        || matches!(focus, Some(Focus::Stream(user)) if !after.contains(user))
    {
        *focus = None;
    }
}

/// Each tile's place, among those of `tiles` tiles: on `wall`, or -1, and in
/// the rows beneath it, or -1.
pub(super) fn places(tiles: usize, wall: &[usize]) -> (Vec<i32>, Vec<i32>) {
    let (mut on_wall, mut in_rows) = (0..0, 0..);
    (0..tiles)
        .map(|tile| {
            if wall.contains(&tile) {
                on_wall.end += 1;
                (on_wall.end - 1, -1)
            } else {
                (-1, in_rows.next().unwrap_or_default())
            }
        })
        .unzip()
}

/// The call's tiles in order: this account's shared screen leads, then the
/// streams of those who film as well, in tiles of their own, then everyone.
#[derive(Clone, Copy)]
pub(super) struct Stage<'a> {
    pub participants: &'a [Participant],
    pub sharing: bool,
}

impl Stage<'_> {
    /// The streams with tiles of their own, as indexes of their participants.
    /// Someone who shares without a camera shows the stream in their tile.
    pub(super) fn streams(&self) -> impl Iterator<Item = usize> + '_ {
        self.participants
            .iter()
            .enumerate()
            .filter(|(_, person)| person.streaming && person.video.is_some() && !person.local)
            .map(|(index, _)| index)
    }

    /// How many tiles there are.
    pub(super) fn tiles(&self) -> usize {
        self.before() + self.participants.len()
    }

    /// How many tiles come before the participants' own.
    pub(super) fn before(&self) -> usize {
        usize::from(self.sharing) + self.streams().count()
    }

    /// The tiles of the streams `showing`, by whose they are, in order, which
    /// share the room when there are two or more.
    pub(super) fn wall(&self, showing: &[String]) -> Vec<usize> {
        let mut wall: Vec<usize> = showing
            .iter()
            .filter_map(|user| self.slot(&Focus::Stream(user.clone())))
            .collect();
        wall.sort_unstable();
        if wall.len() < 2 {
            wall.clear();
        }
        wall
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.participants.iter().position(|person| person.id == id)
    }

    /// The place of the tile `focus` shows, if it's in the call.
    pub(super) fn slot(&self, focus: &Focus) -> Option<usize> {
        match focus {
            Focus::OwnScreen => self.sharing.then_some(0),
            Focus::Person(id) => Some(self.before() + self.index(id)?),
            Focus::Stream(id) => {
                let person = self.index(id)?;
                match self.streams().position(|stream| stream == person) {
                    Some(stream) => Some(usize::from(self.sharing) + stream),
                    None => (self.participants[person].streaming
                        && !self.participants[person].local)
                        .then(|| self.before() + person),
                }
            }
        }
    }

    /// What a click on the tile at `slot` shows large.
    pub(super) fn focus_at(&self, slot: usize) -> Option<Focus> {
        let sharing = usize::from(self.sharing);
        if slot < sharing {
            return Some(Focus::OwnScreen);
        }
        let before = self.before();
        if slot < before {
            let person = self.streams().nth(slot - sharing)?;
            return Some(Focus::Stream(self.participants[person].id.clone()));
        }
        let person = self.participants.get(slot - before)?;
        let id = person.id.clone();
        Some(if person.video.is_some() || !person.streaming {
            Focus::Person(id)
        } else if person.local {
            // This account's own tile, while it shares, stands for its screen.
            if self.sharing {
                Focus::OwnScreen
            } else {
                Focus::Person(id)
            }
        } else {
            Focus::Stream(id)
        })
    }

    /// What the corner shows: what's large, if it moves; else the stream
    /// watched or shared; else the first camera of someone else.
    pub(super) fn pip(&self, focus: Option<&Focus>, watching: Option<&str>) -> Pip {
        match focus {
            Some(Focus::OwnScreen) if self.sharing => return Pip::Shared,
            Some(Focus::Stream(id)) if watching == Some(id.as_str()) => return Pip::Watched,
            Some(Focus::Person(id)) => {
                if let Some(person) = self.index(id)
                    && self.participants[person].video.is_some()
                {
                    return Pip::Camera(person);
                }
            }
            _ => {}
        }
        if watching.is_some() {
            Pip::Watched
        } else if self.sharing {
            Pip::Shared
        } else {
            self.participants
                .iter()
                .position(|person| !person.local && person.video.is_some())
                .map_or(Pip::None, Pip::Camera)
        }
    }

    /// The place of the tile the corner shows, which a click on it shows large.
    pub(super) fn pip_slot(&self, pip: Pip, watching: Option<&str>) -> Option<usize> {
        match pip {
            Pip::None => None,
            Pip::Watched => self.slot(&Focus::Stream(watching?.to_owned())),
            Pip::Shared => self.slot(&Focus::OwnScreen),
            Pip::Camera(person) => Some(self.before() + person),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_tiles_in_sight_are_shown() {
        let tiles = Tiles {
            columns: 2,
            width: 300.0,
            height: 168.75,
            gap: 12.0,
            top: 24.0,
            scrolled: 0.0,
            shown: 380.0,
            ..Tiles::default()
        };
        assert_eq!(tiles.shown_at(0, 2.0, &[]), Some((600, 338)));
        assert!(
            tiles.shown_at(3, 1.0, &[]).is_some(),
            "the second row shows"
        );
        assert_eq!(tiles.shown_at(4, 1.0, &[]), None, "the third starts below");
        let scrolled = Tiles {
            scrolled: 220.0,
            ..tiles
        };
        assert_eq!(scrolled.shown_at(0, 1.0, &[]), None, "scrolled away");
        assert!(scrolled.shown_at(4, 1.0, &[]).is_some());
        assert_eq!(Tiles::default().shown_at(0, 1.0, &[]), None);
    }

    #[test]
    fn streams_on_the_wall_leave_the_rows() {
        let tiles = Tiles {
            columns: 2,
            width: 200.0,
            height: 112.5,
            gap: 10.0,
            top: 400.0,
            scrolled: 0.0,
            shown: 500.0,
            ..Tiles::default()
        };
        // Tiles 0 and 2 are on the wall: 1 and 3 fill the first row, and 4
        // and 5 start the second, below what shows.
        assert!(tiles.shown_at(3, 1.0, &[0, 2]).is_some());
        assert_eq!(tiles.shown_at(4, 1.0, &[0, 2]), None);
        assert_eq!(tiles.shown_at(3, 1.0, &[]), None, "without the wall, row 2");
    }

    #[test]
    fn the_focused_tile_shows_large_and_the_others_in_its_rows() {
        let tiles = Tiles {
            columns: 3,
            width: 192.0,
            height: 108.0,
            gap: 12.0,
            top: 500.0,
            scrolled: 0.0,
            shown: 600.0,
            focused: Some(1),
            focus_width: 800.0,
            focus_height: 450.0,
        };
        assert_eq!(tiles.shown_at(1, 1.0, &[]), Some((800, 450)));
        assert_eq!(tiles.shown_at(0, 1.0, &[]), Some((192, 108)));
        assert_eq!(
            tiles.shown_at(3, 1.0, &[]),
            Some((192, 108)),
            "third in the row"
        );
        assert_eq!(tiles.shown_at(4, 1.0, &[]), None, "the second row is below");
        let alone = Tiles {
            columns: 0,
            ..tiles
        };
        assert_eq!(alone.shown_at(1, 1.0, &[]), Some((800, 450)));
        assert_eq!(alone.shown_at(0, 1.0, &[]), None, "full screen shows one");
    }

    #[test]
    fn a_stream_alone_shows_large_and_one_joining_shows_beside_it() {
        let names = |users: &[&str]| -> Vec<String> {
            users.iter().map(|user| (*user).to_owned()).collect()
        };
        let (none, ana, both) = (names(&[]), names(&["ana"]), names(&["ana", "bo"]));
        let ana_large = Some(Focus::Stream("ana".into()));
        let mut focus = None;
        follow_streams(&mut focus, &none, &ana);
        assert_eq!(focus, ana_large);
        follow_streams(&mut focus, &ana, &both);
        assert_eq!(focus, None, "side by side");
        // The one left alone shows large, as when the other pops out.
        follow_streams(&mut focus, &both, &ana);
        assert_eq!(focus, ana_large);
        follow_streams(&mut focus, &ana, &none);
        assert_eq!(focus, None, "it stopped");
        // One leaving others keeps what the person chose to show large,
        let mut focus = Some(Focus::Person("cy".into()));
        let three = names(&["ana", "bo", "cy"]);
        follow_streams(&mut focus, &three, &both);
        assert_eq!(focus, Some(Focus::Person("cy".into())));
        // unless that was it.
        let mut focus = Some(Focus::Stream("cy".into()));
        follow_streams(&mut focus, &three, &both);
        assert_eq!(focus, None);
    }

    #[test]
    fn two_streams_or_more_share_the_room() {
        let people = [
            person("me", true, false, false),
            person("ana", false, true, false),
            person("bo", false, true, true),
            person("cy", false, true, false),
        ];
        let stage = Stage {
            participants: &people,
            sharing: false,
        };
        let names = |users: &[&str]| -> Vec<String> {
            users.iter().map(|user| (*user).to_owned()).collect()
        };
        assert_eq!(stage.tiles(), 5, "Bo's stream has a tile of its own");
        assert_eq!(stage.wall(&names(&["cy", "bo", "ana"])), [0, 2, 4]);
        assert_eq!(stage.wall(&names(&["ana"])), [] as [usize; 0]);
        assert_eq!(
            stage.wall(&names(&["ana", "gone"])),
            [] as [usize; 0],
            "one stream left has no one to share with"
        );
        assert_eq!(
            places(5, &[0, 2, 4]),
            (vec![0, -1, 1, -1, 2], vec![-1, 0, -1, 1, -1])
        );
        assert_eq!(places(2, &[]), (vec![-1, -1], vec![0, 1]));
    }

    fn person(id: &str, local: bool, streaming: bool, filming: bool) -> Participant {
        Participant {
            id: id.into(),
            user: id.into(),
            person: id.into(),
            local,
            muted: false,
            deafened: false,
            server_muted: false,
            server_deafened: false,
            speaking: false,
            connected: true,
            problem: None,
            streaming,
            video: filming.then(yapfast_core::picture::Feed::new),
            avatar: None,
        }
    }

    #[test]
    fn a_stream_takes_its_own_tile_only_beside_a_camera() {
        let people = [
            person("me", true, true, true),
            person("ana", false, true, false),
            person("bo", false, true, true),
            person("cy", false, false, true),
        ];
        let stage = Stage {
            participants: &people,
            sharing: true,
        };
        assert_eq!(stage.streams().collect::<Vec<_>>(), [2]);
        assert_eq!(stage.before(), 2);
        assert_eq!(stage.slot(&Focus::OwnScreen), Some(0));
        assert_eq!(stage.slot(&Focus::Stream("bo".into())), Some(1));
        assert_eq!(
            stage.slot(&Focus::Stream("ana".into())),
            Some(3),
            "in her tile"
        );
        assert_eq!(
            stage.slot(&Focus::Stream("cy".into())),
            None,
            "not streaming"
        );
        assert_eq!(stage.slot(&Focus::Person("cy".into())), Some(5));
        for slot in 0..6 {
            let focus = stage.focus_at(slot).unwrap();
            assert_eq!(stage.slot(&focus), Some(slot), "{focus:?}");
        }
        assert_eq!(stage.focus_at(3), Some(Focus::Stream("ana".into())));
        assert_eq!(stage.focus_at(6), None);
    }

    #[test]
    fn the_corner_shows_what_is_large_or_else_a_stream_or_a_camera() {
        let people = [
            person("me", true, false, true),
            person("ana", false, true, false),
            person("cy", false, false, true),
        ];
        let stage = Stage {
            participants: &people,
            sharing: false,
        };
        let cy = Focus::Person("cy".into());
        assert_eq!(stage.pip(Some(&cy), Some("ana")), Pip::Camera(2));
        assert_eq!(stage.pip(None, Some("ana")), Pip::Watched);
        assert_eq!(stage.pip_slot(Pip::Watched, Some("ana")), Some(1));
        assert_eq!(
            stage.pip(None, None),
            Pip::Camera(2),
            "not this account's own"
        );
        let sharing = Stage {
            sharing: true,
            ..stage
        };
        assert_eq!(
            sharing.pip(Some(&Focus::OwnScreen), Some("ana")),
            Pip::Shared
        );
        assert_eq!(sharing.pip_slot(Pip::Shared, None), Some(0));
    }
}
