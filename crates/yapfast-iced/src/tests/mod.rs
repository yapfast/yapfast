//! The window driven through `iced_test`'s headless simulator, on the
//! core's preview: each test builds the view, clicks and types into it as a
//! person would, and hands the messages that came out to `App::update`, as
//! iced's runtime does. The tasks `update` returns (focus, the clipboard,
//! timers) don't run here, and the simulator keeps widget state only while
//! it lives, so a field is clicked in the same simulator it's typed into.

use std::{
    path::PathBuf,
    sync::{Arc, Once},
};

use iced::{
    Point, Settings,
    keyboard::{self, key},
    mouse,
    widget::Id,
};
use iced_test::{Simulator, selector::Candidate, simulator::click};
use yapfast_core::{
    application::{Action, Compose, Navigation},
    chat::{Event, MfaMethod},
    runtime::BackgroundRuntime,
};

use crate::{
    app::{App, Message},
    client::Client,
    composer::COMPOSER,
    fonts, mfa, signin,
};

/// The preview at the window's own size, its data in a folder of its own.
fn preview() -> (App, tempfile::TempDir) {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    (start(&folder, true), folder)
}

/// The window on `folder`'s data: the preview, or else the sign-in screen
/// of a Yapfast that has no account yet.
fn start(folder: &tempfile::TempDir, preview: bool) -> App {
    install_fonts();
    let runtime = Arc::new(BackgroundRuntime::new().expect("runtime"));
    let mut client = Client::new(runtime, folder.path().into());
    if preview {
        client.dispatch(Action::Preview);
    }
    let mut app = App::new(client, (1120, 760)).0;
    // As the window opens.
    app.shell.window = Some(iced::window::Id::unique());
    app
}

/// The window's fonts, once per test run.
pub(crate) fn install_fonts() {
    static FONTS: Once = Once::new();
    FONTS.call_once(|| fonts::install(twemoji()));
}

/// The Twemoji font for the tests that draw or offer emoji, made from
/// `vendor/twemoji`'s pictures with the root crate's own font tables, as
/// its `build.rs` packs them, so a test never runs without it. It skips
/// what only the look needs: the pictures' margins and the joined
/// sequences, which the tests don't type.
pub(crate) fn twemoji() -> &'static [u8] {
    static FONT: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/twemoji/72x72");
        let mut pictures: Vec<(String, Vec<u8>)> = std::fs::read_dir(&folder)
            .expect("vendor/twemoji/72x72")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
            .map(|path| {
                let name = path
                    .file_stem()
                    .expect("a name")
                    .to_string_lossy()
                    .to_ascii_lowercase();
                (name, std::fs::read(&path).expect("a picture"))
            })
            .collect();
        assert!(!pictures.is_empty(), "Twemoji's pictures");
        pictures.sort_by(|left, right| left.0.cmp(&right.0));
        let glyph = |index: usize| {
            char::from_u32(emoji_font_tables::TWEMOJI_LAST - index as u32).expect("a code point")
        };
        let also: Vec<(u32, char)> = pictures
            .iter()
            .enumerate()
            .filter_map(|(index, (name, _))| {
                let point = u32::from_str_radix(name, 16).ok()?;
                char::from_u32(point).map(|_| (point, glyph(index)))
            })
            .collect();
        let glyphs: Vec<(char, Vec<u8>)> = pictures
            .into_iter()
            .enumerate()
            .map(|(index, (_, bytes))| (glyph(index), bytes))
            .collect();
        emoji_font_tables::build_joined(
            &emoji_font_tables::TWEMOJI,
            crate::fonts::TWEMOJI,
            &glyphs,
            &also,
            &emoji_font_tables::Joins::default(),
        )
    })
}

/// The root crate's font tables, which its `build.rs` packs Twemoji with.
#[path = "../../../../src/emoji_font_tables.rs"]
#[allow(dead_code)]
mod emoji_font_tables;

/// The workspace's ignored `.local`, where tests keep their fixtures.
fn local() -> PathBuf {
    let local = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.local");
    std::fs::create_dir_all(&local).expect(".local");
    local
}

