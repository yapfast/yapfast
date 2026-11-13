//! What a plugin may do: the capabilities its manifest declared, and how often.
//!
//! A manifest is the whole answer to "may it". A capability the plugin never
//! declared is refused here, before any work happens, whether the guest asked
//! for it directly, or arrived through a hook, an event or a timer: every call
//! the parent answers goes through [`check`] first, so there is one place where
//! a missing capability turns into a refusal instead of a call.
//!
//! Nothing here names a host or holds a token. Reaching a host is a grant,
//! which a person makes and can take back; this module only decides what kind
//! of act a plugin is asking for and how often it may repeat it.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::capability::{Capability, Permissions};
use yapfast_plugin_api::event::{EventEnvelope, EventKind, SubscriptionFilter};

/// Messages one plugin may send in [`SEND_WINDOW`].
///
/// Five is above anything a person types by hand and far below a loop: a plugin
/// that answers each message it sees with a message of its own stays under it,
/// and one that got its termination condition wrong trips it in seconds instead
/// of flooding a channel.
pub const SENDS_PER_WINDOW: usize = 5;

/// The span [`SENDS_PER_WINDOW`] is counted over.
pub const SEND_WINDOW: Duration = Duration::from_secs(10);

/// How long after a person acts a send still counts as their action.
///
/// A command answers, a menu row runs, a button is pressed: for a few seconds
/// afterwards the send that follows is the person's, and `messages.send` alone
/// covers it. Past that it is the plugin acting on its own, which is what
/// `messages.send.background` is reviewed for.
pub const GESTURE_WINDOW: Duration = Duration::from_secs(5);

/// A refusal, in words a person can read.
///
/// The parent hands these to the helper as [`HostReturn::Denied`]'s reason and
/// shows them in the interface, so the sentence says what was refused and why,
/// never a status code or a field name.
///
/// [`HostReturn::Denied`]: yapfast_plugin_api::call::HostReturn::Denied
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Denial {
    reason: String,
}

impl Denial {
    /// A refusal with its reason.
    pub fn new(reason: impl Into<String>) -> Self {
        Denial {
            reason: reason.into(),
        }
    }

    /// The sentence to show, or to log.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for Denial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for Denial {}

/// Whether a plugin declared `asked`.
///
/// The refusal names the capability so the plugin's author can find the line to
/// add, and the person reading the log sees which of the reviewed lines was
/// missing.
pub fn check(granted: &Permissions, asked: Capability) -> Result<(), Denial> {
    if granted.allows(asked) {
        return Ok(());
    }
    Err(Denial::new(format!(
        "this plugin's manifest doesn't declare `{asked}`, so it can't do that"
    )))
}

/// How a send is allowed to happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendPath {
    /// A person just asked the plugin for it.
    Gesture,
    /// Nothing asked for it: a timer, an event or a scheduled action did.
    Background,
}

/// Whether a plugin may send along `path`.
///
/// `messages.send` covers a send the person asked for; acting while nobody is
/// there is the exceptional case, so it is declared and reviewed separately.
pub fn check_send(granted: &Permissions, path: SendPath) -> Result<(), Denial> {
    check(granted, Capability::MessagesSend)?;
    match path {
        SendPath::Gesture => Ok(()),
        SendPath::Background => {
            if granted.allows(Capability::MessagesSendBackground) {
                Ok(())
            } else {
                Err(Denial::new(
                    "this plugin tried to send without you asking, which needs \
                     `messages.send.background`",
                ))
            }
        }
    }
}

/// The capability an event kind needs, if it needs one.
///
/// Reading what people say is `messages.read`; who is in a call is
/// `voice.read`; who is around is `people.read`; which rooms exist is
/// `servers.read`. The two events that only say the app is ready or focused
/// carry nothing about a person, so they need no capability beyond being
/// subscribed.
pub fn event_capability(kind: EventKind) -> Option<Capability> {
    match kind {
        EventKind::MessageCreate
        | EventKind::MessageUpdate
        | EventKind::MessageDelete
        | EventKind::ReactionAdd
        | EventKind::ReactionRemove
        | EventKind::Typing => Some(Capability::MessagesRead),
        EventKind::RoomSelected => Some(Capability::ServersRead),
        EventKind::VoiceState | EventKind::Speaking => Some(Capability::VoiceRead),
        EventKind::Presence => Some(Capability::PeopleRead),
        EventKind::AppFocus | EventKind::AccountReady => None,
    }
}

