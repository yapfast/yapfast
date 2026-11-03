//! Discord's soundboard in a call: which sounds this account may play,
//! playing one for everyone, hearing it first, its favourites, its join
//! sound, and hearing the sounds others play. Discord only passes on who
//! played what; each client downloads the sound and mixes it into its own
//! speakers, which [`crate::discord_audio::Hearing`] does here, so echo
//! cancellation hears it too.

use std::{
    collections::{HashMap, VecDeque},
    io::Cursor,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result, bail};
use yapfast_discord::{
    ready::{self, PRIVATE, permission},
    service::{Discord, Live, Snapshot},
    soundboard::{DEFAULT_VOLUME, JoinSound, MOST_FAVORITES, Sound},
};

use crate::{
    discord_audio::Hearing,
    video::Video,
    video_player::Resampler,
    voice::{SoundChoice, SoundGroup},
};

/// Discord takes sounds up to 5.2 s; anything longer is cut there and a
/// little after, so a file that claims more can't fill the memory.
const LONGEST_SECONDS: usize = 6;
/// What the downloaded files kept for playing again may take: a few dozen
/// typical sounds.
const KEPT_FILES: usize = 2 << 20;

/// How long Discord gets to send every server's sounds for [`Catalog`].
const CATALOG_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Every sound this account may choose as a join sound, which needs Nitro:
/// Discord's own and every server's, outside any call.
pub struct Catalog {
    defaults: Arc<[Sound]>,
    servers: HashMap<String, Vec<Sound>>,
    favorites: Vec<String>,
}

impl Catalog {
    /// Asks Discord for the sounds, waiting a few seconds at most for the
    /// servers' to come.
    pub async fn load(discord: &Discord) -> Result<Self> {
        let mut live = discord.live();
        let guilds: Vec<String> = discord
            .snapshots()
            .borrow()
            .servers
            .iter()
            .map(|server| server.id.clone())
            .collect();
        discord.request_soundboard(guilds.clone());
        let (defaults, favorites) =
            tokio::try_join!(discord.default_sounds(), discord.favorite_sounds())?;
        let mut servers = HashMap::new();
        let _ = tokio::time::timeout(CATALOG_WAIT, async {
            while servers.len() < guilds.len() {
                match live.recv().await {
                    Ok(Live::SoundboardSounds { guild, sounds }) => {
                        servers.insert(guild, sounds);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Ok(_) | Err(_) => {}
                }
            }
        })
        .await;
        Ok(Self {
            defaults,
            servers,
            favorites,
        })
    }

    /// The sounds in groups, as the call's soundboard shows them.
    pub fn groups(&self, snapshot: &Snapshot) -> Vec<SoundGroup> {
        grouped(
            snapshot,
            &self.defaults,
            &self.servers,
            &self.favorites,
            "",
            |sound| sound.available,
        )
    }
}

/// The soundboard of one call, from joining until leaving.
pub(crate) struct Soundboard {
    /// Discord's own sounds, once they came.
    defaults: Arc<[Sound]>,
    /// The sounds of each server asked for, by server.
    servers: HashMap<String, Vec<Sound>>,
    /// The account's favourite sounds, in order.
    favorites: Vec<String>,
    /// What may be played as of the last [`Self::groups`]; none until it
    /// runs, and again once the sounds change.
    shown: Option<Option<Allowed>>,
    files: Arc<Mutex<Files>>,
}

/// Which sounds this account may play in the call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Allowed {
    /// Its servers' sounds, as with Nitro, rather than only the call's.
    everywhere: bool,
}

impl Soundboard {
    /// Asks for the sounds this account may play in a call in `guild`, or
    /// with [`PRIVATE`] a DM's: Discord's own, the server's, and with Nitro
    /// every server's, which Discord's client asks for as a call connects.
    pub(crate) fn start(discord: &Discord, guild: &str) -> Self {
        let snapshot = discord.snapshots().borrow().clone();
        let guilds = if nitro(&snapshot) {
            snapshot
                .servers
                .iter()
                .map(|server| server.id.clone())
                .collect()
        } else if guild != PRIVATE {
            vec![guild.to_owned()]
        } else {
            vec![]
        };
        discord.request_soundboard(guilds);
        Self {
            defaults: Arc::from([]),
            servers: HashMap::new(),
            favorites: Vec::new(),
            shown: None,
            files: Arc::default(),
        }
    }

