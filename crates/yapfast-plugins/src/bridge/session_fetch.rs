use super::*;

impl Session {
    /// Runs an `http.fetch` beside the conversation.
    ///
    /// The guest's request goes through the network gate, which holds the host
    /// list the manifest declares and the consent that widens it. The guest never
    /// sees a socket, and the check is made again on every hop of a redirect.
    pub(super) fn fetch(
        &mut self,
        shared: &Arc<Shared>,
        id: u64,
        plugin: PluginId,
        call: HostCall,
    ) {
        let (permissions, response_cap) = match shared.registry().entry(&plugin) {
            Some(entry) => (
                entry.manifest.permissions.clone(),
                entry.manifest.limits.http_response_bytes,
            ),
            None => {
                self.reply(
                    &plugin,
                    id,
                    HostReturn::Error {
                        message: format!("{plugin} isn't installed"),
                    },
                );
                return;
            }
        };
        // `network` reaches anywhere the gate allows and `network.local` only
        // loopback: a plugin that asks for the second alone is asking for less,
        // and the gate below is what decides which hosts are reachable.
        let may_fetch =
            permissions.allows(Capability::Network) || permissions.allows(Capability::NetworkLocal);
        if !may_fetch
            && let Err(denial) = crate::permissions::check(&permissions, Capability::Network)
        {
            self.reply(
                &plugin,
                id,
                HostReturn::Denied {
                    reason: denial.reason().to_owned(),
                },
            );
            return;
        }
        let request: HttpRequest = match call.parse() {
            Ok(request) => request,
            Err(error) => {
                self.reply(&plugin, id, wrong_payload(call::method::HTTP_FETCH, error));
                return;
            }
        };
        // A build with no transport can't make the request. The contract's
        // `unsupported` is the answer that lets a plugin feature-detect it rather
        // than wait for a result that will never come.
        let Some(gate) = shared.gate() else {
            self.reply(&plugin, id, HostReturn::Unsupported);
            return;
        };
        let writer = self.writer.clone();
        // The task outlives this call, so it owns what it reports through
        // rather than borrowing a reference the caller is about to drop.
        let shared = Arc::clone(shared);
        tokio::spawn(async move {
            // The gate answers about a host nobody has allowed rather than
            // holding the request: this task is what holds it, and answers the
            // guest under its own call id once the person has said yes or no.
            // The guest was given `pending` by the helper, so a late answer is
            // legal.
            let mut asked = false;
            let answer = loop {
                let decided = gate.fetch(&plugin, request.clone(), response_cap).await;
                match decided {
                    Ok(crate::network::Outcome::Done(answer)) => {
                        break match serde_json::to_value(&answer.response) {
                            Ok(value) => HostReturn::Ok(value),
                            Err(error) => HostReturn::Error {
                                message: format!("could not answer `http.fetch`: {error}"),
                            },
                        };
                    }
                    Ok(crate::network::Outcome::AwaitingConsent { host, asking }) => {
                        // One dialog per plugin at a time: a request for another
                        // host waits its turn, and the plugin is told to ask
                        // again rather than left in a queue behind a dialog it
                        // doesn't know about.
                        if asking == crate::network::Asking::Busy {
                            break HostReturn::Denied {
                                reason: format!(
                                    "Yapfast is asking you about another site for {plugin} \
                                     right now; ask again for {host} in a moment"
                                ),
                            };
                        }
                        // The answer was taken back between being given and
                        // being used, so asking again would loop.
                        if asked {
                            break HostReturn::Denied {
                                reason: format!("{host} isn't allowed for {plugin} any more"),
                            };
                        }
                        if asking == crate::network::Asking::Opened {
                            shared.announce(Notice::Consent {
                                plugin: plugin.clone(),
                                host: host.clone(),
                            });
                        }
                        let (answer, receiver) = tokio::sync::oneshot::channel();
                        if let Err(reason) = shared.hold(&plugin, &host, answer) {
                            break HostReturn::Denied { reason };
                        }
                        asked = true;
                        match receiver.await {
                            Ok(Answered::Allowed) => continue,
                            Ok(Answered::Denied(reason)) => {
                                break HostReturn::Denied { reason };
                            }
                            // Nothing holds the other end any more, which is
                            // what a helper that has gone leaves behind.
                            Err(_) => {
                                break HostReturn::Denied {
                                    reason: format!(
                                        "nothing is left to answer about {host}, so this \
                                         request was refused"
                                    ),
                                };
                            }
                        }
                    }
                    Err(error) => {
                        break HostReturn::Denied {
                            reason: format!("{error:#}"),
                        };
                    }
                }
            };
            // The answer travels under the id the guest asked with, so the
            // helper can hand it to the call that is waiting for it.
            match serde_json::to_vec(&ToHelper::Return {
                plugin,
                call: id,
                answer,
            }) {
                Ok(payload) => {
                    let _ = writer.send(Frame::new(FrameKind::Reply, id, payload));
                }
                Err(error) => tracing::warn!(%error, "could not answer a fetch"),
            }
        });
    }

    /// Answers a guest's call under the id it asked with.
    pub(super) fn reply(&self, plugin: &PluginId, id: u64, answer: HostReturn) {
        let message = ToHelper::Return {
            plugin: plugin.clone(),
            call: id,
            answer,
        };
        match serde_json::to_vec(&message) {
            Ok(payload) => self.write(FrameKind::Reply, id, payload),
            Err(error) => tracing::warn!(%error, "could not answer a plugin's call"),
        }
    }
}
