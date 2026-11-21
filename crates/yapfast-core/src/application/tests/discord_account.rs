//! The Discord account: linking it, several accounts and switching between
//! them, its status, and the game it shares.

use super::*;

#[test]
#[cfg(feature = "discord")]
fn links_discord_by_qr_code_and_unlinks_it() {
    use crate::discord_link::Progress;
    use yapfast_discord::service::{Snapshot, Status};
    // Signed in to Matrix, Yapfast links Discord from settings and connects it.
    let mut app = connected();
    assert!(matches!(
        app.dispatch(Action::LinkDiscord).as_slice(),
        [Effect::LinkDiscord]
    ));
    assert_eq!(app.state.discord_link, Some(DiscordLink::Starting));
    app.linking(Progress::Code("https://discord.com/ra/one".into()));
    assert_eq!(
        app.state.discord_link,
        Some(DiscordLink::Code("https://discord.com/ra/one".into()))
    );
    // An expired code makes way for a new one.
    assert!(matches!(
        app.linking(Progress::Expired).as_slice(),
        [Effect::LinkDiscord]
    ));
    assert_eq!(app.state.discord_link, Some(DiscordLink::Starting));
    // Without a code first, it's a failure, not a loop.
    assert!(app.linking(Progress::Expired).is_empty());
    assert!(matches!(
        app.state.discord_link,
        Some(DiscordLink::Failed(_))
    ));
    app.dispatch(Action::LinkDiscord);
    app.linking(Progress::Scanned("alice".into()));
    assert_eq!(
        app.state.discord_link,
        Some(DiscordLink::Scanned("alice".into()))
    );
    // Discord wants a captcha: the browser opens its page, and the dialog
    // can open it again.
    let page = "http://127.0.0.1:9/captcha/1".to_owned();
    assert!(matches!(
        app.linking(Progress::Captcha(page.clone())).as_slice(),
        [Effect::OpenPage(opened)] if *opened == page
    ));
    assert_eq!(
        app.state.discord_link,
        Some(DiscordLink::Captcha(page.clone()))
    );
    assert!(matches!(
        app.dispatch(Action::OpenDiscordCaptcha).as_slice(),
        [Effect::OpenPage(opened)] if *opened == page
    ));
    assert!(matches!(
        app.linking(Progress::Linked).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Start))]
    ));
    assert_eq!(app.state.discord_link, None);
    assert!(app.state.discord_linked);
    assert!(
        app.dispatch(Action::LinkDiscord).is_empty(),
        "already linked"
    );

    // Unlinking forgets everything Discord showed.
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        friends: vec!["2".into()].into(),
        ..Default::default()
    }));
    assert!(matches!(
        app.dispatch(Action::UnlinkDiscord).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Unlink))]
    ));
    app.apply(Event::DiscordUnlinked);
    assert!(!app.state.discord_linked);
    assert_eq!(app.state.discord_user, None);
    assert!(app.state.discord_friends.is_empty());

    // Stopping linking ignores what the stopped linking says next.
    app.dispatch(Action::LinkDiscord);
    assert!(matches!(
        app.dispatch(Action::CancelDiscordLink).as_slice(),
        [Effect::StopDiscordLink]
    ));
    assert!(app.linking(Progress::Linked).is_empty());
    assert!(!app.state.discord_linked);

    // Without a session, as in a build without Matrix, linking signs in.
    let mut app = Application::default();
    app.dispatch(Action::LinkDiscord);
    assert!(matches!(
        app.linking(Progress::Linked).as_slice(),
        [Effect::Start(chat::Start::Restore)]
    ));
    assert!(app.state.busy);
}

#[cfg(feature = "discord")]
fn linked_accounts(app: &mut Application, ids: &[&str], active: &str) {
    app.apply(Event::DiscordAccounts {
        accounts: ids
            .iter()
            .map(|id| chat::DiscordAccount {
                id: (*id).into(),
                username: format!("user{id}"),
                name: format!("User {id}"),
                avatar: None,
            })
            .collect(),
        active: active.into(),
    });
}

