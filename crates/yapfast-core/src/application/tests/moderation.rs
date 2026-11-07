//! Moderation in a Discord server's settings: Safety Setup and Bans.

use super::*;
use crate::chat::{
    Ban, BansAction, GuildSettings, RoleMember, ServerSettingsChange, ServerSettingsPage,
    SettingsSection,
};
use yapfast_discord::ready::permission;

const SERVER: &str = "discord:10";

/// `discord_server`, with Alice's Mods role holding `bits`.
fn granting(bits: u64) -> Application {
    let mut app = discord_server();
    change_server(&mut app, |server| {
        server.put_role(
            &serde_json::json!({"id": "20", "name": "Mods", "position": 2,
                                            "permissions": bits.to_string()}),
        );
    });
    app
}

fn open(app: &mut Application, section: SettingsSection) -> Vec<Effect> {
    app.dispatch(Action::OpenServerSettings(SERVER.into()));
    app.dispatch(Action::ServerSettingsSection(section))
}

fn settings(app: &Application) -> &ServerSettings {
    app.state.server_settings.as_ref().unwrap()
}

fn ban(id: &str, name: &str, reason: Option<&str>) -> Ban {
    Ban {
        user: RoleMember {
            id: id.into(),
            name: name.into(),
            username: None,
            avatar: String::new(),
        },
        reason: reason.map(str::to_owned),
    }
}

fn bans(app: &Application) -> &BansPage {
    match &settings(app).page {
        Some(SettingsPage::Bans(page)) => page,
        _ => panic!("Bans isn't open"),
    }
}

fn safety(app: &Application) -> &SafetyPage {
    match &settings(app).page {
        Some(SettingsPage::Safety(page)) => page,
        _ => panic!("Safety Setup isn't open"),
    }
}

fn asked(effects: &[Effect]) -> Option<&BansAction> {
    match effects {
        [Effect::Backend(Command::Discord(DiscordCommand::Bans { server, action }))]
            if server == SERVER =>
        {
            Some(action)
        }
        _ => None,
    }
}

#[test]
fn moderation_lists_safety_setup_with_manage_server_the_log_with_its_permission_and_bans_with_either()
 {
    let app = granting(permission::BAN_MEMBERS);
    assert_eq!(
        app.settings_sections(SERVER),
        [SettingsSection::Members, SettingsSection::Bans]
    );
    let app = granting(permission::VIEW_AUDIT_LOG);
    assert_eq!(
        app.settings_sections(SERVER),
        [SettingsSection::AuditLog, SettingsSection::Bans]
    );
    let app = granting(permission::MANAGE_GUILD);
    assert_eq!(
        app.settings_sections(SERVER),
        [
            SettingsSection::Profile,
            SettingsSection::Engagement,
            SettingsSection::BoostPerks,
            SettingsSection::Members,
            SettingsSection::Invites,
            SettingsSection::Safety
        ]
    );
}

#[test]
fn safety_setup_saves_each_choice_as_its_made_and_puts_it_back_if_refused() {
    let mut app = granting(permission::MANAGE_GUILD);
    open(&mut app, SettingsSection::Safety);
    app.apply(Event::ServerSettingsLoaded {
        server: SERVER.into(),
        section: SettingsSection::Safety,
        result: Ok(ServerSettingsPage::Guild(GuildSettings::default())),
    });
    let choose = |app: &mut Application, choice| {
        app.dispatch(Action::ChooseServerSetting(SettingsChoice::Safety(choice)))
    };
    let effects = choose(&mut app, SafetyChoice::Verification(3));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::SaveServerSettings { change, .. }))]
            if matches!(**change, ServerSettingsChange::Guild { verification_level: Some(3), .. })
    ));
    assert!(settings(&app).immediate() && !settings(&app).changed());
    assert_eq!(
        safety(&app).verification(),
        3,
        "shown while Discord takes it"
    );
    assert!(
        choose(&mut app, SafetyChoice::ContentFilter(2)).is_empty(),
        "one at a time"
    );
    app.apply(Event::ServerSettingsSaved {
        server: SERVER.into(),
        result: Err("no".into()),
    });
    assert_eq!(safety(&app).verification(), 0);
    assert_eq!(settings(&app).error.as_deref(), Some("no"));

    // She doesn't own the server, so the 2FA switch stays put.
    assert_eq!(app.mfa_rule(SERVER), MfaRule::NotOwner);
    assert!(choose(&mut app, SafetyChoice::RequireMfa(true)).is_empty());
    change_server(&mut app, |server| server.owner_id = Some("1".into()));
    assert_eq!(app.mfa_rule(SERVER), MfaRule::OwnerWithoutMfa);
    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    snapshot.account = Some(yapfast_discord::service::Account {
        username: String::new(),
        name: "Alice".into(),
        avatar: String::new(),
        decoration: None,
        nameplate: None,
        premium_type: 0,
        mfa_enabled: true,
        has_email: true,
    });
    app.apply(discord_event(snapshot));
    assert!(matches!(
        choose(&mut app, SafetyChoice::RequireMfa(true)).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::SaveServerSettings { change, .. }))]
            if matches!(**change, ServerSettingsChange::MfaLevel(1))
    ));
    app.apply(Event::ServerSettingsSaved {
        server: SERVER.into(),
        result: Ok(ServerSettingsPage::Guild(GuildSettings {
            mfa_level: 1,
            ..Default::default()
        })),
    });
    assert!(safety(&app).requires_mfa() && !settings(&app).saving);
    // Leaving needs nothing saved.
    assert!(
        !app.dispatch(Action::ServerSettingsSection(SettingsSection::Profile))
            .is_empty()
    );
}

