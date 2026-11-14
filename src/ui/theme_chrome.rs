//! A theme's `chrome` on the window, through the Slint global `Chrome`
//! (`ui/theme_chrome.slint`). See `docs/themes.md`.
//!
//! [`reset`] puts every value back to stock, which `theme_look::reset` calls
//! first whenever a theme is drawn, cleared or hidden behind a protected
//! screen. A theme without `chrome` leaves every value at its default, so the
//! window draws its stock text and the fill callback is never asked.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use slint::{Color, ComponentHandle, Model as _, ModelRc, SharedString, VecModel};
use yapfast_core::themes::chrome::{
    BOX_CHARS, BarEffect, BoxFrame, Caret, Chrome as Words, CountInk, Hide, Label, Pane, Place,
    Presence, TopBar, TreeList, Workspaces, fill, split_group, split_typing,
};
use yapfast_ui::{Chrome, Look, ThemeContent, Workspace, YapfastWindow};

/// The top bar while a theme shows one: what its workspaces are, and the
/// clock that keeps `{time}`.
struct Bar {
    count: usize,
    from: Workspaces,
    /// The direct messages' workspace's words, when it has one.
    home: Option<String>,
    playing: bool,
    clock: Option<slint::Timer>,
    model: Rc<VecModel<Workspace>>,
}

thread_local! {
    static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) };
}

/// Answers `Chrome.fill` and the other callbacks the chrome asks. Called
/// once per window.
pub fn init(window: &YapfastWindow) {
    let chrome = window.global::<Chrome>();
    chrome.on_fill(|label, kind, text| filled(&label, &kind, &text));
    chrome.on_bar(|template, name, server, channel, now, time| {
        fill(
            &template,
            &[
                ("name", name.as_str()),
                ("server", server.as_str()),
                ("channel", channel.as_str()),
                ("now_playing", now.as_str()),
                ("time", time.as_str()),
            ],
        )
        .into()
    });
    chrome.on_typed(|text, count| {
        let count = usize::try_from(count).unwrap_or(0);
        text.chars().take(count).collect::<String>().into()
    });
    window.global::<ThemeContent>().on_letter(|name| {
        yapfast_core::themes::content::initial_letter(&name)
            .unwrap_or_default()
            .into()
    });
    let weak = window.as_weak();
    window.global::<Look>().on_name_hover(move |name| {
        if let Some(window) = weak.upgrade() {
            super::theme_look::name_hovered(&window, name);
        }
    });
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
    ui.set_font(SharedString::new());
    ui.set_topbar_height(0.);
    ui.set_console(false);
    ui.set_console_typing(0);
    ui.set_console_caret(Color::default());
    // The bar's workspaces stay, so a theme shown again (as after a
    // protected dialog) has them at once; its clock stops.
    BAR.with_borrow_mut(|bar| {
        if let Some(bar) = bar {
            bar.clock = None;
        }
    });
}

/// The chrome's face, the top bar and the console composer, which need the
/// theme's faces and motion settings. `face` is the family of the type role
/// `chrome.font` names; `still` keeps the console's line and placeholder
/// from moving.
/// `still` holds what moves; `readable` (Settings → Appearance → Readable
/// text) also keeps the console's placeholder from typing itself and the top
/// bar's words from scrambling.
pub fn apply_more(
    window: &YapfastWindow,
    words: &Words,
    face: Option<&str>,
    still: bool,
    readable: bool,
) {
    let ui = window.global::<Chrome>();
    if let Some(face) = face {
        ui.set_font(face.into());
    }
    if let Some(console) = &words.console {
        ui.set_console(true);
        // Unset, each follows the theme's colour: see `ui/theme_chrome.slint`.
        ui.set_console_line_set(Color::from_argb_encoded(console.line.unwrap_or(0)));
        ui.set_console_focus_set(Color::from_argb_encoded(console.focus.unwrap_or(0)));
        ui.set_console_draw(if still { 0 } else { console.draw as i64 });
        ui.set_console_typing(if still || readable {
            0
        } else {
            console.typing as i64
        });
        ui.set_console_caret(Color::from_argb_encoded(console.caret.unwrap_or(0)));
    }
    match &words.topbar {
        Some(bar) => show_bar(window, bar, still || readable),
        None => {
            if BAR.with_borrow_mut(Option::take).is_some() {
                ui.set_workspaces(ModelRc::default());
                ui.set_now_playing(SharedString::new());
            }
        }
    }
}

