//! A Discord server's settings, and the two-factor prompt Discord's
//! held-back requests open.

use super::*;
use crate::chat::{
    GuildSettings, MfaMethod, PickedPicture, RoleAction, RoleDetail, RoleManager, RoleMember,
    ServerPicture, ServerProfile, ServerRoles, ServerSettingsChange, ServerSettingsPage,
    SettingsSection,
};

/// `discord_server`, with Alice's Mods role managing the server as well.
fn managing() -> Application {
    let mut app = discord_server();
    grant_mods(&mut app, yapfast_discord::ready::permission::MANAGE_GUILD);
    app
}

/// Gives the Mods role `extra` besides Kick Members and Manage Roles.
fn grant_mods(app: &mut Application, extra: u64) {
    use yapfast_discord::ready::permission;
    change_server(app, |server| {
        let bits = permission::KICK_MEMBERS | permission::MANAGE_ROLES | extra;
        server.put_role(
            &serde_json::json!({"id": "20", "name": "Mods", "position": 2,
                                            "color": 0x3498db, "permissions": bits.to_string()}),
        );
    });
}

#[test]
fn scheduling_events_takes_create_events_or_administrator() {
    use crate::chat::Permission::CreateEvents;
    use yapfast_discord::ready::permission;
    let mut app = discord_server();
    assert!(!app.may("discord:10", CreateEvents));
    grant_mods(&mut app, permission::CREATE_EVENTS);
    assert!(app.may("discord:10", CreateEvents));
    // Discord's client asks for Create Events alone.
    grant_mods(&mut app, permission::MANAGE_EVENTS);
    assert!(!app.may("discord:10", CreateEvents));
    grant_mods(&mut app, permission::MANAGE_GUILD);
    assert!(!app.may("discord:10", CreateEvents));
    grant_mods(&mut app, permission::ADMINISTRATOR);
    assert!(app.may("discord:10", CreateEvents));
}

fn profile(name: &str) -> ServerProfile {
    ServerProfile {
        name: name.into(),
        description: String::new(),
        icon: None,
    }
}

/// The page Discord sends as Server Profile opens.
fn loaded(app: &mut Application, name: &str) {
    app.apply(Event::ServerSettingsLoaded {
        server: "discord:10".into(),
        section: SettingsSection::Profile,
        result: Ok(ServerSettingsPage::Profile(profile(name))),
    });
}

fn saved(effects: Vec<Effect>) -> ServerSettingsChange {
    match effects.as_slice() {
        [
            Effect::Backend(Command::Discord(DiscordCommand::SaveServerSettings {
                server,
                change,
            })),
        ] => {
            assert_eq!(server, "discord:10");
            (**change).clone()
        }
        other => panic!("{} effects", other.len()),
    }
}

#[test]
fn server_settings_open_only_with_a_page_she_may_change() {
    use MenuChoice::*;
    let mut app = discord_server();
    app.state.discord_online = true;
    // Kick Members alone opens none of them.
    change_server(&mut app, |server| {
        server.put_role(
            &serde_json::json!({"id": "20", "name": "Mods", "position": 2,
            "permissions": yapfast_discord::ready::permission::KICK_MEMBERS.to_string()}),
        );
    });
    assert!(!app.menu_choices("discord:10").contains(&ServerSettings));
    assert!(
        app.dispatch(Action::OpenServerSettings("discord:10".into()))
            .is_empty()
    );
    // Manage Roles opens Roles, and Members with Kick Members.
    grant_mods(&mut app, 0);
    assert_eq!(
        app.settings_sections("discord:10"),
        [SettingsSection::Members, SettingsSection::Roles]
    );

    grant_mods(&mut app, yapfast_discord::ready::permission::MANAGE_GUILD);
    let choices = app.menu_choices("discord:10");
    let at = |choice| choices.iter().position(|known| *known == choice).unwrap();
    assert!(at(HideMuted) < at(ServerSettings) && at(ServerSettings) < at(EditServerProfile));
    assert_eq!(
        app.header_choices("discord:10")[0],
        ServerSettings,
        "after Invite, as Discord's dropdown has it"
    );
    assert_eq!(
        app.settings_sections("discord:10"),
        [
            SettingsSection::Profile,
            SettingsSection::Engagement,
            SettingsSection::BoostPerks,
            SettingsSection::Members,
            SettingsSection::Roles,
            SettingsSection::Invites,
            SettingsSection::Safety
        ]
    );

    // Opening asks Discord for the first page.
    assert!(matches!(
        app.dispatch(Action::OpenServerSettings("discord:10:11".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ServerSettings { server, section }))]
            if server == "discord:10" && *section == SettingsSection::Profile
    ));
    assert!(app.state.server_settings.as_ref().unwrap().loading());

    // Losing Manage Server closes them.
    grant_mods(&mut app, 0);
    assert!(app.state.server_settings.is_none());
}

