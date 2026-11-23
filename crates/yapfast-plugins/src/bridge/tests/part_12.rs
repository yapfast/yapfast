//! The abilities added after the audit: `voice.state`, `files.save`, a
//! package's assets, and the render hook's line and marks. Part 12.

use super::*;

/// A plugin carrying a sound and a picture, declaring `capabilities`, written
/// into the store the way an install writes it, assets beside the module.
fn installed_with_assets(directory: &Path, capabilities: &[&str]) -> PluginId {
    let declared: Vec<String> = capabilities
        .iter()
        .map(|capability| format!("\"{capability}\""))
        .collect();
    let manifest = format!(
        r#"{{"api":1,"id":"com.example.showcase","name":"Showcase","version":"1.0.0",
            "wasm":"plugin.wasm","assets":{{"beep":"beep.wav","logo":"logo.png"}},
            "activation":{{"on":["decorate"]}},
            "permissions":{{"capabilities":[{}]}}}}"#,
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

const BEEP: &[u8] = b"RIFF\0\0\0\0WAVEfmt a sound";
const LOGO: &[u8] = b"\x89PNG\r\n\x1a\na picture";

/// Asks one call of a session around `plugin`, with or without a person's
/// click behind it, and answers what came back at once, or `Pending` when the
/// application was handed it.
fn ask(
    directory: &Path,
    plugin: &PluginId,
    method: &str,
    payload: serde_json::Value,
    gesture: bool,
) -> (HostReturn, std::sync::mpsc::Receiver<Notice>) {
    let (wake, notices) = notice_channel();
    let shared = shared(directory, wake);
    if gesture {
        shared.gesture(plugin);
    }
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let call = HostCall {
        method: method.to_owned(),
        payload,
    };
    runtime().block_on(async { session.on_call(&shared, 9, plugin.clone(), call) });
    let answer = match written.recv_timeout(Duration::from_millis(50)) {
        Ok(frame) => returned(&frame).1,
        Err(_) => HostReturn::Pending {
            tag: "the application".into(),
        },
    };
    (answer, notices)
}

fn denied_for(answer: &HostReturn, capability: &str) {
    let HostReturn::Denied { reason } = answer else {
        panic!("should need `{capability}`: {answer:?}");
    };
    assert!(reason.contains(capability), "{reason}");
}

#[test]
fn each_new_ability_needs_the_capability_the_review_showed() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed_with_assets(directory.path(), &[]);
    let (answer, _) = ask(
        directory.path(),
        &plugin,
        call::method::VOICE_STATE,
        serde_json::Value::Null,
        true,
    );
    denied_for(&answer, "voice.read");
    let (answer, notices) = ask(
        directory.path(),
        &plugin,
        call::method::FILES_SAVE,
        serde_json::json!({ "name": "notes.txt", "text": "hi" }),
        true,
    );
    denied_for(&answer, "files.save");
    assert!(
        notices.recv_timeout(Duration::from_millis(50)).is_err(),
        "no dialog opens without `files.save`"
    );
    let (answer, _) = ask(
        directory.path(),
        &plugin,
        call::method::AUDIO_PLAY,
        serde_json::json!({ "asset": "beep" }),
        true,
    );
    denied_for(&answer, "audio.play");
}

#[test]
fn voice_state_answers_from_the_world_with_voice_read() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed_with_assets(directory.path(), &["voice.read"]);
    let (answer, _) = ask(
        directory.path(),
        &plugin,
        call::method::VOICE_STATE,
        serde_json::Value::Null,
        false,
    );
    assert_eq!(
        answer,
        HostReturn::Ok(serde_json::Value::Null),
        "outside a call there is no call, and no gesture is needed to read it"
    );
}

