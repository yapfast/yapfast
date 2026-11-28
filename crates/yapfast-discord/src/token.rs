//! The Discord logins live in the OS credential store, next to Yapfast's Matrix
//! session, and never in Matrix. So does the key [`crate::cache`] encrypts
//! its files with.
//!
//! As in Discord's own apps, several accounts are linked on one device, and
//! one of them is the active account, which Yapfast connects.
//! Each account is one credential, `discord-account-<id>`: its login, and the
//! little the account switcher shows of it before it connects (its username,
//! name, and its avatar's address) and how it was linked ([`Origin`]), which
//! decides whether logging it out ends its session on Discord. Another,
//! `discord-accounts`, lists their ids and says which is active. A login
//! linked before there could be several, under `discord-token`, becomes an
//! account of its own the first time they're read, linked as the marker
//! beside it, `discord-token-origin`, says.

use std::{
    collections::HashMap,
    fmt::Write as _,
    sync::{Mutex, MutexGuard, OnceLock},
};

use anyhow::{Result, anyhow, bail};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD},
};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SERVICE: &str = "io.github.yapfast.Yapfast";
/// The linked accounts' ids, and which one is active.
const INDEX: &str = "discord-accounts";
/// The start of each account's credential, which its id ends.
const ACCOUNT_PREFIX: &str = "discord-account-";
/// Where the login linked before accounts were several was kept, by QR code
/// or by the developer probe's store-token command; moved to an account when
/// read.
const ACCOUNT: &str = "discord-token";
/// Where the first probe kept the token; moved the same way.
const EARLIER_ACCOUNT: &str = "discord-probe-token";
/// How the login under `ACCOUNT` was linked, as [`marker`] writes it, kept
/// beside it before there could be several; moved into its account with it.
const ORIGIN: &str = "discord-token-origin";
/// The key of the files [`crate::cache`] keeps, in hex.
const CACHE_KEY: &str = "discord-cache-key";

/// How a login came to Yapfast, which decides whether logging it out ends its
/// session on Discord.
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
    /// Whether logging out ends the session on Discord: only for Yapfast's
    /// own login. Ending a pasted token's session signs out the app it came
    /// from, so an unknown one is left as a pasted one is.
    pub fn ends_session(self) -> bool {
        self == Self::Qr
    }

    /// As an account's credential keeps it: empty for [`Origin::Unknown`].
    fn name(self) -> &'static str {
        match self {
            Self::Qr => "qr",
            Self::Pasted => "pasted",
            Self::Unknown => "",
        }
    }

    /// What [`Origin::name`] wrote; unknown for anything else.
    fn named(name: &str) -> Self {
        match name {
            "qr" => Self::Qr,
            "pasted" => Self::Pasted,
            _ => Self::Unknown,
        }
    }
}

/// What [`ORIGIN`] kept for `token` linked as `origin`: how, and a hash of
/// the login it's for, so one left beside another login, by an earlier
/// version say, doesn't count. Nothing for [`Origin::Unknown`].
#[cfg(test)]
fn marker(origin: Origin, token: &str) -> Option<String> {
    match origin {
        Origin::Unknown => None,
        origin => Some(format!("{} {}", origin.name(), fingerprint(token))),
    }
}

fn fingerprint(token: &str) -> String {
    let mut hash = String::with_capacity(64);
    for byte in Sha256::digest(token.as_bytes()) {
        let _ = write!(hash, "{byte:02x}");
    }
    hash
}

/// How `token` was linked, by the [`ORIGIN`] marker kept beside it: unknown
/// without one, or with one for another login or that can't be read.
fn origin_in(marker: Option<&str>, token: &str) -> Origin {
    let Some((how, hash)) = marker.and_then(|marker| marker.split_once(' ')) else {
        return Origin::Unknown;
    };
    if hash != fingerprint(token) {
        return Origin::Unknown;
    }
    Origin::named(how)
}

fn named(account: &str) -> Result<Entry> {
    Entry::new(SERVICE, account).map_err(|_| anyhow!("could not access the OS credential store"))
}

