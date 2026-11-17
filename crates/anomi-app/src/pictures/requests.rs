//! Asking for the pictures the rows on screen show and the core hasn't got:
//! the core leaves list avatars to the interface ("Virtualized UI rows
//! request their own avatars" in `request_pictures`,
//! `yapfast-core/src/application.rs`). The Slint rows asked as each was
//! made (`PictureStore.request`, gathered into one `Action::LoadPictures`
//! by a timer in `src/ui/callbacks.rs`); here the projection asks for the
//! rows the list reports in view (task F07's visible keys), and what it
//! asks goes to the core at most every 50 ms, as `report_delay` spaced it.
//! The emoji picker's nearest pictures go ahead of the rest
//! (`request_custom_pictures` and `fetch_shown_pictures`).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use yapfast_core::application::{Action, AppState};

/// The least time between two asks, as `REPORT_SPACING` in the Slint app.
const SPACING: Duration = Duration::from_millis(50);

/// Pictures wanted and not asked for yet. The app keeps one, has it
/// [`Requests::want`] what shows, and at [`Requests::due`] dispatches what
/// [`Requests::take`] gives.
#[derive(Default)]
pub struct Requests {
    /// To ask for, in the order wanted.
    waiting: Vec<String>,
    /// To ask for ahead of everything asked for before, nearest first.
    first: Vec<String>,
    /// What's in either, so a picture shown in forty rows is asked once.
    queued: HashSet<String>,
    /// When the last ask went.
    last: Option<Instant>,
}

impl Requests {
    /// Wants the pictures at `addresses`, which rows on screen show, those
    /// the core hasn't got. The core forgets none it was asked for, so one
    /// on its way isn't asked twice.
    pub fn want<'a>(&mut self, state: &AppState, addresses: impl IntoIterator<Item = &'a str>) {
        for address in addresses {
            if self.wanted(state, address) {
                self.waiting.push(address.to_owned());
            }
        }
    }

    /// Wants these ahead of what was asked before, in their order, as the
    /// emoji picker does for the rows nearest those it shows.
    pub fn want_first<'a>(
        &mut self,
        state: &AppState,
        addresses: impl IntoIterator<Item = &'a str>,
    ) {
        for address in addresses {
            if self.wanted(state, address) {
                self.first.push(address.to_owned());
            }
        }
    }

    fn wanted(&mut self, state: &AppState, address: &str) -> bool {
        !address.is_empty()
            && !state.pictures.contains_key(address)
            && self.queued.insert(address.to_owned())
    }

    /// When the next ask may go: at once after a quiet spell, else 50 ms
    /// after the last; none with nothing waiting, so nothing ticks.
    pub fn due(&self, now: Instant) -> Option<Instant> {
        if self.waiting.is_empty() && self.first.is_empty() {
            return None;
        }
        Some(self.last.map_or(now, |last| (last + SPACING).max(now)))
    }

    /// The core's actions asking for what's waiting, once it's due; none
    /// before.
    pub fn take(&mut self, now: Instant) -> Vec<Action> {
        if self.due(now).is_none_or(|due| due > now) {
            return Vec::new();
        }
        self.last = Some(now);
        self.queued.clear();
        let mut actions = Vec::new();
        if !self.first.is_empty() {
            actions.push(Action::LoadPicturesFirst(std::mem::take(&mut self.first)));
        }
        if !self.waiting.is_empty() {
            actions.push(Action::LoadPictures(std::mem::take(&mut self.waiting)));
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(actions: &[Action]) -> Vec<(bool, Vec<String>)> {
        actions
            .iter()
            .filter_map(|action| match action {
                Action::LoadPicturesFirst(urls) => Some((true, urls.clone())),
                Action::LoadPictures(urls) => Some((false, urls.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn what_shows_is_asked_once_and_asks_are_spaced() {
        let mut state = AppState::default();
        state.pictures.insert("came.png".into(), Default::default());
        let mut requests = Requests::default();
        let now = Instant::now();
        assert_eq!(
            requests.due(now),
            None,
            "nothing waiting, nothing to wake for"
        );
        requests.want(&state, ["a.png", "came.png", "", "a.png", "b.png"]);
        assert_eq!(requests.due(now), Some(now), "after a quiet spell, at once");
        let taken = requests.take(now);
        assert_eq!(
            asked(&taken),
            [(false, vec!["a.png".to_owned(), "b.png".to_owned()])]
        );
        // While busy, the next ask waits out the spacing.
        requests.want(&state, ["c.png"]);
        assert_eq!(requests.due(now), Some(now + SPACING));
        assert!(requests.take(now + Duration::from_millis(20)).is_empty());
        let later = now + SPACING;
        assert_eq!(
            asked(&requests.take(later)),
            [(false, vec!["c.png".to_owned()])]
        );
    }

    #[test]
    fn the_nearest_go_ahead_of_the_rest() {
        let state = AppState::default();
        let mut requests = Requests::default();
        requests.want(&state, ["row.png"]);
        requests.want_first(&state, ["near.png", "row.png", "next.png"]);
        let taken = requests.take(Instant::now());
        assert_eq!(
            asked(&taken),
            [
                (true, vec!["near.png".to_owned(), "next.png".to_owned()]),
                (false, vec!["row.png".to_owned()]),
            ]
        );
    }
}
