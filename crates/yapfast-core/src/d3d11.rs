//! Encoding on the GPU with no pixel copy: native/d3d11enc.c opens NVENC on a
//! D3D11 device and reads the caller's texture there, so a captured frame never
//! becomes an I420 picture in memory on its way to the encoder.
//!
//! native/hwenc.c is the other way to a GPU's encoder, and the one a stream
//! uses today: it takes I420, which costs the processor a conversion and an
//! upload of every frame. This is the same encoders reading the frame where it
//! already is, and it needs two things of its caller that hwenc.c doesn't: the
//! texture has to live on this encoder's device ([`D3d11Encoder::device`]), and
//! it has to be a concrete format — BGRA, RGBA or NV12 — since the driver reads
//! the surface's own format and refuses a typeless one.
//!
//! The one copy is the bitstream: the driver's mapped output is moved into the
//! caller's buffer once per frame, so a packet can outlive the encoder's lock.
//! Nothing else of a frame is touched by the processor, and
//! [`D3d11Encoder::copied`] answers zero while that holds. The API mirrors
//! [`Encoder`](crate::encode::Encoder)'s where it can — an engine and its
//! settings, a bitrate that moves mid-stream, a frame with its time — so a
//! caller that has textures can hold this instead of that, and the stream's
//! bytes and timing are the same either way.
//!
//! A capture is rarely the size of a stream — a display wider than 1080p against
//! a 1080p stream is the ordinary case — so the encoder also scales:
//! [`D3d11Encoder::scale`] puts a captured picture at the stream's size in a
//! texture the encoder keeps and reuses, and [`D3d11Encoder::readback`] reads a
//! tile of it back at whatever size a preview asks for through one staging
//! texture, so a preview costs that tile rather than the screen. Both stay on the
//! GPU and go through the driver's video processor where it takes the texture,
//! and through a pixel shader where it doesn't; [`D3d11Encoder::filter`] says
//! which ran.
//!
//! Wiring this into a stream needs a capture source that fills a texture on
//! [`D3d11Encoder::device`] instead of handing over I420, or an encoder opened on
//! the capture's device with [`D3d11Encoder::open_on`], which is the same one
//! device seen from the other end — [`crate::wgc::Capture::open_on`] and
//! [`crate::dup::Capture::open_on`] are that direction. Either way there is one
//! D3D11 device for a capture and its encoder rather than two, and a texture made
//! by one is read by the other because it is the same object. What the capture
//! path will need from here: its textures made with `D3D11_BIND_SHADER_RESOURCE`,
//! or as render targets, which is what a video processor input view and a shader
//! resource view each take; a `scale` to the stream's size before each
//! `encode_texture` where the capture's size differs; and a `readback` at tile
//! size where a preview used to open a second capture of the same screen.

use std::{
    ffi::{c_char, c_int, c_long, c_longlong, c_uchar, c_uint, c_ulonglong, c_void},
    sync::Arc,
};

use anyhow::{Result, anyhow, bail};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1,
};
use yapfast_discord::{rtp::Codec, stream_connection::Pool};

use crate::{
    encode::{Configuration, Frame},
    gpu,
};

/// An encoder NVENC owns, and a source of frames for the tests.
#[repr(C)]
struct Raw {
    _private: [u8; 0],
}

#[repr(C)]
struct RawSource {
    _private: [u8; 0],
}

/// The two shared pictures and the fence they are drawn behind, as
/// native/d3d11enc.c keeps them.
#[repr(C)]
struct RawShared {
    _private: [u8; 0],
}