fn show_bar(window: &YapfastWindow, bar: &TopBar, still: bool) {
    let ui = window.global::<Chrome>();
    ui.set_topbar_height(bar.height);
    ui.set_topbar_size(bar.size);
    ui.set_topbar_centre(bar.centre.as_str().into());
    ui.set_topbar_right(bar.right.as_str().into());
    // Unset, each follows the theme's colour: see `ui/theme_chrome.slint`.
    let set = |colour: Option<u32>| Color::from_argb_encoded(colour.unwrap_or(0));
    ui.set_topbar_ground_set(set(bar.ground));
    ui.set_topbar_ink_set(set(bar.text));
    ui.set_topbar_current_set(set(bar.current));
    ui.set_topbar_underline_set(set(bar.underline));
    ui.set_topbar_line_set(set(bar.line));
    ui.set_topbar_decrypt(bar.effect == BarEffect::Decrypt && !still);
    let kept = BAR
        .with_borrow_mut(Option::take)
        .filter(|kept| kept.count == bar.workspaces as usize && kept.from == bar.from);
    let model = kept.map_or_else(|| Rc::new(VecModel::default()), |kept| kept.model);
    ui.set_workspaces(ModelRc::from(model.clone()));
    let clock = bar.shows_time().then(|| {
        let weak = window.as_weak();
        let minute = std::cell::Cell::new(u64::MAX);
        // The time, when the minute has turned since it was last set.
        let tick = move || {
            if let Some(window) = weak.upgrade() {
                let seconds = yapfast_core::clock::now_ms() / 1000;
                if minute.replace(seconds / 60) != seconds / 60 {
                    let time =
                        yapfast_core::clock::time_on(seconds, window.get_twelve_hour_clock());
                    window.global::<Chrome>().set_time(time.into());
                }
            }
        };
        tick();
        // Looked at each second, while the window shows (see `repace`).
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), tick);
        if super::theme_pace::hidden() {
            timer.stop();
        }
        timer
    });
    BAR.set(Some(Bar {
        count: bar.workspaces as usize,
        from: bar.from,
        home: bar.home.clone(),
        playing: bar.shows_playing(),
        clock,
        model,
    }));
}

/// The window's pace changed (see `theme_pace`): the top bar's clock
/// stops while nobody sees the window, and catches up when it shows.
pub fn repace() {
    BAR.with_borrow(|bar| {
        let Some(timer) = bar.as_ref().and_then(|bar| bar.clock.as_ref()) else {
            return;
        };
        if super::theme_pace::hidden() {
            timer.stop();
        } else if !timer.running() {
            timer.restart();
        }
    });
}

