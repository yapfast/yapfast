//! Commands for Discord, the work they started that finished, and live
//! events, carried out on the channels.

use super::{
    render::{chat_member, mention_groups, notification, publish, send_topic, sent_id},
    *,
};
use crate::chat::ForwardFailure;
use yapfast_discord::server_settings::GuildSettings as DiscordSettings;

/// Why a Discord command fails for a room that isn't Discord's.
pub(super) const NOT_DISCORD: &str = "That isn't a Discord room.";

/// Carries out a command [`Command::route`] sends to Discord. Opening any
/// room closes the open Discord channel. Without Discord, the error says
/// why, and Discord commands fail with it.
pub fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    channels: &mut Channels,
    done: &UnboundedSender<Done>,
    events: &UnboundedSender<Event>,
) {
    if matches!(command, Command::Open(_) | Command::CloseRoom) {
        channels.close();
    }
    if let Command::Aside(room) = command {
        aside(room, discord, channels, done, events);
        return;
    }
    if let Command::SendPoll { room, poll } = command {
        send_poll(discord, room, poll, events);
        return;
    }
    if let Command::MarkRead { room, sync } = &command {
        if let (Ok(discord), Some((guild, channel))) = (discord, discord::parse_room(room)) {
            discord.mark_read(guild, channel, *sync);
        }
        return;
    }
    if let Command::Typing { room, typing } = &command
        && Network::of(room) == Network::Discord
    {
        let now = Instant::now();
        let again = channels
            .typed
            .as_ref()
            .is_none_or(|(typed, at)| typed != room || now.duration_since(*at) >= TYPING_AGAIN);
        if !typing {
            channels.typed = None;
        } else if again
            && let (Ok(discord), Some((_, channel))) = (discord, discord::parse_room(room))
        {
            channels.typed = Some((room.clone(), now));
            let (discord, channel) = (discord.clone(), channel.to_owned());
            tokio::spawn(async move {
                if let Err(error) = discord.typing(&channel).await {
                    tracing::debug!(%error, "could not say this account is typing");
                }
            });
        }
        return;
    }
    let Some(command) = account::command(
        command,
        discord,
        &channels.searches,
        &channels.previews,
        events,
    ) else {
        return;
    };
    let Some(command) = own_profile::command(command, discord, events) else {
        return;
    };
    let Some(command) = gifs::command(command, discord, events) else {
        return;
    };
    let Some(command) = super::server_settings::command(command, discord, events) else {
        return;
    };
    let Some(command) = super::roles::command(command, discord, &channels.searches, events) else {
        return;
    };
    let Some(command) = super::overwrites::command(command, discord, events) else {
        return;
    };
    let Some(command) = super::audit_log::command(command, discord, events) else {
        return;
    };
    let Some(command) = super::people::command(command, discord, events) else {
        return;
    };
    let Some(command) = super::expressions::command(command, discord, events) else {
        return;
    };
    let Some(command) = super::ownership::command(command, discord, events) else {
        return;
    };
    let room = match &command {
        Command::Open(room)
        | Command::Send { room, .. }
        | Command::Edit { room, .. }
        | Command::React { room, .. }
        | Command::Reactors { room, .. }
        | Command::Vote { room, .. }
        | Command::EndPoll { room, .. }
        | Command::Voters { room, .. }
        | Command::RemoveReactor { room, .. }
        | Command::Delete { room, .. }
        | Command::Pin { room, .. }
        | Command::Retry { room, .. }
        | Command::Cancel { room, .. }
        | Command::SaveAttachment { room, .. }
        | Command::CopyAttachment { room, .. }
        | Command::ViewImage { room, .. }
        | Command::VisibleMessages { room, .. }
        | Command::Earlier(room)
        | Command::Later(room)
        | Command::Around { room, .. }
        | Command::Present(room) => room.clone(),
        Command::Forward {
            room,
            item,
            target,
            note,
            silent,
        } => {
            forward(discord, room, item, target, note, *silent, events);
            return;
        }
        _ => return,
    };
    // Opening another network's room only closes the channel here.
    if Network::of(&room) != Network::Discord {
        return;
    }
    let discord = match discord {
        Ok(discord) => discord,
        Err(why) => {
            let _ = events.send(Event::RoomError {
                room: room.clone(),
                message: why.into(),
            });
            // History asked for says it failed, so nothing waits on it.
            if let Some(direction) = direction(&command) {
                let _ = events.send(Event::HistoryLoaded {
                    room,
                    direction,
                    history: History::Failed,
                });
            }
            return;
        }
    };
    if let Command::Open(_) = command {
        let _ = events.send(Event::MentionGroups {
            room: room.clone(),
            groups: mention_groups(discord, &room),
        });
        // The member list follows as Live::Members, unless Discord sends none.
        let (late, waiting) = (room.clone(), done.clone());
        tokio::spawn(async move {
            tokio::time::sleep(MEMBERS_WAIT).await;
            let _ = waiting.send(Done::MembersLate { room: late });
        });
        // Shown at once as it was kept. Its latest page loads again only
        // when it may have missed live events since it closed.
        if let Some(mut channel) = channels.reopen(&room) {
            discord.view(&channel.guild, Some(&channel.channel));
            if !channel.synced {
                channel.refresh(discord, done);
            }
            if channel.loaded {
                publish(&mut channel, discord, done, events);
            }
            if channel.synced {
                let _ = events.send(Event::HistoryLoaded {
                    room: room.clone(),
                    direction: Direction::Latest,
                    history: channel.history(),
                });
            }
            send_topic(&mut channel, discord, events, true);
            channels.open = Some(channel);
            return;
        }
        match OpenChannel::open(discord, &room, false, done, channels.previews.clone()) {
            // The member list follows as Live::Members.
            Some(mut channel) => {
                // Shown at once as it was kept on disk, while the latest page loads.
                if channel.loaded {
                    publish(&mut channel, discord, done, events);
                }
                send_topic(&mut channel, discord, events, true);
                channels.open = Some(channel);
            }
            None => {
                let _ = events.send(Event::RoomError {
                    room,
                    message: "Could not open this Discord channel.".into(),
                });
            }
        }
        return;
    }
    // Discord ends the typing a message follows.
    if matches!(command, Command::Send { .. }) {
        channels.typed = None;
    }
    let Some(channel) = channels.shown_mut(&room) else {
        let again = "Open this conversation again, then retry.";
        let _ = match command {
            Command::Send { room, body, .. } => events.send(Event::SendResult {
                room,
                body,
                result: Err(again),
            }),
            Command::Edit { room, item, .. } => events.send(Event::EditResult {
                room,
                item,
                result: Err(again.into()),
            }),
            command => match direction(&command) {
                Some(direction) => events.send(Event::HistoryLoaded {
                    room,
                    direction,
                    history: History::Failed,
                }),
                None => Ok(()),
            },
        };
        return;
    };
    match command {
        Command::VisibleMessages { items, at_end, .. } => {
            let moved = channel.visible(items);
            if (at_end && channel.let_go_above_view()) || moved {
                publish(channel, discord, done, events);
            }
        }
        Command::Send {
            room,
            body,
            reply,
            options,
            files,
        } => {
            let result = channel.send(discord, done, body.clone(), reply, options, files);
            publish(channel, discord, done, events);
            let _ = events.send(Event::SendResult { room, body, result });
        }
        Command::Edit { room, item, body } => {
            if let Err(error) = channel.edit(discord, done, item.clone(), body) {
                let _ = events.send(Event::EditResult {
                    room,
                    item,
                    result: Err(error.into()),
                });
            }
        }
        Command::React { item, key, .. } => channel.react(discord, done, &item, &key),
        Command::Reactors {
            item,
            key,
            after,
            limit,
            ..
        } => channel.reactors(discord, events, item, key, after, limit),
        Command::Vote { item, answers, .. } => channel.vote(discord, done, &item, answers),
        Command::EndPoll { item, .. } => channel.end_poll(discord, done, &item),
        Command::Voters {
            item,
            answer,
            after,
            limit,
            ..
        } => channel.voters(discord, events, item, answer, after, limit),
        Command::RemoveReactor {
            item, key, user, ..
        } => channel.remove_reactor(discord, done, &item, &key, user),
        Command::Delete { item, .. } => channel.delete(discord, done, &item),
        Command::Pin { item, pinned, .. } => channel.pin(discord, done, &item, pinned),
        Command::Retry { item, .. } => {
            channel.retry(discord, done, &item);
            publish(channel, discord, done, events);
        }
        Command::Cancel { room, item } => {
            if channel.cancel(&item) {
                publish(channel, discord, done, events);
            } else {
                let _ = events.send(Event::RoomError {
                    room,
                    message: "That message is still sending, so it can't be cancelled.".into(),
                });
            }
        }
        Command::SaveAttachment {
            item, index, path, ..
        } => channel.save(discord, done, &item, index, path),
        Command::CopyAttachment { item, index, .. } => channel.copy(discord, done, &item, index),
        Command::ViewImage { item, index, .. } => channel.view_image(discord, done, item, index),
        Command::Earlier(room) if !channel.earlier(discord, done) => {
            let _ = events.send(Event::HistoryLoaded {
                room,
                direction: Direction::Earlier,
                history: channel.history(),
            });
        }
        // No window shows, or it reached the newest messages kept, which
        // show with it now.
        Command::Later(room) if !channel.later(discord, done) => {
            publish(channel, discord, done, events);
            let _ = events.send(Event::HistoryLoaded {
                room,
                direction: Direction::Later,
                history: channel.history(),
            });
        }
        Command::Around { room, item } => match discord::item_order(&item) {
            Some(order) => {
                if !channel.around(discord, done, order) {
                    let _ = events.send(Event::HistoryLoaded {
                        room,
                        direction: Direction::Around,
                        history: channel.history(),
                    });
                    publish(channel, discord, done, events);
                }
            }
            None => {
                let _ = events.send(Event::RoomError {
                    room: room.clone(),
                    message: "Yapfast couldn't find that message.".into(),
                });
                let _ = events.send(Event::HistoryLoaded {
                    room,
                    direction: Direction::Around,
                    history: History::Failed,
                });
                publish(channel, discord, done, events);
            }
        },
        Command::Present(_) => {
            channel.present();
            if channel.loaded {
                publish(channel, discord, done, events);
            }
        }
        _ => {}
    }
}

