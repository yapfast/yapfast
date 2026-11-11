//! What a plugin is: its handlers, the context they run in, and the exports
//! the helper calls.
//!
//! An author implements [`Plugin`] for a type of their own, overrides the
//! handlers they need, and writes [`yapfast_plugin!`](crate::yapfast_plugin)
//! once. Handlers they don't override do nothing, which is why a plugin that
//! only adds a command never writes a hook.

use crate::call::Pending;
use crate::command::{Command, Reply};
use crate::event::Event;
use crate::hook::HookRequest;
use crate::json;
use crate::wire::{self, CallResult, CommandInvocation, HookOutcome, PluginId};
use crate::Result;

/// What the helper hands `yf_activate`.
///
/// `state` is what the last [`Plugin::deactivate`] handed on, if the instance
/// before this one was evicted cleanly. It is a convenience, not a place to
/// keep anything: the limits put it far below what storage holds.
pub use crate::wire::ActivatePayload as Activation;

/// Which export the helper called.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    /// The instance is new or was evicted; this runs before anything else.
    Activate,
    /// The instance is going away; its hand-off goes to the next activation.
    Deactivate,
    /// A batch of events, or answers to calls left pending, or both.
    Event,
    /// One hook, with the person waiting on the answer.
    Hook,
    /// One command a person typed.
    Command,
}

/// The call a handler is running in.
///
/// It carries what the call came with beyond the payload: the answers to host
/// calls that were left pending, which arrive in an event batch. Nothing in it
/// outlives the call, so a plugin that needs a value later copies it into
/// storage or its own fields.
#[derive(Debug)]
pub struct Context<'a> {
    call: Call,
    results: &'a [CallResult],
}

impl<'a> Context<'a> {
    pub(crate) fn for_call(call: Call, results: &'a [CallResult]) -> Self {
        Context { call, results }
    }

    /// Which export this is.
    pub fn call(&self) -> Call {
        self.call
    }

    /// Answers to host calls this plugin left pending, in the order the helper
    /// answered them. Empty outside an event batch.
    pub fn results(&self) -> &'a [CallResult] {
        self.results
    }

    /// The answer to the call `pending` was left for, if it has arrived.
    pub fn answer(&self, pending: &Pending) -> Option<&'a wire::HostReturn> {
        let call = pending.call()?;
        self.results
            .iter()
            .find(|result| result.call == call)
            .map(|result| &result.answer)
    }
}

/// One plugin's handlers.
///
/// Every handler has a body that does nothing, so an implementation overrides
/// only what it wants to be called for. The type is created once per
/// WebAssembly instance and lives until the instance is evicted, so fields are
/// a cache: anything that must survive eviction goes in
/// [`storage`](crate::storage).
pub trait Plugin: Default + 'static {
    /// The instance is ready: read storage, ask for timers, check settings.
    ///
    /// Runs once per instance, before any event, hook or command, and after
    /// each eviction. It may not rely on an earlier instance's fields.
    fn activate(&mut self, _ctx: &Context<'_>, _activation: Activation) -> Result<()> {
        Ok(())
    }

    /// The instance is going away.
    ///
    /// The string returned is handed to the next activation's
    /// [`Activation::state`]. Eviction is not guaranteed to be preceded by
    /// this: an instance may vanish between any two calls, so this is for a
    /// cheap hand-off and never for anything that must not be lost.
    fn deactivate(&mut self, _ctx: &Context<'_>) -> Result<Option<String>> {
        Ok(None)
    }

    /// Answers to calls left pending arrived.
    ///
    /// Called once per event batch, before its events, and only when there is
    /// at least one answer. Read them with [`Context::answer`].
    fn answered(&mut self, _ctx: &Context<'_>) {}

    /// One event of a batch.
    ///
    /// Events are coalesced, so a burst of typing is several calls to this one
    /// handler in one batch. An error here is logged and the rest of the batch
    /// still runs.
    fn event(&mut self, _ctx: &Context<'_>, _event: Event) -> Result<()> {
        Ok(())
    }

    /// One hook, with the person waiting on the answer.
    ///
    /// The hook fails open: returning an error leaves the message as it was,
    /// whatever the handler meant to do.
    fn hook(&mut self, _ctx: &Context<'_>, _hook: HookRequest) -> Result<HookOutcome> {
        Ok(HookOutcome::Unchanged)
    }

    /// One command a person typed.
    ///
    /// An error is shown to them as the command's failure.
    fn command(&mut self, _ctx: &Context<'_>, _command: Command) -> Result<Reply> {
        Ok(Reply::None)
    }

    /// One thing another plugin published to a topic this one subscribed to.
    ///
    /// Called after the batch's events, once per publish, in the order they were
    /// published. An error here is logged and the rest of the batch still runs.
    fn topic(&mut self, _ctx: &Context<'_>, _topic: wire::TopicEvent) -> Result<()> {
        Ok(())
    }
}