/// Where the credentials are kept: the OS credential store, or, with the test
/// server, memory, so what a run links lasts while Yapfast runs.
trait Vault {
    fn read(&self, name: &str) -> Result<Option<String>>;
    fn write(&self, name: &str, value: &str) -> Result<()>;
    fn delete(&self, name: &str) -> Result<()>;
}

/// The OS credential store.
struct Os;

impl Vault for Os {
    fn read(&self, name: &str) -> Result<Option<String>> {
        match named(name)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => bail!("could not read the login from the OS credential store"),
        }
    }

    fn write(&self, name: &str, value: &str) -> Result<()> {
        named(name)?
            .set_password(value)
            .map_err(|_| anyhow!("could not save the login in the OS credential store"))
    }

    fn delete(&self, name: &str) -> Result<()> {
        match named(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => bail!("could not remove the login from the OS credential store"),
        }
    }
}

/// Credentials in memory, for the test server and for tests.
#[derive(Default)]
struct Memory(Mutex<HashMap<String, String>>);

impl Memory {
    fn items(&self) -> MutexGuard<'_, HashMap<String, String>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Vault for Memory {
    fn read(&self, name: &str) -> Result<Option<String>> {
        Ok(self.items().get(name).cloned())
    }

    fn write(&self, name: &str, value: &str) -> Result<()> {
        self.items().insert(name.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<()> {
        self.items().remove(name);
        Ok(())
    }
}

/// With the test server, the links last while Yapfast runs. It starts with
/// [`crate::client::TEST_TOKEN`] linked, not saying how, as a login an earlier
/// version linked, unless `YAPFAST_DISCORD_TEST_UNLINKED` is set, which
/// shows linking against the fake. With
/// `YAPFAST_DISCORD_TEST_SECOND_ACCOUNT` set, [`crate::client::TEST_SECOND_TOKEN`]
/// is linked too, which shows the account switcher with two. With
/// `YAPFAST_DISCORD_TEST_LEGACY_ACCOUNT` set, only the login an earlier version
/// kept is there, which shows an account that has no profile stored yet.
fn test_vault() -> &'static Memory {
    static VAULT: OnceLock<Memory> = OnceLock::new();
    VAULT.get_or_init(|| {
        let memory = Memory::default();
        let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
        if set("YAPFAST_DISCORD_TEST_LEGACY_ACCOUNT") {
            // The login of an install from before there could be several,
            // which the first read moves, with no profile kept for it.
            let _ = memory.write(ACCOUNT, crate::client::TEST_TOKEN);
        } else if !set("YAPFAST_DISCORD_TEST_UNLINKED") {
            let id = id_of(crate::client::TEST_TOKEN);
            let _ = link(&memory, crate::client::TEST_TOKEN, true, None);
            let _ = describe_in(
                &memory,
                &id,
                "alice",
                "Alice",
                &crate::ready::avatar("1400000000000000001", Some("alice1")),
            );
            if set("YAPFAST_DISCORD_TEST_SECOND_ACCOUNT") {
                let id = id_of(crate::client::TEST_SECOND_TOKEN);
                let _ = link(&memory, crate::client::TEST_SECOND_TOKEN, false, None);
                let _ = describe_in(
                    &memory,
                    &id,
                    "bea",
                    "Bea",
                    &crate::ready::avatar("1400000000000000099", Some("bea1")),
                );
            }
        }
        memory
    })
}

/// Reads and changes go one at a time, since each reads the list of accounts
/// and writes it back, and the session refreshing a login can come while
/// another account is linked.
static GUARD: Mutex<()> = Mutex::new(());

fn with_vault<T>(run: impl FnOnce(&dyn Vault) -> Result<T>) -> Result<T> {
    let _guard = GUARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if crate::client::test_server().is_some() {
        run(test_vault())
    } else {
        run(&Os)
    }
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

/// The accounts' ids, in the order they were linked, and the active one.
#[derive(Default, Serialize, Deserialize)]
struct Index {
    #[serde(default)]
    active: Option<String>,
    #[serde(default)]
    ids: Vec<String>,
}

impl Index {
    /// The active account: the one set, or, if that's gone, the first.
    fn active(&self) -> Option<&str> {
        self.active
            .as_deref()
            .filter(|active| self.ids.iter().any(|id| id == active))
            .or_else(|| self.ids.first().map(String::as_str))
    }
}

/// An account's credential: its login, how it was linked, and what the
/// switcher shows of it.
#[derive(Default, Serialize, Deserialize)]
struct Saved {
    token: String,
    /// [`Origin::name`]; empty, or missing, when Yapfast can't tell.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    linked: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    avatar: String,
}

/// A linked account, as the account switcher lists it. Its login stays in the
/// credential store.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Linked {
    pub id: String,
    /// Such as `derfayberr`; empty until the account has connected once.
    pub username: String,
    pub name: String,
    /// The address of its avatar; empty until the account has connected once.
    pub avatar: String,
}

/// The accounts linked on this device, and which is active.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub active: Option<String>,
    pub accounts: Vec<Linked>,
}