/// Shows a thread beside the open channel, in place of any shown before,
/// or with none, stops.
fn aside(
    room: Option<String>,
    discord: Result<&Discord, &str>,
    channels: &mut Channels,
    done: &UnboundedSender<Done>,
    events: &UnboundedSender<Event>,
) {
    let shown = channels.aside.as_ref();
    if room.is_some() && room.as_ref() == shown.map(|shown| &shown.room) {
        return;
    }
    if let (Some(shown), Ok(discord)) = (shown, discord) {
        discord.beside(&shown.guild, None);
    }
    channels.close_aside();
    let (Some(room), Ok(discord)) = (room, discord) else {
        return;
    };
    // Shown at once as it was kept, while the latest page loads, unless
    // it kept up with live events since it closed.
    let channel = match channels.reopen(&room) {
        Some(mut channel) => {
            discord.beside(&channel.guild, Some(&channel.channel));
            if !channel.synced {
                channel.refresh(discord, done);
            }
            Some(channel)
        }
        None => OpenChannel::open(discord, &room, true, done, channels.previews.clone()),
    };
    let Some(mut channel) = channel else {
        let _ = events.send(Event::RoomError {
            room,
            message: "Could not open this Discord thread.".into(),
        });
        return;
    };
    if channel.loaded {
        publish(&mut channel, discord, done, events);
    }
    channels.aside = Some(channel);
}

