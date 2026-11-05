//! Opus, from the libopus inside Yapfast's libwebrtc archive, which every Yapfast
//! build links already. A second libopus, such as `opus2` builds, would put
//! the same symbols in the binary twice.

use std::{ffi::c_int, ptr::NonNull};

use anyhow::{Result, bail};
// The libopus these call is in libwebrtc, which this crate links.
use webrtc_sys as _;

#[repr(C)]
struct RawEncoder {
    _private: [u8; 0],
}

#[repr(C)]
struct RawDecoder {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn opus_encoder_create(
        rate: i32,
        channels: c_int,
        application: c_int,
        error: *mut c_int,
    ) -> *mut RawEncoder;
    fn opus_encoder_destroy(encoder: *mut RawEncoder);
    fn opus_encode_float(
        encoder: *mut RawEncoder,
        pcm: *const f32,
        frame_size: c_int,
        data: *mut u8,
        max_data_bytes: i32,
    ) -> i32;
    fn opus_encoder_ctl(encoder: *mut RawEncoder, request: c_int, ...) -> c_int;
    fn opus_decoder_create(rate: i32, channels: c_int, error: *mut c_int) -> *mut RawDecoder;
    fn opus_decoder_destroy(decoder: *mut RawDecoder);
    fn opus_decode(
        decoder: *mut RawDecoder,
        data: *const u8,
        len: i32,
        pcm: *mut i16,
        frame_size: c_int,
        decode_fec: c_int,
    ) -> c_int;
}

const RATE: i32 = 48_000;
const APPLICATION_VOIP: c_int = 2048;
const APPLICATION_AUDIO: c_int = 2049;
// libopus's one-int encoder requests, as `opus_defines.h` numbers them.
const SET_BITRATE: c_int = 4002;
const SET_VBR: c_int = 4006;
const SET_COMPLEXITY: c_int = 4010;
const SET_INBAND_FEC: c_int = 4012;
const SET_PACKET_LOSS_PERC: c_int = 4014;
const SET_DTX: c_int = 4016;
const SET_VBR_CONSTRAINT: c_int = 4020;

/// libopus's effort setting, 0 to 10, where 10 is the most work in every
/// frame and 9 is the library's own default.
const COMPLEXITY: c_int = 9;

/// The loss a Voice encoder should expect, in percent. libopus weighs its FEC
/// against this, and spends nothing on FEC when it is zero.
const CALL_PACKET_LOSS: c_int = 5;

/// What an encoder is tuned for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Application {
    /// Speech, as a microphone sends.
    Voice,
    /// Anything else, such as a shared screen's sound.
    Audio,
}

/// An Opus encoder at 48 kHz.
pub struct Encoder {
    raw: NonNull<RawEncoder>,
    channels: usize,
}

// SAFETY: an encoder is used from one thread at a time, through `&mut`.
unsafe impl Send for Encoder {}

/// Sets one int-valued encoder control. libopus answers `OPUS_OK` or a
/// negative code, so a refused setting isn't left looking as if it took.
fn set(encoder: &Encoder, request: c_int, value: c_int, setting: &str) -> Result<()> {
    // SAFETY: these requests take one int, as libopus's macros pass it, and
    // `encoder` owns the pointer it was created with.
    let outcome = unsafe { opus_encoder_ctl(encoder.raw.as_ptr(), request, value) };
    if outcome < 0 {
        bail!("Opus couldn't set {setting} ({outcome})");
    }
    Ok(())
}

