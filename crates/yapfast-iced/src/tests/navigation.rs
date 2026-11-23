//! The rail and the sidebar: servers, folders and DMs opening, their
//! unread marks, categories folding, and the DMs' search or, with its
//! setting, Find or start a conversation.

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
    assert_eq!(
        app.rail.items.len(),
        servers + 2,
        "the line, the servers and the tile that adds one"
    );
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
fn a_dm_being_read_stays_off_the_rail_and_out_of_the_home_count() {
    use crate::rail::{Event, Kind};
    let (mut app, _folder) = preview();
    let _ = app.update(Message::Rail(Event::Dm("riley".into())));
    let read = |app: &mut App, reading: bool| {
        change(app, |state| {
            state.reading = reading;
            let room = state
                .rooms
                .iter_mut()
                .map(Arc::make_mut)
                .find(|room| room.id == "riley")
                .expect("the room");
            // A message came, which the core counts until its read reaches
            // Discord.
            room.unread = 1;
            room.mentions = 1;
        });
    };
    read(&mut app, true);
    assert_eq!(app.rail.items[0].kind, Kind::Divider, "no DM on the rail");
    assert!(!app.rail.home.unread);
    assert_eq!(app.rail.home.badge, "");
    // Scrolled up or the window behind another, the message waits.
    read(&mut app, false);
    assert_eq!(app.rail.items[0].kind, Kind::Dm);
    assert!(app.rail.home.unread);
    assert_eq!(app.rail.home.badge, "1");
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
        });
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
    assert_eq!(closed.len(), 3, "the line, the folder and the add tile");
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
            ("+".into(), false),
        ]
    );
    // Opened, it stays open the next time.
    let again = start(&folder, true);
    assert!(again.layout.open_folders.contains("discord-folder:7"));
    let _ = app.update(Message::Rail(Event::Folder("discord-folder:7".into())));
    assert_eq!(kinds(&app).len(), 3);
}

