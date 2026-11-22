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
    ///
    /// The plugins in the chain may read the file at `path` while it runs, by
    /// the `upload` number the subject carries, and one that answers `hold`
    /// keeps it to replace or send elsewhere; `limit` is the largest file the
    /// room takes, which a replacement is held to.
    pub fn attachment(
        &mut self,
        mut pending: AttachmentPending,
        path: &std::path::Path,
        limit: u64,
    ) -> Option<u64> {
        let room = pending.room.clone();
        // The number `hold_hook` gives it, so the subject can carry it.
        let id = self.next_send + 1;
        pending.upload = id;
        pending.hold = false;
        let chain = self.shared.registry().chain(HookKind::AttachmentPreUpload);
        self.shared.uploads().hold(
            id,
            crate::uploads::Held::new(&room, &pending.name, path, pending.bytes, limit, chain),
        );
        let held = self.hold_hook(
            HookKind::AttachmentPreUpload,
            HookSubject::Attachment(pending),
            &room,
        );
        if held.is_none() {
            self.shared.uploads().release(id);
        }
        held
    }

    /// Asks the plugins that offer to open links about one the person clicked,
    /// each on its own so every offer is its plugin's: only the plugins whose
    /// manifest lists the link's host are asked, so a link to anywhere else
    /// never leaves Yapfast. The ids the offers will come back under, as
    /// [`Notice::LinkOffer`]; none when nobody is asked.
    pub fn link_open(&mut self, url: &str) -> Vec<u64> {
        if !self.shared.enabled.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let Some(host) = link_host(url) else {
            return Vec::new();
        };
        let asked: Vec<PluginId> = {
            let registry = self.shared.registry();
            registry
                .chain(HookKind::LinkOpen)
                .into_iter()
                .filter(|plugin| {
                    registry.entry(plugin).is_some_and(|entry| {
                        entry
                            .manifest
                            .permissions
                            .link_hosts
                            .iter()
                            .any(|pattern| pattern.matches(&host))
                    })
                })
                .collect()
        };
        let mut ids = Vec::new();
        for plugin in asked {
            self.next_send += 1;
            let id = self.next_send;
            let budget = HookKind::LinkOpen.budget();
            let held = HeldSend {
                id,
                draft: Draft {
                    room: String::new(),
                    body: url.to_owned(),
                    reply_to: None,
                    attachments: Vec::new(),
                },
                hook: HookKind::LinkOpen,
                chain: vec![plugin],
                deadline: Instant::now()
                    + Duration::from_millis(budget.chain_ms.unwrap_or(budget.per_plugin_ms)),
                switcher: None,
            };
            let request = HookRequest {
                hook: HookKind::LinkOpen,
                subject: HookSubject::Link(LinkPending {
                    url: url.to_owned(),
                    offer: None,
                }),
            };
            if self.send(Request::Hook { held, request }) {
                ids.push(id);
            }
        }
        ids
    }

    /// Asks the first plugin that ranks the quick switcher how it would order
    /// what Yapfast found for a search. One plugin, so the list has one order
    /// and its footer names whose; the answer comes back as
    /// [`Notice::SwitcherRanked`], and the list shows Yapfast's order until it
    /// does.
    pub fn switcher_rank(&mut self, mut pending: SwitcherPending) -> bool {
        if !self.shared.enabled.load(Ordering::Relaxed) {
            return false;
        }
        let Some(plugin) = self
            .shared
            .registry()
            .chain(HookKind::SwitcherRank)
            .into_iter()
            .next()
        else {
            return false;
        };
        pending.places.truncate(SwitcherPending::MAX_PLACES);
        if pending.query.chars().count() > SwitcherPending::MAX_QUERY {
            pending.query = pending
                .query
                .chars()
                .take(SwitcherPending::MAX_QUERY)
                .collect();
        }
        self.next_send += 1;
        let budget = HookKind::SwitcherRank.budget();
        let held = HeldSend {
            id: self.next_send,
            draft: Draft {
                room: String::new(),
                body: String::new(),
                reply_to: None,
                attachments: Vec::new(),
            },
            hook: HookKind::SwitcherRank,
            chain: vec![plugin],
            deadline: Instant::now()
                + Duration::from_millis(budget.chain_ms.unwrap_or(budget.per_plugin_ms)),
            switcher: Some(pending.clone()),
        };
        let request = HookRequest {
            hook: HookKind::SwitcherRank,
            subject: HookSubject::Switcher(pending),
        };
        self.send(Request::Hook { held, request })
    }

    /// Asks for a picture from the web that `plugin` draws, which comes back
    /// as [`Notice::WebPicture`].
    pub fn web_picture(&mut self, plugin: &PluginId, url: &str) -> bool {
        self.send(Request::WebPicture {
            plugin: plugin.clone(),
            url: url.to_owned(),
        })
    }

    /// Whether an enabled plugin ranks the quick switcher, which a search asks
    /// before it builds anything to show one.
    pub fn ranks_switcher(&self) -> bool {
        self.shared.enabled.load(Ordering::Relaxed)
            && !self
                .shared
                .registry()
                .chain(HookKind::SwitcherRank)
                .is_empty()
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
            switcher: None,
        };
        let request = HookRequest {
            hook,
            subject: HookSubject::Draft(draft.clone()),
        };
        if !self.send(Request::Hook { held, request }) {
            // Nothing can run the chain, so the send goes as it was: the same
            // answer a plugin that traps gets, and the person isn't left with a
            // message that will never leave. Unless a plugin in it says a
            // message doesn't go without it, which holds this one back.
            if let Some(name) = super::types_session::blocker(&self.shared, hook, &chain) {
                self.shared.announce(Notice::SendBlocked {
                    id,
                    reason: super::types_session::blocked_reason(&name, "plugins aren't running"),
                });
                return PreSend::Held { id };
            }
            return PreSend::Send(draft);
        }
        PreSend::Held { id }
    }
}

/// The host of a web link, in lowercase, without a port or a user: what a
/// manifest's `link-hosts` are matched against.
pub(crate) fn link_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}
