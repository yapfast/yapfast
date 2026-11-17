//! The rail and the sidebar: servers, folders and DMs opening, their
//! unread marks, categories folding and the DMs' search.

use std::sync::Arc;

use yapfast_core::application::Navigation;

use super::{change, interact, preview, shows, start, unread};
use crate::app::{App, Message};

#[test]
fn a_channel_clicked_in_the_list_opens() {
    let (mut app, _folder) = preview();
    assert!(!shows(&app, "What are we playing next?"));
    interact(&mut app, |ui| {
        ui.click("games").expect("the games channel");
    });
    assert_eq!(app.client.state().selected.as_deref(), Some("games"));
    assert_eq!(app.header.name, "games");
    assert!(!app.header.direct);
    assert!(shows(&app, "What are we playing next?"));
}

#[test]
fn an_unread_dm_shows_on_the_rail_above_the_servers_and_opens() {
    use crate::rail::{Event, Kind};
    let (mut app, _folder) = preview();
    let servers = app.client.state().servers.len();
    assert_eq!(app.rail.items.len(), servers + 1);
    assert_eq!(
        app.rail.items[0].kind,
        Kind::Divider,
        "the line leads the servers"
    );
    unread(&mut app, "riley", 2);
    assert_eq!(app.rail.items[0].kind, Kind::Dm);
    assert_eq!(
        (
            app.rail.items[0].name.as_str(),
            app.rail.items[0].badge.as_str()
        ),
        ("Riley", "2")
    );
    assert_eq!(app.rail.items[1].kind, Kind::Divider);
    assert_eq!(app.rail.home.badge, "2", "the DM home counts its mentions");
    let _ = app.update(Message::Rail(Event::Dm("riley".into())));
    let state = app.client.state();
    assert_eq!(state.navigation, Navigation::DirectMessages);
    assert_eq!(state.selected.as_deref(), Some("riley"));
}

#[test]
fn a_voice_channels_chat_marks_its_server_only_by_mentions() {
    let (mut app, _folder) = preview();
    let friends = |app: &App| {
        let row = app
            .rail
            .items
            .iter()
            .find(|item| item.id == "friends")
            .unwrap();
        (row.unread, row.badge.clone())
    };
    let discord = |app: &mut App, discord: bool| {
        change(app, |state| {
            let server = state
                .servers
                .iter_mut()
                .find(|server| server.id == "friends");
            Arc::make_mut(server.unwrap()).discord = discord;
        })
    };
    discord(&mut app, true);
    unread(&mut app, "lounge", 0);
    assert_eq!(
        friends(&app),
        (false, String::new()),
        "Discord's client leaves a server read for its voice channels' chat"
    );
    unread(&mut app, "lounge", 1);
    assert_eq!(friends(&app), (true, "1".into()), "a mention there counts");
    unread(&mut app, "lounge", 0);
    discord(&mut app, false);
    assert_eq!(
        friends(&app),
        (true, String::new()),
        "a Matrix space's call room stays as it was"
    );
}

#[test]
fn a_folder_on_the_rail_holds_its_servers_until_opened_and_stays_so() {
    use crate::rail::{Event, Kind};
    let (mut app, folder) = preview();
    let in_folder = yapfast_core::chat::ServerFolder {
        id: "discord-folder:7".into(),
        name: None,
        color: Some(0xff0000),
    };
    change(&mut app, |state| {
        for server in &mut state.servers {
            Arc::make_mut(server).folder = Some(in_folder.clone());
        }
    });
    let kinds = |app: &App| -> Vec<(String, bool)> {
        app.rail
            .items
            .iter()
            .map(|item| (item.id.clone(), matches!(item.kind, Kind::Nested { .. })))
            .collect()
    };
    let closed = kinds(&app);
    assert_eq!(closed.len(), 2, "the line, and the folder");
    assert_eq!(
        app.rail.items[1].name, "Friends, Co-op crew",
        "named by its servers"
    );
    assert!(
        app.rail.items[1].selected,
        "closed, it shows a server of its is open"
    );
    let _ = app.update(Message::Rail(Event::Folder("discord-folder:7".into())));
    assert_eq!(
        kinds(&app),
        [
            ("divider".into(), false),
            ("discord-folder:7".into(), false),
            ("friends".into(), true),
            ("coop".into(), true),
        ]
    );
    // Opened, it stays open the next time.
    let again = start(&folder, true);
    assert!(again.layout.open_folders.contains("discord-folder:7"));
    let _ = app.update(Message::Rail(Event::Folder("discord-folder:7".into())));
    assert_eq!(kinds(&app).len(), 2);
}

