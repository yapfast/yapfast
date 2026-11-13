use super::*;

impl Session {
    /// Registers a timer, which fires a call into the plugin when it comes due.
    ///
    /// The answer is the registration rather than a `Pending` one, because the
    /// parent has no frame that carries a late answer back to a guest: the timer
    /// fires as a `Kick`, which the guest cannot tell apart from any other
    /// arrival. A plugin that needs to know which timer went off has to keep its
    /// own count.
    pub(super) fn set_timer(
        &mut self,
        plugin: &PluginId,
        request: TimerRequest,
        repeating: bool,
    ) -> HostReturn {
        let held = self
            .timers
            .keys()
            .filter(|key| key.plugin == *plugin)
            .count();
        if held >= limits::MAX_TIMERS {
            return HostReturn::Denied {
                reason: format!("{plugin} already holds {} timers", limits::MAX_TIMERS),
            };
        }
        let interval =
            Duration::from_millis(request.interval_ms.max(limits::MIN_TIMER_INTERVAL_MS));
        let key = TimerKey {
            plugin: plugin.clone(),
            tag: request.tag.clone(),
        };
        self.timers.insert(
            key,
            Timer {
                interval,
                repeating,
                due: Instant::now() + interval,
            },
        );
        HostReturn::Ok(serde_json::json!({
            "tag": request.tag,
            "interval_ms": interval.as_millis() as u64,
            "repeating": repeating,
        }))
    }

    /// Drops every timer a plugin holds.
    ///
    /// A repeating timer is the plugin asking to be called again on its own
    /// initiative, so it lives exactly as long as the plugin is on: leaving one
    /// behind would wake a plugin the person turned off.
    pub(super) fn forget_timers(&mut self, plugin: &PluginId) {
        self.timers.retain(|key, _| key.plugin != *plugin);
    }

    /// Fires one timer, as a call into its plugin with nothing new.
    pub(super) fn fire(&mut self, shared: &Shared, key: TimerKey) {
        let Some(timer) = self.timers.get_mut(&key) else {
            return;
        };
        let interval = timer.interval;
        if timer.repeating {
            timer.due = Instant::now() + interval;
        } else {
            self.timers.remove(&key);
        }
        if !self.activate(shared, &key.plugin).runs() {
            return;
        }
        let message = ToHelper::Kick {
            plugin: key.plugin.clone(),
        };
        // Nothing answers a tick: the clock that asked isn't waiting for one, so
        // it goes with no id and leaves nothing to expire. A deadline here
        // faulted a plugin for a request it was never expected to answer.
        self.send_message(&message, FrameKind::Request);
    }
}
