//! Finding the Discord logins the vanilla desktop client already keeps on this
//! device, so Yapfast can link the same accounts without the phone step.
//!
//! Discord's desktop app is Chromium, and keeps each login in its local
//! storage, under `Local Storage/leveldb`. The value is Discord's own
//! obfuscation prefix, [`PREFIX`], then the login encrypted the way Chromium
//! encrypts its passwords: `v10`, a 12-byte nonce, the ciphertext, and a
//! 16-byte tag, AES-256-GCM, under a key the OS keeps for the app (DPAPI on
//! Windows). A bounded copy is opened in memory with a LevelDB reader. Replay
//! is best effort: a bad log record can silently stop that log before later
//! updates or deletions are recovered in rusty-leveldb 4.0.1.
//! The source files are only read. The caller validates candidates with Discord.
//!
//! The OS decides whether this works: a process can read another app's DPAPI
//! secret only as the same user, which is the case here. Importing is offered
//! only where [`supported`] is true.

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, SystemTime},
};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use rusty_leveldb::{DB, LdbIterator, MemEnv, Options};

/// Discord's obfuscation prefix on each stored login: a rickroll, as a joke.
/// The encrypted login is the base64 that follows it.
const PREFIX: &[u8] = b"dQw4w9WgXcQ:";
/// Chromium's marker on a login encrypted with the app's key.
const SCHEME: &[u8] = b"v10";
/// AES-GCM's nonce, then its tag, in bytes.
const NONCE: usize = 12;
const TAG: usize = 16;
// Limits apply before a file is allocated or a candidate is decoded.
const MAX_FILE_BYTES: u64 = 8 << 20;
const MAX_TOTAL_BYTES: u64 = 16 << 20;
const MAX_FILES: usize = 128;
const MAX_CANDIDATES: usize = 64;
const MAX_ENCODED_BYTES: usize = 2048;

/// One of Discord's desktop flavours, each its own install with its own data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavour {
    Stable,
    Ptb,
    Canary,
}

impl Flavour {
    /// Every flavour, in the order they're looked for.
    const ALL: [Flavour; 3] = [Flavour::Stable, Flavour::Ptb, Flavour::Canary];

    /// What the account switcher can call where a login came from.
    pub fn label(self) -> &'static str {
        match self {
            Flavour::Stable => "Discord",
            Flavour::Ptb => "Discord PTB",
            Flavour::Canary => "Discord Canary",
        }
    }

    /// The name of this flavour's data directory under the per-user config.
    fn dir(self) -> &'static str {
        match self {
            Flavour::Stable => "discord",
            Flavour::Ptb => "discordptb",
            Flavour::Canary => "discordcanary",
        }
    }
}

/// A login read from the vanilla client, and which flavour it came from. The
/// token is as Discord stored it; whether the account is still good is for the
/// caller to check against the API, as the files may hold logged-out logins.
#[derive(Clone)]
pub struct Found {
    pub token: String,
    pub flavour: Flavour,
}

impl std::fmt::Debug for Found {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Found")
            .field("token", &"[redacted]")
            .field("flavour", &self.flavour)
            .finish()
    }
}

/// Candidates from readable profiles and the reason each other profile failed.
#[derive(Default, Debug)]
pub struct Discovery {
    pub accounts: Vec<Found>,
    pub unreadable_profiles: Vec<ProfileFailure>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileFailure {
    pub flavour: Flavour,
    pub cause: ReadFailure,
}

impl ProfileFailure {
    pub fn message(self) -> String {
        format!("{} profile: {}", self.flavour.label(), self.cause)
    }
}

/// Safe explanations for the UI; the underlying error chain is logged separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadFailure {
    Location,
    Changed,
    Limit,
    MissingState,
    Key,
    Storage,
}

impl ReadFailure {
    pub fn from_error(error: &anyhow::Error) -> Self {
        if error.is::<ChangedDuringRead>() {
            Self::Changed
        } else {
            error
                .downcast_ref::<Self>()
                .copied()
                .unwrap_or(Self::Storage)
        }
    }
}