/// The top bar's workspaces from the servers in the rail's order, when a
/// theme shows the bar.
pub fn workspaces(state: &yapfast_core::application::AppState) {
    BAR.with_borrow(|bar| {
        let Some(bar) = bar else { return };
        let selected = match &state.navigation {
            yapfast_core::application::Navigation::Server(id) => Some(id.as_str()),
            _ => None,
        };
        let unread_rooms: HashMap<&str, bool> = state
            .rooms
            .iter()
            .map(|room| (room.id.as_str(), room.mentions > 0 || room.unread > 0))
            .collect();
        let unread = |server: &yapfast_core::chat::ChatServer| {
            !server.muted
                && server
                    .channels
                    .iter()
                    .any(|channel| unread_rooms.get(channel.as_str()).copied().unwrap_or(false))
        };
        // Mentions waiting in a server's channels, shown beside its number.
        let mention_rooms: HashMap<&str, u64> = state
            .rooms
            .iter()
            .map(|room| (room.id.as_str(), room.mentions))
            .collect();
        let mentions = |server: &yapfast_core::chat::ChatServer| -> u64 {
            server
                .channels
                .iter()
                .filter_map(|channel| mention_rooms.get(channel.as_str()))
                .sum()
        };
        let badge = |count: u64| -> SharedString {
            match count {
                0 => SharedString::new(),
                1..=99 => count.to_string().into(),
                _ => "99+".into(),
            }
        };
        // With the rail hidden, the bar is how servers are reached: a home
        // workspace for the DMs first, and every server past the bar's
        // last behind "+N".
        let hidden = super::theme_lists::rail_hidden();
        let home = bar.home.clone().or_else(|| hidden.then(|| "~".to_owned()));
        let mut beyond = 0;
        // Each workspace: the server it opens, its name, whether it's open
        // and whether it's unread.
        let mut rows: Vec<Workspace> = Vec::new();
        if let Some(home) = &home {
            let dms = super::refresh::unread_dm_rows(state);
            let waiting: u64 = dms
                .iter()
                .filter_map(|dm| dm.badge.parse::<u64>().ok())
                .sum();
            rows.push(Workspace {
                id: "@dms".into(),
                number: home.as_str().into(),
                name: "Direct messages".into(),
                current: state.navigation == yapfast_core::application::Navigation::DirectMessages,
                unread: !dms.is_empty(),
                badge: badge(waiting),
                home: true,
                ..Workspace::default()
            });
        }
        let first = rows.len();
        let mut push =
            |id: &str, name: &str, current: bool, unread: bool, mentions: u64, folder: &str| {
                if rows.len() - first < bar.count {
                    rows.push(Workspace {
                        id: id.into(),
                        number: (rows.len() - first + 1).to_string().into(),
                        name: name.into(),
                        current,
                        unread,
                        badge: badge(mentions),
                        folder: folder.into(),
                        ..Workspace::default()
                    });
                } else {
                    beyond += 1;
                }
            };
        match bar.from {
            Workspaces::Servers => {
                for server in &state.servers {
                    push(
                        &server.id,
                        &server.name,
                        selected == Some(server.id.as_str()),
                        unread(server),
                        mentions(server),
                        "",
                    );
                }
            }
            Workspaces::Folders => {
                let mut servers = state.servers.iter().peekable();
                while let Some(server) = servers.next() {
                    let Some(folder) = &server.folder else {
                        push(
                            &server.id,
                            &server.name,
                            selected == Some(server.id.as_str()),
                            unread(server),
                            mentions(server),
                            "",
                        );
                        continue;
                    };
                    let mut inside = vec![server];
                    while let Some(next) =
                        servers.next_if(|next| next.folder.as_ref() == Some(folder))
                    {
                        inside.push(next);
                    }
                    // A folder opens the server of it that's open, or its first.
                    let open = inside
                        .iter()
                        .find(|server| selected == Some(server.id.as_str()))
                        .unwrap_or(&inside[0]);
                    let name = folder.name.clone().unwrap_or_else(|| open.name.clone());
                    push(
                        &open.id,
                        &name,
                        inside
                            .iter()
                            .any(|server| selected == Some(server.id.as_str())),
                        inside.iter().any(|server| unread(server)),
                        inside.iter().map(|server| mentions(server)).sum(),
                        &folder.id,
                    );
                }
            }
        }
        if hidden && beyond > 0 {
            rows.push(Workspace {
                id: "@more".into(),
                number: format!("+{beyond}").into(),
                name: format!("{beyond} more servers").into(),
                more: true,
                ..Workspace::default()
            });
        }
        super::theme_lists::picker(state, bar.count);
        let same = bar.model.row_count() == rows.len()
            && rows
                .iter()
                .enumerate()
                .all(|(index, row)| bar.model.row_data(index).as_ref() == Some(row));
        if !same {
            bar.model.set_vec(rows);
        }
    });
}

