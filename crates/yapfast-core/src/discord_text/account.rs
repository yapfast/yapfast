//! Commands about the account rather than a channel: finding people,
//! profiles, role members, member search, friends, the status, calls
//! ringing, message requests, a channel's pins and search, group DMs,
//! notification settings and invites.

use super::{
    channels::SEARCH_PAUSE,
    channels::SharedPreviews,
    commands::NOT_DISCORD,
    render::{chat_profile, found_in, hits_in, with_previews},
    *,
};
use crate::chat::{
    GiftPreview, InvitePreview, JoinedServer, MemberChange, Muting, NotificationChange,
    NotifyLevel, RelationshipChange,
};

/// Carries out a command about the account, or gives back any other.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    searches: &Arc<std::sync::atomic::AtomicU64>,
    previews: &SharedPreviews,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    // Matrix looks for people too.
    if let Command::FindPeople { query } = &command {
        if let Ok(discord) = discord {
            let people = discord
                .find_people(query)
                .into_iter()
                .map(|person| Contact {
                    detail: if person.friend {
                        "Discord friend".into()
                    } else {
                        "On Discord".into()
                    },
                    id: person.id,
                    name: person.name,
                    avatar: Some(person.avatar),
                    discord: true,
                })
                .collect();
            let _ = events.send(Event::People {
                query: query.clone(),
                discord: true,
                people,
            });
        }
        return None;
    }
    if let Command::StartDm { id, discord: true } = command {
        let discord = match discord {
            Ok(discord) => discord,
            Err(why) => {
                let _ = events.send(Event::DmStarted(Err(why.into())));
                return None;
            }
        };
        let (discord, events) = (discord.clone(), events.clone());
        tokio::spawn(async move {
            let result = discord
                .open_dm(&id)
                .await
                .map(|channel| discord::room_id(yapfast_discord::ready::PRIVATE, &channel))
                .map_err(|error| format!("Could not start the DM: {error}"));
            let _ = events.send(Event::DmStarted(result));
        });
        return None;
    }
    if let Command::Discord(DiscordCommand::RoleMembers { room, role }) = command {
        if let (Ok(discord), Some((guild, _))) = (discord, discord::parse_room(&room)) {
            let snapshot = discord.snapshots().borrow().clone();
            let server = snapshot.servers.iter().find(|server| server.id == guild);
            let mut members: Vec<ChatMember> = discord
                .members_with_role(guild, &role)
                .into_iter()
                .map(|user| ChatMember {
                    name: discord.name(guild, &user).unwrap_or_else(|| user.clone()),
                    username: discord.username(&user),
                    avatar: Some(discord.avatar(Some(guild), &user)),
                    color: server.and_then(|server| discord.color(server, &user)),
                    role: Role::Member,
                    group: None,
                    presence: None,
                    activity: None,
                    activities: Default::default(),
                    roles: discord.roles(guild, &user),
                    timed_out_until: None,
                    id: user,
                })
                .collect();
            members.sort_by_key(|member| member.name.to_lowercase());
            let _ = events.send(Event::RoleMembers {
                name: server
                    .and_then(|server| server.role_names.get(&role))
                    .cloned()
                    .unwrap_or_else(|| "role".into()),
                color: server
                    .and_then(|server| server.role_colors.get(&role))
                    .map(|(_, color)| *color),
                room,
                role,
                members,
            });
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::SearchMembers { room, query }) = command {
        if let (Ok(discord), Some((guild, _))) = (discord, discord::parse_room(&room))
            && guild != yapfast_discord::ready::PRIVATE
        {
            let (discord, events, guild) = (discord.clone(), events.clone(), guild.to_owned());
            let searches = searches.clone();
            let search = searches.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            tokio::spawn(async move {
                // Typing on asks for a longer name, which covers this one.
                tokio::time::sleep(SEARCH_PAUSE).await;
                if searches.load(std::sync::atomic::Ordering::Relaxed) != search {
                    return;
                }
                let found = match discord.search_members(&guild, &query).await {
                    Ok(found) => found,
                    Err(error) => {
                        tracing::debug!(%error, "could not search Discord members");
                        return;
                    }
                };
                let snapshot = discord.snapshots().borrow().clone();
                let server = snapshot.servers.iter().find(|server| server.id == guild);
                let members = found
                    .into_iter()
                    .map(|user| ChatMember {
                        name: discord.name(&guild, &user).unwrap_or_else(|| user.clone()),
                        username: discord.username(&user),
                        avatar: Some(discord.avatar(Some(&guild), &user)),
                        color: server.and_then(|server| discord.color(server, &user)),
                        role: Role::Member,
                        group: None,
                        presence: None,
                        activity: None,
                        activities: Default::default(),
                        roles: discord.roles(&guild, &user),
                        timed_out_until: None,
                        id: user,
                    })
                    .collect();
                let _ = events.send(Event::MemberMatches {
                    room,
                    query,
                    members,
                });
            });
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::Profile { room, user }) = command {
        match (discord, discord::parse_room(&room)) {
            (Ok(discord), Some((guild, _))) => {
                let (discord, events, guild) = (discord.clone(), events.clone(), guild.to_owned());
                tokio::spawn(async move {
                    let kept = |profile| {
                        let _ = events.send(Event::KeptProfile {
                            user: user.clone(),
                            profile: chat_profile(&discord, &guild, profile),
                        });
                    };
                    let result = discord
                        .profile(&guild, &user, kept)
                        .await
                        .map(|profile| chat_profile(&discord, &guild, profile))
                        .map_err(|error| {
                            tracing::warn!(%error, "could not fetch a Discord profile");
                            error.to_string()
                        });
                    let _ = events.send(Event::Profile { user, result });
                });
            }
            (discord, _) => {
                let _ = events.send(Event::Profile {
                    user,
                    result: Err(discord.err().unwrap_or(NOT_DISCORD).into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::Friend { user, add }) = command {
        match discord {
            Ok(discord) => {
                let (discord, events) = (discord.clone(), events.clone());
                // The lists change when Discord confirms it.
                tokio::spawn(async move {
                    let result = if add {
                        discord.befriend(&user).await
                    } else {
                        discord.remove_friend_request(&user).await
                    };
                    if let Err(error) = result {
                        tracing::warn!(
                            error = format!("{error:#}"),
                            "could not change a friend request"
                        );
                        let _ = events.send(Event::Error(format!(
                            "Could not {} the friend request: {error}",
                            if add { "send or accept" } else { "remove" }
                        )));
                    }
                });
            }
            Err(why) => {
                let _ = events.send(Event::Error(why.into()));
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::AddFriend { username }) = command {
        match discord {
            Ok(discord) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = discord
                        .add_friend_by_username(&username)
                        .await
                        .map(|()| username)
                        .map_err(|error| {
                            tracing::warn!(
                                error = format!("{error:#}"),
                                "could not send a friend request by username"
                            );
                            error.to_string()
                        });
                    let _ = events.send(Event::FriendAdded(result));
                });
            }
            Err(why) => {
                let _ = events.send(Event::FriendAdded(Err(why.into())));
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::PlacesUsed) = command {
        match discord {
            Ok(discord) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = discord
                        .places_used()
                        .await
                        .map(places_used)
                        .map_err(|error| error.to_string());
                    let _ = events.send(Event::PlacesUsed(result));
                });
            }
            Err(why) => {
                let _ = events.send(Event::PlacesUsed(Err(why.into())));
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::SetCustomStatus(custom)) = command {
        match discord {
            Ok(discord) => {
                let (discord, events) = (discord.clone(), events.clone());
                let custom = custom.map(|custom| yapfast_discord::settings::CustomStatus {
                    text: custom.text,
                    emoji_name: custom.emoji,
                    emoji_id: custom.emoji_id,
                    expires_at_ms: custom.expires_at_ms,
                });
                tokio::spawn(async move {
                    let result = discord.set_custom_status(custom).await.map_err(|error| {
                        tracing::warn!(
                            error = format!("{error:#}"),
                            "could not set the custom status"
                        );
                        error.to_string()
                    });
                    let _ = events.send(Event::CustomStatusSet(result));
                });
            }
            Err(why) => {
                let _ = events.send(Event::CustomStatusSet(Err(why.into())));
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::Inactive { since, show_idle }) = command {
        if let Ok(discord) = discord {
            discord.set_inactive(since, show_idle);
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::SoundboardVolume(volume)) = command {
        match discord {
            Ok(discord) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = discord
                        .set_soundboard_volume(volume)
                        .await
                        .map_err(|error| {
                            tracing::warn!(
                                error = format!("{error:#}"),
                                "could not change the soundboard volume"
                            );
                            error.to_string()
                        });
                    let _ = events.send(Event::SoundboardVolumeSet(result));
                });
            }
            Err(why) => {
                let _ = events.send(Event::SoundboardVolumeSet(Err(why.into())));
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::SoundCatalog) = command {
        match discord {
            Ok(discord) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = match crate::soundboard::Catalog::load(&discord).await {
                        Ok(catalog) => Ok(catalog.groups(&discord.snapshots().borrow()).into()),
                        Err(error) => {
                            tracing::warn!(
                                error = format!("{error:#}"),
                                "could not list the soundboard's sounds"
                            );
                            Err(error.to_string())
                        }
                    };
                    let _ = events.send(Event::SoundCatalog(result));
                });
            }
            Err(why) => {
                let _ = events.send(Event::SoundCatalog(Err(why.into())));
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::SetJoinSound { server, sound }) = command {
        match discord {
            Ok(discord) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let sound = sound
                        .as_ref()
                        .map(|(id, guild)| (id.as_str(), guild.as_deref()));
                    let result = discord
                        .set_join_sound(server.as_deref(), sound)
                        .await
                        .map_err(|error| {
                            tracing::warn!(
                                error = format!("{error:#}"),
                                "could not change the join sound"
                            );
                            error.to_string()
                        });
                    let _ = events.send(Event::JoinSoundSet(result));
                });
            }
            Err(why) => {
                let _ = events.send(Event::JoinSoundSet(Err(why.into())));
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::ShowGame(shown)) = command {
        match discord {
            Ok(discord) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = discord.set_show_game(shown).await.map_err(|error| {
                        tracing::warn!(
                            error = format!("{error:#}"),
                            "could not change sharing the game played"
                        );
                        error.to_string()
                    });
                    let _ = events.send(Event::ShowGameSet(result));
                });
            }
            Err(why) => {
                let _ = events.send(Event::ShowGameSet(Err(why.into())));
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::SetStatus(status)) = command {
        match discord {
            Ok(discord) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = discord.set_status(&status).await.map_err(|error| {
                        tracing::warn!(error = format!("{error:#}"), "could not set the status");
                        error.to_string()
                    });
                    let _ = events.send(Event::StatusSet(result));
                });
            }
            Err(why) => {
                let _ = events.send(Event::StatusSet(Err(why.into())));
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::DeclineCall { room }) = command {
        if let (Ok(discord), Some((_, channel))) = (discord, discord::parse_room(&room)) {
            let (discord, events, channel) = (discord.clone(), events.clone(), channel.to_owned());
            // The call stops ringing when Discord confirms it.
            tokio::spawn(async move {
                if let Err(error) = discord.decline(&channel).await {
                    let _ = events.send(Event::RoomError {
                        room,
                        message: format!("Could not decline the call: {error}"),
                    });
                }
            });
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::AnswerRequest { room, accept }) = command {
        match (discord, discord::parse_room(&room)) {
            (Ok(discord), Some((_, channel))) => {
                let (discord, events, channel) =
                    (discord.clone(), events.clone(), channel.to_owned());
                // The DM list changes when Discord confirms it.
                tokio::spawn(async move {
                    if let Err(error) = discord.answer_request(&channel, accept).await {
                        let _ = events.send(Event::RoomError {
                            room,
                            message: format!(
                                "Could not {} the message request: {error}",
                                if accept { "accept" } else { "ignore" }
                            ),
                        });
                    }
                });
            }
            (discord, _) => {
                let _ = events.send(Event::RoomError {
                    room,
                    message: discord.err().unwrap_or(NOT_DISCORD).into(),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::Pins { room }) = command {
        let place = discord::parse_room(&room)
            .map(|(guild, channel)| (guild.to_owned(), channel.to_owned()));
        match (discord, place) {
            (Ok(discord), Some((guild, channel))) => {
                let (discord, events) = (discord.clone(), events.clone());
                let previews = previews.clone();
                tokio::spawn(async move {
                    let pins = match discord.pins(&guild, &channel).await {
                        Ok(pins) => pins,
                        Err(error) => {
                            tracing::warn!(error = format!("{error:#}"), "could not list pins");
                            let result = Err(error.to_string());
                            let _ = events.send(Event::Pins { room, result });
                            return;
                        }
                    };
                    // The list shows at once, its pictures as they come.
                    let mut found = found_in(&discord, &room, &pins);
                    let _ = events.send(Event::Pins {
                        room: room.clone(),
                        result: Ok(found.clone()),
                    });
                    with_previews(&discord, &previews, &pins, &mut found).await;
                    let _ = events.send(Event::Pins {
                        room,
                        result: Ok(found),
                    });
                });
            }
            (discord, _) => {
                let _ = events.send(Event::Pins {
                    room,
                    result: Err(discord.err().unwrap_or(NOT_DISCORD).into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::Threads {
        room,
        query,
        offset,
    }) = command
    {
        let place = discord::parse_room(&room)
            .map(|(guild, channel)| (guild.to_owned(), channel.to_owned()));
        match (discord, place) {
            (Ok(discord), Some((guild, channel))) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let mut starters = vec![];
                    let result = match discord
                        .threads(&guild, &channel, query.as_deref(), offset)
                        .await
                    {
                        Ok(page) => {
                            let snapshot = discord.snapshots().borrow().clone();
                            let names = snapshot.names.read();
                            let threads = snapshot
                                .servers
                                .iter()
                                .find(|server| server.id == guild)
                                .map(|server| {
                                    starters = post_starters(server, &names, &page.starters);
                                    page.threads
                                        .iter()
                                        .filter_map(|thread| {
                                            // A page's threads have no marks.
                                            discord::listed_thread(
                                                server,
                                                &names,
                                                thread,
                                                Default::default(),
                                            )
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            Ok((threads, page.more))
                        }
                        Err(error) => {
                            tracing::warn!(error = format!("{error:#}"), "could not list threads");
                            Err(format!("Couldn't load the threads: {error}"))
                        }
                    };
                    let _ = events.send(Event::Threads {
                        room,
                        query,
                        offset,
                        result,
                        starters,
                    });
                });
            }
            (discord, _) => {
                let _ = events.send(Event::Threads {
                    room,
                    query,
                    offset,
                    result: Err(discord.err().unwrap_or(NOT_DISCORD).into()),
                    starters: vec![],
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::StartThread {
        room,
        message,
        name,
        private,
        location,
    }) = command
    {
        let place = discord::parse_room(&room)
            .map(|(guild, channel)| (guild.to_owned(), channel.to_owned()));
        let message = message
            .as_deref()
            .and_then(|item| item.strip_prefix("d:"))
            .map(str::to_owned);
        match (discord, place) {
            (Ok(discord), Some((guild, channel))) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = discord
                        .start_thread(
                            &guild,
                            &channel,
                            message.as_deref(),
                            &name,
                            private,
                            location,
                        )
                        .await
                        .map(|thread| discord::room_id(&guild, &thread))
                        .map_err(|error| {
                            tracing::warn!(
                                error = format!("{error:#}"),
                                "could not start a thread"
                            );
                            format!("Couldn't start the thread: {error}")
                        });
                    let _ = events.send(Event::ThreadStarted { room, result });
                });
            }
            (discord, _) => {
                let _ = events.send(Event::ThreadStarted {
                    room,
                    result: Err(discord.err().unwrap_or(NOT_DISCORD).into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::PostInForum {
        room,
        title,
        tags,
        body,
        files,
    }) = command
    {
        let place =
            discord::parse_room(&room).map(|(guild, forum)| (guild.to_owned(), forum.to_owned()));
        match (discord, place) {
            (Ok(discord), Some((guild, forum))) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = async {
                        let mut read = Vec::with_capacity(files.len());
                        for file in files {
                            let name = file
                                .path
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            let data = tokio::fs::read(&file.path)
                                .await
                                .map_err(|_| format!("Yapfast could not read {name}."))?;
                            read.push((
                                super::open_channel::upload_name(&name, file.spoiler),
                                data,
                            ));
                        }
                        discord
                            .post_in_forum(&guild, &forum, &title, &tags, &body, read)
                            .await
                            .map(|post| discord::room_id(&guild, &post))
                            .map_err(|error| {
                                tracing::warn!(
                                    error = format!("{error:#}"),
                                    "could not post in a forum"
                                );
                                format!("Couldn't create the post: {error}")
                            })
                    }
                    .await;
                    let _ = events.send(Event::ForumPosted { room, result });
                });
            }
            (discord, _) => {
                let _ = events.send(Event::ForumPosted {
                    room,
                    result: Err(discord.err().unwrap_or(NOT_DISCORD).into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::ViewForum { room }) = &command {
        if let (Ok(discord), Some((guild, forum))) = (discord, discord::parse_room(room)) {
            // Its posts come with the server's threads; it has no member list.
            discord.view(guild, None);
            // Its topic is the guidelines the form for a post shows.
            let topic = super::render::forum_topic(discord, guild, forum);
            let _ = events.send(Event::Topic {
                room: room.clone(),
                topic,
            });
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::Posts { room, posts }) = command {
        let place =
            discord::parse_room(&room).map(|(guild, forum)| (guild.to_owned(), forum.to_owned()));
        match (discord, place) {
            (Ok(discord), Some((guild, forum))) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let ids: Vec<&str> = posts
                        .iter()
                        .filter_map(|post| Some(discord::parse_room(post)?.1))
                        .collect();
                    let result = match discord.posts(&guild, &forum, &ids).await {
                        Ok(found) => {
                            let snapshot = discord.snapshots().borrow().clone();
                            let names = snapshot.names.read();
                            Ok(snapshot
                                .servers
                                .iter()
                                .find(|server| server.id == guild)
                                .map(|server| post_starters(server, &names, &found))
                                .unwrap_or_default())
                        }
                        Err(error) => {
                            tracing::warn!(error = format!("{error:#}"), "could not load posts");
                            Err(format!("Couldn't load the posts: {error}"))
                        }
                    };
                    let _ = events.send(Event::Posts { room, result });
                });
            }
            (discord, _) => {
                let _ = events.send(Event::Posts {
                    room,
                    result: Err(discord.err().unwrap_or(NOT_DISCORD).into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::CloseThreads) = command {
        if let Ok(discord) = discord {
            discord.forget_listed_threads();
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::Search {
        place,
        query,
        offset,
    }) = command
    {
        // A DM is searched on its own, and a server as a whole.
        let scope = discord::parse_room(&place)
            .map(|(guild, channel)| (guild.to_owned(), Some(channel.to_owned())))
            .or_else(|| discord::parse_server(&place).map(|guild| (guild.to_owned(), None)));
        match (discord, scope) {
            (Ok(discord), Some((guild, channel))) => {
                let (discord, events) = (discord.clone(), events.clone());
                let previews = previews.clone();
                tokio::spawn(async move {
                    let result = match discord
                        .search(&guild, channel.as_deref(), &query, offset)
                        .await
                    {
                        Ok(page) => {
                            let mut hits = hits_in(&discord, &guild, &page.messages);
                            let mut found: Vec<_> = hits
                                .iter_mut()
                                .map(|hit| std::mem::take(&mut hit.message))
                                .collect();
                            with_previews(&discord, &previews, &page.messages, &mut found).await;
                            for (hit, message) in hits.iter_mut().zip(found) {
                                hit.message = message;
                            }
                            Ok((page.total, hits))
                        }
                        Err(error) => {
                            tracing::warn!(error = format!("{error:#}"), "could not search");
                            Err(error.to_string())
                        }
                    };
                    let _ = events.send(Event::Found {
                        place,
                        query,
                        offset,
                        result,
                    });
                });
            }
            (discord, _) => {
                let _ = events.send(Event::Found {
                    place,
                    query,
                    offset,
                    result: Err(discord.err().unwrap_or(NOT_DISCORD).into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::AddToGroup { room, users }) = command {
        let discord = match discord {
            Ok(discord) => discord.clone(),
            Err(why) => {
                let _ = events.send(Event::GroupChanged(Err(why.into())));
                return None;
            }
        };
        let events = events.clone();
        tokio::spawn(async move {
            let channel = room
                .as_deref()
                .and_then(discord::parse_room)
                .map(|(_, channel)| channel.to_owned());
            let result = match (&room, channel) {
                (Some(room), Some(channel)) => discord
                    .add_to_group(&channel, &users)
                    .await
                    .map(|()| room.clone()),
                (Some(_), None) => Err(anyhow::anyhow!(NOT_DISCORD)),
                (None, _) => discord
                    .create_group(&users)
                    .await
                    .map(|channel| discord::room_id(yapfast_discord::ready::PRIVATE, &channel)),
            };
            let result = result.map_err(|error| {
                tracing::warn!(error = format!("{error:#}"), "could not add to a group DM");
                if room.is_some() {
                    format!("Could not add them: {error}")
                } else {
                    format!("Could not make the group: {error}")
                }
            });
            let _ = events.send(Event::GroupChanged(result));
        });
        return None;
    }
    if let Command::Discord(DiscordCommand::CreateInvite {
        room,
        max_age,
        max_uses,
    }) = command
    {
        let channel = discord::parse_room(&room).map(|(_, channel)| channel.to_owned());
        match (discord, channel) {
            (Ok(discord), Some(channel)) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = discord
                        .create_invite(&channel, max_age, max_uses)
                        .await
                        .map_err(|error| {
                            tracing::warn!(
                                error = format!("{error:#}"),
                                "could not make an invite"
                            );
                            format!("Could not make an invite: {error}")
                        });
                    let _ = events.send(Event::InviteMade { room, result });
                });
            }
            (discord, _) => {
                let _ = events.send(Event::InviteMade {
                    room,
                    result: Err(discord.err().unwrap_or(NOT_DISCORD).into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::SendInvite { user, room }) = command {
        let channel = discord::parse_room(&room).map(|(_, channel)| channel.to_owned());
        match (discord, channel) {
            (Ok(discord), Some(channel)) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = discord.send_invite(&user, &channel).await.map_err(|error| {
                        tracing::warn!(error = format!("{error:#}"), "could not send an invite");
                        format!("Couldn't send the invite: {error}")
                    });
                    let _ = events.send(Event::InviteSent { user, room, result });
                });
            }
            (discord, _) => {
                let _ = events.send(Event::InviteSent {
                    user,
                    room,
                    result: Err(discord.err().unwrap_or(NOT_DISCORD).into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::ResolveInvite { code, typed }) = command {
        match discord {
            Ok(discord) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = discord
                        .invite(&code, typed.as_deref())
                        .await
                        .map(|preview| invite_preview(&preview))
                        .map_err(|error| {
                            tracing::warn!(
                                error = format!("{error:#}"),
                                "could not look up an invite"
                            );
                            error.to_string()
                        });
                    let _ = events.send(Event::InviteResolved { code, result });
                });
            }
            Err(error) => {
                let _ = events.send(Event::InviteResolved {
                    code,
                    result: Err(error.into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::JoinServer { preview, from_link }) = command {
        let code = preview.code.clone();
        let invite = discord::parse_room(&preview.room).map(|(guild, channel)| {
            yapfast_discord::invite::Preview {
                code: preview.code.clone(),
                guild: guild.to_owned(),
                name: preview.name.clone(),
                icon: preview.icon.clone(),
                channel: channel.to_owned(),
                channel_name: preview.channel.clone(),
                channel_type: preview.channel_type,
                members: preview.members,
                online: preview.online,
            }
        });
        match (discord, invite) {
            (Ok(discord), Some(invite)) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    use yapfast_discord::invite::JoinFrom;
                    let from = if from_link {
                        JoinFrom::Link
                    } else {
                        JoinFrom::Dialog
                    };
                    let captchas = events.clone();
                    let result = discord
                        .join_invite(&invite, from, |page| {
                            let _ = captchas.send(Event::JoinCaptcha {
                                code: invite.code.clone(),
                                page: page.to_owned(),
                            });
                        })
                        .await
                        .map(|joined| JoinedServer {
                            server: discord::server_id(&joined.guild),
                            room: discord::room_id(&joined.guild, &joined.channel),
                            form: joined.verification_form,
                        })
                        .map_err(|error| {
                            tracing::warn!(error = format!("{error:#}"), "could not join a server");
                            format!("Couldn't join the server: {error}")
                        });
                    let _ = events.send(Event::ServerJoined { code, result });
                });
            }
            (discord, _) => {
                let _ = events.send(Event::ServerJoined {
                    code,
                    result: Err(discord.err().unwrap_or(NOT_DISCORD).into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::ResolveGift { code }) = command {
        match discord {
            Ok(discord) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = discord
                        .gift(&code)
                        .await
                        .map(|preview| gift_preview(&preview))
                        .map_err(|error| {
                            tracing::warn!(
                                error = format!("{error:#}"),
                                "could not look up a gift"
                            );
                            error.to_string()
                        });
                    let _ = events.send(Event::GiftResolved { code, result });
                });
            }
            Err(error) => {
                let _ = events.send(Event::GiftResolved {
                    code,
                    result: Err(error.into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::RedeemGift { preview, room }) = command {
        let code = preview.code.clone();
        let gift = yapfast_discord::gift::Preview {
            code: preview.code.clone(),
            name: preview.name.clone(),
            plan: preview.plan.clone(),
            gifter: preview.gifter.clone(),
            redeemed: preview.redeemed,
            spent: preview.spent,
            expires_at: preview.expires_at.clone(),
        };
        let channel = room
            .as_deref()
            .and_then(discord::parse_room)
            .map(|(_, channel)| channel.to_owned());
        match discord {
            Ok(discord) => {
                let (discord, events) = (discord.clone(), events.clone());
                tokio::spawn(async move {
                    let result = discord
                        .redeem_gift(&gift, channel.as_deref())
                        .await
                        .map_err(|error| {
                            tracing::warn!(error = format!("{error:#}"), "could not redeem a gift");
                            format!("Couldn't redeem the gift: {error}")
                        });
                    let _ = events.send(Event::GiftRedeemed { code, result });
                });
            }
            Err(error) => {
                let _ = events.send(Event::GiftRedeemed {
                    code,
                    result: Err(error.into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(DiscordCommand::OpenThread { room, beside }) = command {
        let place =
            discord::parse_room(&room).map(|(guild, thread)| (guild.to_owned(), thread.to_owned()));
        match (discord, place) {
            (Ok(discord), Some((guild, thread))) => {
                let (discord, events) = (discord.clone(), events.clone());
                // Now, before the list of threads it was in may close.
                let listed = discord.listed_thread(&thread);
                tokio::spawn(async move {
                    let result = discord
                        .open_thread(&guild, &thread, listed, beside)
                        .await
                        .map_err(|error| {
                            tracing::warn!(error = format!("{error:#}"), "could not open a thread");
                            format!("Couldn't open the thread: {error}")
                        });
                    let _ = events.send(Event::ThreadOpened { room, result });
                });
            }
            (discord, _) => {
                let _ = events.send(Event::ThreadOpened {
                    room,
                    result: Err(discord.err().unwrap_or(NOT_DISCORD).into()),
                });
            }
        }
        return None;
    }
    if let Command::Discord(
        command @ (DiscordCommand::LeaveServer { .. } | DiscordCommand::LeaveGroup { .. }),
    ) = command
    {
        leave(command, discord, events);
        return None;
    }
    if let Command::Discord(
        command @ (DiscordCommand::CreateChannel { .. }
        | DiscordCommand::ChannelSettings { .. }
        | DiscordCommand::EditChannel { .. }
        | DiscordCommand::DeleteChannel { .. }),
    ) = command
    {
        manage_channel(command, discord, events);
        return None;
    }
    if let Command::Discord(
        command @ (DiscordCommand::Relationship { .. }
        | DiscordCommand::Note { .. }
        | DiscordCommand::SetNote { .. }
        | DiscordCommand::SetNickname { .. }
        | DiscordCommand::CloseDm { .. }
        | DiscordCommand::Mute { .. }
        | DiscordCommand::Notifications { .. }
        | DiscordCommand::PinChannel { .. }
        | DiscordCommand::MoveChannels { .. }
        | DiscordCommand::FollowChannel { .. }
        | DiscordCommand::ShowAllChannels { .. }
        | DiscordCommand::JoinThread { .. }
        | DiscordCommand::Member { .. }),
    ) = command
    {
        change_person(command, discord, events);
        return None;
    }
    Some(command)
}

/// Whether a member's change comes from a dialog, which waits for how it
/// went; a role, the end of a timeout, or a change to their voice is
/// chosen from the menu.
fn from_dialog(change: &MemberChange) -> bool {
    !matches!(
        change,
        MemberChange::Role { .. }
            | MemberChange::Timeout { until_ms: None, .. }
            | MemberChange::ServerMute(_)
            | MemberChange::ServerDeafen(_)
            | MemberChange::Move(_)
    )
}

/// Carries out a choice from a Discord server's, category's or channel's
/// menu that makes, reads, changes or deletes a channel, and says how it went.
/// Leaves a Discord server or group DM, answering with [`Event::Left`].
fn leave(
    command: DiscordCommand,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) {
    let (place, target) = match command {
        DiscordCommand::LeaveServer { server } => {
            let guild = discord::parse_server(&server).map(str::to_owned);
            (server, guild.map(|guild| (guild, None)))
        }
        DiscordCommand::LeaveGroup { room, silent } => {
            let channel = discord::parse_room(&room)
                .filter(|(guild, _)| *guild == discord::DM_GUILD)
                .map(|(_, channel)| channel.to_owned());
            (room, channel.map(|channel| (channel, Some(silent))))
        }
        _ => return,
    };
    let (discord, (id, silent)) = match (discord, target) {
        (Ok(discord), Some(target)) => (discord.clone(), target),
        (discord, _) => {
            let _ = events.send(Event::Left {
                place,
                result: Err(discord.err().unwrap_or(NOT_DISCORD).into()),
            });
            return;
        }
    };
    let events = events.clone();
    tokio::spawn(async move {
        let (result, what) = match silent {
            None => (discord.leave_server(&id).await, "server"),
            Some(silent) => (discord.leave_group(&id, silent).await, "group"),
        };
        let result = result.map_err(|error| {
            tracing::warn!(error = format!("{error:#}"), what, "could not leave");
            format!("Couldn't leave the {what}: {error}")
        });
        let _ = events.send(Event::Left { place, result });
    });
}

fn manage_channel(
    command: DiscordCommand,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) {
    use crate::chat::{ChannelKind, ChannelSettings};
    let discord = match discord {
        Ok(discord) => discord.clone(),
        Err(why) => {
            let _ = events.send(match command {
                DiscordCommand::ChannelSettings { room } => Event::ChannelLoaded {
                    room,
                    result: Err(why.into()),
                },
                _ => Event::ChannelSaved(Err(why.into())),
            });
            return;
        }
    };
    let events = events.clone();
    let channel = |room: &str| discord::parse_room(room).map(|(_, channel)| channel.to_owned());
    let settings = |settings: &ChannelSettings| yapfast_discord::service::ChannelSettings {
        name: settings.name.clone(),
        topic: settings.topic.clone(),
        slowmode: settings.slowmode,
        nsfw: settings.nsfw,
    };
    tokio::spawn(async move {
        let (result, what) = match command {
            DiscordCommand::ChannelSettings { room } => {
                let result = match channel(&room) {
                    Some(id) => discord
                        .channel_settings(&id)
                        .await
                        .map(|known| ChannelSettings {
                            name: known.name,
                            topic: known.topic,
                            slowmode: known.slowmode,
                            nsfw: known.nsfw,
                        })
                        .map_err(|error| format!("Couldn't load the channel: {error}")),
                    None => Err(NOT_DISCORD.into()),
                };
                let _ = events.send(Event::ChannelLoaded { room, result });
                return;
            }
            DiscordCommand::CreateChannel {
                server,
                kind,
                parent,
                copy_of,
                settings: chosen,
            } => {
                let guild = discord::parse_server(&server)
                    .unwrap_or_default()
                    .to_owned();
                let code = match kind {
                    ChannelKind::Text => 0,
                    ChannelKind::Voice => 2,
                    ChannelKind::Category => 4,
                };
                let parent = parent.as_deref().and_then(channel);
                let copy_of = copy_of.as_deref().and_then(channel);
                let chosen = settings(&chosen);
                let made = discord
                    .stepping_up(|| {
                        discord.create_channel(
                            &guild,
                            code,
                            parent.as_deref(),
                            copy_of.as_deref(),
                            &chosen,
                        )
                    })
                    .await
                    .map(|id| Some(discord::room_id(&guild, &id)));
                (made, "make the channel")
            }
            DiscordCommand::EditChannel {
                room,
                kind,
                settings: chosen,
            } => {
                let chosen = settings(&chosen);
                let result = match channel(&room) {
                    Some(id) => discord
                        .stepping_up(|| {
                            discord.edit_channel(&id, kind == ChannelKind::Text, &chosen)
                        })
                        .await
                        .map(|()| None),
                    None => Err(anyhow::anyhow!(NOT_DISCORD)),
                };
                (result, "save the channel")
            }
            DiscordCommand::DeleteChannel { room } => {
                let result = match channel(&room) {
                    Some(id) => discord
                        .stepping_up(|| discord.delete_channel(&id))
                        .await
                        .map(|()| None),
                    None => Err(anyhow::anyhow!(NOT_DISCORD)),
                };
                (result, "delete the channel")
            }
            _ => return,
        };
        let result = result.map_err(|error| {
            tracing::warn!(error = format!("{error:#}"), "could not {what}");
            format!("Couldn't {what}: {error}")
        });
        let _ = events.send(Event::ChannelSaved(result));
    });
}

/// Whether `muting` mutes, and for how many seconds; none for good.
fn mute(muting: Muting) -> (bool, Option<u64>) {
    match muting {
        Muting::Off => (false, None),
        Muting::Minutes(minutes) => (true, Some(u64::from(minutes) * 60)),
        Muting::Always => (true, None),
    }
}

/// A change to notification settings as the Discord service takes it.
fn notification_change(change: NotificationChange) -> yapfast_discord::notifications::Change {
    use yapfast_discord::notifications::{Change, Level};
    match change {
        NotificationChange::Mute(muting) => {
            let (muted, seconds) = mute(muting);
            Change::Mute { muted, seconds }
        }
        NotificationChange::Level(level) => Change::Level(level.map(|level| match level {
            NotifyLevel::All => Level::All,
            NotifyLevel::Mentions => Level::Mentions,
            NotifyLevel::Nothing => Level::Nothing,
        })),
        NotificationChange::HideMuted(on) => Change::HideMutedChannels(on),
        NotificationChange::SuppressEveryone(on) => Change::SuppressEveryone(on),
        NotificationChange::SuppressRoles(on) => Change::SuppressRoles(on),
    }
}

/// Carries out a choice from a DM's menu: a change to a relationship, a note
/// or a nickname, or to the DM itself. Each says how it went.
fn change_person(
    command: DiscordCommand,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) {
    let discord = match discord {
        Ok(discord) => discord.clone(),
        Err(why) => {
            let _ = events.send(match command {
                DiscordCommand::Note { user } => Event::Note {
                    user,
                    result: Err(why.into()),
                },
                DiscordCommand::SetNote { .. } | DiscordCommand::SetNickname { .. } => {
                    Event::PersonSaved(Err(why.into()))
                }
                DiscordCommand::Member { change, .. } if from_dialog(&change) => {
                    Event::PersonSaved(Err(why.into()))
                }
                _ => Event::Error(why.into()),
            });
            return;
        }
    };
    let events = events.clone();
    let channel = |room: &str| {
        discord::parse_room(room)
            .map(|(_, channel)| channel.to_owned())
            .unwrap_or_default()
    };
    tokio::spawn(async move {
        let (result, what) = match command {
            DiscordCommand::Note { user } => {
                let result = discord.note(&user).await.map_err(|error| error.to_string());
                let _ = events.send(Event::Note { user, result });
                return;
            }
            DiscordCommand::SetNote { user, note } => {
                let result = discord.set_note(&user, &note).await;
                let _ = events.send(Event::PersonSaved(
                    result.map_err(|error| error.to_string()),
                ));
                return;
            }
            DiscordCommand::SetNickname { user, nickname } => {
                let result = discord.set_friend_nickname(&user, &nickname).await;
                let _ = events.send(Event::PersonSaved(
                    result.map_err(|error| error.to_string()),
                ));
                return;
            }
            DiscordCommand::Member {
                server,
                user,
                change,
            } => {
                let guild = discord::parse_server(&server).unwrap_or_default();
                let dialog = from_dialog(&change);
                let (result, what) = match change {
                    MemberChange::Nickname(nick) => (
                        discord
                            .stepping_up(|| discord.set_member_nick(guild, &user, &nick))
                            .await,
                        "change the nickname",
                    ),
                    MemberChange::Role { role, add } => (
                        discord
                            .stepping_up(|| discord.member_role(guild, &user, &role, add))
                            .await,
                        if add {
                            "give the role"
                        } else {
                            "take the role away"
                        },
                    ),
                    MemberChange::Timeout { until_ms, reason } => (
                        discord
                            .stepping_up(|| discord.time_out(guild, &user, until_ms, &reason))
                            .await,
                        if until_ms.is_some() {
                            "time them out"
                        } else {
                            "remove the timeout"
                        },
                    ),
                    MemberChange::Kick { reason } => (
                        discord
                            .stepping_up(|| discord.kick(guild, &user, &reason))
                            .await,
                        "kick them",
                    ),
                    MemberChange::Ban {
                        delete_seconds,
                        reason,
                    } => (
                        discord
                            .stepping_up(|| discord.ban(guild, &user, delete_seconds, &reason))
                            .await,
                        "ban them",
                    ),
                    MemberChange::ServerMute(mute) => (
                        discord
                            .stepping_up(|| discord.server_mute(guild, &user, mute))
                            .await,
                        if mute {
                            "server mute them"
                        } else {
                            "unmute them"
                        },
                    ),
                    MemberChange::ServerDeafen(deaf) => (
                        discord
                            .stepping_up(|| discord.server_deafen(guild, &user, deaf))
                            .await,
                        if deaf {
                            "server deafen them"
                        } else {
                            "undeafen them"
                        },
                    ),
                    MemberChange::Move(to) => {
                        let channel = to
                            .as_deref()
                            .and_then(discord::parse_room)
                            .map(|(_, channel)| channel);
                        (
                            discord
                                .stepping_up(|| discord.move_member(guild, &user, channel))
                                .await,
                            if to.is_some() {
                                "move them"
                            } else {
                                "disconnect them"
                            },
                        )
                    }
                };
                if !dialog {
                    (result, what)
                } else {
                    if let Err(error) = &result {
                        tracing::warn!(error = format!("{error:#}"), "could not {what}");
                    }
                    let result = result.map_err(|error| format!("Could not {what}: {error}"));
                    let _ = events.send(Event::PersonSaved(result));
                    return;
                }
            }
            DiscordCommand::Relationship { user, change } => match change {
                RelationshipChange::Unfriend => {
                    (discord.unfriend(&user).await, "remove the friend")
                }
                RelationshipChange::Block => (discord.block(&user, true).await, "block them"),
                RelationshipChange::Unblock => (discord.block(&user, false).await, "unblock them"),
                RelationshipChange::Ignore => (discord.ignore(&user, true).await, "ignore them"),
                RelationshipChange::Unignore => {
                    (discord.ignore(&user, false).await, "stop ignoring them")
                }
            },
            DiscordCommand::CloseDm { room } => {
                (discord.close_dm(&channel(&room)).await, "close the DM")
            }
            DiscordCommand::Notifications { place, change } => {
                let (guild, channel) = match discord::parse_room(&place) {
                    Some((guild, channel)) => (guild, Some(channel)),
                    None => (discord::parse_server(&place).unwrap_or_default(), None),
                };
                let result = discord
                    .change_notifications(guild, channel, notification_change(change))
                    .await;
                (result, "change the notification settings")
            }
            DiscordCommand::PinChannel { room, flags } => {
                let result = match discord::parse_room(&room) {
                    Some((guild, channel)) => discord.channel_flags(guild, channel, flags).await,
                    None => Err(anyhow::anyhow!(NOT_DISCORD)),
                };
                let pinned = flags & yapfast_discord::notifications::PINNED != 0;
                (
                    result,
                    if pinned {
                        "pin the channel"
                    } else {
                        "unpin the channel"
                    },
                )
            }
            DiscordCommand::MoveChannels {
                server,
                placements,
                lock,
            } => {
                let id = |room: &str| {
                    discord::parse_room(room).map_or_else(String::new, |(_, id)| id.to_owned())
                };
                let placements: Vec<_> = placements
                    .iter()
                    .map(|placement| yapfast_discord::ready::Placement {
                        id: id(&placement.room),
                        position: placement.position,
                        parent: placement
                            .parent
                            .as_ref()
                            .map(|parent| parent.as_deref().map(id)),
                    })
                    .collect();
                let lock = lock.as_deref().map(id);
                let result = match discord::parse_server(&server) {
                    Some(guild) => {
                        discord
                            .stepping_up(|| {
                                discord.move_channels(guild, placements.clone(), lock.as_deref())
                            })
                            .await
                    }
                    None => Err(anyhow::anyhow!(NOT_DISCORD)),
                };
                (result, "move the channel")
            }
            DiscordCommand::FollowChannel { room, flags } => {
                let result = match discord::parse_room(&room) {
                    Some((guild, channel)) => discord.channel_flags(guild, channel, flags).await,
                    None => Err(anyhow::anyhow!(NOT_DISCORD)),
                };
                let followed = yapfast_discord::notifications::Override {
                    flags,
                    ..Default::default()
                }
                .followed();
                (
                    result,
                    if followed {
                        "add it to the channel list"
                    } else {
                        "remove it from the channel list"
                    },
                )
            }
            DiscordCommand::ShowAllChannels {
                server,
                flags,
                follow,
            } => {
                let result = match discord::parse_server(&server) {
                    Some(guild) => discord.show_all_channels(guild, flags, &follow).await,
                    None => Err(anyhow::anyhow!(NOT_DISCORD)),
                };
                (result, "change which channels show")
            }
            DiscordCommand::JoinThread {
                room,
                join,
                location,
            } => (
                discord.join_thread(&channel(&room), join, location).await,
                if join {
                    "join the thread"
                } else {
                    "leave the thread"
                },
            ),
            DiscordCommand::Mute { room, muting } => {
                let (muted, seconds) = mute(muting);
                let result = discord.mute_private(&channel(&room), muted, seconds).await;
                (
                    result,
                    if muted {
                        "mute the DM"
                    } else {
                        "unmute the DM"
                    },
                )
            }
            _ => return,
        };
        if let Err(error) = result {
            tracing::warn!(error = format!("{error:#}"), "could not {what}");
            let _ = events.send(Event::Error(format!("Could not {what}: {error}")));
        }
    });
}

/// Forum posts' first messages from Discord, by room id, as their cards
/// show them.
fn post_starters(
    server: &yapfast_discord::ready::Server,
    names: &yapfast_discord::message::Names,
    starters: &yapfast_discord::service::Starters,
) -> crate::chat::PostStarters {
    starters
        .iter()
        .map(|(post, starter)| {
            (
                discord::room_id(&server.id, post),
                starter
                    .as_ref()
                    .map(|starter| discord::post_starter(server, names, starter)),
            )
        })
        .collect()
}

/// The places used as [`Event::PlacesUsed`] carries them.
pub(super) fn places_used(
    places: Vec<(String, yapfast_discord::frecency::Uses)>,
) -> Vec<(String, crate::chat::PlaceUses)> {
    places
        .into_iter()
        .map(|(id, uses)| {
            let uses = crate::chat::PlaceUses {
                total: uses.total,
                recent: uses.recent,
            };
            (id, uses)
        })
        .collect()
}

/// What an invite shows, as the application keeps it.
fn invite_preview(preview: &yapfast_discord::invite::Preview) -> InvitePreview {
    InvitePreview {
        code: preview.code.clone(),
        server: discord::server_id(&preview.guild),
        name: preview.name.clone(),
        icon: preview.icon.clone(),
        room: discord::room_id(&preview.guild, &preview.channel),
        channel: preview.channel_name.clone(),
        channel_type: preview.channel_type,
        voice: preview.voice(),
        members: preview.members,
        online: preview.online,
    }
}

fn gift_preview(preview: &yapfast_discord::gift::Preview) -> GiftPreview {
    GiftPreview {
        code: preview.code.clone(),
        name: preview.name.clone(),
        plan: preview.plan.clone(),
        gifter: preview.gifter.clone(),
        redeemed: preview.redeemed,
        spent: preview.spent,
        expires_at: preview.expires_at.clone(),
    }
}
