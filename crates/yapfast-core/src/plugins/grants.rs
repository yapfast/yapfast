//! What a person allowed a plugin, per host, and which of those survive a
//! restart.
//!
//! A manifest lists the hosts a plugin means to reach; a person may still need
//! to agree to one, and may take that agreement back. What they answered is
//! kept here, in `grants.json` in the plugin directory, so a decision made once
//! is not asked for again on every start, and the answers that were only meant
//! for this run are kept in memory where they vanish when the app exits.
//!
//! Only hosts are stored, never URLs and never a token: an allow for
//! `api.example.com` says which server may be contacted, and says nothing about
//! what may be sent to it. The gate consults this on every request rather than
//! remembering an answer, so taking a grant back stops the next request and not
//! the next start.
//!
//! The file is written atomically — beside it, then renamed over it — so a
//! crash mid-write leaves the previous answers rather than half of a file. A
//! file that isn't the shape this build writes is not trusted at all: it is
//! moved aside with its contents intact and a line in the log, and the app
//! starts with nothing granted, because a grants file that can't be read is a
//! reason to ask the person again, never a reason to guess.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::capability::HostPattern;
use std::time::{Duration, Instant};

use yapfast_plugin_api::limits;

/// The shape of `grants.json`. A file with another number was written by
/// another version and is refused rather than read as if it were this one.
const VERSION: u32 = 1;

/// How long an answer lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lasts {
    /// Until the app exits.
    Session,
    /// In `grants.json`, so it is still there next time.
    Stored,
}

/// One plugin's hosts, as the file holds them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Held {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    allowed: BTreeSet<HostPattern>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    denied: BTreeSet<HostPattern>,
}

/// The file's contents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    version: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    plugins: BTreeMap<PluginId, Held>,
}

impl Default for Stored {
    fn default() -> Self {
        Stored {
            version: VERSION,
            plugins: BTreeMap::new(),
        }
    }
}

/// What this session added to the file's answers.
#[derive(Clone, Debug, Default)]
struct Session {
    allowed: BTreeSet<HostPattern>,
    denied: BTreeSet<HostPattern>,
}

/// A plugin as this session has seen it: the answers, and what is on screen.
#[derive(Clone, Debug, Default)]
struct Runtime {
    session: Session,
    /// Dialogs the person answered no to, which is what silences the rest.
    denials: u32,
    /// Prompts refused without asking, once its dialogs were silenced.
    silenced: u32,
    /// The host whose dialog is open, if one is, and when it opened. A dialog
    /// left unanswered stops counting after [`limits::CONSENT_WINDOW_MS`], so a
    /// window closed by accident doesn't silence the plugin until someone edits
    /// `grants.json`.
    open: Option<HostPattern>,
    opened_at: Option<Instant>,
}

#[derive(Debug, Default)]
struct State {
    stored: Stored,
    runtime: BTreeMap<PluginId, Runtime>,
}

/// What a request for one host is waiting on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// The person said yes, this run or an earlier one.
    Allowed,
    /// The person said no, or the plugin has used up its prompts.
    Denied { reason: String },
    /// A dialog about this host is already up; the same answer completes this
    /// request, so nothing is asked twice.
    Joined,
    /// Nothing about this host is on screen: open a dialog for it.
    Asked,
    /// A dialog about another host for this plugin is up. Only one is open at a
    /// time, so the person answers about one server at a time.
    Busy,
}

/// The answers a plugin has, per host.
#[derive(Debug)]
pub struct Grants {
    path: PathBuf,
    state: RwLock<State>,
}