#[test]
#[cfg(feature = "discord")]
fn switches_between_linked_discord_accounts() {
    let mut app = connected();
    app.state.discord_linked = true;
    linked_accounts(&mut app, &["1", "2"], "1");
    assert_eq!(app.state.discord_active, "1");

    assert!(
        app.dispatch(Action::SwitchDiscordAccount("1".into()))
            .is_empty(),
        "already active"
    );
    assert!(
        app.dispatch(Action::SwitchDiscordAccount("9".into()))
            .is_empty(),
        "not linked"
    );
    assert!(matches!(
        app.dispatch(Action::SwitchDiscordAccount("2".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::SwitchAccount(id)))] if id == "2"
    ));

    // What the account before showed goes, and linking stays on.
    app.state.account_name = Some("User 1".into());
    app.state.add_friend = Some(AddFriend::default());
    let room = "discord:10:21";
    app.state
        .drafts
        .insert(room.into(), "account one's draft".into());
    app.state.drafts.insert("dm".into(), "matrix draft".into());
    app.apply(Event::DiscordSwitched);
    linked_accounts(&mut app, &["1", "2"], "2");
    assert_eq!(app.state.account_name, None);
    assert!(app.state.add_friend.is_none());
    assert!(app.state.discord_linked);
    assert_eq!(app.state.discord_active, "2");
    assert_eq!(app.state.draft_in(room), "");
    assert_eq!(app.state.draft_in("dm"), "matrix draft");
    app.state
        .drafts
        .insert(room.into(), "account two's draft".into());
    app.apply(Event::DiscordSwitched);
    linked_accounts(&mut app, &["1", "2"], "1");
    assert_eq!(app.state.draft_in(room), "account one's draft");
}

#[cfg(feature = "discord")]
#[test]
fn reconnects_the_linked_discord_account_without_a_matrix_session() {
    let mut app = Application::default();
    app.state.discord_linked = true;
    assert!(matches!(
        app.dispatch(Action::RetrySync).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Reconnect))]
    ));
    app.state.discord_online = true;
    assert!(app.dispatch(Action::RetrySync).is_empty());
}

#[test]
#[cfg(feature = "discord")]
fn a_login_linked_before_accounts_were_several_gets_its_name_from_the_connection() {
    use yapfast_discord::service::{Account, Snapshot, Status};
    let mut app = connected();
    app.state.discord_linked = true;
    // As stored for a migrated login: the id, and nothing of the account.
    let stored = || Event::DiscordAccounts {
        accounts: vec![chat::DiscordAccount {
            id: "1400000000000000001".into(),
            ..Default::default()
        }],
        active: "1400000000000000001".into(),
    };
    app.apply(stored());
    // Never blank: the id stands in for the name.
    assert_eq!(app.state.discord_accounts[0].label(), "1400000000000000001");

    let account = |name: &str| Account {
        username: "derfayberr".into(),
        name: name.into(),
        avatar: "https://cdn.test/avatars/1/a.png".into(),
        decoration: None,
        nameplate: None,
        premium_type: 0,
        mfa_enabled: false,
        has_email: true,
        nsfw_allowed: false,
        nsfw_commands_in_dms: false,
    };
    let snapshot = |name: &str| {
        discord_event(Snapshot {
            status: Status::Online,
            user_id: Some("1400000000000000001".into()),
            account: Some(account(name)),
            ..Default::default()
        })
    };
    app.apply(snapshot("Fayber"));
    let row = &app.state.discord_accounts[0];
    assert_eq!(row.label(), "derfayberr");
    assert_eq!(row.name, "Fayber");
    assert_eq!(
        row.avatar.as_deref(),
        Some("https://cdn.test/avatars/1/a.png")
    );

    // The stored list read again, before the profile was stored, keeps it.
    app.apply(stored());
    assert_eq!(app.state.discord_accounts[0].name, "Fayber");
    // A change of name shows.
    app.apply(snapshot("Fabian"));
    assert_eq!(app.state.discord_accounts[0].name, "Fabian");
}

