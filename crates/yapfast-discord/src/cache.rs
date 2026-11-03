//! Discord as Yapfast last saw it, on disk, so the next start shows it at once
//! while Yapfast connects, and Discord's answers replace it as they come.
//!
//! - `discord-account.json`: who the account is, its servers and their
//!   channels, its DMs, friends and status; the names Yapfast learned, which
//!   messages and member lists show people by; and the member lists shown
//!   lately. Calls and who is in voice are left out, since they're over by
//!   then. Written at most every [`SAVE_DELAY`] while connected.
//! - `discord-messages/`: each channel shown lately, its newest
//!   [`CHANNEL_MESSAGES`] messages, for the [`CHANNELS`] used last.
//! - `discord-profiles/`: each profile shown lately, outside servers or in
//!   one, as Discord last gave it, for the [`PROFILES`] shown last. A card
//!   shows it at once while Discord's answer comes.
//! - `discord-pictures/`: avatars, banners, server icons, emoji and
//!   stickers. Discord gives each version of a picture its own address, so
//!   a kept one never goes stale. The least lately shown go past
//!   [`PICTURES_SIZE`].
//! - `discord-previews/`: the media proxy's previews of attachments and
//!   links. The least lately shown go past [`PREVIEWS_SIZE`].
//! - `discord-videos/`: the video playing, while it does; see [`videos`].
//! - `discord-build-number`: the web client's build number as its page last
//!   gave it, so a start connects with it at once; see
//!   [`crate::api::Api::learn_build_number`].
//! - `discord-favorite-gifs.json`: the favourite GIFs, as Discord's settings
//!   last gave them, so the picker shows them before Discord answers.
//! - `discord-gifs/`: a thumbnail of each favourite GIF shown, by its
//!   picture's address. The least lately shown go past [`GIFS_SIZE`].
//!
//! The files name the login they belong to by a hash of the token, so another
//! account linked meanwhile starts empty, and unlinking deletes them all.
//! They're readable only by the user. The account, messages, profiles,
//! favourite GIFs and previews are encrypted, with a key kept beside the
//! token in the OS credential store (see [`sealed`]), so a backup or a disk
//! read without it shows none of them. The pictures, public on Discord's
//! CDN, aren't, nor the video playing, gone once it stops.

mod sealed;

use std::{
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use tokio::sync::watch;

use crate::{
    member_list::MemberList,
    message::{Message, Names, SavedNames},
    profile::Profile,
    service::{Snapshot, Status},
};

/// Changes within this long go to disk together.
const SAVE_DELAY: Duration = Duration::from_secs(20);
/// Where on Discord's CDN the pictures Yapfast keeps live.
const PICTURE_PATHS: [&str; 9] = [
    "/app-assets/",
    "/app-icons/",
    "/avatars/",
    "/banners/",
    "/embed/avatars/",
    "/guilds/",
    "/icons/",
    "/emojis/",
    "/stickers/",
];
/// The most the kept pictures take; the least lately shown go first.
const PICTURES_SIZE: u64 = 64 * 1024 * 1024;
/// The most the kept previews take; the least lately shown go first.
const PREVIEWS_SIZE: u64 = 192 * 1024 * 1024;
/// The most the favourite GIFs' thumbnails take; the least lately shown go
/// first. A thumbnail takes 14-300 KB, 40 KB at the median.
const GIFS_SIZE: u64 = 50 * 1024 * 1024;
/// Downloads larger than this aren't kept, such as a file being saved.
const LARGEST_PICTURE: usize = 8 * 1024 * 1024;
/// The most channels whose messages are kept; the least lately used go.
const CHANNELS: usize = 300;
/// Of each channel, the newest messages kept.
pub const CHANNEL_MESSAGES: usize = 100;
/// The most profiles kept; the least lately shown go. One takes 1-5 KB.
const PROFILES: usize = 500;

pub(crate) use sealed::Key;
pub(crate) use sealed::{new_key, read_key, write_key};

static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);
/// The login whose files are read and written: a hash of its token.
static LOGIN: Mutex<Option<String>> = Mutex::new(None);
/// The key they're encrypted with, while the credential store gives one.
static KEY: Mutex<Option<Key>> = Mutex::new(None);
/// The member lists to save with the account, as the service last kept them.
static LISTS: Mutex<Vec<MemberList>> = Mutex::new(Vec::new());

#[derive(Deserialize)]
struct Saved {
    snapshot: Arc<Snapshot>,
    #[serde(default)]
    names: SavedNames,
    #[serde(default)]
    lists: Vec<MemberList>,
}

