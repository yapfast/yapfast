//! Audit Log in a Discord server's settings.

use serde_json::{Value, json};

use super::*;
use crate::chat::{
    AuditChange, AuditEntry, AuditLog, AuditQuery, AuditUser, ServerSettingsPage, SettingsSection,
};
use yapfast_discord::ready::permission;

const SERVER: &str = "discord:10";

/// A snowflake made `minutes` after a fixed time.
fn at(minutes: u64) -> String {
    ((1_790_000_000_000 + minutes * 60_000 - 1_420_070_400_000) << 22).to_string()
}

fn entry(minutes: u64, action: u16, target: Option<&str>, user: &str) -> AuditEntry {
    AuditEntry {
        id: at(minutes),
        action,
        target: target.map(str::to_owned),
        user: Some(user.to_owned()),
        reason: None,
        changes: vec![],
        options: Value::Null,
    }
}

fn changed(key: &str, old: Value, new: Value) -> AuditChange {
    AuditChange {
        key: key.into(),
        old,
        new,
    }
}

fn log(entries: Vec<AuditEntry>) -> AuditLog {
    let user = |name: &str| AuditUser {
        name: name.into(),
        avatar: format!("https://cdn/{name}.png"),
    };
    AuditLog {
        entries,
        users: [("1", user("alice")), ("5", user("mallory"))]
            .into_iter()
            .map(|(id, user)| (id.to_owned(), user))
            .collect(),
        names: Default::default(),
    }
}

/// Audit Log open, for Alice holding View Audit Log.
fn open() -> Application {
    let mut app = discord_server();
    change_server(&mut app, |server| {
        server.put_role(&json!({"id": "20", "name": "Mods", "position": 2,
                                "permissions": permission::VIEW_AUDIT_LOG.to_string()}));
    });
    app.dispatch(Action::OpenServerSettings(SERVER.into()));
    app
}

fn page(app: &Application) -> &AuditLogPage {
    match &app.state.server_settings.as_ref().unwrap().page {
        Some(SettingsPage::AuditLog(page)) => page,
        _ => panic!("Audit Log isn't open"),
    }
}

/// A sentence as plain text, bold in `**`.
fn words(words: &AuditWords) -> String {
    words
        .spans()
        .map(|span| match span {
            AuditSpan::Text(text) => text.to_owned(),
            AuditSpan::Bold(text) => format!("**{text}**"),
            AuditSpan::Time(seconds) => format!("**<{seconds}>**"),
        })
        .collect()
}

fn titles(app: &Application) -> Vec<String> {
    page(app).rows.iter().map(|row| words(&row.title)).collect()
}

fn lines(row: &AuditRow) -> Vec<String> {
    row.lines.iter().map(|line| words(&line.words)).collect()
}

fn asked(effects: &[Effect]) -> Option<&AuditQuery> {
    match effects {
        [Effect::Backend(Command::Discord(DiscordCommand::AuditLog { server, query }))]
            if server == SERVER =>
        {
            Some(query)
        }
        _ => None,
    }
}

#[test]
fn audit_log_opens_with_view_audit_log_before_bans() {
    let mut app = open();
    assert_eq!(
        app.settings_sections(SERVER),
        [SettingsSection::AuditLog, SettingsSection::Bans]
    );
    assert!(app.state.server_settings.as_ref().unwrap().immediate());
    // The first page comes as the section's.
    app.apply(Event::ServerSettingsLoaded {
        server: SERVER.into(),
        section: SettingsSection::AuditLog,
        result: Ok(ServerSettingsPage::AuditLog(log(vec![]))),
    });
    assert!(page(&app).rows.is_empty() && !page(&app).older && !page(&app).failed);
}

