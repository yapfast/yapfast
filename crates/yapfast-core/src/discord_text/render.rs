//! Discord's messages, profiles, embeds and members as Yapfast's chat shows
//! them.

use super::{channels::SharedPreviews, *};
use yapfast_discord::message::MentionTarget;
use yapfast_discord::ready::Access;

/// A new Discord message as a notification: who wrote it, by their name in the server,
/// and what it says, or what it carries when it has no text.
pub(super) fn notification(
    discord: &Discord,
    guild: Option<&str>,
    message: &Message,
) -> Notification {
    let snapshot = discord.snapshots().borrow().clone();
    let server = guild.and_then(|guild| snapshot.servers.iter().find(|server| server.id == guild));
    let author = discord
        .name(
            guild.unwrap_or(yapfast_discord::ready::PRIVATE),
            &message.author_id,
        )
        .unwrap_or_else(|| message.author.clone());
    // A message of components says what its first text does.
    let text = match message.content.as_str() {
        "" => message
            .components
            .iter()
            .flat_map(yapfast_discord::components::Card::texts)
            .next()
            .unwrap_or_default(),
        content => content,
    };
    let mut body = crate::chat::notification_text(&crate::spoiler::hide(
        &discord.render(server.map(|server| &*server.server), text),
    ));
    if body.is_empty() {
        body = if message
            .attachments
            .iter()
            .any(|attachment| attachment.shown == Shown::Sticker)
        {
            "Sent a sticker".into()
        } else if message
            .attachments
            .iter()
            .any(|attachment| attachment.image)
        {
            "Sent a picture".into()
        } else if !message.attachments.is_empty() {
            "Sent a file".into()
        } else if let Some(poll) = &message.poll {
            // As Discord's client says it.
            format!("[poll] {}", poll.question)
        } else if let Some(title) = message.embeds.iter().find_map(|embed| embed.title.clone()) {
            title
        } else {
            "Sent a message".into()
        };
    }
    Notification {
        room: crate::discord::room_id(
            guild.unwrap_or(yapfast_discord::ready::PRIVATE),
            &message.channel_id,
        ),
        author,
        body,
        avatar: Some(discord.avatar(guild, &message.author_id)),
        mention: message.mentions_me(
            snapshot.user_id.as_deref().unwrap_or_default(),
            server.map(|server| &*server.marks),
        ),
    }
}

/// A message as a pinned message or a search result shows it: who wrote
/// it, by their name, avatar and role colour in `server`, or for a DM with
/// none, their own; when; and what it says, and the files it carries.
/// [`with_previews`] brings its pictures.
pub(super) fn found(
    discord: &Discord,
    server: Option<&yapfast_discord::ready::Server>,
    message: &Message,
) -> ChatMessage {
    let guild = server.map(|server| server.id.as_str());
    let author = discord
        .name(
            guild.unwrap_or(yapfast_discord::ready::PRIVATE),
            &message.author_id,
        )
        .unwrap_or_else(|| message.author.clone());
    let ordinary = message.system.is_none();
    let timestamp = message.timestamp();
    let (body, mentions) = rendered(discord, server, &message.content);
    let body = match &message.system {
        Some(system) => system_body(&author, system),
        None => body,
    };
    ChatMessage {
        id: format!("d:{}", message.id),
        item: format!("d:{}", message.id),
        sender: discord::sender_id(&message.author_id),
        spoilers: true,
        avatar: Some(discord.avatar(guild, &message.author_id)),
        decoration: discord.decoration(guild, &message.author_id),
        style: discord
            .style(guild, &message.author_id)
            .map(|style| Arc::new(style.into())),
        color: server.and_then(|server| discord.color(server, &message.author_id)),
        bot: message.bot,
        body,
        author,
        formatted: ordinary,
        time: crate::clock::time(timestamp),
        timestamp,
        edited: message.edited,
        attachments: message
            .attachments
            .iter()
            .filter(|attachment| attachment.shown == Shown::Message)
            .map(|attachment| Attachment {
                name: attachment.name.clone(),
                size: attachment.size,
                image: attachment.image,
                dimensions: attachment.dimensions,
                preview: None,
                preview_failed: false,
                placement: Placement::Message,
                spoiler: attachment.spoiler,
                video: attachment.video_url.is_some(),
                gif: super::gifs::favorite_gif(attachment),
                url: Some(
                    attachment
                        .link_url
                        .clone()
                        .unwrap_or_else(|| attachment.url.clone()),
                ),
            })
            .collect(),
        mentions,
        emoji: custom_emoji(message),
        // Shown, not voted in: its author can't end it from here either.
        poll: message.poll.as_ref().map(|poll| chat_poll(poll, false)),
        notice: chat_notice(message),
        ..Default::default()
    }
}