    pub(crate) fn set_defaults(&mut self, sounds: Arc<[Sound]>) {
        self.defaults = sounds;
        self.shown = None;
    }

    pub(crate) fn set_favorites(&mut self, favorites: Vec<String>) {
        if favorites != self.favorites {
            self.favorites = favorites;
            self.shown = None;
        }
    }

    /// Adds the sound `id` to the favourites, last, or takes it out: the
    /// favourites to keep, if they changed. Discord keeps at most
    /// [`MOST_FAVORITES`].
    pub(crate) fn favorite(&mut self, id: &str, favorite: bool) -> Option<Vec<String>> {
        let had = self.favorites.iter().any(|kept| kept == id);
        if favorite == had || (favorite && self.favorites.len() >= MOST_FAVORITES) {
            return None;
        }
        if favorite {
            self.favorites.push(id.to_owned());
        } else {
            self.favorites.retain(|kept| kept != id);
        }
        self.shown = None;
        Some(self.favorites.clone())
    }

    /// A server's sounds, all of them, as Discord sent them.
    pub(crate) fn set_server(&mut self, guild: String, sounds: Vec<Sound>) {
        self.servers.insert(guild, sounds);
        self.shown = None;
    }

    /// A server's sound that was added or changed; one of a server not
    /// asked for is left out.
    pub(crate) fn changed(&mut self, sound: Sound) {
        let Some(sounds) = sound
            .guild
            .as_ref()
            .and_then(|guild| self.servers.get_mut(guild))
        else {
            return;
        };
        match sounds.iter_mut().find(|kept| kept.id == sound.id) {
            Some(kept) => *kept = sound,
            None => sounds.push(sound),
        }
        self.shown = None;
    }

    pub(crate) fn deleted(&mut self, guild: &str, id: &str) {
        if let Some(sounds) = self.servers.get_mut(guild) {
            sounds.retain(|sound| sound.id != id);
            self.shown = None;
        }
    }

    /// The sound with this id, if this account may play it in the call.
    pub(crate) fn find(
        &self,
        snapshot: &Snapshot,
        me: &str,
        guild: &str,
        channel: &str,
        id: &str,
    ) -> Option<Sound> {
        let allowed = allowed(snapshot, me, guild, channel)?;
        self.all()
            .find(|sound| sound.id == id && playable(sound, guild, allowed))
            .cloned()
    }

    fn all(&self) -> impl Iterator<Item = &Sound> {
        self.defaults.iter().chain(self.servers.values().flatten())
    }

    /// The sounds to offer in the call, when they changed since last asked,
    /// or what may be played did: none where the call has no soundboard.
    pub(crate) fn groups(
        &mut self,
        snapshot: &Snapshot,
        me: &str,
        guild: &str,
        channel: &str,
    ) -> Option<Option<Arc<[SoundGroup]>>> {
        let allowed = allowed(snapshot, me, guild, channel);
        if self.shown == Some(allowed) {
            return None;
        }
        self.shown = Some(allowed);
        let Some(allowed) = allowed else {
            return Some(None);
        };
        let groups = grouped(
            snapshot,
            &self.defaults,
            &self.servers,
            &self.favorites,
            guild,
            |sound| playable(sound, guild, allowed),
        );
        Some(Some(groups.into()))
    }

    /// Plays the sound `id` into the call at its own `volume`, downloading
    /// it unless this call played it already.
    pub(crate) fn play(&self, discord: &Discord, hearing: &Arc<Hearing>, id: &str, volume: f32) {
        let (discord, hearing, files, id) = (
            discord.clone(),
            hearing.clone(),
            self.files.clone(),
            id.to_owned(),
        );
        tokio::spawn(async move {
            let played = async {
                let kept = files.lock().unwrap().get(&id);
                let data = match kept {
                    Some(data) => data,
                    None => {
                        let data: Arc<[u8]> = discord.sound_file(&id).await?.into();
                        files.lock().unwrap().keep(&id, data.clone());
                        data
                    }
                };
                let samples = tokio::task::spawn_blocking(move || decode(&data)).await??;
                hearing.play(samples, volume);
                anyhow::Ok(())
            };
            if let Err(error) = played.await {
                tracing::warn!(
                    error = format!("{error:#}"),
                    sound = id,
                    "could not play a soundboard sound"
                );
            }
        });
    }
}

