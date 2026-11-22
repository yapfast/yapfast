//! The helper's main loop: the pipe, the workers, the watchdog, and the
//! runtime they drive.
//!
//! It starts the runtime, answers the parent's `Hello`, and then hands frames to
//! the scheduler until stdin closes or the parent sends `Shutdown`. When the
//! last instance is evicted it exits, so nothing outlives the plugins.
//!
//! This loop does no work itself. Every frame becomes a [`Message`], which the
//! scheduler queues; the answers go out through the pipe the scheduler holds.
//! That is what keeps the loop from being a bottleneck: it only decodes, and
//! spends the rest of its time blocked on the pipe.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use tracing::warn;

use yapfast_plugin_api::API_VERSION;
use yapfast_plugin_api::frame::{Frame, FrameKind};
use yapfast_plugin_api::limits::Limits;
use yapfast_plugin_api::message::{FromHelper, Sandbox, ToHelper};

use crate::backend::{Backend, Events, LoadSpec};
use crate::pipe::{Pipe, Signal};
use crate::runtime::ExtismBackend;
use crate::scheduler::{Config, Message, Scheduler};

/// What `run` was asked to do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Where wasmtime keeps its compiled modules.
    pub cache: Option<PathBuf>,
    /// The plugin store the modules are read from: the one folder outside the
    /// cache the sandbox lets the helper read.
    pub store: Option<PathBuf>,
    /// A folder a plugin is loaded from while its author works on it.
    pub dev: Option<PathBuf>,
}

impl Options {
    /// Reads `--cache DIR` and `--dev DIR`, refusing anything else.
    pub fn parse(mut arguments: impl Iterator<Item = String>) -> anyhow::Result<Self> {
        let mut options = Options::default();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--cache" => {
                    let path = arguments.next().context("`--cache` needs a directory")?;
                    options.cache = Some(PathBuf::from(path));
                }
                "--store" => {
                    let path = arguments.next().context("`--store` needs a directory")?;
                    options.store = Some(PathBuf::from(path));
                }
                "--dev" => {
                    let path = arguments.next().context("`--dev` needs a directory")?;
                    options.dev = Some(PathBuf::from(path));
                }
                other => bail!("unknown argument `{other}`; try `yapfast-plugins --help`"),
            }
        }
        Ok(options)
    }
}

/// Runs the helper until the parent goes away.
pub fn run(options: Options) -> anyhow::Result<()> {
    // First, before a pipe, a subscriber or the runtime starts a thread: a
    // switch left in the environment would let a guest write into the frame
    // stream, and the parent's cleared environment is no help to a helper
    // started by hand or by the measurement harness.
    //
    // Safety: nothing else in this process exists to read the environment yet.
    let dropped = unsafe { crate::environment::clear_extism_variables() };

    // Without a cache directory the runtime compiles every module from cold,
    // which costs about eight times as much; the parent passes one, and a
    // helper started by hand gets a directory that outlives the run.
    let cache = options
        .cache
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join("yapfast-plugins-cache"));
    // The sandbox grants folders that exist, so the cache is made first.
    let _ = std::fs::create_dir_all(&cache);
    // Still one thread: Landlock binds this thread and every thread it starts,
    // which is all of them once the pipe and the workers are up.
    let paths = crate::sandbox::Paths {
        read: options
            .store
            .iter()
            .chain(options.dev.iter())
            .cloned()
            .collect(),
        write: vec![cache.clone()],
    };
    // Safety: as above, nothing has started a thread yet.
    let sandbox = unsafe { crate::sandbox::apply(&paths) };

    let pipe = Pipe::stdio().context("setting up the pipe to Yapfast")?;
    // Logging comes after the pipe, because a plugin's own lines travel as
    // frames: the subscriber files them under their plugin instead of letting
    // them reach the terminal as if the helper had said them.
    crate::logging::init(pipe.reports());
    if dropped > 0 {
        warn!(dropped, "dropped Extism's environment switches");
    }
    for reason in &sandbox.missing {
        warn!("OS sandbox: {reason}");
    }
    let backend =
        ExtismBackend::new(&cache, pipe.reports()).context("starting the WebAssembly runtime")?;
    let mut scheduler = Scheduler::start(backend, pipe.outbox(), pipe.waker(), Config::default())
        .context("starting the plugin workers")?;

    let last = serve(&pipe, &scheduler, options.dev.as_deref(), &sandbox);
    // The workers must be gone before the pipe closes: the last thing written
    // is the reason the helper is leaving, and nothing may follow it.
    scheduler.shutdown();
    drop(scheduler);
    // The log subscriber holds a report sender for the life of the process, so
    // it lets go here: the pipe waits for the relay to drain, and a sender that
    // outlived the call would hold the exit.
    crate::logging::close();
    pipe.finish(last);
    Ok(())
}

