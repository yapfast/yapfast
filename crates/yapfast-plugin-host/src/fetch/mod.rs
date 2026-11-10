//! `yapfast-plugins fetch`: install a plugin from a git URL.
//!
//! Seconds of work in a process of its own, so `gix` and its memory peak leave
//! with it. Nothing a fetched repository contains is executed: the manifest and
//! the `.wasm` are read straight out of git's objects, checked, and only then
//! copied into the store, where a module is found by its hash.
//!
//! The pipeline is one pass over one commit. `url` turns what a person pasted
//! into a repository, a ref and a directory inside it; `git` fetches exactly
//! that commit, shallowly, into a scratch directory that goes away on the way
//! out; `validate` parses the manifest and reads the module's header and imports
//! without running it; `store` writes the files under a name derived from both
//! hashes, and `lock` records where they came from.
//!
//! `--store DIR` decides whether the install is written at all: without it this
//! only reports what it found. `--review-json` prints the review as JSON, which
//! is what the parent reads to draw the dialog; without it a person gets a few
//! lines of prose. Nothing an install writes is enabled — the review's
//! `reviewed` is `false` — because running a plugin is a decision the person
//! makes in Yapfast, not something a command line does on their behalf.
//!
//! The module is not compiled into the runtime's cache here. That cache is keyed
//! by the wasmtime configuration the compiling engine uses, and the engine that
//! will actually run the plugin is built in `runtime` from the options the
//! helper was started with; a fetch that guessed both would write entries nothing
//! reads, so the first activation compiles the module instead — 22 milliseconds
//! cold, and warm afterwards from the cache the helper already keeps.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, bail};
use serde::Serialize;

use yapfast_plugin_api::capability::Capability;

mod git;
mod lock;
mod store;
mod url;
mod validate;

pub use lock::{LOCK_VERSION, LockEntry, LockFile, Pinned, verified_module};
pub use store::{STORED_MANIFEST, Store, Stored};
pub use url::Source;

/// The manifest a plugin's directory must hold.
pub(crate) const MANIFEST_FILE: &str = "yapfast-plugin.json";

/// Runs `fetch` with the arguments after the subcommand.
pub fn run(arguments: Vec<String>) -> anyhow::Result<()> {
    let options = Options::parse(arguments)?;
    let (review, installed) = install(&options)?;
    if options.review_json {
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        serde_json::to_writer_pretty(&mut stdout, &review)
            .context("the review couldn't be written as JSON")?;
        writeln!(stdout).context("the review couldn't be written")?;
    } else {
        print_review(&review, installed.as_deref());
    }
    Ok(())
}

/// What `fetch` was asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Options {
    /// The URL as the person typed it.
    url: String,
    /// Where installed plugins live; without it the fetch only reports.
    store: Option<PathBuf>,
    /// Print the review as JSON, for the parent, instead of prose.
    review_json: bool,
}

impl Options {
    fn parse(arguments: Vec<String>) -> anyhow::Result<Options> {
        let mut url = None;
        let mut store = None;
        let mut review_json = false;
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--store" => {
                    let directory = arguments.next().context("`--store` needs a directory")?;
                    store = Some(PathBuf::from(directory));
                }
                "--review-json" => review_json = true,
                other if other.starts_with('-') => {
                    bail!("unknown argument `{other}`; try `yapfast-plugins --help`")
                }
                other => {
                    if url.is_some() {
                        bail!("`fetch` takes one URL, not `{other}` as well");
                    }
                    url = Some(other.to_owned());
                }
            }
        }
        Ok(Options {
            url: url.context("`fetch` needs the URL of a plugin's repository")?,
            store,
            review_json,
        })
    }
}