/// A Discord message's notice, as the timeline marks it.
pub(super) fn chat_notice(message: &Message) -> Option<crate::chat::Notice> {
    use crate::chat::NoticeKind;
    use yapfast_discord::message::Notice;
    let kind = match message.notice.as_ref()? {
        Notice::Pinned(id) => NoticeKind::Pinned(id.as_ref().map(|id| format!("d:{id}"))),
        Notice::Joined => NoticeKind::Joined,
        Notice::ThreadStarted => NoticeKind::ThreadStarted,
        Notice::Call => NoticeKind::Call,
        Notice::PollResult => NoticeKind::PollResult,
    };
    Some(crate::chat::Notice {
        kind,
        text: message.system.clone()?,
    })
}

/// What a notice, such as a pin or a call, says: its author's name, then
/// what they did. A closed poll's says "Ann's poll".
pub(super) fn system_body(author: &str, system: &str) -> String {
    if system.starts_with('\'') {
        format!("{author}{system}")
    } else {
        format!("{author} {system}")
    }
}

/// A Discord poll as a message shows it; `own` is this account's, which
/// may end it.
pub(super) fn chat_poll(poll: &yapfast_discord::message::Poll, own: bool) -> Poll {
    Poll {
        question: poll.question.clone(),
        answers: poll
            .answers
            .iter()
            .map(|answer| PollAnswer {
                id: answer.id,
                text: answer.text.clone(),
                emoji: answer.emoji.as_ref().map(|(key, _)| key.clone()),
                votes: answer.votes,
                mine: answer.me,
            })
            .collect(),
        expiry: poll.expiry.map(|expiry| expiry / 1000),
        finalized: poll.finalized,
        multiselect: poll.multiselect,
        can_end: own && !poll.finalized,
    }
}

/// Gives `found`, as [`found`] made them from `messages`, the previews of
/// their first [`crate::chat::FOUND_PICTURES`] pictures each, as the timeline shows
/// them: from the previews the recent channels share, or downloaded a few
/// at a time. One that can't be had is marked so.
pub(super) async fn with_previews(
    discord: &Discord,
    previews: &SharedPreviews,
    messages: &[Message],
    found: &mut [ChatMessage],
) {
    use futures_util::StreamExt;
    let mut wanted = Vec::new();
    for (at, (message, found)) in messages.iter().zip(&*found).enumerate() {
        let shown = message
            .attachments
            .iter()
            .filter(|attachment| attachment.shown == Shown::Message);
        wanted.extend(
            shown
                .zip(&found.attachments)
                .enumerate()
                .filter(|(_, (_, attachment))| attachment.image || attachment.video)
                .filter_map(|(index, (attachment, _))| {
                    Some((at, index, attachment.preview_url.clone()?))
                })
                .take(crate::chat::FOUND_PICTURES),
        );
    }
    let cached = |url: &str| {
        previews
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(url)
    };
    let came: Vec<_> = futures_util::stream::iter(wanted)
        .map(|(at, index, url)| async move {
            if let Some(picture) = cached(&url) {
                return (at, index, Some(picture));
            }
            let picture = preview(discord, &url).await;
            if let Some(picture) = &picture {
                previews
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(url, picture.clone());
            }
            (at, index, picture)
        })
        .buffer_unordered(4)
        .collect()
        .await;
    for (at, index, picture) in came {
        let attachment = &mut found[at].attachments[index];
        attachment.preview_failed = picture.is_none();
        attachment.preview = picture;
    }
}

/// A picture downloaded and decoded at the timeline's preview size.
pub(super) async fn preview(discord: &Discord, url: &str) -> Option<Arc<Picture>> {
    let data = discord
        .download(url, MAX_PREVIEW_DOWNLOAD_BYTES)
        .await
        .ok()?;
    let url = url.to_owned();
    tokio::task::spawn_blocking(move || {
        decode(
            &url,
            &data,
            (crate::chat::PREVIEW_WIDTH, crate::chat::PREVIEW_HEIGHT),
        )
    })
    .await
    .ok()
    .flatten()
    .map(Arc::new)
}

