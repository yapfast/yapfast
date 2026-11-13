use super::*;

impl Drop for Session {
    fn drop(&mut self) {
        // A session that ends without a clean stop, such as one aborted while the
        // application exits, must not leave the process behind.
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Session {
    /// Starts a helper and the threads that talk to it.
    pub(super) fn start(data_dir: &Path) -> Result<Session> {
        let Helper {
            child,
            stdin,
            stdout,
        } = spawn::start(data_dir, crate::logging::for_worker())?;
        let (frames, inbound) = tokio::sync::mpsc::unbounded_channel();
        let reader = std::thread::Builder::new()
            .name("yapfast-plugins-read".to_owned())
            .spawn(move || {
                let mut stdout = stdout;
                loop {
                    match frame::read_frame(&mut stdout) {
                        Ok(Some(frame)) => {
                            if frames.send(Inbound::Frame(frame)).is_err() {
                                return;
                            }
                        }
                        Ok(None) => {
                            let _ = frames.send(Inbound::Closed(None));
                            return;
                        }
                        Err(error) => {
                            let _ = frames.send(Inbound::Closed(Some(error.to_string())));
                            return;
                        }
                    }
                }
            })
            .context("could not start the thread reading the plugin helper")?;
        let (writer, writes) = std::sync::mpsc::channel::<Frame>();
        std::thread::Builder::new()
            .name("yapfast-plugins-write".to_owned())
            .spawn(move || {
                let mut stdin = stdin;
                for frame in writes {
                    if frame::write_frame(&mut stdin, &frame).is_err() {
                        return;
                    }
                }
            })
            .context("could not start the thread writing to the plugin helper")?;
        Ok(Session {
            child,
            writer,
            inbound,
            reader: Some(reader),
            next_id: 0,
            pending: BTreeMap::new(),
            loaded: BTreeSet::new(),
            active: BTreeSet::new(),
            batches: BTreeMap::new(),
            painting: Painting::default(),
            timers: BTreeMap::new(),
            watchers: BTreeMap::new(),
            queued: VecDeque::new(),
            handoff: BTreeMap::new(),
            swap: None,
            held: Vec::new(),
            calls: Vec::new(),
            held_calls: Vec::new(),
            dev: BTreeMap::new(),
            asked: false,
            bye: None,
        })
    }

    /// Adds a command to the front of the queue, for one that arrived before the
    /// process did.
    pub(super) fn queue(&mut self, command: Request) {
        self.queued.push_back(command);
    }

    /// Lets every send this session was holding go out unchanged.
    ///
    /// Called when the process ends, whatever the reason: a chain that can no
    /// longer answer must not keep the composer waiting for it.
    pub(super) fn fail_open(&mut self, shared: &Shared) {
        let pending = std::mem::take(&mut self.pending);
        for pending in pending.into_values() {
            match pending.purpose {
                Purpose::Hook(held) => held.let_go(shared),
                // The two that answer a guest are dropped with the process:
                // there is no instance left to be told.
                Purpose::Agent { .. } | Purpose::Invocation { .. } | Purpose::Swap { .. } => {}
            }
        }
    }

    /// Runs the conversation until the helper goes or the parent is done.
    pub(super) async fn run(
        &mut self,
        shared: &Arc<Shared>,
        commands: &mut tokio::sync::mpsc::UnboundedReceiver<Request>,
    ) -> Ending {
        // Nothing else is sent before the helper has said which protocol it
        // speaks: a mismatch would otherwise show up as a timeout per request.
        self.send_message(&ToHelper::Hello { api: API_VERSION }, FrameKind::Hello);
        match tokio::time::timeout(HELLO_TIMEOUT, self.await_ready()).await {
            Ok(Ok(())) => {}
            Ok(Err(reason)) => return Ending::Gone(reason),
            Err(_) => return Ending::Gone("it didn't answer the hello".to_owned()),
        }
        loop {
            while let Some(command) = self.queued.pop_front() {
                if let Some(ending) = self.on_command(shared, command).await {
                    return ending;
                }
            }
            // What woke this loop is worked out before anything is done about it:
            // the `select!` futures hold a borrow of this session, and handling a
            // frame or a command needs it mutably.
            let deadline = self.next_deadline(shared);
            let waking = tokio::select! {
                biased;
                incoming = self.inbound.recv() => Waking::Helper(incoming.map(Box::new)),
                command = commands.recv() => Waking::Application(command.map(Box::new)),
                () = sleep_until(deadline) => Waking::Clock,
            };
            match waking {
                Waking::Helper(Some(inbound)) => match *inbound {
                    Inbound::Frame(frame) => {
                        if let Some(ending) = self.on_frame(shared, frame) {
                            return ending;
                        }
                    }
                    Inbound::Closed(reason) => return self.ending(reason),
                },
                Waking::Helper(None) => return self.ending(None),
                Waking::Application(Some(command)) => {
                    if let Some(ending) = self.on_command(shared, *command).await {
                        return ending;
                    }
                }
                // The bridge is gone: stop talking and let the process go.
                Waking::Application(None) => return Ending::Asked,
                Waking::Clock => self.on_tick(shared),
            }
        }
    }

    /// Waits for the helper's `Ready`, which is the answer to the hello.
    pub(super) async fn await_ready(&mut self) -> Result<(), String> {
        if let Some(incoming) = self.inbound.recv().await {
            let frame = match incoming {
                Inbound::Frame(frame) => frame,
                Inbound::Closed(reason) => {
                    return Err(reason.unwrap_or_else(|| "it closed the pipe".to_owned()));
                }
            };
            // The contract doesn't pin which frame kind carries the greeting, so
            // any of the three is accepted as long as it is a `Ready`.
            match serde_json::from_slice::<FromHelper>(&frame.payload) {
                Ok(FromHelper::Ready { api, version }) => {
                    if api != API_VERSION {
                        return Err(format!(
                            "the helper speaks plugin API {api}, and this Yapfast speaks {API_VERSION}"
                        ));
                    }
                    tracing::debug!(%version, "the plugin helper is up");
                    return Ok(());
                }
                Ok(_) => return Err("it said something before the hello".to_owned()),
                Err(_) => return Err("its hello wasn't a message this build knows".to_owned()),
            }
        }
        Err("it closed the pipe before the hello".to_owned())
    }

    /// What ended this session, once the pipe has closed.
    ///
    /// A helper that goes with nothing outstanding has run out of work, which is
    /// how it is meant to end; one that goes with a request or an instance still
    /// live crashed, and is worth replacing.
    pub(super) fn ending(&self, reason: Option<String>) -> Ending {
        if self.asked {
            return Ending::Asked;
        }
        if self.active.is_empty() && self.pending.is_empty() {
            return Ending::Idle;
        }
        match &self.bye {
            Some(bye) => Ending::Gone(bye.clone()),
            None => Ending::Gone(reason.unwrap_or_else(|| "it closed the pipe".to_owned())),
        }
    }

    /// The next moment this session has work to do, if it has any.
    pub(super) fn next_deadline(&self, shared: &Shared) -> Option<Instant> {
        let pending = self.pending.values().map(|pending| pending.deadline).min();
        let batch = self.batches.values().map(|batch| batch.due).min();
        let timer = self.timers.values().map(|timer| timer.due).min();
        let paint = self.painting.due;
        let save = shared.store().pending_save();
        let dev = self.dev.values().map(|watch| watch.due).min();
        [pending, batch, timer, paint, save, dev]
            .into_iter()
            .flatten()
            .min()
    }
}
