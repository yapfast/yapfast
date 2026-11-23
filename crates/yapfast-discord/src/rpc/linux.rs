//! The local user's Linux socket, with locked, identity-checked cleanup.

use anyhow::{Result, bail, ensure};
use std::{
    fs::{File, Metadata},
    io,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::net::{UnixListener, UnixStream};

pub(super) type Stream = UnixStream;

pub(super) fn runtime_directory() -> PathBuf {
    ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .filter_map(std::env::var_os)
        .find(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// The lock coordinates Yapfast processes; probing the socket also respects
/// Discord and other RPC servers, which do not take our lock.
pub(super) struct Socket {
    listener: UnixListener,
    pub(super) path: PathBuf,
    identity: Metadata,
    _lock: File,
}

impl Socket {
    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "the Windows accept() replaces its pipe instance, and serve() calls both alike"
    )]
    pub(super) async fn accept(&mut self) -> Result<UnixStream> {
        let (stream, _) = self.listener.accept().await?;
        ensure!(
            stream
                .peer_cred()
                .is_ok_and(|peer| peer.uid() == self.identity.uid()),
            "RPC client belongs to another user"
        );
        Ok(stream)
    }

    pub(super) async fn bind(directory: &Path) -> Result<Self> {
        let uid = std::fs::metadata("/proc/self")?.uid();
        for slot in 0..10 {
            let path = directory.join(format!("discord-ipc-{slot}"));
            let lock_path = directory.join(format!("yapfast-discord-ipc-{slot}.lock"));
            use std::os::unix::fs::OpenOptionsExt;
            let lock = match File::options()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&lock_path)
            {
                Ok(lock) => lock,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    // A replaced lock must neither follow a symlink nor
                    // block opening a FIFO.
                    let Ok(lock) = File::options()
                        .write(true)
                        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                        .open(&lock_path)
                    else {
                        continue;
                    };
                    let metadata = lock.metadata()?;
                    let link = std::fs::symlink_metadata(&lock_path)?;
                    if !link.is_file() || metadata.uid() != uid || !same_file(&metadata, &link) {
                        continue;
                    }
                    lock
                }
                Err(_) => continue,
            };
            if lock.try_lock().is_err() {
                continue;
            }
            if let Ok(metadata) = std::fs::symlink_metadata(&path) {
                if !metadata.file_type().is_socket() || metadata.uid() != uid {
                    continue;
                }
                // A live server with a full backlog must not block a Tokio
                // worker while we look for a free slot.
                match tokio::time::timeout(Duration::from_millis(100), UnixStream::connect(&path))
                    .await
                {
                    Ok(Err(error)) if error.kind() == io::ErrorKind::ConnectionRefused => {
                        if !std::fs::symlink_metadata(&path)
                            .is_ok_and(|now| same_file(&metadata, &now))
                        {
                            continue;
                        }
                        std::fs::remove_file(&path)?;
                    }
                    _ => continue,
                }
            }
            let Ok(listener) = UnixListener::bind(&path) else {
                continue;
            };
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            let identity = std::fs::symlink_metadata(&path)?;
            return Ok(Self {
                listener,
                path,
                identity,
                _lock: lock,
            });
        }
        bail!("no free Discord IPC socket in {}", directory.display())
    }
}

fn same_file(left: &Metadata, right: &Metadata) -> bool {
    left.ino() == right.ino() && left.dev() == right.dev()
}

impl Drop for Socket {
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path).is_ok_and(|now| same_file(&self.identity, &now)) {
            let _ = std::fs::remove_file(&self.path);
        }
        // Leave the lock inode in place: unlinking a locked file would let
        // another process create a second independent lock for the same slot.
    }
}
