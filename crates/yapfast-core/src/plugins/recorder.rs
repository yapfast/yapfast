//! A plugin's recording of the microphone: `microphone.start`.
//!
//! One runs at a time, on a thread of its own, from the microphone chosen for
//! calls, after the person said yes the first time, and for as long as it runs
//! the window shows a red bar naming the plugin, with Stop. It writes straight
//! to a file as it goes, 20 ms at a time, so a long recording holds no more
//! than a frame in memory: Ogg Opus, 48 kHz mono at 32 kbit/s, which is what
//! Discord's own clients send as a voice message (docs.discord.food,
//! resources/message, Voice Messages), or a WAV of 16-bit samples. Its waveform
//! is made as it records: the loudest sample of each 100 ms, at most 256
//! points, Discord's waveform. When it ends the plugin's `microphone.start`
//! is answered with the recording, which `microphone.read` reads and
//! `messages.voice` sends.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use anyhow::{Context, Result, bail};
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{HostReturn, Recorded, RecordingFormat, base64};

use crate::audio_io::{Input, RATE};

/// 20 ms of 48 kHz mono, the frame Opus encodes and the microphone is read in.
const FRAME: usize = RATE as usize / 50;
/// The samples in one waveform point: 100 ms.
const POINT: usize = RATE as usize / 10;
/// The most points Discord's waveform holds.
pub const MAX_POINTS: usize = 256;
/// Opus's look-ahead at 48 kHz, which a player skips (RFC 7845, 4.2).
const PRE_SKIP: u16 = 312;
/// The voice message bitrate Discord's clients use.
const BITRATE: i32 = 32_000;

/// Who stopped a recording, as the answer says.
const RUNNING: u8 = 0;
const BY_PLUGIN: u8 = 1;
const BY_PERSON: u8 = 2;

/// A recording under way, which stops when told.
#[derive(Debug)]
pub struct Recording {
    pub plugin: PluginId,
    stop: Arc<AtomicU8>,
}

impl Recording {
    /// Stops it, as the plugin asked.
    pub fn stop_for_plugin(&self) {
        let _ =
            self.stop
                .compare_exchange(RUNNING, BY_PLUGIN, Ordering::Relaxed, Ordering::Relaxed);
    }

    /// Stops it, as the person asked, from the red bar.
    pub fn stop_for_person(&self) {
        let _ =
            self.stop
                .compare_exchange(RUNNING, BY_PERSON, Ordering::Relaxed, Ordering::Relaxed);
    }
}

/// Starts recording for `plugin`, answering its `call` once it ends, into a new
/// file in `folder`.
pub fn start(
    plugin: PluginId,
    call: u64,
    format: RecordingFormat,
    max_seconds: Option<u32>,
    microphone: Option<String>,
    folder: &Path,
    answerer: crate::plugins::bridge::Answerer,
) -> Result<Recording> {
    #[cfg(not(feature = "discord"))]
    if format == RecordingFormat::Opus {
        bail!("this build has no Opus encoder; ask for `pcm`");
    }
    std::fs::create_dir_all(folder).context("couldn't make the recordings' folder")?;
    let extension = match format {
        RecordingFormat::Opus => "ogg",
        RecordingFormat::Pcm => "wav",
    };
    let path = folder.join(format!(
        "{}-{}.{extension}",
        plugin.as_str(),
        uuid::Uuid::new_v4().simple()
    ));
    let stop = Arc::new(AtomicU8::new(RUNNING));
    let running = Recording {
        plugin: plugin.clone(),
        stop: Arc::clone(&stop),
    };
    std::thread::Builder::new()
        .name("yapfast-plugin-recording".into())
        .spawn(move || {
            crate::audio_io::raise_capture_priority();
            let answer = match record(&path, format, max_seconds, microphone.as_deref(), &stop) {
                Ok(made) => {
                    let voice = (format == RecordingFormat::Opus)
                        .then(|| (made.duration_ms, made.waveform.clone()));
                    let recording = answerer.keep_recording(&plugin, path, made.bytes, voice);
                    let stopped_by = match stop.load(Ordering::Relaxed) {
                        BY_PLUGIN => "plugin",
                        BY_PERSON => "person",
                        _ => "length",
                    };
                    match serde_json::to_value(Recorded {
                        recording,
                        format,
                        duration_ms: made.duration_ms,
                        bytes: made.bytes,
                        waveform: made.waveform,
                        stopped_by: stopped_by.to_owned(),
                    }) {
                        Ok(value) => HostReturn::Ok(value),
                        Err(error) => HostReturn::Error {
                            message: error.to_string(),
                        },
                    }
                }
                Err(error) => {
                    let _ = std::fs::remove_file(&path);
                    HostReturn::Error {
                        message: format!("{error:#}"),
                    }
                }
            };
            answerer.answer(&plugin, call, answer);
            answerer.recording_ended(&plugin);
        })
        .context("couldn't start the recording's thread")?;
    Ok(running)
}