#[test]
fn a_server_on_the_rail_opens_its_channels() {
    let (mut app, _folder) = preview();
    let _ = app.update(Message::Rail(crate::rail::Event::Server("coop".into())));
    assert_eq!(app.sidebar.header.name, "Co-op crew");
    assert!(shows(&app, "looking-for-group"));
    assert!(!shows(&app, "games"));
}

#[test]
fn clicking_a_category_folds_it_and_it_stays_folded() {
    let (mut app, folder) = preview();
    assert!(shows(&app, "games"));
    interact(&mut app, |ui| {
        ui.click("TEXT CHANNELS").expect("the heading");
    });
    // The open channel stays; the others go.
    assert!(shows(&app, "general"));
    assert!(!shows(&app, "games"));
    let again = start(&folder, true);
    assert!(again.layout.collapsed["friends"].contains("TEXT CHANNELS"));
    // An unread one stays too.
    unread(&mut app, "games", 0);
    assert!(shows(&app, "games"));
    interact(&mut app, |ui| {
        ui.click("TEXT CHANNELS").expect("the heading");
    });
    assert!(shows(&app, "weekend plans"));
}

#[test]
fn opening_an_unread_channel_clears_its_mark() {
    use crate::sidebar::SideRow;
    let (mut app, _folder) = preview();
    unread(&mut app, "games", 1);
    let marked = |app: &App| {
        app.sidebar.rows.iter().any(|row| {
            matches!(row, SideRow::Room(room) if room.id == "games" && room.unread && room.badge == "1")
        })
    };
    assert!(marked(&app));
    assert!(
        app.rail
            .items
            .iter()
            .any(|item| item.id == "friends" && item.badge == "1")
    );
    interact(&mut app, |ui| {
        ui.click("games").expect("the games channel");
    });
    assert_eq!(app.client.state().selected.as_deref(), Some("games"));
    // Read, the backend's room list says so, and the marks go.
    unread(&mut app, "games", 0);
    change(&mut app, |state| {
        for room in &mut state.rooms {
            Arc::make_mut(room).unread = 0;
        }
    });
    assert!(!marked(&app));
    assert!(
        app.rail
            .items
            .iter()
            .any(|item| item.id == "friends" && item.badge.is_empty() && !item.unread)
    );
}

#[test]
fn the_dm_search_keeps_the_dms_whose_names_hold_it() {
    let (mut app, _folder) = preview();
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    assert!(shows(&app, "Sam") && shows(&app, "Friday crew"));
    let _ = app.update(Message::Side(crate::sidebar::Event::Filter("fri".into())));
    assert!(shows(&app, "Friday crew"));
    assert!(!shows(&app, "Riley"));
    // It's the DM home's: a server shows all its channels, and home again
    // shows every DM.
    let _ = app.update(Message::Rail(crate::rail::Event::Server("coop".into())));
    assert!(shows(&app, "looking-for-group"), "the server's channels");
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    assert!(shows(&app, "Riley"), "every DM again");
}