/// Whether `event` may reach a plugin, by its manifest and its own filter.
///
/// The filter is applied here rather than in the helper: an event nobody wants
/// is never serialized and its contents never leave the parent. A subscription
/// is not a capability of its own, so a plugin that subscribed to messages
/// without `messages.read` still receives none of them.
pub fn allows_event(
    granted: &Permissions,
    filter: &SubscriptionFilter,
    event: &EventEnvelope,
) -> Result<(), Denial> {
    if let Some(capability) = event_capability(event.kind) {
        check(granted, capability)
            .map_err(|_| Denial::new(format!("`{}` needs `{capability}`", event.kind)))?;
    }
    if !filter.allows(event) {
        return Err(Denial::new(format!(
            "the plugin's filter leaves out `{}` in {}",
            event.kind,
            event.room.as_deref().unwrap_or("this room")
        )));
    }
    Ok(())
}

/// The consent dialog's text for a manifest.
///
/// The lines are the API's own [`Capability::summary`] sentences, in the order
/// the dialog lists them, and the callout is
/// [`Permissions::risky_combination`] — computed once, in the contract crate,
/// so the dialog and the plugin's page never disagree about it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Review {
    /// One line per declared capability, in review order.
    pub lines: Vec<Line>,
    /// The combination worth calling out, if there is one.
    pub callout: Option<String>,
}

/// One line of the consent dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    /// The capability the line is about.
    pub capability: Capability,
    /// What the dialog says beside it.
    pub summary: &'static str,
    /// Whether the dialog shows it in the danger tone.
    pub dangerous: bool,
}

/// The review text for a manifest's permissions.
pub fn review(granted: &Permissions) -> Review {
    let lines = Capability::ALL
        .into_iter()
        .filter(|capability| granted.allows(*capability))
        .map(|capability| Line {
            capability,
            summary: capability.summary(),
            dangerous: capability.is_dangerous(),
        })
        .collect();
    Review {
        lines,
        callout: granted.risky_combination(),
    }
}

/// One plugin's sends, as the times they happened.
#[derive(Default)]
struct Window {
    recent: VecDeque<Instant>,
}

/// Counts a plugin's sends over a sliding window.
///
/// A sliding window rather than a bucket per second: a plugin that sends five
/// and waits 9.9 s can't then send five more at the turn of the window, which
/// is what a bucket would allow.
#[derive(Default)]
pub struct SendBudget {
    windows: Mutex<HashMap<PluginId, Window>>,
}

impl SendBudget {
    /// Counts one send, or refuses it and says how long to wait.
    pub fn allow(&self, plugin: &PluginId, now: Instant) -> Result<(), Denial> {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        let window = windows.entry(plugin.clone()).or_default();
        window
            .recent
            .retain(|sent| now.saturating_duration_since(*sent) < SEND_WINDOW);
        if window.recent.len() >= SENDS_PER_WINDOW {
            let oldest = window.recent.front().copied().unwrap_or(now);
            let wait = SEND_WINDOW.saturating_sub(now.saturating_duration_since(oldest));
            return Err(Denial::new(format!(
                "this plugin has already sent {SENDS_PER_WINDOW} messages in {} seconds; \
                 it can send again in {} seconds",
                SEND_WINDOW.as_secs(),
                wait.as_secs() + 1
            )));
        }
        window.recent.push_back(now);
        Ok(())
    }

    /// Forgets a plugin's sends, for when it is turned off.
    pub fn forget(&self, plugin: &PluginId) {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        windows.remove(plugin);
    }
}

/// When a person last asked each plugin to do something.
///
/// The application records this where it knows a gesture happened: a command
/// typed in the composer, a menu row chosen, a button pressed. A send then asks
/// [`Gestures::path`] which kind of send it is, instead of guessing from
/// whatever triggered it.
#[derive(Default)]
pub struct Gestures {
    last: Mutex<HashMap<PluginId, Instant>>,
}

impl Gestures {
    /// Records that a person asked `plugin` for something at `at`.
    pub fn record(&self, plugin: &PluginId, at: Instant) {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        last.insert(plugin.clone(), at);
    }

    /// Whether a send at `now` still follows a gesture.
    pub fn path(&self, plugin: &PluginId, now: Instant) -> SendPath {
        let last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        match last.get(plugin) {
            Some(at) if now.saturating_duration_since(*at) <= GESTURE_WINDOW => SendPath::Gesture,
            _ => SendPath::Background,
        }
    }

