//! A plugin's own login to a service: `oauth.login`.
//!
//! Spotify's Web API, and many like it, want an app of the plugin author's own
//! and the person's consent in a browser. This is Authorization Code with PKCE
//! (RFC 7636), the flow for an app with no secret, done in the parent: the
//! person's browser opens the service's page, the service sends it back to a
//! one-request listener on this computer's loopback, and the code is traded
//! for tokens here. The refresh token stays in the plugin's data folder and
//! never reaches the plugin; the plugin is handed access tokens, which are its
//! own app's, and calls the service's API itself through `http.fetch`.
//!
//! Spotify's rules this follows (developer.spotify.com, Authorization Code with
//! PKCE Flow, Refreshing tokens, Redirect URIs): `localhost` isn't allowed, so
//! the redirect is `http://127.0.0.1:PORT/callback`, and a loopback redirect
//! may take any port when the app registered it without one; a refresh may or
//! may not bring a new refresh token, and the old one is kept when it doesn't.
//!
//! A service whose app sends the browser back to its own server (ReviewDB and
//! Decor, whose Discord apps redirect to their own sites) trades the code on
//! that server and hands a token back: [`login_by_server`] opens the page and
//! waits for the token to come back through a `yapfast://plugin-oauth/<plugin>`
//! link ([`Returns::link`]) or be pasted into the sign-in's dialog
//! ([`Returns::pasted`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use sha2::Digest as _;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use yapfast_plugin_api::account::{OAUTH_REDIRECT_PATH, OAuthDecl, OAuthToken};

/// How long the person has to finish signing in.
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// How long before it stops working an access token is renewed.
const RENEW_BEFORE_MS: u64 = 60 * 1000;

/// What is kept for a signed-in service, in `<data>/oauth/<provider>.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Kept {
    pub access_token: String,
    #[serde(default)]
    pub token_type: String,
    pub expires_at_ms: u64,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl Kept {
    pub fn token(&self, provider: &str) -> OAuthToken {
        OAuthToken {
            provider: provider.to_owned(),
            access_token: self.access_token.clone(),
            token_type: if self.token_type.is_empty() {
                "Bearer".to_owned()
            } else {
                self.token_type.clone()
            },
            expires_at_ms: self.expires_at_ms,
            scopes: self.scopes.clone(),
        }
    }
}

/// A verifier of 64 characters from the unreserved set, and its S256
/// challenge.
pub fn pkce() -> (String, String) {
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let challenge = challenge(&verifier);
    (verifier, challenge)
}

/// `BASE64URL(SHA256(verifier))`, unpadded (RFC 7636, 4.2).
pub fn challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier))
}

/// Writes `text` as a URL's query component.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Reads one query component back.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'+' => out.push(b' '),
            b'%' if at + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        at += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            byte => out.push(byte),
        }
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The page the person's browser is sent to.
pub fn authorize_url(service: &OAuthDecl, redirect: &str, state: &str, challenge: &str) -> String {
    let (authorize, _) = service.endpoints();
    let separator = if authorize.contains('?') { '&' } else { '?' };
    let mut url = format!(
        "{authorize}{separator}client_id={}&response_type=code&redirect_uri={}\
         &code_challenge_method=S256&code_challenge={}&state={}",
        encode(&service.client_id),
        encode(redirect),
        encode(challenge),
        encode(state),
    );
    if !service.scopes.is_empty() {
        url.push_str("&scope=");
        url.push_str(&encode(&service.scopes.join(" ")));
    }
    url
}

/// The code a redirect carries, from the request line the browser sent:
/// `GET /callback?code=...&state=... HTTP/1.1`. A wrong state is someone
/// else's redirect, and an `error` is the person saying no.
pub fn callback_code(request_line: &str, state: &str) -> Result<String> {
    let target = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow!("the browser's request had no address"))?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != OAUTH_REDIRECT_PATH {
        bail!("the browser came back to {path}, not {OAUTH_REDIRECT_PATH}");
    }
    let mut code = None;
    let mut came_state = None;
    let mut error = None;
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        match name {
            "code" => code = Some(decode(value)),
            "state" => came_state = Some(decode(value)),
            "error" => error = Some(decode(value)),
            _ => {}
        }
    }
    if came_state.as_deref() != Some(state) {
        bail!("the sign-in came back with someone else's state, so it was ignored");
    }
    if let Some(error) = error {
        bail!("the sign-in was refused: {error}");
    }
    code.filter(|code| !code.is_empty())
        .ok_or_else(|| anyhow!("the sign-in came back with no code"))
}

