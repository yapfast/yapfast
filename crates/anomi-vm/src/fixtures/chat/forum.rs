//! Samples of a forum for tests and shots, after the fake Discord's data
//! (`testing/fake_discord.ts`, the forum "ideas"): task C26's. `forum()` and
//! the others are the view-model alone; `window(..)` is the whole window on
//! the first server with the forum open.

use iced::Color;
use yapfast_lang::tr;

use crate::Vm;
use crate::chat::forum::{Forum, Mark, Post, Reaction, Tag};
use crate::conversation::{Beside, Body, Conversation};
use crate::fixtures::{pictures, shell};
use crate::header::{Channel, Header, Place, Title, Tools};
use crate::icon::Icon;
use crate::kind::Expected;
use crate::sidebar;

/// A tag with a Unicode emoji, or with a custom one's picture.
fn tag(name: &str, mark: Mark) -> Tag {
    Tag {
        name: name.into(),
        mark,
    }
}

fn emoji(text: &str) -> Mark {
    Mark::Unicode(text.into())
}

/// A custom emoji, as the fake draws it, 24 pixels for the 12 drawn.
fn custom(id: &str) -> Mark {
    Mark::Custom(Expected::Ready(pictures::badge(id, 24)))
}

fn thumbs(count: &str) -> Option<Reaction> {
    Some(Reaction {
        mark: emoji("\u{1F44D}"),
        count: count.into(),
    })
}

fn post(room: &str, name: &str, person: &str, text: &str) -> Post {
    Post {
        room: room.into(),
        name: name.into(),
        person: person.into(),
        text: text.into(),
        reaction: thumbs(""),
        replies: "0".into(),
        time: tr!(">30d ago").into(),
        ..Post::default()
    }
}

/// Bob's and Carol's role colours, as the fake's roles give them.
const BOB: Color = Color::from_rgb(0.20, 0.60, 0.86);

fn older(index: u32) -> Post {
    Post {
        tags: vec![
            tag("Feature", emoji("\u{1F680}")),
            tag("Done", custom("1400000000000000300")),
        ],
        ..post(
            &format!("discord:p{index}"),
            &format!("Old idea {index}"),
            "Carol",
            &format!("Idea {index}, done long ago."),
        )
    }
}

/// The forum "ideas" as the fake serves it: a pinned post, three active
/// ones, one new, one with a picture, and older ones under their heading.
pub fn forum() -> Forum {
    let mut rows = vec![
        Post {
            pinned: true,
            color: Some(BOB),
            ..post(
                "discord:p1",
                "Welcome to #ideas",
                "Bob",
                "Read the guidelines before posting.",
            )
        },
        Post {
            new: true,
            tags: vec![tag("Question", emoji("\u{2753}"))],
            color: Some(BOB),
            ..post(
                "discord:p2",
                "Keyboard shortcut list?",
                "Bob",
                "Is there a list of the shortcuts somewhere?",
            )
        },
        Post {
            tags: vec![
                tag("Bug", emoji("\u{1F41B}")),
                tag("Question", emoji("\u{2753}")),
            ],
            replies: "1".into(),
            picture: Expected::Ready(pictures::sunset(144, 144)),
            ..post(
                "discord:p3",
                "Crash when pasting a GIF",
                "Dave",
                "Pasting a GIF from the browser closes the app.",
            )
        },
        Post {
            tags: vec![tag("Feature", emoji("\u{1F680}"))],
            reaction: thumbs("3"),
            replies: "2".into(),
            ..post(
                "discord:p4",
                "Dark theme for the sidebar",
                "Carol",
                "The sidebar could follow the system theme.",
            )
        },
    ];
    let mut rest: Vec<Post> = (1..=4).map(older).collect();
    rest[0].heading = tr!("OLDER POSTS").into();
    rows.extend(rest);
    Forum {
        rows,
        searchable: true,
        can_post: true,
        more: true,
        ..Forum::default()
    }
}

/// Posts being fetched for the first time.
pub fn loading() -> Forum {
    Forum {
        note: tr!("Loading posts...").into(),
        ..Forum {
            rows: Vec::new(),
            ..forum()
        }
    }
}

/// An older page on its way, under the last card.
pub fn loading_more() -> Forum {
    Forum {
        loading: true,
        ..forum()
    }
}

/// A forum with no posts.
pub fn empty() -> Forum {
    Forum {
        rows: Vec::new(),
        empty: true,
        ..forum()
    }
}

/// A search that found nothing.
pub fn nothing_found() -> Forum {
    Forum {
        rows: Vec::new(),
        empty: true,
        searching: true,
        query: "zebra".into(),
        ..forum()
    }
}

/// A search that found two posts, a thumbnail spoiled, with its text in the
/// field.
pub fn found() -> Forum {
    let mut rows: Vec<Post> = forum().rows.into_iter().skip(1).take(2).collect();
    rows[1].spoiler = true;
    Forum {
        rows,
        searching: true,
        query: "crash".into(),
        more: false,
        ..forum()
    }
}

/// A search that failed, with why.
pub fn failed() -> Forum {
    Forum {
        rows: Vec::new(),
        note: "429 Too Many Requests".into(),
        ..forum()
    }
}

/// An account that may read and not post: no "New Post", the posts of
/// `forum()`.
pub fn read_only() -> Forum {
    Forum {
        can_post: false,
        ..forum()
    }
}

/// A forum without the right to search its titles, and a post with a long
/// title and three tags and two more.
pub fn plain() -> Forum {
    let mut forum = forum();
    forum.searchable = false;
    // The NEW post, so the shot shows the title cut short and the pill kept.
    let long = &mut forum.rows[1];
    long.name = "A title long enough that it cannot fit the card at any width a window has and so it must be cut short with the NEW pill kept right after it, and then some more words to be sure of it, and a few more besides".into();
    long.tags = ["Bug", "Question", "Feature"]
        .map(|name| tag(name, emoji("\u{2753}")))
        .into();
    long.more_tags = "+2".into();
    forum
}

/// The whole window on the first server with the forum "ideas" open.
pub fn window(forum: Forum) -> Vm {
    let mut vm = shell::server_home();
    if let sidebar::Place::Server(server) = &mut vm.sidebar.place {
        for row in &mut server.channels.rows {
            row.selected = row.name == "ideas";
            row.unread = false;
        }
    }
    vm.header = Header::Open(Box::new(Place {
        title: Title::Channel(Channel {
            icon: Icon::MessageSquareText,
            name: "ideas".into(),
            ..Channel::default()
        }),
        tools: Tools {
            notify: Some(false),
            search: true,
            ..Tools::default()
        },
    }));
    vm.conversation = Conversation {
        body: Body::Forum,
        beside: Beside::default(),
        ..Conversation::default()
    };
    vm.chat.forum = forum;
    vm
}

/// The window with the forum of `forum()`.
pub fn open() -> Vm {
    window(forum())
}
