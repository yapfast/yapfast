//! A Community server's Browse Channels: the rows whose channel's name has
//! its search in it, each under its category's heading (`browse_rows`,
//! `src/ui/refresh.rs:124`).

use yapfast_core::application::{AppState, BrowseChannels, BrowsedChannel, BrowsedKind};

use anomi_vm::browse::{Browse, Kind, Row};

/// What the rows were built from: the core's channels, by their hash, and
/// the search.
#[derive(Default)]
pub struct Shown {
    key: Option<(u64, String)>,
}

pub(super) fn show(state: &AppState, browse: &mut Browse, shown: &mut Shown) {
    let channels = state.browse.as_ref().map_or(0, super::fingerprint);
    if shown
        .key
        .as_ref()
        .is_some_and(|(known, filter)| *known == channels && *filter == browse.filter)
    {
        return;
    }
    browse.rows = state
        .browse
        .as_ref()
        .map_or_else(Vec::new, |known| rows(known, &browse.filter));
    shown.key = Some((channels, browse.filter.clone()));
}

fn rows(browse: &BrowseChannels, filter: &str) -> Vec<Row> {
    let query = filter.trim().to_lowercase();
    let row = |channel: &BrowsedChannel| Row {
        id: channel.id.clone(),
        name: channel.name.clone(),
        topic: channel.topic.clone(),
        kind: match channel.kind {
            BrowsedKind::Category => Kind::Category,
            BrowsedKind::Text => Kind::Text,
            BrowsedKind::Announcement => Kind::Announcement,
            BrowsedKind::Voice => Kind::Voice,
            BrowsedKind::Forum => Kind::Forum,
        },
        followed: channel.followed,
        by_category: channel.by_category,
    };
    let mut rows = Vec::new();
    let mut heading = None;
    for channel in &browse.rows {
        if channel.kind == BrowsedKind::Category {
            heading = Some(channel);
            continue;
        }
        if !channel.name.to_lowercase().contains(&query) {
            continue;
        }
        if let Some(category) = heading.take() {
            rows.push(row(category));
        }
        rows.push(row(channel));
    }
    rows
}