/// Builds the view, lets `act` work on it, and applies what came out.
fn interact(app: &mut App, act: impl FnOnce(&mut Simulator<'_, Message>)) {
    let messages: Vec<Message> = {
        let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
        act(&mut ui);
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = app.update(message);
    }
}

/// Whether the window shows `text`: some text reading it is inside the
/// window and the lists it's in, and no dimmed layer drawn after it (a
/// dialog's, a settings card's) lies over it.
fn shows(app: &App, text: &str) -> bool {
    let window = iced::Rectangle::with_size(app.shell.frame);
    let mut ui = Simulator::with_size(Settings::default(), window.size(), app.view());
    let dimmed = Id::new(crate::controls::DIMMED);
    // Each text in view that reads it, and whether a dim came after it.
    let mut found: Vec<(iced::Rectangle, bool)> = Vec::new();
    let _ = ui.find(|candidate: Candidate<'_>| {
        let bounds = candidate
            .visible_bounds()
            .and_then(|bounds| bounds.intersection(&window));
        let reads = match &candidate {
            Candidate::Text { content, .. } => *content == text,
            Candidate::TextInput { state, .. } => state.text() == text,
            _ => false,
        };
        match (bounds, &candidate) {
            (Some(bounds), _) if reads => found.push((bounds, false)),
            (Some(bounds), Candidate::Container { id: Some(id), .. }) if **id == dimmed => {
                for (shown, under) in &mut found {
                    *under |= bounds.contains(shown.center());
                }
            }
            _ => {}
        }
        None::<()>
    });
    found.iter().any(|(_, under)| !under)
}

/// Whether the window holds `text` anywhere, shown or not: in a list
/// scrolled away, under a dialog, cut off.
fn present(app: &App, text: &str) -> bool {
    let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
    ui.find(text).is_ok()
}

/// The middle of the last text in view that reads `text`.
fn last(ui: &mut Simulator<'_, Message>, text: &str) -> Point {
    let mut found = None;
    let _ = ui.find(|candidate: Candidate<'_>| {
        if let Candidate::Text {
            content,
            visible_bounds: Some(bounds),
            ..
        } = candidate
            && content == text
        {
            found = Some(bounds.center());
        }
        None::<()>
    });
    found.unwrap_or_else(|| panic!("{text:?} isn't in view"))
}

/// The middle of a text in view that starts with `prefix`.
fn starting(ui: &mut Simulator<'_, Message>, prefix: &str) -> Option<Point> {
    let mut found = None;
    let _ = ui.find(|candidate: Candidate<'_>| {
        if let Candidate::Text {
            content,
            visible_bounds: Some(bounds),
            ..
        } = candidate
            && content.starts_with(prefix)
        {
            found = Some(bounds.center());
        }
        None::<()>
    });
    found
}

/// The window as drawn now: its width and height in pixels (twice its
/// size, as the simulator draws it) and their RGBA.
fn drawn(ui: &mut Simulator<'_, Message>) -> (u32, u32, Vec<u8>) {
    let snapshot = ui.snapshot(&iced::Theme::Dark).expect("drawn");
    let folder = tempfile::tempdir_in(local()).expect("a folder");
    snapshot
        .matches_image(folder.path().join("drawn.png"))
        .expect("written");
    // Named after the renderer too: the folder's one file.
    let written = std::fs::read_dir(folder.path())
        .expect("the folder")
        .flatten()
        .next()
        .expect("the picture");
    let png = std::fs::read(written.path()).expect("read back");
    let picture = yapfast_core::chat::decode_picture(&png, u32::MAX, u32::MAX).expect("a PNG");
    (picture.width, picture.height, picture.rgba.to_vec())
}

/// Whether hovering `at` draws something in `area` that wasn't there: a
/// tooltip, which no operation finds (iced's tooltip overlay reports none).
fn hover_draws(ui: &mut Simulator<'_, Message>, at: Point, area: iced::Rectangle) -> bool {
    let away = Point::new(1.0, 1.0);
    ui.point_at(away);
    ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved {
        position: away,
    })]);
    let (width, _, before) = drawn(ui);
    ui.point_at(at);
    ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved {
        position: at,
    })]);
    let (_, _, after) = drawn(ui);
    let scale = 2.0;
    let rows = (area.y * scale) as usize..((area.y + area.height) * scale) as usize;
    let columns = (area.x * scale) as usize..((area.x + area.width) * scale) as usize;
    rows.flat_map(|y| columns.clone().map(move |x| (y * width as usize + x) * 4))
        .any(|at| before.get(at..at + 4) != after.get(at..at + 4))
}

