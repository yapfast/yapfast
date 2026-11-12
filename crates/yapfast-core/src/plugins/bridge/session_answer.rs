use super::*;

impl Session {
    pub(super) fn answer(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        call: HostCall,
    ) -> HostReturn {
        let registry = shared.registry();
        let Some(entry) = registry.entry(plugin) else {
            return HostReturn::Error {
                message: format!("{plugin} isn't installed"),
            };
        };
        let permissions = &entry.manifest.permissions;
        let storage_cap = entry.manifest.limits.storage_bytes;
        let method = call.method.as_str();
        let capability = match method {
            call::method::STORAGE_GET
            | call::method::STORAGE_SET
            | call::method::STORAGE_DELETE
            | call::method::STORAGE_LIST => Some(Capability::Storage),
            // A read is a capability, and it is checked here rather than by
            // the method that answers it: a plugin that declares none of these
            // gets `denied` with the capability's name in the sentence.
            call::method::ROOMS_SELECTED | call::method::ROOMS_GET | call::method::ROOMS_LIST => {
                Some(Capability::ServersRead)
            }
            call::method::MESSAGES_GET | call::method::MESSAGES_HISTORY => {
                Some(Capability::MessagesRead)
            }
            call::method::PEOPLE_GET | call::method::PEOPLE_MEMBER => Some(Capability::PeopleRead),
            call::method::MEDIA_GET => Some(Capability::MediaRead),
            // Drawing into the interface is a capability of its own: a plugin
            // that declares nothing must not be able to put a banner under its
            // own name in front of a person, or a dialog that looks like
            // Yapfast asking for something.
            call::method::UI_SET_SLOT
            | call::method::UI_CLEAR_SLOT
            | call::method::UI_NOTICE
            | call::method::UI_TOAST
            | call::method::UI_OPEN_DIALOG => Some(Capability::UiModify),
            _ => None,
        };
        if let Some(capability) = capability
            && let Err(denial) = crate::plugins::permissions::check(permissions, capability)
        {
            return HostReturn::Denied {
                reason: denial.reason().to_owned(),
            };
        }
        match method {
            call::method::STORAGE_GET => {
                let request: StorageGet = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                let value = shared.store().storage_get(plugin, &request.key);
                let value = value.unwrap_or(serde_json::Value::Null);
                // A value can be as large as the plugin's whole quota, and one
                // answer is one frame: the guest is told rather than handed
                // nothing, which would read as an absent key.
                let bytes = value.to_string().len();
                if bytes > yapfast_plugin_api::frame::MAX_FRAME - 4096 {
                    return HostReturn::Error {
                        message: format!(
                            "`{}` is {} bytes, and one answer carries at most {}: \
                             keep a value under that, or split it across keys",
                            request.key,
                            bytes,
                            yapfast_plugin_api::frame::MAX_FRAME - 4096
                        ),
                    };
                }
                HostReturn::Ok(value)
            }
            call::method::STORAGE_SET => {
                let request: StorageSet = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                match shared
                    .store()
                    .storage_set(plugin, &request.key, request.value, storage_cap)
                {
                    Ok(()) => HostReturn::Ok(serde_json::Value::Bool(true)),
                    Err(reason) => HostReturn::Denied { reason },
                }
            }
            call::method::STORAGE_DELETE => {
                let request: StorageGet = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                match shared.store().storage_delete(plugin, &request.key) {
                    Ok(()) => HostReturn::Ok(serde_json::Value::Bool(true)),
                    Err(reason) => HostReturn::Denied { reason },
                }
            }
            call::method::STORAGE_LIST => {
                let request: StorageList = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                let keys = shared.store().storage_list(plugin, &request.prefix);
                match serde_json::to_value(keys) {
                    Ok(value) => HostReturn::Ok(value),
                    Err(error) => HostReturn::Error {
                        message: format!("could not answer `{method}`: {error}"),
                    },
                }
            }
            call::method::SETTINGS_GET => {
                let request: StorageGet = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                let settings = shared.store().settings(plugin, &entry.manifest);
                match settings.get(&request.key) {
                    // A setting the manifest doesn't declare has no answer to
                    // give: an empty one would look like a value the person set.
                    Some(value) => HostReturn::Ok(value.clone()),
                    None => HostReturn::Error {
                        message: format!("{plugin} declares no setting named `{}`", request.key),
                    },
                }
            }
            call::method::TIMERS_AFTER | call::method::TIMERS_EVERY => {
                let request: TimerRequest = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                self.set_timer(plugin, request, method == call::method::TIMERS_EVERY)
            }
            call::method::TIMERS_CANCEL => {
                let request: TimerCancel = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                let key = TimerKey {
                    plugin: plugin.clone(),
                    tag: request.tag,
                };
                HostReturn::Ok(serde_json::Value::Bool(self.timers.remove(&key).is_some()))
            }
            call::method::PLUGIN_PUBLISH => self.publish(shared, plugin, call),
            call::method::PLUGIN_SUBSCRIBE => self.subscribe(shared, plugin, call),
            call::method::APP_VERSION | call::method::APP_LOCALE | call::method::APP_THEME_MODE => {
                answered(shared.world.app())
            }
            call::method::ACCOUNT_ME => match shared.world.me() {
                Some(me) => answered(me),
                // Nobody is signed in, which is a thing a plugin can be told
                // rather than a failure: a build without a link has no account.
                None => HostReturn::Error {
                    message: "no account is signed in".to_owned(),
                },
            },
            call::method::ROOMS_SELECTED => answered_option(shared.world.selected()),
            call::method::ROOMS_GET => match parsed::<RoomGet>(&call, method) {
                Ok(request) => answered_option(shared.world.room(&request.room)),
                Err(answer) => answer,
            },
            call::method::ROOMS_LIST => match parsed::<RoomsList>(&call, method) {
                Ok(request) => answered(shared.world.rooms(request.limit)),
                Err(answer) => answer,
            },
            call::method::MESSAGES_GET => match parsed::<MessageGet>(&call, method) {
                Ok(request) => match shared.world.message(&request.room, &request.message) {
                    Some(message) => answered(message),
                    None => unknown_room(&request.room, "that message"),
                },
                Err(answer) => answer,
            },
            call::method::MESSAGES_HISTORY => match parsed::<MessageHistory>(&call, method) {
                Ok(request) => {
                    match shared.world.history(
                        &request.room,
                        request.before.as_deref(),
                        request.limit,
                    ) {
                        Some(page) => answered(page),
                        None => unknown_room(&request.room, "its history"),
                    }
                }
                Err(answer) => answer,
            },
            call::method::PEOPLE_GET => match parsed::<PeopleGet>(&call, method) {
                Ok(request) => answered_option(shared.world.person(&request.person)),
                Err(answer) => answer,
            },
            call::method::PEOPLE_MEMBER => match parsed::<PeopleMember>(&call, method) {
                Ok(request) => match shared.world.member(&request.server, &request.person) {
                    crate::plugins::world::MemberAnswer::Open(found) => answered_option(found),
                    crate::plugins::world::MemberAnswer::NotOpen => HostReturn::Error {
                        message: format!(
                            "the members of {} aren't open: `people.member` answers for the \
                             conversation on screen",
                            request.server
                        ),
                    },
                },
                Err(answer) => answer,
            },
            call::method::UI_SET_SLOT => {
                let request: SetSlot = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                self.paint_slot(plugin, request)
            }
            call::method::UI_CLEAR_SLOT => {
                let request: ClearSlot = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                self.clear_paint(plugin, &request.slot)
            }
            call::method::UI_NOTICE | call::method::UI_TOAST => {
                let request: Line = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                let slot = if method == call::method::UI_NOTICE {
                    Slot::Banner
                } else {
                    Slot::Toast
                };
                self.paint_line(plugin, slot, &request.text)
            }
            call::method::UI_OPEN_DIALOG => {
                let request: OpenDialog = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                self.paint_slot(
                    plugin,
                    SetSlot {
                        slot: Slot::Dialog.id().to_owned(),
                        tree: request.tree,
                    },
                )
            }
            call::method::MEDIA_GET => {
                let request: MediaGet = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                // The store is the application's, and a build without one
                // answers `unsupported` rather than pretending it has no
                // pictures: the difference matters to a plugin telling the two
                // builds apart.
                let pictures = shared
                    .pictures
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone();
                match pictures {
                    Some(pictures) => {
                        crate::plugins::media::answer(pictures.as_ref(), &request.url)
                    }
                    None => HostReturn::Unsupported,
                }
            }
            // Everything else is named in the contract so a plugin can
            // feature-detect it, and answers so rather than failing to link.
            _ => HostReturn::Unsupported,
        }
    }
}