/// How long the helper waits before it believes nothing is left to do.
///
/// The parent sends `Load` and then `Activate`, two frames: the scheduler finds
/// nothing running between them and says it is done, and on a busy machine the
/// activation is still on its way when it says so. Leaving then lost the
/// activation, and the parent waited out its load deadline for a helper that had
/// already gone. A quiet moment is much cheaper than that.
const IDLE_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether the loop goes on after a frame, and what to tell the parent if not.
enum After {
    Continue,
    /// Stop, with the reason the helper is leaving.
    Stop(Box<FromHelper>),
}

/// Reads frames until the helper has a reason to stop, and gives it.
fn serve(
    pipe: &Pipe,
    scheduler: &Scheduler<ExtismBackend>,
    dev: Option<&Path>,
    sandbox: &crate::sandbox::Report,
) -> Option<FromHelper> {
    // What arrived while the helper was waiting out its grace, before the loop
    // took another turn.
    let mut pending = None;
    loop {
        let signal = match pending.take() {
            Some(signal) => signal,
            None => pipe.recv(),
        };
        match signal {
            Signal::Frame(frame) => match dispatch(frame, pipe, scheduler, dev, sandbox) {
                After::Continue => {}
                After::Stop(report) => return Some(*report),
            },
            // The parent closed stdin: the cleanest goodbye there is, and
            // nothing is left to tell it.
            Signal::Closed => return None,
            Signal::Broken(error) => {
                return Some(FromHelper::Bye {
                    reason: format!("the pipe to Yapfast broke: {error}"),
                });
            }
            // Nothing is running — but the parent's next request may already be
            // in flight, so wait a moment for it before going.
            Signal::Finished => match pipe.recv_timeout(IDLE_GRACE) {
                Some(next) => pending = Some(next),
                None => {
                    return Some(FromHelper::Bye {
                        reason: "no plugin instance is left".into(),
                    });
                }
            },
        }
    }
}