/// A right click on the message reading `text`.
fn right_click(ui: &mut Simulator<'_, Message>, text: &str) {
    let body = ui.find(text).expect("the message");
    ui.point_at(body.visible_bounds().expect("in view").center());
    ui.simulate([
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
}

#[test]
fn the_composer_sends_on_enter() {
    let (mut app, _folder) = preview();
    let before = app.client.state().messages.len();
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.typewrite("hello there");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    let messages = &app.client.state().messages;
    assert_eq!(messages.len(), before + 1);
    assert_eq!(messages.last().expect("sent").body, "hello there");
    assert!(
        app.composer.draft.is_empty(),
        "the core empties the draft once sent"
    );
    assert!(shows(&app, "hello there"));
}

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
fn a_member_clicked_in_the_list_shows_their_card_until_a_click_outside() {
    let (mut app, _folder) = preview();
    // The last "Riley" is the member list's: the timeline comes before it.
    interact(&mut app, |ui| {
        let line = last(ui, "Riley");
        ui.point_at(line);
        ui.simulate(click());
    });
    let view = app.client.state().profile.as_ref().expect("the profile");
    assert_eq!(view.profile.name, "Riley");
    assert_eq!(
        app.profiles.card.as_ref().expect("the card").user,
        view.profile.id
    );
    // Their name on the card itself: the timeline and the list show it too.
    {
        let mut ui = Simulator::with_size(
            iced::Settings::default(),
            app.window,
            crate::profile::card_view(app.ctx()).expect("the card"),
        );
        assert!(ui.find("Riley").is_ok(), "on the card");
    }
    interact(&mut app, |ui| {
        ui.point_at(Point::new(500.0, 300.0));
        ui.simulate(click());
    });
    assert!(app.profiles.card.is_none());
    assert!(app.client.state().profile.is_none());
}

#[test]
fn a_messages_menu_replies_to_it() {
    let (mut app, _folder) = preview();
    let text = "Yep. I should be free after dinner.";
    interact(&mut app, |ui| right_click(ui, text));
    assert!(app.win.menu.is_some(), "the menu opened");
    assert!(
        !shows(&app, "Delete message"),
        "Riley's message isn't ours to delete"
    );
    interact(&mut app, |ui| {
        ui.click("Reply").expect("Reply");
    });
    assert!(app.win.menu.is_none());
    match app.client.state().compose() {
        Some(Compose::Reply { author, .. }) => assert_eq!(author, "Riley"),
        _ => panic!("not replying"),
    }
    assert!(shows(&app, &format!("Replying to Riley: {text}")));
}

#[test]
fn deleting_ones_own_message_asks_first() {
    let (mut app, _folder) = preview();
    let text = "Nice, let's get a game going.";
    interact(&mut app, |ui| right_click(ui, text));
    interact(&mut app, |ui| {
        ui.click("Delete message").expect("Delete message");
    });
    assert!(app.client.state().confirm_delete.is_some());
    assert!(shows(&app, "Are you sure you want to delete this message?"));
    interact(&mut app, |ui| {
        ui.click("Delete").expect("the dialog's Delete");
    });
    assert!(app.client.state().confirm_delete.is_none());
    assert!(!shows(&app, text), "the message went");
}

#[test]
fn the_theme_command_switches_the_colours_and_keeps_the_choice() {
    let (mut app, folder) = preview();
    assert_eq!(app.theme.id, "bonfire", "the default with the Discord look");
    let _ = app.update(Message::Control("theme sakura".into()));
    assert_eq!(app.theme.id, "sakura");
    assert!(!app.theme.dark);
    assert_eq!(app.theme().palette().background, app.theme.chat);
    let layout: serde_json::Value = serde_json::from_slice(&{
        crate::layout::flush(folder.path());
        std::fs::read(folder.path().join("layout.json")).expect("the layout")
    })
    .expect("JSON");
    assert_eq!(layout["theme"], "sakura");
    // A theme Yapfast doesn't have changes nothing.
    let _ = app.update(Message::Control("theme gone".into()));
    assert_eq!(app.theme.id, "sakura");
    // The next start shows the one chosen.
    assert_eq!(start(&folder, true).theme.id, "sakura");
}

#[test]
fn a_custom_theme_in_the_layout_shows_from_the_start() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    std::fs::write(
        folder.path().join("layout.json"),
        r##"{"theme": "custom:1", "custom_themes": [{"id": "custom:1", "name": "Plum",
            "base": "yap", "colours": {"chat": "#2a1a3a", "accent": "#c04080"}}]}"##,
    )
    .expect("the layout");
    let app = start(&folder, true);
    assert_eq!(app.theme.id, "custom:1");
    assert_eq!(app.theme.chat, crate::style::hex(0x2a1a3a));
    assert_eq!(app.theme.accent, crate::style::hex(0xc04080));
    assert_eq!(app.theme.selection.a, 0.35, "worked out from its accent");
}