/// Which history a command asks for, if any.
fn direction(command: &Command) -> Option<Direction> {
    match command {
        Command::Earlier(_) => Some(Direction::Earlier),
        Command::Later(_) => Some(Direction::Later),
        Command::Around { .. } => Some(Direction::Around),
        _ => None,
    }
}

/// Applies background work that finished.
pub fn finished(
    finished: Done,
    discord: Option<&Discord>,
    channels: &mut Channels,
    done: &UnboundedSender<Done>,
    events: &UnboundedSender<Event>,
) {
    let room = match &finished {
        Done::History { room, .. }
        | Done::Sent { room, .. }
        | Done::Edited { room, .. }
        | Done::Deleted { room, .. }
        | Done::Pinned { room, .. }
        | Done::Picture { room, .. }
        | Done::Image { room, .. }
        | Done::Saved { room, .. }
        | Done::Copied { room, .. }
        | Done::Failed { room, .. } => room.clone(),
        Done::Typing { room } => {
            channels.typing_changed(room, events);
            return;
        }
        // The application keeps a list that came; otherwise this says why there's none.
        Done::MembersLate { room } => {
            if channels.is_open(room) {
                let _ = events.send(Event::Members {
                    room: room.clone(),
                    members: Err("Discord didn't send this channel's member list."),
                });
            }
            return;
        }
    };
    // The channel on screen it's for: the open one or the thread beside it.
    let shown = channels.is_open(&room).then(|| room.clone());
    // Closed channels kept lately take what finished too.
    let channel = channels.get_mut(&room);
    let mut changed = false;
    let mut history_loaded = None;
    match (finished, channel) {
        (Done::Edited { item, result, .. }, channel) => {
            let result = match result {
                Ok(message) => {
                    if let Some(channel) = channel {
                        channel.edited(message);
                        changed = true;
                    }
                    Ok(())
                }
                Err(error) => Err(error),
            };
            let _ = events.send(Event::EditResult { room, item, result });
        }
        (Done::Saved { name, result, .. }, _) => {
            let _ = events.send(Event::Saved { room, name, result });
        }
        (Done::Copied { picture, .. }, _) => {
            let _ = events.send(Event::Copied(picture));
        }
        (
            Done::Image {
                item,
                index,
                picture,
                ..
            },
            _,
        ) => {
            let _ = events.send(Event::Image {
                room,
                item,
                index,
                picture,
            });
        }
        (
            Done::History {
                load,
                generation,
                result,
                ..
            },
            channel,
        ) => {
            let mut direction = match load {
                Load::Latest => Direction::Latest,
                Load::Earlier => Direction::Earlier,
                Load::Later => Direction::Later,
                Load::Around(_) => Direction::Around,
            };
            let history = match (result, channel) {
                // Asked for before what shows was replaced; it's dropped.
                (_, Some(channel)) if load != Load::Latest && generation != channel.generation => {
                    None
                }
                (Ok(page), Some(channel)) => {
                    let detached = channel.detached();
                    channel.loaded(load, page);
                    changed = true;
                    // The latest page reached the window, which shows with
                    // the newest messages now, as when a later page does.
                    if detached && !channel.detached() && load == Load::Latest {
                        direction = Direction::Later;
                    }
                    Some(channel.history())
                }
                (Ok(_), None) => Some(History::Failed),
                (Err(error), channel) => {
                    if let Some(channel) = channel {
                        match load {
                            Load::Latest => {}
                            Load::Earlier => channel.loading_earlier = false,
                            Load::Later => channel.loading_later = false,
                            // What showed shows on.
                            Load::Around(_) => {
                                channel.focusing = false;
                                changed = true;
                            }
                        }
                    }
                    let _ = events.send(Event::RoomError {
                        room: room.clone(),
                        message: error,
                    });
                    Some(History::Failed)
                }
            };
            history_loaded = history.map(|history| (room, direction, history));
        }
        (_, None) => {}
        (Done::Sent { nonce, result, .. }, Some(channel)) => {
            if let Some(error) = channel.sent(&nonce, result) {
                let _ = events.send(Event::RoomError {
                    room,
                    message: error,
                });
            }
            changed = true;
        }
        (Done::Deleted { id, result, .. }, Some(channel)) => match result {
            Ok(()) => {
                channel.deleted(&id);
                changed = true;
            }
            Err(error) => {
                let _ = events.send(Event::RoomError {
                    room,
                    message: error,
                });
            }
        },
        (
            Done::Pinned {
                id, pinned, result, ..
            },
            Some(channel),
        ) => match result {
            Ok(()) => {
                channel.pinned(&id, pinned);
                changed = true;
            }
            Err(error) => {
                let _ = events.send(Event::RoomError {
                    room,
                    message: error,
                });
            }
        },
        (Done::Picture { url, picture, .. }, Some(channel)) => {
            channel.picture(url, picture);
            changed = true;
        }
        (Done::Failed { message, .. }, Some(_)) => {
            let _ = events.send(Event::RoomError { room, message });
        }
        // Handled above.
        (Done::Typing { .. } | Done::MembersLate { .. }, Some(_)) => {}
    }
    // A page is said to have come once it's published, so the timeline has
    // it, but a window before it: until then the timeline stays as it was.
    let say = |(room, direction, history)| {
        let _ = events.send(Event::HistoryLoaded {
            room,
            direction,
            history,
        });
    };
    let (window, page) = match history_loaded {
        Some(loaded @ (_, Direction::Around, _)) => (Some(loaded), None),
        loaded => (None, loaded),
    };
    window.map(say);
    if changed
        && let Some(shown) = shown
        && let (Some(discord), Some(channel)) = (discord, channels.shown_mut(&shown))
    {
        publish(channel, discord, done, events);
    }
    page.map(say);
}