#[test]
fn the_sidebars_edge_drags_it_within_limits_and_the_width_is_kept() {
    use crate::sidebar::{Event, WIDTH, width};
    let (mut app, folder) = preview();
    let side = |app: &mut App, event| {
        let _ = app.update(Message::Side(event));
    };
    assert_eq!(width(&app.layout), WIDTH);
    // Each move says how far the pointer is from where it pressed.
    side(&mut app, Event::Resize(20.0));
    side(&mut app, Event::Resize(40.0));
    assert_eq!(width(&app.layout), WIDTH + 40.0);
    assert_eq!(crate::conversation::chat_left(&app), 72.0 + WIDTH + 40.0);
    // No narrower than 180 px.
    side(&mut app, Event::Resize(-1000.0));
    assert_eq!(width(&app.layout), 180.0);
    // No wider than 380 px, nor than leaves the conversation 420 px.
    side(&mut app, Event::Resize(1000.0));
    let beside = app.window.width - crate::conversation::chat_right(&app);
    let most = (app.window.width - 72.0 - 420.0 - beside).clamp(180.0, 380.0);
    assert_eq!(width(&app.layout), most);
    side(&mut app, Event::Resize(-60.0));
    side(&mut app, Event::Resized);
    let kept = WIDTH - 60.0;
    assert_eq!(width(&app.layout), kept);
    // The next start has it.
    crate::layout::flush(folder.path());
    let mut next = super::start(&folder, true);
    assert_eq!(width(&next.layout), kept);
    // A double click gives the sidebar its own width back.
    side(&mut next, Event::ResetWidth);
    assert_eq!(width(&next.layout), WIDTH);
    crate::layout::flush(folder.path());
    assert_eq!(width(&super::start(&folder, true).layout), WIDTH);
}

#[test]
fn the_member_lists_edge_drags_it_within_limits_and_the_width_is_kept() {
    use crate::members::{Event, WIDTH, width};
    let (mut app, folder) = preview();
    let list = |app: &mut App, event| {
        let _ = app.update(Message::Members(event));
    };
    assert_eq!(width(&app.layout), WIDTH);
    // Its edge is its left: a drag to the left widens it.
    list(&mut app, Event::Resize(-35.0));
    assert_eq!(width(&app.layout), WIDTH + 35.0);
    list(&mut app, Event::Resize(1000.0));
    assert_eq!(width(&app.layout), 200.0);
    list(&mut app, Event::Resize(-1000.0));
    let most =
        (app.window.width - crate::conversation::chat_left(&app) - 420.0).clamp(200.0, 480.0);
    assert_eq!(width(&app.layout), most);
    list(&mut app, Event::Resize(-35.0));
    list(&mut app, Event::Resized);
    crate::layout::flush(folder.path());
    let mut next = super::start(&folder, true);
    assert_eq!(width(&next.layout), WIDTH + 35.0);
    list(&mut next, Event::ResetWidth);
    assert_eq!(width(&next.layout), WIDTH);
}

#[test]
fn a_server_out_of_reach_in_an_outage_says_so_in_its_tooltip() {
    // Whether pointing at Co-op crew's tile draws a tooltip reaching past
    // where its bare name's would end.
    let long_tip = |app: &mut App| {
        let mut drawn = false;
        interact(app, |ui| {
            let tile = ui.find("CC").expect("Co-op crew's tile");
            let at = tile.visible_bounds().expect("in view").center();
            let past_the_name = iced::Rectangle {
                x: crate::rail::WIDTH + 140.0,
                y: at.y - 4.0,
                width: 120.0,
                height: 8.0,
            };
            drawn = super::hover_draws(ui, at, past_the_name);
        });
        drawn
    };
    let (mut app, _folder) = preview();
    assert!(!long_tip(&mut app), "its name alone");
    change(&mut app, |state| {
        let coop = state
            .servers
            .iter_mut()
            .find(|server| server.id == "coop")
            .expect("Co-op crew");
        Arc::make_mut(coop).unavailable = true;
    });
    assert!(
        long_tip(&mut app),
        "Co-op crew (unavailable in a Discord outage)"
    );
}

