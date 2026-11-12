//! From `a_folder_plugin_is_reloaded_when_its_module_changes` onward: part 8 of 11.

use super::*;

#[test]
fn a_folder_plugin_is_reloaded_when_its_module_changes() {
    let directory = tempfile::tempdir().expect("a data directory");
    let folder = plugin_folder(
        r#"{"api":1,"id":"dev.local.base","name":"Base","version":"0.1.0","wasm":"plugin.wasm"}"#,
        b"\0asm first",
    );
    let (wake, _notices) = notice_channel();
    let mut bridge = Bridge::open(directory.path(), "0.0.0", wake).expect("a bridge");
    let plugin = bridge.load_folder(folder.path()).expect("a plugin");
    drop(bridge);

    // The session the developer's run would have: the plugin is up, and its
    // folder is watched.
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    session.active.insert(plugin.clone());
    session.watch(plugin.clone(), folder.path().to_owned());
    assert!(
        written.recv_timeout(Duration::from_millis(50)).is_err(),
        "watching a folder writes nothing"
    );

    // A save: a new module, and the poll that comes a second later.
    std::fs::write(folder.path().join("plugin.wasm"), b"\0asm second, longer").expect("a save");
    if let Some(watch) = session.dev.get_mut(&plugin) {
        watch.due = Instant::now() - Duration::from_millis(1);
    }
    session.on_tick(&shared);

    // The running instance says goodbye, and the replacement carries the new
    // module.
    let stop = next_frame(&written);
    assert_eq!(
        message(&stop),
        ToHelper::Deactivate {
            plugin: plugin.clone()
        }
    );
    session.on_frame(
        &shared,
        Frame::new(
            FrameKind::Report,
            stop.id,
            serde_json::to_vec(&FromHelper::Deactivated {
                plugin: plugin.clone(),
                state: Some(r#"{"seen":1}"#.to_owned()),
            })
            .expect("it serializes"),
        ),
    );
    let start = next_frame(&written);
    let ToHelper::Activate {
        plugin: started,
        load: Some(load),
    } = message(&start)
    else {
        panic!("the reloaded plugin is activated with its module");
    };
    assert_eq!(started, plugin);
    assert_eq!(load.state.as_deref(), Some(r#"{"seen":1}"#));
    let installed = shared
        .registry()
        .entry(&plugin)
        .and_then(|entry| entry.module.clone())
        .expect("a module");
    assert_eq!(
        load.wasm, installed.path,
        "what runs is the file the folder holds now"
    );
}

#[test]
fn a_typing_indicator_is_held_for_a_chain_and_goes_when_it_answers() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), TYPING_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let held = hold(
        &mut session,
        &shared,
        7,
        HookKind::TypingPreSend,
        typing("room-1"),
        "room-1",
    )
    .expect("a chain holds the indicator");
    let activate = next_frame(&written);
    assert!(matches!(
        message(&activate),
        ToHelper::Activate { plugin: started, .. } if started == plugin
    ));
    let hook = next_frame(&written);
    let ToHelper::Hook { chain, request } = message(&hook) else {
        panic!("the second frame is the chain");
    };
    assert_eq!(chain, vec![plugin.clone()]);
    assert_eq!(request.hook, HookKind::TypingPreSend);
    assert_eq!(
        request.subject,
        HookSubject::Typing(TypingPending {
            room: "room-1".to_owned()
        }),
        "an indicator has a room and nothing to change"
    );

    assert!(
        session
            .on_frame(&shared, hook_done(hook.id, HookOutcome::Unchanged, None))
            .is_none()
    );
    assert_eq!(
        notices
            .recv_timeout(Duration::from_secs(2))
            .expect("a notice"),
        Notice::Typing {
            id: held,
            room: "room-1".to_owned()
        },
        "the application sends the indicator the chain let go"
    );
}

#[test]
fn a_plugin_that_may_hide_typing_withholds_the_indicator() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), TYPING_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    hold(
        &mut session,
        &shared,
        8,
        HookKind::TypingPreSend,
        typing("room-1"),
        "room-1",
    )
    .expect("a chain holds it");
    let activate = next_frame(&written);
    assert!(matches!(message(&activate), ToHelper::Activate { .. }));
    let hook = next_frame(&written);
    assert!(matches!(message(&hook), ToHelper::Hook { .. }));

    // `typing.intercept` is the grant: the plugin is in this chain because
    // the person said it may hide the indicator, so its refusal is honoured
    // without `on_failure: "block"`, which is about messages.
    assert!(
        session
            .on_frame(
                &shared,
                hook_done(
                    hook.id,
                    HookOutcome::Block {
                        reason: "typing is off".to_owned(),
                    },
                    Some(plugin),
                ),
            )
            .is_none()
    );
    assert!(
        notices.recv_timeout(Duration::from_millis(50)).is_err(),
        "a withheld indicator is not sent, and the person isn't told"
    );
}

#[test]
fn a_file_is_held_for_the_upload_chain_and_comes_back_renamed() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), UPLOAD_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let pending = AttachmentPending {
        room: "room-1".to_owned(),
        name: "cat.JPG".to_owned(),
        bytes: 2_048,
        content_type: Some("image/jpeg".to_owned()),
    };
    let held = hold(
        &mut session,
        &shared,
        9,
        HookKind::AttachmentPreUpload,
        HookSubject::Attachment(pending.clone()),
        "room-1",
    )
    .expect("a chain holds the file");
    let activate = next_frame(&written);
    assert!(matches!(message(&activate), ToHelper::Activate { .. }));
    let hook = next_frame(&written);
    let ToHelper::Hook { chain, request } = message(&hook) else {
        panic!("the second frame is the chain");
    };
    assert_eq!(chain, vec![plugin.clone()]);
    assert_eq!(request.subject, HookSubject::Attachment(pending.clone()));

    let renamed = AttachmentPending {
        name: "cat.jpg".to_owned(),
        ..pending
    };
    assert!(
        session
            .on_frame(
                &shared,
                hook_done(
                    hook.id,
                    HookOutcome::Changed(HookSubject::Attachment(renamed)),
                    Some(plugin),
                ),
            )
            .is_none()
    );
    assert_eq!(
        notices
            .recv_timeout(Duration::from_secs(2))
            .expect("a notice"),
        Notice::Attachment {
            id: held,
            name: Some("cat.jpg".to_owned()),
            blocked: None,
        }
    );
}

#[test]
fn activity_pre_publish_is_the_one_hook_this_build_doesnt_dispatch() {
    // Yapfast publishes an activity — the game it detects, merged into its
    // own presence in `yapfast-discord`'s `show_presence` — but the plugin
    // bridge lives here and that publish is the service's, so no call
    // reaches the hook. The doc says so; this holds the list to the truth,
    // and adding a dispatch fails it until the list and the doc agree.
    let missing: Vec<HookKind> = HookKind::ALL
        .into_iter()
        .filter(|hook| !HookKind::DISPATCHED.contains(hook))
        .collect();
    assert_eq!(
        missing,
        vec![HookKind::ActivityPrePublish],
        "every hook but activity_pre_publish reaches a chain"
    );
}
