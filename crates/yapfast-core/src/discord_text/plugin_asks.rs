//! What a plugin asks of the Discord account that Discord or Spotify answers:
//! its own activity, the bio and pronouns, a search, the signed-in devices,
//! and the linked Spotify's player. Each runs on a task of its own and its
//! answer comes back as [`Event::PluginAnswered`] under the plugin's call, so
//! nothing waits on the network but the plugin that asked.

use super::{commands::NOT_DISCORD, render::hits_in, *};
use crate::chat::PluginAsk;
use yapfast_plugin_api::account::{ActivitySet, SearchHit, SearchPage, SessionView, SpotifyAction};
use yapfast_plugin_api::call::HostReturn;

/// Carries out a plugin's ask, or gives back any other command.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let Command::Discord(DiscordCommand::PluginAsk { plugin, call, ask }) = command else {
        return Some(command);
    };
    let answered = {
        let events = events.clone();
        move |answer: HostReturn| {
            let _ = events.send(Event::PluginAnswered {
                plugin: plugin.clone(),
                call,
                answer,
            });
        }
    };
    let discord = match discord {
        Ok(discord) => discord.clone(),
        Err(why) => {
            answered(HostReturn::Error {
                message: if why.is_empty() { NOT_DISCORD } else { why }.to_owned(),
            });
            return None;
        }
    };
    tokio::spawn(async move {
        let answer = match *ask {
            PluginAsk::Activity(activity) => {
                let activity = activity.map(|activity| gateway_activity(*activity));
                match discord.set_plugin_activity(activity).await {
                    Ok(()) => HostReturn::Ok(serde_json::Value::Bool(true)),
                    Err(error) => failed(&error),
                }
            }
            PluginAsk::Profile { bio, pronouns } => {
                match discord
                    .set_bio_and_pronouns(bio.as_deref(), pronouns.as_deref())
                    .await
                {
                    Ok(()) => HostReturn::Ok(serde_json::Value::Bool(true)),
                    Err(error) => failed(&error),
                }
            }
            PluginAsk::Search(request) => search(&discord, request).await,
            PluginAsk::Sessions => match discord.auth_sessions().await {
                Ok(answer) => listed(sessions_of(&answer)),
                Err(error) => failed(&error),
            },
            PluginAsk::Affinities(request) => affinities(&discord, request).await,
            PluginAsk::Messages {
                room,
                message,
                before,
                limit,
            } => messages(&discord, &room, message, before, limit).await,
            PluginAsk::Voice {
                room,
                path,
                duration_ms,
                waveform,
            } => voice(&discord, &room, &path, duration_ms, &waveform).await,
            PluginAsk::Spotify(action) => match discord.spotify(spotify_command(action)).await {
                Ok(value) => HostReturn::Ok(value),
                Err(refusal) => HostReturn::Error {
                    message: refusal.to_string(),
                },
            },
        };
        answered(answer);
    });
    None
}

fn failed(error: &anyhow::Error) -> HostReturn {
    tracing::debug!(
        error = format!("{error:#}"),
        "a plugin's ask of Discord failed"
    );
    HostReturn::Error {
        message: format!("{error:#}"),
    }
}

fn listed<T: serde::Serialize>(value: T) -> HostReturn {
    match serde_json::to_value(value) {
        Ok(value) => HostReturn::Ok(value),
        Err(error) => HostReturn::Error {
            message: error.to_string(),
        },
    }
}

/// A plugin's activity as the Discord crate shows one.
pub(crate) fn gateway_activity(
    activity: ActivitySet,
) -> yapfast_discord::plugin_activity::PluginActivity {
    yapfast_discord::plugin_activity::PluginActivity {
        kind: activity.activity_type().unwrap_or(0),
        name: activity.name.trim().to_owned(),
        details: activity.details,
        state: activity.state,
        start_ms: activity.start_ms,
        end_ms: activity.end_ms,
        application_id: activity.application_id,
        large_image: activity.large_image,
        large_text: activity.large_text,
        small_image: activity.small_image,
        small_text: activity.small_text,
        buttons: activity
            .buttons
            .into_iter()
            .map(|button| (button.label.trim().to_owned(), button.url))
            .collect(),
        url: activity.url,
        party_id: activity.party.as_ref().and_then(|party| party.id.clone()),
        party_size: activity.party.and_then(|party| party.size),
    }
}

fn spotify_command(action: SpotifyAction) -> yapfast_discord::spotify::Command {
    use yapfast_discord::spotify::Command;
    match action {
        SpotifyAction::Now => Command::Now,
        SpotifyAction::Play => Command::Play,
        SpotifyAction::Pause => Command::Pause,
        SpotifyAction::Next => Command::Next,
        SpotifyAction::Previous => Command::Previous,
        SpotifyAction::Seek { position_ms } => Command::Seek(position_ms),
        SpotifyAction::Volume { percent } => Command::Volume(percent),
        SpotifyAction::Shuffle { on } => Command::Shuffle(on),
        SpotifyAction::Repeat { mode } => Command::Repeat(mode),
    }
}

