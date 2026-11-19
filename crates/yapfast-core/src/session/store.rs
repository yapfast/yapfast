use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use keyring::Entry;
use matrix_sdk::authentication::matrix::MatrixSession;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::i18n::later;

const SERVICE: &str = "io.github.yapfast.Yapfast";

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretStorage {
    Keyring,
    PrivateFile,
}

impl SecretStorage {
    pub fn notice(self) -> &'static str {
        match self {
            Self::Keyring => later("Credentials are stored in the OS credential store."),
            Self::PrivateFile => later(
                "Secret Service is unavailable. Credentials are in a private file readable by your Linux user.",
            ),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ActiveSession {
    version: u32,
    id: Uuid,
    storage: SecretStorage,
}

// Deliberately no Debug implementation: this contains authentication secrets.
#[derive(Serialize, Deserialize)]
pub struct SavedSession {
    pub homeserver: String,
    pub passphrase: String,
    pub session: MatrixSession,
}

/// The process lock covers the entire lifetime of the SDK client and its stores.
pub struct SessionStore {
    root: PathBuf,
    _lock: File,
}

impl SessionStore {
    pub fn open(root: PathBuf) -> Result<Self> {
        private_directory(&root)?;
        let lock_path = root.join("session.lock");
        reject_symlink(&lock_path)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options
            .open(lock_path)
            .context("could not open the session lock")?;
        lock.try_lock()
            .context("another Yapfast process is using this session directory")?;
        Ok(Self { root, _lock: lock })
    }

    pub fn load(&self) -> Result<Option<(Uuid, SecretStorage, SavedSession)>> {
        let path = self.root.join("active.json");
        reject_symlink(&path)?;
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => bail!("could not read session metadata"),
        };
        let active: ActiveSession = serde_json::from_slice(&bytes)
            .context("session metadata is damaged; preserve the stores before repairing it")?;
        if active.version != 1 {
            bail!("this session was written by an unsupported Yapfast version")
        }
        let bytes = match active.storage {
            SecretStorage::Keyring => entry(active.id)?.get_secret().map_err(|_| anyhow::anyhow!("session credentials are unavailable in the OS credential store; unlock it and retry"))?,
            SecretStorage::PrivateFile => {
                #[cfg(not(target_os = "linux"))]
                bail!("private-file credentials are supported only on Linux");
                #[cfg(target_os = "linux")]
                {
                    let path = self.secret_path(active.id);
                    check_private_file(&path)?;
                    fs::read(path).context("could not read private session credentials")?
                }
            }
        };
        let saved = serde_json::from_slice(&bytes).map_err(|_| {
            anyhow::anyhow!(
                "session credentials are damaged; preserve the crypto store before repairing them"
            )
        })?;
        if !self.sdk_path(active.id).is_dir() {
            bail!("the session's crypto store is missing; restore it before signing in again")
        }
        Ok(Some((active.id, active.storage, saved)))
    }

    pub fn choose_storage(&self) -> Result<SecretStorage> {
        if Entry::store_status().is_ok() {
            return Ok(SecretStorage::Keyring);
        }
        #[cfg(target_os = "linux")]
        {
            Ok(SecretStorage::PrivateFile)
        }
        #[cfg(not(target_os = "linux"))]
        {
            bail!("the OS credential store is unavailable; unlock it before signing in")
        }
    }

    pub fn sdk_path(&self, id: Uuid) -> PathBuf {
        self.root.join("stores").join(id.to_string())
    }
    pub fn prepare_sdk_store(&self, id: Uuid) -> Result<PathBuf> {
        let path = self.sdk_path(id);
        private_directory(&path)?;
        Ok(path)
    }
    fn secret_path(&self, id: Uuid) -> PathBuf {
        self.root.join(format!("{id}.secret"))
    }

    /// Retain the store password separately from revocable login credentials.
    pub fn save_store_key(&self, id: Uuid, storage: SecretStorage, passphrase: &str) -> Result<()> {
        match storage {
            SecretStorage::Keyring => Entry::new(SERVICE, &format!("{id}-store-key"))
                .and_then(|entry| entry.set_password(passphrase))
                .map_err(|_| {
                    anyhow::anyhow!(
                        "could not preserve the crypto store password in the OS credential store"
                    )
                })?,
            SecretStorage::PrivateFile => {
                #[cfg(not(target_os = "linux"))]
                bail!("private-file credentials are supported only on Linux");
                #[cfg(target_os = "linux")]
                atomic_write(
                    &self.sdk_path(id).join("store-key.secret"),
                    passphrase.as_bytes(),
                )?;
            }
        }
        atomic_write(
            &self.sdk_path(id).join("store-key.json"),
            &serde_json::to_vec(&ActiveSession {
                version: 1,
                id,
                storage,
            })?,
        )
    }

    pub fn save(&self, id: Uuid, storage: SecretStorage, saved: &SavedSession) -> Result<()> {
        self.save_store_key(id, storage, &saved.passphrase)?;
        let bytes = serde_json::to_vec(saved).context("could not encode session credentials")?;
        match storage {
            SecretStorage::Keyring => entry(id)?.set_secret(&bytes).map_err(|_| {
                anyhow::anyhow!("could not save credentials in the OS credential store")
            })?,
            SecretStorage::PrivateFile => {
                #[cfg(not(target_os = "linux"))]
                bail!("private-file credentials are supported only on Linux");
                #[cfg(target_os = "linux")]
                atomic_write(&self.secret_path(id), &bytes)?;
            }
        }
        atomic_write(
            &self.root.join("active.json"),
            &serde_json::to_vec(&ActiveSession {
                version: 1,
                id,
                storage,
            })?,
        )
    }

    /// Keep encrypted SDK stores. Logging out must never silently delete history keys.
    pub fn clear(&self, id: Uuid, storage: SecretStorage) -> Result<()> {
        // Remove credentials first so a failed deletion can be retried using metadata.
        match storage {
            SecretStorage::Keyring => match entry(id)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(_) => {
                    bail!("could not remove session credentials from the OS credential store")
                }
            },
            SecretStorage::PrivateFile => match fs::remove_file(self.secret_path(id)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => bail!("could not remove private session credentials"),
            },
        }
        fs::remove_file(self.root.join("active.json"))
            .context("could not clear active session metadata")
    }
}

