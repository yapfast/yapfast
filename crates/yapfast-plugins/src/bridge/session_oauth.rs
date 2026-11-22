use super::*;

impl Session {
    /// `oauth.login` and `oauth.token`: a sign-in in the person's browser, or a
    /// fresh access token, each answered once the service has, from a task of
    /// its own so nothing else waits on the network or on the person.
    ///
    /// A sign-in opens the browser, so it stands behind a click of the
    /// person's, as opening any page does; the leaving dialog shows the page
    /// first and names the plugin.
    pub(super) fn oauth(
        &mut self,
        shared: &Arc<Shared>,
        id: u64,
        plugin: PluginId,
        call: HostCall,
    ) {
        let (permissions, services) = match shared.registry().entry(&plugin) {
            Some(entry) => (
                entry.manifest.permissions.clone(),
                entry.manifest.oauth.clone(),
            ),
            None => {
                self.reply(
                    &plugin,
                    id,
                    HostReturn::Error {
                        message: format!("{plugin} isn't installed"),
                    },
                );
                return;
            }
        };
        if let Err(denial) = crate::permissions::check(&permissions, Capability::OAuthLogin) {
            self.reply(
                &plugin,
                id,
                HostReturn::Denied {
                    reason: denial.reason().to_owned(),
                },
            );
            return;
        }
        let request: yapfast_plugin_api::account::OAuthRequest = match call.parse() {
            Ok(request) => request,
            Err(error) => {
                self.reply(&plugin, id, wrong_payload(&call.method, error));
                return;
            }
        };
        let Some(service) = services
            .into_iter()
            .find(|service| service.provider == request.provider)
        else {
            self.reply(
                &plugin,
                id,
                HostReturn::Error {
                    message: format!("the manifest's `oauth` names no `{}`", request.provider),
                },
            );
            return;
        };
        let login = call.method == call::method::OAUTH_LOGIN;
        if login && !shared.has_gesture(&plugin) {
            self.reply(
                &plugin,
                id,
                HostReturn::Denied {
                    reason: format!(
                        "{plugin} wants you to sign in to {} without you doing anything: \
                         that needs a click of yours first",
                        service.provider
                    ),
                },
            );
            return;
        }
        let data_dir = shared.store().data_dir(&plugin);
        let writer = self.writer.clone();
        let shared = Arc::clone(shared);
        tokio::spawn(async move {
            let result = if login {
                let opener = Arc::clone(&shared);
                let asker = plugin.clone();
                let provider = service.provider.clone();
                crate::oauth::login(&service, &data_dir, move |url| {
                    opener.announce(Notice::SignIn {
                        plugin: asker,
                        provider,
                        url,
                    });
                })
                .await
            } else {
                crate::oauth::token(&service, &data_dir).await
            };
            let answer = match result {
                Ok(token) => match serde_json::to_value(token) {
                    Ok(value) => HostReturn::Ok(value),
                    Err(error) => HostReturn::Error {
                        message: error.to_string(),
                    },
                },
                Err(error) => HostReturn::Error {
                    message: format!("{error:#}"),
                },
            };
            let outcome = crate::devtools::outcome(&answer);
            let named = plugin.to_string();
            match serde_json::to_vec(&ToHelper::Return {
                plugin,
                call: id,
                answer,
            }) {
                Ok(payload) => {
                    crate::devtools::answered(&named, id, payload.len(), outcome);
                    let _ = writer.send(Frame::new(FrameKind::Reply, id, payload));
                }
                Err(error) => tracing::warn!(%error, "could not answer a sign-in"),
            }
        });
    }
}

/// `oauth.logout`: forgets the plugin's sign-in to a service it names.
pub(in crate::bridge) fn oauth_logout(
    shared: &Shared,
    plugin: &PluginId,
    call: &HostCall,
) -> HostReturn {
    let declared = shared
        .registry()
        .entry(plugin)
        .map(|entry| entry.manifest.permissions.clone())
        .unwrap_or_default();
    if let Err(denial) = crate::permissions::check(&declared, Capability::OAuthLogin) {
        return HostReturn::Denied {
            reason: denial.reason().to_owned(),
        };
    }
    let request: yapfast_plugin_api::account::OAuthRequest = match call.parse() {
        Ok(request) => request,
        Err(error) => return wrong_payload(&call.method, error),
    };
    let data_dir = shared.store().data_dir(plugin);
    HostReturn::Ok(serde_json::Value::Bool(crate::oauth::logout(
        &data_dir,
        &request.provider,
    )))
}
