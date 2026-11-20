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
        let answer = match ask {
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
}
