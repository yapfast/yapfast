//! Installing a plugin, from a URL to the store: the helper reads it out of a
//! repository, the parent reviews it, and the store writes it.
//!
//! The repository is a local one, made in a temporary directory and fetched
//! through a `file://` address, which is what `fetch::Spec` allows for
//! development. It needs the real `yapfast-plugins`, and a missing one fails the
//! test with the command that builds it: a test that skipped would pass having
//! fetched nothing.
//!
//! ```sh
//! nix develop path:./nix --command cargo build --profile fast -p yapfast-plugin-host
//! nix develop path:./nix --command cargo test --profile fast -p yapfast-core \
//!   --features plugins --test plugins_install
//! ```
//!
//! `YAPFAST_PLUGINS` names a helper built another way; without it, the helper
//! the build made in this test binary's own profile directory is used.

#![cfg(feature = "plugins")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, PoisonError};

use yapfast_core::plugins::installing;
use yapfast_core::plugins::registry::Registry;
use yapfast_core::plugins::store::Store;
use yapfast_plugin_api::PluginId;

/// A repository holding one plugin at `plugins/echo`, and what its address is.
fn repository(directory: &Path) -> String {
    let folder = directory.join("plugins/echo");
    std::fs::create_dir_all(&folder).expect("the plugin's folder");
    std::fs::write(folder.join("yapfast-plugin.json"), manifest()).expect("its manifest");
    std::fs::copy(fixture("bad_answer.wasm"), folder.join("plugin.wasm")).expect("its module");
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
            "A plugin",
        ],
    ] {
        let status = Command::new("git")
            .args(&arguments)
            .current_dir(directory)
            .status()
            .expect("git is in the dev shell");
        assert!(status.success(), "git {arguments:?}");
    }
    let path = directory.to_str().expect("a temporary directory's path");
    format!("file://{path}#path=plugins/echo")
}

fn manifest() -> String {
    r#"{
  "api": 1,
  "id": "dev.local.echo",
  "name": "Echo",
  "version": "0.1.0",
  "description": "Answers every message with what it read.",
  "license": "MIT",
  "wasm": "plugin.wasm",
  "limits": { "memory_pages": 64 },
  "activation": { "on": ["event:message_create"] },
  "permissions": { "capabilities": ["messages.read"], "network": [] }
}
"#
    .to_owned()
}

#[test]
fn a_plugin_is_fetched_reviewed_installed_rolled_back_and_removed() {
    let _environment = helper();

    let source = tempfile::tempdir().expect("a repository's directory");
    let url = repository(source.path());
    let installed = tempfile::tempdir().expect("a data directory");

    // Fetch: the helper reads the manifest and the module out of the repository,
    // and the parent holds both, ready for the review.
    let outcome = installing::fetch(&url).expect("the repository reads");
    let installing::Outcome::Plugin(fetched) = outcome else {
        panic!("a repository with one plugin reads as one plugin");
    };
    let review = fetched.review();
    assert_eq!(review.id, "dev.local.echo");
    assert_eq!(review.path, "plugins/echo");
    assert_eq!(review.capabilities, ["messages.read"]);
    assert_eq!(review.memory_pages, 64);
    assert!(!review.reviewed, "a URL install is an unreviewed source");
    assert!(fetched.module.is_some(), "its module came along");

    // Install: the store writes it, and a store read again knows it.
    let mut store = Store::open(installed.path()).expect("a store");
    store.install(&fetched.package()).expect("it installs");
    let registry = Registry::load(&store);
    let id = PluginId::parse("dev.local.echo").expect("a plugin id");
    let entry = registry.entry(&id).expect("the plugin is installed");
    assert!(!entry.enabled, "installing doesn't enable");
    assert_eq!(entry.manifest.name, "Echo");

    // A second install of the same commit leaves nothing to roll back to.
    assert!(
        store.roll_back(&id).is_err(),
        "there is no earlier version yet"
    );

    // Enabling, then installing the same plugin again, keeps the version it
    // replaced so the page can go back to it.
    store.set_enabled(&id, true).expect("it is on");
    store
        .install(&fetched.package())
        .expect("it installs again");
    store
        .roll_back(&id)
        .expect("the version it replaced is there to go back to");

    // Uninstall: gone from the lock, its files gone with it.
    store.uninstall(&id, false).expect("it is removed");
    assert!(
        Registry::load(&store).entry(&id).is_none(),
        "a store read again has nothing"
    );
}