/// What the token endpoint answered, as kept: a refresh that brings no new
/// refresh token keeps the old one.
pub fn kept_from(answer: &serde_json::Value, now_ms: u64, old: Option<&Kept>) -> Result<Kept> {
    let access_token = answer["access_token"]
        .as_str()
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "the service answered no token: {}",
                answer["error_description"]
                    .as_str()
                    .or(answer["error"].as_str())
                    .unwrap_or("no reason given")
            )
        })?
        .to_owned();
    let expires_in = answer["expires_in"].as_u64().unwrap_or(3600);
    let scopes = match answer["scope"].as_str() {
        Some(scope) => scope.split_whitespace().map(str::to_owned).collect(),
        None => old.map(|old| old.scopes.clone()).unwrap_or_default(),
    };
    Ok(Kept {
        access_token,
        token_type: answer["token_type"].as_str().unwrap_or("Bearer").to_owned(),
        expires_at_ms: now_ms + expires_in * 1000,
        refresh_token: answer["refresh_token"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| old.and_then(|old| old.refresh_token.clone())),
        scopes,
    })
}

/// Where a service's tokens are kept for a plugin.
pub fn kept_path(data_dir: &Path, provider: &str) -> PathBuf {
    data_dir.join("oauth").join(format!("{provider}.json"))
}

pub fn read_kept(path: &Path) -> Option<Kept> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Keeps the tokens where only this account can read them.
pub fn write_kept(path: &Path, kept: &Kept) -> Result<()> {
    let folder = path.parent().context("a token file has a folder")?;
    std::fs::create_dir_all(folder)?;
    let mut file = tempfile::NamedTempFile::new_in(folder)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    std::io::Write::write_all(&mut file, &serde_json::to_vec(kept)?)?;
    file.persist(path)?;
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// The HTTP client the token requests use: the same TLS stack as the rest of
/// the plugin system, no proxy, no cookies.
fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .context("the sign-in's HTTP client couldn't be built")
}

/// Asks the token endpoint, with the form the flow's step names.
async fn token_request(service: &OAuthDecl, form: &[(&str, &str)]) -> Result<serde_json::Value> {
    let (_, token) = service.endpoints();
    let body: Vec<String> = form
        .iter()
        .map(|(name, value)| format!("{}={}", encode(name), encode(value)))
        .collect();
    let response = client()?
        .post(token)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body.join("&"))
        .send()
        .await
        .with_context(|| format!("couldn't reach {token}"))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .with_context(|| format!("{token} answered {status} and then stopped"))?;
    let answer: serde_json::Value = serde_json::from_slice(&body)
        .with_context(|| format!("{token} answered {status} with no JSON"))?;
    Ok(answer)
}

/// Signs in: opens a loopback listener, has `open` show the person the
/// service's page, waits for the redirect, trades its code for tokens and
/// keeps them.
pub async fn login(
    service: &OAuthDecl,
    data_dir: &Path,
    open: impl FnOnce(String),
) -> Result<OAuthToken> {
    let listener = tokio::net::TcpListener::bind((
        std::net::Ipv4Addr::LOCALHOST,
        service.redirect_port.unwrap_or(0),
    ))
    .await
    .context("couldn't listen on this computer's loopback for the sign-in")?;
    let port = listener.local_addr()?.port();
    let redirect = format!("http://127.0.0.1:{port}{OAUTH_REDIRECT_PATH}");
    let (verifier, challenge) = pkce();
    let state = uuid::Uuid::new_v4().simple().to_string();
    open(authorize_url(service, &redirect, &state, &challenge));
    let code = tokio::time::timeout(LOGIN_TIMEOUT, async {
        loop {
            let (mut stream, _) = listener.accept().await?;
            let mut request = vec![0; 8192];
            let read = stream.read(&mut request).await.unwrap_or(0);
            let text = String::from_utf8_lossy(&request[..read]).into_owned();
            let line = text.lines().next().unwrap_or_default().to_owned();
            // A browser asks for a favicon too; only the redirect counts.
            if line.contains("/favicon") {
                let _ = stream
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                    .await;
                continue;
            }
            let result = callback_code(&line, &state);
            let page = match &result {
                Ok(_) => "Signed in. You can close this tab and go back to Yapfast.",
                Err(_) => "The sign-in didn't finish. You can close this tab.",
            };
            let _ = stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{page}",
                        page.len()
                    )
                    .as_bytes(),
                )
                .await;
            return result;
        }
    })
    .await
    .map_err(|_| anyhow!("the sign-in wasn't finished within five minutes"))??;
    let answer = token_request(
        service,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect),
            ("client_id", &service.client_id),
            ("code_verifier", &verifier),
        ],
    )
    .await?;
    let kept = kept_from(&answer, now_ms(), None)?;
    write_kept(&kept_path(data_dir, &service.provider), &kept)?;
    Ok(kept.token(&service.provider))
}

