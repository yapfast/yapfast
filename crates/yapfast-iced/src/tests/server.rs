//! A Discord server's settings: the settings opening from the server's menu
//! and holding their changes until saved, Roles locking what ranks above
//! this account and the permissions it lacks, a role dragged by its handle
//! moving, and Moderation's, People's and Expression's pages and Delete
//! Server as Discord's show them. [`discord_server`] builds the window on
//! such a server for any test that needs one.

use std::sync::Arc;

use iced::{
    Point,
    keyboard::{self, key},
    mouse,
    widget::Id,
};
use yapfast_core::{
    application::{PermissionLock, RoleLock, SettingsPage},
    chat::{Event, RoleDetail, ServerProfile, ServerRoles, ServerSettingsPage, SettingsSection},
    yapfast_discord::{
        marked::{MarkedServer, ServerMarks},
        notifications::Notifications,
        ready::{self, permission},
        service::{Snapshot, Status},
    },
};

use super::{interact, local, present, shows, start};
use crate::app::{App, Message};

/// The server's id in the core.
const SERVER: &str = "discord:10";

/// The window on a Discord server, "10" ("Test"), where Alice, "1", holds
/// @everyone's permissions and `extra` ones: a category "Talk" holding
/// #general and the Lounge, an empty category "Later", and one only Mods
/// see, "Staff". Her settings for the server hide muted channels and
/// suppress role mentions; #general mentions her. `change` is made to the
/// server before the window takes it in.
pub(super) fn discord_server(
    extra: u64,
    change: impl FnOnce(&mut ready::Server, &mut ServerMarks),
) -> (App, tempfile::TempDir) {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    let mut app = start(&folder, false);
    let bits = |bits: u64| bits.to_string();
    let hidden = serde_json::json!([{"id": "10", "type": 0, "allow": "0",
                                     "deny": bits(permission::VIEW_CHANNEL)}]);
    let mut server = ready::server(&serde_json::json!({
        "id": "10", "owner_id": "99",
        "properties": {"name": "Test", "default_message_notifications": 1},
        "roles": [{"id": "10", "name": "@everyone", "position": 0,
                   "permissions": bits(permission::VIEW_CHANNEL | permission::SEND_MESSAGES | extra)}],
        "channels": [
            {"id": "4", "type": 4, "name": "Talk", "position": 0},
            {"id": "5", "type": 4, "name": "Later", "position": 1},
            {"id": "6", "type": 4, "name": "Staff", "position": 2, "permission_overwrites": hidden},
            {"id": "11", "type": 0, "name": "general", "position": 0, "parent_id": "4"},
            {"id": "14", "type": 2, "name": "Lounge", "position": 1, "parent_id": "4"},
        ],
    }))
    .expect("the server");
    let mut marks = ServerMarks {
        notifications: Arc::new(Notifications {
            hide_muted_channels: true,
            suppress_roles: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    let general = server.text_channels[0].id.clone();
    marks
        .channel_mut(&server, &general)
        .expect("#general")
        .mentions = 1;
    change(&mut server, &mut marks);
    app.client.apply(Event::Discord(Arc::new(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        servers: vec![MarkedServer {
            server: Arc::new(server),
            marks: Arc::new(marks),
        }],
        ..Default::default()
    })));
    // Signed in, as `Connected` leaves it, so the window shows the
    // workspace.
    app.client.app.state.account = Some("1".into());
    let _ = app.sync();
    (app, folder)
}

/// The server's menu, as its name in the sidebar opens it, and Server
/// settings chosen from it.
fn open_settings(app: &mut App) {
    let _ = crate::place_menus::open_place_menu(app, SERVER, Point::new(80.0, 44.0), true);
    interact(app, |ui| {
        ui.click("Server settings").expect("Server settings");
    });
    assert!(app.client.state().server_settings.is_some());
}

/// `page` loaded for `section`, as Discord's answer arrives.
fn load(app: &mut App, section: SettingsSection, page: ServerSettingsPage) {
    app.client.apply(Event::ServerSettingsLoaded {
        server: SERVER.into(),
        section,
        result: Ok(page),
    });
    let _ = app.sync();
}

/// Escape, as the window's keys hand it to the app.
fn escape(app: &mut App) {
    let _ = app.update(Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        Default::default(),
    ));
}

/// The open settings' page.
fn page(app: &App) -> &SettingsPage {
    app.client
        .state()
        .server_settings
        .as_ref()
        .and_then(|settings| settings.page.as_ref())
        .expect("a page")
}

#[test]
fn server_settings_open_from_the_menu_and_hold_their_changes_until_saved() {
    let (mut app, _folder) = discord_server(permission::MANAGE_GUILD, |_, _| {});
    open_settings(&mut app);
    for page in [
        "Server Profile",
        "Engagement",
        "Boost Perks",
        "Members",
        "Invites",
        "Safety Setup",
    ] {
        assert!(shows(&app, page), "{page}");
    }
    assert!(!shows(&app, "Roles"), "not without Manage Roles");
    assert!(shows(&app, "Loading..."));

    load(
        &mut app,
        SettingsSection::Profile,
        ServerSettingsPage::Profile(ServerProfile {
            name: "Test".into(),
            description: "Hi\nthere\nin\nfive\nlines".into(),
            icon: None,
        }),
    );
    assert!(!shows(&app, "Loading..."));
    match page(&app) {
        SettingsPage::Profile(page) => assert!(page.description.starts_with("Hi")),
        _ => panic!("not Server Profile"),
    }
    // Its description stays in its 96 px box, over what's left under it.
    interact(&mut app, |ui| {
        let field = ui.find(Id::new("server-description")).expect("the field");
        let left = ui.find("278 left").expect("what's left");
        let (field, left) = (field.bounds(), left.bounds());
        assert!(field.height <= 96.0, "{field:?}");
        assert!(field.y + field.height <= left.y, "{field:?} over {left:?}");
    });
    assert!(!shows(&app, "Save Changes"));

    // Typing shows the bar; closing then keeps the settings open.
    interact(&mut app, |ui| {
        ui.click(Id::new("server-name")).expect("the name's field");
        for _ in 0.."est".len() {
            ui.tap_key(keyboard::Key::Named(key::Named::Backspace));
        }
    });
    assert!(shows(&app, "Save Changes"));
    assert!(shows(
        &app,
        "The server name must be between 2 and 100 characters."
    ));
    escape(&mut app);
    assert!(app.client.state().server_settings.is_some(), "held open");

    // Reset puts Discord's back, and closing closes.
    interact(&mut app, |ui| {
        ui.click("Reset").expect("Reset");
    });
    assert!(!shows(&app, "Save Changes"));
    escape(&mut app);
    assert!(app.client.state().server_settings.is_none());
}

#[test]
fn roles_lock_what_ranks_above_her_and_the_permissions_she_lacks() {
    // She holds Staff, which may kick.
    let (mut app, _folder) = discord_server(permission::MANAGE_ROLES, |server, marks| {
        marks.roles = vec!["20".into()];
        Arc::make_mut(&mut server.role_positions).insert("20".into(), 2);
        Arc::make_mut(&mut server.roles).insert("20".into(), permission::KICK_MEMBERS);
    });
    let role = |id: &str, name: &str, position: i64| RoleDetail {
        id: id.into(),
        name: name.into(),
        position,
        ..Default::default()
    };
    open_settings(&mut app);
    assert!(shows(&app, "Members") && shows(&app, "Roles"));
    assert!(!shows(&app, "Server Profile"));
    interact(&mut app, |ui| {
        ui.click("Roles").expect("Roles");
    });
    load(
        &mut app,
        SettingsSection::Roles,
        ServerSettingsPage::Roles(ServerRoles {
            roles: vec![
                role("40", "Leads", 3),
                role("20", "Staff", 2),
                role("30", "Crew", 1),
                role("10", "@everyone", 0),
            ],
            ..Default::default()
        }),
    );
    assert!(shows(&app, "ROLES – 3"));
    for name in ["Leads", "Staff", "Crew"] {
        assert!(shows(&app, name), "{name}");
    }
    let rules = app.client.app.role_rules(SERVER);
    let locks: Vec<_> = [("40", 3), ("20", 2), ("30", 1)]
        .map(|(id, position)| rules.lock(id, position))
        .into();
    assert_eq!(
        locks,
        [Some(RoleLock::Higher), Some(RoleLock::Highest), None]
    );
    assert_eq!(
        crate::server_settings::roles::lock_text(rules.lock("20", 2)),
        "Role is locked because it is your highest ranked role. Please ask a higher rank or Server Owner for help."
    );

    // Crew's permissions: Kick Members is hers to give, Administrator not.
    interact(&mut app, |ui| {
        ui.click("Crew").expect("Crew's row");
    });
    interact(&mut app, |ui| {
        ui.click("Permissions").expect("the Permissions tab");
    });
    // Far down its list, past the window's foot.
    assert!(present(&app, "Administrator"));
    let on = |app: &App, bit: u64| match page(app) {
        SettingsPage::Roles(page) => page.open.as_ref().expect("Crew open").permissions & bit != 0,
        _ => panic!("not Roles"),
    };
    let rules = app.client.app.role_rules(SERVER);
    assert_eq!(
        rules.permission_lock("30", permission::ADMINISTRATOR, false),
        Some(PermissionLock::NotHeld)
    );
    assert_eq!(
        rules.permission_lock("30", permission::KICK_MEMBERS, false),
        None
    );
    // The search brings it into view, at the foot of the list.
    let search = |app: &mut App, words: &str| {
        let typed = match page(app) {
            SettingsPage::Roles(page) => page.permission_search.chars().count(),
            _ => 0,
        };
        interact(app, |ui| {
            ui.click(Id::new("role-permission-search"))
                .expect("the search");
            for _ in 0..typed {
                ui.tap_key(keyboard::Key::Named(key::Named::Backspace));
            }
            ui.typewrite(words);
        });
    };
    search(&mut app, "administrator");
    interact(&mut app, |ui| {
        ui.click("Administrator").expect("Administrator");
    });
    assert!(!on(&app, permission::ADMINISTRATOR), "locked");
    assert!(!shows(&app, "Save Changes"));
    search(&mut app, "kick");
    interact(&mut app, |ui| {
        ui.click("Kick, Approve, and Reject Members")
            .expect("Kick Members");
    });
    assert!(on(&app, permission::KICK_MEMBERS));
    assert!(shows(&app, "Save Changes"));
}

#[test]
fn a_role_dragged_by_its_handle_moves_in_the_list() {
    // Her own server: every role may move.
    let (mut app, _folder) = discord_server(0, |server, _| {
        server.owner_id = Some("1".into());
    });
    let role = |id: &str, name: &str, position: i64| RoleDetail {
        id: id.into(),
        name: name.into(),
        position,
        ..Default::default()
    };
    open_settings(&mut app);
    interact(&mut app, |ui| {
        ui.click("Roles").expect("Roles");
    });
    load(
        &mut app,
        SettingsSection::Roles,
        ServerSettingsPage::Roles(ServerRoles {
            roles: vec![
                role("40", "Leads", 3),
                role("20", "Staff", 2),
                role("30", "Crew", 1),
                role("10", "@everyone", 0),
            ],
            ..Default::default()
        }),
    );
    let names = |app: &App| -> Vec<String> {
        match page(app) {
            SettingsPage::Roles(page) => {
                page.listed().iter().map(|role| role.name.clone()).collect()
            }
            _ => panic!("not Roles"),
        }
    };
    assert_eq!(names(&app), ["Leads", "Staff", "Crew"]);
    // Crew taken by its handle, at the row's left, and let go on Leads.
    interact(&mut app, |ui| {
        let leads = text_bounds(ui, "Leads");
        let crew = text_bounds(ui, "Crew");
        let handle = leads.x - 54.0;
        ui.point_at(Point::new(handle, crew.center_y()));
        ui.simulate([iced::Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))]);
        let to = Point::new(handle, leads.center_y());
        ui.point_at(to);
        ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved {
            position: to,
        })]);
        ui.simulate([iced::Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        ))]);
    });
    assert_eq!(names(&app), ["Crew", "Leads", "Staff"]);
    assert!(shows(&app, "Save Changes"));
}