#[test]
fn a_middle_click_on_a_server_marks_it_read() {
    // A Discord server whose #general mentions her.
    let (mut app, _folder) = super::server::discord_server(0, |_, _| {});
    let _ = app.update(Message::Rail(crate::rail::Event::MarkRead(
        "discord:10".into(),
    )));
    let marked: Vec<String> = std::mem::take(&mut app.client.sent)
        .into_iter()
        .filter_map(|command| match command {
            yapfast_core::chat::Command::MarkRead { room, .. } => Some(room),
            _ => None,
        })
        .collect();
    assert_eq!(marked, ["discord:10:11"], "its unread channel is read");
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
    assert!(!app.layout.dm_search_switcher, "the filter by default");
    assert!(!shows(&app, "Find or start a conversation"));
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
fn with_its_setting_the_dm_lists_find_or_start_a_conversation_opens_the_quick_switcher() {
    let (mut app, _folder) = preview();
    app.layout.dm_search_switcher = true;
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    assert!(shows(&app, "Find or start a conversation"));
    interact(&mut app, |ui| {
        ui.click("Find or start a conversation")
            .expect("the button");
    });
    assert!(app.client.state().switcher.is_some(), "the quick switcher");
    assert!(shows(&app, "Where would you like to go?"));
}

#[test]
fn the_sidebars_edge_drags_it_within_limits_and_the_width_is_kept() {
    use crate::sidebar::{Event, WIDTH, width};
    let (mut app, folder) = preview();
    let side = |app: &mut App, event| {
        let _ = app.update(Message::Side(event));
    };
    assert_eq!(width(&app.layout, &app.theme), WIDTH);
    // Each move says how far the pointer is from where it pressed.
    side(&mut app, Event::Resize(20.0));
    side(&mut app, Event::Resize(40.0));
    assert_eq!(width(&app.layout, &app.theme), WIDTH + 40.0);
    assert_eq!(crate::conversation::chat_left(&app), 72.0 + WIDTH + 40.0);
    // No narrower than 180 px.
    side(&mut app, Event::Resize(-1000.0));
    assert_eq!(width(&app.layout, &app.theme), 180.0);
    // No wider than 380 px, nor than leaves the conversation 420 px.
    side(&mut app, Event::Resize(1000.0));
    let beside = app.window.width - crate::conversation::chat_right(&app);
    let most = (app.window.width - 72.0 - 420.0 - beside).clamp(180.0, 380.0);
    assert_eq!(width(&app.layout, &app.theme), most);
    side(&mut app, Event::Resize(-60.0));
    side(&mut app, Event::Resized);
    let kept = WIDTH - 60.0;
    assert_eq!(width(&app.layout, &app.theme), kept);
    // The next start has it.
    crate::layout::flush(folder.path());
    let mut next = super::start(&folder, true);
    assert_eq!(width(&next.layout, &next.theme), kept);
    // A double click gives the sidebar its own width back.
    side(&mut next, Event::ResetWidth);
    assert_eq!(width(&next.layout, &next.theme), WIDTH);
    crate::layout::flush(folder.path());
    let restarted = super::start(&folder, true);
    assert_eq!(width(&restarted.layout, &restarted.theme), WIDTH);
}

#[test]
fn the_member_lists_edge_drags_it_within_limits_and_the_width_is_kept() {
    use crate::members::{Event, WIDTH, width};
    let (mut app, folder) = preview();
    let list = |app: &mut App, event| {
        let _ = app.update(Message::Members(event));
    };
    assert_eq!(width(&app.layout, &app.theme), WIDTH);
    // Its edge is its left: a drag to the left widens it.
    list(&mut app, Event::Resize(-35.0));
    assert_eq!(width(&app.layout, &app.theme), WIDTH + 35.0);
    list(&mut app, Event::Resize(1000.0));
    assert_eq!(width(&app.layout, &app.theme), 200.0);
    list(&mut app, Event::Resize(-1000.0));
    let most =
        (app.window.width - crate::conversation::chat_left(&app) - 420.0).clamp(200.0, 480.0);
    assert_eq!(width(&app.layout, &app.theme), most);
    list(&mut app, Event::Resize(-35.0));
    list(&mut app, Event::Resized);
    crate::layout::flush(folder.path());
    let mut next = super::start(&folder, true);
    assert_eq!(width(&next.layout, &next.theme), WIDTH + 35.0);
    list(&mut next, Event::ResetWidth);
    assert_eq!(width(&next.layout, &next.theme), WIDTH);
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
    let _ = app.update(crate::app::settings(Event::Open(Section::Appearance)));
    let _ = app.update(crate::app::settings(Event::Close));
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

#[test]
fn a_dms_nameplate_fills_its_row_to_the_edges() {
    use yapfast_core::chat::Nameplate;
    let (mut app, _folder) = preview();
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    // Sam wears a red nameplate, and their DM is the open one.
    change(&mut app, |state| {
        let room = state
            .rooms
            .iter_mut()
            .find(|room| room.direct && room.name == "Sam")
            .expect("Sam's DM");
        Arc::make_mut(room).nameplate = Some(Arc::new(Nameplate {
            picture: "https://example.invalid/plate.png".into(),
            colors: Some([0xff0000, 0xff0000]),
        }));
    });
    interact(&mut app, |ui| {
        ui.click("Sam").expect("Sam's DM");
    });
    let mut ui =
        iced_test::Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
    let (width, height, rgba) = super::drawn(&mut ui);
    // The rows end 8 px short of the sidebar's right edge.
    let right = 72.0 + crate::sidebar::width(&app.layout, &app.theme) - 8.0;
    // How much of a column of the window the plate's red covers, in px.
    let red = |x: f32| {
        let x = (x * 2.0) as usize;
        let tinted = (0..height as usize)
            .filter(|y| {
                let at = (y * width as usize + x) * 4;
                rgba[at] > rgba[at + 1].saturating_add(40)
            })
            .count();
        tinted as f32 / 2.0
    };
    // It's as tall as the row, 42 px, where the row is straight, runs
    // into its round corners, and ends with it.
    assert_eq!(red(right - 20.0), 42.0);
    assert!(red(right - 3.0) >= 39.0, "to the edge");
    assert_eq!(red(right + 2.0), 0.0, "and no further");
}

#[test]
fn the_account_cards_carets_sit_at_the_middle_of_their_toggles() {
    use crate::call::{Event, Popup};
    let (app, _folder) = preview();
    // The microphone's caret is the 20 px after its 32 px toggle, and a
    // press 8 px above or below the card's middle is on it.
    for off in [-8.0, 8.0] {
        let mut ui = account_card(&app);
        ui.point_at(iced::Point::new(174.0, crate::account::HEIGHT / 2.0 + off));
        ui.simulate(iced_test::simulator::click());
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::Call(Event::Open(Popup::Input)))),
            "{off} px from the middle"
        );
    }
}

