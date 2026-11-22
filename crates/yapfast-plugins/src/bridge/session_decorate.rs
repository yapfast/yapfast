use super::*;

impl Session {
    /// Asks a plugin what it draws under one message on screen.
    ///
    /// The instance is started if it isn't up, and the row stays plain until
    /// the answer comes: a decoration the person reads a moment later is worth
    /// more than a row that waits for one.
    pub(super) fn decorate(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        request: yapfast_plugin_api::message::DecorateRequest,
    ) {
        self.activate(shared, plugin);
        let message = ToHelper::Decorate {
            plugin: plugin.clone(),
            request,
        };
        self.request(
            &message,
            DECORATE_TIMEOUT,
            Purpose::Agent {
                what: "a message decoration",
                plugin: plugin.clone(),
                budget: DECORATE_TIMEOUT,
            },
        );
    }

    /// Asks one plugin how a person looks, once for the pair.
    ///
    /// The answer is kept by the application, so this is asked for a person
    /// whose cache entry is missing or stale and not for every row that person
    /// wrote.
    pub(super) fn identity(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        request: yapfast_plugin_api::identity::IdentityRequest,
    ) {
        self.activate(shared, plugin);
        let message = ToHelper::Identity {
            plugin: plugin.clone(),
            request,
        };
        self.request(
            &message,
            IDENTITY_TIMEOUT,
            Purpose::Agent {
                what: "how someone looks",
                plugin: plugin.clone(),
                budget: IDENTITY_TIMEOUT,
            },
        );
    }

    /// Asks one plugin what it badges one person in the member list with.
    ///
    /// The row shows without a badge until the answer comes: a list scrolls
    /// whether or not a plugin has answered, and a badge is worth a frame.
    pub(super) fn decorate_member(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        request: yapfast_plugin_api::message::DecorateMemberRequest,
    ) {
        self.activate(shared, plugin);
        let message = ToHelper::DecorateMember {
            plugin: plugin.clone(),
            request,
        };
        self.request(
            &message,
            DECORATE_TIMEOUT,
            Purpose::Agent {
                what: "a member badge",
                plugin: plugin.clone(),
                budget: DECORATE_TIMEOUT,
            },
        );
    }

    /// Asks one plugin for the colours of one code block.
    ///
    /// Only the blocks of the messages on screen are asked about, and the answer
    /// is kept by the block's content hash, which is why a block that comes back
    /// on screen never asks again.
    pub(super) fn code_block(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        request: yapfast_plugin_api::code::CodeBlockRequest,
    ) {
        self.activate(shared, plugin);
        let message = ToHelper::CodeBlock {
            plugin: plugin.clone(),
            request,
        };
        self.request(
            &message,
            CODE_BLOCK_TIMEOUT,
            Purpose::Agent {
                what: "a code block's colours",
                plugin: plugin.clone(),
                budget: CODE_BLOCK_TIMEOUT,
            },
        );
    }

