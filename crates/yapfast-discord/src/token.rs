//! The Discord token lives in the OS credential store, next to Yapfast's Matrix
//! session, and never in Matrix. So do how it was linked ([`Origin`]) and the
//! key [`crate::cache`] encrypts its files with.

use std::{
    fmt::Write as _,
    sync::{Mutex, OnceLock},
};

use anyhow::{Result, anyhow, bail};
use keyring::Entry;
use sha2::{Digest, Sha256};

const SERVICE: &str = "io.github.yapfast.Yapfast";
/// The linked Discord account, set by linking by QR code or by the developer
/// probe's store-token command.
const ACCOUNT: &str = "discord-token";
/// Where the first probe kept the token; moved to `ACCOUNT` when read.
const EARLIER_ACCOUNT: &str = "discord-probe-token";
/// How the login was linked, as [`marker`] writes it. A login without one,
/// linked before Yapfast kept this, is [`Origin::Unknown`].
const ORIGIN: &str = "discord-token-origin";
/// The key of the files [`crate::cache`] keeps, in hex.
const CACHE_KEY: &str = "discord-cache-key";

/// How the login on this device came to Yapfast, which decides whether
/// unlinking ends its session on Discord.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Yapfast's own login, by QR code: a session of its own.
    Qr,
    /// A token the person pasted: the session of the app or browser it was
    /// copied from.
    Pasted,
    /// Linked before Yapfast kept how, or by the developer probe.
    Unknown,
}

impl Origin {
    /// Whether unlinking ends the session on Discord: only for Yapfast's own
    /// login. Ending a pasted token's session signs out the app it came
    /// from, so an unknown one is left as a pasted one is.
    pub fn ends_session(self) -> bool {
        self == Self::Qr
    }
}

/// What [`ORIGIN`] keeps for `token` linked as `origin`: how, and a hash of
/// the login it's for, so one left beside another login, by an earlier
/// version say, doesn't count. Nothing for [`Origin::Unknown`].
fn marker(origin: Origin, token: &str) -> Option<String> {
    let how = match origin {
        Origin::Qr => "qr",
        Origin::Pasted => "pasted",
        Origin::Unknown => return None,
    };
    Some(format!("{how} {}", fingerprint(token)))
}

fn fingerprint(token: &str) -> String {
    let mut hash = String::with_capacity(64);
    for byte in Sha256::digest(token.as_bytes()) {
        let _ = write!(hash, "{byte:02x}");
    }
    hash
}

/// How `token` was linked, by the marker kept beside it: unknown without
/// one, or with one for another login or that can't be read.
fn origin_in(marker: Option<&str>, token: &str) -> Origin {
    let Some((how, hash)) = marker.and_then(|marker| marker.split_once(' ')) else {
        return Origin::Unknown;
    };
    if hash != fingerprint(token) {
        return Origin::Unknown;
    }
    match how {
        "qr" => Origin::Qr,
        "pasted" => Origin::Pasted,
        _ => Origin::Unknown,
    }
}

fn entry() -> Result<Entry> {
    named(ACCOUNT)
}

fn named(account: &str) -> Result<Entry> {
    Entry::new(SERVICE, account).map_err(|_| anyhow!("could not access the OS credential store"))
}

/// With the test server, the link lasts while Yapfast runs. It starts linked,
/// with [`crate::client::TEST_TOKEN`] and no marker, as a login an earlier
/// version linked, unless `YAPFAST_DISCORD_TEST_UNLINKED` is set, which shows
/// linking against the fake.
fn test_link() -> &'static Mutex<Option<String>> {
    static LINK: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    LINK.get_or_init(|| {
        let unlinked = std::env::var_os("YAPFAST_DISCORD_TEST_UNLINKED").is_some();
        Mutex::new((!unlinked).then(|| crate::client::TEST_TOKEN.to_owned()))
    })
}

/// The test server's [`ORIGIN`].
fn test_marker() -> &'static Mutex<Option<String>> {
    static MARKER: Mutex<Option<String>> = Mutex::new(None);
    &MARKER
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