impl std::fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Location => "the Windows app data directory could not be found. Check your user profile or use QR login.",
            Self::Changed => "storage kept changing while being read. Close Discord and retry.",
            Self::Limit => "storage exceeds the import size limit. Use QR login to link this account.",
            Self::MissingState => "the Local State file is missing. Open Discord once or use QR login.",
            Self::Key => "Windows could not read or unlock the profile key. Use QR login to link this account.",
            Self::Storage => "storage could not be read. Check file access or use QR login.",
        })
    }
}

impl std::error::Error for ReadFailure {}

/// Whether the OS key for the vanilla client can be opened here.
pub fn supported() -> bool {
    cfg!(windows)
}

/// Reads recovered stored values from each installed flavour. Candidates still
/// need API validation; a saved session can have been revoked since it was kept.
/// One unreadable profile does not discard another profile's candidates.
pub fn discover() -> Result<Discovery> {
    if !supported() {
        return Err(Unsupported.into());
    }
    let root = config_root().ok_or(ReadFailure::Location)?;
    Ok(discover_profiles(&root, read_flavour))
}

fn discover_profiles(root: &Path, mut read: impl FnMut(&Path) -> Result<Vec<String>>) -> Discovery {
    let mut discovery = Discovery::default();
    let mut seen = HashSet::new();
    for flavour in Flavour::ALL {
        let dir = root.join(flavour.dir());
        if !dir.is_dir() {
            continue;
        }
        let logins = match read(&dir) {
            Ok(logins) => logins,
            Err(error) => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "could not read a Discord app profile"
                );
                discovery.unreadable_profiles.push(ProfileFailure {
                    flavour,
                    cause: ReadFailure::from_error(&error),
                });
                continue;
            }
        };
        for token in logins {
            if seen.insert(token.clone()) && discovery.accounts.len() < MAX_CANDIDATES {
                discovery.accounts.push(Found { token, flavour });
            }
        }
    }
    discovery
}

fn read_flavour(dir: &Path) -> Result<Vec<String>> {
    let leveldb = dir.join("Local Storage").join("leveldb");
    if !leveldb.exists() {
        return Ok(Vec::new());
    }
    let state = read_bounded(&dir.join("Local State"), 2 << 20).map_err(|error| {
        if error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
        {
            error.context(ReadFailure::MissingState)
        } else {
            error
        }
    })?;
    let state = std::str::from_utf8(&state).context("the Discord app's Local State isn't UTF-8")?;
    let key = decrypt_key(state).context(ReadFailure::Key)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| {
        anyhow!("the Discord app's key is the wrong size").context(ReadFailure::Key)
    })?;
    read_logins(&leveldb, &cipher)
}

/// The database can recover and compact only its private in-memory copy.
fn read_logins(dir: &Path, cipher: &Aes256Gcm) -> Result<Vec<String>> {
    let mut db = DB::open("discord", snapshot(dir)?)
        .context("could not open the Discord app's storage copy")?;
    let mut entries = db
        .new_iter()
        .context("could not read the Discord app's stored values")?;
    let mut logins = Vec::new();
    while entries.advance() {
        let Some((_, value)) = entries.current() else {
            continue;
        };
        for blob in encrypted_logins(&value) {
            if let Some(token) = decrypt_login(cipher, blob)
                && !logins.contains(&token)
            {
                logins.push(token);
                if logins.len() >= MAX_CANDIDATES {
                    return Ok(logins);
                }
            }
        }
    }
    Ok(logins)
}

/// Captures the recovery files in memory without a second full-file buffer.
/// File sizes and mtimes detect concurrent changes, not corrupt log records:
/// rusty-leveldb 4.0.1 can stop replay at a bad record without reporting it.
fn snapshot(dir: &Path) -> Result<Options> {
    retry_copy(|| snapshot_once(dir), std::thread::sleep)
}

