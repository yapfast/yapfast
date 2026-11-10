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
use yapfast_plugin_api::message::{FromHelper, ToHelper};

use crate::backend::LoadSpec;
use crate::pipe::{Pipe, Signal};
use crate::runtime::ExtismBackend;
use crate::scheduler::{Config, Message, Scheduler};

/// What `run` was asked to do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Where wasmtime keeps its compiled modules.
    pub cache: Option<PathBuf>,
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
    if dropped > 0 {
        warn!(dropped, "dropped Extism's environment switches");
    }
    let pipe = Pipe::stdio().context("setting up the pipe to Yapfast")?;
    // Logging comes after the pipe, because a plugin's own lines travel as
    // frames: the subscriber files them under their plugin instead of letting
    // them reach the terminal as if the helper had said them.
    crate::logging::init(pipe.reports());

    // Without a cache directory the runtime compiles every module from cold,
    // which costs about eight times as much; the parent passes one, and a
    // helper started by hand gets a directory that outlives the run.
    let cache = options
        .cache
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join("yapfast-plugins-cache"));
    let backend =
        ExtismBackend::new(&cache, pipe.reports()).context("starting the WebAssembly runtime")?;
    let mut scheduler = Scheduler::start(backend, pipe.outbox(), pipe.waker(), Config::default())
        .context("starting the plugin workers")?;

    let last = serve(&pipe, &scheduler, options.dev.as_deref());
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

/// Whether the loop goes on after a frame, and what to tell the parent if not.
enum After {
    Continue,
    /// Stop, with the reason the helper is leaving.
    Stop(FromHelper),
}

/// Reads frames until the helper has a reason to stop, and gives it.
fn serve(
    pipe: &Pipe,
    scheduler: &Scheduler<ExtismBackend>,
    dev: Option<&Path>,
) -> Option<FromHelper> {
    loop {
        match pipe.recv() {
            Signal::Frame(frame) => match dispatch(frame, pipe, scheduler, dev) {
                After::Continue => {}
                After::Stop(report) => return Some(report),
            },
            // The parent closed stdin: the cleanest goodbye there is, and
            // nothing is left to tell it.
            Signal::Closed => return None,
            Signal::Broken(error) => {
                return Some(FromHelper::Bye {
                    reason: format!("the pipe to Yapfast broke: {error}"),
                });
            }
            Signal::Finished => {
                return Some(FromHelper::Bye {
                    reason: "no plugin instance is left".into(),
                });
            }
        }
    }
}

/// Turns one frame into what the scheduler should do.
fn dispatch(
    frame: Frame,
    pipe: &Pipe,
    scheduler: &Scheduler<ExtismBackend>,
    dev: Option<&Path>,
) -> After {
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
        ToHelper::Hello { api } => hello(pipe, api, frame.id),
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
                },
            });
            After::Continue
        }
        ToHelper::Unload { plugin } => {
            scheduler.send(Message::Unload { plugin });
            After::Continue
        }
        ToHelper::Activate { plugin } => {
            scheduler.send(Message::Activate { id, plugin });
            After::Continue
        }
        ToHelper::Deactivate { plugin } => {
            scheduler.send(Message::Deactivate { id, plugin });
            After::Continue
        }
        ToHelper::Kick { plugin } => {
            scheduler.send(Message::Kick { plugin });
            After::Continue
        }
        ToHelper::Event { plugin, events } => {
            scheduler.send(Message::Event { id, plugin, events });
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
        ToHelper::Return {
            plugin,
            call,
            answer,
        } => {
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
        ToHelper::Shutdown => After::Stop(FromHelper::Bye {
            reason: "Yapfast asked the helper to exit".into(),
        }),
    }
}

/// Answers the parent's opening frame with what this helper speaks.
fn hello(pipe: &Pipe, api: u32, id: u64) -> After {
    if api > API_VERSION {
        // Loading a plugin written for a newer ABI would fail at
        // instantiation, where nobody could explain it; saying so here is the
        // one thing the helper can do about it.
        return After::Stop(FromHelper::Bye {
            reason: format!("Yapfast speaks plugin API {api} and this helper speaks {API_VERSION}"),
        });
    }
    let ready = FromHelper::Ready {
        api: API_VERSION,
        version: env!("CARGO_PKG_VERSION").to_owned(),
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