/// Links `token` on this device, keeping how it was linked.
pub fn store_as(token: &str, origin: Origin) -> Result<()> {
    let token = clean(token)?;
    save(&token, origin)
}

/// Keeps `token` in place of the linked login, as when Discord refreshes it:
/// it was linked as the one it replaces was.
pub fn store(token: &str) -> Result<()> {
    let token = clean(token)?;
    save(&token, self::origin())
}

/// The token first: either way round, a write that fails leaves a marker
/// for another login, which counts as unknown.
fn save(token: &str, origin: Origin) -> Result<()> {
    write(token)?;
    if let Err(error) = write_marker(marker(origin, token)) {
        tracing::warn!(%error, "could not save how Discord was linked");
    }
    Ok(())
}

fn write(token: &str) -> Result<()> {
    if crate::client::test_server().is_some() {
        *test_link()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(token.to_owned());
        return Ok(());
    }
    entry()?
        .set_password(token)
        .map_err(|_| anyhow!("could not save the token in the OS credential store"))
}

fn write_marker(marker: Option<String>) -> Result<()> {
    if crate::client::test_server().is_some() {
        *test_marker()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = marker;
        return Ok(());
    }
    let entry = named(ORIGIN)?;
    match marker {
        Some(marker) => entry.set_password(&marker),
        None => match entry.delete_credential() {
            Err(keyring::Error::NoEntry) => Ok(()),
            done => done,
        },
    }
    .map_err(|_| anyhow!("could not save how Discord was linked in the OS credential store"))
}

fn read_marker() -> Option<String> {
    if crate::client::test_server().is_some() {
        return test_marker()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
    }
    named(ORIGIN).ok()?.get_password().ok()
}

/// How the linked login was linked: [`Origin::Unknown`] when none is, or
/// Yapfast can't tell.
pub fn origin() -> Origin {
    match load() {
        Ok(token) => origin_in(read_marker().as_deref(), &token),
        Err(_) => Origin::Unknown,
    }
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
            write(&token)?;
            let _ = named(EARLIER_ACCOUNT)?.delete_credential();
            Ok(token)
        }
        Err(keyring::Error::NoEntry) => Err(NotLinked.into()),
        Err(_) => bail!("could not read the token from the OS credential store"),
    }
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
    if crate::client::test_server().is_some() {
        *test_link()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *test_marker()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        return Ok(());
    }
    for account in [ACCOUNT, EARLIER_ACCOUNT, ORIGIN, CACHE_KEY] {
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
    use super::{Origin, clean, marker, origin_in};

    #[test]
    fn cleans_copied_header_values() {
        assert_eq!(clean("  \"abc.def.ghi\"\n").unwrap(), "abc.def.ghi");
        assert!(clean("   ").is_err());
        assert!(clean("Bearer abc").is_err());
    }

    #[test]
    fn only_yapfasts_own_login_ends_its_session() {
        assert!(Origin::Qr.ends_session());
        assert!(!Origin::Pasted.ends_session());
        assert!(!Origin::Unknown.ends_session());
    }

    #[test]
    fn reads_how_a_login_was_linked() {
        for origin in [Origin::Qr, Origin::Pasted] {
            let kept = marker(origin, "abc.def.ghi");
            assert_eq!(origin_in(kept.as_deref(), "abc.def.ghi"), origin);
        }
        assert_eq!(marker(Origin::Unknown, "abc.def.ghi"), None);
    }

    #[test]
    fn a_login_without_a_marker_of_its_own_is_unknown() {
        // Linked before Yapfast kept how.
        assert_eq!(origin_in(None, "abc.def.ghi"), Origin::Unknown);
        // A marker left beside another login.
        let other = marker(Origin::Qr, "old.login.here");
        assert_eq!(origin_in(other.as_deref(), "abc.def.ghi"), Origin::Unknown);
        // One that can't be read.
        assert_eq!(origin_in(Some("qr"), "abc.def.ghi"), Origin::Unknown);
        let hash = marker(Origin::Qr, "abc.def.ghi").unwrap();
        let odd = hash.replacen("qr", "phone", 1);
        assert_eq!(origin_in(Some(&odd), "abc.def.ghi"), Origin::Unknown);
    }
}