/// The sounds `fits` lets through, in groups as Discord's picker lists them:
/// the favourites, in their order, then the server `first`'s sounds, the
/// other servers' in the account's order, and Discord's own.
pub fn grouped(
    snapshot: &Snapshot,
    defaults: &[Sound],
    servers: &HashMap<String, Vec<Sound>>,
    favorites: &[String],
    first: &str,
    fits: impl Fn(&Sound) -> bool,
) -> Vec<SoundGroup> {
    let all = || defaults.iter().chain(servers.values().flatten());
    let choices = |sounds: &mut dyn Iterator<Item = &Sound>| -> Vec<SoundChoice> {
        sounds
            .filter(|sound| fits(sound))
            .map(|sound| choice(sound, favorites.contains(&sound.id)))
            .collect()
    };
    let mut groups = Vec::new();
    let favorite = choices(
        &mut favorites
            .iter()
            .filter_map(|id| all().find(|sound| &sound.id == id)),
    );
    if !favorite.is_empty() {
        groups.push(SoundGroup {
            name: "Favorites".into(),
            sounds: favorite,
        });
    }
    let mut ordered: Vec<&_> = snapshot.servers.iter().collect();
    ordered.sort_by_key(|server| server.id != first);
    groups.extend(ordered.into_iter().filter_map(|server| {
        let sounds = choices(&mut servers.get(&server.id)?.iter());
        (!sounds.is_empty()).then(|| SoundGroup {
            name: server.name.clone(),
            sounds,
        })
    }));
    let defaults = choices(&mut defaults.iter());
    if !defaults.is_empty() {
        groups.push(SoundGroup {
            name: "Default".into(),
            sounds: defaults,
        });
    }
    groups
}

/// The gain the account's soundboard volume plays sounds at, on top of
/// their own; Discord keeps it already on its slider's curve.
pub(crate) fn gain(snapshot: &Snapshot) -> f32 {
    (snapshot.soundboard_volume.unwrap_or(DEFAULT_VOLUME) / 100.0).clamp(0.0, 1.0)
}

/// The join sound the account's settings give a call in `guild`: the
/// server's own, else the one for every server, which a DM's call plays.
pub fn join_sound<'a>(snapshot: &'a Snapshot, guild: &str) -> Option<&'a JoinSound> {
    let server = (guild != PRIVATE)
        .then(|| {
            snapshot
                .join_sounds
                .iter()
                .find(|join| join.server.as_deref() == Some(guild))
        })
        .flatten();
    server.or_else(|| {
        snapshot
            .join_sounds
            .iter()
            .find(|join| join.server.is_none())
    })
}

/// Whether this account has Nitro, which plays any of its servers' sounds.
pub(crate) fn nitro(snapshot: &Snapshot) -> bool {
    snapshot
        .account
        .as_ref()
        .is_some_and(|account| account.premium_type > 0)
}

/// Which sounds this account may play in `channel`, or none where it may
/// play none: in a server's voice channel it needs Speak and Use Soundboard,
/// as Discord's client checks; a DM's call always has one.
fn allowed(snapshot: &Snapshot, me: &str, guild: &str, channel: &str) -> Option<Allowed> {
    let nitro = nitro(snapshot);
    if guild == PRIVATE {
        return Some(Allowed { everywhere: nitro });
    }
    let server = snapshot.servers.iter().find(|server| server.id == guild)?;
    let found = server
        .voice_channels
        .iter()
        .find(|found| found.id == channel)?;
    let permissions = server.channel_permissions(found, me, &server.marks.roles);
    let needed = permission::SPEAK | permission::USE_SOUNDBOARD;
    (permissions & needed == needed).then_some(Allowed {
        everywhere: nitro && permissions & permission::USE_EXTERNAL_SOUNDS != 0,
    })
}

/// Whether `sound` may play in a call in `guild`: Discord's own anywhere,
/// a server's in its own voice channels, and with Nitro anywhere; never
/// while the server lacks the boosts its slot needs.
fn playable(sound: &Sound, guild: &str, allowed: Allowed) -> bool {
    sound.available
        && match &sound.guild {
            None => true,
            Some(own) => own == guild || allowed.everywhere,
        }
}