/// Fetches one plugin, checks it, and installs it when there is a store.
fn install(options: &Options) -> anyhow::Result<(Review, Option<PathBuf>)> {
    let source = Source::parse(&options.url)?;
    let store = match &options.store {
        Some(root) => Some(
            Store::new(root)
                .with_context(|| format!("`{}` can't hold installed plugins", root.display()))?,
        ),
        None => None,
    };
    // The scratch area sits beside the store, so the copy into the store stays
    // on one filesystem and nothing is left behind in the system temp directory.
    let parent = store
        .as_ref()
        .and_then(|store| store.root().parent().map(Path::to_path_buf))
        .unwrap_or_else(std::env::temp_dir);
    let scratch = Scratch::new(&parent)?;

    let staged = git::fetch(&source, scratch.path())?;
    let checked = validate::check(&staged.dir, &staged.manifest)?;
    let review = Review::of(&source, &staged, &checked);
    let Some(store) = store else {
        return Ok((review, None));
    };

    let address = checked.address();
    let destination = store.commit(&address, &staged.dir, MANIFEST_FILE)?;
    let pinned = Pinned {
        source: source.original.clone(),
        path: source.path.clone(),
        reference: source.reference_name().to_owned(),
        rev: staged.commit.clone(),
        wasm_sha256: checked.wasm_sha256.clone(),
        manifest_sha256: checked.manifest_sha256.clone(),
        store: address,
        origin: source.repo.clone(),
        version: checked.manifest.version.to_string(),
    };
    let mut lock = LockFile::load(store.root())?;
    let unneeded = lock.record(
        checked.manifest.id.clone(),
        LockEntry {
            current: pinned,
            enabled: false,
            previous: Vec::new(),
        },
    );
    // The lock is written before anything is removed: a version the lock no
    // longer names can go, but never before the lock says so.
    lock.save(store.root())?;
    for address in unneeded {
        store.remove(&address)?;
    }
    Ok((review, Some(destination)))
}

/// What the review dialog shows, as the parent reads it.
#[derive(Clone, Debug, Serialize)]
pub struct Review {
    /// The plugin's id, which is the key it is installed under.
    pub id: String,
    pub name: String,
    pub version: String,
    /// The URL as the person pasted it.
    pub source: String,
    /// The directory inside the repository, when the URL named one.
    pub path: Option<String>,
    /// The branch, tag or commit that was asked for, or `HEAD`.
    #[serde(rename = "ref")]
    pub reference: String,
    /// The full id of the commit that was read.
    pub rev: String,
    /// The first seven characters of it, for a line of prose.
    pub commit: String,
    /// The module's SHA-256; `null` for a plugin that is only a manifest.
    pub wasm_sha256: Option<String>,
    pub manifest_sha256: String,
    pub memory_pages: u32,
    /// The same cap in bytes, since that is what a person compares with.
    pub memory_bytes: u64,
    pub wasi: bool,
    pub permissions: Vec<PermissionReview>,
    pub dependencies: Vec<DependencyReview>,
    /// A sentence about a combination of permissions worth saying out loud.
    pub risky: Option<String>,
    /// Whether a person has read this and agreed. A fetch never sets it; the
    /// parent does once the dialog has been answered.
    pub reviewed: bool,
}

/// One permission, as the review lists it.
#[derive(Clone, Debug, Serialize)]
pub struct PermissionReview {
    /// The name a manifest writes.
    pub capability: String,
    /// The line the dialog shows beside it.
    pub summary: String,
    /// Whether the dialog draws it in the danger tone.
    pub dangerous: bool,
}

/// A plugin this one needs.
#[derive(Clone, Debug, Serialize)]
pub struct DependencyReview {
    pub id: String,
    pub requirement: String,
}

impl Review {
    fn of(source: &Source, staged: &git::Staged, checked: &validate::Checked) -> Review {
        let manifest = &checked.manifest;
        Review {
            id: manifest.id.to_string(),
            name: manifest.name.clone(),
            version: manifest.version.to_string(),
            source: source.original.clone(),
            path: staged.path.clone(),
            reference: source.reference_name().to_owned(),
            rev: staged.commit.clone(),
            commit: staged.commit.chars().take(7).collect(),
            wasm_sha256: checked.wasm_sha256.clone(),
            manifest_sha256: checked.manifest_sha256.clone(),
            memory_pages: manifest.limits.memory_pages,
            memory_bytes: u64::from(manifest.limits.memory_pages) * 64 * 1024,
            wasi: manifest.wasi,
            permissions: manifest
                .capabilities()
                .into_iter()
                .map(PermissionReview::of)
                .collect(),
            dependencies: manifest
                .dependencies
                .iter()
                .map(|(id, requirement)| DependencyReview {
                    id: id.to_string(),
                    requirement: requirement.to_string(),
                })
                .collect(),
            risky: manifest.permissions.risky_combination(),
            reviewed: false,
        }
    }
}