/// [`Saved`] as it's written, from what the service holds rather than
/// copies of it.
#[derive(Serialize)]
struct Saving<'a> {
    snapshot: &'a Snapshot,
    names: LockedNames<'a>,
    lists: &'a [MemberList],
}

/// The names, read only while they're written, so learning names waits
/// for them alone rather than the whole account.
struct LockedNames<'a>(&'a RwLock<Names>);

impl Serialize for LockedNames<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let names = self
            .0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        names.saving().serialize(serializer)
    }
}

/// What the account file held for this login.
#[derive(Default)]
pub(crate) struct Loaded {
    pub snapshot: Snapshot,
    pub names: Names,
    pub lists: Vec<MemberList>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Keeps the account in `file` from now on, and the rest beside it.
pub fn keep_in(file: PathBuf) {
    *lock(&FILE) = Some(file);
    if let (Some(pictures), Some(gifs), Some(messages), Some(profiles), Some(videos)) = (
        pictures(),
        gifs(),
        messages_directory(),
        profiles_directory(),
        videos(),
    ) {
        let text = [self::file(), favorite_gifs_file()];
        let previews = previews();
        // Pictures kept before the previews were apart, and encrypted,
        // have them among them: they go, out of the way first.
        let earlier = pictures.with_extension("earlier");
        if let Some(previews) = &previews
            && !previews.exists()
        {
            let _ = std::fs::rename(&pictures, &earlier);
            let _ = std::fs::create_dir_all(previews);
        }
        std::thread::spawn(move || {
            let _ = std::fs::remove_dir_all(&earlier);
            unsealed(text.into_iter().flatten(), [&messages, &profiles]);
            if let Some(previews) = &previews {
                prune(previews, PREVIEWS_SIZE, usize::MAX);
            }
            prune(&pictures, PICTURES_SIZE, usize::MAX);
            prune(&gifs, GIFS_SIZE, usize::MAX);
            prune(&messages, u64::MAX, CHANNELS);
            prune(&profiles, u64::MAX, PROFILES);
            // Videos played before Yapfast last closed.
            prune(&videos, 0, 0);
        });
    }
}

/// Deletes `files`, and those in `directories`, that Yapfast kept before it
/// encrypted them.
fn unsealed(files: impl Iterator<Item = PathBuf>, directories: [&Path; 2]) {
    let kept = directories
        .into_iter()
        .filter_map(|directory| std::fs::read_dir(directory).ok())
        .flatten()
        .filter_map(|entry| Some(entry.ok()?.path()))
        // Not one being written, which ends in `.partial`.
        .filter(|file| {
            file.extension()
                .is_some_and(|extension| extension == "json")
        });
    for file in files.chain(kept) {
        if file.is_file() && !sealed::is_sealed(&file) {
            let _ = std::fs::remove_file(&file);
        }
    }
}

fn file() -> Option<PathBuf> {
    lock(&FILE).clone()
}

/// Public, account-independent game catalogue, stored beside the account cache.
pub(crate) fn games_file() -> Option<PathBuf> {
    Some(file()?.with_file_name("discord-games.json"))
}

/// The web client's build number as its page last gave it, public and the
/// same for every account, kept beside the account cache. It reads the
/// disk, so the async side calls it off its threads.
pub(crate) fn build_number() -> Option<u64> {
    std::fs::read_to_string(build_number_file()?)
        .ok()?
        .trim()
        .parse()
        .ok()
        .filter(|&number| number > 0)
}

/// Keeps the build number for [`build_number`].
pub(crate) fn keep_build_number(number: u64) {
    let Some(file) = build_number_file() else {
        return;
    };
    if let Err(error) = std::fs::write(&file, format!("{number}\n")) {
        tracing::warn!(%error, "could not keep Discord's build number");
    }
}

fn build_number_file() -> Option<PathBuf> {
    Some(file()?.with_file_name("discord-build-number"))
}

fn pictures() -> Option<PathBuf> {
    Some(file()?.with_file_name("discord-pictures"))
}

fn previews() -> Option<PathBuf> {
    Some(file()?.with_file_name("discord-previews"))
}

fn favorite_gifs_file() -> Option<PathBuf> {
    Some(file()?.with_file_name("discord-favorite-gifs.json"))
}

/// The favourite GIFs kept for this login. It reads the disk, so the async
/// side calls it off its threads.
pub fn favorite_gifs() -> Option<Vec<crate::gifs::Favorite>> {
    // As Discord's settings carry them: see [`crate::gifs::write_favorites`].
    let settings: String = read_sealed(&favorite_gifs_file()?, "the saved favourite GIFs")?;
    crate::gifs::read_favorites(&settings)
}

/// Keeps the favourite GIFs for this login, as [`favorite_gifs`] reads them.
pub fn keep_favorite_gifs(favorites: &[crate::gifs::Favorite]) {
    let Some(file) = favorite_gifs_file() else {
        return;
    };
    if let Err(error) = write_sealed(&file, &crate::gifs::write_favorites(favorites)) {
        tracing::warn!(%error, "could not save the favourite GIFs");
    }
}

fn gifs() -> Option<PathBuf> {
    Some(file()?.with_file_name("discord-gifs"))
}

/// Where the thumbnail of the favourite GIF whose picture is at `url` is
/// kept, and where its picture downloads to first.
fn gif_file(url: &str) -> Option<PathBuf> {
    Some(gifs()?.join(hash(url)))
}

/// The thumbnail kept of the favourite GIF whose picture is at `url`.
pub fn kept_gif(url: &str) -> Option<PathBuf> {
    let file = gif_file(url).filter(|file| file.is_file())?;
    // Shown now, so pruning keeps it longer.
    touch(&file);
    Some(file)
}

/// A new file to download the picture at `url` into, before it becomes the
/// favourite's thumbnail or makes one.
pub fn gif_download(url: &str) -> Option<(PathBuf, std::fs::File)> {
    let file = gif_file(url)?.with_extension("download");
    std::fs::create_dir_all(file.parent()?).ok()?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let opened = options.open(&file).ok()?;
    Some((file, opened))
}

/// Keeps `thumbnail` as the favourite's at `url`: a file to move there, as a
/// download from [`gif_download`] is, or bytes to write.
pub fn keep_gif(url: &str, thumbnail: KeptGif<'_>) -> Option<PathBuf> {
    let file = gif_file(url)?;
    let kept = match thumbnail {
        KeptGif::File(from) => std::fs::rename(from, &file).map_err(anyhow::Error::from),
        KeptGif::Bytes(data) => write_bytes(&file, data),
    };
    match kept {
        Ok(()) => Some(file),
        Err(error) => {
            tracing::warn!(%error, "could not keep a favourite GIF's thumbnail");
            None
        }
    }
}

/// A favourite GIF's thumbnail to keep.
pub enum KeptGif<'a> {
    File(&'a Path),
    Bytes(&'a [u8]),
}

fn messages_directory() -> Option<PathBuf> {
    Some(file()?.with_file_name("discord-messages"))
}

fn profiles_directory() -> Option<PathBuf> {
    Some(file()?.with_file_name("discord-profiles"))
}

/// Where a video downloads while it plays, on disk rather than in memory,
/// as the temporary directory may be. The file goes when playing stops.
pub fn videos() -> Option<PathBuf> {
    Some(file()?.with_file_name("discord-videos"))
}

fn hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn current_login() -> Option<String> {
    lock(&LOGIN).clone()
}

fn current_key() -> Option<Key> {
    *lock(&KEY)
}

/// What was saved for this login, to show while connecting. From now on
/// the files are this login's, encrypted with `key`; without one, nothing is
/// read or kept.
pub(crate) fn load(token: &str, key: Option<Key>) -> Loaded {
    *lock(&LOGIN) = Some(hash(token));
    *lock(&KEY) = key;
    let Some(saved) =
        file().and_then(|file| read_sealed::<Saved>(&file, "the saved Discord account"))
    else {
        return Loaded::default();
    };
    let mut snapshot = Arc::unwrap_or_clone(saved.snapshot);
    snapshot.status = Status::Connecting;
    for conversation in &mut snapshot.conversations {
        Arc::make_mut(conversation).call = false;
    }
    Loaded {
        snapshot,
        names: Names::restore(saved.names),
        lists: saved.lists,
    }
}

/// Discord refreshed the login to `token`: the files the one before kept
/// are this one's from now on. Only the login's hash at the start of each
/// is written, in place, since the new one is as long. It writes to disk,
/// so the async side calls it off its threads.
pub(crate) fn relogin(token: &str) {
    let login = hash(token);
    let Some(earlier) = lock(&LOGIN).replace(login.clone()) else {
        return;
    };
    let kept = [messages_directory(), profiles_directory(), previews()]
        .into_iter()
        .flatten()
        .filter_map(|directory| std::fs::read_dir(directory).ok())
        .flatten()
        .filter_map(|entry| Some(entry.ok()?.path()));
    for file in [file(), favorite_gifs_file()]
        .into_iter()
        .flatten()
        .chain(kept)
    {
        if let Err(error) = sealed::relabel(&file, &earlier, &login)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "could not keep a Discord file for the refreshed login");
        }
    }
}

