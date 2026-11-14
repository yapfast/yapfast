use super::*;

impl Drop for Supervisor {
    /// Forgets the helper's process id: the task is what holds the process, so
    /// once this is dropped the process is ending, whether it was asked to or
    /// the task was aborted because nothing is enabled any more. The page would
    /// otherwise show a helper that isn't there.
    fn drop(&mut self) {
        self.shared.set_helper_pid(None);
    }
}

/// How much memory a process has resident, in bytes.
///
/// Linux reports it in `/proc/<pid>/status`; elsewhere this doesn't read it,
/// and the page says the helper is running without a number.
pub(super) fn resident_memory(pid: u32) -> Option<u64> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kilobytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kilobytes * 1024)
}

/// Milliseconds since the Unix epoch, which is what a topic event carries.
pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// A fault's sentence, as a line in the plugin's own log: the row that says
/// "Suspended: kept crashing" has the why under it, and the detail's log keeps
/// it after the status has moved on.
pub(super) fn failure(reason: &str) -> LogRecord {
    LogRecord {
        level: LogLevel::Error,
        target: "yapfast".to_owned(),
        message: reason.to_owned(),
        at: now_ms(),
    }
}

/// Notes a failure in the plugin's log, through the registry's ring.
pub(super) fn note_failure(shared: &Shared, plugin: &PluginId, reason: &str) {
    shared.registry_mut().note_log(plugin, failure(reason));
}

/// A running supervisor task and the channel into it.
pub struct Supervisor {
    pub(super) commands: tokio::sync::mpsc::UnboundedSender<Request>,
    /// The half both threads read, to say the process is gone when this is.
    pub(super) shared: Arc<Shared>,
    /// Aborted when this value goes, which also drops the session and with it
    /// the helper's process. The application's runtime, which runs it, outlives
    /// the bridge.
    pub(super) task: Task,
}

impl Bridge {
    /// Hands a plugin what the person did with something it drew.
    ///
    /// The interface changed when the person clicked — a toggle took its value,
    /// a notice left the screen — so nothing here answers into it. The input
    /// goes to the plugin that declared the thing: a menu row names its plugin,
    /// a slot belongs to one, and a notice is the one that drew it.
    pub fn input(&mut self, input: &crate::Input) {
        let (plugin, event) = match input {
            crate::Input::Menu {
                plugin,
                action,
                target,
            } => (
                plugin.clone(),
                yapfast_plugin_api::message::InputEvent::Menu {
                    action: action.clone(),
                    target: target.clone(),
                },
            ),
            crate::Input::Widget {
                plugin,
                slot,
                id,
                value,
                subject,
            } => (
                plugin.clone(),
                yapfast_plugin_api::message::InputEvent::Widget {
                    slot: slot.id().to_owned(),
                    id: id.clone(),
                    value: value.clone(),
                    subject: subject.clone(),
                },
            ),
            crate::Input::Dismissed { plugin, slot } => (
                plugin.clone(),
                yapfast_plugin_api::message::InputEvent::Dismissed {
                    slot: slot.id().to_owned(),
                },
            ),
        };
        let Ok(plugin) = PluginId::parse(&plugin) else {
            // A slot drawn by something that isn't a plugin id can't be handed
            // to a plugin; the interface's own id and a plugin's never collide,
            // so this is a manifest that changed under a click.
            return;
        };
        self.send(Request::Input {
            plugin,
            input: event,
        });
    }

    /// Hands one event to the plugins subscribed to it.
    ///
    /// The event is serialized once, and only if someone wants it; the filters
    /// are applied here, so a plugin that asked for one room doesn't wake the
    /// helper for the others.
    pub fn observe(&mut self, event: EventEnvelope) {
        if self.shared.wanted.load(Ordering::Relaxed) & Subscriptions::bit(event.kind) == 0 {
            return;
        }
        let targets = self.shared.registry().subscribers(&event);
        if targets.is_empty() {
            return;
        }
        let Some(shared) = SharedEvent::new(event) else {
            return;
        };
        for plugin in targets {
            self.send(Request::Event {
                plugin,
                event: shared.clone(),
            });
        }
    }

