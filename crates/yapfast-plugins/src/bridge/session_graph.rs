use super::*;

impl Session {
    /// Runs one plugin's call into another, which is what `plugin.call` asks for.
    ///
    /// The answer can't be built here: it is an export of the callee, and only
    /// the helper holds its instance. So this checks the edge, starts the callee
    /// if it isn't up, and sends the call; the answer comes back as `Invoked` and
    /// is handed to the caller under the number its `yf_call` answered
    /// `pending` with.
    pub(super) fn call_plugin(
        &mut self,
        shared: &Shared,
        call: u64,
        from: PluginId,
        request: HostCall,
    ) {
        let call_request: PluginCall = match request.parse() {
            Ok(call_request) => call_request,
            Err(error) => {
                self.reply(&from, call, wrong_payload(call::method::PLUGIN_CALL, error));
                return;
            }
        };
        let to = call_request.plugin.clone();
        // §10.4's cap: a call carries its arguments, and a caller that sends a
        // megabyte through one has to be told rather than have the frame refused
        // somewhere further down. Base64 is four bytes for three.
        let most_encoded = yapfast_plugin_api::limits::MAX_CALL_BYTES
            .saturating_mul(4)
            .div_ceil(3);
        if call_request.args_base64.len() > most_encoded {
            self.reply(
                &from,
                call,
                HostReturn::Denied {
                    reason: format!(
                        "this call's arguments are {} bytes, and a call carries at most {}: \
                         pass a handle and read what it names instead",
                        call_request.args_base64.len() / 4 * 3,
                        yapfast_plugin_api::limits::MAX_CALL_BYTES
                    ),
                },
            );
            return;
        }
        // A swap in progress is not an absent plugin: the call waits for it, so a
        // dependent asking its dependency something during an update gets an
        // answer rather than "not installed".
        if self.in_swap(&to) {
            // A dependency being replaced holds its callers' calls until it is
            // back. Bounded, because a guest asking in a loop must not be able to
            // grow this queue while it waits.
            if self.held_calls.len() >= HELD_CALLS_MAX {
                self.reply(
                    &from,
                    call,
                    HostReturn::Denied {
                        reason: format!(
                            "`{}` is being replaced and already has {HELD_CALLS_MAX} calls \
                             waiting for it: ask again when it is back",
                            to
                        ),
                    },
                );
                return;
            }
            self.held_calls.push((from, call, request));
            return;
        }
        if yapfast_plugin_api::message::EXPORTS.contains(&call_request.function.as_str()) {
            self.reply(
                &from,
                call,
                HostReturn::Error {
                    message: format!(
                        "`{}` belongs to the parent, not to the plugin: a call runs only an \
                         export the callee wrote for it",
                        call_request.function
                    ),
                },
            );
            return;
        }
        let registry = shared.registry();
        if registry.entry(&to).is_none() {
            drop(registry);
            self.reply(
                &from,
                call,
                HostReturn::Error {
                    message: format!("{to} isn't installed"),
                },
            );
            return;
        }
        if !registry.callable(&to) {
            drop(registry);
            self.reply(
                &from,
                call,
                HostReturn::Denied {
                    reason: format!("{to} is installed but not enabled"),
                },
            );
            return;
        }
        // Calls follow declared edges and nothing else: that is what makes a
        // loop of them impossible, and so what lets one wait for an answer.
        if !registry.declares(&from, &to) {
            drop(registry);
            self.reply(
                &from,
                call,
                HostReturn::Denied {
                    reason: format!(
                        "{from} doesn't declare {to} as a dependency, so it can't call it"
                    ),
                },
            );
            return;
        }
        drop(registry);
        if let Some(chain) = self.call_loop(&from, &to) {
            self.reply(
                &from,
                call,
                HostReturn::Denied {
                    reason: format!(
                        "a call from {from} to {to} would come back around: {}",
                        chain.join(" → ")
                    ),
                },
            );
            return;
        }
        // A call wakes its callee, as any other trigger does: a plugin that
        // was evicted is instantiated again rather than answering that it is
        // not there.
        if !self.activate(shared, &to).runs() {
            self.reply(
                &from,
                call,
                HostReturn::Error {
                    message: format!("{to} couldn't be started"),
                },
            );
            return;
        }
        let message = ToHelper::Invoke {
            plugin: to.clone(),
            function: call_request.function.clone(),
            args_base64: call_request.args_base64,
        };
        let frame = self.request(
            &message,
            CALL_TIMEOUT,
            Purpose::Invocation {
                caller: from.clone(),
                call,
                function: call_request.function.clone(),
            },
        );
        self.calls.push(InFlight { frame, from, to });
    }