/// The preview's Friends server as a Discord one, with a voice channel
/// beside Lounge for each kind Discord's server list leaves out: muted,
/// the inactive channel, a Stage, one hidden from this account, and one
/// holding only someone blocked.
fn voice_server(app: &mut App) {
    use yapfast_core::chat::{Blocked, MarkKind, VoicePerson};
    let person = |id: &str, name: &str| VoicePerson {
        id: id.into(),
        name: name.into(),
        ..Default::default()
    };
    change(app, |state| {
        let lounge = state
            .rooms
            .iter()
            .find(|room| room.id == "lounge")
            .expect("Lounge")
            .clone();
        let left_out = |id: &str, set: &dyn Fn(&mut yapfast_core::chat::ChatRoom)| {
            let mut room = (*lounge).clone();
            room.id = id.into();
            room.name = id.into();
            room.people = vec![person(&format!("{id}-1"), "Someone")];
            set(&mut room);
            Arc::new(room)
        };
        let extra = [
            left_out("quiet", &|room| room.muted = true),
            left_out("afk", &|_| {}),
            left_out("talks", &|room| room.mark.kind = MarkKind::Stage),
            left_out("staff", &|room| room.locked = true),
            left_out("den", &|room| room.people = vec![person("9", "Blocked")]),
        ];
        let server = Arc::make_mut(&mut state.servers[0]);
        server.discord = true;
        server.afk_channel = Some("afk".into());
        server
            .channels
            .extend(extra.iter().map(|room| room.id.clone()));
        state.rooms.extend(extra);
        let lounge = state
            .rooms
            .iter_mut()
            .find(|room| room.id == "lounge")
            .expect("Lounge");
        Arc::make_mut(lounge).people = vec![person("2", "Ann"), person("3", "Ben")];
        state.discord_blocked = vec![Blocked {
            id: "9".into(),
            name: "Blocked".into(),
            username: None,
            avatar: None,
        }];
    });
}

/// A voice channel hidden from this account, listed with Show channels you
/// can't access, shows who is in it under its lock, as other voice channels
/// do, and a click on it joins nothing.
#[test]
fn a_hidden_voice_channel_lists_who_is_in_it_and_joins_nothing() {
    let (mut app, _folder) = preview();
    voice_server(&mut app);
    change(&mut app, |state| {
        let staff = state
            .rooms
            .iter_mut()
            .find(|room| room.id == "staff")
            .expect("staff");
        let staff = Arc::make_mut(staff);
        staff.name = "No Access".into();
        staff.people[0].name = "Hana".into();
        staff.people[0].deafened = true;
    });
    assert!(shows(&app, "No Access"));
    assert!(shows(&app, "Hana"), "who is in it shows under it");
    interact(&mut app, |ui| {
        ui.click("No Access").expect("the hidden channel");
    });
    let _ = app.sync();
    let state = app.client.state();
    assert_eq!(state.voice.room, None, "it can't be joined");
    assert!(!state.voice.active());
}

#[test]
fn a_server_with_people_in_voice_shows_a_speaker_as_discords_list_does() {
    use crate::rail::Media;
    let (mut app, _folder) = preview();
    let friends = |app: &App| {
        let item = app
            .rail
            .items
            .iter()
            .find(|item| item.id == "friends")
            .expect("Friends")
            .clone();
        let busy: Vec<String> = item.busy.0.iter().map(|room| room.id.clone()).collect();
        (item.media, item.call, busy)
    };
    assert_eq!(friends(&app), (None, false, vec![]));
    voice_server(&mut app);
    assert_eq!(
        friends(&app),
        (Some(Media::Voice), false, vec!["lounge".to_owned()]),
        "Lounge only: muted, inactive, Stage, hidden and blocked-only channels don't count"
    );
    let set_lounge = |app: &mut App, set: fn(&mut yapfast_core::chat::VoicePerson)| {
        change(app, |state| {
            let lounge = state
                .rooms
                .iter_mut()
                .find(|room| room.id == "lounge")
                .expect("Lounge");
            set(&mut Arc::make_mut(lounge).people[0]);
        });
    };
    set_lounge(&mut app, |ann| ann.video = true);
    assert_eq!(friends(&app).0, Some(Media::Video), "a camera over a voice");
    set_lounge(&mut app, |ann| ann.streaming = true);
    assert_eq!(
        friends(&app).0,
        Some(Media::Screen),
        "a screen over a camera"
    );
    // A muted server shows no speaker, but its tooltip still lists them.
    change(&mut app, |state| {
        Arc::make_mut(&mut state.servers[0]).muted = true;
    });
    assert_eq!(friends(&app), (None, false, vec!["lounge".to_owned()]));
    // In a call there, it shows green whatever its mute, with this
    // account's own call's marks: no screen of its own, a camera in it.
    change(&mut app, |state| {
        state.voice.phase = yapfast_core::voice::Phase::Connected;
        state.voice.room = Some("lounge".into());
    });
    assert_eq!(friends(&app).0, Some(Media::Video));
    assert!(friends(&app).1, "green");
}

