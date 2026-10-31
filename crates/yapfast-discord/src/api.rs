//! Discord's HTTP API, called the way the web client calls it.

use std::sync::RwLock;

use anyhow::{Context, Result, anyhow, bail};
use reqwest::{Client, Method, RequestBuilder, Response, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    captcha::Captcha,
    client,
    limits::{self, Limits, Refusals, Stopped},
    mfa, scheduled_event,
    settings::{self, CustomStatus},
};

const API: &str = "https://discord.com/api/v9";
/// The web client's page, which carries its build number.
const WEB_APP: &str = "https://discord.com/app";
const UNAUTHORIZED: &str = "Discord rejected the login. Link Discord again.";
/// How a message goes, besides what it says.
#[derive(Clone, Copy, Debug)]
pub struct Sending<'a> {
    /// Matches the message to its echo from the gateway.
    pub nonce: &'a str,
    /// The message it replies to, in the same channel.
    pub reply: Option<&'a str>,
    /// Whether a reply notifies the author of what it replies to.
    pub ping_reply: bool,
    /// Notifies nobody, as `@silent` does.
    pub silent: bool,
}

/// A poll to post, as Discord's "Create a Poll" dialog makes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewPoll {
    pub question: String,
    pub answers: Vec<NewAnswer>,
    /// How long it takes votes, in hours: up to 32 days'.
    pub hours: u32,
    pub multiselect: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewAnswer {
    pub text: String,
    /// The emoji itself, or a custom one's `name:id`, as a reaction takes it.
    pub emoji: Option<String>,
}

impl NewPoll {
    /// The poll as a message's `poll` field takes it. A custom emoji goes
    /// by its id alone, as Discord's client sends it.
    fn body(&self) -> Value {
        let answers: Vec<Value> = self
            .answers
            .iter()
            .map(|answer| {
                let mut media = json!({"text": answer.text});
                match answer.emoji.as_deref().map(|emoji| emoji.split_once(':')) {
                    Some(Some((_, id))) => media["emoji"] = json!({"id": id, "name": ""}),
                    Some(None) => media["emoji"] = json!({"name": answer.emoji}),
                    None => {}
                }
                json!({"poll_media": media})
            })
            .collect();
        json!({
            "question": {"text": self.question},
            "answers": answers,
            "allow_multiselect": self.multiselect,
            "duration": self.hours,
            "layout_type": 1,
        })
    }
}

/// The message flag of `@silent`: no push or desktop notifications.
const SUPPRESS_NOTIFICATIONS: u64 = 1 << 12;
/// The type of a message reference that forwards the message, rather than
/// replying to it.
const FORWARD: u64 = 1;
/// The most messages one history request returns.
pub const PAGE: usize = 50;

/// How many archived threads a page of a channel's list of threads holds,
/// as Discord's client asks for them.
pub const THREAD_PAGE: u32 = 25;

/// How many bans Bans fetches at a time, as Discord's client does; a
/// batch this full may have more after it.
pub const BANS_PAGE: usize = 1000;

/// What a members search gave: Discord's answer, or the seconds to wait
/// while it indexes the server.
#[derive(Debug)]
pub enum MembersSearch {
    Found(Value),
    Indexing(f64),
}

/// How many audit log entries Audit Log fetches at a time, as Discord's
/// client does; a page this full may have older ones.
pub const AUDIT_PAGE: usize = 50;
/// How many posts' messages Discord's client asks a forum for at a time.
pub const POSTS_PER_ASK: usize = 10;

/// Which of a channel's messages a page holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span<'a> {
    /// The newest.
    Latest,
    /// Those before a snowflake.
    Before(&'a str),
    /// Those after a snowflake, which Discord's client asks for as the
    /// timeline scrolls down through older messages.
    After(&'a str),
    /// Those on both sides of a snowflake, as for a jump to a message;
    /// Discord answers even when no message has it.
    Around(&'a str),
}

#[derive(Debug, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub global_name: Option<String>,
}

/// A file uploaded to Discord's storage, ready to attach to a message.
pub struct Upload {
    pub filename: String,
    pub uploaded_filename: String,
}

/// The most files one message can carry.
pub const MAX_FILES: usize = 10;

/// The most channels one bulk read acknowledgement carries.
pub const ACK_BULK_LIMIT: usize = 100;

const MIB: u64 = 1024 * 1024;

/// The largest file this account may send, in bytes: by its Nitro, as
/// [`crate::service::Account::premium_type`], or in a server boosted to
/// `premium_tier` 2 or 3, by the server's, whichever allows more. These are
/// Discord's limits since August 2026. Its client's own constant without
/// Nitro is still 10 MiB, but the client raises every limit to at least
/// 20 MiB before checking a file or naming the limit.
pub fn upload_limit(premium_type: u8, premium_tier: Option<u8>) -> u64 {
    let own = match premium_type {
        2 => 500 * MIB,
        1 | 3 => 50 * MIB,
        _ => 20 * MIB,
    };
    let boosted = match premium_tier {
        Some(3) => 100 * MIB,
        Some(2) => 50 * MIB,
        _ => 0,
    };
    own.max(boosted)
}

/// Discord answered a download with this error status.
#[derive(Debug)]
pub struct Refused(pub StatusCode);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Discord answered {}", self.0)
    }
}

impl std::error::Error for Refused {}

/// A download that Discord stopped answering for [`STALL`], given up so it
/// doesn't hold its turn.
#[derive(Debug)]
pub struct Stalled;

impl std::fmt::Display for Stalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Discord stopped sending the file")
    }
}

impl std::error::Error for Stalled {}

/// How long a download may wait for Discord's next bytes. A connection lost
/// to a network change or sleep sends nothing more, and without a limit its
/// downloads would hold their turns, and every download after them would
/// wait, until Yapfast closed.
const STALL: std::time::Duration = std::time::Duration::from_secs(20);

/// Discord's own code and words for a refusal, in its error's chain: `{:#}`
/// shows them, and [`code`] finds the code.
#[derive(Debug)]
pub struct DiscordError {
    pub code: u64,
    pub message: String,
}

impl std::fmt::Display for DiscordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "code {}: {}", self.code, self.message)
    }
}

impl std::error::Error for DiscordError {}

/// The code Discord refused a request with, if it gave one.
pub fn code(error: &anyhow::Error) -> Option<u64> {
    error
        .downcast_ref::<DiscordError>()
        .map(|refusal| refusal.code)
}

/// Whether a failed [`Api::download`] may work if tried later: Discord was
/// out of reach, failed or asked Yapfast to slow down, rather than refusing the
/// file or Yapfast refusing what came.
pub fn may_work_later(error: &anyhow::Error) -> bool {
    match error.downcast_ref::<Refused>() {
        Some(Refused(status)) => {
            status.is_server_error()
                || matches!(
                    *status,
                    StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_MANY_REQUESTS
                )
        }
        None => {
            error.downcast_ref::<reqwest::Error>().is_some()
                || error.downcast_ref::<Stalled>().is_some()
        }
    }
}

pub struct Api {
    http: Client,
    /// Empty without a login; Discord may refresh it in READY.
    token: RwLock<String>,
    /// The API's base URL: Discord's, or the test server's.
    base: String,
    limits: Limits,
    refusals: Refusals,
    /// Downloads going at once, so opening the emoji picker or a busy channel
    /// spreads its pictures out rather than asking for hundreds together.
    downloads: tokio::sync::Semaphore,
    /// [`STALL`], shorter in tests.
    stall: std::time::Duration,
}

/// The most files Yapfast downloads from Discord at once. They share one HTTP/2
/// connection, as a browser's requests to one host do.
const DOWNLOADS: usize = 8;