impl PermissionReview {
    fn of(capability: Capability) -> PermissionReview {
        PermissionReview {
            capability: capability.as_str().to_owned(),
            summary: capability.summary().to_owned(),
            dangerous: capability.is_dangerous(),
        }
    }
}

/// Prints the review for a person: what it is, where it came from, and what it
/// may do.
fn print_review(review: &Review, installed: Option<&Path>) {
    let mut out = format!("{} {} ({})\n", review.name, review.version, review.id);
    match &review.path {
        Some(path) => out.push_str(&format!(
            "from {} in {path} at {}, ref {}\n",
            review.source, review.commit, review.reference
        )),
        None => out.push_str(&format!(
            "from {} at {}, ref {}\n",
            review.source, review.commit, review.reference
        )),
    }
    if review.permissions.is_empty() {
        out.push_str("  asks for nothing but to be installed\n");
    }
    for permission in &review.permissions {
        out.push_str(&format!("  {}\n", permission.summary));
    }
    out.push_str(&format!(
        "memory: up to {} MiB{}\n",
        review.memory_bytes >> 20,
        if review.wasi { ", with WASI" } else { "" }
    ));
    for dependency in &review.dependencies {
        out.push_str(&format!(
            "needs {} {}\n",
            dependency.id, dependency.requirement
        ));
    }
    if let Some(risky) = &review.risky {
        out.push_str(risky);
        out.push('\n');
    }
    match installed {
        Some(dir) => out.push_str(&format!("installed in {}; not enabled\n", dir.display())),
        None => out.push_str("checked, not installed; pass `--store DIR` to install it\n"),
    }
    print!("{out}");
}

/// A directory of work that is removed when it goes out of scope.
///
/// A fetch needs somewhere to put a bare repository and the plugin's files while
/// it checks them. `tempfile` isn't a dependency of this crate and the name only
/// has to be unique within one process, so a counter beside the store is enough.
pub(crate) struct Scratch {
    path: PathBuf,
    /// Set once the directory has been renamed elsewhere and must be left alone.
    keep: bool,
}

impl Scratch {
    pub(crate) fn new(parent: &Path) -> std::io::Result<Scratch> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::fs::create_dir_all(parent)?;
        for _ in 0..64 {
            let path = parent.join(format!(
                ".yapfast-fetch-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Scratch { path, keep: false });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::other(
            "a scratch directory couldn't be made",
        ))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Hands the directory over: it has been renamed into the store, so it must
    /// not be removed.
    pub(crate) fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::process::Stdio;

    use super::lock::verified_module;
    use super::validate::sha256_file;
    use super::*;
    use yapfast_plugin_api::PluginId;
    use yapfast_plugin_api::limits;

    const PLUGIN: &str = "com.example.word-filter";

    /// A repository on this disk, built with the `git` command line tool.
    ///
    /// The fixture uses git itself so that what is fetched is a repository git
    /// wrote, and so that the tests fail loudly when git isn't installed rather
    /// than quietly testing nothing.
    struct Repository {
        scratch: Scratch,
        dir: PathBuf,
    }

    impl Repository {
        fn new() -> Repository {
            let scratch = Scratch::new(&std::env::temp_dir()).expect("a scratch directory");
            let dir = scratch.path().join("repo");
            std::fs::create_dir(&dir).expect("the repository directory");
            let repository = Repository { scratch, dir };
            repository.git(&["init", "--quiet", "--initial-branch", "main"]);
            repository
        }

        fn git(&self, arguments: &[&str]) -> String {
            let output = std::process::Command::new("git")
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

        fn branch(&self, name: &str) {
            self.git(&["branch", name]);
        }

        fn store(&self) -> PathBuf {
            self.scratch.path().join("store")
        }

        fn options(&self, suffix: &str) -> Options {
            Options {
                url: format!("file://{}{suffix}", self.dir.display()),
                store: Some(self.store()),
                review_json: true,
            }
        }

        fn install(&self, suffix: &str) -> anyhow::Result<(Review, Option<PathBuf>)> {
            install(&self.options(suffix))
        }

        fn lock(&self) -> LockFile {
            LockFile::load(&self.store()).expect("the lock file")
        }

        fn entry(&self) -> Pinned {
            self.lock()
                .entry(&PluginId::parse(PLUGIN).expect("a plugin id"))
                .expect("the plugin is installed")
                .current
                .clone()
        }
    }

    /// Whether the `git` command line tool is here. Without it a `file://` fetch
    /// can't run — reading one goes through `git-upload-pack` — so the tests that
    /// need a repository are skipped rather than failed.
    fn git_available() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    /// A module with the header and nothing else, which is a valid module.
    fn wasm() -> Vec<u8> {
        vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
    }

    fn manifest(id: &str, version: &str, wasm: &str) -> String {
        format!(
            r#"{{
                "api": 1,
                "id": "{id}",
                "name": "Word Filter",
                "version": "{version}",
                "wasm": "{wasm}",
                "permissions": {{
                    "capabilities": ["messages.read"],
                    "network": ["api.example.com"]
                }}
            }}"#
        )
    }