/// The devices Discord lists, read-only, as a plugin is told them.
fn sessions_of(answer: &serde_json::Value) -> Vec<SessionView> {
    answer["user_sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|session| {
            let text = |value: &serde_json::Value| value.as_str().map(str::to_owned);
            Some(SessionView {
                id_hash: session["id_hash"].as_str()?.to_owned(),
                last_used: text(&session["approx_last_used_time"]),
                os: text(&session["client_info"]["os"]),
                platform: text(&session["client_info"]["platform"]),
                location: text(&session["client_info"]["location"]),
            })
        })
        .collect()
}

/// A message, or a page of them, of a Discord conversation the window doesn't
/// hold, as the plugin is told them: `messages.get` and `messages.history`
/// for any channel the account can read, asked of Discord as its client asks.
async fn messages(
    discord: &Discord,
    room: &str,
    message: Option<String>,
    before: Option<String>,
    limit: Option<usize>,
) -> HostReturn {
    use yapfast_discord::api::Span;
    let Some((guild, channel)) = discord::parse_room(room) else {
        return HostReturn::Error {
            message: format!("{room} isn't a Discord conversation"),
        };
    };
    // Both forms of a message's id are read: the timeline's `d:<id>` and the
    // bare Discord one.
    let bare = |id: &str| id.strip_prefix("d:").unwrap_or(id).to_owned();
    match message {
        Some(id) => {
            let id = bare(&id);
            match discord.history(guild, channel, Span::Around(&id)).await {
                Ok(page) => match page.messages.iter().find(|message| message.id == id) {
                    Some(found) => {
                        listed(crate::application::plugins::message_view(discord, room, found).0)
                    }
                    None => HostReturn::Ok(serde_json::Value::Null),
                },
                Err(error) => failed(&error),
            }
        }
        None => {
            let before = before.map(|id| bare(&id));
            let span = match &before {
                Some(id) => Span::Before(id),
                None => Span::Latest,
            };
            match discord.history(guild, channel, span).await {
                Ok(page) => {
                    let limit = limit.unwrap_or(50).clamp(1, 100);
                    let skip = page.messages.len().saturating_sub(limit);
                    let views: Vec<_> = page
                        .messages
                        .iter()
                        .skip(skip)
                        .map(|message| {
                            crate::application::plugins::message_view(discord, room, message).0
                        })
                        .collect();
                    listed(views)
                }
                Err(error) => failed(&error),
            }
        }
    }
}

/// A recording sent as a voice message, as Discord's clients send one: the
/// Ogg Opus uploaded, then a message with only it, flagged a voice message,
/// with its length and waveform.
async fn voice(
    discord: &Discord,
    room: &str,
    path: &std::path::Path,
    duration_ms: u64,
    waveform: &str,
) -> HostReturn {
    let Some((guild, channel)) = discord::parse_room(room) else {
        return HostReturn::Error {
            message: format!("{room} isn't a Discord conversation"),
        };
    };
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return HostReturn::Error {
                message: format!("the recording can't be read: {error}"),
            };
        }
    };
    match discord
        .send_voice_message(guild, channel, bytes, duration_ms as f64 / 1000.0, waveform)
        .await
    {
        Ok(()) => HostReturn::Ok(serde_json::Value::Bool(true)),
        Err(error) => failed(&error),
    }
}

/// Discord's affinity scores, read-only, as a plugin is told them: people by
/// their user id, highest first, and servers as `rooms.list` names them.
async fn affinities(
    discord: &Discord,
    request: yapfast_plugin_api::account::AffinitiesRequest,
) -> HostReturn {
    use yapfast_plugin_api::account::AffinityOf;
    let which = match (request.of, request.detailed) {
        (AffinityOf::Servers, _) => yapfast_discord::service::Affinities::Servers,
        (AffinityOf::Users, false) => yapfast_discord::service::Affinities::Users,
        (AffinityOf::Users, true) => yapfast_discord::service::Affinities::UsersDetailed,
    };
    match discord.affinities(which).await {
        Ok(answer) => listed(affinities_of(&answer)),
        Err(error) => failed(&error),
    }
}

