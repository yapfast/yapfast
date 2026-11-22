use super::*;

use yapfast_plugin_api::call::{HostCall, HostReturn};

const VOICE_MANIFEST: &str = r#"{"api":1,"id":"com.example.voice","name":"Voice Messages","version":"1.0.0",
    "wasm":"plugin.wasm","permissions":{"capabilities":["microphone","messages.send"]}}"#;

fn ask(
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
    match written.recv_timeout(Duration::from_millis(100)) {
        Ok(frame) => returned(&frame).1,
        Err(_) => HostReturn::Pending {
            tag: "waiting".into(),
        },
    }
}

/// The microphone is asked about the first time, in the dialog the powerful
/// tier's first use opens, and a recording starts only once the person said
/// yes; before that the call waits for the answer.
#[test]
fn a_recording_waits_for_the_persons_yes() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), VOICE_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let answer = ask(
        &shared,
        &plugin,
        call::method::MICROPHONE_START,
        serde_json::json!({ "format": "opus" }),
    );
    assert!(matches!(answer, HostReturn::Pending { .. }), "{answer:?}");
    let asked = notices.try_iter().any(|notice| {
        matches!(
            notice,
            Notice::Powerful {
                capability: Capability::Microphone,
                ..
            }
        )
    });
    assert!(asked, "the person is asked");
}

/// A recording of the plugin's own goes as a voice message under the send's
/// rails; one that isn't its own, or isn't Opus, doesn't.
#[test]
fn a_recording_is_sent_as_a_voice_message() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), VOICE_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let path = directory.path().join("voice.ogg");
    std::fs::write(&path, b"OggS").expect("a recording");
    let opus = shared.keep_recording(&plugin, path.clone(), 4, Some((1500, "AAh/".into())));
    let pcm = shared.keep_recording(&plugin, path.clone(), 4, None);
    shared.gesture(&plugin);

    let answer = ask(
        &shared,
        &plugin,
        call::method::MESSAGES_VOICE,
        serde_json::json!({ "room": "room-1", "recording": opus }),
    );
    assert!(matches!(answer, HostReturn::Pending { .. }), "{answer:?}");
    let handed = notices
        .try_iter()
        .find_map(|notice| match notice {
            Notice::Call { request, .. } => Some(request),
            _ => None,
        })
        .expect("handed to the application");
    assert_eq!(
        handed,
        AppCall::SendVoice {
            room: "room-1".into(),
            path,
            duration_ms: 1500,
            waveform: "AAh/".into(),
        }
    );
    for recording in [pcm, 999] {
        let answer = ask(
            &shared,
            &plugin,
            call::method::MESSAGES_VOICE,
            serde_json::json!({ "room": "room-1", "recording": recording }),
        );
        assert!(matches!(answer, HostReturn::Error { .. }), "{answer:?}");
    }
    // Read back in pieces, as an answer's body is.
    let answer = ask(
        &shared,
        &plugin,
        call::method::MICROPHONE_READ,
        serde_json::json!({ "body": opus }),
    );
    let HostReturn::Ok(piece) = answer else {
        panic!("a piece: {answer:?}");
    };
    assert_eq!(piece["eof"], true);
}

/// A message or history of a Discord conversation the window doesn't hold is
/// asked of Discord rather than refused.
#[test]
fn history_of_another_conversation_is_asked_of_discord() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(
        directory.path(),
        r#"{"api":1,"id":"com.example.logger","name":"Logger","version":"1.0.0",
            "wasm":"plugin.wasm","permissions":{"capabilities":["messages.read"]}}"#,
        b"not a real module",
    );
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let answer = ask(
        &shared,
        &plugin,
        call::method::MESSAGES_HISTORY,
        serde_json::json!({ "room": "discord:1:9", "before": "d:55", "limit": 20 }),
    );
    assert!(matches!(answer, HostReturn::Pending { .. }), "{answer:?}");
    let handed = notices
        .try_iter()
        .find_map(|notice| match notice {
            Notice::Call { request, .. } => Some(request),
            _ => None,
        })
        .expect("handed to the application");
    assert_eq!(
        handed,
        AppCall::Messages {
            room: "discord:1:9".into(),
            message: None,
            before: Some("d:55".into()),
            limit: Some(20),
        }
    );
}
