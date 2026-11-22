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
}
