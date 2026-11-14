//! `stream_worker`'s encoding.

use super::*;

/// What turns a frame into a packet.
///
/// All three are the same stream from the packetizer's side: the bytes, their
/// time on the stream clock and whether the frame is whole all come out of
/// [`Encoding::encode_i420`], [`Encoding::encode_dmabuf`] or
/// [`d3d11::D3d11Encoder::encode_texture`] in [`encode::Frame`], and go on to
/// the sender as one shape.
// One per stream, held for its length: its size is no cost, and boxing the
// encoder would add a pointer to every frame's call instead.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Encoding {
    /// FFmpeg's encoder on the processor — x264, libvpx, or one of FFmpeg's
    /// hardware encoders — reading an I420 picture the capture converted.
    Picture(Encoder),
    /// NVENC on the card, reading the capture's texture where the frame
    /// already is. The one copy a frame still costs is its bitstream out of
    /// the driver, which [`d3d11::D3d11Encoder::copied`] says is the only one.
    #[cfg(any(wgc, dup))]
    Texture(d3d11::D3d11Encoder),
    /// NVENC on the card, reading the compositor's picture after the card has
    /// imported and converted it in its own memory: the Linux counterpart of
    /// [`Self::Texture`], for a capture whose frames are DMA-BUFs rather than
    /// textures. No pixel of a frame crosses to the processor here either.
    #[cfg(nvcuda)]
    Cuda(Cuda),
    /// A capture's own texture, encoded where it lies by an FFmpeg session that
    /// reads surfaces — Quick Sync and AMF on Windows. The session was opened on
    /// the capture's device and is handed the same texture the window draws, so
    /// no frame is converted or uploaded first; [`Self::Texture`] is NVENC's own
    /// session, which FFmpeg has no path for.
    #[cfg(all(target_os = "windows", hwenc))]
    QsvTextures(encode::Encoder),
}

/// NVIDIA's card as the way a captured picture reaches its encoder: the
/// compositor's DMA-BUF is imported through EGL, scaled and converted to NV12
/// by [`cuda::Convert`]'s kernels on the card, and read there by
/// [`cuda::Encoder`]. What crosses the bus is the access unit the session
/// answers with, and nothing else.
#[cfg(nvcuda)]
pub(crate) struct Cuda {
    /// The import, the kernels and the one buffer they write frame after frame.
    pub(crate) convert: cuda::Convert,
    /// The session opened on that buffer, which reads it where it lies.
    pub(crate) encoder: cuda::Encoder,
    pub(crate) codec: Codec,
    pub(crate) size: (u32, u32),
    /// The kbit/s the session aims for now, which follows the sender's target as
    /// the session fed from memory does.
    pub(crate) kbps: u32,
    /// The line the log and the Stream tab show: which backend runs, the card it
    /// runs on, and that its pictures never come back to the processor.
    pub(crate) engine: String,
}

