//! `stream_worker`'s sink.

use super::*;

/// Where each frame goes: the UI's preview, then Discord once the stream
/// has a server.
pub(crate) struct Sink {
    pub(crate) writer: Writer,
    /// The picture the preview is scaled into, kept from one frame to the
    /// next: at most [`PREVIEW_WIDTH`] by [`PREVIEW_HEIGHT`].
    pub(crate) preview: Vec<u8>,
    /// What is captured, in the words the call's Stream tab shows: which of
    /// the ways of capturing ran and the size of what it captures. Set once,
    /// by the capture before it starts.
    pub(crate) capture: String,
    /// Whether the UI shows the preview, which it says as that changes: one
    /// isn't made while nothing displays it.
    pub(crate) preview_shown: bool,
    /// The size in pixels of the tile showing the preview, once the UI has
    /// said it: what the preview is written at, rather than a fixed box.
    pub(crate) preview_tile: Option<(u32, u32)>,
    pub(crate) controls: channel::Receiver<Control>,
    /// The server to send to from the next frame, which gives the stream its size.
    pub(crate) server: Option<StreamServer>,
    /// Whether to share other applications' sound too: not for the test
    /// pattern.
    pub(crate) sound: bool,
    pub(crate) video: Option<Video>,
    pub(crate) fps: u32,
    pub(crate) effort: Effort,
    pub(crate) hardware: bool,
    /// Where this share's pictures come from, which decides what encodes
    /// them. Chosen once, before the capture starts: see `capture_screen`.
    pub(crate) footage: Footage,
    pub(crate) started: Instant,
    pub(crate) streaming: bool,
    /// When the stream's next preview is due: from when it starts.
    pub(crate) preview_due: Option<Instant>,
    /// A preview made but not reported yet.
    pub(crate) thumbnail: Option<String>,
    /// When the stream connection ended, while Discord may still name a
    /// server it moved the stream to.
    pub(crate) lost: Option<Instant>,
    /// Where the pictures come from and how they reach the window, which is
    /// the half of a share no packet shows.
    pub(crate) diagnosis: Diagnosis,
    /// How many pictures in a row the card's own path refused, which is how a
    /// share tells a compositor handing one buffer of another layout from a
    /// picture path that has gone: see [`Sink::dmabuf`].
    #[cfg(nvcuda)]
    pub(crate) refused_pictures: u32,
}

/// Where a share's pictures come from, which is what decides who encodes
/// them. It is chosen once, before the capture starts, and never changes
/// under a stream: a capture that has to change its size rebuilds its
/// encoder, but not its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Footage {
    /// A screen, window or camera libwebrtc captured and the processor
    /// converted to I420: a picture in memory, which FFmpeg's encoder reads
    /// from there.
    InMemory,
    /// A screen Windows Graphics Capture or DXGI's Desktop Duplication filled
    /// a texture with, which NVENC reads on the card that shows it. Nothing of
    /// a frame crosses the processor on its way to the encoder.
    #[cfg(any(wgc, dup))]
    OnTheCard,
    /// A screen or window the desktop's portal or the X server handed over as
    /// the DMA-BUFs its compositor drew. The window imports them and draws them
    /// where they lie; the encoder reads the descriptor itself where it can, and
    /// the same picture converted on the processor where it can't — which is
    /// the one copy a frame costs here, and [`Sink::dmabuf`] says when it takes
    /// it.
    #[cfg(target_os = "linux")]
    Dmabuf,
}

impl Sink {
    /// Takes a frame at the size it's sent: the preview shows it, scaled
    /// down to fit, while the UI shows a preview, and Discord gets it once
    /// the stream has a server.
    pub(crate) fn frame(&mut self, picture: &I420Buffer) -> Result<()> {
        self.preview(picture);
        // A share of the card's frames has no picture to send: they reach the
        // stream through `textured`, as the texture they already are. A picture
        // that arrives while that share runs is the UI's alone, and `preview`
        // has just had it.
        if self.footage != Footage::InMemory {
            return Ok(());
        }
        let Some((pts, keyframe)) = self.ready((picture.width(), picture.height()), None)? else {
            return Ok(());
        };
        let video = self.video.as_mut().expect("ready has opened the stream");
        let made = {
            let encoder = video
                .encoder
                .as_mut()
                .expect("ready has opened the stream's encoder");
            (encoder.codec(), encoder.encode_i420(picture, pts, keyframe))
        };
        let (codec, made) = made;
        let Some(made) = made else {
            return Ok(());
        };
        self.delivered(codec, made, keyframe)
    }

