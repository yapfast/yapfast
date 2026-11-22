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
