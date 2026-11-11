//! Installing a plugin, from a URL to the store: the helper reads it out of a
//! repository, the parent reviews it, and the store writes it.
//!
//! The repository is a local one, made in a temporary directory and fetched
//! through a `file://` address, which is what `fetch::Spec` allows for
//! development. It needs the real `yapfast-plugins`, which `YAPFAST_PLUGINS`
//! names; without one it says so and passes.
//!
//! ```sh
//! cargo build --profile fast -p yapfast-plugin-host
//! YAPFAST_PLUGINS=$PWD/target/fast/yapfast-plugins \
//!   cargo test --profile fast -p yapfast-core --features plugins --test plugins_install
//! ```

#![cfg(feature = "plugins")]

use std::path::{Path, PathBuf};
use std::process::Command;

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
    let Some(helper) = helper() else {
        eprintln!("no yapfast-plugins beside the build and no YAPFAST_PLUGINS; skipped");
        return;
    };
    // SAFETY: this is the only test in this binary, so nothing else reads the
    // environment while it runs.
    unsafe { std::env::set_var("YAPFAST_PLUGINS", &helper) };

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

#[test]
fn the_same_id_from_two_folders_is_refused() {
    let Some(helper) = helper() else {
        eprintln!("no yapfast-plugins beside the build and no YAPFAST_PLUGINS; skipped");
        return;
    };
    unsafe { std::env::set_var("YAPFAST_PLUGINS", &helper) };
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

fn helper() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("YAPFAST_PLUGINS") {
        let named = PathBuf::from(named);
        if named.is_file() {
            return Some(named);
        }
    }
    let beside = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fast/yapfast-plugins");
    beside.is_file().then_some(beside)
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testing/plugins")
        .join(name)
}