/// The longest token a service's server may hand back.
const TOKEN_LIMIT: usize = 4096;

/// The sign-ins waiting for a service's server to hand a token back, one per
/// plugin: a new sign-in of the plugin's replaces its last.
#[derive(Default)]
pub struct Returns {
    waiting: Mutex<BTreeMap<String, Waiting>>,
}

struct Waiting {
    provider: String,
    state: String,
    answer: tokio::sync::oneshot::Sender<Result<Kept, String>>,
}

impl Returns {
    fn wait(
        &self,
        plugin: &str,
        provider: &str,
        state: &str,
    ) -> tokio::sync::oneshot::Receiver<Result<Kept, String>> {
        let (answer, waited) = tokio::sync::oneshot::channel();
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                plugin.to_owned(),
                Waiting {
                    provider: provider.to_owned(),
                    state: state.to_owned(),
                    answer,
                },
            );
        waited
    }

    fn take(&self, plugin: &str) -> Option<Waiting> {
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(plugin)
    }

    /// Whether `plugin` is waiting for a token now.
    pub fn waits(&self, plugin: &str) -> bool {
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(plugin)
    }

    /// A `yapfast://plugin-oauth/<plugin>?provider=…&state=…&token=…` link's
    /// query. Its `state` must be the sign-in's, so a link someone else made
    /// can't sign the plugin in to their account; one that doesn't match
    /// leaves the sign-in waiting.
    pub fn link(&self, plugin: &str, query: &str) -> Result<()> {
        let pairs = pairs(query);
        let get = |name: &str| pairs.get(name).map(String::as_str);
        let mut waiting = self.waiting.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(wait) = waiting.get(plugin) else {
            bail!("{plugin} isn't signing in to anything now");
        };
        if get("state") != Some(wait.state.as_str()) {
            bail!("the link back carried someone else's state, so it was ignored");
        }
        if get("provider").is_some_and(|provider| provider != wait.provider) {
            bail!("the link back named another service");
        }
        let kept = match get("error") {
            Some(error) => Err(format!("the sign-in was refused: {error}")),
            None => kept_from_pairs(&pairs).map_err(|error| error.to_string()),
        };
        if let Some(wait) = waiting.remove(plugin) {
            let _ = wait.answer.send(kept);
        }
        Ok(())
    }

    /// What the person pasted into the sign-in's dialog: the token, the page's
    /// JSON with a `token` in it, or the address the browser ended on.
    pub fn pasted(&self, plugin: &str, text: &str) -> Result<()> {
        let kept = token_from_paste(text)?;
        let wait = self
            .take(plugin)
            .ok_or_else(|| anyhow!("{plugin} isn't signing in to anything now"))?;
        let _ = wait.answer.send(Ok(kept));
        Ok(())
    }

    /// The person closed the sign-in's dialog.
    pub fn cancel(&self, plugin: &str) {
        if let Some(wait) = self.take(plugin) {
            let _ = wait
                .answer
                .send(Err("the sign-in was closed before it finished".to_owned()));
        }
    }
}

/// A query's pairs, decoded; the first of a name counts.
fn pairs(query: &str) -> BTreeMap<String, String> {
    let mut pairs = BTreeMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        pairs.entry(decode(name)).or_insert_with(|| decode(value));
    }
    pairs
}

