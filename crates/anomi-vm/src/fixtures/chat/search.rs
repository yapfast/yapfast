//! Samples of searching messages for tests and shots, after the fake
//! Discord's data: task C21's. Its history of "Message N" posts, Bob's, found
//! by a search for "Message": 55 of them, the newest first.

use iced::Color;

use crate::Vm;
use crate::chat::search::{Answer, Cell, Found, Poll, Search};
use crate::fixtures::{avatar, conversation, pictures, shell, text};
use crate::kind::Expected;
use yapfast_lang::{tr, trn};

/// How many "Message" finds the fake's history holds.
pub const FOUND: usize = 55;

/// Bob's role colour in the fake's server.
const BLUE: Color = Color::from_rgb(0.2, 0.596, 0.859);

/// The `index`th message of the fake's history as a search finds it: Bob's,
/// at a minute apiece.
pub fn found(index: usize) -> Found {
    Found {
        item: format!("14000000000001{index:04}"),
        room: "1400000000000000200".into(),
        author: "Bob".into(),
        avatar: pictures::pictured(avatar("B", 0xc06c84), "Bob"),
        color: Some(BLUE),
        when: format!("01.10.2026 16:{:02}", index + 4),
        body: Some(text::history(index)),
        ..Found::default()
    }
}

/// The first `count` finds of the history, newest first, the first under
/// the channel's name as a server's results are.
pub fn results(count: usize) -> Vec<Found> {
    (0..count)
        .map(|at| Found {
            heading: if at == 0 {
                "general".into()
            } else {
                String::new()
            },
            ..found(FOUND - at)
        })
        .collect()
}

/// A find with two pictures, one a spoiler and one a video, in a gallery of
/// 240 by 120.
pub fn pictures_found() -> Found {
    let cell = |x, picture: Expected, spoiler, video| Cell {
        x,
        y: 0.0,
        width: 118.0,
        height: 120.0,
        picture,
        spoiler,
        video,
    };
    Found {
        body: Some(text::history(0)),
        images: vec![
            cell(
                0.0,
                Expected::Ready(pictures::sunset(118, 120)),
                true,
                false,
            ),
            cell(
                122.0,
                Expected::Ready(pictures::sunset(118, 120)),
                false,
                true,
            ),
        ],
        gallery: (240.0, 120.0),
        ..found(9)
    }
}

/// A find with pictures still on their way: skeletons at their size.
pub fn pictures_waiting() -> Found {
    Found {
        images: vec![Cell {
            width: 240.0,
            height: 120.0,
            picture: Expected::Waiting,
            ..Cell::default()
        }],
        gallery: (240.0, 120.0),
        ..found(8)
    }
}

/// A find that is only files, named.
pub fn files_found() -> Found {
    Found {
        body: None,
        files: "release-notes.txt, report.pdf".into(),
        ..found(7)
    }
}

/// A find that asks a poll, its results showing.
pub fn poll_found() -> Found {
    let answer = |text: &str, votes: &str, percent, picked, leading| Answer {
        text: text.into(),
        votes: votes.into(),
        percent,
        picked,
        leading,
        ..Answer::default()
    };
    Found {
        body: None,
        poll: Some(Poll {
            question: "Where do we meet?".into(),
            multiselect: false,
            results: true,
            answers: vec![
                answer("The usual place", "3 votes", 75, true, true),
                answer("Somewhere new", "1 vote", 25, false, false),
            ],
            total: "4 votes".into(),
            left: "Poll closed".into(),
            ..Poll::default()
        }),
        ..found(6)
    }
}

/// The panel with the history's results.
pub fn searching() -> Search {
    Search {
        text: "Message".into(),
        placeholder: tr!("Search {}", "Server"),
        heading: trn!("{n} Result", "{n} Results", FOUND),
        results: results(12),
        more: true,
        ..Search::default()
    }
}

/// A search on its way: no results yet.
pub fn waiting() -> Search {
    bare(Search {
        heading: tr!("Searching...").into(),
        ..searching()
    })
}

/// A search that found nothing, and says so.
pub fn nothing() -> Search {
    bare(Search {
        text: "zzz".into(),
        heading: trn!("{n} Result", "{n} Results", 0),
        note: tr!("Nothing in this server matches. Try other words.").into(),
        ..searching()
    })
}

/// A few results with the next page on its way: the disabled "Loading..."
/// button is in view.
pub fn loading() -> Search {
    Search {
        results: results(3),
        more: true,
        loading: true,
        ..searching()
    }
}

/// A few results and "Show more" in view.
pub fn more() -> Search {
    Search {
        results: results(3),
        more: true,
        ..searching()
    }
}

/// Every kind of find, one under another.
pub fn kinds() -> Search {
    Search {
        results: vec![
            results(1).remove(0),
            pictures_found(),
            pictures_waiting(),
            files_found(),
            poll_found(),
        ],
        more: false,
        ..searching()
    }
}

/// `search` before any result came.
fn bare(search: Search) -> Search {
    Search {
        results: Vec::new(),
        more: false,
        ..search
    }
}

/// The window on #general with the search beside it, as the golden
/// `header/search_results.png`.
pub fn beside() -> Vm {
    beside_with(searching())
}

/// The window with `search` beside the channel.
pub fn beside_with(search: Search) -> Vm {
    let mut vm = shell::channel_with(conversation::searching);
    vm.chat.search = search;
    vm
}