/// The exports the helper calls, generated for one [`Plugin`] type.
///
/// Written once, after the implementation:
///
/// ```ignore
/// use yapfast_pdk::prelude::*;
///
/// #[derive(Default)]
/// struct Greeter;
///
/// impl Plugin for Greeter {
///     fn command(&mut self, _ctx: &Context<'_>, command: Command) -> Result<Reply> {
///         let name = command.required_string("name")?;
///         Ok(Reply::notice(format!("Hello, {name}!")))
///     }
/// }
///
/// yapfast_plugin!(Greeter);
/// ```
///
/// The handlers nobody overrides are generated as no-ops, so a plugin exports
/// all five and implements one.
// Wrapping the whole expansion keeps every generated name out of the author's
// namespace. The exports sit in a module of their own because the attribute
// that makes them exports names `extism_pdk` in the code it writes, so that
// crate has to be in scope where they are written.
#[macro_export]
macro_rules! yapfast_plugin {
    ($plugin:ty) => {
        ::std::thread_local! {
            // One instance's worth of plugin state. The helper calls one
            // export at a time and never re-enters, so a `RefCell` is enough,
            // and it costs nothing on a target without threads.
            static YAPFAST_PLUGIN: ::core::cell::RefCell<$plugin> =
                ::core::cell::RefCell::new(<$plugin as ::core::default::Default>::default());
        }

        mod __yapfast_exports {
            use super::YAPFAST_PLUGIN;
            use ::yapfast_pdk::extism_pdk;

            #[::yapfast_pdk::extism_pdk::plugin_fn]
            pub fn yf_activate(input: String) -> extism_pdk::FnResult<String> {
                let answer = YAPFAST_PLUGIN.with(|plugin| {
                    ::yapfast_pdk::plugin::__private::activate(&mut *plugin.borrow_mut(), &input)
                })?;
                Ok(answer)
            }

            #[::yapfast_pdk::extism_pdk::plugin_fn]
            pub fn yf_deactivate(input: String) -> extism_pdk::FnResult<String> {
                let answer = YAPFAST_PLUGIN.with(|plugin| {
                    ::yapfast_pdk::plugin::__private::deactivate(&mut *plugin.borrow_mut(), &input)
                })?;
                Ok(answer)
            }

            #[::yapfast_pdk::extism_pdk::plugin_fn]
            pub fn yf_event(input: String) -> extism_pdk::FnResult<String> {
                let answer = YAPFAST_PLUGIN.with(|plugin| {
                    ::yapfast_pdk::plugin::__private::event(&mut *plugin.borrow_mut(), &input)
                })?;
                Ok(answer)
            }

            #[::yapfast_pdk::extism_pdk::plugin_fn]
            pub fn yf_hook(input: String) -> extism_pdk::FnResult<String> {
                let answer = YAPFAST_PLUGIN.with(|plugin| {
                    ::yapfast_pdk::plugin::__private::hook(&mut *plugin.borrow_mut(), &input)
                })?;
                Ok(answer)
            }

            #[::yapfast_pdk::extism_pdk::plugin_fn]
            pub fn yf_command(input: String) -> extism_pdk::FnResult<String> {
                let answer = YAPFAST_PLUGIN.with(|plugin| {
                    ::yapfast_pdk::plugin::__private::command(&mut *plugin.borrow_mut(), &input)
                })?;
                Ok(answer)
            }
        }
    };
}