fn snapshot_once(dir: &Path) -> Result<Options> {
    let files = storage_files(dir)?;
    let options = Options {
        env: Rc::new(Box::new(MemEnv::new())),
        create_if_missing: false,
        reuse_logs: false,
        reuse_manifest: false,
        max_open_files: MAX_FILES,
        ..Options::default()
    };
    for (name, stamp) in &files {
        let writer = options
            .env
            .open_writable_file(&Path::new("discord").join(name))
            .context("could not prepare the storage copy")?;
        copy_storage_file(&dir.join(name), *stamp, writer)?;
    }
    if storage_files(dir)? != files {
        return Err(ChangedDuringRead.into());
    }
    Ok(options)
}

fn copy_storage_file(
    path: &Path,
    expected: (u64, SystemTime),
    mut writer: impl Write,
) -> Result<()> {
    let (mut file, before) = open_source(path, MAX_FILE_BYTES)?;
    if (before.len(), before.modified()?) != expected {
        return Err(ChangedDuringRead.into());
    }
    let copied = std::io::copy(
        &mut Read::by_ref(&mut file).take(MAX_FILE_BYTES + 1),
        &mut writer,
    )?;
    let after = file.metadata()?;
    if copied != expected.0 || (after.len(), after.modified()?) != expected {
        return Err(ChangedDuringRead.into());
    }
    Ok(())
}

#[derive(Debug)]
struct ChangedDuringRead;

impl std::fmt::Display for ChangedDuringRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the Discord app's storage changed while reading")
    }
}

impl std::error::Error for ChangedDuringRead {}

fn retry_copy<T>(
    mut copy: impl FnMut() -> Result<T>,
    mut backoff: impl FnMut(Duration),
) -> Result<T> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match copy() {
            Err(error)
                if attempts < 3
                    && (error.is::<ChangedDuringRead>()
                        || error
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)) =>
            {
                backoff(Duration::from_millis(25 * attempts));
            }
            result => return result,
        }
    }
}

fn storage_files(dir: &Path) -> Result<BTreeMap<PathBuf, (u64, SystemTime)>> {
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(text) = name.to_str() else {
            continue;
        };
        let numeric = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let needed = text == "CURRENT"
            || text.strip_prefix("MANIFEST-").is_some_and(numeric)
            || text
                .split_once('.')
                .is_some_and(|(n, ext)| numeric(n) && matches!(ext, "ldb" | "log"));
        if !needed {
            continue;
        }
        if !entry.file_type()?.is_file() {
            bail!("unexpected storage file type");
        }
        let meta = entry.metadata()?;
        total = total.saturating_add(meta.len());
        if meta.len() > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES || files.len() >= MAX_FILES {
            return Err(ReadFailure::Limit.into());
        }
        files.insert(PathBuf::from(name), (meta.len(), meta.modified()?));
    }
    Ok(files)
}

fn open_source(path: &Path, limit: u64) -> Result<(fs::File, fs::Metadata)> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        bail!("unexpected storage file type");
    }
    let file = fs::File::open(path)?;
    let before = file.metadata()?;
    if before.len() > limit {
        return Err(ReadFailure::Limit.into());
    }
    Ok((file, before))
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    retry_copy(|| read_bounded_once(path, limit), std::thread::sleep)
}

fn read_bounded_once(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let (mut file, before) = open_source(path, limit)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    if bytes.len() as u64 != before.len()
        || before.len() != after.len()
        || before.modified()? != after.modified()?
    {
        return Err(ChangedDuringRead.into());
    }
    Ok(bytes)
}

/// Decode markers inside one recovered value, with bounded candidate lengths.
fn encrypted_logins(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut blobs = Vec::new();
    let mut from = 0;
    while from < bytes.len() && blobs.len() < MAX_CANDIDATES {
        let Some(at) = find(&bytes[from..], PREFIX) else {
            break;
        };
        let start = from + at + PREFIX.len();
        let end = start
            + bytes[start..]
                .iter()
                .take_while(|byte| is_base64(**byte))
                .count();
        if (1..=MAX_ENCODED_BYTES).contains(&(end - start))
            && let Ok(blob) = STANDARD.decode(&bytes[start..end])
        {
            blobs.push(blob);
        }
        from = end;
    }
    blobs
}