impl Api {
    pub fn new(token: String) -> Result<Self> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let http = Client::builder()
            .user_agent(client::user_agent())
            .cookie_provider(crate::cookies::jar())
            .connect_timeout(std::time::Duration::from_secs(15))
            // Requests to a host share one HTTP/2 connection. While some are
            // waiting, a ping that goes unanswered closes it, so the next
            // request opens a new one rather than waiting on a lost one.
            .http2_keep_alive_interval(std::time::Duration::from_secs(10))
            .http2_keep_alive_timeout(std::time::Duration::from_secs(10))
            .build()
            .context("could not set up HTTPS")?;
        let base = match client::test_server() {
            Some(server) => format!("{server}/api/v9"),
            None => API.to_owned(),
        };
        Ok(Self {
            http,
            token: RwLock::new(token),
            base,
            limits: Limits::default(),
            refusals: Refusals::default(),
            downloads: tokio::sync::Semaphore::new(DOWNLOADS),
            stall: STALL,
        })
    }

    /// Without a login, for linking one.
    pub fn anonymous() -> Result<Self> {
        Self::new(String::new())
    }

    /// Reads the web client's current build number from its page, as a
    /// browser loads that page before the client connects, and keeps it on
    /// disk for the next start. Once a run is enough; until then, and if the
    /// page can't be read, Yapfast sends the last one it knew.
    pub async fn learn_build_number(&self) {
        /// The page sets it near the top; the whole page is about 75 KiB.
        const READ: usize = 256 << 10;
        if client::test_server().is_some() || client::knows_build_number() {
            return;
        }
        let read = async {
            let mut response = self
                .http
                .get(WEB_APP)
                .header(header::ACCEPT, "text/html")
                .send()
                .await?
                .error_for_status()?;
            let mut page = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                page.extend_from_slice(&chunk);
                if client::learn_build_number(&page) || page.len() >= READ {
                    break;
                }
            }
            anyhow::Ok(())
        };
        match tokio::time::timeout(std::time::Duration::from_secs(10), read).await {
            Ok(Ok(())) if client::knows_build_number() => {
                let number = client::known_build_number();
                let _ =
                    tokio::task::spawn_blocking(move || crate::cache::keep_build_number(number))
                        .await;
            }
            Ok(Ok(())) => tracing::warn!("Discord's web page gave no build number"),
            Ok(Err(error)) => tracing::warn!(%error, "could not read Discord's web page"),
            Err(_) => tracing::warn!("Discord's web page didn't load"),
        }
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        // Only the API gets the client's description, as in the web client;
        // it changes through a run, so each request makes it again.
        let request = self
            .http
            .request(method, format!("{}{path}", self.base))
            .header("X-Super-Properties", client::super_properties());
        // A request sent again once confirmed carries what confirmed it.
        let request = match mfa::token() {
            Some(confirmed) => request.header("X-Discord-MFA-Authorization", confirmed),
            None => request,
        };
        let token = self.token();
        if token.is_empty() {
            return request;
        }
        request.header(header::AUTHORIZATION, token.as_str())
    }

    fn token(&self) -> std::sync::RwLockReadGuard<'_, String> {
        self.token
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Asks with `token` from now on: Discord refreshed the login to it.
    pub fn relogin(&self, token: &str) {
        *self
            .token
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = token.to_owned();
    }

    /// Trades a remote login's ticket for the login, encrypted with the key
    /// that started it, in base64; see [`crate::remote_auth`]. Discord may
    /// ask for a [`Captcha`] first, whose solution goes with the next try.
    pub async fn remote_auth_login(
        &self,
        ticket: &str,
        solved: Option<(&Captcha, &str)>,
    ) -> Result<String> {
        let body = json!({"ticket": ticket});
        let mut request = self.request(Method::POST, "/users/@me/remote-auth/login");
        if let Some((captcha, solution)) = solved {
            for (name, value) in captcha.headers(solution) {
                request = request.header(name, value);
            }
        }
        let answer = self.json(with_json(request, &body)).await?;
        answer["encrypted_token"]
            .as_str()
            .map(str::to_owned)
            .context("Discord didn't hand over the login.")
    }

    /// Ends this login's session on Discord, as signing out of the web client
    /// does. The token stops working.
    pub async fn logout(&self) -> Result<()> {
        let body = json!({"provider": null, "voip_provider": null});
        self.empty(with_json(self.request(Method::POST, "/auth/logout"), &body))
            .await
    }

    /// The account the token belongs to.
    pub async fn me(&self) -> Result<User> {
        let body = self.json(self.request(Method::GET, "/users/@me")).await?;
        serde_json::from_value(body).context("could not understand Discord's answer")
    }

    /// Streams the games Discord detects to `file`, so no whole copy of the
    /// list sits in memory. As Discord's desktop client does, it asks the
    /// CDN, which answers without the list when `etag` is still its version.
    pub(crate) async fn detectable_games(
        &self,
        file: std::fs::File,
        etag: Option<&str>,
    ) -> Result<GameList> {
        use tokio::io::AsyncWriteExt;
        const LIMIT: u64 = 32 * 1024 * 1024;
        let mut request = self
            .http
            .get(format!("{}/detectables/games-v1.json", crate::ready::cdn()));
        if let Some(etag) = etag {
            request = request.header(header::IF_NONE_MATCH, etag);
        }
        let mut response = request.send().await.context("Can't reach Discord.")?;
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(GameList::Unchanged);
        }
        if !response.status().is_success() {
            return Err(Refused(response.status()).into());
        }
        let etag = response
            .headers()
            .get(header::ETAG)
            .and_then(|etag| etag.to_str().ok())
            .map(str::to_owned);
        let mut file = tokio::fs::File::from_std(file);
        let mut size = 0;
        while let Some(chunk) = response.chunk().await? {
            size += chunk.len() as u64;
            if size > LIMIT {
                bail!("game catalogue exceeds 32 MiB");
            }
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        Ok(GameList::Written(etag))
    }

    /// The application a game's Rich Presence names, for its name, as the
    /// desktop client asks when a game connects.
    #[cfg(target_os = "linux")]
    pub(crate) async fn rpc_application(&self, id: &str) -> Result<Value> {
        let path = format!("/oauth2/applications/{id}/rpc");
        self.json(self.request(Method::GET, &path)).await
    }

    /// Discord's media proxy's names for images a game's Rich Presence gives
    /// by address, as the desktop client asks for them.
    #[cfg(target_os = "linux")]
    pub(crate) async fn rpc_external_assets(&self, id: &str, urls: &[&str]) -> Result<Value> {
        let path = format!("/applications/{id}/external-assets");
        self.json(with_json(
            self.request(Method::POST, &path),
            &json!({"urls": urls}),
        ))
        .await
    }

    /// The images an application uploaded, by name, for its Rich Presence.
    #[cfg(target_os = "linux")]
    pub(crate) async fn rpc_assets(&self, id: &str) -> Result<Value> {
        let path = format!("/oauth2/applications/{id}/assets");
        self.json(self.request(Method::GET, &path)).await
    }

    /// Up to [`PAGE`] messages of a channel, newest first, as [`Span`] says.
    pub async fn messages(&self, channel: &str, span: Span<'_>) -> Result<Vec<Value>> {
        let mut path = format!("/channels/{channel}/messages?limit={PAGE}");
        match span {
            Span::Latest => {}
            Span::Before(id) => path.push_str(&format!("&before={id}")),
            Span::After(id) => path.push_str(&format!("&after={id}")),
            Span::Around(id) => path.push_str(&format!("&around={id}")),
        }
        match self.json(self.request(Method::GET, &path)).await? {
            Value::Array(messages) => Ok(messages),
            _ => bail!("could not understand Discord's answer"),
        }
    }

    /// Sends a message, as a reply when `options` names one in the same
    /// channel, with uploaded files.
    pub async fn send(
        &self,
        guild: Option<&str>,
        channel: &str,
        content: &str,
        options: Sending<'_>,
        uploads: &[Upload],
    ) -> Result<Value> {
        let mut body = message_body(content, options.nonce);
        if options.silent {
            body["flags"] = SUPPRESS_NOTIFICATIONS.into();
        }
        if !uploads.is_empty() {
            body["attachments"] = attachments(uploads);
        }
        if let Some(reply) = options.reply {
            body["message_reference"] = json!({
                "guild_id": guild,
                "channel_id": channel,
                "message_id": reply,
                "type": 0,
            });
            // As Discord's client asks for a reply with its mention off:
            // everyone the text mentions, but not the author replied to.
            if !options.ping_reply {
                body["allowed_mentions"] = json!({
                    "parse": ["users", "roles", "everyone"],
                    "replied_user": false,
                });
            }
        }
        let path = format!("/channels/{channel}/messages");
        self.json(with_json(self.request(Method::POST, &path), &body))
            .await
    }

    /// Posts a poll, a message with nothing else.
    pub async fn send_poll(&self, channel: &str, poll: &NewPoll, nonce: &str) -> Result<Value> {
        let mut body = message_body("", nonce);
        body["poll"] = poll.body();
        let path = format!("/channels/{channel}/messages");
        self.json(with_json(self.request(Method::POST, &path), &body))
            .await
    }

    /// Sends a message of text alone from somewhere other than the
    /// composer, saying where as Discord's client does: `context_menu` for
    /// the link Invite to Server sends, `forwarding` for the message added
    /// to a forward.
    pub async fn send_from(
        &self,
        place: &str,
        channel: &str,
        content: &str,
        nonce: &str,
        silent: bool,
    ) -> Result<Value> {
        let mut body = message_body(content, nonce);
        if silent {
            body["flags"] = SUPPRESS_NOTIFICATIONS.into();
        }
        let path = format!("/channels/{channel}/messages");
        let request = with_json(self.request(Method::POST, &path), &body)
            .header("X-Context-Properties", context(place));
        self.json(request).await
    }

    /// Forwards message `id` of `channel`, in server `guild` if any, to
    /// `target`, as Discord's client does: a message with no content of its
    /// own whose reference is a forward. Discord copies the original into
    /// it.
    pub async fn forward(
        &self,
        guild: Option<&str>,
        channel: &str,
        id: &str,
        target: &str,
        nonce: &str,
        silent: bool,
    ) -> Result<Value> {
        let mut body = message_body("", nonce);
        if silent {
            body["flags"] = SUPPRESS_NOTIFICATIONS.into();
        }
        body["message_reference"] = json!({
            "guild_id": guild,
            "channel_id": channel,
            "message_id": id,
            "type": FORWARD,
        });
        let path = format!("/channels/{target}/messages");
        let request = with_json(self.request(Method::POST, &path), &body)
            .header("X-Context-Properties", context("forwarding"));
        self.json(request).await
    }

    /// A user's profile, with what they share with this account, and their
    /// profile in `guild`.
    pub async fn profile(&self, user: &str, guild: Option<&str>) -> Result<Value> {
        let mut path = format!(
            "/users/{user}/profile?with_mutual_guilds=true&with_mutual_friends=true&with_mutual_friends_count=false"
        );
        if let Some(guild) = guild {
            path.push_str(&format!("&guild_id={guild}"));
        }
        self.json(self.request(Method::GET, &path)).await
    }

    /// Applications' public objects, with their names and icons, as the
    /// client fetches them for activities: `[{id, name, icon, ...}]`.
    pub async fn public_applications(&self, ids: &[&str]) -> Result<Value> {
        let query: Vec<_> = ids
            .iter()
            .map(|id| format!("application_ids={id}"))
            .collect();
        let path = format!("/applications/public?{}", query.join("&"));
        self.json(self.request(Method::GET, &path)).await
    }

    /// The message that each of up to 25 message requests is waiting with,
    /// as Discord's requests list shows it: `[{channel_id, message_preview}]`.
    pub async fn request_previews(&self, channels: &[String]) -> Result<Value> {
        let query: Vec<_> = channels
            .iter()
            .map(|channel| format!("channel_ids={channel}"))
            .collect();
        let path = format!(
            "/users/@me/message-requests/supplemental-data?{}",
            query.join("&")
        );
        self.json(self.request(Method::GET, &path)).await
    }

    /// Accepts a message request, which makes it an ordinary DM.
    pub async fn accept_request(&self, channel: &str) -> Result<()> {
        let path = format!("/channels/{channel}/recipients/@me");
        // 2 is ACCEPTED.
        let body = json!({"consent_status": 2});
        self.empty(with_json(self.request(Method::PUT, &path), &body))
            .await
    }

    /// Ignores a message request: it leaves this account's DMs, and the
    /// sender isn't told.
    pub async fn ignore_request(&self, channel: &str) -> Result<()> {
        let path = format!("/channels/{channel}/recipients/@me");
        self.empty(self.request(Method::DELETE, &path)).await
    }

    /// Uploads a file to Discord's storage, as the official client does
    /// before sending a message with it.
    pub async fn upload(&self, channel: &str, filename: &str, data: Vec<u8>) -> Result<Upload> {
        let path = format!("/channels/{channel}/attachments");
        let body = json!({"files": [{
            "id": "0",
            "filename": filename,
            "file_size": data.len(),
            "is_clip": false,
        }]});
        let answer = self
            .json(with_json(self.request(Method::POST, &path), &body))
            .await?;
        let target = &answer["attachments"][0];
        let (Some(url), Some(uploaded_filename)) = (
            target["upload_url"].as_str(),
            target["upload_filename"].as_str(),
        ) else {
            bail!("could not understand Discord's answer");
        };
        // The upload URL is signed, so it takes no login.
        let response = self
            .http
            .put(url)
            .body(data)
            .send()
            .await
            .context("Can't reach Discord's file storage.")?;
        if !response.status().is_success() {
            bail!("Discord's file storage answered {}.", response.status());
        }
        Ok(Upload {
            filename: filename.to_owned(),
            uploaded_filename: uploaded_filename.to_owned(),
        })
    }

    /// Removes an uploaded file that no message used.
    pub async fn discard(&self, upload: &Upload) -> Result<()> {
        let name: Vec<_> = upload.uploaded_filename.split('/').map(encode).collect();
        let path = format!("/attachments/{}", name.join("/"));
        self.empty(self.request(Method::DELETE, &path)).await
    }

    pub async fn edit(&self, channel: &str, message: &str, content: &str) -> Result<Value> {
        let path = format!("/channels/{channel}/messages/{message}");
        let body = json!({"content": content});
        self.json(with_json(self.request(Method::PATCH, &path), &body))
            .await
    }

    /// Deletes a message. One Discord no longer has is as good as deleted:
    /// another client, or a moderator, got there first.
    pub async fn delete(&self, channel: &str, message: &str) -> Result<()> {
        let path = format!("/channels/{channel}/messages/{message}");
        match self.empty(self.request(Method::DELETE, &path)).await {
            Err(error)
                if error
                    .downcast_ref::<Refused>()
                    .is_some_and(|refused| refused.0 == StatusCode::NOT_FOUND) =>
            {
                Ok(())
            }
            deleted => deleted,
        }
    }

    /// Marks a channel read up to `message`, as the official client does
    /// when a channel is on screen.
    pub async fn ack(&self, channel: &str, message: &str) -> Result<()> {
        let path = format!("/channels/{channel}/messages/{message}/ack");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        // Days since Discord's epoch.
        let days = (crate::message::snowflake_at(now) >> 22) / 86_400_000;
        let body = json!({"token": null, "last_viewed": days});
        self.json(with_json(self.request(Method::POST, &path), &body))
            .await
            .map(drop)
    }

    /// Marks channels read, each up to its message, in one request, as the
    /// official client does for a whole server; at most [`ACK_BULK_LIMIT`].
    pub async fn ack_bulk(&self, reads: &[(String, String)]) -> Result<()> {
        let read_states: Vec<Value> = reads
            .iter()
            .map(|(channel, message)| json!({"channel_id": channel, "message_id": message}))
            .collect();
        let body = json!({"read_states": read_states});
        self.empty(with_json(
            self.request(Method::POST, "/read-states/ack-bulk"),
            &body,
        ))
        .await
    }

    /// Adds or removes this account's reaction; `emoji` is the emoji itself
    /// or `name:id` for a custom one.
    pub async fn react(&self, channel: &str, message: &str, emoji: &str, add: bool) -> Result<()> {
        let path = format!(
            "/channels/{channel}/messages/{message}/reactions/{}/@me?location=Message&type=0",
            encode(emoji)
        );
        let method = if add { Method::PUT } else { Method::DELETE };
        self.empty(self.request(method, &path)).await
    }

    /// Removes `user`'s reaction to a message, which takes Manage Messages
    /// for anyone else's; `emoji` is as [`Self::react`] takes it.
    pub async fn remove_reaction(
        &self,
        channel: &str,
        message: &str,
        emoji: &str,
        user: &str,
    ) -> Result<()> {
        let path = format!(
            "/channels/{channel}/messages/{message}/reactions/{}/0/{user}?location=Message&burst=false",
            encode(emoji)
        );
        self.empty(self.request(Method::DELETE, &path)).await
    }

    /// Who reacted to a message with `emoji` (itself, or `name:id` for a
    /// custom one), as user objects: up to `limit`, by user id, after the
    /// user `after`. Super reactions aren't among them.
    pub async fn reactors(
        &self,
        channel: &str,
        message: &str,
        emoji: &str,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Value>> {
        let mut path = format!(
            "/channels/{channel}/messages/{message}/reactions/{}?limit={limit}&type=0",
            encode(emoji)
        );
        if let Some(after) = after {
            path.push_str(&format!("&after={after}"));
        }
        match self.json(self.request(Method::GET, &path)).await? {
            Value::Array(users) => Ok(users),
            _ => bail!("could not understand Discord's answer"),
        }
    }

    /// Votes for `answers` of a message's poll in place of this account's
    /// earlier vote; none takes the vote back.
    pub async fn vote(&self, channel: &str, message: &str, answers: &[u64]) -> Result<()> {
        let path = format!("/channels/{channel}/polls/{message}/answers/@me");
        let body = json!({"answer_ids": answers});
        self.empty(with_json(self.request(Method::PUT, &path), &body))
            .await
    }

    /// Closes this account's poll now. Discord sends the message again,
    /// closed, as a MESSAGE_UPDATE.
    pub async fn end_poll(&self, channel: &str, message: &str) -> Result<()> {
        let path = format!("/channels/{channel}/polls/{message}/expire");
        self.empty(self.request(Method::POST, &path)).await
    }

    /// Who voted for `answer` of a message's poll, as user objects: up to
    /// `limit`, at most 100, by user id, after the user `after`.
    pub async fn voters(
        &self,
        channel: &str,
        message: &str,
        answer: u64,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Value>> {
        let mut path =
            format!("/channels/{channel}/polls/{message}/answers/{answer}?limit={limit}");
        if let Some(after) = after {
            path.push_str(&format!("&after={after}"));
        }
        match self.json(self.request(Method::GET, &path)).await? {
            Value::Object(mut answer) => match answer.remove("users") {
                Some(Value::Array(users)) => Ok(users),
                _ => bail!("could not understand Discord's answer"),
            },
            _ => bail!("could not understand Discord's answer"),
        }
    }

    /// Opens the DM with `user`, or finds the one already open; returns its channel id.
    pub async fn open_dm(&self, user: &str) -> Result<String> {
        let body = json!({"recipients": [user]});
        let channel = self
            .json(with_json(
                self.request(Method::POST, "/users/@me/channels"),
                &body,
            ))
            .await?;
        channel["id"]
            .as_str()
            .map(str::to_owned)
            .context("Discord didn't open the DM.")
    }

    /// Makes a group DM with `recipients`, as adding friends to a one-to-one
    /// DM does, which stays as it was; returns its channel id.
    pub async fn create_group(&self, recipients: &[String]) -> Result<String> {
        let body = json!({"recipients": recipients});
        let channel = self
            .json(with_json(
                self.request(Method::POST, "/users/@me/channels"),
                &body,
            ))
            .await?;
        channel["id"]
            .as_str()
            .map(str::to_owned)
            .context("Discord didn't make the group.")
    }

    /// Adds `user` to a group DM.
    pub async fn add_recipient(&self, channel: &str, user: &str) -> Result<()> {
        let path = format!("/channels/{channel}/recipients/{user}");
        self.empty(with_json(self.request(Method::PUT, &path), &json!({})))
            .await
    }

    /// Pins a message to its channel's pinned messages, or unpins it.
    pub async fn pin(&self, channel: &str, message: &str, pinned: bool) -> Result<()> {
        let path = format!("/channels/{channel}/messages/pins/{message}");
        let method = if pinned { Method::PUT } else { Method::DELETE };
        self.empty(self.request(method, &path)).await
    }

    /// A channel's pinned messages, the latest pinned first: up to [`PAGE`].
    pub async fn pins(&self, channel: &str) -> Result<Vec<Value>> {
        let path = format!("/channels/{channel}/messages/pins?limit={PAGE}");
        let mut answer = self.json(self.request(Method::GET, &path)).await?;
        match answer["items"].take() {
            Value::Array(items) => Ok(items
                .into_iter()
                .map(|mut item| item["message"].take())
                .collect()),
            _ => bail!("could not understand Discord's answer"),
        }
    }

    /// Discord's answer to a search of messages, the newest first, from the
    /// `offset`th; see [`crate::service::Found`]. With a server, it searches
    /// the server, in the channels `params` names with `channel_id`, or all
    /// of them; without one, the DM `channel`. `params` are the rest of the
    /// query string, such as `content`, `author_id` and `has`, each name
    /// repeated for several values, as Discord's client sends them.
    pub async fn search(
        &self,
        guild: Option<&str>,
        channel: &str,
        params: &[(&str, String)],
        offset: u32,
    ) -> Result<Value> {
        let mut query = String::new();
        for (name, value) in params {
            query.push_str(&format!("{name}={}&", encode(value)));
        }
        query.push_str(&format!("offset={offset}"));
        let path = match guild {
            Some(guild) => format!("/guilds/{guild}/messages/search?{query}"),
            None => format!("/channels/{channel}/messages/search?{query}"),
        };
        self.json(self.request(Method::GET, &path)).await
    }

    /// Discord's answer to a search of a channel's threads, as its client's
    /// list of threads asks: the archived ones, the newest message first,
    /// [`THREAD_PAGE`] from the `offset`th; or with a `name`, those of
    /// either kind it matches.
    pub async fn search_threads(
        &self,
        channel: &str,
        name: Option<&str>,
        offset: u32,
    ) -> Result<Value> {
        let path = match name {
            Some(name) => format!(
                "/channels/{channel}/threads/search?name={}&tag_setting=match_some",
                encode(name)
            ),
            None => format!(
                "/channels/{channel}/threads/search?archived=true&sort_by=last_message_time\
                 &sort_order=desc&limit={THREAD_PAGE}&tag_setting=match_some&offset={offset}"
            ),
        };
        self.json(self.request(Method::GET, &path)).await
    }

    /// Starts a thread in `channel`, from its `message` or from none, with
    /// `body` as Discord's client sends it; the thread's channel object.
    /// Discord sends THREAD_CREATE, and adds this account to it.
    pub async fn create_thread(
        &self,
        channel: &str,
        message: Option<&str>,
        body: &Value,
    ) -> Result<Value> {
        let path = match message {
            Some(message) => format!("/channels/{channel}/messages/{message}/threads"),
            None => format!("/channels/{channel}/threads"),
        };
        self.json(with_json(self.request(Method::POST, &path), body))
            .await
    }

    /// Posts in a forum, as Discord's client does: a thread named `title`,
    /// archiving after `archive_after` minutes of quiet, with the forum's
    /// tags by id and a first message of `content` and uploaded files. The
    /// post's channel object; Discord sends THREAD_CREATE and the message.
    pub async fn create_post(
        &self,
        forum: &str,
        title: &str,
        archive_after: u16,
        tags: &[String],
        content: &str,
        uploads: &[Upload],
    ) -> Result<Value> {
        let path = format!("/channels/{forum}/threads?use_nested_fields=true");
        self.json(with_json(
            self.request(Method::POST, &path),
            &post_body(title, archive_after, tags, content, uploads),
        ))
        .await
    }

    /// The first and newest messages of a forum's posts, and who started
    /// them, by post: `{"threads": {id: {"first_message", ...}}}`. Discord's
    /// client asks for at most [`POSTS_PER_ASK`] at a time.
    pub async fn post_data(&self, forum: &str, posts: &[&str]) -> Result<Value> {
        let path = format!("/channels/{forum}/post-data");
        let body = serde_json::json!({ "thread_ids": posts });
        self.json(with_json(self.request(Method::POST, &path), &body))
            .await
    }

    /// Joins a thread, or leaves it with `join` false, as Discord's client
    /// does: a POST to join, though the docs say PUT, and `location` saying
    /// where in the client, such as "Context Menu". Discord answers with
    /// THREAD_MEMBERS_UPDATE.
    pub async fn thread_member(&self, thread: &str, join: bool, location: &str) -> Result<()> {
        let path = format!(
            "/channels/{thread}/thread-members/@me?location={}",
            encode(location)
        );
        let method = if join { Method::POST } else { Method::DELETE };
        self.empty(self.request(method, &path)).await
    }

    /// Shows others that this account is typing in `channel`.
    pub async fn typing(&self, channel: &str) -> Result<()> {
        let path = format!("/channels/{channel}/typing");
        self.empty(self.request(Method::POST, &path)).await
    }

    /// Sends `user` a friend request, or accepts theirs. `place` is where in
    /// Discord's client this happens, which it tells Discord with each change
    /// to a relationship, such as "User Profile" or "Friends".
    pub async fn add_friend(&self, user: &str, place: &str) -> Result<()> {
        let path = format!("/users/@me/relationships/{user}");
        let request = with_json(self.request(Method::PUT, &path), &json!({}));
        self.empty(request.header("X-Context-Properties", context(place)))
            .await
    }

    /// Sends a friend request to whoever has `username`.
    pub async fn add_friend_by_username(&self, username: &str) -> Result<()> {
        let body = json!({"username": username, "discriminator": null});
        let request = with_json(
            self.request(Method::POST, "/users/@me/relationships"),
            &body,
        );
        self.empty(request.header("X-Context-Properties", context("Add Friend")))
            .await
    }

    /// Sets the custom status, or clears it with `None`, in the account's settings.
    pub async fn set_custom_status(&self, custom: Option<&CustomStatus>) -> Result<()> {
        let body = json!({"settings": settings::write_custom(custom)});
        self.json(with_json(
            self.request(Method::PATCH, "/users/@me/settings-proto/1"),
            &body,
        ))
        .await
        .map(drop)
    }

    /// The GIF picker's home: trending terms, each with a GIF, and trending GIFs.
    pub async fn gifs_trending(&self) -> Result<Value> {
        let path = format!(
            "/gifs/trending?media_format={}&locale={}",
            crate::gifs::MEDIA_FORMAT,
            crate::gifs::LOCALE
        );
        self.json(self.request(Method::GET, &path)).await
    }

    /// The GIFs trending now, for the picker's "Trending GIFs".
    pub async fn gifs_trending_gifs(&self) -> Result<Value> {
        let path = format!(
            "/gifs/trending-gifs?media_format={}&locale={}",
            crate::gifs::MEDIA_FORMAT,
            crate::gifs::LOCALE
        );
        self.json(self.request(Method::GET, &path)).await
    }

    /// GIFs for `query`, as many as Discord gives at once.
    pub async fn gifs_search(&self, query: &str) -> Result<Value> {
        let path = format!(
            "/gifs/search?q={}&media_format={}&locale={}",
            encode(query),
            crate::gifs::MEDIA_FORMAT,
            crate::gifs::LOCALE
        );
        self.json(self.request(Method::GET, &path)).await
    }

    /// Terms like `query` to search for instead.
    pub async fn gifs_suggest(&self, query: &str, limit: u32) -> Result<Value> {
        let path = format!(
            "/gifs/suggest?q={}&limit={limit}&locale={}",
            encode(query),
            crate::gifs::LOCALE
        );
        self.json(self.request(Method::GET, &path)).await
    }

    /// Tells the provider which GIF a search for `query` led to.
    pub async fn gif_select(&self, id: &str, query: &str) -> Result<()> {
        let body = json!({"id": id, "q": query});
        self.empty(with_json(self.request(Method::POST, "/gifs/select"), &body))
            .await
    }

    /// The account's settings, `PreloadedUserSettings`, in base64.
    pub async fn user_settings(&self) -> Result<String> {
        let answer = self
            .json(self.request(Method::GET, "/users/@me/settings-proto/1"))
            .await?;
        Ok(answer["settings"].as_str().unwrap_or_default().to_owned())
    }

    /// Replaces the top-level fields `settings`, base64
    /// `PreloadedUserSettings`, holds, each whole.
    pub async fn set_user_settings(&self, settings: &str) -> Result<()> {
        let body = json!({"settings": settings});
        self.json(with_json(
            self.request(Method::PATCH, "/users/@me/settings-proto/1"),
            &body,
        ))
        .await
        .map(drop)
    }

    /// The account's second settings, `FrecencyUserSettings`, in base64.
    pub async fn frecency_settings(&self) -> Result<String> {
        let answer = self
            .json(self.request(Method::GET, "/users/@me/settings-proto/2"))
            .await?;
        Ok(answer["settings"].as_str().unwrap_or_default().to_owned())
    }

    /// Replaces the fields `settings`, base64 `FrecencyUserSettings`, holds.
    pub async fn set_frecency_settings(&self, settings: &str) -> Result<()> {
        let body = json!({"settings": settings});
        self.json(with_json(
            self.request(Method::PATCH, "/users/@me/settings-proto/2"),
            &body,
        ))
        .await
        .map(drop)
    }

    /// Sets the status: `online`, `idle`, `dnd` or `invisible`.
    pub async fn set_status(&self, status: &str) -> Result<()> {
        let body = json!({"settings": settings::write_status(status)});
        self.json(with_json(
            self.request(Method::PATCH, "/users/@me/settings-proto/1"),
            &body,
        ))
        .await
        .map(drop)
    }

    /// Shares the game this account plays, everywhere it's signed in, or
    /// stops sharing it.
    pub async fn set_show_game(&self, shown: bool) -> Result<()> {
        let body = json!({"settings": settings::write_show_game(shown)});
        self.json(with_json(
            self.request(Method::PATCH, "/users/@me/settings-proto/1"),
            &body,
        ))
        .await
        .map(drop)
    }

    /// Ignores a friend request, cancels one, or ends a friendship.
    pub async fn remove_relationship(&self, user: &str, place: &str) -> Result<()> {
        let path = format!("/users/@me/relationships/{user}");
        let request = self.request(Method::DELETE, &path);
        self.empty(request.header("X-Context-Properties", context(place)))
            .await
    }

    /// Blocks `user`, which ends a friendship or friend request with them.
    pub async fn block(&self, user: &str, place: &str) -> Result<()> {
        let path = format!("/users/@me/relationships/{user}");
        let request = with_json(self.request(Method::PUT, &path), &json!({"type": 2}));
        self.empty(request.header("X-Context-Properties", context(place)))
            .await
    }

    /// Ignores `user`, or stops ignoring them: Discord hides their messages
    /// and quiets their DMs, and they aren't told.
    pub async fn ignore(&self, user: &str, ignore: bool) -> Result<()> {
        let path = format!("/users/@me/relationships/{user}/ignore");
        let method = if ignore { Method::PUT } else { Method::DELETE };
        self.empty(self.request(method, &path)).await
    }

    /// Sets the nickname this account gives a friend, or clears it with an
    /// empty one.
    pub async fn set_friend_nickname(&self, user: &str, nickname: &str) -> Result<()> {
        let path = format!("/users/@me/relationships/{user}");
        let nickname = (!nickname.is_empty()).then_some(nickname);
        let body = json!({"nickname": nickname});
        self.empty(with_json(self.request(Method::PATCH, &path), &body))
            .await
    }

    /// This account's note on `user`; empty when it has none.
    pub async fn note(&self, user: &str) -> Result<String> {
        let path = format!("/users/@me/notes/{user}");
        match self.json(self.request(Method::GET, &path)).await {
            Ok(answer) => Ok(answer["note"].as_str().unwrap_or_default().to_owned()),
            // Discord keeps no empty notes.
            Err(error)
                if matches!(
                    error.downcast_ref::<Refused>(),
                    Some(Refused(StatusCode::NOT_FOUND))
                ) =>
            {
                Ok(String::new())
            }
            Err(error) => Err(error),
        }
    }

    /// Sets this account's note on `user`, or removes it with an empty one.
    pub async fn set_note(&self, user: &str, note: &str) -> Result<()> {
        let path = format!("/users/@me/notes/{user}");
        let body = json!({"note": note});
        self.empty(with_json(self.request(Method::PUT, &path), &body))
            .await
    }

    /// Closes a DM, which leaves the DM list until either side writes again.
    pub async fn close_dm(&self, channel: &str) -> Result<()> {
        let path = format!("/channels/{channel}");
        self.json(self.request(Method::DELETE, &path))
            .await
            .map(drop)
    }

    /// Leaves a group DM; with `silent`, as its dialog's "Leave without
    /// notifying other members" does, Discord posts no message that this
    /// account left. Discord's client sends `silent` either way.
    pub async fn leave_group(&self, channel: &str, silent: bool) -> Result<()> {
        let path = format!("/channels/{channel}?silent={silent}");
        self.json(self.request(Method::DELETE, &path))
            .await
            .map(drop)
    }

    /// Changes this account's notification settings for a server, or with
    /// [`crate::ready::PRIVATE`] its DMs, by `body`, a partial settings
    /// object; see [`crate::notifications::Change::body`]. Discord answers
    /// with USER_GUILD_SETTINGS_UPDATE too.
    pub async fn guild_settings(&self, guild: &str, body: &Value) -> Result<()> {
        let path = format!("/users/@me/guilds/{guild}/settings");
        self.json(with_json(self.request(Method::PATCH, &path), body))
            .await
            .map(drop)
    }

    /// Changes this account's settings for one or more servers, never its
    /// DMs, by `body`, `{"guilds": {"<id>": <partial settings>}}`, as
    /// Discord's client does; see [`crate::notifications::flags_body`].
    /// Discord answers with USER_GUILD_SETTINGS_UPDATE for each server.
    pub async fn guilds_settings(&self, body: &Value) -> Result<()> {
        self.json(with_json(
            self.request(Method::PATCH, "/users/@me/guilds/settings"),
            body,
        ))
        .await
        .map(drop)
    }

    // A server member's menu: nicknames, roles and moderation. Each fires
    // GUILD_MEMBER_UPDATE, or for a kick GUILD_MEMBER_REMOVE and a ban
    // GUILD_BAN_ADD; see <https://docs.discord.food/resources/guild>.

    /// Sets the nickname of `user` in a server, or with none this account's
    /// own, which takes Change Nickname rather than Manage Nicknames; empty
    /// resets it.
    pub async fn set_member_nick(&self, guild: &str, user: Option<&str>, nick: &str) -> Result<()> {
        let path = format!("/guilds/{guild}/members/{}", user.unwrap_or("@me"));
        let body = json!({"nick": (!nick.is_empty()).then_some(nick)});
        self.empty(with_json(self.request(Method::PATCH, &path), &body))
            .await
    }

    /// Changes this account's member object in a server: its nickname and
    /// server avatar, as Discord's per-server profile editor sends them.
    /// Discord answers with the member, and GUILD_MEMBER_UPDATE.
    pub async fn modify_member_me(&self, guild: &str, body: &Value) -> Result<Value> {
        let path = format!("/guilds/{guild}/members/@me");
        self.json(with_json(self.request(Method::PATCH, &path), body))
            .await
    }

    /// Changes this account's profile in a server: its banner, bio and
    /// pronouns. Discord answers with the profile, and no gateway event.
    pub async fn modify_guild_profile(&self, guild: &str, body: &Value) -> Result<Value> {
        let path = format!("/guilds/{guild}/profile/@me");
        self.json(with_json(self.request(Method::PATCH, &path), body))
            .await
    }

    /// Turns a confirmation into the token a request held back for
    /// two-factor authentication goes again with: `data` is the code, the
    /// password or the backup code, by `method`.
    pub async fn finish_mfa(
        &self,
        ticket: &str,
        method: mfa::Method,
        data: &str,
    ) -> Result<String> {
        let body = json!({"ticket": ticket, "mfa_type": method.name(), "data": data});
        let answer = self
            .json(with_json(self.request(Method::POST, "/mfa/finish"), &body))
            .await?;
        answer["token"]
            .as_str()
            .map(str::to_owned)
            .context("Discord gave no token for the confirmation")
    }

    /// Has Discord text a code for a ticket; it answers with the phone
    /// number it went to, partly hidden.
    pub async fn send_mfa_sms(&self, ticket: &str) -> Result<String> {
        let body = json!({"ticket": ticket});
        let answer = self
            .json(with_json(
                self.request(Method::POST, "/auth/mfa/sms/send"),
                &body,
            ))
            .await?;
        Ok(answer["phone"].as_str().unwrap_or_default().to_owned())
    }

    /// A server's profile, as its settings' Server Profile page shows it:
    /// its name, icon and description, among more.
    pub async fn server_profile(&self, guild: &str) -> Result<Value> {
        let path = format!("/guilds/{guild}/profile");
        self.json(self.request(Method::GET, &path)).await
    }

    /// Changes a server's profile, as Server Profile's Save Changes does;
    /// it takes Manage Server. Discord answers with the profile, and
    /// GUILD_UPDATE.
    pub async fn modify_server_profile(&self, guild: &str, body: &Value) -> Result<Value> {
        let path = format!("/guilds/{guild}/profile");
        self.json(with_json(self.request(Method::PATCH, &path), body))
            .await
    }

    /// A server's guild object, with its settings.
    pub async fn guild(&self, guild: &str) -> Result<Value> {
        self.json(self.request(Method::GET, &format!("/guilds/{guild}")))
            .await
    }

    /// Changes a server's settings, as Discord's Engagement, Boost Perks and
    /// Safety Setup pages do; it takes Manage Server. Discord answers with
    /// the guild, and GUILD_UPDATE.
    pub async fn modify_guild(&self, guild: &str, body: &Value) -> Result<Value> {
        let path = format!("/guilds/{guild}");
        self.json(with_json(self.request(Method::PATCH, &path), body))
            .await
    }

    /// Up to [`BANS_PAGE`] of a server's bans, after a user's id, as Bans
    /// fetches them; it takes Ban Members or View Audit Log.
    pub async fn bans(&self, guild: &str, after: Option<&str>) -> Result<Value> {
        let mut path = format!("/guilds/{guild}/bans?limit={BANS_PAGE}");
        if let Some(after) = after {
            path.push_str(&format!("&after={after}"));
        }
        self.json(self.request(Method::GET, &path)).await
    }

    /// A server's bans of users named like `query`, or by id, up to 10,
    /// as Bans' search asks; `user_ids` repeats for each.
    pub async fn search_bans(
        &self,
        guild: &str,
        query: Option<&str>,
        user_ids: &[String],
    ) -> Result<Value> {
        let mut path = format!("/guilds/{guild}/bans/search?limit=10");
        if let Some(query) = query {
            path.push_str(&format!("&query={}", encode(query)));
        }
        for id in user_ids {
            path.push_str(&format!("&user_ids={}", encode(id)));
        }
        self.json(self.request(Method::GET, &path)).await
    }

    /// Lifts a ban, as Bans' Revoke Ban does, without a reason; Discord
    /// sends GUILD_BAN_REMOVE.
    pub async fn unban(&self, guild: &str, user: &str) -> Result<()> {
        let path = format!("/guilds/{guild}/bans/{user}");
        self.empty(self.request(Method::DELETE, &path)).await
    }

    /// A page of a server's audit log, newest first: before an entry's id,
    /// by one user, or of one action, as Audit Log asks; it takes View
    /// Audit Log.
    pub async fn audit_log(
        &self,
        guild: &str,
        before: Option<&str>,
        user: Option<&str>,
        action: Option<u16>,
    ) -> Result<Value> {
        let mut path = format!("/guilds/{guild}/audit-logs?limit={AUDIT_PAGE}");
        if let Some(before) = before {
            path.push_str(&format!("&before={}", encode(before)));
        }
        if let Some(user) = user {
            path.push_str(&format!("&user_id={}", encode(user)));
        }
        if let Some(action) = action {
            path.push_str(&format!("&action_type={action}"));
        }
        self.json(self.request(Method::GET, &path)).await
    }

    /// Members of a server as Members in its settings searches them, a
    /// chunk at a time: `body` is Discord's `members-search` query. Discord
    /// answers 202 while it indexes the server, with how many seconds to
    /// wait before asking again.
    pub async fn search_members(&self, guild: &str, body: &Value) -> Result<MembersSearch> {
        let path = format!("/guilds/{guild}/members-search");
        let response = self
            .execute(with_json(self.request(Method::POST, &path), body))
            .await?;
        let indexing = response.status() == StatusCode::ACCEPTED;
        let body = response
            .bytes()
            .await
            .context("could not read Discord's answer")?;
        let body: Value =
            serde_json::from_slice(&body).context("could not understand Discord's answer")?;
        Ok(if indexing {
            MembersSearch::Indexing(body["retry_after"].as_f64().unwrap_or(5.0))
        } else {
            MembersSearch::Found(body)
        })
    }

    /// A server's invites, as Invites in its settings lists them; it takes
    /// Manage Server.
    pub async fn guild_invites(&self, guild: &str) -> Result<Value> {
        let path = format!("/guilds/{guild}/invites");
        self.json(self.request(Method::GET, &path)).await
    }

    /// Revokes an invite, as Invites' X does.
    pub async fn revoke_invite(&self, code: &str) -> Result<()> {
        let path = format!("/invites/{}", encode(code));
        self.empty(self.request(Method::DELETE, &path)).await
    }

    /// A server's custom emoji, each with who uploaded it, as Emoji in its
    /// settings lists them.
    pub async fn guild_emojis(&self, guild: &str) -> Result<Value> {
        let path = format!("/guilds/{guild}/emojis");
        self.json(self.request(Method::GET, &path)).await
    }

    /// Adds a custom emoji: `image` is a data URI. Discord answers with
    /// the emoji.
    pub async fn create_emoji(&self, guild: &str, name: &str, image: &str) -> Result<Value> {
        let path = format!("/guilds/{guild}/emojis");
        let body = json!({"name": name, "image": image});
        self.json(with_json(self.request(Method::POST, &path), &body))
            .await
    }

    /// Renames a custom emoji, as Emoji's name field does.
    pub async fn rename_emoji(&self, guild: &str, emoji: &str, name: &str) -> Result<Value> {
        let path = format!("/guilds/{guild}/emojis/{emoji}");
        let body = json!({ "name": name });
        self.json(with_json(self.request(Method::PATCH, &path), &body))
            .await
    }

    pub async fn delete_emoji(&self, guild: &str, emoji: &str) -> Result<()> {
        let path = format!("/guilds/{guild}/emojis/{emoji}");
        self.empty(self.request(Method::DELETE, &path)).await
    }

    /// A server's stickers, each with who uploaded it.
    pub async fn guild_stickers(&self, guild: &str) -> Result<Value> {
        let path = format!("/guilds/{guild}/stickers");
        self.json(self.request(Method::GET, &path)).await
    }

    /// Uploads a sticker as Discord's client does: a form with `name`,
    /// `tags`, `description` and the file, in that order. Discord answers
    /// with the sticker.
    pub async fn create_sticker(
        &self,
        guild: &str,
        fields: [(&str, &str); 3],
        file: (&str, &str, &[u8]),
    ) -> Result<Value> {
        let path = format!("/guilds/{guild}/stickers");
        let (content_type, body) = form(&fields, file);
        self.json(
            self.request(Method::POST, &path)
                .header(header::CONTENT_TYPE, content_type)
                .body(body),
        )
        .await
    }

    /// Changes a sticker's name, related emoji and description; its file
    /// stays.
    pub async fn edit_sticker(&self, guild: &str, sticker: &str, body: &Value) -> Result<Value> {
        let path = format!("/guilds/{guild}/stickers/{sticker}");
        self.json(with_json(self.request(Method::PATCH, &path), body))
            .await
    }

    pub async fn delete_sticker(&self, guild: &str, sticker: &str) -> Result<()> {
        let path = format!("/guilds/{guild}/stickers/{sticker}");
        self.empty(self.request(Method::DELETE, &path)).await
    }

    /// Pauses a server's invites or DMs until a time, or resumes them, as
    /// Security Actions does: `body` has `invites_disabled_until`,
    /// `dms_disabled_until` and `lockdown_duration_hours`. Discord answers
    /// with the server's `incidents_data`, and GUILD_UPDATE.
    pub async fn incident_actions(&self, guild: &str, body: &Value) -> Result<Value> {
        let path = format!("/guilds/{guild}/incident-actions");
        self.json(with_json(self.request(Method::PUT, &path), body))
            .await
    }

    /// Whether a server's moderators need two-factor authentication, 1, or
    /// not, 0; only its owner may, with two-factor authentication on.
    /// Discord answers with `{level}`.
    pub async fn set_mfa_level(&self, guild: &str, level: u8) -> Result<Value> {
        let path = format!("/guilds/{guild}/mfa");
        self.json(with_json(
            self.request(Method::POST, &path),
            &json!({ "level": level }),
        ))
        .await
    }

    /// Deletes a server, as only its owner may.
    pub async fn delete_guild(&self, guild: &str) -> Result<()> {
        let path = format!("/guilds/{guild}/delete");
        self.empty(self.request(Method::POST, &path)).await
    }

    /// Gives a server to `owner`, with the code emailed for it, or none
    /// when two-factor authentication confirms it instead.
    pub async fn transfer_guild(&self, guild: &str, owner: &str, code: Option<&str>) -> Result<()> {
        let path = format!("/guilds/{guild}");
        self.empty(with_json(
            self.request(Method::PATCH, &path),
            &json!({ "owner_id": owner, "code": code }),
        ))
        .await
    }

    /// Has Discord email the owner a code to give the server away.
    pub async fn send_guild_pincode(&self, guild: &str) -> Result<()> {
        let path = format!("/guilds/{guild}/pincode");
        self.empty(self.request(Method::PUT, &path)).await
    }

    /// How many members hold each role, by role id.
    pub async fn role_member_counts(&self, guild: &str) -> Result<Value> {
        let path = format!("/guilds/{guild}/roles/member-counts");
        self.json(self.request(Method::GET, &path)).await
    }

    /// Makes a role with `body`'s fields; it takes Manage Roles. Discord
    /// answers with the role, and GUILD_ROLE_CREATE.
    pub async fn create_role(&self, guild: &str, body: &Value) -> Result<Value> {
        let path = format!("/guilds/{guild}/roles");
        self.json(with_json(self.request(Method::POST, &path), body))
            .await
    }

    /// Changes a role with `body`'s fields. Discord answers with the role,
    /// and GUILD_ROLE_UPDATE.
    pub async fn modify_role(&self, guild: &str, role: &str, body: &Value) -> Result<Value> {
        let path = format!("/guilds/{guild}/roles/{role}");
        self.json(with_json(self.request(Method::PATCH, &path), body))
            .await
    }

    /// Moves roles, `[{"id", "position"}, ...]`. Discord answers with every
    /// role, and GUILD_ROLE_UPDATE for each moved.
    pub async fn move_roles(&self, guild: &str, positions: &Value) -> Result<Value> {
        let path = format!("/guilds/{guild}/roles");
        self.json(with_json(self.request(Method::PATCH, &path), positions))
            .await
    }

    /// Deletes a role, and GUILD_ROLE_DELETE follows.
    pub async fn delete_role(&self, guild: &str, role: &str) -> Result<()> {
        let path = format!("/guilds/{guild}/roles/{role}");
        self.empty(self.request(Method::DELETE, &path)).await
    }

    /// The ids of up to 100 members holding a role.
    pub async fn role_member_ids(&self, guild: &str, role: &str) -> Result<Value> {
        let path = format!("/guilds/{guild}/roles/{role}/member-ids");
        self.json(self.request(Method::GET, &path)).await
    }

    /// Gives a role to these members at once, as Add Members does.
    pub async fn add_role_members(
        &self,
        guild: &str,
        role: &str,
        members: &[String],
    ) -> Result<Value> {
        let path = format!("/guilds/{guild}/roles/{role}/members");
        let body = json!({ "member_ids": members });
        self.json(with_json(self.request(Method::PATCH, &path), &body))
            .await
    }

    /// Sets a member's roles, all of them, as Manage Members' remove does.
    pub async fn set_member_roles(&self, guild: &str, user: &str, roles: &[String]) -> Result<()> {
        let path = format!("/guilds/{guild}/members/{user}");
        let body = json!({ "roles": roles });
        self.empty(with_json(self.request(Method::PATCH, &path), &body))
            .await
    }

    /// Changes this account's user: its display name and avatar, as
    /// Discord's profile editor sends them. Discord answers with the user,
    /// and USER_UPDATE.
    pub async fn modify_me(&self, body: &Value) -> Result<Value> {
        self.json(with_json(self.request(Method::PATCH, "/users/@me"), body))
            .await
    }

    /// Changes this account's main profile: its bio, pronouns, banner and
    /// banner colour. Discord answers with the profile, and USER_UPDATE.
    pub async fn modify_profile_me(&self, body: &Value) -> Result<Value> {
        self.json(with_json(
            self.request(Method::PATCH, "/users/@me/profile"),
            body,
        ))
        .await
    }

    /// Changes a member's voice state, as the voice menu does: `mute` and
    /// `deaf` for the server's, and `channel_id` to move them, which with
    /// null disconnects them. Discord answers with the member, and
    /// VOICE_STATE_UPDATE.
    pub async fn modify_member_voice(&self, guild: &str, user: &str, body: &Value) -> Result<()> {
        let path = format!("/guilds/{guild}/members/{user}");
        self.empty(with_json(self.request(Method::PATCH, &path), body))
            .await
    }

    /// Gives a member a role, or with `add` false takes it away.
    pub async fn member_role(&self, guild: &str, user: &str, role: &str, add: bool) -> Result<()> {
        let path = format!("/guilds/{guild}/members/{user}/roles/{role}");
        let method = if add { Method::PUT } else { Method::DELETE };
        self.empty(self.request(method, &path)).await
    }

    /// Times a member out until `until`, an ISO 8601 time at most 28 days
    /// away, or with none ends their timeout.
    pub async fn time_out(
        &self,
        guild: &str,
        user: &str,
        until: Option<&str>,
        reason: &str,
    ) -> Result<()> {
        let path = format!("/guilds/{guild}/members/{user}");
        let body = json!({"communication_disabled_until": until});
        let request = with_json(self.request(Method::PATCH, &path), &body);
        self.empty(audit_reason(request, reason)).await
    }

    /// Removes a member from a server; they may join again with an invite.
    pub async fn kick(&self, guild: &str, user: &str, reason: &str) -> Result<()> {
        let path = format!("/guilds/{guild}/members/{user}");
        self.empty(audit_reason(self.request(Method::DELETE, &path), reason))
            .await
    }

    /// Bans a member, deleting what they wrote in the last `delete_seconds`,
    /// up to a week.
    pub async fn ban(
        &self,
        guild: &str,
        user: &str,
        delete_seconds: u32,
        reason: &str,
    ) -> Result<()> {
        let path = format!("/guilds/{guild}/bans/{user}");
        let body = json!({"delete_message_seconds": delete_seconds.min(MAX_BAN_DELETE)});
        let request = with_json(self.request(Method::PUT, &path), &body);
        self.empty(audit_reason(request, reason)).await
    }

    /// Makes an invite to a server's channel that lasts `max_age` seconds,
    /// 0 for good, and works `max_uses` times, 0 for any number, as
    /// Discord's invite dialog asks; its code.
    pub async fn create_invite(
        &self,
        channel: &str,
        max_age: u32,
        max_uses: u32,
    ) -> Result<String> {
        let path = format!("/channels/{channel}/invites");
        let body = json!({"max_age": max_age, "max_uses": max_uses, "temporary": false});
        self.invite(with_json(self.request(Method::POST, &path), &body))
            .await
    }

    /// Makes an invite to a server's channel for one person, as Discord's
    /// Invite to Server does: it works once and lasts Discord's default of
    /// a day, and `unique` asks for a new one rather than a similar one
    /// reused; its code.
    pub async fn create_invite_once(&self, channel: &str) -> Result<String> {
        let path = format!("/channels/{channel}/invites");
        let body = json!({"max_uses": 1, "unique": true});
        let request = with_json(self.request(Method::POST, &path), &body)
            .header("X-Context-Properties", context("User Invite Context Menu"));
        self.invite(request).await
    }

    /// The code of the invite a request made.
    async fn invite(&self, request: RequestBuilder) -> Result<String> {
        let response = self.execute(request).await?;
        // Discord answers 204, with nothing, when the server paused invites.
        if response.status() == StatusCode::NO_CONTENT {
            bail!("This server has paused invites.");
        }
        let invite: Value = serde_json::from_slice(
            &response
                .bytes()
                .await
                .context("could not read Discord's answer")?,
        )
        .context("could not understand Discord's answer")?;
        invite["code"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("Discord made no invite."))
    }

    /// An invite, as Discord's client asks before joining by it: with the
    /// server's member and online counts, when it expires and what joining
    /// would let this account do. `typed` is what someone typed into Join a
    /// Server, which the client sends along.
    pub async fn resolve_invite(&self, code: &str, typed: Option<&str>) -> Result<Value> {
        let mut path = format!(
            "/invites/{}?with_counts=true&with_expiration=true&with_permissions=true",
            encode(code)
        );
        if let Some(typed) = typed {
            path.push_str(&format!("&inputValue={}", encode(typed)));
        }
        self.json(self.request(Method::GET, &path)).await
    }

    /// A server's scheduled event, with how many people are interested
    /// in it, as Discord's client asks before showing it.
    pub async fn scheduled_event(&self, guild: &str, event: &str) -> Result<Value> {
        let path = format!("/guilds/{guild}/scheduled-events/{event}?with_user_count=true");
        self.json(self.request(Method::GET, &path)).await
    }

    /// This account's scheduled-event subscriptions in a server.
    pub async fn own_scheduled_events(&self, guild: &str) -> Result<Value> {
        let path = format!("/users/@me/scheduled-events?guild_ids={guild}");
        self.json(self.request(Method::GET, &path)).await
    }

    /// Marks this account interested in a server's scheduled event, or
    /// no longer interested in it.
    pub async fn set_scheduled_event_interest(
        &self,
        guild: &str,
        event: &str,
        interested: bool,
    ) -> Result<()> {
        let path = format!("/guilds/{guild}/scheduled-events/{event}/users/@me");
        let request = if interested {
            with_json(
                self.request(Method::PUT, &path),
                &json!({ "response": scheduled_event::INTERESTED }),
            )
        } else {
            self.request(Method::DELETE, &path)
        };
        self.empty(request).await
    }

    /// A gift code, with the subscription plan it grants, as Discord's
    /// client asks for a gift link in a message: without the SKU's
    /// application, which Yapfast doesn't show.
    pub async fn gift_code(&self, code: &str) -> Result<Value> {
        let path = format!(
            "/entitlements/gift-codes/{}?with_application=false&with_subscription_plan=true",
            encode(code)
        );
        self.json(self.request(Method::GET, &path)).await
    }

    /// Redeems a gift code for this account, in the channel its link was
    /// opened in when it was one, as Discord's client sends it without a
    /// payment source; the entitlement it grants.
    pub async fn redeem_gift_code(&self, code: &str, channel: Option<&str>) -> Result<Value> {
        let path = format!("/entitlements/gift-codes/{}/redeem", encode(code));
        let body = json!({"channel_id": channel, "gateway_checkout_context": null});
        self.json(with_json(self.request(Method::POST, &path), &body))
            .await
    }

    /// What authorizing an application would do, for the link whose
    /// query `query` is, as Discord's client asks before showing it.
    pub async fn oauth_preview(&self, query: &str) -> Result<Value> {
        let path = format!("/oauth2/authorize?{query}");
        self.json(self.request(Method::GET, &path)).await
    }

    /// Authorizes an application, for the link whose query `query` is,
    /// in the server `guild` when it installs to one, with a bot's
    /// `permissions`; the address Discord sends the person to next.
    pub async fn authorize_oauth(
        &self,
        query: &str,
        user_install: bool,
        guild: Option<&str>,
        permissions: Option<u64>,
    ) -> Result<Value> {
        let path = format!("/oauth2/authorize?{query}");
        let body = json!({
            "authorize": true,
            "integration_type": u64::from(user_install),
            "guild_id": guild,
            "permissions": permissions.map(|bits| bits.to_string()),
        });
        self.json(with_json(self.request(Method::POST, &path), &body))
            .await
    }

    /// Joins a server by an invite, from this gateway `session`, as
    /// Discord's client does; `context` says where, as
    /// [`crate::invite::Preview::context`] makes it. Discord may ask for a
    /// [`Captcha`] first, whose solution goes with the next try.
    pub async fn accept_invite(
        &self,
        code: &str,
        session: &str,
        context: &Value,
        solved: Option<(&Captcha, &str)>,
    ) -> Result<Value> {
        let path = format!("/invites/{}", encode(code));
        let body = json!({"session_id": session});
        let mut request = with_json(self.request(Method::POST, &path), &body)
            .header("X-Context-Properties", encoded_context(context));
        if let Some((captcha, solution)) = solved {
            for (name, value) in captcha.headers(solution) {
                request = request.header(name, value);
            }
        }
        self.json(request).await
    }

    /// A channel object, as `GET /channels/{id}` gives it.
    pub async fn channel(&self, channel: &str) -> Result<Value> {
        let path = format!("/channels/{channel}");
        self.json(self.request(Method::GET, &path)).await
    }

    /// Makes a channel or category in a server from `body`, a channel
    /// object's fields; the new channel object. Discord sends CHANNEL_CREATE.
    pub async fn create_channel(&self, guild: &str, body: &Value) -> Result<Value> {
        let path = format!("/guilds/{guild}/channels");
        self.json(with_json(self.request(Method::POST, &path), body))
            .await
    }

    /// Changes a channel's or category's fields that `body` names; Discord
    /// sends CHANNEL_UPDATE.
    pub async fn edit_channel(&self, channel: &str, body: &Value) -> Result<()> {
        let path = format!("/channels/{channel}");
        self.json(with_json(self.request(Method::PATCH, &path), body))
            .await
            .map(drop)
    }

    /// Moves a server's channels and categories, `[{"id", "position",
    /// "parent_id", "lock_permissions"}, ...]`, the last two optional;
    /// Discord sends CHANNEL_UPDATE for each that changed.
    pub async fn move_channels(&self, guild: &str, moves: &Value) -> Result<()> {
        let path = format!("/guilds/{guild}/channels");
        self.empty(with_json(self.request(Method::PATCH, &path), moves))
            .await
    }

    /// Deletes a server's channel or category for good; Discord sends
    /// CHANNEL_DELETE, and moves a category's channels out of it.
    pub async fn delete_channel(&self, channel: &str) -> Result<()> {
        let path = format!("/channels/{channel}");
        self.json(self.request(Method::DELETE, &path))
            .await
            .map(drop)
    }

    /// Sets a role's or member's overwrite in a channel or category, as
    /// Channel Permissions saves each one; Discord sends CHANNEL_UPDATE.
    pub async fn put_overwrite(&self, channel: &str, id: &str, body: &Value) -> Result<()> {
        let path = format!("/channels/{channel}/permissions/{id}");
        self.empty(with_json(self.request(Method::PUT, &path), body))
            .await
    }

    /// Takes a role's or member's overwrite off a channel or category;
    /// Discord sends CHANNEL_UPDATE.
    pub async fn delete_overwrite(&self, channel: &str, id: &str) -> Result<()> {
        let path = format!("/channels/{channel}/permissions/{id}");
        self.empty(self.request(Method::DELETE, &path)).await
    }

    /// Leaves a server, as a member rather than one lurking in it.
    pub async fn leave_guild(&self, guild: &str) -> Result<()> {
        let path = format!("/users/@me/guilds/{guild}");
        self.empty(with_json(
            self.request(Method::DELETE, &path),
            &json!({"lurking": false}),
        ))
        .await
    }

    /// Whether this account may ring the others in a DM, beyond calling them.
    pub async fn ringable(&self, channel: &str) -> Result<bool> {
        let path = format!("/channels/{channel}/call");
        let answer = self.json(self.request(Method::GET, &path)).await?;
        Ok(answer["ringable"].as_bool().unwrap_or(false))
    }

    /// Rings everyone in a DM or group DM call who isn't in it yet.
    pub async fn ring(&self, channel: &str) -> Result<()> {
        let path = format!("/channels/{channel}/call/ring");
        let body = json!({"recipients": null});
        self.empty(with_json(self.request(Method::POST, &path), &body))
            .await
    }

    /// Stops ringing this account for a call, which declines it.
    pub async fn stop_ringing(&self, channel: &str, me: &str) -> Result<()> {
        let path = format!("/channels/{channel}/call/stop-ringing");
        let body = json!({"recipients": [me]});
        self.empty(with_json(self.request(Method::POST, &path), &body))
            .await
    }

    /// Shows `thumbnail`, a JPEG as a data URI, as the preview of this
    /// account's live stream, as Discord's client does every few minutes.
    pub async fn stream_preview(&self, stream_key: &str, thumbnail: &str) -> Result<()> {
        let path = format!("/streams/{stream_key}/preview");
        let body = json!({"thumbnail": thumbnail});
        self.empty(with_json(self.request(Method::POST, &path), &body))
            .await
    }

    /// Discord's own soundboard sounds, which every account may play.
    pub async fn default_soundboard_sounds(&self) -> Result<Vec<crate::soundboard::Sound>> {
        let sounds = self
            .json(self.request(Method::GET, "/soundboard-default-sounds"))
            .await?;
        Ok(crate::soundboard::parse_all(&sounds))
    }

    /// Plays `sound` in the call `channel`, as Discord's client does: with
    /// its emoji, and its server unless it's one of Discord's own. Discord
    /// sends everyone there VOICE_CHANNEL_EFFECT_SEND, and plays nothing
    /// itself.
    pub async fn send_soundboard_sound(
        &self,
        channel: &str,
        sound: &crate::soundboard::Sound,
    ) -> Result<()> {
        let path = format!("/channels/{channel}/send-soundboard-sound");
        let mut body = json!({
            "sound_id": sound.id,
            "emoji_id": sound.emoji_id,
            "emoji_name": sound.emoji_name,
        });
        if let Some(guild) = &sound.guild {
            body["source_guild_id"] = json!(guild);
        }
        self.empty(with_json(self.request(Method::POST, &path), &body))
            .await
    }

    /// Plays this account's join sound, which its settings name, for the
    /// call `channel` it just joined, with one of Discord's emoji
    /// animations, as Discord's client does: premium animations 10 to 15.
    pub async fn custom_call_sound(&self, channel: &str, animation: u32) -> Result<()> {
        let path = format!("/channels/{channel}/custom-call-sounds");
        let body = json!({"animation_type": 0, "animation_id": animation});
        self.empty(with_json(self.request(Method::POST, &path), &body))
            .await
    }

    /// Downloads a file from Discord's CDN or media proxy, or the test
    /// server, refusing anything over `limit` bytes. It waits its turn among
    /// [`DOWNLOADS`].
    pub async fn download(&self, url: &str, limit: usize) -> Result<Vec<u8>> {
        allowed(url)?;
        let _turn = self.downloads.acquire().await?;
        self.read_file(url, limit).await
    }

    /// Downloads a file as [`Self::download`] does, without waiting for a
    /// turn: one someone opened, such as the image the viewer shows, which
    /// shouldn't wait behind the previews and avatars a channel asks for.
    pub async fn download_now(&self, url: &str, limit: usize) -> Result<Vec<u8>> {
        allowed(url)?;
        self.read_file(url, limit).await
    }

    /// Downloads a file without checking where it is from.
    async fn read_file(&self, url: &str, limit: usize) -> Result<Vec<u8>> {
        let mut response = self.get_file(url).await?;
        let mut data = Vec::new();
        while let Some(chunk) = self.next_chunk(&mut response).await? {
            data.extend_from_slice(&chunk);
            if data.len() > limit {
                bail!("the file is too large");
            }
        }
        Ok(data)
    }

    /// Downloads a file from Discord's CDN or media proxy into `file` as it
    /// arrives, as [`Self::download`] does, returning its size.
    pub async fn download_into(
        &self,
        url: &str,
        file: &mut std::fs::File,
        limit: u64,
    ) -> Result<u64> {
        use std::io::Write;
        allowed(url)?;
        let mut response = self.get_file(url).await?;
        let mut size = 0;
        while let Some(chunk) = self.next_chunk(&mut response).await? {
            size += chunk.len() as u64;
            if size > limit {
                bail!("the file is too large");
            }
            file.write_all(&chunk).context("could not keep the file")?;
        }
        file.flush().context("could not keep the file")?;
        Ok(size)
    }

    /// Asks for a file, giving up if Discord doesn't answer within [`STALL`].
    async fn get_file(&self, url: &str) -> Result<Response> {
        let response = tokio::time::timeout(self.stall, self.http.get(url).send())
            .await
            .map_err(|_| Stalled)?
            .context("could not reach Discord")?;
        if !response.status().is_success() {
            return Err(Refused(response.status()).into());
        }
        Ok(response)
    }

    /// The next part of a file, or none at its end. Discord sending nothing
    /// for [`STALL`] ends the download.
    async fn next_chunk(
        &self,
        response: &mut Response,
    ) -> Result<Option<impl std::ops::Deref<Target = [u8]>>> {
        tokio::time::timeout(self.stall, response.chunk())
            .await
            .map_err(|_| Stalled)?
            .context("the download stopped")
    }

    async fn json(&self, request: RequestBuilder) -> Result<Value> {
        let response = self.execute(request).await?;
        let body = response
            .bytes()
            .await
            .context("could not read Discord's answer")?;
        serde_json::from_slice(&body).context("could not understand Discord's answer")
    }

    async fn empty(&self, request: RequestBuilder) -> Result<()> {
        self.execute(request).await.map(drop)
    }

    /// Sends a request within Discord's rate limits; see [`crate::limits`].
    /// A 429 means Discord didn't act on it, so it goes once more after the
    /// wait Discord asks for.
    async fn execute(&self, request: RequestBuilder) -> Result<Response> {
        let (client, request) = request.build_split();
        let mut request = request.context("could not build the request")?;
        let route = limits::route(request.method(), request.url().path());
        let mut retried = false;
        loop {
            match self.refusals.check() {
                Ok(()) => {}
                Err(Stopped::Unauthorized) => bail!(UNAUTHORIZED),
                Err(Stopped::Refused(wait)) => bail!(
                    "Discord refused too many of Yapfast's requests lately, so Yapfast stopped asking. Try again in {} min.",
                    wait.as_secs().div_ceil(60)
                ),
            }
            if let Err(wait) = self.limits.acquire(&route).await {
                bail!(
                    "Discord asks Yapfast to slow down. Try again in {} s.",
                    wait.as_secs()
                );
            }
            let again = if retried { None } else { request.try_clone() };
            let response = client
                .execute(request)
                .await
                .context("Can't reach Discord.")?;
            let status = response.status();
            let linked = !self.token().is_empty();
            if linked {
                self.refusals.answered(status);
            }
            if status == StatusCode::UNAUTHORIZED && linked {
                let body = body(response).await;
                // Discord's answer to a login it no longer takes, rather than
                // to one request.
                if body["code"] == 0 {
                    tracing::warn!("Discord refused the login; Yapfast stops asking");
                    self.refusals.unauthorized();
                }
                return Err(refusal(status, &body));
            }
            if status != StatusCode::TOO_MANY_REQUESTS {
                self.limits.update(&route, response.headers());
                return checked(response).await;
            }
            let headers = response.headers().clone();
            let body = body(response).await;
            self.limits.limited(&route, &headers, &body);
            let Some(next) = again else {
                return Err(refusal(status, &body));
            };
            tracing::warn!(
                route,
                "Discord limited a request; sending it again after the wait"
            );
            request = next;
            retried = true;
        }
    }
}

/// What asking for the games Discord detects found.
pub(crate) enum GameList {
    /// The list is as the version asked with.
    Unchanged,
    /// A new list went to the file, with its version.
    Written(Option<String>),
}

/// Turns Discord's refusals into errors a person can act on.
async fn checked(response: Response) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    Err(refusal(status, &body(response).await))
}