/// What a finished recording came to.
struct Made {
    duration_ms: u64,
    bytes: u64,
    waveform: String,
}

fn record(
    path: &Path,
    format: RecordingFormat,
    max_seconds: Option<u32>,
    microphone: Option<&str>,
    stop: &AtomicU8,
) -> Result<Made> {
    let mut input = Input::<f32>::open(
        microphone,
        "Plugin recording",
        RATE,
        1,
        std::time::Duration::from_millis(20),
    )
    .context("the microphone couldn't be opened")?;
    let mut writer = Writer::new(path, format)?;
    let mut waveform = Waveform::default();
    let most = max_seconds.map(|seconds| u64::from(seconds) * u64::from(RATE));
    let mut samples: u64 = 0;
    let mut frame = [0f32; FRAME];
    while stop.load(Ordering::Relaxed) == RUNNING && most.is_none_or(|most| samples < most) {
        input.read(&mut frame).context("the microphone stopped")?;
        waveform.push(&frame);
        writer.write(&frame)?;
        samples += FRAME as u64;
    }
    let bytes = writer.finish(samples)?;
    Ok(Made {
        duration_ms: samples * 1000 / u64::from(RATE),
        bytes,
        waveform: base64::encode(&waveform.points()),
    })
}

/// The loudness of each 100 ms, as it is recorded.
#[derive(Default)]
pub struct Waveform {
    points: Vec<u8>,
    peak: f32,
    counted: usize,
}

impl Waveform {
    pub fn push(&mut self, samples: &[f32]) {
        for sample in samples {
            self.peak = self.peak.max(sample.abs());
            self.counted += 1;
            if self.counted == POINT {
                self.points.push(loudness(self.peak));
                self.peak = 0.0;
                self.counted = 0;
            }
        }
    }

    /// The points, at most [`MAX_POINTS`]: a longer recording's are averaged
    /// down, as Discord's clients do.
    pub fn points(&self) -> Vec<u8> {
        let mut all = self.points.clone();
        if self.counted > 0 {
            all.push(loudness(self.peak));
        }
        if all.len() <= MAX_POINTS {
            return all;
        }
        (0..MAX_POINTS)
            .map(|index| {
                let from = index * all.len() / MAX_POINTS;
                let to = ((index + 1) * all.len() / MAX_POINTS).max(from + 1);
                let sum: u32 = all[from..to].iter().map(|&point| u32::from(point)).sum();
                (sum / (to - from) as u32) as u8
            })
            .collect()
    }
}