    /// Turns a plugin on or off: the registry, the lock file, the helper's
    /// instances and the page all follow, without a restart.
    pub fn set_enabled(&mut self, plugin: &PluginId, enabled: bool) -> anyhow::Result<()> {
        let turning: Vec<PluginId> = {
            let mut registry = self.shared.registry_mut();
            let store = self.shared.store();
            registry.set_enabled(plugin, enabled, &store)?
        };
        self.shared.publish();
        if enabled {
            for id in &turning {
                self.shared.declare(id);
            }
            // The person is asking for it to work: a helper that gave up
            // deserves another try.
            self.shared.clear_off();
            self.send(Request::Activate {
                plugins: vec![plugin.clone()],
            });
        } else {
            // The plugins that needed it are off too, and the registry answered
            // with them first: stopping a dependency before the plugins that
            // use it is the order the person watching would name.
            for id in &turning {
                self.shared.declare(id);
            }
            // A plugin that is off has no helper to run its requests, and
            // nothing will ever answer the questions they are waiting on.
            for id in &turning {
                self.shared
                    .release_plugin(id, &format!("{id} was turned off"));
                // And what it drew goes with it: a plugin that isn't running
                // shows nothing, so the interface is never left with a dead
                // tree.
                self.shared.announce(Notice::SlotsGone(id.clone()));
            }
            if self.enabled() {
                for id in &turning {
                    self.send(Request::Deactivate { plugin: id.clone() });
                }
            } else {
                // Nothing is left to run, so the process goes: that is what an idle
                // Yapfast is. Asking rather than dropping the task lets the helper
                // write what the plugin saved on its way out, which `close` waits
                // for.
                self.close();
            }
        }
        for id in &turning {
            self.announce(id);
        }
        Ok(())
    }

    /// Records that the person answered about `host`, and lets go the requests
    /// that were waiting on that answer.
    ///
    /// The answer itself is kept by the gate's grants, which the application
    /// writes before calling this; what is left is the requests: each is made
    /// again, and this time the gate finds an answer instead of a question.
    pub fn answer_consent(&self, plugin: &PluginId, host: &HostPattern, allow: bool) -> usize {
        let answered = if allow {
            Answered::Allowed
        } else {
            Answered::Denied(format!("you refused {host} for {plugin}"))
        };
        self.shared.answer_held(plugin, &host.to_string(), answered)
    }

    /// Refuses the requests waiting about a host whose grant was taken back.
    pub fn refuse_host(&self, plugin: &PluginId, host: &HostPattern) -> usize {
        let reason = format!("you took back the grant for {host}");
        self.shared
            .answer_held(plugin, &host.to_string(), Answered::Denied(reason))
    }

    /// Runs a command a person typed, through the plugin that declares it.
    ///
    /// The answer comes back as a `CommandDone` frame, which the application
    /// shows; a command that doesn't answer within two seconds is reported as
    /// failed rather than left hanging.
    pub fn run_command(&mut self, plugin: &PluginId, invocation: CommandInvocation) {
        self.send(Request::Command {
            plugin: plugin.clone(),
            invocation,
        });
    }

    /// Drops the plugins' instances now, keeping the process.
    ///
    /// The idle clock and the memory budget belong to the helper, which knows
    /// when an instance has been quiet; this is for a reason it can't know, such
    /// as the window going to the tray, where the person isn't looking and the
    /// memory should go back anyway.
    pub fn evict(&mut self) {
        let plugins: Vec<PluginId> = self
            .shared
            .statuses()
            .into_iter()
            .filter(|(_, status)| matches!(status, PluginStatus::Active))
            .map(|(plugin, _)| plugin)
            .collect();
        if !plugins.is_empty() {
            self.send(Request::Evict { plugins });
        }
    }

    /// What the person allows the helper: its whole memory budget and how long
    /// an instance may sit without a call before it is evicted.
    pub fn limits(&self) -> limits::Limits {
        self.shared.limits()
    }

    /// Applies what the person allows the helper, as [`Self::limits`] reports it
    /// from now on.
    ///
    /// The pair is snapped to the offered steps, written to `budget.json` so it
    /// is still there next time, kept here even when the write failed so the
    /// choice applies now, and sent to a helper that is already running. A
    /// helper that starts later is told the same pair right after its `Ready`,
    /// so nothing is lost by the gate on the send: with no helper running there
    /// is nothing to tell, and moving a picker must not start a process.
    ///
    /// An error is the write's, returned after everything else has been done:
    /// the choice applies now but was not remembered, which is the caller's to
    /// say.
    pub fn set_limits(&mut self, chosen: limits::Limits) -> anyhow::Result<()> {
        let chosen = chosen.snapped();
        let root = self.shared.store().root().to_owned();
        let saved = crate::budget::save(&root, chosen);
        self.shared.set_limits(chosen);
        if self.shared.helper_pid().is_some() {
            self.send(Request::Limits {
                memory_mib: chosen.memory_mib,
                idle_minutes: chosen.idle_minutes,
            });
        }
        saved
    }

    /// Every plugin's state, for the page that lists them.
    pub fn statuses(&self) -> Vec<(PluginId, PluginStatus)> {
        self.shared.statuses()
    }

    /// One plugin's name as its manifest writes it, which is what the interface
    /// shows where an id would be a machine's word for it.
    pub fn name(&self, plugin: &PluginId) -> Option<String> {
        self.shared
            .registry()
            .entry(plugin)
            .map(|entry| entry.manifest.name.clone())
    }

    /// What both threads of the plugin system read, which the pages' views are
    /// built from.
    pub(crate) fn shared(&self) -> &Shared {
        &self.shared
    }
}