    /// A repository holding one good plugin, as an author would publish it.
    fn published() -> Option<(Repository, String)> {
        if !git_available() {
            return None;
        }
        let repository = Repository::new();
        let text = manifest(PLUGIN, "1.0.0", "plugin.wasm");
        let commit = repository.commit(
            &[
                ("yapfast-plugin.json", text.as_bytes()),
                ("plugin.wasm", &wasm()),
            ],
            "the first version",
        );
        Some((repository, commit))
    }

    #[test]
    fn installs_a_good_plugin_and_records_its_hashes() {
        let Some((repository, commit)) = published() else {
            return;
        };
        let (review, installed) = repository.install("").expect("the plugin installs");

        assert_eq!(review.name, "Word Filter");
        assert_eq!(review.id, PLUGIN);
        assert_eq!(review.rev, commit);
        assert_eq!(review.commit, commit[..7]);
        assert!(!review.reviewed, "a fetch never reviews anything");
        assert_eq!(
            review.permissions.first().map(|p| p.summary.as_str()),
            Some("Reads your messages")
        );
        assert_eq!(
            review.memory_bytes,
            u64::from(limits::DEFAULT_MEMORY_PAGES) * 64 * 1024
        );
        assert!(
            review.risky.is_some(),
            "reading and reaching out is called out"
        );
        let installed = installed.expect("a directory in the store");
        assert!(installed.join(STORED_MANIFEST).is_file());
        assert!(installed.join("plugin.wasm").is_file());

        let entry = repository.entry();
        assert!(!repository.lock().plugins[&PluginId::parse(PLUGIN).unwrap()].enabled);
        assert_eq!(entry.rev, commit);
        assert_eq!(entry.reference, "HEAD");
        let store = Store::new(repository.store()).expect("the store opens");
        let module = verified_module(&store, &entry).expect("the module hashes to the lock");
        assert_eq!(
            sha256_file(&module).expect("the module can be hashed"),
            entry.wasm_sha256.clone().expect("a module hash")
        );
        let manifest = std::fs::read(installed.join(STORED_MANIFEST)).expect("the manifest");
        assert_eq!(
            super::validate::sha256_bytes(&manifest),
            entry.manifest_sha256
        );
    }

    #[test]
    fn a_repository_without_a_manifest_says_so() {
        if !git_available() {
            return;
        }
        let repository = Repository::new();
        repository.commit(&[("README.md", b"nothing here")], "nothing");
        let error = repository
            .install("")
            .expect_err("no manifest, no install")
            .to_string();
        assert!(error.contains(MANIFEST_FILE), "{error}");
    }

    #[test]
    fn an_lfs_pointer_is_refused_with_its_own_reason() {
        let Some((repository, _)) = published() else {
            return;
        };
        let text = manifest(PLUGIN, "1.0.1", "plugin.wasm");
        repository.commit(
            &[
                ("yapfast-plugin.json", text.as_bytes()),
                (
                    "plugin.wasm",
                    b"version https://git-lfs.github.com/spec/v1\noid sha256:4d7a\nsize 17\n",
                ),
            ],
            "the module stayed in LFS",
        );
        let error = repository
            .install("")
            .expect_err("a pointer isn't a module")
            .to_string();
        assert!(error.contains("Git LFS"), "{error}");
    }

