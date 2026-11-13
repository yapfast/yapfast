//! `stream_worker`'s watch.

use super::*;

/// How a watched stream's sound plays, and Yapfast's word on hearing it.
pub(crate) struct Sound {
    pub(crate) speakers: Option<String>,
    pub(crate) hearing: Arc<AtomicBool>,
    pub(crate) volume: Arc<stream_audio::Volume>,
    pub(crate) controls: channel::Receiver<Control>,
}

/// What a watched stream's frames are decoded with, and whether the frame in
/// hand may reach it.
///
/// A decoder that has just failed, or just been replaced because the stream
/// turned to another codec, holds nothing the frames after it build on: each
/// references a picture it never decoded, and would fail too. So the watch
/// waits for a keyframe, as the assembler does when it had to drop frames,
/// instead of spending a decode on every delta of a chain already broken.
pub(crate) struct Decoding {
    pub(crate) decoder: Option<Live>,
    /// No frame reaches the decoder until a keyframe does.
    pub(crate) awaiting_keyframe: bool,
    /// The codec this build has no decoder for, so frames of it are dropped
    /// without trying again. A watch waits through them: the stream's sound
    /// plays on, and a streamer who turns to another codec gets pictures
    /// back.
    pub(crate) undecodable: Option<Codec>,
    /// Whether the machine's decoder gave up on this stream, which leaves the
    /// software one to take the next keyframe and every frame after it.
    pub(crate) broken: bool,
    /// How a decoder is made, which a test makes fail the way a build without
    /// a codec's decoder does.
    pub(crate) make: fn(Codec) -> Result<Live>,
}

/// What became of a frame handed to a [`Decoding`].
///
/// [`Decoded::Waiting`] and [`Decoded::Held`] are frames the watch drops as
/// they are: one before any work is done on it, the other after the decoder
/// took it and kept it.
pub(crate) enum Decoded<'a> {
    /// A picture to show.
    Picture(Frame<'a>),
    Held,
    Waiting,
    /// The decoder couldn't use the frame, so the frames after it wait.
    Failed(anyhow::Error),
}

impl Decoding {
    pub(crate) fn new() -> Self {
        Self {
            decoder: None,
            // The assembler starts a stream at a keyframe.
            awaiting_keyframe: false,
            undecodable: None,
            broken: false,
            make: Live::new,
        }
    }

    /// Decodes `frame` if it should be; [`Decoded::Waiting`] says it wasn't
    /// worth the work and the next frame may be.
    pub(crate) fn decode(&mut self, frame: &ReceivedFrame) -> Decoded<'_> {
        let codec = frame.codec;
        if self
            .decoder
            .as_ref()
            .is_some_and(|decoder| decoder.codec() != codec)
        {
            // Nothing one decoder held carries into another: the frames
            // after a codec change reference pictures only the new decoder
            // makes.
            self.decoder = None;
            self.awaiting_keyframe = true;
        }
        if self.decoder.is_none() {
            if self.undecodable == Some(codec) {
                return Decoded::Waiting;
            }
            let make = self.make;
            let made = if self.broken {
                Live::software(codec)
            } else {
                make(codec)
            };
            match made {
                Ok(decoder) => {
                    self.decoder = Some(decoder);
                    self.undecodable = None;
                }
                Err(error) => {
                    // A codec this build can't decode ends this stream's
                    // video, not the watch: its sound plays on, and a stream
                    // Discord moves to a codec this build has gets pictures
                    // again.
                    tracing::warn!(
                        error = format!("{error:#}"),
                        codec = ?codec,
                        "no decoder for a watched stream"
                    );
                    self.undecodable = Some(codec);
                    return Decoded::Waiting;
                }
            }
        }
        if self.awaiting_keyframe {
            if !frame.keyframe {
                return Decoded::Waiting;
            }
            self.awaiting_keyframe = false;
        }
        let Some(decoder) = self.decoder.as_mut() else {
            return Decoded::Waiting;
        };
        let on_the_card = decoder.on_the_card();
        match decoder.decode(&frame.data, frame.keyframe) {
            Ok(Some(picture)) => Decoded::Picture(picture),
            Ok(None) => Decoded::Held,
            Err(error) => {
                self.awaiting_keyframe = true;
                // A stream the machine's decoder gave up on is the software
                // decoder's from the next keyframe; one the software decoder
                // couldn't read is read again when the chain is whole.
                self.broken = on_the_card;
                Decoded::Failed(error)
            }
        }
    }

    /// The machine's decoder couldn't show a picture, so it stops being used
    /// for this stream: the frames after it wait for a keyframe, which the
    /// software decoder starts again from.
    pub(crate) fn abandon(&mut self) {
        self.decoder = None;
        self.broken = true;
        self.awaiting_keyframe = true;
    }
}