    /// Takes a picture the desktop captured as the DMA-BUF the compositor drew
    /// it in, `shot`, at the size the stream sends, `sent`.
    ///
    /// On NVIDIA's card the picture goes to the encoder where it lies: the card
    /// imports it, scales and converts it there, and the session reads it in its
    /// own memory, so no pixel of it comes back to the processor. A picture that
    /// path can't take gives the path up and is converted on the processor, as
    /// every picture of the share is from then on.
    ///
    /// The encoder is otherwise handed the descriptor itself where it reads one: a
    /// VA-API session imports the picture on the card and no pixel of it comes
    /// back to the processor. Every other encoder reads memory — and so does a
    /// VA-API session handed a screen, which is BGRA where a VA-API surface is
    /// made of NV12 — and for those the picture is read back into the buffer kept
    /// for it and converted, once, with the reason in the share's report. That
    /// copy is the one this path makes of a frame, and [`Taken::Sent`] says which
    /// of the ways sent it.
    #[cfg(target_os = "linux")]
    pub(crate) fn dmabuf(
        &mut self,
        shot: &Shot,
        sent: (u32, u32),
        scratch: &mut Scratch,
    ) -> Result<Taken> {
        let Some((pts, keyframe)) = self.ready(sent, None)? else {
            return Ok(Taken::Held);
        };
        let time_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let (codec, kbps, made) = {
            let video = self.video.as_mut().expect("ready has opened the stream");
            let Some(encoder) = video.encoder.as_mut() else {
                return Ok(Taken::Held);
            };
            (
                encoder.codec(),
                encoder.bitrate(),
                encoder.encode_dmabuf(shot, scratch, sent, pts, keyframe, time_ms)?,
            )
        };
        let made = match made {
            // A picture the card's own path can't take is dropped, not the path.
            // A compositor hands a buffer of another layout now and then — one
            // it drew for something else, or one tiled differently after a
            // change — and giving the path up for it would cost the share the
            // card's own copy and the pointer the portal paints into its
            // pictures for the rest of the stream. Pictures refused one after
            // another are the layout changed for good, or the device gone, and
            // that is when the encoder FFmpeg opens takes the share over: this
            // picture is converted on the processor and encoded here, exactly
            // as every picture of a share without the rung is, and the ones
            // after it follow the same way.
            #[cfg(nvcuda)]
            Sent::Card(why) => {
                self.refused_pictures = self.refused_pictures.saturating_add(1);
                if self.refused_pictures < REFUSED_PICTURES {
                    if self.refused_pictures == 1 {
                        tracing::info!(
                            why = why.as_str(),
                            "a picture the card can't take where it lies: the frame is dropped"
                        );
                    }
                    return Ok(Taken::Held);
                }
                tracing::info!(
                    why = why.as_str(),
                    pictures = self.refused_pictures,
                    "converting every picture: the card can't take them where they lie"
                );
                self.diagnosis.fallback = Some(format!(
                    "the card can't take a picture where it lies: {why}"
                ));
                // No encoder at all for this codec is the answer a picture
                // nothing here can read gets, which moves the share to the
                // capturer that has pixels rather than failing it.
                let mut encoder = match Encoder::new(
                    codec,
                    sent.0,
                    sent.1,
                    self.fps,
                    kbps,
                    self.effort,
                    self.hardware,
                ) {
                    Ok(encoder) => encoder,
                    Err(error) => return Ok(Taken::Processor(format!("{error:#}"))),
                };
                let made = pictured(&mut encoder, shot, scratch, sent, pts, keyframe, time_ms)?;
                let video = self.video.as_mut().expect("ready has opened the stream");
                video.encoder = Some(Encoding::Picture(encoder));
                made
            }
            made => made,
        };
        match made {
            Sent::Frame { frame, converted } => {
                // The card's path has taken a picture again, so the ones before
                // it were its now and then rather than gone.
                #[cfg(nvcuda)]
                {
                    self.refused_pictures = 0;
                }
                // Which encoder opened doesn't change under a stream, so the
                // answer to "does it read a descriptor" would be the same for
                // every frame after this one: it is said once, in the log and in
                // the share's report, and a reader then knows why a frame cost a
                // copy.
                if let Some(refused) = converted
                    && self.diagnosis.fallback.is_none()
                {
                    tracing::info!(
                        why = %refused,
                        "converting every picture: the encoder reads pictures"
                    );
                    self.diagnosis.fallback =
                        Some(format!("the encoder reads pictures: {refused}"));
                }
                self.delivered(codec, Ok(frame), keyframe)?;
                Ok(Taken::Sent)
            }
            Sent::Unreadable(why) => Ok(Taken::Processor(why)),
            // The card's own path answered this one where it was given up,
            // above: a picture either comes back from it ready to send or the
            // path is gone. Were one to reach here, it is one nothing here can
            // send.
            #[cfg(nvcuda)]
            Sent::Card(why) => Ok(Taken::Processor(why)),
        }
    }