/// A token a service handed back, with `expires_in` seconds when it said.
fn kept_from_pairs(pairs: &BTreeMap<String, String>) -> Result<Kept> {
    let token = pairs
        .get("token")
        .or_else(|| pairs.get("access_token"))
        .ok_or_else(|| anyhow!("that has no token in it"))?;
    let expires_in = pairs.get("expires_in").and_then(|text| text.parse().ok());
    kept_token(token, expires_in)
}

fn kept_token(token: &str, expires_in: Option<u64>) -> Result<Kept> {
    let token = token.trim();
    if token.is_empty()
        || token.len() > TOKEN_LIMIT
        || !token.bytes().all(|byte| byte.is_ascii_graphic())
    {
        bail!("that isn't a token: one word of up to {TOKEN_LIMIT} plain characters");
    }
    Ok(Kept {
        access_token: token.to_owned(),
        token_type: "Bearer".to_owned(),
        // No end given: kept until `oauth.logout`.
        expires_at_ms: expires_in.map_or(0, |seconds| now_ms() + seconds * 1000),
        refresh_token: None,
        scopes: Vec::new(),
    })
}

/// Reads what the person pasted: the page's JSON (`token` or
/// `access_token`, ReviewDB's and Decor's answers), the address the browser
/// ended on (its query or fragment), or the token by itself.
pub fn token_from_paste(text: &str) -> Result<Kept> {
    let text = text.trim();
    if text.starts_with('{') {
        let json: serde_json::Value =
            serde_json::from_str(text).context("that looks like JSON but isn't")?;
        let token = json["token"]
            .as_str()
            .or(json["access_token"].as_str())
            .ok_or_else(|| anyhow!("the page's answer has no token in it"))?;
        return kept_token(token, json["expires_in"].as_u64());
    }
    if let Some((_, rest)) = text.split_once("://") {
        let query = rest.split_once(['?', '#']).map_or("", |(_, query)| query);
        return kept_from_pairs(&pairs(&query.replace('#', "&")));
    }
    kept_token(text, None)
}

/// Signs in through a service's own server: opens its page with the app's
/// registered `redirect`, then waits for the token to come back by a link or
/// a paste. The server holds the app's secret and trades the code itself.
pub async fn login_by_server(
    service: &OAuthDecl,
    data_dir: &Path,
    plugin: &str,
    returns: &Returns,
    open: impl FnOnce(String),
) -> Result<OAuthToken> {
    let redirect = service
        .redirect
        .as_deref()
        .ok_or_else(|| anyhow!("{} has no `redirect`", service.provider))?;
    let state = uuid::Uuid::new_v4().simple().to_string();
    let waited = returns.wait(plugin, &service.provider, &state);
    open(server_authorize_url(service, redirect, &state));
    let kept = match tokio::time::timeout(LOGIN_TIMEOUT, waited).await {
        Ok(Ok(Ok(kept))) => kept,
        Ok(Ok(Err(why))) => bail!("{why}"),
        Ok(Err(_)) => bail!("another sign-in of this plugin's took this one's place"),
        Err(_) => {
            returns.take(plugin);
            bail!("the sign-in wasn't finished within five minutes")
        }
    };
    let kept = Kept {
        scopes: service.scopes.clone(),
        ..kept
    };
    write_kept(&kept_path(data_dir, &service.provider), &kept)?;
    Ok(kept.token(&service.provider))
}

/// The page a server sign-in opens: the code flow with the app's own
/// redirect and no PKCE, since the server trades the code.
pub fn server_authorize_url(service: &OAuthDecl, redirect: &str, state: &str) -> String {
    let (authorize, _) = service.endpoints();
    let separator = if authorize.contains('?') { '&' } else { '?' };
    let mut url = format!(
        "{authorize}{separator}client_id={}&response_type=code&redirect_uri={}&state={}",
        encode(&service.client_id),
        encode(redirect),
        encode(state),
    );
    if !service.scopes.is_empty() {
        url.push_str("&scope=");
        url.push_str(&encode(&service.scopes.join(" ")));
    }
    url
}

