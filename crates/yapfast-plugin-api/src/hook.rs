//! The hooks a plugin intercepts, and the budgets they must fit in.
//!
//! A hook is on the path the person is waiting on, so it has a deadline, and it
//! fails open: a plugin that traps, runs late or answers nonsense leaves the
//! message as it was. Only a plugin that says
//! `"on_failure": "block"` may stop one, which needs `messages.intercept` and
//! shows in the consent dialog.

use serde::{Deserialize, Serialize};

use crate::event::AttachmentView;

/// A hook a plugin can subscribe to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookKind {
    /// Before an outgoing message is sent.
    MessagePreSend,
    /// Before an edit is sent.
    MessagePreEdit,
    /// The typing indicator, before it is sent.
    TypingPreSend,
    /// A file, before it is uploaded.
    AttachmentPreUpload,
    /// An activity, before it is published.
    ActivityPrePublish,
    /// A link the person clicked, before the leaving dialog: a plugin may
    /// offer to open it in an app, as a second button beside Visit site.
    LinkOpen,
    /// What the quick switcher found for a search, before it is shown: a
    /// plugin may reorder it or leave places out.
    SwitcherRank,
}

impl HookKind {
    /// The hooks this build hands to a chain, which is every one but
    /// [`HookKind::ActivityPrePublish`].
    ///
    /// Yapfast does publish an activity — the game it detects, merged into its
    /// own presence — but that happens in `yapfast-discord`'s session, which has
    /// no bridge to ask, so nothing dispatches `activity_pre_publish` yet and
    /// the constant says so rather than leaving it to be guessed. The bridge's
    /// test holds this list to [`HookKind::ALL`]: adding a dispatch fails it
    /// until the list is updated.
    pub const DISPATCHED: [HookKind; 6] = [
        HookKind::MessagePreSend,
        HookKind::MessagePreEdit,
        HookKind::TypingPreSend,
        HookKind::AttachmentPreUpload,
        HookKind::LinkOpen,
        HookKind::SwitcherRank,
    ];

    /// Every hook kind.
    pub const ALL: [HookKind; 7] = [
        HookKind::MessagePreSend,
        HookKind::MessagePreEdit,
        HookKind::TypingPreSend,
        HookKind::AttachmentPreUpload,
        HookKind::ActivityPrePublish,
        HookKind::LinkOpen,
        HookKind::SwitcherRank,
    ];

    /// The name a manifest writes, without the prefix.
    pub fn as_str(self) -> &'static str {
        match self {
            HookKind::MessagePreSend => "message_pre_send",
            HookKind::MessagePreEdit => "message_pre_edit",
            HookKind::TypingPreSend => "typing_pre_send",
            HookKind::AttachmentPreUpload => "attachment_pre_upload",
            HookKind::ActivityPrePublish => "activity_pre_publish",
            HookKind::LinkOpen => "link_open",
            HookKind::SwitcherRank => "switcher_rank",
        }
    }

    /// The capability that lets a plugin intercept this.
    pub fn capability(self) -> crate::capability::Capability {
        use crate::capability::Capability;
        match self {
            HookKind::MessagePreSend | HookKind::MessagePreEdit => Capability::MessagesIntercept,
            HookKind::TypingPreSend => Capability::TypingIntercept,
            HookKind::AttachmentPreUpload => Capability::UploadsIntercept,
            HookKind::ActivityPrePublish => Capability::PresenceWrite,
            HookKind::LinkOpen => Capability::LinksOffer,
            HookKind::SwitcherRank => Capability::SwitcherRank,
        }
    }

    /// What this hook must answer within.
    pub fn budget(self) -> Budget {
        match self {
            HookKind::MessagePreSend | HookKind::MessagePreEdit => Budget {
                lane: Lane::Interactive,
                per_plugin_ms: 25,
                chain_ms: Some(100),
            },
            HookKind::TypingPreSend => Budget {
                lane: Lane::Interactive,
                per_plugin_ms: 10,
                chain_ms: Some(25),
            },
            HookKind::AttachmentPreUpload => Budget {
                lane: Lane::Interactive,
                per_plugin_ms: 50,
                chain_ms: Some(150),
            },
            HookKind::ActivityPrePublish => Budget {
                lane: Lane::Background,
                per_plugin_ms: 25,
                chain_ms: Some(100),
            },
            // The leaving dialog is on screen while the plugins answer, so the
            // person waits on nothing; an offer later than this isn't shown.
            HookKind::LinkOpen => Budget {
                lane: Lane::Interactive,
                per_plugin_ms: 50,
                chain_ms: Some(150),
            },
            // The list shows at once in Yapfast's own order, and a ranking
            // that comes later than this is let go rather than moving rows
            // under the person's keys.
            HookKind::SwitcherRank => Budget {
                lane: Lane::Interactive,
                per_plugin_ms: 30,
                chain_ms: Some(60),
            },
        }
    }
}