#[test]
fn server_profile_saves_what_changed_and_waits_for_reset_or_save_to_leave() {
    let mut app = managing();
    app.state.discord_online = true;
    app.dispatch(Action::OpenServerSettings("discord:10".into()));
    // A page from another server, or another page, is dropped.
    app.apply(Event::ServerSettingsLoaded {
        server: "discord:11".into(),
        section: SettingsSection::Profile,
        result: Ok(ServerSettingsPage::Profile(profile("Other"))),
    });
    assert!(app.state.server_settings.as_ref().unwrap().loading());
    loaded(&mut app, "Test");
    let settings = app.state.server_settings.as_ref().unwrap();
    assert!(!settings.loading() && !settings.changed());

    app.dispatch(Action::WriteServerSetting {
        field: SettingsText::Description,
        text: "Where we talk ".into(),
    });
    assert!(app.state.server_settings.as_ref().unwrap().changed());

    // Leaving with changes shakes the bar instead.
    app.dispatch(Action::CloseServerSettings);
    let settings = app.state.server_settings.as_ref().unwrap();
    assert_eq!(settings.nudge, 1);

    // A name Discord refuses holds the save back.
    app.dispatch(Action::WriteServerSetting {
        field: SettingsText::Name,
        text: "T".into(),
    });
    assert!(app.dispatch(Action::SaveServerSettings).is_empty());
    app.dispatch(Action::WriteServerSetting {
        field: SettingsText::Name,
        text: "Test".into(),
    });
    let ServerSettingsChange::Profile {
        name,
        description,
        icon,
    } = saved(app.dispatch(Action::SaveServerSettings))
    else {
        panic!("Server Profile's change");
    };
    assert_eq!(
        (name, description.as_deref(), icon.is_none()),
        (None, Some("Where we talk"), true)
    );
    assert!(app.state.server_settings.as_ref().unwrap().saving);
    // Nothing changes while Discord takes it.
    app.dispatch(Action::UndoServerSettings);
    assert!(app.state.server_settings.as_ref().unwrap().changed());

    // A refusal keeps the draft to try again.
    app.apply(Event::ServerSettingsSaved {
        server: "discord:10".into(),
        result: Err("Couldn't save the changes: no".into()),
    });
    let settings = app.state.server_settings.as_ref().unwrap();
    assert!(!settings.saving && settings.changed());
    assert!(settings.error.as_deref().unwrap().contains("no"));

    // Discord's answer is the page as it now stands.
    saved(app.dispatch(Action::SaveServerSettings));
    let mut now = profile("Test");
    now.description = "Where we talk".into();
    app.apply(Event::ServerSettingsSaved {
        server: "discord:10".into(),
        result: Ok(ServerSettingsPage::Profile(now)),
    });
    let settings = app.state.server_settings.as_ref().unwrap();
    assert!(!settings.saving && !settings.changed() && settings.error.is_none());
    app.dispatch(Action::CloseServerSettings);
    assert!(app.state.server_settings.is_none());
}

