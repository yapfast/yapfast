//! Sound through PulseAudio, or PipeWire's PulseAudio server.

use std::{cell::RefCell, rc::Rc, time::Duration};

use anyhow::{Context as _, Result, anyhow, bail};
use libpulse_binding::{
    callbacks::ListResult,
    context::{self, Context, FlagSet as ContextFlags},
    def::BufferAttr,
    mainloop::standard::{IterateResult, Mainloop},
    sample::{Format as PulseFormat, Spec},
    stream::Direction,
};
use libpulse_simple_binding::Simple;

use super::Format;
use crate::voice::NAMED;

pub struct Output(Simple);

impl Output {
    pub fn open(
        speaker: Option<&str>,
        name: &'static str,
        format: Format,
        queue: Option<Duration>,
    ) -> Result<Self> {
        let sink = sink(speaker)?;
        let attributes = queue
            .map(|queue| -> Result<BufferAttr> {
                Ok(BufferAttr {
                    maxlength: u32::MAX,
                    tlength: u32::try_from(format.bytes(queue))?,
                    prebuf: u32::MAX,
                    minreq: u32::MAX,
                    fragsize: u32::MAX,
                })
            })
            .transpose()?;
        open(
            Direction::Playback,
            sink.as_deref(),
            name,
            format,
            attributes.as_ref(),
        )
        .map(Self)
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.0.write(bytes).map_err(|error| anyhow!("{error}"))
    }

    pub fn drain(&mut self) -> Result<()> {
        self.0.drain().map_err(|error| anyhow!("{error}"))
    }

    pub fn flush(&mut self) {
        let _ = self.0.flush();
    }

    pub fn latency(&self) -> Duration {
        self.0
            .get_latency()
            .map_or(Duration::ZERO, |latency| Duration::from_micros(latency.0))
    }
}

pub struct Input(Simple);

impl Input {
    pub fn open(
        microphone: Option<&str>,
        name: &'static str,
        format: Format,
        fragment: Duration,
    ) -> Result<Self> {
        let source = source(microphone)?;
        let attributes = BufferAttr {
            maxlength: u32::MAX,
            tlength: u32::MAX,
            prebuf: u32::MAX,
            minreq: u32::MAX,
            fragsize: u32::try_from(format.bytes(fragment))?,
        };
        open(
            Direction::Record,
            source.as_deref(),
            name,
            format,
            Some(&attributes),
        )
        .map(Self)
    }

    pub fn read(&mut self, bytes: &mut [u8]) -> Result<()> {
        self.0.read(bytes).map_err(|error| anyhow!("{error}"))
    }
}

fn open(
    direction: Direction,
    device: Option<&str>,
    name: &str,
    format: Format,
    attributes: Option<&BufferAttr>,
) -> Result<Simple> {
    let spec = Spec {
        // The samples go as they are in memory.
        format: if format.float {
            PulseFormat::FLOAT32NE
        } else {
            PulseFormat::S16NE
        },
        channels: u8::try_from(format.channels)?,
        rate: format.rate,
    };
    Simple::new(
        None, "Yapfast", direction, device, name, &spec, None, attributes,
    )
    .map_err(|error| anyhow!("{error}"))
}

/// The PulseAudio source behind a WebRTC device: its first entry is the
/// default source, prefixed "default: ", and the rest are the sources that
/// are not monitors, by description.
fn source(microphone: Option<&str>) -> Result<Option<String>> {
    device(microphone, false)
        .map_err(|error| error.context("this microphone is no longer connected"))
}

/// The PulseAudio sink behind a WebRTC playout device, named the same way.
fn sink(speakers: Option<&str>) -> Result<Option<String>> {
    device(speakers, true).map_err(|error| error.context("these speakers are no longer connected"))
}

fn device(id: Option<&str>, sink: bool) -> Result<Option<String>> {
    let Some(description) = id.and_then(|id| id.strip_prefix(NAMED)) else {
        return Ok(None);
    };
    if description.starts_with("default: ") {
        return Ok(None);
    }
    list(sink)?
        .into_iter()
        .find(|found| found.description == description)
        .map(|found| Some(found.name))
        .context("device not found")
}