/// Which account a login belongs to: the user id its first part spells, as
/// Discord's logins do, or, for one that doesn't, such as the test server's, a
/// hash of it. It only has to name the account, and stay the same for it.
fn id_of(token: &str) -> String {
    let spelled = token.split('.').next().and_then(|part| {
        let part = part.trim_end_matches('=');
        let bytes = STANDARD_NO_PAD
            .decode(part)
            .or_else(|_| URL_SAFE_NO_PAD.decode(part))
            .ok()?;
        String::from_utf8(bytes).ok()
    });
    match spelled {
        Some(id)
            if (15..=20).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            id
        }
        _ => {
            let digest = Sha256::digest(token.as_bytes());
            let mut hash = String::with_capacity(16);
            for byte in digest.iter().take(8) {
                let _ = write!(hash, "{byte:02x}");
            }
            format!("login-{hash}")
        }
    }
}

fn account_name(id: &str) -> String {
    format!("{ACCOUNT_PREFIX}{id}")
}

fn read_index(vault: &dyn Vault) -> Result<Index> {
    match vault.read(INDEX)? {
        Some(text) => Ok(serde_json::from_str(&text).unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read the list of Discord accounts");
            Index::default()
        })),
        None => migrate(vault),
    }
}

fn write_index(vault: &dyn Vault, index: &Index) -> Result<()> {
    vault.write(INDEX, &serde_json::to_string(index)?)
}

fn read_saved(vault: &dyn Vault, id: &str) -> Result<Option<Saved>> {
    Ok(vault
        .read(&account_name(id))?
        .and_then(|text| serde_json::from_str(&text).ok()))
}

fn write_saved(vault: &dyn Vault, id: &str, saved: &Saved) -> Result<()> {
    vault.write(&account_name(id), &serde_json::to_string(saved)?)
}

/// The login linked before there could be several, made the account of its
/// own, linked as the marker beside it says: one without a marker of its own
/// is unknown, as it was then.
fn migrate(vault: &dyn Vault) -> Result<Index> {
    let mut index = Index::default();
    for earlier in [ACCOUNT, EARLIER_ACCOUNT] {
        let Some(token) = vault.read(earlier)? else {
            continue;
        };
        let id = id_of(&token);
        let origin = origin_in(vault.read(ORIGIN)?.as_deref(), &token);
        write_saved(
            vault,
            &id,
            &Saved {
                token,
                linked: origin.name().to_owned(),
                ..Saved::default()
            },
        )?;
        index = Index {
            active: Some(id.clone()),
            ids: vec![id],
        };
        write_index(vault, &index)?;
        for earlier in [ACCOUNT, EARLIER_ACCOUNT, ORIGIN] {
            vault.delete(earlier)?;
        }
        break;
    }
    Ok(index)
}

/// The account `token` belongs to among those linked: the one with its id,
/// else the one that holds it, which a login Discord refreshed may.
fn locate(vault: &dyn Vault, index: &Index, token: &str) -> Result<Option<String>> {
    let id = id_of(token);
    if index.ids.contains(&id) {
        return Ok(Some(id));
    }
    for id in &index.ids {
        if read_saved(vault, id)?.is_some_and(|saved| saved.token == token) {
            return Ok(Some(id.clone()));
        }
    }
    Ok(None)
}