/// A picture downloaded from `url`, decoded to fit `width` by `height`: as
/// Lottie when `url` is a Lottie sticker's, which nothing else is.
pub(super) fn decode(url: &str, data: &[u8], (width, height): (u32, u32)) -> Option<Picture> {
    let picture = if yapfast_discord::ready::lottie_sticker(url) {
        crate::chat::decode_lottie(data, width, height)
    } else {
        crate::chat::decode_picture(data, width, height)
    };
    picture.map(|picture| crate::discord::read_kept(url, picture))
}

/// Messages found in a room, as [`found`] shows them, with the room's server.
pub(super) fn found_in(discord: &Discord, room: &str, messages: &[Message]) -> Vec<ChatMessage> {
    let snapshot = discord.snapshots().borrow().clone();
    let server = discord::parse_room(room)
        .and_then(|(guild, _)| snapshot.servers.iter().find(|server| server.id == guild))
        .map(|server| &*server.server);
    messages
        .iter()
        .map(|message| found(discord, server, message))
        .collect()
}

/// Messages a search found in a server, or with [`yapfast_discord::ready::PRIVATE`]
/// in DMs, as [`found`] shows them, each with its room.
pub(super) fn hits_in(discord: &Discord, guild: &str, messages: &[Message]) -> Vec<SearchHit> {
    let snapshot = discord.snapshots().borrow().clone();
    let server = snapshot
        .servers
        .iter()
        .find(|server| server.id == guild)
        .map(|server| &*server.server);
    messages
        .iter()
        .map(|message| SearchHit {
            room: discord::room_id(guild, &message.channel_id),
            message: found(discord, server, message),
        })
        .collect()
}

/// The custom emoji in a message's text and reactions.
pub(super) fn custom_emoji(message: &Message) -> Vec<CustomEmoji> {
    let mut found = emoji_in(&message.content);
    for card in &message.components {
        for text in card.texts().chain(card.rows.iter().flat_map(button_emoji)) {
            for emoji in emoji_in(text) {
                if !found.iter().any(|known| known.id == emoji.id) {
                    found.push(emoji);
                }
            }
        }
    }
    let reactions = message.reactions.iter().filter_map(|reaction| {
        let (_, id) = reaction.emoji.split_once(':')?;
        Some((&reaction.key, id))
    });
    let answers = message
        .poll
        .iter()
        .flat_map(|poll| &poll.answers)
        .filter_map(|answer| {
            let (key, id) = answer.emoji.as_ref()?;
            Some((key, id.as_deref()?))
        });
    for (key, id) in reactions.chain(answers) {
        if !found.iter().any(|known| known.id == id) {
            found.push(CustomEmoji {
                text: key.clone(),
                id: id.to_owned(),
                animated: false,
                usable: true,
                picture: yapfast_discord::ready::custom_emoji(
                    id,
                    false,
                    crate::chat::EMOJI_PICTURE_SIZE,
                ),
            });
        }
    }
    found
}

/// The custom emoji in Discord's markup.
fn emoji_in(content: &str) -> Vec<CustomEmoji> {
    yapfast_discord::message::custom_emoji(content)
        .into_iter()
        .map(|emoji| CustomEmoji {
            text: format!(":{}:", emoji.name),
            // Its address says whether it moves, which is how its frames come:
            // Discord serves an animated emoji as a GIF and a still one as a PNG.
            picture: yapfast_discord::ready::custom_emoji(
                &emoji.id,
                emoji.animated,
                crate::chat::EMOJI_PICTURE_SIZE,
            ),
            id: emoji.id,
            animated: emoji.animated,
            usable: true,
        })
        .collect()
}

