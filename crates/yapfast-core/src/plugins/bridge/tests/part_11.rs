//! From `every_curated_action_needs_its_capability_and_then_reaches_the_application` onward: part 11 of 11.

use super::*;

/// M9's exit for the curated actions: each needs the capability §9.1 gives
/// it, and with it the application is asked to do it.
#[test]
fn every_curated_action_needs_its_capability_and_then_reaches_the_application() {
    for (method, payload, capability, expected) in curated_actions() {
        let (answer, notices) = ask_action(method, payload.clone(), &[]);
        match capability {
            Some(capability) => {
                let HostReturn::Denied { reason } = answer else {
                    panic!("{method} should need `{capability}`: {answer:?}");
                };
                assert!(
                    reason.contains(capability.as_str()),
                    "{method} names the capability it needs: {reason}"
                );
                assert!(
                    notices.recv_timeout(Duration::from_millis(50)).is_err(),
                    "{method} reaches no one without `{capability}`"
                );
            }
            None => assert!(
                matches!(answer, HostReturn::Pending { .. }),
                "navigation needs no capability: {method} {answer:?}"
            ),
        }

        let declared: Vec<&str> = capability.map(|c| vec![c.as_str()]).unwrap_or_default();
        let (answer, notices) = ask_action(method, payload, &declared);
        assert!(
            matches!(answer, HostReturn::Pending { .. }),
            "with its capability {method} waits for the application: {answer:?}"
        );
        let notice = notices
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|_| panic!("{method} should reach the application"));
        let Notice::Call { request, .. } = notice else {
            panic!("{method} asks the application: {notice:?}");
        };
        assert_eq!(request, expected, "{method}");
    }
}

/// §9.1's rule for navigation: it needs no capability, and it does need a
/// person's own hand behind it.
#[test]
fn a_navigation_action_without_a_gesture_is_refused() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(
        directory.path(),
        &action_manifest(&[]),
        b"not a real module",
    );
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let call = HostCall::new(
        call::method::DISCORD_OPEN_ROOM,
        RoomGet {
            room: "room-1".to_owned(),
        },
    )
    .expect("a call payload");
    runtime().block_on(async { session.on_call(&shared, 11, plugin, call) });
    let (id, answer) = returned(&next_frame(&written));
    assert_eq!(id, 11);
    let HostReturn::Denied { reason } = answer else {
        panic!("opening a room with nobody there is refused: {answer:?}");
    };
    assert!(reason.contains("click"), "{reason}");
    assert!(
        notices.recv_timeout(Duration::from_millis(50)).is_err(),
        "the window doesn't move"
    );
}