/// Where the text in view that reads `text` is.
fn text_bounds(ui: &mut iced_test::Simulator<'_, Message>, text: &str) -> iced::Rectangle {
    ui.find(text)
        .expect(text)
        .visible_bounds()
        .expect("in view")
}

/// The open page as Safety Setup.
fn safety(app: &App) -> &yapfast_core::application::SafetyPage {
    match page(app) {
        SettingsPage::Safety(page) => page,
        _ => panic!("not Safety Setup"),
    }
}

#[test]
fn moderation_shows_safety_setup_and_bans_as_discords_pages_do() {
    use yapfast_core::{
        application::MfaRule,
        chat::{Ban, GuildSettings, RoleMember},
    };
    let (mut app, _folder) = discord_server(
        permission::BAN_MEMBERS | permission::MANAGE_GUILD,
        |_, _| {},
    );
    open_settings(&mut app);
    for page in ["MODERATION", "Safety Setup", "Bans"] {
        assert!(shows(&app, page), "{page}");
    }

    // Safety Setup: Discord's levels, and the 2FA switch only its owner
    // moves.
    interact(&mut app, |ui| {
        ui.click("Safety Setup").expect("Safety Setup");
    });
    load(
        &mut app,
        SettingsSection::Safety,
        ServerSettingsPage::Guild(GuildSettings {
            verification_level: 2,
            features: vec!["COMMUNITY".into()],
            ..Default::default()
        }),
    );
    for level in ["None", "Low", "Medium", "High", "Highest"] {
        assert!(shows(&app, level), "{level}");
    }
    assert!(
        shows(&app, "This option is not available for Community Servers."),
        "None, not on a Community server"
    );
    assert!(!safety(&app).offers_verification(0) && safety(&app).offers_verification(4));
    assert_eq!(safety(&app).verification(), 2);
    let rule = app.client.app.mfa_rule(SERVER);
    assert_eq!(rule, MfaRule::NotOwner);
    assert_eq!(
        safety(&app).mfa_lock(rule),
        Some(
            "This setting can only be changed by a server owner with 2FA enabled on their account."
        )
    );
    // Choosing one goes to Discord at once, without the bar.
    interact(&mut app, |ui| {
        ui.click("Highest").expect("Highest");
    });
    let saving = app
        .client
        .state()
        .server_settings
        .as_ref()
        .is_some_and(|settings| settings.saving);
    assert!(saving && !shows(&app, "Save Changes"));
    assert_eq!(safety(&app).verification(), 4);
    app.client.apply(Event::ServerSettingsSaved {
        server: SERVER.into(),
        result: Ok(ServerSettingsPage::Guild(GuildSettings {
            verification_level: 4,
            ..Default::default()
        })),
    });
    let _ = app.sync();

    // Bans: the list, and a ban's dialog.
    interact(&mut app, |ui| {
        ui.click("Bans").expect("Bans");
    });
    let ban = |id: &str, name: &str, username: Option<&str>, reason: Option<&str>| Ban {
        user: RoleMember {
            id: id.into(),
            name: name.into(),
            username: username.map(str::to_owned),
            avatar: String::new(),
        },
        reason: reason.map(str::to_owned),
    };
    load(
        &mut app,
        SettingsSection::Bans,
        ServerSettingsPage::Bans(vec![
            ban("5", "Mallory", Some("mallory"), Some("spam")),
            ban("6", "trent", None, None),
        ]),
    );
    for name in ["Server Ban List", "Mallory", "mallory", "trent"] {
        assert!(shows(&app, name), "{name}");
    }
    assert!(!shows(&app, "Page 1 of 1"), "no pager for one page");
    interact(&mut app, |ui| {
        ui.click("trent").expect("trent's row");
    });
    for words in ["Ban Reason", "No reason provided", "Revoke Ban", "Done"] {
        assert!(shows(&app, words), "{words}");
    }
    interact(&mut app, |ui| {
        ui.click("Done").expect("Done");
    });
    assert!(!shows(&app, "Ban Reason"));
    match page(&app) {
        SettingsPage::Bans(page) => assert!(page.open.is_none()),
        _ => panic!("not Bans"),
    }
}

