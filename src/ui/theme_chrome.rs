//! A theme's `chrome` on the window, through the Slint global `Chrome`
//! (`ui/theme_chrome.slint`). See `docs/themes.md`.
//!
//! [`reset`] puts every value back to stock, which `theme_look::reset` calls
//! first whenever a theme is drawn, cleared or hidden behind a protected
//! screen. A theme without `chrome` leaves every value at its default, so the
//! window draws its stock text and the fill callback is never asked.

use slint::{Color, ComponentHandle, SharedString};
use yapfast_core::themes::chrome::{
    Caret, Chrome as Words, Hide, Label, Place, Presence, TreeList, fill, split_group, split_typing,
};
use yapfast_ui::{Chrome, YapfastWindow};

/// Answers `Chrome.fill`. Called once per window.
pub fn init(window: &YapfastWindow) {
    window
        .global::<Chrome>()
        .on_fill(|label, kind, text| filled(&label, &kind, &text));
}

/// A label with its placeholders filled from what the interface passes.
fn filled(label: &str, kind: &str, text: &str) -> SharedString {
    let out = match kind {
        "group" => {
            let (role, count) = split_group(text);
            fill(label, &[("role", role), ("n", count)])
        }
        "typing" => {
            let (names, count) = split_typing(text);
            fill(label, &[("name", &names), ("n", &count.to_string())])
        }
        "channel" | "server" | "name" | "n" => fill(label, &[(kind, text)]),
        _ => fill(label, &[]),
    };
    out.into()
}

/// Every value back to stock.
pub fn reset(window: &YapfastWindow) {
    let ui = window.global::<Chrome>();
    for label in Label::ALL {
        set_label(&ui, *label, "");
    }
    ui.set_reaction("(+{n})".into());
    for place in Place::ALL {
        set_mark(&ui, *place, "", "");
    }
    ui.set_time_before(SharedString::new());
    ui.set_time_after(SharedString::new());
    for hidden in Hide::ALL {
        set_hidden(&ui, *hidden, false);
    }
    ui.set_rail_text(false);
    ui.set_rail_width(150.);
    ui.set_caret_block(false);
    ui.set_prompt(SharedString::new());
    ui.set_status_column(false);
    for presence in Presence::ALL {
        set_status(&ui, *presence, "");
    }
    for list in TreeList::ALL {
        set_tree(&ui, *list, false);
    }
    ui.set_tree_token(0);
    ui.set_tree_colour_set(Color::default());
    ui.set_folders(false);
}

/// Sets a theme's chrome.
pub fn apply(window: &YapfastWindow, words: &Words) {
    if words.is_stock() {
        return;
    }
    let ui = window.global::<Chrome>();
    for (label, text) in &words.labels {
        set_label(&ui, *label, text);
    }
    for place in Place::ALL {
        let before = words.prefix.get(place).map_or("", String::as_str);
        let after = words.suffix.get(place).map_or("", String::as_str);
        set_mark(&ui, *place, before, after);
    }
    if let Some((before, after)) = &words.time {
        ui.set_time_before(before.as_str().into());
        ui.set_time_after(after.as_str().into());
    }
    for hidden in &words.hide {
        set_hidden(&ui, *hidden, true);
    }
    ui.set_rail_text(words.rail_text());
    if let Some(width) = words.rail_width {
        ui.set_rail_width(width);
    }
    ui.set_caret_block(words.caret == Some(Caret::Block));
    if let Some(prompt) = &words.prompt {
        ui.set_prompt(prompt.as_str().into());
    }
    if let Some(folders) = &words.folders {
        ui.set_folders(true);
        ui.set_folder_open(folders.open.as_str().into());
        ui.set_folder_closed(folders.closed.as_str().into());
        ui.set_folder_suffix(folders.suffix.as_str().into());
    }
    for list in &words.tree {
        set_tree(&ui, *list, true);
    }
    if let Some(ink) = words.tree_ink {
        ui.set_tree_token(ink.index());
    }
    if let Some(colour) = words.tree_colour {
        ui.set_tree_colour_set(Color::from_argb_encoded(colour));
    }
    if !words.status.is_empty() {
        ui.set_status_column(true);
        for (presence, text) in &words.status {
            set_status(&ui, *presence, text);
        }
    }
}

