//! `d3d11.rs`'s session.

use super::*;

/// A texture the encoder reads, and which of its subresources: the surface a
/// capture source has just filled, which stays the caller's. Its handle is an
/// `ID3D11Texture2D` on [`D3d11Encoder::device`]'s device, and the shim checks
/// that on the first frame it reads it, by asking the texture which device it is
/// on and comparing that pointer with its own: a texture from another device is
/// memory it can't read, and two devices made on one adapter are still two.
///
/// It is a lease of the owner that gave it out — the capture, the [`Source`], the
/// encoder's scaler, the [`Shared`] set — and `'a` is that owner's borrow: see
/// [`crate::lease`]. Every call it is given to reads the texture's vtable
/// through the pointer, so a use of it after the owner has gone, or after the
/// owner has replaced the picture it names, would be a read of freed memory; the
/// borrow makes it a compile error instead. A lease takes nothing of the owner's
/// and frees nothing when it goes, so a frame costs a pointer and a number.
///
/// The safe ways to hold one are the ones an owner answered with —
/// [`Source::texture`], [`D3d11Encoder::scale`], [`Shared::picture`] — and forging
/// one is `unsafe`.
pub struct Texture<'a> {
    /// The `ID3D11Texture2D` itself, handed to a session opened on the
    /// capture's own device.
    pub handle: *mut c_void,
    /// Which subresource of it, for a texture array; 0 for an ordinary one.
    pub subresource: u32,
    /// The borrow of the owner that gave it out, which is the whole of why a
    /// texture cannot be used past it. Nothing here reads it: the compiler is
    /// what holds the owner to it.
    #[allow(dead_code)]
    pub(crate) owner: Lease<'a>,
    /// For the one picture the encoder's scaler keeps: the claim on it, which
    /// keeps a scale or readback to another size from freeing the texture this
    /// names, and which lets go of it when this does. Held for that and for its
    /// borrow; nothing here reads it either.
    #[allow(dead_code)]
    pub(crate) picture: Option<Loan<'a>>,
}

impl<'a> Texture<'a> {
    /// The whole of an ordinary texture that the caller itself holds: the
    /// surface a capture has just filled, or one a test drew into.
    ///
    /// The lifetime is `'static` because this cannot name the borrow of whatever
    /// owns `handle` from a bare pointer, so the caller makes the claim instead:
    /// a texture wrapped here may be used anywhere, which is exactly what the
    /// leases in this file exist to stop safe code from doing. Every stream's
    /// captured texture goes through here, because a capture hands out the raw
    /// pointer of the surface its pool filled and nothing that says how long it
    /// is good for.
    ///
    /// # Safety
    ///
    /// `handle` must be an `ID3D11Texture2D` on the device of every
    /// [`D3d11Encoder`] it is given to, and it must stay alive — not released,
    /// and not handed back to the capture that made it — until the last call
    /// that reads it has returned. The shim dereferences the object on the
    /// first frame it reads and on every scale, readback and encode after that,
    /// so a null, freed or foreign texture is a fault there rather than an
    /// error here, and no call in this file can tell.
    pub unsafe fn new(handle: *mut c_void) -> Texture<'static> {
        Texture {
            handle,
            subresource: 0,
            // The caller's promise is the whole of this lifetime, and `()` is
            // borrowed by nothing.
            owner: Lease::of(&()),
            picture: None,
        }
    }

    /// The whole of a texture `owner` gave out, and the borrow that says it is
    /// good only while the owner is: what a source, a scaler and a share answer
    /// with. Its device is the one the owner made and it lives as long as the
    /// owner does, which is why this is not `unsafe` and not public: only the
    /// owners in this file can name it.
    pub(crate) fn of<O: ?Sized>(handle: *mut c_void, owner: &'a O) -> Self {
        Self {
            handle,
            subresource: 0,
            owner: Lease::of(owner),
            picture: None,
        }
    }
}

impl std::fmt::Debug for Texture<'_> {
    /// The handle and which subresource of it, which is what a log line about a
    /// frame says; the borrow is not something a log can show.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Texture")
            .field("handle", &self.handle)
            .field("subresource", &self.subresource)
            .finish_non_exhaustive()
    }
}