#[test]
fn a_layout_key_of_the_wrong_kind_loses_only_itself() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    std::fs::write(
        folder.path().join("layout.json"),
        r#"{"theme": "sakura", "collapse_long": "yes", "members_hidden": true}"#,
    )
    .expect("the layout");
    let app = start(&folder, true);
    assert_eq!(app.theme.id, "sakura");
    assert!(app.layout.members_hidden);
    assert!(!app.layout.collapse_long);
}

#[test]
fn without_an_account_the_sign_in_screen_shows_and_the_preview_leaves_it() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let mut app = start(&folder, false);
    assert!(!crate::signin::workspace(&app));
    assert!(shows(&app, "Sign in to Yapfast"));
    if signin::MATRIX {
        assert!(shows(&app, "Use your existing Matrix account."));
    } else {
        assert!(shows(&app, "Show the code"));
        interact(&mut app, |ui| {
            ui.click("Use a token").expect("Use a token");
        });
        interact(&mut app, |ui| {
            ui.click(Id::new(signin::TOKEN)).expect("the token's field");
            ui.typewrite("a-token");
        });
        assert_eq!(app.sign_in.token, "a-token");
        assert!(shows(&app, "Link"));
        // Neither a log line of what's typed nor of the screen tells it.
        let typed = signin::Event::Token("a-token".into());
        assert!(!format!("{typed:?}").contains("a-token"));
        assert!(!format!("{:?}", app.sign_in).contains("a-token"));
    }
    interact(&mut app, |ui| {
        ui.click("Explore local preview").expect("the preview");
    });
    assert!(crate::signin::workspace(&app));
    assert!(shows(&app, "general"));
}

#[test]
#[cfg(not(feature = "matrix"))]
fn a_pasted_token_goes_once_the_account_is_linked() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let mut app = start(&folder, false);
    let _ = app.update(Message::SignIn(signin::Event::OpenToken));
    let _ = app.update(Message::SignIn(signin::Event::Token("a-token".into())));
    // Refused, it stays to fix.
    let _ = app.sync();
    assert_eq!(app.sign_in.token, "a-token");
    // Linked, it goes, and its field with it.
    app.client.app.state.account = Some("1".into());
    let _ = app.sync();
    assert!(app.sign_in.token.is_empty());
    assert!(!app.sign_in.token_open);
}

#[test]
#[cfg(not(feature = "matrix"))]
fn linking_shows_the_code_then_who_scanned_it() {
    use yapfast_core::{application::DiscordLink, discord_link::Progress};
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let mut app = start(&folder, false);
    // As "Show the code" starts it, without reaching Discord.
    app.client.app.state.discord_link = Some(DiscordLink::Starting);
    let _ = app.sync();
    assert!(shows(&app, "Getting a code..."));
    let _ = app.update(Message::Linking(Progress::Code(
        "https://discord.com/ra/a-code".into(),
    )));
    assert!(app.sign_in.code.is_some(), "the QR code drawn");
    assert!(shows(&app, "Scan it with the Discord app"));
    let _ = app.update(Message::Linking(Progress::Scanned("riley".into())));
    assert!(app.sign_in.code.is_none(), "the code let go");
    assert!(shows(&app, "Scanned"));
    assert!(shows(&app, "Confirm on your phone"));
    let _ = app.update(Message::Linking(Progress::Failed("It broke.".into())));
    assert!(shows(&app, "It broke."));
    assert!(shows(&app, "Try again"));
}