impl Encoder {
    pub fn new(channels: u16, application: Application, bitrate: i32) -> Result<Self> {
        let mut error = 0;
        let application = match application {
            Application::Voice => APPLICATION_VOIP,
            Application::Audio => APPLICATION_AUDIO,
        };
        // SAFETY: valid arguments; `error` is written before returning.
        let raw =
            unsafe { opus_encoder_create(RATE, c_int::from(channels), application, &mut error) };
        let Some(raw) = NonNull::new(raw).filter(|_| error == 0) else {
            bail!("could not start the Opus encoder ({error})");
        };
        let encoder = Self {
            raw,
            channels: usize::from(channels),
        };
        // A control libopus refuses leaves the encoder at defaults that don't
        // match what the caller asked for, so report it; returning early drops
        // `encoder`, which frees the encoder just created.
        set(&encoder, SET_BITRATE, bitrate, "the bitrate")?;
        // Effort per frame, on libopus's 0 to 10 scale. 9 is the library's
        // default, already near the top; 10 works harder in every 20 ms tick
        // for a change nobody here has listened to, and less than 9 would
        // spend less care on the sound for a saving nobody measured as safe.
        set(&encoder, SET_COMPLEXITY, COMPLEXITY, "the complexity")?;
        // Variable bitrate, but constrained: quiet frames spend less and loud
        // ones a little more, while the total stays near the bitrate the
        // caller set. Unconstrained, an awkward transient asked for double a
        // 64 kbit/s stream's budget in one frame, 130 kbit/s against 98 with
        // the constraint on a synthetic mix; the stream's packets are paced
        // and its path drops what arrives late, so the smaller peak is the
        // one that keeps the sound whole.
        set(&encoder, SET_VBR, 1, "variable bitrate")?;
        set(
            &encoder,
            SET_VBR_CONSTRAINT,
            1,
            "constrained variable bitrate",
        )?;
        // Discontinuous transmission would send nothing during quiet sound.
        // That suits a microphone between sentences, not shared music: the
        // stream's RTP timestamps advance one 20 ms frame at a time, a
        // listener's buffer expects a packet for each, and the mixer already
        // stops sending when no application plays, so there is nothing here
        // for DTX to save.
        set(&encoder, SET_DTX, 0, "discontinuous transmission")?;
        // Signal type stays at libopus's automatic choice. The application
        // already separates speech from other sound, and calls send through
        // Audio mode, so calling Audio "music" would mislabel as music the
        // very frames that most need the encoder's speech handling.
        if application == APPLICATION_VOIP {
            // In-band FEC lets a receiver rebuild a lost frame from the next
            // packet, and libopus spends on it only when it also knows the
            // loss to expect: FEC alone left packet sizes unchanged here,
            // here, while FEC with an estimate put recoverable data in every
            // voiced one. A call's speech is worth the band that costs, so
            // Voice asks for both. Audio, which is a stream's shared music,
            // does not: the low band FEC duplicates is the part music can
            // most afford to lose, and asking at 128 kbit/s stereo measured
            // about a quarter less payload. Whether that trade sounds better
            // is a listening comparison this machine cannot make.
            set(&encoder, SET_INBAND_FEC, 1, "inband FEC")?;
            set(
                &encoder,
                SET_PACKET_LOSS_PERC,
                CALL_PACKET_LOSS,
                "the packet loss",
            )?;
        }
        Ok(encoder)
    }

    /// Encodes one frame of interleaved samples into `out`, returning the
    /// packet's length.
    pub fn encode_float(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize> {
        let frame = pcm.len() / self.channels;
        // SAFETY: `pcm` holds `frame` samples per channel and `out` its length.
        let length = unsafe {
            opus_encode_float(
                self.raw.as_ptr(),
                pcm.as_ptr(),
                frame as c_int,
                out.as_mut_ptr(),
                out.len().min(i32::MAX as usize) as i32,
            )
        };
        if length < 0 {
            bail!("Opus couldn't encode a frame ({length})");
        }
        Ok(length as usize)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: made by opus_encoder_create, destroyed once.
        unsafe { opus_encoder_destroy(self.raw.as_ptr()) };
    }
}

/// An Opus decoder at 48 kHz.
pub struct Decoder {
    raw: NonNull<RawDecoder>,
    channels: usize,
}

// SAFETY: as for the encoder.
unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new(channels: u16) -> Result<Self> {
        let mut error = 0;
        // SAFETY: valid arguments; `error` is written before returning.
        let raw = unsafe { opus_decoder_create(RATE, c_int::from(channels), &mut error) };
        let Some(raw) = NonNull::new(raw).filter(|_| error == 0) else {
            bail!("could not start the Opus decoder ({error})");
        };
        Ok(Self {
            raw,
            channels: usize::from(channels),
        })
    }

    /// Decodes a packet into interleaved samples, returning the samples per
    /// channel. `fec` reads the forward error correction a packet carries
    /// for the one before it.
    pub fn decode(&mut self, packet: &[u8], pcm: &mut [i16], fec: bool) -> Result<usize> {
        self.run(Some(packet), pcm, fec)
    }

    /// Conceals a lost packet, filling all of `pcm`: a frame's worth of
    /// sound that carries on from the last.
    pub fn conceal(&mut self, pcm: &mut [i16]) -> Result<usize> {
        self.run(None, pcm, false)
    }

