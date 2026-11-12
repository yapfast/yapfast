//! A plugin's own sound, and its words through the OS's speech.
//!
//! Both are played the way Yapfast's own sounds are: the application asks, the
//! sound is decoded here and handed to a thread of its own, which opens the
//! speakers through [`crate::audio_io`] and writes it to the end. Nothing on
//! the interface thread waits, and a call that arrives while another sound
//! plays starts beside it rather than after it.
//!
//! The sound is whatever the plugin brings: a WAV file read here, and, in a
//! build with Discord's media stack, the MP3 and Ogg the soundboard plays too.
//! Speech is the operating system's own voice, so there is nothing to ship and
//! nothing to configure: `spd-say` on the desktop Linux uses, `say` on macOS,
//! and SAPI through PowerShell on Windows.

use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::audio_io::{Output, RATE};

/// The longest a plugin's sound may play. A sound longer than this is cut
/// where it is: a plugin is a notification, not a player.
pub const LONGEST_SECONDS: usize = 10;

/// The most a plugin may hand over as speech, in characters, so `audio.speak`
/// cannot read a book aloud with one call.
pub const MAX_SPEECH_CHARS: usize = 1_000;

/// The most of a plugin's sounds that play at once, so a plugin that asks for
/// one per event doesn't hold a thread and its samples for each.
const MAX_PLAYING: usize = 4;
static PLAYING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Decodes a sound file into 48 kHz interleaved stereo samples.
pub fn decode(data: &[u8]) -> Result<Vec<i16>> {
    let samples = if data.starts_with(b"RIFF") {
        wav(data)?
    } else {
        other(data)?
    };
    if samples.is_empty() {
        bail!("the sound is empty");
    }
    Ok(cut(samples))
}

/// A sound file that isn't a WAV: whatever the build's decoder reads.
#[cfg(feature = "discord")]
fn other(data: &[u8]) -> Result<Vec<i16>> {
    // The soundboard's own decoder, which is how a soundboard sound, an MP3 or
    // an Ogg becomes what the speakers take.
    crate::soundboard::decode(data)
}

#[cfg(not(feature = "discord"))]
fn other(_data: &[u8]) -> Result<Vec<i16>> {
    bail!("this build reads only WAV files: it carries no other sound decoder")
}

/// A sound, played on a thread of its own at `volume` from
/// [`crate::sounds::gain`], as it is when that is absent.
///
/// The sound is decoded before this answers, so a plugin is told when what it
/// brought isn't a sound Yapfast can play; nothing waits for it to end.
pub fn play(data: Vec<u8>, volume: Option<u8>) -> Result<()> {
    use std::sync::atomic::Ordering;
    let mut samples = decode(&data)?;
    let gain = crate::sounds::gain(volume.unwrap_or(100));
    samples
        .iter_mut()
        .for_each(|sample| *sample = (f32::from(*sample) * gain) as i16);
    if PLAYING.fetch_add(1, Ordering::Relaxed) >= MAX_PLAYING {
        PLAYING.fetch_sub(1, Ordering::Relaxed);
        bail!("{MAX_PLAYING} sounds already play: let one finish");
    }
    let spawned = std::thread::Builder::new()
        .name("yapfast-plugin-sound".into())
        .spawn(move || {
            if let Err(error) = play_samples(&samples) {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "could not play a plugin's sound"
                );
            }
            PLAYING.fetch_sub(1, Ordering::Relaxed);
        });
    if let Err(error) = spawned {
        PLAYING.fetch_sub(1, Ordering::Relaxed);
        return Err(error).context("could not start the sound's thread");
    }
    Ok(())
}

/// Says `text` in the operating system's own voice.
///
/// The text is never handed to a shell: it is one argument of the program, or,
/// where the program takes a script, a file the script reads. So nothing a
/// plugin writes can become a command.
pub fn speak(text: &str) -> Result<()> {
    let text = text.trim();
    if text.is_empty() {
        bail!("there is nothing to say");
    }
    if text.chars().count() > MAX_SPEECH_CHARS {
        bail!("speech is at most {MAX_SPEECH_CHARS} characters");
    }
    let mut child = speech(format!("{text} "))?;
    // Reaped by a thread, so no finished child is left behind and nothing here
    // waits on the one speaking.
    std::thread::Builder::new()
        .name("yapfast-plugin-speech".into())
        .spawn(move || {
            if let Err(error) = child.wait() {
                tracing::warn!(%error, "the speech program could not be waited for");
            }
        })
        .context("could not start the speech thread")?;
    Ok(())
}