#[test]
fn server_profile_follows_a_rename_elsewhere_unless_its_draft_renamed_it() {
    let mut app = managing();
    app.state.discord_online = true;
    app.dispatch(Action::OpenServerSettings("discord:10".into()));
    loaded(&mut app, "Test");
    change_server(&mut app, |server| server.name = "Crew".into());
    let Some(SettingsPage::Profile(page)) = &app.state.server_settings.as_ref().unwrap().page
    else {
        panic!("Server Profile");
    };
    assert_eq!(
        (page.name.as_str(), page.original.name.as_str()),
        ("Crew", "Crew")
    );

    app.dispatch(Action::WriteServerSetting {
        field: SettingsText::Name,
        text: "Mine".into(),
    });
    change_server(&mut app, |server| server.name = "Crew 2".into());
    let Some(SettingsPage::Profile(page)) = &app.state.server_settings.as_ref().unwrap().page
    else {
        panic!("Server Profile");
    };
    assert_eq!(page.name, "Mine");
}

#[test]
fn an_icon_is_read_then_saved_and_an_animated_one_takes_level_1() {
    let mut app = managing();
    app.state.discord_online = true;
    app.dispatch(Action::OpenServerSettings("discord:10".into()));
    loaded(&mut app, "Test");
    let picked = |mime| PickedPicture {
        mime,
        data: Arc::from(&b"picture"[..]),
        preview: Arc::new(chat::Picture::default()),
    };
    let read = |app: &mut Application, mime| {
        let effects = app.dispatch(Action::PickServerPicture {
            kind: ServerPicture::Icon,
            path: "icon.png".into(),
        });
        assert!(matches!(
            effects.as_slice(),
            [Effect::Backend(Command::Discord(
                DiscordCommand::ServerPicture { .. }
            ))]
        ));
        app.apply(Event::ServerPicture {
            kind: ServerPicture::Icon,
            path: "icon.png".into(),
            result: Ok(picked(mime)),
        });
    };
    read(&mut app, "image/gif");
    let settings = app.state.server_settings.as_ref().unwrap();
    assert!(settings.error.as_deref().unwrap().contains("level 1"));
    assert!(!settings.changed());

    read(&mut app, "image/png");
    let ServerSettingsChange::Profile { icon, .. } =
        saved(app.dispatch(Action::SaveServerSettings))
    else {
        panic!("Server Profile's change");
    };
    assert_eq!(icon.flatten().map(|icon| icon.mime), Some("image/png"));
}

/// `managing`'s settings open at `section`, with `settings` from Discord.
fn guild_page(section: SettingsSection, settings: GuildSettings) -> Application {
    let mut app = managing();
    app.state.discord_online = true;
    app.dispatch(Action::OpenServerSettings("discord:10".into()));
    loaded(&mut app, "Test");
    assert!(matches!(
        app.dispatch(Action::ServerSettingsSection(section)).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ServerSettings { section: asked, .. }))]
            if *asked == section
    ));
    app.apply(Event::ServerSettingsLoaded {
        server: "discord:10".into(),
        section,
        result: Ok(ServerSettingsPage::Guild(settings)),
    });
    app
}

/// Engagement's fields of a save.
type Engagement = (
    Option<Option<String>>,
    Option<u64>,
    Option<Option<String>>,
    Option<u32>,
);

fn engagement(effects: Vec<Effect>) -> Engagement {
    match saved(effects) {
        ServerSettingsChange::Guild {
            system_channel,
            system_flags,
            afk_channel,
            afk_timeout,
            ..
        } => (system_channel, system_flags, afk_channel, afk_timeout),
        ServerSettingsChange::Profile { .. }
        | ServerSettingsChange::Roles { .. }
        | ServerSettingsChange::MfaLevel(_) => panic!("a guild change"),
    }
}