/// The card's video processor between a capture and a session that reads
/// surfaces: BGRA at the capture's size in, NV12 at the stream's size out, with
/// no pixel reaching the processor. Quick Sync and AMF read a surface, not a
/// picture, and a capture draws one they can't use as it is.
#[cfg(d3d11conv)]
pub struct Converter {
    pub(crate) raw: *mut Raw,
    /// The size the pictures come at, which the capture changes under us: a new
    /// one makes another converter, since the processor is described by both.
    pub(crate) from: (u32, u32),
    pub(crate) to: (u32, u32),
}

#[cfg(d3d11conv)]
impl Converter {
    /// A converter from `from` to `to`, on the device the capture fills.
    pub fn new(device: *mut std::ffi::c_void, from: (u32, u32), to: (u32, u32)) -> Result<Self> {
        let mut why = [0u8; 192];
        // SAFETY: the device is the capture's, which outlives this; the buffer is
        // one the shim ends with a zero.
        let raw = unsafe {
            yapfast_d3d11conv_new(
                device,
                from.0,
                from.1,
                to.0,
                to.1,
                why.as_mut_ptr().cast(),
                why.len(),
            )
        };
        if raw.is_null() {
            bail!("no video processor to convert with: {}", c_text(&why));
        }
        Ok(Self { raw, from, to })
    }

    /// Converts `texture` and gives back the NV12 texture the processor wrote,
    /// which is this converter's and valid until the next call or its drop.
    pub fn run(&mut self, texture: &Texture<'_>) -> Result<*mut std::ffi::c_void> {
        let mut why = [0u8; 192];
        // SAFETY: the texture is the capture's and lives for the call; the shim
        // reads it on the same device it was made on.
        let out = unsafe {
            yapfast_d3d11conv_run(
                self.raw,
                texture.handle,
                texture.subresource,
                why.as_mut_ptr().cast(),
                why.len(),
            )
        };
        if out.is_null() {
            bail!(
                "the video processor wouldn't convert the picture: {}",
                c_text(&why)
            );
        }
        Ok(out)
    }

    /// Whether this converter was opened for the same sizes: a capture that
    /// resized needs another one.
    pub fn fits(&self, from: (u32, u32), to: (u32, u32)) -> bool {
        self.from == from && self.to == to
    }

    /// Pictures the processor converted.
    pub fn converted(&self) -> u64 {
        // SAFETY: raw is an open converter.
        unsafe { yapfast_d3d11conv_converted(self.raw) }
    }
}

// SAFETY: a converter is used by the thread that made the share, as the encoder
// that reads its pictures is; the device's video processor takes one caller at a
// time and nothing here shares it.
#[cfg(d3d11conv)]
unsafe impl Send for Converter {}

#[cfg(d3d11conv)]
impl Drop for Converter {
    fn drop(&mut self) {
        // SAFETY: raw came from the constructor and is freed once.
        unsafe { yapfast_d3d11conv_free(self.raw) };
    }
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
    pub(crate) fn held(self) -> c_uint {
        match self {
            Self::Ring => 4,
            Self::One => 1,
        }
    }
}

