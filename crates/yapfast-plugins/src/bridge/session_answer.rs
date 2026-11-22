use super::*;

impl Session {
    pub(super) fn answer(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        call: HostCall,
    ) -> HostReturn {
        // The powerful tier's reads and writes take the registry themselves,
        // so they go before the guard below is held.
        match call.method.as_str() {
            call::method::FS_LIST
            | call::method::FS_STAT
            | call::method::FS_READ
            | call::method::FS_WRITE => return self.files_call(shared, plugin, &call),
            call::method::NET_RESPOND => return self.respond(shared, plugin, &call),
            call::method::NET_SEND | call::method::NET_CLOSE => {
                return self.socket_call(shared, plugin, &call);
            }
            call::method::UPLOADS_READ
            | call::method::UPLOADS_WRITE
            | call::method::UPLOADS_FINISH => return upload_call(shared, plugin, &call),
            call::method::HTTP_READ | call::method::MICROPHONE_READ => {
                let request: call::BodyRead = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(&call.method, error),
                };
                return match shared.read_body(plugin, &request) {
                    Ok((piece, bytes, eof)) => answered(&call::UploadPiece {
                        data_base64: call::base64::encode(&piece),
                        offset: request.offset,
                        bytes,
                        eof,
                    }),
                    Err(message) => HostReturn::Error { message },
                };
            }
            call::method::HTTP_RELEASE => {
                let body = call
                    .payload
                    .get("body")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_default();
                return HostReturn::Ok(serde_json::Value::Bool(shared.release_body(plugin, body)));
            }
            call::method::OAUTH_LOGOUT => {
                return super::session_oauth::oauth_logout(shared, plugin, &call);
            }
            _ => {}
        }
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
            | call::method::STORAGE_LIST
            | call::method::STORAGE_CLEAR => Some(Capability::Storage),
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
            // Who is in the call and who is talking is what people do, read
            // the way presence is: a capability the review names.
            call::method::VOICE_STATE => Some(Capability::VoiceRead),
            // A plugin's slots, notices, toasts and dialogs need no
            // capability: each is the plugin's own surface and is drawn under
            // its name, so nothing it shows can pass for Yapfast's. `ui.modify`
            // is for changing Yapfast's own interface, which says so at
            // install in the danger tone; asking it for a panel made every
            // plugin that draws one look as though it rewrote the window.
            call::method::WINDOW_SET_TITLE => Some(Capability::WindowTitle),
            _ => None,
        };
        if let Some(capability) = capability
            && let Err(denial) = crate::permissions::check(permissions, capability)
        {
            return HostReturn::Denied {
                reason: denial.reason().to_owned(),
            };
        }
        match method {
            call::method::WINDOW_SET_TITLE => {
                let request: call::WindowTitle = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                let suffix = match request.suffix.as_deref().map(str::trim) {
                    None | Some("") => None,
                    Some(words) => {
                        if words.chars().count() > call::WindowTitle::MAX_SUFFIX {
                            return HostReturn::Error {
                                message: format!(
                                    "the window's title takes at most {} characters from a plugin",
                                    call::WindowTitle::MAX_SUFFIX
                                ),
                            };
                        }
                        if let Err(why) = yapfast_plugin_api::text::check(words) {
                            return HostReturn::Error {
                                message: format!("those words can't go in the title: {why}"),
                            };
                        }
                        Some(words.to_owned())
                    }
                };
                shared.announce(Notice::WindowTitle {
                    plugin: plugin.clone(),
                    suffix,
                });
                HostReturn::Ok(serde_json::Value::Bool(true))
            }
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
            // PLAN.md 10.4's last storage method: everything the plugin keeps,
            // gone, its quota with it. It takes no payload.
            call::method::STORAGE_CLEAR => {
                shared.store().storage_clear(plugin);
                HostReturn::Ok(serde_json::Value::Bool(true))
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
            call::method::OVERLAY_SET | call::method::OVERLAY_CLEAR => {
                if let Err(denial) = crate::permissions::check(permissions, Capability::Overlay) {
                    return HostReturn::Denied {
                        reason: denial.reason().to_owned(),
                    };
                }
                // Its answer to a frame, or a change of its own: either way the
                // next frame may go.
                shared.overlay_waiting().remove(plugin);
                let layer = if method == call::method::OVERLAY_CLEAR {
                    None
                } else {
                    let layer: call::OverlaySet = match call.parse() {
                        Ok(layer) => layer,
                        Err(error) => return wrong_payload(method, error),
                    };
                    if let Err(message) = overlay_problems(&layer, &entry.manifest) {
                        return HostReturn::Error { message };
                    }
                    Some(layer)
                };
                shared.announce(Notice::Overlay {
                    plugin: plugin.clone(),
                    layer,
                });
                HostReturn::Ok(serde_json::Value::Bool(true))
            }
            call::method::UI_REDECORATE => {
                // What was kept of its answers goes, and the rows on screen
                // ask again: a plugin whose answer changed (a review loaded,
                // a setting of its own) has no other way to say so.
                shared.decorated().forget(plugin);
                shared.announce(Notice::Redecorate {
                    plugin: plugin.clone(),
                });
                HostReturn::Ok(serde_json::Value::Bool(true))
            }
            call::method::IDENTITY_REFRESH => {
                let person = call
                    .payload
                    .get("person")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                shared.announce(Notice::IdentityRefresh {
                    plugin: plugin.clone(),
                    person,
                });
                HostReturn::Ok(serde_json::Value::Bool(true))
            }
            call::method::APP_NOW => HostReturn::Ok(serde_json::json!(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |since| since.as_millis() as u64)
            )),
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
                    crate::bridge::world::MemberAnswer::Open(found) => answered_option(found),
                    crate::bridge::world::MemberAnswer::NotOpen => HostReturn::Error {
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
                // A part's own slot is the plugin's only when its manifest
                // replaces that part, which the review listed.
                if let Some(part) = Slot::from_id(&request.slot).and_then(Slot::replaces)
                    && !entry
                        .manifest
                        .ui
                        .as_ref()
                        .is_some_and(|ui| ui.parts.replace.iter().any(|named| named == part.id()))
                {
                    return HostReturn::Denied {
                        reason: format!(
                            "`{}` draws in place of Yapfast's own, which needs `ui.modify` and \
                             the part in the manifest's `ui.parts.replace`",
                            request.slot
                        ),
                    };
                }
                self.paint_slot(plugin, &entry.manifest.parts, request)
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
                self.paint_line(plugin, &entry.manifest.parts, slot, &request.text)
            }
            call::method::UI_OPEN_DIALOG => {
                let request: OpenDialog = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                self.paint_slot(
                    plugin,
                    &entry.manifest.parts,
                    SetSlot {
                        slot: Slot::Dialog.id().to_owned(),
                        tree: request.tree,
                        part: None,
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
                    Some(pictures) => crate::media::answer(pictures.as_ref(), &request.url),
                    None => HostReturn::Unsupported,
                }
            }
            call::method::VOICE_STATE => answered_option(shared.world.voice()),
            // A plugin's own assets need no capability: they are what its
            // package carried and the review listed, and nobody else's.
            call::method::ASSETS_LIST => match shared.asset_infos(plugin) {
                Ok(infos) => answered(infos),
                Err(message) => HostReturn::Error { message },
            },
            call::method::ASSETS_GET => {
                let request: yapfast_plugin_api::call::AssetGet = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                match shared.asset(plugin, &request.name) {
                    Ok((kind, bytes)) => answered(yapfast_plugin_api::call::AssetBytes {
                        name: request.name,
                        kind,
                        bytes: bytes.len() as u64,
                        bytes_base64: call::base64::encode(&bytes),
                    }),
                    Err(message) => HostReturn::Error { message },
                }
            }
            // Everything else is named in the contract so a plugin can
            // feature-detect it, and answers so rather than failing to link.
            _ => HostReturn::Unsupported,
        }
    }
}

