//! Hooks: the points a plugin may intercept on someone else's way through.
//!
//! A hook is on the path the person is waiting on, so its deadline is short and
//! it fails open: an answer of `Unchanged`, an error, or running late all leave
//! the message as it was. Only a plugin whose manifest says it may block, and
//! which holds the capability that allows it, can stop a send.

pub use crate::wire::{
    AttachmentPending, Draft, HookKind, HookOutcome, HookRequest, HookSubject, LinkOffer,
    LinkPending, SwitcherPending, SwitcherPlace, TypingPending,
};

impl HookRequest {
    /// The draft this hook is about, for the hooks about outgoing text.
    pub fn draft(&self) -> Option<&Draft> {
        match &self.subject {
            HookSubject::Draft(draft) => Some(draft),
            _ => None,
        }
    }

    /// The typing indicator this hook is about, for `typing.pre_send`.
    pub fn typing(&self) -> Option<&TypingPending> {
        match &self.subject {
            HookSubject::Typing(typing) => Some(typing),
            _ => None,
        }
    }

    /// The link this hook is about, for `link_open`.
    pub fn link(&self) -> Option<&LinkPending> {
        match &self.subject {
            HookSubject::Link(link) => Some(link),
            _ => None,
        }
    }

    /// The switcher's search and places, for `switcher_rank`.
    pub fn switcher(&self) -> Option<&SwitcherPending> {
        match &self.subject {
            HookSubject::Switcher(switcher) => Some(switcher),
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

    /// An offer to open `link` in an app: the button's words and the address,
    /// whose scheme the manifest's `schemes` must list.
    pub fn offer(link: &LinkPending, label: impl Into<String>, uri: impl Into<String>) -> Self {
        HookOutcome::Changed(HookSubject::Link(LinkPending {
            url: link.url.clone(),
            offer: Some(LinkOffer {
                label: label.into(),
                uri: uri.into(),
            }),
        }))
    }

    /// The switcher's places in this order: the ids of places it was given,
    /// leaving out the ones to hide.
    pub fn ranked(asked: &SwitcherPending, order: &[&str]) -> Self {
        let places = order
            .iter()
            .filter_map(|id| asked.places.iter().find(|place| place.id == *id).cloned())
            .collect();
        HookOutcome::Changed(HookSubject::Switcher(SwitcherPending {
            query: asked.query.clone(),
            places,
        }))
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

    #[test]
    fn a_link_offer_and_a_ranking_read_and_answer_as_the_app_writes_them() {
        let link: HookRequest = serde_json::from_value(json!({
            "hook": "link_open",
            "subject": {"link": {"url": "https://open.spotify.com/track/1"}},
        }))
        .expect("a link hook");
        let asked = link.link().expect("a link");
        let answer = serde_json::to_value(HookOutcome::offer(asked, "Open in Spotify", "spotify:track:1"))
            .unwrap();
        assert_eq!(
            answer,
            json!({"changed": {"link": {"url": "https://open.spotify.com/track/1",
                "offer": {"label": "Open in Spotify", "uri": "spotify:track:1"}}}})
        );
        let rank: HookRequest = serde_json::from_value(json!({
            "hook": "switcher_rank",
            "subject": {"switcher": {"query": "ge", "places": [
                {"id": "a", "name": "general", "kind": "channel"},
                {"id": "b", "name": "games", "kind": "voice"}]}},
        }))
        .expect("a switcher hook");
        let HookOutcome::Changed(HookSubject::Switcher(ranked)) =
            HookOutcome::ranked(rank.switcher().unwrap(), &["b"])
        else {
            panic!("a ranking");
        };
        assert_eq!(ranked.places.len(), 1);
        assert_eq!(ranked.places[0].id, "b");
    }

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

    /// Every subject the app writes reads here: one missing is a module that
    /// traps the first time its plugin is put in front of that hook.
    #[test]
    fn every_subject_the_app_writes_reads() {
        let typing: HookRequest = serde_json::from_value(json!({
            "hook": "typing_pre_send",
            "subject": {"typing": {"room": "room-1"}},
        }))
        .expect("a typing hook the app writes");
        assert_eq!(typing.typing().expect("a typing subject").room, "room-1");
        for subject in [
            json!({"attachment": {"room": "r", "name": "a.png", "bytes": 1}}),
            json!({"activity": {"name": "a game"}}),
        ] {
            serde_json::from_value::<HookSubject>(subject.clone())
                .unwrap_or_else(|error| panic!("{subject}: {error}"));
        }
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
