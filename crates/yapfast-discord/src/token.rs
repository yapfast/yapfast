//! The Discord token lives in the OS credential store, next to Yapfast's Matrix
//! session, and never in Matrix. So does the key [`crate::cache`] encrypts
//! its files with.

use std::sync::{Mutex, OnceLock};

use anyhow::{Result, anyhow, bail};
#[cfg(not(target_os = "macos"))]
use keyring::Entry;

#[cfg(not(target_os = "macos"))]
const SERVICE: &str = "io.github.yapfast.Yapfast";
/// The linked Discord account, set by linking by QR code or by the developer
/// probe's store-token command.
const ACCOUNT: &str = "discord-token";
/// Where the first probe kept the token; moved to `ACCOUNT` when read.
const EARLIER_ACCOUNT: &str = "discord-probe-token";
/// The key of the files [`crate::cache`] keeps, in hex.
const CACHE_KEY: &str = "discord-cache-key";

#[cfg(not(target_os = "macos"))]
fn entry() -> Result<Entry> {
    named(ACCOUNT)
}

#[cfg(not(target_os = "macos"))]
fn named(account: &str) -> Result<Entry> {
    Entry::new(SERVICE, account).map_err(|_| anyhow!("could not access the OS credential store"))
}

#[cfg(target_os = "macos")]
fn secret_file(account: &str) -> Result<std::path::PathBuf> {
    let name = format!("{account}.secret");
    crate::cache::previews()
        .map(|path| path.with_file_name(&name))
        .or_else(|| {
            std::env::var_os("HOME").map(|home| {
                std::path::PathBuf::from(home)
                    .join("Library/Application Support/yapfast")
                    .join(&name)
            })
        })
        .ok_or_else(|| anyhow!("could not locate the application data directory"))
}

#[cfg(target_os = "macos")]
fn write_secret(account: &str, value: &str) -> Result<()> {
    use std::{
        io::Write,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    };
    let file = secret_file(account)?;
    if let Some(parent) = file.parent() {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(parent)?;
    }
    let partial = file.with_extension("secret.partial");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true).mode(0o600);
    let mut handle = options.open(&partial)?;
    let _ = handle.set_permissions(std::fs::Permissions::from_mode(0o600));
    handle.write_all(value.as_bytes())?;
    handle.sync_all()?;
    drop(handle);
    std::fs::rename(&partial, &file)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_secret(account: &str) -> Result<Option<String>> {
    let file = secret_file(account)?;
    match std::fs::read_to_string(file) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => bail!("could not read the token from the credential store"),
    }
}

#[cfg(target_os = "macos")]
fn delete_secret(account: &str) -> Result<()> {
    let file = secret_file(account)?;
    match std::fs::remove_file(file) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => bail!("could not remove the token from the credential store"),
    }
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
    #[cfg(target_os = "macos")]
    return write_secret(ACCOUNT, &token);
    #[cfg(not(target_os = "macos"))]
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
    #[cfg(target_os = "macos")]
    {
        if let Some(token) = read_secret(ACCOUNT)? {
            return Ok(token);
        }
        if let Some(token) = read_secret(EARLIER_ACCOUNT)? {
            store(&token)?;
            let _ = delete_secret(EARLIER_ACCOUNT);
            return Ok(token);
        }
        return Err(NotLinked.into());
    }
    #[cfg(not(target_os = "macos"))]
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
    #[cfg(target_os = "macos")]
    {
        if let Some(hex) = read_secret(CACHE_KEY)?
            && let Some(key) = crate::cache::read_key(&hex)
        {
            return Ok(key);
        }
        let key = crate::cache::new_key();
        write_secret(CACHE_KEY, &crate::cache::write_key(&key))?;
        return Ok(key);
    }
    #[cfg(not(target_os = "macos"))]
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
    #[cfg(target_os = "macos")]
    {
        for account in [ACCOUNT, EARLIER_ACCOUNT, CACHE_KEY] {
            delete_secret(account)?;
        }
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
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
