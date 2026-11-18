//! The Discord token lives in the OS credential store, next to Yapfast's Matrix
//! session, and never in Matrix. So does the key [`crate::cache`] encrypts
//! its files with.

use std::sync::{Mutex, OnceLock};

#[cfg(not(target_os = "android"))]
use anyhow::anyhow;
use anyhow::{Result, bail};
#[cfg(not(target_os = "android"))]
use keyring::Entry;

#[cfg(not(target_os = "android"))]
const SERVICE: &str = "io.github.yapfast.Yapfast";
/// The linked Discord account, set by linking by QR code or by the developer
/// probe's store-token command.
#[cfg(not(target_os = "android"))]
const ACCOUNT: &str = "discord-token";
/// Where the first probe kept the token; moved to `ACCOUNT` when read.
#[cfg(not(target_os = "android"))]
const EARLIER_ACCOUNT: &str = "discord-probe-token";
/// Where the key [`crate::cache`] encrypts its files with sits in hex.
#[cfg(not(target_os = "android"))]
const CACHE_KEY: &str = "discord-cache-key";

/// Configures the file storing Discord credentials on platforms without a
/// system keyring (Android). No-op elsewhere.
#[allow(unused_variables)]
pub fn keep_in(file: std::path::PathBuf) {
    #[cfg(target_os = "android")]
    android::keep_in(file);
}

#[cfg(not(target_os = "android"))]
fn entry() -> Result<Entry> {
    named(ACCOUNT)
}

#[cfg(not(target_os = "android"))]
fn named(account: &str) -> Result<Entry> {
    Entry::new(SERVICE, account).map_err(|_| anyhow!("could not access the OS credential store"))
}

/// With the test server, the link lasts while Yapfast runs. It starts linked,
/// with [`crate::client::TEST_TOKEN`], unless `YAPFAST_DISCORD_TEST_UNLINKED` is
/// set, which shows linking against the fake.
fn test_link() -> &'static Mutex<Option<String>> {
    static LINK: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    LINK.get_or_init(|| {
        let unlinked = std::env::var_os("YAPFAST_DISCORD_TEST_UNLINKED").is_some();
        Mutex::new((!unlinked).then(|| crate::client::TEST_TOKEN.to_owned()))
    })
}

/// No Discord account is linked on this device, which [`load`] says rather
/// than failing.
#[derive(Debug)]
pub struct NotLinked;

impl std::fmt::Display for NotLinked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no Discord account is linked")
    }
}

impl std::error::Error for NotLinked {}

pub fn store(token: &str) -> Result<()> {
    let token = clean(token)?;
    if crate::client::test_server().is_some() {
        *test_link()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(token);
        return Ok(());
    }
    #[cfg(target_os = "android")]
    {
        android::store(&token)
    }
    #[cfg(not(target_os = "android"))]
    entry()?
        .set_password(&token)
        .map_err(|_| anyhow!("could not save the token in the OS credential store"))
}

pub fn load() -> Result<String> {
    if crate::client::test_server().is_some() {
        return test_link()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .ok_or_else(|| NotLinked.into());
    }
    #[cfg(target_os = "android")]
    {
        android::load()
    }
    #[cfg(not(target_os = "android"))]
    {
        match entry()?.get_password() {
            Ok(token) => return Ok(token),
            Err(keyring::Error::NoEntry) => {}
            Err(_) => bail!("could not read the token from the OS credential store"),
        }
        match named(EARLIER_ACCOUNT)?.get_password() {
            Ok(token) => {
                store(&token)?;
                let _ = named(EARLIER_ACCOUNT)?.delete_credential();
                Ok(token)
            }
            Err(keyring::Error::NoEntry) => Err(NotLinked.into()),
            Err(_) => bail!("could not read the token from the OS credential store"),
        }
    }
}

/// The key [`crate::cache`] encrypts its files with, made the first time.
/// With the test server, always the same one, so what a run kept shows in
/// the next, as with Discord.
pub(crate) fn cache_key() -> Result<crate::cache::Key> {
    if crate::client::test_server().is_some() {
        return Ok([7; 32]);
    }
    #[cfg(target_os = "android")]
    {
        android::cache_key()
    }
    #[cfg(not(target_os = "android"))]
    {
        let entry = named(CACHE_KEY)?;
        match entry.get_password() {
            Ok(hex) => {
                if let Some(key) = crate::cache::read_key(&hex) {
                    return Ok(key);
                }
            }
            Err(keyring::Error::NoEntry) => {}
            Err(_) => bail!("could not read the cache key from the OS credential store"),
        }
        // None yet, or one that isn't a key: what it encrypted can't be read.
        let key = crate::cache::new_key();
        entry
            .set_password(&crate::cache::write_key(&key))
            .map_err(|_| anyhow!("could not save the cache key in the OS credential store"))?;
        Ok(key)
    }
}

