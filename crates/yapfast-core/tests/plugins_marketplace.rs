//! The marketplace, from a registry to an installed plugin: the helper reads a
//! registry repository, the catalog is checked entry by entry, and what an
//! install fetches is compared with the entry that listed it.
//!
//! Both repositories are local ones made in temporary directories and read
//! through `file://` addresses, which is what `fetch` allows for development and
//! for the tests: nothing here reaches the network. It needs the real
//! `yapfast-plugins`, and a missing one fails the test with the command that
//! builds it: a test that skipped would pass having browsed nothing.
//!
//! ```sh
//! nix develop path:./nix --command cargo build --profile fast -p yapfast-plugin-host
//! nix develop path:./nix --command cargo test --profile fast -p yapfast-core \
//!   --features plugins --test plugins_marketplace
//! ```
//!
//! `YAPFAST_PLUGINS` names a helper built another way; without it, the helper
//! the build made in this test binary's own profile directory is used.

#![cfg(feature = "plugins")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, PoisonError};

use yapfast_core::plugins::installing;
use yapfast_core::plugins::marketplace;
use yapfast_core::plugins::registry::Registry;
use yapfast_core::plugins::store::{self, Store};

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
    let _environment = helper();

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
    assert_eq!(
        catalog.warnings(&[]).len(),
        0,
        "nobody has the delisted one"
    );
    let listed = catalog.entry(ID).expect("its entry").clone();
    assert_eq!(listed.rev, rev);

    // Installing: the entry's own commit is fetched, and what came back is
    // compared with what the entry pinned. A `file://` source takes no
    // `@ref`, so the fixture is read at its only commit, which is the pin.
    let installing::Outcome::Plugin(fetched) =
        installing::fetch(&format!("{repository}#path=plugins/echo")).expect("the plugin reads")
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
        Registry::load(&store).entry(&fetched.id).is_some(),
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
    let checked = Command::new(locate())
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
    let refused = Command::new(locate())
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

/// One test at a time, for the process's environment: a registry is read through
/// the helper, which takes `YAPFAST_PLUGINS` when it starts, and `setenv` while
/// another thread reads the environment is the data race the standard library
/// marks it as. This binary holds one test today; the guard is what keeps the
/// next one from racing it.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// Points the process at the helper these tests run, and answers the guard that
/// keeps the environment to this test until it ends: keep it, or another test
/// may set the variable while this one's helper is being started.
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
         helper this test reads a registry through, so build it in the dev shell and run the test \
         again:\n\
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