#[test]
#[cfg(feature = "discord")]
fn an_account_row_is_never_blank() {
    let row = |id: &str, username: &str, name: &str| chat::DiscordAccount {
        id: id.into(),
        username: username.into(),
        name: name.into(),
        avatar: None,
    };
    assert_eq!(row("1", "bea", "Bea").label(), "bea");
    assert_eq!(row("1", "", "Bea").label(), "Bea");
    assert_eq!(row("14", "", "").label(), "14");
    assert_eq!(row("login-00ff", "", "").label(), "Account");
    assert_eq!(row("", "", "").label(), "Account");
}

#[test]
#[cfg(feature = "discord")]
fn manages_linked_discord_accounts() {
    use crate::discord_link::Progress;
    let mut app = connected();
    app.state.discord_linked = true;
    assert!(
        app.dispatch(Action::LinkDiscord).is_empty(),
        "linked accounts are added from Manage Accounts"
    );
    app.dispatch(Action::OpenManageAccounts);
    assert!(app.state.manage_accounts.is_none(), "none linked to manage");
    linked_accounts(&mut app, &["1", "2"], "1");
    app.dispatch(Action::OpenManageAccounts);
    assert!(app.state.manage_accounts.is_some());
    assert!(app.state.dialog_open());

    // The "more" menu opens for one account, closes when opened again, and
    // closes with an empty id.
    app.dispatch(Action::OpenAccountMenu("2".into()));
    assert_eq!(
        app.state
            .manage_accounts
            .as_ref()
            .and_then(|d| d.menu.as_deref()),
        Some("2")
    );
    app.dispatch(Action::OpenAccountMenu("2".into()));
    assert!(app.state.manage_accounts.as_ref().unwrap().menu.is_none());

    // Logging the other account out asks only the backend.
    assert!(matches!(
        app.dispatch(Action::LogOutDiscordAccount("2".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::RemoveAccount(id)))] if id == "2"
    ));
    assert!(
        app.dispatch(Action::LogOutDiscordAccount("9".into()))
            .is_empty(),
        "not linked"
    );

    // Adding an account links beside those linked, and the new one connects.
    assert!(matches!(
        app.dispatch(Action::LinkDiscord).as_slice(),
        [Effect::LinkDiscord]
    ));
    assert_eq!(app.state.discord_link, Some(DiscordLink::Starting));
    assert!(matches!(
        app.linking(Progress::Linked).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Start))]
    ));

    // Closing it stops linking under way.
    app.dispatch(Action::LinkDiscord);
    assert!(matches!(
        app.dispatch(Action::CloseManageAccounts).as_slice(),
        [Effect::StopDiscordLink]
    ));
    assert!(app.state.manage_accounts.is_none());

    // The active account logs out of settings without the others.
    assert!(matches!(
        app.dispatch(Action::UnlinkDiscord).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::RemoveAccount(id)))] if id == "1"
    ));

    // The last one leaving closes the dialog.
    linked_accounts(&mut app, &[], "");
    assert!(app.state.manage_accounts.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn links_discord_by_token() {
    use crate::discord_link::Progress;
    let mut app = Application::default();
    assert!(
        app.dispatch(Action::LinkDiscordToken("  ".into()))
            .is_empty(),
        "nothing pasted"
    );
    assert!(matches!(
        app.dispatch(Action::LinkDiscordToken("abc.def".into())).as_slice(),
        [Effect::LinkDiscordToken(token)] if token == "abc.def"
    ));
    assert_eq!(app.state.discord_link, Some(DiscordLink::Checking));
    app.linking(Progress::Failed("Discord didn't accept that token.".into()));
    assert!(matches!(
        app.state.discord_link,
        Some(DiscordLink::Failed(_))
    ));
    // Pasted again, it signs in as linking by QR code does.
    app.dispatch(Action::LinkDiscordToken("abc.def".into()));
    assert!(matches!(
        app.linking(Progress::Linked).as_slice(),
        [Effect::Start(chat::Start::Restore)]
    ));
    assert!(app.state.discord_linked);
}

#[test]
#[cfg(feature = "discord")]
fn first_import_keeps_its_partial_failure_notice_while_connecting() {
    use crate::discord_link::Progress;
    let mut app = Application::default();
    app.dispatch(Action::ImportDiscord);
    let message = "Imported 1 account. 2 sessions could not be linked. 1 unreadable profile.";
    assert!(matches!(
        app.linking(Progress::Imported(message.into())).as_slice(),
        [Effect::Start(chat::Start::Restore)]
    ));
    assert_eq!(app.state.notice.as_deref(), Some(message));
    assert!(app.state.busy);
}

#[test]
#[cfg(feature = "discord")]
fn distinguishes_failed_import_from_failed_qr_linking() {
    use crate::discord_link::Progress;
    let mut app = Application::default();
    app.dispatch(Action::LinkDiscord);
    app.linking(Progress::Failed("QR failure".into()));
    assert_eq!(
        app.state.discord_link,
        Some(DiscordLink::Failed("QR failure".into()))
    );
    app.dispatch(Action::ImportDiscord);
    app.linking(Progress::Failed("Import failure".into()));
    assert_eq!(
        app.state.discord_link,
        Some(DiscordLink::ImportFailed("Import failure".into()))
    );
}

#[test]
#[cfg(feature = "discord")]
fn cancelling_a_first_import_reports_saved_accounts_without_connecting() {
    use crate::discord_link::Progress;
    let mut app = Application::default();
    app.dispatch(Action::ImportDiscord);
    app.dispatch(Action::CancelDiscordLink);
    let message = "Import cancelled. Imported 1 account.";
    assert!(
        app.linking(Progress::ImportCancelled {
            message: message.into(),
            linked: true,
            save_pending: false
        })
        .is_empty()
    );
    assert_eq!(app.state.discord_link, None);
    assert!(app.state.discord_linked);
    assert!(app.state.account.is_none());
    assert!(!app.state.busy);
    assert_eq!(app.state.notice.as_deref(), Some(message));
    assert!(!app.state.notice.as_ref().unwrap().is_error());
    app.apply(Event::Error("offline".into()));
    assert!(app.state.notice.as_ref().unwrap().is_error());
    assert!(matches!(
        app.dispatch(Action::Restore).as_slice(),
        [Effect::Start(chat::Start::Restore)]
    ));
}

#[test]
#[cfg(feature = "discord")]
fn cancelling_before_queued_import_success_does_not_start_a_connection() {
    use crate::discord_link::Progress;
    let mut app = Application::default();
    app.dispatch(Action::ImportDiscord);
    app.dispatch(Action::CancelDiscordLink);
    assert!(
        app.linking(Progress::Imported("Imported 1 account.".into()))
            .is_empty()
    );
    assert_eq!(
        app.state.notice.as_deref(),
        Some("Import cancelled. Imported 1 account.")
    );
    assert!(!app.state.busy);
}

#[test]
#[cfg(feature = "discord")]
fn a_cancel_deadline_releases_linking_and_a_late_save_refreshes_accounts() {
    use crate::discord_link::Progress;
    let mut app = connected();
    app.state.discord_linked = true;
    linked_accounts(&mut app, &["1"], "1");
    app.dispatch(Action::OpenManageAccounts);
    app.dispatch(Action::ImportDiscord);
    assert!(matches!(
        app.dispatch(Action::CancelDiscordLink).as_slice(),
        [Effect::CancelDiscordImport]
    ));
    assert!(
        app.linking(Progress::ImportCancelled {
            message: "Import cancelled. A save may still finish.".into(),
            linked: false,
            save_pending: true,
        })
        .is_empty(),
        "do not read a store still held by the pending save"
    );
    assert_eq!(app.state.discord_link, None);
    assert!(!app.state.notice.as_ref().unwrap().is_error());
    assert!(matches!(
        app.dispatch(Action::LinkDiscord).as_slice(),
        [Effect::LinkDiscord]
    ));
    assert!(matches!(
        app.linking(Progress::ImportSaveFinished { linked: true })
            .as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::ListAccounts
        ))]
    ));
    assert_eq!(app.state.discord_link, Some(DiscordLink::Starting));
}