#[test]
fn a_closed_folder_shows_a_speaker_only_for_this_accounts_call() {
    use crate::rail::{Event, Media};
    let (mut app, _folder) = preview();
    voice_server(&mut app);
    change(&mut app, |state| {
        for server in &mut state.servers {
            Arc::make_mut(server).folder = Some(yapfast_core::chat::ServerFolder {
                id: "discord-folder:7".into(),
                name: None,
                color: None,
            });
        }
    });
    let folder = |app: &App| {
        let item = &app.rail.items[1];
        assert_eq!(item.id, "discord-folder:7");
        (item.media, item.call)
    };
    assert_eq!(folder(&app), (None, false), "others in voice don't mark it");
    change(&mut app, |state| {
        state.voice.phase = yapfast_core::voice::Phase::Connected;
        state.voice.room = Some("party-voice".into());
    });
    assert_eq!(folder(&app), (Some(Media::Voice), true));
    // Open, the folder leaves it to its servers: Friends grey, Co-op green.
    let _ = app.update(Message::Rail(Event::Folder("discord-folder:7".into())));
    let marks: Vec<_> = app
        .rail
        .items
        .iter()
        .filter(|item| item.id == "friends" || item.id == "coop")
        .map(|item| (item.id.as_str(), item.media, item.call))
        .collect();
    assert_eq!(
        marks,
        [
            ("friends", Some(Media::Voice), false),
            ("coop", Some(Media::Voice), true)
        ]
    );
}

#[test]
fn a_busy_servers_tooltip_lists_its_voice_channels_under_the_pointer() {
    use crate::rail::Event;
    use iced::{Point, Rectangle, Size};
    let (mut app, _folder) = preview();
    // Below where a plain tooltip reaches: it is 32 px tall, centred on
    // the tile, and its shadow 12 px past that. The card reaches lower,
    // to the channel's faces.
    let below_name = |tile: Point| {
        Rectangle::new(
            Point::new(tile.x + 44.0, tile.y + 36.0),
            Size::new(160.0, 8.0),
        )
    };
    let tile = |ui: &mut iced_test::Simulator<'_, Message>| {
        // Friends is the tile above Co-op crew's.
        let coop = ui
            .find("CC")
            .expect("Co-op crew's tile")
            .visible_bounds()
            .expect("in view")
            .center();
        Point::new(coop.x, coop.y - crate::rail::TILE - crate::rail::GAP)
    };
    let _ = app.update(Message::Rail(Event::Point("friends".into(), true)));
    interact(&mut app, |ui| {
        let at = tile(ui);
        assert!(
            !super::hover_draws(ui, at, below_name(at)),
            "nobody in voice: just its name"
        );
    });
    voice_server(&mut app);
    interact(&mut app, |ui| {
        let at = tile(ui);
        assert!(
            super::hover_draws(ui, at, below_name(at)),
            "Lounge, with Ann's and Ben's faces"
        );
    });
}

