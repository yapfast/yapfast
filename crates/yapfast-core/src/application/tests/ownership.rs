//! Delete Server and Transfer Ownership in a Discord server's settings.

use super::*;
use crate::chat::{JoinMethod, ListedMember, RoleMember, ServerSettingsPage, SettingsSection};

const SERVER: &str = "discord:10";

/// `discord_server` owned by Alice, "1", with two-factor authentication on
/// or off, and an email address or none.
fn owned(mfa: bool, email: bool) -> Application {
    let mut app = discord_server();
    change_server(&mut app, |server| server.owner_id = Some("1".into()));
    account(&mut app, mfa, email);
    app
}

fn account(app: &mut Application, mfa: bool, email: bool) {
    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    snapshot.account = Some(yapfast_discord::service::Account {
        username: String::new(),
        name: "Alice".into(),
        avatar: String::new(),
        decoration: None,
        nameplate: None,
        premium_type: 0,
        mfa_enabled: mfa,
        has_email: email,
    });
    app.apply(discord_event(snapshot));
}

fn ownership(app: &Application) -> Option<&Ownership> {
    app.state.server_settings.as_ref()?.ownership.as_ref()
}

fn sent(effects: Vec<Effect>) -> Option<DiscordCommand> {
    match <[_; 1]>::try_from(effects).ok()? {
        [Effect::Backend(Command::Discord(command))] => Some(command),
        _ => None,
    }
}

fn member(id: &str, bot: bool) -> ListedMember {
    ListedMember {
        person: RoleMember {
            id: id.into(),
            name: format!("user{id}"),
            username: Some(format!("user{id}")),
            avatar: String::new(),
        },
        roles: vec![],
        joined: 0,
        timed_out_until: 0,
        unusual_dms: false,
        spammer: false,
        quarantined: false,
        joined_by: JoinMethod::Unknown,
        bot,
    }
}

/// Members open with Alice, Bob and a bot.
fn members(app: &mut Application) {
    app.dispatch(Action::OpenServerSettings(SERVER.into()));
    app.dispatch(Action::ServerSettingsSection(SettingsSection::Members));
    app.apply(Event::ServerSettingsLoaded {
        server: SERVER.into(),
        section: SettingsSection::Members,
        result: Ok(ServerSettingsPage::Members {
            members: vec![member("1", false), member("2", false), member("9", true)],
            total: Some(3),
        }),
    });
}

#[test]
fn only_an_owner_who_meets_the_servers_2fa_rule_may_give_it_up() {
    let mut app = discord_server();
    assert!(!app.may_give_up(SERVER), "Bob owns it");
    change_server(&mut app, |server| server.owner_id = Some("1".into()));
    assert!(app.may_give_up(SERVER));
    // Moderators need 2FA: so does the owner, to delete it.
    change_server(&mut app, |server| server.mfa_level = 1);
    assert!(!app.may_give_up(SERVER));
    account(&mut app, true, true);
    assert!(app.may_give_up(SERVER));
}

#[test]
fn deleting_without_2fa_takes_the_servers_name_in_any_case() {
    let mut app = owned(false, true);
    app.dispatch(Action::OpenServerSettings(SERVER.into()));
    app.dispatch(Action::Ownership(OwnershipInput::Delete));
    let Some(Ownership::Delete(ask)) = ownership(&app) else {
        panic!("no dialog");
    };
    assert!(ask.by_name && ask.name == "Test");
    app.dispatch(Action::Ownership(OwnershipInput::Type("Test ".into())));
    assert!(sent(app.dispatch(Action::Ownership(OwnershipInput::Confirm))).is_none());
    assert!(matches!(ownership(&app), Some(Ownership::Delete(ask)) if ask.wrong));
    app.dispatch(Action::Ownership(OwnershipInput::Type("tEST".into())));
    assert!(matches!(
        sent(app.dispatch(Action::Ownership(OwnershipInput::Confirm))),
        Some(DiscordCommand::DeleteServer { server }) if server == SERVER
    ));
    // While Discord deletes it, the settings stay.
    app.dispatch(Action::Ownership(OwnershipInput::Cancel));
    app.dispatch(Action::CloseServerSettings);
    assert!(ownership(&app).is_some());
    app.apply(Event::ServerDeleted {
        server: SERVER.into(),
        result: Err("Couldn't delete the server: no".into()),
    });
    assert!(matches!(ownership(&app), Some(Ownership::Delete(ask))
        if !ask.sending && ask.error.is_some()));
    app.dispatch(Action::Ownership(OwnershipInput::Confirm));
    app.apply(Event::ServerDeleted {
        server: SERVER.into(),
        result: Ok(()),
    });
    assert!(app.state.server_settings.is_none());
}