/// The OS's speech program, given the words to say.
#[cfg(target_os = "linux")]
fn speech(text: String) -> Result<std::process::Child> {
    let started = Command::new("spd-say")
        .arg("--")
        .arg(text)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("the speech dispatcher, `spd-say`, isn't on this computer")?;
    Ok(started)
}

#[cfg(target_os = "macos")]
fn speech(text: String) -> Result<std::process::Child> {
    let started = Command::new("say")
        .arg(text)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("macOS's `say` couldn't be started")?;
    Ok(started)
}

/// Windows speaks through SAPI, whose PowerShell call is a script: the words go
/// in a file the script reads, so nothing a plugin writes is ever parsed as
/// part of it.
#[cfg(target_os = "windows")]
fn speech(text: String) -> Result<std::process::Child> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "yapfast-speech-{}-{}.txt",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, text.as_bytes()).context("could not write the words to say")?;
    const SCRIPT: &str = "Add-Type -AssemblyName System.Speech; $speech = New-Object \
                          System.Speech.Synthesis.SpeechSynthesizer; $speech.Speak(\
                          [IO.File]::ReadAllText($env:YAPFAST_SPEECH))";
    let started = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("YAPFAST_SPEECH", &path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("PowerShell, which speaks on Windows, couldn't be started");
    if started.is_err() {
        let _ = std::fs::remove_file(&path);
    }
    Ok(started?)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn speech(_text: String) -> Result<std::process::Child> {
    bail!("this system has no speech Yapfast knows how to ask")
}

/// Plays interleaved 48 kHz stereo samples to the end, on the default output.
fn play_samples(samples: &[i16]) -> Result<()> {
    let mut output =
        Output::<i16>::open(None, "Plugin sound", RATE, 2, None).context("no speakers")?;
    let mut chunk = [0i16; RATE as usize / 50 * 2];
    for part in samples.chunks(chunk.len()) {
        chunk[..part.len()].copy_from_slice(part);
        output
            .write(&chunk[..part.len()])
            .context("could not play")?;
    }
    output.drain().context("could not finish playing")?;
    Ok(())
}

/// A sound's samples, cut where [`LONGEST_SECONDS`] ends.
fn cut(mut samples: Vec<i16>) -> Vec<i16> {
    let longest = LONGEST_SECONDS * RATE as usize * 2;
    if samples.len() > longest {
        samples.truncate(longest);
    }
    samples
}

/// A WAV file as 48 kHz interleaved stereo samples: 16-bit PCM, to any number
/// of channels, moved to the shape the speakers take.
fn wav(data: &[u8]) -> Result<Vec<i16>> {
    const BAD: &str = "this isn't a WAV file Yapfast can read";
    let small = |at: usize| -> Result<u32> {
        data.get(at..at + 2)
            .map(|bytes| u32::from(u16::from_le_bytes([bytes[0], bytes[1]])))
            .ok_or_else(|| anyhow::anyhow!("{BAD}"))
    };
    let word = |at: usize| -> Result<u32> {
        data.get(at..at + 4)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| anyhow::anyhow!("{BAD}"))
    };
    if data.get(8..12) != Some(b"WAVE".as_slice()) {
        bail!("{BAD}");
    }
    let mut at = 12;
    while let Some(header) = data.get(at..at + 8) {
        // A chunk is its id and then its length, as RIFF writes one.
        let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let body = at + 8;
        if &header[0..4] == b"fmt " {
            // 1 is PCM; 0xFFFE is the extensible form of the same thing.
            if !matches!(small(body)?, 1 | 0xfffe) || small(body + 14)? != 16 {
                bail!("only 16-bit PCM WAV files are read");
            }
            let channels = small(body + 2)?.max(1) as usize;
            let rate = word(body + 4)?.max(1);
            let mut sound = body + length;
            while let Some(header) = data.get(sound..sound + 8) {
                let length =
                    u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
                if &header[0..4] == b"data" {
                    let pcm = data
                        .get(sound + 8..(sound + 8 + length).min(data.len()))
                        .ok_or_else(|| anyhow::anyhow!("{BAD}"))?;
                    return Ok(wave(pcm, channels, rate));
                }
                sound += 8 + length + (length & 1);
            }
            bail!("this WAV file has no sound in it");
        }
        at += 8 + length + (length & 1);
    }
    bail!("{BAD}")
}

