//! The hooks the interface asks: a link's offer to open it in an app, the
//! quick switcher's order, and the words after the window's title. Part 14.

use super::*;
use yapfast_plugin_api::hook::{LinkOffer, LinkPending, SwitcherPending, SwitcherPlace};

const OFFER_MANIFEST: &str = r#"{"api":1,"id":"com.example.openinapp","name":"Open In App",
        "version":"1.0.0","wasm":"plugin.wasm",
        "activation":{"on":["event:app_focus"]},
        "permissions":{"capabilities":["links.offer"],"link-hosts":["open.spotify.com"],
                       "schemes":["spotify"]}}"#;

/// Runs one held hook through a session and answers it as the helper would,
/// returning the notice the bridge announced, if any.
fn answer_hook(
    manifest: &str,
    hook: HookKind,
    subject: HookSubject,
    switcher: Option<SwitcherPending>,
    answer: HookOutcome,
) -> Option<Notice> {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), manifest, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, written, _inbound) = session_stub();
    let body = match &subject {
        HookSubject::Link(link) => link.url.clone(),
        _ => String::new(),
    };
    let held = HeldSend {
        id: 21,
        draft: Draft {
            room: String::new(),
            body,
            reply_to: None,
            attachments: Vec::new(),
        },
        hook,
        chain: vec![plugin.clone()],
        deadline: Instant::now() + Duration::from_millis(500),
        switcher,
    };
    runtime().block_on(session.on_command(
        &shared,
        Request::Hook {
            held,
            request: HookRequest { hook, subject },
        },
    ));
    let _activate = next_frame(&written);
    let frame = next_frame(&written);
    assert!(
        session
            .on_frame(&shared, hook_done(frame.id, answer, Some(plugin)))
            .is_none()
    );
    notices.recv_timeout(Duration::from_millis(300)).ok()
}

fn link(offer: Option<LinkOffer>) -> HookSubject {
    HookSubject::Link(LinkPending {
        url: "https://open.spotify.com/track/1".into(),
        offer,
    })
}

#[test]
fn an_offer_to_open_a_link_is_its_plugins_and_opens_a_listed_scheme() {
    let offer = |uri: &str| LinkOffer {
        label: "Open in Spotify".into(),
        uri: uri.into(),
    };
    let shown = answer_hook(
        OFFER_MANIFEST,
        HookKind::LinkOpen,
        link(None),
        None,
        HookOutcome::Changed(link(Some(offer("spotify:track:1")))),
    );
    match shown {
        Some(Notice::LinkOffer {
            id,
            plugin,
            label,
            uri,
        }) => {
            assert_eq!(id, 21);
            assert_eq!(plugin.as_str(), "com.example.openinapp");
            assert_eq!(label, "Open in Spotify");
            assert_eq!(uri, "spotify:track:1");
        }
        other => panic!("an offer, not {other:?}"),
    }
    // A scheme the manifest doesn't list is no offer.
    let refused = answer_hook(
        OFFER_MANIFEST,
        HookKind::LinkOpen,
        link(None),
        None,
        HookOutcome::Changed(link(Some(offer("steam://run/1")))),
    );
    assert!(refused.is_none(), "{refused:?}");
    // Nor is one that changed the link it was asked about.
    let moved = answer_hook(
        OFFER_MANIFEST,
        HookKind::LinkOpen,
        link(None),
        None,
        HookOutcome::Changed(HookSubject::Link(LinkPending {
            url: "https://evil.example/".into(),
            offer: Some(offer("spotify:track:1")),
        })),
    );
    assert!(moved.is_none(), "{moved:?}");
}

#[test]
fn only_a_link_to_a_listed_host_reaches_a_plugin() {
    assert_eq!(
        super::super::supervisor_hooks::link_host("https://user@Open.Spotify.com:443/x?y"),
        Some("open.spotify.com".to_owned())
    );
    assert_eq!(
        super::super::supervisor_hooks::link_host("spotify:track:1"),
        None
    );
    let directory = tempfile::tempdir().expect("a temporary directory");
    let _plugin = installed(directory.path(), OFFER_MANIFEST, b"not a real module");
    let (wake, _notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let registry = shared.registry();
    let chain = registry.chain(HookKind::LinkOpen);
    assert_eq!(chain.len(), 1, "links.offer puts it in the chain");
    let patterns = &registry
        .entry(&chain[0])
        .expect("installed")
        .manifest
        .permissions
        .link_hosts;
    assert!(
        patterns
            .iter()
            .any(|pattern| pattern.matches("open.spotify.com"))
    );
    assert!(
        !patterns
            .iter()
            .any(|pattern| pattern.matches("example.com"))
    );
}

const RANK_MANIFEST: &str = r#"{"api":1,"id":"com.example.rank","name":"Rank",
        "version":"1.0.0","wasm":"plugin.wasm",
        "activation":{"on":["event:app_focus"]},
        "permissions":{"capabilities":["switcher.rank"]}}"#;

