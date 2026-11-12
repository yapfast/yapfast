//! The §14 security rules that are about a process and its state rather than
//! about the network gate: a run started with `--no-plugins` starts nothing, a
//! plugin turned off takes away every part of the interface it changed, and a
//! symbolic link in a fetched repository is refused instead of followed.
//!
//! They are their own file rather than additions to `plugins_gate.rs` because
//! that file is the gate's decisions, made without a store, a process or an
//! application in the way: two of these need the application's own state, and one
//! needs the real `yapfast-plugins` and `git`. Keeping them here leaves the
//! gate's tests runnable with neither.
//!
//! ```sh
//! nix develop path:./nix --command cargo build --profile fast -p yapfast-plugin-host
//! nix develop path:./nix --command cargo test --profile fast -p yapfast-core \
//!   --features plugins --test plugins_security
//! ```
//!
//! `YAPFAST_PLUGINS` names a helper built another way; without it, the helper
//! the build made in this test binary's own profile directory is used.

#![cfg(feature = "plugins")]

use std::path::{Path, PathBuf};
// The rules below that only Unix can express, and the tools only they use: a link
// checked into a repository is mode 120000 there, while Git on Windows writes one
// as a plain file unless it was cloned with symlinks. Gating what only they use
// keeps a Windows check from warning about a tool nothing there asks for.
#[cfg(unix)]
use std::process::Command;
// Ungated, because the control in `a_run_started_with_no_plugins_starts_nothing`
// takes this lock on every platform; only the guard's type is Unix's, since only
// the unix-only `helper()` names it.
#[cfg(unix)]
use std::sync::MutexGuard;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use yapfast_core::application::Application;
use yapfast_core::application::Effect as AppEffect;
use yapfast_core::application::plugins::{Effect as PluginEffect, News};
use yapfast_core::chat::Event;
use yapfast_core::cli;
use yapfast_core::plugins;
use yapfast_core::plugins::bridge::HelperState;
#[cfg(unix)]
use yapfast_core::plugins::installing;
use yapfast_core::plugins::slots::{Badge, Hidden, Output, Slot};
use yapfast_core::plugins::state::Notice;
use yapfast_core::plugins::store::{Package, Store};
use yapfast_core::plugins::ui::{PluginName, Tree};
use yapfast_plugin_api::PluginId;

/// The two plugins the interface is told about, as the page lists them.
const ONE: &str = "dev.local.one";
const TWO: &str = "dev.local.two";

/// How long the control is given to start the helper: a process, a pipe and the
/// module loaded into it, as `plugins_runtime.rs` allows for the same start.
const START: Duration = Duration::from_secs(30);

/// The process's own state, one test at a time: `YAPFAST_PLUGINS`, which the
/// helper is started from, and the flag [`cli::parse_no_plugins`] records.
///
/// `setenv` while another thread reads the environment is the data race the
/// standard library marks it as, and the flag is one process-wide answer, so
/// both are taken under this.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// `--no-plugins` reads nothing and starts nothing: the flag is taken out of the
/// arguments before anything is opened, and the application that would own a
/// helper's process is left without one.
///
/// The same store without the flag is the control, so what this proves is that
/// the flag and not an empty store is what kept the plugin system shut. It is the
/// state that owns the helper rather than `pgrep` that is asked: the field's
/// absence is the application's own way of saying no process exists, and a scan
/// of the machine's processes would count the ones other test binaries are
/// running. The control waits for the helper it starts, so "starts nothing" is
/// proved against a run that did start one.
#[test]
fn a_run_started_with_no_plugins_starts_nothing() {
    let _environment = ENVIRONMENT.lock().unwrap_or_else(PoisonError::into_inner);
    let directory = tempfile::tempdir().expect("a data directory");
    // A plugin that runs at startup: the one kind that would ask for a helper
    // without a person touching anything.
    install_startup(directory.path());

    // What `yapfast --no-plugins` does before the window exists to be asked.
    let mut arguments = vec!["yapfast".to_owned(), cli::NO_PLUGINS.to_owned()];
    assert!(
        cli::parse_no_plugins(&mut arguments),
        "this run asked for none"
    );
    assert_eq!(arguments, ["yapfast".to_owned()], "nothing else changed");
    assert!(!cli::plugins_allowed());
    assert!(
        plugins::safe_mode(),
        "the interface's half of the same answer"
    );

    let (events, _listener) = tokio::sync::mpsc::unbounded_channel();
    let mut app = Application::default();
    let effects = app.open_plugins(directory.path(), "0.3.0", events);
    assert!(
        effects.is_empty(),
        "nothing was read, so no plugin reports a status"
    );
    assert!(
        app.state.plugins.is_none(),
        "and there is no plugin state, which is the one thing that starts a helper"
    );

    // The control: the same store, in a run that asked for no such thing. What
    // it has to reach is a running helper, not merely a plugin system: with only
    // the `Option` read, this half would pass for a run whose helper was
    // `Missing`, which is a run that could start nothing and so says nothing
    // about what the flag took away.
    assert!(!cli::parse_no_plugins(&mut Vec::<String>::new()));
    assert!(cli::plugins_allowed());
    // The guard above is already held, so this is the way in rather than
    // `helper()`, which would take it a second time.
    point_at_helper();
    let (events, _listener) = tokio::sync::mpsc::unbounded_channel();
    let mut app = Application::default();
    app.open_plugins(directory.path(), "0.3.0", events);
    let state = app
        .state
        .plugins
        .as_mut()
        .expect("the store opens when the flag is absent, so the flag is what stopped it");
    wait_running(state);
    state.shutdown();
}

