//! Linking an account by QR code, through Discord's remote login gateway, as
//! the login page of Discord's web client does. Yapfast makes an RSA key and
//! hands Discord its public half. Discord proves it can reach Yapfast by sending
//! a nonce encrypted to the key, then gives a fingerprint, which the code
//! carries as `https://discord.com/ra/{fingerprint}`. Scanning it with the
//! phone app sends who scanned, and confirming there sends a ticket, which
//! `POST /users/@me/remote-auth/login` trades for the login, encrypted to the
//! key again. The gateway closes the connection after a couple of minutes
//! without a scan, and a new code needs a new connection.

use std::{fmt, time::Duration};

use tokio::time::Interval;

use anyhow::{Context, Result, anyhow, bail};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use futures_util::{SinkExt, StreamExt};
use rsa::{Oaep, RsaPrivateKey, pkcs8::EncodePublicKey, rand_core::OsRng};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::{HeaderValue, header},
};

use crate::{
    api::Api,
    captcha::{self, Captcha},
    client, ready, websocket,
};

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

const URL: &str = "wss://remote-auth-gateway.discord.gg/?v=2";
/// The key size Discord's web client uses.
const KEY_BITS: usize = 2048;

/// How far linking got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// The code to show, as the address the phone app scans.
    Code(String),
    /// Someone scanned it, and their app asks them to confirm.
    Scanned { name: String, avatar: String },
    /// They confirmed, and Discord wants a captcha solved first: the page
    /// with it, to open in the browser.
    Captcha(String),
}

/// Why linking stopped without a login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// The gateway closed the connection, as it does a while after the code
    /// shows. A new link shows a new code.
    Expired,
    /// The person said no on their phone.
    Cancelled,
}

impl fmt::Display for Stopped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Stopped::Expired => "The code expired.",
            Stopped::Cancelled => "Linking was cancelled on the phone.",
        })
    }
}

impl std::error::Error for Stopped {}

/// Links an account: tells `step` what to show as it goes, and returns the
/// login once the person confirms on their phone. Errors are [`Stopped`]
/// when the code expired or the person said no.
pub async fn link(mut step: impl FnMut(Step)) -> Result<String> {
    Api::anonymous()?.learn_build_number().await;
    let key = tokio::task::spawn_blocking(|| RsaPrivateKey::new(&mut OsRng, KEY_BITS))
        .await?
        .context("could not make a key to link with")?;
    let public = key
        .to_public_key()
        .to_public_key_der()
        .context("could not encode the key to link with")?;
    let url = match client::test_server() {
        Some(server) => {
            let server = server
                .replacen("https://", "wss://", 1)
                .replacen("http://", "ws://", 1);
            format!("{server}/remote-auth/?v=2")
        }
        None => URL.to_owned(),
    };
    let mut request = url.into_client_request()?;
    let headers = request.headers_mut();
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(client::user_agent()),
    );
    headers.insert(
        header::ORIGIN,
        HeaderValue::from_static("https://discord.com"),
    );
    let (mut socket, _) = websocket::connect(request)
        .await
        .context("Can't reach Discord's login service.")?;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(40));
    heartbeat.tick().await;
    loop {
        let message = tokio::select! {
            _ = heartbeat.tick() => {
                socket.send(text(json!({"op": "heartbeat"}))).await?;
                continue;
            }
            message = socket.next() => message,
        };
        let message = match message {
            Some(Ok(Message::Text(message))) => message,
            Some(Ok(Message::Close(_))) | None => return Err(Stopped::Expired.into()),
            Some(Ok(_)) => continue,
            Some(Err(error)) => {
                tracing::debug!(%error, "Discord's login service dropped the connection");
                return Err(Stopped::Expired.into());
            }
        };
        let message: Value = serde_json::from_str(&message)
            .context("could not understand Discord's login service")?;
        match message["op"].as_str().unwrap_or_default() {
            "hello" => {
                if let Some(interval) = message["heartbeat_interval"].as_u64() {
                    let interval = Duration::from_millis(interval.max(1_000));
                    heartbeat =
                        tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
                }
                let init =
                    json!({"op": "init", "encoded_public_key": STANDARD.encode(public.as_bytes())});
                socket.send(text(init)).await?;
            }
            "nonce_proof" => {
                let nonce = decrypt(&key, &message["encrypted_nonce"])?;
                let proof = URL_SAFE_NO_PAD.encode(Sha256::digest(&nonce));
                socket
                    .send(text(json!({"op": "nonce_proof", "proof": proof})))
                    .await?;
            }
            "pending_remote_init" => {
                let fingerprint = message["fingerprint"]
                    .as_str()
                    .context("Discord's login service sent no code")?;
                step(Step::Code(format!("https://discord.com/ra/{fingerprint}")));
            }
            "pending_ticket" => {
                let payload = decrypt(&key, &message["encrypted_user_payload"])?;
                step(scanned(&String::from_utf8_lossy(&payload)));
            }
            "pending_login" => {
                let ticket = message["ticket"]
                    .as_str()
                    .context("Discord's login service sent no ticket")?;
                let encrypted = trade(ticket, &mut socket, &mut heartbeat, &mut step).await?;
                let _ = socket.close(None).await;
                return login(&key, &Value::String(encrypted));
            }
            // The first version of the protocol sent the login itself.
            "pending_finish" => {
                let _ = socket.close(None).await;
                return login(&key, &message["encrypted_token"]);
            }
            "cancel" => return Err(Stopped::Cancelled.into()),
            _ => {}
        }
    }
}

