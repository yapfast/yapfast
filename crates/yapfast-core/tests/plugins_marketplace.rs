//! The marketplace, from a registry to an installed plugin: the helper reads a
//! registry repository, the catalog is checked entry by entry, and what an
//! install fetches is compared with the entry that listed it.
//!
//! Both repositories are local ones made in temporary directories and read
//! through `file://` addresses, which is what `fetch` allows for development and
//! for the tests: nothing here reaches the network. It needs the real
//! `yapfast-plugins`, which `YAPFAST_PLUGINS` names; without one it says so and
//! passes.
//!
//! ```sh
//! cargo build --profile fast -p yapfast-plugin-host
//! YAPFAST_PLUGINS=$PWD/target/fast/yapfast-plugins \
//!   cargo test --profile fast -p yapfast-core --features plugins --test plugins_marketplace
//! ```

#![cfg(feature = "plugins")]

use std::path::{Path, PathBuf};
use std::process::Command;

use yapfast_core::plugins::installing;
use yapfast_core::plugins::marketplace;
use yapfast_core::plugins::store::{self, Store};
use yapfast_core::plugins::registry::Registry;

const ID: &str = "dev.local.echo";

/// The plugin's manifest, as `plugins/echo/yapfast-plugin.json` holds it.
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

/// An entry for that plugin, pinned to `rev` and to the module's hash.
fn entry(repository: &str, rev: &str, sha256: &str) -> String {
    format!(
        r#"{{
  "api": 1,
  "id": "{ID}",
  "name": "Echo",
  "version": "0.1.0",
  "description": "Answers every message with what it read.",
  "authors": [],
  "repo": "{repository}",
  "path": "plugins/echo",
  "rev": "{rev}",
  "sha256": "{sha256}",
  "category": "Chat",
  "tags": ["test"],
  "source": "{repository}#path=plugins/echo",
  "license": "MIT",
  "icon": "icons/{ID}.webp",
  "permissions": {{ "capabilities": ["messages.read"], "network": [] }}
}}"#
    )
}

/// A WebP's header for a 128 by 128 picture: what the catalog measures an icon
/// by, without decoding it.
fn icon() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&22u32.to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(b"VP8X");
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&[0, 0, 0]);
    bytes.extend_from_slice(&[127, 0, 0]);
    bytes.extend_from_slice(&[127, 0, 0]);
    bytes
}