/// Turning a plugin off takes away everything it changed, and nothing another
/// plugin drew: the runtime reports the removal of that plugin's own parts, and
/// the application forgets its trees in every slot, what it drew under messages
/// and the badges it put on people.
///
/// The store changing under the instances — an install, an uninstall, a roll
/// back — takes everything away, and says so once.
#[test]
fn disabling_a_plugin_takes_away_every_part_it_changed() {
    let mut app = Application::default();
    app.enable_plugins(names());
    draw(&mut app, ONE);
    draw(&mut app, TWO);
    let slots = app
        .state
        .plugin_slots
        .as_ref()
        .expect("the plugins are known");
    assert_eq!(slots.drawn(Slot::Banner).count(), 2, "both drew a banner");
    assert_eq!(slots.decorations("m-1").count(), 2, "and under a message");
    assert_eq!(slots.badges("person-1").count(), 2, "and beside a person");
    let banners = slots.changes(Slot::Banner);

    // What the runtime reports when the plugin is turned off.
    let effects = app.apply(Event::Plugin(News::Notice(Notice::SlotsGone(
        PluginId::parse(ONE).expect("a plugin id"),
    ))));
    assert!(
        asked_to_redraw(&effects),
        "the interface is told the slots moved"
    );
    let slots = app
        .state
        .plugin_slots
        .as_ref()
        .expect("the plugins are known");
    let drawn: Vec<&str> = slots
        .drawn(Slot::Banner)
        .map(|drawn| slots.names()[drawn.plugin].id.as_str())
        .collect();
    assert_eq!(drawn, [TWO], "one plugin's tree, and not the other's");
    let decorated: Vec<&str> = slots
        .decorations("m-1")
        .map(|kept| slots.names()[kept.plugin].id.as_str())
        .collect();
    assert_eq!(decorated, [TWO], "its decorations go with it");
    let badged: Vec<&str> = slots
        .badges("person-1")
        .map(|(name, _)| name.id.as_str())
        .collect();
    assert_eq!(badged, [TWO], "and the badge it put on a person");
    assert!(
        slots.changes(Slot::Banner) > banners,
        "the slot changed, which is what redraws it"
    );

    // A store that changed under the instances takes all of it away.
    let effects = app.apply(Event::Plugin(News::Notice(Notice::SlotsReset)));
    assert!(asked_to_redraw(&effects));
    let slots = app
        .state
        .plugin_slots
        .as_ref()
        .expect("the plugins are known");
    assert_eq!(slots.drawn(Slot::Banner).count(), 0, "every tree is gone");
    assert_eq!(slots.decorations("m-1").count(), 0, "and every decoration");
    assert_eq!(slots.badges("person-1").count(), 0, "and every badge");

    // Nothing left is no redraw, from a reset or from a plugin that never drew.
    let again = app.apply(Event::Plugin(News::Notice(Notice::SlotsReset)));
    assert!(!asked_to_redraw(&again), "an empty reset draws nothing");
    let quiet = app.apply(Event::Plugin(News::Notice(Notice::SlotsGone(
        PluginId::parse("dev.local.nothing").expect("a plugin id"),
    ))));
    assert!(!asked_to_redraw(&quiet), "and neither does a quiet plugin");
}

