//! Samples of a Community server's Browse Channels for tests and shots, after
//! the fake Discord's data: task N14's.

use crate::browse::{Browse, Kind, Row};

fn heading(id: &str, name: &str, followed: bool) -> Row {
    Row {
        id: id.into(),
        name: name.into(),
        kind: Kind::Category,
        followed,
        ..Row::default()
    }
}

fn channel(
    id: &str,
    name: &str,
    topic: &str,
    kind: Kind,
    followed: bool,
    by_category: bool,
) -> Row {
    Row {
        id: id.into(),
        name: name.into(),
        topic: topic.into(),
        kind,
        followed,
        by_category,
    }
}

/// "Server"'s channels in the fake Discord (`testing/fake_discord.ts`
/// `guildChannels`), under its category "Channels", with one more category
/// followed whole for the locked state.
pub fn rows() -> Vec<Row> {
    vec![
        heading("c1", "Channels", false),
        channel(
            "1",
            "general",
            "Co-op nights, plans and links.",
            Kind::Text,
            true,
            false,
        ),
        channel(
            "2",
            "ideas",
            "One idea per post. Search before posting.",
            Kind::Forum,
            false,
            false,
        ),
        channel("3", "Lounge", "", Kind::Voice, false, false),
        channel("4", "Create Room", "", Kind::Voice, false, false),
        heading("c2", "Announcements", true),
        channel(
            "5",
            "news",
            "News from the team",
            Kind::Announcement,
            true,
            true,
        ),
    ]
}

/// Every channel listed, the search empty.
pub fn browse() -> Browse {
    Browse {
        rows: rows(),
        filter: String::new(),
    }
}

/// A search that matched one: its channel under its category's heading.
pub fn filtered() -> Browse {
    let rows = rows();
    Browse {
        rows: vec![rows[0].clone(), rows[1].clone()],
        filter: "gen".into(),
    }
}

/// A search nothing matched.
pub fn nothing_matches() -> Browse {
    Browse {
        rows: Vec::new(),
        filter: "zzz".into(),
    }
}

/// A server with no channels to browse.
pub fn empty() -> Browse {
    Browse::default()
}
