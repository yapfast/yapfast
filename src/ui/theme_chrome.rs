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
    BarEffect, Caret, Chrome as Words, Hide, Label, Place, Presence, TopBar, TreeList, Workspaces,
    fill, split_group, split_typing,
};
use yapfast_ui::{Chrome, Look, ThemeContent, Workspace, YapfastWindow};

/// The top bar while a theme shows one: what its workspaces are, and the
/// clock that keeps `{time}`.
struct Bar {
    count: usize,
    from: Workspaces,
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
pub fn apply_more(window: &YapfastWindow, words: &Words, face: Option<&str>, still: bool) {
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
        ui.set_console_typing(if still { 0 } else { console.typing as i64 });
        ui.set_console_caret(Color::from_argb_encoded(console.caret.unwrap_or(0)));
    }
    match &words.topbar {
        Some(bar) => show_bar(window, bar, still),
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
        let tick = move || {
            if let Some(window) = weak.upgrade() {
                let seconds = yapfast_core::clock::now_ms() / 1000;
                let time = yapfast_core::clock::time_on(seconds, window.get_twelve_hour_clock());
                window.global::<Chrome>().set_time(time.into());
            }
        };
        tick();
        // Each minute, a second after it turns.
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(1),
            move || {
                if yapfast_core::clock::now_ms() / 1000 % 60 <= 1 {
                    tick();
                }
            },
        );
        timer
    });
    BAR.set(Some(Bar {
        count: bar.workspaces as usize,
        from: bar.from,
        playing: bar.shows_playing(),
        clock,
        model,
    }));
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
        // Each workspace: the server it opens, its name, whether it's open
        // and whether it's unread.
        let mut rows: Vec<Workspace> = Vec::new();
        let mut push = |id: &str, name: &str, current: bool, unread: bool| {
            if rows.len() < bar.count {
                rows.push(Workspace {
                    id: id.into(),
                    number: (rows.len() + 1).to_string().into(),
                    name: name.into(),
                    current,
                    unread,
                });
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
                    );
                }
            }
        }
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
    if !words.status.is_empty() {
        ui.set_status_column(true);
        for (presence, text) in &words.status {
            set_status(&ui, *presence, text);
        }
    }
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
        apply_more(&window, &words, Some("Mono"), false);
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