fn set_label(ui: &Chrome<'_>, label: Label, text: &str) {
    let text: SharedString = text.into();
    match label {
        Label::MembersTitle => ui.set_members_title(text),
        Label::MembersGroup => ui.set_members_group(text),
        Label::Category => ui.set_category(text),
        Label::Composer => ui.set_composer(text),
        Label::New => ui.set_new_label(text),
        Label::Typing => ui.set_typing(text),
        Label::Welcome => ui.set_welcome(text),
        Label::Start => ui.set_start(text),
        Label::RailTitle => ui.set_rail_title(text),
        Label::RailFooter => ui.set_rail_footer(text),
        Label::RailHome => ui.set_rail_home(text),
        Label::Reaction => ui.set_reaction(text),
        Label::TreeRoot => ui.set_tree_root(text),
    }
}

fn set_mark(ui: &Chrome<'_>, place: Place, before: &str, after: &str) {
    let (before, after): (SharedString, SharedString) = (before.into(), after.into());
    match place {
        Place::Channel => {
            ui.set_channel_before(before);
            ui.set_channel_after(after);
        }
        Place::Server => {
            ui.set_server_before(before);
            ui.set_server_after(after);
        }
        Place::Member => {
            ui.set_member_before(before);
            ui.set_member_after(after);
        }
        Place::Author => {
            ui.set_author_before(before);
            ui.set_author_after(after);
        }
        Place::Header => {
            ui.set_header_before(before);
            ui.set_header_after(after);
        }
    }
}

fn set_hidden(ui: &Chrome<'_>, hidden: Hide, on: bool) {
    match hidden {
        Hide::MemberAvatars => ui.set_hide_member_avatars(on),
        Hide::ChannelIcons => ui.set_hide_channel_icons(on),
        Hide::HeaderButtons => ui.set_hide_header_buttons(on),
        // The rail's text mode: see `Words::rail_text`.
        Hide::ServerIcons => {}
        Hide::AccountAvatar => ui.set_hide_account_avatar(on),
        Hide::ActivityCards => ui.set_hide_activity_cards(on),
        Hide::ReactionChips => ui.set_reaction_text(on),
    }
}

fn set_tree(ui: &Chrome<'_>, list: TreeList, on: bool) {
    match list {
        TreeList::Channels => ui.set_tree_channels(on),
        TreeList::Members => ui.set_tree_members(on),
        TreeList::Dms => ui.set_tree_dms(on),
        TreeList::Rail => ui.set_tree_rail(on),
        TreeList::Replies => ui.set_tree_replies(on),
    }
}

fn set_status(ui: &Chrome<'_>, presence: Presence, text: &str) {
    let text: SharedString = text.into();
    match presence {
        Presence::Online => ui.set_status_online(text),
        Presence::Idle => ui.set_status_idle(text),
        Presence::Dnd => ui.set_status_dnd(text),
        Presence::Offline => ui.set_status_offline(text),
        Presence::Streaming => ui.set_status_streaming(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_what_the_interface_passes() {
        assert_eq!(filled("{role} ({n})", "group", "Leads — 2"), "Leads (2)");
        assert_eq!(filled("$ {channel}", "channel", "general"), "$ general");
        assert_eq!(
            filled(
                "{name} typing [{n}]",
                "typing",
                "Sam and Riley are typing..."
            ),
            "Sam and Riley typing [2]"
        );
        assert_eq!(filled("pts/{n}", "n", "3"), "pts/3");
        assert_eq!(
            filled("~ $ cd {server}", "server", "yapfast"),
            "~ $ cd yapfast"
        );
    }
}