/// `uploads.read`, `uploads.write` and `uploads.finish`: a file the
/// `attachment.pre_upload` chain holds, read in pieces, replaced, and let go.
/// Each needs `uploads.intercept`, which is what put the plugin in the chain,
/// and each is refused for a file that isn't this plugin's to touch.
pub(super) fn upload_call(shared: &Shared, plugin: &PluginId, call: &HostCall) -> HostReturn {
    let permissions = shared
        .registry()
        .entry(plugin)
        .map(|entry| entry.manifest.permissions.clone())
        .unwrap_or_default();
    if let Err(denial) = crate::permissions::check(&permissions, Capability::UploadsIntercept) {
        return HostReturn::Denied {
            reason: denial.reason().to_owned(),
        };
    }
    let method = call.method.as_str();
    let failed = |message: String| HostReturn::Error { message };
    match method {
        call::method::UPLOADS_READ => {
            let request: call::UploadRead = match call.parse() {
                Ok(request) => request,
                Err(error) => return wrong_payload(method, error),
            };
            match shared.uploads().read(plugin, &request) {
                Ok((piece, bytes, eof)) => answered(&call::UploadPiece {
                    data_base64: call::base64::encode(&piece),
                    offset: request.offset,
                    bytes,
                    eof,
                }),
                Err(message) => failed(message),
            }
        }
        call::method::UPLOADS_WRITE => {
            let request: call::UploadWrite = match call.parse() {
                Ok(request) => request,
                Err(error) => return wrong_payload(method, error),
            };
            let Some(bytes) = call::base64::decode(&request.data_base64) else {
                return failed("`data_base64` isn't base64".to_owned());
            };
            match shared
                .uploads()
                .write(plugin, request.upload, request.offset, &bytes)
            {
                Ok(written) => HostReturn::Ok(serde_json::json!({ "written": written })),
                Err(message) => failed(message),
            }
        }
        _ => {
            let request: call::UploadFinish = match call.parse() {
                Ok(request) => request,
                Err(error) => return wrong_payload(method, error),
            };
            let name = match request.name.as_deref().map(saved_name) {
                Some(Ok(name)) => Some(name),
                Some(Err(message)) => {
                    return failed(message.replace("`files.save` suggests", "an upload has"));
                }
                None => None,
            };
            let finished =
                shared
                    .uploads()
                    .finish(plugin, request.upload, request.send, name, request.insert);
            match finished {
                Ok(finished) => {
                    shared.announce(Notice::UploadFinished {
                        id: request.upload,
                        file: finished.file,
                        name: finished.name,
                        insert: finished.insert,
                    });
                    HostReturn::Ok(serde_json::Value::Bool(true))
                }
                Err(message) => failed(message),
            }
        }
    }
}

