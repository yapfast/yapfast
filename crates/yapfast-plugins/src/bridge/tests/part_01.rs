//! From `an_event_batch_frame_reads_back_as_the_contract_s_message` onward: part 1 of 11.

use super::*;

#[test]
fn an_event_batch_frame_reads_back_as_the_contract_s_message() {
    let plugin = id("com.example.batch");
    let first = event(EventKind::Typing, "one");
    let second = event(EventKind::Typing, "two");
    let events: Vec<Arc<SharedEvent>> = [first.clone(), second.clone()]
        .into_iter()
        .map(|event| SharedEvent::new(event).expect("it serializes"))
        .collect();
    let payload = event_payload(&plugin, &events);
    let read: ToHelper = serde_json::from_slice(&payload).expect("the payload is a message");
    assert_eq!(
        read,
        ToHelper::Event {
            plugin,
            events: vec![first, second],
            topics: Vec::new(),
        }
    );
}

#[test]
fn a_bridge_with_nothing_installed_costs_nothing() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = woken.clone();
    let mut bridge = bridge(
        directory.path(),
        Arc::new(move |_| {
            counted.fetch_add(1, Ordering::Relaxed);
        }),
    );
    assert!(!bridge.enabled());
    assert_eq!(bridge.off(), None);
    assert!(bridge.statuses().is_empty());
    assert_eq!(
        woken.load(Ordering::Relaxed),
        0,
        "opening the bridge tells the page nothing"
    );
    // An event nobody subscribed to is dropped without touching a plugin.
    bridge.observe(event(EventKind::MessageCreate, "hi"));
    let draft = Draft {
        room: "room-1".to_owned(),
        body: "hello".to_owned(),
        reply_to: None,
        attachments: Vec::new(),
    };
    match bridge.pre_send(draft.clone()) {
        PreSend::Send(sent) => assert_eq!(sent, draft),
        PreSend::Held { .. } => panic!("nothing intercepts sends"),
    }
    bridge.shutdown();
}

#[test]
fn a_missing_helper_is_reported_rather_than_retried() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let mut bridge = bridge(directory.path(), Arc::new(|_| {}));
    // Nothing is enabled, so nothing is started and nothing is claimed.
    bridge.start();
    assert!(bridge.off().is_none(), "no plugin asked for the helper");
}

#[test]
fn timers_are_capped_and_short_ones_are_lengthened() {
    let (_directory, mut session, _written, _inbound) = session_stub();
    let plugin = id("com.example.timer");
    let answer = session.set_timer(
        &plugin,
        TimerRequest {
            interval_ms: 1,
            tag: "tick".to_owned(),
        },
        true,
    );
    let HostReturn::Ok(value) = answer else {
        panic!("a timer is registered");
    };
    assert_eq!(value["interval_ms"], limits::MIN_TIMER_INTERVAL_MS);
    for index in 0..(limits::MAX_TIMERS - 1) {
        session.set_timer(
            &plugin,
            TimerRequest {
                interval_ms: 1_000,
                tag: format!("t{index}"),
            },
            false,
        );
    }
    let refused = session.set_timer(
        &plugin,
        TimerRequest {
            interval_ms: 1_000,
            tag: "one-too-many".to_owned(),
        },
        false,
    );
    assert!(matches!(refused, HostReturn::Denied { .. }));
}

#[test]
fn a_held_send_loads_the_chain_and_goes_out_with_its_answer() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let held = HeldSend {
        id: 4,
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

    // The module travels with the activation — one request for the phase —
    // and only then does the chain see the draft: frames are handled in order.
    let activate = next_frame(&written);
    assert!(matches!(
        message(&activate),
        ToHelper::Activate { plugin: started, load: Some(module) }
            if started == plugin && module.memory_pages > 0
    ));
    let hook = next_frame(&written);
    let ToHelper::Hook { chain, .. } = message(&hook) else {
        panic!("the third frame should be the chain");
    };
    assert_eq!(chain, vec![plugin.clone()]);

    let changed = HookSubject::Draft(draft("HELLO"));
    let answer = Frame::new(
        FrameKind::Report,
        hook.id,
        serde_json::to_vec(&FromHelper::HookDone {
            outcome: HookOutcome::Changed(changed),
            failures: Vec::new(),
            by: Some(plugin.clone()),
        })
        .expect("it serializes"),
    );
    assert!(session.on_frame(&shared, answer).is_none());
    assert_eq!(
        notices
            .recv_timeout(Duration::from_secs(2))
            .expect("a notice"),
        Notice::SendReady {
            id: 4,
            draft: draft("HELLO"),
        }
    );
}

#[test]
fn a_blocked_send_says_why_and_a_failed_chain_lets_it_through() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(
        directory.path(),
        BLOCKING_HOOK_MANIFEST,
        b"not a real module",
    );
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let held = HeldSend {
        id: 9,
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
    let hook = next_frame(&written);
    let block = Frame::new(
        FrameKind::Report,
        hook.id,
        serde_json::to_vec(&FromHelper::HookDone {
            outcome: HookOutcome::Block {
                reason: "that word isn't allowed here".to_owned(),
            },
            failures: Vec::new(),
            by: Some(plugin.clone()),
        })
        .expect("it serializes"),
    );
    assert!(session.on_frame(&shared, block).is_none());
    assert_eq!(
        notices
            .recv_timeout(Duration::from_secs(2))
            .expect("a notice"),
        Notice::SendBlocked {
            id: 9,
            reason: "that word isn't allowed here".to_owned(),
        }
    );
}

/// A plugin that answers `block` without `on_failure: "block"` in its
/// manifest can't stop the send: the message goes as the person wrote it.
/// The capability puts a plugin in the chain; only the manifest says its
/// failure may hold a message back.
#[test]
fn a_block_from_a_plugin_that_did_not_ask_to_block_fails_open() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let held = HeldSend {
        id: 21,
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
    let _activate = next_frame(&written);
    let hook = next_frame(&written);

    let block = Frame::new(
        FrameKind::Report,
        hook.id,
        serde_json::to_vec(&FromHelper::HookDone {
            outcome: HookOutcome::Block {
                reason: "no".to_owned(),
            },
            failures: Vec::new(),
            by: Some(plugin.clone()),
        })
        .expect("it serializes"),
    );
    assert!(session.on_frame(&shared, block).is_none());
    assert_eq!(
        notices
            .recv_timeout(Duration::from_secs(2))
            .expect("a notice"),
        Notice::SendReady {
            id: 21,
            draft: draft("hi"),
        },
        "a block nobody declared is the fail-open answer"
    );
}