/// An answer's JSON body, or null.
async fn body(response: Response) -> Value {
    match response.bytes().await {
        Ok(body) => serde_json::from_slice(&body).unwrap_or_default(),
        Err(_) => Value::Null,
    }
}

/// A refusal with this status and body, as an error a person can act on.
fn refusal(status: StatusCode, body: &Value) -> anyhow::Error {
    // Discord's own words go in the error's chain, for logs: `{:#}` shows them.
    // The status stays at its root, for callers to tell refusals apart.
    let detail = anyhow::Error::new(Refused(status));
    let detail = match (body["code"].as_u64(), body["message"].as_str()) {
        (Some(code), Some(message)) => detail.context(DiscordError {
            code,
            message: message.to_owned(),
        }),
        _ => detail,
    };
    // A request held back for two-factor authentication carries its ticket.
    if status == StatusCode::UNAUTHORIZED
        && body["code"] == mfa::MFA_REQUIRED
        && let Some(required) = mfa::Required::from_body(body)
    {
        return detail.context(required);
    }
    detail.context(match (status, body["code"].as_u64()) {
        (_, Some(UNKNOWN_USERNAME)) => anyhow!("Discord has nobody by that username."),
        (_, Some(REQUESTS_CLOSED)) => {
            anyhow!("They don't take friend requests from people they don't know.")
        }
        (_, Some(ALREADY_FRIENDS)) => anyhow!("You're already friends."),
        (_, Some(UNKNOWN_APPLICATION)) => anyhow!("Discord doesn't know that application."),
        (_, Some(UNKNOWN_INVITE | INVALID_INVITE)) => {
            anyhow!("This invite is invalid or has expired.")
        }
        (_, Some(BANNED)) => anyhow!("You're banned from this server."),
        (_, Some(TOO_MANY_SERVERS)) => {
            anyhow!("You're in as many servers as Discord allows. Leave one to join another.")
        }
        (_, Some(SERVER_FULL)) => anyhow!("This server is full."),
        (_, Some(INVITES_PAUSED)) => anyhow!("This server has paused invites."),
        (_, Some(UNKNOWN_GIFT)) => anyhow!("This gift code is invalid or has expired."),
        (_, Some(GIFT_EXHAUSTED)) => anyhow!("This gift has already been redeemed."),
        (_, Some(GIFT_OWNED)) => anyhow!("You already have what this gift grants."),
        (_, Some(GIFT_SELF)) => anyhow!("You can't redeem a gift you made."),
        (_, Some(GIFT_SUBSCRIPTION_MANAGED)) => anyhow!(
            "You subscribe to Nitro through another provider, such as an app store, so you can't redeem Nitro gifts."
        ),
        (_, Some(GIFT_SUBSCRIPTION_INCOMPATIBLE)) => {
            anyhow!("You can't redeem this gift while you're subscribed to Nitro.")
        }
        (_, Some(GIFT_INVOICE_OPEN)) => {
            anyhow!("You can't redeem this gift while your subscription has an open invoice.")
        }
        (_, Some(GIFT_FRAUD_REJECTED)) => anyhow!("Discord won't let this account use this gift."),
        (_, Some(UNKNOWN_EVENT)) => anyhow!("This event doesn't exist."),
        (_, Some(mfa::INVALID_MFA)) => anyhow!("That didn't work. Check it and try again."),
        (_, Some(mfa::MFA_REQUIRED)) => {
            anyhow!("Discord asks for two-factor authentication, which Yapfast can't do here.")
        }
        (_, Some(MISSING_PERMISSIONS)) => {
            anyhow!("You don't have permission to do that in this server.")
        }
        (StatusCode::BAD_REQUEST, Some(INVALID_FORM_BODY)) if form_errors(body).is_some() => {
            anyhow!(form_errors(body).unwrap_or_default())
        }
        // Only a 401 without a code of its own is about the login.
        (StatusCode::UNAUTHORIZED, Some(code)) if code != 0 => match body["message"].as_str() {
            Some(message) => anyhow!("Discord refused: {message}"),
            None => anyhow!("Discord answered {status}."),
        },
        _ => match status {
            StatusCode::UNAUTHORIZED => anyhow!(UNAUTHORIZED),
            StatusCode::FORBIDDEN => {
                anyhow!("Discord says you don't have permission to do that here.")
            }
            StatusCode::NOT_FOUND => anyhow!("Discord no longer has that message or channel."),
            StatusCode::PAYLOAD_TOO_LARGE => {
                anyhow!("Discord says the file is too large for your account.")
            }
            StatusCode::TOO_MANY_REQUESTS => {
                let wait = body["retry_after"].as_f64().unwrap_or(1.0).ceil();
                anyhow!("Discord asks Yapfast to slow down. Try again in {wait} s.")
            }
            StatusCode::BAD_REQUEST if body["captcha_key"].is_array() => {
                let message = "Discord asked for a captcha. Do this in the Discord app for now.";
                // The captcha, for a caller that can show it.
                return match Captcha::from_body(body) {
                    Some(captcha) => anyhow::Error::new(captcha).context(message),
                    None => anyhow!(message),
                };
            }
            _ => match body["message"].as_str() {
                Some(message) => anyhow!("Discord refused: {message}"),
                None => anyhow!("Discord answered {status}."),
            },
        },
    })
}

