//! What Settings → Plugins shows of the runtime: a copy of it the page holds
//! while it is open.
//!
//! The page's [`View`] is plain data, so this builds one from what the runtime
//! knows — the registry's entries, the store's lock file and settings, the
//! grants — and the page reads it without touching a lock. Nothing here starts
//! a plugin or a process.
//!
//! The pieces the milestones after M1 bring are left at their empty values
//! rather than guessed: a marketplace catalog, an update to roll back to, and
//! a plugin's icon. The page draws those parts only when they are there.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, PoisonError};
use std::time::SystemTime;

use yapfast_plugin_api::capability::Capability;
use yapfast_plugin_api::message::PluginStatus;

use crate::bridge::{self, Bridge};
use crate::grants::Grants;
use crate::page::{Advanced, Grant, Helper, Installed, LogLine, Origin, Status, Suspension, View};
use crate::registry::Listed;
use crate::settings::{Form, Schema, Values};
use crate::store::{Lock, LockEntry, Store};
use yapfast_plugin_api::PluginId;

/// What the page reads from disk, kept between draws.
///
/// The page is drawn again for every plugin status and log line while it is
/// open, on the thread that draws, and each draw read `lock.json`, every
/// manifest, every icon and every settings file. Now a draw looks at two
/// modification times: the store's directories are named by their content, so
/// a plugin's icon and settings schema are read once per install, and the lock
/// and settings files are read again only when they changed on disk.
#[derive(Default)]
pub(crate) struct DiskCache {
    lock: Option<(Option<SystemTime>, Arc<Lock>)>,
    plugins: HashMap<PluginId, Disk>,
}

/// One installed plugin's files, as the page last read them.
struct Disk {
    /// The install these were read from: the store directory and the manifest
    /// it was reviewed with, which together name its files.
    install: (String, String),
    icon: Option<Arc<[u8]>>,
    schema: Arc<Schema>,
    /// The settings file's modification time when `values` were read from it.
    settings: Option<(Option<SystemTime>, Values)>,
}

impl DiskCache {
    /// The lock file, read again only if it changed since the last draw.
    fn lock(&mut self, store: &Store) -> Option<Arc<Lock>> {
        let modified = modified(&store.lock_path());
        if let Some((seen, lock)) = &self.lock
            && *seen == modified
            && modified.is_some()
        {
            return Some(Arc::clone(lock));
        }
        let lock = Arc::new(store.lock().ok()?);
        self.lock = Some((modified, Arc::clone(&lock)));
        Some(lock)
    }

    /// One plugin's icon and settings form, read from disk only for what
    /// changed since the last draw.
    fn plugin(
        &mut self,
        store: &Store,
        plugin: &PluginId,
        manifest: &yapfast_plugin_api::PluginManifest,
        entry: &LockEntry,
    ) -> (Option<Arc<[u8]>>, Form) {
        let install = (entry.store.clone(), entry.manifest_sha256.clone());
        let disk = match self.plugins.get_mut(plugin) {
            Some(disk) if disk.install == install && !install.0.is_empty() => disk,
            _ => {
                let schema = store
                    .manifest_text(entry)
                    .ok()
                    .and_then(|text| crate::Manifest::parse(&text).ok())
                    .map(|manifest| Arc::clone(&manifest.settings))
                    .unwrap_or_default();
                let disk = Disk {
                    install,
                    icon: store.icon(entry, manifest).map(Arc::from),
                    schema,
                    settings: None,
                };
                self.plugins.insert(plugin.clone(), disk);
                self.plugins.get_mut(plugin).expect("just inserted")
            }
        };
        let modified = modified(&store.settings_path(plugin));
        let values = match &disk.settings {
            Some((seen, values)) if *seen == modified => values.clone(),
            _ => {
                let stored = store.settings(plugin, manifest);
                let (values, _refused) = disk.schema.read(&stored);
                disk.settings = Some((modified, values.clone()));
                values
            }
        };
        (
            disk.icon.clone(),
            Form::new(Arc::clone(&disk.schema), values),
        )
    }

    /// Forgets the plugins that are no longer installed.
    fn keep(&mut self, installed: &[Listed]) {
        self.plugins
            .retain(|id, _| installed.iter().any(|listed| &listed.manifest.id == id));
    }
}

/// When a file last changed, or `None` when it isn't there.
fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

impl Bridge {
    /// What Settings → Plugins shows: a row per installed plugin, what each is
    /// doing, and what the helper is doing.
    ///
    /// `clock` writes a log line's time, because a clock is the application's:
    /// its hour format and its time zone are settings this crate doesn't hold.
    pub fn page(&self, grants: &Grants, clock: &dyn Fn(u64) -> String) -> View {
        let limits = self.limits();
        View {
            helper: helper(self.helper()),
            installed: self.installed(grants, clock),
            advanced: Advanced {
                memory_budget: limits.memory_mib,
                idle_minutes: limits.idle_minutes,
                // What the helper said in its `Ready` (PLAN.md 9.5): a part of
                // the OS sandbox it couldn't apply, on this system, is named in
                // the log, and here the dialog says the plugins run under
                // wasmtime's sandbox alone.
                sandbox_unavailable: self.shared().sandbox_unavailable(),
            },
            ..View::default()
        }
    }