/// A symbolic link in a fetched repository is refused, not followed: a plugin's
/// files are real ones, so nothing in a repository can point the helper at a file
/// outside it — which is what a link whose bytes are a path is for.
///
/// Both places a tree can hold one are asked: where a plugin's module should be,
/// and where a folder its manifest names should be. The other way a tree names a
/// file it shouldn't — `..` in a path — is tested in the helper's own tests; this
/// goes through the parent the way an install does, so the sentence checked here
/// is the one a person installing it would read.
///
/// The test is Unix's alone: a link checked into a repository is mode 120000
/// there, while Git on Windows writes one as a plain file unless it was cloned
/// with symlinks, so there would be no link to refuse.
#[cfg(unix)]
#[test]
fn a_symbolic_link_in_a_repository_is_refused() {
    let _environment = helper();
    let source = tempfile::tempdir().expect("a repository's directory");
    // Where a plugin's module should be.
    let echo = source.path().join("plugins/echo");
    write_plugin(&echo, "dev.local.echo", "plugin.wasm");
    std::os::unix::fs::symlink("../../../outside.wasm", echo.join("plugin.wasm"))
        .expect("a symbolic link where the module should be");
    // And a folder in the path its manifest names.
    let linked = source.path().join("plugins/linked");
    write_plugin(&linked, "dev.local.linked", "sub/plugin.wasm");
    std::os::unix::fs::symlink("../../../outside", linked.join("sub"))
        .expect("a symbolic link where a folder should be");
    commit(source.path());

    for (path, named) in [("plugins/echo", "plugin.wasm"), ("plugins/linked", "`sub`")] {
        let url = format!("file://{}#path={path}", source.path().display());
        let error = installing::fetch(&url).expect_err("the link is refused, not read");
        assert!(error.contains("symbolic link"), "{path}: {error}");
        assert!(error.contains(named), "{path} names {named}: {error}");
    }
}

/// Whether the application was asked to draw the slots again.
fn asked_to_redraw(effects: &[AppEffect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, AppEffect::Plugin(PluginEffect::Slots)))
}

/// The plugins the interface knows, which is what the store's manifests give it.
fn names() -> Vec<PluginName> {
    vec![
        PluginName {
            id: ONE.to_owned(),
            name: "One".to_owned(),
        },
        PluginName {
            id: TWO.to_owned(),
            name: "Two".to_owned(),
        },
    ]
}

/// Shows what one plugin draws: a tree in a slot, one under a message and a badge
/// beside a person's name — every part of the interface a plugin changes that
/// outlives the call that asked for it.
fn draw(app: &mut Application, plugin: &str) {
    let tree = Tree::parse(&format!(
        r#"{{ "rows": [{{ "cells": [{{ "text": "{plugin}" }}] }}] }}"#
    ))
    .expect("a tree");
    let badge = Badge::parse(&serde_json::json!({ "text": "Link" })).expect("a badge");
    for output in [
        Output::SetSlot {
            plugin: plugin.to_owned(),
            slot: Slot::Banner,
            tree: tree.clone(),
        },
        Output::Decorate {
            plugin: plugin.to_owned(),
            message: "m-1".to_owned(),
            tree: Some(tree),
            hide: Hidden::default(),
        },
        Output::MemberBadge {
            plugin: plugin.to_owned(),
            member: "person-1".to_owned(),
            badge: Some(badge),
        },
    ] {
        app.apply(Event::Plugin(News::Notice(Notice::Slot(output))));
    }
}

/// Installs a plugin that runs at startup into the store in `directory`, on.
fn install_startup(directory: &Path) {
    let manifest = format!(
        r#"{{
  "api": 1,
  "id": "{ONE}",
  "name": "One",
  "version": "0.1.0",
  "wasm": "plugin.wasm",
  "limits": {{ "memory_pages": 64 }},
  "activation": {{ "on": ["startup"] }},
  "permissions": {{ "capabilities": [], "network": [] }}
}}
"#
    );
    let module = std::fs::read(fixture("bad_answer.wasm")).expect("the fixture");
    let id = PluginId::parse(ONE).expect("a plugin id");
    let mut store = Store::open(directory).expect("a store");
    store
        .install(&Package {
            id: id.clone(),
            source: "https://example.org/ada/startup".to_owned(),
            path: String::new(),
            reference: "main".to_owned(),
            rev: "0".repeat(40),
            manifest,
            module,
            icon: None,
        })
        .expect("it installs");
    store.set_enabled(&id, true).expect("it is on");
}