#[test]
fn a_held_request_asks_for_a_code_and_cancelling_closes_the_prompt() {
    let (mut app, _folder) = preview();
    app.client.apply(Event::MfaRequired {
        id: 7,
        methods: vec![MfaMethod::App, MfaMethod::Backup],
    });
    let _ = app.sync();
    assert!(shows(&app, "Multi-Factor Authentication"));
    assert!(shows(
        &app,
        "Enter the 6-digit code from your authenticator app."
    ));
    interact(&mut app, |ui| {
        ui.click(Id::new(mfa::CODE)).expect("the code's field");
        ui.typewrite("123456");
    });
    // Enter sends once the code is whole, in the view built after it.
    interact(&mut app, |ui| {
        ui.click(Id::new(mfa::CODE)).expect("the code's field");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    let prompt = app.client.app.mfa_prompt().expect("the prompt");
    assert_eq!(prompt.code, "123456");
    assert!(prompt.busy, "confirming");
    assert!(shows(&app, "Confirming..."));
    // Discord turned it down: the prompt stays for another try.
    app.client.apply(Event::MfaFinished {
        id: 7,
        result: Err("Invalid two-factor code".into()),
    });
    let _ = app.sync();
    assert!(shows(&app, "Invalid two-factor code"));
    // The other way lists the methods, and a backup code warns first.
    interact(&mut app, |ui| {
        ui.click("Verify with something else")
            .expect("the other way");
    });
    interact(&mut app, |ui| {
        ui.click("Use a backup code").expect("a backup code");
    });
    assert!(shows(&app, "Slow down! Are you sure?"));
    assert_eq!(app.mfa.wait.left, 10, "the button waits");
    interact(&mut app, |ui| {
        ui.click("Cancel").expect("Cancel");
    });
    assert!(app.client.app.mfa_prompt().is_none());
    assert!(!shows(&app, "Multi-Factor Authentication"));
}

#[test]
fn a_saved_account_restoring_at_start_shows_only_the_notice_until_it_fails() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let mut app = start(&folder, false);
    // As a start on a saved account leaves the core, without reaching it.
    let state = &mut app.client.app.state;
    state.busy = true;
    state.notice = Some("Connecting to Discord...".into());
    app.sign_in.starting = true;
    let _ = app.sync();
    assert!(shows(&app, "Connecting to Discord..."));
    assert!(!shows(&app, "Sign in to Yapfast"));
    let state = &mut app.client.app.state;
    state.busy = false;
    state.notice = Some("Discord said no.".into());
    let _ = app.sync();
    assert!(!app.sign_in.starting);
    assert!(shows(&app, "Sign in to Yapfast"));
}

/// The core's state, changed as the backend would, then taken in.
fn change(app: &mut App, change: impl FnOnce(&mut yapfast_core::application::AppState)) {
    change(&mut app.client.app.state);
    app.client.app.state.list_changes += 1;
    app.client.app.state.people_changes += 1;
    let _ = app.sync();
}

/// The open room holding one message, Sam's, with an 800 by 600 picture,
/// which shows smaller: a copy the window makes. That copy made.
fn a_picture_shown(app: &mut App) -> Arc<yapfast_core::chat::Picture> {
    use yapfast_core::chat::{Attachment, ChatMessage, Picture, Pixels, Placement};
    let picture = Arc::new(Picture {
        width: 800,
        height: 600,
        rgba: Pixels::new(800, 600, vec![128; 800 * 600 * 4]),
        ..Default::default()
    });
    let shown = picture.clone();
    change(app, |state| {
        state.messages = Arc::new(vec![Arc::new(ChatMessage {
            item: "i0".into(),
            author: "Sam".into(),
            attachments: vec![Attachment {
                name: "a.png".into(),
                size: None,
                image: true,
                dimensions: Some((800, 600)),
                preview: Some(shown),
                preview_failed: false,
                placement: Placement::Message,
                gif: None,
                spoiler: false,
                video: false,
                url: None,
                description: None,
            }],
            ..Default::default()
        })]);
    });
    drop(app.view());
    app.win.pictures.fitter.borrow_mut().make_asked(&picture);
    drop(app.view());
    assert!(
        !app.win.pictures.fitter.borrow().is_empty(),
        "the copy kept"
    );
    picture
}

#[test]
fn the_sign_in_screen_lets_go_of_the_timelines_pictures() {
    let (mut app, _folder) = preview();
    let _picture = a_picture_shown(&mut app);
    change(&mut app, |state| state.preview = false);
    assert!(!crate::signin::workspace(&app));
    drop(app.view());
    assert!(app.win.pictures.fitter.borrow().is_empty(), "the copy went");
}

/// Marks a room unread with `mentions` of this account.
fn unread(app: &mut App, id: &str, mentions: u64) {
    change(app, |state| {
        let room = state
            .rooms
            .iter_mut()
            .map(Arc::make_mut)
            .find(|room| room.id == id)
            .expect("the room");
        room.unread = mentions.max(1);
        room.mentions = mentions;
    });
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

/// The open room's messages made `count` long, each its own, the newest
/// last; returns their items.
fn long_history(app: &mut App, count: usize) -> Vec<String> {
    let mut items = Vec::new();
    change(app, |state| {
        let template = (*state.messages[0]).clone();
        let messages: Vec<_> = (0..count)
            .map(|index| {
                let item = format!("long-{index}");
                items.push(item.clone());
                Arc::new(yapfast_core::chat::ChatMessage {
                    id: item.clone(),
                    item,
                    body: format!("Message number {index}"),
                    timestamp: template.timestamp + index as u64 * 600,
                    reply: None,
                    own: false,
                    reactions: Vec::new(),
                    attachments: Vec::new(),
                    ..template.clone()
                })
            })
            .collect();
        state.messages = Arc::new(messages);
    });
    items
}

#[test]
fn switching_channels_shows_each_ones_messages_header_and_topic() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        state.topic = Some(Arc::new(yapfast_core::chat::Topic {
            body: "Plans for **game night**".into(),
            mentions: Vec::new(),
            emoji: Vec::new(),
        }));
    });
    let general = app.header.name.clone();
    assert!(
        shows(&app, "Plans for game night"),
        "the topic, on one line"
    );
    interact(&mut app, |ui| {
        ui.click("games").expect("the games channel");
    });
    assert_eq!(app.header.name, "games");
    assert!(shows(&app, "What are we playing next?"));
    interact(&mut app, |ui| {
        ui.click(general.as_str()).expect("the first channel");
    });
    assert_eq!(app.header.name, general);
    assert!(!shows(&app, "What are we playing next?"));
    assert_eq!(
        app.timeline.from_end, 0.0,
        "a channel opens at its newest message"
    );
}