    /// The chain a call would close into a loop, if it would.
    ///
    /// The caller chain is walked from the plugin making the call outwards: if
    /// the callee is already waiting on it somewhere up the chain, the call is
    /// one that comes back around and is refused with the names in order.
    pub(super) fn call_loop(&self, from: &PluginId, to: &PluginId) -> Option<Vec<String>> {
        let mut chain = vec![from.to_string(), to.to_string()];
        let mut current = from.clone();
        let mut seen = BTreeSet::from([from.clone()]);
        while let Some(caller) = self
            .calls
            .iter()
            .find(|call| call.to == current)
            .map(|call| call.from.clone())
        {
            chain.insert(1, caller.to_string());
            if caller == *to {
                chain.insert(0, to.to_string());
                chain.dedup();
                return Some(chain);
            }
            if !seen.insert(caller.clone()) {
                return Some(chain);
            }
            current = caller;
        }
        None
    }

    /// Sends one plugin's publish to everyone subscribed to its topic.
    ///
    /// Nothing is waited on and nothing is answered late: the publisher is
    /// answered at once with the count, and the subscribers get the topic in
    /// their next event batch, under the coalescing window events already have.
    pub(super) fn publish(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        request: HostCall,
    ) -> HostReturn {
        let publish: yapfast_plugin_api::call::PluginPublish = match request.parse() {
            Ok(publish) => publish,
            Err(error) => return wrong_payload(call::method::PLUGIN_PUBLISH, error),
        };
        if publish.topic.is_empty() || publish.topic.len() > limits::MAX_TOPIC_NAME_BYTES {
            return HostReturn::Error {
                message: format!(
                    "a topic's name is 1 to {} bytes",
                    limits::MAX_TOPIC_NAME_BYTES
                ),
            };
        }
        let topic = TopicEvent {
            topic: publish.topic.clone(),
            from: plugin.clone(),
            at: now_ms(),
            payload: publish.payload,
        };
        let targets = shared.registry().subscribers_to(&publish.topic, plugin);
        if targets.is_empty() {
            return HostReturn::Ok(serde_json::json!({ "delivered": 0 }));
        }
        let Some(shared_topic) = SharedTopic::new(topic) else {
            return HostReturn::Error {
                message: "the published payload couldn't be serialized".to_owned(),
            };
        };
        for target in &targets {
            self.deliver_topic(shared, target.clone(), shared_topic.clone());
        }
        HostReturn::Ok(serde_json::json!({ "delivered": targets.len() }))
    }

    /// Starts or stops hearing a topic.
    pub(super) fn subscribe(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        request: HostCall,
    ) -> HostReturn {
        let subscribe: PluginSubscribe = match request.parse() {
            Ok(subscribe) => subscribe,
            Err(error) => return wrong_payload(call::method::PLUGIN_SUBSCRIBE, error),
        };
        match shared
            .registry_mut()
            .subscribe(plugin, &subscribe.topic, !subscribe.cancel)
        {
            Ok(subscribed) => HostReturn::Ok(serde_json::json!({
                "topic": subscribe.topic,
                "subscribed": subscribed,
            })),
            Err(reason) => HostReturn::Denied { reason },
        }
    }
}
