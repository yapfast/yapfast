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

use std::path::{Path, PathBuf};
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

/// The access token, renewed first when it is about to stop working.
pub async fn token(service: &OAuthDecl, data_dir: &Path) -> Result<OAuthToken> {
    let path = kept_path(data_dir, &service.provider);
    let kept = read_kept(&path).ok_or_else(|| {
        anyhow!(
            "not signed in to {} yet: call oauth.login",
            service.provider
        )
    })?;
    if kept.expires_at_ms > now_ms() + RENEW_BEFORE_MS {
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
}
