//! What the list tells its owner about the rows in view: their keys with six
//! rows of margin, for the pictures to load and the messages read, and where
//! the view is in the list, for loading more at its edges. It tells when
//! those change, not as each pixel scrolls by, as Slint's timeline told on
//! `changed top-index` and `changed bottom-index`; a list that
//! [asks for pixels](super::List::report_pixels), as one piece that scrolls
//! does, tells as its offset or height changes too, as Slint's pickers'
//! grids told `scrolled-to(scrolled, shown-height)`. Each report carries
//! the offset as it is when it goes. At most once every 50 ms: at once
//! after a quiet spell, then spaced while it scrolls. The Slint side:
//! `media_viewport` and `report_delay` in `src/ui/callbacks.rs:707`,
//! `:4273`, the timeline's `top-index`, `bottom-index`, `wants-earlier` and
//! `wants-later` in `ui/yapfast.slint:1909`, and the pickers' `scrolled-to`
//! in `ui/gifs.slint:94`, `ui/stickers.slint:62`.

use std::time::{Duration, Instant};

/// Rows past each edge of the view whose keys are reported too.
pub const MARGIN: usize = 6;
/// The least time between two reports.
pub const SPACING: Duration = Duration::from_millis(50);

/// The rows in view and where the view is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Viewport {
    /// The first and last row in view, by index.
    pub first: usize,
    pub last: usize,
    /// The keys of the rows in view and of up to [`MARGIN`] rows on each
    /// side, in order.
    pub keys: Vec<u64>,
    /// The key of the first row in view, for coming back to it.
    pub top: u64,
    /// The list's start is less than a view's height above the view: time
    /// to load earlier rows.
    pub near_start: bool,
    /// Its end is less than a view's height below.
    pub near_end: bool,
    /// The last row's bottom is at the view's end.
    pub at_end: bool,
    /// The list follows its end: rows that come or grow there stay in view.
    pub following: bool,
    /// How far the view is from the list's start, and its height, in
    /// logical pixels: Slint's `content-y` and `visible-height`. Their
    /// change is news only to a list that
    /// [asks for pixels](super::List::report_pixels), as a
    /// [`scroll_view`](super::scroll_view) whose one row says nothing.
    pub offset: f32,
    pub height: f32,
}

impl Viewport {
    /// The same rows in view, wherever the view is in pixels.
    fn same_rows(&self, other: &Self) -> bool {
        let rows = |viewport: &Self| {
            (
                viewport.first,
                viewport.last,
                viewport.top,
                viewport.near_start,
                viewport.near_end,
                viewport.at_end,
                viewport.following,
            )
        };
        rows(self) == rows(other) && self.keys == other.keys
    }
}

/// The reports made and due.
#[derive(Debug, Default)]
pub struct Reports {
    /// The latest viewport seen, not reported yet.
    due: Option<Viewport>,
    /// The last reported, which a new one must differ from.
    sent: Option<Viewport>,
    /// When the last report went, or will go.
    last: Option<Instant>,
}

impl Reports {
    /// Notes the viewport as it is now; it's reported if it changed: its
    /// rows, or, for a list that asked for `pixels`, its offset or height.
    pub fn see(&mut self, viewport: Viewport, pixels: bool) {
        let same = self.sent.as_ref().is_some_and(|sent| {
            if pixels {
                *sent == viewport
            } else {
                sent.same_rows(&viewport)
            }
        });
        self.due = (!same).then_some(viewport);
    }

    /// The report to make at `now`, if one is due and the spacing allows;
    /// otherwise when to ask again, if one is due. Unspaced, as behind
    /// another window, where nothing scrolls and a later frame would wake
    /// the window for nothing, it goes at once.
    pub fn take(&mut self, now: Instant, spaced: bool) -> Result<Viewport, Option<Instant>> {
        if self.due.is_none() {
            return Err(None);
        }
        if spaced
            && let Some(last) = self.last
            && now < last + SPACING
        {
            return Err(Some(last + SPACING));
        }
        let viewport = self.due.take().unwrap_or_default();
        self.sent = Some(viewport.clone());
        self.last = Some(now);
        Ok(viewport)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport(first: usize) -> Viewport {
        Viewport {
            first,
            last: first + 10,
            ..Viewport::default()
        }
    }

    #[test]
    fn reports_at_once_after_a_quiet_spell_and_spaced_while_busy() {
        let start = Instant::now();
        let mut reports = Reports::default();
        assert_eq!(reports.take(start, true), Err(None), "nothing seen");
        reports.see(viewport(0), false);
        assert_eq!(reports.take(start, true), Ok(viewport(0)));
        reports.see(viewport(1), false);
        let later = start + Duration::from_millis(10);
        assert_eq!(reports.take(later, true), Err(Some(start + SPACING)));
        reports.see(viewport(2), false);
        assert_eq!(
            reports.take(start + SPACING, true),
            Ok(viewport(2)),
            "the latest"
        );
        reports.see(viewport(3), false);
        assert_eq!(
            reports.take(start + SPACING, false),
            Ok(viewport(3)),
            "behind: at once"
        );
        // The same again is no news.
        reports.see(viewport(3), false);
        assert_eq!(reports.take(start + SPACING * 3, true), Err(None));
    }

    #[test]
    fn pixels_are_news_only_to_a_list_that_asked_for_them() {
        let start = Instant::now();
        let scrolled = |offset| Viewport {
            offset,
            ..viewport(0)
        };
        for pixels in [false, true] {
            let mut reports = Reports::default();
            reports.see(scrolled(0.0), pixels);
            assert_eq!(reports.take(start, true), Ok(scrolled(0.0)));
            reports.see(scrolled(4.0), pixels);
            let told = reports.take(start + SPACING, true);
            if pixels {
                assert_eq!(told, Ok(scrolled(4.0)));
            } else {
                assert_eq!(told, Err(None), "the same rows: nothing new");
            }
        }
        // A change of rows carries the offset as it is then.
        let mut reports = Reports::default();
        reports.see(scrolled(0.0), false);
        let _ = reports.take(start, true);
        reports.see(scrolled(4.0), false);
        let moved = Viewport {
            offset: 40.0,
            ..viewport(1)
        };
        reports.see(moved.clone(), false);
        assert_eq!(reports.take(start + SPACING, true), Ok(moved));
    }
}