    /// Hands the interface what a plugin drew under a message, or takes away
    /// what it drew before.
    ///
    /// A tree, an annotation or a list of marks that doesn't check out is
    /// refused where it arrives, and the rest of the answer still stands: the
    /// row stays as it was where the plugin went wrong, and the plugin's page
    /// can say nothing, because a refusal here is a plugin bug rather than a
    /// person's decision.
    pub(super) fn on_decorated(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        message: &str,
        tree: Option<serde_json::Value>,
        mut hide: crate::slots::Hidden,
        (annotation, marks, author): (
            Option<serde_json::Value>,
            Option<serde_json::Value>,
            Option<serde_json::Value>,
        ),
    ) {
        let tree = match tree {
            Some(value) => match serde_json::to_string(&value)
                .map_err(|error| error.to_string())
                .and_then(|text| crate::ui::Tree::parse(&text))
            {
                Ok(mut tree) => {
                    // A decoration is the part its manifest names for the
                    // `message` slot, if it names one.
                    tree.part = shared.registry().entry(plugin).and_then(|entry| {
                        crate::ui::part_id(
                            plugin.as_str(),
                            &entry.manifest.parts,
                            Slot::Message,
                            None,
                        )
                        .unwrap_or_default()
                    });
                    Some(tree)
                }
                Err(refusal) => {
                    tracing::warn!(%plugin, %refusal, "a plugin drew a message decoration Yapfast refused");
                    // Kept as no answer, so the same refusal isn't asked for
                    // again every time the row comes into view.
                    shared.decorated().answered(plugin, message, None);
                    return;
                }
            },
            None => None,
        };
        let (filters, reads) = shared
            .registry()
            .entry(plugin)
            .map_or((false, false), |entry| {
                let permissions = &entry.manifest.permissions;
                (
                    permissions.allows(Capability::MessagesFilter),
                    permissions.allows(Capability::MessagesRead),
                )
            });
        // Hiding is a capability of its own, so a plugin that declared only
        // `decorate` can draw under a row and cannot make it go away.
        if !hide.none() && !filters {
            tracing::warn!(
                %plugin,
                "a plugin asked to hide a message or its media without `messages.filter`"
            );
            hide = crate::slots::Hidden::default();
        }
        // A line under the text and styles over its words are about what the
        // message says, which is `messages.read`'s: a plugin without it was
        // asked with the body left out, and has no words to style.
        let mut inline = crate::slots::Inline::default();
        if annotation.is_some() || marks.is_some() {
            if reads {
                inline.annotation = annotation.and_then(|value| {
                    yapfast_plugin_api::message::Annotation::check(&value)
                        .inspect_err(|refusal| {
                            tracing::warn!(%plugin, %refusal, "a plugin's annotation was refused");
                        })
                        .ok()
                });
                inline.marks = marks
                    .and_then(|value| {
                        yapfast_plugin_api::message::Mark::check_all(&value)
                            .inspect_err(|refusal| {
                                tracing::warn!(%plugin, %refusal, "a plugin's marks were refused");
                            })
                            .ok()
                    })
                    .unwrap_or_default();
            } else {
                tracing::warn!(
                    %plugin,
                    "a plugin annotated or styled a message without `messages.read`"
                );
            }
        }
        // Badges on the author's line say something about the author, not the
        // text, so they need no capability: each says whose it is on hover.
        inline.author = author
            .and_then(|value| {
                yapfast_plugin_api::message::AuthorBadge::check_all(&value)
                    .inspect_err(|refusal| {
                        tracing::warn!(%plugin, %refusal, "a plugin's author badges were refused");
                    })
                    .ok()
            })
            .unwrap_or_default();
        let output = (tree.is_some() || !hide.none() || !inline.is_empty()).then(|| {
            crate::slots::Output::Decorate {
                plugin: plugin.to_string(),
                message: message.to_owned(),
                tree,
                hide,
                inline,
            }
        });
        // Kept under the version and the text it was asked about, so the row
        // coming back into view is answered without the guest.
        shared.decorated().answered(plugin, message, output.clone());
        if let Some(output) = output {
            shared.announce(Notice::Slot(output));
        }
    }

    /// Files a plugin's colours for one block, or its silence about it.
    pub(super) fn on_code_block(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        block: &str,
        spans: Option<Vec<yapfast_plugin_api::code::Span>>,
    ) {
        let Some(rank) = shared
            .registry()
            .code_colourers()
            .iter()
            .position(|asking| asking == plugin)
        else {
            return;
        };
        // The helper checks that the spans spell the block, because it is the
        // side holding the request's text; here they are only filed, by a hash,
        // so the cache holds no copy of the code.
        shared.announce(Notice::CodeBlock {
            plugin: plugin.clone(),
            block: block.to_owned(),
            rank,
            spans,
        });
    }

    /// Files a plugin's answer about one person: the application keeps it, and
    /// only the plugin's place in the chain decides whether it wins.
    ///
    /// An answer from a plugin that is no longer enabled is dropped, which is
    /// what a frame that raced a suspend or a disable looks like.
    pub(super) fn on_identity(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        person: &str,
        server: Option<String>,
        identity: Option<yapfast_plugin_api::identity::Identity>,
    ) {
        let Some(rank) = shared
            .registry()
            .identities()
            .iter()
            .position(|asking| asking == plugin)
        else {
            return;
        };
        shared.announce(Notice::Identity {
            plugin: plugin.clone(),
            person: person.to_owned(),
            server,
            rank,
            identity,
        });
    }

    /// Reports the badge a plugin puts beside one person's name in the member
    /// list, or that it answered with none.
    pub(super) fn on_member_decorated(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        member: &str,
        badge: Option<serde_json::Value>,
    ) {
        let badge = match badge {
            Some(value) => match crate::slots::Badge::parse(&value) {
                Ok(badge) => Some(badge),
                Err(refusal) => {
                    tracing::warn!(%plugin, %refusal, "a plugin answered with a member badge Yapfast refused");
                    return;
                }
            },
            // An answer of none is kept as such: the person was asked, and
            // scrolling back to them asks nothing again.
            None => None,
        };
        shared.announce(Notice::Slot(crate::slots::Output::MemberBadge {
            plugin: plugin.to_string(),
            member: member.to_owned(),
            badge,
        }));
    }
}