/// What the Advanced dialog's file picker does: a plugin's file becomes an
/// installed plugin, off until the person turns it on, and the store is the one
/// place that remembers they did.
#[test]
fn a_picked_wasm_file_installs_and_waits_to_be_enabled() {
    let data = tempfile::tempdir().expect("a data directory");
    let source = tempfile::tempdir().expect("a folder to pick from");
    let module = source.path().join("plugin.wasm");
    std::fs::copy(fixture("bad_answer.wasm"), &module).expect("the module");
    std::fs::write(source.path().join("yapfast-plugin.json"), manifest()).expect("its manifest");

    let fetched = installing::from_wasm(&module).expect("the module and its manifest read");
    assert_eq!(
        fetched.id.to_string(),
        "dev.local.echo",
        "the manifest's id"
    );
    // The capability its made manifest declares is what the row shows, and
    // nothing has run.
    assert_eq!(fetched.review().capabilities, ["messages.read"]);

    let mut store = Store::open(data.path()).expect("a store");
    store.install(&fetched.package()).expect("it installs");
    let id = fetched.id.clone();
    let registry = Registry::load(&store);
    let entry = registry.entry(&id).expect("installed");
    assert!(!entry.enabled, "installed and off, until the person says");

    // Enabling is that choice, and `lock.json` is where it is kept: a store read
    // again finds it, which is a restart.
    store.set_enabled(&id, true).expect("it is on");
    let again = Registry::load(&Store::open(data.path()).expect("a store"));
    assert!(
        again.entry(&id).expect("still installed").enabled,
        "on after a restart"
    );
}

/// The user's own path: a built module sits in `target/…/release`, two folders
/// under the manifest that describes it, and the manifest is what gets installed.
#[test]
fn a_module_in_a_build_folder_finds_the_manifest_above_it() {
    let plugin = tempfile::tempdir().expect("the plugin's folder");
    std::fs::write(plugin.path().join("yapfast-plugin.json"), manifest()).expect("its manifest");
    std::fs::copy(
        fixture("bad_answer.wasm"),
        plugin.path().join("plugin.wasm"),
    )
    .expect("its module");
    let built = plugin.path().join("target/wasm32-unknown-unknown/release");
    std::fs::create_dir_all(&built).expect("a build folder");
    let picked = built.join("ping.wasm");
    std::fs::copy(fixture("bad_answer.wasm"), &picked).expect("the built module");

    let fetched = installing::from_wasm(&picked).expect("the manifest two folders up");
    assert_eq!(
        fetched.id.to_string(),
        "dev.local.echo",
        "not one made from the file"
    );
    // And it installs as the plugin its manifest describes, module and all.
    let data = tempfile::tempdir().expect("a data directory");
    let mut store = Store::open(data.path()).expect("a store");
    store.install(&fetched.package()).expect("it installs");
    assert!(
        Registry::load(&store).entry(&fetched.id).is_some(),
        "the store has it"
    );
}

/// A module with no manifest anywhere is refused, with a sentence saying what
/// the manifest is for — installed, it would be a plugin nothing could call.
#[test]
fn a_wasm_with_no_manifest_is_refused_with_a_sentence() {
    let folder = tempfile::tempdir().expect("a folder");
    let module = folder.path().join("ping.wasm");
    std::fs::copy(fixture("bad_answer.wasm"), &module).expect("a module");
    let error = installing::from_wasm(&module).expect_err("there is no manifest");
    assert!(error.contains("yapfast-plugin.json"), "{error}");
    assert!(
        error.contains("what a plugin is called"),
        "it says what the manifest is for: {error}"
    );
}

