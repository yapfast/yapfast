//! A theme's `chrome` on the window, through the Slint global `Chrome`
//! (`ui/theme_chrome.slint`). See `docs/themes.md`.
//!
//! [`reset`] puts every value back to stock, which `theme_look::reset` calls
//! first whenever a theme is drawn, cleared or hidden behind a protected
//! screen. A theme without `chrome` leaves every value at its default, so the
//! window draws its stock text and the fill callback is never asked.

use slint::{Color, ComponentHandle, SharedString};
use yapfast_core::themes::chrome::{
    BOX_CHARS, BoxFrame, Caret, Chrome as Words, CountInk, Hide, Label, Pane, Place, Presence,
    TreeList, fill, split_group, split_typing,
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
        "channel" | "server" | "name" | "n" | "date" => fill(label, &[(kind, text)]),
        // A box title: the pane's name and its count, on two lines.
        "box" => {
            let (name, count) = text.split_once('\n').unwrap_or((text, ""));
            fill(label, &[("name", name), ("n", count)])
        }
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
    ui.set_count_mentions(SharedString::new());
    ui.set_count_unread(SharedString::new());
    ui.set_count_token(0);
    set_box(&ui, None);
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
        ui.set_prompt(super::theme_trim::kept(prompt).into());
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
    if let Some(counts) = &words.counts {
        ui.set_count_mentions(super::theme_trim::kept(&counts.mentions).into());
        ui.set_count_unread(super::theme_trim::kept(&counts.unread).into());
        ui.set_count_token(match counts.ink.unwrap_or(CountInk::Danger) {
            CountInk::Danger => 0,
            CountInk::Loud => 1,
            CountInk::Accent => 2,
            CountInk::Text => 3,
            CountInk::Secondary => 4,
            CountInk::Muted => 5,
        });
    }
    set_box(&ui, words.box_frame.as_ref());
    if !words.status.is_empty() {
        ui.set_status_column(true);
        for (presence, text) in &words.status {
            set_status(&ui, *presence, text);
        }
    }
}

/// How long the box's across and down lines run before each pane clips
/// them: past the widest and tallest window at the smallest face.
const BOX_ACROSS: usize = 1200;
const BOX_DOWN: usize = 400;

/// A theme's box frames, or none.
fn set_box(ui: &Chrome<'_>, frame: Option<&BoxFrame>) {
    let on = |pane| frame.is_some_and(|frame| frame.panes.contains(&pane));
    ui.set_box_chat(on(Pane::Chat));
    ui.set_box_sidebar(on(Pane::Sidebar));
    ui.set_box_members(on(Pane::Members));
    let chars = frame.map_or(BOX_CHARS, |frame| frame.chars);
    let text = |c: char| SharedString::from(c.to_string());
    ui.set_box_tl(text(chars[0]));
    ui.set_box_tr(text(chars[1]));
    ui.set_box_bl(text(chars[2]));
    ui.set_box_br(text(chars[3]));
    let Some(frame) = frame else {
        ui.set_box_across(SharedString::new());
        ui.set_box_down(SharedString::new());
        ui.set_box_title_chat(SharedString::new());
        ui.set_box_title_sidebar(SharedString::new());
        ui.set_box_title_members(SharedString::new());
        ui.set_box_size(14.);
        ui.set_box_token(-1);
        ui.set_box_colour_set(Color::default());
        return;
    };
    ui.set_box_across(chars[4].to_string().repeat(BOX_ACROSS).into());
    ui.set_box_down(vec![chars[5].to_string(); BOX_DOWN].join("\n").into());
    let title = |pane| SharedString::from(super::theme_trim::kept(frame.title_for(pane)));
    ui.set_box_title_chat(title(Pane::Chat));
    ui.set_box_title_sidebar(title(Pane::Sidebar));
    ui.set_box_title_members(title(Pane::Members));
    ui.set_box_size(frame.size);
    ui.set_box_token(frame.ink.map_or(-1, |ink| ink.index()));
    ui.set_box_colour_set(
        frame
            .colour
            .map_or(Color::default(), Color::from_argb_encoded),
    );
}

fn set_label(ui: &Chrome<'_>, label: Label, text: &str) {
    let text: SharedString = super::theme_trim::kept(text).into();
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
        Label::Day => ui.set_day(text),
        // Trim's: see `theme_trim::apply`.
        Label::Topic | Label::MemberCount => {}
    }
}

fn set_mark(ui: &Chrome<'_>, place: Place, before: &str, after: &str) {
    let (before, after): (SharedString, SharedString) = (
        super::theme_trim::kept(before).into(),
        super::theme_trim::kept(after).into(),
    );
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
    fn theme_box_frames_and_counts_reach_the_window_and_go() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let words = yapfast_core::themes::package::Package::parse(
            r##"{ "theme_api": 1, "id": "com.example.box", "name": "Box", "version": "1.0.0",
                "chrome": { "box": { "panes": ["chat", "members"], "titles": { "members": "[ who: {n} ]" }, "colour": "muted" },
                            "tree": { "counts": { "mentions": "[{n}!]", "unread": "*", "colour": "loud" } } },
                "variants": { "dark": { "base": "yap" } } }"##,
        )
        .unwrap()
        .resolve(yapfast_core::themes::package::Mode::Dark)
        .unwrap()
        .chrome;
        apply(&window, &words);
        let ui = window.global::<Chrome>();
        assert!(ui.get_box_chat() && ui.get_box_members() && !ui.get_box_sidebar());
        assert_eq!(ui.get_box_title_chat(), "[ {name} ]");
        assert_eq!(ui.get_box_title_members(), "[ who: {n} ]");
        assert_eq!(ui.get_box_across().chars().count(), BOX_ACROSS);
        assert!(ui.get_box_across().chars().all(|c| c == '─'));
        assert_eq!(ui.get_box_down().lines().count(), BOX_DOWN);
        assert_eq!(ui.get_box_token(), 1);
        assert_eq!(ui.get_count_mentions(), "[{n}!]");
        assert_eq!(ui.get_count_token(), 1);
        reset(&window);
        assert!(!ui.get_box_chat() && !ui.get_box_members());
        assert!(ui.get_box_across().is_empty() && ui.get_count_mentions().is_empty());
        assert_eq!(ui.get_box_token(), -1);
    }

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
        assert_eq!(filled("[ {name} ]", "box", "#general\n"), "[ #general ]");
        assert_eq!(filled("[ who: {n} ]", "box", "members\n7"), "[ who: 7 ]");
        assert_eq!(filled("[{n}!]", "n", "3"), "[3!]");
        assert_eq!(
            filled("~ $ cd {server}", "server", "yapfast"),
            "~ $ cd yapfast"
        );
    }
}