/// Sends the open server channel's topic, as [`Event::Topic`], when it
/// opens (`opened`), and after that when the topic changes, or the names
/// of people it mentions do.
pub(super) fn send_topic(
    channel: &mut OpenChannel,
    discord: &Discord,
    events: &UnboundedSender<Event>,
    opened: bool,
) {
    if channel.guild == yapfast_discord::ready::PRIVATE {
        return;
    }
    let snapshot = discord.snapshots().borrow().clone();
    let server = snapshot
        .servers
        .iter()
        .find(|server| server.id == channel.guild)
        .map(|server| &*server.server);
    let raw = server
        .and_then(|server| {
            server
                .text_channels
                .iter()
                .find(|known| known.id == channel.channel)
        })
        .and_then(|known| known.topic.clone());
    let names = raw
        .as_deref()
        .filter(|raw| raw.contains("<@"))
        .map_or(0, |_| discord.names_version());
    let sent = (raw, names);
    if !opened && channel.topic_sent.as_ref() == Some(&sent) {
        return;
    }
    let topic = sent.0.as_deref().map(|raw| topic(discord, server, raw));
    channel.topic_sent = Some(sent);
    let _ = events.send(Event::Topic {
        room: channel.room.clone(),
        topic,
    });
}

/// A channel's topic, from Discord's markup, as Yapfast shows it.
fn topic(
    discord: &Discord,
    server: Option<&yapfast_discord::ready::Server>,
    raw: &str,
) -> Arc<Topic> {
    let (body, mentions) = rendered(discord, server, raw);
    Arc::new(Topic {
        body,
        mentions,
        emoji: emoji_in(raw),
    })
}

/// A forum's topic, its post guidelines, by server and channel id.
pub(super) fn forum_topic(discord: &Discord, guild: &str, forum: &str) -> Option<Arc<Topic>> {
    let snapshot = discord.snapshots().borrow().clone();
    let server = snapshot.servers.iter().find(|server| server.id == guild)?;
    let raw = server.channel(forum)?.topic.clone()?;
    Some(topic(discord, Some(server), &raw))
}

/// A message's body as Yapfast shows it, and who and what it mentions there.
pub(super) fn rendered(
    discord: &Discord,
    server: Option<&yapfast_discord::ready::Server>,
    content: &str,
) -> (String, Vec<Mentioned>) {
    let (body, mentions) = discord.render_mentions(server, content);
    let mentions = mentions
        .into_iter()
        .map(|mention| {
            // @everyone and @here take the colour of mentions.
            let (target, color) = match mention.target {
                MentionTarget::User(id) => {
                    let color = server.and_then(|server| discord.color(server, &id));
                    (Target::User(id), color)
                }
                MentionTarget::Role(id) => {
                    let color = server
                        .and_then(|server| server.role_colors.get(&id))
                        .map(|(_, color)| *color);
                    (Target::Role(id), color)
                }
                MentionTarget::Channel(id) => {
                    let guild = server.map_or("", |server| server.id.as_str());
                    (Target::Channel(discord::room_id(guild, &id)), None)
                }
                MentionTarget::Everyone => (Target::Everyone, None),
            };
            Mentioned {
                text: mention.text,
                target,
                color,
                at: mention.at,
            }
        })
        .collect();
    (body, mentions)
}

/// The roles a message in a server channel may mention, after @everyone and @here
/// for those allowed them, then the channels it may link with `#`.
pub(super) fn mention_groups(discord: &Discord, room: &str) -> Vec<MentionGroup> {
    let Some((guild, channel)) = discord::parse_room(room) else {
        return vec![];
    };
    let snapshot = discord.snapshots().borrow().clone();
    let Some(server) = snapshot.servers.iter().find(|server| server.id == guild) else {
        return vec![];
    };
    // A voice channel's chat may mention everyone too.
    let everyone = server
        .channel(channel)
        .is_some_and(|known| known.marks.mention_everyone);
    let mut groups = Vec::new();
    if everyone {
        for name in ["everyone", "here"] {
            groups.push(MentionGroup {
                name: name.into(),
                mention: format!("@{name}"),
                color: None,
                channel: false,
                voice: false,
            });
        }
    }
    groups.extend(
        server
            .mentionable(everyone)
            .into_iter()
            .map(|(id, name, color)| MentionGroup {
                name,
                mention: format!("<@&{id}>"),
                color,
                channel: false,
                voice: false,
            }),
    );
    // Its text and voice channels this account can see, in Discord's order.
    let text = server.text_channels().map(|known| (known, false));
    let voice = server.voice_channels().map(|known| (known, true));
    groups.extend(
        text.chain(voice)
            .filter(|(known, _)| known.marks.access != Access::Hidden)
            .map(|(known, voice)| MentionGroup {
                name: known.channel.name.clone(),
                mention: format!("<#{}>", known.channel.id),
                color: None,
                channel: true,
                voice,
            }),
    );
    groups
}

