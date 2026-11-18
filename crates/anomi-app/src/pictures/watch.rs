//! Which of the core's pictures an area's rows show, by address, so that a
//! picture coming or going reaches the rows that show it and nothing else:
//! the core counts every picture that arrives or goes in one number
//! (`picture_changes`), which moves as often as the timeline's avatars, a
//! picker's emoji or a server's icons come. An area notes each address as it
//! builds a row ([`Watch::note`]) and, as the count moves, asks which rows'
//! pictures changed ([`Watch::moved`]): it builds those again, or with no
//! row of its own picture among them, nothing. An area whose rows look
//! their pictures up deep in its builders builds them in [`noting`], which
//! notes every address [`super::find`] looks up meanwhile. The Slint app's
//! rows asked their pictures of a store that filled them as they came
//! (`PictureStore`, `src/ui/pictures.rs`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use yapfast_core::application::AppState;
use yapfast_core::chat::Picture;

/// The pictures an area's rows show: by address, which picture each showed
/// (the core's own, by pointer; none while it hadn't come) and which rows.
#[derive(Debug)]
pub struct Watch<K> {
    /// The core's count of pictures coming and going when last asked.
    changes: u64,
    shown: HashMap<Box<str>, Seen<K>>,
}

#[derive(Debug)]
struct Seen<K> {
    picture: usize,
    rows: Vec<K>,
}

impl<K> Default for Watch<K> {
    fn default() -> Self {
        Self {
            changes: 0,
            shown: HashMap::new(),
        }
    }
}

/// The picture at an address as a number that changes when it comes, goes
/// or is replaced; 0 while there is none.
fn at(state: &AppState, address: &str) -> usize {
    state
        .pictures
        .get(address)
        .map_or(0, |picture: &Arc<Picture>| Arc::as_ptr(picture).addr())
}

impl<K: Copy + PartialEq> Watch<K> {
    /// Forgets every row, as the area builds them all again.
    pub fn clear(&mut self) {
        self.shown.clear();
    }

    /// `row` shows the picture at `address` as `state` has it. Ask
    /// [`Watch::moved`] of the same state first, so that what's noted and
    /// what was noted before agree.
    pub fn note(&mut self, state: &AppState, address: &str, row: K) {
        if address.is_empty() {
            return;
        }
        match self.shown.get_mut(address) {
            Some(seen) => {
                if !seen.rows.contains(&row) {
                    seen.rows.push(row);
                }
            }
            None => {
                self.shown.insert(
                    address.into(),
                    Seen {
                        picture: at(state, address),
                        rows: vec![row],
                    },
                );
            }
        }
    }

    /// The rows whose pictures came, went or changed since they were
    /// noted, each once; none at once while the core's count stood still.
    pub fn moved(&mut self, state: &AppState) -> Vec<K> {
        if self.changes == state.picture_changes {
            return Vec::new();
        }
        self.changes = state.picture_changes;
        let mut rows: Vec<K> = Vec::new();
        for (address, seen) in &mut self.shown {
            let now = at(state, address);
            if now == seen.picture {
                continue;
            }
            seen.picture = now;
            for row in &seen.rows {
                if !rows.contains(row) {
                    rows.push(*row);
                }
            }
        }
        rows
    }

    /// Keeps only the rows `keep` says still show, and the addresses some
    /// row still shows.
    pub fn retain(&mut self, mut keep: impl FnMut(K) -> bool) {
        self.shown.retain(|_, seen| {
            seen.rows.retain(|row| keep(*row));
            !seen.rows.is_empty()
        });
    }

    /// The addresses it watches, to ask for those not here yet.
    pub fn addresses(&self) -> impl Iterator<Item = &str> {
        self.shown.keys().map(|address| &**address)
    }

    /// How many addresses it watches.
    pub fn len(&self) -> usize {
        self.shown.len()
    }

    /// It watches nothing.
    pub fn is_empty(&self) -> bool {
        self.shown.is_empty()
    }
}

thread_local! {
    /// The addresses [`super::find`] looked up while [`noting`] runs.
    static NOTED: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// Runs `build`, an area building all its rows again, and has `watch`
/// watch every picture it looked up through [`super::find`] in place of
/// those it watched: the area is built again only once one of them comes or
/// goes.
pub fn noting<T>(state: &AppState, watch: &mut Watch<()>, build: impl FnOnce() -> T) -> T {
    let outer = NOTED.replace(Some(Vec::new()));
    let made = build();
    let noted = NOTED.replace(outer).unwrap_or_default();
    watch.clear();
    for address in &noted {
        watch.note(state, address, ());
    }
    made
}

/// `address` was looked up: noted while [`noting`] runs.
pub(super) fn looked_up(address: &str) {
    NOTED.with_borrow_mut(|noted| {
        if let Some(noted) = noted {
            noted.push(address.to_owned());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_reaches_only_the_rows_that_show_it() {
        let mut state = AppState::default();
        let mut watch: Watch<u64> = Watch::default();
        assert!(watch.moved(&state).is_empty());
        watch.note(&state, "a.png", 1);
        watch.note(&state, "a.png", 2);
        watch.note(&state, "b.png", 3);
        watch.note(&state, "", 4);
        // Another area's picture comes: no row of this one changes.
        state.pictures.insert("c.png".into(), Default::default());
        state.picture_changes += 1;
        assert!(watch.moved(&state).is_empty());
        state.pictures.insert("a.png".into(), Default::default());
        state.picture_changes += 1;
        assert_eq!(watch.moved(&state), [1, 2]);
        assert!(watch.moved(&state).is_empty(), "told once");
        // It goes again: those rows let go of it.
        state.pictures.remove("a.png");
        state.picture_changes += 1;
        assert_eq!(watch.moved(&state), [1, 2]);
        watch.retain(|row| row != 2);
        state.pictures.insert("a.png".into(), Default::default());
        state.picture_changes += 1;
        assert_eq!(watch.moved(&state), [1]);
        assert_eq!(watch.len(), 2);
    }

    #[test]
    fn what_a_build_looks_up_is_watched() {
        let mut state = AppState::default();
        let mut watch = Watch::default();
        let made = noting(&state, &mut watch, || {
            super::super::find(&state, Some("icon.png")).is_none()
        });
        assert!(made);
        assert_eq!(watch.len(), 1);
        assert!(super::super::find(&state, Some("elsewhere.png")).is_none());
        assert_eq!(watch.len(), 1, "only while noting");
        state.pictures.insert("icon.png".into(), Default::default());
        state.picture_changes += 1;
        assert_eq!(watch.moved(&state), [()]);
    }
}