/// An encoder on the GPU reading D3D11 textures: NVENC through a D3D11 device,
/// one codec to an instance, opened with the tuning the stream ships.
pub struct D3d11Encoder {
    pub(crate) raw: *mut Raw,
    pub(crate) codec: Codec,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) kbps: u32,
    /// Which codecs the driver offered this session, as it named them, so a
    /// stream says what its card refused rather than quietly offering less.
    pub(crate) codecs: String,
    pub(crate) configuration: Configuration,
    /// The buffers encoded frames are written into, reused as frames are sent.
    pub(crate) out: Arc<Pool>,
    /// The one picture the scaler keeps, and how many leases of it are out: a
    /// scale to another size frees it, so it waits for them rather than freeing
    /// a texture somebody holds. Its scaler state lives in the shim, where the
    /// size the picture is at is read beside this.
    pub(crate) scaled: Loans,
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
    pub(crate) fn adopt(
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
            scaled: Loans::new(),
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
    ///
    /// The picture answered with is a lease of this encoder, so it cannot be used
    /// after the encoder has gone, and a scale to another size while one is out is
    /// refused rather than freeing the texture that lease names: a scale to the
    /// size that is already out writes over that picture, which is what a stream
    /// does frame after frame.
    ///
    /// `texture` must be one this encoder's device can read — a capture's
    /// surface, a [`Source::texture`], or a picture a scale answered — and its
    /// lease says it is alive: the shim reads the object through the pointer, and
    /// a texture whose owner has gone is a compile error before it is a fault
    /// there.
    pub fn scale(
        &self,
        texture: Texture<'_>,
        width: u32,
        height: u32,
        resize: Resize,
    ) -> Result<Texture<'_>> {
        // The picture the scaler keeps is freed by a scale to another size, and
        // the leases that name it are borrows of it: a size that would free it
        // waits for them, which is the one thing a borrow cannot say on its own.
        if let Err((out_width, out_height)) = self.scaled.free_at((width, height)) {
            bail!(
                "the {out_width}x{out_height} picture the scaler made is still out: it is encoded \
                 or read back before another size is scaled"
            );
        }
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
        Ok(Texture {
            handle: scaled,
            subresource: 0,
            owner: Lease::of(self),
            picture: Some(self.scaled.lend((width, height))),
        })
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
    ///
    /// A readback scales through the same one picture the scaler keeps, so a tile
    /// of another size waits while a scale's picture is still out, exactly as
    /// [`Self::scale`] does.
    ///
    /// `texture` carries the same requirement as it does for [`Self::scale`]: on
    /// this encoder's device, and its lease alive.
    pub fn readback(
        &self,
        texture: Texture<'_>,
        width: u32,
        height: u32,
        pixels: &mut [u8],
    ) -> Result<usize> {
        // The tile is scaled into the picture the scaler keeps, at the tile's own
        // size, so a size that would free a picture somebody holds waits for it.
        if let Err((out_width, out_height)) = self.scaled.free_at((width, height)) {
            bail!(
                "the {out_width}x{out_height} picture the scaler made is still out: it is encoded \
                 before a {width}x{height} tile is read back"
            );
        }
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
    /// crate::encode::CLOCK) units. None when the encoder made no packet. The
    /// frame the encoder produced is copied into a buffer of the pool's, which
    /// grows to hold it first if it has to: the picture is not submitted again,
    /// since the encoder has already encoded it and moved on.
    ///
    /// `texture` carries the same requirement as it does for [`Self::scale`]: on
    /// this encoder's device, and its lease alive.
    pub fn encode_texture(
        &self,
        texture: Texture<'_>,
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame>> {
        let mut is_key: c_int = 0;
        // SAFETY: the texture is the caller's and lives as long as the call,
        // and the flag has a place of its own; both stay put during the call.
        let written = unsafe {
            yapfast_d3d11enc_encode(
                self.raw,
                texture.handle,
                texture.subresource,
                pts as c_longlong,
                c_int::from(keyframe),
                &mut is_key,
            )
        };
        let size = match written {
            ..0 => bail!("{:?} encoding failed: {}", self.codec, self.error()),
            0 => return Ok(None),
            size => size as usize,
        };
        // The buffer that holds a frame is the one the pool hands out: a frame
        // that outgrew it grows the pool first, so the copy below fits and the
        // frame stays the one the encoder made.
        while self.out.capacity() < size {
            if !self.out.grow() {
                bail!(
                    "the {:?} frame doesn't fit {} bytes",
                    self.codec,
                    self.out.capacity()
                );
            }
        }
        let mut out = self.out.checkout();
        let spare = out.spare();
        // SAFETY: the buffer is at least the unit's size, which the pool was
        // grown to before this call, and both stay put during it.
        let copied = unsafe { yapfast_d3d11enc_drain(self.raw, spare.as_mut_ptr(), spare.len()) };
        if copied < 0 {
            bail!("the {:?} encoder lost the frame it produced", self.codec);
        }
        out.finish(copied as usize);
        Ok(Some(Frame {
            data: out,
            keyframe: is_key != 0,
            // The frame was encoded at this time, whatever the pool had to do
            // to hold it: one picture, one submission.
            timestamp: pts as u32,
        }))
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
    pub(crate) fn error(&self) -> String {
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
