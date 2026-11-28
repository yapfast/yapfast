//! Privacy pages exercise existing preferences, account gating and browser
//! destinations with the preview's fixtures. No account or browser is used.

use iced_test::Simulator;
use yapfast_core::{
    application::{Action, DmScreen, Navigation},
    chat::{Command, DiscordCommand, Event as BackendEvent},
};

use super::{change, interact, preview, shows, start};
use crate::{
    app::{App, Message},
    settings::{
        Event, Section,
        privacy::{Event as PrivacyEvent, Handoff},
    },
};

fn open(app: &mut App, section: Section) {
    app.shell.frame.height = 2400.0;
    crate::window_bar::fit_window(app);
    let _ = app.update(crate::app::settings(Event::Open(section)));
}

fn linked(app: &mut App) {
    change(app, |state| {
        state.account = Some("42".into());
        state.preview = false;
        state.discord_linked = true;
        state.discord_user = Some("42".into());
    });
}

fn click(app: &mut App, label: &str) {
    interact(app, |ui| {
        ui.click(label).expect(label);
    });
}

fn handoff(app: &App, label: &str) -> Handoff {
    let mut ui = Simulator::with_size(iced::Settings::default(), app.shell.frame, app.view());
    ui.click(label).expect(label);
    let mut messages = ui.into_messages();
    let Some(Message::Settings(event)) = messages.next() else {
        panic!("a browser handoff from {label}");
    };
    let Event::Privacy(PrivacyEvent::Open(destination)) = *event else {
        panic!("a browser handoff from {label}");
    };
    assert!(messages.next().is_none(), "one explicit action");
    destination
}

#[test]
fn settings_privacy_pages_offer_explicit_official_browser_handoffs() {
    let (mut app, _folder) = preview();
    let commands_before = app.client.sent.len();
    for (section, title, destination, path) in [
        (
            Section::ContentSocial,
            "Content & Social",
            Handoff::ContentSocial,
            "217916488",
        ),
        (
            Section::DataPrivacy,
            "Data & Privacy",
            Handoff::DataPrivacy,
            "/privacy/policy/",
        ),
        (
            Section::FamilyCenter,
            "Family Center",
            Handoff::FamilyCenter,
            "14155043715735",
        ),
        (
            Section::FriendRequests,
            "Friend Requests",
            Handoff::FriendRequests,
            "217916488",
        ),
        (
            Section::StreamerMode,
            "Streamer Mode",
            Handoff::StreamerMode,
            "218485407",
        ),
        (
            Section::ActivityPrivacy,
            "Activity Privacy",
            Handoff::ActivityPrivacy,
            "7931156448919",
        ),
    ] {
        open(&mut app, section);
        assert!(shows(&app, title), "{title}");
        assert!(shows(&app, "IN DISCORD"));
        let guide = if section == Section::DataPrivacy {
            "Read Discord's privacy policy"
        } else {
            "Read Discord's guide"
        };
        assert_eq!(handoff(&app, guide), destination);
        assert!(destination.url().contains(path));
        assert!(
            destination
                .url()
                .starts_with("https://support.discord.com/")
                || destination == Handoff::DataPrivacy
        );
        let browser = handoff(&app, "Open Discord in browser");
        assert_eq!(browser, Handoff::Discord);
        assert_eq!(browser.url(), "https://discord.com/channels/@me");
    }
    assert_eq!(
        app.client.sent.len(),
        commands_before,
        "no account commands from browsing pages"
    );
}

#[test]
fn settings_privacy_sections_are_searchable_and_navigable() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Account);
    let _ = app.update(crate::app::settings(Event::Search("privacy".into())));
    assert!(!shows(&app, "Family Center"));
    click(&mut app, "Data & Privacy");
    assert_eq!(
        app.win.settings.get().expect("settings").section(),
        Section::DataPrivacy
    );
    click(&mut app, "Activity Privacy");
    assert_eq!(
        app.win.settings.get().expect("settings").section(),
        Section::ActivityPrivacy
    );
}