#[test]
fn audit_log_shows_rows_that_open_their_numbered_lines() {
    use yapfast_core::chat::{AuditEntry, AuditLog, AuditUser};
    let (mut app, _folder) = discord_server(permission::VIEW_AUDIT_LOG, |_, _| {});
    open_settings(&mut app);
    for page in ["MODERATION", "Audit Log", "Bans"] {
        assert!(shows(&app, page), "{page}");
    }
    interact(&mut app, |ui| {
        ui.click("Audit Log").expect("Audit Log");
    });
    // A kick a minute ago, with its reason, and an unban without lines.
    let now = yapfast_core::clock::now_ms();
    let id = |ago_ms: u64| (((now - ago_ms) - 1_420_070_400_000) << 22).to_string();
    let entry = |id: String, action: u16, reason: Option<&str>| AuditEntry {
        id,
        action,
        target: Some("5".into()),
        user: Some("1".into()),
        reason: reason.map(str::to_owned),
        ..Default::default()
    };
    load(
        &mut app,
        SettingsSection::AuditLog,
        ServerSettingsPage::AuditLog(AuditLog {
            entries: vec![
                entry(id(60_000), 20, Some("spam")),
                entry(id(120_000), 23, None),
            ],
            users: [("1", "alice"), ("5", "mallory")]
                .into_iter()
                .map(|(id, name)| {
                    let user = AuditUser {
                        name: name.into(),
                        avatar: String::new(),
                    };
                    (id.to_owned(), user)
                })
                .collect(),
            names: Default::default(),
        }),
    );
    let log = |app: &App| match page(app) {
        SettingsPage::AuditLog(page) => page.clone(),
        _ => panic!("not Audit Log"),
    };
    assert_eq!(log(&app).rows.len(), 2);
    assert!(shows(&app, "All Users") && shows(&app, "All Actions"));
    // A minute ago: yesterday's in the first minute after midnight. The
    // kick is the newest, the first row.
    let when = |app: &mut App| {
        let mut at = None;
        interact(app, |ui| {
            let _ = ui.find(|candidate: iced_test::selector::Candidate<'_>| {
                if let iced_test::selector::Candidate::Text {
                    content,
                    visible_bounds: Some(bounds),
                    ..
                } = candidate
                    && at.is_none()
                    && (content.starts_with("Today at ") || content.starts_with("Yesterday at "))
                {
                    at = Some(bounds.center());
                }
                None::<()>
            });
        });
        at.expect("when the kick was")
    };
    when(&mut app);
    assert!(!shows(&app, "01—"));

    // The row opens on a click, its lines numbered.
    let at = when(&mut app);
    interact(&mut app, |ui| {
        ui.point_at(at);
        ui.simulate([
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ]);
    });
    assert_eq!(log(&app).open, Some(0));
    assert!(shows(&app, "01—"));

    // The user filter lists who's in the log.
    interact(&mut app, |ui| {
        ui.click("All Users").expect("the user filter");
    });
    assert!(shows(&app, "alice"));
    interact(&mut app, |ui| {
        ui.click("All Users").expect("the user filter");
    });

    // Filtering by Create Channel asks Discord from the newest.
    interact(&mut app, |ui| {
        ui.click("All Actions").expect("the action filter");
    });
    interact(&mut app, |ui| {
        ui.click("Create Channel").expect("Create Channel");
    });
    let filtered = log(&app);
    assert!(filtered.fetching() && filtered.rows.is_empty());
    assert_eq!(filtered.action, Some(10));
    assert!(shows(&app, "Loading..."));
}