#[test]
#[cfg(feature = "discord")]
fn retries_and_cancels_discord_import_without_reconnecting_the_active_account() {
    use crate::discord_link::Progress;
    let mut app = Application::default();
    assert!(matches!(
        app.dispatch(Action::ImportDiscord).as_slice(),
        [Effect::ImportDiscord]
    ));
    assert_eq!(app.state.discord_link, Some(DiscordLink::Importing));
    assert!(
        app.dispatch(Action::ImportDiscord).is_empty(),
        "already importing"
    );
    app.linking(Progress::Failed("Close Discord and retry.".into()));
    assert!(matches!(
        app.dispatch(Action::ImportDiscord).as_slice(),
        [Effect::ImportDiscord]
    ));
    assert!(matches!(
        app.dispatch(Action::CancelDiscordLink).as_slice(),
        [Effect::CancelDiscordImport]
    ));
    assert_eq!(app.state.discord_link, Some(DiscordLink::CancellingImport));
    assert!(app.dispatch(Action::ImportDiscord).is_empty());
    assert!(app.dispatch(Action::LinkDiscord).is_empty());
    assert!(
        app.linking(Progress::ImportCancelled {
            message: "Import cancelled.".into(),
            linked: false,
            save_pending: false
        })
        .is_empty()
    );
    assert_eq!(app.state.discord_link, None);

    let mut app = connected();
    app.state.discord_linked = true;
    linked_accounts(&mut app, &["1"], "1");
    app.dispatch(Action::OpenManageAccounts);
    assert!(matches!(
        app.dispatch(Action::ImportDiscord).as_slice(),
        [Effect::ImportDiscord]
    ));
    assert!(matches!(
        app.dispatch(Action::CancelDiscordLink).as_slice(),
        [Effect::CancelDiscordImport]
    ));
    assert_eq!(app.state.discord_active, "1");
    assert_eq!(app.state.discord_link, Some(DiscordLink::CancellingImport));
    assert!(matches!(
        app.linking(Progress::ImportCancelled {
            message: "Import cancelled. Imported 1 account.".into(),
            linked: true,
            save_pending: false
        })
        .as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::ListAccounts
        ))]
    ));
    app.dispatch(Action::ImportDiscord);
    let effects = app.linking(Progress::Imported("Imported 1 account.".into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::ListAccounts
        ))]
    ));
    assert_eq!(app.state.discord_active, "1");
    assert!(
        app.state
            .notice
            .as_deref()
            .unwrap()
            .contains("Imported 1 account")
    );
}