#[test]
fn entries_read_as_discords_sentences_and_lines() {
    let mut app = open();
    let mut kick = entry(60, 20, Some("5"), "1");
    kick.reason = Some("spam".into());
    let mut role = entry(50, 31, Some("30"), "1");
    role.changes = vec![
        changed("name", json!("Helpers"), json!("Helpers+")),
        changed(
            "permissions",
            json!("0"),
            json!((permission::KICK_MEMBERS | permission::VIEW_CHANNEL).to_string()),
        ),
    ];
    // Two nickname changes close together read as one row.
    let mut nick = entry(40, 24, Some("5"), "1");
    nick.changes = vec![changed("nick", Value::Null, json!("Mal"))];
    let mut nick_again = entry(45, 24, Some("5"), "1");
    nick_again.changes = vec![
        changed("nick", json!("Mal"), json!("Mallory")),
        changed(
            "communication_disabled_until",
            Value::Null,
            json!("2026-10-21T16:20:00+00:00"),
        ),
    ];
    let mut overwrite = entry(30, 13, Some("11"), "1");
    overwrite.options = json!({"id": "30", "type": "0", "role_name": "Helpers"});
    overwrite.changes = vec![
        changed(
            "allow",
            Value::Null,
            json!(permission::MANAGE_CHANNELS.to_string()),
        ),
        changed("type", Value::Null, json!(0)),
    ];
    // A channel deleted later is named by what the deletion recorded.
    let mut renamed = entry(10, 11, Some("77"), "1");
    renamed.changes = vec![changed("topic", Value::Null, json!("cats"))];
    let mut deleted = entry(20, 12, Some("77"), "1");
    deleted.changes = vec![
        changed("name", json!("old-chat"), Value::Null),
        changed("type", json!(0), Value::Null),
    ];
    let mut moved = entry(5, 26, None, "1");
    moved.options =
        json!({"channel_id": "14", "count": "3", "unique_count": "1", "member_id": "5"});
    let unknown = entry(1, 999, None, "9");
    app.apply(Event::ServerSettingsLoaded {
        server: SERVER.into(),
        section: SettingsSection::AuditLog,
        result: Ok(ServerSettingsPage::AuditLog(log(vec![
            kick, role, nick_again, nick, overwrite, deleted, renamed, moved, unknown,
        ]))),
    });
    assert_eq!(
        titles(&app),
        [
            "**alice** kicked **mallory**",
            "**alice** updated the role **Helpers**",
            "**alice** updated **mallory**",
            "**alice** created channel overrides for **#general**",
            "**alice** removed **#old-chat**",
            "**alice** made changes to **#old-chat**",
            "**alice** moved **mallory** to **Lounge** (**3 actions**)",
            "**Unknown User** made changes",
        ]
    );
    let rows = &page(&app).rows;
    assert_eq!(lines(&rows[0]), ["With reason **spam**"]);
    assert_eq!(rows[0].kind, AuditKind::Delete);
    let actor = rows[0]
        .actor
        .map(|index| &page(&app).actors[index as usize].1);
    assert_eq!(actor.unwrap().avatar, "https://cdn/alice.png");
    assert_eq!(
        lines(&rows[1]),
        [
            "Changed the name from **Helpers** to **Helpers+**",
            "**Granted** permissions",
        ]
    );
    assert_eq!(rows[1].lines[1].items, ["Kick Members", "View Channels"]);
    // Grouped: the oldest's id, both times, every change in order.
    assert_eq!(
        (rows[2].id.to_string(), rows[2].start < rows[2].end),
        (at(40), true)
    );
    assert_eq!(
        lines(&rows[2]),
        [
            "Set their nickname to **Mal**",
            "Changed their nickname from **Mal** to **Mallory**",
            "Set timeout for user until **<1792599600>**",
        ]
    );
    assert_eq!(
        lines(&rows[3]),
        ["**Granted** a permission for **Helpers**"]
    );
    assert_eq!(rows[3].lines[0].items, ["Manage Channel"]);
    // A deletion shows only its reason.
    assert!(rows[4].lines.is_empty());
    assert_eq!(lines(&rows[5]), ["Set the topic to **cats**"]);
}

#[test]
fn filters_and_older_pages_ask_discord_and_drop_what_came_too_late() {
    let mut app = open();
    let full: Vec<AuditEntry> = (0..AUDIT_PAGE as u64)
        .rev()
        .map(|n| entry(n * 60, 22, Some("5"), "1"))
        .collect();
    app.apply(Event::ServerSettingsLoaded {
        server: SERVER.into(),
        section: SettingsSection::AuditLog,
        result: Ok(ServerSettingsPage::AuditLog(log(full))),
    });
    assert!(page(&app).older);
    assert_eq!(page(&app).actors.len(), 1);

    // Older entries start from the last row.
    let more = app.dispatch(Action::AuditLog(AuditInput::More));
    let older = asked(&more).cloned().unwrap();
    assert_eq!(older.before.as_deref(), Some(at(0).as_str()));
    assert!(
        app.dispatch(Action::AuditLog(AuditInput::More)).is_empty(),
        "one at a time"
    );
    assert!(
        app.dispatch(Action::AuditLog(AuditInput::Action(Some(20))))
            .is_empty(),
        "filters wait, as in Discord's client"
    );
    app.apply(Event::AuditLogLoaded {
        server: SERVER.into(),
        query: older,
        result: Ok(log(vec![entry(0, 20, Some("5"), "5")])),
    });
    assert_eq!(page(&app).rows.len(), AUDIT_PAGE + 1);
    assert!(!page(&app).older);
    assert_eq!(page(&app).actors.len(), 2);

    // A filter starts again from the newest.
    let filtered = app.dispatch(Action::AuditLog(AuditInput::User(Some("5".into()))));
    let query = asked(&filtered).cloned().unwrap();
    assert_eq!(
        query,
        AuditQuery {
            before: None,
            user: Some("5".into()),
            action: None,
            target: None,
        }
    );
    assert!(page(&app).rows.is_empty() && page(&app).fetching());
    // An answer to another query is dropped.
    app.apply(Event::AuditLogLoaded {
        server: SERVER.into(),
        query: AuditQuery::default(),
        result: Ok(log(vec![entry(0, 20, Some("5"), "1")])),
    });
    assert!(page(&app).fetching());
    app.apply(Event::AuditLogLoaded {
        server: SERVER.into(),
        query,
        result: Err("no".into()),
    });
    assert!(page(&app).failed && !page(&app).fetching());
}

#[test]
fn a_row_opens_its_lines_one_at_a_time() {
    let mut app = open();
    let mut first = entry(10, 20, Some("5"), "1");
    first.reason = Some("spam".into());
    let mut second = entry(5, 22, Some("5"), "1");
    second.reason = Some("more spam".into());
    app.apply(Event::ServerSettingsLoaded {
        server: SERVER.into(),
        section: SettingsSection::AuditLog,
        result: Ok(ServerSettingsPage::AuditLog(log(vec![
            first,
            second,
            entry(1, 23, Some("5"), "1"),
        ]))),
    });
    let toggle = |app: &mut Application, row| {
        app.dispatch(Action::AuditLog(AuditInput::Toggle(row)));
    };
    toggle(&mut app, 0);
    assert_eq!(page(&app).open, Some(0));
    toggle(&mut app, 1);
    assert_eq!(page(&app).open, Some(1));
    // A row with nothing to show doesn't open.
    toggle(&mut app, 2);
    assert_eq!(page(&app).open, Some(1));
    toggle(&mut app, 1);
    assert_eq!(page(&app).open, None);
}