fn entry(id: Uuid) -> Result<Entry> {
    Entry::new(SERVICE, &id.to_string())
        .map_err(|_| anyhow::anyhow!("could not access the OS credential store"))
}

fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!("a session path is a symbolic link; choose a private directory")
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => bail!("could not inspect a session path"),
    }
}

fn private_directory(path: &Path) -> Result<()> {
    reject_symlink(path)?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .context("could not create the session directory")?;
    #[cfg(unix)]
    check_private_file(path)?;
    Ok(())
}

#[cfg(unix)]
fn check_private_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    reject_symlink(path)?;
    let metadata = fs::metadata(path).context("private session file is missing")?;
    if metadata.permissions().mode() & 0o077 != 0 {
        bail!(
            "session files must be readable only by their owner; fix permissions before continuing"
        )
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    reject_symlink(path)?;
    let parent = path.parent().context("session path has no directory")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)
        .context("could not create a private session file")?;
    file.write_all(bytes)
        .context("could not write session data")?;
    file.as_file()
        .sync_all()
        .context("could not flush session data")?;
    file.persist(path)
        .map_err(|_| anyhow::anyhow!("could not replace session data"))?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn another_process_cannot_open_the_same_crypto_store() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("session");
        let store = SessionStore::open(root.clone()).unwrap();
        assert!(SessionStore::open(root.clone()).is_err());
        drop(store);
        assert!(SessionStore::open(root).is_ok());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn private_session_round_trip_keeps_tokens_out_of_metadata() {
        use matrix_sdk::{SessionMeta, authentication::SessionTokens, ruma::user_id};
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::open(dir.path().join("session")).unwrap();
        let id = Uuid::new_v4();
        store.prepare_sdk_store(id).unwrap();
        let saved = SavedSession {
            homeserver: "http://localhost:18008".into(),
            passphrase: "test-store-password".into(),
            session: MatrixSession {
                meta: SessionMeta {
                    user_id: user_id!("@alice:synapse").to_owned(),
                    device_id: "TEST".into(),
                },
                tokens: SessionTokens {
                    access_token: "test-access-token".into(),
                    refresh_token: None,
                },
            },
        };
        store.save(id, SecretStorage::PrivateFile, &saved).unwrap();
        let metadata = fs::read_to_string(store.root.join("active.json")).unwrap();
        assert!(!metadata.contains("test-access-token"));
        assert!(!metadata.contains("test-store-password"));
        let (_, _, restored) = store.load().unwrap().unwrap();
        assert_eq!(restored.session, saved.session);
        check_private_file(&store.secret_path(id)).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(store.secret_path(id), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(store.load().is_err());
        fs::set_permissions(store.secret_path(id), fs::Permissions::from_mode(0o600)).unwrap();
        fs::remove_file(store.secret_path(id)).unwrap();
        assert!(store.load().is_err());
        store.save(id, SecretStorage::PrivateFile, &saved).unwrap();
        store.clear(id, SecretStorage::PrivateFile).unwrap();
        assert!(store.load().unwrap().is_none());
        assert!(store.sdk_path(id).is_dir());
        assert_eq!(
            fs::read_to_string(store.sdk_path(id).join("store-key.secret")).unwrap(),
            saved.passphrase
        );
    }
    #[test]
    fn damaged_metadata_never_becomes_a_new_session() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::open(dir.path().join("session")).unwrap();
        atomic_write(&store.root.join("active.json"), b"broken").unwrap();
        assert!(store.load().is_err());
        assert_eq!(fs::read(store.root.join("active.json")).unwrap(), b"broken");
    }
}
