use super::*;

impl Session {
    /// Whether a swap is running.
    pub(super) fn swapping(&self) -> bool {
        self.swap.is_some()
    }

    /// Whether a swap has this plugin in it, which is what a call waits for.
    pub(super) fn in_swap(&self, plugin: &PluginId) -> bool {
        self.swap
            .as_ref()
            .is_some_and(|swap| swap.participants.contains(plugin))
    }

    /// Whether one of the application's commands is work for a plugin a swap is
    /// replacing or restarting.
    pub(super) fn holds_for_swap(&self, command: &Request) -> bool {
        match command {
            Request::Activate { plugins } => plugins.iter().any(|plugin| self.in_swap(plugin)),
            Request::Deactivate { plugin } => self.in_swap(plugin),
            Request::Event { plugin, .. } => self.in_swap(plugin),
            // A setting the person changed reaches the running instance; one
            // that is between versions gets it on the other side of the swap,
            // where the instance that starts reads the new value anyway.
            Request::SettingsNews { plugin, .. } => self.in_swap(plugin),
            Request::Hook { held, .. } => held.chain.iter().any(|plugin| self.in_swap(plugin)),
            Request::Command { plugin, .. } => self.in_swap(plugin),
            // A row that comes back after a swap is decorated again by the
            // interface's next report of what is on screen.
            Request::Decorate { plugin, .. } => self.in_swap(plugin),
            Request::DecorateMember { plugin, .. } => self.in_swap(plugin),
            // A person's answer is asked for again when the surfaces that show
            // them ask the cache, which forgets every answer at a swap.
            Request::Identity { plugin, .. } => self.in_swap(plugin),
            // A block on screen is asked about again by the next report of what
            // is on screen, which forgets every block at a swap.
            Request::CodeBlock { plugin, .. } => self.in_swap(plugin),
            // An answer to a guest's call is about a guest that may be shut
            // down with the swap, so it goes as it comes.
            Request::CallAnswer { .. } => false,
            // A click is about something the instance being replaced drew: the
            // new one draws that surface again, and its own row may not be
            // there. Held for the swap it would arrive at a version that never
            // showed it.
            Request::Input { .. } => false,
            // An eviction is the application asking for memory back, and one
            // that raced a swap would be undoing it; it waits the 100 ms. A
            // second swap waits for the one running for the same reason: the
            // steps of one replace the other's.
            Request::Evict { .. } | Request::Watch { .. } => true,
            Request::Swap { .. } => self.swapping(),
            Request::Shutdown => false,
        }
    }

    /// Starts replacing a plugin, with the plugins that need it stopped first.
    ///
    /// The steps run one at a time, each waiting for the helper to say it is
    /// done, so there is no moment in which a dependent is running while what it
    /// needs is between two versions.
    pub(super) fn begin_swap(
        &mut self,
        shared: &Shared,
        plugin: PluginId,
        stop: Vec<PluginId>,
        start: Vec<PluginId>,
    ) {
        let mut steps = VecDeque::new();
        let mut participants = BTreeSet::from([plugin.clone()]);
        for dependent in stop {
            participants.insert(dependent.clone());
            steps.push_back(Step::Stop(dependent));
        }
        steps.push_back(Step::Replace);
        steps.push_back(Step::StartSelf);
        for dependent in start {
            participants.insert(dependent.clone());
            steps.push_back(Step::Start(dependent));
        }
        self.swap = Some(Swap {
            plugin,
            participants,
            steps,
            waiting: None,
        });
        self.advance_swap(shared);
    }

    /// Moves a swap on to its next step, or finishes it.
    pub(super) fn advance_swap(&mut self, shared: &Shared) {
        loop {
            // The borrow of the swap ends before the step runs, because the step
            // is a request of this session's own.
            let stepped = match self.swap.as_mut() {
                None => return,
                Some(swap) => (swap.plugin.clone(), swap.steps.pop_front()),
            };
            let (plugin, Some(step)) = stepped else {
                self.finish_swap(shared);
                return;
            };
            let id = match step {
                Step::Stop(dependent) => {
                    if self.active.contains(&dependent) {
                        self.deactivate_asking(&dependent)
                    } else {
                        None
                    }
                }
                Step::Replace => {
                    // The old instance says what it hands on, then its module is
                    // forgotten so the next activation carries the new one.
                    self.handoff.remove(&plugin);
                    let asked = if self.active.contains(&plugin) {
                        self.deactivate_asking(&plugin)
                    } else {
                        None
                    };
                    self.active.remove(&plugin);
                    self.loaded.remove(&plugin);
                    asked
                }
                // The replacement itself, started once the helper has said the
                // old instance is gone.
                Step::StartSelf => match self.activate(shared, &plugin) {
                    Started::Asked(id) => Some(id),
                    Started::Active | Started::Refused => None,
                },
                Step::Start(dependent) => match self.activate(shared, &dependent) {
                    Started::Asked(id) => Some(id),
                    // It came back by itself, on the event that woke it: the
                    // swap has nothing to wait for.
                    Started::Active | Started::Refused => None,
                },
            };
            if let Some(swap) = self.swap.as_mut() {
                swap.waiting = id;
            }
            if id.is_some() {
                return;
            }
        }
    }

    /// Says a swap step is done, if this frame was the one it waited for.
    pub(super) fn swap_answered(&mut self, shared: &Shared, frame: u64) {
        let Some(swap) = self.swap.as_mut() else {
            return;
        };
        if swap.waiting != Some(frame) {
            return;
        }
        swap.waiting = None;
        self.advance_swap(shared);
    }

    /// Ends a swap: what was held for it runs now, and the calls it was given are
    /// answered by the plugins they were meant for.
    pub(super) fn finish_swap(&mut self, shared: &Shared) {
        self.swap = None;
        let held = std::mem::take(&mut self.held);
        self.queued.extend(held);
        let calls = std::mem::take(&mut self.held_calls);
        for (caller, call, request) in calls {
            self.call_plugin(shared, call, caller, request);
        }
    }

    /// Asks the helper to`yf_deactivate` a plugin, and answers with the frame that
    /// will say it is gone.
    pub(super) fn deactivate_asking(&mut self, plugin: &PluginId) -> Option<u64> {
        self.forget_timers(plugin);
        let message = ToHelper::Deactivate {
            plugin: plugin.clone(),
        };
        Some(self.request(
            &message,
            DEACTIVATE_TIMEOUT,
            Purpose::Swap {
                plugin: plugin.clone(),
            },
        ))
    }
}
