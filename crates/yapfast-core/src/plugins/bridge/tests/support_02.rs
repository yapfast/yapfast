//! The harness the tests in this directory share.

use super::*;

/// A plugin that may read pictures, and nothing else.
pub(super) const READER_MANIFEST: &str = r#"{"api":1,"id":"com.example.reader","name":"Reader",
        "version":"1.0.0","wasm":"plugin.wasm",
        "permissions":{"capabilities":["media.read"]}}"#;

/// A picture store with one picture in it, which is what `media.get` reads.
pub(super) struct HeldPicture;

impl crate::plugins::media::Pictures for HeldPicture {
    fn bytes(&self, url: &str) -> Option<Vec<u8>> {
        (url == "https://cdn.example.test/a.png").then(|| b"hello".to_vec())
    }
}

/// A plugin that watches who is around and who is talking, which is what
/// the two coalesced taps are for.
pub(super) const WATCHER_MANIFEST: &str = r#"{"api":1,"id":"com.example.watcher","name":"Watcher",
        "version":"1.0.0","wasm":"plugin.wasm",
        "permissions":{"capabilities":["people.read","voice.read"]},
        "activation":{"on":["event:presence","event:speaking"]}}"#;

/// A manifest that may reach the network and names no host, so every
/// request it makes is a question for the person.
pub(super) const CONSENT_MANIFEST: &str = r#"{"api":1,"id":"com.example.fetcher","name":"Fetcher",
        "version":"1.0.0","wasm":"plugin.wasm",
        "permissions":{"capabilities":["network"],"network":[]}}"#;

/// A transport that answers 200 with one piece of body and no socket.
pub(super) struct Answered200;

impl Transport for Answered200 {
    fn send(&self, plan: ConnectPlan) -> BoxFuture<'static, Result<Served>> {
        Box::pin(async move {
            Ok(Served {
                status: 200,
                headers: Vec::new(),
                url: format!("https://{}{}", plan.host, plan.path),
                body: stream::once(async { Ok(b"hi".to_vec()) }).boxed(),
            })
        })
    }
}

/// A resolver that gives every name one public address.
pub(super) struct Anywhere;

impl Resolver for Anywhere {
    fn resolve(&self, _host: String, _port: u16) -> BoxFuture<'static, Result<Vec<IpAddr>>> {
        Box::pin(async move { Ok(vec![IpAddr::from([93, 184, 216, 34])]) })
    }
}

/// The gate the application installs, with the transport and resolver above,
/// and this plugin's manifest declared to it.
pub(super) fn install_gate(shared: &Shared, directory: &Path, plugin: &PluginId) -> Arc<Grants> {
    let grants = Arc::new(Grants::load(directory.join("grants.json")).expect("a grants file"));
    let gate = Arc::new(
        NetworkGate::new(
            Arc::clone(&grants),
            Arc::new(Answered200),
            directory.join("plugins-http"),
        )
        .with_resolver(Arc::new(Anywhere)),
    );
    *shared.gate.lock().unwrap_or_else(PoisonError::into_inner) = Some(gate);
    shared.declare(plugin);
    grants
}

/// Answers a question the way the application does: the answer is kept by
/// the grants first, and only then are the requests waiting on it let go.
pub(super) fn answer_about(
    grants: &Grants,
    shared: &Shared,
    plugin: &PluginId,
    host: &str,
    allow: bool,
) -> usize {
    let pattern = HostPattern::parse(host).expect("a test host");
    let lasts = crate::plugins::grants::Lasts::Session;
    if allow {
        grants.allow(plugin, &pattern, lasts).expect("an allow");
        shared.answer_held(plugin, host, Answered::Allowed)
    } else {
        grants.deny(plugin, &pattern, lasts).expect("a deny");
        let reason = format!("you refused {host} for {plugin}");
        shared.answer_held(plugin, host, Answered::Denied(reason))
    }
}

/// One `http.fetch` call, as the helper hands one over.
pub(super) fn fetch_call(url: &str) -> HostCall {
    HostCall::new(call::method::HTTP_FETCH, HttpRequest::get(url)).expect("a call")
}

/// A plugin that draws in the interface and needs no capability for it.
pub(super) const PAINTER_MANIFEST: &str = r#"{"api":1,"id":"com.example.painter","name":"Painter",
        "version":"1.0.0","wasm":"plugin.wasm",
        "permissions":{"capabilities":["ui.modify"]}}"#;

/// One `ui.set_slot` call, as the helper hands one over.
pub(super) fn slot_call(slot: &str, tree: serde_json::Value) -> HostCall {
    HostCall::new(
        call::method::UI_SET_SLOT,
        serde_json::json!({ "slot": slot, "tree": tree }),
    )
    .expect("a call")
}

/// The tree of one text cell.
pub(super) fn text_tree(text: &str) -> serde_json::Value {
    serde_json::json!({ "rows": [{ "cells": [{ "text": text }] }] })
}

/// How many of `plugin`'s requests for `host` are waiting on a dialog.
pub(super) fn waiting(shared: &Shared, plugin: &PluginId, host: &str) -> usize {
    shared
        .held
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&(plugin.clone(), host.to_owned()))
        .map_or(0, Vec::len)
}