/// Shows someone's stream: whole frames from the connection, decoded, then
/// scaled down to the tile showing them, and plays its sound.
pub(crate) fn watch(
    server: StreamServer,
    mut writer: Writer,
    (mut max_width, mut max_height): (u32, u32),
    sound: Sound,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<()> {
    let mut receiver = StreamReceiver::start(server, crate::decode::DECODES.to_vec())?;
    // The clock the streamer's sender reports put its sound and picture on,
    // read before the player takes the sounds: what places each of the two
    // positions a viewer's lip sync is measured from.
    let sounds = receiver.take_sounds();
    let clock = sounds.as_ref().map(stream_connection::SoundReceiver::clock);
    let player = sounds.and_then(|sounds| {
        stream_audio::Player::start(
            sounds,
            sound.speakers,
            sound.hearing.clone(),
            sound.volume.clone(),
        )
        .inspect_err(|error| tracing::warn!(error = format!("{error:#}"), "watching without sound"))
        .ok()
    });
    let mut decoding = Decoding::new();
    // A picture that doesn't fit the tile showing it scales through this, at
    // the size it shows; one that does goes to the shared frame as it is.
    let mut scratch = vec![];
    // A whole frame taken off the stream only to learn that a newer picture
    // was already waiting: decoded on the next turn, in order, so taking it
    // never skips one.
    let mut ahead: Option<ReceivedFrame> = None;
    // When the last picture reached the shared frame, and how many a newer one
    // replaced before it could.
    let mut showing: Option<Instant> = None;
    let mut replaced = 0u64;
    let mut failures = 0u64;
    // Whether a window shows these pictures at all, as the UI says when that
    // changes: a hidden or minimised window shows none, and a picture nobody
    // can see isn't converted or written. The stream is decoded all the same.
    let mut visible = true;
    // When the watch last said how it is decoding and showing the stream: the
    // same once a second a share's report keeps, so a viewer's Stream tab has
    // something true to show too.
    let mut told = Instant::now() - SENDING_EVERY;
    // The streamer's two clocks as its sender reports put them, and where the
    // sound audible now sits on the picture's timeline: the sound is the master
    // the pictures are judged against. Both reset on a stop, a deafen or a
    // clock the streamer broke, since nothing measured across one means anything
    // after.
    let mut timeline = av_timing::Timeline::new();
    // The stream time of the picture last handed to the window and how its
    // time is known, which the presentation skew is read against.
    let mut presented: Option<av_timing::Presented> = None;
    // The pictures the sound had passed by, said once a second with the rest of
    // how the watch is doing.
    let mut dropped = 0u64;
    while !stop.load(Ordering::Relaxed) {
        while let Ok(control) = sound.controls.try_recv() {
            match control {
                Control::Hear(hearing) => {
                    sound.hearing.store(hearing, Ordering::Relaxed);
                    // Deafened: nothing of this stream is heard, so the sound
                    // is no longer what the picture is timed against.
                    if !hearing {
                        timeline.reset();
                        presented = None;
                    }
                }
                Control::Volume(volume) => sound.volume.set(volume),
                Control::Shown(shown) => visible = shown,
                // The tile showing the stream says how large it is when that
                // changes, not with every frame: the stream is scaled to it
                // from then on, and the frames file is grown once to hold it.
                Control::Size { width, height } => {
                    if let Some(size) = tile_size((width, height))
                        && size != (max_width, max_height)
                    {
                        (max_width, max_height) = size;
                        if let Err(error) = writer.reserve(max_width, max_height) {
                            tracing::warn!(
                                error = format!("{error:#}"),
                                "could not make room in the frames for the tile"
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        if let Link::Ended(reason) = receiver.link() {
            bail!("Lost the stream: {reason}");
        }
        let Some(frame) = ahead
            .take()
            .or_else(|| receiver.next(Duration::from_millis(100)))
        else {
            continue;
        };
        // What decodes this stream, read before the decoder is borrowed to
        // decode: a watch says it once a second while it plays.
        let decoder = decoder_name(decoding.decoder.as_ref());
        let picture = match decoding.decode(&frame) {
            Decoded::Picture(picture) => picture,
            Decoded::Held | Decoded::Waiting => continue,
            Decoded::Failed(error) => {
                failures += 1;
                // The first of a run, and then now and again.
                if failures == 1 || failures.is_multiple_of(100) {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        failures,
                        keyframe = frame.keyframe,
                        size = frame.data.len(),
                        "a stream frame didn't decode"
                    );
                }
                receiver.request_keyframe();
                continue;
            }
        };
        // The whole frame that would replace this picture, if it is already
        // here. It waits for the next turn rather than being dropped, so
        // looking ahead never skips a frame the decoder needs.
        let newer = match receiver.next(Duration::ZERO) {
            Some(frame) => {
                ahead = Some(frame);
                true
            }
            None => false,
        };
        // How the watch is doing, said once a second while it plays: a hidden
        // tile is still a stream being decoded, and its report says so rather
        // than going quiet and leaving the call's Stream Tab with nothing.
        if told.elapsed() >= SENDING_EVERY {
            told = Instant::now();
            let (width, height) = picture.size();
            // The streamer's two clocks, read where the connection keeps them:
            // its sender reports come about once a second, so this is read
            // once a second too rather than locking them for every frame. The
            // timeline carries each on from its own report between reads.
            let now = Instant::now();
            let (mut heard, mut queued) = (None, None);
            if let Some(clock) = clock.as_ref() {
                if let Some(report) = clock.audio() {
                    timeline.heard(av_timing::Clock::Sound, mapping_of(report));
                }
                if let Some(report) = clock.video() {
                    timeline.heard(av_timing::Clock::Picture, mapping_of(report));
                }
            }
            if let Some((played, held)) = player.as_ref().and_then(stream_audio::Player::heard) {
                heard = Some(played);
                queued = Some(held);
            }
            // How far the picture runs from the sound, on the streamer's own
            // timeline: the two clocks carried to now, and what the device
            // says it has played against the picture last handed over. Both
            // are `None` while a mapping is missing or older than five
            // seconds, and the device's buffer is only in the second.
            let held = Duration::from_millis(u64::from(queued.unwrap_or(0)));
            let arrival_skew = timeline.arrival_ms(now);
            let presentation_skew =
                timeline.presentation_ms(now, held, presented.map(|presented| presented.time_ms));
            tracing::debug!(
                arrival_skew_ms = ?arrival_skew,
                presentation_skew_ms = ?presentation_skew,
                dropped,
                "watched stream A/V timing"
            );
            report(Report::Sending(StreamStats {
                codec: frame.codec.name().to_owned(),
                decoder: Some(decoder.clone()),
                // A watched picture is converted into the frames file for the
                // window to show: this side copies it, and says so.
                presentation: Some(Presentation::Copy),
                present_luid: writer.presented_on(),
                width,
                height,
                preview: visible,
                sound_heard_ms: heard,
                sound_queued_ms: queued,
                arrival_skew_ms: arrival_skew,
                presentation_skew_ms: presentation_skew,
                late_pictures: Some(dropped),
                ..StreamStats::default()
            }));
        }
        match worth_showing(visible, showing, Instant::now(), newer) {
            Worth::Show => {}
            Worth::Replaced => {
                replaced += 1;
                // The first of a run, and then now and again, as for a stream
                // frame that didn't decode: a watch that shows fewer pictures
                // than it decodes is one whose decode is outrunning its screen.
                if replaced == 1 || replaced.is_multiple_of(100) {
                    let (width, height) = picture.size();
                    tracing::debug!(
                        replaced,
                        width,
                        height,
                        "a decoded picture a newer one replaced wasn't shown"
                    );
                }
                continue;
            }
            // The frame was decoded — the pictures after it are made from it,
            // and waiting for a keyframe when the window came back would show
            // the still it stopped on until one came — but nothing is
            // converted or written for a window that shows no picture.
            Worth::Unseen => continue,
        }
        // Only the Windows decode path below sets it.
        #[cfg_attr(not(d3d11dec), allow(unused_mut))]
        let mut abandoned = false;
        // The picture's own time on the stream, which the frames file carries to
        // the window: the sender's 90 kHz RTP clock in milliseconds, so a
        // picture is told from the one before it and can be matched against the
        // sound when the two are put on one timeline.
        let time_ms = u64::from(frame.timestamp) * 1_000 / 90_000;
        // The sound already audible is the master: a picture it has passed by
        // more than 80 ms is dropped rather than shown late. One it hasn't
        // reached is shown when it arrives: a decoded picture lives only until
        // the decoder's next frame, so holding it back would mean copying it to
        // keep it, which is a picture's worth of memory and a copy per frame
        // for a wait of at most 80 ms. Nothing here blocks, so neither the UI
        // nor the network thread waits on a picture's turn.
        let now = Instant::now();
        let held = player
            .as_ref()
            .and_then(stream_audio::Player::heard)
            .map_or(Duration::ZERO, |(_, queued)| {
                Duration::from_millis(u64::from(queued))
            });
        if let Some(audible) = timeline.audible_ms(now, held)
            && av_timing::decide(time_ms, audible) == av_timing::Show::Drop
        {
            dropped += 1;
            continue;
        }
        let (width, height) = match picture {
            Frame::Software(picture) => write_picture(
                &mut writer,
                &picture.i420(),
                &mut scratch,
                (max_width, max_height),
                time_ms,
            ),
            // The machine's decoder left the picture on the graphics card, at
            // the size the stream was encoded at: the card scales it to the
            // tile in the same pass that reads its colour, and only the tile
            // comes back to the processor.
            #[cfg(d3d11dec)]
            Frame::Hardware(hardware) => {
                let (stream_width, stream_height) = hardware.size();
                let (width, height) = fit(stream_width, stream_height, max_width, max_height);
                let written = writer.write_timed_rendered(width, height, time_ms, |pixels| {
                    hardware.rgba(pixels, width, height).is_ok()
                });
                if !written {
                    abandoned = true;
                }
                (width, height)
            }
            // The card's decoder on Linux leaves the picture in a DMA-BUF: no
            // pixel of it is copied, and the window imports it where it lies if
            // it can. A window that can't is what the publish says so, and the
            // stream falls back to the processor's decoder from a keyframe —
            // the picture before this one stays up rather than a blank tile.
            #[cfg(vaapidec)]
            Frame::Hardware(hardware) => {
                let (stream_width, stream_height) = hardware.size();
                let (width, height) = fit(stream_width, stream_height, max_width, max_height);
                let published = hardware
                    .dmabuf()
                    .is_some_and(|picture| writer.publish_dmabuf(&picture));
                hardware.release();
                if !published {
                    abandoned = true;
                }
                (width, height)
            }
        };
        if abandoned {
            tracing::warn!("a picture the graphics card decoded wasn't shown");
            decoding.abandon();
            receiver.request_keyframe();
        }
        let first = showing.is_none();
        // Once the conversion is done, not when it began: what the next
        // picture is measured from is when this one reached the screen.
        showing = Some(Instant::now());
        // What reached the window, for the presentation skew: a worker hands
        // a picture over through the frames file and the renderer's own
        // completion is the window's to observe, so this is its submission and
        // says so.
        if !abandoned {
            presented = Some(av_timing::Presented::submitted(time_ms, Instant::now()));
        }
        if first {
            // What decodes it and where it goes, which is the half a viewer
            // can't tell from the picture: the card leaves it on the card and
            // only the tile comes back, and the processor reads every frame.
            tracing::info!(
                width,
                height,
                codec = ?frame.codec,
                on_the_card = decoding
                    .decoder
                    .as_ref()
                    .is_some_and(Live::on_the_card),
                "showing the stream"
            );
            report(Report::Streaming);
        }
    }
    Ok(())
}

/// Writes `picture` into the shared frames at the size the tile showing it
/// takes: its own when that fits inside `(max_width, max_height)`, and else
/// scaled down to fit, through `scratch`, which is sized once and kept. A
/// stream smaller than its tile keeps its own size — nothing is scaled up —
/// so a small stream costs no more than it is. Says the size it was written
/// at, which is the size it shows at.
pub(crate) fn write_picture(
    writer: &mut Writer,
    picture: &I420<'_>,
    scratch: &mut Vec<u8>,
    (max_width, max_height): (u32, u32),
    time_ms: u64,
) -> (u32, u32) {
    let (width, height) = fit(picture.width, picture.height, max_width, max_height);
    writer.write_timed(width, height, time_ms, |pixels| {
        picture.to_rgba(scratch, pixels, width, height)
    });
    (width, height)
}

/// One of the streamer's clocks as its last sender report puts it, in the
/// timeline's own terms. The connection keeps the two separately, so which
/// clock a report is of is already known here and no SSRC has to be carried.
pub(crate) fn mapping_of(report: stream_connection::SenderReport) -> av_timing::Mapping {
    av_timing::Mapping {
        ntp: report.ntp,
        rtp: report.rtp,
        heard: report.heard,
    }
}

/// What a decoded picture is worth, as no screen takes every one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Worth {
    /// Convert it to RGBA and write it into the shared frames.
    Show,
    /// Let it go: a newer whole frame was already in hand, and the screen
    /// drew the picture before this within [`SHOW_EVERY`] of it, so no screen
    /// would draw it.
    Replaced,
    /// Let it go: no window shows a picture, being hidden to the tray or
    /// minimised. The frame was decoded all the same.
    Unseen,
}

/// What a picture decoded now is worth, given whether a window shows
/// pictures at all, when one last reached the screen, and whether a newer
/// whole frame was already in hand.
///
/// Decoding is never skipped this way — a picture the next one references has
/// to be decoded for the pictures after it, and the whole frame taken to find
/// a newer one is decoded on the next turn. What is skipped is the RGBA
/// conversion of a picture the screen would never show, which is the work the
/// picture the viewer is waiting for would otherwise pay for. A picture that
/// arrives a frame period after the last is always shown, whatever else is in
/// hand, so a watch that keeps up shows every frame and leaves nothing out —
/// and the first picture after a hidden window comes back is one of those,
/// the last shown being long enough ago.
pub(crate) fn worth_showing(
    visible: bool,
    shown: Option<Instant>,
    now: Instant,
    newer: bool,
) -> Worth {
    if !visible {
        return Worth::Unseen;
    }
    match shown {
        Some(shown) if newer && now.duration_since(shown) < SHOW_EVERY => Worth::Replaced,
        _ => Worth::Show,
    }
}
