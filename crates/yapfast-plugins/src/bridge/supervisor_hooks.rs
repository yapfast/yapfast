use super::*;

impl Bridge {
    /// Runs the `message.pre_send` chain, if anything intercepts sends.
    pub fn pre_send(&mut self, draft: Draft) -> PreSend {
        if !self.shared.intercepts.load(Ordering::Relaxed) {
            return PreSend::Send(draft);
        }
        self.intercept(HookKind::MessagePreSend, draft)
    }

    /// Runs the chain in front of an edit, which is a different set of plugins
    /// from the one in front of a send: a plugin that rewrites what is typed
    /// needn't intercept what leaves, and one that filters sends usually
    /// doesn't want to see the same text twice.
    pub fn pre_edit(&mut self, draft: Draft) -> PreSend {
        self.intercept(HookKind::MessagePreEdit, draft)
    }

    /// Runs the `typing.pre_send` chain in front of a typing indicator, and
    /// answers the id it is held under when one may withhold it.
    ///
    /// `None` leaves the indicator exactly as it was: nothing intercepts
    /// typing, which is the whole cost of this call in the common case. A
    /// hold's answer comes back as [`Notice::Typing`], and the indicator goes
    /// then, so the person's composer never waits on a plugin.
    pub fn typing(&mut self, room: &str) -> Option<u64> {
        self.hold_hook(
            HookKind::TypingPreSend,
            HookSubject::Typing(TypingPending {
                room: room.to_owned(),
            }),
            room,
        )
    }

    /// Runs the `attachment.pre_upload` chain in front of a file the composer
    /// is about to take, and answers the id it is held under when one may
    /// rename or withhold it.
    ///
    /// The file itself stays in the application: the chain is about its name,
    /// its size and its type, and the answer names what the upload should be
    /// called or why it doesn't go. [`Notice::Attachment`] carries it back.
    pub fn attachment(&mut self, pending: AttachmentPending) -> Option<u64> {
        let room = pending.room.clone();
        self.hold_hook(
            HookKind::AttachmentPreUpload,
            HookSubject::Attachment(pending),
            &room,
        )
    }

    /// Holds a hook that isn't a send until its chain answers. False when
    /// nothing intercepts it, which is the answer that costs one lookup.
    pub(super) fn hold_hook(
        &mut self,
        hook: HookKind,
        subject: HookSubject,
        room: &str,
    ) -> Option<u64> {
        self.next_send += 1;
        let id = self.next_send;
        let held = held_hook(&self.shared, id, hook, room)?;
        let request = HookRequest { hook, subject };
        if !self.send(Request::Hook { held, request }) {
            return None;
        }
        Some(id)
    }

    /// What the application knows, for it to publish into: the account, the
    /// lists, the open room and its members. The bridge answers a guest's
    /// reads from the same value.
    pub fn world(&self) -> &Arc<dyn World> {
        &self.shared.world
    }

    /// Holds an outgoing draft for one chain, whichever hook it is.
    pub(super) fn intercept(&mut self, hook: HookKind, draft: Draft) -> PreSend {
        let chain = self.shared.registry().chain(hook);
        if chain.is_empty() {
            return PreSend::Send(draft);
        }
        self.next_send += 1;
        let id = self.next_send;
        // The deadline is the person's, not the helper's: it starts the moment
        // they pressed send, so a helper that is starting, a module that is
        // compiling and every plugin in the chain all spend the same budget.
        // A chain that has none of it left when it reaches the helper doesn't
        // get a fresh one there.
        let budget = hook.budget();
        let deadline =
            Instant::now() + Duration::from_millis(budget.chain_ms.unwrap_or(budget.per_plugin_ms));
        let held = HeldSend {
            id,
            draft: draft.clone(),
            hook,
            chain: chain.clone(),
            deadline,
        };
        let request = HookRequest {
            hook,
            subject: HookSubject::Draft(draft.clone()),
        };
        if !self.send(Request::Hook { held, request }) {
            // Nothing can run the chain, so the send goes as it was: the same
            // answer a plugin that traps gets, and the person isn't left with a
            // message that will never leave.
            return PreSend::Send(draft);
        }
        PreSend::Held { id }
    }
}