#[test]
#[cfg(feature = "discord")]
fn the_add_friend_page_goes_when_the_friends_screen_does() {
    use yapfast_discord::service::{Snapshot, Status};
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::DirectMessages));
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    app.dispatch(Action::OpenAddFriend);
    assert!(app.state.add_friend.is_some());
    // The friends screen again leaves it, friend requests are another screen.
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    assert!(app.state.add_friend.is_some());
    app.dispatch(Action::OpenDmScreen(DmScreen::Requests));
    assert!(app.state.add_friend.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn adds_friends_by_username_and_sets_the_custom_status() {
    use yapfast_discord::service::{Snapshot, Status};
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        ..Default::default()
    }));

    app.dispatch(Action::OpenAddFriend);
    assert!(matches!(
        app.dispatch(Action::AddFriend(" @Erin ".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::AddFriend { username }))] if username == "erin"
    ));
    assert!(
        app.dispatch(Action::AddFriend("erin".into())).is_empty(),
        "one request at a time"
    );
    app.apply(Event::FriendAdded(Err(
        "Discord has nobody by that username.".into(),
    )));
    let dialog = app.state.add_friend.as_ref().expect("still open");
    assert!(!dialog.sending);
    assert!(matches!(&dialog.outcome, Some(Err(_))));

    app.dispatch(Action::OpenStatusEditor);
    assert_eq!(
        app.dispatch(Action::SetCustomStatus {
            emoji: Some(":pog:".into()),
            text: "Hi".into(),
            clear_after: ClearAfter::Never,
        })
        .len(),
        0
    );
    assert_eq!(
        app.state
            .status_editor
            .as_ref()
            .and_then(|editor| editor.error.as_deref()),
        Some("A custom emoji in a status needs Discord Nitro.")
    );
    let effects = app.dispatch(Action::SetCustomStatus {
        emoji: Some("🔧".into()),
        text: " Fixing the bridge ".into(),
        clear_after: ClearAfter::Minutes(30),
    });
    let [Effect::Backend(Command::Discord(DiscordCommand::SetCustomStatus(Some(status))))] =
        effects.as_slice()
    else {
        panic!("the status goes to Discord");
    };
    assert_eq!(status.text, "Fixing the bridge");
    assert_eq!(status.emoji.as_deref(), Some("🔧"));
    let left = status.expires_at_ms.unwrap() as i64 - chrono::Local::now().timestamp_millis();
    assert!((29 * 60_000..=30 * 60_000).contains(&left));
    app.apply(Event::CustomStatusSet(Ok(())));
    assert!(app.state.status_editor.is_none());

    // Nothing to say clears it.
    app.dispatch(Action::OpenStatusEditor);
    assert!(matches!(
        app.dispatch(Action::SetCustomStatus {
            emoji: None,
            text: " ".into(),
            clear_after: ClearAfter::Today,
        })
        .as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::SetCustomStatus(None)
        ))]
    ));
}