    /// The same for a frame the card captured as a texture `content` big, at
    /// the size the stream sends, `size`: a screen larger than the stream is
    /// scaled to it on the card, in one pass through the driver's video
    /// processor or its shader, and its encoder reads the result where it
    /// lies. No pixel of the frame is converted, uploaded or copied by the
    /// processor; the one copy it still costs is its bitstream out of the
    /// driver.
    #[cfg(any(wgc, dup))]
    pub(crate) fn textured(
        &mut self,
        texture: d3d11::Texture,
        content: (u32, u32),
        size: (u32, u32),
        device: *mut c_void,
    ) -> Result<()> {
        let Some((pts, keyframe)) = self.ready(size, Some(device))? else {
            return Ok(());
        };
        let video = self.video.as_mut().expect("ready has opened the stream");
        // A card whose FFmpeg encoder reads a surface takes the texture itself;
        // NVENC's own session scales it on the card first and reads that. Both
        // hand back an access unit, so what comes after is the same either way.
        let made = match video.encoder.as_mut() {
            // A card whose FFmpeg encoder reads a surface takes the texture the
            // capture filled, as it is.
            #[cfg(all(target_os = "windows", hwenc))]
            Some(Encoding::QsvTextures(encoder)) => {
                let codec = encoder.codec();
                // The capture's picture is BGRA at its own size where the surface
                // the session reads is NV12 at the stream's: the card's video
                // processor is the one step between them, and it costs the
                // processor nothing.
                #[cfg(d3d11conv)]
                let picture = {
                    let fits = video
                        .converter
                        .as_ref()
                        .is_some_and(|converter| converter.fits(content, size));
                    if !fits {
                        video.converter = Some(
                            d3d11::Converter::new(device, content, size)
                                .map_err(|error| anyhow::anyhow!("{error:#}"))?,
                        );
                    }
                    let converter = video.converter.as_mut().expect("made above");
                    converter.run(&texture)?
                };
                #[cfg(not(d3d11conv))]
                let picture = {
                    let _ = (content, device);
                    texture.handle
                };
                (codec, encoder.encode_texture(picture, 0, pts, keyframe))
            }
            Some(Encoding::Texture(encoder)) => {
                let codec = encoder.codec();
                // A screen the stream's size is read where it lies, and one of
                // another size is scaled to the stream's on the card: the size a
                // capture comes at is the screen's, and the size a stream goes at
                // is Discord's to choose.
                let texture = if content == size {
                    texture
                } else {
                    encoder.scale(texture, size.0, size.1, d3d11::Resize::Shrink)?
                };
                // A failure here is the session's own, and a session the card took
                // away is one to open again rather than something to withdraw a
                // codec over: the capture that feeds it is the caller's to rebuild
                // with it, so this says nothing to Discord and leaves the codecs it
                // offers alone.
                (codec, encoder.encode_texture(texture, pts, keyframe))
            }
            _ => return Ok(()),
        };
        let (codec, made) = made;
        match made? {
            Some(frame) => {
                video.sender.send(stream_connection::Frame {
                    data: frame.data,
                    codec,
                    timestamp: frame.timestamp,
                    keyframe: frame.keyframe,
                });
            }
            // A session that answered with nothing for this picture — a
            // hardware encoder's first frame often does — has not made the
            // whole frame a viewer asked for, so the request is put back
            // rather than taken for answered: a still desktop would otherwise
            // never produce it.
            None if keyframe => video.sender.force_keyframe(),
            None => {}
        }
        Ok(())
    }

    /// A preview of a frame the card captured, read back from the card rather
    /// than captured a second time: the picture the stream is sending is scaled
    /// on the GPU to fit the tile, copied into the one staging texture the
    /// preview's session keeps, and mapped once.
    ///
    /// What the card reads back is BGRA, so the tile is written to the UI as
    /// RGBA a pixel at a time, and Discord's thumbnail is made first, when one
    /// is due, from the same read: libyuv calls the card's order ARGB.
    #[cfg(any(wgc, dup))]
    pub(crate) fn preview_from_card(
        &mut self,
        tile: &mut d3d11::D3d11Encoder,
        texture: d3d11::Texture,
        content: (u32, u32),
        pixels: &mut Vec<u8>,
    ) -> Result<()> {
        let due = self.preview_due.is_some_and(|due| due <= Instant::now());
        if !self.preview_shown && !due {
            return Ok(());
        }
        let (width, height) = preview_size(content.0, content.1, self.constrained());
        let length = (width * height * 4) as usize;
        if pixels.len() < length {
            pixels.resize(length, 0);
        }
        let read = &mut pixels[..length];
        tile.readback(texture, width, height, read)?;
        if due {
            // The tile as a picture, once every five minutes rather than every
            // frame: a screen's colour as the card already shows it, so no tone
            // curve is asked of a texture the compositor has finished with.
            let mut picture = I420Buffer::new(width, height);
            convert(&mut picture, read, width * 4, &Tone::SDR);
            let shown = planes(&picture);
            if !is_black(shown.y) {
                match thumbnail(&shown) {
                    Ok(thumbnail) => self.thumbnail = Some(thumbnail),
                    Err(error) => {
                        tracing::warn!(error = format!("{error:#}"), "could not make a preview")
                    }
                }
                self.preview_due = Some(Instant::now() + THUMBNAIL_EVERY);
            }
        }
        let Sink { writer, .. } = self;
        writer.write(width, height, |rgba| {
            for (pixel, read) in rgba.chunks_exact_mut(4).zip(read.chunks_exact(4)) {
                pixel.copy_from_slice(&[read[2], read[1], read[0], 255]);
            }
        });
        Ok(())
    }