fn loudness(peak: f32) -> u8 {
    (peak.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Where the samples go.
enum Writer {
    #[cfg(feature = "discord")]
    Opus {
        ogg: Ogg,
        encoder: yapfast_discord::opus::Encoder,
        packet: Vec<u8>,
    },
    Wav(Wav),
}

impl Writer {
    fn new(path: &Path, format: RecordingFormat) -> Result<Self> {
        Ok(match format {
            #[cfg(feature = "discord")]
            RecordingFormat::Opus => {
                use yapfast_discord::opus::{Application, Encoder};
                Writer::Opus {
                    ogg: Ogg::new(path)?,
                    encoder: Encoder::new(1, Application::Voice, BITRATE)?,
                    packet: vec![0; 4000],
                }
            }
            #[cfg(not(feature = "discord"))]
            RecordingFormat::Opus => bail!("this build has no Opus encoder"),
            RecordingFormat::Pcm => Writer::Wav(Wav::new(path)?),
        })
    }

    fn write(&mut self, frame: &[f32]) -> Result<()> {
        match self {
            #[cfg(feature = "discord")]
            Writer::Opus {
                ogg,
                encoder,
                packet,
            } => {
                let length = encoder.encode_float(frame, packet)?;
                ogg.packet(&packet[..length], frame.len() as u64)
            }
            Writer::Wav(wav) => wav.write(frame),
        }
    }

    fn finish(self, _samples: u64) -> Result<u64> {
        match self {
            #[cfg(feature = "discord")]
            Writer::Opus { ogg, .. } => ogg.finish(),
            Writer::Wav(wav) => wav.finish(),
        }
    }
}

/// An Ogg Opus file as it is written (RFC 7845): the identification header
/// and the comment header on pages of their own, then the audio a second of
/// packets to a page.
pub struct Ogg {
    file: BufWriter<File>,
    serial: u32,
    sequence: u32,
    /// The samples encoded so far, after the pre-skip, which is what a page's
    /// granule position counts.
    granule: u64,
    pending: Vec<Vec<u8>>,
    segments: usize,
    written: u64,
}

impl Ogg {
    pub fn new(path: &Path) -> Result<Self> {
        let file = File::create(path).context("couldn't make the recording's file")?;
        let mut ogg = Ogg {
            file: BufWriter::new(file),
            serial: rand::random(),
            sequence: 0,
            granule: u64::from(PRE_SKIP),
            pending: Vec::new(),
            segments: 0,
            written: 0,
        };
        let mut head = Vec::with_capacity(19);
        head.extend_from_slice(b"OpusHead");
        head.push(1);
        head.push(1);
        head.extend_from_slice(&PRE_SKIP.to_le_bytes());
        head.extend_from_slice(&RATE.to_le_bytes());
        head.extend_from_slice(&0i16.to_le_bytes());
        head.push(0);
        ogg.page(0x02, 0, &[&head])?;
        let vendor = b"Yapfast";
        let mut tags = Vec::new();
        tags.extend_from_slice(b"OpusTags");
        tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        tags.extend_from_slice(vendor);
        tags.extend_from_slice(&0u32.to_le_bytes());
        ogg.page(0, 0, &[&tags])?;
        Ok(ogg)
    }

    /// One packet of `samples` samples.
    pub fn packet(&mut self, packet: &[u8], samples: u64) -> Result<()> {
        let segments = packet.len() / 255 + 1;
        if self.segments + segments > 255 || self.pending.len() >= 50 {
            self.flush(0)?;
        }
        self.pending.push(packet.to_vec());
        self.segments += segments;
        self.granule += samples;
        Ok(())
    }

    fn flush(&mut self, flags: u8) -> Result<()> {
        let pending = std::mem::take(&mut self.pending);
        let packets: Vec<&[u8]> = pending.iter().map(Vec::as_slice).collect();
        // A page's granule is where the last packet that ends on it ends; the
        // pending packets all end here, and the count runs ahead of them by
        // nothing, as they were counted as they came.
        self.page(flags, self.granule, &packets)?;
        self.segments = 0;
        Ok(())
    }

    /// Ends the stream on a last page and closes the file: its size.
    pub fn finish(mut self) -> Result<u64> {
        self.flush(0x04)?;
        self.file.flush()?;
        Ok(self.written)
    }

    fn page(&mut self, flags: u8, granule: u64, packets: &[&[u8]]) -> Result<()> {
        let mut lacing = Vec::new();
        for packet in packets {
            lacing.extend(std::iter::repeat_n(255u8, packet.len() / 255));
            lacing.push((packet.len() % 255) as u8);
        }
        if lacing.len() > 255 {
            bail!("an Ogg page holds at most 255 segments");
        }
        let mut page =
            Vec::with_capacity(27 + lacing.len() + packets.iter().map(|p| p.len()).sum::<usize>());
        page.extend_from_slice(b"OggS");
        page.push(0);
        page.push(flags);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&self.serial.to_le_bytes());
        page.extend_from_slice(&self.sequence.to_le_bytes());
        page.extend_from_slice(&[0; 4]);
        page.push(lacing.len() as u8);
        page.extend_from_slice(&lacing);
        for packet in packets {
            page.extend_from_slice(packet);
        }
        let crc = ogg_crc(&page);
        page[22..26].copy_from_slice(&crc.to_le_bytes());
        self.file.write_all(&page)?;
        self.written += page.len() as u64;
        self.sequence += 1;
        Ok(())
    }
}

/// Ogg's CRC-32: polynomial 0x04C11DB7, not reflected, starting at zero.
pub fn ogg_crc(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (index, entry) in table.iter_mut().enumerate() {
            let mut value = (index as u32) << 24;
            for _ in 0..8 {
                value = if value & 0x8000_0000 != 0 {
                    (value << 1) ^ 0x04C1_1DB7
                } else {
                    value << 1
                };
            }
            *entry = value;
        }
        table
    });
    data.iter().fold(0u32, |crc, &byte| {
        (crc << 8) ^ table[(((crc >> 24) as u8) ^ byte) as usize]
    })
}

