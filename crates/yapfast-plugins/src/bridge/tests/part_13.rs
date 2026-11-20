//! The powerful tier: every powerful call refused without its capability,
//! with the switch off, and before the person's first answer; a folder that
//! keeps a plugin inside it; an endpoint on loopback only, behind its token;
//! and an agent's sends at the rate its manifest names. Part 13.

use super::*;

/// A plugin declaring `capabilities`, with `extra` added to its permissions.
fn powerful_plugin(directory: &Path, capabilities: &[&str], extra: &str) -> PluginId {
    let declared: Vec<String> = capabilities
        .iter()
        .map(|capability| format!("\"{capability}\""))
        .collect();
    let manifest = format!(
        r#"{{"api":1,"id":"com.example.powerful","name":"Powerful","version":"1.0.0",
            "wasm":"plugin.wasm",
            "permissions":{{"capabilities":[{}]{extra}}}}}"#,
        declared.join(",")
    );
    installed(directory, &manifest, b"not a real module")
}

/// One call through a session, and its answer: at once, or `Pending` when the
/// application was handed it.
fn call_on(
    shared: &Arc<Shared>,
    plugin: &PluginId,
    method: &str,
    payload: serde_json::Value,
) -> (HostReturn, std::sync::mpsc::Receiver<Frame>) {
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let call = HostCall {
        method: method.to_owned(),
        payload,
    };
    runtime().block_on(async { session.on_call(shared, 9, plugin.clone(), call) });
    let answer = match written.recv_timeout(Duration::from_millis(200)) {
        Ok(frame) => returned(&frame).1,
        Err(_) => HostReturn::Pending {
            tag: "the application".into(),
        },
    };
    (answer, written)
}

fn denied(answer: &HostReturn) -> String {
    match answer {
        HostReturn::Denied { reason } => reason.clone(),
        other => panic!("refused, not {other:?}"),
    }
}

#[test]
fn a_powerful_call_needs_its_capability_the_switch_and_a_first_yes() {
    for (capability, method, payload, extra) in [
        (
            "fs.folder",
            call::method::FS_LIST,
            serde_json::json!({ "path": "" }),
            "",
        ),
        (
            "fs.full",
            call::method::FS_READ,
            serde_json::json!({ "path": ".bashrc" }),
            "",
        ),
        (
            "net.listen",
            call::method::NET_LISTEN,
            serde_json::Value::Null,
            "",
        ),
        (
            "fs.folder",
            call::method::FS_THUMBNAIL,
            serde_json::json!({ "path": "a.png" }),
            "",
        ),
    ] {
        // Without the capability: refused, naming the one it needs.
        let directory = tempfile::tempdir().expect("a directory");
        let plugin = powerful_plugin(directory.path(), &[], extra);
        let (wake, notices) = notice_channel();
        let without = shared(directory.path(), wake);
        let (answer, _) = call_on(&without, &plugin, method, payload.clone());
        let reason = denied(&answer);
        assert!(
            reason.contains("fs.folder") || reason.contains(capability),
            "{method}: {reason}"
        );

        // Declared, with the switch off: refused, saying where the switch is.
        let directory = tempfile::tempdir().expect("a directory");
        let plugin = powerful_plugin(directory.path(), &[capability], extra);
        let (wake, notices_on) = notice_channel();
        let shared = shared(directory.path(), wake);
        let (answer, _) = call_on(&shared, &plugin, method, payload.clone());
        assert!(denied(&answer).contains("Advanced"), "{method}");
        assert!(
            notices_on.recv_timeout(Duration::from_millis(50)).is_err(),
            "{method}: nothing is asked while the switch is off"
        );

        // On, and not answered yet: refused, and the person asked once.
        shared.powerful.set_enabled(true).expect("saved");
        let (answer, _) = call_on(&shared, &plugin, method, payload.clone());
        assert!(denied(&answer).contains("first time"), "{method}");
        let asked = notices_on
            .recv_timeout(Duration::from_secs(1))
            .expect("the person is asked");
        assert!(
            matches!(&asked, Notice::Powerful { capability: asked_for, .. } if asked_for.as_str() == capability),
            "{method}: {asked:?}"
        );
        let (_, _) = call_on(&shared, &plugin, method, payload);
        assert!(
            notices_on.recv_timeout(Duration::from_millis(100)).is_err(),
            "{method}: a second call while the question is up asks nothing more"
        );
        drop(notices);
    }
}

