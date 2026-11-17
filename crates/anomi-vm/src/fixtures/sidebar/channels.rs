//! Samples of the channel list. Task N04's.

use crate::sidebar::channels::{ChannelKind, ChannelRow, Channels};

/// The first server's channels, #general open and the forum unread.
pub fn channels() -> Channels {
    let channel = |id: &str, name: &str, heading: &str, kind| ChannelRow {
        id: id.into(),
        name: name.into(),
        heading: heading.into(),
        kind,
        ..ChannelRow::default()
    };
    let mut rows = vec![
        channel("discord:c1", "general", "Channels", ChannelKind::Text),
        channel("discord:c2", "Plans", "", ChannelKind::Thread),
        channel("discord:c3", "ideas", "", ChannelKind::Forum),
        channel("discord:c4", "Lounge", "", ChannelKind::Voice),
    ];
    rows[0].selected = true;
    rows[2].unread = true;
    Channels { rows }
}