impl Grants {
    /// Reads `grants.json`, or starts with nothing granted.
    ///
    /// A missing file is a fresh start; a file that can't be read, or isn't
    /// this shape, is moved aside as `grants.json.invalid` and logged, so the
    /// answers it holds are still on disk for the person to look at while the
    /// app asks again. Only an unreadable directory is an error, since then
    /// nothing saved later would survive either.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let stored = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Stored>(&bytes) {
                Ok(stored) if stored.version == VERSION => stored,
                Ok(stored) => {
                    quarantine(
                        &path,
                        &format!(
                            "it says version {}, and this build writes {VERSION}",
                            stored.version
                        ),
                    );
                    Stored::default()
                }
                Err(error) => {
                    quarantine(&path, &error.to_string());
                    Stored::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Stored::default(),
            Err(error) => {
                return Err(error).context("could not read the plugins' grants file");
            }
        };
        Ok(Grants {
            path,
            state: RwLock::new(State {
                stored,
                runtime: BTreeMap::new(),
            }),
        })
    }

    /// Where the answers are written.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What the person has answered about `host`, or what asking would do.
    ///
    /// A host with no answer opens a dialog only while the plugin's prompts
    /// last: after [`limits::MAX_DENIALS_BEFORE_SILENCE`] refusals in one
    /// session the request is denied without a dialog, because a plugin that
    /// keeps asking is asking a person who has already said no.
    pub fn ask(&self, plugin: &PluginId, host: &HostPattern) -> Ask {
        let mut state = self.state.write().unwrap_or_else(PoisonError::into_inner);
        let stored = state
            .stored
            .plugins
            .get(plugin)
            .cloned()
            .unwrap_or_default();
        let runtime = state.runtime.entry(plugin.clone()).or_default();

        // A refusal outlives everything else about a host: an allow that was
        // added after it wins, because the mutators remove one when adding the
        // other.
        if stored.denied.contains(host) || runtime.session.denied.contains(host) {
            return Ask::Denied {
                reason: format!("you refused {host} for this plugin"),
            };
        }
        if stored.allowed.contains(host) || runtime.session.allowed.contains(host) {
            return Ask::Allowed;
        }
        if runtime.denials >= limits::MAX_DENIALS_BEFORE_SILENCE {
            runtime.silenced = runtime.silenced.saturating_add(1);
            return Ask::Denied {
                reason: format!(
                    "this plugin has had {} requests refused, so Yapfast no longer asks; \
                     you can allow {host} from the plugin's page",
                    runtime.denials
                ),
            };
        }
        // A dialog that has been open longer than the window is treated as
        // dismissed: the person isn't looking at it, and treating it as an
        // answer would refuse the host for good on nobody's word.
        if let Some(opened_at) = runtime.opened_at
            && opened_at.elapsed() >= Duration::from_millis(limits::CONSENT_WINDOW_MS)
        {
            runtime.open = None;
            runtime.opened_at = None;
        }
        match &runtime.open {
            Some(open) if open == host => Ask::Joined,
            Some(_) => Ask::Busy,
            None => {
                runtime.open = Some(host.clone());
                runtime.opened_at = Some(Instant::now());
                Ask::Asked
            }
        }
    }

    /// Records the person's answer to a dialog and closes it.
    ///
    /// A refusal counts towards [`limits::MAX_DENIALS_BEFORE_SILENCE`], and an
    /// allow clears the count for that host by removing the refusal it
    /// contradicts. Dismissing a dialog without answering is a refusal: the
    /// plugin asked, and nobody said yes, so the question is not put again for
    /// the same host until the person allows it from the plugin's page.
    pub fn answer(
        &self,
        plugin: &PluginId,
        host: &HostPattern,
        allow: bool,
        lasts: Lasts,
    ) -> Result<()> {
        if allow {
            self.allow(plugin, host, lasts)
        } else {
            self.deny(plugin, host, lasts)?;
            let mut state = self.state.write().unwrap_or_else(PoisonError::into_inner);
            let runtime = state.runtime.entry(plugin.clone()).or_default();
            runtime.denials = runtime.denials.saturating_add(1);
            Ok(())
        }
    }

    /// Allows `host` for `plugin`, and saves the file when the answer lasts.
    pub fn allow(&self, plugin: &PluginId, host: &HostPattern, lasts: Lasts) -> Result<()> {
        self.change(plugin, host, lasts, |held, session| {
            held.denied.remove(host);
            session.denied.remove(host);
            match lasts {
                Lasts::Session => {
                    session.allowed.insert(host.clone());
                }
                Lasts::Stored => {
                    held.allowed.insert(host.clone());
                }
            }
        })
    }

    /// Refuses `host` for `plugin`, and saves the file when the answer lasts.
    pub fn deny(&self, plugin: &PluginId, host: &HostPattern, lasts: Lasts) -> Result<()> {
        self.change(plugin, host, lasts, |held, session| {
            held.allowed.remove(host);
            session.allowed.remove(host);
            match lasts {
                Lasts::Session => {
                    session.denied.insert(host.clone());
                }
                Lasts::Stored => {
                    held.denied.insert(host.clone());
                }
            }
        })
    }

    /// Takes back everything known about `host`, and refuses it from now on.
    ///
    /// The refusal is for this session, so the grant is gone from the file as
    /// well and a restart asks again; until then, a request already in flight
    /// finds nothing that would let it through.
    pub fn revoke(&self, plugin: &PluginId, host: &HostPattern) -> Result<()> {
        let stored_changed = {
            let mut state = self.state.write().unwrap_or_else(PoisonError::into_inner);
            {
                let runtime = state.runtime.entry(plugin.clone()).or_default();
                runtime.session.allowed.remove(host);
                runtime.session.denied.insert(host.clone());
                if runtime.open.as_ref() == Some(host) {
                    runtime.open = None;
                }
            }
            match state.stored.plugins.get_mut(plugin) {
                Some(held) => {
                    let changed = held.allowed.remove(host) | held.denied.remove(host);
                    if held.allowed.is_empty() && held.denied.is_empty() {
                        state.stored.plugins.remove(plugin);
                    }
                    changed
                }
                None => false,
            }
        };
        if stored_changed { self.save() } else { Ok(()) }
    }

    /// Every host `plugin` was allowed, for the page that lists them.
    pub fn allowed(&self, plugin: &PluginId) -> BTreeSet<HostPattern> {
        let state = self.state.read().unwrap_or_else(PoisonError::into_inner);
        let mut hosts = state
            .stored
            .plugins
            .get(plugin)
            .map(|held| held.allowed.clone())
            .unwrap_or_default();
        if let Some(runtime) = state.runtime.get(plugin) {
            hosts.extend(runtime.session.allowed.iter().cloned());
        }
        hosts
    }

    /// A sentence about the requests this plugin can no longer ask about, for
    /// the page that lists it; `None` while it may still ask.
    pub fn summary(&self, plugin: &PluginId) -> Option<String> {
        let state = self.state.read().unwrap_or_else(PoisonError::into_inner);
        let runtime = state.runtime.get(plugin)?;
        if runtime.silenced == 0 {
            return None;
        }
        Some(format!(
            "{} requests were refused without asking, after {} refusals",
            runtime.silenced, runtime.denials
        ))
    }

    /// Forgets a plugin's session answers and its dialogs, for when it is
    /// turned off.
    pub fn forget(&self, plugin: &PluginId) {
        let mut state = self.state.write().unwrap_or_else(PoisonError::into_inner);
        state.runtime.remove(plugin);
    }

    /// Writes the file, replacing it whole.
    pub fn save(&self) -> Result<()> {
        let mut state = self.state.write().unwrap_or_else(PoisonError::into_inner);
        state.stored.version = VERSION;
        // Refusals and allows for one host contradict each other; the mutators
        // keep them apart, and this keeps a hand-edited file honest.
        state.stored.plugins.retain(|_, held| {
            held.allowed.retain(|host| !held.denied.contains(host));
            !held.allowed.is_empty() || !held.denied.is_empty()
        });
        let text = serde_json::to_vec_pretty(&state.stored)
            .context("could not write the plugins' grants as JSON")?;
        write_atomically(&self.path, &text)
    }

    /// Applies one answer to both views of a plugin's hosts, then saves when
    /// the answer is meant to survive.
    fn change(
        &self,
        plugin: &PluginId,
        host: &HostPattern,
        lasts: Lasts,
        edit: impl FnOnce(&mut Held, &mut Session),
    ) -> Result<()> {
        {
            let mut state = self.state.write().unwrap_or_else(PoisonError::into_inner);
            let mut held = state
                .stored
                .plugins
                .get(plugin)
                .cloned()
                .unwrap_or_default();
            let runtime = state.runtime.entry(plugin.clone()).or_default();
            edit(&mut held, &mut runtime.session);
            // An answer about a host, from a dialog or from the plugin's page,
            // ends the question about it: only one dialog per plugin is open at
            // a time, and this is what lets the next host be asked about even
            // when nobody answered the dialog itself.
            if runtime.open.as_ref() == Some(host) {
                runtime.open = None;
            }
            if held.allowed.is_empty() && held.denied.is_empty() {
                state.stored.plugins.remove(plugin);
            } else {
                state.stored.plugins.insert(plugin.clone(), held);
            }
        }
        match lasts {
            Lasts::Session => Ok(()),
            Lasts::Stored => self.save(),
        }
    }
}