/// A manifest beside the file is what the plugin is, whatever the file is
/// called.
#[test]
fn a_manifest_beside_the_file_is_what_the_plugin_is() {
    let folder = tempfile::tempdir().expect("a folder");
    std::fs::write(folder.path().join("yapfast-plugin.json"), manifest()).expect("its manifest");
    std::fs::copy(
        fixture("bad_answer.wasm"),
        folder.path().join("plugin.wasm"),
    )
    .expect("its module");

    let fetched = installing::from_wasm(&folder.path().join("plugin.wasm")).expect("it reads");
    assert_eq!(fetched.id.to_string(), "dev.local.echo");
}

#[test]
fn a_file_that_is_not_a_module_is_refused_by_name() {
    let folder = tempfile::tempdir().expect("a folder");
    std::fs::write(folder.path().join("yapfast-plugin.json"), manifest()).expect("its manifest");
    let file = folder.path().join("plugin.wasm");
    std::fs::write(&file, b"#!/bin/sh\necho hi\n").expect("something else");
    let error = installing::from_wasm(&file).expect_err("that isn't a module");
    assert!(error.contains("isn't a WebAssembly module"), "{error}");
}

#[test]
fn the_same_id_from_two_folders_is_refused() {
    let _environment = helper();
    let installed = tempfile::tempdir().expect("a data directory");
    let mut store = Store::open(installed.path()).expect("a store");

    let one = fetched_from("plugins/echo");
    let other = fetched_from("plugins/other");
    assert_eq!(one.id, other.id, "both folders declare one id");
    store.install(&one.package()).expect("the first installs");
    let error = store
        .install(&other.package())
        .expect_err("the second is refused")
        .to_string();
    assert!(error.contains("already uses"), "{error}");
}

/// One plugin's fetch, from a repository made for it.
fn fetched_from(folder: &str) -> installing::Fetched {
    let source = tempfile::tempdir().expect("a repository's directory");
    let plugin = source.path().join(folder);
    std::fs::create_dir_all(&plugin).expect("the plugin's folder");
    std::fs::write(plugin.join("yapfast-plugin.json"), manifest()).expect("its manifest");
    std::fs::copy(fixture("bad_answer.wasm"), plugin.join("plugin.wasm")).expect("its module");
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
            "A plugin",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(&arguments)
                .current_dir(source.path())
                .status()
                .expect("git")
                .success()
        );
    }
    let url = format!("file://{}#path={folder}", source.path().display());
    match installing::fetch(&url).expect("the repository reads") {
        installing::Outcome::Plugin(fetched) => fetched,
        installing::Outcome::Index { .. } => panic!("one plugin, not an index"),
    }
}

/// One test at a time, for the process's environment: the two tests here fetch
/// through the helper, which reads `YAPFAST_PLUGINS` when it starts, and `setenv`
/// while another thread reads the environment is the data race the standard
/// library marks it as.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// Points the process at the helper these tests run, and answers the guard that
/// keeps the environment to this test until it ends: keep it, or another test
/// may set the variable while this one's fetch is starting.
fn helper() -> MutexGuard<'static, ()> {
    let environment = ENVIRONMENT.lock().unwrap_or_else(PoisonError::into_inner);
    let path = locate();
    // The helper reads this itself; the parent only has to pass the variable on.
    // SAFETY: the guard above is what every test in this binary takes before it
    // touches the environment, and every test writes the same path, so no test
    // reads an environment another is changing.
    unsafe { std::env::set_var("YAPFAST_PLUGINS", &path) };
    environment
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
    let name = if cfg!(windows) {
        "yapfast-plugins.exe"
    } else {
        "yapfast-plugins"
    };
    let directory = profile();
    let beside = directory.join(name);
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

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testing/plugins")
        .join(name)
}