// SAFETY: the shim's own declarations; every pointer passed is one it returned,
// and the buffers are as large as their sizes say.
unsafe extern "C" {
    #[allow(clippy::too_many_arguments)]
    fn yapfast_d3d11enc_new(
        codec: c_uint,
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        luid_low: c_uint,
        luid_high: c_int,
        tuning: *const c_char,
        report: *mut c_char,
        report_size: usize,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_d3d11enc_new_on(
        device: *mut c_void,
        codec: c_uint,
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        surfaces: c_uint,
        tuning: *const c_char,
        report: *mut c_char,
        report_size: usize,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_d3d11enc_encode(
        encoder: *mut Raw,
        texture: *mut c_void,
        subresource: c_uint,
        pts: c_longlong,
        keyframe: c_int,
        out: *mut c_uchar,
        capacity: usize,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_d3d11enc_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_d3d11enc_scale(
        encoder: *mut Raw,
        texture: *mut c_void,
        subresource: c_uint,
        width: c_uint,
        height: c_uint,
        upscale: c_int,
    ) -> *mut c_void;
    fn yapfast_d3d11enc_readback(
        encoder: *mut Raw,
        texture: *mut c_void,
        subresource: c_uint,
        width: c_uint,
        height: c_uint,
        pixels: *mut c_void,
        size: usize,
    ) -> c_long;
    fn yapfast_d3d11enc_filter(encoder: *mut Raw, filter: *mut c_char, filter_size: usize);
    fn yapfast_d3d11enc_scaled_textures(encoder: *mut Raw) -> c_uint;
    fn yapfast_d3d11enc_staging_textures(encoder: *mut Raw) -> c_uint;
    fn yapfast_d3d11enc_asked(encoder: *mut Raw, asked: *mut c_char, asked_size: usize);
    fn yapfast_d3d11enc_config(encoder: *mut Raw, config: *mut c_char, config_size: usize);
    fn yapfast_d3d11enc_tuning(encoder: *mut Raw, tuning: *mut c_char, tuning_size: usize);
    fn yapfast_d3d11enc_codecs(encoder: *mut Raw, codecs: *mut c_char, codecs_size: usize);
    fn yapfast_d3d11enc_error(encoder: *mut Raw, error: *mut c_char, error_size: usize);
    fn yapfast_d3d11enc_device(encoder: *mut Raw) -> *mut c_void;
    fn yapfast_d3d11enc_copied(encoder: *mut Raw) -> c_ulonglong;
    fn yapfast_d3d11enc_free(encoder: *mut Raw);
    #[allow(clippy::too_many_arguments)]
    fn yapfast_d3d11enc_shared_new(
        encoder: *mut Raw,
        width: c_uint,
        height: c_uint,
        process: c_uint,
        fence: *mut c_ulonglong,
        luid: *mut c_ulonglong,
        why: *mut c_char,
        why_size: usize,
    ) -> *mut RawShared;
    fn yapfast_d3d11enc_shared_draw(
        encoder: *mut Raw,
        shared: *mut RawShared,
        at: c_uint,
        texture: *mut c_void,
        subresource: c_uint,
        handle: *mut c_ulonglong,
        value: *mut c_ulonglong,
    ) -> c_int;
    fn yapfast_d3d11enc_shared_picture(shared: *mut RawShared) -> *mut c_void;
    fn yapfast_d3d11enc_shared_free(shared: *mut RawShared);
    fn yapfast_d3d11enc_source_new(
        encoder: *mut Raw,
        frames: c_uint,
        why: *mut c_char,
        why_size: usize,
    ) -> *mut RawSource;
    fn yapfast_d3d11enc_source_draw(source: *mut RawSource, frame: c_uint);
    fn yapfast_d3d11enc_source_texture(source: *mut RawSource) -> *mut c_void;
    fn yapfast_d3d11enc_source_quads(
        source: *mut RawSource,
        why: *mut c_char,
        why_size: usize,
    ) -> c_int;
    fn yapfast_d3d11enc_source_free(source: *mut RawSource);
}

/// DXGI's vendor id for NVIDIA, whose cards are the only ones carrying NVENC,
/// the only encoder this backend opens. `gpu` ranks adapters by the same number,
/// privately, and this copy is what decides whether the adapter the setting
/// chose can encode here at all.
const NVIDIA: u32 = 0x10DE;

/// An adapter as DXGI describes it: the facts [`gpu::choose`] ranks, and the
/// name its driver gives it, which a failure names it by and [`gpu::Adapter`]
/// has no field for.
struct Named {
    facts: gpu::Adapter,
    name: String,
}

/// The adapters this machine presents, in DXGI's order, which starts with the
/// one the desktop is shown on.
///
/// Enumerated when a device is made and not kept: it costs a few microseconds
/// once per open, where a list kept against a card arriving or a display being
/// plugged in would be stale in the one moment it mattered, and it costs nothing
/// per frame either way.
fn adapters() -> Result<Vec<Named>> {
    // SAFETY: DXGI is asked for its factory and then for each adapter by index,
    // and every adapter it hands back is released when it goes out of scope.
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(|status| anyhow!("no DXGI factory: {status}"))?;
    let mut found = Vec::new();
    let mut index = 0;
    // Past the last adapter EnumAdapters1 answers DXGI_ERROR_NOT_FOUND, which is
    // the end of the machine's list rather than a failure of this open.
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(index) } {
        index += 1;
        // An adapter whose description can't be read is one no LUID can be
        // matched against, so it is left out rather than ranked on guesses.
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        found.push(Named {
            facts: gpu::Adapter {
                luid: gpu::Luid {
                    low: desc.AdapterLuid.LowPart,
                    high: desc.AdapterLuid.HighPart,
                },
                vendor: desc.VendorId,
                dedicated_memory: desc.DedicatedVideoMemory as u64,
                shared_memory: desc.SharedSystemMemory as u64,
                software: (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0,
            },
            name: wide(&desc.Description),
        });
    }
    Ok(found)
}

/// The adapter this encoder's device is made on, under `preference`: the
/// machine's adapters run through [`gpu::choose`], which is where the policy
/// lives, and the one it answers with handed back by name.
///
/// A chosen adapter that isn't NVIDIA's fails here, before a device or a session
/// is opened. NVENC exists only on NVIDIA's cards, so the GPU built into the
/// processor — what `Integrated` asks for — has no encoder this backend can
/// reach. Saying so in words leaves the caller free to encode another way, where
/// a device quietly made on the other adapter would drop the setting without
/// saying it had.
fn chosen(preference: gpu::Preference) -> Result<Named> {
    let mut machine = adapters()?;
    let Some(best) = gpu::choose(preference, machine.iter().map(|named| named.facts)) else {
        bail!(
            "no hardware adapter to make a device on: this machine has {}",
            listed(&machine)
        );
    };
    // choose answers with one of the adapters it was given, and one machine's
    // adapters have one LUID each, so this finds exactly the one it ranked.
    let index = machine
        .iter()
        .position(|named| named.facts.luid == best.luid)
        .expect("choose answers with an adapter it was given");
    if machine[index].facts.vendor != NVIDIA {
        bail!(
            "no NVENC on {}, the {preference:?} GPU chosen: only NVIDIA's cards carry one, so \
             this backend can't encode there and the stream needs another encoder; this machine \
             has {}",
            machine[index].name,
            listed(&machine)
        );
    }
    Ok(machine.swap_remove(index))
}

/// The machine's adapters as one sentence names them, for a failure that has to
/// say what this machine has rather than only what was wanted.
fn listed(machine: &[Named]) -> String {
    if machine.is_empty() {
        return "no adapters at all".to_owned();
    }
    machine
        .iter()
        .map(|named| format!("{} (0x{:04X})", named.name, named.facts.vendor))
        .collect::<Vec<_>>()
        .join(", ")
}

/// DXGI's description of an adapter as text: UTF-16 with a zero on the end, or a
/// full buffer with none when a driver filled every unit.
fn wide(text: &[u16]) -> String {
    let end = text
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}

/// A texture the encoder reads, and which of its subresources: the surface a
/// capture source has just filled, which stays the caller's. Its handle is an
/// `ID3D11Texture2D` on [`D3d11Encoder::device`]'s device, and the shim checks
/// that on the first frame it reads it, by asking the texture which device it is
/// on and comparing that pointer with its own: a texture from another device is
/// memory it can't read, and two devices made on one adapter are still two.
#[derive(Clone, Copy, Debug)]
pub struct Texture {
    /// The `ID3D11Texture2D` itself.
    pub handle: *mut c_void,
    /// Which subresource of it, for a texture array; 0 for an ordinary one.
    pub subresource: u32,
}

impl Texture {
    /// The whole of an ordinary texture.
    pub fn new(handle: *mut c_void) -> Self {
        Self {
            handle,
            subresource: 0,
        }
    }
}

/// The codec as native/d3d11enc.c numbers it, and the size checked before either
/// open is attempted: NVENC takes even dimensions only, and a codec this backend
/// has no number for is refused in words rather than by a status number.
fn codec_number(codec: Codec, width: u32, height: u32) -> Result<c_uint> {
    if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        bail!("{codec:?} frames need an even size, not {width}x{height}");
    }
    Ok(match codec {
        Codec::H264 => 0,
        Codec::H265 => 1,
        Codec::Av1 => 2,
        Codec::Vp8 => bail!("VP8 doesn't encode on the GPU; libvpx does it on the processor"),
    })
}

/// How far a [`D3d11Encoder::scale`] may resize a picture, since a stream's size
/// is fixed and a capture's is whatever the screen is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resize {
    /// Only to a size no larger than the source's: what a display wider than the
    /// stream needs, and the case the scaler exists for.
    Shrink,
    /// To any size, a larger one included: a small window that is the whole of
    /// what a viewer watches, shown at the stream's size.
    Either,
}

/// How many of a capture's surfaces a session keeps registered and mapped
/// between frames, which is memory the capture pays for twice over: every
/// surface a session holds is a full-size one it has taken a reference of, so
/// the capture can't reuse it while the session keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surfaces {
    /// The ring a capture source hands back, kept so that a frame costs an
    /// encode and no registration: what a session encoding a share wants.
    Ring,
    /// One, for a session that reads a frame in ten to show a preview: it has
    /// no ring to keep up with, and at 1080p each of the four a ring costs is
    /// another eight megabytes the capture can't fill.
    One,
}

impl Surfaces {
    /// The number of the capture's surfaces a session keeps for it, which the
    /// shim holds to as many as it has room for.
    fn held(self) -> c_uint {
        match self {
            Self::Ring => 4,
            Self::One => 1,
        }
    }
}

/// An encoder on the GPU reading D3D11 textures: NVENC through a D3D11 device,
/// one codec to an instance, opened with the tuning the stream ships.
pub struct D3d11Encoder {
    raw: *mut Raw,
    codec: Codec,
    width: u32,
    height: u32,
    kbps: u32,
    /// Which codecs the driver offered this session, as it named them, so a
    /// stream says what its card refused rather than quietly offering less.
    codecs: String,
    configuration: Configuration,
    /// The buffers encoded frames are written into, reused as frames are sent.
    out: Arc<Pool>,
}

// SAFETY: no library keeps thread-local state past a call; one thread uses the
// encoder at a time, as the D3D11 device requires of its immediate context.
unsafe impl Send for D3d11Encoder {}

impl D3d11Encoder {
    /// An encoder for `width` by `height` frames, which must be even, at `fps`
    /// and `kbps`, for one codec: the compositor's size, and the codec Discord
    /// chose. H264, H265 and AV1 all encode here, where the card has them; AV1
    /// needs an RTX 40 series or newer, and a driver without one is refused by
    /// name rather than by status number.
    ///
    /// This is the open for a caller with no device to offer, and it makes one on
    /// the adapter the GPU setting chose. A caller that already has one — a
    /// capture source's, so that the texture it fills is a texture this reads —
    /// uses [`Self::open_on`] instead.
    pub fn new(codec: Codec, width: u32, height: u32, fps: u32, kbps: u32) -> Result<Self> {
        let number = codec_number(codec, width, height)?;
        // Which adapter the device is made on is decided here, once per open:
        // the shim is handed that adapter's LUID and finds exactly it, so the
        // policy lives in Rust and the device in C.
        let preference = gpu::preference();
        let picked = chosen(preference)?;
        let mut report = [0u8; 192];
        // SAFETY: plain values, and a buffer of the size given, which the shim
        // ends with a zero. No tuning named asks for the shipped one, which is
        // what a stream that says nothing about its quality is given.
        let raw = unsafe {
            yapfast_d3d11enc_new(
                number,
                width,
                height,
                fps,
                kbps,
                picked.facts.luid.low,
                picked.facts.luid.high,
                std::ptr::null(),
                report.as_mut_ptr().cast(),
                report.len(),
            )
        };
        if raw.is_null() {
            let report = c_text(&report);
            bail!("no NVENC encoder opened: {report}");
        }
        let (encoder, config) = Self::adopt(raw, codec, width, height, kbps, &report);
        // What it runs on, what it took, and the settings a change to the
        // picture's quality moves, said once at the open so a stream is
        // compared with the encoder that made it rather than with what it was
        // meant to be.
        tracing::debug!(
            engine = encoder.configuration.engine.as_str(),
            ?codec,
            ?preference,
            adapter = picked.name.as_str(),
            tuning = encoder.configuration.tuning.as_str(),
            config = config.as_str(),
            codecs = encoder.codecs.as_str(),
            "encoding on the GPU from D3D11 textures"
        );
        Ok(encoder)
    }