/// Links `token`, or, if its account is linked already, takes its new login.
/// `activate` makes it the active account. A login linked as `origin` keeps
/// it; with none, an account linked already stays as it was linked, and a new
/// one is unknown.
fn link(vault: &dyn Vault, token: &str, activate: bool, origin: Option<Origin>) -> Result<String> {
    let mut index = read_index(vault)?;
    let id = match locate(vault, &index, token)? {
        Some(id) => id,
        None => {
            let id = id_of(token);
            index.ids.push(id.clone());
            id
        }
    };
    let mut saved = read_saved(vault, &id)?.unwrap_or_default();
    token.clone_into(&mut saved.token);
    if let Some(origin) = origin {
        origin.name().clone_into(&mut saved.linked);
    }
    write_saved(vault, &id, &saved)?;
    if activate || index.active().is_none() {
        index.active = Some(id.clone());
    }
    write_index(vault, &index)?;
    Ok(id)
}

fn describe_in(
    vault: &dyn Vault,
    id: &str,
    username: &str,
    name: &str,
    avatar: &str,
) -> Result<()> {
    let Some(mut saved) = read_saved(vault, id)? else {
        return Ok(());
    };
    if (
        saved.username.as_str(),
        saved.name.as_str(),
        saved.avatar.as_str(),
    ) == (username, name, avatar)
    {
        return Ok(());
    }
    username.clone_into(&mut saved.username);
    name.clone_into(&mut saved.name);
    avatar.clone_into(&mut saved.avatar);
    write_saved(vault, id, &saved)
}

fn listing_in(vault: &dyn Vault) -> Result<Listing> {
    let index = read_index(vault)?;
    let mut accounts = Vec::with_capacity(index.ids.len());
    for id in &index.ids {
        let Some(saved) = read_saved(vault, id)? else {
            continue;
        };
        accounts.push(Linked {
            id: id.clone(),
            username: saved.username,
            name: saved.name,
            avatar: saved.avatar,
        });
    }
    Ok(Listing {
        active: index.active().map(str::to_owned),
        accounts,
    })
}

/// Forgets the account `id`: whether it was the active one, and the active
/// account after, which is the first left if it was.
fn forget_in(vault: &dyn Vault, id: &str) -> Result<(bool, Option<String>)> {
    let mut index = read_index(vault)?;
    let was_active = index.active() == Some(id);
    index.ids.retain(|other| other != id);
    vault.delete(&account_name(id))?;
    if was_active {
        index.active = index.ids.first().cloned();
    }
    write_index(vault, &index)?;
    Ok((was_active, index.active().map(str::to_owned)))
}

/// Links the account `token` belongs to, linked as `origin`, and makes it the
/// active one; one linked already takes the login and how it came.
pub fn store_as(token: &str, origin: Origin) -> Result<()> {
    let token = clean(token)?;
    with_vault(|vault| link(vault, &token, true, Some(origin)).map(drop))
}

/// Links the account `token` belongs to, and makes it the active one, not
/// saying how: one linked already takes the login and stays as it was
/// linked, and a new one is [`Origin::Unknown`].
pub fn store(token: &str) -> Result<()> {
    let token = clean(token)?;
    with_vault(|vault| link(vault, &token, true, None).map(drop))
}

/// Links a login imported from the vanilla Discord client. Unlike [`store`],
/// it doesn't take over the active account from one already linked: an import
/// adds accounts beside the one in use, and only becomes active when none was.
/// An already linked account keeps its stored session; only its name is updated.
/// Returns whether the account was new, so the caller can count what it added.
pub fn adopt(token: &str, username: &str, name: &str) -> Result<bool> {
    let token = clean(token)?;
    with_vault(|vault| adopt_in(vault, &token, username, name))
}

fn adopt_in(vault: &dyn Vault, token: &str, username: &str, name: &str) -> Result<bool> {
    let index = read_index(vault)?;
    let known = locate(vault, &index, token)?;
    let added = known.is_none();
    let id = match known {
        Some(id) => id,
        None => link(vault, token, false, None)?,
    };
    let avatar = read_saved(vault, &id)?.unwrap_or_default().avatar;
    describe_in(vault, &id, username, name, &avatar)?;
    Ok(added)
}