/// A WAV of 16-bit 48 kHz mono as it is written, its sizes filled in at the
/// end.
pub struct Wav {
    file: BufWriter<File>,
    data: u64,
}

impl Wav {
    pub fn new(path: &Path) -> Result<Self> {
        let file = File::create(path).context("couldn't make the recording's file")?;
        let mut wav = Wav {
            file: BufWriter::new(file),
            data: 0,
        };
        wav.header(0)?;
        Ok(wav)
    }

    fn header(&mut self, data: u32) -> Result<()> {
        let mut header = Vec::with_capacity(44);
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&(36 + data).to_le_bytes());
        header.extend_from_slice(b"WAVEfmt ");
        header.extend_from_slice(&16u32.to_le_bytes());
        header.extend_from_slice(&1u16.to_le_bytes());
        header.extend_from_slice(&1u16.to_le_bytes());
        header.extend_from_slice(&RATE.to_le_bytes());
        header.extend_from_slice(&(RATE * 2).to_le_bytes());
        header.extend_from_slice(&2u16.to_le_bytes());
        header.extend_from_slice(&16u16.to_le_bytes());
        header.extend_from_slice(b"data");
        header.extend_from_slice(&data.to_le_bytes());
        self.file.write_all(&header)?;
        Ok(())
    }

    pub fn write(&mut self, frame: &[f32]) -> Result<()> {
        for sample in frame {
            let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
            self.file.write_all(&value.to_le_bytes())?;
        }
        self.data += frame.len() as u64 * 2;
        Ok(())
    }

    pub fn finish(mut self) -> Result<u64> {
        let data = u32::try_from(self.data).context("a WAV holds at most 4 GiB")?;
        self.file.seek(SeekFrom::Start(0))?;
        self.header(data)?;
        self.file.flush()?;
        Ok(44 + self.data)
    }
}

