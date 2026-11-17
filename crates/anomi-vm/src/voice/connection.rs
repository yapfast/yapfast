//! A call's connection details: what the popout shows and what it asks for
//! (`CallLinkCard`, `ui/voice.slint:206`; `src/ui/call_link.rs`). The call
//! measures all along; the app copies its numbers in once a second while
//! the popout shows, and nothing otherwise, as `CallLink.read()` did.

use iced::Point;

use crate::menu::Spot;

/// The graph's box, as the card draws its path in, and the pings it spans:
/// five minutes of them, as the call keeps.
pub const GRAPH: (f32, f32) = (300.0, 100.0);
pub const SPAN: usize = 60;

/// The popout's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Connection,
    Privacy,
    Stream,
}

/// How a shared screen goes out, each value already as it reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stream {
    pub encoder: String,
    pub codec: String,
    pub picture: String,
    pub sent: String,
    pub target: String,
    pub keyframes: String,
    pub resent: String,
}

/// A call's connection details: what its popout draws. The window holds it
/// as `vm.voice.connection`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Connection {
    /// The tab picked; [`Connection::shown`] is the one that shows.
    pub tab: Tab,
    /// The pings of the last five minutes, the newest last, in milliseconds.
    pub pings: Vec<u16>,
    /// "12 ms", or empty before the first. The signal icon's tooltip too.
    pub ping: String,
    /// The average ping, or empty while there is none.
    pub average: String,
    /// "0.5%", or empty while it isn't known.
    pub loss: String,
    /// The voice privacy code in the groups of five digits it is read in:
    /// six, or none while nobody else is in the call.
    pub code: Vec<String>,
    /// How this account's screen goes out, while it is live.
    pub stream: Option<Stream>,
    /// "End-to-end encrypted" rather than "Secure connection".
    pub end_to_end: bool,
    /// The code was just copied: "Copied" for two seconds, which the app
    /// counts.
    pub copied: bool,
}

impl Connection {
    /// The tab that shows: the one picked while it is there, else
    /// Connection.
    pub fn shown(&self) -> Tab {
        match self.tab {
            Tab::Privacy if !self.code.is_empty() => Tab::Privacy,
            Tab::Stream if self.stream.is_some() => Tab::Stream,
            _ => Tab::Connection,
        }
    }

    /// Whether there are tabs to pick between.
    pub fn tabbed(&self) -> bool {
        !self.code.is_empty() || self.stream.is_some()
    }

    /// The ping at the graph's top: the smallest 1, 2 or 5 times a power
    /// of ten above them all.
    pub fn top(&self) -> u32 {
        let highest = self.pings.iter().copied().max().map_or(0, u32::from);
        [1, 2, 5]
            .into_iter()
            .flat_map(|step| (1..6).map(move |power| step * 10u32.pow(power)))
            .filter(|&top| top >= highest)
            .min()
            .unwrap_or(highest)
    }

    /// The pings as steps in the [`GRAPH`] box, the newest at the right
    /// edge: each ping holds for the step before it, so even one shows.
    pub fn steps(&self) -> Vec<Point> {
        let top = self.top() as f32;
        let step = GRAPH.0 / SPAN as f32;
        let y = |ping: u16| GRAPH.1 - f32::from(ping) / top * GRAPH.1;
        let first = SPAN.saturating_sub(self.pings.len());
        let mut points = Vec::new();
        let newest = self.pings.iter().rev().take(SPAN).rev();
        for (index, &ping) in newest.enumerate() {
            let x = (first + index + 1) as f32 * step;
            if points.is_empty() {
                points.push(Point::new(x - step, y(ping)));
            } else {
                let last = points.last().map_or(0.0, |point: &Point| point.x);
                points.push(Point::new(last, y(ping)));
            }
            points.push(Point::new(x, y(ping)));
        }
        points
    }
}

/// A call's connection details: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The signal icon or the call's status was pressed: open the popout at
    /// this spot.
    Open(Spot),
    /// The pointer came onto the signal icon or the status: read the link
    /// again, so the tooltip's ping is current (`ui/voice.slint:153`).
    Read,
    /// A tab was picked.
    Tab(Tab),
    /// Copy the voice privacy code, its groups apart by spaces; then show
    /// "Copied" for two seconds.
    Copy(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pings(pings: &[u16]) -> Connection {
        Connection {
            pings: pings.to_vec(),
            ..Connection::default()
        }
    }

    #[test]
    fn the_graph_tops_at_a_round_ping() {
        assert_eq!(pings(&[]).top(), 10);
        assert_eq!(pings(&[40, 80]).top(), 100);
        assert_eq!(pings(&[260]).top(), 500);
        assert_eq!(pings(&[1500]).top(), 2000);
    }

    #[test]
    fn the_graph_steps_to_the_right_edge() {
        assert!(pings(&[]).steps().is_empty());
        let points = pings(&[40, 80]).steps();
        let at: Vec<(f32, f32)> = points.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(
            at,
            [(290.0, 60.0), (295.0, 60.0), (295.0, 20.0), (300.0, 20.0)]
        );
    }

    #[test]
    fn a_tab_that_is_not_there_shows_connection() {
        let mut connection = Connection {
            tab: Tab::Privacy,
            ..Connection::default()
        };
        assert_eq!(connection.shown(), Tab::Connection);
        assert!(!connection.tabbed());
        connection.code = vec!["12345".into(); 6];
        assert_eq!(connection.shown(), Tab::Privacy);
        connection.tab = Tab::Stream;
        assert_eq!(connection.shown(), Tab::Connection);
        connection.stream = Some(Stream::default());
        assert_eq!(connection.shown(), Tab::Stream);
    }

    #[test]
    fn only_the_last_five_minutes_are_drawn() {
        let long: Vec<u16> = (0..100).collect();
        let points = pings(&long).steps();
        assert_eq!(points.len(), SPAN * 2);
        assert_eq!(points.last().map(|p| p.x), Some(300.0));
    }
}