    /// Publishes the tile of a frame the card captured as a picture the UI's
    /// window shows where the card drew it: the same size the tile is read back
    /// at, scaled on the GPU into a texture both processes hold, and one handle
    /// and two numbers published where the tile's pixels would have been.
    ///
    /// Answers whether the UI can show such a picture at all. `false` is an
    /// answer for the rest of the stream rather than for this frame: whether a
    /// pair of adapters can share a texture doesn't change while the stream
    /// lasts, and the tile's pixels keep going out exactly as they did, so a UI
    /// that can't show this one has nothing to fall back to but what it already
    /// has.
    #[cfg(any(wgc, dup))]
    pub(crate) fn share_from_card(
        &mut self,
        slot: u32,
        tile: &mut d3d11::D3d11Encoder,
        shared: &mut d3d11::Shared,
        texture: d3d11::Texture,
    ) -> Result<bool> {
        let (texture, fence_value) = shared.draw(slot, tile, texture)?;
        let (width, height) = shared.size();
        Ok(self.writer.publish_into(&Drawn {
            luid: shared.luid(),
            fence: shared.fence(),
            texture,
            fence_value,
            width,
            height,
        }))
    }

    /// The preview, and Discord's thumbnail when one is due: the picture scaled
    /// down to fit, written for the UI, and a JPEG of it when it isn't black.
    /// This is the frame's own work, and nothing makes one while the UI shows
    /// no preview and none is due for Discord.
    pub(crate) fn preview(&mut self, picture: &I420Buffer) {
        let (width, height) = (picture.width(), picture.height());
        let due = self.preview_due.is_some_and(|due| due <= Instant::now());
        if self.preview_shown || due {
            // The tile's own size while only the preview needs the picture, so a
            // small tile costs a small frame and a large one grows the frames
            // file once. Discord's still is made from the wider box, since a
            // tile smaller than the still would cost it its detail.
            let box_size = match (self.preview_tile, due) {
                (Some(tile), false) => tile,
                _ => (PREVIEW_WIDTH, PREVIEW_HEIGHT),
            };
            let preview = fit(width, height, box_size.0, box_size.1);
            let shown = planes(picture);
            let Sink {
                writer,
                preview: scratch,
                ..
            } = self;
            writer.write(preview.0, preview.1, |pixels| {
                shown.to_rgba(&mut *scratch, pixels, preview.0, preview.1);
            });
            // What the preview shows: the picture itself when it's that size
            // already, or the one `to_rgba` scaled into the buffer kept for it.
            let shown = if preview == (width, height) {
                shown
            } else {
                previewed(&scratch[..], preview.0, preview.1)
            };
            if due && !is_black(shown.y) {
                match thumbnail(&shown) {
                    Ok(thumbnail) => self.thumbnail = Some(thumbnail),
                    Err(error) => {
                        tracing::warn!(error = format!("{error:#}"), "could not make a preview")
                    }
                }
                self.preview_due = Some(Instant::now() + THUMBNAIL_EVERY);
            }
        }
    }

