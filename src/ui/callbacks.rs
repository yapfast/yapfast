//! What the window's callbacks do: most dispatch an action to the application.

use super::*;

pub(super) fn bind_callbacks(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    super::games::bind(window, adapter);
    super::gifs::bind(window, adapter);
    super::polls::bind(window, adapter);
    super::call_link::bind(window, adapter);
    super::server_settings::bind(window, adapter);
    super::channel_settings::bind(window, adapter);
    super::title_bar::bind(window, adapter);
    // UI bindings can request images while refresh borrows the adapter.
    // Deliver their requests in one batch after the current UI work.
    let pending = Rc::new(RefCell::new(HashSet::new()));
    let timer = Rc::new(slint::Timer::default());
    let last = Rc::new(Cell::new(None));
    let state = Rc::downgrade(adapter);
    let weak = window.as_weak();
    window
        .global::<PictureStore>()
        .on_picture(|url, _revision| avatar_picture(url.as_str()));
    window.global::<PictureStore>().on_request(move |url| {
        if url.is_empty() {
            return;
        }
        pending.borrow_mut().insert(url.to_string());
        if !timer.running() {
            let (pending, state, weak) = (pending.clone(), state.clone(), weak.clone());
            timer.start(
                slint::TimerMode::SingleShot,
                report_delay(&last),
                move || {
                    let urls = pending.borrow_mut().drain().collect();
                    if let (Some(adapter), Some(window)) = (state.upgrade(), weak.upgrade()) {
                        adapter
                            .borrow_mut()
                            .action(&window, Action::LoadPictures(urls));
                    }
                },
            );
        }
    });
    let timer = Rc::new(slint::Timer::default());
    let last = Rc::new(Cell::new(None));
    let state = Rc::downgrade(adapter);
    let weak = window.as_weak();
    window.on_media_viewport(move || {
        if timer.running() {
            return;
        }
        let (state, weak) = (state.clone(), weak.clone());
        timer.start(
            slint::TimerMode::SingleShot,
            report_delay(&last),
            move || {
                let (Some(adapter), Some(window)) = (state.upgrade(), weak.upgrade()) else {
                    return;
                };
                let mut adapter = adapter.borrow_mut();
                let messages = &adapter.app.state.messages;
                let Some(room) = adapter.app.state.selected.clone() else {
                    return;
                };
                if messages.is_empty() || window.get_top_index() < 0 {
                    return;
                }
                const MARGIN: usize = 6;
                let first = (window.get_top_index() as usize).min(messages.len() - 1);
                let last = (window.get_bottom_index().max(window.get_top_index()) as usize)
                    .min(messages.len() - 1);
                let items = messages[first.saturating_sub(MARGIN)
                    ..=last.saturating_add(MARGIN).min(messages.len() - 1)]
                    .iter()
                    .map(|message| message.item.clone())
                    .collect();
                let at_end = window.invoke_timeline_at_end();
                adapter.action(
                    &window,
                    Action::VisibleMessages {
                        room,
                        items,
                        at_end,
                    },
                );
            },
        );
    });
    // The thread beside the open channel reports its rows in view the same
    // way, the latest of those a burst of scrolling reported.
    let timer = Rc::new(slint::Timer::default());
    let last = Rc::new(Cell::new(None));
    let range = Rc::new(Cell::new((-1, -1)));
    let state = Rc::downgrade(adapter);
    let weak = window.as_weak();
    window.on_aside_viewport(move |top, bottom| {
        range.set((top, bottom));
        if timer.running() {
            return;
        }
        let (state, weak, range) = (state.clone(), weak.clone(), range.clone());
        timer.start(
            slint::TimerMode::SingleShot,
            report_delay(&last),
            move || {
                let (Some(adapter), Some(window)) = (state.upgrade(), weak.upgrade()) else {
                    return;
                };
                let mut adapter = adapter.borrow_mut();
                let Some(aside) = &adapter.app.state.aside else {
                    return;
                };
                let (top, bottom) = range.get();
                let messages = &aside.messages;
                if messages.is_empty() || top < 0 {
                    return;
                }
                const MARGIN: usize = 6;
                let first = (top as usize).min(messages.len() - 1);
                let last = (bottom.max(top) as usize).min(messages.len() - 1);
                let items = messages[first.saturating_sub(MARGIN)
                    ..=last.saturating_add(MARGIN).min(messages.len() - 1)]
                    .iter()
                    .map(|message| message.item.clone())
                    .collect();
                let room = aside.room.clone();
                adapter.action(
                    &window,
                    Action::VisibleMessages {
                        room,
                        items,
                        at_end: false,
                    },
                );
            },
        );
    });
    macro_rules! bind {
        ($callback:ident, $action:expr) => {{
            let adapter = adapter.clone();
            let weak = window.as_weak();
            window.$callback(move || {
                if let Some(window) = weak.upgrade() {
                    adapter.borrow_mut().action(&window, $action);
                }
            });
        }};
    }
    bind!(on_explore_preview, Action::Preview);
    bind!(on_restore, Action::Restore);
    bind!(on_logout, Action::Logout);
    bind!(on_refresh_trust, Action::RefreshTrust);
    bind!(on_retry_sync, Action::RetrySync);
    bind!(
        on_navigate_dms,
        Action::Navigate(Navigation::DirectMessages)
    );
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_send(move || {
            let Some(window) = weak.upgrade() else { return };
            // Sending goes to the newest message, as Discord does, so the sent one shows.
            // From a window of older messages, that's once the newest replace it:
            // following its end before then would load earlier pages as they go in.
            if window.get_can_send() {
                if window.get_detached() {
                    adapter.borrow_mut().latest_due = true;
                } else {
                    window.invoke_scroll_to_latest();
                }
            }
            adapter.borrow_mut().action(&window, Action::Send);
        });
    }
    bind!(on_earlier, Action::Earlier);
    bind!(on_later, Action::Later);
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_jump_to_present(move || {
            let Some(window) = weak.upgrade() else { return };
            // The newest messages show at the end once they replace the window.
            adapter.borrow_mut().latest_due = true;
            adapter.borrow_mut().action(&window, Action::JumpToPresent);
        });
    }
    bind!(on_jump_to_unread, Action::JumpToUnread);
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_reading_changed(move |reading| {
            if let Some(window) = weak.upgrade() {
                adapter
                    .borrow_mut()
                    .action(&window, Action::Reading(reading));
            }
        });
    }
    bind!(on_accept_request, Action::AnswerRequest(true));
    bind!(on_enlarge_profile, Action::EnlargeProfile);
    bind!(on_close_profile, Action::CloseProfile);
    bind!(on_ignore_request, Action::AnswerRequest(false));
    bind!(on_leave_voice, Action::Voice(voice::Command::Leave));
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_message_from_profile(move |user, text| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(
                &window,
                Action::MessageFromProfile {
                    user: user.into(),
                    text: text.into(),
                },
            );
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_befriend(move |user| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::Befriend(user.to_string()));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_unblock(move |user| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(
                &window,
                Action::ChangeRelationship {
                    user: user.to_string(),
                    change: yapfast_core::chat::RelationshipChange::Unblock,
                },
            );
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_remove_friend_request(move |user| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::RemoveFriendRequest(user.to_string()));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_start_call(move || {
        if let Some(window) = weak.upgrade() {
            let id = state.borrow().app.state.selected.clone();
            if let Some(id) = id {
                state
                    .borrow_mut()
                    .action(&window, Action::RequestJoinVoice(id));
            }
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_answer_call(move |room, accept| {
        if let Some(window) = weak.upgrade() {
            let room = room.to_string();
            state.borrow_mut().action(
                &window,
                if accept {
                    Action::AcceptCall(room)
                } else {
                    Action::DeclineCall(room)
                },
            );
        }
    });
    bind!(on_confirm_switch, Action::ConfirmVoiceSwitch);
    bind!(on_cancel_switch, Action::CancelVoiceSwitch);
    bind!(on_dismiss_voice_error, Action::DismissVoiceError);
    bind!(on_refresh_devices, Action::Voice(voice::Command::Devices));
    bind!(on_confirm_delete_message, Action::ConfirmDelete);
    bind!(on_cancel_delete, Action::CancelDelete);
    bind!(on_cancel_compose, Action::CancelCompose);
    bind!(
        on_toggle_reply_ping,
        Action::ToggleReplyPing { aside: false }
    );
    bind!(
        on_aside_toggle_reply_ping,
        Action::ToggleReplyPing { aside: true }
    );
    bind!(on_aside_close, Action::CloseAside);
    bind!(on_aside_expand, Action::ExpandAside);
    bind!(on_aside_earlier, Action::AsideEarlier);
    bind!(on_aside_send, Action::SendAside);
    bind!(on_aside_cancel_compose, Action::CancelAsideCompose);
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_aside_draft_edited(move |text| {
            let Some(window) = weak.upgrade() else { return };
            let mut adapter = adapter.borrow_mut();
            if adapter.app.state.aside_draft() != text.as_str() {
                adapter.action(&window, Action::SetAsideDraft(text.into()));
            }
        });
    }
    bind!(on_close_forward, Action::CloseForward);
    bind!(on_close_switcher, Action::CloseSwitcher);
    bind!(on_dismiss_notice, Action::DismissNotice);
    bind!(on_close_image, Action::CloseImage);
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_view_image(move |item, index| {
            if let Some(window) = weak.upgrade() {
                let action = Action::ViewImage {
                    item: item.into(),
                    index: usize::try_from(index).unwrap_or_default(),
                };
                adapter.borrow_mut().action(&window, action);
            }
        });
    }
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_step_image(move |step| {
            if let Some(window) = weak.upgrade() {
                adapter
                    .borrow_mut()
                    .action(&window, Action::StepImage(step as isize));
            }
        });
    }
    macro_rules! bind_item {
        ($callback:ident, $action:expr) => {{
            let adapter = adapter.clone();
            let weak = window.as_weak();
            window.$callback(move |item| {
                if let Some(window) = weak.upgrade() {
                    adapter.borrow_mut().action(&window, $action(item.into()));
                }
            });
        }};
    }
    bind_item!(on_reply, Action::Reply);
    bind_item!(on_jump_to, Action::Jump);
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_jump_in_room(move |room, item| {
            if let Some(window) = weak.upgrade() {
                let (room, item) = (room.into(), item.into());
                adapter
                    .borrow_mut()
                    .action(&window, Action::JumpInRoom { room, item });
            }
        });
    }
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_prepare_menu(move |id, person| {
            if let Some(window) = weak.upgrade() {
                let menu = menus::menu(&adapter.borrow(), &id, person);
                menus::show(&window, menu);
            }
        });
    }
    {
        let adapter = adapter.clone();
        // An empty category's heading names the category itself.
        window.on_category_of(move |room| {
            let app = &adapter.borrow().app;
            app.category_of(&room)
                .or_else(|| app.state.is_category(&room).then(|| room.to_string()))
                .unwrap_or_default()
                .into()
        });
    }
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_choose_menu(move |target, person, id| {
            if let Some(window) = weak.upgrade() {
                menus::choose(
                    &mut adapter.borrow_mut(),
                    &window,
                    target.into(),
                    person,
                    &id,
                );
            }
        });
    }
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_show_friend_profile(move |user, large| {
            if let Some(window) = weak.upgrade() {
                let user = user.into();
                adapter
                    .borrow_mut()
                    .action(&window, Action::ShowFriendProfile { user, large });
            }
        });
    }
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_enlarge_panel_profile(move || {
            let Some(window) = weak.upgrade() else { return };
            let mut adapter = adapter.borrow_mut();
            if let Some(room) = adapter.app.state.selected.clone() {
                adapter.action(&window, Action::ShowDmProfile(room));
            }
        });
    }
    bind_item!(
        on_show_profile,
        (|user| Action::ShowProfile { user, large: false })
    );
    bind_item!(on_edit, Action::Edit);
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_edit_last(move || {
            let Some(window) = weak.upgrade() else { return };
            adapter.borrow_mut().action(&window, Action::EditLast);
            // The message's text, with the cursor after it.
            let end = window.get_draft().len();
            window.invoke_place_composer_caret(i32::try_from(end).unwrap_or(i32::MAX));
        });
    }
    bind_item!(on_request_delete, Action::RequestDelete);
    bind_item!(on_pin_message, Action::PinMessage);
    bind_item!(on_retry, Action::Retry);
    bind_item!(on_cancel_send, Action::CancelSend);
    bind_item!(on_start_forward, Action::StartForward);
    bind_item!(on_forward_choose, Action::ChooseForward);
    bind_item!(on_forward_send, Action::Forward);
    bind_item!(on_switcher_search, Action::SearchSwitcher);
    bind_item!(on_switch_to, Action::SwitchTo);
    bind!(on_open_new_dm, Action::OpenNewDm);
    bind!(on_open_add_friend, Action::OpenAddFriend);
    bind!(on_open_requests, Action::OpenDmScreen(DmScreen::Requests));
    bind!(on_open_friends, Action::OpenDmScreen(DmScreen::Friends));
    bind!(on_open_browse, Action::BrowseChannels);
    bind_item!(on_message_friend, Action::MessagePerson);
    bind!(on_link_discord, Action::LinkDiscord);
    bind_item!(on_link_discord_token, Action::LinkDiscordToken);
    bind!(on_open_discord_captcha, Action::OpenDiscordCaptcha);
    bind!(on_cancel_discord_link, Action::CancelDiscordLink);
    bind!(on_unlink_discord, Action::UnlinkDiscord);
    bind!(on_close_add_friend, Action::CloseAddFriend);
    bind!(on_close_person, Action::ClosePersonDialog);
    bind!(on_close_invite, Action::CloseInvite);
    bind!(on_open_join_server, Action::OpenJoinServer);
    bind!(on_join_server, Action::JoinServer);
    bind!(on_open_join_captcha, Action::OpenJoinCaptcha);
    bind!(on_open_join_form, Action::OpenJoinForm);
    bind!(on_close_join, Action::CloseJoin);
    bind!(on_authorize_oauth, Action::AuthorizeOAuth);
    bind!(on_close_oauth, Action::CloseOAuth);
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_choose_oauth_guild(move |index| {
            let guild = {
                let adapter = adapter.borrow();
                adapter
                    .app
                    .state
                    .oauth
                    .as_ref()
                    .and_then(|dialog| dialog.preview.as_ref())
                    .and_then(|preview| {
                        preview
                            .guilds
                            .iter()
                            .filter(|guild| guild.can_add)
                            .nth(usize::try_from(index).unwrap_or_default())
                    })
                    .map(|guild| guild.id.clone())
            };
            if let (Some(window), Some(guild)) = (weak.upgrade(), guild) {
                adapter
                    .borrow_mut()
                    .action(&window, Action::ChooseOAuthGuild(guild));
            }
        });
    }
    bind_item!(on_find_invite, Action::FindInvite);
    bind_item!(on_open_invite, Action::OpenInvite);
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_make_invite(move |age, uses| {
            let pick = |list: &[u32], index: i32| {
                usize::try_from(index)
                    .ok()
                    .and_then(|index| list.get(index))
                    .copied()
            };
            if let (Some(window), Some(max_age), Some(max_uses)) = (
                weak.upgrade(),
                pick(&INVITE_AGES, age),
                pick(&INVITE_USES, uses),
            ) {
                adapter
                    .borrow_mut()
                    .action(&window, Action::MakeInvite { max_age, max_uses });
            }
        });
    }
    bind!(on_close_channel, Action::CloseChannelDialog);
    bind!(on_edit_profile, Action::EditProfile);
    bind!(on_undo_profile, Action::UndoProfileEdits);
    bind!(on_save_profile, Action::SaveProfile);
    bind!(on_close_profile_editor, Action::CloseProfileEditor);
    bind!(on_unlock_nitro, Action::UnlockNitro);
    let profile_picture = |kind: i32| {
        if kind == 1 {
            ProfilePicture::Banner
        } else {
            ProfilePicture::Avatar
        }
    };
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_write_profile(move |field, text| {
            let field = match field {
                0 => ProfileText::Name,
                1 => ProfileText::Pronouns,
                2 => ProfileText::Bio,
                _ => ProfileText::Colour,
            };
            if let Some(window) = weak.upgrade() {
                adapter.borrow_mut().action(
                    &window,
                    Action::WriteProfile {
                        field,
                        text: text.into(),
                    },
                );
            }
        });
    }
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_reset_profile_picture(move |kind| {
            if let Some(window) = weak.upgrade() {
                adapter
                    .borrow_mut()
                    .action(&window, Action::ResetProfilePicture(profile_picture(kind)));
            }
        });
    }
    {
        let state = adapter.clone();
        window.on_pick_profile_picture(move |kind| {
            let kind = profile_picture(kind);
            let weak = Rc::downgrade(&state);
            state.borrow_mut().dialog(async move {
                let file = rfd::AsyncFileDialog::new()
                    .set_title(match kind {
                        ProfilePicture::Avatar => "Change Avatar",
                        ProfilePicture::Banner => "Change Banner",
                    })
                    .add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "webp"])
                    .pick_file()
                    .await;
                if let (Some(file), Some(adapter)) = (file, weak.upgrade()) {
                    let window = adapter.borrow().window.upgrade();
                    if let Some(window) = window {
                        let path = file.path().to_owned();
                        adapter
                            .borrow_mut()
                            .action(&window, Action::PickProfilePicture { kind, path });
                    }
                }
            });
        });
    }
    bind!(on_confirm_leave, Action::Leave);
    bind!(on_toggle_silent_leave, Action::ToggleSilentLeave);
    bind!(on_close_leave, Action::CloseLeave);
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_save_channel(move || {
            let Some(window) = weak.upgrade() else { return };
            let kind = if window.get_channel_kind() == 1 {
                ChannelKind::Voice
            } else {
                ChannelKind::Text
            };
            let settings = ChannelSettings {
                name: window.get_channel_name().into(),
                ..Default::default()
            };
            adapter
                .borrow_mut()
                .action(&window, Action::SaveChannel { kind, settings });
        });
    }
    bind_item!(on_answer_person, Action::AnswerPerson);
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_choose_person_option(move |index| {
            if let (Some(window), Ok(index)) = (weak.upgrade(), usize::try_from(index)) {
                adapter
                    .borrow_mut()
                    .action(&window, Action::ChoosePersonOption(index));
            }
        });
    }
    bind_item!(on_add_friend, Action::AddFriend);
    bind!(on_open_status, Action::OpenStatusEditor);
    bind_item!(on_set_status, Action::SetStatus);
    bind!(on_close_status, Action::CloseStatusEditor);
    bind!(on_clear_status, Action::ClearCustomStatus);
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_save_status(move || {
        let Some(window) = weak.upgrade() else { return };
        let key = window.get_status_emoji().key.to_string();
        let action = Action::SetCustomStatus {
            emoji: (!key.is_empty()).then_some(key),
            text: window.get_status_text().to_string(),
            clear_after: match window.get_status_clear_after() {
                1 => ClearAfter::Minutes(30),
                2 => ClearAfter::Minutes(60),
                3 => ClearAfter::Minutes(240),
                4 => ClearAfter::Today,
                _ => ClearAfter::Never,
            },
        };
        state.borrow_mut().action(&window, action);
    });
    bind!(on_close_role, Action::CloseRole);
    bind!(on_close_new_dm, Action::CloseNewDm);
    bind!(on_more_results, Action::MoreResults);
    bind!(on_open_add_people, Action::OpenAddPeople);
    bind!(on_confirm_add_people, Action::AddPeople);
    bind!(on_close_add_people, Action::CloseAddPeople);
    bind_item!(on_choose_person, Action::ChoosePerson);
    bind_item!(on_search, Action::Search);
    bind_item!(on_search_threads, Action::SearchThreads);
    bind!(on_more_threads, Action::MoreThreads);
    bind!(on_create_thread, Action::CreateThread);
    bind!(on_close_new_thread, Action::CloseNewThread);
    bind!(on_leave_new_thread, Action::LeaveNewThread);
    bind!(on_stay_in_new_thread, Action::StayInNewThread);
    bind!(on_new_post, Action::NewPost);
    bind!(on_create_post, Action::CreatePost);
    bind!(on_close_post, Action::CloseNewPost);
    bind!(on_leave_post, Action::LeaveNewPost);
    bind!(on_stay_post, Action::StayInNewPost);
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_show_pins(move |shown| {
            if let Some(window) = weak.upgrade() {
                adapter
                    .borrow_mut()
                    .action(&window, Action::ShowPins(shown));
            }
        });
    }
    {
        let (adapter, weak) = (adapter.clone(), window.as_weak());
        window.on_show_threads(move |shown| {
            if let Some(window) = weak.upgrade() {
                adapter
                    .borrow_mut()
                    .action(&window, Action::ShowThreads(shown));
            }
        });
    }
    {
        let (adapter, weak) = (adapter.clone(), window.as_weak());
        window.on_new_thread(move |item, from| {
            use yapfast_core::application::ThreadFrom;
            let from = match from.as_str() {
                "message" => ThreadFrom::Message,
                "list" => ThreadFrom::List,
                "list-empty" => ThreadFrom::ListEmpty,
                _ => ThreadFrom::PlusButton,
            };
            let message = (!item.is_empty()).then(|| item.to_string());
            if let Some(window) = weak.upgrade() {
                adapter
                    .borrow_mut()
                    .action(&window, Action::NewThread { message, from });
            }
        });
    }
    {
        let (adapter, weak) = (adapter.clone(), window.as_weak());
        window.on_new_thread_name_edited(move |name| {
            if let Some(window) = weak.upgrade() {
                adapter.borrow_mut().action(
                    &window,
                    Action::WriteNewThread {
                        name: Some(name.into()),
                        body: None,
                    },
                );
            }
        });
    }
    {
        let (adapter, weak) = (adapter.clone(), window.as_weak());
        window.on_new_thread_body_edited(move |body| {
            if let Some(window) = weak.upgrade() {
                adapter.borrow_mut().action(
                    &window,
                    Action::WriteNewThread {
                        name: None,
                        body: Some(body.into()),
                    },
                );
            }
        });
    }
    {
        let (adapter, weak) = (adapter.clone(), window.as_weak());
        window.on_post_title_edited(move |title| {
            if let Some(window) = weak.upgrade() {
                adapter.borrow_mut().action(
                    &window,
                    Action::WriteNewPost {
                        title: Some(title.into()),
                        body: None,
                    },
                );
            }
        });
    }
    {
        let (adapter, weak) = (adapter.clone(), window.as_weak());
        window.on_post_body_edited(move |body| {
            if let Some(window) = weak.upgrade() {
                adapter.borrow_mut().action(
                    &window,
                    Action::WriteNewPost {
                        title: None,
                        body: Some(body.into()),
                    },
                );
            }
        });
    }
    {
        let (adapter, weak) = (adapter.clone(), window.as_weak());
        window.on_toggle_post_tag(move |tag| {
            if let Some(window) = weak.upgrade() {
                adapter
                    .borrow_mut()
                    .action(&window, Action::TogglePostTag(tag.into()));
            }
        });
    }
    {
        let (adapter, weak) = (adapter.clone(), window.as_weak());
        window.on_set_thread_private(move |private| {
            if let Some(window) = weak.upgrade() {
                adapter
                    .borrow_mut()
                    .action(&window, Action::SetThreadPrivate(private));
            }
        });
    }
    {
        // As Discord's: a click shows the thread in place of its channel,
        // and Shift-click beside it.
        let (adapter, weak) = (adapter.clone(), window.as_weak());
        window.on_open_listed_thread(move |room, beside| {
            if let Some(window) = weak.upgrade() {
                let action = match beside {
                    true => Action::OpenAside(room.into()),
                    false => Action::Open(room.into()),
                };
                adapter.borrow_mut().action(&window, action);
            }
        });
    }
    {
        let (adapter, weak) = (adapter.clone(), window.as_weak());
        window.on_search_posts(move |query| {
            if let Some(window) = weak.upgrade() {
                adapter
                    .borrow_mut()
                    .action(&window, Action::SearchPosts(query.into()));
            }
        });
    }
    {
        let (adapter, weak) = (adapter.clone(), window.as_weak());
        window.on_more_posts(move || {
            if let Some(window) = weak.upgrade() {
                adapter.borrow_mut().action(&window, Action::MorePosts);
            }
        });
    }
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_add_people_search(move || {
            if let Some(window) = weak.upgrade() {
                let query = window.get_add_people_query().to_string();
                adapter
                    .borrow_mut()
                    .action(&window, Action::AddPeopleQuery(query));
            }
        });
    }
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_new_dm_search(move || {
        if let Some(window) = weak.upgrade() {
            let query = window.get_new_dm_query().to_string();
            state
                .borrow_mut()
                .action(&window, Action::NewDmQuery(query));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_start_dm(move |index| {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        let contact = usize::try_from(index).ok().and_then(|index| {
            adapter
                .app
                .state
                .new_dm
                .as_ref()
                .and_then(|new_dm| new_dm.people().nth(index).cloned())
        });
        if let Some(contact) = contact {
            adapter.action(&window, Action::StartDm(contact));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_forward_search(move || {
        if let Some(window) = weak.upgrade() {
            let query = window.get_forward_query().into();
            state
                .borrow_mut()
                .action(&window, Action::SearchForward(query));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_emoji_search(move |query| {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        let custom = adapter.app.usable_emoji();
        // Opening the picker fetches the custom emoji's pictures.
        if query.is_empty() {
            let pictures = custom
                .iter()
                .flat_map(|set| &set.emoji)
                .map(|emoji| emoji.picture.clone())
                .take(MAX_PICKER_EMOJI)
                .collect();
            adapter.action(&window, Action::LoadPictures(pictures));
        }
        let (rows, sections) = emoji::rows(
            &query,
            &adapter.app.frequent_reactions(16),
            &custom,
            |set| {
                let icon = adapter.app.state.pictures.get(set.icon.as_deref()?)?;
                Some(picture(icon))
            },
        );
        update_model(&adapter.models.emoji_rows, rows);
        // Matches have no sections; the sidebar keeps those it jumps back to.
        if !sections.is_empty() {
            update_model(&adapter.models.emoji_sections, sections);
        }
    });
    bind_item!(on_select_reaction, Action::ShowReactors);
    bind!(on_more_reactors, Action::MoreReactors);
    bind_item!(on_remove_reactor, Action::RemoveReactor);
    bind!(on_close_reactions, Action::CloseReactions);
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_hover_reaction(move |item, key| {
            if let Some(window) = weak.upgrade() {
                let hovered = (!key.is_empty()).then(|| (item.into(), key.into()));
                adapter
                    .borrow_mut()
                    .action(&window, Action::HoverReaction(hovered));
            }
        });
    }
    {
        let adapter = adapter.clone();
        let weak = window.as_weak();
        window.on_view_reactions(move |item, key| {
            if let Some(window) = weak.upgrade() {
                let key = (!key.is_empty()).then(|| key.into());
                adapter.borrow_mut().action(
                    &window,
                    Action::ShowReactions {
                        item: item.into(),
                        key,
                    },
                );
            }
        });
    }
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_react(move |item, key| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(
                &window,
                Action::React {
                    item: item.into(),
                    key: key.into(),
                },
            );
        }
    });
    let state = adapter.clone();
    window.on_sidebar_resized(move |width| {
        let mut adapter = state.borrow_mut();
        adapter.layout.sidebar_width = Some(width);
        adapter.layout.write(&adapter.root);
    });
    let state = adapter.clone();
    window.on_aside_resized(move |width| {
        let mut adapter = state.borrow_mut();
        adapter.layout.aside_width = Some(width);
        adapter.layout.write(&adapter.root);
    });
    let state = adapter.clone();
    window.on_member_list_toggled(move |shown| {
        let mut adapter = state.borrow_mut();
        adapter.layout.members_hidden = !shown;
        adapter.layout.write(&adapter.root);
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_profile_panel_toggled(move |shown| {
        let mut adapter = state.borrow_mut();
        adapter.layout.profile_panel_hidden = !shown;
        adapter.layout.write(&adapter.root);
        if let Some(window) = weak.upgrade() {
            adapter.action(&window, Action::ShowProfilePanel(shown));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_show_hidden_channels_toggled(move |shown| {
        let mut adapter = state.borrow_mut();
        adapter.layout.show_hidden_channels = shown;
        adapter.layout.write(&adapter.root);
        if let Some(window) = weak.upgrade() {
            adapter.action(&window, Action::ShowHiddenChannels(shown));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_reveal_attachment(move |item, index| {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        adapter.revealed.insert(format!("{item}/a{index}"));
        adapter.restyle = true;
        adapter.refresh(&window);
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_open_link(move |link| {
        // A blocked message shows when asked for.
        if let Some(item) = link.strip_prefix(BLOCKED_LINK) {
            let Some(window) = weak.upgrade() else { return };
            let mut adapter = state.borrow_mut();
            adapter.revealed.insert(format!("{item}/blocked"));
            adapter.restyle = true;
            adapter.refresh(&window);
            return;
        }
        // A spoiler shows what it hides.
        if let Some((item, index)) = spoiler::target(&link) {
            let Some(window) = weak.upgrade() else { return };
            let mut adapter = state.borrow_mut();
            adapter.revealed.insert(format!("{item}/{index}"));
            adapter.restyle = true;
            adapter.refresh(&window);
            return;
        }
        // A pin's notice jumps to the message pinned, or shows the pinned
        // messages.
        if let Some(item) = link.strip_prefix(messages::JUMP_LINK) {
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .action(&window, Action::Jump(item.to_owned()));
            }
            return;
        }
        if link == messages::PINS_LINK {
            if let Some(window) = weak.upgrade() {
                window.invoke_open_pins();
            }
            return;
        }
        // A mention opens the person's card, or the role's members, beside the
        // mention, or by the pointer when the link's place isn't known.
        let author = link
            .strip_prefix(messages::AUTHOR_LINK)
            .map(|user| mention::Target::User(user.to_owned()));
        if let Some(target) = author.or_else(|| mention::target(&link)) {
            let Some(window) = weak.upgrade() else { return };
            if let Some(rect) = i_slint_core::yapfast_selection::clicked_link() {
                window.invoke_anchor_link(
                    rect.origin.x,
                    rect.origin.y,
                    rect.size.width,
                    rect.size.height,
                );
            }
            let action = match target {
                mention::Target::User(user) => {
                    window.set_profile_at_pointer(true);
                    Action::ShowProfile { user, large: false }
                }
                mention::Target::Role(role) => Action::ShowRole(role),
                // As in Discord's client, @everyone and @here open nothing.
                mention::Target::Everyone => return,
            };
            state.borrow_mut().action(&window, action);
            return;
        }
        // Discord's own links open their place here, when Yapfast can show
        // it, and in the browser otherwise. An invite shows its server
        // first and joins only when asked, whatever the text says.
        #[cfg(feature = "discord")]
        if let Some(route) =
            yapfast_core::discord::discord_route(links::masked(&link).unwrap_or(&link))
                .filter(|route| state.borrow().app.state.opens_discord_route(route))
        {
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .action(&window, discord_link_action(&window, route));
            }
            return;
        }
        // A link whose text shows something else asks first.
        if let Some(masked) = links::masked(&link) {
            if let Some(window) = weak.upgrade() {
                window.set_leaving_site(links::host(masked).into());
                window.set_leaving_link(masked.into());
            }
            return;
        }
        open_web_page(&link);
    });
    let weak = window.as_weak();
    window.on_visit_leaving_link(move || {
        let Some(window) = weak.upgrade() else { return };
        open_web_page(&window.get_leaving_link());
        window.set_leaving_link(SharedString::default());
    });
    let weak = window.as_weak();
    window.on_close_leaving_link(move || {
        if let Some(window) = weak.upgrade() {
            window.set_leaving_link(SharedString::default());
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_choose_theme(move |id| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().choose_theme(&window, &id);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_new_theme(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().new_theme(&window);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_edit_theme(move |id| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().edit_theme(&window, &id);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_theme_start_chosen(move |index| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().theme_start_chosen(&window, index);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_theme_colour_edited(move |index, text| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .theme_colour_edited(&window, index, &text);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_theme_dark_toggled(move |dark| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().theme_dark_toggled(&window, dark);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_save_theme(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().save_theme(&window);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_cancel_theme(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().cancel_theme(&window);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_delete_theme(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().delete_theme(&window);
        }
    });
    let weak = window.as_weak();
    window.on_call_volumes_changed(move || {
        if let Some(window) = weak.upgrade() {
            play_call_volumes(&window);
        }
    });
    let weak = window.as_weak();
    window.on_record_talk_key(move |on| {
        if let Some(window) = weak.upgrade() {
            window.set_talk_recording(on);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_preferences_changed(move || {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        let layout = &mut adapter.layout;
        layout.twelve_hour_clock = window.get_twelve_hour_clock();
        layout.clock_seconds = window.get_clock_seconds();
        layout.text_size = Some(window.get_text_size());
        layout.link_previews = Some(window.get_link_previews());
        layout.reply_ping = Some(window.get_reply_ping_default());
        layout.strip_tracking = Some(window.get_strip_tracking());
        layout.animate = chosen(&ANIMATES, window.get_animate());
        ANIMATE.set(layout.animate);
        show_motion(&window);
        layout.unmute_on_join = Some(window.get_unmute_on_join());
        layout.deafen_on_join = window.get_deafen_on_join();
        layout.level_voice = Some(window.get_level_voice());
        yapfast_core::leveling::set_enabled(window.get_level_voice());
        layout.noise_suppression = chosen(&SUPPRESSORS, window.get_noise_suppression());
        layout.noise_strength = chosen(&STRENGTHS, window.get_noise_strength());
        yapfast_core::noise::set(layout.noise_suppression);
        yapfast_core::noise::set_strength(layout.noise_strength);
        layout.reduce_room_echo = window.get_dereverb();
        yapfast_core::noise::set_dereverb(layout.reduce_room_echo);
        layout.input_sensitivity = Some(window.get_sensitivity());
        yapfast_core::noise::set_sensitivity(layout.sensitivity() as f32);
        layout.input_mode = chosen(&INPUT_MODES, window.get_input_mode());
        yapfast_core::push_to_talk::set_on(layout.input_mode == InputMode::PushToTalk);
        layout.push_to_talk_release_delay = Some(window.get_talk_delay().max(0) as u32);
        yapfast_core::push_to_talk::set_release_delay(layout.release_delay());
        layout.microphone_volume = Some(window.get_microphone_volume().clamp(0, 200) as u8);
        layout.speaker_volume = Some(window.get_speaker_volume().clamp(0, 200) as u8);
        play_call_volumes(&window);
        layout.share_typing = Some(window.get_share_typing());
        layout.notifications = Some(window.get_notifications());
        layout.notification_text = Some(window.get_notification_text());
        layout.flash_taskbar = Some(window.get_flash_taskbar());
        layout.sound_set = chosen(&SoundSet::ALL, window.get_sound_set());
        layout.sound_volume = Some(window.get_sound_volume().clamp(0, 200) as u8);
        layout.adjust_each_sound = window.get_adjust_each_sound();
        layout.gpu_rendering = window.get_gpu_rendering();
        layout.gpu_encoding = Some(window.get_gpu_encoding());
        #[cfg(feature = "discord")]
        yapfast_core::stream_worker::set_hardware_encoding(window.get_gpu_encoding());
        layout.close_to_tray = Some(window.get_close_to_tray());
        layout.multiple_windows = window.get_multiple_windows();
        adapter.layout.write(&adapter.root);
        adapter.watch_talk_key(&window);
        let start = window.get_start_on_sign_in();
        if AUTOSTART.get() && start != autostart::enabled() {
            match autostart::set(start) {
                Ok(()) => tracing::info!(start, "starting at sign-in changed"),
                Err(error) => {
                    tracing::warn!(%error, "could not change starting at sign-in");
                    window.set_start_on_sign_in(!start);
                }
            }
        }
        let (reply_ping, strip_tracking) =
            (window.get_reply_ping_default(), window.get_strip_tracking());
        adapter.app.dispatch(Action::ReplyPing(reply_ping));
        adapter.app.dispatch(Action::StripTracking(strip_tracking));
        adapter.restyle = true;
        adapter.refresh(&window);
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_each_sound_changed(move |index, percent| {
        let Some((row, &sound)) = usize::try_from(index)
            .ok()
            .and_then(|row| Some((row, Sound::ALL.get(row)?)))
        else {
            return;
        };
        // The sliders show the list again when they next appear.
        if let Some(window) = weak.upgrade() {
            window.get_each_sound().set_row_data(row, percent);
        }
        let mut adapter = state.borrow_mut();
        let percent = percent.clamp(0, 100) as u8;
        if percent == 100 {
            adapter.layout.each_sound.remove(&sound);
        } else {
            adapter.layout.each_sound.insert(sound, percent);
        }
        adapter.layout.write(&adapter.root);
    });
    let state = adapter.clone();
    window.on_play_sound(move |index| {
        if let Some(&sound) = usize::try_from(index)
            .ok()
            .and_then(|index| Sound::ALL.get(index))
        {
            state.borrow().play(sound);
        }
    });
    window.on_volume_of(|person, stream| {
        let kind = if stream {
            yapfast_core::volume::Kind::Stream
        } else {
            yapfast_core::volume::Kind::Voice
        };
        yapfast_core::volume::to_slider(yapfast_core::volume::get(kind, &person)).round() as i32
    });
    let state = adapter.clone();
    window.on_volume_changed(move |person, stream, value, done| {
        let kind = if stream {
            yapfast_core::volume::Kind::Stream
        } else {
            yapfast_core::volume::Kind::Voice
        };
        // Within a step of the default is the default, as Discord snaps it.
        let default = kind.default_gain();
        let volume = if (value as f32 - yapfast_core::volume::to_slider(default)).abs() < 1.0 {
            default
        } else {
            yapfast_core::volume::from_slider(value as f32)
        };
        yapfast_core::volume::set(kind, &person, volume);
        // The drag's last step is remembered, rather than every one.
        if done {
            let mut adapter = state.borrow_mut();
            let volumes = if stream {
                &mut adapter.layout.stream_volumes
            } else {
                &mut adapter.layout.volumes
            };
            if volume == default {
                volumes.remove(person.as_str());
            } else {
                volumes.insert(person.into(), volume);
            }
            adapter.layout.write(&adapter.root);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_hide_blocked_toggled(move |hide| {
        let mut adapter = state.borrow_mut();
        adapter.layout.hide_blocked = hide;
        adapter.layout.write(&adapter.root);
        if let Some(window) = weak.upgrade() {
            adapter.action(&window, Action::HideBlocked(hide));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_show_idle_toggled(move |shown| {
        let mut adapter = state.borrow_mut();
        adapter.layout.show_idle = Some(shown);
        adapter.layout.write(&adapter.root);
        if let Some(window) = weak.upgrade() {
            adapter.idle_setting_changed(&window);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_sync_discord_reads_toggled(move |sync| {
        let mut adapter = state.borrow_mut();
        adapter.layout.sync_discord_reads = Some(sync);
        adapter.layout.write(&adapter.root);
        if let Some(window) = weak.upgrade() {
            adapter.action(&window, Action::SyncDiscordReads(sync));
        }
    });
    // The channel's composer and the thread beside's pick files alike.
    let pick = |state: Rc<RefCell<Adapter>>, stage: fn(Vec<PathBuf>) -> Action| {
        move || {
            let weak = Rc::downgrade(&state);
            state.borrow_mut().dialog(async move {
                let files = rfd::AsyncFileDialog::new()
                    .set_title("Attach files")
                    .pick_files()
                    .await;
                if let (Some(files), Some(adapter)) = (files, weak.upgrade()) {
                    let window = adapter.borrow().window.upgrade();
                    if let Some(window) = window {
                        let paths = files.iter().map(|file| file.path().to_owned()).collect();
                        adapter.borrow_mut().action(&window, stage(paths));
                    }
                }
            });
        }
    };
    window.on_attach(pick(adapter.clone(), Action::Stage));
    window.on_aside_attach(pick(adapter.clone(), Action::StageAside));
    let state = adapter.clone();
    window.on_unstage(move |index| {
        let mut adapter = state.borrow_mut();
        if let (Ok(index), Some(window)) = (usize::try_from(index), adapter.window.upgrade()) {
            adapter.action(&window, Action::Unstage(index));
        }
    });
    let state = adapter.clone();
    window.on_toggle_spoiler(move |index| {
        let mut adapter = state.borrow_mut();
        if let (Ok(index), Some(window)) = (usize::try_from(index), adapter.window.upgrade()) {
            adapter.action(&window, Action::ToggleSpoiler(index));
        }
    });
    let state = adapter.clone();
    window.on_aside_unstage(move |index| {
        let mut adapter = state.borrow_mut();
        if let (Ok(index), Some(window)) = (usize::try_from(index), adapter.window.upgrade()) {
            adapter.action(&window, Action::UnstageAside(index));
        }
    });
    let state = adapter.clone();
    window.on_aside_toggle_spoiler(move |index| {
        let mut adapter = state.borrow_mut();
        if let (Ok(index), Some(window)) = (usize::try_from(index), adapter.window.upgrade()) {
            adapter.action(&window, Action::ToggleAsideSpoiler(index));
        }
    });
    let state = adapter.clone();
    window.on_paste_into_composer(move || state.borrow_mut().paste(false));
    let state = adapter.clone();
    window.on_aside_paste(move || state.borrow_mut().paste(true));
    let state = adapter.clone();
    window.on_copy_attachment(move |item, index| {
        let mut adapter = state.borrow_mut();
        if let (Ok(index), Some(window)) = (usize::try_from(index), adapter.window.upgrade()) {
            let item = item.into();
            adapter.action(&window, Action::CopyAttachment { item, index });
        }
    });
    window.on_selection_at(|x, y| {
        i_slint_core::yapfast_selection::copied_at(i_slint_core::lengths::LogicalPoint::new(x, y))
            .unwrap_or_default()
            .into()
    });
    window.on_link_at(|x, y| {
        let point = i_slint_core::lengths::LogicalPoint::new(x, y);
        i_slint_core::yapfast_selection::pressed_link_at(point)
            .as_deref()
            .and_then(web_address)
            .unwrap_or_default()
            .into()
    });
    let state = adapter.clone();
    window.on_attachment_link(move |item, index| {
        let adapter = state.borrow();
        usize::try_from(index)
            .ok()
            .and_then(|index| {
                adapter
                    .app
                    .state
                    .message(&item)?
                    .attachments
                    .get(index)?
                    .url
                    .as_deref()
            })
            .unwrap_or_default()
            .into()
    });
    let state = adapter.clone();
    window.on_save_attachment(move |item, index| {
        let item = item.to_string();
        let index = usize::try_from(index).unwrap_or_default();
        let name = state
            .borrow()
            .app
            .state
            .message(&item)
            .and_then(|message| message.attachments.get(index))
            .map(|attachment| attachment.name.clone())
            .unwrap_or_default();
        let weak = Rc::downgrade(&state);
        state.borrow_mut().dialog(async move {
            // Only the file name is suggested; a sender cannot choose the
            // folder, hide the file with a leading dot, or reverse how its
            // extension reads.
            let name = std::path::Path::new(&name)
                .file_name()
                .map(|name| {
                    name.to_string_lossy()
                        .chars()
                        .filter(|&c| !c.is_control() && !links::hidden(c))
                        .collect::<String>()
                        .trim_start_matches(['.', ' '])
                        .to_owned()
                })
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "attachment".into());
            let file = rfd::AsyncFileDialog::new()
                .set_title("Save attachment")
                .set_file_name(name)
                .save_file()
                .await;
            if let (Some(file), Some(adapter)) = (file, weak.upgrade()) {
                let window = adapter.borrow().window.upgrade();
                if let Some(window) = window {
                    adapter.borrow_mut().action(
                        &window,
                        Action::SaveAttachment {
                            item,
                            index,
                            path: file.path().to_owned(),
                        },
                    );
                }
            }
        });
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_login(move || {
        if let Some(window) = weak.upgrade() {
            let mut state = state.borrow_mut();
            let busy = state.app.state.busy;
            state.action(
                &window,
                Action::Login {
                    homeserver: window.get_homeserver().into(),
                    username: window.get_username().into(),
                    password: window.get_password().into(),
                },
            );
            if !busy && state.app.state.busy {
                window.set_password(SharedString::default());
            }
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_recover(move || {
        if let Some(window) = weak.upgrade() {
            let mut state = state.borrow_mut();
            let busy = state.app.state.busy;
            state.action(&window, Action::Recover(window.get_recovery().into()));
            if !busy && state.app.state.busy {
                window.set_recovery(SharedString::default());
            }
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_navigate_server(move |id| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::Navigate(Navigation::Server(id.into())));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_toggle_folder(move |id| {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        if !adapter.layout.open_folders.remove(id.as_str()) {
            adapter.layout.open_folders.insert(id.into());
        }
        adapter.layout.write(&adapter.root);
        // What the sidebar was built from leaves the layout out.
        adapter.sidebar = None;
        adapter.show_sidebar(&window);
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_open_aside(move |id| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::OpenAside(id.into()));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_open_conversation(move |id| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(&window, Action::Open(id.into()));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_open_chat(move |id| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::OpenChat(id.into()));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_show_voice_chat(move |chat| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::ShowVoiceChat(chat));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_select_room(move |id| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::SelectRoom(id.into()));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_composer_caret(move |text, offset| {
        let Some(window) = weak.upgrade() else { return };
        // Choosing a suggestion moves the cursor while it's being handled.
        let Ok(mut adapter) = state.try_borrow_mut() else {
            return;
        };
        // The cursor moves before the edit is reported; take the text first, so the
        // refresh below doesn't put back the text before it.
        if adapter.app.state.draft() != text.as_str() {
            adapter.action(&window, Action::SetDraft(text.to_string()));
        }
        let offset = usize::try_from(offset).ok();
        adapter.caret = adapter.app.state.selected.clone().zip(offset);
        // The `:name` word, unless its suggestions were closed.
        let word = offset.and_then(|offset| emoji::query(&text, offset));
        if word.is_none() {
            adapter.emoji_dismissed = None;
        }
        let word = word
            .filter(|(at, _)| adapter.emoji_dismissed != Some(*at))
            .map(|(at, name)| (at, name.to_owned()));
        if adapter.emoji_word != word {
            adapter.emoji_word = word;
            window.set_emoji_suggestion_index(0);
            adapter.show_emoji_suggestions(&window);
            // Custom emoji show their pictures once they arrive.
            let wanted: Vec<String> = adapter
                .app
                .usable_emoji()
                .into_iter()
                .flat_map(|set| set.emoji)
                .filter(|custom| {
                    let key = emoji::custom_key(custom);
                    !adapter.app.state.pictures.contains_key(&custom.picture)
                        && adapter
                            .models
                            .emoji_suggestions
                            .iter()
                            .any(|cell| cell.key == key)
                })
                .map(|custom| custom.picture)
                .collect();
            if !wanted.is_empty() {
                adapter.action(&window, Action::LoadPictures(wanted));
            }
        }
        let found = offset.and_then(|offset| mention::query(&text, offset));
        if found.is_none() {
            adapter.mention_dismissed = None;
        }
        let found = found.filter(|(at, _)| adapter.mention_dismissed != Some(*at));
        adapter.mention_at = found.map(|(at, _)| at);
        let query = found.map(|(_, query)| query.to_owned());
        if adapter.app.state.mention_query != query {
            window.set_mention_index(0);
            adapter.action(&window, Action::MentionQuery(query));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_mention_chosen(move |index| {
        let Some(window) = weak.upgrade() else { return };
        let caret = {
            let mut adapter = state.borrow_mut();
            let chosen = usize::try_from(index).ok().and_then(|index| {
                adapter
                    .app
                    .state
                    .mention_suggestions()
                    .into_iter()
                    .nth(index)
            });
            let (Some(chosen), Some(at), Some(query)) = (
                chosen,
                adapter.mention_at,
                adapter.app.state.mention_query.clone(),
            ) else {
                return;
            };
            let text = adapter.app.state.draft().to_owned();
            let end = at + 1 + query.len();
            if text.get(at..end) != Some(format!("@{query}").as_str()) {
                return;
            }
            let inserted = format!("@{} ", chosen.name);
            let text = format!("{}{inserted}{}", &text[..at], &text[end..]);
            adapter.mention_at = None;
            adapter.action(&window, Action::MentionQuery(None));
            adapter.action(&window, Action::SetDraft(text));
            at + inserted.len()
        };
        window.invoke_place_composer_caret(i32::try_from(caret).unwrap_or(i32::MAX));
        window.invoke_focus_composer();
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_emoji_suggestion_chosen(move |index| {
        let Some(window) = weak.upgrade() else { return };
        let caret = {
            let mut adapter = state.borrow_mut();
            let chosen = usize::try_from(index)
                .ok()
                .and_then(|index| adapter.models.emoji_suggestions.row_data(index));
            let word = adapter.emoji_word.take();
            adapter.show_emoji_suggestions(&window);
            let (Some(chosen), Some((at, name))) = (chosen, word) else {
                return;
            };
            let text = adapter.app.state.draft().to_owned();
            let end = at + 1 + name.len();
            if text.get(at..end) != Some(format!(":{name}").as_str()) {
                return;
            }
            let inserted = format!("{} ", emoji::composer_text(&chosen.key));
            let text = format!("{}{inserted}{}", &text[..at], &text[end..]);
            adapter.action(&window, Action::SetDraft(text));
            at + inserted.len()
        };
        window.invoke_place_composer_caret(i32::try_from(caret).unwrap_or(i32::MAX));
        window.invoke_focus_composer();
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_emoji_suggestion_dismissed(move || {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        adapter.emoji_dismissed = adapter.emoji_word.take().map(|(at, _)| at);
        adapter.show_emoji_suggestions(&window);
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_insert_emoji(move |key| {
        let Some(window) = weak.upgrade() else { return };
        let caret = {
            let mut adapter = state.borrow_mut();
            let text = adapter.app.state.draft().to_owned();
            // At the cursor this room's composer last had, else at the end.
            let at = adapter
                .caret
                .as_ref()
                .filter(|(room, _)| adapter.app.state.selected.as_ref() == Some(room))
                .map(|&(_, at)| at)
                .filter(|&at| text.is_char_boundary(at))
                .unwrap_or(text.len());
            let inserted = emoji::composer_text(&key);
            let text = format!("{}{inserted}{}", &text[..at], &text[at..]);
            adapter.action(&window, Action::SetDraft(text));
            at + inserted.len()
        };
        window.invoke_place_composer_caret(i32::try_from(caret).unwrap_or(i32::MAX));
        window.invoke_focus_composer();
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_mention_dismissed(move || {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        adapter.mention_dismissed = adapter.mention_at.take();
        adapter.action(&window, Action::MentionQuery(None));
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_draft_edited(move |text| {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        // The caret's move usually took the text already; a second refresh
        // with nothing changed is wasted work on every keystroke.
        if adapter.app.state.draft() != text.as_str() {
            adapter.action(&window, Action::SetDraft(text.into()));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    // The filters only narrow the sidebar's lists.
    window.on_filter_edited(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().show_sidebar(&window);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_toggle_category(move |category| {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        let Navigation::Server(server) = adapter.app.state.navigation.clone() else {
            return;
        };
        let collapsed = adapter.layout.collapsed.entry(server.clone()).or_default();
        if !collapsed.remove(category.as_str()) {
            collapsed.insert(category.into());
        }
        if collapsed.is_empty() {
            adapter.layout.collapsed.remove(&server);
        }
        adapter.layout.write(&adapter.root);
        // What the sidebar was built from leaves the layout out.
        adapter.sidebar = None;
        adapter.show_sidebar(&window);
    });
    // Dragging a Discord server's channels and categories, by their places
    // in its channel list; see RoomList.
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_start_arranging(move |place| {
        let Some(window) = weak.upgrade() else {
            return false;
        };
        let mut adapter = state.borrow_mut();
        let room = usize::try_from(place - 1)
            .ok()
            .and_then(|at| adapter.places.get(at))
            .cloned();
        adapter.arranging = room;
        adapter.show_arranging(&window);
        adapter.arranging.is_some()
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_arrange_room(move |place| {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        window.set_room_arranging(0);
        let Some(room) = adapter.arranging.take() else {
            return;
        };
        let landing = adapter
            .places
            .iter()
            .position(|known| *known == room)
            .zip(usize::try_from(place - 1).ok())
            .and_then(|(dragged, onto)| adapter.app.landing(&adapter.places, dragged, onto));
        if let Some(landing) = landing {
            adapter.action(&window, Action::MoveChannel { room, landing });
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_stop_arranging(move || {
        let Some(window) = weak.upgrade() else { return };
        state.borrow_mut().arranging = None;
        window.set_room_arranging(0);
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_answer_move_sync(move |sync| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::AnswerMoveSync(Some(sync)));
        }
    });
    bind!(on_close_move_sync, Action::AnswerMoveSync(None));
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_friend_filter_edited(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().show_sidebar(&window);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_browse_filter_edited(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().show_sidebar(&window);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_browse_follow(move |room, follow| {
        if let Some(window) = weak.upgrade() {
            let room = room.into();
            state
                .borrow_mut()
                .action(&window, Action::BrowseFollow { room, follow });
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_join_voice(move || {
        if let Some(window) = weak.upgrade() {
            let id = state.borrow().app.state.selected.clone();
            if let Some(id) = id {
                state
                    .borrow_mut()
                    .action(&window, Action::RequestJoinVoice(id));
            }
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_toggle_mute(move || {
        if let Some(window) = weak.upgrade() {
            if !state.borrow().app.state.voice.active() {
                state.borrow_mut().toggle_on_join(&window, false);
                return;
            }
            let muted = state.borrow().app.state.voice.muted;
            state
                .borrow_mut()
                .action(&window, Action::Voice(voice::Command::Mute(!muted)));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_toggle_share(move || {
        if let Some(window) = weak.upgrade() {
            let command = {
                let state = state.borrow();
                let voice = &state.app.state.voice;
                if voice.sharing.is_some() {
                    voice::Command::StopLive
                } else {
                    voice::Command::GoLive(voice.quality)
                }
            };
            state.borrow_mut().action(&window, Action::Voice(command));
        }
    });
    #[cfg(feature = "discord")]
    window.set_share_lists(yapfast_core::stream_worker::LISTS_SOURCES);
    #[cfg(feature = "discord")]
    let weak = window.as_weak();
    #[cfg(feature = "discord")]
    window.on_list_share_sources(move || {
        use yapfast_core::stream_worker::{Report, Source, Worker};
        // A picture fills a card in the share menu, in pixels.
        let scale = weak
            .upgrade()
            .map_or(1.0, |window| window.window().scale_factor());
        let (width, height) = ((160.0 * scale).ceil() as u32, (90.0 * scale).ceil() as u32);
        // Listing starts a capturer for screens and one for windows, so it
        // runs beside the window, and the menu opens when it's done.
        let weak = weak.clone();
        std::thread::spawn(move || {
            let listed = yapfast_core::stream_worker::shareable();
            let (screens, windows): (Vec<_>, Vec<_>) = listed
                .into_iter()
                .partition(|listed| matches!(listed.source, Source::Display(_)));
            let ids: Vec<u64> = screens
                .iter()
                .filter_map(|listed| match listed.source {
                    Source::Display(id) => Some(id),
                    _ => None,
                })
                .collect();
            let opened = weak.upgrade_in_event_loop(move |window| {
                let rows: Vec<ShareScreen> = screens
                    .iter()
                    .map(|listed| ShareScreen {
                        title: listed.title.as_str().into(),
                        picture: Default::default(),
                    })
                    .collect();
                let titles: Vec<SharedString> = windows
                    .iter()
                    .map(|listed| listed.title.as_str().into())
                    .collect();
                window.set_share_screens(Rc::new(VecModel::from(rows)).into());
                window.set_share_windows(Rc::new(VecModel::from(titles)).into());
                SHAREABLE.with_borrow_mut(|shareable| {
                    *shareable = screens
                        .iter()
                        .chain(&windows)
                        .map(|listed| listed.source)
                        .collect();
                });
                window.invoke_show_share_menu();
            });
            if opened.is_err() || ids.is_empty() {
                return;
            }
            // The pictures come from a worker, so capturing the screens takes
            // memory only until they're in.
            let (_worker, mut reports) = match Worker::glance(ids, width, height) {
                Ok(started) => started,
                Err(error) => {
                    tracing::warn!("no pictures of the screens to share: {error:#}");
                    return;
                }
            };
            while let Some(report) = reports.blocking_recv() {
                match report {
                    Report::Screen {
                        id,
                        width,
                        height,
                        rgba,
                    } => {
                        let pixels =
                            slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                                &rgba, width, height,
                            );
                        let _ = weak.upgrade_in_event_loop(move |window| {
                            let at = SHAREABLE.with_borrow(|shareable| {
                                shareable
                                    .iter()
                                    .position(|&source| source == Source::Display(id))
                            });
                            let screens = window.get_share_screens();
                            if let Some(at) = at
                                && let Some(mut row) = screens.row_data(at)
                            {
                                row.picture = slint::Image::from_rgba8(pixels);
                                screens.set_row_data(at, row);
                            }
                        });
                    }
                    Report::Ended { error } => {
                        if let Some(error) = error {
                            tracing::warn!("no pictures of the screens to share: {error}");
                        }
                        break;
                    }
                    _ => {}
                }
            }
        });
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_go_live(move |source, resolution, fps, high_quality| {
        if let Some(window) = weak.upgrade() {
            #[cfg(feature = "discord")]
            {
                let picked =
                    SHAREABLE.with_borrow(|shareable| shareable.get(source as usize).copied());
                yapfast_core::stream_worker::pick(
                    picked.unwrap_or(yapfast_core::stream_worker::Source::Screen),
                );
            }
            #[cfg(not(feature = "discord"))]
            let _ = source;
            let resolution = match resolution {
                0 => voice::Resolution::P720,
                2 => voice::Resolution::Source,
                _ => voice::Resolution::P1080,
            };
            let quality = voice::StreamQuality {
                resolution,
                fps: fps.clamp(1, 60) as u32,
                high_quality,
            };
            state
                .borrow_mut()
                .action(&window, Action::Voice(voice::Command::GoLive(quality)));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_switch_live(move |source| {
        if let Some(window) = weak.upgrade() {
            #[cfg(feature = "discord")]
            if let Some(picked) =
                SHAREABLE.with_borrow(|shareable| shareable.get(source as usize).copied())
            {
                yapfast_core::stream_worker::pick(picked);
            }
            #[cfg(not(feature = "discord"))]
            let _ = source;
            state
                .borrow_mut()
                .action(&window, Action::Voice(voice::Command::SwitchLive));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_show_voice_person(move |room, user| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(
                &window,
                Action::ShowVoicePerson {
                    room: room.into(),
                    user: user.into(),
                },
            );
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_watch_stream(move |id| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::Voice(voice::Command::Watch(id.into())));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_stop_watching(move || {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::Voice(voice::Command::StopWatching));
        }
    });
    // Frames skip the application state, which would redraw far more than the pictures.
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_stream_tick(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let adapter = state.borrow();
        let voice = &adapter.app.state.voice;
        let image = |width, height, pixels: &[u8]| {
            slint::Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(
                pixels, width, height,
            ))
        };
        if let Some(frame) = voice
            .sharing
            .as_ref()
            .and_then(|sharing| sharing.frames.take(image))
        {
            window.set_share_frame(frame);
        }
        place_videos(&adapter, &window);
        let frames = &adapter.models.participant_frames;
        for (index, person) in voice.participants.iter().enumerate() {
            if let Some(frame) = person
                .video
                .as_ref()
                .and_then(|feed| feed.take(pictures::frame))
                && index < frames.row_count()
            {
                frames.set_row_data(index, frame);
            }
        }
        if let Some(frame) = adapter
            .camera_test
            .as_ref()
            .and_then(|(_, feed)| feed.take(pictures::frame))
        {
            window.set_camera_preview(frame);
        }
        if let Some(frame) = voice
            .watching
            .as_ref()
            .and_then(|watching| watching.frames.as_ref())
            .and_then(|frames| frames.take(image))
        {
            window.set_watch_frame(frame);
        }
        let video = adapter.app.state.video.as_ref();
        let taken =
            video.and_then(|video| Some((video, video.frames.as_ref()?.take_timed(image)?)));
        if taken.is_none() && video.is_some_and(|video| !video.playing && !video.loading) {
            // Half a second without a picture: a paused video's ticks stop.
            adapter.video_quiet.set(adapter.video_quiet.get() + 1);
            if adapter.video_quiet.get() > 15 {
                window.set_video_active(false);
            }
        }
        if let Some((video, (frame, time))) = taken {
            adapter.video_quiet.set(0);
            let player = window.global::<VideoPlayer>();
            player.set_frame(frame);
            player.set_has_frame(true);
            // The last picture shows a little before the end.
            let time = match video.duration_ms {
                Some(duration) if video.ended => duration,
                _ => time,
            };
            if let Some(duration) = video.duration_ms.filter(|duration| *duration > 0) {
                player.set_progress((time as f64 / duration as f64).min(1.0) as f32);
            }
            player.set_time(refresh::video_time(time, video.duration_ms).into());
        }
    });
    // The video playing: started from its cell, controlled from its bar.
    let player = window.global::<VideoPlayer>();
    let state = adapter.clone();
    let weak = window.as_weak();
    player.on_play(move |item, index, width, height| {
        if let Some(window) = weak.upgrade() {
            let scale = window.window().scale_factor();
            let pixels = |side: f32| (side * scale).round().max(1.0) as u32;
            let (width, height) = (pixels(width), pixels(height));
            let action = Action::PlayVideo {
                item: item.into(),
                index: index as usize,
                width,
                height,
            };
            let mut adapter = state.borrow_mut();
            adapter.video_cell.set((width, height));
            adapter.action(&window, action);
        }
    });
    // Full screen, its pictures come at the viewer's size; back in its
    // cell, at the cell's.
    let state = adapter.clone();
    let weak = window.as_weak();
    player.on_expand(move |expanded| {
        if let Some(window) = weak.upgrade() {
            window.global::<VideoPlayer>().set_expanded(expanded);
            if !expanded {
                let mut adapter = state.borrow_mut();
                let (width, height) = adapter.video_cell.get();
                adapter.action(&window, Action::Video(VideoCommand::Size { width, height }));
            }
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    player.on_resize(move |width, height| {
        if let Some(window) = weak.upgrade()
            && window.global::<VideoPlayer>().get_expanded()
        {
            let scale = window.window().scale_factor();
            let pixels = |side: f32| (side * scale).round().max(1.0) as u32;
            let size = VideoCommand::Size {
                width: pixels(width),
                height: pixels(height),
            };
            state.borrow_mut().action(&window, Action::Video(size));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    player.on_toggle(move || {
        if let Some(window) = weak.upgrade() {
            let playing = state
                .borrow()
                .app
                .state
                .video
                .as_ref()
                .is_some_and(|video| video.playing);
            let command = if playing {
                VideoCommand::Pause
            } else {
                VideoCommand::Play
            };
            state.borrow_mut().action(&window, Action::Video(command));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    player.on_seek(move |fraction| {
        if let Some(window) = weak.upgrade() {
            let duration = state
                .borrow()
                .app
                .state
                .video
                .as_ref()
                .and_then(|video| video.duration_ms);
            if let Some(duration) = duration {
                let at = (f64::from(fraction.clamp(0.0, 1.0)) * duration as f64) as u64;
                state
                    .borrow_mut()
                    .action(&window, Action::Video(VideoCommand::Seek(at)));
            }
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    player.on_mute(move |muted| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::Video(VideoCommand::Mute(muted)));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    player.on_change_volume(move |volume| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::Video(VideoCommand::Volume(volume)));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_join_unmuted(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(&window, Action::UnmuteOnJoin);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_join_deafened(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(&window, Action::DeafenOnJoin);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_toggle_deafen(move || {
        if let Some(window) = weak.upgrade() {
            if !state.borrow().app.state.voice.active() {
                state.borrow_mut().toggle_on_join(&window, true);
                return;
            }
            let deafened = state.borrow().app.state.voice.deafened;
            state
                .borrow_mut()
                .action(&window, Action::Voice(voice::Command::Deafen(!deafened)));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_select_microphone(move |index| {
        if let Some(window) = weak.upgrade() {
            let id = usize::try_from(index).ok().and_then(|index| {
                state
                    .borrow()
                    .app
                    .state
                    .voice
                    .microphones
                    .get(index)
                    .map(|device| device.id.clone())
            });
            if let Some(id) = id {
                let mut adapter = state.borrow_mut();
                if adapter.meter.is_some() {
                    adapter.meter = Some(InputMeter::start(Some(&id)));
                }
                #[cfg(feature = "discord")]
                if adapter.gate_meter.is_some() {
                    adapter.gate_meter =
                        Some(yapfast_core::discord_audio::GateMeter::start(Some(&id)));
                }
                adapter.action(&window, Action::Voice(voice::Command::Microphone(id)));
            }
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_select_speaker(move |index| {
        if let Some(window) = weak.upgrade() {
            let id = usize::try_from(index).ok().and_then(|index| {
                state
                    .borrow()
                    .app
                    .state
                    .voice
                    .speakers
                    .get(index)
                    .map(|device| device.id.clone())
            });
            if let Some(id) = id {
                state
                    .borrow_mut()
                    .action(&window, Action::Voice(voice::Command::Speaker(id)));
            }
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_play_soundboard(move |id| {
        let Some(window) = weak.upgrade() else { return };
        state.borrow_mut().action(
            &window,
            Action::Voice(voice::Command::Soundboard(id.into())),
        );
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_preview_soundboard(move |id| {
        let Some(window) = weak.upgrade() else { return };
        state.borrow_mut().action(
            &window,
            Action::Voice(voice::Command::PreviewSound(id.into())),
        );
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_favorite_soundboard(move |id, favorite| {
        let Some(window) = weak.upgrade() else { return };
        state.borrow_mut().action(
            &window,
            Action::Voice(voice::Command::FavoriteSound(id.into(), favorite)),
        );
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_soundboard_volume_committed(move |volume| {
        let Some(window) = weak.upgrade() else { return };
        state
            .borrow_mut()
            .action(&window, Action::SoundboardVolume(volume as f32));
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_entrance_sounds_wanted(move || {
        let Some(window) = weak.upgrade() else { return };
        state.borrow_mut().action(&window, Action::LoadSoundCatalog);
    });
    // Another server's entrance sound shows.
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_entrance_server_chosen(move |_| {
        let Some(window) = weak.upgrade() else { return };
        state.borrow_mut().refresh(&window);
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_choose_entrance_sound(move |id| {
        let Some(window) = weak.upgrade() else { return };
        let server = {
            let adapter = state.borrow();
            usize::try_from(window.get_entrance_server())
                .ok()
                .and_then(|index| adapter.entrance_servers.get(index).cloned())
                .flatten()
        };
        let sound = (!id.is_empty()).then(|| id.to_string());
        state
            .borrow_mut()
            .action(&window, Action::SetJoinSound { server, sound });
    });
    // The sounds' custom emoji come once the soundboard shows.
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_soundboard_opened(move || {
        let Some(window) = weak.upgrade() else { return };
        let pictures: Vec<String> = state
            .borrow()
            .app
            .state
            .voice
            .soundboard
            .iter()
            .flat_map(|groups| groups.iter())
            .flat_map(|group| &group.sounds)
            .filter_map(|sound| sound.picture.clone())
            .collect();
        if !pictures.is_empty() {
            state
                .borrow_mut()
                .action(&window, Action::LoadPictures(pictures));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_toggle_camera(move || {
        let Some(window) = weak.upgrade() else { return };
        // A click while the camera starts cancels it.
        let on = state.borrow().app.state.voice.camera_phase == voice::CameraPhase::Off;
        state
            .borrow_mut()
            .action(&window, Action::Voice(voice::Command::Video(on)));
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_call_tiles(
        move |columns,
              width,
              height,
              gap,
              top,
              scrolled,
              shown,
              focused,
              focus_width,
              focus_height| {
            let Some(window) = weak.upgrade() else { return };
            let mut adapter = state.borrow_mut();
            adapter.tiles = Some(Tiles {
                columns: columns.max(0) as u32,
                width,
                height,
                gap,
                top,
                scrolled,
                shown,
                focused: usize::try_from(focused).ok(),
                focus_width,
                focus_height,
            });
            place_videos(&adapter, &window);
        },
    );
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_focus_tile(move |slot| {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        let adapter = &mut *adapter;
        let voice = &adapter.app.state.voice;
        let stage = Stage {
            participants: &voice.participants,
            sharing: voice.sharing.is_some(),
        };
        adapter.call_focus = usize::try_from(slot)
            .ok()
            .and_then(|slot| stage.focus_at(slot));
        adapter.show_stage(&window);
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_select_camera(move |index| {
        let Some(window) = weak.upgrade() else { return };
        let id = usize::try_from(index).ok().and_then(|index| {
            let adapter = state.borrow();
            adapter
                .app
                .state
                .voice
                .cameras
                .get(index)
                .map(|device| device.id.clone())
        });
        if let Some(id) = id {
            let mut adapter = state.borrow_mut();
            adapter.action(&window, Action::Voice(voice::Command::Camera(id)));
            if adapter.camera_test.is_some() {
                adapter.camera_test = None;
                start_camera_test(&mut adapter, &window);
            }
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_test_camera(move |on| {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        adapter.camera_test = None;
        window.set_camera_note(SharedString::default());
        if on {
            start_camera_test(&mut adapter, &window);
        }
    });
    let state = adapter.clone();
    window.on_metering(move |on| {
        let mut adapter = state.borrow_mut();
        let microphone = adapter.app.state.voice.microphone.clone();
        adapter.meter = on.then(|| InputMeter::start(microphone.as_deref()));
    });
    // The level skips the application state, which would redraw far more than the meter.
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_meter_tick(move || {
        if let (Some(window), Some(meter)) = (weak.upgrade(), &state.borrow().meter) {
            window.set_input_level(meter.level());
            window.set_meter_note(SharedString::from(if !meter::AVAILABLE {
                "The level meter isn't available on this system yet."
            } else if meter.failed() {
                "Yapfast can't read this microphone. Check that it's connected, or choose another."
            } else {
                ""
            }));
        }
    });
    #[cfg(feature = "discord")]
    {
        let state = adapter.clone();
        window.on_gate_metering(move |on| {
            let mut adapter = state.borrow_mut();
            let microphone = adapter.app.state.voice.microphone.clone();
            adapter.gate_meter =
                on.then(|| yapfast_core::discord_audio::GateMeter::start(microphone.as_deref()));
        });
        let state = adapter.clone();
        let weak = window.as_weak();
        window.on_gate_meter_tick(move || {
            if let (Some(window), Some(meter)) = (weak.upgrade(), &state.borrow().gate_meter) {
                window.set_gate_level(meter.level().max(-100.0));
                window.set_gate_sending(meter.sending());
                window.set_gate_meter_note(SharedString::from(
                    if !yapfast_core::discord_audio::AVAILABLE {
                        "Discord voice and its level aren't available on this system yet."
                    } else if meter.failed() {
                        "Yapfast can't read this microphone. Check that it's connected, or choose another."
                    } else {
                        ""
                    },
                ));
            }
        });
    }
}

#[cfg(feature = "discord")]
thread_local! {
    /// What the share menu lists to share, in its order.
    static SHAREABLE: RefCell<Vec<yapfast_core::stream_worker::Source>> = const { RefCell::new(Vec::new()) };
}

/// Films with the chosen camera into the voice settings' preview, which is
/// about 400 by 225 points.
fn start_camera_test(adapter: &mut Adapter, window: &YapfastWindow) {
    let scale = window.window().scale_factor();
    let feed = yapfast_core::picture::Feed::new();
    feed.show_at(Some(((400.0 * scale) as u32, (225.0 * scale) as u32)));
    let camera = adapter.app.state.voice.camera.clone();
    match yapfast_core::camera::Camera::open(camera.as_deref(), Box::new(feed.sink())) {
        Ok(camera) => adapter.camera_test = Some((camera, feed)),
        Err(error) => window.set_camera_note(error.to_string().into()),
    }
}

/// Tells each camera in the call where its tile shows, or that it doesn't,
/// so pictures are made only for tiles in sight, at their size. With the
/// call off screen, the camera in the corner shows there.
fn place_videos(adapter: &Adapter, window: &YapfastWindow) {
    let voice = &adapter.app.state.voice;
    let shown = window.get_grid_shown() && voice.active();
    let scale = window.window().scale_factor();
    let corner = usize::try_from(window.get_pip_person())
        .ok()
        .filter(|_| window.get_pip_shown() && voice.active());
    let pixels = |side: f32| (side * scale).round() as u32;
    // Shared screens take the grid's first places.
    let before = Stage {
        participants: &voice.participants,
        sharing: voice.sharing.is_some(),
    }
    .before();
    for (index, person) in voice.participants.iter().enumerate() {
        if let Some(feed) = &person.video {
            let at = if corner == Some(index) {
                Some((pixels(PIP_WIDTH), pixels(PIP_HEIGHT)))
            } else {
                adapter
                    .tiles
                    .filter(|_| shown)
                    .and_then(|tiles| tiles.shown_at(index + before, scale))
            };
            feed.show_at(at);
        }
    }
}

/// The corner's size, as StreamPip draws it.
const PIP_WIDTH: f32 = 320.0;
const PIP_HEIGHT: f32 = 180.0;

/// Opens a web page in the browser. Only web pages open: a message can't
/// start other programs.
/// The web address a link in a message leads to: a masked link's, behind
/// the question asked before leaving. A mention or Yapfast's own link has none.
pub(super) fn web_address(link: &str) -> Option<&str> {
    let link = links::masked(link).unwrap_or(link);
    (links::openable(link) && mention::target(link).is_none()).then_some(link)
}

/// What opening one of Discord's own links does. A profile link anchors
/// its card to the clicked words, or the pointer, as a mention does.
#[cfg(feature = "discord")]
fn discord_link_action(window: &YapfastWindow, route: discord::DiscordRoute) -> Action {
    match route {
        discord::DiscordRoute::Invite { code } => Action::OpenInviteLink(code),
        discord::DiscordRoute::Authorize { query } => Action::OpenOAuthLink(query),
        discord::DiscordRoute::DirectMessages => Action::Navigate(Navigation::DirectMessages),
        discord::DiscordRoute::Server { guild } => {
            Action::Navigate(Navigation::Server(discord::server_id(&guild)))
        }
        discord::DiscordRoute::Channel { guild, channel } => {
            Action::Open(discord::room_id(&guild, &channel))
        }
        discord::DiscordRoute::Message {
            guild,
            channel,
            message,
        } => Action::JumpInRoom {
            room: discord::room_id(&guild, &channel),
            item: format!("d:{message}"),
        },
        discord::DiscordRoute::User { user } => {
            if let Some(rect) = i_slint_core::yapfast_selection::clicked_link() {
                window.invoke_anchor_link(
                    rect.origin.x,
                    rect.origin.y,
                    rect.size.width,
                    rect.size.height,
                );
            }
            window.set_profile_at_pointer(true);
            Action::ShowProfile { user, large: false }
        }
    }
}

fn open_web_page(link: &str) {
    if links::openable(link) {
        super::browser::open(link);
    }
}

/// The least time between two reports of what's in view, or of the pictures
/// asked for, so scrolling sends one report per this much.
const REPORT_SPACING: Duration = Duration::from_millis(50);

/// How long to wait before a report, noting it as made then: no time after
/// a quiet spell, so a channel just shown asks for its previews and avatars
/// at once, else until [`REPORT_SPACING`] after the last. Even no time
/// gathers everything the current layout asks for.
fn report_delay(last: &Cell<Option<std::time::Instant>>) -> Duration {
    let now = std::time::Instant::now();
    let delay = last.get().map_or(Duration::ZERO, |last| {
        REPORT_SPACING.saturating_sub(now.saturating_duration_since(last))
    });
    last.set(Some(now + delay));
    delay
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn reports_at_once_after_a_quiet_spell_and_spaced_while_busy() {
        let last = Cell::new(None);
        assert_eq!(report_delay(&last), Duration::ZERO);
        let busy = report_delay(&last);
        assert!(busy > Duration::from_millis(40) && busy <= REPORT_SPACING);
        last.set(Some(Instant::now() - REPORT_SPACING));
        assert_eq!(report_delay(&last), Duration::ZERO);
    }
}
