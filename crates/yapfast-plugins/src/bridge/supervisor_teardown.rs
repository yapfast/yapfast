use super::*;

/// Waits with the plugin system off, answering the sends that were already on
/// their way.
///
/// The bridge stops sending the moment it is told plugins are off, so nothing new
/// arrives here. What can arrive is a send that was between the application and
/// this task when the helper gave up: without an answer the composer would wait
/// for a chain that no longer exists, so it goes out as the person wrote it. The
/// task then holds nothing but a channel, and ends when the bridge does.
pub(super) async fn stand_down(
    shared: &Shared,
    commands: &mut tokio::sync::mpsc::UnboundedReceiver<Request>,
) {
    while let Some(command) = commands.recv().await {
        if let Request::Hook { held, .. } = command {
            let chain = held.chain.clone();
            held.fail(shared, &chain, "plugins are off");
        }
    }
}

impl Bridge {
    /// The application's runtime, which the work that takes seconds runs on.
    pub(super) fn tasks(&self) -> &dyn Spawn {
        &*self.host.tasks
    }

    /// Every installed plugin's manifest, state and recent log, for the page
    /// that lists them.
    pub fn listed(&self) -> Vec<crate::registry::Listed> {
        self.shared.registry().listed()
    }

    /// What the helper process is doing, for the page that says so.
    pub fn helper(&self) -> HelperState {
        if self.shared.off().is_some() {
            return if self.shared.helper_installed() {
                HelperState::Crashed
            } else {
                HelperState::Missing
            };
        }
        match self.shared.helper_pid() {
            Some(pid) => HelperState::Running {
                memory: resident_memory(pid),
            },
            None => HelperState::Stopped,
        }
    }

    /// Why the plugin system is off, when it is: the page shows this, and it is
    /// `None` while the helper is running or simply not needed yet.
    pub fn off(&self) -> Option<String> {
        self.shared.off()
    }

    /// Stops the helper, if it is running. Called when the application exits.
    pub fn shutdown(&mut self) {
        self.close();
        self.shared.store().flush();
    }

    /// Sends one command to the supervisor, starting it if nothing is running.
    ///
    /// `false` means there is nobody to run it: no plugin is enabled, or the
    /// helper was given up on, and the caller has to answer for itself.
    pub(super) fn send(&mut self, command: Request) -> bool {
        let Some(supervisor) = self.supervisor() else {
            return false;
        };
        if supervisor.commands.send(command).is_err() {
            // The task has ended, which it does only when it gave up or was
            // asked to stop; either way it has already said why.
            tracing::debug!("the plugin supervisor is no longer listening");
            return false;
        }
        true
    }

    /// The supervisor, started on first use: nothing about plugins runs until a
    /// plugin has something to do.
    pub(super) fn supervisor(&mut self) -> Option<&Supervisor> {
        if !self.enabled() || self.shared.off().is_some() {
            self.supervisor = None;
            return None;
        }
        if self.supervisor.is_none() {
            let (commands, receiver) = tokio::sync::mpsc::unbounded_channel();
            let task = match self.host.tasks.spawn(Box::pin(supervise(
                self.closing.take(),
                self.shared.clone(),
                receiver,
                self.data_dir.clone(),
                self.host.stderr,
            ))) {
                Ok(task) => task,
                Err(error) => {
                    self.shared
                        .give_up(format!("plugins couldn't start their runtime: {error}"));
                    return None;
                }
            };
            self.supervisor = Some(Supervisor {
                commands,
                shared: Arc::clone(&self.shared),
                task,
            });
        }
        self.supervisor.as_ref()
    }

    /// Stops the helper without waiting for it: what turning the last plugin
    /// off and an uninstall do, from the thread that draws.
    ///
    /// The wait for its plugins to save, and the store's flush after it, run on
    /// the application's runtime. The next supervisor waits for them before it
    /// starts a helper, so a fresh process never runs beside the last one.
    pub(super) fn close_soon(&mut self) {
        let Some(supervisor) = self.supervisor.take() else {
            return;
        };
        if self.shared.off().is_some() {
            // No process to stop: dropping the task is enough.
            return;
        }
        let _ = supervisor.commands.send(Request::Shutdown);
        let previous = self.closing.take();
        let stopping = async move {
            if let Some(previous) = previous {
                let _ = previous.await;
            }
            let mut supervisor = supervisor;
            if tokio::time::timeout(CLOSE_WAIT, &mut supervisor.task)
                .await
                .is_err()
            {
                tracing::warn!("the plugin helper is still stopping; leaving it to exit");
            }
            // Whatever a plugin saved is written before the process that asked
            // for it is gone, so the last change is not lost to a stop.
            supervisor.shared.store().flush();
        };
        // A runtime that won't take the work drops it, and with it the
        // supervisor, which stops the helper without the wait.
        if let Ok(task) = self.host.tasks.spawn(Box::pin(stopping)) {
            self.closing = Some(task);
        }
    }

    /// Stops the helper and waits for it, so the next start is a fresh process
    /// rather than a second one beside the first. Only the application's exit
    /// waits like this; everything a person does goes through
    /// [`Self::close_soon`].
    pub(super) fn close(&mut self) {
        if let Some(mut closing) = self.closing.take()
            && !self.host.tasks.wait(&mut closing, CLOSE_WAIT)
        {
            tracing::warn!("a plugin helper that was stopping hasn't finished");
        }
        // Kept whole rather than taken apart: dropping it is what forgets the
        // helper's process id, and every way out of this function goes through
        // that drop.
        let Some(mut supervisor) = self.supervisor.take() else {
            return;
        };
        if self.shared.off().is_some() {
            // Plugins are off, so there is no process to stop: dropping the task
            // is enough, and it holds only a channel.
            return;
        }
        let _ = supervisor.commands.send(Request::Shutdown);
        if !self.host.tasks.wait(&mut supervisor.task, CLOSE_WAIT) {
            tracing::warn!("the plugin helper is still stopping; leaving it to exit");
        }
        // Whatever a plugin saved is written before the process that asked for
        // it is gone, so the last change is not lost to a stop.
        self.shared.store().flush();
    }

    /// Tells the page about one plugin's status, if there is anything to tell.
    pub(super) fn announce(&self, plugin: &PluginId) {
        let status = self
            .shared
            .registry()
            .entry(plugin)
            .map(|entry| entry.status.clone());
        if let Some(status) = status {
            self.shared.announce(Notice::Status {
                plugin: plugin.clone(),
                status,
            });
        }
    }
}