#[test]
fn attaching_a_file_needs_messages_attach_and_a_folder() {
    let directory = tempfile::tempdir().expect("a directory");
    let plugin = powerful_plugin(directory.path(), &["messages.send"], "");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    shared.gesture(&plugin);
    let (answer, _) = call_on(
        &shared,
        &plugin,
        call::method::MESSAGES_SEND,
        serde_json::json!({ "room": "room-1", "body": "hi", "files": ["a.png"] }),
    );
    assert!(denied(&answer).contains("messages.attach"));
}

#[test]
fn a_folder_keeps_the_plugin_inside_it_through_the_bridge() {
    let directory = tempfile::tempdir().expect("a directory");
    let plugin = powerful_plugin(
        directory.path(),
        &["fs.folder", "messages.send", "messages.attach"],
        r#","files":{"write":false}"#,
    );
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let folder = directory.path().join("pictures");
    std::fs::create_dir_all(&folder).expect("a folder");
    std::fs::write(folder.join("cat.png"), b"a picture").expect("a file");
    std::fs::write(directory.path().join("lock.txt"), b"outside").expect("a file");
    shared.powerful.set_enabled(true).expect("saved");
    shared
        .powerful
        .allow(&plugin, Capability::FsFolder, Some(folder.clone()))
        .expect("saved");

    let (answer, _) = call_on(
        &shared,
        &plugin,
        call::method::FS_READ,
        serde_json::json!({ "path": "cat.png" }),
    );
    assert!(matches!(answer, HostReturn::Ok(_)), "{answer:?}");
    for escape in ["../lock.txt", "/etc/passwd", "sub/../../lock.txt"] {
        let (answer, _) = call_on(
            &shared,
            &plugin,
            call::method::FS_READ,
            serde_json::json!({ "path": escape }),
        );
        assert!(
            matches!(answer, HostReturn::Error { .. }),
            "`{escape}`: {answer:?}"
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(directory.path().join("lock.txt"), folder.join("out.txt"))
            .expect("a link");
        let (answer, _) = call_on(
            &shared,
            &plugin,
            call::method::FS_READ,
            serde_json::json!({ "path": "out.txt" }),
        );
        assert!(
            matches!(answer, HostReturn::Error { .. }),
            "a link out: {answer:?}"
        );
    }
    let (answer, _) = call_on(
        &shared,
        &plugin,
        call::method::FS_WRITE,
        serde_json::json!({ "path": "new.txt", "text": "x" }),
    );
    assert!(
        matches!(&answer, HostReturn::Error { message } if message.contains("not write")),
        "a folder given to read is not written: {answer:?}"
    );

    // A file sent with a message is one of the folder's, resolved inside it.
    shared.gesture(&plugin);
    let (answer, _) = call_on(
        &shared,
        &plugin,
        call::method::MESSAGES_SEND,
        serde_json::json!({ "room": "room-1", "body": "look", "files": ["../lock.txt"] }),
    );
    assert!(matches!(answer, HostReturn::Error { .. }), "{answer:?}");
    shared.gesture(&plugin);
    let (answer, _) = call_on(
        &shared,
        &plugin,
        call::method::MESSAGES_SEND,
        serde_json::json!({ "room": "room-1", "body": "look", "files": ["cat.png"] }),
    );
    assert!(matches!(answer, HostReturn::Pending { .. }), "{answer:?}");
    let sent = std::iter::from_fn(|| notices.recv_timeout(Duration::from_millis(200)).ok())
        .find_map(|notice| match notice {
            Notice::Call {
                request: AppCall::Send { files, .. },
                ..
            } => Some(files),
            _ => None,
        })
        .expect("the application sends it");
    assert_eq!(
        sent,
        [std::fs::canonicalize(folder.join("cat.png")).expect("a path")]
    );
}

#[test]
fn an_endpoint_listens_on_loopback_behind_its_token_and_hands_requests_to_the_plugin() {
    use std::io::{Read, Write};
    let directory = tempfile::tempdir().expect("a directory");
    let plugin = powerful_plugin(directory.path(), &["net.listen"], "");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    shared.powerful.set_enabled(true).expect("saved");
    shared
        .powerful
        .allow(&plugin, Capability::NetListen, None)
        .expect("saved");
    let tasks = runtime();
    let (_session_directory, mut session, written, _inbound) = session_stub();
    tasks.block_on(async {
        session.on_call(
            &shared,
            21,
            plugin.clone(),
            HostCall {
                method: call::method::NET_LISTEN.to_owned(),
                payload: serde_json::Value::Null,
            },
        );
    });
    let (call, started) = returned(&next_frame(&written));
    assert_eq!(call, 21);
    let HostReturn::Ok(started) = started else {
        panic!("it listens: {started:?}");
    };
    let port = started["port"].as_u64().expect("a port") as u16;
    {
        let endpoints = shared.endpoints.lock().expect("the endpoints");
        let endpoint = endpoints.get(&plugin).expect("serving");
        assert!(
            endpoint
                .addresses
                .iter()
                .all(|address| address.ip().is_loopback()),
            "loopback only: {:?}",
            endpoint.addresses
        );
    }
    let token = shared.powerful.token(&plugin).expect("its token");
    let ask = move |headers: String| {
        std::thread::spawn(move || {
            let mut stream =
                std::net::TcpStream::connect(("127.0.0.1", port)).expect("a connection");
            stream
                .write_all(
                    format!("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\n{headers}Content-Length: 2\r\n\r\n{{}}")
                        .as_bytes(),
                )
                .expect("sent");
            let mut answer = String::new();
            stream.read_to_string(&mut answer).expect("an answer");
            answer
        })
    };
    let refused = ask(String::new()).join().expect("a client");
    assert!(refused.starts_with("HTTP/1.1 401"), "{refused}");
    assert!(
        written.recv_timeout(Duration::from_millis(100)).is_err(),
        "the plugin never hears of a request without the token"
    );
    let client = ask(format!("Authorization: Bearer {token}\r\n"));
    let (call, incoming) = returned(&next_frame(&written));
    assert_eq!(call, 21, "a request is another answer to `net.listen`");
    let HostReturn::Ok(incoming) = incoming else {
        panic!("{incoming:?}");
    };
    assert_eq!(incoming["path"], "/mcp");
    assert!(
        !incoming["headers"].to_string().contains(&token),
        "the token isn't handed to the plugin"
    );
    let request = incoming["request"].as_u64().expect("its number");
    let (answer, _) = call_on(
        &shared,
        &plugin,
        call::method::NET_RESPOND,
        serde_json::json!({ "request": request, "text": "{\"ok\":true}" }),
    );
    assert_eq!(answer, HostReturn::Ok(serde_json::Value::Bool(true)));
    let answered = client.join().expect("a client");
    assert!(answered.starts_with("HTTP/1.1 200"), "{answered}");
    assert!(answered.ends_with("{\"ok\":true}"), "{answered}");
    drop(tasks);
}

#[test]
fn an_agent_sends_without_a_keystroke_only_when_allowed_and_at_its_rate() {
    let directory = tempfile::tempdir().expect("a directory");
    let plugin = powerful_plugin(
        directory.path(),
        &["messages.send", "messages.send.agent"],
        r#","agent-per-minute":1"#,
    );
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let send = serde_json::json!({ "room": "room-1", "body": "from the agent" });
    let (answer, _) = call_on(&shared, &plugin, call::method::MESSAGES_SEND, send.clone());
    assert!(denied(&answer).contains("Advanced"), "the switch first");
    shared.powerful.set_enabled(true).expect("saved");
    shared
        .powerful
        .allow(&plugin, Capability::MessagesSendAgent, None)
        .expect("saved");
    let (answer, _) = call_on(&shared, &plugin, call::method::MESSAGES_SEND, send.clone());
    assert!(matches!(answer, HostReturn::Pending { .. }), "{answer:?}");
    let seen: Vec<Notice> =
        std::iter::from_fn(|| notices.recv_timeout(Duration::from_millis(200)).ok()).collect();
    assert!(
        seen.iter()
            .any(|notice| matches!(notice, Notice::AgentSent { .. })),
        "the window is told, for its indicator: {seen:?}"
    );
    assert!(
        seen.iter().any(|notice| matches!(
            notice,
            Notice::Call {
                request: AppCall::Send { agent: true, .. },
                ..
            }
        )),
        "and the send says it was an agent's"
    );
    let (answer, _) = call_on(&shared, &plugin, call::method::MESSAGES_SEND, send);
    assert!(denied(&answer).contains("1 times a minute"), "its rate");
}
