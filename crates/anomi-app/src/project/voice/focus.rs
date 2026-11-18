//! Which of the call's tiles is which, which one shows large, and what the
//! corner shows while the call is off screen: `Stage`, `Focus` and `Pip` of
//! `src/ui/tiles.rs`. The tiles' order: this account's shared screen leads,
//! then the streams of those who film as well, in tiles of their own, then
//! everyone.

use yapfast_core::voice::Participant;

/// The tile shown large, as a click on it chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Focus {
    /// Someone's tile, by participant id: their camera, or their picture.
    Person(String),
    /// Someone's stream: in a tile of its own while they film too, or else
    /// in theirs.
    Stream(String),
    /// This account's shared screen.
    OwnScreen,
}

/// What the corner shows while the call is off screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pip {
    None,
    Watched,
    Shared,
    /// The camera of the participant at this index.
    Camera(usize),
}

/// The call's tiles in order (see the module's doc).
#[derive(Clone, Copy)]
pub struct Stage<'a> {
    pub participants: &'a [Participant],
    pub sharing: bool,
}

impl Stage<'_> {
    /// The streams with tiles of their own, as indexes of their
    /// participants: someone who shares without a camera shows the stream
    /// in their own tile.
    pub fn streams(&self) -> impl Iterator<Item = usize> + '_ {
        self.participants
            .iter()
            .enumerate()
            .filter(|(_, person)| person.streaming && person.video.is_some() && !person.local)
            .map(|(index, _)| index)
    }

    /// How many tiles come before the participants' own.
    pub fn before(&self) -> usize {
        usize::from(self.sharing) + self.streams().count()
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.participants.iter().position(|person| person.id == id)
    }

    /// The slot of the tile `focus` shows, if it's in the call.
    pub fn slot(&self, focus: &Focus) -> Option<usize> {
        match focus {
            Focus::OwnScreen => self.sharing.then_some(0),
            Focus::Person(id) => Some(self.before() + self.index(id)?),
            Focus::Stream(id) => {
                let person = self.index(id)?;
                match self.streams().position(|stream| stream == person) {
                    Some(stream) => Some(usize::from(self.sharing) + stream),
                    None => {
                        let them = &self.participants[person];
                        (them.streaming && !them.local).then(|| self.before() + person)
                    }
                }
            }
        }
    }

    /// What a click on the tile at `slot` shows large.
    pub fn focus_at(&self, slot: usize) -> Option<Focus> {
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
    pub fn pip(&self, focus: Option<&Focus>, watching: Option<&str>) -> Pip {
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

    /// The slot of the tile the corner shows, which a click on it shows
    /// large.
    pub fn pip_slot(&self, pip: Pip, watching: Option<&str>) -> Option<usize> {
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

    fn person(id: &str, streaming: bool, filming: bool) -> Participant {
        Participant {
            id: id.into(),
            user: id.into(),
            person: id.into(),
            local: id == "me",
            muted: false,
            deafened: false,
            server_muted: false,
            server_deafened: false,
            speaking: false,
            connected: true,
            problem: None,
            streaming,
            video: filming.then(yapfast_core::picture::Feed::default),
            avatar: None,
        }
    }

    #[test]
    fn a_stream_of_someone_who_films_has_a_tile_of_its_own_before_everyone() {
        let people = [
            person("me", true, false),
            person("bob", true, true),
            person("carol", true, false),
        ];
        let stage = Stage {
            participants: &people,
            sharing: true,
        };
        assert_eq!(stage.before(), 2);
        assert_eq!(stage.slot(&Focus::OwnScreen), Some(0));
        assert_eq!(stage.slot(&Focus::Stream("bob".into())), Some(1));
        assert_eq!(stage.slot(&Focus::Stream("carol".into())), Some(4));
        assert_eq!(stage.focus_at(1), Some(Focus::Stream("bob".into())));
        assert_eq!(stage.focus_at(3), Some(Focus::Person("bob".into())));
        assert_eq!(stage.focus_at(4), Some(Focus::Stream("carol".into())));
        assert_eq!(stage.focus_at(2), Some(Focus::OwnScreen), "my own tile");
    }

    #[test]
    fn the_corner_shows_what_is_large_else_what_is_watched() {
        let people = [person("me", false, false), person("bob", true, true)];
        let stage = Stage {
            participants: &people,
            sharing: false,
        };
        let bob = Focus::Person("bob".into());
        assert_eq!(stage.pip(Some(&bob), Some("bob")), Pip::Camera(1));
        assert_eq!(stage.pip(None, Some("bob")), Pip::Watched);
        assert_eq!(stage.pip_slot(Pip::Watched, Some("bob")), Some(0));
        assert_eq!(stage.pip(None, None), Pip::Camera(1));
    }
}
