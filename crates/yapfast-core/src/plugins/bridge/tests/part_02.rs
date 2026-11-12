//! From `a_settings_change_reaches_an_instance_that_asked_for_it` onward: part 2 of 11.

use super::*;

/// A plugin that asks to hear about its settings is answered `pending` now,
/// and a change arrives later under the same call id, as the answer to that
/// call. Nothing restarts, and a plugin that never asked hears nothing.
#[test]
fn a_settings_change_reaches_an_instance_that_asked_for_it() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let ask = Frame::new(
        FrameKind::Request,
        0,
        serde_json::to_vec(&FromHelper::Call {
            plugin: plugin.clone(),
            call: 5,
            request: call::HostCall::new(call::method::SETTINGS_ON_CHANGE, serde_json::json!({}))
                .expect("a call"),
        })
        .expect("it serializes"),
    );
    assert!(session.on_frame(&shared, ask).is_none());
    let reply = next_frame(&written);
    let ToHelper::Return {
        call: answered,
        answer,
        ..
    } = message(&reply)
    else {
        panic!("a host call is answered with a return");
    };
    assert_eq!(answered, 5);
    assert_eq!(
        answer,
        HostReturn::Pending {
            tag: "5".to_owned()
        },
        "the guest isn't held while a person decides"
    );

    // Nothing is said to a plugin that didn't ask.
    let other = id("com.example.other");
    runtime().block_on(session.on_command(
        &shared,
        Request::SettingsNews {
            plugin: other,
            news: SettingsNews {
                changed: BTreeMap::from([("mode".to_owned(), serde_json::json!("hide"))]),
                widget: None,
            },
        },
    ));
    assert!(
        written.recv_timeout(Duration::from_millis(50)).is_err(),
        "a plugin that never asked is told nothing"
    );

    // The change comes as the answer to the call that asked for it.
    runtime().block_on(session.on_command(
        &shared,
        Request::SettingsNews {
            plugin: plugin.clone(),
            news: SettingsNews {
                changed: BTreeMap::from([("mode".to_owned(), serde_json::json!("hide"))]),
                widget: None,
            },
        },
    ));
    let news = next_frame(&written);
    let ToHelper::Return {
        call: answered,
        answer,
        ..
    } = message(&news)
    else {
        panic!("a settings change is the answer to the call that asked");
    };
    assert_eq!(answered, 5);
    assert_eq!(
        answer,
        HostReturn::Ok(serde_json::json!({ "changed": { "mode": "hide" } }))
    );
}

#[test]
fn a_send_goes_out_unchanged_when_the_process_ends() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let held = HeldSend {
        id: 12,
        draft: draft("hi"),
        hook: HookKind::MessagePreSend,
        chain: vec![plugin.clone()],
        deadline: Instant::now() + Duration::from_millis(100),
    };
    let request = HookRequest {
        hook: HookKind::MessagePreSend,
        subject: HookSubject::Draft(draft("hi")),
    };
    runtime().block_on(session.on_command(&shared, Request::Hook { held, request }));
    let activate = next_frame(&written);
    assert!(matches!(
        message(&activate),
        ToHelper::Activate { load: Some(_), .. }
    ));
    let _hook = next_frame(&written);

    // The helper went away with the send still waiting: the message goes as
    // the person wrote it rather than never arriving.
    session.fail_open(&shared);
    assert_eq!(
        notices
            .recv_timeout(Duration::from_secs(2))
            .expect("a notice"),
        Notice::SendReady {
            id: 12,
            draft: draft("hi"),
        }
    );
}

#[test]
fn a_read_is_answered_from_what_the_application_published() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), READ_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let world = &shared.world;
    world.publish_account(
        Some("person-1"),
        Some("Ann"),
        Some("ann"),
        None,
        Some("online"),
        true,
    );
    world.publish_lists(
        1,
        vec![yapfast_plugin_api::event::RoomView {
            id: "discord:1:2".to_owned(),
            name: "general".to_owned(),
            network: Network::Discord,
            server: Some("1".to_owned()),
            dm: false,
        }],
    );
    world.publish_selected(Some("discord:1:2"));
    world.publish_timeline(
        "discord:1:2",
        &Arc::new(vec![Arc::new(crate::chat::ChatMessage {
            id: "message-1".to_owned(),
            item: "message-1".to_owned(),
            sender: "person-2".to_owned(),
            author: "Bo".to_owned(),
            body: "hello".to_owned(),
            timestamp: 1_700_000_000,
            ..crate::chat::ChatMessage::default()
        })]),
    );

    // `app.version` is the program's own, which only it knows.
    let app = ask(
        &mut session,
        &written,
        &shared,
        &plugin,
        HostCall::new(call::method::APP_VERSION, serde_json::json!({})).expect("a call"),
    );
    let HostReturn::Ok(answer) = app else {
        panic!("`app.version` answers: {app:?}");
    };
    assert_eq!(answer["version"], "0.0.0");

    let me = ask(
        &mut session,
        &written,
        &shared,
        &plugin,
        HostCall::new(call::method::ACCOUNT_ME, serde_json::json!({})).expect("a call"),
    );
    let HostReturn::Ok(me) = me else {
        panic!("`account.me` answers: {me:?}");
    };
    assert_eq!(me["name"], "Ann");

    let rooms = ask(
        &mut session,
        &written,
        &shared,
        &plugin,
        HostCall::new(call::method::ROOMS_LIST, RoomsList { limit: Some(5) }).expect("a call"),
    );
    let HostReturn::Ok(rooms) = rooms else {
        panic!("`rooms.list` answers: {rooms:?}");
    };
    assert_eq!(rooms[0]["name"], "general");

    let selected = ask(
        &mut session,
        &written,
        &shared,
        &plugin,
        HostCall::new(call::method::ROOMS_SELECTED, serde_json::json!({})).expect("a call"),
    );
    assert_eq!(
        selected,
        HostReturn::Ok(serde_json::json!({
            "id": "discord:1:2",
            "name": "general",
            "network": "discord",
            "server": "1",
            "dm": false,
        }))
    );

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
    let HostReturn::Ok(history) = history else {
        panic!("`messages.history` answers: {history:?}");
    };
    assert_eq!(history[0]["body"], "hello");
    assert_eq!(history[0]["at"], 1_700_000_000_000_u64);
}