#[test]
fn deleting_with_2fa_asks_only_to_confirm() {
    let mut app = owned(true, true);
    app.dispatch(Action::OpenServerSettings(SERVER.into()));
    app.dispatch(Action::Ownership(OwnershipInput::Delete));
    assert!(matches!(ownership(&app), Some(Ownership::Delete(ask)) if !ask.by_name));
    assert!(sent(app.dispatch(Action::Ownership(OwnershipInput::Confirm))).is_some());
}

#[test]
fn transfer_ownership_is_offered_on_members_but_not_to_oneself_or_a_bot() {
    let mut app = owned(true, true);
    members(&mut app);
    assert!(
        app.person_choices("2")
            .contains(&MenuChoice::TransferOwnership)
    );
    assert!(
        !app.person_choices("1")
            .contains(&MenuChoice::TransferOwnership)
    );
    assert!(
        !app.person_choices("9")
            .contains(&MenuChoice::TransferOwnership)
    );
    // Not for Bob's server.
    let mut app = discord_server();
    members(&mut app);
    assert!(
        !app.person_choices("2")
            .contains(&MenuChoice::TransferOwnership)
    );
    // Nor a bot, by id.
    let mut app = owned(true, true);
    members(&mut app);
    app.dispatch(Action::Ownership(OwnershipInput::Transfer("9".into())));
    assert!(ownership(&app).is_none());
}

#[test]
fn transfer_with_2fa_goes_once_acknowledged() {
    let mut app = owned(true, true);
    members(&mut app);
    app.dispatch(Action::Ownership(OwnershipInput::Transfer("2".into())));
    assert!(matches!(ownership(&app), Some(Ownership::Transfer(ask))
        if ask.to.name == "user2" && !ask.ready()));
    assert!(sent(app.dispatch(Action::Ownership(OwnershipInput::Confirm))).is_none());
    app.dispatch(Action::Ownership(OwnershipInput::Agree));
    assert!(matches!(
        sent(app.dispatch(Action::Ownership(OwnershipInput::Confirm))),
        Some(DiscordCommand::TransferServer { server, user, code: None })
            if server == SERVER && user == "2"
    ));
    app.apply(Event::ServerTransferred {
        server: SERVER.into(),
        result: Ok(()),
    });
    assert!(app.state.server_settings.is_none());
}

#[test]
fn transfer_without_2fa_takes_the_emailed_code() {
    let mut app = owned(false, true);
    members(&mut app);
    app.dispatch(Action::Ownership(OwnershipInput::Transfer("2".into())));
    app.dispatch(Action::Ownership(OwnershipInput::Agree));
    assert!(matches!(
        sent(app.dispatch(Action::Ownership(OwnershipInput::Confirm))),
        Some(DiscordCommand::SendServerPincode { server }) if server == SERVER
    ));
    app.apply(Event::ServerPincodeSent {
        server: SERVER.into(),
        result: Ok(()),
    });
    app.dispatch(Action::Ownership(OwnershipInput::Type("123456".into())));
    // Another code, which says it came.
    assert!(sent(app.dispatch(Action::Ownership(OwnershipInput::Resend))).is_some());
    app.apply(Event::ServerPincodeSent {
        server: SERVER.into(),
        result: Ok(()),
    });
    assert!(matches!(ownership(&app), Some(Ownership::Transfer(ask))
        if ask.resent && ask.code.as_deref() == Some("123456")));
    assert!(matches!(
        sent(app.dispatch(Action::Ownership(OwnershipInput::Confirm))),
        Some(DiscordCommand::TransferServer { user, code: Some(code), .. })
            if user == "2" && code == "123456"
    ));
    // A wrong code shows under the field, which stays.
    app.apply(Event::ServerTransferred {
        server: SERVER.into(),
        result: Err("Code: Invalid code".into()),
    });
    assert!(matches!(ownership(&app), Some(Ownership::Transfer(ask))
        if ask.code.is_some() && ask.error.as_deref() == Some("Code: Invalid code")));

    // Without an email, nothing can confirm it but Discord.
    let mut app = owned(false, false);
    members(&mut app);
    app.dispatch(Action::Ownership(OwnershipInput::Transfer("2".into())));
    app.dispatch(Action::Ownership(OwnershipInput::Agree));
    assert!(matches!(
        sent(app.dispatch(Action::Ownership(OwnershipInput::Confirm))),
        Some(DiscordCommand::TransferServer { code: None, .. })
    ));
}

#[test]
fn a_verified_server_isnt_given_away() {
    let mut app = owned(true, true);
    change_server(&mut app, |server| server.verified = true);
    members(&mut app);
    app.dispatch(Action::Ownership(OwnershipInput::Transfer("2".into())));
    app.dispatch(Action::Ownership(OwnershipInput::Agree));
    assert!(matches!(ownership(&app), Some(Ownership::Transfer(ask))
        if ask.verified && !ask.agreed && !ask.ready()));
}
