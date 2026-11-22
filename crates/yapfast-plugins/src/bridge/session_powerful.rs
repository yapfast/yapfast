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

    /// The powerful capability `request` needs and the person hasn't been
    /// asked about yet, if it needs one: such a call waits for their answer
    /// rather than being refused, and goes ahead the moment they say yes.
    pub(super) fn powerful_unasked(
        &self,
        shared: &Shared,
        plugin: &PluginId,
        request: &HostCall,
    ) -> Option<Capability> {
        let permissions = shared
            .registry()
            .entry(plugin)
            .map(|entry| entry.manifest.permissions.clone())?;
        let method = request.method.as_str();
        let capability = match method {
            call::method::FS_LIST
            | call::method::FS_STAT
            | call::method::FS_READ
            | call::method::FS_WRITE
            | call::method::FS_THUMBNAIL => {
                if permissions.allows(Capability::FsFolder) {
                    Capability::FsFolder
                } else {
                    Capability::FsFull
                }
            }
            call::method::MESSAGES_SEND | call::method::MESSAGES_ATTACH
                if !request
                    .payload
                    .get("files")
                    .and_then(serde_json::Value::as_array)
                    .is_none_or(Vec::is_empty) =>
            {
                if permissions.allows(Capability::FsFolder) {
                    Capability::FsFolder
                } else {
                    Capability::FsFull
                }
            }
            // Asked about the first time like the tier, though not behind
            // its switch.
            call::method::PROFILE_SET => Capability::ProfileWrite,
            call::method::NET_LISTEN => Capability::NetListen,
            call::method::NET_CONNECT => Capability::NetSocket,
            // A plugin with hosts of its own reaches those while `net.any`
            // waits, so only one with none waits on the question.
            call::method::HTTP_FETCH if !permissions.allows(Capability::Network) => {
                Capability::NetAny
            }
            call::method::MESSAGES_EDIT
            | call::method::MESSAGES_DELETE
            | call::method::MESSAGES_REACT
            | call::method::MESSAGES_SEND
            | call::method::MESSAGES_ATTACH
                if !shared.has_gesture(plugin)
                    && !permissions.allows(Capability::MessagesSendBackground) =>
            {
                Capability::MessagesSendAgent
            }
            _ => return None,
        };
        (permissions.allows(capability)
            && shared.powerful.check(plugin, capability) == Err(crate::powerful::Gate::Ask))
        .then_some(capability)
    }

    /// Keeps a call until the person answers the powerful question it needs,
    /// asking it if it isn't on screen. False when too many already wait, and
    /// the call was answered with why.
    pub(super) fn wait_for_powerful(
        &mut self,
        shared: &Shared,
        plugin: PluginId,
        capability: Capability,
        call: u64,
        request: HostCall,
    ) {
        let mut waiting = shared
            .powerful_waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // What waits holds its payload in memory, so a plugin that calls in a
        // loop while the person reads the question holds a handful, not all.
        if waiting
            .iter()
            .filter(|(owner, ..)| *owner == plugin)
            .count()
            >= POWERFUL_WAITING
        {
            drop(waiting);
            self.reply(
                &plugin,
                call,
                HostReturn::Denied {
                    reason: crate::powerful::Gate::Ask.reason(&plugin, capability),
                },
            );
            return;
        }
        waiting.push((plugin.clone(), capability, call, request));
        drop(waiting);
        let fresh = shared
            .powerful_asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert((plugin.clone(), capability));
        if fresh {
            shared.announce(Notice::Powerful { plugin, capability });
        }
    }

    /// The person answered: what waited runs now, through every gate again,
    /// or is refused with their no.
    pub(super) fn powerful_answered(
        &mut self,
        shared: &Arc<Shared>,
        plugin: &PluginId,
        capability: Capability,
        allow: bool,
    ) {
        let ready: Vec<(u64, HostCall)> = {
            let mut waiting = shared
                .powerful_waiting
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let mut ready = Vec::new();
            waiting.retain(|(owner, asked, call, request)| {
                if owner == plugin && *asked == capability {
                    ready.push((*call, request.clone()));
                    false
                } else {
                    true
                }
            });
            ready
        };
        for (call, request) in ready {
            if allow {
                self.on_call(shared, call, plugin.clone(), request);
            } else {
                self.reply(
                    plugin,
                    call,
                    HostReturn::Denied {
                        reason: format!(
                            "the person said no to {plugin} using `{capability}`; it asks again \
                             the next time it is used"
                        ),
                    },
                );
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

/// How many of one plugin's calls wait on a powerful question at once.
const POWERFUL_WAITING: usize = 16;

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