/// What Discord found wrong in a form it refused (code 50035), field by
/// field, as "Name: Must be between 2 and 100 in length."; none when it
/// named no field.
fn form_errors(body: &Value) -> Option<String> {
    fn walk(value: &Value, field: &str, found: &mut Vec<String>) {
        let Some(object) = value.as_object() else {
            return;
        };
        if let Some(errors) = object.get("_errors").and_then(Value::as_array) {
            for error in errors {
                if let Some(message) = error["message"].as_str() {
                    found.push(match field {
                        "" => message.to_owned(),
                        field => format!("{field}: {message}"),
                    });
                }
            }
        }
        for (key, value) in object.iter().filter(|(key, _)| *key != "_errors") {
            // An index names the field around it: `traits.0.label` is a
            // trait's label.
            let field = if key.bytes().all(|byte| byte.is_ascii_digit()) {
                field.to_owned()
            } else {
                let mut words = key.replace('_', " ");
                if let Some(first) = words.get_mut(..1) {
                    first.make_ascii_uppercase();
                }
                words
            };
            walk(value, &field, found);
        }
    }
    let mut found = Vec::new();
    walk(&body["errors"], "", &mut found);
    (!found.is_empty()).then(|| found.join(" "))
}

/// Discord's error codes that Yapfast words itself.
const REQUESTS_CLOSED: u64 = 80000;
const UNKNOWN_USERNAME: u64 = 80004;
const ALREADY_FRIENDS: u64 = 80007;
const UNKNOWN_APPLICATION: u64 = 10002;
const UNKNOWN_INVITE: u64 = 10006;
const INVALID_INVITE: u64 = 50020;
const BANNED: u64 = 40007;
const TOO_MANY_SERVERS: u64 = 30001;
const SERVER_FULL: u64 = 30019;
const UNKNOWN_GIFT: u64 = 10038;
const GIFT_EXHAUSTED: u64 = 50050;
const GIFT_OWNED: u64 = 50051;
const GIFT_SELF: u64 = 50054;
const GIFT_FRAUD_REJECTED: u64 = 50097;
const GIFT_SUBSCRIPTION_MANAGED: u64 = 100021;
const GIFT_SUBSCRIPTION_INCOMPATIBLE: u64 = 100023;
const GIFT_INVOICE_OPEN: u64 = 100024;
const INVITES_PAUSED: u64 = 40069;
const UNKNOWN_EVENT: u64 = 10070;
const MISSING_PERMISSIONS: u64 = 50013;
const INVALID_FORM_BODY: u64 = 50035;