    /// Everything a frame needs before it's encoded, and nothing of the encoding
    /// itself: the stream opened at this size and this codec, the rate the
    /// sender's congestion controller asks for followed, and the frame's own time
    /// and whether it has to be whole. None when the frame is dropped instead — the queue is full, or
    /// nothing has said where the stream goes yet.
    pub(crate) fn ready(
        &mut self,
        (width, height): (u32, u32),
        device: Option<*mut c_void>,
    ) -> Result<Option<(u64, bool)>> {
        if let Some(server) = self.server.take() {
            let (offers, encoder) = self.offers(width, height, device)?;
            let sender = StreamSender::start(server, width, height, self.fps, offers)?;
            let sound = if self.sound {
                stream_audio::Capture::start(sender.sound())
                    .inspect_err(|error| {
                        tracing::warn!(error = format!("{error:#}"), "sharing without sound")
                    })
                    .ok()
            } else {
                None
            };
            self.video = Some(Video {
                sender,
                encoder: Some(encoder),
                sound,
                reopen: false,
                skipped: 0,
                refused: None,
                #[cfg(all(target_os = "windows", hwenc, d3d11conv))]
                converter: None,
                measured: (Instant::now(), 0),
            });
        }
        let Some(video) = self.video.as_ref() else {
            return Ok(None);
        };
        // Discord's choice of codec changes as viewers come and go.
        let codec = video
            .sender
            .codec()
            .or_else(|| video.encoder.as_ref().map(Encoding::codec))
            .unwrap_or(stream_connection::SEND_CODEC);
        let resized = video
            .encoder
            .as_ref()
            .is_none_or(|encoder| encoder.size() != (width, height));
        // An encoder the card took away between frames is opened again at the
        // size the frames are now, whether or not that changed. The session that
        // is going is let go of first, before another opens: a driver does not
        // survive one session's teardown while another is live (see
        // [`Sink::offers`]), and a switch to the codec Discord chose is exactly
        // that.
        if resized
            || video.reopen
            || video
                .encoder
                .as_ref()
                .is_none_or(|encoder| encoder.codec() != codec)
        {
            if let Some(video) = self.video.as_mut() {
                video.encoder = None;
            }
            let opened = self.encoder(codec, width, height, device);
            let video = self.video.as_mut().expect("checked above");
            match opened {
                Ok(encoder) => {
                    video.encoder = Some(encoder);
                    video.reopen = false;
                    if resized {
                        video.sender.resize(width, height);
                    }
                }
                // A GPU codec that opened at the start may not open again,
                // or at another size: Discord then chooses another, which
                // the next frame opens.
                Err(error) if codec != stream_connection::SEND_CODEC => {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        ?codec,
                        "the GPU's encoder didn't open again"
                    );
                    video.sender.withdraw(codec);
                    return Ok(None);
                }
                Err(error) => return Err(error),
            }
        }
        let Some(video) = self.video.as_mut() else {
            return Ok(None);
        };
        let aimed = video.encoder.as_ref().expect("opened above").bitrate();
        // The encoder follows the stream's target, which the controller moves
        // a report at a time. Not on every wobble: a tenth is the width of a
        // change worth telling the rate control about.
        let wanted = (video.sender.target() / 1000) as u32;
        // A rate the encoder refused is not asked for again until the target
        // moves: the condition would stay true every frame otherwise, and a
        // warning a frame is a storm in the log and an allocation with it.
        if wanted.abs_diff(aimed) > aimed / 10 && video.refused != Some(wanted) {
            match video
                .encoder
                .as_mut()
                .expect("opened above")
                .set_bitrate(wanted)
            {
                Ok(()) => {
                    video.refused = None;
                    tracing::debug!(kbps = wanted, "the encoder follows the stream's target");
                }
                Err(error) => {
                    video.refused = Some(wanted);
                    tracing::warn!(error = format!("{error:#}"), "could not change the bitrate")
                }
            }
        }
        // A full queue drops the frame anyway, so the encode it would throw
        // away is skipped; the request for a whole frame waits for the one
        // that goes.
        if video.sender.backlogged() {
            video.skipped = video.skipped.saturating_add(1);
            return Ok(None);
        }
        let pts = self.started.elapsed().as_micros() as u64 * encode::CLOCK / 1_000_000;
        let keyframe = video.sender.take_keyframe_request();
        // How long the picture waited on this side of the stream: the capture
        // had it when the pass began, and the encoder is given it now. A frame
        // that came from the capturer's own thread says nothing, since nothing
        // here saw when it arrived.
        self.diagnosis.queue_delay_ms = self
            .diagnosis
            .captured_at
            .take()
            .map(|at| u32::try_from(at.elapsed().as_millis()).unwrap_or(u32::MAX));
        Ok(Some((pts, keyframe)))
    }

    /// Puts an encoded frame on the stream, as the packetizer, its resend history
    /// and the ceiling on keyframe requests have always seen them: the same
    /// bytes, the same time on the stream clock and the same `keyframe` flag,
    /// whichever encoder made them.
    pub(crate) fn delivered(
        &mut self,
        codec: Codec,
        made: Result<Option<encode::Frame>>,
        keyframe: bool,
    ) -> Result<()> {
        let Some(video) = self.video.as_mut() else {
            return Ok(());
        };
        match made {
            Ok(Some(frame)) => video.sender.send(stream_connection::Frame {
                data: frame.data,
                codec,
                timestamp: frame.timestamp,
                keyframe: frame.keyframe,
            }),
            // Nothing came out of this picture: the whole frame a viewer asked
            // for is still owed, so the request goes back rather than being
            // taken for answered.
            Ok(None) if keyframe => video.sender.force_keyframe(),
            Ok(None) => {}
            // Only the GPU encodes AV1 and H265: Discord chooses another
            // codec instead, which the next frame encodes.
            Err(error) if codec != stream_connection::SEND_CODEC => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    ?codec,
                    "the GPU's encoder failed"
                );
                video.sender.withdraw(codec);
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }

    /// Whether a preview is wanted now: the UI shows one, or Discord's thumbnail
    /// is due. Nothing is made while neither is.
    #[cfg(any(wgc, dup, target_os = "linux"))]
    pub(crate) fn preview_wanted(&self) -> bool {
        self.preview_shown || self.preview_due.is_some_and(|due| due <= Instant::now())
    }

    /// Whether Discord's still of the stream is due, which is made from the
    /// tile's pixels: the one reason a frame is read back into memory while the
    /// window shows the card's own picture.
    #[cfg(any(wgc, dup, target_os = "linux"))]
    pub(crate) fn still_wanted(&self) -> bool {
        self.preview_due.is_some_and(|due| due <= Instant::now())
    }

    /// Discord's still from the picture the card's own path converted last, in
    /// `scratch.full`: none when the card isn't what encodes this share.
    #[cfg(nvcuda)]
    pub(crate) fn card_still(&self, scratch: &mut Scratch) -> Option<Result<()>> {
        let Some(Encoding::Cuda(cuda)) = self.video.as_ref()?.encoder.as_ref() else {
            return None;
        };
        let (width, height) = cuda.convert.size();
        let full = buffer(&mut scratch.full, width, height);
        let (y_stride, u_stride, v_stride) = full.strides();
        Some(
            cuda.convert
                .still(full.data_mut(), [y_stride, u_stride, v_stride]),
        )
    }

    /// The preview of a window that draws on the processor, made on the card
    /// from the picture the card's own path took last, at the size a preview is
    /// made at, in `scratch.full`: only its pixels come back. None when the card
    /// isn't what encodes this share, and an error for a picture the card holds
    /// no copy of — one whose rows the processor reads itself.
    #[cfg(nvcuda)]
    pub(crate) fn card_preview(&self, scratch: &mut Scratch) -> Option<Result<()>> {
        let Some(Encoding::Cuda(cuda)) = self.video.as_ref()?.encoder.as_ref() else {
            return None;
        };
        let (width, height) = cuda.convert.size();
        let (width, height) = fit(width, height, PREVIEW_WIDTH, PREVIEW_HEIGHT);
        let full = buffer(&mut scratch.full, width & !1, height & !1);
        let (y_stride, u_stride, v_stride) = full.strides();
        Some(cuda.convert.preview(
            (width, height),
            full.data_mut(),
            [y_stride, u_stride, v_stride],
        ))
    }

    /// Whether a machine short of memory has this share giving back what it can
    /// spare: a smaller preview, and buffers it will never use again.
    #[cfg(any(wgc, dup))]
    pub(crate) fn constrained(&self) -> bool {
        self.diagnosis.memory.constrained().is_some()
    }

    /// Whether the stream needs a picture now, whatever the screen is doing: a
    /// connection that hasn't opened yet, an encoder the card took away or a
    /// codec Discord chose since the last frame, or a viewer whose decoder lost
    /// its place and asked for a whole one. A stick-still desktop is otherwise
    /// left alone: nothing here is about the preview.
    #[cfg(any(wgc, dup, target_os = "linux"))]
    pub(crate) fn picture_needed(&self) -> bool {
        let Some(video) = self.video.as_ref() else {
            // No stream yet: the first picture is what opens it, and what a
            // connection asks for.
            return self.server.is_some();
        };
        video.encoder.is_none() || video.reopen || video.sender.keyframe_wanted()
    }

    /// Hands the next pictures to the processor's encoder, for a window, a
    /// camera or a screen the card couldn't open. The session that read the
    /// card's textures goes with the device that made it, and the change of
    /// kind asks for a whole frame, so a viewer's decoder starts on the
    /// pictures that come rather than depending on the last one the card made.
    #[cfg(any(wgc, dup, target_os = "linux"))]
    pub(crate) fn fall_back_to_the_processor(&mut self) {
        // The device the encoder read on goes with the capture that made it, and
        // a session that outlived its device reads memory nobody owns. A Linux
        // share's encoder read nothing on the capture's behalf — its frames were
        // pictures converted on the processor either way — so it keeps it.
        #[cfg(any(wgc, dup))]
        self.let_go();
        self.footage = Footage::InMemory;
        self.diagnosis.presentation = Some(Presentation::Copy);
        self.diagnosis.fallback = Some("the processor captures this source".to_owned());
        if let Some(video) = self.video.as_mut() {
            video.sender.force_keyframe();
        }
    }

    /// Lets go of the stream's encoder, keeping the rest: the device the
    /// encoder read on is about to go with the capture that made it, and a
    /// session that outlived its device reads memory nobody owns. The next
    /// frame opens another on the device the capture has then.
    #[cfg(any(wgc, dup))]
    pub(crate) fn let_go(&mut self) {
        if let Some(video) = self.video.as_mut() {
            video.encoder = None;
            video.reopen = true;
        }
    }

    /// The codecs to offer Discord, most wanted first, and an encoder for
    /// the first: AV1, then H265, where the GPU's encoder for each opens,
    /// then H264. Discord streams the first every viewer decodes. The
    /// settings' own codec, where one is chosen and this machine encodes it,
    /// is put in front of all of them.
    ///
    /// A card's own session is asked which codecs its driver encodes rather
    /// than another opened for each codec and given up beside it: a driver does
    /// not survive a session's teardown while another is live. A share's worker
    /// died with SIGSEGV in `libnvcuvid`'s own thread a second after its two
    /// codec probes had been let go of, the kept session encoding at the time,
    /// so what a card can send is read from the session that stays open. An
    /// encoder FFmpeg opens has no such list to read, and is opened for each
    /// codec as it always was.
    pub(crate) fn offers(
        &self,
        width: u32,
        height: u32,
        device: Option<*mut c_void>,
    ) -> Result<(Vec<Codec>, Encoding)> {
        // The settings' codec is asked for first, so the encoder opened for the
        // stream is that codec's. H264 joins the probe only where the setting
        // names it: it is the stream's fallback whatever the setting says.
        let choice = stream_codec();
        let mut probes = GPU_CODECS.to_vec();
        if let Some(wanted) = choice.wanted().filter(|wanted| !probes.contains(wanted)) {
            probes.push(wanted);
        }
        let mut first = None;
        for codec in preferred(probes, choice)
            .into_iter()
            .filter(|_| self.hardware)
        {
            if let Ok(encoder) = self.open(codec, width, height, device) {
                first = Some(encoder);
                break;
            }
        }
        let encoder = match first {
            Some(encoder) => encoder,
            None => self.open(stream_connection::SEND_CODEC, width, height, device)?,
        };
        let mut offers = match &encoder {
            // The codecs the session's driver encodes, which is what the card
            // can send: nothing else is opened to ask.
            #[cfg(nvcuda)]
            Encoding::Cuda(cuda) => cuda.encoder.driver_codecs().unwrap_or_default(),
            // What FFmpeg's encoders can send, asked by opening one for each
            // codec: a hardware encoder has no list of its own to read.
            _ => {
                let mut offers = vec![encoder.codec()];
                for codec in GPU_CODECS
                    .iter()
                    .copied()
                    .filter(|codec| self.hardware && *codec != encoder.codec())
                {
                    if self.open(codec, width, height, device).is_ok() {
                        offers.push(codec);
                    }
                }
                offers.sort_by_key(|codec| GPU_CODECS.iter().position(|one| one == codec));
                offers
            }
        };
        if !offers.contains(&stream_connection::SEND_CODEC) {
            offers.push(stream_connection::SEND_CODEC);
        }
        // A codec the setting names that this machine has no encoder for is
        // nothing to reorder around: the share offers what it can, and says so
        // once, rather than failing to start.
        if let Some(wanted) = choice.wanted().filter(|wanted| !offers.contains(wanted)) {
            tracing::warn!(
                wanted = wanted.name(),
                "the settings ask for a codec this machine doesn't encode; the share offers the \
                 ones it does"
            );
        }
        let offers = preferred(offers, choice);
        log_encoder(&encoder);
        Ok((offers, encoder))
    }

    /// An encoder for the stream in `codec`, as [`Self::open`] opens it.
    pub(crate) fn encoder(
        &self,
        codec: Codec,
        width: u32,
        height: u32,
        device: Option<*mut c_void>,
    ) -> Result<Encoding> {
        let encoder = self.open(codec, width, height, device)?;
        log_encoder(&encoder);
        Ok(encoder)
    }

    /// An encoder in `codec`, at the stream's target, or what a stream of this
    /// size opens at before it has one, reading the frames where this share's
    /// capture puts them: on the card, a session opens on the device the
    /// capture fills its textures on, which the stream hands over as `device`.
    pub(crate) fn open(
        &self,
        codec: Codec,
        width: u32,
        height: u32,
        _device: Option<*mut c_void>,
    ) -> Result<Encoding> {
        // Where the controller has got to, so an encoder opened mid-stream
        // starts at the rate the stream sends at; a stream's first encoder,
        // opened before the stream has a target, starts at the ceiling the
        // picture declares, which is where the controller opens it too.
        let aimed = self.video.as_ref().map_or_else(
            || congestion::start(u64::from(kbps(width, height, self.fps)) * 1000) / 1000,
            |video| video.sender.target() / 1000,
        ) as u32;
        match self.footage {
            // The card's session reads a texture only on the device it opened
            // on, so it opens on the capture's own, which is the whole of what
            // lets a frame reach it without being copied first. It reads the
            // ring the capture fills, so it keeps the surfaces of one.
            #[cfg(any(wgc, dup))]
            Footage::OnTheCard => {
                let device = _device.context("the card's encoder needs the capture's device")?;
                // A card whose FFmpeg encoder reads a capture's own texture —
                // Quick Sync and AMF — is opened on the capture's device and
                // handed the texture itself. NVENC has no such path in FFmpeg,
                // so it keeps the native session that scales on the card.
                #[cfg(all(target_os = "windows", hwenc))]
                if !encode::planned(codec).is_some_and(|name| name.ends_with("_nvenc"))
                    && surfaces_offered()
                    && let Ok(encoder) = encode::Encoder::on_device(
                        codec,
                        width,
                        height,
                        self.fps,
                        aimed,
                        self.effort,
                        self.hardware,
                        device,
                    )
                    && encoder.took_surfaces()
                {
                    return Ok(Encoding::QsvTextures(encoder));
                }
                Ok(Encoding::Texture(d3d11::D3d11Encoder::open_on(
                    device,
                    codec,
                    width,
                    height,
                    self.fps,
                    aimed,
                    d3d11::Surfaces::Ring,
                )?))
            }
            // A picture the desktop's compositor drew is a descriptor the encoder
            // is handed itself, which an encoder that reads memories or an
            // import it doesn't carry answers with a refusal: the session that
            // reads pictures is opened for it either way, and it is the one
            // [`Sink::dmabuf`] falls back to. The device argument is the card's
            // texture path's and nothing here opens on it.
            Footage::InMemory => Ok(Encoding::Picture(Encoder::new(
                codec,
                width,
                height,
                self.fps,
                aimed,
                self.effort,
                self.hardware,
            )?)),
            // The session that reads pictures, which is also the one [`Sink::dmabuf`]
            // falls back to: the card's own on a machine with NVIDIA's, or an
            // encoder FFmpeg opens on any other.
            #[cfg(target_os = "linux")]
            Footage::Dmabuf => {
                // NVENC is on the card the picture is already on, so the driver
                // imports it where the compositor drew it and converts it there,
                // and the session that reads it needs no copy of any pixel. On a
                // machine with NVIDIA's card that is the path to open first:
                // FFmpeg has no way from a descriptor into NVENC, so its session
                // would be one opened for a picture it can't read and given up
                // again — a teardown the card's own session does not survive (see
                // [`Sink::offers`]), and a log line for an encoder that never
                // encodes a frame. A card that can't take the path says why once,
                // and every picture goes the processor's way, through the session
                // opened below.
                #[cfg(nvcuda)]
                if encode::nvidia_card()
                    && let Some(cuda) = Cuda::open(codec, width, height, self.fps, aimed)
                {
                    return Ok(Encoding::Cuda(cuda));
                }
                Ok(Encoding::Picture(Encoder::new(
                    codec,
                    width,
                    height,
                    self.fps,
                    aimed,
                    self.effort,
                    self.hardware,
                )?))
            }
        }
    }

    /// Takes Yapfast's controls, reports the stream starting, and fails once
    /// its connection ended. Gives the source a [`Control::Switch`] asked for.
    pub(crate) fn check(
        &mut self,
        reading: pressure::Reading,
        report: &mut dyn FnMut(Report),
    ) -> Result<Option<Source>> {
        // The machine's memory, sampled by the loop once a second rather than
        // from a thread of its own: a share gives back what it can spare while
        // it is short, and says so in its report until it has had the room back
        // for a while. The stream that goes out is never what is given up.
        let now = Instant::now();
        match self.diagnosis.memory.update(reading, now) {
            pressure::Change::Enter(reason) => {
                tracing::info!(
                    why = reason.name(),
                    "the machine is short of memory; a share keeps only what it sends"
                );
                if let Some(video) = self.video.as_mut() {
                    video.trim_pool();
                }
            }
            pressure::Change::Leave => {
                tracing::info!("the machine has room again; a share takes its own back")
            }
            pressure::Change::Same => {}
        }
        let mut switch = None;
        while let Ok(control) = self.controls.try_recv() {
            match control {
                Control::Send(server) => self.server = Some(server),
                Control::Switch(source) => switch = Some(source),
                Control::Preview(shown) => self.preview_shown = shown,
                // The tile showing this share's preview says how large it is
                // when that changes, not with every frame: the preview is
                // written at the size it shows at from then on, and a larger
                // tile grows the frames file once.
                Control::Size { width, height } => {
                    self.preview_tile = tile_size((width, height));
                }
                _ => {}
            }
        }
        match self.video.as_ref().map(|video| video.sender.link()) {
            Some(Link::Connected) if !self.streaming => {
                self.streaming = true;
                self.preview_due = Some(Instant::now());
                report(Report::Streaming);
            }
            // Discord ends the connection when it moves the stream to
            // another server, which then arrives as another Send.
            Some(Link::Ended(reason)) => {
                let lost = *self.lost.get_or_insert_with(Instant::now);
                if lost.elapsed() >= MOVE_GRACE {
                    bail!("Lost the stream connection: {reason}");
                }
            }
            _ => self.lost = None,
        }
        if let Some(thumbnail) = self.thumbnail.take() {
            report(Report::Preview { thumbnail });
        }
        if let Some(video) = self.video.as_mut()
            && self.streaming
            && video.measured.0.elapsed() >= SENDING_EVERY
        {
            let mut sending = video.sending(self.fps);
            // What is captured, which is the half a person can't see from the
            // picture: a screen shared from the card and the same screen
            // shared from the processor look alike once they arrive.
            sending.capture = self.capture.clone();
            // And where it went: the adapters, the way the window shows it,
            // whether a preview is being made, why the share isn't on the path
            // it asked for, and what the machine had it give back.
            sending.presentation = self.diagnosis.presentation;
            sending.capture_luid = self.diagnosis.gpu_luid;
            sending.encode_luid = self.diagnosis.gpu_luid;
            sending.present_luid = self.writer.presented_on();
            sending.preview = self.preview_shown;
            sending.fallback = self.diagnosis.fallback.clone();
            sending.queue_delay_ms = self.diagnosis.queue_delay_ms;
            sending.memory = self
                .diagnosis
                .memory
                .constrained()
                .map(|reason| reason.name().to_owned());
            report(Report::Sending(sending));
        }
        Ok(switch)
    }
}