impl std::fmt::Display for HookKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which queue a call goes in: one the person waits on, or one they don't.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    Interactive,
    Background,
}

/// How long a hook may take: per plugin, and for the whole chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub lane: Lane,
    pub per_plugin_ms: u64,
    /// `None` when the hook is asynchronous and no one waits for it.
    pub chain_ms: Option<u64>,
}

/// The thing a hook is about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookSubject {
    /// Text about to be sent.
    Draft(Draft),
    /// A typing indicator about to be sent, which a plugin may withhold.
    Typing(TypingPending),
    Attachment(AttachmentPending),
    /// An activity about to be published, as its JSON.
    Activity(serde_json::Value),
    /// A link the person clicked, and the offer a plugin answers with.
    Link(LinkPending),
    /// What the quick switcher found, which a plugin answers in its order.
    Switcher(SwitcherPending),
}

/// A link the person clicked, on its way to the leaving dialog.
///
/// A plugin answers with the same link and an `offer`; the link itself can't
/// change, and the leaving dialog shows whatever is offered beside Visit site,
/// with the plugin's name, so the person still chooses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkPending {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offer: Option<LinkOffer>,
}

/// An app a plugin offers to open a link in: a button's words and the address
/// it opens, whose scheme the manifest's `schemes` must list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkOffer {
    /// The button's words, such as "Open in Spotify": 1 to
    /// [`LinkOffer::MAX_LABEL`] characters on one line.
    pub label: String,
    /// What opens, such as `spotify:track:…`.
    pub uri: String,
}

impl LinkOffer {
    /// The most characters an offer's button says.
    pub const MAX_LABEL: usize = 32;
    /// The longest address an offer opens, in bytes.
    pub const MAX_URI: usize = 2048;
}

/// The quick switcher's search and what it found, in Yapfast's order.
///
/// A plugin answers with `places` in its own order, and may leave some out;
/// a place it adds, or one named twice, makes the whole answer ignored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitcherPending {
    pub query: String,
    pub places: Vec<SwitcherPlace>,
}

/// One place the switcher found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitcherPlace {
    pub id: String,
    pub name: String,
    /// `dm`, `channel`, `voice`, `thread` or `server`.
    pub kind: String,
}

impl SwitcherPending {
    /// The most places a plugin is shown: the first rows of the list, which
    /// are the ones a person reads.
    pub const MAX_PLACES: usize = 50;
    /// The longest search a plugin is shown, in characters.
    pub const MAX_QUERY: usize = 100;

    /// The ids of `answer`'s places, in its order, when it is a reordering of
    /// some of these places and nothing else: none added, none twice.
    pub fn reordered(&self, answer: &SwitcherPending) -> Option<Vec<String>> {
        if answer.query != self.query {
            return None;
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut order = Vec::with_capacity(answer.places.len());
        for place in &answer.places {
            if !self.places.iter().any(|given| given.id == place.id) || !seen.insert(&place.id) {
                return None;
            }
            order.push(place.id.clone());
        }
        Some(order)
    }
}

/// An outgoing message, as a hook may change it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub room: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub attachments: Vec<AttachmentView>,
}

