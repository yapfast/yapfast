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

/// The Advanced limits: applied at once, shown on the page, and still there
/// when the bridge is opened again.
#[test]
fn changing_the_limits_writes_them_down_and_shows_them_again() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let (wake, _notices) = notice_channel();
    let mut opened = bridge(directory.path(), wake);
    assert_eq!(opened.limits(), limits::Limits::default());
    let grants = Grants::load(directory.path().join("grants.json")).expect("a grants file");
    let view = opened.page(&grants, &|at| at.to_string());
    assert_eq!(
        view.advanced.memory_budget, 256,
        "the page shows the pair the helper has always run with"
    );
    assert_eq!(view.advanced.idle_minutes, 5);

    opened
        .set_limits(limits::Limits {
            memory_mib: 512,
            idle_minutes: 15,
        })
        .expect("the choice is written down");
    let stored: serde_json::Value = serde_json::from_slice(
        &std::fs::read(directory.path().join("plugins").join("budget.json"))
            .expect("the choice is in budget.json"),
    )
    .expect("budget.json is JSON");
    assert_eq!(stored["memory_mib"], 512);
    assert_eq!(stored["idle_minutes"], 15);
    let view = opened.page(&grants, &|at| at.to_string());
    assert_eq!(view.advanced.memory_budget, 512, "the page projects it");
    assert_eq!(view.advanced.idle_minutes, 15);
    drop(opened);

    // What was written down is what the next bridge starts with.
    let (wake, _notices) = notice_channel();
    let reopened = bridge(directory.path(), wake);
    assert_eq!(
        reopened.limits(),
        limits::Limits {
            memory_mib: 512,
            idle_minutes: 15
        }
    );
    let view = reopened.page(&grants, &|at| at.to_string());
    assert_eq!(view.advanced.memory_budget, 512);
    assert_eq!(view.advanced.idle_minutes, 15);
}

/// A picker moved with plugins enabled but nothing running starts no helper:
/// choosing limits is not work, and a helper that starts later is told the
/// pair after its `Ready`.
#[test]
fn changing_the_limits_starts_no_helper() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let mut bridge = bridge(directory.path(), wake);
    assert!(bridge.enabled(), "the plugin is on");
    bridge
        .set_limits(limits::Limits {
            memory_mib: 128,
            idle_minutes: 1,
        })
        .expect("the choice is written down");
    assert!(
        bridge.shared().helper_pid().is_none(),
        "no helper process was started"
    );
    assert!(bridge.supervisor.is_none(), "and no supervisor either");
    assert_eq!(bridge.helper(), HelperState::Stopped);
    bridge.shutdown();
}

/// A change that was sent while the old helper was still up can become the
/// supervisor's first command after that helper exits. It is not work: the next
/// real helper reads the shared pair after `Ready`, so this starts nothing.
#[test]
fn a_limits_change_waiting_after_an_exit_starts_no_helper() {
    static STARTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    fn stderr() -> std::process::Stdio {
        STARTS.fetch_add(1, Ordering::Relaxed);
        std::process::Stdio::null()
    }

    let directory = tempfile::tempdir().expect("a temporary directory");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (commands, receiver) = tokio::sync::mpsc::unbounded_channel();
    commands
        .send(Request::Limits {
            memory_mib: 128,
            idle_minutes: 1,
        })
        .expect("the late change is queued");
    commands
        .send(Request::Shutdown)
        .expect("the test can finish");

    let finished = runtime().block_on(async {
        tokio::time::timeout(
            Duration::from_secs(2),
            supervise(
                Arc::clone(&shared),
                receiver,
                directory.path().to_owned(),
                stderr,
            ),
        )
        .await
    });
    assert_eq!(
        STARTS.load(Ordering::Relaxed),
        0,
        "the late change never tried to start a replacement helper"
    );
    finished.expect("dropping the late change does not wait for a helper");
    assert!(shared.helper_pid().is_none());
    assert!(shared.off().is_none(), "no failed start was recorded");
}

/// A helper is given the person's limits right after its `Ready` and before
/// any work reaches it, and a change later is one frame with no answer.
#[test]
fn a_helper_gets_the_limits_after_its_hello_and_before_any_work() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    shared.set_limits(limits::Limits {
        memory_mib: 512,
        idle_minutes: 15,
    });
    let (_session_directory, mut session, written, inbound) = session_stub();
    let (commands, mut receiver) = tokio::sync::mpsc::unbounded_channel::<Request>();
    let run_shared = Arc::clone(&shared);
    let runtime = runtime();
    let running = runtime.spawn(async move { session.run(&run_shared, &mut receiver).await });

    let hello = next_frame(&written);
    assert_eq!(message(&hello), ToHelper::Hello { api: API_VERSION });
    inbound
        .send(Inbound::Frame(Frame::new(
            FrameKind::Hello,
            0,
            serde_json::to_vec(&FromHelper::Ready {
                api: API_VERSION,
                version: "test".to_owned(),
            })
            .expect("it serializes"),
        )))
        .expect("the session is listening");
    let limits = next_frame(&written);
    assert_eq!(
        message(&limits),
        ToHelper::Limits {
            memory_mib: 512,
            idle_minutes: 15
        },
        "the pair the person chose goes first"
    );
    assert_eq!(limits.id, 0, "no answer is expected for it");

    // Work reaches the helper after it.
    commands
        .send(Request::Activate {
            plugins: vec![plugin],
        })
        .expect("the session is listening");
    let work = next_frame(&written);
    assert!(
        matches!(message(&work), ToHelper::Activate { .. }),
        "the first work frame comes after the limits: {:?}",
        message(&work)
    );

    // A change reaches a running helper as the same frame again.
    commands
        .send(Request::Limits {
            memory_mib: 128,
            idle_minutes: 1,
        })
        .expect("the session is listening");
    let changed = next_frame(&written);
    assert_eq!(changed.id, 0, "no answer is expected for it");
    assert_eq!(
        message(&changed),
        ToHelper::Limits {
            memory_mib: 128,
            idle_minutes: 1
        }
    );

    drop(commands);
    let ending = runtime.block_on(running).expect("the run ends");
    assert!(matches!(ending, Ending::Asked));
}

/// A change to the limits waits for a swap, as an eviction does: lowering the
/// budget evicts, and one that raced a swap would be undoing it.
#[test]
fn a_limits_change_waits_for_a_swap() {
    let (_directory, session, _written, _inbound) = session_stub();
    let change = Request::Limits {
        memory_mib: 128,
        idle_minutes: 1,
    };
    assert!(
        session.holds_for_swap(&change),
        "like an eviction, it goes after the swap"
    );
}