#[test]
fn settings_privacy_content_choices_reach_the_core_and_survive_restart() {
    let (mut app, folder) = preview();
    linked(&mut app);
    open(&mut app, Section::ContentSocial);
    click(&mut app, "Show link previews");
    click(&mut app, "Mention people you reply to");
    click(&mut app, "Hide messages from people you blocked");
    assert!(!app.client.state().reply_ping);
    assert!(app.client.state().hide_blocked);
    let again = start(&folder, true);
    assert_eq!(again.layout.prefs.link_previews, Some(false));
    assert!(!again.client.state().reply_ping);
    assert!(again.client.state().hide_blocked);
}

#[test]
fn settings_privacy_data_choices_reach_the_core_and_survive_restart() {
    let (mut app, folder) = preview();
    linked(&mut app);
    open(&mut app, Section::DataPrivacy);
    click(&mut app, "Remove tracking from links you send");
    click(&mut app, "Mark messages read on Discord");
    assert!(!app.client.state().strip_tracking);
    assert!(!app.client.state().sync_discord_reads);
    let again = start(&folder, true);
    assert!(!again.client.state().strip_tracking);
    assert!(!again.client.state().sync_discord_reads);
}

#[test]
fn settings_privacy_streamer_controls_keep_the_real_notification_preferences() {
    let (mut app, folder) = preview();
    open(&mut app, Section::StreamerMode);
    assert!(shows(
        &app,
        "Prepare Yapfast for sharing your screen. Names, messages and invite links remain visible; there is no automatic streamer mode."
    ));
    click(&mut app, "Show message text in notifications");
    click(&mut app, "Show desktop notifications");
    click(&mut app, "Flash the taskbar");
    let again = start(&folder, true);
    assert_eq!(again.layout.prefs.notification_text, Some(false));
    assert_eq!(again.layout.prefs.notifications, Some(false));
    assert_eq!(again.layout.prefs.flash_taskbar, Some(false));
}

#[test]
fn settings_privacy_activity_sharing_uses_the_account_command_and_existing_failure_path() {
    let (mut app, folder) = preview();
    linked(&mut app);
    change(&mut app, |state| state.discord_show_game = true);
    open(&mut app, Section::ActivityPrivacy);
    click(&mut app, "Let others see when you're typing");
    click(&mut app, "Show me as idle when I'm away");
    assert!(!app.client.share_typing);
    app.client.sent.clear();
    click(&mut app, "Share the game you play");
    assert!(
        app.client
            .sent
            .iter()
            .any(|command| matches!(command, Command::Discord(DiscordCommand::ShowGame(false))))
    );
    // Exercise a refused account save with fixture state, without a backend.
    change(&mut app, |state| state.discord_show_game = false);
    app.client
        .apply(BackendEvent::ShowGameSet(Err("fixture refusal".into())));
    assert!(app.client.state().discord_show_game);
    assert!(
        app.client
            .state()
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("fixture refusal"))
    );
    let again = start(&folder, true);
    assert!(!again.client.share_typing);
    assert_eq!(again.layout.prefs.show_idle, Some(false));
}

#[test]
fn settings_privacy_account_controls_are_absent_in_preview_or_without_an_account() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        state.account = Some("42".into());
        state.discord_linked = true;
        state.discord_user = Some("42".into());
    });
    for preview in [true, false] {
        change(&mut app, |state| {
            state.preview = preview;
            if !preview {
                state.discord_user = None;
            }
        });
        for (section, hidden) in [
            (
                Section::ContentSocial,
                "Hide messages from people you blocked",
            ),
            (Section::DataPrivacy, "Mark messages read on Discord"),
            (Section::FriendRequests, "Review requests in Yapfast"),
            (Section::ActivityPrivacy, "Share the game you play"),
        ] {
            open(&mut app, section);
            assert!(!shows(&app, hidden), "{hidden}");
            assert!(shows(&app, "Open Discord in browser"));
        }
    }
}

#[test]
fn settings_privacy_requests_opens_the_real_requests_screen_from_a_server() {
    let (mut app, _folder) = preview();
    linked(&mut app);
    app.client
        .dispatch(Action::Navigate(Navigation::Server("friends".into())));
    let _ = app.sync();
    open(&mut app, Section::FriendRequests);
    click(&mut app, "Review requests in Yapfast");
    assert!(!app.win.settings.is_open());
    assert_eq!(app.client.state().navigation, Navigation::DirectMessages);
    assert_eq!(app.client.state().dm_screen, Some(DmScreen::Requests));
}