/// The sink or source a stream for `id` would open now, by its name and
/// index, which a device gets anew each time it comes back: the server moves
/// a stream to another device when its own goes away, and leaves it there
/// when that returns. Empty for the default, which streams follow anyway.
#[cfg(feature = "discord")]
pub fn endpoint(id: Option<&str>, sink: bool) -> String {
    let Some(description) = id
        .and_then(|id| id.strip_prefix(NAMED))
        .filter(|description| !description.starts_with("default: "))
    else {
        return String::new();
    };
    list(sink)
        .ok()
        .and_then(|devices| {
            devices
                .into_iter()
                .find(|found| found.description == description)
        })
        .map(|found| format!("{}#{}", found.name, found.index))
        .unwrap_or_default()
}

/// A sink or source as the server lists it.
struct Listed {
    name: String,
    description: String,
    #[cfg_attr(not(feature = "discord"), allow(dead_code))]
    index: u32,
}

/// Every sink, or every source that is not a monitor.
fn list(sinks: bool) -> Result<Vec<Listed>> {
    let mut mainloop = Mainloop::new().context("no PulseAudio main loop")?;
    let mut context = Context::new(&mainloop, "Yapfast").context("no PulseAudio context")?;
    context
        .connect(None, ContextFlags::NOFLAGS, None)
        .map_err(|error| anyhow!("{error}"))
        .context("could not reach the sound server")?;
    loop {
        iterate(&mut mainloop)?;
        match context.get_state() {
            context::State::Ready => break,
            context::State::Failed | context::State::Terminated => {
                bail!("the sound server refused Yapfast")
            }
            _ => {}
        }
    }
    let found = Rc::new(RefCell::new(Vec::new()));
    let done = Rc::new(RefCell::new(false));
    let introspect = context.introspect();
    // Each listing is its own operation type; keep whichever runs alive.
    let _sinks = sinks.then(|| {
        let found = found.clone();
        let done = done.clone();
        introspect.get_sink_info_list(move |result| match result {
            ListResult::Item(info) => {
                if let (Some(name), Some(description)) = (&info.name, &info.description) {
                    found.borrow_mut().push(Listed {
                        name: name.to_string(),
                        description: description.to_string(),
                        index: info.index,
                    });
                }
            }
            ListResult::End | ListResult::Error => *done.borrow_mut() = true,
        })
    });
    let _sources = (!sinks).then(|| {
        let found = found.clone();
        let done = done.clone();
        introspect.get_source_info_list(move |result| match result {
            ListResult::Item(info) if info.monitor_of_sink.is_none() => {
                if let (Some(name), Some(description)) = (&info.name, &info.description) {
                    found.borrow_mut().push(Listed {
                        name: name.to_string(),
                        description: description.to_string(),
                        index: info.index,
                    });
                }
            }
            ListResult::Item(_) => {}
            ListResult::End | ListResult::Error => *done.borrow_mut() = true,
        })
    });
    while !*done.borrow() {
        iterate(&mut mainloop)?;
    }
    context.disconnect();
    Ok(found.take())
}

fn iterate(mainloop: &mut Mainloop) -> Result<()> {
    match mainloop.iterate(true) {
        IterateResult::Success(_) => Ok(()),
        IterateResult::Quit(_) | IterateResult::Err(_) => {
            bail!("the sound server connection ended")
        }
    }
}

#[cfg(feature = "discord")]
pub use others::others;

/// What other applications play, for Go Live's sound.
#[cfg(feature = "discord")]
mod others {
    use std::{
        cell::RefCell,
        collections::HashMap,
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        thread::JoinHandle,
    };

    use anyhow::{Context as _, Result, anyhow, bail};
    use libpulse_binding::{
        callbacks::ListResult,
        context::{
            self, Context, FlagSet as ContextFlags,
            introspect::SinkInputInfo,
            subscribe::{Facility, InterestMaskSet, Operation},
        },
        def::BufferAttr,
        mainloop::standard::{IterateResult, Mainloop},
        sample::{Format as PulseFormat, Spec},
        stream::{FlagSet as StreamFlags, PeekResult, Stream},
        time::MicroSeconds,
    };

    use super::super::{Hear, Heard, RATE};

    /// Monitors each application's playback stream on its own
    /// (`pa_stream_set_monitor_stream`, as pavucontrol's level meters do).
    pub fn others(
        skip: fn(&str) -> bool,
        heard: Hear,
        stop: Arc<AtomicBool>,
    ) -> Result<JoinHandle<()>> {
        let (ready, started) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("yapfast-stream-capture".into())
            .spawn(move || {
                if let Err(error) = run(skip, heard, &stop, &ready) {
                    let _ = ready.try_send(Err(anyhow!("{error:#}")));
                    tracing::warn!(error = format!("{error:#}"), "stopped capturing sound");
                }
            })
            .context("could not start capturing sound")?;
        started
            .recv()
            .context("sound capture stopped before it started")??;
        Ok(thread)
    }