#[test]
fn engagement_picks_channels_and_system_messages_as_discords_page_does() {
    let mut app = guild_page(
        SettingsSection::Engagement,
        GuildSettings {
            system_channel: Some("11".into()),
            afk_timeout: 300,
            ..Default::default()
        },
    );
    assert_eq!(
        app.settings_channels("discord:10", false),
        [
            ("11".to_owned(), "general".to_owned()),
            ("12".into(), "news".into()),
            ("13".into(), "memes".into())
        ]
    );
    assert_eq!(
        app.settings_channels("discord:10", true),
        [("14".to_owned(), "Lounge".to_owned())]
    );

    // No timeout without an inactive channel, as Discord greys it out.
    app.dispatch(Action::ChooseServerSetting(SettingsChoice::AfkTimeout(900)));
    assert!(!app.state.server_settings.as_ref().unwrap().changed());
    app.dispatch(Action::ChooseServerSetting(SettingsChoice::AfkChannel(
        Some("14".into()),
    )));
    app.dispatch(Action::ChooseServerSetting(SettingsChoice::AfkTimeout(900)));
    app.dispatch(Action::ChooseServerSetting(SettingsChoice::AfkTimeout(7)));
    app.dispatch(Action::ToggleServerSetting(SettingsToggle::SystemMessage(
        WELCOME,
    )));
    assert_eq!(
        engagement(app.dispatch(Action::SaveServerSettings)),
        (None, Some(WELCOME), Some(Some("14".into())), Some(900))
    );

    // Once saved, Boost Perks opens.
    let settings = GuildSettings {
        system_flags: WELCOME,
        afk_channel: Some("14".into()),
        afk_timeout: 900,
        ..Default::default()
    };
    app.apply(Event::ServerSettingsSaved {
        server: "discord:10".into(),
        result: Ok(ServerSettingsPage::Guild(settings.clone())),
    });
    app.apply(Event::ServerSettingsChanged {
        server: "discord:10".into(),
        settings: Box::new(settings),
    });
    assert!(!app.state.server_settings.as_ref().unwrap().changed());
    assert!(matches!(
        app.dispatch(Action::ServerSettingsSection(SettingsSection::BoostPerks))
            .as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::ServerSettings { .. }
        ))]
    ));
}

#[test]
fn a_change_elsewhere_reaches_an_open_page_but_not_its_draft() {
    let mut app = guild_page(
        SettingsSection::Engagement,
        GuildSettings {
            system_channel: Some("11".into()),
            ..Default::default()
        },
    );
    app.dispatch(Action::ChooseServerSetting(SettingsChoice::Notifications(
        1,
    )));
    app.apply(Event::ServerSettingsChanged {
        server: "discord:10".into(),
        settings: Box::new(GuildSettings {
            system_channel: Some("12".into()),
            default_notifications: 0,
            ..Default::default()
        }),
    });
    let Some(SettingsPage::Engagement(page)) = &app.state.server_settings.as_ref().unwrap().page
    else {
        panic!("Engagement");
    };
    assert_eq!(page.system_channel.as_deref(), Some("12"));
    assert_eq!(page.default_notifications, 1, "her own choice stays");
}

#[test]
fn boost_perks_unlock_each_picture_at_its_level() {
    let mut app = guild_page(
        SettingsSection::BoostPerks,
        GuildSettings {
            premium_tier: 1,
            ..Default::default()
        },
    );
    // A banner takes level 2.
    assert!(
        app.dispatch(Action::PickServerPicture {
            kind: ServerPicture::Banner,
            path: "banner.png".into(),
        })
        .is_empty()
    );
    assert_eq!(
        app.dispatch(Action::PickServerPicture {
            kind: ServerPicture::Splash,
            path: "splash.gif".into(),
        })
        .len(),
        1
    );
    let picked = |mime| PickedPicture {
        mime,
        data: Arc::from(&b"picture"[..]),
        preview: Arc::new(chat::Picture::default()),
    };
    app.apply(Event::ServerPicture {
        kind: ServerPicture::Splash,
        path: "splash.gif".into(),
        result: Ok(picked("image/gif")),
    });
    let settings = app.state.server_settings.as_ref().unwrap();
    assert!(settings.error.is_some() && !settings.changed());

    app.dispatch(Action::PickServerPicture {
        kind: ServerPicture::Splash,
        path: "splash.png".into(),
    });
    assert!(
        app.dispatch(Action::SaveServerSettings).is_empty(),
        "while it's read"
    );
    app.apply(Event::ServerPicture {
        kind: ServerPicture::Splash,
        path: "splash.png".into(),
        result: Ok(picked("image/png")),
    });
    app.dispatch(Action::ToggleServerSetting(SettingsToggle::ProgressBar));
    let ServerSettingsChange::Guild {
        splash,
        progress_bar,
        banner,
        ..
    } = saved(app.dispatch(Action::SaveServerSettings))
    else {
        panic!("a guild change");
    };
    assert_eq!(
        splash.flatten().map(|splash| splash.mime),
        Some("image/png")
    );
    assert_eq!((progress_bar, banner.is_none()), (Some(true), true));
}

