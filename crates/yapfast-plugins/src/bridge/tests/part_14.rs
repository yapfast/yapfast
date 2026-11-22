//! The account's own calls: a plugin's files sent with a message, its
//! activity, the profile, search, drafts, sessions and Spotify, and the
//! keybinds and sounds its manifest names. Each is refused without its
//! capability, and each that passes reaches the application as the call the
//! plugin made.

use super::*;
use std::path::Path;

const BEEP: &[u8] = b"RIFF\0\0\0\0WAVEfmt a sound";
const LOGO: &[u8] = b"\x89PNG\r\n\x1a\na picture";

/// A plugin with a sound and a picture, a keybind and a replaced sound, and
/// the capabilities named.
fn account_plugin(directory: &Path, capabilities: &[&str]) -> PluginId {
    let declared: Vec<String> = capabilities
        .iter()
        .map(|capability| format!("\"{capability}\""))
        .collect();
    let keybinds = capabilities.contains(&"keybinds");
    let sounds = capabilities.contains(&"sounds.replace");
    let manifest = format!(
        r#"{{"api":1,"id":"com.example.account","name":"Account","version":"1.0.0",
            "wasm":"plugin.wasm","assets":{{"beep":"beep.wav","logo":"logo.png"}},
            {}{}
            "permissions":{{"capabilities":[{}]}}}}"#,
        if keybinds {
            r#""keybinds":[{"action":"next","label":"Next song"}],"#
        } else {
            ""
        },
        if sounds {
            r#""sounds":{"message":"beep"},"#
        } else {
            ""
        },
        declared.join(",")
    );
    let plugin = installed(directory, &manifest, b"not a real module");
    let lock: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(directory.join("plugins").join("lock.json")).expect("a lock"),
    )
    .expect("a lock file");
    let sha = lock["plugins"][plugin.as_str()]["wasm_sha256"]
        .as_str()
        .expect("its digest")
        .to_owned();
    let assets = directory
        .join("plugins")
        .join("store")
        .join(sha)
        .join("assets");
    std::fs::create_dir_all(&assets).expect("an assets directory");
    std::fs::write(assets.join("beep"), BEEP).expect("a sound");
    std::fs::write(assets.join("logo"), LOGO).expect("a picture");
    plugin
}

/// One call, answered at once or handed to the application (`Pending`).
fn ask_with(
    shared: &Arc<Shared>,
    plugin: &PluginId,
    method: &str,
    payload: serde_json::Value,
) -> HostReturn {
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let call = HostCall {
        method: method.to_owned(),
        payload,
    };
    runtime().block_on(async { session.on_call(shared, 9, plugin.clone(), call) });
    match written.recv_timeout(Duration::from_millis(50)) {
        Ok(frame) => returned(&frame).1,
        Err(_) => HostReturn::Pending {
            tag: "the application".into(),
        },
    }
}

fn denied_for(answer: &HostReturn, capability: &str) {
    let HostReturn::Denied { reason } = answer else {
        panic!("should need `{capability}`: {answer:?}");
    };
    assert!(reason.contains(capability), "{reason}");
}

/// The call the application was handed.
fn handed(notices: &std::sync::mpsc::Receiver<Notice>) -> AppCall {
    loop {
        match notices.recv_timeout(Duration::from_secs(2)) {
            Ok(Notice::Call { request, .. }) => return request,
            Ok(_) => continue,
            Err(_) => panic!("the application wasn't handed the call"),
        }
    }
}

#[test]
fn each_account_call_needs_the_capability_the_review_showed() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = account_plugin(directory.path(), &["messages.send"]);
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    shared.gesture(&plugin);
    for (method, payload, capability) in [
        (
            call::method::ACTIVITY_SET,
            serde_json::json!({ "name": "Pomodoro" }),
            "activity.set",
        ),
        (
            call::method::ACTIVITY_CLEAR,
            serde_json::Value::Null,
            "activity.set",
        ),
        (
            call::method::PROFILE_SET,
            serde_json::json!({ "bio": "hi" }),
            "profile.write",
        ),
        (
            call::method::MESSAGES_SEARCH,
            serde_json::json!({ "place": "room-1", "query": "cats" }),
            "messages.search",
        ),
        (
            call::method::DRAFTS_LIST,
            serde_json::Value::Null,
            "drafts.read",
        ),
        (
            call::method::ACCOUNT_SESSIONS,
            serde_json::Value::Null,
            "account.sessions",
        ),
        (
            call::method::SPOTIFY_NOW,
            serde_json::Value::Null,
            "spotify.control",
        ),
        (
            call::method::SPOTIFY_NEXT,
            serde_json::Value::Null,
            "spotify.control",
        ),
        (
            call::method::MESSAGES_ATTACH,
            serde_json::json!({ "room": "room-1", "body": "look",
                "attachments": [{ "name": "logo.png", "asset": "logo" }] }),
            "messages.attach",
        ),
    ] {
        let answer = ask_with(&shared, &plugin, method, payload);
        denied_for(&answer, capability);
    }
}