/// What `file` holds for this login, as it reads: none when it's missing,
/// another login's, or doesn't read with this key.
fn read_sealed<T: DeserializeOwned>(file: &Path, what: &str) -> Option<T> {
    let opening = match open_sealed(file) {
        Ok(opening) => opening,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::PermissionDenied {
                tracing::warn!(%error, "could not read {what}");
            }
            return None;
        }
    };
    match serde_json::from_reader(opening) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::warn!(%error, "could not read {what}");
            None
        }
    }
}

/// Opens `file`, encrypted for this login. Another login's, or any without
/// a key, is denied.
fn open_sealed(file: &Path) -> std::io::Result<sealed::Opening<std::fs::File>> {
    let denied = || std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    let (key, login) = (current_key(), current_login());
    let (Some(key), Some(login)) = (key, login) else {
        return Err(denied());
    };
    let (owner, opening) = sealed::open(file, &key)?;
    if owner != login {
        return Err(denied());
    }
    Ok(opening)
}

/// Writes `value` to `file` for this login, encrypted, as it goes. Without
/// a key it isn't kept.
fn write_sealed(file: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    write_sealed_with(file, |sealing| Ok(serde_json::to_writer(sealing, value)?))
}

/// Writes what `fill` puts in the writer to `file` as [`write_sealed`] does.
fn write_sealed_with(
    file: &Path,
    fill: impl FnOnce(
        &mut sealed::Sealing<&mut std::io::BufWriter<std::fs::File>>,
    ) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (Some(key), Some(login)) = (current_key(), current_login()) else {
        return Ok(());
    };
    write_with(file, |writer| {
        let mut sealing = sealed::seal(writer, &key, &login)?;
        fill(&mut sealing)?;
        sealing.finish()?;
        Ok(())
    })
}

