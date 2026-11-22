//! From `a_publish_reaches_the_subscribers_and_stops_when_one_leaves` onward: part 7 of 11.

use super::*;

#[test]
fn a_publish_reaches_the_subscribers_and_stops_when_one_leaves() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let ids = install_all(directory.path(), &[BASE_MANIFEST, TOP_MANIFEST]);
    let (base, top) = (ids[0].clone(), ids[1].clone());
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let subscribed = session.subscribe(
        &shared,
        &base,
        HostCall::new(
            call::method::PLUGIN_SUBSCRIBE,
            PluginSubscribe {
                topic: "notes".to_owned(),
                cancel: false,
            },
        )
        .expect("a subscribe payload"),
    );
    assert_eq!(
        subscribed,
        HostReturn::Ok(serde_json::json!({ "topic": "notes", "subscribed": true }))
    );

    let published = session.answer(
        &shared,
        &top,
        HostCall::new(
            call::method::PLUGIN_PUBLISH,
            yapfast_plugin_api::call::PluginPublish {
                topic: "notes".to_owned(),
                payload: serde_json::json!({ "body": "hi" }),
            },
        )
        .expect("a publish payload"),
    );
    assert_eq!(
        published,
        HostReturn::Ok(serde_json::json!({ "delivered": 1 }))
    );
    // A publish is coalesced, so it arrives with the window the events wait
    // in: the subscriber is called once, with the topic in its batch.
    std::thread::sleep(Duration::from_millis(COALESCE_MS + 50));
    session.on_tick(&shared);
    let activate = next_frame(&written);
    assert!(matches!(message(&activate), ToHelper::Activate { .. }));
    let batch = next_frame(&written);
    let ToHelper::Event { plugin, topics, .. } = message(&batch) else {
        panic!("a topic arrives as an event batch");
    };
    assert_eq!(plugin, base);
    assert_eq!(topics.len(), 1);
    assert_eq!(topics[0].topic, "notes");
    assert_eq!(topics[0].from, top);
    assert_eq!(topics[0].payload, serde_json::json!({ "body": "hi" }));

    // Leaving the topic stops it: nothing is delivered.
    session.subscribe(
        &shared,
        &base,
        HostCall::new(
            call::method::PLUGIN_SUBSCRIBE,
            PluginSubscribe {
                topic: "notes".to_owned(),
                cancel: true,
            },
        )
        .expect("a subscribe payload"),
    );
    let published = session.answer(
        &shared,
        &top,
        HostCall::new(
            call::method::PLUGIN_PUBLISH,
            yapfast_plugin_api::call::PluginPublish {
                topic: "notes".to_owned(),
                payload: serde_json::json!(1),
            },
        )
        .expect("a publish payload"),
    );
    assert_eq!(
        published,
        HostReturn::Ok(serde_json::json!({ "delivered": 0 }))
    );
}

