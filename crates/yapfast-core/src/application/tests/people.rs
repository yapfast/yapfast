//! Members and Invites in a Discord server's settings.

use super::*;
use crate::chat::{
    GuildSettings, InvitesAction, JoinMethod, ListedMember, MembersQuery, RoleMember, ServerInvite,
    ServerSettingsPage, SettingsSection,
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

fn person(id: &str, name: &str) -> RoleMember {
    RoleMember {
        id: id.into(),
        name: name.into(),
        username: None,
        avatar: format!("https://cdn/{id}.png"),
    }
}

/// A member who joined `joined` milliseconds in.
fn member(id: u64, joined: u64) -> ListedMember {
    ListedMember {
        person: person(&id.to_string(), &format!("member{id}")),
        roles: vec![],
        joined,
        timed_out_until: 0,
        unusual_dms: false,
        spammer: false,
        quarantined: false,
        joined_by: JoinMethod::Invite("abc".into()),
        bot: false,
    }
}

fn members(app: &Application) -> &MembersPage {
    match &app.state.server_settings.as_ref().unwrap().page {
        Some(SettingsPage::Members(page)) => page,
        _ => panic!("Members isn't open"),
    }
}

fn invites(app: &Application) -> &InvitesPage {
    match &app.state.server_settings.as_ref().unwrap().page {
        Some(SettingsPage::Invites(page)) => page,
        _ => panic!("Invites isn't open"),
    }
}

/// What Members asked Discord for, among the avatars it loads.
fn searched(effects: &[Effect]) -> Option<&MembersQuery> {
    effects.iter().find_map(|effect| match effect {
        Effect::Backend(Command::Discord(DiscordCommand::Members { server, query }))
            if server == SERVER =>
        {
            Some(query)
        }
        _ => None,
    })
}

fn asked(effects: &[Effect]) -> Option<&InvitesAction> {
    match effects {
        [Effect::Backend(Command::Discord(DiscordCommand::Invites { server, action }))]
            if server == SERVER =>
        {
            Some(action)
        }
        _ => None,
    }
}

/// Members open with `listed`, newest first, of `total`.
fn listing(app: &mut Application, listed: Vec<ListedMember>, total: Option<u64>) {
    open(app, SettingsSection::Members);
    app.apply(Event::ServerSettingsLoaded {
        server: SERVER.into(),
        section: SettingsSection::Members,
        result: Ok(ServerSettingsPage::Members {
            members: listed,
            total,
        }),
    });
}

#[test]
fn members_shows_for_moderators_and_invites_for_manage_server() {
    // Alice's Mods kick and manage roles.
    let app = discord_server();
    assert_eq!(
        app.settings_sections(SERVER),
        [SettingsSection::Members, SettingsSection::Roles]
    );
    let app = granting(permission::MANAGE_NICKNAMES);
    assert_eq!(app.settings_sections(SERVER), [SettingsSection::Members]);
    let app = granting(permission::MANAGE_GUILD);
    let sections = app.settings_sections(SERVER);
    let at = |section| sections.iter().position(|&known| known == section).unwrap();
    assert!(at(SettingsSection::BoostPerks) < at(SettingsSection::Members));
    assert!(at(SettingsSection::Members) < at(SettingsSection::Invites));
    assert!(at(SettingsSection::Invites) < at(SettingsSection::Safety));
    let mut app = app;
    open(&mut app, SettingsSection::Invites);
    assert!(app.state.server_settings.as_ref().unwrap().immediate());
}

#[test]
fn members_pages_through_discords_chunks_and_drops_late_answers() {
    let mut app = discord_server();
    // A full chunk, oldest last, of more.
    let chunk: Vec<_> = (0..MEMBERS_CHUNK as u64)
        .map(|n| member(1000 + n, 10_000 - n))
        .collect();
    listing(&mut app, chunk, Some(300));
    let page = members(&app);
    assert_eq!(
        (page.count(), page.pages(), page.on_page().len()),
        (300, 25, 12)
    );
    assert!(page.more && !page.searched());

    // Pages within the chunk ask for nothing; past it, for what follows
    // its last member.
    // Each answer also loads the page's avatars.
    assert_eq!(
        searched(&app.dispatch(Action::Members(MemberInput::Page(3)))),
        None
    );
    let next = app.dispatch(Action::Members(MemberInput::Page(20)));
    let query = searched(&next).cloned().unwrap();
    assert_eq!(
        query.after,
        Some((10_000 - (MEMBERS_CHUNK as u64 - 1), "1249".into()))
    );
    assert_eq!(
        searched(&app.dispatch(Action::Members(MemberInput::Page(21)))),
        None,
        "one chunk at a time"
    );
    app.apply(Event::MembersFound {
        server: SERVER.into(),
        query,
        result: Ok(((0..50).map(|n| member(2000 + n, 5_000 - n)).collect(), None)),
    });
    let page = members(&app);
    assert_eq!((page.members.len(), page.page, page.more), (300, 21, false));

    // After a member's change, the search starts over on the same page and
    // fetches past the first chunk again to fill it.
    let again = app.members_again();
    let query = searched(&again).cloned().unwrap();
    assert_eq!(query.after, None);
    let first = (0..MEMBERS_CHUNK as u64).map(|n| member(1000 + n, 10_000 - n));
    let next = app.apply(Event::MembersFound {
        server: SERVER.into(),
        query,
        result: Ok((first.collect(), Some(300))),
    });
    assert_eq!(members(&app).page, 21);
    let query = searched(&next).cloned().expect("the next chunk");
    assert!(query.after.is_some());
    app.apply(Event::MembersFound {
        server: SERVER.into(),
        query,
        result: Ok(((0..50).map(|n| member(2000 + n, 5_000 - n)).collect(), None)),
    });
    assert_eq!(members(&app).on_page().len(), 12);

    // A hundred a page fetch 500 at a time, as Discord's client does.
    app.dispatch(Action::Members(MemberInput::PageSize(100)));
    let search = app.dispatch(Action::Members(MemberInput::Search("member".into())));
    assert_eq!(searched(&search).map(MembersQuery::chunk), Some(500));
    app.dispatch(Action::Members(MemberInput::Search(String::new())));
    app.dispatch(Action::Members(MemberInput::PageSize(12)));

    // A search starts over; an answer to what was asked before is dropped.
    let search = app.dispatch(Action::Members(MemberInput::Search(" bob ".into())));
    let query = searched(&search).cloned().unwrap();
    assert_eq!((query.text.as_str(), &query.after), ("bob", &None));
    assert!(members(&app).members.is_empty() && members(&app).searched());
    app.apply(Event::MembersIndexing {
        server: SERVER.into(),
        query: query.clone(),
    });
    assert!(members(&app).indexing);
    app.apply(Event::MembersFound {
        server: SERVER.into(),
        query: MembersQuery::default(),
        result: Ok((vec![member(7, 1)], None)),
    });
    assert!(members(&app).fetching());
    app.apply(Event::MembersFound {
        server: SERVER.into(),
        query,
        result: Ok((vec![member(7, 1), member(8, 2)], Some(2))),
    });
    let page = members(&app);
    assert!(!page.fetching() && !page.indexing);
    // Newest first, as Discord's sort 0.
    assert_eq!(page.members[0].person.id, "8");

    // Joined Discord, oldest first: by id.
    let sorted = app.dispatch(Action::Members(MemberInput::Sort(4)));
    let query = searched(&sorted).cloned().unwrap();
    assert_eq!(query.sort, 4);
    app.apply(Event::MembersFound {
        server: SERVER.into(),
        query,
        result: Ok((vec![member(9, 3), member(8, 2)], None)),
    });
    assert_eq!(members(&app).members[0].person.id, "8");

    // A bigger page keeps the first member shown in view.
    assert_eq!(
        searched(&app.dispatch(Action::Members(MemberInput::PageSize(7)))),
        None
    );
    app.dispatch(Action::Members(MemberInput::PageSize(25)));
    assert_eq!(members(&app).page_size, 25);
}

#[test]
fn a_listed_members_menu_acts_in_the_settings_server() {
    let mut app = discord_server();
    let mut boss = member(600, 2);
    // Above Alice's Mods.
    boss.roles = vec!["40".into()];
    listing(&mut app, vec![member(500, 1), boss], None);
    assert_eq!(app.member_place("500"), Some(SERVER));
    let choices = app.person_choices("500");
    assert!(choices.contains(&MenuChoice::Kick));
    assert!(!choices.contains(&MenuChoice::Mention));
    // Their listed roles outrank Alice's.
    assert!(!app.person_choices("600").contains(&MenuChoice::Kick));

    app.dispatch(Action::AskMember {
        place: SERVER.into(),
        user: "500".into(),
        ask: PersonAsk::Kick,
    });
    let kick = app.dispatch(Action::AnswerPerson("spam".into()));
    assert!(matches!(
        kick.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Member { server, user, .. }))]
            if server == SERVER && user == "500"
    ));
    // Once it's done, Members shows it.
    let again = app.apply(Event::PersonSaved(Ok(())));
    let query = searched(&again).unwrap();
    assert_eq!(
        (query.text.as_str(), query.after.as_ref(), query.chunk()),
        ("", None, 250)
    );
}