/// The X-Context-Properties header Discord's client sends: where an action
/// happened, as base64 JSON.
fn context(place: &str) -> String {
    encoded_context(&json!({"location": place}))
}

/// The X-Context-Properties header of a whole context, for an action whose
/// context says more than where.
fn encoded_context(context: &Value) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(context.to_string())
}

/// A new message's body as Discord's client sends it, before attachments
/// and a reply.
fn message_body(content: &str, nonce: &str) -> Value {
    json!({
        "mobile_network_type": "unknown",
        "content": content,
        "nonce": nonce,
        "tts": false,
        "flags": 0,
    })
}

/// Uploaded files as a message's `attachments` name them.
fn attachments(uploads: &[Upload]) -> Value {
    uploads
        .iter()
        .enumerate()
        .map(|(index, upload)| {
            json!({
                "id": index.to_string(),
                "filename": upload.filename,
                "uploaded_filename": upload.uploaded_filename,
            })
        })
        .collect()
}

/// A forum post as Discord's client asks for one: the thread, and its
/// first message nested in it, which carries no nonce.
fn post_body(
    title: &str,
    archive_after: u16,
    tags: &[String],
    content: &str,
    uploads: &[Upload],
) -> Value {
    let mut message = json!({ "content": content });
    if !uploads.is_empty() {
        message["attachments"] = attachments(uploads);
    }
    json!({
        "name": title,
        "auto_archive_duration": archive_after,
        "applied_tags": tags,
        "message": message,
    })
}