/// Turns one frame into what the scheduler should do.
fn dispatch(
    frame: Frame,
    pipe: &Pipe,
    scheduler: &Scheduler<ExtismBackend>,
    dev: Option<&Path>,
    sandbox: &crate::sandbox::Report,
) -> After {
    // An event batch, the one frame that comes by the thousand, is read as its
    // two arrays and handed on as the bytes the parent wrote.
    if let Some(batch) = event_frame(&frame.payload) {
        let id = match frame.kind {
            FrameKind::Event => 0,
            _ => frame.id,
        };
        scheduler.send(Message::Event {
            id,
            plugin: batch.plugin,
            events: Events::raw(batch.events, batch.topics),
        });
        return After::Continue;
    }
    let request: ToHelper = match serde_json::from_slice(&frame.payload) {
        Ok(request) => request,
        Err(error) => {
            // A frame that doesn't decode means the two sides disagree about
            // the API, which the handshake should have caught; dropping it is
            // better than dropping the plugins that are still working.
            warn!(%error, "a frame from Yapfast didn't decode; it is dropped");
            return After::Continue;
        }
    };
    // An event batch sent in a frame of its own kind is fire and forget; the
    // same batch sent as a request is answered with `EventDone`.
    let id = match frame.kind {
        FrameKind::Event => 0,
        _ => frame.id,
    };
    match request {
        ToHelper::Hello { api } => hello(pipe, api, frame.id, sandbox),
        ToHelper::Limits {
            memory_mib,
            idle_minutes,
        } => {
            apply_limits(scheduler, memory_mib, idle_minutes);
            After::Continue
        }
        ToHelper::Load {
            plugin,
            wasm,
            memory_pages,
            wasi,
        } => {
            scheduler.send(Message::Load {
                spec: LoadSpec {
                    plugin,
                    wasm: module_path(dev, wasm),
                    memory_pages,
                    wasi,
                    state: None,
                    hooks: false,
                },
            });
            After::Continue
        }
        ToHelper::Decorate {
            plugin,
            request: decorate,
        } => {
            let message = decorate.message.clone();
            scheduler.send(Message::Decorate {
                id,
                plugin,
                message,
                request: decorate,
            });
            After::Continue
        }
        ToHelper::Identity { plugin, request } => {
            scheduler.send(Message::Identity {
                id,
                plugin,
                request,
            });
            After::Continue
        }
        ToHelper::CodeBlock { plugin, request } => {
            scheduler.send(Message::CodeBlock {
                id,
                plugin,
                request,
            });
            After::Continue
        }
        ToHelper::DecorateMember {
            plugin,
            request: decorate,
        } => {
            let member = decorate.member.clone();
            scheduler.send(Message::DecorateMember {
                id,
                plugin,
                member,
                request: decorate,
            });
            After::Continue
        }
        ToHelper::Input { plugin, input } => {
            scheduler.send(Message::Input { id, plugin, input });
            After::Continue
        }
        ToHelper::Kept {
            plugin,
            settings,
            storage,
        } => {
            // Kept here and not by the scheduler: a guest's read takes it at
            // once, from whichever thread runs the guest.
            crate::runtime::kept::keep(plugin, settings, storage);
            After::Continue
        }
        ToHelper::Unload { plugin } => {
            crate::runtime::kept::forget(&plugin);
            scheduler.send(Message::Unload { plugin });
            After::Continue
        }
        ToHelper::Activate { plugin, load } => {
            let load = load.map(|load| LoadSpec {
                plugin: plugin.clone(),
                wasm: module_path(dev, load.wasm),
                memory_pages: load.memory_pages,
                wasi: load.wasi,
                state: load.state,
                hooks: load.hooks,
            });
            scheduler.send(Message::Activate { id, plugin, load });
            After::Continue
        }
        ToHelper::Deactivate { plugin } => {
            scheduler.send(Message::Deactivate { id, plugin });
            After::Continue
        }
        ToHelper::Kick { plugin, timer } => {
            scheduler.send(Message::Kick { plugin, timer });
            After::Continue
        }
        ToHelper::Event {
            plugin,
            events,
            topics,
        } => {
            scheduler.send(Message::Event {
                id,
                plugin,
                events: Events::new(&events, &topics),
            });
            After::Continue
        }
        ToHelper::Hook { chain, request } => {
            scheduler.send(Message::Hook { id, chain, request });
            After::Continue
        }
        ToHelper::Command { plugin, command } => {
            scheduler.send(Message::Command {
                id,
                plugin,
                command,
            });
            After::Continue
        }
        ToHelper::Invoke {
            plugin,
            function,
            args_base64,
        } => {
            scheduler.send(Message::Invoke {
                id,
                plugin,
                function,
                args_base64,
            });
            After::Continue
        }
        ToHelper::Return {
            plugin,
            call,
            answer,
        } => {
            // A guest call waiting for this answer takes it now; only one
            // that was told `pending` hears it in a batch.
            let Some(answer) = crate::backend::answers::deliver(&plugin, call, answer) else {
                return After::Continue;
            };
            scheduler.send(Message::Return {
                plugin,
                call,
                answer,
            });
            After::Continue
        }
        ToHelper::Evict { plugin } => {
            scheduler.send(Message::Evict { plugin });
            After::Continue
        }
        ToHelper::Shutdown => After::Stop(Box::new(FromHelper::Bye {
            reason: "Yapfast asked the helper to exit".into(),
        })),
    }
}

/// What dispatch does with a [`ToHelper::Limits`], and the one place the
/// helper turns its two numbers into a budget and a timeout.
///
/// Both are snapped to the steps the settings offer: a frame can carry any
/// number, and one off the list is a rounding question rather than a reason to
/// empty the helper over a budget of zero bytes. Public so the scheduler tests
/// drive the same path the pipe's frames take.
pub fn apply_limits<B: Backend + 'static>(
    scheduler: &Scheduler<B>,
    memory_mib: u32,
    idle_minutes: u32,
) {
    let limits = Limits {
        memory_mib,
        idle_minutes,
    }
    .snapped();
    scheduler.set_limits(limits.idle_after(), limits.bytes());
}