/// What decodes a watched stream: the card, where the machine's decoder left
/// the picture there and only the tile comes back, or the processor, which
/// reads every frame.
pub(crate) fn decoder_name(decoder: Option<&Live>) -> String {
    match decoder {
        Some(decoder) if decoder.on_the_card() => "the card's decoder".to_owned(),
        _ => "the processor's decoder".to_owned(),
    }
}

/// Whether a picture's luma is black, as a screen is before what's shared
/// draws: Discord's client waits for one that isn't before sending a preview.
pub(crate) fn is_black(y: &[u8]) -> bool {
    // Black's luma is 16; some of a picture of anything is brighter.
    y.iter().step_by(7).all(|&luma| luma <= 24)
}

/// A preview of a picture as Discord's client makes one: scaled down to fit
/// 512 by 288, a JPEG as a data URI.
pub(crate) fn thumbnail(picture: &I420<'_>) -> Result<String> {
    use base64::Engine;
    use image::{ExtendedColorType, codecs::jpeg::JpegEncoder};
    let (width, height) = fit(
        picture.width,
        picture.height,
        THUMBNAIL_WIDTH,
        THUMBNAIL_HEIGHT,
    );
    let mut scratch = vec![];
    let mut rgba = vec![0; (width * height * 4) as usize];
    picture.to_rgba(&mut scratch, &mut rgba, width, height);
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    let mut jpeg = vec![];
    JpegEncoder::new_with_quality(&mut jpeg, 92).encode(
        &rgb,
        width,
        height,
        ExtendedColorType::Rgb8,
    )?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(jpeg)
    ))
}
