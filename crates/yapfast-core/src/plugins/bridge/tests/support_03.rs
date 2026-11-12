//! The harness the tests in this directory share.

use super::*;

pub(super) fn answer_decorated(
    session: &mut Session,
    shared: &Arc<Shared>,
    ask: u64,
    plugin: &PluginId,
    tree: Option<serde_json::Value>,
    hide: bool,
    hide_media: bool,
) {
    assert!(
        session
            .on_frame(
                shared,
                Frame::new(
                    FrameKind::Report,
                    ask,
                    serde_json::to_vec(&FromHelper::Decorated {
                        plugin: plugin.clone(),
                        message: "message-1".to_owned(),
                        tree,
                        hide,
                        hide_media,
                    })
                    .expect("it serializes"),
                ),
            )
            .is_none()
    );
}

/// A manifest that declares `capabilities` and nothing else.
pub(super) fn action_manifest(capabilities: &[&str]) -> String {
    let declared: Vec<String> = capabilities
        .iter()
        .map(|capability| format!("\"{capability}\""))
        .collect();
    format!(
        r#"{{"api":1,"id":"com.example.actions","name":"Actions","version":"1.0.0",
                "wasm":"plugin.wasm",
                "permissions":{{"capabilities":[{}]}}}}"#,
        declared.join(",")
    )
}

/// One curated action of PLAN §17.2 or an audio call: the method, its
/// payload, the capability it needs, and what the application is asked to
/// do.
#[allow(clippy::type_complexity)]
pub(super) fn curated_actions()
-> Vec<(&'static str, serde_json::Value, Option<Capability>, AppCall)> {
    let picture = call::base64::encode(&[0x89, b'P', b'N', b'G']);
    vec![
        (
            call::method::DISCORD_SET_STATUS,
            serde_json::json!({ "status": "dnd" }),
            Some(Capability::PresenceWrite),
            AppCall::SetStatus {
                status: "dnd".into(),
            },
        ),
        (
            call::method::DISCORD_SET_CUSTOM_STATUS,
            serde_json::json!({ "text": "away", "clear_after": 600 }),
            Some(Capability::PresenceWrite),
            AppCall::SetCustomStatus {
                emoji: None,
                text: "away".into(),
                clear_after: Some(600),
            },
        ),
        (
            call::method::DISCORD_SET_ACTIVITY,
            serde_json::json!({ "enabled": true }),
            Some(Capability::PresenceWrite),
            AppCall::SetActivity { enabled: true },
        ),
        (
            call::method::DISCORD_CLEAR_ACTIVITY,
            serde_json::Value::Null,
            Some(Capability::PresenceWrite),
            AppCall::SetActivity { enabled: false },
        ),
        (
            call::method::DISCORD_MARK_READ,
            serde_json::json!({ "room": "room-1" }),
            Some(Capability::NotificationsManage),
            AppCall::MarkRead {
                room: "room-1".into(),
            },
        ),
        (
            call::method::DISCORD_SET_MUTE,
            serde_json::json!({ "room": "room-1", "muting": { "minutes": 30 } }),
            Some(Capability::NotificationsManage),
            AppCall::SetMute {
                room: "room-1".into(),
                muting: crate::chat::Muting::Minutes(30),
            },
        ),
        (
            call::method::DISCORD_OPEN_ROOM,
            serde_json::json!({ "room": "room-1" }),
            None,
            AppCall::OpenRoom {
                room: "room-1".into(),
            },
        ),
        (
            call::method::DISCORD_START_REPLY,
            serde_json::json!({ "room": "room-1", "message": "message-1" }),
            None,
            AppCall::StartReply {
                room: "room-1".into(),
                message: "message-1".into(),
            },
        ),
        (
            call::method::DISCORD_START_EDIT,
            serde_json::json!({ "room": "room-1", "message": "message-1" }),
            None,
            AppCall::StartEdit {
                room: "room-1".into(),
                message: "message-1".into(),
            },
        ),
        (
            call::method::DISCORD_REACTORS,
            serde_json::json!({ "room": "room-1", "message": "message-1" }),
            Some(Capability::MessagesRead),
            AppCall::Reactors {
                room: "room-1".into(),
                message: "message-1".into(),
            },
        ),
        (
            call::method::DISCORD_PAUSE_INVITES,
            serde_json::json!({ "server": "discord:1", "invites": true, "hours": 1 }),
            Some(Capability::ServersManage),
            AppCall::PauseInvites {
                server: "discord:1".into(),
                invites: true,
                dms: false,
                hours: 1,
            },
        ),
        (
            call::method::DISCORD_ADD_EMOJI,
            serde_json::json!({ "server": "discord:1", "name": "bug", "image_base64": picture }),
            Some(Capability::ServersManage),
            AppCall::AddEmoji {
                server: "discord:1".into(),
                name: "bug".into(),
                image: vec![0x89, b'P', b'N', b'G'],
            },
        ),
        (
            call::method::DISCORD_ADD_STICKER,
            serde_json::json!({ "server": "discord:1", "name": "bug", "image_base64": picture }),
            Some(Capability::ServersManage),
            AppCall::AddSticker {
                server: "discord:1".into(),
                name: "bug".into(),
                tags: String::new(),
                description: String::new(),
                image: vec![0x89, b'P', b'N', b'G'],
            },
        ),
        (
            call::method::AUDIO_PLAY,
            serde_json::json!({ "sound_base64": call::base64::encode(b"a sound") }),
            Some(Capability::AudioPlay),
            AppCall::PlaySound {
                sound: b"a sound".to_vec(),
                volume: None,
            },
        ),
        (
            call::method::AUDIO_SPEAK,
            serde_json::json!({ "text": "the build failed" }),
            Some(Capability::AudioSpeak),
            AppCall::Speak {
                text: "the build failed".into(),
            },
        ),
    ]
}

/// Asks one action of a session around a plugin declaring `capabilities`,
/// with a gesture behind it, and answers what came back.
#[allow(clippy::type_complexity)]
pub(super) fn ask_action(
    method: &str,
    payload: serde_json::Value,
    capabilities: &[&str],
) -> (HostReturn, std::sync::mpsc::Receiver<Notice>) {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(
        directory.path(),
        &action_manifest(capabilities),
        b"not a real module",
    );
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    // A person clicked a moment ago, so only the capability can refuse
    // what the call asks for.
    shared.gesture();
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let call = HostCall {
        method: method.to_owned(),
        payload,
    };
    runtime().block_on(async { session.on_call(&shared, 9, plugin, call) });
    let answer = match written.recv_timeout(Duration::from_millis(50)) {
        Ok(frame) => returned(&frame).1,
        // Nothing was written: the application is answering it.
        Err(_) => HostReturn::Pending {
            tag: "the application".into(),
        },
    };
    (answer, notices)
}