#[test]
fn a_save_needs_a_click_a_name_and_bytes_that_fit_one_frame() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed_with_assets(directory.path(), &["files.save"]);
    let save = |payload: serde_json::Value, gesture: bool| {
        ask(
            directory.path(),
            &plugin,
            call::method::FILES_SAVE,
            payload,
            gesture,
        )
    };

    let (answer, notices) = save(
        serde_json::json!({ "name": "notes.txt", "text": "hi" }),
        false,
    );
    let HostReturn::Denied { reason } = answer else {
        panic!("a save with nobody there is refused: {answer:?}");
    };
    assert!(reason.contains("save dialog"), "{reason}");
    assert!(notices.recv_timeout(Duration::from_millis(50)).is_err());

    for name in ["../notes.txt", "a/b.txt", "C:\\notes.txt", ".bashrc", ""] {
        let (answer, _) = save(serde_json::json!({ "name": name, "text": "hi" }), true);
        assert!(
            matches!(answer, HostReturn::Error { .. }),
            "`{name}` is no file name: {answer:?}"
        );
    }
    let (answer, _) = save(
        serde_json::json!({ "name": "notes.txt", "text": "hi", "bytes_base64": "aGk=" }),
        true,
    );
    assert!(
        matches!(answer, HostReturn::Error { .. }),
        "one or the other"
    );
    let big = call::base64::encode(&vec![0u8; limits::MAX_SAVED_FILE_BYTES + 1]);
    let (answer, _) = save(
        serde_json::json!({ "name": "big.bin", "bytes_base64": big }),
        true,
    );
    let HostReturn::Error { message } = answer else {
        panic!("a file past the cap is refused: {answer:?}");
    };
    assert!(
        message.contains(&limits::MAX_SAVED_FILE_BYTES.to_string()),
        "{message}"
    );

    let (answer, notices) = save(
        serde_json::json!({ "name": "notes.txt", "text": "hi" }),
        true,
    );
    assert!(matches!(answer, HostReturn::Pending { .. }), "{answer:?}");
    let Notice::Call { request, .. } = notices
        .recv_timeout(Duration::from_secs(2))
        .expect("the application is asked")
    else {
        panic!("a save is a call the application carries out");
    };
    assert_eq!(
        request,
        AppCall::SaveFile {
            name: "notes.txt".into(),
            bytes: b"hi".to_vec(),
        }
    );
}

#[test]
fn a_package_s_assets_are_listed_read_played_and_made_emoji_by_name() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed_with_assets(directory.path(), &["audio.play", "servers.manage"]);

    let (answer, _) = ask(
        directory.path(),
        &plugin,
        call::method::ASSETS_LIST,
        serde_json::Value::Null,
        false,
    );
    let HostReturn::Ok(listed) = answer else {
        panic!("the plugin's own assets need no capability: {answer:?}");
    };
    assert_eq!(listed[0]["name"], "beep");
    assert_eq!(listed[0]["kind"], "wav");
    assert_eq!(listed[1]["kind"], "png");

    let (answer, _) = ask(
        directory.path(),
        &plugin,
        call::method::ASSETS_GET,
        serde_json::json!({ "name": "logo" }),
        false,
    );
    let HostReturn::Ok(got) = answer else {
        panic!("{answer:?}");
    };
    assert_eq!(
        call::base64::decode(got["bytes_base64"].as_str().expect("bytes")).expect("base64"),
        LOGO
    );
    let (answer, _) = ask(
        directory.path(),
        &plugin,
        call::method::ASSETS_GET,
        serde_json::json!({ "name": "../lock.json" }),
        false,
    );
    assert!(
        matches!(answer, HostReturn::Error { .. }),
        "a name, never a path"
    );

    let (answer, notices) = ask(
        directory.path(),
        &plugin,
        call::method::AUDIO_PLAY,
        serde_json::json!({ "asset": "beep", "volume": 50 }),
        false,
    );
    assert!(matches!(answer, HostReturn::Pending { .. }), "{answer:?}");
    let Notice::Call { request, .. } = notices
        .recv_timeout(Duration::from_secs(2))
        .expect("the application plays it")
    else {
        panic!("playing is the application's");
    };
    assert_eq!(
        request,
        AppCall::PlaySound {
            sound: BEEP.to_vec(),
            volume: Some(50),
        }
    );

    let (answer, _) = ask(
        directory.path(),
        &plugin,
        call::method::AUDIO_PLAY,
        serde_json::json!({ "asset": "logo" }),
        false,
    );
    assert!(
        matches!(&answer, HostReturn::Error { message } if message.contains("image/png")),
        "a picture isn't a sound: {answer:?}"
    );
    let (answer, _) = ask(
        directory.path(),
        &plugin,
        call::method::AUDIO_PLAY,
        serde_json::json!({ "asset": "beep", "sound_base64": "aGk=" }),
        false,
    );
    assert!(
        matches!(answer, HostReturn::Error { .. }),
        "one or the other"
    );

    let (answer, notices) = ask(
        directory.path(),
        &plugin,
        call::method::DISCORD_ADD_EMOJI,
        serde_json::json!({ "server": "discord:1", "name": "logo", "asset": "logo" }),
        false,
    );
    assert!(matches!(answer, HostReturn::Pending { .. }), "{answer:?}");
    let Notice::Call { request, .. } = notices
        .recv_timeout(Duration::from_secs(2))
        .expect("the application adds it")
    else {
        panic!("adding an emoji is the application's");
    };
    assert_eq!(
        request,
        AppCall::AddEmoji {
            server: "discord:1".into(),
            name: "logo".into(),
            image: LOGO.to_vec(),
        },
        "a plugin's picture becomes an emoji a person can react with"
    );
}