/// A Discord profile as Yapfast's profile card shows it, with the server's role
/// names, colours and names known now.
/// A profile's badges, as the card shows them.
pub(super) fn chat_badges(badges: Vec<yapfast_discord::profile::Badge>) -> Vec<ProfileBadge> {
    badges
        .into_iter()
        .map(|badge| ProfileBadge {
            description: badge.description,
            icon: badge.icon,
            link: badge.link,
            level: badge.level.map(|level| ProfileBadgeLevel {
                kind: level.kind,
                title: level.title,
                progress: level.progress,
                picture: level.picture,
            }),
        })
        .collect()
}

pub(super) fn chat_profile(
    discord: &Discord,
    guild: &str,
    profile: yapfast_discord::profile::Profile,
) -> Profile {
    let snapshot = discord.snapshots().borrow().clone();
    let server = snapshot
        .servers
        .iter()
        .find(|server| server.id == guild)
        .map(|server| &*server.server);
    let bio_emoji = profile.bio.as_deref().map(emoji_in).unwrap_or_default();
    Profile {
        name: profile.name,
        avatar: Some(profile.avatar),
        decoration: discord.decoration(Some(guild), &profile.user_id),
        style: discord
            .style(Some(guild), &profile.user_id)
            .map(|style| Arc::new(style.into())),
        effect: profile.effect,
        theme: profile.theme,
        banner: profile.banner,
        accent: profile.accent,
        pronouns: profile.pronouns.unwrap_or_default(),
        bio: profile
            .bio
            .map(|bio| discord.render(server, &bio))
            .unwrap_or_default(),
        roles: server
            .map(|server| server.named_roles(&profile.roles))
            .unwrap_or_default()
            .into_iter()
            .map(|(name, color)| ProfileRole { name, color })
            .collect(),
        created: Some(crate::clock::day(profile.created / 1000)),
        joined: profile.joined.map(|ms| crate::clock::day(ms / 1000)),
        servers: profile
            .mutual_servers
            .into_iter()
            .filter_map(|mutual| {
                let server = snapshot
                    .servers
                    .iter()
                    .find(|server| server.id == mutual.id)?;
                Some(ProfileServer {
                    id: discord::server_id(&server.id),
                    name: server.name.clone(),
                    icon: server.icon.clone(),
                    banner: server.banner.clone(),
                    members: server.member_count,
                    channels: server.text_channels.len() + server.voice_channels.len(),
                    nick: mutual.nick,
                })
            })
            .collect(),
        friends: profile
            .mutual_friends
            .into_iter()
            .map(|friend| ProfileFriend {
                id: friend.id,
                name: friend.name,
                avatar: Some(friend.avatar),
            })
            .collect(),
        tag: discord::server_tag(profile.tag),
        badges: chat_badges(profile.badges),
        bio_emoji,
        // The note comes with its own request; see `fill_profile`.
        note: String::new(),
        connections: profile
            .connections
            .into_iter()
            .map(|connection| ProfileConnection {
                kind: connection.kind,
                name: connection.name,
                verified: connection.verified,
                url: connection.url,
                since: connection.since.map(|ms| crate::clock::day(ms / 1000)),
            })
            .collect(),
        handle: profile.username,
        id: profile.user_id,
        // What the member list knows; the profile doesn't say.
        presence: None,
        activity: None,
        activities: Default::default(),
    }
}

/// The emoji of a row's buttons, in Discord's markup.
fn button_emoji(row: &yapfast_discord::components::Row) -> Vec<&str> {
    use yapfast_discord::components::{Beside, Row};
    let buttons = match row {
        Row::Buttons(buttons) => buttons.iter().collect(),
        Row::Text {
            beside: Some(Beside::Button(button)),
            ..
        } => vec![button],
        _ => Vec::new(),
    };
    buttons
        .into_iter()
        .filter_map(|button| button.emoji.as_deref())
        .collect()
}