/// Discord refreshed the login `earlier` to `token`: its account keeps it,
/// where it was in the list, whether it's the active one and how it was
/// linked. Nothing is linked if the account was logged out meanwhile.
pub fn refresh(earlier: &str, token: &str) -> Result<()> {
    with_vault(|vault| {
        let index = read_index(vault)?;
        let Some(id) = locate(vault, &index, earlier)? else {
            return Ok(());
        };
        let mut saved = read_saved(vault, &id)?.unwrap_or_default();
        token.clone_into(&mut saved.token);
        write_saved(vault, &id, &saved)
    })
}

/// Keeps what the account switcher shows of the account `token` belongs to,
/// as the account just connected.
pub fn describe(token: &str, username: &str, name: &str, avatar: &str) -> Result<()> {
    with_vault(|vault| {
        let index = read_index(vault)?;
        match locate(vault, &index, token)? {
            Some(id) => describe_in(vault, &id, username, name, avatar),
            None => Ok(()),
        }
    })
}

/// The active account's login.
pub fn load() -> Result<String> {
    with_vault(|vault| {
        let index = read_index(vault)?;
        let saved = match index.active() {
            Some(id) => read_saved(vault, id)?,
            None => None,
        };
        match saved {
            Some(saved) if !saved.token.is_empty() => Ok(saved.token),
            _ => Err(NotLinked.into()),
        }
    })
}

/// How the active account was linked: [`Origin::Unknown`] when none is, or
/// Yapfast can't tell.
pub fn origin() -> Origin {
    with_vault(|vault| match read_index(vault)?.active() {
        Some(id) => origin_in_vault(vault, id),
        None => Ok(Origin::Unknown),
    })
    .unwrap_or(Origin::Unknown)
}

/// How the account `id` was linked, whichever is active.
pub fn origin_of(id: &str) -> Origin {
    with_vault(|vault| origin_in_vault(vault, id)).unwrap_or(Origin::Unknown)
}

fn origin_in_vault(vault: &dyn Vault, id: &str) -> Result<Origin> {
    Ok(read_saved(vault, id)?.map_or(Origin::Unknown, |saved| Origin::named(&saved.linked)))
}

/// The login of the account `id`, whichever is active.
pub fn token_of(id: &str) -> Result<String> {
    with_vault(|vault| match read_saved(vault, id)? {
        Some(saved) if !saved.token.is_empty() => Ok(saved.token),
        _ => Err(NotLinked.into()),
    })
}

/// The accounts linked on this device, in the order they were linked, and
/// which is active.
pub fn listing() -> Result<Listing> {
    with_vault(listing_in)
}

/// Makes the account `id` the active one, which [`load`] gives from then on.
pub fn switch(id: &str) -> Result<()> {
    with_vault(|vault| {
        let mut index = read_index(vault)?;
        if !index.ids.iter().any(|other| other == id) {
            bail!("That account isn't linked on this device.");
        }
        index.active = Some(id.to_owned());
        write_index(vault, &index)
    })
}