/// Moves a file this build can't read out of the way, keeping its contents.
fn quarantine(path: &Path, why: &str) {
    let aside = path.with_extension("json.invalid");
    tracing::warn!(
        path = %path.display(),
        why,
        kept_at = %aside.display(),
        "the plugins' grants file isn't the one this build writes; starting with nothing granted"
    );
    // The rename can fail on a read-only directory; the log line has already
    // said what was wrong, and losing it is not worth failing the start.
    if let Err(error) = std::fs::rename(path, &aside) {
        tracing::warn!(why = %error, "could not move the unreadable grants file aside");
    }
}

/// Writes `bytes` to `path` so that a crash leaves either file whole.
///
/// The temporary file is in the same directory, so the rename is one `rename`
/// within one filesystem, and a reader never sees a half-written file.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(directory)
        .with_context(|| format!("could not make {}", directory.display()))?;
    let mut file = tempfile::NamedTempFile::new_in(directory)
        .with_context(|| format!("could not write beside {}", path.display()))?;
    std::io::Write::write_all(&mut file, bytes)
        .with_context(|| format!("could not write {}", path.display()))?;
    file.as_file()
        .sync_all()
        .with_context(|| format!("could not flush {}", path.display()))?;
    file.persist(path)
        .map(|_| ())
        .with_context(|| format!("could not replace {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(text: &str) -> PluginId {
        PluginId::parse(text).expect("a test id is valid")
    }

    fn host(text: &str) -> HostPattern {
        HostPattern::parse(text).expect("a test host is valid")
    }

    #[test]
    fn an_answer_survives_a_restart_and_a_session_one_does_not() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("grants.json");
        let plugin = id("com.example.weather");
        {
            let grants = Grants::load(&path).expect("a fresh file");
            grants
                .answer(&plugin, &host("api.example.com"), true, Lasts::Stored)
                .expect("stored");
            grants
                .answer(&plugin, &host("cdn.example.com"), true, Lasts::Session)
                .expect("session");
            assert_eq!(grants.ask(&plugin, &host("api.example.com")), Ask::Allowed);
        }
        let reopened = Grants::load(&path).expect("the file it wrote");
        let stored = host("api.example.com");
        let session = host("cdn.example.com");
        assert_eq!(reopened.ask(&plugin, &stored), Ask::Allowed);
        assert_eq!(
            reopened.ask(&plugin, &session),
            Ask::Asked,
            "a session answer is gone with the process"
        );
    }

    #[test]
    fn a_file_that_isnt_this_shape_starts_empty_and_keeps_its_contents() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("grants.json");
        let nonsense = br#"{"version": 99, "plugins": {"com.example.weather": {}}}"#;
        std::fs::write(&path, nonsense).expect("a file to refuse");
        let grants = Grants::load(&path).expect("loading is forgiving");
        let plugin = id("com.example.weather");
        assert_eq!(grants.ask(&plugin, &host("api.example.com")), Ask::Asked);
        assert!(
            !path.exists(),
            "the refused file is not left to be read again"
        );
        let kept = path.with_extension("json.invalid");
        assert_eq!(
            std::fs::read(&kept).expect("the refused file is kept"),
            nonsense,
            "the contents are kept for the person to look at"
        );
    }

    #[test]
    fn only_one_dialog_per_plugin_and_the_same_host_joins_it() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let grants = Grants::load(directory.path().join("grants.json")).expect("a fresh file");
        let plugin = id("com.example.weather");
        let first = host("api.example.com");
        let second = host("cdn.example.com");
        assert_eq!(grants.ask(&plugin, &first), Ask::Asked);
        assert_eq!(grants.ask(&plugin, &first), Ask::Joined);
        assert_eq!(grants.ask(&plugin, &second), Ask::Busy);
        grants
            .answer(&plugin, &first, true, Lasts::Session)
            .expect("an answer");
        assert_eq!(grants.ask(&plugin, &first), Ask::Allowed);
        assert_eq!(
            grants.ask(&plugin, &second),
            Ask::Asked,
            "once the dialog closes, the next host is asked about"
        );
    }

    #[test]
    fn enough_refusals_silence_the_plugin_and_say_so() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let grants = Grants::load(directory.path().join("grants.json")).expect("a fresh file");
        let plugin = id("com.example.weather");
        for round in 0..limits::MAX_DENIALS_BEFORE_SILENCE {
            let asked = host(&format!("host{round}.example.com"));
            assert_eq!(grants.ask(&plugin, &asked), Ask::Asked);
            grants
                .answer(&plugin, &asked, false, Lasts::Session)
                .expect("an answer");
        }
        let next = host("late.example.com");
        let Ask::Denied { reason } = grants.ask(&plugin, &next) else {
            panic!("a silenced plugin is refused without a dialog");
        };
        assert!(reason.contains("no longer asks"), "{reason}");
        assert!(grants.summary(&plugin).is_some());
        // The sentinel: a plugin that was never refused still gets its dialog.
        assert_eq!(grants.summary(&id("com.example.other")), None);
    }

    #[test]
    fn a_revoked_grant_refuses_the_next_request_and_leaves_no_allow_behind() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("grants.json");
        let grants = Grants::load(&path).expect("a fresh file");
        let plugin = id("com.example.weather");
        let api = host("api.example.com");
        grants
            .answer(&plugin, &api, true, Lasts::Stored)
            .expect("stored");
        assert_eq!(grants.ask(&plugin, &api), Ask::Allowed);
        grants.revoke(&plugin, &api).expect("a revocation");
        let Ask::Denied { reason } = grants.ask(&plugin, &api) else {
            panic!("a revoked grant refuses the next request");
        };
        assert!(reason.contains("api.example.com"), "{reason}");
        assert!(!grants.allowed(&plugin).contains(&api));
        let reopened = Grants::load(&path).expect("the file it wrote");
        assert_eq!(
            reopened.ask(&plugin, &api),
            Ask::Asked,
            "a revocation is not a stored refusal: a restart asks again"
        );
    }

    #[test]
    fn a_stored_refusal_needs_no_second_dialog() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("grants.json");
        let plugin = id("com.example.weather");
        let api = host("api.example.com");
        {
            let grants = Grants::load(&path).expect("a fresh file");
            grants
                .answer(&plugin, &api, false, Lasts::Stored)
                .expect("stored");
        }
        let reopened = Grants::load(&path).expect("the file it wrote");
        assert!(matches!(reopened.ask(&plugin, &api), Ask::Denied { .. }));
    }
}
