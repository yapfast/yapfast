//! Multiple Discord identities stored independently in the operating system's
//! credential store. This module deliberately stores metadata separately from
//! credentials: listing accounts never reads or exposes their tokens.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow, bail};
use keyring::Entry;
use serde::{Deserialize, Serialize};

const SERVICE: &str = "io.github.yapfast.Yapfast";
const INDEX: &str = "discord-account-index";
const ACTIVE: &str = "discord-active-account";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub username: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Registry {
    accounts: BTreeMap<String, Account>,
}

fn entry(name: &str) -> Result<Entry> {
    Entry::new(SERVICE, name).map_err(|_| anyhow!("could not access the OS credential store"))
}

fn registry() -> Result<Registry> {
    match entry(INDEX)?.get_password() {
        Ok(value) => serde_json::from_str(&value)
            .map_err(|_| anyhow!("the saved Discord account list is damaged")),
        Err(keyring::Error::NoEntry) => Ok(Registry::default()),
        Err(_) => bail!("could not read the saved Discord account list"),
    }
}

fn save_registry(value: &Registry) -> Result<()> {
    entry(INDEX)?.set_password(&serde_json::to_string(value)?)
        .map_err(|_| anyhow!("could not save the Discord account list"))
}

/// Lists linked accounts without reading any credential.
pub fn list() -> Result<Vec<Account>> {
    Ok(registry()?.accounts.into_values().collect())
}

/// Returns the account selected for the next backend start.
pub fn active() -> Result<Option<String>> {
    match entry(ACTIVE)?.get_password() {
        Ok(id) => Ok(Some(id)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => bail!("could not read the selected Discord account"),
    }
}

/// Saves a token under its Discord user ID and selects that account.
/// The caller must validate the token with Discord before calling this.
pub fn save(id: &str, username: &str, token: &str) -> Result<()> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        bail!("Discord returned an invalid account ID");
    }
    let token = token.trim().trim_matches(['"', '\'']).trim();
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        bail!("That does not look like a Discord token.");
    }
    entry(&format!("discord-token-{id}"))?.set_password(token)
        .map_err(|_| anyhow!("could not save the Discord token"))?;
    let mut saved = registry()?;
    saved.accounts.insert(id.to_owned(), Account {
        id: id.to_owned(),
        username: username.to_owned(),
    });
    save_registry(&saved)?;
    entry(ACTIVE)?.set_password(id)
        .map_err(|_| anyhow!("could not select the Discord account"))
}

/// Reads the token for one account. Tokens must never be logged or displayed.
pub fn token(id: &str) -> Result<String> {
    entry(&format!("discord-token-{id}"))?.get_password()
        .map_err(|_| anyhow!("could not read the selected Discord token"))
}

/// Selects an already-linked account for the next backend start.
pub fn select(id: &str) -> Result<()> {
    if !registry()?.accounts.contains_key(id) {
        bail!("that Discord account is not linked on this device");
    }
    entry(ACTIVE)?.set_password(id)
        .map_err(|_| anyhow!("could not select the Discord account"))
}

/// Removes one account without touching other account credentials.
pub fn remove(id: &str) -> Result<()> {
    match entry(&format!("discord-token-{id}"))?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(_) => bail!("could not remove the Discord token"),
    }
    let mut saved = registry()?;
    saved.accounts.remove(id);
    save_registry(&saved)?;
    if active()?.as_deref() == Some(id) {
        match saved.accounts.keys().next() {
            Some(next) => select(next)?,
            None => { let _ = entry(ACTIVE)?.delete_credential(); }
        }
    }
    Ok(())
}