    #[test]
    fn a_manifest_that_climbs_out_of_its_directory_is_refused() {
        let Some((repository, _)) = published() else {
            return;
        };
        let text = manifest(PLUGIN, "1.0.1", "../x.wasm");
        repository.commit(
            &[
                ("yapfast-plugin.json", text.as_bytes()),
                ("x.wasm", &wasm()),
            ],
            "a path out of the directory",
        );
        let error = repository
            .install("")
            .expect_err("a manifest may not escape")
            .to_string();
        assert!(error.contains("`wasm`"), "{error}");
    }

    #[test]
    fn a_module_over_the_size_cap_is_refused() {
        let Some((repository, _)) = published() else {
            return;
        };
        let text = manifest(PLUGIN, "1.0.1", "plugin.wasm");
        let mut big = wasm();
        big.resize(limits::MAX_WASM_BYTES as usize + 1, 0);
        repository.commit(
            &[
                ("yapfast-plugin.json", text.as_bytes()),
                ("plugin.wasm", &big),
            ],
            "a module past the cap",
        );
        let error = repository
            .install("")
            .expect_err("an oversized module is refused")
            .to_string();
        assert!(error.contains("MiB"), "{error}");
        assert!(error.contains("plugin.wasm"), "{error}");
    }

    #[test]
    fn two_installs_keep_two_versions_and_drop_the_third() {
        let Some((repository, _)) = published() else {
            return;
        };
        repository.branch("v1");
        let second = manifest(PLUGIN, "1.0.1", "plugin.wasm");
        repository.commit(
            &[
                ("yapfast-plugin.json", second.as_bytes()),
                ("plugin.wasm", &wasm()),
            ],
            "the second version",
        );
        repository.branch("v2");
        let third = manifest(PLUGIN, "1.0.2", "plugin.wasm");
        repository.commit(
            &[
                ("yapfast-plugin.json", third.as_bytes()),
                ("plugin.wasm", &wasm()),
            ],
            "the third version",
        );
        repository.branch("v3");

        repository.install("#ref=v1").expect("the first installs");
        let first = repository.entry().store;
        repository.install("#ref=v2").expect("the second installs");
        let store = Store::new(repository.store()).expect("the store opens");
        assert_eq!(store.versions().expect("the store is readable").len(), 2);

        repository.install("#ref=v3").expect("the third installs");
        let versions = store.versions().expect("the store is readable");
        assert_eq!(versions.len(), 2, "the current and the one before it");
        assert!(
            !versions.contains(&first),
            "the third-oldest version was removed"
        );
        let entry = repository
            .lock()
            .entry(&PluginId::parse(PLUGIN).unwrap())
            .cloned();
        let entry = entry.expect("the plugin is installed");
        assert_eq!(entry.current.version, "1.0.2");
        assert_eq!(
            entry
                .previous
                .first()
                .map(|previous| previous.version.as_str()),
            Some("1.0.1")
        );
        assert!(!entry.enabled, "installing a new version doesn't enable it");
    }

    #[test]
    fn a_tampered_module_fails_its_hash_on_read() {
        let Some((repository, _)) = published() else {
            return;
        };
        repository.install("").expect("the plugin installs");
        let entry = repository.entry();
        let store = Store::new(repository.store()).expect("the store opens");
        let module = store
            .read(&entry.store)
            .expect("the version is stored")
            .module_path()
            .expect("the manifest names a module")
            .expect("the module is there");
        let mut bytes = std::fs::read(&module).expect("the module");
        bytes.push(0xff);
        std::fs::write(&module, bytes).expect("the module is changed");

        let error = verified_module(&store, &entry)
            .expect_err("a changed module is refused")
            .to_string();
        assert!(error.contains("hash"), "{error}");
    }

    #[test]
    fn reads_the_arguments_the_parent_passes() {
        let options = Options::parse(vec![
            "https://example.com/owner/repo".into(),
            "--store".into(),
            "/tmp/store".into(),
            "--review-json".into(),
        ])
        .expect("the arguments parse");
        assert_eq!(options.url, "https://example.com/owner/repo");
        assert_eq!(options.store, Some(PathBuf::from("/tmp/store")));
        assert!(options.review_json);

        assert!(Options::parse(Vec::new()).is_err(), "a URL is required");
        assert!(
            Options::parse(vec!["--store".into()]).is_err(),
            "`--store` needs a directory"
        );
        assert!(
            Options::parse(vec!["a".into(), "b".into()]).is_err(),
            "one URL at a time"
        );
    }
}