/// The work behind each export, kept out of the macro so the decoding of the
/// ABI lives in one readable place instead of inside an expansion.
#[doc(hidden)]
pub mod __private {
    use super::*;

    /// The answer to `yf_activate`, which carries nothing.
    pub fn activate<P: Plugin>(plugin: &mut P, input: &str) -> Result<String> {
        let activation: Activation = json::from_str(input)?;
        let context = Context::for_call(Call::Activate, &[]);
        plugin.activate(&context, activation)?;
        Ok("null".to_owned())
    }

    /// The answer to `yf_deactivate`: the hand-off, or `null`.
    pub fn deactivate<P: Plugin>(plugin: &mut P, _input: &str) -> Result<String> {
        let context = Context::for_call(Call::Deactivate, &[]);
        let state = plugin.deactivate(&context)?;
        Ok(json::to_string(&wire::DeactivatePayload { state })?)
    }

    /// The answer to `yf_event`, which carries nothing: what an event handler
    /// does, it does through host calls, and a failure is logged rather than
    /// thrown away with the rest of the batch.
    pub fn event<P: Plugin>(plugin: &mut P, input: &str) -> Result<String> {
        let wire::EventBatch {
            events,
            results,
            topics,
        } = json::from_str(input)?;
        let context = Context::for_call(Call::Event, &results);
        if !results.is_empty() {
            plugin.answered(&context);
        }
        // Moved rather than cloned: an event is the largest thing a plugin is
        // handed, and its envelope is already decoded.
        for envelope in events {
            let kind = envelope.kind.clone();
            if let Err(error) = plugin.event(&context, Event::from(envelope)) {
                crate::log::error(format!("handling `{kind}` failed: {error}"));
            }
        }
        for topic in topics {
            let name = topic.topic.clone();
            if let Err(error) = plugin.topic(&context, topic) {
                crate::log::error(format!("handling topic `{name}` failed: {error}"));
            }
        }
        Ok("null".to_owned())
    }

    /// The answer to `yf_hook`.
    pub fn hook<P: Plugin>(plugin: &mut P, input: &str) -> Result<String> {
        let request: HookRequest = json::from_str(input)?;
        let context = Context::for_call(Call::Hook, &[]);
        let outcome = match plugin.hook(&context, request) {
            Ok(outcome) => outcome,
            Err(error) => {
                // Fail open, as the ABI does: the message goes as it was.
                crate::log::error(format!("a hook failed: {error}"));
                HookOutcome::Unchanged
            }
        };
        Ok(json::to_string(&outcome)?)
    }

    /// The answer to `yf_command`, whose failure is shown to the person
    /// rather than thrown.
    pub fn command<P: Plugin>(plugin: &mut P, input: &str) -> Result<String> {
        let invocation: CommandInvocation = json::from_str(input)?;
        let context = Context::for_call(Call::Command, &[]);
        let outcome = match plugin.command(&context, Command::from(invocation)) {
            Ok(reply) => wire::CommandOutcome::from(reply),
            Err(error) => wire::CommandOutcome::Failed {
                message: error.to_string(),
            },
        };
        Ok(json::to_string(&outcome)?)
    }
}

impl Activation {
    /// The id of the plugin the helper instantiated.
    ///
    /// Read from the activation rather than from anywhere global: a plugin is
    /// told which plugin it is, and may not assume its own id otherwise.
    pub fn plugin_id(&self) -> &PluginId {
        &self.plugin
    }
}
