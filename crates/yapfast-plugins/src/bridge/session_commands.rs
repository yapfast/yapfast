use super::*;

impl Session {
    /// Handles one command from the application.
    pub(super) async fn on_command(
        &mut self,
        shared: &Arc<Shared>,
        command: Request,
    ) -> Option<Ending> {
        // Work for a plugin taking part in a swap waits for it to finish: a
        // dependent's handler that asked its dependency something would otherwise
        // run while that dependency is between two versions. Shutdown is never
        // held, and neither is the swap itself.
        if self.swapping() && self.holds_for_swap(&command) {
            self.held.push(command);
            return None;
        }
        match command {
            Request::Activate { plugins } => {
                for plugin in plugins {
                    self.activate(shared, &plugin);
                }
                None
            }
            Request::Limits {
                memory_mib,
                idle_minutes,
            } => {
                // Fire and forget, id 0: the helper applies what it is given
                // and says nothing. Sent twice — once after `Ready` and again
                // on a change — it is idempotent, so a change that raced the
                // handshake is applied twice rather than lost.
                self.send_message(
                    &ToHelper::Limits {
                        memory_mib,
                        idle_minutes,
                    },
                    FrameKind::Request,
                );
                None
            }
            Request::Deactivate { plugin } => {
                self.deactivate(&plugin);
                None
            }
            Request::Event { plugin, event } => {
                self.deliver(shared, plugin, event);
                None
            }
            Request::Hook { held, request } => {
                self.hold(shared, held, request);
                None
            }
            Request::SettingsNews { plugin, news } => {
                self.news(&plugin, news);
                None
            }
            Request::Command { plugin, invocation } => {
                self.command(shared, &plugin, invocation);
                None
            }
            Request::Decorate { plugin, request } => {
                self.decorate(shared, &plugin, request);
                None
            }
            Request::DecorateMember { plugin, request } => {
                self.decorate_member(shared, &plugin, request);
                None
            }
            Request::Input { plugin, input } => {
                self.input(shared, &plugin, input);
                None
            }
            Request::Identity { plugin, request } => {
                self.identity(shared, &plugin, request);
                None
            }
            Request::CodeBlock { plugin, request } => {
                self.code_block(shared, &plugin, request);
                None
            }
            Request::CallAnswer {
                plugin,
                call,
                answer,
            } => {
                self.reply(&plugin, call, answer);
                None
            }
            Request::Swap {
                plugin,
                stop,
                start,
            } => {
                self.begin_swap(shared, plugin, stop, start);
                None
            }
            Request::Watch { plugin, folder } => {
                self.watch(plugin, folder);
                None
            }
            Request::Evict { plugins } => {
                for plugin in plugins {
                    self.evict(&plugin);
                }
                None
            }
            Request::Shutdown => Some(self.stop().await),
        }
    }

    /// Reports a command's answer: a line for the person, or the reason it
    /// failed, on the application's own event path.
    pub(super) fn on_command_done(
        &mut self,
        id: u64,
        shared: &Arc<Shared>,
        plugin: &PluginId,
        outcome: CommandOutcome,
    ) {
        // Its answer, so its deadline has nothing left to fault.
        self.pending.remove(&id);
        match outcome {
            CommandOutcome::None => {}
            CommandOutcome::Notice { text } => {
                shared.announce(Notice::Command {
                    plugin: plugin.clone(),
                    text,
                });
            }
            CommandOutcome::Value(value) => {
                tracing::info!(%plugin, %value, "a plugin command answered with a value");
            }
            // A command that failed says why where the person typed it: a
            // helper's log line is not where they look.
            CommandOutcome::Failed { message } => {
                shared.announce(Notice::Command {
                    plugin: plugin.clone(),
                    text: message,
                });
            }
        }
    }
}