/// The recordings' folder under the plugins' data.
pub fn folder(attach: &Path) -> PathBuf {
    attach.join("recordings")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crc_is_oggs() {
        // The page CRC of the empty string is zero, and of "OggS" with this
        // polynomial unreflected from zero.
        assert_eq!(ogg_crc(b""), 0);
        let mut page = b"OggS\0\x02".to_vec();
        page.extend_from_slice(&[0; 20]);
        page.push(0);
        let crc = ogg_crc(&page);
        assert_ne!(crc, 0);
        assert_eq!(ogg_crc(&page), crc, "a table read twice is the same table");
    }

    #[test]
    fn a_long_waveform_is_averaged_to_discords_256_points() {
        let mut waveform = Waveform::default();
        waveform.push(&vec![0.5; POINT * 3]);
        assert_eq!(waveform.points(), vec![128, 128, 128]);
        let mut long = Waveform::default();
        for index in 0..600 {
            long.push(&vec![if index % 2 == 0 { 1.0 } else { 0.0 }; POINT]);
        }
        let points = long.points();
        assert_eq!(points.len(), MAX_POINTS);
        assert!(points.iter().all(|&point| (80..=180).contains(&point)));
    }

    #[test]
    fn a_wav_says_its_own_length() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.wav");
        let mut wav = Wav::new(&path).unwrap();
        wav.write(&[0.5; FRAME]).unwrap();
        assert_eq!(wav.finish().unwrap(), 44 + FRAME as u64 * 2);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(
            u32::from_le_bytes(bytes[40..44].try_into().unwrap()),
            FRAME as u32 * 2
        );
    }

    /// Pages that read back: each begins `OggS`, carries its own CRC, counts
    /// up, the first begins the stream and the last ends it, and the headers
    /// are RFC 7845's.
    #[test]
    fn an_ogg_opus_file_is_well_formed() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.ogg");
        let mut ogg = Ogg::new(&path).unwrap();
        for _ in 0..120 {
            ogg.packet(&[0xf8, 0xff, 0xfe], FRAME as u64).unwrap();
        }
        let size = ogg.finish().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len() as u64, size);
        let mut at = 0;
        let mut pages = Vec::new();
        while at < bytes.len() {
            assert_eq!(&bytes[at..at + 4], b"OggS");
            let segments = usize::from(bytes[at + 26]);
            let body: usize = bytes[at + 27..at + 27 + segments]
                .iter()
                .map(|&lace| usize::from(lace))
                .sum();
            let end = at + 27 + segments + body;
            let mut page = bytes[at..end].to_vec();
            let crc = u32::from_le_bytes(page[22..26].try_into().unwrap());
            page[22..26].copy_from_slice(&[0; 4]);
            assert_eq!(ogg_crc(&page), crc, "page {}", pages.len());
            pages.push((
                bytes[at + 5],
                u64::from_le_bytes(bytes[at + 6..at + 14].try_into().unwrap()),
                bytes[at + 27 + segments..end].to_vec(),
            ));
            at = end;
        }
        assert_eq!(pages[0].0, 0x02, "the first page begins the stream");
        assert!(pages[0].2.starts_with(b"OpusHead"));
        assert!(pages[1].2.starts_with(b"OpusTags"));
        assert_eq!(pages.last().unwrap().0, 0x04, "the last ends it");
        assert_eq!(
            pages.last().unwrap().1,
            u64::from(PRE_SKIP) + 120 * FRAME as u64,
            "the last granule is every sample after the pre-skip"
        );
    }

    /// A recording of the silent stand-in microphone the tests use: it runs
    /// to its length, is an Ogg Opus file with a waveform of silence, and
    /// says how long it ran.
    #[test]
    fn a_recording_runs_to_its_length_as_ogg_opus() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.ogg");
        let stop = AtomicU8::new(RUNNING);
        let made = record(&path, RecordingFormat::Opus, Some(1), None, &stop).unwrap();
        assert_eq!(made.duration_ms, 1000);
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"OggS"));
        assert_eq!(made.bytes, bytes.len() as u64);
        let points = base64::decode(&made.waveform).unwrap();
        assert_eq!(points, vec![0; 10], "a point a 100 ms, all silent");
    }
}