/// The member lists to save with the account from now on.
pub(crate) fn keep_lists(lists: Vec<MemberList>) {
    *lock(&LISTS) = lists;
}

/// Saves the account while connected, a while after it changes, until the
/// service ends.
pub(crate) async fn keep(mut snapshots: watch::Receiver<Arc<Snapshot>>, names: Arc<RwLock<Names>>) {
    let mut ended = false;
    while !ended {
        ended = snapshots.changed().await.is_err();
        if !ended {
            tokio::time::sleep(SAVE_DELAY).await;
        }
        let snapshot = snapshots.borrow_and_update().clone();
        if snapshot.status != Status::Online {
            continue;
        }
        let Some(file) = file() else {
            continue;
        };
        let (names, lists) = (names.clone(), lock(&LISTS).clone());
        let written =
            tokio::task::spawn_blocking(move || save(&file, &snapshot, &names, &lists)).await;
        if let Ok(Err(error)) = written {
            tracing::warn!(%error, "could not save the Discord account");
        }
    }
}

/// Where a channel's messages are kept. Channel ids are numbers, which keeps
/// the name a plain file name.
fn channel_file(channel: &str) -> Option<PathBuf> {
    if channel.is_empty() || !channel.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(messages_directory()?.join(format!("{channel}.json")))
}

/// The messages kept of a channel for this login, oldest first.
pub fn messages(channel: &str) -> Vec<Message> {
    let Some(file) = channel_file(channel) else {
        return Vec::new();
    };
    let Some(messages) = read_sealed(&file, "a saved Discord channel") else {
        return Vec::new();
    };
    // Used now, so pruning keeps it longer.
    touch(&file);
    messages
}

/// Keeps a channel's newest [`CHANNEL_MESSAGES`] messages, oldest first. It
/// writes to disk, so the async side calls it off its threads.
pub fn save_messages(channel: &str, messages: Vec<Message>) {
    let Some(file) = channel_file(channel) else {
        return;
    };
    let newest = &messages[messages.len().saturating_sub(CHANNEL_MESSAGES)..];
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .map_err(anyhow::Error::from)
        .and_then(|()| write_sealed(&file, &newest));
    if let Err(error) = written {
        tracing::warn!(%error, "could not save a Discord channel");
    }
}

/// Where a user's profile in a server is kept, or with [`PRIVATE`] outside
/// any. Ids are numbers, which keeps the name a plain file name.
///
/// [`PRIVATE`]: crate::ready::PRIVATE
fn profile_file(guild: &str, user: &str) -> Option<PathBuf> {
    let number = |id: &str| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit());
    let name = match guild {
        crate::ready::PRIVATE if number(user) => format!("{user}.json"),
        _ if number(user) && number(guild) => format!("{user}-{guild}.json"),
        _ => return None,
    };
    Some(profiles_directory()?.join(name))
}