/// Applies a gateway event to the open channel and those kept. Missed
/// events reload the open one; the others load again when next opened.
pub fn live(
    event: Result<Live, RecvError>,
    discord: &Discord,
    channels: &mut Channels,
    done: &UnboundedSender<Done>,
    events: &UnboundedSender<Event>,
) {
    if let Ok(Live::FavoriteGifs(favorites)) = &event {
        let _ = events.send(Event::FavoriteGifs(Ok(gifs::favorites(favorites))));
        tokio::spawn(yapfast_discord::service::keep_favorite_gifs(
            favorites.clone(),
        ));
        return;
    }
    if let Ok(Live::MfaRequired { id, methods }) = &event {
        let methods = methods
            .iter()
            .map(|&method| super::server_settings::method(method))
            .collect();
        let _ = events.send(Event::MfaRequired { id: *id, methods });
        return;
    }
    if let Ok(Live::ServerSettings { guild, settings }) = &event {
        let _ = events.send(Event::ServerSettingsChanged {
            server: discord::server_id(guild),
            settings: Box::new(super::server_settings::guild_settings(
                DiscordSettings::clone(settings),
            )),
        });
        return;
    }
    if let Ok(Live::ServerRole { guild, role }) = &event {
        let _ = events.send(Event::ServerRoleChanged {
            server: discord::server_id(guild),
            role: Box::new(super::roles::detail((**role).clone(), 0)),
        });
        return;
    }
    if let Ok(Live::ServerBan { guild, ban, banned }) = &event {
        let _ = events.send(Event::ServerBan {
            server: discord::server_id(guild),
            ban: Box::new(super::server_settings::ban((**ban).clone())),
            banned: *banned,
        });
        return;
    }
    if let Ok(Live::ServerStickers { guild, stickers }) = &event {
        let _ = events.send(Event::ServerStickers {
            server: discord::server_id(guild),
            stickers: stickers
                .iter()
                .cloned()
                .map(super::expressions::server_sticker)
                .collect(),
        });
        return;
    }
    if let Ok(Live::ServerRoleDeleted { guild, role }) = &event {
        let _ = events.send(Event::ServerRoleDeleted {
            server: discord::server_id(guild),
            role: role.clone(),
        });
        return;
    }
    if let Ok(Live::PlacesUsed(places)) = &event {
        let places = account::places_used(places.clone());
        let _ = events.send(Event::PlacesUsed(Ok(places)));
        return;
    }
    if let Ok(Live::Notify { guild, message }) = &event {
        let _ = events.send(Event::Notify(notification(
            discord,
            guild.as_deref(),
            message,
        )));
        return;
    }
    if let Ok(Live::Typing {
        guild,
        channel,
        user,
        name,
    }) = &event
    {
        // Nobody blocked is seen typing.
        if discord.snapshots().borrow().blocked.contains(user) {
            return;
        }
        let room = discord::room_id(
            guild.as_deref().unwrap_or(yapfast_discord::ready::PRIVATE),
            channel,
        );
        channels
            .typing
            .entry(room.clone())
            .or_default()
            .insert(user.clone(), (name.clone(), Instant::now() + TYPING));
        channels.typing_changed(&room, events);
        let done = done.clone();
        tokio::spawn(async move {
            tokio::time::sleep(TYPING).await;
            let _ = done.send(Done::Typing { room });
        });
        return;
    }
    // A message ends its author's typing.
    if let Ok(Live::Created(message)) = &event {
        let room = channels
            .typing
            .keys()
            .find(|room| {
                discord::parse_room(room).is_some_and(|(_, channel)| channel == message.channel_id)
            })
            .cloned();
        if let Some(room) = room
            && let Some(typing) = channels.typing.get_mut(&room)
            && typing.remove(&message.author_id).is_some()
        {
            channels.typing_changed(&room, events);
        }
    }
    match &event {
        Ok(event) => channels.follow(event),
        // Behind on live events, the kept channels load again when opened.
        Err(_) => channels.missed(),
    }
    // The thread beside the open channel follows as it does.
    if let Some(aside) = channels.aside.as_mut() {
        match &event {
            Ok(Live::Members { .. }) => {}
            Ok(event) => {
                if aside.live(event) {
                    publish(aside, discord, done, events);
                }
            }
            Err(_) => aside.refresh(discord, done),
        }
    }
    let Some(channel) = channels.open.as_mut() else {
        return;
    };
    match event {
        Ok(Live::Members {
            channel: id,
            members,
        }) => {
            if id == channel.channel || id == channel.members_of {
                let _ = events.send(Event::Members {
                    room: channel.room.clone(),
                    members: Ok(members.into_iter().map(chat_member).collect()),
                });
            }
        }
        Ok(event) => {
            if channel.live(&event) {
                publish(channel, discord, done, events);
            }
        }
        Err(_) => channel.refresh(discord, done),
    }
}