/// The service as this plugin signs in to it: the client id the person put in
/// the setting `client_id_setting` names, when they did, over the manifest's.
pub fn resolve(mut service: OAuthDecl, settings: &serde_json::Value) -> Result<OAuthDecl> {
    if let Some(key) = &service.client_id_setting
        && let Some(own) = settings[key.as_str()]
            .as_str()
            .map(str::trim)
            .filter(|own| !own.is_empty())
    {
        if own.len() > 128
            || !own
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        {
            bail!("the client id in the plugin's settings isn't one");
        }
        service.client_id = own.to_owned();
    }
    if service.client_id.is_empty() {
        bail!(
            "fill in your own {} app's client id in the plugin's settings first",
            service.provider
        );
    }
    Ok(service)
}

/// The access token, renewed first when it is about to stop working.
pub async fn token(service: &OAuthDecl, data_dir: &Path) -> Result<OAuthToken> {
    let path = kept_path(data_dir, &service.provider);
    let kept = read_kept(&path).ok_or_else(|| {
        anyhow!(
            "not signed in to {} yet: call oauth.login",
            service.provider
        )
    })?;
    // 0 is a token a service's server gave with no end.
    if kept.expires_at_ms == 0 || kept.expires_at_ms > now_ms() + RENEW_BEFORE_MS {
        return Ok(kept.token(&service.provider));
    }
    let refresh = kept.refresh_token.clone().ok_or_else(|| {
        anyhow!(
            "the {} sign-in ran out and can't be renewed: call oauth.login",
            service.provider
        )
    })?;
    let answer = token_request(
        service,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", &service.client_id),
        ],
    )
    .await?;
    let renewed = kept_from(&answer, now_ms(), Some(&kept))?;
    write_kept(&path, &renewed)?;
    Ok(renewed.token(&service.provider))
}

