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

use std::sync::Arc;

use yapfast_plugin_api::capability::Capability;
use yapfast_plugin_api::message::PluginStatus;

use crate::plugins::bridge::{self, Bridge};
use crate::plugins::grants::Grants;
use crate::plugins::page::{Grant, Helper, Installed, LogLine, Origin, Status, Suspension, View};
use crate::plugins::registry::Listed;
use crate::plugins::settings::Form;

impl Bridge {
    /// What Settings → Plugins shows: a row per installed plugin, what each is
    /// doing, and what the helper is doing.
    pub fn page(&self, grants: &Grants) -> View {
        View {
            helper: helper(self.helper()),
            installed: self.installed(grants),
            ..View::default()
        }
    }

    /// One row per installed plugin, in id order.
    fn installed(&self, grants: &Grants) -> Vec<Installed> {
        let shared = self.shared();
        let mut store = shared.store();
        let lock = store.lock().ok();
        self.listed()
            .into_iter()
            .map(|listed| {
                let id = &listed.manifest.id;
                let entry = lock
                    .as_ref()
                    .and_then(|lock| lock.plugins.get(id).cloned())
                    .unwrap_or_default();
                Installed {
                    id: id.to_string(),
                    name: listed.manifest.name.clone(),
                    description: listed.manifest.description.clone().unwrap_or_default(),
                    authors: listed.manifest.authors.clone(),
                    version: listed.manifest.version.to_string(),
                    icon: None,
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
                    quiet_denials: 0,
                    settings: settings(&mut store, &listed, &entry),
                    log: listed.log.iter().map(log_line).collect(),
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
        PluginStatus::Faulted { reason, .. } | PluginStatus::Suspended { reason } => {
            Status::Suspended(suspension(reason))
        }
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
fn settings(
    store: &mut crate::plugins::store::Store,
    listed: &Listed,
    entry: &crate::plugins::store::LockEntry,
) -> Form {
    let schema = store
        .manifest_text(entry)
        .ok()
        .and_then(|text| crate::plugins::Manifest::parse(&text).ok())
        .map(|manifest| Arc::clone(&manifest.settings))
        .unwrap_or_default();
    let stored = store.settings(&listed.manifest.id, &listed.manifest);
    let (values, _refused) = schema.read(&stored);
    Form::new(schema, values)
}

/// One of a plugin's log lines, as the page shows it: local clock time, and
/// what it said.
fn log_line(record: &yapfast_plugin_api::message::LogRecord) -> LogLine {
    LogLine {
        at: crate::clock::time_with(record.at / 1000, false, true),
        text: format!("{}: {}", record.target, record.message),
        error: record.level == yapfast_plugin_api::message::LogLevel::Error,
    }
}