/// A message's components as Yapfast shows them, their mentions written as
/// names.
pub(super) fn chat_card(
    discord: &Discord,
    server: Option<&yapfast_discord::ready::Server>,
    card: &yapfast_discord::components::Card,
) -> Card {
    use yapfast_discord::components::{Beside as DiscordBeside, Row, Style};
    let button = |button: &yapfast_discord::components::Button| CardButton {
        label: match &button.emoji {
            Some(emoji) => format!("{} {}", discord.render(server, emoji), button.label)
                .trim_end()
                .to_owned(),
            None => button.label.clone(),
        },
        style: match button.style {
            Style::Primary => ButtonStyle::Primary,
            Style::Secondary => ButtonStyle::Secondary,
            Style::Success => ButtonStyle::Success,
            Style::Danger => ButtonStyle::Danger,
            Style::Link => ButtonStyle::Link,
        },
        url: button.url.clone(),
        disabled: button.disabled,
    };
    Card {
        framed: card.framed,
        color: card.color,
        rows: card
            .rows
            .iter()
            .map(|row| match row {
                Row::Text { text, beside } => CardRow::Text {
                    text: discord.render(server, text),
                    beside: beside.as_ref().map(|beside| match beside {
                        DiscordBeside::Thumbnail(slot) => Beside::Thumbnail(*slot),
                        DiscordBeside::Button(each) => Beside::Button(button(each)),
                    }),
                },
                Row::Media(slot) => CardRow::Media(*slot),
                Row::Separator { divider, large } => CardRow::Separator {
                    divider: *divider,
                    large: *large,
                },
                Row::Buttons(buttons) => CardRow::Buttons(buttons.iter().map(button).collect()),
                Row::Select { text, disabled } => CardRow::Select {
                    text: text.clone(),
                    disabled: *disabled,
                },
                Row::Unknown => CardRow::Unknown,
            })
            .collect(),
    }
}

/// An embed as Yapfast shows it, its mentions written as names.
pub(super) fn chat_embed(
    discord: &Discord,
    server: Option<&yapfast_discord::ready::Server>,
    embed: &yapfast_discord::embed::Embed,
) -> Embed {
    let text = |text: &Option<String>| text.clone().unwrap_or_default();
    Embed {
        color: embed.color,
        provider: text(&embed.provider),
        author: text(&embed.author),
        title: text(&embed.title),
        url: text(&embed.url),
        description: embed
            .description
            .as_deref()
            .map(|description| discord.render(server, description))
            .unwrap_or_default(),
        fields: embed
            .fields
            .iter()
            .map(|field| EmbedField {
                name: field.name.clone(),
                value: discord.render(server, &field.value),
                inline: field.inline,
            })
            .collect(),
        footer: text(&embed.footer),
        video: embed.video,
    }
}

pub(super) fn chat_member(member: ListedMember) -> ChatMember {
    ChatMember {
        id: member.user_id,
        name: member.name,
        username: member.username,
        role: Role::Member,
        avatar: Some(member.avatar),
        decoration: member.decoration,
        nameplate: member.nameplate.map(Into::into),
        style: member.style.map(|style| Arc::new(style.into())),
        group: Some(member.group),
        group_icon: member.group_icon,
        role_icon: member.role_icon,
        presence: Some(discord::presence(member.presence)),
        color: member.color,
        activity: member.activity,
        activities: member.activities,
        roles: member.roles,
        timed_out_until: member.timed_out_until,
        tag: discord::server_tag(member.tag),
    }
}

pub(super) fn publish(
    channel: &mut OpenChannel,
    discord: &Discord,
    done: &UnboundedSender<Done>,
    events: &UnboundedSender<Event>,
) {
    // The timeline stays as it is while the messages around one jumped to
    // are on their way.
    if channel.focusing {
        return;
    }
    let messages = channel.publish(discord, done);
    let _ = events.send(Event::Timeline {
        room: channel.room.clone(),
        messages,
    });
}

/// The Discord message id of a sent message's timeline item.
pub(super) fn sent_id(item: &str) -> Option<&str> {
    item.strip_prefix("d:")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Discord serves an animated emoji as a GIF and a still one as a PNG, so
    /// the address is what gives Yapfast an animation to draw at all.
    #[test]
    fn an_animated_emoji_is_asked_for_as_a_gif() {
        let moving = emoji_in("hi <a:party:7>");
        assert!(moving[0].animated);
        assert!(
            moving[0].picture.contains("/emojis/7.gif"),
            "{}",
            moving[0].picture
        );
        let still = emoji_in("hi <:talk:8>");
        assert!(!still[0].animated);
        assert!(
            still[0].picture.contains("/emojis/8.png"),
            "{}",
            still[0].picture
        );
    }
}