#[test]
fn a_held_back_request_asks_for_two_factor_authentication() {
    let mut app = discord_server();
    let methods = vec![MfaMethod::Passkey, MfaMethod::App, MfaMethod::Sms];
    assert!(app.apply(Event::MfaRequired { id: 7, methods }).is_empty());
    let prompt = app.mfa_prompt().unwrap();
    assert_eq!(
        prompt.method,
        Some(MfaMethod::App),
        "a passkey takes Discord's app"
    );

    // Six digits, as Discord's client checks.
    app.dispatch(Action::WriteMfa("12 345".into()));
    assert!(app.dispatch(Action::ConfirmMfa).is_empty());
    app.dispatch(Action::WriteMfa(" 123 456".into()));
    assert!(matches!(
        app.dispatch(Action::ConfirmMfa).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::FinishMfa { id: 7, method: MfaMethod::App, code }))]
            if code == "123456"
    ));
    assert!(app.mfa_prompt().unwrap().busy);
    app.apply(Event::MfaFinished {
        id: 7,
        result: Err("That didn't work. Check it and try again.".into()),
    });
    let prompt = app.mfa_prompt().unwrap();
    assert!(!prompt.busy && prompt.error.is_some());

    // A text goes out as its way is chosen.
    assert!(matches!(
        app.dispatch(Action::ChooseMfa(Some(MfaMethod::Sms)))
            .as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::TextMfa(
            7
        )))]
    ));
    app.apply(Event::MfaTexted {
        id: 7,
        result: Ok("******1234".into()),
    });
    assert_eq!(
        app.mfa_prompt().unwrap().phone.as_deref(),
        Some("******1234")
    );
    assert!(
        app.dispatch(Action::ChooseMfa(Some(MfaMethod::Backup)))
            .is_empty(),
        "Discord didn't offer it"
    );

    // Another held at once waits its turn; taking one shows the next.
    app.apply(Event::MfaRequired {
        id: 8,
        methods: vec![MfaMethod::Backup, MfaMethod::Password],
    });
    assert_eq!(app.mfa_prompt().unwrap().id, 7);
    app.apply(Event::MfaFinished {
        id: 7,
        result: Ok(()),
    });
    let prompt = app.mfa_prompt().unwrap();
    assert_eq!((prompt.id, prompt.method), (8, Some(MfaMethod::Backup)));
    // Discord's warning first, each time a backup code is chosen.
    assert!(prompt.warning());
    assert!(app.dispatch(Action::ConfirmMfa).is_empty());
    assert!(!app.mfa_prompt().unwrap().warning());
    app.dispatch(Action::WriteMfa("abcd-efgh".into()));
    assert_eq!(prompt_answer(&app), "abcdefgh");
    app.dispatch(Action::ChooseMfa(Some(MfaMethod::Password)));
    app.dispatch(Action::ChooseMfa(Some(MfaMethod::Backup)));
    assert!(app.mfa_prompt().unwrap().warning());

    // Cancelling fails the request waiting on it.
    assert!(matches!(
        app.dispatch(Action::CancelMfa).as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::CancelMfa(8)
        ))]
    ));
    assert!(app.mfa_prompt().is_none());
}

fn prompt_answer(app: &Application) -> String {
    app.mfa_prompt().unwrap().answer()
}

/// A role as Discord sends it to Roles.
fn detail(id: &str, name: &str, position: i64, permissions: u64) -> RoleDetail {
    RoleDetail {
        id: id.into(),
        name: name.into(),
        position,
        permissions,
        members: 1,
        ..Default::default()
    }
}