#[cfg(nvcuda)]
impl Cuda {
    /// The card's own path for a stream of this size in `codec`, or `None` with
    /// the reason in the log: the share then encodes through the encoder FFmpeg
    /// opens. Said every time it is asked for, since a share asks again when its
    /// codec or its size changes, and a share that dropped to an encoder that
    /// uploads each frame has to say why it did.
    pub(crate) fn open(codec: Codec, width: u32, height: u32, fps: u32, kbps: u32) -> Option<Self> {
        let opened = cuda::Convert::open(width, height).and_then(|convert| {
            let encoder = cuda::Encoder::open(codec, width, height, fps, kbps, &convert)?;
            Ok((convert, encoder))
        });
        let (convert, encoder) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                tracing::info!(
                    why = format!("{error:#}").as_str(),
                    "converting every picture: the card can't take them where they lie"
                );
                return None;
            }
        };
        // The line the report carries from here on: the session, by FFmpeg's
        // name for it, then the card and its driver, and where its pictures
        // come from. The name comes first because the Stream tab reads where
        // the encoding runs from it, as it does for the encoders FFmpeg opens.
        // Said once, at the open, with what the session actually took, as the
        // VA-API path says its own.
        let name = encode::nvenc_name(codec).expect("NVENC opened a session for this codec");
        let engine = format!(
            "{name} ({}), encoding DMA-BUFs where they lie",
            convert.describe()
        );
        let (config, tuning) = (encoder.config(), encoder.tuning());
        tracing::info!(
            engine = engine.as_str(),
            config = config.as_str(),
            tuning = tuning.as_str(),
            "encoding the card's own pictures: the compositor's DMA-BUF is imported through EGL \
             and scaled and converted to NV12 on the card, and the session reads it there"
        );
        Some(Self {
            convert,
            encoder,
            codec,
            size: (width, height),
            kbps,
            engine,
        })
    }

    /// The captured picture, imported and converted on the card and then
    /// submitted to the session that reads it there, at `pts` on the stream
    /// clock. `keyframe` asks for an IDR, and the session makes one.
    ///
    /// `hdr` is the display's own answer about the screen the picture came from,
    /// which the kernels need to read HDR's codes as the SDR picture they stand
    /// for. A screen showing SDR passes false, and the picture is converted
    /// exactly as it was before there was a curve.
    ///
    /// `Err` is a picture the card can't take — a tiled buffer its driver only
    /// gives back as an array, a picture on a device that isn't the one the
    /// import opened — which is the path's reason to be given up rather than
    /// the frame's failure. An encode that fails after the convert is the
    /// session's own failure, and is the frame's.
    pub(crate) fn encode_dmabuf(
        &mut self,
        shot: &Shot,
        pts: u64,
        keyframe: bool,
        hdr: bool,
    ) -> Result<std::result::Result<Option<encode::Frame>, String>> {
        if let Err(error) = self.convert.convert(&shot.descriptor(), hdr) {
            return Ok(Err(format!("{error:#}")));
        }
        let packet = self.encoder.encode(pts, keyframe)?;
        // The picture was encoded where it lies, so a frame the session says it
        // copied would be one the processor paid for, which is the thing this
        // path is here to keep out of the stream.
        debug_assert_eq!(
            self.encoder.copied(),
            0,
            "the card copied a picture's pixels"
        );
        Ok(Ok(packet.map(|packet| encode::Frame {
            // The unit is the pool's buffer the sender takes, not a copy of it.
            data: packet.data,
            keyframe: packet.keyframe,
            // The one picture this submission made, at the time it was made: as
            // the encoder fed from memory stamps each of its own.
            timestamp: pts as u32,
        })))
    }
}

impl Encoding {
    pub(crate) fn codec(&self) -> Codec {
        match self {
            Self::Picture(encoder) => encoder.codec(),
            #[cfg(any(wgc, dup))]
            Self::Texture(encoder) => encoder.codec(),
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => cuda.codec,
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder.codec(),
        }
    }

    pub(crate) fn size(&self) -> (u32, u32) {
        match self {
            Self::Picture(encoder) => encoder.size(),
            #[cfg(any(wgc, dup))]
            Self::Texture(encoder) => encoder.size(),
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => cuda.size,
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder.size(),
        }
    }

    pub(crate) fn bitrate(&self) -> u32 {
        match self {
            Self::Picture(encoder) => encoder.bitrate(),
            #[cfg(any(wgc, dup))]
            Self::Texture(encoder) => encoder.bitrate(),
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => cuda.kbps,
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder.bitrate(),
        }
    }

    pub(crate) fn set_bitrate(&mut self, kbps: u32) -> Result<()> {
        match self {
            Self::Picture(encoder) => encoder.set_bitrate(kbps),
            #[cfg(any(wgc, dup))]
            Self::Texture(encoder) => encoder.set_bitrate(kbps),
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => {
                cuda.encoder.set_bitrate(kbps)?;
                cuda.kbps = kbps;
                Ok(())
            }
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder.set_bitrate(kbps),
        }
    }

    /// Gives back the output buffers lying idle, for a machine short of
    /// memory: a buffer being encoded into or waiting to be sent is untouched.
    pub(crate) fn trim_pool(&mut self) {
        match self {
            Self::Picture(encoder) => encoder.trim_pool(),
            // The card's session keeps its surfaces with the capture that
            // made them, and gives them up with it rather than here.
            #[cfg(any(wgc, dup))]
            Self::Texture(_) => {}
            // The convert's buffer and the session's output buffer are the
            // card's and the convert's own, and go with the share rather than
            // back to anything here.
            #[cfg(nvcuda)]
            Self::Cuda(_) => {}
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder.trim_pool(),
        }
    }

    /// Which encoder runs, such as "x264" or "h264_nvenc/d3d11".
    pub(crate) fn engine(&self) -> &str {
        match self {
            Self::Picture(encoder) => encoder.engine(),
            #[cfg(any(wgc, dup))]
            Self::Texture(encoder) => encoder.engine(),
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => cuda.engine.as_str(),
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder.engine(),
        }
    }