/// Decrypts one login blob with the app's key, or `None` if it isn't a login
/// of the form this understands or the key doesn't open it (a stale blob from
/// another key, say).
fn decrypt_login(cipher: &Aes256Gcm, blob: Vec<u8>) -> Option<String> {
    if blob.len() < SCHEME.len() + NONCE + TAG || &blob[..SCHEME.len()] != SCHEME {
        return None;
    }
    let (nonce, body) = blob[SCHEME.len()..].split_at(NONCE);
    let plain = cipher.decrypt(Nonce::from_slice(nonce), body).ok()?;
    let token = String::from_utf8(plain).ok()?;
    // Logins are ASCII; anything else is a blob that happened to decrypt.
    (!token.is_empty() && token.is_ascii()).then_some(token)
}

/// Where the Discord flavours keep their per-user data.
#[cfg(windows)]
fn config_root() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(not(windows))]
fn config_root() -> Option<PathBuf> {
    // Reached only through tests; importing is unsupported off Windows.
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

/// Unwraps the app's AES key from its `Local State` file, decrypting it with
/// the OS secret the app wrapped it under.
#[cfg(windows)]
fn decrypt_key(local_state: &str) -> Result<Vec<u8>> {
    let state: serde_json::Value =
        serde_json::from_str(local_state).context("the Discord app's Local State isn't JSON")?;
    let wrapped = state
        .get("os_crypt")
        .and_then(|crypt| crypt.get("encrypted_key"))
        .and_then(|key| key.as_str())
        .ok_or_else(|| anyhow!("the Discord app's Local State has no stored key"))?;
    let mut wrapped = STANDARD
        .decode(wrapped)
        .context("the Discord app's stored key isn't base64")?;
    // Chromium prefixes the wrapped key with "DPAPI"; the rest is what the OS
    // secret protects.
    const MARK: &[u8] = b"DPAPI";
    if wrapped.len() < MARK.len() || &wrapped[..MARK.len()] != MARK {
        bail!("the Discord app's key isn't in the expected form");
    }
    wrapped.drain(..MARK.len());
    dpapi_unprotect(&wrapped)
}

#[cfg(not(windows))]
fn decrypt_key(_local_state: &str) -> Result<Vec<u8>> {
    Err(Unsupported.into())
}

/// Hands a blob to the Windows data-protection API, which returns it in the
/// clear when the current user is the one it was protected for, as it is for
/// anything the Discord app on this account stored.
#[cfg(windows)]
fn dpapi_unprotect(wrapped: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
        },
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(wrapped.len())
            .context("the Discord app's wrapped key is too large")?,
        pbData: wrapped.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: `input` points at `wrapped` for the length given, and `output`
    // is read only after a success and freed with LocalFree, as the API asks.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        bail!("Windows wouldn't unlock the Discord app's key");
    }
    // SAFETY: on success the API gives a buffer of cbData bytes at pbData.
    let key = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    // SAFETY: the buffer is the API's to free, with LocalFree.
    unsafe {
        LocalFree(output.pbData as *mut _);
    }
    Ok(key)
}

/// The first index of `needle` in `haystack`, or `None`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Whether a byte is one base64 uses, the alphabet and its padding.
fn is_base64(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')
}

/// Returned where the vanilla client's logins can't be read on this system,
/// so the caller can say so rather than show it as a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsupported;

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Importing from the Discord app isn't supported on this system yet.")
    }
}

impl std::error::Error for Unsupported {}

#[cfg(test)]
mod tests {
    use super::*;