fn invite(code: &str, inviter: &str, expires: Option<u64>) -> ServerInvite {
    ServerInvite {
        code: code.into(),
        inviter: Some(person(inviter, inviter)),
        channel: "#general".into(),
        uses: 1,
        max_uses: 0,
        expires,
        roles: vec![],
        bypass: false,
    }
}

/// Invites open, for Alice holding Manage Server and Create Invite.
fn invites_open(settings: GuildSettings) -> Application {
    let mut app = granting(permission::MANAGE_GUILD | permission::CREATE_INSTANT_INVITE);
    open(&mut app, SettingsSection::Invites);
    app.apply(Event::ServerSettingsLoaded {
        server: SERVER.into(),
        section: SettingsSection::Invites,
        result: Ok(ServerSettingsPage::Invites {
            invites: vec![invite("zzz", "Carol", None), invite("aaa", "bob", Some(1))],
            settings,
        }),
    });
    app
}

#[test]
fn invites_list_by_inviter_and_revoke_at_once() {
    let mut app = invites_open(GuildSettings::default());
    let codes = |app: &Application| {
        invites(app)
            .invites
            .iter()
            .map(|invite| invite.code.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(codes(&app), ["aaa", "zzz"]);
    assert!(
        app.dispatch(Action::Invites(InviteInput::Revoke("nope".into())))
            .is_empty()
    );
    let revoke = app.dispatch(Action::Invites(InviteInput::Revoke("zzz".into())));
    assert_eq!(asked(&revoke), Some(&InvitesAction::Revoke("zzz".into())));
    app.apply(Event::InviteRevoked {
        server: SERVER.into(),
        code: "zzz".into(),
        result: Ok(()),
    });
    assert_eq!(codes(&app), ["aaa"]);

    // Create Invite Link opens the invite dialog; the invite it makes is
    // listed.
    app.dispatch(Action::Invites(InviteInput::Create));
    let room = app.state.invite.as_ref().unwrap().room.clone();
    let made = app.apply(Event::InviteMade {
        room,
        result: Ok("new".into()),
    });
    assert_eq!(asked(&made), Some(&InvitesAction::Reload));
    app.apply(Event::InvitesLoaded {
        server: SERVER.into(),
        result: Ok(vec![invite("new", "Alice", None)]),
    });
    assert_eq!(codes(&app), ["new"]);
}

#[test]
fn security_actions_pause_invites_and_dms_for_some_hours() {
    let now = crate::clock::now_ms() / 1000;
    let mut app = invites_open(GuildSettings {
        dms_paused_until: Some(now + 600),
        ..GuildSettings::default()
    });
    assert!(!invites(&app).paused(now) && invites(&app).dms_paused(now));
    app.dispatch(Action::Invites(InviteInput::Security));
    let draft = invites(&app).security.clone().unwrap();
    assert_eq!((draft.hours, draft.invites, draft.dms), (2, false, true));
    // Save waits for a change.
    assert!(
        app.dispatch(Action::Invites(InviteInput::SaveSecurity))
            .is_empty()
    );
    app.dispatch(Action::Invites(InviteInput::Hours(5)));
    app.dispatch(Action::Invites(InviteInput::Hours(6)));
    app.dispatch(Action::Invites(InviteInput::PauseInvites));
    let save = app.dispatch(Action::Invites(InviteInput::SaveSecurity));
    assert_eq!(
        asked(&save),
        Some(&InvitesAction::Pause {
            invites: true,
            dms: true,
            hours: 6
        })
    );
    // A refusal keeps the dialog to try again.
    app.apply(Event::ServerPaused {
        server: SERVER.into(),
        result: Err("no".into()),
    });
    let draft = invites(&app).security.clone().unwrap();
    assert_eq!((draft.saving, draft.error.as_deref()), (false, Some("no")));
    app.dispatch(Action::Invites(InviteInput::SaveSecurity));
    app.apply(Event::ServerPaused {
        server: SERVER.into(),
        result: Ok((Some(now + 6 * 3600), Some(now + 6 * 3600))),
    });
    assert!(invites(&app).security.is_none() && invites(&app).paused(now));
}