/// The profile kept of a user in a server, or outside any, for this login.
/// It reads the disk, so the async side calls it off its threads.
pub(crate) fn profile(guild: &str, user: &str) -> Option<Profile> {
    let file = profile_file(guild, user)?;
    let profile = read_sealed(&file, "a saved Discord profile")?;
    // Shown now, so pruning keeps it longer.
    touch(&file);
    Some(profile)
}

/// Keeps a user's profile as [`profile`] reads it. It writes to disk, so the
/// async side calls it off its threads.
pub(crate) fn save_profile(guild: &str, user: &str, profile: &Profile) {
    let Some(file) = profile_file(guild, user) else {
        return;
    };
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .map_err(anyhow::Error::from)
        .and_then(|()| write_sealed(&file, profile));
    if let Err(error) = written {
        tracing::warn!(%error, "could not save a Discord profile");
    }
}

/// Where the picture at `url` is kept, if it's one Yapfast keeps: plain
/// when it's public on Discord's CDN, else encrypted. The media proxy signs
/// its addresses for a while; the signature isn't part of the key, so a
/// picture signed again is found.
fn picture_file(url: &str) -> Option<KeptPicture> {
    let (host, path) = url.split_once("://")?.1.split_once('/')?;
    let path = format!("/{path}");
    // The test server stands in for the media proxy.
    let proxied = host == "media.discordapp.net"
        || host.starts_with("images-ext-") && host.ends_with(".discordapp.net")
        || crate::client::test_server().is_some_and(|server| url.starts_with(server));
    let public = PICTURE_PATHS.iter().any(|kept| path.starts_with(kept));
    let name = hash(&unsigned(url));
    if public {
        Some(KeptPicture::Plain(pictures()?.join(name)))
    } else if proxied {
        Some(KeptPicture::Sealed(previews()?.join(name)))
    } else {
        None
    }
}

/// A picture kept on disk: plain, or encrypted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeptPicture {
    Plain(PathBuf),
    Sealed(PathBuf),
}

impl KeptPicture {
    fn file(&self) -> &Path {
        match self {
            Self::Plain(file) | Self::Sealed(file) => file,
        }
    }

    /// How long the picture is, without its encryption.
    pub fn length(&self) -> Option<u64> {
        let size = std::fs::metadata(self.file()).ok()?.len();
        match self {
            Self::Plain(_) => Some(size),
            Self::Sealed(_) => sealed::length(size),
        }
    }

    /// The picture, read as it's needed, from wherever it seeks.
    pub fn open(&self) -> std::io::Result<KeptReader> {
        Ok(match self {
            Self::Plain(file) => KeptReader::Plain(std::fs::File::open(file)?),
            Self::Sealed(file) => KeptReader::Sealed(open_sealed(file)?),
        })
    }

    /// The whole picture.
    fn read(&self) -> std::io::Result<Vec<u8>> {
        match self {
            Self::Plain(file) => std::fs::read(file),
            Self::Sealed(file) => {
                let mut opening = open_sealed(file)?;
                let mut data = Vec::with_capacity(opening.length() as usize);
                opening.read_to_end(&mut data)?;
                Ok(data)
            }
        }
    }

    /// Keeps `data` as the picture. Without a key, an encrypted one isn't.
    fn write(&self, data: &[u8]) -> anyhow::Result<()> {
        if let Some(directory) = self.file().parent() {
            std::fs::create_dir_all(directory)?;
        }
        match self {
            Self::Plain(file) => write_bytes(file, data),
            Self::Sealed(file) => write_sealed_with(file, |sealing| Ok(sealing.write_all(data)?)),
        }
    }
}

/// A kept picture as it's read.
pub enum KeptReader {
    Plain(std::fs::File),
    Sealed(sealed::Opening<std::fs::File>),
}

impl KeptReader {
    /// How long the picture is, without its encryption.
    pub fn length(&self) -> Option<u64> {
        match self {
            Self::Plain(file) => file.metadata().ok().map(|kept| kept.len()),
            Self::Sealed(opening) => Some(opening.length()),
        }
    }
}

impl Read for KeptReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(file) => file.read(out),
            Self::Sealed(opening) => opening.read(out),
        }
    }
}

impl Seek for KeptReader {
    fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
        match self {
            Self::Plain(file) => file.seek(to),
            Self::Sealed(opening) => opening.seek(to),
        }
    }
}

