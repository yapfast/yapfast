//! From `a_line_is_drawn_in_the_banner_or_the_toast_and_an_empty_one_is_an_error` onward: part 5 of 11.

use super::*;

#[test]
fn a_line_is_drawn_in_the_banner_or_the_toast_and_an_empty_one_is_an_error() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), PAINTER_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, _written, _inbound) = session_stub();

    let empty = HostCall::new(call::method::UI_NOTICE, Line { text: "  ".into() }).expect("a call");
    assert!(matches!(
        session.answer(&shared, &plugin, empty),
        HostReturn::Error { .. }
    ));

    let notice = HostCall::new(
        call::method::UI_NOTICE,
        Line {
            text: "**hi** there".into(),
        },
    )
    .expect("a call");
    assert_eq!(
        session.answer(&shared, &plugin, notice),
        answered(SlotDrawn::yes())
    );
    let toast = HostCall::new(
        call::method::UI_TOAST,
        Line {
            text: "done".into(),
        },
    )
    .expect("a call");
    assert_eq!(
        session.answer(&shared, &plugin, toast),
        answered(SlotDrawn::yes())
    );
    session.paint(&shared, Instant::now());
    let slots: Vec<Slot> = std::iter::from_fn(|| notices.try_recv().ok())
        .filter_map(|notice| match notice {
            Notice::Slot(slots::Output::SetSlot { slot, .. }) => Some(slot),
            _ => None,
        })
        .collect();
    assert_eq!(slots, [Slot::Banner, Slot::Toast]);
}

#[test]
fn a_dialog_is_one_slot_a_plugin_opens_one_at_a_time() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), PAINTER_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, _written, _inbound) = session_stub();

    for text in ["first", "second"] {
        let open = HostCall::new(
            call::method::UI_OPEN_DIALOG,
            OpenDialog {
                tree: text_tree(text),
            },
        )
        .expect("a call");
        assert_eq!(
            session.answer(&shared, &plugin, open),
            answered(SlotDrawn::yes())
        );
    }
    session.paint(&shared, Instant::now());
    let drawn: Vec<slots::Output> = std::iter::from_fn(|| notices.try_recv().ok())
        .filter_map(|notice| match notice {
            Notice::Slot(output) => Some(output),
            _ => None,
        })
        .collect();
    assert_eq!(drawn.len(), 1, "one dialog for one plugin at a time");
    let slots::Output::SetSlot { slot, tree, .. } = &drawn[0] else {
        panic!("a slot's tree");
    };
    assert_eq!(*slot, Slot::Dialog);
    assert_eq!(tree.rows[0].cells[0].text, "second");
}

#[test]
fn a_tree_is_drawn_as_a_part_its_manifest_names() {
    const NAMING: &str = r#"{"api":1,"id":"com.example.painter","name":"Painter",
        "version":"1.0.0","wasm":"plugin.wasm",
        "permissions":{"capabilities":["ui.modify"]},
        "parts":[{"name":"digest","slot":"banner"},{"name":"ask","slot":"dialog"}]}"#;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), NAMING, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, _written, _inbound) = session_stub();

    let named = |slot: &str, part: Option<&str>| {
        HostCall::new(
            call::method::UI_SET_SLOT,
            SetSlot {
                slot: slot.to_owned(),
                tree: text_tree("hi"),
                part: part.map(str::to_owned),
            },
        )
        .expect("a call")
    };
    assert_eq!(
        session.answer(&shared, &plugin, named("banner", None)),
        answered(SlotDrawn::yes())
    );
    assert_eq!(
        session.answer(&shared, &plugin, named("dialog", Some("ask"))),
        answered(SlotDrawn::yes())
    );
    for (slot, part) in [("toast", "digest"), ("banner", "nothing")] {
        assert!(
            matches!(
                session.answer(&shared, &plugin, named(slot, Some(part))),
                HostReturn::Denied { .. }
            ),
            "{part} in {slot} isn't a part the manifest gives that slot"
        );
    }
    let line = HostCall::new(
        call::method::UI_TOAST,
        Line {
            text: "done".into(),
        },
    )
    .expect("a call");
    assert_eq!(
        session.answer(&shared, &plugin, line),
        answered(SlotDrawn::yes())
    );
    session.paint(&shared, Instant::now());
    let parts: Vec<(Slot, Option<String>)> = std::iter::from_fn(|| notices.try_recv().ok())
        .filter_map(|notice| match notice {
            Notice::Slot(slots::Output::SetSlot { slot, tree, .. }) => Some((slot, tree.part)),
            _ => None,
        })
        .collect();
    assert_eq!(
        parts,
        [
            (Slot::Banner, Some("com.example.painter/digest".to_owned())),
            (Slot::Dialog, Some("com.example.painter/ask".to_owned())),
            (Slot::Toast, None),
        ]
    );
}