/// A file on its way up, as a hook may rename or strip it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentPending {
    pub room: String,
    pub name: String,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// The number `uploads.read`, `uploads.write`, `uploads.finish` and
    /// `http.fetch`'s `body_upload` name this file by, while it is held.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub upload: u64,
    /// In an answer: the plugin keeps the file to work on, and says what goes
    /// with `uploads.finish`. The composer goes on without it meanwhile.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hold: bool,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

/// A hook's question to one plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRequest {
    pub hook: HookKind,
    pub subject: HookSubject,
}

/// What a plugin answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookOutcome {
    /// It touched nothing, or doesn't handle this hook. The chain moves on.
    Unchanged,
    /// The new subject; the next plugin in the chain sees this one.
    Changed(HookSubject),
    /// Stop the send. Honoured only from a plugin whose manifest says
    /// `"on_failure": "block"`, which needs `messages.intercept`; from any
    /// other it is the fail-open answer and the message goes as it was.
    Block { reason: String },
}

/// A typing indicator about to be sent.
///
/// There is nothing in it to change: a plugin either lets it go or withholds
/// it, which is what `typing.intercept` is for. The room is there so a plugin
/// can tell one conversation from another.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypingPending {
    pub room: String,
}

impl HookOutcome {
    /// The subject to carry forward, if this answer changed it.
    pub fn subject(&self) -> Option<&HookSubject> {
        match self {
            HookOutcome::Changed(subject) => Some(subject),
            HookOutcome::Unchanged | HookOutcome::Block { .. } => None,
        }
    }

    /// Whether this answer stops the send.
    pub fn blocks(&self) -> bool {
        matches!(self, HookOutcome::Block { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hook_has_a_budget_inside_its_chain() {
        for hook in HookKind::ALL {
            let budget = hook.budget();
            assert!(budget.per_plugin_ms > 0);
            if let Some(chain) = budget.chain_ms {
                assert!(budget.per_plugin_ms <= chain);
            }
        }
    }

    #[test]
    fn the_two_message_hooks_share_one_budget() {
        assert_eq!(
            HookKind::MessagePreSend.budget().chain_ms,
            Some(100),
            "a whole send may wait 100 ms, and no more"
        );
        assert_eq!(
            HookKind::MessagePreEdit.budget(),
            HookKind::MessagePreSend.budget()
        );
    }

    #[test]
    fn a_ranking_keeps_to_the_places_it_was_given() {
        let place = |id: &str| SwitcherPlace {
            id: id.into(),
            name: id.into(),
            kind: "channel".into(),
        };
        let given = SwitcherPending {
            query: "ge".into(),
            places: vec![place("a"), place("b"), place("c")],
        };
        let answer = |ids: &[&str]| SwitcherPending {
            query: "ge".into(),
            places: ids.iter().map(|id| place(id)).collect(),
        };
        assert_eq!(
            given.reordered(&answer(&["c", "a"])),
            Some(vec!["c".to_owned(), "a".to_owned()]),
            "reordered, and b left out"
        );
        assert_eq!(given.reordered(&answer(&["a", "z"])), None, "z was added");
        assert_eq!(given.reordered(&answer(&["a", "a"])), None, "a twice");
        let mut other = answer(&["a"]);
        other.query = "x".into();
        assert_eq!(given.reordered(&other), None, "about another search");
    }

    #[test]
    fn a_draft_survives_a_round_trip() {
        let request = HookRequest {
            hook: HookKind::MessagePreSend,
            subject: HookSubject::Draft(Draft {
                room: "room-1".into(),
                body: "hello".into(),
                reply_to: None,
                attachments: Vec::new(),
            }),
        };
        let text = serde_json::to_string(&request).unwrap();
        assert_eq!(serde_json::from_str::<HookRequest>(&text).unwrap(), request);
    }
}