pub fn forget() -> Result<()> {
    crate::cache::forget();
    if crate::client::test_server().is_some() {
        *test_link()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        return Ok(());
    }
    #[cfg(target_os = "android")]
    {
        android::forget()
    }
    #[cfg(not(target_os = "android"))]
    {
        for account in [ACCOUNT, EARLIER_ACCOUNT, CACHE_KEY] {
            match named(account)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(_) => bail!("could not remove the token from the OS credential store"),
            }
        }
        Ok(())
    }
}

/// Accept the header value as copied from browser developer tools, with or
/// without surrounding quotes or whitespace.
pub fn clean(token: &str) -> Result<String> {
    let token = token.trim().trim_matches(['"', '\'']).trim();
    if token.is_empty() || token.contains(char::is_whitespace) {
        bail!("That doesn't look like a Discord token.");
    }
    Ok(token.to_owned())
}

#[cfg(test)]
mod tests {
    use super::clean;

    #[test]
    fn cleans_copied_header_values() {
        assert_eq!(clean("  \"abc.def.ghi\"\n").unwrap(), "abc.def.ghi");
        assert!(clean("   ").is_err());
        assert!(clean("Bearer abc").is_err());
    }
}

#[cfg(target_os = "android")]
mod android {
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        path::{Path, PathBuf},
        sync::Mutex,
    };

    use anyhow::{Context, Result, bail};
    use serde::{Deserialize, Serialize};

    use super::NotLinked;

    static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);
    static LOCK: Mutex<()> = Mutex::new(());

    #[derive(Default, Serialize, Deserialize)]
    struct Secrets {
        token: Option<String>,
        cache_key: Option<String>,
    }

    pub(super) fn keep_in(file: PathBuf) {
        *FILE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(file);
    }

    fn path() -> Result<PathBuf> {
        FILE.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .context("Android Discord credential path has not been initialized")
    }

    fn check_private(path: &Path) -> Result<()> {
        match fs::symlink_metadata(path) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    bail!("refusing to follow a symlinked credential file");
                }
                if meta.permissions().mode() & 0o077 != 0 {
                    bail!("Discord credential file must be readable only by its owner");
                }
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("could not inspect Discord credential file"),
        }
    }

    fn read_secrets(path: &Path) -> Result<Secrets> {
        check_private(path)?;
        match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
        {
            Ok(file) => serde_json::from_reader(file)
                .context("could not parse the stored Discord credentials"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Secrets::default()),
            Err(error) => Err(error).context("could not open Discord credential file"),
        }
    }

    fn write_secrets(path: &Path, secrets: &Secrets) -> Result<()> {
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .context("could not create Discord credential directory")?;
        }
        check_private(path)?;
        let partial = path.with_extension("partial");
        check_private(&partial)?;
        let bytes = serde_json::to_vec(secrets)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&partial)
            .context("could not write Discord credential file")?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&partial, path).context("could not replace Discord credential file")?;
        Ok(())
    }

    pub(super) fn store(token: &str) -> Result<()> {
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let path = path()?;
        let mut secrets = read_secrets(&path)?;
        secrets.token = Some(token.to_owned());
        write_secrets(&path, &secrets)
    }

    pub(super) fn load() -> Result<String> {
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let path = path()?;
        let secrets = read_secrets(&path)?;
        secrets.token.ok_or_else(|| NotLinked.into())
    }

    pub(super) fn cache_key() -> Result<crate::cache::Key> {
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let path = path()?;
        let mut secrets = read_secrets(&path)?;
        if let Some(hex) = secrets.cache_key.as_deref()
            && let Some(key) = crate::cache::read_key(hex)
        {
            return Ok(key);
        }
        let key = crate::cache::new_key();
        secrets.cache_key = Some(crate::cache::write_key(&key));
        write_secrets(&path, &secrets)?;
        Ok(key)
    }

    pub(super) fn forget() -> Result<()> {
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok(path) = path() else {
            return Ok(());
        };
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("could not remove Discord credentials"),
        }
    }
}
