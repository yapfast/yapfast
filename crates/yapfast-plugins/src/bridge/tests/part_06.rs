//! From `a_request_for_another_host_while_a_dialog_is_up_is_refused_with_a_reason` onward: part 6 of 11.

use super::*;

#[test]
fn a_request_for_another_host_while_a_dialog_is_up_is_refused_with_a_reason() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), CONSENT_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    install_gate(&shared, directory.path(), &plugin);
    let runtime = runtime();
    runtime.block_on(async {
        session.on_call(
            &shared,
            10,
            plugin.clone(),
            fetch_call("https://api.example.com/x"),
        );
    });
    wait_held(&shared, &plugin, "api.example.com", 1);

    runtime.block_on(async {
        session.on_call(
            &shared,
            11,
            plugin.clone(),
            fetch_call("https://cdn.example.com/y"),
        );
    });
    let reply = next_frame(&written);
    let (call, answer) = returned(&reply);
    assert_eq!(call, 11, "the second request is answered at once");
    let HostReturn::Denied { reason } = answer else {
        panic!("only one dialog is open per plugin");
    };
    assert!(reason.contains("another site"), "{reason}");
    assert!(
        reason.contains("cdn.example.com"),
        "the plugin is told which host to ask about again: {reason}"
    );
    assert_eq!(
        waiting(&shared, &plugin, "cdn.example.com"),
        0,
        "the refused request is not held"
    );
    assert_eq!(waiting(&shared, &plugin, "api.example.com"), 1);
}

#[test]
fn turning_a_plugin_off_lets_go_the_requests_it_had_waiting() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), CONSENT_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let mut bridge = bridge(directory.path(), wake);
    let shared = Arc::clone(&bridge.shared);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    install_gate(&shared, directory.path(), &plugin);
    let runtime = runtime();
    runtime.block_on(async {
        session.on_call(
            &shared,
            12,
            plugin.clone(),
            fetch_call("https://api.example.com/x"),
        );
    });
    wait_held(&shared, &plugin, "api.example.com", 1);

    bridge.set_enabled(&plugin, false).expect("turning it off");
    let (call, answer) = returned(&next_frame(&written));
    assert_eq!(call, 12);
    let HostReturn::Denied { reason } = answer else {
        panic!("nothing will answer a question about a plugin that is off");
    };
    assert!(reason.contains("turned off"), "{reason}");
    assert_eq!(waiting(&shared, &plugin, "api.example.com"), 0);
}

#[test]
fn a_call_reaches_its_callee_and_comes_back_under_the_caller_s_number() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let ids = install_all(directory.path(), &[BASE_MANIFEST, TOP_MANIFEST]);
    let (base, top) = (ids[0].clone(), ids[1].clone());
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    // The caller doesn't declare the callee as a dependency of anything
    // else, and the callee is asleep: a call wakes it first.
    session.on_call(&shared, 7, top.clone(), call_request(&base, "answer"));
    let activate = next_frame(&written);
    assert!(
        matches!(message(&activate), ToHelper::Activate { plugin, .. } if plugin == base),
        "the callee is started before it is called"
    );
    let invoke = next_frame(&written);
    assert_eq!(
        message(&invoke),
        ToHelper::Invoke {
            plugin: base.clone(),
            function: "answer".to_owned(),
            args_base64: "aGk=".to_owned(),
        }
    );

    let answer = Frame::new(
        FrameKind::Report,
        invoke.id,
        serde_json::to_vec(&FromHelper::Invoked {
            answer_base64: Some("aGVsbG8=".to_owned()),
            fault: None,
        })
        .expect("it serializes"),
    );
    assert!(session.on_frame(&shared, answer).is_none());
    let reply = next_frame(&written);
    assert_eq!(
        message(&reply),
        ToHelper::Return {
            plugin: top,
            call: 7,
            answer: HostReturn::Ok(serde_json::json!({ "answer_base64": "aGVsbG8=" })),
        }
    );
}

#[test]
fn a_call_to_a_plugin_the_caller_does_not_declare_is_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let ids = install_all(directory.path(), &[BASE_MANIFEST, TOP_MANIFEST]);
    let (base, top) = (ids[0].clone(), ids[1].clone());
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    // The other way round: base declares nothing, so it may not call top.
    session.on_call(&shared, 3, base.clone(), call_request(&top, "answer"));
    let reply = next_frame(&written);
    let ToHelper::Return {
        plugin,
        call: 3,
        answer,
    } = message(&reply)
    else {
        panic!("a refused call is answered");
    };
    assert_eq!(plugin, base);
    let HostReturn::Denied { reason } = answer else {
        panic!("a call along no declared edge is refused");
    };
    assert!(reason.contains("com.example.top"), "{reason}");
    assert!(reason.contains("dependency"), "{reason}");
}

#[test]
fn a_call_to_a_plugin_that_is_off_is_refused_and_names_it() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let ids = install_all(directory.path(), &[BASE_MANIFEST, TOP_MANIFEST]);
    let (base, top) = (ids[0].clone(), ids[1].clone());
    {
        // The person turned the dependency off.
        let store = Store::open(directory.path()).expect("a store");
        store.set_enabled(&base, false).expect("it is installed");
    }
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    *shared.registry_mut() = Registry::load(&shared.store());
    let (_session_directory, mut session, written, _inbound) = session_stub();

    session.on_call(&shared, 9, top.clone(), call_request(&base, "answer"));
    let reply = next_frame(&written);
    let ToHelper::Return {
        plugin,
        call: 9,
        answer,
    } = message(&reply)
    else {
        panic!("a call to an off plugin is answered");
    };
    assert_eq!(plugin, top);
    assert!(
        matches!(&answer, HostReturn::Denied { reason } if reason.contains("not enabled")),
        "{answer:?}"
    );
}

#[test]
fn a_call_that_would_come_back_around_names_the_chain() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    // Both declare each other, which the graph refuses to order; the entries
    // are still what the registry answers a call's edge from.
    let ids = install_all(
        directory.path(),
        &[
            r#"{"api":1,"id":"com.example.base","name":"Base","version":"1.0.0",
                    "wasm":"plugin.wasm","dependencies":{"com.example.top":"^1"},
                    "permissions":{"capabilities":[]}}"#,
            TOP_MANIFEST,
        ],
    );
    let (base, top) = (ids[0].clone(), ids[1].clone());
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    // base is already waiting on a call into top, so top calling base would
    // close the loop.
    session.calls.push(InFlight {
        frame: 1,
        from: base.clone(),
        to: top.clone(),
    });

    session.on_call(&shared, 4, top.clone(), call_request(&base, "answer"));
    let reply = next_frame(&written);
    let ToHelper::Return {
        plugin,
        call: 4,
        answer,
    } = message(&reply)
    else {
        panic!("a looping call is answered, not deadlocked");
    };
    assert_eq!(plugin, top);
    let HostReturn::Denied { reason } = answer else {
        panic!("a loop is refused");
    };
    assert!(reason.contains("com.example.base"), "{reason}");
    assert!(reason.contains("com.example.top"), "{reason}");
    assert!(reason.contains('→'), "{reason}");
}