/// The fixture's roles, as Roles shows them: Bot, Mods (hers), Helpers and
/// @everyone.
fn roles() -> ServerRoles {
    use yapfast_discord::ready::permission;
    let mods = permission::KICK_MEMBERS | permission::MANAGE_ROLES;
    ServerRoles {
        roles: vec![
            RoleDetail {
                managed: Some(RoleManager::Bot),
                ..detail("40", "Bot", 3, 0)
            },
            detail("20", "Mods", 2, mods),
            detail("30", "Helpers", 1, 0),
            detail("10", "@everyone", 0, permission::VIEW_CHANNEL),
        ],
        ..Default::default()
    }
}

/// `managing`'s settings open on Roles, as Discord sent them.
fn roles_page() -> Application {
    roles_page_with(roles())
}

fn roles_page_with(roles: ServerRoles) -> Application {
    let mut app = managing();
    app.state.discord_online = true;
    app.dispatch(Action::OpenServerSettings("discord:10".into()));
    loaded(&mut app, "Test");
    assert!(matches!(
        app.dispatch(Action::ServerSettingsSection(SettingsSection::Roles))
            .as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::ServerSettings {
                section: SettingsSection::Roles,
                ..
            }
        ))]
    ));
    app.apply(Event::ServerSettingsLoaded {
        server: "discord:10".into(),
        section: SettingsSection::Roles,
        result: Ok(ServerSettingsPage::Roles(roles)),
    });
    app
}

fn roles_of(app: &Application) -> &RolesPage {
    match &app.state.server_settings.as_ref().unwrap().page {
        Some(SettingsPage::Roles(page)) => page,
        _ => panic!("Roles"),
    }
}

fn role_action(effects: Vec<Effect>) -> RoleAction {
    match effects.as_slice() {
        [Effect::Backend(Command::Discord(DiscordCommand::RoleAction { server, action }))] => {
            assert_eq!(server, "discord:10");
            action.clone()
        }
        other => panic!("{} effects", other.len()),
    }
}

#[test]
fn roles_lock_what_ranks_at_or_above_her_highest() {
    use yapfast_discord::ready::permission;
    let mut app = roles_page();
    let rules = app.role_rules("discord:10");
    assert_eq!(rules.lock("40", 3), Some(RoleLock::Higher));
    assert_eq!(rules.lock("20", 2), Some(RoleLock::Highest));
    assert_eq!(rules.lock("30", 1), None);
    assert_eq!(
        rules.permission_lock("30", permission::ADMINISTRATOR, false),
        Some(PermissionLock::NotHeld)
    );

    // Her own highest role can't change, nor can it move.
    app.dispatch(Action::Roles(RoleInput::Open(Some("20".into()))));
    app.dispatch(Action::Roles(RoleInput::Name("Mine".into())));
    assert!(!app.state.server_settings.as_ref().unwrap().changed());
    app.dispatch(Action::Roles(RoleInput::Move {
        role: "30".into(),
        to: 0,
    }));
    assert_eq!(roles_of(&app).order, ["40", "20", "30"]);

    // Helpers takes what she holds, not what she doesn't.
    app.dispatch(Action::Roles(RoleInput::Open(Some("30".into()))));
    app.dispatch(Action::Roles(RoleInput::Permission(
        permission::KICK_MEMBERS,
    )));
    app.dispatch(Action::Roles(RoleInput::Permission(
        permission::ADMINISTRATOR,
    )));
    app.dispatch(Action::Roles(RoleInput::Name("Helpers+".into())));
    app.dispatch(Action::Roles(RoleInput::Color(DEFAULT_ROLE_COLOR)));
    let Some(ServerSettingsChange::Roles { positions, role }) =
        app.state.server_settings.as_ref().unwrap().change()
    else {
        panic!("a roles change");
    };
    assert!(positions.is_empty());
    let role = role.unwrap();
    assert_eq!(
        (role.name.as_str(), role.permissions, role.color),
        ("Helpers+", permission::KICK_MEMBERS, 0)
    );

    // Another role waits for the bar.
    app.dispatch(Action::Roles(RoleInput::Open(Some("10".into()))));
    let settings = app.state.server_settings.as_ref().unwrap();
    assert_eq!(settings.nudge, 1);
    assert_eq!(roles_of(&app).open.as_ref().unwrap().id, "30");
}