/// The three answers Discord gives (docs.discord.food, resources/user,
/// User Affinity Object and its v2), read into one shape.
fn affinities_of(answer: &serde_json::Value) -> yapfast_plugin_api::account::Affinities {
    use yapfast_plugin_api::account::{Affinities, ServerAffinity, UserAffinity};
    let float = |value: &serde_json::Value| value.as_f64();
    let rank = |value: &serde_json::Value| value.as_u64().and_then(|n| u32::try_from(n).ok());
    let text = |value: &serde_json::Value| value.as_str().map(str::to_owned);
    let mut users: Vec<UserAffinity> = answer["user_affinities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            // v1 names the person `user_id`, v2 `other_user_id`.
            if let Some(person) = entry["user_id"].as_str() {
                return Some(UserAffinity {
                    person: person.to_owned(),
                    affinity: float(&entry["affinity"]).unwrap_or_default(),
                    ..UserAffinity::default()
                });
            }
            let person = entry["other_user_id"].as_str()?.to_owned();
            let communication = float(&entry["communication_probability"]);
            Some(UserAffinity {
                person,
                affinity: communication.unwrap_or_default(),
                is_friend: entry["is_friend"].as_bool(),
                user_segment: text(&entry["user_segment"]),
                other_user_segment: text(&entry["other_user_segment"]),
                dm_probability: float(&entry["dm_probability"]),
                dm_rank: rank(&entry["dm_rank"]),
                vc_probability: float(&entry["vc_probability"]),
                vc_rank: rank(&entry["vc_rank"]),
                server_message_probability: float(&entry["server_message_probability"]),
                server_message_rank: rank(&entry["server_message_rank"]),
                communication_probability: communication,
                communication_rank: rank(&entry["communication_rank"]),
            })
        })
        .collect();
    users.sort_by(|a, b| b.affinity.total_cmp(&a.affinity));
    let mut servers: Vec<ServerAffinity> = answer["guild_affinities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(ServerAffinity {
                server: crate::discord::server_id(entry["guild_id"].as_str()?),
                affinity: float(&entry["affinity"]).unwrap_or_default(),
            })
        })
        .collect();
    servers.sort_by(|a, b| b.affinity.total_cmp(&a.affinity));
    Affinities { users, servers }
}

/// One page of a search, as the search box asks Discord for it, newest first.
async fn search(
    discord: &Discord,
    request: yapfast_plugin_api::account::SearchRequest,
) -> HostReturn {
    let scope = discord::parse_room(&request.place)
        .map(|(guild, channel)| (guild.to_owned(), Some(channel.to_owned())))
        .or_else(|| discord::parse_server(&request.place).map(|guild| (guild.to_owned(), None)));
    let Some((guild, channel)) = scope else {
        return HostReturn::Error {
            message: format!(
                "{} isn't a Discord conversation or server to search",
                request.place
            ),
        };
    };
    let searched = discord
        .search(
            &guild,
            channel.as_deref(),
            &request.query,
            yapfast_discord::search::Sort::Newest,
            request.offset,
            &|| {},
        )
        .await;
    match searched {
        Ok(yapfast_discord::service::Searched::Found(page)) => {
            let hits = hits_in(discord, &guild, &page.messages, &page.threads)
                .into_iter()
                .take(yapfast_plugin_api::account::SEARCH_PAGE)
                .map(|hit| SearchHit {
                    room: hit.room,
                    message: hit.message.id,
                    author: hit.message.sender,
                    body: hit.message.body,
                    at: hit.message.timestamp,
                })
                .collect();
            listed(SearchPage {
                total: page.total,
                hits,
                indexing: page.indexing_older,
            })
        }
        Ok(yapfast_discord::service::Searched::Indexing) => listed(SearchPage {
            indexing: true,
            ..SearchPage::default()
        }),
        Err(error) => failed(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sessions_are_read_as_discord_lists_them() {
        let answer = json!({"user_sessions": [
            {"id_hash": "abc", "approx_last_used_time": "2026-11-20T10:00:00Z",
             "client_info": {"os": "Linux", "platform": "Yapfast", "location": "Berlin"}},
            {"client_info": {}}
        ]});
        let sessions = sessions_of(&answer);
        assert_eq!(sessions.len(), 1, "one without an id hash is left out");
        assert_eq!(sessions[0].os.as_deref(), Some("Linux"));
        assert_eq!(sessions[0].location.as_deref(), Some("Berlin"));
    }

    #[test]
    fn affinities_are_read_from_each_of_discords_answers() {
        // docs.discord.food, resources/user: v1's example shape.
        let v1 = affinities_of(&json!({"user_affinities": [
            {"user_id": "2", "affinity": 0.25},
            {"user_id": "3", "affinity": 0.75},
            {"affinity": 1.0}
        ]}));
        assert_eq!(v1.users.len(), 2, "one without an id is left out");
        assert_eq!(v1.users[0].person, "3", "highest first");
        assert!(v1.users[0].dm_rank.is_none());
        assert!(v1.servers.is_empty());

        // v2, from the same page's example.
        let v2 = affinities_of(&json!({"user_affinities": [{
            "other_user_id": "1001086404203389018", "user_segment": "HFU_MAU",
            "other_user_segment": "HFU_MAU", "is_friend": true,
            "dm_probability": 0.8697, "dm_rank": 1, "vc_probability": 0.0048,
            "vc_rank": 4, "server_message_probability": 0.8469,
            "server_message_rank": 6, "communication_probability": 0.5738,
            "communication_rank": 1
        }]}));
        let person = &v2.users[0];
        assert_eq!(person.person, "1001086404203389018");
        assert_eq!(person.affinity, 0.5738);
        assert_eq!(person.is_friend, Some(true));
        assert_eq!(person.vc_rank, Some(4));
        assert_eq!(person.user_segment.as_deref(), Some("HFU_MAU"));

        let servers = affinities_of(&json!({"guild_affinities": [
            {"guild_id": "9", "affinity": 0.5}
        ]}));
        assert_eq!(servers.servers[0].server, crate::discord::server_id("9"));
    }
}