/// `url` without the media proxy's signature: its `ex`, `is` and `hm`.
fn unsigned(url: &str) -> String {
    let Some((address, query)) = url.split_once('?') else {
        return url.to_owned();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            !["ex=", "is=", "hm="]
                .iter()
                .any(|signed| pair.starts_with(signed))
        })
        .collect();
    if kept.is_empty() {
        address.to_owned()
    } else {
        format!("{address}?{}", kept.join("&"))
    }
}

/// The picture at `url`, if it was kept.
pub(crate) async fn picture(url: &str) -> Option<Vec<u8>> {
    let kept = picture_file(url)?;
    tokio::task::spawn_blocking(move || {
        let data = kept.read().ok()?;
        // Shown lately, so pruning keeps it longer.
        touch(kept.file());
        Some(data)
    })
    .await
    .ok()?
}

/// The picture at `url` as it's kept, if it is.
pub fn kept_picture(url: &str) -> Option<KeptPicture> {
    picture_file(url).filter(|kept| kept.file().is_file())
}

/// Keeps the picture downloaded from `url`, if it's one Yapfast keeps, and
/// gives it back; none only when keeping it panicked.
pub(crate) async fn keep_picture(url: &str, data: Vec<u8>) -> Option<Vec<u8>> {
    let kept = picture_file(url).filter(|_| data.len() <= LARGEST_PICTURE);
    let Some(kept) = kept else {
        return Some(data);
    };
    let (data, written) = tokio::task::spawn_blocking(move || {
        let written = kept.write(&data);
        (data, written)
    })
    .await
    .ok()?;
    if let Err(error) = written {
        tracing::warn!(%error, "could not keep a Discord picture");
    }
    Some(data)
}

fn touch(file: &Path) {
    if let Ok(kept) = std::fs::File::options().write(true).open(file) {
        let _ = kept.set_modified(std::time::SystemTime::now());
    }
}

/// Deletes the least lately used files until those left take under three
/// quarters of `size` and number under three quarters of `count`, once
/// they pass either.
fn prune(directory: &Path, size: u64, count: usize) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut kept: Vec<_> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let metadata = entry.metadata().ok()?;
            Some((metadata.modified().ok()?, metadata.len(), entry.path()))
        })
        .collect();
    let mut total: u64 = kept.iter().map(|(_, length, _)| length).sum();
    let mut left = kept.len();
    if total <= size && left <= count {
        return;
    }
    kept.sort();
    for (_, length, path) in kept {
        if total <= size / 4 * 3 && left <= count.saturating_mul(3) / 4 {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total -= length;
            left -= 1;
        }
    }
}

/// Deletes everything kept, as when unlinking.
pub fn forget() {
    for file in [file(), favorite_gifs_file()].into_iter().flatten() {
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(%error, "could not delete the saved Discord account"),
        }
    }
    for directory in [
        pictures(),
        previews(),
        gifs(),
        messages_directory(),
        profiles_directory(),
        videos(),
    ]
    .into_iter()
    .flatten()
    {
        match std::fs::remove_dir_all(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(%error, "could not delete kept Discord files"),
        }
    }
    lock(&LISTS).clear();
    *lock(&KEY) = None;
}

/// Writes the account to `file` as it goes, rather than all at once: as
/// JSON in memory, with a copy of the names, saving took 7-11 MiB every 20 s
/// on an account of 3,000 people.
fn save(
    file: &Path,
    snapshot: &Snapshot,
    names: &RwLock<Names>,
    lists: &[MemberList],
) -> anyhow::Result<()> {
    let saving = Saving {
        snapshot,
        names: LockedNames(names),
        lists,
    };
    write_sealed(file, &saving)
}

/// Writes beside `file`, readable only by this user, then swaps it in, so
/// it's never seen half written.
fn write_bytes(file: &Path, data: &[u8]) -> anyhow::Result<()> {
    write_with(file, |writer| Ok(writer.write_all(data)?))
}