#[test]
fn a_role_takes_any_colour_and_an_emoji_in_its_pictures_place() {
    let mut boosted = roles();
    boosted.role_icons = true;
    boosted.roles[2].icon = Some("abc".into());
    let mut app = roles_page_with(boosted);
    app.dispatch(Action::Roles(RoleInput::Open(Some("30".into()))));
    let role = |app: &Application| match app.state.server_settings.as_ref().unwrap().change() {
        Some(ServerSettingsChange::Roles { role, .. }) => role,
        _ => None,
    };
    app.dispatch(Action::Roles(RoleInput::Color(0x123456)));
    assert_eq!(role(&app).unwrap().color, 0x123456);
    app.dispatch(Action::Roles(RoleInput::Color(DEFAULT_ROLE_COLOR)));
    assert!(role(&app).is_none());

    // Only an emoji, which takes the picture's place.
    app.dispatch(Action::Roles(RoleInput::Emoji("abc".into())));
    assert!(role(&app).is_none());
    app.dispatch(Action::Roles(RoleInput::Emoji(" 🔥 ".into())));
    let change = role(&app).unwrap();
    assert_eq!(change.unicode_emoji.as_deref(), Some("🔥"));
    assert!(matches!(change.icon, Some(None)), "the picture goes");
    // Remove Icon takes both.
    app.dispatch(Action::RemoveServerPicture(ServerPicture::RoleIcon));
    let change = role(&app).unwrap();
    assert!(change.unicode_emoji.is_none() && matches!(change.icon, Some(None)));
}

#[test]
fn the_owner_reorders_roles_and_saves_the_order_first() {
    let mut app = roles_page();
    change_server(&mut app, |server| server.owner_id = Some("1".into()));
    app.dispatch(Action::Roles(RoleInput::Move {
        role: "30".into(),
        to: 0,
    }));
    assert_eq!(roles_of(&app).order, ["30", "40", "20"]);
    let ServerSettingsChange::Roles { positions, role } =
        saved(app.dispatch(Action::SaveServerSettings))
    else {
        panic!("a roles change");
    };
    assert_eq!(
        positions,
        [
            ("30".to_owned(), 3),
            ("40".to_owned(), 2),
            ("20".to_owned(), 1)
        ]
    );
    assert!(role.is_none());

    // Discord's answer replaces the page.
    let mut moved = roles();
    moved.roles[2].position = 3;
    moved.roles[0].position = 2;
    moved.roles[1].position = 1;
    moved.roles.sort_by_key(|role| -role.position);
    app.apply(Event::ServerSettingsSaved {
        server: "discord:10".into(),
        result: Ok(ServerSettingsPage::Roles(moved)),
    });
    assert!(!app.state.server_settings.as_ref().unwrap().changed());
    assert_eq!(roles_of(&app).order, ["30", "40", "20"]);
}

#[test]
fn a_role_changed_elsewhere_reaches_the_open_role_but_not_its_draft() {
    let mut app = roles_page();
    app.dispatch(Action::Roles(RoleInput::Open(Some("30".into()))));
    app.dispatch(Action::Roles(RoleInput::Name("Mine".into())));
    app.apply(Event::ServerRoleChanged {
        server: "discord:10".into(),
        role: Box::new(RoleDetail {
            hoist: true,
            ..detail("30", "Theirs", 1, 0)
        }),
    });
    let page = roles_of(&app);
    let draft = page.open.as_ref().unwrap();
    assert_eq!((draft.name.as_str(), draft.hoist), ("Mine", true));
    assert_eq!(page.role("30").unwrap().members, 1, "the count stays");
    app.apply(Event::ServerRoleDeleted {
        server: "discord:10".into(),
        role: "30".into(),
    });
    assert!(roles_of(&app).open.is_none());
}

