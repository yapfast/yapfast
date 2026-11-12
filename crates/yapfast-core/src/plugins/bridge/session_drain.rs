use super::*;

impl Session {
    /// Sends `Shutdown` and waits for the process to go, killing it if it
    /// lingers.
    pub(super) async fn stop(&mut self) -> Ending {
        self.asked = true;
        self.send_message(&ToHelper::Shutdown, FrameKind::Request);
        let deadline = Instant::now() + SHUTDOWN_WAIT;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Ok(None) => {
                    tracing::warn!("the plugin helper didn't stop; ending it");
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
                Err(error) => {
                    tracing::warn!(%error, "could not wait for the plugin helper");
                    break;
                }
            }
        }
        Ending::Asked
    }

    /// Gives up on a request that ran out of time.
    pub(super) fn expire(&mut self, shared: &Arc<Shared>, id: u64) {
        let Some(pending) = self.pending.remove(&id) else {
            return;
        };
        match pending.purpose {
            Purpose::Hook(held) => {
                // Fail open, as the contract says: a plugin that runs late
                // leaves the message as the person wrote it.
                tracing::warn!(
                    chain = held.chain.len(),
                    hook = %held.hook,
                    "a hook chain ran out of time; the subject goes as it was"
                );
                held.let_go(shared);
            }
            Purpose::Agent {
                what,
                plugin,
                budget,
            } => {
                tracing::warn!(%plugin, what, "the helper didn't answer in time");
                let reason = format!(
                    "the helper didn't answer its {what} request within {} s",
                    budget.as_secs_f32()
                );
                note_failure(shared, &plugin, &reason);
                let status = PluginStatus::Faulted {
                    reason,
                    memory_pages: 0,
                };
                shared.registry_mut().set_status(&plugin, status.clone());
                shared.announce(Notice::Status { plugin, status });
            }
            Purpose::Invocation {
                caller,
                call,
                function,
            } => {
                // The caller did nothing wrong, so it is answered rather than
                // faulted: an instance that doesn't come back is its own fault,
                // reported where it happened.
                self.calls.retain(|inflight| inflight.frame != id);
                self.reply(
                    &caller,
                    call,
                    HostReturn::Error {
                        message: format!(
                            "the call to `{function}` didn't answer within {} s",
                            CALL_TIMEOUT.as_secs_f32()
                        ),
                    },
                );
            }
            Purpose::Swap { plugin } => {
                tracing::warn!(%plugin, "a swap step didn't answer in time; carrying on");
                self.swap_answered(shared, id);
            }
        }
    }

    /// Sends a request and remembers what its answer is for.
    pub(super) fn request(
        &mut self,
        message: &ToHelper,
        budget: Duration,
        purpose: Purpose,
    ) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        match serde_json::to_vec(message) {
            Ok(payload) => {
                self.pending.insert(
                    id,
                    Pending {
                        deadline: Instant::now() + budget,
                        purpose,
                    },
                );
                self.write(FrameKind::Request, id, payload);
            }
            Err(error) => tracing::warn!(%error, "a plugin message couldn't be serialized"),
        }
        id
    }

    /// Sends a message that isn't paired with an answer.
    pub(super) fn send_message(&mut self, message: &ToHelper, kind: FrameKind) {
        match serde_json::to_vec(message) {
            Ok(payload) => self.write(kind, 0, payload),
            Err(error) => tracing::warn!(%error, "a plugin message couldn't be serialized"),
        }
    }

    /// Hands one frame to the thread that writes it.
    ///
    /// Left unqueued rather than awaited: the helper may be slow to read, and a
    /// task that waited on it couldn't run the timers that are supposed to notice
    /// that.
    pub(super) fn write(&self, kind: FrameKind, id: u64, payload: Vec<u8>) {
        if self.writer.send(Frame::new(kind, id, payload)).is_err() {
            tracing::debug!("the plugin helper is no longer being written to");
        }
    }
}
