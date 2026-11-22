//! Byte-mode Windows pipes use the same framing as Linux's Unix sockets.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

pub(super) type Stream = NamedPipeServer;

pub(super) fn runtime_directory() -> PathBuf {
    PathBuf::from(r"\\.\pipe")
}

pub(super) struct Socket {
    listener: NamedPipeServer,
    pub(super) path: PathBuf,
}

impl Socket {
    pub(super) async fn bind(directory: &Path) -> Result<Self> {
        Self::bind_prefix(directory, "discord-ipc")
    }

    fn bind_prefix(directory: &Path, prefix: &str) -> Result<Self> {
        for slot in 0..10 {
            let path = directory.join(format!("{prefix}-{slot}"));
            // Claim an unused name; never add an instance to Discord's pipe.
            if let Ok(listener) = Self::instance(&path, true) {
                return Ok(Self { listener, path });
            }
        }
        bail!("no free Discord IPC pipe in {}", directory.display())
    }

    fn instance(path: &Path, first: bool) -> std::io::Result<NamedPipeServer> {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .in_buffer_size(4096)
            .out_buffer_size(4096)
            .create(path)
    }

    pub(super) async fn accept(&mut self) -> Result<Stream> {
        self.listener.connect().await?;
        // Keep a listening instance alive before handing off the connected
        // one. Clients can reconnect without losing the claimed pipe name.
        let next = Self::instance(&self.path, false)?;
        Ok(std::mem::replace(&mut self.listener, next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::windows::named_pipe::ClientOptions,
    };

    #[tokio::test]
    async fn pipes_keep_their_slot_across_clients_and_release_on_drop() {
        let prefix = format!("yapfast-test-{}", std::process::id());
        let directory = runtime_directory();
        let mut first = Socket::bind_prefix(&directory, &prefix).unwrap();
        let second = Socket::bind_prefix(&directory, &prefix).unwrap();
        assert!(first.path.ends_with(format!("{prefix}-0")));
        assert!(second.path.ends_with(format!("{prefix}-1")));

        let mut client = ClientOptions::new().open(&first.path).unwrap();
        let mut connected = first.accept().await.unwrap();
        // A second client can connect while the first is still active.
        let mut other = ClientOptions::new().open(&first.path).unwrap();
        let mut another = first.accept().await.unwrap();
        super::super::write(&mut client, 3, b"ping").await.unwrap();
        assert_eq!(
            super::super::read(&mut connected).await.unwrap(),
            (3, b"ping".to_vec())
        );
        other.write_all(b"x").await.unwrap();
        assert_eq!(another.read_u8().await.unwrap(), b'x');
        drop(client);
        drop(other);
        drop(connected);
        drop(another);

        // The pending instance keeps the slot reserved even with no clients.
        let third = Socket::bind_prefix(&directory, &prefix).unwrap();
        assert!(third.path.ends_with(format!("{prefix}-2")));
        let path = first.path.clone();
        drop(first);
        // Tokio closes pending overlapped I/O through the reactor; the name
        // becomes free once those completions have been processed.
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Ok(reclaimed) = Socket::instance(&path, true) {
                    drop(reclaimed);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn rpc_disconnect_restores_the_other_application_without_a_timer() {
        switches_while_loading(false).await;
    }

    #[tokio::test]
    async fn rpc_close_restores_the_other_application_without_waiting_for_http() {
        switches_while_loading(true).await;
    }

    async fn switches_while_loading(close: bool) {
        use crate::{
            api::Api,
            service::{Snapshot, Status},
        };
        use serde_json::{Value, json};
        use std::{sync::Arc, time::Duration};
        use tokio::{net::TcpListener, sync::watch, time::timeout};

        let prefix = format!("yapfast-switch-test-{}-{close}", std::process::id());
        let socket = Socket::bind_prefix(&runtime_directory(), &prefix).unwrap();
        let path = socket.path.clone();
        let http = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut api = Api::anonymous().unwrap();
        api.base = format!("http://{}", http.local_addr().unwrap());
        let (loading, mut image_loading) = watch::channel(false);
        let http_worker = tokio::spawn(async move {
            loop {
                let (mut stream, _) = http.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    request.push(stream.read_u8().await.unwrap());
                    assert!(request.len() < 8192);
                }
                let request = String::from_utf8(request).unwrap();
                if request.starts_with("POST /applications/22/external-assets ") {
                    // Discord's image lookup stalls while the app exits.
                    loading.send_replace(true);
                    std::future::pending::<()>().await;
                }
                assert!(request.starts_with("GET /oauth2/applications/"));
                let body = br#"{"name":"Fixture"}"#;
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                stream.write_all(body).await.unwrap();
            }
        });
        let (_snapshots, snapshots) = watch::channel(Arc::new(Snapshot {
            status: Status::Online,
            user_id: Some("42".into()),
            ..Default::default()
        }));
        let (shown, mut activity) = watch::channel(None);
        let server = tokio::spawn(async move {
            super::super::serve(socket, Arc::new(api), snapshots, &shown).await;
        });

        async fn command(
            stream: &mut tokio::net::windows::named_pipe::NamedPipeClient,
            opcode: u32,
            value: Value,
        ) -> Value {
            super::super::write(stream, opcode, &serde_json::to_vec(&value).unwrap())
                .await
                .unwrap();
            let (opcode, payload) = super::super::read(stream).await.unwrap();
            assert_eq!(opcode, 1);
            serde_json::from_slice(&payload).unwrap()
        }
        async fn shows(activity: &mut watch::Receiver<Option<Arc<Value>>>, details: &str) {
            timeout(
                Duration::from_secs(1),
                activity.wait_for(|value| {
                    value
                        .as_ref()
                        .is_some_and(|value| value["details"] == details)
                }),
            )
            .await
            .unwrap()
            .unwrap();
        }
        let mut first = ClientOptions::new().open(&path).unwrap();
        assert_eq!(
            command(&mut first, 0, json!({"v":1,"client_id":"11"})).await["evt"],
            "READY"
        );
        command(&mut first, 1, json!({"cmd":"SET_ACTIVITY","nonce":"first","args":{"pid":1,"activity":{"details":"First"}}})).await;
        shows(&mut activity, "First").await;
        let mut second = ClientOptions::new().open(&path).unwrap();
        assert_eq!(
            command(&mut second, 0, json!({"v":1,"client_id":"22"})).await["evt"],
            "READY"
        );
        command(&mut second, 1, json!({"cmd":"SET_ACTIVITY","nonce":"second","args":{"pid":2,"activity":{"details":"Second"}}})).await;
        shows(&mut activity, "Second").await;
        drop(second);
        shows(&mut activity, "First").await;
        let mut second = ClientOptions::new().open(&path).unwrap();
        command(&mut second, 0, json!({"v":1,"client_id":"22"})).await;
        command(&mut second, 1, json!({"cmd":"SET_ACTIVITY","nonce":"again","args":{"pid":2,"activity":{"details":"Second"}}})).await;
        shows(&mut activity, "Second").await;
        super::super::write(&mut second, 1, &serde_json::to_vec(&json!({"cmd":"SET_ACTIVITY","nonce":"image","args":{"pid":2,"activity":{"details":"New image","assets":{"large_image":"https://example.com/cover.png"}}}})).unwrap()).await.unwrap();
        timeout(
            Duration::from_secs(1),
            image_loading.wait_for(|loading| *loading),
        )
        .await
        .unwrap()
        .unwrap();
        if close {
            super::super::write(&mut second, 2, &[]).await.unwrap();
            shows(&mut activity, "First").await;
            drop(second);
        } else {
            drop(second);
            shows(&mut activity, "First").await;
        }
        drop(first);
        timeout(Duration::from_secs(1), activity.wait_for(Option::is_none))
            .await
            .unwrap()
            .unwrap();
        drop(activity);
        timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
        http_worker.abort();
    }
}
