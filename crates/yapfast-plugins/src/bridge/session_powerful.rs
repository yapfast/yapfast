//! The powerful tier's calls: a plugin's folder, its thumbnails and its local
//! endpoint, each answered here only after three gates — the capability in the
//! manifest, the switch in Advanced, and the person's answer the first time.

use super::*;

impl Session {
    /// Whether `plugin` may use the powerful `capability` now, or the answer
    /// that says why not. The first use opens the person's dialog, once: a
    /// plugin that calls again while it is up is told the same, and no second
    /// dialog stacks on the first.
    pub(super) fn powerful_gate(
        &self,
        shared: &Shared,
        plugin: &PluginId,
        permissions: &yapfast_plugin_api::capability::Permissions,
        capability: Capability,
    ) -> Result<(), HostReturn> {
        crate::permissions::check(permissions, capability).map_err(|denial| {
            HostReturn::Denied {
                reason: denial.reason().to_owned(),
            }
        })?;
        match shared.powerful.check(plugin, capability) {
            Ok(()) => Ok(()),
            Err(gate) => {
                if gate == crate::powerful::Gate::Ask {
                    let fresh = shared
                        .powerful_asked
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert((plugin.clone(), capability));
                    if fresh {
                        shared.announce(Notice::Powerful {
                            plugin: plugin.clone(),
                            capability,
                        });
                    }
                }
                Err(HostReturn::Denied {
                    reason: gate.reason(plugin, capability),
                })
            }
        }
    }

    /// The folder a plugin reaches: the one the person picked for `fs.folder`,
    /// or for `fs.full` the home folder or the disk, as its manifest says.
    pub(super) fn files_root(
        &self,
        shared: &Shared,
        plugin: &PluginId,
    ) -> Result<crate::files::Root, HostReturn> {
        let permissions = shared
            .registry()
            .entry(plugin)
            .map(|entry| entry.manifest.permissions.clone())
            .ok_or_else(|| HostReturn::Error {
                message: format!("{plugin} isn't installed"),
            })?;
        let write = permissions.writes_files();
        let path = if permissions.allows(Capability::FsFolder) {
            self.powerful_gate(shared, plugin, &permissions, Capability::FsFolder)?;
            shared
                .powerful
                .folder(plugin)
                .ok_or_else(|| HostReturn::Denied {
                    reason: format!("{plugin} has no folder yet: the person picks one"),
                })?
        } else if permissions.allows(Capability::FsFull) {
            self.powerful_gate(shared, plugin, &permissions, Capability::FsFull)?;
            match permissions
                .files
                .map(|files| files.scope)
                .unwrap_or_default()
            {
                yapfast_plugin_api::capability::FileScope::Home => crate::files::home()
                    .ok_or_else(|| HostReturn::Error {
                        message: "this computer has no home folder Yapfast can find".to_owned(),
                    })?,
                yapfast_plugin_api::capability::FileScope::Disk => crate::files::disk(),
            }
        } else {
            return Err(HostReturn::Denied {
                reason: format!(
                    "{plugin} reads files with `fs.folder` or `fs.full`, and declares neither"
                ),
            });
        };
        crate::files::Root::new(&path, write).map_err(|message| HostReturn::Error { message })
    }

    /// `fs.list`, `fs.stat`, `fs.read` and `fs.write`, answered at once.
    pub(super) fn files_call(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        call: &HostCall,
    ) -> HostReturn {
        let method = call.method.as_str();
        let root = match self.files_root(shared, plugin) {
            Ok(root) => root,
            Err(answer) => return answer,
        };
        let answer = match method {
            call::method::FS_LIST => match parsed::<call::FsPath>(call, method) {
                Ok(request) => root.list(&request.path).map(answered),
                Err(answer) => return answer,
            },
            call::method::FS_STAT => match parsed::<call::FsPath>(call, method) {
                Ok(request) => root.stat(&request.path).map(answered),
                Err(answer) => return answer,
            },
            call::method::FS_READ => match parsed::<call::FsRead>(call, method) {
                Ok(request) => root.read(&request).map(answered),
                Err(answer) => return answer,
            },
            call::method::FS_WRITE => match parsed::<call::FsWrite>(call, method) {
                Ok(request) => {
                    let bytes = match (request.text, request.bytes_base64) {
                        (Some(text), None) => Ok(text.into_bytes()),
                        (None, Some(encoded)) => brought(&encoded, crate::files::MAX_WRITE_BYTES),
                        _ => {
                            Err("`fs.write` takes `text` or `bytes_base64`, and one of them"
                                .to_owned())
                        }
                    };
                    bytes
                        .and_then(|bytes| root.write(&request.path, &bytes, request.append))
                        .map(|written| HostReturn::Ok(serde_json::json!({ "written": written })))
                }
                Err(answer) => return answer,
            },
            _ => return HostReturn::Unsupported,
        };
        answer.unwrap_or_else(|message| HostReturn::Error { message })
    }