#[test]
fn this_account_in_a_call_on_another_client_says_so_under_its_channel() {
    // Whether pointing at Dana under the Lounge draws a tooltip above her.
    let tip_above = |app: &mut App| {
        let mut drawn = false;
        interact(app, |ui| {
            let name = ui.find("Dana").expect("Dana under the Lounge");
            let row = name.visible_bounds().expect("in view");
            let above = iced::Rectangle {
                x: crate::rail::WIDTH,
                y: row.y - 24.0,
                width: 200.0,
                height: 16.0,
            };
            drawn = super::hover_draws(ui, row.center(), above);
        });
        drawn
    };
    let dana = |elsewhere: bool| yapfast_core::chat::VoicePerson {
        id: "dana".into(),
        name: "Dana".into(),
        elsewhere,
        ..Default::default()
    };
    let (mut app, _folder) = preview();
    let put = |app: &mut App, person| {
        change(app, |state| {
            let lounge = state
                .rooms
                .iter_mut()
                .find(|room| room.id == "lounge")
                .expect("the preview's Lounge");
            Arc::make_mut(lounge).people = vec![person];
        });
    };
    put(&mut app, dana(false));
    assert!(!tip_above(&mut app), "someone else in the call");
    put(&mut app, dana(true));
    assert!(tip_above(&mut app), "Connected on another client");
}

/// The account card drawn alone, 300 px wide.
fn account_card(app: &App) -> iced_test::Simulator<'_, Message> {
    iced_test::Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(300.0, crate::account::HEIGHT + crate::account::INSET),
        crate::account::view(app.ctx()),
    )
}

/// What a click on the account card's microphone sends: the card is 284
/// px from 8 px in, its controls 14 px from its end, the cog, both
/// devices' toggles with their carets 5 px apart.
fn account_mic_click(app: &App) -> Vec<Message> {
    let mut ui = account_card(app);
    ui.point_at(iced::Point::new(148.0, crate::account::HEIGHT / 2.0));
    ui.simulate(iced_test::simulator::click());
    ui.into_messages().collect()
}

fn phase(app: &mut App, phase: yapfast_core::voice::Phase) {
    change(app, |state| {
        state.voice.phase = phase;
        state.voice.room = Some("!lounge:test".into());
    });
}

#[test]
fn the_account_cards_mic_and_headphones_wait_while_a_call_joins() {
    use crate::account::Event;
    use yapfast_core::voice::Phase;
    let (mut app, _folder) = preview();
    let mute = |messages: &[Message]| {
        messages
            .iter()
            .any(|message| matches!(message, Message::Account(Event::Mute)))
    };
    assert!(
        mute(&account_mic_click(&app)),
        "out of a call it says how calls join"
    );
    phase(&mut app, Phase::Joining);
    assert!(
        !mute(&account_mic_click(&app)),
        "greyed while the call joins"
    );
    let muted = app.client.state().voice.muted;
    let _ = app.update(Message::Account(Event::Mute));
    assert_eq!(
        app.client.state().voice.muted,
        muted,
        "nor toggled by its message"
    );
    phase(&mut app, Phase::Connected);
    assert!(
        mute(&account_mic_click(&app)),
        "pressable once the voice is through"
    );
    phase(&mut app, Phase::Leaving);
    assert!(!mute(&account_mic_click(&app)), "greyed while it leaves");
}