/// Forgets the account `id`, and the files kept for it if it was the active
/// one. The active account after is the first left if it was, and none if
/// that was the last; otherwise it's the one that was.
pub fn forget_account(id: &str) -> Result<Option<String>> {
    let (was_active, active) = with_vault(|vault| forget_in(vault, id))?;
    if was_active {
        crate::cache::forget();
    }
    Ok(active)
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

/// Forgets every account on this device, and the files kept for them.
pub fn forget() -> Result<()> {
    crate::cache::forget();
    with_vault(|vault| {
        for id in read_index(vault)?.ids {
            vault.delete(&account_name(&id))?;
        }
        for name in [INDEX, ACCOUNT, EARLIER_ACCOUNT, ORIGIN, CACHE_KEY] {
            vault.delete(name)?;
        }
        Ok(())
    })
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
    use super::*;

    /// A login as Discord makes them: the user id first, in base64.
    fn login(id: &str, rest: &str) -> String {
        format!("{}.{rest}", STANDARD_NO_PAD.encode(id))
    }

    #[test]
    fn importing_a_known_account_preserves_its_independent_session() {
        let vault = Memory::default();
        let id = "100000000000000001";
        let independent = login(id, "qr-session");
        link(&vault, &independent, true, None).unwrap();
        describe_in(&vault, id, "old", "Old", "avatar").unwrap();
        assert!(!adopt_in(&vault, &login(id, "desktop-session"), "new", "New").unwrap());
        let saved = read_saved(&vault, id).unwrap().unwrap();
        assert_eq!(saved.token, independent);
        assert_eq!(
            (
                saved.username.as_str(),
                saved.name.as_str(),
                saved.avatar.as_str()
            ),
            ("new", "New", "avatar")
        );
    }

    fn ids(vault: &dyn Vault) -> Vec<String> {
        listing_in(vault)
            .unwrap()
            .accounts
            .into_iter()
            .map(|account| account.id)
            .collect()
    }

    #[test]
    fn cleans_copied_header_values() {
        assert_eq!(clean("  \"abc.def.ghi\"\n").unwrap(), "abc.def.ghi");
        assert!(clean("   ").is_err());
        assert!(clean("Bearer abc").is_err());
    }

    #[test]
    fn names_an_account_by_the_user_id_in_its_login() {
        assert_eq!(
            id_of(&login("1400000000000000001", "abc.def")),
            "1400000000000000001"
        );
        // Not a login of Discord's: a stable name of its own.
        let test = id_of("yapfast-test-token");
        assert!(test.starts_with("login-"));
        assert_eq!(test, id_of("yapfast-test-token"));
        assert_ne!(test, id_of("yapfast-test-token-two"));
    }

    #[test]
    fn a_migrated_login_is_listed_without_a_profile_until_described() {
        let vault = Memory::default();
        let token = login("1400000000000000001", "abc.def");
        vault.write(ACCOUNT, &token).unwrap();
        let listed = listing_in(&vault).unwrap();
        assert_eq!(listed.active.as_deref(), Some("1400000000000000001"));
        let [account] = listed.accounts.as_slice() else {
            panic!("one account");
        };
        assert_eq!(account.id, "1400000000000000001");
        assert!(
            account.username.is_empty() && account.name.is_empty() && account.avatar.is_empty()
        );
        assert!(vault.read(ACCOUNT).unwrap().is_none(), "moved, not copied");

        // What the connection says of the account is kept with it, and kept
        // current when it changes.
        let id = locate(&vault, &read_index(&vault).unwrap(), &token)
            .unwrap()
            .unwrap();
        describe_in(
            &vault,
            &id,
            "derfayberr",
            "Fayber",
            "https://cdn.test/a.png",
        )
        .unwrap();
        describe_in(
            &vault,
            &id,
            "derfayberr",
            "Fabian",
            "https://cdn.test/a.png",
        )
        .unwrap();
        let account = &listing_in(&vault).unwrap().accounts[0];
        assert_eq!(
            (
                account.username.as_str(),
                account.name.as_str(),
                account.avatar.as_str()
            ),
            ("derfayberr", "Fabian", "https://cdn.test/a.png")
        );
        assert_eq!(read_saved(&vault, &id).unwrap().unwrap().token, token);
    }

    #[test]
    fn links_accounts_and_the_last_linked_is_active() {
        let vault = Memory::default();
        assert_eq!(listing_in(&vault).unwrap(), Listing::default());
        let (first, second) = (
            login("100000000000000001", "a"),
            login("100000000000000002", "b"),
        );
        link(&vault, &first, true, None).unwrap();
        link(&vault, &second, true, None).unwrap();
        let listing = listing_in(&vault).unwrap();
        assert_eq!(listing.active.as_deref(), Some("100000000000000002"));
        assert_eq!(ids(&vault), ["100000000000000001", "100000000000000002"]);
    }

    #[test]
    fn linking_an_account_again_takes_its_new_login_and_activates_it() {
        let vault = Memory::default();
        link(&vault, &login("100000000000000001", "old"), true, None).unwrap();
        link(&vault, &login("100000000000000002", "b"), true, None).unwrap();
        link(&vault, &login("100000000000000001", "new"), true, None).unwrap();
        assert_eq!(ids(&vault).len(), 2, "one account, not two");
        let listing = listing_in(&vault).unwrap();
        assert_eq!(listing.active.as_deref(), Some("100000000000000001"));
        let saved = read_saved(&vault, "100000000000000001").unwrap().unwrap();
        assert_eq!(saved.token, login("100000000000000001", "new"));
    }

    #[test]
    fn describes_an_account_for_the_switcher() {
        let vault = Memory::default();
        let token = login("100000000000000001", "a");
        let id = link(&vault, &token, true, None).unwrap();
        assert_eq!(listing_in(&vault).unwrap().accounts[0].name, "");
        describe_in(&vault, &id, "ann", "Ann", "https://cdn/ann.png").unwrap();
        let account = listing_in(&vault).unwrap().accounts.remove(0);
        assert_eq!(
            (
                account.username.as_str(),
                account.name.as_str(),
                account.avatar.as_str()
            ),
            ("ann", "Ann", "https://cdn/ann.png")
        );
        // An account that isn't linked isn't made by describing it.
        describe_in(&vault, "100000000000000009", "x", "X", "").unwrap();
        assert_eq!(ids(&vault).len(), 1);
    }

    #[test]
    fn importing_preserves_the_active_account_and_existing_avatar() {
        let vault = Memory::default();
        let first = login("100000000000000001", "a");
        let second = login("100000000000000002", "b");
        link(&vault, &first, true, None).unwrap();
        assert!(adopt_in(&vault, &second, "bea", "Bea").unwrap());
        assert_eq!(
            listing_in(&vault).unwrap().active.as_deref(),
            Some("100000000000000001")
        );
        describe_in(
            &vault,
            "100000000000000002",
            "bea",
            "Bea",
            "https://cdn/bea.png",
        )
        .unwrap();
        assert!(!adopt_in(&vault, &second, "bea", "New name").unwrap());
        let account = &listing_in(&vault).unwrap().accounts[1];
        assert_eq!(account.avatar, "https://cdn/bea.png");
        assert_eq!(account.name, "New name");
    }

    #[test]
    fn the_first_imported_account_becomes_active() {
        let vault = Memory::default();
        adopt_in(&vault, &login("100000000000000001", "a"), "ann", "Ann").unwrap();
        assert_eq!(
            listing_in(&vault).unwrap().active.as_deref(),
            Some("100000000000000001")
        );
    }

    #[test]
    fn a_refreshed_login_keeps_its_account_in_place() {
        let vault = Memory::default();
        // Logins that don't spell an id are named by a hash of the first, which
        // the refreshed one doesn't match: it is found by the login it replaces.
        let id = link(&vault, "first-login", true, None).unwrap();
        link(&vault, "other-login", true, None).unwrap();
        let index = read_index(&vault).unwrap();
        assert_eq!(
            locate(&vault, &index, "first-login").unwrap(),
            Some(id.clone())
        );
        let mut saved = read_saved(&vault, &id).unwrap().unwrap();
        saved.token = "refreshed-login".into();
        write_saved(&vault, &id, &saved).unwrap();
        let index = read_index(&vault).unwrap();
        assert_eq!(locate(&vault, &index, "refreshed-login").unwrap(), Some(id));
        // The active account stays the one that was.
        assert_eq!(index.active(), Some(id_of("other-login").as_str()));
    }

    #[test]
    fn forgetting_the_active_account_activates_the_first_left() {
        let vault = Memory::default();
        for number in 1..=3 {
            link(
                &vault,
                &login(&format!("10000000000000000{number}"), "x"),
                true,
                None,
            )
            .unwrap();
        }
        // Not the active one: it stays active.
        let (was_active, active) = forget_in(&vault, "100000000000000001").unwrap();
        assert!(!was_active);
        assert_eq!(active.as_deref(), Some("100000000000000003"));
        // The active one: the first left takes over.
        let (was_active, active) = forget_in(&vault, "100000000000000003").unwrap();
        assert!(was_active);
        assert_eq!(active.as_deref(), Some("100000000000000002"));
        let (_, active) = forget_in(&vault, "100000000000000002").unwrap();
        assert_eq!(active, None);
        assert_eq!(listing_in(&vault).unwrap(), Listing::default());
        assert!(read_saved(&vault, "100000000000000002").unwrap().is_none());
    }

    #[test]
    fn moves_the_login_linked_before_accounts_were_several() {
        let vault = Memory::default();
        let token = login("100000000000000001", "a");
        vault.write(ACCOUNT, &token).unwrap();
        let listing = listing_in(&vault).unwrap();
        assert_eq!(listing.active.as_deref(), Some("100000000000000001"));
        assert_eq!(listing.accounts.len(), 1);
        assert!(vault.read(ACCOUNT).unwrap().is_none(), "moved, not copied");
        assert_eq!(
            read_saved(&vault, "100000000000000001")
                .unwrap()
                .unwrap()
                .token,
            token
        );
        // The first probe's login moves the same way.
        let vault = Memory::default();
        vault.write(EARLIER_ACCOUNT, &token).unwrap();
        assert_eq!(listing_in(&vault).unwrap().accounts.len(), 1);
        assert!(vault.read(EARLIER_ACCOUNT).unwrap().is_none());
    }

    #[test]
    fn the_active_account_is_the_first_when_the_one_set_is_gone() {
        let index = Index {
            active: Some("gone".into()),
            ids: vec!["a".into(), "b".into()],
        };
        assert_eq!(index.active(), Some("a"));
        assert_eq!(Index::default().active(), None);
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
            assert_eq!(Origin::named(origin.name()), origin);
        }
        assert_eq!(marker(Origin::Unknown, "abc.def.ghi"), None);
        assert_eq!(Origin::named(""), Origin::Unknown);
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

    #[test]
    fn each_account_keeps_how_it_was_linked() {
        let vault = Memory::default();
        let (qr, pasted) = (
            login("100000000000000001", "a"),
            login("100000000000000002", "b"),
        );
        let qr_id = link(&vault, &qr, true, Some(Origin::Qr)).unwrap();
        let pasted_id = link(&vault, &pasted, true, Some(Origin::Pasted)).unwrap();
        assert_eq!(origin_in_vault(&vault, &qr_id).unwrap(), Origin::Qr);
        assert_eq!(origin_in_vault(&vault, &pasted_id).unwrap(), Origin::Pasted);

        // A login Discord refreshed keeps how its account was linked, and so
        // does linking it again without saying how.
        let mut saved = read_saved(&vault, &qr_id).unwrap().unwrap();
        saved.token = login("100000000000000001", "refreshed");
        write_saved(&vault, &qr_id, &saved).unwrap();
        link(&vault, &login("100000000000000001", "again"), false, None).unwrap();
        assert_eq!(origin_in_vault(&vault, &qr_id).unwrap(), Origin::Qr);
        // Linking it again by pasting its token makes it a pasted one.
        link(
            &vault,
            &login("100000000000000001", "c"),
            false,
            Some(Origin::Pasted),
        )
        .unwrap();
        assert_eq!(origin_in_vault(&vault, &qr_id).unwrap(), Origin::Pasted);
        // One linked without saying how is unknown, and so is one not linked.
        let other = link(&vault, &login("100000000000000003", "d"), false, None).unwrap();
        assert_eq!(origin_in_vault(&vault, &other).unwrap(), Origin::Unknown);
        assert_eq!(
            origin_in_vault(&vault, "100000000000000009").unwrap(),
            Origin::Unknown
        );
    }

    #[test]
    fn the_moved_login_keeps_how_it_was_linked() {
        let token = login("100000000000000001", "a");
        let moved = |marker: Option<String>| {
            let vault = Memory::default();
            vault.write(ACCOUNT, &token).unwrap();
            if let Some(marker) = marker {
                vault.write(ORIGIN, &marker).unwrap();
            }
            let id = listing_in(&vault).unwrap().active.unwrap();
            assert!(vault.read(ORIGIN).unwrap().is_none(), "moved, not copied");
            origin_in_vault(&vault, &id).unwrap()
        };
        assert_eq!(moved(marker(Origin::Qr, &token)), Origin::Qr);
        assert_eq!(moved(marker(Origin::Pasted, &token)), Origin::Pasted);
        // Linked before Yapfast kept how: as a pasted one, its session stays.
        assert_eq!(moved(None), Origin::Unknown);
        // A marker for another login doesn't count.
        assert_eq!(moved(marker(Origin::Qr, "another.login")), Origin::Unknown);
    }
}