#[test]
fn the_history_edge_offers_earlier_messages_then_shows_them_loading() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        state.history_start = false;
    });
    assert!(shows(&app, "Earlier messages"));
    let _ = app.update(Message::Earlier);
    assert!(
        !app.client.state().loading_history,
        "the preview has no earlier history"
    );
    change(&mut app, |state| state.loading_history = true);
    assert!(shows(&app, "Loading earlier messages..."));
    // The page comes: older messages above, the newest still in view.
    let newest = app.timeline.messages.last().expect("messages").body.clone();
    change(&mut app, |state| {
        let mut older = (*state.messages[0]).clone();
        older.id = "older".into();
        older.item = "older".into();
        older.body = "From long ago".into();
        older.timestamp -= 3600;
        let mut messages = vec![Arc::new(older)];
        messages.extend(state.messages.iter().cloned());
        state.messages = Arc::new(messages);
        state.loading_history = false;
        state.history_start = true;
    });
    assert!(shows(&app, "From long ago"));
    assert!(shows(&app, &newest));
    assert!(shows(&app, "This is the start of #general."));
}

#[test]
fn the_preview_says_so_and_a_notice_shows_until_dismissed() {
    let (mut app, _folder) = preview();
    assert!(shows(
        &app,
        "Local preview: sample data. Nothing leaves this computer."
    ));
    change(&mut app, |state| {
        state.notice = Some("Discord asked Yapfast to slow down.".into());
    });
    assert!(shows(&app, "Discord asked Yapfast to slow down."));
    interact(&mut app, |ui| {
        ui.click("Dismiss").expect("Dismiss");
    });
    assert!(app.client.state().notice.is_none());
    assert!(!shows(&app, "Discord asked Yapfast to slow down."));
}

#[test]
fn a_window_of_older_messages_ends_with_newer_messages_loading_or_failed() {
    let (mut app, _folder) = preview();
    let height = *app.timeline.heights.last().expect("rows");
    assert!(!shows(&app, "Newer messages"));
    change(&mut app, |state| state.detached = true);
    assert!(shows(&app, "Newer messages"));
    assert_eq!(
        *app.timeline.heights.last().expect("rows"),
        height + crate::timeline::EDGE,
        "the last row makes room for it"
    );
    change(&mut app, |state| state.loading_later = true);
    assert!(shows(&app, "Loading newer messages..."));
    change(&mut app, |state| {
        state.loading_later = false;
        state.later_failed = true;
    });
    assert!(shows(&app, "Couldn't load newer messages. Retry"));
    // Back at the present: the edge goes, and its room with it.
    change(&mut app, |state| {
        state.later_failed = false;
        state.detached = false;
    });
    assert!(!shows(&app, "Newer messages"));
    assert_eq!(*app.timeline.heights.last().expect("rows"), height);
}