fn choice(sound: &Sound, favorite: bool) -> SoundChoice {
    let picture = sound
        .emoji_id
        .as_deref()
        .map(|id| ready::custom_emoji(id, false, 48));
    SoundChoice {
        id: sound.id.clone(),
        name: sound.name.clone(),
        // A custom emoji's name isn't a glyph.
        emoji: match picture {
            Some(_) => String::new(),
            None => sound.emoji_name.clone().unwrap_or_default(),
        },
        picture,
        favorite,
        guild: sound.guild.clone(),
    }
}

/// The files this call downloaded, the latest last.
#[derive(Default)]
struct Files {
    kept: VecDeque<(String, Arc<[u8]>)>,
    size: usize,
}

impl Files {
    fn get(&mut self, id: &str) -> Option<Arc<[u8]>> {
        let at = self.kept.iter().position(|(kept, _)| kept == id)?;
        let found = self.kept.remove(at)?;
        let data = found.1.clone();
        self.kept.push_back(found);
        Some(data)
    }

    fn keep(&mut self, id: &str, data: Arc<[u8]>) {
        if self.kept.iter().any(|(kept, _)| kept == id) {
            return;
        }
        self.size += data.len();
        self.kept.push_back((id.to_owned(), data));
        while self.size > KEPT_FILES
            && let Some((_, gone)) = self.kept.pop_front()
        {
            self.size -= gone.len();
        }
    }
}

/// A sound's file, MP3 or Ogg, as 48 kHz interleaved stereo.
pub(crate) fn decode(data: &[u8]) -> Result<Vec<i16>> {
    let mut file = Video::open_sound(Cursor::new(data)).context("not a sound Yapfast can play")?;
    let (rate, channels) = file.sound.context("the file has no sound")?;
    let mut resampler = Resampler::new(rate, channels);
    let longest = LONGEST_SECONDS * crate::audio_io::RATE as usize * 2;
    let mut samples = Vec::new();
    while samples.len() < longest && file.decode_sound()?.is_some() {
        let sound = file.sound();
        resampler.follow(sound.rate, sound.channels);
        samples.extend(
            resampler
                .run(sound.samples)
                .into_iter()
                .map(|sample| (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16),
        );
    }
    if samples.is_empty() {
        bail!("the sound is empty");
    }
    samples.truncate(longest);
    samples.shrink_to_fit();
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../testing/fixtures")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn decodes_mp3_and_ogg_to_48_khz_stereo() {
        // A 44.1 kHz mono MP3 and a 48 kHz mono Ogg Opus file, as Discord's
        // sounds come.
        for (name, seconds) in [("soundboard-tone.mp3", 0.5), ("voice-tone.ogg", 2.0)] {
            let samples = decode(&fixture(name)).unwrap();
            let length = samples.len() as f32 / 2.0 / crate::audio_io::RATE as f32;
            assert!((length - seconds).abs() < 0.1, "{name} lasts {length} s");
            let loudest = samples
                .iter()
                .map(|sample| sample.unsigned_abs())
                .max()
                .unwrap();
            assert!(loudest > 3000, "{name} peaks at {loudest}");
        }
        assert!(decode(b"not a sound").is_err());
    }

    #[test]
    fn keeps_the_latest_files_up_to_their_size() {
        let mut files = Files::default();
        let half: Arc<[u8]> = vec![0; KEPT_FILES / 2].into();
        files.keep("1", half.clone());
        files.keep("2", half.clone());
        // Played again, the first is the latest, and the second goes.
        assert!(files.get("1").is_some());
        files.keep("3", half);
        assert!(files.get("2").is_none());
        assert!(files.get("1").is_some() && files.get("3").is_some());
    }

    #[test]
    fn plays_discords_sounds_anywhere_and_a_servers_in_it() {
        let sound = |guild: Option<&str>, available| Sound {
            id: "1".into(),
            name: "quack".into(),
            guild: guild.map(str::to_owned),
            volume: 1.0,
            emoji_id: None,
            emoji_name: None,
            available,
        };
        let without = Allowed { everywhere: false };
        let with = Allowed { everywhere: true };
        assert!(playable(&sound(None, true), PRIVATE, without));
        assert!(playable(&sound(Some("10"), true), "10", without));
        assert!(!playable(&sound(Some("10"), true), "11", without));
        assert!(playable(&sound(Some("10"), true), "11", with));
        assert!(!playable(&sound(Some("10"), false), "10", with));
    }
}
