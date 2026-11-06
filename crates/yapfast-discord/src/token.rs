//! The Discord token lives in the OS credential store, next to Yapfast's Matrix
//! session, and never in Matrix. So does the key [`crate::cache`] encrypts
//! its files with.

use std::sync::{Mutex, OnceLock};

use anyhow::{Result, anyhow, bail};
use keyring::Entry;

const SERVICE: &str = "io.github.yapfast.Yapfast";
/// The linked Discord account, set by linking by QR code or by the developer
/// probe's store-token command.
const ACCOUNT: &str = "discord-token";
/// Where the first probe kept the token; moved to `ACCOUNT` when read.
const EARLIER_ACCOUNT: &str = "discord-probe-token";
/// The key of the files [`crate::cache`] keeps, in hex.
const CACHE_KEY: &str = "discord-cache-key";

fn entry() -> Result<Entry> {
    named(ACCOUNT)
}

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

/// Deletes credentials from the former single-account store after that
/// login has been migrated into the account registry.
pub fn forget_legacy() -> Result<()> {
    for account in [ACCOUNT, EARLIER_ACCOUNT] {
        match named(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => bail!("could not remove the legacy Discord token"),
        }
    }
    Ok(())
}

/// The key [`crate::cache`] encrypts its files with, made the first time.
/// With the test server, always the same one, so what a run kept shows in
/// the next, as with Discord.
pub(crate) fn cache_key() -> Result<crate::cache::Key> {
    if crate::client::test_server().is_some() {
        return Ok([7; 32]);
    }
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

pub fn forget() -> Result<()> {
    crate::cache::forget();
    if let Some(id) = crate::accounts::active()? {
        return crate::accounts::remove(&id);
    }
    if crate::client::test_server().is_some() {
        *test_link()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        return Ok(());
    }
    for account in [ACCOUNT, EARLIER_ACCOUNT, CACHE_KEY] {
        match named(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => bail!("could not remove the token from the OS credential store"),
        }
    }
    Ok(())
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
