//! From `a_plugin_says_how_a_person_looks_once_for_the_pair` onward: part 10 of 11.

use super::*;

#[test]
fn a_plugin_says_how_a_person_looks_once_for_the_pair() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), FACES_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let person = yapfast_plugin_api::identity::IdentityRequest {
        person: "person-1".to_owned(),
        server: Some("server-1".to_owned()),
        name: "Ada".to_owned(),
        username: Some("ada".to_owned()),
        own: false,
    };
    runtime().block_on(session.on_command(
        &shared,
        Request::Identity {
            plugin: plugin.clone(),
            request: person.clone(),
        },
    ));

    // The instance comes up first, then the pair is asked about.
    let activate = next_frame(&written);
    assert!(matches!(
        message(&activate),
        ToHelper::Activate { plugin: started, .. } if started == plugin
    ));
    let ask = next_frame(&written);
    let ToHelper::Identity {
        plugin: asked,
        request,
    } = message(&ask)
    else {
        panic!("the second frame is the question about a person");
    };
    assert_eq!(asked, plugin);
    assert_eq!(request, person, "the pair travels as the plugin reads it");

    let identity = yapfast_plugin_api::identity::Identity {
        display_name: Some("Ada Lovelace".to_owned()),
        avatar: Some("http://example.test/a.png".to_owned()),
        ..Default::default()
    };
    assert!(
        session
            .on_frame(
                &shared,
                Frame::new(
                    FrameKind::Report,
                    ask.id,
                    serde_json::to_vec(&FromHelper::Identity {
                        plugin: plugin.clone(),
                        person: "person-1".to_owned(),
                        server: Some("server-1".to_owned()),
                        identity: Some(identity.clone()),
                    })
                    .expect("it serializes"),
                ),
            )
            .is_none()
    );
    let notice = notices
        .recv_timeout(Duration::from_secs(2))
        .expect("a notice");
    let Notice::Identity {
        plugin: said_by,
        person: said_about,
        server,
        rank,
        identity: answered,
    } = notice
    else {
        panic!("an answer about a person is its own notice: {notice:?}");
    };
    assert_eq!(said_by, plugin);
    assert_eq!(said_about, "person-1");
    assert_eq!(server.as_deref(), Some("server-1"));
    assert_eq!(rank, 0);
    assert_eq!(answered, Some(identity));
}

#[test]
fn a_plugin_that_may_not_read_people_is_never_asked_how_they_look() {
    // The trigger without `people.read` leaves the summary flag off, so the
    // surfaces ask nobody; the registry says why, and the bridge agrees.
    let directory = tempfile::tempdir().expect("a temporary directory");
    installed(
        directory.path(),
        r#"{"api":1,"id":"com.example.blind","name":"Blind",
                "version":"1.0.0","wasm":"plugin.wasm",
                "activation":{"on":["identity"]}}"#,
        b"not a real module",
    );
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let mut bridge = Bridge {
        data_dir: directory.path().to_owned(),
        host: host(),
        fetches: Vec::new(),
        shared,
        supervisor: None,
        next_send: 0,
    };
    assert!(!bridge.identifies());
    bridge.ask_identities(&[yapfast_plugin_api::identity::IdentityRequest {
        person: "person-1".to_owned(),
        server: None,
        name: "Ada".to_owned(),
        username: None,
        own: false,
    }]);
}

#[test]
fn a_send_without_a_gesture_names_the_capability_it_needs() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), SENDER_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    let call = HostCall::new(
        call::method::MESSAGES_SEND,
        SendMessage {
            room: "room-1".to_owned(),
            body: "hi".to_owned(),
        },
    )
    .expect("a call payload");
    runtime().block_on(async { session.on_call(&shared, 7, plugin, call) });

    let (id, answer) = returned(&next_frame(&written));
    assert_eq!(id, 7);
    let HostReturn::Denied { reason } = answer else {
        panic!("a send nobody asked for is refused");
    };
    assert!(reason.contains("messages.send.background"), "{reason}");
}

#[test]
fn a_send_the_person_just_acted_for_goes_to_the_application() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), SENDER_MANIFEST, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    // The person typed a moment ago, which is the gesture a send stands
    // behind.
    shared.gesture();
    let call = HostCall::new(
        call::method::MESSAGES_SEND,
        SendMessage {
            room: "room-1".to_owned(),
            body: "hi".to_owned(),
        },
    )
    .expect("a call payload");
    runtime().block_on(async { session.on_call(&shared, 8, plugin.clone(), call) });

    let notice = notices
        .recv_timeout(Duration::from_secs(2))
        .expect("a notice");
    let Notice::Call {
        plugin: asked_by,
        call: answered_under,
        request,
    } = notice
    else {
        panic!("a send is the application's to do: {notice:?}");
    };
    assert_eq!(asked_by, plugin);
    assert_eq!(answered_under, 8);
    assert_eq!(
        request,
        AppCall::Send {
            room: "room-1".to_owned(),
            body: "hi".to_owned()
        }
    );
    assert!(
        written.recv_timeout(Duration::from_millis(50)).is_err(),
        "the guest waits for the application's answer, not this thread's"
    );
}

#[test]
fn six_sends_in_ten_seconds_are_one_too_many() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), SENDER_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    shared.gesture();
    for call in 1..=6u64 {
        let request = HostCall::new(
            call::method::MESSAGES_SEND,
            SendMessage {
                room: "room-1".to_owned(),
                body: format!("{call}"),
            },
        )
        .expect("a call payload");
        runtime().block_on(async { session.on_call(&shared, call, plugin.clone(), request) });
    }
    // The first five went to the application and wrote nothing; the sixth
    // is refused here, which is the frame this sees.
    let (id, answer) = returned(&next_frame(&written));
    assert_eq!(id, 6);
    let HostReturn::Denied { reason } = answer else {
        panic!("the sixth send in ten seconds is refused");
    };
    assert!(reason.contains("5 messages"), "{reason}");
}

#[test]
fn nothing_holds_what_no_plugin_intercepts() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    // This one intercepts sends, and neither typing nor uploads.
    installed(directory.path(), HOOK_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();

    assert!(
        hold(
            &mut session,
            &shared,
            10,
            HookKind::TypingPreSend,
            typing("room-1"),
            "room-1",
        )
        .is_none()
    );
    assert!(
        hold(
            &mut session,
            &shared,
            11,
            HookKind::AttachmentPreUpload,
            HookSubject::Attachment(AttachmentPending {
                room: "room-1".to_owned(),
                name: "a.txt".to_owned(),
                bytes: 1,
                content_type: None,
            }),
            "room-1",
        )
        .is_none()
    );
    assert!(
        written.recv_timeout(Duration::from_millis(50)).is_err(),
        "nothing is asked of a helper when nothing intercepts"
    );
}