    /// Forgets a plugin's last gesture, for when it is turned off.
    pub fn forget(&self, plugin: &PluginId) {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        last.remove(plugin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_plugin_api::capability::HostPattern;
    use yapfast_plugin_api::event::{EventEnvelope, Network};

    fn id(text: &str) -> PluginId {
        PluginId::parse(text).expect("a test id is valid")
    }

    fn declares(capabilities: &[Capability]) -> Permissions {
        Permissions {
            capabilities: capabilities.to_vec(),
            ..Permissions::default()
        }
    }

    fn event(kind: EventKind, room: Option<&str>) -> EventEnvelope {
        EventEnvelope {
            kind,
            network: Network::Discord,
            at: 0,
            room: room.map(str::to_owned),
            author: None,
            own: false,
            payload: serde_json::Value::Null,
        }
    }

    #[test]
    fn a_missing_capability_is_refused_by_name() {
        let granted = declares(&[Capability::Storage]);
        let denial = check(&granted, Capability::MessagesRead).expect_err("not declared");
        assert!(
            denial.reason().contains("messages.read"),
            "{}",
            denial.reason()
        );
        assert!(check(&granted, Capability::Storage).is_ok());
    }

    #[test]
    fn sending_in_the_background_needs_its_own_capability() {
        let granted = declares(&[Capability::MessagesSend]);
        assert!(check_send(&granted, SendPath::Gesture).is_ok());
        let denial = check_send(&granted, SendPath::Background).expect_err("not declared");
        assert!(
            denial.reason().contains("messages.send.background"),
            "{}",
            denial.reason()
        );

        let background = declares(&[Capability::MessagesSend, Capability::MessagesSendBackground]);
        assert!(check_send(&background, SendPath::Background).is_ok());
    }

    #[test]
    fn a_gesture_goes_stale() {
        let gestures = Gestures::default();
        let plugin = id("com.example.shout");
        let start = Instant::now();
        gestures.record(&plugin, start);
        assert_eq!(gestures.path(&plugin, start), SendPath::Gesture);
        assert_eq!(
            gestures.path(&plugin, start + GESTURE_WINDOW),
            SendPath::Gesture
        );
        assert_eq!(
            gestures.path(&plugin, start + GESTURE_WINDOW + Duration::from_millis(1)),
            SendPath::Background
        );
        assert_eq!(
            gestures.path(&id("com.example.other"), start),
            SendPath::Background
        );
    }

    #[test]
    fn the_sixth_send_in_a_window_is_refused() {
        let budget = SendBudget::default();
        let plugin = id("com.example.shout");
        let start = Instant::now();
        for step in 0..SENDS_PER_WINDOW {
            assert!(
                budget
                    .allow(&plugin, start + Duration::from_millis(step as u64 * 100))
                    .is_ok(),
                "send {step} is inside the allowance"
            );
        }
        let denial = budget
            .allow(&plugin, start + Duration::from_secs(1))
            .expect_err("the sixth is over it");
        assert!(denial.reason().contains("5"), "{}", denial.reason());

        // The window slides: once the first send is a window old, there is room
        // again, and the plugin's neighbour was never affected.
        assert!(
            budget
                .allow(&plugin, start + SEND_WINDOW + Duration::from_millis(1))
                .is_ok()
        );
        assert!(budget.allow(&id("com.example.other"), start).is_ok());
    }

    #[test]
    fn a_message_event_needs_messages_read() {
        let quiet = declares(&[Capability::Storage]);
        let filter = SubscriptionFilter::default();
        let denial = allows_event(
            &quiet,
            &filter,
            &event(EventKind::MessageCreate, Some("room-1")),
        )
        .expect_err("no messages.read");
        assert!(denial.reason().contains("messages.read"));

        let reader = declares(&[Capability::MessagesRead]);
        assert!(
            allows_event(
                &reader,
                &filter,
                &event(EventKind::MessageCreate, Some("room-1"))
            )
            .is_ok()
        );
        // A capability gets past the first check but not past the plugin's own
        // filter: the two are separate gates.
        let elsewhere = SubscriptionFilter {
            room: Some("room-2".into()),
            exclude_own: false,
        };
        assert!(
            allows_event(
                &reader,
                &elsewhere,
                &event(EventKind::MessageCreate, Some("room-1"))
            )
            .is_err()
        );
        // Events that say nothing about a person need no capability of their own.
        assert!(allows_event(&quiet, &filter, &event(EventKind::AppFocus, None)).is_ok());
        assert!(
            allows_event(&quiet, &filter, &event(EventKind::Presence, None)).is_err(),
            "presence is people.read"
        );
    }

    #[test]
    fn the_review_is_the_contract_s_own_text() {
        let permissions = Permissions {
            capabilities: vec![Capability::MessagesRead, Capability::Network],
            network: vec![HostPattern::parse("api.example.com").expect("a valid host")],
            ..Permissions::default()
        };
        let text = review(&permissions);
        assert_eq!(text.lines.len(), 2);
        assert_eq!(text.lines[0].summary, "Reads your messages");
        assert_eq!(
            text.callout,
            permissions.risky_combination(),
            "the callout is the API's sentence, not a second copy of it"
        );
        assert!(
            text.callout
                .as_deref()
                .is_some_and(|line| line.contains("api.example.com"))
        );

        let dangerous = review(&declares(&[Capability::MessagesSendBackground]));
        assert!(dangerous.lines[0].dangerous);
        assert!(
            dangerous.callout.is_none(),
            "one capability is not a combination"
        );
    }
}