    /// An encoder for `codec` on `device`, a D3D11 device that already exists —
    /// in a stream, the one a capture source is filling textures on — so that a
    /// captured frame is a texture this session reads with nothing copied in
    /// between. Everything [`Self::new`] promises holds here too: the same codec
    /// and size checks, the same tuning and settings, the same keyframe and
    /// bitrate contract, and the same report of what opened.
    ///
    /// `device` is an `ID3D11Device` somebody has already made, and it is
    /// **borrowed**: this does not AddRef it and dropping the encoder does not
    /// release it, so the caller keeps it alive for at least as long as the
    /// encoder and releases its own reference afterwards. That is what makes a
    /// stream cost one device per capture rather than one per capture and one
    /// per encoder. It has to sit on an NVIDIA card, which the open refuses by
    /// name where it doesn't.
    ///
    /// A device from a capture source is [`crate::wgc::Capture::device`] or
    /// [`crate::dup::Capture::device`]; the adapter preference of
    /// [`Self::new`] doesn't apply, since the device is already made and the
    /// adapter it is on is the answer.
    ///
    /// `surfaces` is how many of the capture's surfaces the session holds, and
    /// so pins: [`Surfaces::Ring`] for a session encoding the share, which reads
    /// the ring the capture fills, and [`Surfaces::One`] for one that reads a
    /// frame in ten to show a preview.
    ///
    // `device` is an opaque handle in the same sense [`Texture`] is: nothing in
    // this crate reads through it, it is handed to the shim, and the shim holds
    // it for the session's length without ever taking a reference of its own.
    // The obligation a caller carries is the one above — keep it alive — which is
    // a lifetime this signature can't name and a stream keeps by owning the
    // capture beside the encoder.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn open_on(
        device: *mut c_void,
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        surfaces: Surfaces,
    ) -> Result<Self> {
        let number = codec_number(codec, width, height)?;
        let mut report = [0u8; 192];
        // SAFETY: `device` is an `ID3D11Device` the caller keeps alive for the
        // length of this call and beyond, and `report` is a buffer of the size
        // given, which the shim ends with a zero.
        let raw = unsafe {
            yapfast_d3d11enc_new_on(
                device,
                number,
                width,
                height,
                fps,
                kbps,
                surfaces.held(),
                std::ptr::null(),
                report.as_mut_ptr().cast(),
                report.len(),
            )
        };
        if raw.is_null() {
            let report = c_text(&report);
            bail!("no NVENC encoder opened on that device: {report}");
        }
        let (encoder, config) = Self::adopt(raw, codec, width, height, kbps, &report);
        tracing::debug!(
            engine = encoder.configuration.engine.as_str(),
            ?codec,
            ?surfaces,
            tuning = encoder.configuration.tuning.as_str(),
            config = config.as_str(),
            codecs = encoder.codecs.as_str(),
            "encoding on the GPU from D3D11 textures on a device the caller brought"
        );
        Ok(encoder)
    }

    /// The encoder behind `raw`, with what it says about itself read back: the
    /// tail both opens share, so that a session on a device of its own and one
    /// on a device the caller brought answer [`Self::configuration`] and
    /// [`Self::codecs`] identically. Also answered is the sentence the shim
    /// says the session is configured with, which is the open's own log line and
    /// nothing a stream reads again.
    fn adopt(
        raw: *mut Raw,
        codec: Codec,
        width: u32,
        height: u32,
        kbps: u32,
        report: &[u8; 192],
    ) -> (Self, String) {
        // SAFETY: raw is an open encoder, and each buffer is as large as its
        // size; every one of these ends what it writes with a zero.
        let (asked, tuning, config, codecs) = unsafe {
            let mut asked = [0u8; 32];
            let mut tuning = [0u8; 96];
            let mut config = [0u8; 128];
            let mut codecs = [0u8; 64];
            yapfast_d3d11enc_asked(raw, asked.as_mut_ptr().cast(), asked.len());
            yapfast_d3d11enc_tuning(raw, tuning.as_mut_ptr().cast(), tuning.len());
            yapfast_d3d11enc_config(raw, config.as_mut_ptr().cast(), config.len());
            yapfast_d3d11enc_codecs(raw, codecs.as_mut_ptr().cast(), codecs.len());
            (
                c_text(&asked),
                c_text(&tuning),
                c_text(&config),
                c_text(&codecs),
            )
        };
        let pixels = (width * height) as usize;
        let encoder = Self {
            raw,
            codec,
            width,
            height,
            kbps,
            codecs,
            configuration: Configuration {
                engine: c_text(report),
                asked,
                tuning,
                // Every setting an NVENC session is opened with here is one the
                // stream needs, so a driver that won't take one fails the open
                // instead of dropping it: there is nothing to warn about later.
                settings_dropped: String::new(),
            },
            out: Arc::new(Pool::new(pixels * 2 + 4096)),
        };
        (encoder, config)
    }

    /// Which encoder runs, such as "h264_nvenc/d3d11": the codec's FFmpeg name
    /// and the backend that reads its frames, so a log says which of the two GPU
    /// paths a stream got.
    pub fn engine(&self) -> &str {
        &self.configuration.engine
    }

    /// What this encoder was opened with, for a log or a measurement to quote
    /// beside its numbers.
    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    /// The codecs this session's driver offered, such as "h264 hevc av1", and
    /// empty when it wouldn't say: what the stream can offer its viewers, read
    /// from the card that would encode them.
    pub fn codecs(&self) -> &str {
        &self.codecs
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The bitrate aimed for, in kbit/s.
    pub fn bitrate(&self) -> u32 {
        self.kbps
    }

    /// The D3D11 device the textures have to live on, as an `ID3D11Device`: the
    /// one this encoder made, or the one it was opened on with
    /// [`Self::open_on`]. A capture source opens on it, or is opened on by it —
    /// [`crate::wgc::Capture::open_on`] and [`crate::dup::Capture::open_on`] —
    /// and one that does neither hands over textures from another device, which
    /// the shim refuses on the first frame rather than encoding the wrong
    /// memory.
    ///
    /// It belongs to this encoder when the encoder made it and to whoever made
    /// it otherwise; neither a caller nor a capture may release it.
    pub fn device(&self) -> *mut c_void {
        // SAFETY: raw is an open encoder.
        unsafe { yapfast_d3d11enc_device(self.raw) }
    }

    /// How many frames' pixels the processor has moved since this encoder
    /// opened: zero, for as long as the picture goes to the encoder on the GPU.
    /// A capture source that started converting frames would say so here without
    /// any frame looking different.
    pub fn copied(&self) -> u64 {
        // SAFETY: raw is an open encoder.
        unsafe { yapfast_d3d11enc_copied(self.raw) as u64 }
    }

    /// Aims for `kbps` from the next frame on, without a keyframe: the session is
    /// reconfigured, so the frames a viewer is watching stay references and the
    /// picture doesn't blink.
    pub fn set_bitrate(&mut self, kbps: u32) -> Result<()> {
        // SAFETY: raw is an open encoder.
        if unsafe { yapfast_d3d11enc_set_bitrate(self.raw, kbps) } != 0 {
            bail!(
                "the {:?} encoder refused {kbps} kbit/s: {}",
                self.codec,
                self.error()
            );
        }
        self.kbps = kbps;
        Ok(())
    }
    /// Scales `texture` on the GPU to `width` by `height`, as far as `resize`
    /// allows, and answers the texture to encode: it is in the size asked for, on
    /// this encoder's device, and the encoder reads it exactly as it reads a
    /// capture's own. Nothing of the picture crosses the processor — the driver's
    /// video processor scales it where it takes these formats and a shader does
    /// where it doesn't, and [`Self::filter`] says which.
    ///
    /// The texture belongs to the encoder and is reused, so it is the same
    /// texture, and the same picture, until a scale to another size replaces it:
    /// encode it before scaling again. A scale to the source's own size is a
    /// copy, which is exact.
    pub fn scale(
        &mut self,
        texture: Texture,
        width: u32,
        height: u32,
        resize: Resize,
    ) -> Result<Texture> {
        // SAFETY: raw is an open encoder, and the texture is the caller's and
        // lives as long as the call; the picture answered with belongs to the
        // encoder and is only read.
        let scaled = unsafe {
            yapfast_d3d11enc_scale(
                self.raw,
                texture.handle,
                texture.subresource,
                width,
                height,
                c_int::from(resize == Resize::Either),
            )
        };
        if scaled.is_null() {
            bail!(
                "{:?} couldn't scale a picture to {width}x{height}: {}",
                self.codec,
                self.error()
            );
        }
        Ok(Texture::new(scaled))
    }
    /// Reads `texture` back at `width` by `height` as BGRA — `blue`, `green`,
    /// `red`, `alpha`, one row after another with no padding — into `pixels`,
    /// and answers the bytes it wrote, which is the tile's `width * height * 4`.
    ///
    /// This is a preview's tile rather than a whole frame: the picture is scaled
    /// on the GPU into the texture [`Self::scale`] uses, copied there into one
    /// staging texture this encoder keeps, and mapped, so what crosses the
    /// processor is the tile and nothing of it is allocated per call. A readback
    /// is a smaller look at a picture and never a stretched one — scaling up is
    /// [`Self::scale`]'s business, not a preview's — so a larger size is refused
    /// rather than answered.
    pub fn readback(
        &mut self,
        texture: Texture,
        width: u32,
        height: u32,
        pixels: &mut [u8],
    ) -> Result<usize> {
        // SAFETY: raw is an open encoder, the texture is the caller's and lives as
        // long as the call, and the buffer is as large as its length and stays put
        // during it.
        let written = unsafe {
            yapfast_d3d11enc_readback(
                self.raw,
                texture.handle,
                texture.subresource,
                width,
                height,
                pixels.as_mut_ptr().cast(),
                pixels.len(),
            )
        };
        match written {
            ..0 => bail!(
                "{:?} couldn't read {width}x{height} back: {}",
                self.codec,
                self.error()
            ),
            size => Ok(size as usize),
        }
    }
    /// Which filter scaled the last picture, in words: "the video processor",
    /// "a pixel shader", or "a copy, the sizes being the same". Empty until
    /// something is scaled, and worth a line in a log beside the numbers, since a
    /// driver that declines the processor scales in a shader instead.
    pub fn filter(&self) -> String {
        let mut filter = [0u8; 96];
        // SAFETY: raw is an open encoder, and a buffer of the size given.
        unsafe { yapfast_d3d11enc_filter(self.raw, filter.as_mut_ptr().cast(), filter.len()) };
        c_text(&filter)
    }
    /// How many textures the scaler has made to scale pictures into: one, however
    /// many frames went through it, and one more for each new size a capture asks
    /// for. A count that climbs a frame at a time is a size that keeps changing.
    pub fn scaled_textures(&self) -> u32 {
        // SAFETY: raw is an open encoder.
        unsafe { yapfast_d3d11enc_scaled_textures(self.raw) }
    }
    /// How many staging textures a readback has made: one for the size a preview
    /// asks for, however many tiles were read through it.
    pub fn staging_textures(&self) -> u32 {
        // SAFETY: raw is an open encoder.
        unsafe { yapfast_d3d11enc_staging_textures(self.raw) }
    }

    /// Encodes `texture`'s picture, shown at `pts` in [`CLOCK`](
    /// crate::encode::CLOCK) units. None when the encoder made no packet. A frame
    /// too big for the buffer grows the pool's buffers, doubling until one holds
    /// it, and is encoded again at the same time.
    pub fn encode_texture(
        &mut self,
        texture: Texture,
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame>> {
        let mut out = self.out.checkout();
        loop {
            let mut is_key: c_int = 0;
            let spare = out.spare();
            // SAFETY: the texture is the caller's and lives as long as the call,
            // the buffer is as large as its size, and the flag has a place of its
            // own; all stay put during the call.
            let written = unsafe {
                yapfast_d3d11enc_encode(
                    self.raw,
                    texture.handle,
                    texture.subresource,
                    pts as c_longlong,
                    c_int::from(keyframe),
                    spare.as_mut_ptr(),
                    spare.len(),
                    &mut is_key,
                )
            };
            match written {
                // -2 is the shim's "won't fit", as native/hwenc.c says it.
                -2 => {
                    if !self.out.grow() {
                        bail!(
                            "the {:?} frame doesn't fit {} bytes",
                            self.codec,
                            self.out.capacity()
                        );
                    }
                    out = self.out.checkout();
                }
                ..0 => bail!("{:?} encoding failed: {}", self.codec, self.error()),
                0 => return Ok(None),
                size => {
                    out.finish(size as usize);
                    return Ok(Some(Frame {
                        data: out,
                        keyframe: is_key != 0,
                        // The frame was encoded at this time, whether it took one
                        // call or, grown, two.
                        timestamp: pts as u32,
                    }));
                }
            }
        }
    }

    /// Draws frames for this encoder to read — a stand-in for the capture
    /// source, which doesn't make textures yet. `frames` is how many pictures it
    /// can draw before repeating one, and it holds a texture on the encoder's
    /// device for as long as it lives.
    pub fn source(&self, frames: u32) -> Result<Source> {
        let mut why = [0u8; 192];
        // SAFETY: raw is an open encoder, and a buffer of the size given, which
        // the shim ends with a zero.
        let raw = unsafe {
            yapfast_d3d11enc_source_new(self.raw, frames, why.as_mut_ptr().cast(), why.len())
        };
        if raw.is_null() {
            bail!("no texture to draw on: {}", c_text(&why));
        }
        Ok(Source { raw })
    }

    /// What the driver said about the frame it wouldn't encode.
    fn error(&self) -> String {
        let mut error = [0u8; 192];
        // SAFETY: raw is an open encoder, and a buffer of the size given.
        unsafe { yapfast_d3d11enc_error(self.raw, error.as_mut_ptr().cast(), error.len()) };
        c_text(&error)
    }
}

impl Drop for D3d11Encoder {
    fn drop(&mut self) {
        // SAFETY: raw came from the shim's constructor and is freed once.
        unsafe { yapfast_d3d11enc_free(self.raw) };
    }
}

/// A picture drawn on the card for the UI's window to show where it lies: two
/// tile-sized BGRA textures and one fence on an encoder's device, sharing the
/// handles of all three into the UI's process.
///
/// The UI's window draws on a Direct3D 12 device of its own and opens the handle
/// of the texture a picture was published under, so what crosses the process
/// boundary is a handle and two numbers rather than the tile's pixels. Two
/// textures alternate, so the one the UI is showing is never the one being
/// drawn, and the fence value that comes back with each picture is what says it
/// is drawn: see [`crate::frames::Drawn`], which is what a caller publishes.
///
/// It holds textures on its encoder's device, so it goes before the encoder it
/// draws for, and it is worth nothing without the UI's process being alive:
/// every failure here is a stream that writes pixels instead.
pub struct Shared {
    raw: *mut RawShared,
    size: (u32, u32),
    luid: u64,
    fence: u64,
}

// SAFETY: the two textures and the fence are the device's, and every call that
// draws takes the share by `&mut self` while the handle shared out is a number.
unsafe impl Send for Shared {}

impl Shared {
    /// Two pictures of `width` by `height` and one fence on `tile`'s device, with
    /// the handles of all three duplicated into `ui`, the process that opens
    /// them. Answers why not where the card or the UI's process won't have them:
    /// a caller that gets an error writes the tile's pixels as it always did.
    pub fn new(tile: &D3d11Encoder, width: u32, height: u32, ui: u32) -> Result<Self> {
        let (mut fence, mut luid) = (0, 0);
        let mut why = [0u8; 192];
        // SAFETY: raw is an open encoder, and the out-pointers are two locals and
        // a buffer of the size given.
        let raw = unsafe {
            yapfast_d3d11enc_shared_new(
                tile.raw,
                width,
                height,
                ui,
                &mut fence,
                &mut luid,
                why.as_mut_ptr().cast(),
                why.len(),
            )
        };
        if raw.is_null() {
            bail!(
                "a {width}x{height} picture couldn't be shared with the UI: {}",
                c_text(&why)
            );
        }
        Ok(Self {
            raw,
            size: (width, height),
            luid,
            fence,
        })
    }

    /// The size of each picture, which is the size it is drawn at and the size
    /// the UI opens it at.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The adapter the pictures were made on, as the UI's window compares its
    /// own device against.
    pub fn luid(&self) -> u64 {
        self.luid
    }

    /// The fence's handle as the UI's process holds it: one fence for a stream's
    /// pictures, waited on by the value each of them is published with.
    pub fn fence(&self) -> u64 {
        self.fence
    }

    /// Scales `texture` into shared picture `at` on the GPU, in one pass
    /// through the driver's video processor or its shader, and answers that
    /// picture's handle as the UI's process holds it with the fence value that
    /// says it is drawn.
    ///
    /// The caller names the picture because the frames file's slot decides
    /// which one may be drawn into: a slot is only drawn into once its reader
    /// has moved past the picture there, so a picture a person is looking at
    /// is never written over.
    ///
    /// The value is signalled after the scale is queued and nothing here waits
    /// for it: the UI does, and so does the next draw into the same picture.
    pub fn draw(
        &mut self,
        at: u32,
        tile: &mut D3d11Encoder,
        texture: Texture,
    ) -> Result<(u64, u64)> {
        let (mut handle, mut value) = (0, 0);
        // SAFETY: raw and the encoder are both open, the texture is the caller's
        // and outlives the call, and the out-pointers are two locals.
        let drawn = unsafe {
            yapfast_d3d11enc_shared_draw(
                tile.raw,
                self.raw,
                at,
                texture.handle,
                texture.subresource,
                &mut handle,
                &mut value,
            )
        };
        if drawn <= 0 {
            bail!(
                "{:?} couldn't draw a {}x{} picture for the UI: {}",
                tile.codec,
                self.size.0,
                self.size.1,
                tile.error()
            );
        }
        Ok((handle, value))
    }

    /// The picture the last draw wrote, as a texture on the encoder's device:
    /// what a test reads back to see the tile the UI would show, and what the
    /// stream never needs, since it publishes the handle instead.
    pub fn picture(&self) -> Texture {
        // SAFETY: raw is an open share, and the texture it names lives as long as
        // it does.
        Texture::new(unsafe { yapfast_d3d11enc_shared_picture(self.raw) })
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        // SAFETY: raw came from the shim's constructor and is freed once. The
        // handles duplicated into the UI's process are that process's, and go
        // when it closes them.
        unsafe { yapfast_d3d11enc_shared_free(self.raw) };
    }
}

/// The frames a [`D3d11Encoder`] reads before a capture source fills its
/// textures: one BGRA texture on the encoder's device, which a compute shader
/// refills there, so nothing of a frame crosses the processor and a measurement
/// of the encode is the encode's own cost.
///
/// It holds a texture on its encoder's device, so it goes before the encoder it
/// draws for: a texture whose device has been released can't be read.
pub struct Source {
    raw: *mut RawSource,
}

impl Source {
    /// Refills the texture on the GPU with the picture `frame` shows.
    pub fn draw(&self, frame: u32) {
        // SAFETY: raw is a source that hasn't been freed, and the frame index is
        // plain data.
        unsafe { yapfast_d3d11enc_source_draw(self.raw, frame) };
    }

    /// The texture to encode next, which the source keeps and refills.
    pub fn texture(&self) -> Texture {
        // SAFETY: raw is a source that hasn't been freed, and its texture lives
        // as long as it does.
        Texture::new(unsafe { yapfast_d3d11enc_source_texture(self.raw) })
    }
    /// Draws four solid quadrants instead of the moving picture: one known colour
    /// in each corner of the texture, which is the picture a scaler is held to.
    pub fn quads(&self) -> Result<()> {
        let mut why = [0u8; 192];
        // SAFETY: raw is a source that hasn't been freed, and a buffer of the size
        // given, which the shim ends with a zero.
        let drew =
            unsafe { yapfast_d3d11enc_source_quads(self.raw, why.as_mut_ptr().cast(), why.len()) };
        if drew != 0 {
            bail!("no quadrants to draw: {}", c_text(&why));
        }
        Ok(())
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        // SAFETY: raw came from the shim's constructor and is freed once.
        unsafe { yapfast_d3d11enc_source_free(self.raw) };
    }
}

/// The text the shim wrote into `bytes`, which ends with a zero.
fn c_text(bytes: &[u8]) -> String {
    std::ffi::CStr::from_bytes_until_nul(bytes).map_or_else(
        |_| String::new(),
        |text| text.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // What the shim measures for a test that asks it, and nothing a stream
    // runs: the devices this process has opened, so a second frame can be held
    // to the first's session, and the processor time an encode costs, which a
    // wall clock would charge the driver's wait on the GPU to.
    unsafe extern "C" {
        fn yapfast_d3d11enc_devices_created() -> c_uint;
        fn yapfast_d3d11enc_cpu_time() -> c_ulonglong;
    }
    /// One GPU test at a time: the encoder counts the devices this process has
    /// opened, so a test held to its own count has to be the only one opening
    /// devices while it counts. Taken for a test's whole body, and taken again
    /// after a panic in another test rather than poisoning every test after it.
    static ONE_GPU_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn one_gpu_test_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        ONE_GPU_TEST
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // The settings native/d3d11enc.c opens NVENC with for the tuning named, and
    // whether the stream can't do without each: the list that open reads, so a
    // test holds it to what native/hwenc.c's own list says rather than to a copy
    // of either.
    #[cfg(hwenc)]
    unsafe extern "C" {
        fn yapfast_d3d11enc_tuning_settings(
            tuning: *const c_char,
            asked: *mut c_char,
            asked_size: usize,
        ) -> c_int;
        fn yapfast_hwenc_nvenc_settings(
            tuning: *const c_char,
            asked: *mut c_char,
            asked_size: usize,
        ) -> c_int;
    }

    /// The two GPU backends ask NVENC for the same thing: p4 at low-latency
    /// tuning, capped VBR, no frame allowed to wait and an IDR on demand, with the
    /// same settings marked as the ones the stream can't do without. A tuning that
    /// changed on one side and not the other would encode a picture nobody
    /// measured, and neither a stream nor a comparison would say which it got.
    #[cfg(hwenc)]
    #[test]
    fn both_gpu_backends_ask_nvenc_for_the_same_thing() {
        let mut d3d11 = [0u8; 192];
        let mut ffmpeg = [0u8; 192];
        // SAFETY: a name the row has, or none at all for the shipped one, and a
        // buffer of the size given on each side.
        let (ours, theirs) = unsafe {
            (
                yapfast_d3d11enc_tuning_settings(
                    std::ptr::null(),
                    d3d11.as_mut_ptr().cast(),
                    d3d11.len(),
                ),
                yapfast_hwenc_nvenc_settings(
                    std::ptr::null(),
                    ffmpeg.as_mut_ptr().cast(),
                    ffmpeg.len(),
                ),
            )
        };
        assert_eq!((ours, theirs), (0, 0));
        assert_eq!(
            c_text(&d3d11),
            c_text(&ffmpeg),
            "the two backends' settings"
        );
        // A tuning nobody has is refused rather than quietly replaced by this
        // one, on both sides of the comparison.
        let mut none = [0u8; 192];
        // SAFETY: as above, with a name no row has.
        let refused = unsafe {
            yapfast_d3d11enc_tuning_settings(c"p7".as_ptr(), none.as_mut_ptr().cast(), none.len())
        };
        assert_eq!(refused, -1);
    }

    /// The setting chooses between the adapters this machine really presents,
    /// and the adapter it can't encode on says so rather than letting a stream
    /// find out from a driver's status number.
    ///
    /// `Integrated` is the case that matters: on a machine whose processor has
    /// graphics the device would be made on Intel's, where NVENC isn't, so the
    /// open has to fail naming that adapter. A machine without one falls back to
    /// its card, as it always did. What was enumerated is printed either way, so
    /// a run says which machine it checked.
    #[test]
    fn the_setting_picks_an_adapter_and_nvenc_only_exists_on_a_card() {
        let machine = adapters().expect("DXGI enumerates this machine's adapters");
        eprintln!("adapters: {}", listed(&machine));
        // What `Integrated` would be made a device on, read with the same policy
        // the choice uses rather than by repeating Intel's vendor id here.
        let integrated = machine.iter().find(|named| {
            gpu::Preference::Integrated.rank(&named.facts) == Some(gpu::Rank::Preferred)
        });
        match chosen(gpu::Preference::Dedicated) {
            Ok(picked) => assert_eq!(picked.facts.vendor, NVIDIA, "{} isn't a card", picked.name),
            Err(error) => {
                // No NVIDIA card here, or none that ranked above the rest: that
                // is a machine this backend has never encoded on.
                eprintln!("dedicated: {error}");
            }
        }
        match chosen(gpu::Preference::Integrated) {
            Ok(picked) => assert!(
                integrated.is_none(),
                "Integrated chose {} and this machine's integrated GPU can't encode",
                picked.name
            ),
            Err(error) => {
                let said = error.to_string();
                eprintln!("integrated: {said}");
                if let Some(named) = integrated {
                    assert!(said.contains("no NVENC"), "{said}");
                    assert!(
                        said.contains(&named.name),
                        "the failure doesn't name {}: {said}",
                        named.name
                    );
                }
            }
        }
    }

    /// Thirty frames of each codec NVENC has here, read from a texture the GPU
    /// fills: every frame is an access unit, the first is a keyframe and the rest
    /// follow it, and the session encodes all thirty — one device, one session, no
    /// pixel of any frame touched by the processor. The first frames are read as
    /// the codec's own structure, since that is what the packetizer and every
    /// viewer's client depend on and the one thing a wrong driver call would break
    /// without looking broken.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn thirty_frames_of_each_codec_encode_from_a_texture() {
        let _gpu = one_gpu_test_at_a_time();
        // 1080p at 60, the shape a stream of a game is, so the numbers this prints
        // are the ones a stream would see.
        let (width, height, fps, kbps) = (1920u32, 1080u32, 60u32, 8000u32);
        let before = unsafe { yapfast_d3d11enc_devices_created() };
        let mut opened = 0;
        for codec in [Codec::H264, Codec::H265, Codec::Av1] {
            let mut encoder = match D3d11Encoder::new(codec, width, height, fps, kbps) {
                Ok(encoder) => encoder,
                Err(error) => {
                    // A card without the codec is not a failure of this backend:
                    // AV1 needs an RTX 40 series or newer, and the open says which
                    // codecs the driver has.
                    eprintln!("{codec:?} didn't open here: {error:#}");
                    continue;
                }
            };
            opened += 1;
            eprintln!(
                "{codec:?}: {} ({}), {} fps, driver has {}",
                encoder.engine(),
                encoder.configuration().tuning,
                fps,
                encoder.codecs()
            );
            let device = encoder.device();
            assert!(!device.is_null());
            let source = encoder.source(60).unwrap();
            let texture = source.texture();
            assert!(!texture.handle.is_null());

            // The frames' processor time alone: the driver waits on the GPU inside
            // this call, and a wall clock would charge that to the processor.
            let mut frames = vec![];
            for frame in 0..30u32 {
                source.draw(frame);
                // Half a millisecond a frame at 60 fps, in 90 kHz units: one
                // stream's clock, the same one every other engine runs on.
                let pts = u64::from(frame) * 1500;
                if frame == 15 {
                    // A bitrate that moves mid-stream, without a keyframe: the
                    // network changed and the viewers keep their references.
                    encoder.set_bitrate(kbps / 2).unwrap();
                    assert_eq!(encoder.bitrate(), kbps / 2);
                }
                let encoded = encoder
                    .encode_texture(texture, pts, frame == 0)
                    .unwrap()
                    .unwrap_or_else(|| panic!("{codec:?} made no packet for frame {frame}"));
                assert!(
                    !encoded.data.is_empty(),
                    "frame {frame} of {codec:?} is an empty access unit"
                );
                assert_eq!(encoded.timestamp, pts as u32);
                if frame == 0 {
                    assert!(
                        encoded.keyframe,
                        "the first frame of {codec:?} isn't a keyframe"
                    );
                    // Its structure, as the packetizer reads it: parameter sets
                    // before the picture, in the form this codec's packets take.
                    match codec {
                        Codec::H264 => assert_eq!(
                            yapfast_discord::rtp::nal_units(&encoded.data)[0][0] & 0x1f,
                            7,
                            "an H.264 keyframe starts with its sequence parameter set"
                        ),
                        Codec::H265 => assert_eq!(
                            (yapfast_discord::rtp::nal_units(&encoded.data)[0][0] >> 1) & 0x3f,
                            32,
                            "an H.265 keyframe starts with its video parameter set"
                        ),
                        _ => assert_eq!(
                            (yapfast_discord::rtp::obu_elements(&encoded.data)
                                .expect("OBUs within the frame")[0][0]
                                >> 3)
                                & 0x0f,
                            1,
                            "an AV1 keyframe starts with its sequence header"
                        ),
                    }
                }
                frames.push(encoded);
            }
            let bytes: usize = frames.iter().map(|frame| frame.data.len()).sum();
            // Only the frame it was asked for is a keyframe: the interval is three
            // seconds, and these are half a second.
            assert_eq!(
                frames.iter().filter(|frame| frame.keyframe).count(),
                1,
                "a keyframe the stream wasn't asked for"
            );
            // What a frame costs the processor, over a run long enough to read:
            // the process's own counter moves in ticks of about 15 ms, so thirty
            // frames that take a quarter of one can only be timed by the score.
            // Three hundred of them are two and a half seconds of a stream, and
            // the same run the reference measurement used, so the number below is
            // comparable with it.
            const TIMED: u32 = 300;
            let cpu = unsafe { yapfast_d3d11enc_cpu_time() };
            for frame in 30..30 + TIMED {
                source.draw(frame);
                let encoded = encoder
                    .encode_texture(texture, u64::from(frame) * 1500, false)
                    .unwrap()
                    .unwrap_or_else(|| panic!("{codec:?} made no packet for frame {frame}"));
                assert!(
                    !encoded.data.is_empty(),
                    "frame {frame} of {codec:?} is empty"
                );
            }
            let cpu = unsafe { yapfast_d3d11enc_cpu_time() }.saturating_sub(cpu);
            eprintln!(
                "  {bytes} bytes in the first {}, {:.4} ms of the processor per frame",
                frames.len(),
                cpu as f64 / f64::from(TIMED) / 10_000.0
            );
            // The pixels went to the encoder on the GPU: a converted frame would
            // say so here without any frame looking wrong.
            assert_eq!(encoder.copied(), 0, "the processor moved a frame's pixels");
            // And the thirty frames took the session that was opened, not another
            // device or another encoder.
            assert_eq!(encoder.device(), device);
            assert_eq!(
                unsafe { yapfast_d3d11enc_devices_created() },
                before + opened,
                "an encode opened another D3D11 device"
            );
        }
        assert_eq!(opened, 3, "not every codec opened on this GPU");
    }
    /// The four quadrant colours as memory holds them: a BGRA texture's first
    /// byte is blue, and 0.2 and 0.8 of full scale are 51 and 204.
    const BLUE: [u8; 4] = [204, 51, 51, 255];
    const RED: [u8; 4] = [51, 51, 204, 255];
    const GREEN: [u8; 4] = [51, 204, 51, 255];
    const WHITE: [u8; 4] = [204, 204, 204, 255];
    /// Where each quadrant's own colour is read: each quarter's middle, which is
    /// nowhere near a seam, and the colour the picture has there.
    fn quarters(width: u32, height: u32) -> [(&'static str, u32, u32, [u8; 4]); 4] {
        [
            ("top left", width / 4, height / 4, BLUE),
            ("top right", width * 3 / 4, height / 4, RED),
            ("bottom left", width / 4, height * 3 / 4, GREEN),
            ("bottom right", width * 3 / 4, height * 3 / 4, WHITE),
        ]
    }
    /// The BGRA pixel at (`x`, `y`) of a picture `width` pixels wide.
    fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * width + x) * 4) as usize;
        pixels[at..at + 4].try_into().expect("four bytes a pixel")
    }
    /// Holds a scaled pixel to the colour its quarter had in the source: a solid
    /// colour through four taps and a bilinear filter loses a rounding or two,
    /// and a driver converting it on its way through loses a couple more, so four
    /// of 255 is the tolerance a colour is held to here.
    fn assert_solid(got: [u8; 4], colour: [u8; 4], quarter: &str) {
        for (channel, (got, had)) in ["blue", "green", "red", "alpha"]
            .into_iter()
            .zip(got.iter().zip(colour.iter()))
        {
            assert!(
                got.abs_diff(*had) <= 4,
                "{quarter}'s {channel} is {got} where the picture had {had}"
            );
        }
    }
    /// A known picture scaled down keeps its structure: four solid quadrants of a
    /// 1080p capture are four quadrants of a 480x270 picture, each in the corner
    /// it was in and each still one colour. That is the whole of what a display
    /// wider than 1080p needs to reach a 1080p stream, and a filter that mixed the
    /// quarters or lost one would look like a smeared screen rather than a wrong
    /// number.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn a_scaled_picture_keeps_its_four_quadrants() {
        let _gpu = one_gpu_test_at_a_time();
        let (width, height) = (480u32, 270u32);
        let mut encoder = D3d11Encoder::new(Codec::H264, 1920, 1080, 60, 8000)
            .expect("an NVENC encoder on this machine");
        let source = encoder.source(1).expect("a texture to draw on");
        source.quads().expect("the quadrants to draw");
        // Four times smaller on each side, as a 3840-wide screen is on its way to
        // a 1080p stream.
        let scaled = encoder
            .scale(source.texture(), width, height, Resize::Shrink)
            .expect("a picture scaled down four times");
        eprintln!("scaled by {}", encoder.filter());
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        assert_eq!(
            encoder
                .readback(scaled, width, height, &mut pixels)
                .expect("the scaled picture read back"),
            pixels.len()
        );
        for (quarter, x, y, colour) in quarters(width, height) {
            assert_solid(pixel(&pixels, width, x, y), colour, quarter);
        }
    }
    /// A picture drawn for the UI is a whole tile of the capture, drawn again and
    /// again without it being written over: the two draws answer different
    /// textures, so the one the UI would be showing is never the one the next
    /// frame fills, and climbing fence values, so a wait can tell one from the
    /// other. What the second one holds is the capture's own quarters, which is
    /// the picture the UI would put on screen.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn a_shared_picture_is_drawn_in_two_before_either_is_drawn_over() {
        let _gpu = one_gpu_test_at_a_time();
        let (width, height) = (480u32, 270u32);
        let mut encoder = D3d11Encoder::new(Codec::H264, 1920, 1080, 60, 8000)
            .expect("an NVENC encoder on this machine");
        let source = encoder.source(1).expect("a texture to draw on");
        source.quads().expect("the quadrants to draw");
        // This process, which DuplicateHandle answers as readily as another one:
        // what the UI does with the handles it is given is the UI's test, and a
        // handle here is a handle whether or not anybody else opens it.
        let mut shared = Shared::new(&encoder, width, height, std::process::id())
            .expect("a tile-sized picture to share");
        assert_eq!(shared.size(), (width, height));
        assert_ne!(
            shared.luid(),
            0,
            "the picture has no adapter to be shown on"
        );
        assert_ne!(shared.fence(), 0, "the picture has no fence to wait on");
        let (first, first_value) = shared
            .draw(0, &mut encoder, source.texture())
            .expect("the first picture drawn");
        let (second, second_value) = shared
            .draw(1, &mut encoder, source.texture())
            .expect("the second picture drawn");
        assert_ne!(
            first, second,
            "both pictures are one texture, which is one written under the UI"
        );
        assert!(
            second_value > first_value,
            "the fence went back, to {second_value} from {first_value}"
        );
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        assert_eq!(
            encoder
                .readback(shared.picture(), width, height, &mut pixels)
                .expect("the shared picture read back"),
            pixels.len()
        );
        for (quarter, x, y, colour) in quarters(width, height) {
            assert_solid(pixel(&pixels, width, x, y), colour, quarter);
        }
        eprintln!(
            "two {width}x{height} pictures at {} and {} behind a fence at {second_value}",
            first, second
        );
    }
    /// A readback at a picture's own size is that picture's pixels, exactly: the
    /// copy is a copy, with no scaler in it to move a value, so the two pixels
    /// either side of a quadrant's seam keep the colours the picture put there.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn a_readback_returns_the_textures_own_pixels() {
        let _gpu = one_gpu_test_at_a_time();
        let (width, height) = (1920u32, 1080u32);
        let mut encoder = D3d11Encoder::new(Codec::H264, width, height, 60, 8000)
            .expect("an NVENC encoder on this machine");
        let source = encoder.source(1).expect("a texture to draw on");
        source.quads().expect("the quadrants to draw");
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        assert_eq!(
            encoder
                .readback(source.texture(), width, height, &mut pixels)
                .expect("the picture read back at its own size"),
            pixels.len()
        );
        for (quarter, x, y, colour) in quarters(width, height) {
            assert_eq!(pixel(&pixels, width, x, y), colour, "{quarter}");
        }
        // The seam, on the row above the halves' own: the last column of the left
        // half is the left half's colour to the byte and the first of the right
        // half is the right half's, where a scale would have blended them.
        let middle = (width / 2, height / 2 - 1);
        assert_eq!(pixel(&pixels, width, middle.0 - 1, middle.1), BLUE);
        assert_eq!(pixel(&pixels, width, middle.0, middle.1), RED);
        eprintln!("{} bytes read back at the picture's own size", pixels.len());
    }
    /// A readback at a smaller size is genuinely smaller: the bytes it answers
    /// with, and the buffer they fit in, are the tile's and not the frame's. A
    /// preview that cost a whole frame's read per frame is what this replaces, and
    /// the tile still holds a picture of the whole screen rather than the corner
    /// of one.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn a_readback_at_a_smaller_size_is_genuinely_smaller() {
        let _gpu = one_gpu_test_at_a_time();
        let (width, height) = (1920u32, 1080u32);
        let (tile_width, tile_height) = (480u32, 270u32);
        let mut encoder = D3d11Encoder::new(Codec::H264, width, height, 60, 8000)
            .expect("an NVENC encoder on this machine");
        let source = encoder.source(1).expect("a texture to draw on");
        source.quads().expect("the quadrants to draw");
        let mut tile = vec![0u8; (tile_width * tile_height * 4) as usize];
        let read = encoder
            .readback(source.texture(), tile_width, tile_height, &mut tile)
            .expect("a tile of a 1080p picture");
        let frame = (width * height * 4) as usize;
        assert_eq!(read, tile.len(), "the bytes written are the buffer's");
        assert_eq!(read, frame / 16, "a sixteenth of the frame, by area");
        assert!(read < frame, "a tile is smaller than the frame it is of");
        for (quarter, x, y, colour) in quarters(tile_width, tile_height) {
            assert_solid(pixel(&tile, tile_width, x, y), colour, quarter);
        }
        eprintln!(
            "{read} bytes of a {frame}-byte frame, scaled by {}",
            encoder.filter()
        );
    }
    /// A hundred readbacks reuse the one scaled picture and the one staging
    /// texture: nothing is allocated per call and the hundredth tile is the first
    /// one's picture. Memory is a product goal here, so this is a count the
    /// encoder reports rather than a hope about the code — and a preview reads a
    /// tile thirty times a second for as long as somebody is watching.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn a_hundred_readbacks_reuse_their_textures() {
        let _gpu = one_gpu_test_at_a_time();
        let (tile_width, tile_height) = (480u32, 270u32);
        let mut encoder = D3d11Encoder::new(Codec::H264, 1920, 1080, 60, 8000)
            .expect("an NVENC encoder on this machine");
        let source = encoder.source(1).expect("a texture to draw on");
        source.quads().expect("the quadrants to draw");
        let texture = source.texture();
        let mut tile = vec![0u8; (tile_width * tile_height * 4) as usize];
        encoder
            .readback(texture, tile_width, tile_height, &mut tile)
            .expect("the first tile");
        let first = tile.clone();
        assert_eq!(
            (encoder.scaled_textures(), encoder.staging_textures()),
            (1, 1),
            "the first readback made more than one texture"
        );
        for read in 1..100 {
            let written = encoder
                .readback(texture, tile_width, tile_height, &mut tile)
                .unwrap_or_else(|why| panic!("tile {read}: {why:#}"));
            assert_eq!(written, tile.len());
            assert_eq!(tile, first, "tile {read} isn't the picture the first was");
        }
        assert_eq!(
            (encoder.scaled_textures(), encoder.staging_textures()),
            (1, 1),
            "a hundred readbacks made another texture"
        );
        eprintln!(
            "{} bytes a tile, a hundred of them, through {} staging texture(s) and {} scaled",
            tile.len(),
            encoder.staging_textures(),
            encoder.scaled_textures()
        );
    }

    /// A captured texture is the encoder's texture: the capture makes the device,
    /// the encoder opens on that same one instead of making its own, and the very
    /// texture the compositor filled encodes to a bitstream whose first frame is a
    /// keyframe. That is the whole texture path end to end — screen, texture,
    /// driver's encoder, packet — and the device count is the half of it no picture
    /// can show: one device for a capture and its encoder, not one each.
    ///
    /// Nothing here wires the pair into a stream; it is the same two calls a
    /// stream will make, in the same order, so that the API is proven rather than
    /// assumed.
    #[test]
    #[ignore = "needs a screen and an NVIDIA GPU: it captures the primary display and encodes it"]
    fn a_captured_frame_encodes_on_the_captures_own_device() {
        use std::time::{Duration, Instant};

        use crate::wgc::{Arrival, Capture, Monitor};

        let _gpu = one_gpu_test_at_a_time();
        let mut capture =
            Capture::open(Monitor::PRIMARY).expect("a capture of the primary display");
        eprintln!("{}", capture.describe());
        let device = capture.device();
        assert!(!device.is_null(), "a capture with no device");

        // One frame, waited for rather than demanded: the compositor makes one
        // when it has something to draw, which on the first call is a few tens of
        // milliseconds.
        let started = Instant::now();
        let (captured_width, captured_height) = loop {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "no frame in 10 s, which is what a capture of a screen that isn't drawing does"
            );
            match capture.frame().expect("a frame") {
                Arrival::Frame => break capture.content_size(),
                Arrival::Idle => std::thread::sleep(Duration::from_millis(5)),
                // The pool is rebuilt at the new size by the shim, so the next
                // frame is the one to encode.
                Arrival::Resized => continue,
                Arrival::Closed => panic!("the primary display's capture closed"),
            }
        };
        let texture = capture.texture();
        assert!(!texture.is_null(), "a frame arrived with no texture");

        // The captured texture itself is what is encoded, so that the frame the
        // encoder reads is the frame the compositor filled. NVENC takes even
        // dimensions; H.264 stops at 4096 a side where H.265 and AV1 go to 8192,
        // and a display past that goes through the scaler, which is what a stream
        // does with any display wider than the stream's own size.
        let (captured_even_width, captured_even_height) =
            (captured_width & !1, captured_height & !1);
        assert!(
            captured_even_width > 0 && captured_even_height > 0,
            "a {captured_width}x{captured_height} display"
        );
        let within = |side: u32| captured_even_width <= side && captured_even_height <= side;
        let (codec, width, height) = if within(4096) {
            (Codec::H264, captured_even_width, captured_even_height)
        } else if within(8192) {
            (Codec::H265, captured_even_width, captured_even_height)
        } else {
            (Codec::H264, 1920u32, 1080u32)
        };

        // The count is this process's, so it is read either side of the open that
        // must not move it: what an encoder on a device handed to it must not do is
        // make a second device.
        let devices_before = unsafe { yapfast_d3d11enc_devices_created() };
        let mut encoder =
            D3d11Encoder::open_on(device, codec, width, height, 60, 8000, Surfaces::Ring)
                .unwrap_or_else(|why| panic!("no encoder on the capture's device: {why:#}"));
        assert_eq!(
            unsafe { yapfast_d3d11enc_devices_created() },
            devices_before,
            "the encoder opened on a device and made a second one anyway"
        );
        assert_eq!(
            encoder.device(),
            device,
            "the encoder is on a device of its own rather than the capture's"
        );
        let scaled = (width, height) != (captured_width, captured_height);
        let texture = if scaled {
            encoder
                .scale(Texture::new(texture), width, height, Resize::Shrink)
                .expect("the capture's picture scaled to the encoder's size")
        } else {
            Texture::new(texture)
        };

        eprintln!(
            "{} ({}), {width}x{height} from a {captured_width}x{captured_height} capture ({}), \
             device {device:p}",
            encoder.engine(),
            encoder.configuration().tuning,
            if scaled {
                "through the scaler"
            } else {
                "the captured texture itself"
            }
        );
        let packet = encoder
            .encode_texture(texture, 0, true)
            .expect("the captured frame encoded")
            .expect("a packet for the captured frame");
        assert!(!packet.data.is_empty(), "an empty access unit");
        assert!(packet.keyframe, "the first frame isn't a keyframe");
        // Its structure, as the packetizer reads it: parameter sets before the
        // picture, in the form this codec's packets take — the same check the
        // drawn-texture test makes, now made about a screen.
        match codec {
            Codec::H265 => assert_eq!(
                (yapfast_discord::rtp::nal_units(&packet.data)[0][0] >> 1) & 0x3f,
                32,
                "an H.265 keyframe starts with its video parameter set"
            ),
            _ => assert_eq!(
                yapfast_discord::rtp::nal_units(&packet.data)[0][0] & 0x1f,
                7,
                "an H.264 keyframe starts with its sequence parameter set"
            ),
        }
        // No pixel of it touched by the processor: the same claim the drawn-texture
        // tests make, now made about a screen.
        assert_eq!(
            encoder.copied(),
            0,
            "the capture's pixels crossed the processor"
        );
        eprintln!(
            "{} bytes for the captured frame, a keyframe, {} frames' pixels copied by the processor",
            packet.data.len(),
            encoder.copied()
        );

        // The capture outlives the encoder here, which is the ownership rule read
        // the other way: the encoder borrowed the device and released nothing, so
        // the capture still has one to close.
        drop(encoder);
        assert_eq!(
            capture.device(),
            device,
            "the encoder took the device with it"
        );
        drop(capture);
    }
}