    /// `fs.thumbnail`, decoded on a blocking thread and answered when it is
    /// done, so neither this task nor the interface waits on a picture.
    pub(super) fn thumbnail(
        &mut self,
        shared: &Arc<Shared>,
        id: u64,
        plugin: PluginId,
        call: HostCall,
    ) {
        let root = match self.files_root(shared, &plugin) {
            Ok(root) => root,
            Err(answer) => {
                self.reply(&plugin, id, answer);
                return;
            }
        };
        let request: call::FsThumbnail = match call.parse() {
            Ok(request) => request,
            Err(error) => {
                self.reply(
                    &plugin,
                    id,
                    wrong_payload(call::method::FS_THUMBNAIL, error),
                );
                return;
            }
        };
        let writer = self.writer.clone();
        tokio::task::spawn_blocking(move || {
            let answer = match crate::files::thumbnail(&root, &request) {
                Ok(made) => answered(made),
                Err(message) => HostReturn::Error { message },
            };
            if let Ok(payload) = serde_json::to_vec(&ToHelper::Return {
                plugin,
                call: id,
                answer,
            }) {
                let _ = writer.send(Frame::new(FrameKind::Reply, id, payload));
            }
        });
    }

    /// `net.listen`: serves on loopback behind the plugin's token, and hands
    /// each request to the plugin as another answer to this call.
    pub(super) fn listen(&mut self, shared: &Arc<Shared>, id: u64, plugin: PluginId) {
        let Some(permissions) = shared
            .registry()
            .entry(&plugin)
            .map(|entry| entry.manifest.permissions.clone())
        else {
            self.reply(
                &plugin,
                id,
                HostReturn::Error {
                    message: format!("{plugin} isn't installed"),
                },
            );
            return;
        };
        if let Err(answer) =
            self.powerful_gate(shared, &plugin, &permissions, Capability::NetListen)
        {
            self.reply(&plugin, id, answer);
            return;
        }
        let token = match shared.powerful.token(&plugin) {
            Ok(token) => token,
            Err(error) => {
                self.reply(
                    &plugin,
                    id,
                    HostReturn::Error {
                        message: format!("the endpoint's token couldn't be kept: {error:#}"),
                    },
                );
                return;
            }
        };
        // A second `net.listen` replaces the first, which is what an instance
        // started again does; the old endpoint's port is free before the new
        // one binds it.
        shared
            .endpoints
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&plugin);
        let writer = self.writer.clone();
        let answers = Arc::clone(shared);
        let owner = plugin.clone();
        let deliver: crate::listen::Deliver = Arc::new(move |incoming, reply| {
            let request = incoming.request;
            answers
                .answers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert((owner.clone(), request), reply);
            let answer = answered(incoming);
            if let Ok(payload) = serde_json::to_vec(&ToHelper::Return {
                plugin: owner.clone(),
                call: id,
                answer,
            }) {
                let _ = writer.send(Frame::new(FrameKind::Reply, id, payload));
            }
        });
        match crate::listen::start(permissions.listen_port, token, deliver) {
            Ok(endpoint) => {
                let port = endpoint.port;
                shared
                    .endpoints
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(plugin.clone(), endpoint);
                self.reply(
                    &plugin,
                    id,
                    HostReturn::Ok(serde_json::json!({
                        "listening": true,
                        "port": port,
                        "url": format!("http://127.0.0.1:{port}/"),
                    })),
                );
            }
            Err(error) => self.reply(
                &plugin,
                id,
                HostReturn::Error {
                    message: format!("Yapfast couldn't listen on this computer: {error}"),
                },
            ),
        }
    }

    /// `net.respond`: the plugin's answer to one request its endpoint holds.
    pub(super) fn respond(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        call: &HostCall,
    ) -> HostReturn {
        let request: call::Respond = match parsed(call, call::method::NET_RESPOND) {
            Ok(request) => request,
            Err(answer) => return answer,
        };
        let body = match (request.text, request.body_base64) {
            (Some(text), None) => text.into_bytes(),
            (None, Some(encoded)) => match brought(&encoded, crate::listen::MAX_BODY_BYTES) {
                Ok(bytes) => bytes,
                Err(message) => return HostReturn::Error { message },
            },
            (None, None) => Vec::new(),
            (Some(_), Some(_)) => {
                return HostReturn::Error {
                    message: "`net.respond` takes `text` or `body_base64`, not both".to_owned(),
                };
            }
        };
        let waiting = shared
            .answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&(plugin.clone(), request.request));
        match waiting {
            Some(sender) => {
                let _ = sender.send(crate::listen::Reply {
                    status: request.status,
                    headers: request.headers,
                    body,
                });
                HostReturn::Ok(serde_json::Value::Bool(true))
            }
            None => HostReturn::Error {
                message: format!(
                    "no request {} is waiting for {plugin}: it was answered, or timed out",
                    request.request
                ),
            },
        }
    }
}

impl Shared {
    /// Whether `plugin` may send one more as an agent: `per_minute` in any
    /// sixty seconds.
    pub(super) fn may_send_as_agent(&self, plugin: &PluginId, per_minute: u8) -> bool {
        let mut sends = self
            .agent_sends
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let held = sends.entry(plugin.clone()).or_default();
        let since = Instant::now() - Duration::from_secs(60);
        held.retain(|at| *at > since);
        if held.len() >= usize::from(per_minute) {
            return false;
        }
        held.push(Instant::now());
        true
    }
}
