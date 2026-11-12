//! From `a_kept_picture_comes_back_to_the_plugin_that_may_read_it` onward: part 3 of 11.

use super::*;

#[test]
fn a_kept_picture_comes_back_to_the_plugin_that_may_read_it() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let ids = install_all(directory.path(), &[READER_MANIFEST, HOOK_MANIFEST]);
    let (plugin, readerless) = (ids[0].clone(), ids[1].clone());
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    *shared.pictures.lock().expect("the store lock") = Some(Arc::new(HeldPicture));
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let kept = ask(
        &mut session,
        &written,
        &shared,
        &plugin,
        HostCall::new(
            call::method::MEDIA_GET,
            MediaGet {
                url: "https://cdn.example.test/a.png".to_owned(),
            },
        )
        .expect("a call"),
    );
    let HostReturn::Ok(value) = kept else {
        panic!("a kept picture is an answer: {kept:?}");
    };
    assert_eq!(value["bytes"], 5);
    assert_eq!(value["bytes_base64"], "aGVsbG8=");

    // The same call without `media.read` is denied by the name of what it
    // needs, and the store is never asked.
    let denied = ask(
        &mut session,
        &written,
        &shared,
        &readerless,
        HostCall::new(
            call::method::MEDIA_GET,
            MediaGet {
                url: "https://cdn.example.test/a.png".to_owned(),
            },
        )
        .expect("a call"),
    );
    let HostReturn::Denied { reason } = denied else {
        panic!("a picture without its capability is denied: {denied:?}");
    };
    assert!(reason.contains("media.read"), "{reason}");
}

#[test]
fn a_picture_nobody_keeps_says_so_rather_than_being_fetched() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), READER_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    *shared.pictures.lock().expect("the store lock") = Some(Arc::new(HeldPicture));
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let missing = ask(
        &mut session,
        &written,
        &shared,
        &plugin,
        HostCall::new(
            call::method::MEDIA_GET,
            MediaGet {
                url: "https://cdn.example.test/b.png".to_owned(),
            },
        )
        .expect("a call"),
    );
    let HostReturn::Error { message } = missing else {
        panic!("a picture nobody keeps says why: {missing:?}");
    };
    assert!(message.contains("never asks the network"), "{message}");
}

#[test]
fn a_read_without_its_capability_is_denied_by_the_name_of_what_it_needs() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    // The hook plugin keeps storage and messages.read, and nothing about
    // servers or people.
    let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let rooms = ask(
        &mut session,
        &written,
        &shared,
        &plugin,
        HostCall::new(call::method::ROOMS_LIST, serde_json::json!({})).expect("a call"),
    );
    let HostReturn::Denied { reason } = rooms else {
        panic!("a read without its capability is denied: {rooms:?}");
    };
    assert!(reason.contains("servers.read"), "{reason}");

    let people = ask(
        &mut session,
        &written,
        &shared,
        &plugin,
        HostCall::new(
            call::method::PEOPLE_GET,
            PeopleGet {
                person: "person-2".to_owned(),
            },
        )
        .expect("a call"),
    );
    let HostReturn::Denied { reason } = people else {
        panic!("a read without its capability is denied: {people:?}");
    };
    assert!(reason.contains("people.read"), "{reason}");

    // `messages.read` is declared, so the same shape answers.
    let history = ask(
        &mut session,
        &written,
        &shared,
        &plugin,
        HostCall::new(
            call::method::MESSAGES_HISTORY,
            MessageHistory {
                room: "discord:1:2".to_owned(),
                before: None,
                limit: None,
            },
        )
        .expect("a call"),
    );
    let HostReturn::Error { message } = history else {
        panic!("a room nobody has open says why: {history:?}");
    };
    assert!(message.contains("rooms.selected"), "{message}");
}

#[test]
fn a_storage_call_is_answered_under_the_id_it_asked_with() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let request = HostCall::new(
        call::method::STORAGE_SET,
        StorageSet {
            key: "seen".to_owned(),
            value: serde_json::json!(3),
        },
    )
    .expect("a call");
    let frame = Frame::new(
        FrameKind::Request,
        0,
        serde_json::to_vec(&FromHelper::Call {
            plugin: plugin.clone(),
            call: 11,
            request,
        })
        .expect("it serializes"),
    );
    assert!(session.on_frame(&shared, frame).is_none());

    let reply = next_frame(&written);
    assert_eq!(reply.kind, FrameKind::Reply);
    assert_eq!(reply.id, 11);
    let ToHelper::Return {
        call: answered,
        answer,
        ..
    } = message(&reply)
    else {
        panic!("a host call is answered with a return");
    };
    assert_eq!(answered, 11);
    assert_eq!(answer, HostReturn::Ok(serde_json::json!(true)));
    assert_eq!(
        shared.store().storage_get(&plugin, "seen"),
        Some(serde_json::json!(3))
    );
}

#[test]
fn a_capability_the_manifest_does_not_declare_is_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    // A plugin with no `storage` line in its manifest.
    let manifest = r#"{"api":1,"id":"com.example.plain","name":"Plain","version":"1.0.0",
            "wasm":"plugin.wasm"}"#;
    let plugin = installed(directory.path(), manifest, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let request = HostCall::new(
        call::method::STORAGE_GET,
        StorageGet {
            key: "seen".to_owned(),
        },
    )
    .expect("a call");
    let frame = Frame::new(
        FrameKind::Request,
        0,
        serde_json::to_vec(&FromHelper::Call {
            plugin: plugin.clone(),
            call: 2,
            request,
        })
        .expect("it serializes"),
    );
    assert!(session.on_frame(&shared, frame).is_none());
    let reply = next_frame(&written);
    let ToHelper::Return { answer, .. } = message(&reply) else {
        panic!("a host call is answered with a return");
    };
    assert!(matches!(answer, HostReturn::Denied { .. }), "{answer:?}");
}

#[test]
fn typed_events_arrive_as_one_batch_after_their_window() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let runtime = runtime();
    for body in ["one", "two"] {
        let event = SharedEvent::new(event(EventKind::Typing, body)).expect("it serializes");
        runtime.block_on(session.on_command(
            &shared,
            Request::Event {
                plugin: plugin.clone(),
                event,
            },
        ));
    }
    // Nothing is sent while the window is open: a burst is one call.
    assert!(
        written.recv_timeout(Duration::from_millis(50)).is_err(),
        "typing is held for its window"
    );

    std::thread::sleep(Duration::from_millis(COALESCE_MS + 50));
    session.on_tick(&shared);
    let activate = next_frame(&written);
    assert!(matches!(
        message(&activate),
        ToHelper::Activate { load: Some(_), .. }
    ));
    let batch = next_frame(&written);
    assert_eq!(batch.kind, FrameKind::Event);
    let ToHelper::Event {
        plugin: to, events, ..
    } = message(&batch)
    else {
        panic!("a batch is one event frame");
    };
    assert_eq!(to, plugin);
    assert_eq!(events.len(), 2, "one burst arrives as one call");
}
