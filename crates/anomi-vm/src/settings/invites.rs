//! A server's Invites settings: what it shows and what it asks for. Task X04
//! (docs/iced/tasks/settings.md). The Slint side: `InvitesSection`
//! (`ui/server_people.slint:295`) over `InvitesPrompt`.

use std::time::Instant;

use iced::Color;

use crate::kind::Avatar;

/// One invite of the table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub code: String,
    pub inviter: Avatar,
    /// The inviter's username, and the channel the invite leads to.
    pub name: String,
    pub channel: String,
    pub uses: String,
    /// Seconds left when the page was read; negative never expires.
    pub expires_in: i64,
    /// The highest role it gives, its colour, and "+2" for the rest.
    pub role: String,
    pub role_color: Color,
    pub more_roles: String,
}

impl Row {
    /// Seconds left after `ticks` seconds on the page, `None` for never.
    pub fn left(&self, ticks: u32) -> Option<i64> {
        (self.expires_in >= 0).then(|| (self.expires_in - i64::from(ticks)).max(0))
    }
}

/// A server's Invites settings: what its screen draws. The window holds it as
/// `vm.settings.invites`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Invites {
    pub rows: Vec<Row>,
    /// Invites are paused: the rows dim, and the button enables them again.
    pub paused: bool,
    pub may_create: bool,
    /// When `rows` came: the countdowns run from it.
    pub since: Option<Instant>,
    /// Whole seconds from `since` to the last tick. The app ticks once a
    /// second only while the window is in front and `wants_ticks`; the first
    /// tick back counts the seconds spent behind, as the Slint page did.
    pub ticks: u32,
    /// The code of the row under the pointer: its remove button shows.
    pub hovered: Option<String>,
}

impl Invites {
    /// Whether anything counts down: the app subscribes to a one second
    /// clock only then, and only while the window is in front.
    pub fn wants_ticks(&self) -> bool {
        self.rows
            .iter()
            .any(|row| row.left(self.ticks).is_some_and(|left| left > 0))
    }

    /// A page of invites read at `now`: the countdowns start over.
    pub fn set_rows(&mut self, rows: Vec<Row>, now: Instant) {
        self.rows = rows;
        self.since = Some(now);
        self.ticks = 0;
    }

    /// The clock read `now` in front ([`Intent::Tick`]).
    pub fn tick(&mut self, now: Instant) {
        if let Some(since) = self.since {
            let seconds = now.saturating_duration_since(since).as_secs();
            self.ticks = u32::try_from(seconds).unwrap_or(u32::MAX);
        }
    }

    /// The pointer entered (`inside`) or left the row with `code`
    /// ([`Intent::Hover`]). Leaving clears only that row's hover: crossing
    /// to the row above enters it before this one is left.
    pub fn hover(&mut self, code: String, inside: bool) {
        if inside {
            self.hovered = Some(code);
        } else if self.hovered.as_ref() == Some(&code) {
            self.hovered = None;
        }
    }
}

/// Days (if any), hours, minutes and seconds left, "01:02:03:04", as
/// Discord's invites count down; `None` is "∞".
pub fn countdown(left: Option<i64>) -> String {
    let Some(left) = left else {
        return "∞".into();
    };
    let (days, hours) = (left / 86_400, left / 3_600 % 24);
    let (minutes, seconds) = (left / 60 % 60, left % 60);
    if days > 0 {
        format!("{days:02}:{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }
}

/// A server's Invites settings: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Revoke the invite with this code.
    Revoke(String),
    /// Create an invite link.
    Create,
    /// Open Security Actions (to pause or enable invites).
    SecurityActions,
    /// The pointer entered (`true`) or left (`false`) the row with this
    /// code: [`Invites::hover`].
    Hover(String, bool),
    /// The one second clock, while the window is in front and
    /// [`Invites::wants_ticks`]: [`Invites::tick`].
    Tick(Instant),
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn counts_down_in_columns() {
        assert_eq!(countdown(None), "∞");
        assert_eq!(countdown(Some(3_723)), "01:02:03");
        assert_eq!(countdown(Some(90_061)), "01:01:01:01");
    }

    #[test]
    fn holds_at_zero_and_stops_wanting_ticks() {
        let start = Instant::now();
        let mut page = Invites::default();
        page.set_rows(
            vec![Row {
                expires_in: 2,
                ..Row::default()
            }],
            start,
        );
        assert!(page.wants_ticks());
        page.tick(start + Duration::from_secs(1));
        assert_eq!(page.rows[0].left(page.ticks), Some(1));
        page.tick(start + Duration::from_secs(3));
        assert_eq!(page.rows[0].left(page.ticks), Some(0));
        assert!(!page.wants_ticks());
    }

    #[test]
    fn the_first_tick_back_counts_the_time_behind() {
        let start = Instant::now();
        let mut page = Invites::default();
        page.set_rows(vec![Row::default()], start);
        page.tick(start + Duration::from_millis(600_400));
        assert_eq!(page.ticks, 600);
    }

    #[test]
    fn leaving_a_row_keeps_the_one_entered_first() {
        let mut page = Invites::default();
        page.hover("lower".into(), true);
        page.hover("upper".into(), true);
        page.hover("lower".into(), false);
        assert_eq!(page.hovered.as_deref(), Some("upper"));
        page.hover("upper".into(), false);
        assert_eq!(page.hovered, None);
    }
}