    /// The encoder's side of a stream's report, which says what made the
    /// picture rather than what was meant to.
    pub(crate) fn report(&self) -> StreamStats {
        match self {
            Self::Picture(encoder) => encoder_report(encoder),
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder_report(encoder),
            #[cfg(any(wgc, dup))]
            Self::Texture(encoder) => {
                let (width, height) = encoder.size();
                StreamStats {
                    engine: encoder.engine().to_owned(),
                    codec: encoder.codec().name().to_owned(),
                    width,
                    height,
                    // A session here opens every setting the stream needs or
                    // fails the open, so one that opened dropped nothing.
                    settings_dropped: String::new(),
                    ..StreamStats::default()
                }
            }
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => StreamStats {
                engine: cuda.engine.clone(),
                codec: cuda.codec.name().to_owned(),
                width: cuda.size.0,
                height: cuda.size.1,
                // As above: what this session asks for it gets, or it doesn't
                // open at all.
                settings_dropped: String::new(),
                ..StreamStats::default()
            },
        }
    }

    /// An encoded frame from an I420 picture, for an encoder that reads
    /// pictures: `None` when this stream's frames are the card's, which are not
    /// pictures at all.
    pub(crate) fn encode_i420(
        &mut self,
        picture: &I420Buffer,
        pts: u64,
        keyframe: bool,
    ) -> Option<Result<Option<encode::Frame>>> {
        match self {
            Self::Picture(encoder) => Some(encoder.encode_i420(picture, pts, keyframe)),
            #[cfg(any(wgc, dup))]
            Self::Texture(_) => None,
            // This stream's frames are the card's, made from a descriptor; a
            // picture in memory is the UI's preview and nothing this sends.
            #[cfg(nvcuda)]
            Self::Cuda(_) => None,
            // Read where the capture drew it: a picture in memory is the UI's
            // preview, not something this stream sends.
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(_) => None,
        }
    }

    /// Encodes the picture the desktop captured as `shot`, at `sent` — the size
    /// the stream sends — by handing the descriptor itself to the encoder where
    /// it reads one, so the card imports the picture and no pixel of it comes
    /// back to the processor.
    ///
    /// An encoder that reads memory answers with the reason it won't take a
    /// descriptor ([`encode::Refused`]) — x264 does, and so does a VA-API session
    /// handed a screen, which is BGRA where a VA-API surface is made of NV12 —
    /// and is then given the same picture converted into `scratch`. That copy is
    /// this path's one cost of a frame, it is the reason the share reports, and
    /// it lasts until an encoder here reads a compositor's picture where it lies.
    ///
    /// Where [`Self::Cuda`] carries the stream, the card has already imported and
    /// converted the picture in its own memory, and no pixel of it comes back to
    /// the processor: the conversion on the processor is what a picture that path
    /// can't take gets, from the frame it can't on.
    ///
    /// `hdr` is the display's own answer about the screen, which only the card's
    /// own path reads: it is where the curve over HDR's codes runs.
    #[allow(clippy::too_many_arguments)]
    #[cfg(target_os = "linux")]
    pub(crate) fn encode_dmabuf(
        &mut self,
        shot: &Shot,
        scratch: &mut Scratch,
        sent: (u32, u32),
        pts: u64,
        keyframe: bool,
        time_ms: u64,
        hdr: bool,
    ) -> Result<Sent> {
        match self {
            Self::Picture(encoder) => {
                pictured(encoder, shot, scratch, sent, pts, keyframe, time_ms)
            }
            #[cfg(any(wgc, dup))]
            Self::Texture(_) => bail!("the card's encoder reads textures, not a desktop's DMA-BUF"),
            // The picture is imported and converted on the card, and the session
            // reads it there: what a frame costs is its bitstream. A picture this
            // path can't take gives the path up rather than the frame, and the
            // caller hands the same picture to the processor's copy.
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => match cuda.encode_dmabuf(shot, pts, keyframe, hdr)? {
                Ok(frame) => Ok(Sent::Frame {
                    frame,
                    converted: None,
                }),
                Err(why) => Ok(Sent::Card(why)),
            },
        }
    }
}

