use super::*;

impl Session {
    /// Reports a chain's answer to the application, or fails open.
    pub(super) fn on_hook_done(
        &mut self,
        shared: &Arc<Shared>,
        id: u64,
        outcome: HookOutcome,
        failures: Vec<(PluginId, Fault)>,
        by: Option<PluginId>,
    ) {
        for (plugin, fault) in failures {
            tracing::warn!(%plugin, reason = %fault.reason, kind = ?fault.kind, "a plugin failed in a hook chain");
        }
        let Some(pending) = self.pending.remove(&id) else {
            // The chain answered after its deadline, and the send has already
            // gone: nothing to do but say so.
            tracing::debug!("a hook chain answered after its budget");
            return;
        };
        let Purpose::Hook(held) = pending.purpose else {
            return;
        };
        // A plugin may stop what is going only where the person granted it.
        // For a message that is `on_failure: "block"`, which needs
        // `messages.intercept` and shows in the review dialog; anyone else's is
        // the fail-open answer, whatever it meant by it. The other chains are
        // the grant itself: a plugin is in the typing chain because
        // `typing.intercept` lets it withhold the indicator, and in the upload
        // chain because `uploads.intercept` lets it drop the file.
        let may_block = match held.hook {
            HookKind::MessagePreSend | HookKind::MessagePreEdit => {
                by.as_ref().is_some_and(|plugin| {
                    shared
                        .registry()
                        .entry(plugin)
                        .is_some_and(|entry| entry.manifest.on_failure.blocks())
                })
            }
            HookKind::TypingPreSend
            | HookKind::AttachmentPreUpload
            | HookKind::ActivityPrePublish => true,
        };
        match outcome {
            HookOutcome::Block { reason } if may_block => match held.hook {
                HookKind::MessagePreSend | HookKind::MessagePreEdit => {
                    shared.announce(Notice::SendBlocked {
                        id: held.id,
                        reason,
                    });
                }
                // The indicator is simply not sent, and the reason goes to the
                // log rather than the room: the person didn't ask for it.
                HookKind::TypingPreSend => tracing::debug!(
                    plugin = ?by,
                    %reason,
                    "a plugin withheld a typing indicator"
                ),
                HookKind::AttachmentPreUpload => shared.announce(Notice::Attachment {
                    id: held.id,
                    name: None,
                    blocked: Some(reason),
                }),
                HookKind::ActivityPrePublish => {}
            },
            HookOutcome::Block { .. } => {
                tracing::warn!(
                    plugin = %by.as_ref().map_or_else(String::new, PluginId::to_string),
                    "a plugin stopped a message without `on_failure` saying it may; the draft goes as it was"
                );
                held.let_go(shared);
            }
            HookOutcome::Changed(HookSubject::Draft(draft))
                if matches!(
                    held.hook,
                    HookKind::MessagePreSend | HookKind::MessagePreEdit
                ) =>
            {
                shared.announce(Notice::SendReady { id: held.id, draft });
            }
            HookOutcome::Changed(HookSubject::Attachment(attachment))
                if held.hook == HookKind::AttachmentPreUpload =>
            {
                shared.announce(Notice::Attachment {
                    id: held.id,
                    name: Some(attachment.name),
                    blocked: None,
                });
            }
            HookOutcome::Changed(HookSubject::Typing(_)) => {
                // There is nothing in an indicator to change; a chain that
                // answered with one let it go.
                held.let_go(shared);
            }
            HookOutcome::Changed(_) => {
                // A chain answers about the subject it was given; another one
                // means a plugin answered the wrong question, and the subject
                // goes as it was.
                tracing::warn!(hook = %held.hook, "a hook chain answered about something else");
                held.let_go(shared);
            }
            HookOutcome::Unchanged => held.let_go(shared),
        }
    }
}