/// Writes what `fill` puts in the writer as [`write_bytes`] does.
fn write_with(
    file: &Path,
    fill: impl FnOnce(&mut std::io::BufWriter<std::fs::File>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let partial = file.with_extension("partial");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut writer = std::io::BufWriter::new(options.open(&partial)?);
    fill(&mut writer)?;
    writer.flush()?;
    drop(writer);
    std::fs::rename(&partial, file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        marked::{ChannelMarks, MarkedServer, ServerMarks},
        ready::{Channel, Server},
        service::{Account, Conversation},
    };

    const KEY: Key = [1; 32];

    // One test, since the files' place and the login are the process's.
    #[test]
    fn keeps_the_account_messages_and_pictures_for_their_login() {
        let directory =
            std::env::temp_dir().join(format!("yapfast-discord-cache-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        keep_in(directory.join("discord-account.json"));
        assert!(
            load("token", Some(KEY)).snapshot.servers.is_empty(),
            "nothing yet"
        );
        let snapshot = Snapshot {
            status: Status::Online,
            user_id: Some("1".into()),
            account: Some(Account {
                name: "Alice".into(),
                avatar: "a.png".into(),
                premium_type: 0,
                mfa_enabled: false,
                has_email: true,
            }),
            servers: vec![MarkedServer::marked(
                Server {
                    id: "10".into(),
                    name: "Server".into(),
                    text_channels: vec![Channel {
                        id: "11".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                ServerMarks {
                    muted: true,
                    ..Default::default()
                },
                |_| ChannelMarks {
                    unread: true,
                    mentions: 2,
                    ..Default::default()
                },
            )],
            conversations: vec![Arc::new(Conversation {
                id: "20".into(),
                call: true,
                ..Default::default()
            })],
            ..Default::default()
        };
        let mut names = Names::default();
        names.learn_member(
            "10",
            &json!({"user": {"id": "2", "global_name": "Bob"}, "nick": "Bobby", "roles": ["30"]}),
        );
        save(&file().unwrap(), &snapshot, &RwLock::new(names), &[]).unwrap();
        let shown = load("token", Some(KEY));
        assert_eq!(shown.snapshot.status, Status::Connecting);
        assert_eq!(shown.snapshot.account.unwrap().name, "Alice");
        assert_eq!(shown.snapshot.servers[0].name, "Server");
        // With what's this account's there, in the server's own object.
        assert_eq!(shown.snapshot.servers, snapshot.servers);
        assert!(
            !shown.snapshot.conversations[0].call,
            "calls are over by now"
        );
        assert_eq!(shown.names.name("10", "2"), Some("Bobby"));
        // Encrypted: not readable on disk, nor with another key.
        let written = std::fs::read(file().unwrap()).unwrap();
        assert!(!written.windows(5).any(|window| window == b"Alice"));
        assert!(load("token", Some([2; 32])).snapshot.servers.is_empty());
        assert!(load("token", None).snapshot.servers.is_empty());
        load("token", Some(KEY));

        // A channel's newest messages.
        let message = |id: u64| Message {
            id: id.to_string(),
            channel_id: "40".into(),
            content: format!("message {id}"),
            ..Default::default()
        };
        save_messages(
            "40",
            (1..=CHANNEL_MESSAGES as u64 + 5).map(message).collect(),
        );
        let kept = messages("40");
        assert_eq!(kept.len(), CHANNEL_MESSAGES);
        assert_eq!(kept[0].id, "6", "the oldest go");
        assert!(messages("../40").is_empty() && messages("41").is_empty());
        // A channel kept before Yapfast encrypted them goes.
        let earlier = messages_directory().unwrap().join("41.json");
        std::fs::write(&earlier, br#"{"login":"","messages":[]}"#).unwrap();
        unsealed(
            std::iter::empty(),
            [&messages_directory().unwrap(), &directory.join("none")],
        );
        assert!(!earlier.exists() && channel_file("40").unwrap().exists());

        // Profiles, in a server and outside any.
        let bob = |name: &str| Profile {
            user_id: "2".into(),
            name: name.into(),
            ..Default::default()
        };
        save_profile("10", "2", &bob("Bobby"));
        save_profile(crate::ready::PRIVATE, "2", &bob("Bob"));
        assert_eq!(profile("10", "2"), Some(bob("Bobby")));
        assert_eq!(profile(crate::ready::PRIVATE, "2"), Some(bob("Bob")));
        assert_eq!(profile("11", "2"), None);
        assert_eq!(profile("10", "../2"), None);

        // The favourite GIFs.
        assert_eq!(favorite_gifs(), None);
        let favorite = crate::gifs::Favorite {
            url: "https://klipy.com/gifs/cat-1".into(),
            src: "https://static.klipy.com/cat.webp".into(),
            width: 200,
            height: 100,
            order: 1,
            ..Default::default()
        };
        keep_favorite_gifs(std::slice::from_ref(&favorite));
        assert_eq!(favorite_gifs(), Some(vec![favorite]));
        // Their thumbnails, made from a download or written.
        let (gif, video) = (
            "https://media.tenor.com/a.gif",
            "https://media.tenor.com/b.mp4",
        );
        assert_eq!(kept_gif(gif), None);
        let (downloaded, mut file) = gif_download(gif).unwrap();
        std::io::Write::write_all(&mut file, b"gif").unwrap();
        drop(file);
        let kept = keep_gif(gif, KeptGif::File(&downloaded)).unwrap();
        assert!(!downloaded.exists());
        assert_eq!(kept_gif(gif), Some(kept.clone()));
        assert_eq!(std::fs::read(&kept).unwrap(), b"gif");
        keep_gif(video, KeptGif::Bytes(b"webm")).unwrap();
        assert_eq!(std::fs::read(kept_gif(video).unwrap()).unwrap(), b"webm");

        // Avatars and the media proxy's previews are kept, whatever their
        // signature; attachments' own files aren't.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime.block_on(async {
            let avatar = "https://cdn.discordapp.com/avatars/1/abc.png?size=128";
            let preview =
                "https://media.discordapp.net/attachments/2/3/a.png?ex=1&is=2&hm=3&width=400";
            let resigned =
                "https://media.discordapp.net/attachments/2/3/a.png?ex=7&is=8&hm=9&width=400";
            let original = "https://cdn.discordapp.com/attachments/2/3/a.png";
            keep_picture(avatar, b"avatar".to_vec()).await;
            keep_picture(preview, b"preview".to_vec()).await;
            keep_picture(original, b"original".to_vec()).await;
            assert_eq!(picture(avatar).await.as_deref(), Some(&b"avatar"[..]));
            assert_eq!(picture(resigned).await.as_deref(), Some(&b"preview"[..]));
            assert_eq!(picture(original).await, None);
            // The preview is encrypted; the avatar, public, isn't.
            let (Some(KeptPicture::Plain(avatar)), Some(kept)) =
                (kept_picture(avatar), kept_picture(preview))
            else {
                panic!("the avatar is plain");
            };
            assert_eq!(std::fs::read(avatar).unwrap(), b"avatar");
            assert!(matches!(kept, KeptPicture::Sealed(_)));
            assert!(
                !std::fs::read(kept.file())
                    .unwrap()
                    .windows(7)
                    .any(|window| window == b"preview")
            );
            assert_eq!(kept.length(), Some(7));
            let mut opened = kept.open().unwrap();
            opened.seek(std::io::SeekFrom::Start(3)).unwrap();
            let mut rest = String::new();
            opened.read_to_string(&mut rest).unwrap();
            assert_eq!(rest, "view");
        });

        // Discord refreshed the login: what's kept is the new one's.
        relogin("refreshed");
        assert_eq!(
            load("refreshed", Some(KEY)).snapshot.servers[0].name,
            "Server"
        );
        assert_eq!(messages("40").len(), CHANNEL_MESSAGES);
        assert_eq!(profile("10", "2"), Some(bob("Bobby")));
        assert!(favorite_gifs().is_some());
        assert!(load("token", Some(KEY)).snapshot.servers.is_empty());

        // Another login sees none of it.
        assert!(load("another", Some(KEY)).snapshot.servers.is_empty());
        assert!(messages("40").is_empty());
        assert_eq!(profile("10", "2"), None);
        assert_eq!(favorite_gifs(), None);
        load("token", Some(KEY));
        forget();
        assert!(load("token", Some(KEY)).snapshot.servers.is_empty());
        assert!(messages("40").is_empty());
        assert_eq!(favorite_gifs(), None);
        assert!(!pictures().unwrap().exists() && !previews().unwrap().exists());
        assert!(!profiles_directory().unwrap().exists());
        assert_eq!(kept_gif(gif), None);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn prunes_the_least_lately_used_files() {
        let directory =
            std::env::temp_dir().join(format!("yapfast-discord-pictures-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let now = std::time::SystemTime::now();
        for (index, name) in ["a", "b", "c", "d", "e"].iter().enumerate() {
            let file = directory.join(name);
            std::fs::write(&file, [0; 100]).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&file)
                .unwrap()
                .set_modified(now - Duration::from_secs(100 - index as u64 * 10))
                .unwrap();
        }
        let left = || {
            let mut left: Vec<_> = std::fs::read_dir(&directory)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect();
            left.sort();
            left
        };
        // Over 480 bytes, down to 360: the two oldest go.
        prune(&directory, 480, usize::MAX);
        assert_eq!(left(), ["c", "d", "e"]);
        // Over 2 files, down to 1.
        prune(&directory, u64::MAX, 2);
        assert_eq!(left(), ["e"]);
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
