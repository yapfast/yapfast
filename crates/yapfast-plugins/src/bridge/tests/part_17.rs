use super::*;

use yapfast_plugin_api::call::{HostCall, HostReturn};

const SPOTIFY_MANIFEST: &str = r#"{"api":1,"id":"com.example.spotify","name":"Spot","version":"1.0.0",
    "wasm":"plugin.wasm","permissions":{"capabilities":["oauth.login"],"network":["api.spotify.com"]},
    "oauth":[{"provider":"spotify","client_id":"abc123","scopes":["user-read-playback-state"]}]}"#;

fn ask(shared: &Arc<Shared>, plugin: &PluginId, method: &str, provider: &str) -> HostReturn {
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let call = HostCall {
        method: method.to_owned(),
        payload: serde_json::json!({ "provider": provider }),
    };
    // A token is answered from a task of its own, which runs while this waits.
    runtime().block_on(async {
        session.on_call(shared, 9, plugin.clone(), call);
        tokio::time::sleep(Duration::from_millis(200)).await;
    });
    returned(
        &written
            .recv_timeout(Duration::from_secs(2))
            .expect("an answer"),
    )
    .1
}

/// A sign-in opens the browser, so it needs the person's click; a service the
/// manifest doesn't name, or a token before any sign-in, is said plainly.
#[test]
fn a_sign_in_needs_a_click_and_a_service_the_manifest_names() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), SPOTIFY_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);

    let answer = ask(&shared, &plugin, call::method::OAUTH_LOGIN, "spotify");
    let HostReturn::Denied { reason } = answer else {
        panic!("a sign-in without a click: {answer:?}");
    };
    assert!(reason.contains("click"), "{reason}");

    shared.gesture(&plugin);
    let answer = ask(&shared, &plugin, call::method::OAUTH_LOGIN, "twitch");
    assert!(
        matches!(&answer, HostReturn::Error { message } if message.contains("twitch")),
        "{answer:?}"
    );

    let answer = ask(&shared, &plugin, call::method::OAUTH_TOKEN, "spotify");
    assert!(
        matches!(&answer, HostReturn::Error { message } if message.contains("oauth.login")),
        "not signed in yet: {answer:?}"
    );

    let logout = crate::bridge::session_oauth::oauth_logout(
        &shared,
        &plugin,
        &HostCall {
            method: call::method::OAUTH_LOGOUT.to_owned(),
            payload: serde_json::json!({ "provider": "spotify" }),
        },
    );
    assert_eq!(logout, HostReturn::Ok(serde_json::Value::Bool(false)));
}

/// A plugin whose decorations or identities changed says so, with no
/// capability of its own: the application is told to ask again.
#[test]
fn a_plugin_asks_for_its_decorations_and_identities_to_be_asked_again() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(
        directory.path(),
        r#"{"api":1,"id":"com.example.reviews","name":"Reviews","version":"1.0.0",
            "wasm":"plugin.wasm","permissions":{"capabilities":[]}}"#,
        b"not a real module",
    );
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    for (method, payload) in [
        (call::method::UI_REDECORATE, serde_json::json!({})),
        (call::method::UI_REDECORATE_MEMBERS, serde_json::json!({})),
        (
            call::method::IDENTITY_REFRESH,
            serde_json::json!({ "person": "42" }),
        ),
    ] {
        let call = HostCall {
            method: method.to_owned(),
            payload,
        };
        runtime().block_on(async { session.on_call(&shared, 9, plugin.clone(), call) });
        let answer = returned(
            &written
                .recv_timeout(Duration::from_secs(2))
                .expect("an answer"),
        )
        .1;
        assert_eq!(
            answer,
            HostReturn::Ok(serde_json::Value::Bool(true)),
            "{method}"
        );
    }
    let told: Vec<Notice> = notices.try_iter().collect();
    assert!(told.contains(&Notice::Redecorate {
        plugin: plugin.clone()
    }));
    assert!(told.contains(&Notice::RedecorateMembers {
        plugin: plugin.clone()
    }));
    assert!(told.contains(&Notice::IdentityRefresh {
        plugin,
        person: Some("42".to_owned()),
    }));
}

/// An overlay shows the package's own pictures, at a pace the plugin names, and
/// needs `overlay`.
#[test]
fn an_overlay_is_drawn_from_the_packages_pictures() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(
        directory.path(),
        r#"{"api":1,"id":"com.example.cat","name":"Cat","version":"1.0.0",
            "wasm":"plugin.wasm","assets":{"cat":"cat.png"},
            "permissions":{"capabilities":["overlay"]}}"#,
        b"not a real module",
    );
    let elsewhere = tempfile::tempdir().expect("a temporary directory");
    let quiet = installed(
        elsewhere.path(),
        r#"{"api":1,"id":"com.example.quiet","name":"Quiet","version":"1.0.0",
            "wasm":"plugin.wasm","assets":{"cat":"cat.png"},
            "permissions":{"capabilities":[]}}"#,
        b"not a real module",
    );
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (quiet_wake, _quiet_notices) = notice_channel();
    let quiet_shared = super::support_01::shared(elsewhere.path(), quiet_wake);
    let ask_in = |shared: &Arc<Shared>, plugin: &PluginId, payload: serde_json::Value| {
        let (_session_directory, mut session, written, _inbound) = session_stub();
        let call = HostCall {
            method: call::method::OVERLAY_SET.to_owned(),
            payload,
        };
        runtime().block_on(async { session.on_call(shared, 9, plugin.clone(), call) });
        returned(
            &written
                .recv_timeout(Duration::from_secs(2))
                .expect("an answer"),
        )
        .1
    };
    let ask = |plugin: &PluginId, payload: serde_json::Value| ask_in(&shared, plugin, payload);
    let cat = serde_json::json!({ "fps": 12, "sprites": [
        { "id": "neko", "image": "cat", "x": 10.0, "y": 20.0, "crop": [0, 0, 32, 32] }
    ] });
    assert!(matches!(
        ask_in(&quiet_shared, &quiet, cat.clone()),
        HostReturn::Denied { .. }
    ));
    assert_eq!(
        ask(&plugin, cat),
        HostReturn::Ok(serde_json::Value::Bool(true))
    );
    let shown = notices
        .try_iter()
        .find_map(|notice| match notice {
            Notice::Overlay { plugin: by, layer } if by == plugin => layer,
            _ => None,
        })
        .expect("the layer");
    assert_eq!(shown.fps, 12);
    assert_eq!(shown.sprites[0].crop, Some([0, 0, 32, 32]));
    for wrong in [
        serde_json::json!({ "sprites": [{ "id": "a", "image": "dog", "x": 0.0, "y": 0.0 }] }),
        serde_json::json!({ "fps": 61 }),
    ] {
        assert!(
            matches!(ask(&plugin, wrong.clone()), HostReturn::Error { .. }),
            "{wrong}"
        );
    }
}
