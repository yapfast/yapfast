//! Hooks: the points a plugin may intercept on someone else's way through.
//!
//! A hook is on the path the person is waiting on, so its deadline is short and
//! it fails open: an answer of `Unchanged`, an error, or running late all leave
//! the message as it was. Only a plugin whose manifest says it may block, and
//! which holds the capability that allows it, can stop a send.

pub use crate::wire::{AttachmentPending, Draft, HookKind, HookOutcome, HookRequest, HookSubject};

impl HookRequest {
    /// The draft this hook is about, for the hooks about outgoing text.
    pub fn draft(&self) -> Option<&Draft> {
        match &self.subject {
            HookSubject::Draft(draft) => Some(draft),
            _ => None,
        }
    }

    /// The file this hook is about, for the upload hook.
    pub fn attachment(&self) -> Option<&AttachmentPending> {
        match &self.subject {
            HookSubject::Attachment(attachment) => Some(attachment),
            _ => None,
        }
    }
}

impl HookOutcome {
    /// Nothing changed: the chain moves on, and the message is sent as it was.
    pub fn unchanged() -> Self {
        HookOutcome::Unchanged
    }

    /// The new draft, for a `message.pre_send` or `message.pre_edit` hook.
    ///
    /// Anything the plugin doesn't want to change it copies from the draft it
    /// was given: the outcome replaces the whole subject, not the fields it
    /// names.
    pub fn changed_draft(draft: Draft) -> Self {
        HookOutcome::Changed(HookSubject::Draft(draft))
    }

    /// The new subject, whatever it is.
    pub fn changed(subject: HookSubject) -> Self {
        HookOutcome::Changed(subject)
    }

    /// Stop the send, with a reason the person is shown.
    ///
    /// Ignored unless the plugin may intercept; a plugin that means to stop
    /// sends says so in its manifest, and asks for the capability that allows
    /// it.
    pub fn block(reason: impl Into<String>) -> Self {
        HookOutcome::Block {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> HookRequest {
        serde_json::from_value(json!({
            "hook": "message_pre_send",
            "subject": {"draft": {"room": "room-1", "body": "hi"}},
        }))
        .expect("a request the app writes")
    }

    #[test]
    fn a_draft_hook_reads_its_draft() {
        let request = request();
        assert_eq!(request.hook, HookKind::MessagePreSend);
        assert_eq!(request.draft().expect("a draft").body, "hi");
        assert!(request.attachment().is_none());
    }

    #[test]
    fn an_outcome_is_the_json_the_app_reads() {
        let request = request();
        let draft = request.draft().expect("a draft").clone();
        let shouted = Draft {
            body: draft.body.to_uppercase(),
            ..draft
        };
        assert_eq!(
            serde_json::to_value(HookOutcome::changed_draft(shouted)).expect("json"),
            json!({"changed": {"draft": {"room": "room-1", "body": "HI", "attachments": []}}})
        );
        assert_eq!(
            serde_json::to_value(HookOutcome::block("no")).expect("json"),
            json!({"block": {"reason": "no"}})
        );
        assert_eq!(
            serde_json::to_value(HookOutcome::unchanged()).expect("json"),
            json!("unchanged")
        );
    }
}