#[test]
fn a_plugins_own_files_go_with_a_send_on_the_sends_own_rails() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = account_plugin(directory.path(), &["messages.send", "messages.attach"]);
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let attach = serde_json::json!({ "room": "room-1", "body": "look",
        "attachments": [
            { "name": "logo.png", "asset": "logo" },
            { "name": "pet.gif", "bytes_base64": call::base64::encode(b"GIF89a"), "spoiler": true }
        ] });

    // Nobody clicked: the same refusal any send without a person gets.
    let answer = ask_with(
        &shared,
        &plugin,
        call::method::MESSAGES_ATTACH,
        attach.clone(),
    );
    denied_for(&answer, "messages.send.background");

    shared.gesture(&plugin);
    let answer = ask_with(&shared, &plugin, call::method::MESSAGES_ATTACH, attach);
    assert!(matches!(answer, HostReturn::Pending { .. }), "{answer:?}");
    let AppCall::Send { made, files, .. } = handed(&notices) else {
        panic!("a send");
    };
    assert!(files.is_empty());
    assert_eq!(made.len(), 2);
    assert_eq!(made[0].name, "logo.png");
    assert_eq!(made[0].bytes, LOGO);
    assert_eq!(made[1].bytes, b"GIF89a");
    assert!(made[1].spoiler);

    for wrong in [
        serde_json::json!([{ "name": "../x.png", "asset": "logo" }]),
        serde_json::json!([{ "name": "x.png", "asset": "logo", "bytes_base64": "aGk=" }]),
        serde_json::json!([{ "name": "x.png" }]),
        serde_json::json!([{ "name": "x.png", "asset": "missing" }]),
    ] {
        let answer = ask_with(
            &shared,
            &plugin,
            call::method::MESSAGES_SEND,
            serde_json::json!({ "room": "room-1", "body": "", "attachments": wrong }),
        );
        assert!(
            matches!(answer, HostReturn::Error { .. }),
            "{wrong}: {answer:?}"
        );
    }
    let eleven: Vec<serde_json::Value> = (0..11)
        .map(|n| serde_json::json!({ "name": format!("{n}.png"), "asset": "logo" }))
        .collect();
    let answer = ask_with(
        &shared,
        &plugin,
        call::method::MESSAGES_SEND,
        serde_json::json!({ "room": "room-1", "body": "", "attachments": eleven }),
    );
    let HostReturn::Error { message } = answer else {
        panic!("Discord takes ten files: {answer:?}");
    };
    assert!(message.contains("10"), "{message}");
}