/// Why an overlay can't be drawn as asked: too many sprites or frames, a
/// picture that isn't one of the package's, or a size that isn't one.
fn overlay_problems(
    layer: &call::OverlaySet,
    manifest: &yapfast_plugin_api::PluginManifest,
) -> Result<(), String> {
    if layer.sprites.len() > call::OverlaySet::MAX_SPRITES {
        return Err(format!(
            "an overlay draws at most {} sprites",
            call::OverlaySet::MAX_SPRITES
        ));
    }
    if layer.fps > call::OverlaySet::MAX_FPS {
        return Err(format!(
            "an overlay is asked for at most {} frames a second, the screen's pace",
            call::OverlaySet::MAX_FPS
        ));
    }
    for sprite in &layer.sprites {
        if !manifest.assets.contains_key(&sprite.image) {
            return Err(format!(
                "`{}` isn't one of the manifest's `assets`",
                sprite.image
            ));
        }
        let finite = [sprite.x, sprite.y]
            .into_iter()
            .chain(sprite.width)
            .chain(sprite.height)
            .chain(sprite.opacity)
            .all(f32::is_finite);
        if !finite
            || sprite.width.is_some_and(|w| w < 0.0)
            || sprite.height.is_some_and(|h| h < 0.0)
        {
            return Err(format!(
                "sprite `{}` has a place or size that isn't a number",
                sprite.id
            ));
        }
    }
    Ok(())
}