    fn run(&mut self, packet: Option<&[u8]>, pcm: &mut [i16], fec: bool) -> Result<usize> {
        let (data, length) = packet.map_or((std::ptr::null(), 0), |packet| {
            (packet.as_ptr(), packet.len() as i32)
        });
        // SAFETY: `pcm` holds this many samples per channel; a null packet
        // asks for concealment.
        let samples = unsafe {
            opus_decode(
                self.raw.as_ptr(),
                data,
                length,
                pcm.as_mut_ptr(),
                (pcm.len() / self.channels) as c_int,
                c_int::from(fec),
            )
        };
        if samples < 0 {
            bail!("Opus couldn't decode a packet ({samples})");
        }
        Ok(samples as usize)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: made by opus_decoder_create, destroyed once.
        unsafe { opus_decoder_destroy(self.raw.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // libopus's getters for the controls `new` sets, and for the ones it
    // deliberately leaves alone. A check that asks the encoder what it holds
    // is worth more than one that only trusts the constants.
    const GET_BITRATE: c_int = 4003;
    const GET_VBR: c_int = 4007;
    const GET_COMPLEXITY: c_int = 4011;
    const GET_INBAND_FEC: c_int = 4013;
    const GET_PACKET_LOSS_PERC: c_int = 4015;
    const GET_DTX: c_int = 4017;
    const GET_VBR_CONSTRAINT: c_int = 4021;
    const GET_SIGNAL: c_int = 4025;
    const SIGNAL_AUTO: c_int = -1000;

    /// The bandwidth libopus reports for a packet that carries the full band.
    const FULLBAND: c_int = 1105;

    // libopus's packet-header readers. They read the TOC byte and frame count
    // the encoder actually wrote, the only public way to ask what mode and
    // framing a packet ended up with.
    unsafe extern "C" {
        fn opus_packet_get_bandwidth(data: *const u8) -> c_int;
        fn opus_packet_get_nb_channels(data: *const u8) -> c_int;
        fn opus_packet_get_samples_per_frame(data: *const u8, rate: i32) -> c_int;
    }

    /// libopus's own special bitrates: `OPUS_AUTO`, and the highest rate the
    /// encoder can spend.
    const OPUS_AUTO: i32 = -1000;
    const OPUS_BITRATE_MAX: i32 = -1;

    /// Reads back what the encoder took. `new` already reports a refusal, so a
    /// getter failing here is a broken libopus rather than a setting to fix.
    fn get(encoder: &Encoder, request: c_int) -> c_int {
        let mut value: c_int = 0;
        // SAFETY: a get request takes one `int*`, and `value` outlives the call.
        let outcome =
            unsafe { opus_encoder_ctl(encoder.raw.as_ptr(), request, &mut value as *mut c_int) };
        assert!(outcome >= 0, "libopus couldn't read a setting ({outcome})");
        value
    }

    /// A quiet 20 ms stereo frame, for checks that only need a real packet.
    fn tone() -> Vec<f32> {
        (0..960)
            .flat_map(|at| {
                let sample = (at as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                [sample, sample]
            })
            .collect()
    }

    /// A speech-like 20 ms mono frame: a 140 Hz voice's harmonics, which gives
    /// the low band FEC copies something to carry.
    fn voice() -> Vec<f32> {
        (0..960)
            .map(|at| {
                let phase = at as f32 * 140.0 * std::f32::consts::TAU / 48_000.0;
                let mut sample = 0.0;
                for harmonic in 1..=8 {
                    sample += (phase * harmonic as f32).sin() / harmonic as f32;
                }
                0.3 * sample
            })
            .collect()
    }

    /// Encodes each frame into a packet of its own.
    fn encode(encoder: &mut Encoder, frames: &[Vec<f32>]) -> Vec<Vec<u8>> {
        let mut packet = [0u8; 1500];
        frames
            .iter()
            .map(|frame| {
                let length = encoder
                    .encode_float(frame, &mut packet)
                    .expect("a real frame");
                packet[..length].to_vec()
            })
            .collect()
    }

    /// Decodes packet `lost + 1` asking for the FEC data it carries for the
    /// lost frame, and separately conceals that same frame from the same
    /// state. libopus falls back to concealment when a packet has no FEC data,
    /// so equal samples mean none was there; any difference is FEC the packet
    /// really carried.
    fn fec_against_concealment(packets: &[Vec<u8>], lost: usize, channels: u16) -> usize {
        let samples = 960 * usize::from(channels);
        let mut pcm = vec![0i16; samples];
        let mut recovering = Decoder::new(channels).unwrap();
        for packet in &packets[..lost] {
            recovering.decode(packet, &mut pcm, false).unwrap();
        }
        let mut recovered = vec![0i16; samples];
        recovering
            .decode(&packets[lost + 1], &mut recovered, true)
            .unwrap();
        let mut concealing = Decoder::new(channels).unwrap();
        for packet in &packets[..lost] {
            concealing.decode(packet, &mut pcm, false).unwrap();
        }
        let mut concealed = vec![0i16; samples];
        concealing.conceal(&mut concealed).unwrap();
        recovered
            .iter()
            .zip(&concealed)
            .filter(|(recovered, concealed)| recovered != concealed)
            .count()
    }

    #[test]
    fn a_stream_encoder_takes_the_settings_it_is_given() {
        // Shared sound: Audio application, and the settings `new` pins for it.
        // These are libopus's defaults, but they are Yapfast's choice here so
        // that a different libopus cannot change them underneath the stream.
        let encoder = Encoder::new(2, Application::Audio, 64_000).unwrap();
        assert_eq!(get(&encoder, GET_BITRATE), 64_000);
        assert_eq!(get(&encoder, GET_COMPLEXITY), COMPLEXITY);
        assert_eq!(get(&encoder, GET_VBR), 1, "variable bitrate");
        assert_eq!(get(&encoder, GET_VBR_CONSTRAINT), 1, "constrained VBR");
        assert_eq!(get(&encoder, GET_DTX), 0, "no discontinuous transmission");
        assert_eq!(get(&encoder, GET_INBAND_FEC), 0, "no FEC for music");
        assert_eq!(get(&encoder, GET_PACKET_LOSS_PERC), 0, "no loss estimate");
        assert_eq!(get(&encoder, GET_SIGNAL), SIGNAL_AUTO, "signal left alone");
    }

    #[test]
    fn a_voice_encoder_takes_fec_and_a_loss_estimate() {
        // Speech, as a microphone sends it. FEC only means anything with a
        // loss estimate beside it, so both must be what the wrapper asked.
        let encoder = Encoder::new(1, Application::Voice, 64_000).unwrap();
        assert_eq!(get(&encoder, GET_INBAND_FEC), 1);
        assert_eq!(get(&encoder, GET_PACKET_LOSS_PERC), CALL_PACKET_LOSS);
        assert_eq!(get(&encoder, GET_COMPLEXITY), COMPLEXITY);
        assert_eq!(get(&encoder, GET_VBR_CONSTRAINT), 1, "constrained VBR");
        assert_eq!(get(&encoder, GET_DTX), 0);
    }

    #[test]
    fn a_stream_packet_is_20_ms_of_full_band_stereo() {
        // What a listener receives, read from the packet's own header: two
        // channels, 960 samples (20 ms at 48 kHz), and the full 20 kHz band
        // the Audio application is meant to keep.
        let mut encoder = Encoder::new(2, Application::Audio, 64_000).unwrap();
        let mut packet = [0u8; 1500];
        let length = encoder.encode_float(&tone(), &mut packet).unwrap();
        assert!(length > 3, "a real packet, not {length} bytes");
        // SAFETY: the header readers only read the TOC byte libopus just wrote.
        unsafe {
            assert_eq!(opus_packet_get_nb_channels(packet.as_ptr()), 2);
            assert_eq!(
                opus_packet_get_samples_per_frame(packet.as_ptr(), RATE),
                960
            );
            assert_eq!(opus_packet_get_bandwidth(packet.as_ptr()), FULLBAND);
        }
    }

    #[test]
    fn a_call_packet_carries_fec_for_the_frame_before_it() {
        // The wrapper asks for FEC on Voice. That is only worth anything if
        // libopus put recoverable data in the packet: decoding a packet's FEC
        // for a lost frame must differ from plain concealment of that frame.
        // If the encoder ever stops carrying it, this fails rather than the
        // flag quietly doing nothing.
        let mut encoder = Encoder::new(1, Application::Voice, 64_000).unwrap();
        let frames: Vec<Vec<f32>> = (0..5).map(|_| voice()).collect();
        let packets = encode(&mut encoder, &frames);
        let differing: usize = (1..=3)
            .map(|lost| fec_against_concealment(&packets, lost, 1))
            .sum();
        assert!(
            differing > 0,
            "no packet carried FEC for the frame before it"
        );
    }

    #[test]
    fn stream_sound_has_no_fec_to_fall_back_on() {
        // Audio gets neither FEC nor a loss estimate, so a packet holds
        // nothing for the frame before it: asking for FEC decodes exactly the
        // concealment the stream's receiver falls back on for a lost packet.
        let mut encoder = Encoder::new(2, Application::Audio, 64_000).unwrap();
        let frames: Vec<Vec<f32>> = (0..5).map(|_| tone()).collect();
        let packets = encode(&mut encoder, &frames);
        let differing: usize = (1..=3)
            .map(|lost| fec_against_concealment(&packets, lost, 2))
            .sum();
        assert_eq!(differing, 0, "shared sound carried FEC");
    }

    #[test]
    fn every_application_takes_every_legal_kind_of_bitrate() {
        // Only libopus decides which bitrates its rate control takes, from
        // the encoder's own configuration, so the wrapper must not vet them:
        // its sentinels and the rates it accepts are legal here.
        for (application, name) in [(Application::Voice, "voice"), (Application::Audio, "audio")] {
            for bitrate in [OPUS_AUTO, OPUS_BITRATE_MAX, 500, 32_000, 64_000, 128_000] {
                if let Err(error) = Encoder::new(2, application, bitrate) {
                    panic!("{name} at {bitrate} bit/s: {error}");
                }
            }
        }
    }

    #[test]
    fn every_application_still_encodes_after_configuring() {
        // Voice sets FEC and a loss estimate; Audio sets neither. Both must
        // come out of `new` working, with every control accepted.
        let mut packet = [0u8; 1500];
        for (application, name) in [(Application::Voice, "voice"), (Application::Audio, "audio")] {
            let mut encoder = Encoder::new(2, application, 64_000)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let length = encoder
                .encode_float(&tone(), &mut packet)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(length > 3, "{name}: a real packet, not {length} bytes");
        }
    }

    #[test]
    fn a_bitrate_libopus_refuses_names_the_setting() {
        // The bitrate control runs after the encoder exists, so this also
        // covers the post-creation failure path: the `?` returns, and the
        // `Encoder` built above it frees the native encoder on the way out.
        let error = match Encoder::new(2, Application::Audio, -1_500) {
            Ok(_) => panic!("libopus took a bitrate it has no such value for"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(message.contains("bitrate"), "{message}");
        assert!(message.contains("Opus"), "{message}");
    }

    #[test]
    fn a_refused_control_names_itself_and_spares_the_encoder() {
        // libopus takes a loss estimate of 0 to 100 and a complexity of 0 to
        // 10. A refusal is reported with the setting that failed, and the
        // encoder is still usable: the helper borrows it rather than
        // consuming or freeing it.
        let mut encoder = Encoder::new(1, Application::Audio, 64_000).unwrap();
        let error = set(&encoder, SET_PACKET_LOSS_PERC, 101, "the packet loss")
            .expect_err("libopus refuses a loss estimate over 100");
        assert!(error.to_string().contains("packet loss"), "{error}");
        let error = set(&encoder, SET_COMPLEXITY, 11, "the complexity")
            .expect_err("libopus refuses a complexity over 10");
        assert!(error.to_string().contains("complexity"), "{error}");
        let mut packet = [0u8; 1500];
        let mono: Vec<f32> = tone().chunks_exact(2).map(|pair| pair[0]).collect();
        assert!(encoder.encode_float(&mono, &mut packet).unwrap() > 3);
    }

    #[test]
    fn a_tone_survives_encoding_and_loss() {
        let mut encoder = Encoder::new(1, Application::Voice, 64_000).unwrap();
        let mut decoder = Decoder::new(2).unwrap();
        let mut packet = [0u8; 1500];
        let mut pcm = [0i16; 960 * 2];
        let mut loudest = 0;
        for frame in 0..10 {
            let tone: Vec<f32> = (0..960)
                .map(|at| {
                    ((frame * 960 + at) as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin()
                        * 0.5
                })
                .collect();
            let length = encoder.encode_float(&tone, &mut packet).unwrap();
            assert!(length > 3, "a real packet");
            assert_eq!(
                decoder.decode(&packet[..length], &mut pcm, false).unwrap(),
                960
            );
            loudest = loudest.max(
                pcm.iter()
                    .map(|sample| sample.unsigned_abs())
                    .max()
                    .unwrap(),
            );
        }
        assert!(loudest > 8_000, "the tone came through: {loudest}");
        assert_eq!(
            decoder.conceal(&mut pcm).unwrap(),
            960,
            "a lost frame fills in"
        );
    }
}