#[test]
fn members_and_invites_list_as_discords_pages_do() {
    use yapfast_core::{
        application::{InviteInput, MembersPage},
        chat::{GuildSettings, JoinMethod, ListedMember, RoleMember, ServerInvite},
    };

    use crate::server_settings::{Event as Settings, people};
    let (mut app, _folder) = discord_server(
        permission::KICK_MEMBERS | permission::MANAGE_GUILD | permission::CREATE_INSTANT_INVITE,
        |server, _| {
            server.put_role(
                &serde_json::json!({"id": "20", "name": "Mods", "position": 2,
                                                "color": 0x3498db, "permissions": "0"}),
            );
            server.put_role(
                &serde_json::json!({"id": "30", "name": "Helpers", "position": 1,
                                                "permissions": "0"}),
            );
        },
    );
    open_settings(&mut app);
    for page in ["PEOPLE", "Members", "Invites"] {
        assert!(shows(&app, page), "{page}");
    }

    // Members: a moderator joined three days ago, timed out, and someone
    // whose join Discord doesn't know.
    interact(&mut app, |ui| {
        ui.click("Members").expect("Members");
    });
    let now = yapfast_core::clock::now_ms();
    let member = |id: &str, name: &str, roles: &[&str], joined_by| ListedMember {
        person: RoleMember {
            id: id.into(),
            name: name.into(),
            username: Some(name.to_lowercase()),
            avatar: String::new(),
        },
        roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        joined: now - 3 * 86_400_000,
        timed_out_until: 0,
        unusual_dms: false,
        spammer: false,
        quarantined: false,
        joined_by,
        bot: false,
    };
    let mut mod_ = member(
        "1400000000000000005",
        "Mallory",
        &["30", "20"],
        JoinMethod::Invite("abc".into()),
    );
    mod_.timed_out_until = now + 3_660_000;
    load(
        &mut app,
        SettingsSection::Members,
        ServerSettingsPage::Members {
            members: vec![mod_, member("6", "trent", &[], JoinMethod::Unknown)],
            total: Some(2),
        },
    );
    for words in [
        "Recent Members",
        "JOIN METHOD",
        "Mallory",
        "mallory",
        "Mods",
        "+1",
        "3 days ago",
        "abc",
        "trent",
        "Unknown",
        "Showing 2 members",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    fn members(app: &App) -> &MembersPage {
        match page(app) {
            SettingsPage::Members(page) => page,
            _ => panic!("not Members"),
        }
    }
    // A click opens a member's profile.
    interact(&mut app, |ui| {
        ui.click("trent").expect("trent's row");
    });
    let shown = app
        .client
        .state()
        .profile
        .as_ref()
        .map(|shown| shown.profile.id.clone());
    assert_eq!(shown.as_deref(), Some("6"));
    escape(&mut app);
    assert!(
        app.client.state().profile.is_none(),
        "the profile closes first"
    );
    assert!(app.client.state().server_settings.is_some());

    // The search goes once the typing settles, the last typing's only.
    interact(&mut app, |ui| {
        ui.click(Id::new("members-search")).expect("the search");
        ui.typewrite("mal");
    });
    let settled = |app: &mut App, typing| {
        let _ = app.update(Message::ServerSettings(Settings::MemberLocal(
            people::Event::Settled(typing),
        )));
    };
    settled(&mut app, 2);
    assert!(members(&app).text.is_empty(), "not while typing");
    settled(&mut app, 3);
    assert_eq!(members(&app).text, "mal");
    assert!(shows(&app, "Search Results"));

    // An order starts again.
    interact(&mut app, |ui| {
        ui.click("Member Since (Newest first)").expect("the order");
    });
    interact(&mut app, |ui| {
        ui.click("Member Since (Oldest first)")
            .expect("Oldest first");
    });
    let sorted = members(&app);
    assert!(sorted.fetching() && sorted.members.is_empty() && sorted.sort == 2);
    assert!(shows(&app, "Showing ... members"));

    // Invites: by inviter, counting down, and Security Actions.
    interact(&mut app, |ui| {
        ui.click("Invites").expect("Invites");
    });
    let invite = |code: &str, inviter: &str, max_uses, expires| ServerInvite {
        code: code.into(),
        inviter: Some(RoleMember {
            id: "2".into(),
            name: inviter.to_uppercase(),
            username: Some(inviter.into()),
            avatar: String::new(),
        }),
        channel: "#general".into(),
        uses: 3,
        max_uses,
        expires,
        roles: vec!["20".into()],
        bypass: false,
    };
    load(
        &mut app,
        SettingsSection::Invites,
        ServerSettingsPage::Invites {
            invites: vec![
                invite("zzz", "carol", 10, Some(now / 1000 + 90_000)),
                invite("bob1", "bob", 0, None),
            ],
            settings: GuildSettings::default(),
        },
    );
    for words in [
        "ACTIVE INVITE LINKS",
        "Pause Invites",
        "Create Invite Link",
        "bob",
        "carol",
        "3/10",
        "∞",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    let mut countdown = None;
    interact(&mut app, |ui| countdown = super::starting(ui, "01:0"));
    assert!(countdown.is_some(), "a day and an hour left");
    interact(&mut app, |ui| {
        ui.click("Pause Invites").expect("Pause Invites");
    });
    assert!(shows(&app, "Security Actions") && shows(&app, "2 hours"));
    let draft = |app: &App| match page(app) {
        SettingsPage::Invites(page) => page.security.clone(),
        _ => panic!("not Invites"),
    };
    assert!(draft(&app).is_some_and(|draft| !draft.changed()));
    let _ = app.update(Message::ServerSettings(Settings::Invite(
        InviteInput::PauseInvites,
    )));
    assert!(draft(&app).is_some_and(|draft| draft.invites && draft.changed()));
    escape(&mut app);
    assert!(draft(&app).is_none() && !shows(&app, "Security Actions"));
}

#[test]
fn delete_server_ends_the_owners_sidebar_and_asks_for_the_name() {
    use yapfast_core::application::Ownership;
    let (mut app, _folder) = discord_server(0, |server, _| {
        server.owner_id = Some("1".into());
    });
    open_settings(&mut app);
    assert!(shows(&app, "Delete Server"));
    interact(&mut app, |ui| {
        ui.click("Delete Server").expect("Delete Server");
    });
    let asked = |app: &App| {
        app.client
            .state()
            .server_settings
            .as_ref()
            .and_then(|settings| settings.ownership.clone())
    };
    match asked(&app) {
        Some(Ownership::Delete(ask)) => assert!(ask.by_name, "without 2FA"),
        other => panic!("not Delete Server: {other:?}"),
    }
    for words in ["Delete 'Test'", "ENTER SERVER NAME"] {
        assert!(shows(&app, words), "{words}");
    }
    // A wrong name, and Enter.
    interact(&mut app, |ui| {
        ui.click(Id::new("delete-server-name"))
            .expect("the name's field");
        ui.typewrite("nope");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    assert!(shows(&app, "You didn't enter the server name correctly"));
    escape(&mut app);
    assert!(asked(&app).is_none());
    assert!(app.client.state().server_settings.is_some(), "still open");
}

#[test]
fn emoji_and_stickers_show_as_discords_pages_do() {
    use yapfast_core::{
        application::EmojiInput,
        chat::{GuildSettings, RoleMember, ServerEmoji, ServerSticker, StickerForm},
    };

    use crate::server_settings::Event as Settings;
    let (mut app, _folder) = discord_server(permission::CREATE_GUILD_EXPRESSIONS, |_, _| {});
    open_settings(&mut app);
    for page in ["EXPRESSION", "Emoji", "Stickers"] {
        assert!(shows(&app, page), "{page}");
    }
    let person = |id: &str| RoleMember {
        id: id.into(),
        name: format!("user{id}"),
        username: None,
        avatar: String::new(),
    };
    let emojis = |app: &App| match page(app) {
        SettingsPage::Emoji(page) => page
            .emojis
            .iter()
            .map(|emoji| emoji.name.clone())
            .collect::<Vec<_>>(),
        _ => panic!("not Emoji"),
    };

    // Emoji: newest first; only her own may change.
    interact(&mut app, |ui| {
        ui.click("Emoji").expect("Emoji");
    });
    let emoji = |id: &str, by: &str| ServerEmoji {
        id: id.into(),
        name: format!("emoji{id}"),
        animated: false,
        available: true,
        user: Some(person(by)),
        picture: format!("https://cdn/emojis/{id}.png"),
    };
    load(
        &mut app,
        SettingsSection::Emoji,
        ServerSettingsPage::Emoji {
            emojis: vec![emoji("5", "2"), emoji("6", "1")],
            settings: GuildSettings::default(),
        },
    );
    for words in ["Upload Emoji", "– 48 slots available", "emoji5", "user2"] {
        assert!(shows(&app, words), "{words}");
    }
    assert!(!shows(&app, "Animated Emoji"), "no animated emoji");
    let field = |id: &str| Id::from(format!("emoji-name-{id}"));
    let mut fields = (false, false);
    interact(&mut app, |ui| {
        fields = (ui.find(field("6")).is_ok(), ui.find(field("5")).is_ok());
    });
    assert_eq!(fields, (true, false), "only her own is a field");
    // Renamed in place, on Enter.
    interact(&mut app, |ui| {
        ui.click(field("6")).expect("emoji6's field");
        ui.tap_key(keyboard::Key::Named(key::Named::End));
        ui.typewrite("x");
        ui.tap_key(keyboard::Key::Named(key::Named::Enter));
    });
    assert_eq!(emojis(&app), ["emoji5", "emoji6x"]);
    let _ = app.update(Message::ServerSettings(Settings::Emoji(
        EmojiInput::Delete("6".into()),
    )));
    assert_eq!(emojis(&app), ["emoji5"]);

    // Stickers: Discord's cards, and the form.
    interact(&mut app, |ui| {
        ui.click("Stickers").expect("Stickers");
    });
    let sticker = |id: &str| ServerSticker {
        id: id.into(),
        form: StickerForm {
            name: format!("sticker{id}"),
            tags: "👋".into(),
            description: String::new(),
        },
        format: 1,
        available: true,
        user_id: Some("1".into()),
        user: Some(person("1")),
        picture: Some(format!("https://cdn/stickers/{id}.png")),
    };
    load(
        &mut app,
        SettingsSection::Stickers,
        ServerSettingsPage::Stickers {
            stickers: vec![sticker("7"), sticker("8"), sticker("9")],
            settings: GuildSettings::default(),
        },
    );
    for words in [
        "Free Slots",
        "2 of 5 slots available",
        "Level 1",
        "2 Boosts",
        "sticker7",
        "sticker9",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    interact(&mut app, |ui| {
        ui.click("Upload Sticker").expect("Upload Sticker");
    });
    interact(&mut app, |ui| {
        ui.click(Id::new("sticker-name")).expect("the name's field");
        ui.typewrite("wave");
        ui.click(Id::new("sticker-tags")).expect("related emoji");
        ui.typewrite("👋");
    });
    let draft = |app: &App| match page(app) {
        SettingsPage::Stickers(page) => page.draft.clone(),
        _ => panic!("not Stickers"),
    };
    let form = draft(&app).expect("the form");
    assert!(form.editing.is_none() && !form.ready(), "no file yet");
    assert_eq!(form.form.name, "wave");
    assert!(shows(
        &app,
        "File should be APNG, JPG, PNG or GIF (512 KB max)"
    ));
    interact(&mut app, |ui| {
        ui.click("Cancel").expect("Cancel");
    });
    assert!(draft(&app).is_none());
}

#[test]
fn an_emoji_name_being_written_goes_as_the_page_changes() {
    use yapfast_core::chat::{
        Command, DiscordCommand, EmojiAction, GuildSettings, RoleMember, ServerEmoji,
    };
    let (mut app, _folder) = discord_server(permission::CREATE_GUILD_EXPRESSIONS, |_, _| {});
    open_settings(&mut app);
    interact(&mut app, |ui| {
        ui.click("Emoji").expect("Emoji");
    });
    load(
        &mut app,
        SettingsSection::Emoji,
        ServerSettingsPage::Emoji {
            emojis: vec![ServerEmoji {
                id: "6".into(),
                name: "emoji6".into(),
                animated: false,
                available: true,
                user: Some(RoleMember {
                    id: "1".into(),
                    name: "user1".into(),
                    username: None,
                    avatar: String::new(),
                }),
                picture: "https://cdn/emojis/6.png".into(),
            }],
            settings: GuildSettings::default(),
        },
    );
    interact(&mut app, |ui| {
        ui.click(Id::from("emoji-name-6".to_owned()))
            .expect("emoji6's field");
        ui.tap_key(keyboard::Key::Named(key::Named::End));
        ui.typewrite("x");
    });
    let _ = std::mem::take(&mut app.client.sent);
    interact(&mut app, |ui| {
        ui.click("Stickers").expect("Stickers");
    });
    let renamed = app.client.sent.iter().any(|command| {
        matches!(
            command,
            Command::Discord(DiscordCommand::Emoji {
                action: EmojiAction::Rename { id, name },
                ..
            }) if id == "6" && name == "emoji6x"
        )
    });
    assert!(renamed, "the name typed goes with the page left");
}

#[test]
fn a_name_begun_in_another_emojis_row_sends_only_the_row_left() {
    use yapfast_core::chat::{
        Command, DiscordCommand, EmojiAction, GuildSettings, RoleMember, ServerEmoji,
    };
    let (mut app, _folder) = discord_server(permission::CREATE_GUILD_EXPRESSIONS, |_, _| {});
    open_settings(&mut app);
    interact(&mut app, |ui| {
        ui.click("Emoji").expect("Emoji");
    });
    let emoji = |id: &str| ServerEmoji {
        id: id.into(),
        name: format!("emoji{id}"),
        animated: false,
        available: true,
        user: Some(RoleMember {
            id: "1".into(),
            name: "user1".into(),
            username: None,
            avatar: String::new(),
        }),
        picture: format!("https://cdn/emojis/{id}.png"),
    };
    load(
        &mut app,
        SettingsSection::Emoji,
        ServerSettingsPage::Emoji {
            emojis: vec![emoji("6"), emoji("7")],
            settings: GuildSettings::default(),
        },
    );
    for id in ["6", "7"] {
        interact(&mut app, |ui| {
            ui.click(Id::from(format!("emoji-name-{id}")))
                .expect("the emoji's field");
            ui.tap_key(keyboard::Key::Named(key::Named::End));
            ui.typewrite("x");
        });
    }
    let renamed: Vec<(&str, &str)> = app
        .client
        .sent
        .iter()
        .filter_map(|command| match command {
            Command::Discord(DiscordCommand::Emoji {
                action: EmojiAction::Rename { id, name },
                ..
            }) => Some((id.as_str(), name.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        renamed,
        [("6", "emoji6x")],
        "the name still being written in the second row waits"
    );
}