    /// What PulseAudio's callbacks found, handled after each dispatch so
    /// no callback touches the context.
    enum Change {
        /// A playback stream appeared or changed: look it up.
        Check(u32),
        /// A playback stream, the sink it plays on, and its process.
        Found {
            input: u32,
            sink: u32,
            process: Option<u32>,
            /// An application to leave out.
            skipped: bool,
        },
        /// The monitor source of the sink a stream plays on.
        Monitor {
            input: u32,
            sink: u32,
            process: Option<u32>,
            source: u32,
        },
        Gone(u32),
    }

    type Changes = Rc<RefCell<Vec<Change>>>;
    type Shared = Rc<RefCell<Hear>>;
    type Monitors = HashMap<u32, (u32, Rc<RefCell<Stream>>)>;

    fn run(
        skip: fn(&str) -> bool,
        heard: Hear,
        stop: &AtomicBool,
        ready: &mpsc::SyncSender<Result<()>>,
    ) -> Result<()> {
        let heard: Shared = Rc::new(RefCell::new(heard));
        let mut mainloop = Mainloop::new().context("no PulseAudio main loop")?;
        let context = Rc::new(RefCell::new(
            Context::new(&mainloop, "Yapfast").context("no PulseAudio context")?,
        ));
        context
            .borrow_mut()
            .connect(None, ContextFlags::NOFLAGS, None)
            .context("could not reach PulseAudio")?;
        loop {
            if let IterateResult::Quit(_) | IterateResult::Err(_) = mainloop.iterate(true) {
                bail!("PulseAudio's main loop failed");
            }
            match context.borrow().get_state() {
                context::State::Ready => break,
                context::State::Failed | context::State::Terminated => {
                    bail!("could not reach PulseAudio")
                }
                _ => {}
            }
        }
        let changes = Changes::default();
        let found = changes.clone();
        context.borrow_mut().set_subscribe_callback(Some(Box::new(
            move |facility, operation, index| {
                if facility != Some(Facility::SinkInput) {
                    return;
                }
                found.borrow_mut().push(match operation {
                    Some(Operation::Removed) => Change::Gone(index),
                    _ => Change::Check(index),
                });
            },
        )));
        context
            .borrow_mut()
            .subscribe(InterestMaskSet::SINK_INPUT, |_| {});
        let found = changes.clone();
        context
            .borrow()
            .introspect()
            .get_sink_input_info_list(move |result| {
                if let ListResult::Item(info) = result {
                    found.borrow_mut().push(found_change(info, skip));
                }
            });
        let _ = ready.send(Ok(()));

        // Yapfast's own processes: this worker and the Yapfast that started it.
        let excluded = [std::process::id(), std::os::unix::process::parent_id()];
        let mut streams = Monitors::new();
        while !stop.load(Ordering::Relaxed) {
            mainloop
                .prepare(Some(MicroSeconds(50_000)))
                .map_err(|error| anyhow!("{error}"))?;
            mainloop.poll().map_err(|error| anyhow!("{error}"))?;
            mainloop.dispatch().map_err(|error| anyhow!("{error}"))?;
            if let context::State::Failed | context::State::Terminated =
                context.borrow().get_state()
            {
                bail!("PulseAudio went away");
            }
            let pending = std::mem::take(&mut *changes.borrow_mut());
            for change in pending {
                match change {
                    Change::Check(input) => {
                        let found = changes.clone();
                        context
                            .borrow()
                            .introspect()
                            .get_sink_input_info(input, move |result| {
                                if let ListResult::Item(info) = result {
                                    found.borrow_mut().push(found_change(info, skip));
                                }
                            });
                    }
                    Change::Found {
                        input,
                        sink,
                        process,
                        skipped,
                    } => {
                        if skipped || process.is_some_and(|process| excluded.contains(&process)) {
                            continue;
                        }
                        if streams.get(&input).is_some_and(|(on, _)| *on == sink) {
                            continue;
                        }
                        let found = changes.clone();
                        context
                            .borrow()
                            .introspect()
                            .get_sink_info_by_index(sink, move |result| {
                                if let ListResult::Item(info) = result {
                                    found.borrow_mut().push(Change::Monitor {
                                        input,
                                        sink,
                                        process,
                                        source: info.monitor_source,
                                    });
                                }
                            });
                    }
                    Change::Monitor {
                        input,
                        sink,
                        process,
                        source,
                    } => {
                        forget(&mut streams, &heard, input);
                        match monitor(&mut context.borrow_mut(), &heard, input, process, source) {
                            Ok(stream) => {
                                streams.insert(input, (sink, stream));
                            }
                            Err(error) => {
                                tracing::warn!(
                                    error = format!("{error:#}"),
                                    "could not capture an application's sound"
                                )
                            }
                        }
                    }
                    Change::Gone(input) => forget(&mut streams, &heard, input),
                }
            }
        }
        for input in streams.keys().copied().collect::<Vec<_>>() {
            forget(&mut streams, &heard, input);
        }
        context.borrow_mut().disconnect();
        Ok(())
    }