/// Forwards a Discord message to another Discord conversation, as Discord's
/// client does, then sends `note` there. Discord turns down a forward
/// between networks.
/// Posts a poll in a Discord conversation, as [`Event::PollPosted`] says.
fn send_poll(
    discord: Result<&Discord, &str>,
    room: String,
    poll: crate::chat::NewPoll,
    events: &UnboundedSender<Event>,
) {
    let place =
        discord::parse_room(&room).map(|(guild, channel)| [guild, channel].map(str::to_owned));
    let (discord, Some([guild, channel])) = (discord, place) else {
        let _ = events.send(Event::PollPosted {
            room,
            result: Err(NOT_DISCORD.into()),
        });
        return;
    };
    let discord = match discord {
        Ok(discord) => discord.clone(),
        Err(why) => {
            let _ = events.send(Event::PollPosted {
                room,
                result: Err(why.into()),
            });
            return;
        }
    };
    let poll = yapfast_discord::api::NewPoll {
        question: poll.question,
        answers: poll
            .answers
            .into_iter()
            .map(|answer| yapfast_discord::api::NewAnswer {
                text: answer.text,
                emoji: answer.emoji,
            })
            .collect(),
        hours: poll.hours,
        multiselect: poll.multiselect,
    };
    let events = events.clone();
    tokio::spawn(async move {
        let result = discord
            .send_poll(&guild, &channel, &poll)
            .await
            .map_err(|error| format!("Couldn't post the poll: {error}"));
        let _ = events.send(Event::PollPosted { room, result });
    });
}

