//! From `an_activation_answer_clears_its_request` onward: part 4 of 11.

use super::*;

/// The answer to an activation clears the request that asked for it. It
/// did not, so every plugin that loaded was faulted thirty seconds later
/// with "the helper didn't answer its activation request" — a plugin that
/// had been running the whole time.
#[test]
fn an_activation_answer_clears_its_request() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    runtime().block_on(session.on_command(
        &shared,
        Request::Activate {
            plugins: vec![plugin.clone()],
        },
    ));
    let activate = next_frame(&written);
    assert!(matches!(
        message(&activate),
        ToHelper::Activate { load: Some(_), .. }
    ));
    assert_eq!(
        session.pending.len(),
        1,
        "the activation is being waited on"
    );

    let answer = Frame::new(
        FrameKind::Reply,
        activate.id,
        serde_json::to_vec(&FromHelper::Activated {
            plugin: plugin.clone(),
            state: None,
            memory_pages: 256,
        })
        .expect("it serializes"),
    );
    assert!(session.on_frame(&shared, answer).is_none());
    assert!(
        session.pending.is_empty(),
        "answered, so its deadline can't fault a plugin that loaded"
    );
    assert!(session.active.contains(&plugin));
}

/// A command's answer clears the command's request too: without it every
/// `/command` faulted its plugin two seconds after answering, so the person
/// saw `pong` and then a plugin that had "kept crashing".
#[test]
fn a_command_answer_clears_its_request() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let invocation = CommandInvocation {
        name: "ping".to_owned(),
        options: serde_json::Map::new(),
    };
    runtime().block_on(session.on_command(
        &shared,
        Request::Command {
            plugin: plugin.clone(),
            invocation,
        },
    ));
    let activate = next_frame(&written);
    let command = next_frame(&written);
    assert!(matches!(message(&command), ToHelper::Command { .. }));
    assert_eq!(session.pending.len(), 2, "the activation and the command");

    for (frame, report) in [
        (
            activate,
            FromHelper::Activated {
                plugin: plugin.clone(),
                state: None,
                memory_pages: 256,
            },
        ),
        (
            command,
            FromHelper::CommandDone {
                plugin: plugin.clone(),
                outcome: CommandOutcome::Notice {
                    text: "pong".to_owned(),
                },
            },
        ),
    ] {
        let answer = Frame::new(
            FrameKind::Reply,
            frame.id,
            serde_json::to_vec(&report).expect("it serializes"),
        );
        assert!(session.on_frame(&shared, answer).is_none());
    }
    assert!(
        session.pending.is_empty(),
        "answered, so neither deadline faults a plugin that worked"
    );
}

#[test]
fn a_message_event_goes_out_at_once() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let event = SharedEvent::new(event(EventKind::MessageCreate, "hi")).expect("it serializes");
    runtime().block_on(session.on_command(
        &shared,
        Request::Event {
            plugin: plugin.clone(),
            event,
        },
    ));
    let activate = next_frame(&written);
    assert!(matches!(
        message(&activate),
        ToHelper::Activate { load: Some(_), .. }
    ));
    let batch = next_frame(&written);
    let ToHelper::Event { events, .. } = message(&batch) else {
        panic!("a message event is one event frame");
    };
    assert_eq!(events.len(), 1, "a message is not coalesced");
}

#[test]
fn presence_and_speaking_arrive_as_one_batch_and_not_one_call_each() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), WATCHER_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    // A burst: two people came online and one of them started talking, all
    // inside the quarter-second window the coalesced kinds wait in.
    for (kind, body) in [
        (EventKind::Presence, "person-1"),
        (EventKind::Presence, "person-2"),
        (EventKind::Speaking, "person-1"),
    ] {
        runtime().block_on(session.on_command(
            &shared,
            Request::Event {
                plugin: plugin.clone(),
                event: SharedEvent::new(event(kind, body)).expect("it serializes"),
            },
        ));
    }
    // Nothing is sent while the window is open: a burst is one call.
    assert!(
        written.recv_timeout(Duration::from_millis(50)).is_err(),
        "presence and speaking are held for their window"
    );
    std::thread::sleep(Duration::from_millis(COALESCE_MS + 50));
    session.on_tick(&shared);
    let activate = next_frame(&written);
    assert!(matches!(
        message(&activate),
        ToHelper::Activate { load: Some(_), .. }
    ));
    // One frame, once the window has gone by, with all three: a plugin that
    // watched a busy call would otherwise be woken per frame it speaks.
    let batch = next_frame(&written);
    let ToHelper::Event {
        events: sent,
        topics,
        ..
    } = message(&batch)
    else {
        panic!("a coalesced burst is one event frame");
    };
    assert!(topics.is_empty());
    let kinds: Vec<EventKind> = sent.iter().map(|event| event.kind).collect();
    assert_eq!(
        kinds,
        vec![
            EventKind::Presence,
            EventKind::Presence,
            EventKind::Speaking
        ]
    );
}

#[test]
fn a_slot_is_drawn_once_a_frame_and_a_slot_that_isnt_there_is_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), PAINTER_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, _written, _inbound) = session_stub();
    let now = Instant::now();

    // A slot this build doesn't have, and the message slot, are refused with
    // a sentence that says what to use instead: a plugin's author reads it.
    let answer = session.answer(
        &shared,
        &plugin,
        slot_call("panel", serde_json::json!({ "rows": [] })),
    );
    let HostReturn::Denied { reason } = answer else {
        panic!("a slot that isn't there is refused");
    };
    assert!(reason.contains("no slot called `panel`"), "{reason}");
    assert!(reason.contains("banner"), "{reason}");
    let answer = session.answer(
        &shared,
        &plugin,
        slot_call("message", serde_json::json!({ "rows": [] })),
    );
    let HostReturn::Denied { reason } = answer else {
        panic!("the message slot is refused");
    };
    assert!(reason.contains("message.decorate"), "{reason}");

    // Three drawings of one slot are one frame, and it carries the last.
    for text in ["first", "second", "third"] {
        let answer = session.answer(&shared, &plugin, slot_call("banner", text_tree(text)));
        assert_eq!(answer, answered(SlotDrawn::yes()));
    }
    assert!(
        notices.try_recv().is_err(),
        "nothing is drawn before its frame"
    );
    session.paint(&shared, now);
    let Notice::Slot(slots::Output::SetSlot { slot, tree, .. }) =
        notices.recv().expect("the frame's drawing")
    else {
        panic!("a slot's tree");
    };
    assert_eq!(slot, Slot::Banner);
    assert_eq!(tree.rows[0].cells[0].text, "third", "the last one wins");
    assert!(notices.try_recv().is_err(), "one frame, one drawing");

    // A tree over the cap is refused, and nothing is drawn.
    let rows: Vec<serde_json::Value> = (0..65)
        .map(|_| serde_json::json!({ "cells": [] }))
        .collect();
    let answer = session.answer(
        &shared,
        &plugin,
        slot_call("toast", serde_json::json!({ "rows": rows })),
    );
    let HostReturn::Denied { reason } = answer else {
        panic!("a tree over the cap is refused");
    };
    assert!(reason.contains("over 64"), "{reason}");
    session.paint(&shared, now + PAINT_PERIOD);
    assert!(notices.try_recv().is_err(), "a refused tree draws nothing");
}
