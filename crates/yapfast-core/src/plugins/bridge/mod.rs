//! The bridge to the helper: the run loop's task, the frames, and the answers.
//!
//! One Tokio task owns the conversation with the helper process. The
//! application's calls become [`crate::plugins::state::Command`]s into that task;
//! the helper's frames become [`crate::plugins::state::Notice`]s back through the
//! closure the bridge was opened with, which the application turns into an event.
//! A guest's host call arrives as a frame and is answered here, where the token,
//! the storage, the settings, the timer table and the network gate are: none of
//! those ever enter the helper.
//!
//! Three properties shape the code here. An event that no enabled plugin
//! subscribed to must cost one load of an atomic counter and nothing else, so the
//! subscriptions are a bitmask in [`crate::plugins::registry`] and are checked
//! before anything is locked or serialized. A hook is on the path a person waits
//! on, so a chain has the budget the contract gives it and fails open. And the
//! task never blocks: the pipes are read and written by threads of their own,
//! because a helper that stops reading would otherwise stall the timers that
//! exist to notice it.
//!
//! The implementation is split by what a reader is looking for rather than by
//! type: [`mod@supervisor_queries`] answers what the application asks before it
//! draws, [`mod@supervisor_hooks`] carries a hook to the guest, the `session_*`
//! modules are one plugin instance's life through the helper, and `types_*` hold
//! the shapes they pass to each other. Each file is small enough to read in one
//! pass, which is the only reason the split exists.

use crate::plugins::registry::{Registry, Subscriptions};
use crate::plugins::settings;
use crate::plugins::slots::{self, Slot};
use crate::plugins::spawn::{self, Helper, Restarts};
use crate::plugins::state::{AppCall, Notice, PreSend};
use crate::plugins::store::{Module, Store};
use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};
use yapfast_plugin_api::call::{
    self, AddEmoji, AddSticker, ClearSlot, CommandInvocation, CommandOutcome, CopyText,
    DraftRequest, EditMessage, HostCall, HostReturn, HttpRequest, InsertText, Line, MediaGet,
    MessageGet, MessageHistory, MessageRef, Mute, NotifySend, OpenDialog, OpenUrl, PauseInvites,
    PeopleGet, PeopleMember, PlaySound, PluginCall, PluginSubscribe, ReactMessage, RoomGet,
    RoomsList, SendMessage, SetActivity, SetCustomStatus, SetMute, SetSlot, SetStatus,
    SettingsNews, SlotDrawn, Speak, StorageGet, StorageList, StorageSet, TimerCancel, TimerRequest,
};
use yapfast_plugin_api::capability::{Capability, HostPattern};
use yapfast_plugin_api::event::{EventEnvelope, EventKind, TopicEvent};
use yapfast_plugin_api::frame::{self, Frame, FrameKind};
use yapfast_plugin_api::hook::{
    AttachmentPending, Draft, HookKind, HookOutcome, HookRequest, HookSubject, TypingPending,
};
use yapfast_plugin_api::limits;
use yapfast_plugin_api::manifest::CommandDecl;
use yapfast_plugin_api::message::{
    Activation, Fault, FromHelper, LogLevel, LogRecord, PluginStatus, ToHelper,
};
use yapfast_plugin_api::{API_VERSION, PluginId};

mod caps;
mod consts;
mod helpers;

mod painting;
mod session_activation;
mod session_answer;
mod session_calls;
mod session_commands;
mod session_decorate;
mod session_dev;
mod session_drain;
mod session_events;
mod session_fetch;
mod session_frame;
mod session_graph;
mod session_hooks;
mod session_lifecycle;
mod session_slots;
mod session_swap;
mod session_timers;
mod shared;
mod shared_events;
mod shared_held;
mod supervisor_decorate;
mod supervisor_dev;
mod supervisor_hooks;
mod supervisor_install;
mod supervisor_queries;
mod supervisor_run;
mod supervisor_start;
mod supervisor_state;
mod supervisor_teardown;
mod types_dev;
mod types_request;
mod types_session;
#[allow(unused_imports)] // the prelude the files beside this one share
use caps::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use consts::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use helpers::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use painting::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_activation::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_answer::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_calls::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_commands::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_decorate::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_dev::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_drain::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_events::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_fetch::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_frame::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_graph::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_hooks::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_lifecycle::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_slots::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_swap::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use session_timers::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use shared::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use shared_events::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use shared_held::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use supervisor_decorate::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use supervisor_dev::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use supervisor_hooks::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use supervisor_install::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use supervisor_queries::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use supervisor_run::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use supervisor_start::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use supervisor_state::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use supervisor_teardown::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use types_dev::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use types_request::*;
#[allow(unused_imports)] // the prelude the files beside this one share
use types_session::*;

#[cfg(test)]
mod tests;

/// The parent's half of the plugin system.
pub struct Bridge {
    pub(super) data_dir: PathBuf,
    /// A runtime of its own for the work that takes seconds, such as a fetch.
    /// Separate from the supervisor's so a fetch doesn't hold the helper's
    /// thread, and created on first use so nothing runs while nothing is asked.
    pub(super) background: Option<crate::runtime::BackgroundRuntime>,
    /// The fetch tasks that runtime holds, capped: a task cancelled by being
    /// dropped would report nothing, so they are kept until they end.
    pub(super) fetches: Vec<crate::runtime::BackgroundTask<()>>,
    pub(super) shared: Arc<Shared>,
    /// The running supervisor, or nothing when the helper isn't needed.
    pub(super) supervisor: Option<Supervisor>,
    /// Bumped for each send held for a hook chain: the composer's spinner and
    /// the answer that replaces it pair on this number.
    pub(super) next_send: u64,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// What the helper process is doing, for the page that says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelperState {
    /// No process: nothing has asked for one, or what did has gone quiet.
    Stopped,
    /// Its resident memory in bytes, where this system can tell it.
    Running { memory: Option<u64> },
    /// The binary isn't beside Yapfast's, so nothing can start.
    Missing,
    /// It crashed three times in a minute, so plugins stay off until Yapfast
    /// starts again.
    Crashed,
}