/// A picture handed to the encoder FFmpeg opened, at `sent`: read where it lies
/// where that encoder takes a descriptor, and converted into `scratch` where it
/// doesn't — which is the one copy a picture of that path costs, and
/// [`Sent::Frame::converted`] says why it was made.
#[cfg(target_os = "linux")]
pub(crate) fn pictured(
    encoder: &mut Encoder,
    shot: &Shot,
    scratch: &mut Scratch,
    sent: (u32, u32),
    pts: u64,
    keyframe: bool,
    time_ms: u64,
) -> Result<Sent> {
    match encoder.encode_dmabuf(&shot.descriptor(), keyframe, time_ms) {
        Ok(frame) => Ok(Sent::Frame {
            frame: frame?,
            converted: None,
        }),
        Err(refused) => match shot.converted(scratch, sent) {
            Ok(picture) => Ok(Sent::Frame {
                frame: encoder.encode_i420(picture, pts, keyframe)?,
                converted: Some(refused),
            }),
            Err(why) => Ok(Sent::Unreadable(format!("{why:#}"))),
        },
    }
}

pub(crate) struct Video {
    pub(crate) sender: StreamSender,
    /// The encoder the stream sends through: none while the card's session is
    /// being rebuilt, since the device it read on may already have gone with
    /// the capture that made it, and the next frame opens another.
    pub(crate) encoder: Option<Encoding>,
    /// Other applications' sound, while sharing a screen.
    pub(crate) sound: Option<stream_audio::Capture>,
    /// Whether the encoder has to be opened again before the next frame: an
    /// encoder whose session the card took away mid-stream, which is a
    /// session to build again rather than one to keep asking.
    pub(crate) reopen: bool,
    /// Frames the send queue was too full to take, since the stream started:
    /// a sender that can't keep up shows here first.
    pub(crate) skipped: u64,
    /// The rate the encoder refused, until the sender's target moves: asked
    /// again with every frame, a refusal would be a warning a frame.
    pub(crate) refused: Option<u32>,
    /// The card's video processor between a capture's BGRA picture at its own
    /// size and the NV12 surface at the stream's that a session reading surfaces
    /// takes: made on the first frame, and again when a size changes.
    #[cfg(all(target_os = "windows", hwenc, d3d11conv))]
    pub(crate) converter: Option<d3d11::Converter>,
    /// When the stream last reported how it goes out, and the bytes it had
    /// sent then.
    pub(crate) measured: (Instant, u64),
}
impl Video {
    /// Gives back what the encoder can spare, for a machine short of memory.
    pub(crate) fn trim_pool(&mut self) {
        if let Some(encoder) = self.encoder.as_mut() {
            encoder.trim_pool();
        }
    }

    /// How the stream goes out, with the kbit/s sent since it last said.
    pub(crate) fn sending(&mut self, fps: u32) -> StreamStats {
        let sent = self.sender.sent();
        let (since, before) = self.measured;
        let bits = sent.video_bytes.saturating_sub(before) * 8;
        let milliseconds = since.elapsed().as_millis().max(1) as u64;
        self.measured = (Instant::now(), sent.video_bytes);
        // The sound shared with the screen, while there is one: what waits to
        // be mixed is delay a viewer hears before the picture it came with. A
        // share with no sound says unknown rather than nothing waited.
        let sound = self.sound.as_ref().map(stream_audio::Capture::stats);
        StreamStats {
            fps,
            target_kbps: (self.sender.target() / 1000) as u32,
            sent_kbps: (bits / milliseconds) as u32,
            keyframe_requests: sent.keyframe_requests,
            resent: sent.resent,
            skipped: self.skipped,
            sound_held_ms: sound.map(|sound| sound.held_ms),
            sound_dropped_ms: sound.map(|sound| sound.dropped_ms),
            ..self
                .encoder
                .as_ref()
                .map(Encoding::report)
                .unwrap_or_default()
        }
    }
}

/// The kbit/s a stream of `width` by `height` at `fps` declares it is worth,
/// which is what an encoder is opened at before the stream has a target of its
/// own: a card's encoder opened to capture with, and the preview's, run for
/// nothing but a picture and a thumbnail. See [`stream_connection::max_bitrate`].
/// Whether a share may hand its session the capture's own surface, through the
/// card's video processor: the capture's BGRA picture at its own size becomes the
/// NV12 surface at the stream's, which is what Quick Sync and AMF read. On by
/// default, since it is the card doing that work either way; the variable turns
/// it off for a comparison against the upload path.
#[cfg(all(target_os = "windows", hwenc))]
pub(crate) fn surfaces_offered() -> bool {
    std::env::var_os("YAPFAST_STREAM_SURFACES").is_none_or(|asked| asked != "0")
}
