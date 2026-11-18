//! A server's categories as its list shows them: folded as it was last left
//! (`fold_collapsed`, `src/ui/refresh.rs:5593`), each empty one as its
//! heading alone where Discord's order puts it (`with_empty_categories`,
//! `:5633`), and the places its channels and categories drag by
//! (`show_sidebar`, `:1507`-`:1530`).

use std::collections::BTreeSet;

use yapfast_core::application::ListedCategory;

use anomi_vm::sidebar::channels::ChannelRow;

/// Rows under a folded category: only the selected channel, unread ones,
/// voice channels with people in them and the call's stay, as in Discord. A
/// category with none of those keeps its heading alone, to open it again.
pub(super) fn fold_collapsed(
    rows: Vec<ChannelRow>,
    collapsed: &BTreeSet<String>,
    call: Option<&str>,
) -> Vec<ChannelRow> {
    let mut group = String::new();
    let mut folded = Vec::with_capacity(rows.len());
    for mut row in rows {
        if !row.heading.is_empty() {
            group.clone_from(&row.category);
        }
        if group.is_empty() || !collapsed.contains(&group) {
            folded.push(row);
            continue;
        }
        row.collapsed = true;
        if row.selected || row.unread || !row.people.is_empty() || call == Some(row.id.as_str()) {
            folded.push(row);
        } else if !row.heading.is_empty() {
            // Its id still names the category's first channel, for the
            // heading's menu.
            folded.push(ChannelRow {
                id: row.id,
                heading: row.heading,
                category: row.category,
                collapsed: true,
                hidden: true,
                ..ChannelRow::default()
            });
        }
    }
    folded
}

/// Rows with a heading alone for each of a server's `categories` that holds
/// no channels, where Discord's order puts it among those that do.
pub(super) fn with_empty_categories(
    rows: Vec<ChannelRow>,
    categories: &[ListedCategory],
) -> Vec<ChannelRow> {
    if !categories.iter().any(|category| category.empty) {
        return rows;
    }
    // Its id names the category, for the heading's menu.
    let heading = |category: &ListedCategory| ChannelRow {
        id: category.id.clone(),
        heading: category.name.clone(),
        category: category.id.clone(),
        hidden: true,
        ..ChannelRow::default()
    };
    let mut next = 0;
    let mut listed = Vec::with_capacity(rows.len() + categories.len());
    for row in rows {
        if !row.heading.is_empty()
            && let Some(at) = categories[next..]
                .iter()
                .position(|known| known.id == row.category)
        {
            listed.extend(
                categories[next..next + at]
                    .iter()
                    .filter(|category| category.empty)
                    .map(heading),
            );
            next += at + 1;
        }
        listed.push(row);
    }
    listed.extend(
        categories[next..]
            .iter()
            .filter(|category| category.empty)
            .map(heading),
    );
    listed
}

/// Numbers the rows after the pinned ones by the places they drag by and
/// take drops at, from 1: each category's heading, and each channel that
/// shows outside a folded one, as Discord's client lists them to drop on.
/// Threads stay where they are. The room ids the places stand for go to
/// `places`.
pub(super) fn number(rows: &mut [ChannelRow], pinned: usize, places: &mut Vec<String>) {
    for row in rows.iter_mut().skip(pinned) {
        if !row.heading.is_empty() {
            places.push(row.category.clone());
            row.heading_place = places.len() as u32;
        }
        if !row.hidden && !row.thread() && !row.collapsed {
            places.push(row.id.clone());
            row.place = places.len() as u32;
        }
    }
}