#[test]
fn a_closed_folder_shows_up_to_four_server_icons_in_a_two_by_two_grid() {
    use iced_test::{Simulator, selector::Candidate};
    use yapfast_core::chat::{ChatServer, Picture, Pixels, ServerFolder};

    let (mut app, _folder) = preview();
    let in_folder = ServerFolder {
        id: "discord-folder:7".into(),
        name: None,
        color: Some(0xff0000),
    };
    change(&mut app, |state| {
        let channels = state.servers[0].channels.clone();
        state.servers = ["Alpha", "Beta", "Gamma", "Delta"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                Arc::new(ChatServer {
                    id: if index == 0 {
                        "friends".into()
                    } else {
                        format!("s{index}")
                    },
                    name: name.into(),
                    channels: if index == 0 {
                        channels.clone()
                    } else {
                        Vec::new()
                    },
                    folder: Some(in_folder.clone()),
                    ..Default::default()
                })
            })
            .collect();
    });

    let mut ui = Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
    let mut folder_bounds = None;
    let mut initials: [Option<iced::Rectangle>; 4] = [None; 4];
    let _ = ui.find(|candidate: Candidate<'_>| {
        match candidate {
            Candidate::Container {
                visible_bounds: Some(bounds),
                ..
            } if bounds.x < crate::rail::WIDTH
                && (bounds.width - crate::rail::FOLDER).abs() < 0.5
                && (bounds.height - crate::rail::FOLDER).abs() < 0.5 =>
            {
                folder_bounds = Some(bounds);
            }
            Candidate::Text {
                content,
                visible_bounds: Some(bounds),
                ..
            } if bounds.x < crate::rail::WIDTH => {
                if let Some(slot) = ["A", "B", "G", "D"]
                    .iter()
                    .position(|label| *label == content)
                {
                    initials[slot] = Some(bounds);
                }
            }
            _ => {}
        }
        None::<()>
    });

    let folder = folder_bounds.expect("the closed 48x48 folder");
    let slots = initials.map(|slot| slot.expect("folder preview initial in view"));
    for (index, bounds) in slots.iter().enumerate() {
        assert!(
            bounds.width > 0.0 && bounds.height > 0.0,
            "slot {index} has non-empty bounds: {bounds:?}"
        );
        assert!(
            bounds.x >= folder.x
                && bounds.y >= folder.y
                && bounds.x + bounds.width <= folder.x + folder.width
                && bounds.y + bounds.height <= folder.y + folder.height,
            "slot {index} ({bounds:?}) sits inside the 48x48 folder ({folder:?})"
        );
    }
    // Top-left (0), top-right (1), bottom-left (2), bottom-right (3).
    assert!(slots[1].center_x() > slots[0].center_x());
    assert!((slots[1].center_y() - slots[0].center_y()).abs() < 2.0);
    assert!(slots[2].center_y() > slots[0].center_y());
    assert!((slots[2].center_x() - slots[0].center_x()).abs() < 2.0);
    assert!(slots[3].center_x() > slots[2].center_x());
    assert!(slots[3].center_y() > slots[1].center_y());
    drop(ui);

    // Picture icons also render in all 4 quadrants of the 48x48 folder.
    let green = Arc::new(Picture {
        width: 19,
        height: 19,
        rgba: Pixels::new(19, 19, [0_u8, 255, 0, 255].repeat(19 * 19)),
        ..Default::default()
    });
    change(&mut app, |state| {
        for (index, server) in state.servers.iter_mut().enumerate() {
            let url = format!("https://example.invalid/folder-{index}.png");
            Arc::make_mut(server).icon = Some(url.clone());
            state.pictures.insert(url, Arc::clone(&green));
        }
    });
    let mut ui = Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
    let (width, _, rgba) = super::drawn(&mut ui);
    let pixel = |x: f32, y: f32| {
        let px = (x * 2.0) as usize;
        let py = (y * 2.0) as usize;
        let at = (py * width as usize + px) * 4;
        [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
    };
    for index in 0..4 {
        let cx = folder.x + 4.0 + (index % 2) as f32 * 21.0 + 9.5;
        let cy = folder.y + 4.0 + (index / 2) as f32 * 21.0 + 9.5;
        let [r, g, b, _] = pixel(cx, cy);
        assert!(
            g > 200 && g > r.saturating_add(100) && g > b.saturating_add(100),
            "slot {index} picture at ({cx}, {cy}) should be green, got [{r}, {g}, {b}]"
        );
    }
}

#[test]
fn holding_alt_over_a_channel_peeks_at_it_only_with_the_switch_on() {
    use crate::{peek, sidebar};
    let (mut app, _folder) = preview();
    let open = app.client.state().selected.clone();
    assert_ne!(open.as_deref(), Some("games"));
    let hold = |app: &mut App, alt: bool| {
        let modifiers = if alt {
            iced::keyboard::Modifiers::ALT
        } else {
            iced::keyboard::Modifiers::default()
        };
        let _ = app.update(Message::Modifiers(modifiers));
    };
    let _ = app.update(Message::Side(sidebar::Event::Point("games".into(), true)));
    hold(&mut app, true);
    let _ = app.update(Message::Peek(peek::Event::Due("games".into())));
    assert!(app.client.state().peek.is_none(), "off, Alt does nothing");
    hold(&mut app, false);

    app.layout.peek_channels = true;
    hold(&mut app, true);
    assert!(app.peek.wanted.is_some(), "the row under the pointer");
    let _ = app.update(Message::Peek(peek::Event::Due("games".into())));
    assert!(app.client.state().peek.is_some());
    assert!(!app.peek.found.is_empty(), "its messages' cards");
    assert!(shows(&app, "#games"), "beside the list");
    assert_eq!(app.client.state().selected, open, "the open channel stays");
    // Ctrl and Alt together don't peek; letting go closes it.
    let _ = app.update(Message::Modifiers(
        iced::keyboard::Modifiers::ALT | iced::keyboard::Modifiers::CTRL,
    ));
    assert!(app.client.state().peek.is_none());
    assert!(app.peek.found.is_empty(), "its cards go with it");
    // A late timer for a row left asks for nothing.
    let _ = app.update(Message::Peek(peek::Event::Due("games".into())));
    assert!(app.client.state().peek.is_none());
}