#[test]
fn a_registry_is_browsed_installed_and_checked_against_its_entry() {
    let Some(helper) = helper() else {
        eprintln!("no yapfast-plugins beside the build and no YAPFAST_PLUGINS; skipped");
        return;
    };
    // SAFETY: no other test in this binary changes the environment.
    unsafe { std::env::set_var("YAPFAST_PLUGINS", &helper) };

    // The plugin's repository: one plugin in one folder.
    let plugin = tempfile::tempdir().expect("a plugin repository");
    let folder = plugin.path().join("plugins/echo");
    std::fs::create_dir_all(&folder).expect("its folder");
    std::fs::write(folder.join("yapfast-plugin.json"), manifest()).expect("its manifest");
    let module = fixture("bad_answer.wasm");
    std::fs::copy(&module, folder.join("plugin.wasm")).expect("its module");
    let repository = format!("file://{}", plugin.path().display());
    let rev = commit(plugin.path());
    let sha256 = store::sha256(&std::fs::read(&module).expect("the module's bytes"));

    // The registry: the entry, its icon, and the index CI builds.
    let registry = tempfile::tempdir().expect("a registry repository");
    std::fs::create_dir_all(registry.path().join("plugins")).expect("the entry folder");
    std::fs::create_dir_all(registry.path().join("icons")).expect("the icon folder");
    let entry_file = registry.path().join(format!("plugins/{ID}.json"));
    std::fs::write(&entry_file, entry(&repository, &rev, &sha256)).expect("its entry");
    std::fs::write(registry.path().join(format!("icons/{ID}.webp")), icon()).expect("its icon");
    std::fs::write(
        registry.path().join("index.json"),
        format!(
            r#"{{ "api": 1, "plugins": ["{ID}"], "delisted": [
                {{ "id": "org.example.leaky", "reason": "it uploaded what it read", "security": true }}
            ] }}"#
        ),
    )
    .expect("its index");
    commit(registry.path());

    // Browsing: the helper reads the registry, and the catalog holds the row,
    // the entry behind it and the icon's bytes.
    let address = format!("file://{}", registry.path().display());
    let catalog = marketplace::load(&address).expect("the registry reads");
    assert_eq!(catalog.listings.len(), 1, "one row");
    assert_eq!(catalog.listings[0].id, ID);
    assert!(catalog.listings[0].reviewed, "a maintainer read it");
    assert_eq!(catalog.listings[0].category, "Chat");
    assert_eq!(
        catalog.listings[0].icon.as_deref(),
        Some(marketplace::icon_url(ID).as_str())
    );
    assert_eq!(catalog.icon_count(), 1, "the icon's bytes are held");
    assert_eq!(catalog.warnings(&[]).len(), 0, "nobody has the delisted one");
    let listed = catalog.entry(ID).expect("its entry").clone();
    assert_eq!(listed.rev, rev);

    // Installing: the entry's own commit is fetched, and what came back is
    // compared with what the entry pinned. A `file://` source takes no
    // `@ref`, so the fixture is read at its only commit, which is the pin.
    let installing::Outcome::Plugin(fetched) =
        installing::fetch(&format!("{repository}#path=plugins/echo"))
            .expect("the plugin reads")
    else {
        panic!("one plugin, not an index");
    };
    assert_eq!(fetched.rev, rev, "the commit the entry pins");
    catalog
        .verify(&listed, &fetched)
        .expect("the plugin is what the registry listed");
    let installed = tempfile::tempdir().expect("a data directory");
    let mut store = Store::open(installed.path()).expect("a store");
    store.install(&fetched.package()).expect("it installs");
    assert!(
        Registry::load(&store)
            .entry(&fetched.id)
            .is_some(),
        "the store has it"
    );

    // A hash that isn't the module's, and a commit that isn't the one fetched:
    // each is refused, in a sentence saying nothing was installed.
    let tampered = marketplace::Entry {
        sha256: "0".repeat(64),
        ..listed.clone()
    };
    let error = catalog
        .verify(&tampered, &fetched)
        .expect_err("not that module");
    assert!(error.contains("nothing was installed"), "{error}");
    let moved = marketplace::Entry {
        rev: "1".repeat(40),
        ..listed.clone()
    };
    let error = catalog
        .verify(&moved, &fetched)
        .expect_err("not that commit");
    assert!(error.contains("pinned to commit"), "{error}");

    // What a registry's CI runs on the pull request that added the entry.
    let checked = Command::new(&helper)
        .arg("verify-entry")
        .arg(&entry_file)
        .output()
        .expect("the helper runs");
    let printed = String::from_utf8_lossy(&checked.stdout);
    assert!(
        checked.status.success() && printed.contains("\"kind\":\"verified\""),
        "the entry is accepted: {printed}"
    );

    // And the entry a maintainer would refuse: a license a listing may not
    // carry, which CI says before anyone reads the source.
    let bad = registry.path().join("plugins/com.example.unlicensed.json");
    std::fs::write(
        &bad,
        entry(&repository, &rev, &sha256)
            .replace(ID, "com.example.unlicensed")
            .replace("\"license\": \"MIT\"", "\"license\": \"WTFPL\""),
    )
    .expect("an entry");
    let refused = Command::new(&helper)
        .arg("verify-entry")
        .arg(&bad)
        .output()
        .expect("the helper runs");
    let printed = String::from_utf8_lossy(&refused.stdout);
    assert!(!refused.status.success(), "it is refused: {printed}");
    assert!(printed.contains("OSI-approved"), "{printed}");
}

/// Initialises a repository in `directory` with everything in it committed, and
/// returns the commit.
fn commit(directory: &Path) -> String {
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
            "A registry's plugin",
        ],
    ] {
        let status = Command::new("git")
            .args(&arguments)
            .current_dir(directory)
            .status()
            .expect("git is in the dev shell");
        assert!(status.success(), "git {arguments:?}");
    }
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(directory)
        .output()
        .expect("git is in the dev shell");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
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