    fn found_change(info: &SinkInputInfo, skip: fn(&str) -> bool) -> Change {
        Change::Found {
            input: info.index,
            sink: info.sink,
            process: info
                .proplist
                .get_str("application.process.id")
                .and_then(|process| process.parse().ok()),
            skipped: ["application.name", "application.process.binary"]
                .into_iter()
                .filter_map(|key| info.proplist.get_str(key))
                .any(|name| skip(&name)),
        }
    }

    /// Records one playback stream from the monitor source of its sink, as
    /// 48 kHz stereo floats.
    fn monitor(
        context: &mut Context,
        heard: &Shared,
        input: u32,
        process: Option<u32>,
        source: u32,
    ) -> Result<Rc<RefCell<Stream>>> {
        let spec = Spec {
            format: PulseFormat::FLOAT32NE,
            rate: RATE,
            channels: 2,
        };
        let stream = Stream::new(context, "Yapfast stream sound", &spec, None)
            .context("PulseAudio refused a capture stream")?;
        let stream = Rc::new(RefCell::new(stream));
        stream
            .borrow_mut()
            .set_monitor_stream(input)
            .map_err(|error| anyhow!("{error}"))?;
        (heard.borrow_mut())(Heard::Started {
            source: input,
            process,
        });
        let weak = Rc::downgrade(&stream);
        let into = heard.clone();
        // For fragments that don't start on a float's alignment.
        let mut unaligned: Vec<f32> = Vec::new();
        stream
            .borrow_mut()
            .set_read_callback(Some(Box::new(move |_| {
                let Some(stream) = weak.upgrade() else { return };
                let mut stream = stream.borrow_mut();
                while let Ok(peeked) = stream.peek() {
                    match peeked {
                        PeekResult::Data(bytes) => {
                            // SAFETY: every bit pattern is a valid f32.
                            let samples = match unsafe { bytes.align_to::<f32>() } {
                                ([], samples, []) => samples,
                                _ => {
                                    unaligned.clear();
                                    unaligned.extend(bytes.chunks_exact(4).map(|sample| {
                                        f32::from_ne_bytes([
                                            sample[0], sample[1], sample[2], sample[3],
                                        ])
                                    }));
                                    &unaligned
                                }
                            };
                            (into.borrow_mut())(Heard::Samples {
                                source: input,
                                samples,
                            });
                        }
                        PeekResult::Hole(_) => {}
                        PeekResult::Empty => break,
                    }
                    if stream.discard().is_err() {
                        break;
                    }
                }
            })));
        // About 10 ms at a time.
        let attributes = BufferAttr {
            maxlength: u32::MAX,
            tlength: u32::MAX,
            prebuf: u32::MAX,
            minreq: u32::MAX,
            fragsize: RATE / 100 * 2 * 4,
        };
        stream
            .borrow_mut()
            .connect_record(
                Some(&source.to_string()),
                Some(&attributes),
                StreamFlags::ADJUST_LATENCY | StreamFlags::DONT_MOVE,
            )
            .map_err(|error| anyhow!("{error}"))?;
        tracing::debug!(?process, "capturing an application's sound");
        Ok(stream)
    }

    /// Stops recording a playback stream.
    fn forget(streams: &mut Monitors, heard: &Shared, input: u32) {
        if let Some((_, stream)) = streams.remove(&input) {
            let mut stream = stream.borrow_mut();
            stream.set_read_callback(None);
            let _ = stream.disconnect();
        }
        (heard.borrow_mut())(Heard::Stopped { source: input });
    }
}
