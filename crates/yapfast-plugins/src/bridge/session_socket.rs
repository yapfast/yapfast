//! `net.socket`'s calls: `net.connect`, whose later answers carry what the
//! connection hears, and `net.send` and `net.close`, answered at once.

use super::*;

impl Session {
    /// `net.connect`: opens a connection for the plugin, behind the powerful
    /// tier's gates, and answers this call again for everything it hears.
    pub(super) fn connect(
        &mut self,
        shared: &Arc<Shared>,
        id: u64,
        plugin: PluginId,
        call: HostCall,
    ) {
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
            self.powerful_gate(shared, &plugin, &permissions, Capability::NetSocket)
        {
            self.reply(&plugin, id, answer);
            return;
        }
        let request: call::NetConnect = match call.parse() {
            Ok(request) => request,
            Err(error) => {
                self.reply(&plugin, id, wrong_payload(call::method::NET_CONNECT, error));
                return;
            }
        };
        if let Err(message) = crate::socket::checked_host(&request.host) {
            self.reply(&plugin, id, HostReturn::Error { message });
            return;
        }
        let socket = shared.next_socket.fetch_add(1, Ordering::Relaxed);
        let mut sockets = shared
            .sockets
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let open = sockets.entry(plugin.clone()).or_default();
        // Each connection is a task and a buffer in Yapfast; this keeps a
        // plugin that opens them in a loop from taking Yapfast down with it.
        if open.len() >= limits::MAX_SOCKETS {
            drop(sockets);
            self.reply(
                &plugin,
                id,
                HostReturn::Error {
                    message: format!(
                        "{plugin} has {} connections open, which is as many as Yapfast keeps \
                         for one plugin; close one first",
                        limits::MAX_SOCKETS
                    ),
                },
            );
            return;
        }
        let writer = self.writer.clone();
        let owner = plugin.clone();
        // Weak: the connection's task must not keep the plugin system alive
        // after everything else has let go of it.
        let weak = Arc::downgrade(shared);
        let deliver: crate::socket::Deliver = Arc::new(move |news| {
            if let call::SocketNews::Closed { socket, .. } = &news
                && let Some(shared) = weak.upgrade()
                && let Some(open) = shared
                    .sockets
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get_mut(&owner)
            {
                // Dropping the handle aborts the task running this closure,
                // which is harmless: this is its last step, and an abort only
                // lands at an await it will never reach.
                open.remove(socket);
            }
            if let Ok(payload) = serde_json::to_vec(&ToHelper::Return {
                plugin: owner.clone(),
                call: id,
                answer: answered(news),
            }) {
                let _ = writer.send(Frame::new(FrameKind::Reply, id, payload));
            }
        });
        let handle = crate::socket::open(socket, request, permissions.socket_local, deliver);
        open.insert(socket, handle);
    }

    /// `net.send` and `net.close`, for a connection the plugin holds.
    pub(super) fn socket_call(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        call: &HostCall,
    ) -> HostReturn {
        let method = call.method.as_str();
        let (socket, out) = if method == call::method::NET_CLOSE {
            match parsed::<call::NetClose>(call, method) {
                Ok(close) => (close.socket, crate::socket::Out::Close),
                Err(answer) => return answer,
            }
        } else {
            let sent: call::NetSend = match parsed(call, method) {
                Ok(sent) => sent,
                Err(answer) => return answer,
            };
            let out = match (sent.text, sent.data_base64) {
                (Some(text), None) => crate::socket::Out::Text(text),
                (None, Some(encoded)) => match brought(&encoded, frame::MAX_FRAME) {
                    Ok(bytes) => crate::socket::Out::Bytes(bytes),
                    Err(message) => return HostReturn::Error { message },
                },
                _ => {
                    return HostReturn::Error {
                        message: "`net.send` takes `text` or `data_base64`, and one of them"
                            .to_owned(),
                    };
                }
            };
            (sent.socket, out)
        };
        let sockets = shared
            .sockets
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match sockets.get(plugin).and_then(|open| open.get(&socket)) {
            Some(handle) if handle.out.send(out).is_ok() => {
                HostReturn::Ok(serde_json::Value::Bool(true))
            }
            _ => HostReturn::Error {
                message: format!("{plugin} has no open connection {socket}: it closed"),
            },
        }
    }
}