/// Answers the parent's opening frame with what this helper speaks.
fn hello(pipe: &Pipe, api: u32, id: u64, sandbox: &crate::sandbox::Report) -> After {
    if api > API_VERSION {
        // Loading a plugin written for a newer ABI would fail at
        // instantiation, where nobody could explain it; saying so here is the
        // one thing the helper can do about it.
        return After::Stop(Box::new(FromHelper::Bye {
            reason: format!("Yapfast speaks plugin API {api} and this helper speaks {API_VERSION}"),
        }));
    }
    let ready = FromHelper::Ready {
        api: API_VERSION,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        sandbox: Sandbox {
            applied: sandbox.applied.clone(),
            missing: sandbox.missing.clone(),
        },
    };
    if let Err(error) = pipe.outbox().answer(FrameKind::Hello, id, &ready) {
        warn!(%error, "the handshake never reached Yapfast");
    }
    After::Continue
}

/// A module path from the parent. With `--dev`, a relative one names a plugin
/// its author is working on, so it resolves inside that folder rather than in
/// the working directory Yapfast happened to start the helper in.
fn module_path(dev: Option<&Path>, wasm: PathBuf) -> PathBuf {
    match dev {
        Some(folder) if wasm.is_relative() => folder.join(wasm),
        _ => wasm,
    }
}

/// An `event` frame's fields, the arrays left as JSON.
#[derive(serde::Deserialize)]
struct EventFrame<'a> {
    t: &'a str,
    plugin: yapfast_plugin_api::PluginId,
    #[serde(borrow)]
    events: &'a serde_json::value::RawValue,
    #[serde(borrow, default)]
    topics: Option<&'a serde_json::value::RawValue>,
}

/// The frame as an event batch, if it is one: the tag is the first field either
/// side writes, so anything else is decoded the ordinary way, and so is an
/// event frame this reading doesn't take.
fn event_frame(payload: &[u8]) -> Option<EventFrame<'_>> {
    if !payload.starts_with(br#"{"t":"event","#) {
        return None;
    }
    serde_json::from_slice::<EventFrame<'_>>(payload)
        .ok()
        .filter(|frame| frame.t == "event")
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_plugin_api::PluginId;
    use yapfast_plugin_api::event::{EventEnvelope, EventKind, Network, TopicEvent};

    fn envelope(body: &str) -> EventEnvelope {
        EventEnvelope {
            kind: EventKind::MessageCreate,
            network: Network::Discord,
            at: 1,
            room: Some("room-1".into()),
            author: Some("2".into()),
            own: false,
            payload: serde_json::json!({ "body": body, "nested": { "a": [1, 2.5, null] } }),
        }
    }

    /// An event frame read as its two arrays hands the guest the same batch the
    /// ordinary decode would have, topics or not.
    #[test]
    fn an_event_frame_is_read_without_decoding_its_events() {
        let plugin = PluginId::parse("com.example.reader").unwrap();
        let topics = vec![TopicEvent {
            topic: "news".to_owned(),
            from: PluginId::parse("com.example.source").unwrap(),
            at: 2,
            payload: serde_json::json!({ "n": "é \"quoted\"" }),
        }];
        for topics in [Vec::new(), topics] {
            let events = vec![envelope("hi"), envelope("🙂")];
            let payload = serde_json::to_vec(&ToHelper::Event {
                plugin: plugin.clone(),
                events: events.clone(),
                topics: topics.clone(),
            })
            .unwrap();
            let frame = event_frame(&payload).expect("an event frame takes the fast path");
            assert_eq!(frame.plugin, plugin);
            assert_eq!(
                Events::raw(frame.events, frame.topics).batch(),
                Events::new(&events, &topics).batch()
            );
        }
    }

    /// Anything that isn't an event batch is left to the ordinary decode.
    #[test]
    fn another_frame_is_not_an_event_batch() {
        let payload = serde_json::to_vec(&ToHelper::Shutdown).unwrap();
        assert!(event_frame(&payload).is_none());
        assert!(event_frame(br#"{"t":"event","plugin":"not an id","events":[]}"#).is_none());
        assert!(event_frame(br#"{"t":"event","plugin":"com.example.a","events":[}"#).is_none());
    }
}