#[test]
#[cfg(feature = "discord")]
fn discord_dialogs_stop_waiting_when_the_backend_cant_answer() {
    use yapfast_discord::service::{Snapshot, Status};
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        ..Default::default()
    }));
    app.dispatch(Action::OpenAddFriend);
    let effects = app.dispatch(Action::AddFriend("erin".into()));
    let [Effect::Backend(command)] = effects.as_slice() else {
        panic!("the request goes to Discord");
    };
    app.backend_failed(command);
    let dialog = app.state.add_friend.as_ref().expect("still open");
    assert!(!dialog.sending);
    assert!(matches!(&dialog.outcome, Some(Err(_))));

    // The backend stopping ends the status editor's and a profile's waits.
    app.dispatch(Action::OpenStatusEditor);
    app.dispatch(Action::SetCustomStatus {
        emoji: None,
        text: "Hi".into(),
        clear_after: ClearAfter::Never,
    });
    assert!(
        app.state
            .status_editor
            .as_ref()
            .is_some_and(|editor| editor.saving)
    );
    app.state.profile = Some(Arc::new(ProfileView {
        profile: Profile {
            id: "2".into(),
            ..Default::default()
        },
        large: false,
        loading: true,
        kept: false,
        note: None,
        ..Default::default()
    }));
    app.stopped();
    let editor = app.state.status_editor.as_ref().expect("still open");
    assert!(!editor.saving && editor.error.is_some());
    let profile = app.state.profile.as_ref().expect("still open");
    assert!(!profile.loading && profile.note.is_some());
}