#[test]
fn the_unread_bar_jumps_to_the_new_divider_out_of_view() {
    let (mut app, _folder) = preview();
    let items = long_history(&mut app, 80);
    change(&mut app, |state| {
        state.first_unread = Some(items[10].clone());
        state.caught_up = false;
    });
    assert!(
        !shows(&app, "Message number 10"),
        "the divider is above the view"
    );
    interact(&mut app, |ui| {
        let bar = starting(ui, "70 new messages since ").expect("the bar counts what's new");
        ui.point_at(bar);
        ui.simulate(click());
    });
    let _ = app.sync();
    // The rows round it are built; the scroll there is a task, which
    // doesn't run here, so the list itself stays where it was.
    assert!(present(&app, "Message number 10"), "the view went to it");
    assert!(present(&app, "New"));
    assert!(app.timeline.from_end > 0.0);
}

#[test]
fn a_menus_keys_pass_over_greyed_rows_and_clicks_on_them_do_nothing() {
    use crate::menus::{Menu, MenuAct, MenuRow};
    use yapfast_core::application::MenuChoice;
    let (mut app, _folder) = preview();
    let row = |choice, disabled| MenuRow {
        disabled,
        ..MenuRow::new(MenuAct::Place(choice), "row", crate::pictures::Icon::Id)
    };
    app.win.menu = Some(Menu {
        place: Some("nowhere".into()),
        ..Menu::new(
            Point::new(200.0, 120.0),
            vec![
                row(MenuChoice::MarkRead, false),
                row(MenuChoice::AddFriend, true),
                row(MenuChoice::OpenChat, false),
                row(MenuChoice::AcceptFriend, true),
            ],
        )
    });
    let down = || {
        Message::Key(
            keyboard::Key::Named(key::Named::ArrowDown),
            keyboard::Modifiers::default(),
        )
    };
    let mut lit = Vec::new();
    for _ in 0..4 {
        let _ = app.update(down());
        lit.push(app.win.menu.as_ref().expect("the menu stays").lit);
    }
    assert_eq!(lit, [Some(0), Some(2), Some(0), Some(2)]);
    let _ = app.update(Message::MenuChoose(1));
    assert!(app.win.menu.is_some(), "a greyed row keeps the menu open");
}

#[test]
fn a_link_preview_and_a_file_show_in_their_cards() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let mut message = (**state.messages.last().expect("a message")).clone();
        message.id = "card".into();
        message.item = "card".into();
        message.body = "Look at this".into();
        message.reply = None;
        message.reactions = Vec::new();
        message.embeds = vec![yapfast_core::chat::Embed {
            provider: "Example".into(),
            title: "Trip notes".into(),
            url: "https://example.com/notes".into(),
            description: "Everything for **Saturday**".into(),
            ..Default::default()
        }];
        message.attachments = vec![yapfast_core::chat::Attachment {
            name: "packing.txt".into(),
            size: Some(2048),
            image: false,
            dimensions: None,
            preview: None,
            preview_failed: false,
            placement: yapfast_core::chat::Placement::Message,
            spoiler: false,
            video: false,
            gif: None,
            url: None,
            description: None,
        }];
        let mut messages = (*state.messages).clone();
        messages.push(Arc::new(message));
        state.messages = Arc::new(messages);
    });
    assert!(shows(&app, "Look at this"), "the message shows");
    for text in [
        "Example",
        "Trip notes",
        "Everything for Saturday",
        "packing.txt",
        "2 KB",
    ] {
        assert!(shows(&app, text), "{text:?} shows");
    }
}

/// Scrolls the timeline `pixels` up from where the window shows it, as
/// a touchpad does over it.
fn scroll_up(app: &mut App, pixels: f32) {
    let middle = Point::new(
        crate::rail::WIDTH + crate::sidebar::WIDTH + 200.0,
        app.window.height / 2.0,
    );
    interact(app, |ui| {
        ui.point_at(middle);
        ui.simulate(crate::widgets::swipe(pixels));
    });
}

/// The message at the view's top, and how far into it the view starts.
fn at_top(app: &App) -> (String, f32) {
    crate::timeline::scroll::reading(app).expect("reading earlier messages")
}

/// Messages like those of [`long_history`], numbered `from..to`.
fn numbered(app: &App, from: usize, to: usize) -> Vec<Arc<yapfast_core::chat::ChatMessage>> {
    let template = app.client.state().messages.last().expect("a message");
    (from..to)
        .map(|index| {
            let item = format!("long-{index}");
            Arc::new(yapfast_core::chat::ChatMessage {
                id: item.clone(),
                item,
                body: format!("Message number {index}"),
                timestamp: template.timestamp + 600,
                ..(**template).clone()
            })
        })
        .collect()
}

