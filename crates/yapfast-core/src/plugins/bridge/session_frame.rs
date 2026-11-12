use super::*;

impl Session {
    /// Handles one frame from the helper.
    pub(super) fn on_frame(&mut self, shared: &Arc<Shared>, frame: Frame) -> Option<Ending> {
        let message: FromHelper = match serde_json::from_slice(&frame.payload) {
            Ok(message) => message,
            Err(error) => {
                // A frame this build can't read is a version skew the hello
                // should have caught; it is dropped rather than fatal, so the
                // rest of the plugins keep working.
                tracing::warn!(%error, kind = frame.kind as u8, "the helper sent a message this build doesn't know");
                return None;
            }
        };
        match message {
            FromHelper::Ready { api, version } => {
                tracing::debug!(api, %version, "the plugin helper greeted twice");
                None
            }
            FromHelper::Call {
                plugin,
                call,
                request,
            } => {
                // The number the guest was told to expect its answer under, not
                // the frame's: only the helper's runtime knows which call it is
                // answering, so it travels in the message.
                self.on_call(shared, call, plugin, request);
                None
            }
            FromHelper::Activated { plugin, .. } => {
                // Its answer: forgetting the request here is what stops the
                // deadline from faulting a plugin that loaded perfectly well.
                // The other answers clear their own request where they are
                // handled, because those handlers need the purpose they carried.
                self.pending.remove(&frame.id);
                self.active.insert(plugin.clone());
                let status = PluginStatus::Active;
                shared.registry_mut().set_status(&plugin, status.clone());
                shared.announce(Notice::Status { plugin, status });
                // A swap waiting on this instance goes on to its next step.
                self.swap_answered(shared, frame.id);
                None
            }
            FromHelper::Deactivated { plugin, state } => {
                self.pending.remove(&frame.id);
                self.active.remove(&plugin);
                // What it handed on is kept until the swap that stopped it loads
                // its replacement, which is how a plugin's state crosses an
                // update.
                if state.is_some() {
                    self.handoff.insert(plugin.clone(), state);
                }
                let status = PluginStatus::Enabled;
                shared.registry_mut().set_status(&plugin, status.clone());
                shared.announce(Notice::Status { plugin, status });
                self.swap_answered(shared, frame.id);
                None
            }
            FromHelper::Invoked {
                answer_base64,
                fault,
            } => {
                self.calls.retain(|call| call.frame != frame.id);
                let Some(pending) = self.pending.remove(&frame.id) else {
                    tracing::debug!("a call between plugins answered after its budget");
                    return None;
                };
                let Purpose::Invocation {
                    caller,
                    call,
                    function,
                } = pending.purpose
                else {
                    return None;
                };
                let answer = match fault {
                    Some(fault) => HostReturn::Error {
                        message: format!("`{function}` failed: {}", fault.reason),
                    },
                    None => match answer_base64 {
                        Some(answer) => HostReturn::Ok(serde_json::json!({
                            "answer_base64": answer,
                        })),
                        None => HostReturn::Ok(serde_json::json!(null)),
                    },
                };
                self.reply(&caller, call, answer);
                None
            }
            FromHelper::HookDone {
                outcome,
                failures,
                by,
            } => {
                self.on_hook_done(shared, frame.id, outcome, failures, by);
                None
            }
            FromHelper::CommandDone { plugin, outcome } => {
                self.on_command_done(frame.id, shared, &plugin, outcome);
                None
            }
            FromHelper::Decorated {
                plugin,
                message,
                tree,
                hide,
                hide_media,
            } => {
                self.pending.remove(&frame.id);
                self.on_decorated(
                    shared,
                    &plugin,
                    &message,
                    tree,
                    crate::plugins::slots::Hidden {
                        row: hide,
                        media: hide_media,
                    },
                );
                None
            }
            FromHelper::Identity {
                plugin,
                person,
                server,
                identity,
            } => {
                self.pending.remove(&frame.id);
                self.on_identity(shared, &plugin, &person, server, identity);
                None
            }
            FromHelper::CodeBlock {
                plugin,
                block,
                spans,
            } => {
                self.pending.remove(&frame.id);
                self.on_code_block(shared, &plugin, &block, spans);
                None
            }
            FromHelper::MemberDecorated {
                plugin,
                member,
                badge,
            } => {
                self.pending.remove(&frame.id);
                self.on_member_decorated(shared, &plugin, &member, badge);
                None
            }
            FromHelper::EventDone { .. } => None,
            FromHelper::Status { plugin, status } => {
                if !matches!(status, PluginStatus::Active) {
                    self.active.remove(&plugin);
                    // An instance that isn't running can't be told of a change:
                    // the next one asks again when it starts.
                    if !self.active.contains(&plugin) {
                        self.watchers.remove(&plugin);
                    }
                }
                shared.registry_mut().set_status(&plugin, status.clone());
                shared.announce(Notice::Status { plugin, status });
                None
            }
            FromHelper::Fault { plugin, fault } => {
                self.watchers.remove(&plugin);
                self.on_fault(shared, plugin, fault);
                // A fault is this step's answer too: the plugin it was about is
                // gone, and the swap carries on rather than waiting for an
                // instance that doesn't exist.
                self.swap_answered(shared, frame.id);
                None
            }
            FromHelper::Log { plugin, record } => {
                shared.registry_mut().note_log(&plugin, record);
                None
            }
            FromHelper::Bye { reason } => {
                tracing::info!(%reason, "the plugin helper is exiting");
                self.bye = Some(reason.clone());
                Some(self.ending(Some(reason)))
            }
        }
    }

    /// Records a fault and stops routing work to a plugin that failed.
    pub(super) fn on_fault(&mut self, shared: &Arc<Shared>, plugin: PluginId, fault: Fault) {
        tracing::warn!(%plugin, reason = %fault.reason, kind = ?fault.kind, "a plugin faulted");
        self.active.remove(&plugin);
        if matches!(
            fault.kind,
            yapfast_plugin_api::message::FaultKind::BadModule
        ) {
            // A module that didn't instantiate has to be handed over again
            // before the next trigger can try it.
            self.loaded.remove(&plugin);
        }
        let reason = fault.reason;
        note_failure(shared, &plugin, &reason);
        let status = PluginStatus::Faulted {
            reason,
            memory_pages: 0,
        };
        shared.registry_mut().set_status(&plugin, status.clone());
        shared.announce(Notice::Status { plugin, status });
    }
}