#[test]
fn clears_a_status_today_at_midnight() {
    use chrono::{Datelike, Local, TimeZone, Timelike};
    let evening = Local.with_ymd_and_hms(2026, 10, 7, 21, 30, 0).unwrap();
    let at = ClearAfter::Today.at(evening).unwrap();
    let midnight = Local.timestamp_millis_opt(at as i64).unwrap();
    assert_eq!(
        (midnight.day(), midnight.hour(), midnight.minute()),
        (8, 0, 0)
    );
    assert_eq!(ClearAfter::Never.at(evening), None);
}

#[test]
#[cfg(feature = "discord")]
fn shares_the_game_as_the_account_settings_say() {
    use yapfast_discord::{
        own_activity::Playing,
        service::{Snapshot, Status},
    };
    let mut app = Application::default();
    let snapshot = |show_game: bool, playing: Option<&str>| Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        show_game,
        playing: playing.map(|name| {
            Arc::new(Playing {
                name: name.into(),
                started_ms: Some(5),
            })
        }),
        ..Default::default()
    };
    app.apply(discord_event(snapshot(true, Some("Game"))));
    assert!(app.state.discord_show_game);
    assert_eq!(
        app.state.discord_playing,
        Some(chat::Playing {
            name: "Game".into(),
            started_ms: Some(5)
        })
    );
    assert!(
        app.dispatch(Action::ShowGame(true)).is_empty(),
        "it's shared already"
    );
    assert!(matches!(
        app.dispatch(Action::ShowGame(false)).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ShowGame(
            false
        )))]
    ));
    assert!(!app.state.discord_show_game, "the switch moves at once");
    app.apply(Event::ShowGameSet(Err("Discord said no.".into())));
    assert!(app.state.discord_show_game, "and back when Discord refuses");
    assert!(app.state.notice.is_some());
    app.apply(discord_event(snapshot(false, None)));
    assert!(!app.state.discord_show_game);
    assert_eq!(app.state.discord_playing, None);
}

#[test]
#[cfg(feature = "discord")]
fn sets_the_status_and_holds_notifications_while_busy() {
    use yapfast_discord::service::{Snapshot, Status};
    let mut app = Application::default();
    let snapshot = |presence: &str| Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        presence: presence.into(),
        ..Default::default()
    };
    app.apply(discord_event(snapshot("online")));
    assert!(
        app.dispatch(Action::SetStatus("online".into())).is_empty(),
        "it's already online"
    );
    assert!(matches!(
        app.dispatch(Action::SetStatus("dnd".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::SetStatus(status)))] if status == "dnd"
    ));
    app.apply(Event::StatusSet(Ok(())));
    app.apply(discord_event(snapshot("dnd")));
    assert_eq!(app.state.discord_presence.as_deref(), Some("dnd"));
    let notify = |app: &mut Application| {
        app.apply(Event::Notify(chat::Notification {
            room: "dm".into(),
            author: "Sam".into(),
            body: "Around?".into(),
            avatar: None,
            mention: false,
        }))
    };
    assert!(
        notify(&mut app).is_empty(),
        "Do Not Disturb holds them back"
    );
    app.apply(discord_event(snapshot("idle")));
    assert_eq!(notify(&mut app).len(), 1);
    app.apply(Event::StatusSet(Err("Discord said no.".into())));
    assert_eq!(
        app.state.notice.as_deref(),
        Some("Couldn't change your status: Discord said no.")
    );
}

#[test]
fn going_idle_while_signed_out_tells_nobody() {
    let inactive = || Action::Inactive {
        since: Some(1),
        show_idle: true,
    };
    let mut app = Application::default();
    assert!(app.dispatch(inactive()).is_empty());
    let mut app = connected();
    assert!(matches!(
        app.dispatch(inactive()).as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::Inactive { .. }
        ))]
    ));
}