#[test]
fn an_activity_search_sessions_drafts_and_spotify_reach_the_application() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = account_plugin(
        directory.path(),
        &[
            "activity.set",
            "messages.search",
            "drafts.read",
            "account.sessions",
            "spotify.control",
        ],
    );
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);

    let answer = ask_with(
        &shared,
        &plugin,
        call::method::ACTIVITY_SET,
        serde_json::json!({ "name": "Pomodoro", "details": "Focus",
            "buttons": [{ "label": "Site", "url": "https://example.com" }] }),
    );
    assert!(matches!(answer, HostReturn::Pending { .. }), "{answer:?}");
    let AppCall::SetPluginActivity {
        activity: Some(activity),
    } = handed(&notices)
    else {
        panic!("an activity");
    };
    assert_eq!(activity.name, "Pomodoro");

    // Discord's own limits are the only ones: a type that isn't a plugin's,
    // a picture with no application to take it, and a third button.
    for wrong in [
        serde_json::json!({ "name": "Live", "kind": "streaming" }),
        serde_json::json!({ "name": "Art", "large_image": "https://example.com/a.png" }),
        serde_json::json!({ "name": "x".repeat(129) }),
    ] {
        let answer = ask_with(&shared, &plugin, call::method::ACTIVITY_SET, wrong.clone());
        assert!(
            matches!(answer, HostReturn::Error { .. }),
            "{wrong}: {answer:?}"
        );
    }

    ask_with(
        &shared,
        &plugin,
        call::method::ACTIVITY_CLEAR,
        serde_json::Value::Null,
    );
    assert_eq!(
        handed(&notices),
        AppCall::SetPluginActivity { activity: None }
    );
    ask_with(
        &shared,
        &plugin,
        call::method::DRAFTS_LIST,
        serde_json::Value::Null,
    );
    assert_eq!(handed(&notices), AppCall::Drafts);
    ask_with(
        &shared,
        &plugin,
        call::method::ACCOUNT_SESSIONS,
        serde_json::Value::Null,
    );
    assert_eq!(handed(&notices), AppCall::Sessions);
    ask_with(
        &shared,
        &plugin,
        call::method::MESSAGES_SEARCH,
        serde_json::json!({ "place": "room-1", "query": "cats", "offset": 25 }),
    );
    let AppCall::Search(search) = handed(&notices) else {
        panic!("a search");
    };
    assert_eq!(search.offset, 25);

    ask_with(
        &shared,
        &plugin,
        call::method::SPOTIFY_SEEK,
        serde_json::json!({ "position_ms": 3000 }),
    );
    assert_eq!(
        handed(&notices),
        AppCall::Spotify(yapfast_plugin_api::account::SpotifyAction::Seek { position_ms: 3000 })
    );
    let answer = ask_with(
        &shared,
        &plugin,
        call::method::SPOTIFY_VOLUME,
        serde_json::json!({ "percent": 150 }),
    );
    assert!(matches!(answer, HostReturn::Error { .. }), "{answer:?}");
}

#[test]
fn a_profile_write_is_asked_about_the_first_time_without_the_powerful_switch() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = account_plugin(directory.path(), &["profile.write"]);
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    assert!(!shared.powerful.enabled(), "the Advanced switch is off");
    let bio = serde_json::json!({ "bio": "hello", "pronouns": "they/them" });

    let answer = ask_with(&shared, &plugin, call::method::PROFILE_SET, bio.clone());
    let HostReturn::Denied { reason } = answer else {
        panic!("the first use asks: {answer:?}");
    };
    assert!(reason.contains("being asked"), "{reason}");
    let asked = notices
        .recv_timeout(Duration::from_secs(2))
        .expect("the person is asked");
    assert!(
        matches!(
            asked,
            Notice::Powerful {
                capability: Capability::ProfileWrite,
                ..
            }
        ),
        "{asked:?}"
    );

    shared
        .powerful
        .allow(&plugin, Capability::ProfileWrite, None)
        .expect("saved");
    let answer = ask_with(&shared, &plugin, call::method::PROFILE_SET, bio);
    assert!(matches!(answer, HostReturn::Pending { .. }), "{answer:?}");
    let AppCall::SetProfile(profile) = handed(&notices) else {
        panic!("a profile write");
    };
    assert_eq!(profile.bio.as_deref(), Some("hello"));

    let long = serde_json::json!({ "bio": "a".repeat(191) });
    let answer = ask_with(&shared, &plugin, call::method::PROFILE_SET, long);
    assert!(matches!(answer, HostReturn::Error { .. }), "{answer:?}");

    shared
        .powerful
        .revoke(&plugin, Capability::ProfileWrite)
        .expect("saved");
    let answer = ask_with(
        &shared,
        &plugin,
        call::method::PROFILE_SET,
        serde_json::json!({ "bio": "again" }),
    );
    assert!(
        matches!(answer, HostReturn::Denied { .. }),
        "revoked asks again"
    );
}

#[test]
fn keybinds_and_sounds_come_only_from_a_plugin_that_declares_them() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = account_plugin(directory.path(), &["keybinds", "sounds.replace"]);
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let offered = shared.keybind_actions();
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].label, "Next song");
    assert_eq!(offered[0].name, "Account");
    assert!(shared.declares_keybind(&plugin, "next"));
    assert!(!shared.declares_keybind(&plugin, "delete-everything"));
    let (from, sound) = shared.sound_for("message").expect("a replaced sound");
    assert_eq!(from, plugin);
    assert_eq!(sound, BEEP);
    assert!(shared.sound_for("mention").is_none(), "only what it names");

    let other = tempfile::tempdir().expect("a temporary directory");
    let quiet = account_plugin(other.path(), &[]);
    let (wake, _notices) = notice_channel();
    let none = super::shared(other.path(), wake);
    assert!(none.keybind_actions().is_empty());
    assert!(!none.declares_keybind(&quiet, "next"));
    assert!(none.sound_for("message").is_none());
}
