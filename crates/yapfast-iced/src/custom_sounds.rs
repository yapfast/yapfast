//! The user's own sound files for Yapfast's sounds and call tones, kept on
//! this device decoded, as 48 kHz interleaved stereo PCM.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use yapfast_core::{
    ring::Tone,
    sounds::{RATE, Sound},
    themes::media::{self, SoundSlot},
};

pub(crate) type CustomSounds = BTreeMap<SoundSlot, Arc<[i16]>>;

/// What a sound plays while the custom set is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    /// What it plays without the custom set: the theme's sound, the set's
    /// sound or the call tone.
    #[default]
    Default,
    /// The file uploaded for it.
    File,
}

pub(crate) fn ring_slot(tone: Tone) -> SoundSlot {
    match tone {
        Tone::Incoming => SoundSlot::Ring,
        Tone::Outgoing => SoundSlot::Ringback,
    }
}

fn path(root: &Path, slot: SoundSlot) -> PathBuf {
    root.join("custom-sounds")
        .join(format!("{}.pcm", slot.name()))
}

pub(crate) fn decode(slot: SoundSlot, file: &Path) -> Result<Arc<[i16]>, String> {
    let bytes = read(file, media::MAX_SOUND_BYTES)
        .map_err(|error| format!("Couldn't read the selected file. {error}"))?;
    let samples =
        media::decode_sound(file, &bytes).map_err(|error| format!("The selected file {error}."))?;
    let longest = slot.longest();
    if media::seconds(&samples) > longest {
        return Err(format!("Use a sound no longer than {longest} seconds."));
    }
    Ok(samples.into())
}

pub(crate) fn load(root: &Path) -> CustomSounds {
    let mut sounds = CustomSounds::new();
    let slots = Sound::ALL
        .map(SoundSlot::of_sound)
        .into_iter()
        .chain(Tone::ALL.map(ring_slot));
    for slot in slots {
        let max_bytes = (slot.longest() * RATE as f32) as u64 * 4;
        match read(&path(root, slot), max_bytes) {
            Ok(bytes) if !bytes.is_empty() && bytes.len().is_multiple_of(4) => {
                let (pairs, _) = bytes.as_chunks::<2>();
                let samples = pairs.iter().copied().map(i16::from_le_bytes).collect();
                sounds.insert(slot, samples);
            }
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                tracing::warn!(?slot, %error, "could not load a custom sound");
            }
            Ok(_) | Err(_) => {}
        }
    }
    sounds
}

pub(crate) fn save(root: &Path, slot: SoundSlot, samples: &[i16]) -> io::Result<()> {
    let path = path(root, slot);
    let folder = path.parent().expect("a custom sound has a folder");
    fs::create_dir_all(folder)?;
    let mut file = tempfile::NamedTempFile::new_in(folder)?;
    {
        let mut writer = BufWriter::new(file.as_file_mut());
        for sample in samples {
            writer.write_all(&sample.to_le_bytes())?;
        }
        writer.flush()?;
    }
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub(crate) fn remove(root: &Path, slot: SoundSlot) -> io::Result<()> {
    match fs::remove_file(path(root, slot)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn read(path: &Path, max: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The file is too large.",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_sounds_survive_reopen_and_can_be_replaced_or_removed() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testing/fixtures/soundboard-tone.mp3");
        let imported = decode(SoundSlot::Ring, &fixture).unwrap();
        save(root, SoundSlot::Message, &[100, -100]).unwrap();
        save(root, SoundSlot::Ring, &imported).unwrap();
        save(root, SoundSlot::Ringback, &[200, -200]).unwrap();
        save(root, SoundSlot::Message, &[300, -300]).unwrap();
        let sounds = load(root);
        assert_eq!(sounds[&SoundSlot::Message].as_ref(), &[300, -300]);
        assert_eq!(sounds[&SoundSlot::Ring], imported);
        assert_eq!(sounds[&SoundSlot::Ringback].as_ref(), &[200, -200]);

        remove(root, SoundSlot::Ring).unwrap();
        remove(root, SoundSlot::Ring).unwrap();
        assert!(!load(root).contains_key(&SoundSlot::Ring));
        assert!(load(root).contains_key(&SoundSlot::Ringback));
    }
}