/// A `multipart/form-data` body of text `fields` and a file, given by its
/// name, type and bytes, with its content type.
fn form(fields: &[(&str, &str)], file: (&str, &str, &[u8])) -> (String, Vec<u8>) {
    let boundary = format!("yapfast{:016x}", rand::random::<u64>());
    let mut body = Vec::with_capacity(file.2.len() + 512);
    for (name, value) in fields {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    let (filename, mime, data) = file;
    let filename = filename.replace(['"', '\r', '\n'], "_");
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {mime}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

fn with_json(request: RequestBuilder, body: &Value) -> RequestBuilder {
    request
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.to_string())
}

/// Percent-encodes a path segment.
/// The most of a banned member's messages a ban deletes: a week.
pub const MAX_BAN_DELETE: u32 = 7 * 24 * 3600;

/// Discord keeps up to 512 characters of an audit log's reason.
const MAX_REASON: usize = 512;

/// Gives a moderation request the reason the audit log shows, URL-encoded as
/// Discord reads it; none when it's empty.
fn audit_reason(request: RequestBuilder, reason: &str) -> RequestBuilder {
    let reason = reason.trim();
    if reason.is_empty() {
        return request;
    }
    let end = reason
        .char_indices()
        .nth(MAX_REASON)
        .map_or(reason.len(), |(end, _)| end);
    request.header("X-Audit-Log-Reason", encode(&reason[..end]))
}

fn encode(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len() * 3);
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// Refuses a file from anywhere but Discord's CDN and media proxy, the GIF
/// providers and games' pictures, or the test server.
fn allowed(url: &str) -> Result<()> {
    let test_file = client::test_server()
        .and_then(|server| url.strip_prefix(server))
        .is_some_and(|path| path.starts_with('/'));
    if !discord_file(url)
        && !test_file
        && !crate::gifs::provider_media(url)
        && !crate::activity::outside_picture(url)
    {
        bail!("not a Discord file");
    }
    Ok(())
}

/// Whether `url` is on Discord's CDN or its media proxy, which serves
/// embeds' pictures from `images-ext-1.discordapp.net` and the like.
fn discord_file(url: &str) -> bool {
    let Some(host) = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split_once('/'))
        .map(|(host, _)| host)
    else {
        return false;
    };
    let proxy = host
        .strip_prefix("images-ext-")
        .and_then(|host| host.strip_suffix(".discordapp.net"))
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()));
    proxy || host == "cdn.discordapp.com" || host == "media.discordapp.net"
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_poll_goes_as_discords_client_sends_it() {
        use super::{NewAnswer, NewPoll};
        let answer = |text: &str, emoji: Option<&str>| NewAnswer {
            text: text.into(),
            emoji: emoji.map(Into::into),
        };
        let poll = NewPoll {
            question: "Lunch?".into(),
            answers: vec![
                answer("Pizza", Some("🍕")),
                answer("Soup", Some("talk:77")),
                answer("Salad", None),
            ],
            hours: 24,
            multiselect: true,
        };
        assert_eq!(
            poll.body(),
            serde_json::json!({
                "question": {"text": "Lunch?"},
                "answers": [
                    {"poll_media": {"text": "Pizza", "emoji": {"name": "🍕"}}},
                    {"poll_media": {"text": "Soup", "emoji": {"id": "77", "name": ""}}},
                    {"poll_media": {"text": "Salad"}},
                ],
                "allow_multiselect": true,
                "duration": 24,
                "layout_type": 1,
            })
        );
    }

    #[test]
    fn a_forum_post_nests_its_first_message() {
        let upload = super::Upload {
            filename: "crash.png".into(),
            uploaded_filename: "uploads/1/crash.png".into(),
        };
        assert_eq!(
            super::post_body("Crash", 4320, &["7".into()], "It closes.", &[upload]),
            serde_json::json!({
                "name": "Crash",
                "auto_archive_duration": 4320,
                "applied_tags": ["7"],
                "message": {
                    "content": "It closes.",
                    "attachments": [
                        { "id": "0", "filename": "crash.png", "uploaded_filename": "uploads/1/crash.png" },
                    ],
                },
            })
        );
    }

    #[test]
    fn downloads_that_may_work_later() {
        use super::{Refused, StatusCode, may_work_later};
        let refused = |status| anyhow::Error::from(Refused(status));
        assert!(may_work_later(&refused(StatusCode::SERVICE_UNAVAILABLE)));
        assert!(may_work_later(&refused(StatusCode::TOO_MANY_REQUESTS)));
        assert!(!may_work_later(&refused(StatusCode::NOT_FOUND)));
        assert!(!may_work_later(&refused(StatusCode::FORBIDDEN)));
        assert!(!may_work_later(&anyhow::anyhow!("the file is too large")));
    }

    #[test]
    fn upload_limits_follow_nitro_and_boosts() {
        use super::{MIB, upload_limit};
        assert_eq!(upload_limit(0, None), 20 * MIB, "without Nitro");
        assert_eq!(upload_limit(3, None), 50 * MIB, "Nitro Basic");
        assert_eq!(upload_limit(2, None), 500 * MIB, "Nitro");
        assert_eq!(upload_limit(0, Some(1)), 20 * MIB, "level 1 adds nothing");
        assert_eq!(upload_limit(0, Some(2)), 50 * MIB);
        assert_eq!(upload_limit(0, Some(3)), 100 * MIB);
        assert_eq!(upload_limit(2, Some(3)), 500 * MIB, "the larger wins");
    }

    #[test]
    fn says_where_actions_happen_as_discords_client_does() {
        // The values Discord's client sends, as discord.py-self records them.
        assert_eq!(
            super::context("User Profile"),
            "eyJsb2NhdGlvbiI6IlVzZXIgUHJvZmlsZSJ9"
        );
        assert_eq!(
            super::context("Friends"),
            "eyJsb2NhdGlvbiI6IkZyaWVuZHMifQ=="
        );
    }

    #[test]
    fn encodes_emoji_for_paths() {
        assert_eq!(super::encode("👍"), "%F0%9F%91%8D");
        assert_eq!(super::encode("pog:5"), "pog%3A5");
    }

    #[tokio::test]
    async fn a_download_discord_stops_sending_gives_up() {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        // The first answer stops partway through the file; the second never
        // comes. Both keep their connections open.
        tokio::spawn(async move {
            let mut held = vec![];
            for answer in [
                &b"HTTP/1.1 200 OK\r\ncontent-length: 8\r\n\r\nhalf"[..],
                b"",
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let _ = socket.read(&mut [0; 1024]).await;
                socket.write_all(answer).await.unwrap();
                held.push(socket);
            }
            std::future::pending::<()>().await;
        });
        let mut api = super::Api::anonymous().unwrap();
        api.stall = std::time::Duration::from_millis(200);
        let url = format!("http://{address}/a.png");
        for _ in 0..2 {
            let error = api.read_file(&url, 1 << 10).await.unwrap_err();
            assert!(error.is::<super::Stalled>(), "{error:#}");
            assert!(super::may_work_later(&error));
        }
    }

    #[test]
    fn downloads_only_from_discord() {
        assert!(super::discord_file(
            "https://cdn.discordapp.com/attachments/1/2/a.png"
        ));
        assert!(super::discord_file(
            "https://images-ext-2.discordapp.net/external/x/https/example.com/a.png"
        ));
        assert!(!super::discord_file(
            "https://images-ext-.discordapp.net/a.png"
        ));
        assert!(!super::discord_file("https://example.com/a.png"));
        assert!(!super::discord_file(
            "https://cdn.discordapp.com.example.com/a.png"
        ));
    }

    #[test]
    fn refusals_carry_discords_code_and_say_what_went_wrong() {
        use super::{StatusCode, code, mfa, refusal};
        use serde_json::json;
        let form = json!({"code": 50035, "message": "Invalid Form Body", "errors": {
            "name": {"_errors": [{"code": "BASE_TYPE_BAD_LENGTH", "message": "Must be between 2 and 100 in length."}]},
            "traits": {"0": {"label": {"_errors": [{"message": "Too long."}]}}},
        }});
        let error = refusal(StatusCode::BAD_REQUEST, &form);
        assert_eq!(code(&error), Some(50035));
        assert_eq!(
            error.to_string(),
            "Name: Must be between 2 and 100 in length. Label: Too long."
        );

        let denied = json!({"code": 50013, "message": "Missing Permissions"});
        let error = refusal(StatusCode::FORBIDDEN, &denied);
        assert_eq!(
            error.to_string(),
            "You don't have permission to do that in this server."
        );

        let held = json!({"code": 60003, "message": "Two factor is required for this operation",
            "mfa": {"ticket": "t", "methods": [{"type": "totp"}]}});
        let error = refusal(StatusCode::UNAUTHORIZED, &held);
        let required = error.downcast_ref::<mfa::Required>().unwrap();
        assert_eq!(required.methods, [mfa::Method::Totp]);
        assert_eq!(code(&error), Some(60003));

        // Only a 401 without a code is about the login.
        let other = json!({"code": 50001, "message": "Missing Access"});
        assert_eq!(
            refusal(StatusCode::UNAUTHORIZED, &other).to_string(),
            "Discord refused: Missing Access"
        );
        let login = json!({"code": 0, "message": "401: Unauthorized"});
        assert_eq!(
            refusal(StatusCode::UNAUTHORIZED, &login).to_string(),
            super::UNAUTHORIZED
        );
    }
}