#[test]
fn new_messages_leave_the_earlier_ones_being_read_where_they_are() {
    let (mut app, _folder) = preview();
    long_history(&mut app, 80);
    scroll_up(&mut app, 900.0);
    assert!(
        !app.timeline.at_end && app.timeline.from_end > 0.0,
        "the wheel went up"
    );
    let read = at_top(&app);
    // Three come; then three more as the oldest three go, as the core
    // keeps only so many.
    let new = numbered(&app, 80, 83);
    change(&mut app, |state| {
        let mut messages = (*state.messages).clone();
        messages.extend(new);
        state.messages = Arc::new(messages);
    });
    assert_eq!(at_top(&app), read, "the view stayed on the same message");
    let new = numbered(&app, 83, 86);
    change(&mut app, |state| {
        let mut messages = state.messages[3..].to_vec();
        messages.extend(new);
        state.messages = Arc::new(messages);
    });
    assert_eq!(at_top(&app), read, "and stays as the oldest go");
    assert!(
        !shows(&app, "Message number 85"),
        "the newest is out of view"
    );
}

#[test]
fn sending_while_reading_earlier_messages_goes_to_the_newest() {
    let (mut app, _folder) = preview();
    long_history(&mut app, 80);
    scroll_up(&mut app, 1500.0);
    assert!(!app.timeline.at_end);
    assert!(!shows(&app, "Message number 79"));
    interact(&mut app, |ui| {
        ui.click(Id::new(COMPOSER)).expect("the composer");
        ui.typewrite("back to the present");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    assert!(
        app.timeline.at_end && app.timeline.from_end == 0.0,
        "the view went to the end"
    );
    // The preview's room takes its own history back as it sends.
    assert!(shows(&app, "back to the present"));
    let (_, last) = app.timeline.in_view(0.0);
    assert_eq!(last, app.timeline.messages.len(), "the newest is in view");
}

#[test]
fn messages_the_core_makes_anew_keep_their_text_laid_out() {
    let (mut app, _folder) = preview();
    long_history(&mut app, 40);
    let laid: Vec<_> = app
        .timeline
        .looks
        .iter()
        .map(|look| look.body.clone())
        .collect();
    // As when a name or a role changes: every message anew, one with
    // another author and one with other words.
    change(&mut app, |state| {
        let mut messages: Vec<_> = state
            .messages
            .iter()
            .map(|message| Arc::new((**message).clone()))
            .collect();
        Arc::make_mut(&mut messages[36]).author = "Someone else".into();
        Arc::make_mut(&mut messages[38]).body = "Other words".into();
        state.messages = Arc::new(messages);
    });
    for (index, (look, before)) in app.timeline.looks.iter().zip(&laid).enumerate() {
        assert_eq!(Arc::ptr_eq(&look.body, before), index != 38, "row {index}");
    }
    assert!(shows(&app, "Other words"));
}

mod arrange;
mod calls;
mod cards;
mod channel_settings;
mod dialogs;
mod editor;
mod header;
mod link_dialogs;
mod media;
mod menus;
mod people;
mod polls;
mod rows;
mod server;
mod server_dialogs;
mod settings;
mod shell;
mod threads;
mod widgets;
mod writing;

/// The time now, in Unix milliseconds.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

#[test]
fn at_midnight_the_message_times_are_worked_out_again() {
    let (mut app, _folder) = preview();
    assert!(
        app.timeline.clocks.day_armed,
        "a timer to midnight while a timeline shows"
    );
    let time = app.timeline.looks[0].time.clone();
    app.timeline.looks[0].time = "as at noon yesterday".into();
    let _ = app.update(Message::Day);
    assert_eq!(app.timeline.looks[0].time, time, "worked out for now");
    assert!(
        app.timeline.clocks.day_armed,
        "and the next midnight's armed"
    );
}

#[test]
fn the_minute_timer_runs_only_while_something_shows_minutes() {
    let (mut app, _folder) = preview();
    // Nothing in the preview shows minutes: no timer.
    app.roster.activity_cards.clear();
    app.timeline.clocks = Default::default();
    let _ = crate::timeline::clocks::arm_clocks(&mut app);
    assert!(!app.timeline.clocks.minute_armed);
    // A call's length in the sidebar does.
    app.timeline.clocks.calls = true;
    let _ = crate::timeline::clocks::arm_clocks(&mut app);
    assert!(app.timeline.clocks.minute_armed);
    // The minute turns; with nothing left to show it, none is armed again.
    app.timeline.clocks.calls = false;
    let _ = app.update(Message::Minute);
    assert!(!app.timeline.clocks.minute_armed);
}