/// Writes a plugin's folder: a manifest naming `wasm` as its module, and no
/// module of its own, which is what the links below stand in for.
#[cfg(unix)]
fn write_plugin(folder: &Path, id: &str, wasm: &str) {
    std::fs::create_dir_all(folder).expect("the plugin's folder");
    let manifest = format!(
        r#"{{
  "api": 1,
  "id": "{id}",
  "name": "Echo",
  "version": "0.1.0",
  "wasm": "{wasm}",
  "permissions": {{ "capabilities": [], "network": [] }}
}}
"#
    );
    std::fs::write(folder.join("yapfast-plugin.json"), manifest).expect("its manifest");
}

/// Initialises a repository in `directory` with everything in it committed, so a
/// `file://` fetch reads a commit rather than a working tree, as it does for a
/// person installing from a URL.
#[cfg(unix)]
fn commit(directory: &Path) {
    for arguments in [
        vec!["init", "--quiet", "--initial-branch", "main"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.org",
            "commit",
            "--quiet",
            "-m",
            "A plugin with a link in it",
        ],
    ] {
        let status = Command::new("git")
            .args(&arguments)
            .current_dir(directory)
            .status()
            .expect("git is in the dev shell");
        assert!(status.success(), "git {arguments:?}");
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testing/plugins")
        .join(name)
}

/// Points the process at the helper, for a test that already holds the guard and
/// so must not take it again.
fn point_at_helper() {
    let path = locate();
    // The helper reads this itself; the parent only has to pass the variable on.
    // SAFETY: the guard is what every test in this binary takes before it touches
    // the environment, and every test writes the same path, so no test reads an
    // environment another is changing.
    unsafe { std::env::set_var("YAPFAST_PLUGINS", &path) };
}

/// Points the process at the helper the links test runs, and answers the guard
/// that keeps the environment to this test until it ends: keep it, or another test
/// may set the variable while this one's fetch is starting.
#[cfg(unix)]
fn helper() -> MutexGuard<'static, ()> {
    let environment = ENVIRONMENT.lock().unwrap_or_else(PoisonError::into_inner);
    point_at_helper();
    environment
}

/// Waits until the plugin system has the helper an enabled startup plugin asked
/// for, and fails when it has none to start.
///
/// `Missing` ends the wait at once: nothing will put a process behind a state
/// that says the binary isn't there, so the thirty seconds would only say the
/// same thing later.
fn wait_running(state: &mut plugins::PluginState) {
    let started = Instant::now();
    loop {
        match state.helper() {
            HelperState::Running { .. } => return,
            HelperState::Missing => panic!(
                "there is no helper to start, so even a run without `--no-plugins` would start \
                 nothing, and the control proves nothing"
            ),
            stuck if started.elapsed() > START => panic!(
                "the control never got a helper running, so `--no-plugins` took nothing away: \
                 {stuck:?} after {START:?}"
            ),
            _ => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Where the helper is: what `YAPFAST_PLUGINS` names, or the one this build made
/// in the test binary's own profile directory.
///
/// The profile is the test's own: `cargo test --profile fast` runs it from
/// `target/fast/deps/`, so the helper a build of the workspace makes is in
/// `target/fast/`, whatever `CARGO_TARGET_DIR` or `--target` puts in front of
/// it. A missing helper fails the test rather than skipping it.
fn locate() -> PathBuf {
    if let Some(named) = std::env::var_os("YAPFAST_PLUGINS") {
        let named = PathBuf::from(named);
        assert!(
            named.is_file(),
            "YAPFAST_PLUGINS names {}, which isn't a file",
            named.display()
        );
        return named;
    }
    let directory = profile();
    let beside = directory.join("yapfast-plugins");
    assert!(beside.is_file(), "{}", missing(&directory));
    beside
}

/// The directory this test binary was built into, which is its profile's.
fn profile() -> PathBuf {
    let exe = std::env::current_exe().expect("a test binary has a path");
    let directory = exe
        .parent()
        .and_then(Path::parent)
        .expect("a target directory above the test binary's deps");
    directory.to_path_buf()
}

/// What to say when the helper isn't where it should be: the command that builds
/// it, with the profile this test needs.
fn missing(directory: &Path) -> String {
    let named = directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Cargo calls the debug profile `debug` on disk and `dev` on the command
    // line; every other profile has the one name.
    let profile = match named.as_str() {
        "debug" => "dev",
        other => other,
    };
    format!(
        "there's no yapfast-plugins helper in {}, and YAPFAST_PLUGINS names none. It is the real \
         helper this test fetches through, so build it in the dev shell and run the test again:\n\
         \x20 nix develop path:./nix --command cargo build --profile {profile} -p yapfast-plugin-host\n\
         or set YAPFAST_PLUGINS to one you built another way. A test that ran nothing doesn't pass.",
        directory.display()
    )
}