#[test]
fn bans_page_search_and_revoke_as_discords_client_does() {
    let mut app = granting(permission::BAN_MEMBERS);
    let loading = open(&mut app, SettingsSection::Bans);
    assert!(matches!(
        loading.as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::ServerSettings {
                section: SettingsSection::Bans,
                ..
            }
        ))]
    ));
    let first: Vec<Ban> = (0..BANS_BATCH)
        .map(|n| ban(&(n + 100).to_string(), "someone", None))
        .collect();
    app.apply(Event::ServerSettingsLoaded {
        server: SERVER.into(),
        section: SettingsSection::Bans,
        result: Ok(ServerSettingsPage::Bans(first)),
    });
    assert_eq!(bans(&app).pages(), 11);

    // The page past those fetched asks for the next batch.
    let more = app.dispatch(Action::Bans(BanInput::Page(10)));
    assert_eq!(asked(&more), Some(&BansAction::More("1099".into())));
    app.apply(Event::BansLoaded {
        server: SERVER.into(),
        searched: false,
        result: Ok(vec![ban("5000", "Mallory", Some("spam"))]),
    });
    assert_eq!(bans(&app).on_page().len(), 1);

    // A search asks Discord, and shows what it and those fetched match.
    app.dispatch(Action::Bans(BanInput::Query("mal".into())));
    let search = app.dispatch(Action::Bans(BanInput::Search));
    assert_eq!(asked(&search), Some(&BansAction::Search("mal".into())));
    assert_eq!(bans(&app).page, 0);
    assert_eq!(bans(&app).shown().len(), 1);

    // Revoke Ban lifts it, and the dialog closes.
    app.dispatch(Action::Bans(BanInput::Open("5000".into())));
    assert_eq!(
        bans(&app).open_ban().and_then(|ban| ban.reason.as_deref()),
        Some("spam")
    );
    let unban = app.dispatch(Action::Bans(BanInput::Unban));
    assert_eq!(asked(&unban), Some(&BansAction::Unban("5000".into())));
    app.apply(Event::Unbanned {
        server: SERVER.into(),
        user: "5000".into(),
        result: Ok(()),
    });
    assert!(bans(&app).open.is_none() && bans(&app).shown().is_empty());

    // A ban made elsewhere shows, without a reason.
    app.apply(Event::ServerBan {
        server: SERVER.into(),
        ban: Box::new(ban("6000", "Malcolm", None)),
        banned: true,
    });
    assert_eq!(bans(&app).shown().len(), 1);
    assert!(!settings(&app).changed() && !settings(&app).saving);
}

#[test]
fn only_ban_members_revokes_a_ban() {
    let mut app = granting(permission::VIEW_AUDIT_LOG);
    open(&mut app, SettingsSection::Bans);
    app.apply(Event::ServerSettingsLoaded {
        server: SERVER.into(),
        section: SettingsSection::Bans,
        result: Ok(ServerSettingsPage::Bans(vec![ban("5", "Mallory", None)])),
    });
    app.dispatch(Action::Bans(BanInput::Open("5".into())));
    assert!(bans(&app).open_ban().is_some());
    assert!(app.dispatch(Action::Bans(BanInput::Unban)).is_empty());
}
