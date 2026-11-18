use super::*;

fn launch(token: Option<&str>, link: Option<&str>) -> Launch {
    Launch {
        token: token.map(str::to_owned),
        link: link.map(str::to_owned),
    }
}

fn opened(instance: &mut Instance) -> std::sync::mpsc::Receiver<Launch> {
    let (sender, receiver) = std::sync::mpsc::channel();
    instance.listen(move |launch| {
        let _ = sender.send(launch);
    });
    receiver
}

#[test]
fn launch_lines_carry_tokens_and_links() {
    let with_link = launch(
        Some("token-1"),
        Some("https://discord.com/channels/10/11/12"),
    );
    assert_eq!(
        launch_line(&with_link),
        "open token-1 https://discord.com/channels/10/11/12"
    );
    assert_eq!(launch_from_line(&launch_line(&with_link)), Some(with_link));
    assert_eq!(
        launch_from_line("open token-1"),
        Some(launch(Some("token-1"), None))
    );
    assert_eq!(
        launch_from_line("open - yapfast://channels/@me/20"),
        Some(launch(None, Some("yapfast://channels/@me/20")))
    );
    assert_eq!(launch_from_line("open"), Some(Launch::default()));
    // A start writes a link with a space as one word, as the core encodes
    // it, which arrives whole.
    let spaced = launch(None, Some("https://discord.com/channels/10/11?a%20b"));
    assert_eq!(launch_from_line(&launch_line(&spaced)), Some(spaced));
    assert_eq!(launch_from_line("open one two three"), None);
    assert_eq!(launch_from_line("close token-1"), None);
}

#[test]
fn queued_links_are_taken_in_order_and_cleared() {
    let root = tempfile::tempdir().unwrap();
    queue_link(root.path(), "https://discord.com/channels/10/11").unwrap();
    queue_link(root.path(), "https://discord.com/users/99").unwrap();
    assert_eq!(
        drain_link_queue(root.path()),
        [
            "https://discord.com/channels/10/11",
            "https://discord.com/users/99"
        ]
    );
    assert!(drain_link_queue(root.path()).is_empty());
    queue_link(root.path(), "https://discord.gg/stale").unwrap();
    clear_link_queue(root.path());
    assert!(drain_link_queue(root.path()).is_empty());
}

#[test]
fn a_later_start_shows_the_first_window() {
    let root = tempfile::tempdir().unwrap();
    let Start::Run(mut first) = claim_with(root.path(), true, false, Launch::default()) else {
        panic!("the first start runs");
    };
    let requests = opened(&mut first);
    assert!(matches!(
        claim_with(
            root.path(),
            true,
            false,
            launch(Some("token-1"), Some("https://discord.com/channels/10/11"))
        ),
        Start::Handed
    ));
    let wait = Duration::from_secs(5);
    assert_eq!(
        requests.recv_timeout(wait).unwrap(),
        launch(Some("token-1"), Some("https://discord.com/channels/10/11"))
    );
    assert!(matches!(
        claim_with(root.path(), true, false, Launch::default()),
        Start::Handed
    ));
    assert_eq!(requests.recv_timeout(wait).unwrap(), Launch::default());
    // Starting at sign-in leaves the running one as it is.
    assert!(matches!(
        claim_with(root.path(), true, true, Launch::default()),
        Start::Handed
    ));
    // Allowed, another runs beside the first, without listening.
    let Start::Run(another) = claim_with(root.path(), false, false, Launch::default()) else {
        panic!("allowed, a later start runs");
    };
    assert!(another._lock.is_none() && another.listener.is_none());
    assert!(requests.recv_timeout(Duration::from_millis(200)).is_err());
    // Once the first ends, the next start is the first.
    drop(first);
    assert!(matches!(
        claim_with(root.path(), true, false, Launch::default()),
        Start::Run(Instance {
            _lock: Some(_),
            listener: Some(_)
        })
    ));
}
