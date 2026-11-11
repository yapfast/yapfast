//! The hooks a plugin intercepts, and the budgets they must fit in.
//!
//! A hook is on the path the person is waiting on, so it has a deadline, and it
//! fails open: a plugin that traps, runs late or answers nonsense leaves the
//! message as it was. Only a plugin that says
//! `on_failure = "block"` may stop one, which needs `messages.intercept` and
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
}

impl HookKind {
    /// Every hook kind.
    pub const ALL: [HookKind; 5] = [
        HookKind::MessagePreSend,
        HookKind::MessagePreEdit,
        HookKind::TypingPreSend,
        HookKind::AttachmentPreUpload,
        HookKind::ActivityPrePublish,
    ];

    /// The name a manifest writes, without the prefix.
    pub fn as_str(self) -> &'static str {
        match self {
            HookKind::MessagePreSend => "message_pre_send",
            HookKind::MessagePreEdit => "message_pre_edit",
            HookKind::TypingPreSend => "typing_pre_send",
            HookKind::AttachmentPreUpload => "attachment_pre_upload",
            HookKind::ActivityPrePublish => "activity_pre_publish",
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
    Attachment(AttachmentPending),
    /// An activity about to be published, as its JSON.
    Activity(serde_json::Value),
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
    /// Stop the send. Only respected for a plugin whose manifest sets
    /// `on_failure` or which holds `messages.intercept`.
    Block { reason: String },
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