/// Forgets a sign-in. The service keeps its own record of the app, which the
/// person removes on its site.
pub fn logout(data_dir: &Path, provider: &str) -> bool {
    std::fs::remove_file(kept_path(data_dir, provider)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_challenge_is_rfc_7636s() {
        // RFC 7636, appendix B.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let (verifier, made) = pkce();
        assert_eq!(verifier.len(), 64);
        assert_eq!(made, challenge(&verifier));
    }

    #[test]
    fn the_authorize_page_asks_for_the_flow_and_the_redirect_comes_back_checked() {
        let spotify = OAuthDecl {
            provider: "spotify".into(),
            client_id: "abc".into(),
            scopes: vec![
                "user-read-playback-state".into(),
                "user-modify-playback-state".into(),
            ],
            ..OAuthDecl::default()
        };
        let url = authorize_url(&spotify, "http://127.0.0.1:4321/callback", "s1", "ch");
        assert!(url.starts_with("https://accounts.spotify.com/authorize?client_id=abc&"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A4321%2Fcallback"));
        assert!(url.contains("code_challenge_method=S256&code_challenge=ch&state=s1"));
        assert!(url.ends_with("scope=user-read-playback-state%20user-modify-playback-state"));

        assert_eq!(
            callback_code("GET /callback?code=A%2Bb&state=s1 HTTP/1.1", "s1").unwrap(),
            "A+b"
        );
        assert!(callback_code("GET /callback?code=x&state=other HTTP/1.1", "s1").is_err());
        assert!(
            callback_code("GET /callback?error=access_denied&state=s1 HTTP/1.1", "s1")
                .unwrap_err()
                .to_string()
                .contains("access_denied")
        );
        assert!(callback_code("GET /elsewhere?code=x&state=s1 HTTP/1.1", "s1").is_err());
    }

    #[test]
    fn a_refresh_without_a_new_refresh_token_keeps_the_old_one() {
        let first = kept_from(
            &serde_json::json!({ "access_token": "a1", "token_type": "Bearer", "expires_in": 3600,
                "refresh_token": "r1", "scope": "user-read-playback-state" }),
            1_000,
            None,
        )
        .unwrap();
        assert_eq!(first.expires_at_ms, 1_000 + 3_600_000);
        let renewed = kept_from(
            &serde_json::json!({ "access_token": "a2", "expires_in": 3600 }),
            2_000,
            Some(&first),
        )
        .unwrap();
        assert_eq!(renewed.refresh_token.as_deref(), Some("r1"));
        assert_eq!(renewed.scopes, vec!["user-read-playback-state"]);
        assert!(
            kept_from(&serde_json::json!({ "error": "invalid_grant" }), 0, None)
                .unwrap_err()
                .to_string()
                .contains("invalid_grant")
        );

        let folder = tempfile::tempdir().unwrap();
        let path = kept_path(folder.path(), "spotify");
        write_kept(&path, &renewed).unwrap();
        assert_eq!(read_kept(&path), Some(renewed));
        assert!(logout(folder.path(), "spotify"));
        assert!(read_kept(&path).is_none());
    }

    #[test]
    fn a_pasted_answer_is_read_as_json_an_address_or_the_token() {
        let token = |text: &str| token_from_paste(text).map(|kept| kept.access_token);
        assert_eq!(
            token(r#"{"success":true,"token":"abc.def"}"#).unwrap(),
            "abc.def"
        );
        assert_eq!(token("  tok-123  ").unwrap(), "tok-123");
        assert_eq!(
            token("https://site.example/done?state=x&token=a%2Bb").unwrap(),
            "a+b"
        );
        assert_eq!(
            token("https://site.example/done#access_token=frag&expires_in=60").unwrap(),
            "frag"
        );
        assert!(
            token_from_paste("https://site.example/done#access_token=frag&expires_in=60")
                .unwrap()
                .expires_at_ms
                > 0
        );
        assert_eq!(token_from_paste("plain").unwrap().expires_at_ms, 0);
        assert!(token("two words").is_err());
        assert!(token(r#"{"error":"no"}"#).is_err());
        assert!(token("").is_err());
    }

    #[test]
    fn a_link_back_needs_the_sign_ins_state() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let returns = Returns::default();
        let waited = returns.wait("com.example.r", "reviewdb", "s1");
        assert!(returns.waits("com.example.r"));
        assert!(
            returns
                .link("com.example.r", "state=other&token=t")
                .is_err()
        );
        assert!(
            returns
                .link("com.example.other", "state=s1&token=t")
                .is_err()
        );
        assert!(
            returns.waits("com.example.r"),
            "a wrong link leaves it waiting"
        );
        returns
            .link("com.example.r", "provider=reviewdb&state=s1&token=t1")
            .unwrap();
        let kept = runtime.block_on(waited).unwrap().unwrap();
        assert_eq!(kept.access_token, "t1");
        assert!(!returns.waits("com.example.r"));

        let waited = returns.wait("com.example.r", "reviewdb", "s2");
        returns.cancel("com.example.r");
        assert!(runtime.block_on(waited).unwrap().is_err());
    }

    #[test]
    fn a_client_id_comes_from_the_setting_when_the_person_filled_it_in() {
        let service = OAuthDecl {
            provider: "spotify".into(),
            client_id: "authors".into(),
            client_id_setting: Some("app".into()),
            ..OAuthDecl::default()
        };
        let resolved = |settings| resolve(service.clone(), &settings).map(|s| s.client_id);
        assert_eq!(
            resolved(serde_json::json!({ "app": " mine123 " })).unwrap(),
            "mine123"
        );
        assert_eq!(
            resolved(serde_json::json!({ "app": "" })).unwrap(),
            "authors"
        );
        assert!(resolved(serde_json::json!({ "app": "has space" })).is_err());
        let none = OAuthDecl {
            client_id: String::new(),
            ..service.clone()
        };
        assert!(resolve(none, &serde_json::json!({})).is_err());

        let server = OAuthDecl {
            provider: "reviewdb".into(),
            client_id: "915".into(),
            authorize: Some("https://discord.com/oauth2/authorize".into()),
            redirect: Some("https://manti.vendicated.dev/api/reviewdb/auth".into()),
            scopes: vec!["identify".into()],
            ..OAuthDecl::default()
        };
        let url = server_authorize_url(&server, server.redirect.as_deref().unwrap(), "s1");
        assert!(
            url.starts_with(
                "https://discord.com/oauth2/authorize?client_id=915&response_type=code"
            )
        );
        assert!(
            url.contains("redirect_uri=https%3A%2F%2Fmanti.vendicated.dev%2Fapi%2Freviewdb%2Fauth")
        );
        assert!(!url.contains("code_challenge"));
        assert!(url.ends_with("&state=s1&scope=identify"));
    }
}
