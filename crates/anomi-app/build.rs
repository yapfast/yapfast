//! Hands `src/build_info.rs` what the Slint app's `build.rs` hands its own:
//! the program's version (the workspace's, not this crate's), the commit and
//! its date, and whether this is a release build. Nix's sandbox has no Git,
//! so the packaging gives the first two in `YAPFAST_COMMIT` and
//! `YAPFAST_COMMIT_DATE`; a build in a checkout asks Git. Only a release's
//! packaging sets `YAPFAST_RELEASE`.

use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    for name in ["YAPFAST_COMMIT", "YAPFAST_COMMIT_DATE", "YAPFAST_RELEASE"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = root.join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    if let Some(version) = version(&manifest) {
        println!("cargo:rustc-env=ANOMI_BUILD_VERSION={version}");
    }
    // Only this repository's own checkout counts, as in the Slint app's.
    let checkout = git(&root, &["rev-parse", "--show-prefix"]).is_some_and(|p| p.is_empty());
    if checkout {
        for path in ["HEAD", "logs/HEAD"] {
            if let Some(path) = git(&root, &["rev-parse", "--git-path", path])
                .map(|path| root.join(path))
                .filter(|path| path.exists())
            {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
    let given = |name: &str| env::var(name).ok().filter(|value| !value.is_empty());
    let commit =
        given("YAPFAST_COMMIT").or_else(|| git(&root, &["rev-parse", "HEAD"]).filter(|_| checkout));
    let date = given("YAPFAST_COMMIT_DATE").or_else(|| {
        // UTC, as Nix's `lastModifiedDate` is.
        let output = Command::new("git")
            .current_dir(&root)
            .args(["log", "-1", "--date=format-local:%Y-%m-%d", "--format=%cd"])
            .env("TZ", "UTC")
            .output()
            .ok()
            .filter(|output| checkout && output.status.success())?;
        String::from_utf8(output.stdout)
            .ok()
            .map(|date| date.trim().to_owned())
    });
    if let Some(commit) = commit.filter(|commit| !commit.is_empty()) {
        println!("cargo:rustc-env=ANOMI_BUILD_COMMIT={commit}");
    }
    if let Some(date) = date.filter(|date| !date.is_empty()) {
        println!("cargo:rustc-env=ANOMI_BUILD_DATE={date}");
    }
    if given("YAPFAST_RELEASE").is_some() {
        println!("cargo:rustc-env=ANOMI_BUILD_RELEASE=1");
    }
}

/// The `version` of the workspace's own package: the program's.
fn version(manifest: &Path) -> Option<String> {
    let text = format!("\n{}", std::fs::read_to_string(manifest).ok()?);
    let package = text
        .split("\n[")
        .find(|part| part.starts_with("package]"))?;
    let line = package.lines().find(|line| line.starts_with("version"))?;
    Some(line.split('"').nth(1)?.to_owned())
}

/// What `git` prints for `arguments` in `root`, when it runs and succeeds.
fn git(root: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(arguments)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_owned())
}
