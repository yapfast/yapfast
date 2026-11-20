//! The helper's OS sandbox, applied in a process of its own through
//! `yapfast-plugins sandbox-check`, since a sandbox can't be lifted from the
//! test runner again.

#![cfg(target_os = "linux")]

use std::collections::BTreeMap;
use std::process::Command;

fn check() -> (BTreeMap<String, String>, Vec<String>) {
    let store = tempfile::tempdir().expect("a store");
    std::fs::write(store.path().join("plugin.wasm"), b"\0asm").expect("a module");
    let cache = tempfile::tempdir().expect("a cache");
    let outside = tempfile::tempdir().expect("somewhere else");
    let secret = outside.path().join("token");
    std::fs::write(&secret, b"secret").expect("a secret");
    let output = Command::new(env!("CARGO_BIN_EXE_yapfast-plugins"))
        .arg("sandbox-check")
        .arg(store.path())
        .arg(cache.path())
        .arg(&secret)
        .output()
        .expect("the helper runs");
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).expect("text");
    let mut values = BTreeMap::new();
    let mut missing = Vec::new();
    for line in text.lines() {
        let (key, value) = line.split_once('=').expect("key=value");
        if key == "missing" {
            missing.push(value.to_owned());
        } else {
            values.insert(key.to_owned(), value.to_owned());
        }
    }
    (values, missing)
}

#[test]
fn the_sandbox_keeps_the_store_and_cache_and_nothing_else() {
    let (values, missing) = check();
    let applied = &values["applied"];
    // The store and the cache stay reachable whatever applied.
    assert_eq!(values["read_store"], "true", "{values:?} {missing:?}");
    assert_eq!(values["write_cache"], "true", "{values:?} {missing:?}");
    if applied.contains("landlock") {
        assert_eq!(
            values["read_outside"], "false",
            "a file outside the store was read"
        );
    } else {
        eprintln!("Landlock is unavailable here: {missing:?}");
    }
    if applied.contains("seccomp") {
        assert_eq!(values["socket"], "false", "a socket was opened");
        assert_eq!(values["exec"], "false", "a program was started");
    } else {
        eprintln!("seccomp is unavailable here: {missing:?}");
    }
}