#[test]
fn an_update_stops_dependents_first_and_no_dependent_runs_against_nothing() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let ids = install_all(directory.path(), &[BASE_MANIFEST, TOP_MANIFEST]);
    let (base, top) = (ids[0].clone(), ids[1].clone());
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    // Both are up, which is what an update finds.
    session.active.insert(base.clone());
    session.active.insert(top.clone());

    runtime().block_on(session.on_command(
        &shared,
        Request::Swap {
            plugin: base.clone(),
            stop: vec![top.clone()],
            start: vec![top.clone()],
        },
    ));
    // The dependent goes first: nothing is left needing the plugin while it
    // is replaced.
    let stop_top = next_frame(&written);
    assert_eq!(
        message(&stop_top),
        ToHelper::Deactivate {
            plugin: top.clone()
        }
    );
    assert!(
        written.recv_timeout(Duration::from_millis(100)).is_err(),
        "nothing else moves until the helper says the dependent is gone"
    );
    session.on_frame(
        &shared,
        Frame::new(
            FrameKind::Report,
            stop_top.id,
            serde_json::to_vec(&FromHelper::Deactivated {
                plugin: top.clone(),
                state: None,
            })
            .expect("it serializes"),
        ),
    );

    // Now the plugin itself: its old instance says goodbye, and its state is
    // what the next activation is given.
    let stop_base = next_frame(&written);
    assert_eq!(
        message(&stop_base),
        ToHelper::Deactivate {
            plugin: base.clone()
        }
    );
    session.on_frame(
        &shared,
        Frame::new(
            FrameKind::Report,
            stop_base.id,
            serde_json::to_vec(&FromHelper::Deactivated {
                plugin: base.clone(),
                state: Some(r#"{"seen":3}"#.to_owned()),
            })
            .expect("it serializes"),
        ),
    );

    // The replacement is loaded and started, still before the dependent.
    let start_base = next_frame(&written);
    let ToHelper::Activate {
        plugin: started,
        load: Some(load),
    } = message(&start_base)
    else {
        panic!("the replacement is activated with its module");
    };
    assert_eq!(started, base);
    assert_eq!(
        load.state.as_deref(),
        Some(r#"{"seen":3}"#),
        "the state a plugin handed on crosses the swap"
    );

    // A dependent calling the plugin now waits rather than being told it is
    // not installed: this is the window the milestone is about.
    session.on_call(&shared, 11, top.clone(), call_request(&base, "answer"));
    assert!(
        written.recv_timeout(Duration::from_millis(100)).is_err(),
        "the call waits for the swap instead of being refused"
    );

    session.on_frame(
        &shared,
        Frame::new(
            FrameKind::Report,
            start_base.id,
            serde_json::to_vec(&FromHelper::Activated {
                plugin: base.clone(),
                state: None,
                memory_pages: 64,
            })
            .expect("it serializes"),
        ),
    );
    // Only now does the dependent come back.
    let start_top = next_frame(&written);
    assert!(
        matches!(message(&start_top), ToHelper::Activate { plugin, .. } if plugin == top),
        "the dependent starts after its dependency is back"
    );
    session.on_frame(
        &shared,
        Frame::new(
            FrameKind::Report,
            start_top.id,
            serde_json::to_vec(&FromHelper::Activated {
                plugin: top.clone(),
                state: None,
                memory_pages: 64,
            })
            .expect("it serializes"),
        ),
    );
    // And the call it made during the swap is answered by the plugin it was
    // meant for.
    let invoke = next_frame(&written);
    assert_eq!(
        message(&invoke),
        ToHelper::Invoke {
            plugin: base.clone(),
            function: "answer".to_owned(),
            args_base64: "aGk=".to_owned(),
        }
    );
}

#[test]
fn turning_a_dependency_off_turns_off_what_needs_it() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let ids = install_all(directory.path(), &[BASE_MANIFEST, TOP_MANIFEST]);
    let (base, top) = (ids[0].clone(), ids[1].clone());
    let (wake, _notices) = notice_channel();
    let mut bridge = bridge(directory.path(), wake);

    bridge.set_enabled(&base, false).expect("it is installed");
    let statuses = bridge.statuses();
    for plugin in [&base, &top] {
        let status = statuses
            .iter()
            .find(|(id, _)| id == plugin)
            .map(|(_, status)| status.clone());
        assert_eq!(
            status,
            Some(PluginStatus::Installed),
            "{plugin} is off, and so is what needed it"
        );
    }
}

#[test]
fn a_plugin_loaded_from_a_folder_is_installed_and_on() {
    let directory = tempfile::tempdir().expect("a data directory");
    let folder = plugin_folder(
        r#"{"api":1,"id":"dev.local.base","name":"Base","version":"0.1.0","wasm":"plugin.wasm"}"#,
        b"\0asm and then some",
    );
    let (wake, _notices) = notice_channel();
    let mut bridge = bridge(directory.path(), wake);

    let plugin = bridge
        .load_folder(folder.path())
        .expect("a folder becomes a plugin");
    assert_eq!(plugin.as_str(), "dev.local.base");
    let statuses = bridge.statuses();
    assert_eq!(
        statuses
            .iter()
            .find(|(id, _)| *id == plugin)
            .map(|(_, status)| status.clone()),
        Some(PluginStatus::Enabled),
        "the person asked for it to run"
    );
    // Reading the same folder again is an update of that plugin, not a
    // second one.
    bridge.load_folder(folder.path()).expect("a reload");
    assert_eq!(bridge.listed().len(), 1);
}