/// Waits until `count` requests for `(plugin, host)` are held. The task that
/// holds one does so just after it announces, so a test waits for the state
/// rather than for the notice.
pub(super) fn wait_held(shared: &Shared, plugin: &PluginId, host: &str, count: usize) {
    for _ in 0..400 {
        if waiting(shared, plugin, host) == count {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!(
        "{count} requests for {host} should be waiting, and {} are",
        waiting(shared, plugin, host)
    );
}

/// The answer a frame carries back to the guest.
pub(super) fn returned(frame: &Frame) -> (u64, HostReturn) {
    let ToHelper::Return { call, answer, .. } = message(frame) else {
        panic!("a host call is answered with a return");
    };
    (call, answer)
}

/// A manifest for a plugin that declares the base plugin as a dependency, so
/// it may call it.
pub(super) const BASE_MANIFEST: &str = r#"{"api":1,"id":"com.example.base","name":"Base","version":"1.0.0",
        "wasm":"plugin.wasm",
        "permissions":{"capabilities":[]}}"#;

pub(super) const TOP_MANIFEST: &str = r#"{"api":1,"id":"com.example.top","name":"Top","version":"1.0.0",
        "wasm":"plugin.wasm",
        "dependencies":{"com.example.base":"^1"},
        "permissions":{"capabilities":[]}}"#;

/// A `plugin.call` host call, as a guest builds one.
pub(super) fn call_request(to: &PluginId, function: &str) -> HostCall {
    HostCall::new(
        call::method::PLUGIN_CALL,
        PluginCall {
            plugin: to.clone(),
            function: function.to_owned(),
            args_base64: "aGk=".to_owned(),
        },
    )
    .expect("a call payload")
}

/// A folder a plugin's author has open: its manifest, and a module the
/// manifest names. `module` is the file's bytes, so a test can change it.
pub(super) fn plugin_folder(manifest: &str, module: &[u8]) -> tempfile::TempDir {
    let folder = tempfile::tempdir().expect("a folder");
    std::fs::write(folder.path().join("yapfast-plugin.json"), manifest).expect("a manifest");
    std::fs::write(folder.path().join("plugin.wasm"), module).expect("a module");
    folder
}

/// A manifest for a plugin that may withhold typing, and one that may
/// rename an upload: each needs the capability its hook names.
pub(super) const TYPING_MANIFEST: &str = r#"{"api":1,"id":"com.example.typing","name":"Typing","version":"1.0.0",
        "wasm":"plugin.wasm",
        "permissions":{"capabilities":["typing.intercept"]}}"#;

pub(super) const UPLOAD_MANIFEST: &str = r#"{"api":1,"id":"com.example.upload","name":"Upload","version":"1.0.0",
        "wasm":"plugin.wasm",
        "permissions":{"capabilities":["uploads.intercept"]}}"#;

/// A plugin that draws under messages on screen.
pub(super) const DECORATOR_MANIFEST: &str = r#"{"api":1,"id":"com.example.decorator","name":"Decorator",
        "version":"1.0.0","wasm":"plugin.wasm",
        "activation":{"on":["decorate"]},
        "permissions":{"capabilities":[]}}"#;

/// A plugin that draws under messages and may hide them, which needs
/// `messages.filter` for the hiding half.
pub(super) const FILTER_MANIFEST: &str = r#"{"api":1,"id":"com.example.filter","name":"Filter",
        "version":"1.0.0","wasm":"plugin.wasm",
        "activation":{"on":["decorate"]},
        "permissions":{"capabilities":["messages.filter"]}}"#;

/// A plugin that may send messages, and may write the composer.
pub(super) const SENDER_MANIFEST: &str = r#"{"api":1,"id":"com.example.sender","name":"Sender",
        "version":"1.0.0","wasm":"plugin.wasm",
        "permissions":{"capabilities":["messages.send"]}}"#;

/// A plugin that says how people look, which needs `people.read`.
pub(super) const FACES_MANIFEST: &str = r#"{"api":1,"id":"com.example.faces","name":"Faces",
        "version":"1.0.0","wasm":"plugin.wasm",
        "activation":{"on":["identity"]},
        "permissions":{"capabilities":["people.read"]}}"#;

/// A plugin that colours code blocks, which needs `messages.read`.
pub(super) const COLOURER_MANIFEST: &str = r#"{"api":1,"id":"com.example.colourer","name":"Colour",
        "version":"1.0.0","wasm":"plugin.wasm",
        "activation":{"on":["code"]},
        "permissions":{"capabilities":["messages.read"]}}"#;

/// The reply a chain sends for the request it was handed: the helper's own
/// frame id is the one the parent is waiting under.
pub(super) fn hook_done(id: u64, outcome: HookOutcome, by: Option<PluginId>) -> Frame {
    Frame::new(
        FrameKind::Reply,
        id,
        serde_json::to_vec(&FromHelper::HookDone {
            outcome,
            failures: Vec::new(),
            by,
        })
        .expect("it serializes"),
    )
}

/// Asks one plugin what it draws under a message, and answers the frames the
/// helper would write, so a test can get at the reply's id.
pub(super) fn decorate_ask(
    session: &mut Session,
    shared: &Arc<Shared>,
    written: &std::sync::mpsc::Receiver<Frame>,
    plugin: &PluginId,
) -> u64 {
    runtime().block_on(session.on_command(
        shared,
        Request::Decorate {
            plugin: plugin.clone(),
            request: yapfast_plugin_api::message::DecorateRequest {
                room: "room-1".to_owned(),
                message: "message-1".to_owned(),
                author: "person-1".to_owned(),
                body: "hi".to_owned(),
                own: false,
            },
        },
    ));
    let activate = next_frame(written);
    assert!(matches!(message(&activate), ToHelper::Activate { .. }));
    let ask = next_frame(written);
    assert!(matches!(message(&ask), ToHelper::Decorate { .. },));
    ask.id
}