#[test]
fn the_account_cards_avatar_rings_while_its_microphone_carries_voice() {
    use yapfast_core::voice::{Participant, Phase};
    let (mut app, _folder) = preview();
    phase(&mut app, Phase::Connected);
    let quiet = super::drawn(&mut account_card(&app));
    change(&mut app, |state| {
        state.voice.participants = vec![Participant {
            id: "me".into(),
            user: "me".into(),
            person: "me".into(),
            local: true,
            muted: false,
            deafened: false,
            server_muted: false,
            server_deafened: false,
            speaking: true,
            connected: true,
            problem: None,
            streaming: false,
            video: None,
            avatar: None,
        }];
    });
    let speaking = super::drawn(&mut account_card(&app));
    // The ring is drawn 3 px round the avatar, from 13 px in.
    let (width, _, _) = quiet;
    let scale = 2;
    let ring = |pixels: &[u8]| {
        let y = (crate::account::HEIGHT as usize / 2) * scale;
        let x = 13 * scale + 1;
        let at = (y * width as usize + x) * 4;
        pixels[at..at + 4].to_vec()
    };
    assert_ne!(ring(&quiet.2), ring(&speaking.2), "the green ring");
    let live = app.look().theme.live;
    let green = ring(&speaking.2);
    assert!(
        (green[1] as f32 / 255.0 - live.g).abs() < 0.1,
        "in the live colour: {green:?}"
    );
}

#[test]
fn the_account_cards_avatar_opens_the_status_popover_only_with_discord_linked() {
    let (mut app, _folder) = preview();
    let click_avatar = |app: &App| -> Vec<Message> {
        let mut ui = account_card(app);
        ui.point_at(iced::Point::new(36.0, crate::account::HEIGHT / 2.0));
        ui.simulate(iced_test::simulator::click());
        ui.into_messages().collect()
    };
    let opens = |messages: &[Message]| {
        messages
            .iter()
            .any(|message| matches!(message, Message::Status(crate::status::Event::Toggle)))
    };
    change(&mut app, |state| state.discord_linked = false);
    assert!(
        !opens(&click_avatar(&app)),
        "nothing until Discord is linked"
    );
    change(&mut app, |state| state.discord_linked = true);
    assert!(opens(&click_avatar(&app)));
}

#[test]
fn the_account_cards_cog_opens_settings_where_they_were_last() {
    use crate::settings::{Event, Section};
    let (mut app, _folder) = preview();
    let click_cog = |app: &mut App| {
        let messages: Vec<Message> = {
            let mut ui = account_card(app);
            ui.point_at(iced::Point::new(262.0, crate::account::HEIGHT / 2.0));
            ui.simulate(iced_test::simulator::click());
            ui.into_messages().collect()
        };
        for message in messages {
            let _ = app.update(message);
        }
    };
    let shown = |app: &App| {
        app.win
            .settings
            .get()
            .map(crate::settings::Settings::section)
    };
    click_cog(&mut app);
    assert_eq!(shown(&app), Some(Section::Account), "My Account at first");
    let _ = app.update(Message::Settings(Event::Open(Section::Appearance)));
    let _ = app.update(Message::Settings(Event::Close));
    assert_eq!(shown(&app), None);
    click_cog(&mut app);
    assert_eq!(shown(&app), Some(Section::Appearance), "where they were");
}

#[test]
fn the_account_card_without_a_discord_presence_shows_the_previews_dot() {
    let (app, _folder) = preview();
    let (width, _, pixels) = super::drawn(&mut account_card(&app));
    // The dot's middle: 7 px into its 14 px, which sits 27 px into the
    // avatar 16 px in, the avatar centred down the card.
    let scale = 2.0;
    let (x, y) = (
        16.0 + 27.0 + 7.0,
        (crate::account::HEIGHT - 40.0) / 2.0 + 27.0 + 7.0,
    );
    let at = (((y * scale) as usize) * width as usize + (x * scale) as usize) * 4;
    let accent = app.look().theme.accent;
    let drawn = &pixels[at..at + 3];
    let near = |drawn: u8, want: f32| (drawn as f32 / 255.0 - want).abs() < 0.08;
    assert!(
        near(drawn[0], accent.r) && near(drawn[1], accent.g) && near(drawn[2], accent.b),
        "the accent's dot: {drawn:?}"
    );
}
