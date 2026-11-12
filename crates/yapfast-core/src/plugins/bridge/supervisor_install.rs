use super::*;

impl Bridge {
    /// Hands the bridge the network gate, which is what makes `http.fetch` work.
    ///
    /// The gate is the application's, because it holds the consent dialogs and
    /// the transport; the bridge only passes requests to it and answers the guest
    /// with what it decided. Every enabled plugin's declaration is given to the
    /// gate here, so a request is checked against the manifest the person
    /// reviewed rather than against what the guest claims.
    pub fn install_network(&mut self, gate: Arc<crate::plugins::network::NetworkGate>) {
        *self
            .shared
            .gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(gate);
        let plugins: Vec<PluginId> = {
            let registry = self.shared.registry();
            registry
                .statuses()
                .into_iter()
                .filter(|(plugin, _)| registry.entry(plugin).is_some_and(|entry| entry.runs()))
                .map(|(plugin, _)| plugin)
                .collect()
        };
        for plugin in plugins {
            self.shared.declare(&plugin);
        }
    }

    /// Hands the bridge the picture store `media.get` reads, which is the
    /// application's because the store is the Discord client's disk cache.
    ///
    /// The parent never fetches a picture for a plugin: bytes come from what a
    /// conversation, a member list or a profile has already shown, which is what
    /// keeps `media.get` from being a request the network gate never saw.
    pub fn install_pictures(&mut self, pictures: Arc<dyn crate::plugins::media::Pictures>) {
        *self
            .shared
            .pictures
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(pictures);
    }

    /// Reads a plugin out of its repository for the page's Add from URL, and
    /// announces it as [`Notice::Fetched`].
    ///
    /// It takes seconds, so it runs on a runtime of its own: the window's thread
    /// must not wait, and the supervisor's runtime belongs to the helper.
    pub fn fetch(&mut self, url: String) {
        let shared = Arc::clone(&self.shared);
        let asked = url.clone();
        let task = match self.background() {
            Ok(runtime) => runtime.spawn(async move {
                let found =
                    tokio::task::spawn_blocking(move || crate::plugins::installing::fetch(&asked))
                        .await;
                let result = match found {
                    Ok(Ok(crate::plugins::installing::Outcome::Plugin(fetched))) => Ok(fetched),
                    // A repository that holds several plugins: each is installed
                    // by its own address, so say how rather than picking one.
                    Ok(Ok(crate::plugins::installing::Outcome::Index { paths, .. })) => {
                        Err(format!(
                            "That repository holds several plugins; install one by its \
                             address, as …#path={}.",
                            paths.first().cloned().unwrap_or_default()
                        ))
                    }
                    Ok(Err(error)) => Err(error),
                    Err(error) => Err(format!("the fetch didn't finish: {error}")),
                };
                shared.announce(Notice::Fetched { url, result });
            }),
            Err(error) => {
                shared.announce(Notice::Fetched {
                    url,
                    result: Err(format!("{error:#}")),
                });
                return;
            }
        };
        self.fetches.push(task);
        // Four is more than one person asks for in a row, and each one ends by
        // itself; the oldest's handle is only kept alive so the task isn't
        // cancelled before it reports.
        if self.fetches.len() > 4 {
            self.fetches.remove(0);
        }
    }

    /// Reads the marketplace's registry for the page, and announces it as
    /// [`Notice::Catalog`].
    ///
    /// Like [`Bridge::fetch`], it runs on a runtime of its own: the fetch takes
    /// seconds, and the window must not wait for it.
    pub fn catalog(&mut self, url: String) {
        let shared = Arc::clone(&self.shared);
        let asked = url.clone();
        let task = match self.background() {
            Ok(runtime) => runtime.spawn(async move {
                let read =
                    tokio::task::spawn_blocking(move || crate::plugins::marketplace::load(&asked))
                        .await;
                let result = match read {
                    Ok(Ok(catalog)) => Ok(Box::new(catalog)),
                    Ok(Err(error)) => Err(error),
                    Err(error) => Err(format!("the marketplace wasn't read: {error}")),
                };
                shared.announce(Notice::Catalog {
                    source: url,
                    result,
                });
            }),
            Err(error) => {
                shared.announce(Notice::Catalog {
                    source: url,
                    result: Err(format!("{error:#}")),
                });
                return;
            }
        };
        self.fetches.push(task);
        if self.fetches.len() > 4 {
            self.fetches.remove(0);
        }
    }

    /// Writes a fetched plugin into the store, and reads the store again.
    ///
    /// The helper stays up when it is up: its instances are swapped in order,
    /// with the plugins that need this one stopped first and started again after,
    /// so an update leaves no moment in which a dependent runs against nothing.
    pub fn install(&mut self, package: &crate::plugins::store::Package) -> anyhow::Result<()> {
        let plugin = package.id.clone();
        {
            let mut store = self.shared.store();
            store.install(package)?;
        }
        self.refresh(&plugin);
        Ok(())
    }

    /// Removes a plugin, and its data when `delete_data`, then reads the store
    /// again.
    ///
    /// The plugins that needed it are turned off first: a plugin left enabled
    /// over a dependency that isn't installed would be one whose every call was
    /// refused.
    pub fn uninstall(&mut self, plugin: &PluginId, delete_data: bool) -> anyhow::Result<()> {
        let stopping: Vec<PluginId> = self.shared.registry().dependents(plugin);
        {
            let mut registry = self.shared.registry_mut();
            // One guard for both: the store's lock isn't reentrant, so taking it
            // twice here would deadlock the thread that removes a plugin.
            let mut store = self.shared.store();
            for dependent in &stopping {
                if let Err(error) = registry.set_enabled(dependent, false, &store) {
                    tracing::warn!(%error, %dependent, "could not turn off a plugin that needed one being removed");
                }
            }
            store.uninstall(plugin, delete_data)?;
        }
        for dependent in &stopping {
            self.announce(dependent);
        }
        self.reload();
        Ok(())
    }

    /// Goes back to the version an install replaced.
    pub fn roll_back(&mut self, plugin: &PluginId) -> anyhow::Result<()> {
        {
            let mut store = self.shared.store();
            store.roll_back(plugin)?;
        }
        self.refresh(plugin);
        Ok(())
    }
}