fn places(query: &str, ids: &[&str]) -> SwitcherPending {
    SwitcherPending {
        query: query.into(),
        places: ids
            .iter()
            .map(|id| SwitcherPlace {
                id: (*id).into(),
                name: (*id).into(),
                kind: "channel".into(),
            })
            .collect(),
    }
}

#[test]
fn a_ranking_reorders_and_filters_what_it_was_shown_and_nothing_else() {
    let asked = places("ge", &["a", "b", "c"]);
    let ranked = answer_hook(
        RANK_MANIFEST,
        HookKind::SwitcherRank,
        HookSubject::Switcher(asked.clone()),
        Some(asked.clone()),
        HookOutcome::Changed(HookSubject::Switcher(places("ge", &["c", "a"]))),
    );
    match ranked {
        Some(Notice::SwitcherRanked { query, order, .. }) => {
            assert_eq!(query, "ge");
            assert_eq!(order, ["c", "a"]);
        }
        other => panic!("a ranking, not {other:?}"),
    }
    let added = answer_hook(
        RANK_MANIFEST,
        HookKind::SwitcherRank,
        HookSubject::Switcher(asked.clone()),
        Some(asked),
        HookOutcome::Changed(HookSubject::Switcher(places("ge", &["a", "z"]))),
    );
    assert!(added.is_none(), "a place it made up: {added:?}");
}

#[test]
fn the_title_takes_a_few_plain_words_with_window_title_and_none_without() {
    let manifest = r#"{"api":1,"id":"com.example.title","name":"Title","version":"1.0.0",
        "wasm":"plugin.wasm","permissions":{"capabilities":["window.title"]}}"#;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let plugin = installed(directory.path(), manifest, b"not a real module");
    let (wake, notices) = notice_channel();
    let shared = shared(directory.path(), wake);
    let (_session_directory, mut session, _written, _inbound) = session_stub();
    let title = |suffix: &str| HostCall {
        method: call::method::WINDOW_SET_TITLE.to_owned(),
        payload: serde_json::json!({ "suffix": suffix }),
    };
    let answer = session.answer(&shared, &plugin, title("3 unread"));
    assert!(matches!(answer, HostReturn::Ok(_)), "{answer:?}");
    assert_eq!(
        notices
            .recv_timeout(Duration::from_secs(1))
            .expect("a notice"),
        Notice::WindowTitle {
            plugin: plugin.clone(),
            suffix: Some("3 unread".into())
        }
    );
    assert!(matches!(
        session.answer(&shared, &plugin, title(&"x".repeat(40))),
        HostReturn::Error { .. }
    ));
    assert!(matches!(
        session.answer(&shared, &plugin, title("a\u{202e}b")),
        HostReturn::Error { .. }
    ));

    let directory = tempfile::tempdir().expect("a temporary directory");
    let quiet = installed(
        directory.path(),
        r#"{"api":1,"id":"com.example.quiet","name":"Quiet","version":"1.0.0",
            "wasm":"plugin.wasm","permissions":{"capabilities":["storage"]}}"#,
        b"not a real module",
    );
    let (wake, _notices) = notice_channel();
    let shared = super::support_01::shared(directory.path(), wake);
    let refused = session.answer(&shared, &quiet, title("hi"));
    match refused {
        HostReturn::Denied { reason } => assert!(reason.contains("window.title"), "{reason}"),
        other => panic!("refused, not {other:?}"),
    }
}

/// The two samples' manifests are ones this build reads whole: every change
/// the Extras sample makes to the interface applies, and both name only what
/// they use.
#[test]
fn the_samples_manifests_read_and_their_interface_changes_all_apply() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    for sample in ["extras", "socket-echo"] {
        let text = std::fs::read_to_string(root.join(sample).join("yapfast-plugin.json"))
            .expect("the sample's manifest");
        PluginManifest::parse(&text).unwrap_or_else(|error| panic!("{sample}: {error}"));
        let manifest =
            crate::Manifest::parse(&text).unwrap_or_else(|error| panic!("{sample}: {error}"));
        let checked = crate::ui::check(&manifest);
        assert!(
            checked.refused.is_empty(),
            "{sample}: {:?}",
            checked.refused
        );
    }
    let extras = PluginManifest::parse(
        &std::fs::read_to_string(root.join("extras/yapfast-plugin.json")).unwrap(),
    )
    .unwrap();
    for capability in [
        Capability::LinksOffer,
        Capability::SwitcherRank,
        Capability::WindowTitle,
        Capability::AppTiming,
        Capability::RelationshipsRead,
        Capability::ImagesWeb,
    ] {
        assert!(extras.permissions.allows(capability), "{capability}");
    }
}