    fn seal(key: &[u8], token: &str) -> Vec<u8> {
        let cipher = Aes256Gcm::new_from_slice(key).unwrap();
        let nonce = [9u8; NONCE];
        let body = cipher
            .encrypt(Nonce::from_slice(&nonce), token.as_bytes())
            .unwrap();
        let mut blob = SCHEME.to_vec();
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&body);
        blob
    }

    #[test]
    fn decodes_two_candidates_from_a_current_value() {
        let key = [3u8; 32];
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let first = seal(&key, "first.token.value");
        let second = seal(&key, "second.token.value");
        // One value can contain several encrypted sessions with separators.
        let mut bytes = b"\x00\x01key\x00\x02".to_vec();
        bytes.extend_from_slice(PREFIX);
        bytes.extend_from_slice(STANDARD.encode(&first).as_bytes());
        bytes.extend_from_slice(b"\x00\x02other\x00");
        bytes.extend_from_slice(PREFIX);
        bytes.extend_from_slice(STANDARD.encode(&second).as_bytes());
        bytes.extend_from_slice(b"\xff\xfftrailing");

        let blobs = encrypted_logins(&bytes);
        assert_eq!(blobs.len(), 2);
        let tokens: Vec<_> = blobs
            .into_iter()
            .filter_map(|blob| decrypt_login(&cipher, blob))
            .collect();
        assert_eq!(tokens, ["first.token.value", "second.token.value"]);
    }

    #[test]
    fn ignores_a_blob_another_key_sealed() {
        let ours = Aes256Gcm::new_from_slice(&[1u8; 32]).unwrap();
        let theirs = seal(&[2u8; 32], "someone.elses.token");
        assert_eq!(decrypt_login(&ours, theirs), None);
    }

    #[test]
    fn ignores_bytes_that_only_look_like_a_login() {
        let cipher = Aes256Gcm::new_from_slice(&[4u8; 32]).unwrap();
        let mut bytes = PREFIX.to_vec();
        bytes.extend_from_slice(b"not-valid-base64-or-a-login!!!");
        // No panic, and nothing taken as a login.
        let blobs = encrypted_logins(&bytes);
        assert!(
            blobs
                .iter()
                .all(|blob| decrypt_login(&cipher, blob.clone()).is_none())
        );
    }

    #[test]
    fn an_incomplete_marker_at_the_end_is_not_a_login() {
        assert!(encrypted_logins(PREFIX).is_empty());
        let mut bytes = b"other data\0".to_vec();
        bytes.extend_from_slice(PREFIX);
        assert!(encrypted_logins(&bytes).is_empty());
    }

    #[test]
    fn debugging_a_candidate_does_not_reveal_its_token() {
        let found = Found {
            token: "private-session".into(),
            flavour: Flavour::Stable,
        };
        assert!(!format!("{found:?}").contains("private-session"));
    }

    // Real LevelDB files, written by the format implementation. Only synthetic
    // session strings enter these fixtures.
    fn database(compressed: bool, write: impl FnOnce(&mut rusty_leveldb::DB)) -> tempfile::TempDir {
        use rusty_leveldb::{DB, Options};
        use std::io::Read;
        let options = Options {
            compressor: u8::from(compressed),
            write_buffer_size: 4096,
            ..Options::default()
        };
        let env = options.env.clone();
        let mut db = DB::open("fixture", options).unwrap();
        write(&mut db);
        db.flush().unwrap();
        drop(db);
        let dir = tempfile::tempdir().unwrap();
        for name in env.children(std::path::Path::new("fixture")).unwrap() {
            let mut bytes = Vec::new();
            env.open_sequential_file(&std::path::Path::new("fixture").join(&name))
                .unwrap()
                .read_to_end(&mut bytes)
                .unwrap();
            fs::write(dir.path().join(name), bytes).unwrap();
        }
        dir
    }

    fn encoded(token: &str) -> Vec<u8> {
        let mut value = PREFIX.to_vec();
        value.extend_from_slice(STANDARD.encode(seal(&[3; 32], token)).as_bytes());
        value
    }

    #[test]
    fn reads_only_current_values_from_compressed_tables() {
        let dir = database(true, |db| {
            db.put(b"session", &encoded("old.token.value")).unwrap();
            db.put(b"deleted", &encoded("deleted.token.value")).unwrap();
            for n in 0..80 {
                db.put(format!("padding-{n:03}").as_bytes(), &[b'x'; 2048])
                    .unwrap();
            }
            db.put(b"session", &encoded("current.token.value")).unwrap();
            db.delete(b"deleted").unwrap();
        });
        assert!(
            fs::read_dir(dir.path()).unwrap().any(|e| e
                .unwrap()
                .path()
                .extension()
                .is_some_and(|e| e == "ldb"))
        );
        let before = profile_bytes(dir.path());
        let cipher = Aes256Gcm::new_from_slice(&[3; 32]).unwrap();
        assert_eq!(
            read_logins(dir.path(), &cipher).unwrap(),
            ["current.token.value"]
        );
        assert_eq!(
            profile_bytes(dir.path()),
            before,
            "source files are read only"
        );
    }

    fn profile_bytes(dir: &std::path::Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                let path = e.unwrap().path();
                (path.clone(), fs::read(path).unwrap())
            })
            .collect()
    }

    #[test]
    fn reconstructs_a_login_split_across_wal_blocks() {
        // The encrypted value crosses several 32 KiB physical records.
        let expected = "a".repeat(70_000);
        let dir = database(false, |db| {
            db.put(b"session", &encoded(&expected)).unwrap();
        });
        let cipher = Aes256Gcm::new_from_slice(&[3; 32]).unwrap();
        // The bounded candidate decoder rejects huge sessions. Use a big
        // preceding value to place a normal session across a block boundary.
        assert!(read_logins(dir.path(), &cipher).unwrap().is_empty());
        let dir = database(false, |db| {
            let mut batch = rusty_leveldb::WriteBatch::default();
            batch.put(b"padding", &vec![b'x'; 32_700]);
            batch.put(b"session", &encoded("split.token.value"));
            db.write(batch, true).unwrap();
        });
        let whole = encoded("split.token.value");
        let logs: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "log"))
            .map(|path| fs::read(path).unwrap())
            .collect();
        assert!(logs.iter().any(|log| log.len() > 32_768));
        assert!(
            logs.iter()
                .all(|log| !log.windows(whole.len()).any(|part| part == whole))
        );
        assert_eq!(
            read_logins(dir.path(), &cipher).unwrap(),
            ["split.token.value"]
        );
    }

    #[test]
    fn a_broken_flavour_does_not_discard_another_flavours_accounts() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("discord")).unwrap();
        fs::create_dir(dir.path().join("discordcanary")).unwrap();
        let result = discover_profiles(dir.path(), |profile| {
            if profile.ends_with("discord") {
                Ok(vec!["first.token.value".into()])
            } else {
                Err(anyhow!("unreadable fixture"))
            }
        });
        assert_eq!(result.accounts.len(), 1);
        assert_eq!(result.unreadable_profiles.len(), 1);
        assert_eq!(result.unreadable_profiles[0].cause, ReadFailure::Storage);
    }

    #[test]
    fn oversized_storage_files_are_rejected_before_reading() {
        let dir = tempfile::tempdir().unwrap();
        let file = fs::File::create(dir.path().join("000001.log")).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap();
        let error = snapshot(dir.path()).err().unwrap();
        assert_eq!(ReadFailure::from_error(&error), ReadFailure::Limit);
    }

    #[test]
    fn discovery_keeps_specific_causes_and_only_changes_suggest_closing_discord() {
        for cause in [
            ReadFailure::Changed,
            ReadFailure::Limit,
            ReadFailure::MissingState,
            ReadFailure::Key,
            ReadFailure::Storage,
        ] {
            let dir = tempfile::tempdir().unwrap();
            fs::create_dir(dir.path().join("discord")).unwrap();
            let result = discover_profiles(dir.path(), |_| {
                Err(anyhow!("fixture root cause").context(cause))
            });
            let failure = result.unreadable_profiles[0];
            assert_eq!(failure.cause, cause);
            assert_eq!(
                failure.message().contains("Close Discord"),
                cause == ReadFailure::Changed
            );
        }
        assert_eq!(
            ReadFailure::from_error(&anyhow!(ChangedDuringRead).context("copy")),
            ReadFailure::Changed
        );
    }

    #[test]
    fn missing_local_state_has_its_own_failure() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Local Storage/leveldb")).unwrap();
        let error = read_flavour(dir.path()).unwrap_err();
        assert_eq!(ReadFailure::from_error(&error), ReadFailure::MissingState);
    }

    #[test]
    fn sst_files_are_not_copied_for_a_reader_that_only_opens_ldb() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("000001.sst"), b"unused").unwrap();
        fs::write(dir.path().join("000002.ldb"), b"used").unwrap();
        let files = storage_files(dir.path()).unwrap();
        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            [&PathBuf::from("000002.ldb")]
        );
    }

    #[test]
    fn a_changing_copy_retries_twice_before_succeeding() {
        let mut attempts = 0;
        let mut delays = Vec::new();
        let value = retry_copy(
            || {
                attempts += 1;
                if attempts < 3 {
                    Err(ChangedDuringRead.into())
                } else {
                    Ok(42)
                }
            },
            |delay| delays.push(delay),
        )
        .unwrap();
        assert_eq!((value, attempts), (42, 3));
        assert_eq!(
            delays,
            [
                std::time::Duration::from_millis(25),
                std::time::Duration::from_millis(50)
            ]
        );
    }

    #[test]
    fn a_persistently_changing_copy_stops_after_three_attempts() {
        let mut attempts = 0;
        let result: Result<()> = retry_copy(
            || {
                attempts += 1;
                Err(ChangedDuringRead.into())
            },
            |_| {},
        );
        assert!(result.unwrap_err().is::<ChangedDuringRead>());
        assert_eq!(attempts, 3);
    }

    #[test]
    fn copy_retries_a_compacted_away_file_but_not_a_permanent_error() {
        let mut attempts = 0;
        retry_copy(
            || {
                attempts += 1;
                if attempts == 1 {
                    Err(std::io::Error::from(std::io::ErrorKind::NotFound).into())
                } else {
                    Ok(())
                }
            },
            |_| {},
        )
        .unwrap();
        assert_eq!(attempts, 2);
        let mut delays = 0;
        let result: Result<()> = retry_copy(|| Err(anyhow!("size limit")), |_| delays += 1);
        assert!(result.is_err());
        assert_eq!(delays, 0);
    }

    #[test]
    fn copying_a_file_streams_it_in_small_chunks_and_detects_a_changed_length() {
        #[derive(Default)]
        struct Count {
            total: usize,
            largest: usize,
        }
        impl Write for Count {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.total += bytes.len();
                self.largest = self.largest.max(bytes.len());
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("000001.log");
        let file = fs::File::create(&path).unwrap();
        file.set_len(4 << 20).unwrap();
        let metadata = file.metadata().unwrap();
        let stamp = (metadata.len(), metadata.modified().unwrap());
        let mut count = Count::default();
        copy_storage_file(&path, stamp, &mut count).unwrap();
        assert_eq!(count.total, 4 << 20);
        assert!(count.largest <= 64 << 10);
        println!(
            "copied {} bytes; largest write {} bytes",
            count.total, count.largest
        );
        file.set_len(metadata.len() + 1).unwrap();
        let error = copy_storage_file(&path, stamp, &mut count).unwrap_err();
        assert!(error.is::<ChangedDuringRead>());
    }

    #[test]
    fn aggregate_storage_limit_is_enforced_before_copying() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..3 {
            let file = fs::File::create(dir.path().join(format!("{n:06}.log"))).unwrap();
            file.set_len(8 << 20).unwrap();
        }
        assert!(snapshot(dir.path()).is_err());
    }

    #[test]
    fn profile_failures_log_the_error_chain_without_logging_candidates() {
        use std::sync::{Arc, Mutex};
        let logs = Arc::new(Mutex::new(Vec::<u8>::new()));
        struct LogWriter(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for LogWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let sink = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || LogWriter(sink.clone()))
            .finish();
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("discord")).unwrap();
        fs::create_dir(dir.path().join("discordcanary")).unwrap();
        tracing::subscriber::with_default(subscriber, || {
            discover_profiles(dir.path(), |profile| {
                if profile.ends_with("discord") {
                    Ok(vec!["private-session".into()])
                } else {
                    Err(anyhow!("profile failure").context("profile context"))
                }
            });
        });
        let text = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
        assert!(text.contains("profile context") && text.contains("profile failure"));
        assert!(!text.contains("private-session"));
    }

    #[test]
    fn unsupported_off_windows_is_the_only_reason_discover_errs_there() {
        if !supported() {
            assert!(discover().unwrap_err().is::<Unsupported>());
        }
    }
}