#[test]
fn a_plugin_draws_at_most_ten_frames_a_second_and_a_quiet_one_wakes_nothing() {
    let now = Instant::now();
    let mut painting = Painting::default();
    let output = |text: &str| slots::Output::SetSlot {
        plugin: "com.example.painter".to_owned(),
        slot: Slot::Banner,
        tree: crate::plugins::ui::Tree::parse(&text_tree(text).to_string()).expect("a tree"),
    };

    painting.queue(now, output("one"));
    assert_eq!(painting.due, Some(now), "the first drawing goes at once");
    assert_eq!(painting.take(now).len(), 1);
    assert_eq!(painting.due, None, "nothing waits, so no timer wakes");

    // Drawing again in the same moment waits for the frame window.
    painting.queue(now + Duration::from_millis(1), output("two"));
    assert_eq!(painting.due, Some(now + PAINT_PERIOD));
    painting.queue(now + Duration::from_millis(2), output("three"));
    assert_eq!(painting.drawn.len(), 1, "the last tree for a place wins");
    assert_eq!(
        painting.take(now + PAINT_PERIOD).len(),
        1,
        "ten frames a second, not one per drawing"
    );
}

#[test]
fn a_fetch_for_a_host_nobody_allowed_is_held_rather_than_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), CONSENT_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    install_gate(&shared, directory.path(), &plugin);

    let runtime = runtime();
    runtime.block_on(async {
        session.on_call(
            &shared,
            7,
            plugin.clone(),
            fetch_call("https://api.example.com/x"),
        );
    });
    wait_held(&shared, &plugin, "api.example.com", 1);
    let Notice::Consent {
        plugin: asked,
        host,
    } = notices
        .recv_timeout(Duration::from_secs(2))
        .expect("the person is asked about the host")
    else {
        panic!("an undeclared host is a question, not anything else");
    };
    assert_eq!(asked, plugin);
    assert_eq!(host, "api.example.com");
    // The guest was given `pending` by the helper, and nothing is answered
    // while the question stands: a late answer is what it expects.
    assert!(
        written.recv_timeout(Duration::from_millis(150)).is_err(),
        "a request waiting on a dialog is not refused"
    );
}

#[test]
fn an_allow_completes_the_held_request_and_the_answer_reaches_the_guest() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), CONSENT_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let grants = install_gate(&shared, directory.path(), &plugin);
    let runtime = runtime();
    runtime.block_on(async {
        session.on_call(
            &shared,
            7,
            plugin.clone(),
            fetch_call("https://api.example.com/x"),
        );
    });
    wait_held(&shared, &plugin, "api.example.com", 1);

    assert_eq!(
        answer_about(&grants, &shared, &plugin, "api.example.com", true),
        1
    );
    let reply = next_frame(&written);
    assert_eq!(reply.kind, FrameKind::Reply);
    assert_eq!(reply.id, 7, "the answer goes back under the call's own id");
    let (call, answer) = returned(&reply);
    assert_eq!(call, 7);
    let HostReturn::Ok(value) = answer else {
        panic!("an allowed request is answered with its response");
    };
    assert_eq!(value["status"], 200);
    assert_eq!(value["url"], "https://api.example.com/x");
}

#[test]
fn a_denial_hands_the_guest_the_reason() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), CONSENT_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let grants = install_gate(&shared, directory.path(), &plugin);
    let runtime = runtime();
    runtime.block_on(async {
        session.on_call(
            &shared,
            7,
            plugin.clone(),
            fetch_call("https://api.example.com/x"),
        );
    });
    wait_held(&shared, &plugin, "api.example.com", 1);

    assert_eq!(
        answer_about(&grants, &shared, &plugin, "api.example.com", false),
        1
    );
    let (_, answer) = returned(&next_frame(&written));
    let HostReturn::Denied { reason } = answer else {
        panic!("a denial is a refusal with a reason");
    };
    assert_eq!(
        reason,
        format!("you refused api.example.com for {plugin}"),
        "the guest is told what the person said"
    );
}

#[test]
fn a_second_request_for_the_same_host_joins_the_dialog_and_one_answer_frees_both() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), CONSENT_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let grants = install_gate(&shared, directory.path(), &plugin);
    let runtime = runtime();
    runtime.block_on(async {
        for id in [8, 9] {
            session.on_call(
                &shared,
                id,
                plugin.clone(),
                fetch_call("https://api.example.com/x"),
            );
        }
    });
    wait_held(&shared, &plugin, "api.example.com", 2);
    // One question, not two: whichever asked second found a dialog up.
    let Notice::Consent { host, .. } = notices
        .recv_timeout(Duration::from_secs(2))
        .expect("the person is asked once")
    else {
        panic!("an undeclared host is a question");
    };
    assert_eq!(host, "api.example.com");
    assert!(
        notices.recv_timeout(Duration::from_millis(200)).is_err(),
        "a request that joins a dialog asks nothing new"
    );

    assert_eq!(
        answer_about(&grants, &shared, &plugin, "api.example.com", true),
        2
    );
    let mut answered = Vec::new();
    for _ in 0..2 {
        let reply = next_frame(&written);
        let (call, answer) = returned(&reply);
        assert!(matches!(answer, HostReturn::Ok(_)), "{answer:?}");
        answered.push(call);
    }
    answered.sort_unstable();
    assert_eq!(answered, vec![8, 9]);
}