/// Answers one decoration with a line and marks, and says what reached the
/// interface.
fn decorated_with(capabilities: &[&str], annotation: serde_json::Value) -> Option<slots::Output> {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed_with_assets(directory.path(), capabilities);
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let row = yapfast_plugin_api::message::DecorateRequest {
        room: "room-1".to_owned(),
        message: "message-1".to_owned(),
        author: "person-1".to_owned(),
        body: "hi".to_owned(),
        own: false,
        pinned: false,
        stickers: Vec::new(),
    };
    let version = decorate_cache::version_key("1.0.0", None);
    assert_eq!(
        shared.decorated().look(&plugin, version, &row),
        decorate_cache::Look::Ask
    );
    let ask = decorate_ask(&mut session, &shared, &written, &plugin);
    session.on_frame(
        &shared,
        Frame::new(
            FrameKind::Report,
            ask,
            serde_json::to_vec(&FromHelper::Decorated {
                plugin: plugin.clone(),
                message: "message-1".to_owned(),
                tree: None,
                hide: false,
                hide_media: false,
                blur_media: false,
                annotation: Some(annotation),
                marks: Some(serde_json::json!([{ "text": "hi", "style": "bold" }])),
                author: None,
                pictures: Some(serde_json::json!([{ "src": "emoji:12", "text": "hi" }])),
                tooltips: Some(serde_json::json!([{ "mention": "7", "text": "Ada" }])),
                emoji: Some(serde_json::json!([{ "emoji": ":kekw:", "with": "lol" }])),
            })
            .expect("it serializes"),
        ),
    );
    let drawn = match notices.recv_timeout(Duration::from_millis(200)) {
        Ok(Notice::Slot(output)) => Some(output),
        Ok(other) => panic!("a decoration is a slot notice: {other:?}"),
        Err(_) => None,
    };
    // The answer is kept, so the row coming back asks nobody.
    let kept = shared.decorated().look(&plugin, version, &row);
    assert_eq!(
        kept,
        decorate_cache::Look::Answered(drawn.clone().map(Box::new))
    );
    drawn
}

#[test]
fn a_line_and_marks_need_messages_read_and_are_kept_for_the_row() {
    let line = serde_json::json!({ "text": "hello, translated" });
    assert_eq!(
        decorated_with(&[], line.clone()),
        None,
        "without `messages.read` there is nothing to draw"
    );
    let Some(slots::Output::Decorate { plugin, inline, .. }) =
        decorated_with(&["messages.read"], line)
    else {
        panic!("with it, the line and the marks are drawn");
    };
    assert_eq!(plugin, "com.example.showcase", "named by its plugin");
    assert_eq!(
        inline.annotation.map(|line| line.text).as_deref(),
        Some("hello, translated")
    );
    assert_eq!(inline.marks.len(), 1);
    assert_eq!(inline.pictures.len(), 1, "a picture beside the words");
    assert_eq!(inline.tooltips[0].text, "Ada");
    assert_eq!(inline.emoji[0].with.as_deref(), Some("lol"));
}

#[test]
fn a_line_that_doesn_t_check_out_is_dropped_and_the_marks_stand() {
    let Some(slots::Output::Decorate { inline, .. }) = decorated_with(
        &["messages.read"],
        serde_json::json!({ "text": "official notice", "tone": "danger" }),
    ) else {
        panic!("the marks still draw");
    };
    assert!(inline.annotation.is_none(), "Yapfast's own tone is refused");
    assert_eq!(inline.marks.len(), 1);
}