    /// One row per installed plugin, in id order.
    fn installed(&self, grants: &Grants, clock: &dyn Fn(u64) -> String) -> Vec<Installed> {
        let shared = self.shared();
        let store = shared.store();
        let mut cache = self
            .page_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let lock = cache.lock(&store);
        let listed = self.listed();
        cache.keep(&listed);
        listed
            .into_iter()
            .map(|listed| {
                let id = &listed.manifest.id;
                let entry = lock
                    .as_ref()
                    .and_then(|lock| lock.plugins.get(id).cloned())
                    .unwrap_or_default();
                let (icon, settings) = cache.plugin(&store, id, &listed.manifest, &entry);
                Installed {
                    id: id.to_string(),
                    name: listed.manifest.name.clone(),
                    description: listed.manifest.description.clone().unwrap_or_default(),
                    authors: listed.manifest.authors.clone(),
                    version: listed.manifest.version.to_string(),
                    icon,
                    enabled: listed.enabled,
                    status: status(&listed.status),
                    update: None,
                    previous: None,
                    origin: if entry.source.is_empty() {
                        Origin::Folder
                    } else {
                        Origin::Url
                    },
                    source: entry.source.clone(),
                    path: entry.path.clone(),
                    commit: entry.rev.clone(),
                    license: listed.manifest.license.clone().unwrap_or_default(),
                    capabilities: listed
                        .manifest
                        .permissions
                        .capabilities
                        .iter()
                        .map(Capability::to_string)
                        .collect(),
                    grants: granted(grants, &listed),
                    memory_pages: listed.manifest.limits.memory_pages,
                    quiet_denials: grants.quiet_denials(&listed.manifest.id),
                    settings,
                    log: listed
                        .log
                        .iter()
                        .map(|record| log_line(record, clock))
                        .collect(),
                }
            })
            .collect()
    }
}

/// The helper's state, as the page names it.
fn helper(state: bridge::HelperState) -> Helper {
    match state {
        bridge::HelperState::Stopped => Helper::Stopped,
        bridge::HelperState::Running { memory } => Helper::Running { memory },
        bridge::HelperState::Missing => Helper::Missing,
        bridge::HelperState::Crashed => Helper::Crashed,
    }
}

/// What one plugin is doing, as the page names it.
fn status(status: &PluginStatus) -> Status {
    match status {
        PluginStatus::Installed => Status::Off,
        PluginStatus::Enabled => Status::Idle,
        PluginStatus::Active => Status::Running,
        // A fault is retried on the plugin's next trigger; a suspension is the
        // three-strike stop. Saying "suspended" for the first was wrong, and told
        // a person their plugin was off when it was still on.
        PluginStatus::Faulted { reason, .. } => Status::Faulted(reason.clone()),
        PluginStatus::Suspended { reason } => Status::Suspended(suspension(reason)),
    }
}

/// Why an instance went away.
///
/// The status carries the runtime's own sentence rather than a kind, so the
/// sentence decides between the three states a person acts on differently —
/// running out of pages, running too long — and everything else is shown in the
/// runtime's words rather than flattened into "crashed": an activation that
/// returned a status and an answer the ABI refused are both worth reading, and
/// only the helper knows which happened.
fn suspension(reason: &str) -> Suspension {
    let lowered = reason.to_ascii_lowercase();
    if lowered.contains("memory") || lowered.contains("oom") {
        Suspension::OutOfMemory
    } else if lowered.contains("deadline") || lowered.contains("took") {
        Suspension::TooSlow
    } else if reason.trim().is_empty() {
        Suspension::Crashed
    } else {
        Suspension::Failed(reason.trim().to_owned())
    }
}

/// The hosts a plugin may reach: the ones its manifest declares, and the ones
/// the person allowed that it didn't, which is how a plugin gets a host after
/// asking.
fn granted(grants: &Grants, listed: &Listed) -> Vec<Grant> {
    let allowed = grants.allowed(&listed.manifest.id);
    let declared = &listed.manifest.permissions.network;
    let mut rows: Vec<Grant> = declared
        .iter()
        .map(|host| Grant {
            host: host.to_string(),
            declared: true,
            allowed: allowed.contains(host),
            remembered: true,
        })
        .collect();
    for pattern in &allowed {
        if !declared.contains(pattern) {
            rows.push(Grant {
                host: pattern.to_string(),
                declared: false,
                allowed: true,
                remembered: true,
            });
        }
    }
    rows
}

/// A plugin's own settings, with the values it was given.
///
/// The interface reads the manifest itself, for the `settings` section the
/// runtime's copy leaves out: a form with no schema rather than none at all is
/// what a manifest that can't be read leaves.
pub(crate) fn form(
    store: &mut crate::store::Store,
    plugin: &yapfast_plugin_api::PluginId,
    manifest: &yapfast_plugin_api::PluginManifest,
    entry: &crate::store::LockEntry,
) -> Form {
    let schema = store
        .manifest_text(entry)
        .ok()
        .and_then(|text| crate::Manifest::parse(&text).ok())
        .map(|manifest| Arc::clone(&manifest.settings))
        .unwrap_or_default();
    let stored = store.settings(plugin, manifest);
    let (values, _refused) = schema.read(&stored);
    Form::new(schema, values)
}

/// One of a plugin's log lines, as the page shows it: the time the application
/// writes, and what it said.
fn log_line(
    record: &yapfast_plugin_api::message::LogRecord,
    clock: &dyn Fn(u64) -> String,
) -> LogLine {
    LogLine {
        at: clock(record.at),
        text: format!("{}: {}", record.target, record.message),
        error: record.level == yapfast_plugin_api::message::LogLevel::Error,
    }
}
