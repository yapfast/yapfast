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

        // On, and not answered yet: the call waits on the person, who is
        // asked once.
        shared.powerful.set_enabled(true).expect("saved");
        let (answer, _) = call_on(&shared, &plugin, method, payload.clone());
        assert!(
            matches!(answer, HostReturn::Pending { .. }),
            "{method}: waits rather than refused: {answer:?}"
        );
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

#[test]
fn a_first_use_waits_for_the_answer_then_goes_on_or_is_refused() {
    let directory = tempfile::tempdir().expect("a directory");
    let plugin = powerful_plugin(directory.path(), &["net.listen"], "");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    shared.powerful.set_enabled(true).expect("saved");
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let listen = || HostCall {
        method: call::method::NET_LISTEN.to_owned(),
        payload: serde_json::Value::Null,
    };

    // A no refuses what waited, saying so, and the next use asks again.
    runtime().block_on(async { session.on_call(&shared, 3, plugin.clone(), listen()) });
    assert!(matches!(
        notices.recv_timeout(Duration::from_secs(1)),
        Ok(Notice::Powerful { .. })
    ));
    assert!(
        written.recv_timeout(Duration::from_millis(100)).is_err(),
        "nothing is answered while the person reads the question"
    );
    shared.powerful_asked.lock().unwrap().clear();
    runtime().block_on(async {
        session.powerful_answered(&shared, &plugin, Capability::NetListen, false)
    });
    let (call, answer) = returned(
        &written
            .recv_timeout(Duration::from_secs(1))
            .expect("an answer"),
    );
    assert_eq!(call, 3);
    assert!(denied(&answer).contains("said no"), "{answer:?}");

    // A yes runs what waited, through every gate again, and it goes on.
    runtime().block_on(async { session.on_call(&shared, 4, plugin.clone(), listen()) });
    assert!(written.recv_timeout(Duration::from_millis(100)).is_err());
    shared
        .powerful
        .allow(&plugin, Capability::NetListen, None)
        .expect("saved");
    runtime().block_on(async {
        session.powerful_answered(&shared, &plugin, Capability::NetListen, true)
    });
    let (call, answer) = returned(
        &written
            .recv_timeout(Duration::from_secs(2))
            .expect("an answer"),
    );
    assert_eq!(call, 4);
    match answer {
        HostReturn::Ok(value) => assert_eq!(value["listening"], true, "{value}"),
        other => panic!("listening, not {other:?}"),
    }
    shared.stop_serving(&plugin);
}

#[test]
fn a_socket_opens_carries_bytes_and_closes_behind_the_powerful_gates() {
    let directory = tempfile::tempdir().expect("a directory");
    let plugin = powerful_plugin(directory.path(), &["net.socket"], r#","socket-local":true"#);
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    shared.powerful.set_enabled(true).expect("saved");
    shared
        .powerful
        .allow(&plugin, Capability::NetSocket, None)
        .expect("saved");
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let rt = runtime();
    let port = rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut peer, _) = listener.accept().await.unwrap();
            let mut got = [0u8; 2];
            peer.read_exact(&mut got).await.unwrap();
            peer.write_all(&got).await.unwrap();
            let mut rest = Vec::new();
            let _ = peer.read_to_end(&mut rest).await;
        });
        port
    });
    let connect = HostCall {
        method: call::method::NET_CONNECT.to_owned(),
        payload: serde_json::json!({ "host": "127.0.0.1", "port": port }),
    };
    rt.block_on(async { session.on_call(&shared, 5, plugin.clone(), connect) });
    let news = |written: &std::sync::mpsc::Receiver<Frame>| -> serde_json::Value {
        match returned(&written.recv_timeout(Duration::from_secs(3)).expect("news")).1 {
            HostReturn::Ok(value) => value,
            other => panic!("news, not {other:?}"),
        }
    };
    let connected = news(&written);
    assert_eq!(connected["news"], "connected", "{connected}");
    let socket = connected["socket"].as_u64().expect("a socket number");
    let sent = rt.block_on(async {
        session.answer(
            &shared,
            &plugin,
            HostCall {
                method: call::method::NET_SEND.to_owned(),
                payload: serde_json::json!({ "socket": socket, "text": "hi" }),
            },
        )
    });
    assert!(matches!(sent, HostReturn::Ok(_)), "{sent:?}");
    let echoed = news(&written);
    assert_eq!(echoed["news"], "data", "{echoed}");
    assert_eq!(echoed["data_base64"], "aGk=");
    let closed = rt.block_on(async {
        session.answer(
            &shared,
            &plugin,
            HostCall {
                method: call::method::NET_CLOSE.to_owned(),
                payload: serde_json::json!({ "socket": socket }),
            },
        )
    });
    assert!(matches!(closed, HostReturn::Ok(_)));
    let last = news(&written);
    assert_eq!(last["news"], "closed", "{last}");
    // A connection that closed is no longer one to send on.
    for _ in 0..50 {
        if shared
            .sockets
            .lock()
            .unwrap()
            .get(&plugin)
            .is_none_or(BTreeMap::is_empty)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let after = rt.block_on(async {
        session.answer(
            &shared,
            &plugin,
            HostCall {
                method: call::method::NET_SEND.to_owned(),
                payload: serde_json::json!({ "socket": socket, "text": "again" }),
            },
        )
    });
    assert!(matches!(after, HostReturn::Error { .. }), "{after:?}");
}

#[test]
fn a_socket_without_its_capability_or_to_discord_is_refused() {
    let directory = tempfile::tempdir().expect("a directory");
    let plugin = powerful_plugin(directory.path(), &[], "");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (answer, _) = call_on(
        &shared,
        &plugin,
        call::method::NET_CONNECT,
        serde_json::json!({ "host": "example.com", "port": 443 }),
    );
    assert!(denied(&answer).contains("net.socket"), "{answer:?}");

    let directory = tempfile::tempdir().expect("a directory");
    let plugin = powerful_plugin(directory.path(), &["net.socket"], "");
    let (wake, _notices) = notice_channel();
    let shared = super::support_01::shared(directory.path(), wake);
    shared.powerful.set_enabled(true).expect("saved");
    shared
        .powerful
        .allow(&plugin, Capability::NetSocket, None)
        .expect("saved");
    let (answer, _) = call_on(
        &shared,
        &plugin,
        call::method::NET_CONNECT,
        serde_json::json!({ "host": "gateway.discord.gg", "port": 443, "tls": true }),
    );
    match answer {
        HostReturn::Error { message } => assert!(message.contains("Discord"), "{message}"),
        other => panic!("refused, not {other:?}"),
    }
}
