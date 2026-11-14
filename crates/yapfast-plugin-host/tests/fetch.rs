//! `fetch` against repositories built on this disk.
//!
//! Every test makes a real repository with the `git` command line tool and
//! fetches it through a `file://` URL, so nothing here reaches the network. A
//! local fetch also runs git's own `upload-pack`, so the tests need `git` on
//! `PATH`: they fail with git's own words rather than quietly testing nothing
//! when it isn't there.
//!
//! The modules are `testing/plugins/echo.wasm`, the fixture the rest of the
//! plugin tests use, so what is fetched is a module this build would load.

use std::path::{Path, PathBuf};
use std::process::Command;

use yapfast_plugin_host::fetch::{self, Report};

/// The module a fixture plugin carries.
fn echo() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/plugins/echo.wasm"))
        .expect("the echo fixture is in the repository")
}

/// A manifest for a plugin of one module, and an icon when it names one.
fn manifest(wasm: &str, icon: Option<&str>) -> String {
    let icon = match icon {
        Some(icon) => format!(r#","icon":"{icon}""#),
        None => String::new(),
    };
    format!(
        r#"{{"api":1,"id":"com.example.echo","name":"Echo","version":"1.0.0","wasm":"{wasm}"{icon}}}"#
    )
}

/// A repository on this disk, built with the `git` command line tool.
///
/// git writes it, so what is fetched is a repository git wrote, and the tests
/// need nothing from this crate to make one.
struct Repository {
    /// Owns the directory the repository lives in, and deletes it when the test
    /// ends.
    _scratch: tempfile::TempDir,
    dir: PathBuf,
}

impl Repository {
    fn new() -> Repository {
        let scratch = tempfile::tempdir().expect("a temporary directory");
        let dir = scratch.path().join("repo");
        std::fs::create_dir(&dir).expect("the repository directory");
        let repository = Repository {
            _scratch: scratch,
            dir,
        };
        repository.git(&["init", "--quiet", "--initial-branch", "main"]);
        repository
    }

    fn git(&self, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(arguments)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// Writes `files` and commits them, returning the commit's id.
    fn commit(&self, files: &[(&str, &[u8])], message: &str) -> String {
        for (path, bytes) in files {
            let path = self.dir.join(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("a directory in the repository");
            }
            std::fs::write(&path, bytes).expect("a file in the repository");
        }
        self.git(&["add", "--all"]);
        self.git(&["commit", "--quiet", "--message", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// The repository as a `file://` URL, with a `#ref=`/`#path=` suffix.
    fn url(&self, suffix: &str) -> String {
        format!("file://{}{suffix}", self.dir.display())
    }
}

/// The plugin report a fetch found, as its parts.
fn plugin(report: &Report) -> (&str, &str, &str, &str, &str, Option<&str>, Option<&str>) {
    match report {
        Report::Plugin {
            source,
            path,
            reference,
            rev,
            manifest,
            wasm,
            icon,
        } => (
            source,
            path,
            reference,
            rev,
            manifest,
            wasm.as_deref(),
            icon.as_deref(),
        ),
        other => panic!("expected a plugin, got {other:?}"),
    }
}

#[test]
fn a_plugin_in_a_folder_of_a_repository() {
    let repository = Repository::new();
    let text = manifest("plugin.wasm", None);
    let commit = repository.commit(
        &[
            ("plugins/foo/yapfast-plugin.json", text.as_bytes()),
            ("plugins/foo/plugin.wasm", &echo()),
        ],
        "a plugin in a folder",
    );

    let fetched = fetch::fetch(&repository.url("#path=plugins/foo")).expect("the plugin is read");
    let (source, path, reference, rev, read, wasm, icon) = plugin(&fetched.report);
    assert_eq!(
        source,
        &repository.url(""),
        "the source is the repository, without the folder"
    );
    assert_eq!(path, "plugins/foo");
    assert_eq!(reference, "HEAD");
    assert_eq!(rev, commit);
    assert_eq!(read, text);
    assert!(icon.is_none(), "this plugin brings no icon");

    let wasm = PathBuf::from(wasm.expect("a module"));
    assert_eq!(
        std::fs::read(&wasm).expect("the module was written"),
        echo(),
        "the bytes are the ones the repository holds"
    );
    let directory = wasm.parent().expect("a directory of its own").to_path_buf();
    drop(fetched);
    assert!(
        !directory.exists(),
        "nothing is left behind when nothing took the files"
    );
}

#[test]
fn a_plugin_at_the_root_of_a_repository() {
    let repository = Repository::new();
    let text = manifest("echo.wasm", None);
    repository.commit(
        &[
            ("yapfast-plugin.json", text.as_bytes()),
            ("echo.wasm", &echo()),
        ],
        "a plugin at the root",
    );

    let fetched = fetch::fetch(&repository.url("")).expect("the plugin is read");
    let (_, path, _, _, read, wasm, _) = plugin(&fetched.report);
    assert_eq!(path, "", "a plugin at the root has an empty folder");
    assert_eq!(read, text);
    assert!(wasm.is_some());
}

#[test]
fn an_icon_is_written_beside_the_module() {
    let repository = Repository::new();
    let text = manifest("plugin.wasm", Some("icons/icon.png"));
    let picture = b"\x89PNG\r\n\x1a\nnot really a picture".to_vec();
    repository.commit(
        &[
            ("yapfast-plugin.json", text.as_bytes()),
            ("plugin.wasm", &echo()),
            ("icons/icon.png", &picture),
        ],
        "a plugin with an icon",
    );

    let fetched = fetch::fetch(&repository.url("")).expect("the plugin is read");
    let (_, _, _, _, _, _, icon) = plugin(&fetched.report);
    let icon = PathBuf::from(icon.expect("an icon"));
    assert_eq!(std::fs::read(&icon).expect("the icon was written"), picture);
    assert_eq!(
        icon.strip_prefix(
            icon.parent()
                .and_then(Path::parent)
                .expect("the temporary directory")
        ),
        Ok(Path::new("icons/icon.png")),
        "the icon keeps its path inside the plugin"
    );
}

#[test]
fn an_icon_over_64_kib_is_refused() {
    let repository = Repository::new();
    let text = manifest("plugin.wasm", Some("icon.png"));
    let picture = vec![0x89u8; (64 << 10) + 1];
    repository.commit(
        &[
            ("yapfast-plugin.json", text.as_bytes()),
            ("plugin.wasm", &echo()),
            ("icon.png", &picture),
        ],
        "an icon over the cap",
    );

    let error = fetch::fetch(&repository.url("")).expect_err("the icon is refused");
    assert!(
        error.to_string().contains("an icon is at most 64 KiB"),
        "{error}"
    );
}

#[test]
fn the_index_of_a_repository_that_holds_several_plugins() {
    let repository = Repository::new();
    let index = br#"{"api":1,"plugins":["plugins/one","plugins/two"]}"#;
    let text = manifest("plugin.wasm", None);
    repository.commit(
        &[
            ("yapfast-plugins.json", index),
            ("plugins/one/yapfast-plugin.json", text.as_bytes()),
            ("plugins/one/plugin.wasm", &echo()),
            ("plugins/two/yapfast-plugin.json", text.as_bytes()),
            ("plugins/two/plugin.wasm", &echo()),
        ],
        "a repository of several plugins",
    );

    let fetched = fetch::fetch(&repository.url("")).expect("the index is read");
    match &fetched.report {
        Report::Index { source, paths } => {
            assert_eq!(source, &repository.url(""));
            assert_eq!(paths, &["plugins/one", "plugins/two"]);
        }
        other => panic!("expected an index, got {other:?}"),
    }
}

#[test]
fn a_repository_with_no_plugin_says_what_to_do() {
    let repository = Repository::new();
    repository.commit(&[("README.md", b"nothing here")], "no plugin at all");

    let error = fetch::fetch(&repository.url("")).expect_err("there is no plugin");
    let text = error.to_string();
    assert!(text.contains("has no `yapfast-plugin.json`"), "{text}");
    assert!(text.contains("#path=plugins/<name>"), "{text}");
}

#[test]
fn a_folder_that_has_no_manifest_says_so() {
    let repository = Repository::new();
    repository.commit(
        &[
            (
                "plugins/one/yapfast-plugin.json",
                manifest("plugin.wasm", None).as_bytes(),
            ),
            ("plugins/one/plugin.wasm", &echo()),
            ("plugins/two/plugin.wasm", &echo()),
        ],
        "one folder without a manifest",
    );

    let error = fetch::fetch(&repository.url("#path=plugins/two")).expect_err("no manifest there");
    let text = error.to_string();
    assert!(text.contains("`plugins/two`"), "{text}");
    assert!(text.contains("has no `yapfast-plugin.json`"), "{text}");
}

#[test]
fn a_module_that_is_a_git_lfs_pointer_says_to_check_it_in() {
    let repository = Repository::new();
    let pointer = b"version https://git-lfs.github.com/spec/v1\noid sha256:4d7a2146\nsize 631\n";
    repository.commit(
        &[
            (
                "yapfast-plugin.json",
                manifest("plugin.wasm", None).as_bytes(),
            ),
            ("plugin.wasm", pointer),
        ],
        "a pointer instead of the module",
    );

    let error = fetch::fetch(&repository.url("")).expect_err("a pointer is not a module");
    let text = error.to_string();
    assert!(text.contains("Git LFS pointer"), "{text}");
    assert!(text.contains("Check in the file itself"), "{text}");
}

#[test]
fn a_module_named_outside_the_plugin_is_refused() {
    let repository = Repository::new();
    repository.commit(
        &[
            (
                "yapfast-plugin.json",
                manifest("../outside.wasm", None).as_bytes(),
            ),
            ("outside.wasm", &echo()),
            ("plugins/foo/plugin.wasm", &echo()),
        ],
        "a module above the folder",
    );

    let error = fetch::fetch(&repository.url("")).expect_err("the path leaves the plugin");
    let text = error.to_string();
    assert!(text.contains("can't be used"), "{text}");
    assert!(
        text.contains("`wasm` must be a path inside the plugin's directory"),
        "{text}"
    );
}

#[test]
fn the_helper_prints_one_json_object() {
    let repository = Repository::new();
    repository.commit(
        &[
            (
                "yapfast-plugin.json",
                manifest("plugin.wasm", None).as_bytes(),
            ),
            ("plugin.wasm", &echo()),
        ],
        "a plugin the helper can print",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_yapfast-plugins"))
        .arg("fetch")
        .arg(repository.url(""))
        .output()
        .expect("the helper runs");
    assert!(
        output.status.success(),
        "the helper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("the report is UTF-8");
    assert_eq!(
        stdout.lines().count(),
        1,
        "one object, on one line: {stdout}"
    );
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("the report is JSON");
    assert_eq!(json["kind"], "plugin");
    assert_eq!(json["path"], "");
    assert_eq!(json["reference"], "HEAD");
    assert_eq!(json["icon"], serde_json::Value::Null);
    assert_eq!(json["manifest"], manifest("plugin.wasm", None));

    // The parent copies the module and deletes the directory; this test is the
    // parent, so it does the same.
    let wasm = PathBuf::from(json["wasm"].as_str().expect("a path to the module"));
    assert_eq!(std::fs::read(&wasm).expect("the module is there"), echo());
    let directory = wasm
        .parent()
        .expect("the directory the helper left")
        .to_path_buf();
    std::fs::remove_dir_all(&directory).expect("the temporary directory is removed");
}

#[test]
fn a_failure_is_one_json_object_and_a_non_zero_status() {
    let repository = Repository::new();
    repository.commit(&[("README.md", b"nothing here")], "no plugin at all");

    let output = Command::new(env!("CARGO_BIN_EXE_yapfast-plugins"))
        .arg("fetch")
        .arg(repository.url(""))
        .output()
        .expect("the helper runs");
    assert!(!output.status.success(), "a failure is a failure");
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the report is JSON");
    assert_eq!(json["kind"], "error");
    assert!(
        json["error"]
            .as_str()
            .expect("a sentence")
            .contains("has no `yapfast-plugin.json`"),
        "{json}"
    );
}