/// What's playing on the account, `title · artist`, when a theme's top bar
/// shows it.
pub fn now_playing(window: &YapfastWindow, state: &yapfast_core::application::AppState) {
    let wanted = BAR.with_borrow(|bar| bar.as_ref().is_some_and(|bar| bar.playing));
    if !wanted {
        return;
    }
    use yapfast_core::chat::ActivityKind;
    let own = state.discord_user.as_deref();
    let listed = own
        .and_then(|own| state.person(own))
        .map(|person| person.activities);
    let activities = listed
        .filter(|activities| !activities.is_empty())
        .or_else(|| {
            state
                .account_profile
                .as_ref()
                .map(|profile| profile.activities.clone())
        });
    let line = activities
        .iter()
        .flat_map(|activities| activities.iter())
        .find(|activity| activity.kind == ActivityKind::Listening)
        .map(|activity| {
            let title = activity.details.as_deref().unwrap_or(&*activity.name);
            match activity.state.as_deref() {
                Some(artist) if !artist.is_empty() => format!("{title} · {artist}"),
                _ => title.to_owned(),
            }
        })
        .unwrap_or_default();
    let chrome = window.global::<Chrome>();
    if chrome.get_now_playing().as_str() != line {
        chrome.set_now_playing(line.into());
    }
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
    fn the_top_bar_numbers_servers_and_shows_whats_playing() {
        use std::sync::Arc;
        use yapfast_core::application::{AppState, Navigation};
        use yapfast_core::chat::{Activity, ActivityKind, ChatServer, Profile};
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        init(&window);
        let words = Words {
            topbar: Some(TopBar {
                workspaces: 2,
                right: "{now_playing}".into(),
                ..TopBar::default()
            }),
            ..Words::default()
        };
        apply_more(&window, &words, Some("Mono"), false, false);
        let chrome = window.global::<Chrome>();
        assert_eq!(chrome.get_topbar_height(), 26.);
        assert_eq!(chrome.get_font(), "Mono");
        let server = |id: &str, name: &str| {
            Arc::new(ChatServer {
                id: id.into(),
                name: name.into(),
                ..ChatServer::default()
            })
        };
        let mut state = AppState::default();
        state.servers = vec![
            server("a", "Alpha"),
            server("b", "Beta"),
            server("c", "Gamma"),
        ];
        state.navigation = Navigation::Server("b".into());
        workspaces(&state);
        let rows = chrome.get_workspaces();
        assert_eq!(rows.row_count(), 2, "only as many as the theme asks");
        let second = rows.row_data(1).unwrap();
        assert_eq!(
            (second.number.as_str(), second.name.as_str()),
            ("2", "Beta")
        );
        assert!(second.current && !rows.row_data(0).unwrap().current);
        state.account_profile = Some(Arc::new(Profile {
            activities: Arc::from([Activity {
                kind: ActivityKind::Listening,
                name: "Spotify".into(),
                details: Some("Low Water".into()),
                state: Some("Harbor Lights".into()),
                party: None,
                application_id: None,
                start: None,
                end: None,
                large_image: None,
                large_text: None,
                small_image: None,
                small_text: None,
                url: None,
                link: None,
                emoji: None,
            }]),
            ..Profile::default()
        }));
        now_playing(&window, &state);
        assert_eq!(chrome.get_now_playing(), "Low Water · Harbor Lights");
        assert_eq!(
            chrome.invoke_bar(
                "{name}@{server} {now_playing}".into(),
                "xoey".into(),
                "hurt".into(),
                "general".into(),
                "song".into(),
                "12:00".into()
            ),
            "xoey@hurt song"
        );
        assert_eq!(chrome.invoke_typed("message".into(), 3), "mes");
        reset(&window);
        assert_eq!(chrome.get_topbar_height(), 0.);
    }

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