/// 16-bit PCM of `channels` at `rate`, as 48 kHz interleaved stereo.
fn wave(pcm: &[u8], channels: usize, rate: u32) -> Vec<i16> {
    let frames = pcm.len() / 2 / channels;
    if frames == 0 {
        return Vec::new();
    }
    // One sample of a frame, every channel of it averaged into the middle.
    let sample = |frame: usize| -> f64 {
        (0..channels)
            .map(|channel| {
                let index = (frame * channels + channel) * 2;
                pcm.get(index..index + 2).map_or(0.0, |bytes| {
                    f64::from(i16::from_le_bytes([bytes[0], bytes[1]]))
                })
            })
            .sum::<f64>()
            / channels as f64
    };
    let step = f64::from(rate) / f64::from(RATE);
    let longest = LONGEST_SECONDS * RATE as usize;
    let mut out = Vec::new();
    let mut at = 0.0f64;
    while out.len() / 2 < longest && (at as usize) < frames {
        // Frames between stored ones by a straight line, the quietest
        // resampling there is: a plugin's sound is short, and its source is
        // usually 48 kHz already.
        let index = at as usize;
        let next = (index + 1).min(frames - 1);
        let between = at - index as f64;
        let value = sample(index) * (1.0 - between) + sample(next) * between;
        let value = value.clamp(-32768.0, 32767.0) as i16;
        out.push(value);
        out.push(value);
        at += step;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV file of `samples` mono frames at `rate`.
    fn wav_of(samples: &[i16], rate: u32) -> Vec<u8> {
        let mut pcm = Vec::new();
        for sample in samples {
            pcm.extend_from_slice(&sample.to_le_bytes());
        }
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16u32.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&rate.to_le_bytes());
        file.extend_from_slice(&(rate * 2).to_le_bytes());
        file.extend_from_slice(&2u16.to_le_bytes());
        file.extend_from_slice(&16u16.to_le_bytes());
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        file.extend_from_slice(&pcm);
        file
    }

    #[test]
    fn a_plugins_wav_becomes_what_the_speakers_take() {
        // A tenth of a second of 48 kHz mono, so the rate needs no change.
        let samples: Vec<i16> = (0..4_800).map(|at| (at % 100) as i16).collect();
        let decoded = decode(&wav_of(&samples, 48_000)).expect("a WAV file");
        assert_eq!(decoded.len(), 4_800 * 2, "stereo frames");
        assert!(decoded.iter().any(|sample| *sample != 0));
    }

    #[test]
    fn another_rate_is_moved_to_the_speakers_own() {
        let samples: Vec<i16> = vec![1_000; 2_400];
        let decoded = decode(&wav_of(&samples, 24_000)).expect("a WAV file");
        assert_eq!(decoded.len(), 4_800 * 2, "twice as many frames");
    }

    #[test]
    fn a_long_sound_is_cut_where_the_cap_is() {
        let samples = vec![100i16; 48_000 * 30];
        let decoded = decode(&wav_of(&samples, 48_000)).expect("a WAV file");
        assert_eq!(decoded.len(), LONGEST_SECONDS * RATE as usize * 2);
    }

    #[test]
    fn what_is_not_a_sound_is_refused() {
        assert!(decode(b"not a sound").is_err());
        assert!(decode(b"RIFF....WAVE").is_err());
        assert!(decode(&wav_of(&[], 48_000)).is_err(), "no sound in it");
        assert!(speak("   ").is_err());
        assert!(speak(&"a".repeat(MAX_SPEECH_CHARS + 1)).is_err());
    }
}