fn forward(
    discord: Result<&Discord, &str>,
    room: &str,
    item: &str,
    target: &str,
    note: &Option<String>,
    silent: bool,
    events: &UnboundedSender<Event>,
) {
    let fail = |why: String| {
        let _ = events.send(Event::Forwarded {
            room: target.to_owned(),
            result: Err(ForwardFailure::Forward(why)),
        });
    };
    let discord = match discord {
        Ok(discord) => discord,
        Err(why) => return fail(why.into()),
    };
    let (Some((guild, channel)), Some((target_guild, target_channel))) =
        (discord::parse_room(room), discord::parse_room(target))
    else {
        return fail("Forwarding doesn't cross between Discord and Matrix.".into());
    };
    let Some(id) = sent_id(item) else {
        return fail("You can forward that message once it has been sent.".into());
    };
    // Checked first, so a message too long to add isn't forwarded without it.
    if note
        .as_ref()
        .is_some_and(|note| note.chars().count() > MAX_CHARS)
    {
        return fail("The message you added is over 2,000 characters.".into());
    }
    let (discord, events) = (discord.clone(), events.clone());
    let (source, place) = (
        [guild, channel, id].map(str::to_owned),
        [target_guild, target_channel].map(str::to_owned),
    );
    let (room, note) = (target.to_owned(), note.clone());
    tokio::spawn(async move {
        let [guild, channel, id] = &source;
        let [target_guild, target] = &place;
        let mut result = discord
            .forward((guild, channel, id), (target_guild, target), silent)
            .await
            .map_err(|error| {
                tracing::warn!(error = format!("{error:#}"), "could not forward a message");
                ForwardFailure::Forward(error.to_string())
            });
        if let (Ok(()), Some(note)) = (&result, note) {
            result = discord
                .send_forward_note((target_guild, target), &note, silent)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "could not send a forward's note"
                    );
                    ForwardFailure::Note(error.to_string())
                });
        }
        let _ = events.send(Event::Forwarded { room, result });
    });
}