/// Trades a confirmed login's ticket for the login, showing Discord's
/// captcha when it asks for one. The login service hears heartbeats while
/// someone solves it, as it would while the code shows.
async fn trade(
    ticket: &str,
    socket: &mut Socket,
    heartbeat: &mut Interval,
    step: &mut impl FnMut(Step),
) -> Result<String> {
    let api = Api::anonymous()?;
    let mut solved: Option<(Captcha, String)> = None;
    for shown in 0..=captcha::TRIES {
        let solution = solved
            .as_ref()
            .map(|(captcha, solution)| (captcha, solution.as_str()));
        let error = match api.remote_auth_login(ticket, solution).await {
            Ok(encrypted) => return Ok(encrypted),
            Err(error) => error,
        };
        let Some(captcha) = error.downcast_ref::<Captcha>().cloned() else {
            return Err(error);
        };
        if shown == captcha::TRIES {
            bail!("Discord didn't take the captcha. Try again.");
        }
        let page = captcha::Page::serve(&captcha).await?;
        step(Step::Captcha(page.url.clone()));
        let solution = page.solution();
        tokio::pin!(solution);
        let mut open = true;
        let solution = loop {
            tokio::select! {
                solution = &mut solution => break solution?,
                _ = heartbeat.tick(), if open => {
                    open = socket.send(text(json!({"op": "heartbeat"}))).await.is_ok();
                }
                message = socket.next(), if open => open = matches!(message, Some(Ok(_))),
            }
        };
        solved = Some((captcha, solution));
    }
    unreachable!("the last captcha returns")
}

fn text(value: Value) -> Message {
    Message::Text(value.to_string().into())
}

/// Something Discord encrypted to the key, from base64.
fn decrypt(key: &RsaPrivateKey, encrypted: &Value) -> Result<Vec<u8>> {
    let encrypted = STANDARD
        .decode(
            encrypted
                .as_str()
                .context("Discord's login service sent nothing to read")?,
        )
        .context("could not read Discord's login service")?;
    key.decrypt(Oaep::new::<Sha256>(), &encrypted)
        .map_err(|_| anyhow!("could not read what Discord's login service sent"))
}

fn login(key: &RsaPrivateKey, encrypted: &Value) -> Result<String> {
    let token = String::from_utf8(decrypt(key, encrypted)?)
        .context("Discord's login service sent a login Yapfast can't read")?;
    if token.is_empty() || token.contains(char::is_whitespace) {
        bail!("Discord's login service sent a login Yapfast can't read");
    }
    Ok(token)
}

/// Who scanned, from `id:discriminator:avatar:username`.
fn scanned(payload: &str) -> Step {
    let mut parts = payload.splitn(4, ':');
    let id = parts.next().unwrap_or_default();
    let _discriminator = parts.next();
    let avatar = parts.next().filter(|hash| !hash.is_empty() && *hash != "0");
    let name = parts.next().unwrap_or_default().to_owned();
    Step::Scanned {
        name,
        avatar: ready::avatar(id, avatar),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_who_scanned() {
        assert_eq!(
            scanned("1400000000000000001:0:alice1:alice"),
            Step::Scanned {
                name: "alice".into(),
                avatar: ready::avatar("1400000000000000001", Some("alice1")),
            }
        );
        let Step::Scanned { avatar, .. } = scanned("1400000000000000001:0::bob:by") else {
            unreachable!()
        };
        assert_eq!(avatar, ready::avatar("1400000000000000001", None));
    }

    #[test]
    fn decrypts_what_discord_encrypted_to_the_key() {
        let key = RsaPrivateKey::new(&mut OsRng, 1024).unwrap();
        let sealed = key
            .to_public_key()
            .encrypt(&mut OsRng, Oaep::new::<Sha256>(), b"yapfast-token")
            .unwrap();
        let sealed = Value::String(STANDARD.encode(sealed));
        assert_eq!(login(&key, &sealed).unwrap(), "yapfast-token");
        assert!(decrypt(&key, &Value::String("not base64!".into())).is_err());
    }
}