#[test]
fn making_copying_and_deleting_go_at_once_and_open_what_they_made() {
    let mut app = roles_page();
    assert!(matches!(
        role_action(app.dispatch(Action::Roles(RoleInput::Create))),
        RoleAction::Create
    ));
    assert!(app.state.server_settings.as_ref().unwrap().saving);
    let mut made = roles();
    made.roles.insert(3, detail("50", "new role", 1, 0));
    made.open = Some("50".into());
    app.apply(Event::ServerSettingsSaved {
        server: "discord:10".into(),
        result: Ok(ServerSettingsPage::Roles(made)),
    });
    assert_eq!(roles_of(&app).open.as_ref().unwrap().id, "50");

    // A copy is named as Discord names it; Bot, managed, has none.
    let RoleAction::Duplicate { role, name } =
        role_action(app.dispatch(Action::Roles(RoleInput::Duplicate("30".into()))))
    else {
        panic!("a copy");
    };
    assert_eq!((role.id.as_str(), name.as_str()), ("30", "Helpers copy"));
    app.apply(Event::ServerSettingsSaved {
        server: "discord:10".into(),
        result: Err("no".into()),
    });
    assert!(
        app.dispatch(Action::Roles(RoleInput::Duplicate("40".into())))
            .is_empty()
    );

    // Deleting asks first.
    assert!(
        app.dispatch(Action::Roles(RoleInput::Delete("30".into())))
            .is_empty()
    );
    assert_eq!(
        roles_of(&app).confirm,
        Some(RoleConfirm::Delete("30".into()))
    );
    assert!(matches!(
        role_action(app.dispatch(Action::Roles(RoleInput::Confirm))),
        RoleAction::Delete(role) if role == "30"
    ));
}

#[test]
fn manage_members_lists_adds_and_removes_members() {
    let mut app = roles_page();
    app.dispatch(Action::Roles(RoleInput::Open(Some("30".into()))));
    assert!(matches!(
        role_action(app.dispatch(Action::Roles(RoleInput::Tab(RoleTab::Members)))),
        RoleAction::Members(role) if role == "30"
    ));
    let member = |id: &str, name: &str| RoleMember {
        id: id.into(),
        name: name.into(),
        username: Some(name.to_lowercase()),
        avatar: format!("https://cdn/{id}.png"),
    };
    app.apply(Event::RoleHolders {
        server: "discord:10".into(),
        role: "30".into(),
        result: Ok(vec![member("2", "Bob"), member("3", "Carol")]),
        count: None,
    });
    app.dispatch(Action::Roles(RoleInput::MemberSearch("car".into())));
    assert_eq!(roles_of(&app).shown_members().len(), 1);

    // Remove member asks first.
    app.dispatch(Action::Roles(RoleInput::RemoveMember("3".into())));
    assert!(matches!(
        role_action(app.dispatch(Action::Roles(RoleInput::Confirm))),
        RoleAction::RemoveMember { role, member } if role == "30" && member == "3"
    ));
    app.apply(Event::RoleHolders {
        server: "discord:10".into(),
        role: "30".into(),
        result: Ok(vec![member("2", "Bob")]),
        count: Some(1),
    });

    // Add Members finds those without the role.
    assert!(matches!(
        role_action(app.dispatch(Action::Roles(RoleInput::AddMembers))),
        RoleAction::FindMembers(query) if query.is_empty()
    ));
    app.apply(Event::RoleCandidates {
        server: "discord:10".into(),
        query: String::new(),
        members: vec![member("2", "Bob"), member("4", "Dave")],
    });
    let found: Vec<String> = roles_of(&app)
        .adding
        .as_ref()
        .unwrap()
        .found
        .iter()
        .map(|member| member.id.clone())
        .collect();
    assert_eq!(found, ["4"]);
    app.dispatch(Action::Roles(RoleInput::Pick("4".into())));
    assert!(matches!(
        role_action(app.dispatch(Action::Roles(RoleInput::AddPicked))),
        RoleAction::AddMembers { role, members } if role == "30" && members == ["4"]
    ));
}
