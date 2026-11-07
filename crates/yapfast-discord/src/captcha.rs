//! Discord's captchas, which it asks for before a risky request such as
//! trading a QR login's ticket: hCaptcha, with a site key and request data
//! that change each time. Yapfast has no web view, so it serves a page with
//! the challenge on this machine's loopback address and opens it in the
//! browser. The page tells hCaptcha it's on discord.com, as hCaptcha's
//! mobile SDKs name their host, and posts the solution back. The request is
//! then sent again with it in `X-Captcha-Key`; see
//! https://docs.discord.food/topics/captcha-handling.
//!
//! The listener lives only while the page waits, so nothing stays running
//! once the captcha is solved or linking stops.

use std::{fmt, time::Duration};

use anyhow::{Context, Result, bail};
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

/// How long the page waits for someone to solve it.
const WAIT: Duration = Duration::from_secs(10 * 60);
/// The most a request to the page may send: the solution is a few KiB.
const MOST: usize = 64 * 1024;

/// A captcha Discord asked for, from its answer's body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Captcha {
    pub sitekey: String,
    /// Data the challenge must carry, when Discord sends it.
    pub rqdata: Option<String>,
    /// What the retried request must carry back, when Discord sends them.
    pub rqtoken: Option<String>,
    pub session_id: Option<String>,
}

impl fmt::Display for Captcha {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Discord asked for a captcha")
    }
}

impl std::error::Error for Captcha {}

impl Captcha {
    /// The hCaptcha a refusal's body asks for. Other services, which
    /// Discord doesn't use now, aren't solved.
    pub fn from_body(body: &Value) -> Option<Self> {
        if !body["captcha_key"].is_array() || body["captcha_service"] != "hcaptcha" {
            return None;
        }
        let text = |key: &str| body[key].as_str().map(str::to_owned);
        Some(Self {
            sitekey: text("captcha_sitekey")?,
            rqdata: text("captcha_rqdata"),
            rqtoken: text("captcha_rqtoken"),
            session_id: text("captcha_session_id"),
        })
    }

    /// The headers a retried request carries with `solution`.
    pub fn headers<'a>(&'a self, solution: &'a str) -> Vec<(&'static str, &'a str)> {
        let mut headers = vec![("X-Captcha-Key", solution)];
        if let Some(rqtoken) = &self.rqtoken {
            headers.push(("X-Captcha-Rqtoken", rqtoken));
        }
        if let Some(session) = &self.session_id {
            headers.push(("X-Captcha-Session-Id", session));
        }
        headers
    }
}

/// How many captchas a request shows before giving up: Discord asks again
/// when it doesn't take a solution.
pub const TRIES: usize = 3;

/// Sends a request until Discord takes it, showing each captcha it asks for
/// first. `send` sends the request, with the last captcha's solution after
/// the first try, and `shown` hears the address of each captcha's page,
/// which waits until it's solved.
pub async fn solving<T, Sent: Future<Output = Result<T>>>(
    mut send: impl FnMut(Option<(Captcha, String)>) -> Sent,
    mut shown: impl FnMut(&str),
) -> Result<T> {
    let mut solved: Option<(Captcha, String)> = None;
    for tries in 0..=TRIES {
        let error = match send(solved.take()).await {
            Ok(answer) => return Ok(answer),
            Err(error) => error,
        };
        let Some(captcha) = error.downcast_ref::<Captcha>().cloned() else {
            return Err(error);
        };
        if tries == TRIES {
            bail!("Discord didn't take the captcha. Try again.");
        }
        let page = Page::serve(&captcha).await?;
        shown(&page.url);
        solved = Some((captcha, page.solution().await?));
    }
    unreachable!("the last captcha returns")
}

/// The page with a captcha, served until it's solved.
pub struct Page {
    /// Where to open it.
    pub url: String,
    path: String,
    html: String,
    listener: TcpListener,
}

impl Page {
    /// Serves `captcha` on the loopback address, at a path no other page
    /// can guess.
    pub async fn serve(captcha: &Captcha) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .context("could not show the captcha")?;
        let port = listener.local_addr()?.port();
        let path = format!("/captcha/{:032x}", rand::random::<u128>());
        Ok(Self {
            url: format!("http://127.0.0.1:{port}{path}"),
            html: page(captcha),
            path,
            listener,
        })
    }

    /// Waits for the solution: the page is sent to whoever asks for it,
    /// and the solution comes back as a POST to the same path.
    pub async fn solution(&self) -> Result<String> {
        tokio::time::timeout(WAIT, async {
            loop {
                let (stream, _) = self.listener.accept().await?;
                match self.answer(stream).await {
                    Ok(Some(solution)) => return Ok(solution),
                    Ok(None) => {}
                    Err(error) => tracing::debug!(%error, "a captcha page request failed"),
                }
            }
        })
        .await
        .context("Nobody solved the captcha in time. Try again.")?
    }

    /// Answers one request, giving the solution if it brought one.
    async fn answer(&self, mut stream: TcpStream) -> Result<Option<String>> {
        let request = read(&mut stream).await?;
        let (head, body) = request
            .split_once("\r\n\r\n")
            .context("an incomplete request")?;
        let mut words = head.split(' ');
        let (method, path) = (words.next(), words.next());
        if path != Some(self.path.as_str()) {
            respond(&mut stream, "404 Not Found", "text/plain", "").await?;
            return Ok(None);
        }
        match method {
            Some("GET") => {
                respond(
                    &mut stream,
                    "200 OK",
                    "text/html; charset=utf-8",
                    &self.html,
                )
                .await?;
                Ok(None)
            }
            Some("POST") if !body.trim().is_empty() => {
                respond(&mut stream, "204 No Content", "text/plain", "").await?;
                Ok(Some(body.trim().to_owned()))
            }
            _ => {
                respond(&mut stream, "400 Bad Request", "text/plain", "").await?;
                Ok(None)
            }
        }
    }
}

/// A whole request: its head, and as much body as it says it has.
async fn read(stream: &mut TcpStream) -> Result<String> {
    let mut request = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            bail!("the request ended early");
        }
        request.extend_from_slice(&chunk[..read]);
        if request.len() > MOST {
            bail!("the request is too large");
        }
        let text = String::from_utf8_lossy(&request);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if body.len() >= length {
                return Ok(text.into_owned());
            }
        }
    }
}

async fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &str) -> Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

/// The page: hCaptcha's widget for `captcha`, which posts its solution
/// back to the address it came from.
fn page(captcha: &Captcha) -> String {
    // As a JavaScript string, which can't end the script element early.
    let script = |value: &str| {
        serde_json::to_string(value)
            .unwrap_or_default()
            .replace("</", "<\\/")
    };
    let rqdata = captcha.rqdata.as_deref().map_or("null".to_owned(), script);
    format!(
        r#"<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width">
<title>Link Discord to Yapfast</title>
<style>
  body {{ margin: 0; min-height: 100vh; display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 16px;
    background: #1e1f22; color: #dbdee1; font: 15px system-ui, sans-serif; }}
  h1 {{ font-size: 20px; margin: 0; }}
  p {{ margin: 0; color: #b5bac1; }}
</style>
<h1>Link Discord to Yapfast</h1>
<p id="note">Discord wants to check you're a person before it links your account.</p>
<div id="captcha"></div>
<script>
  const sitekey = {sitekey};
  const rqdata = {rqdata};
  const note = document.getElementById("note");
  function ready() {{
    const id = hcaptcha.render("captcha", {{ sitekey, theme: "dark", callback: solved }});
    if (rqdata) hcaptcha.setData(id, {{ rqdata }});
  }}
  async function solved(solution) {{
    note.textContent = "Sending it to Yapfast...";
    try {{
      const answer = await fetch(location.pathname, {{ method: "POST", body: solution }});
      note.textContent = answer.ok
        ? "Done. You can close this page and go back to Yapfast."
        : "RatFast didn't take it. Start linking again in Yapfast.";
    }} catch {{
      note.textContent = "RatFast stopped waiting. Start linking again in Yapfast.";
    }}
  }}
</script>
<script src="https://js.hcaptcha.com/1/api.js?render=explicit&onload=ready&recaptchacompat=off&host=discord.com" async defer></script>
</html>
"#,
        sitekey = script(&captcha.sitekey),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn captcha() -> Captcha {
        Captcha::from_body(&json!({
            "captcha_key": ["captcha-required"],
            "captcha_service": "hcaptcha",
            "captcha_sitekey": "a9b5fb07-92ff-493f-86fe-352a2803b3df",
            "captcha_rqdata": "data</script>",
            "captcha_rqtoken": "token",
            "captcha_session_id": "session",
        }))
        .unwrap()
    }

    #[test]
    fn reads_discords_captcha() {
        let captcha = captcha();
        assert_eq!(captcha.rqdata.as_deref(), Some("data</script>"));
        assert_eq!(
            captcha.headers("solved"),
            [
                ("X-Captcha-Key", "solved"),
                ("X-Captcha-Rqtoken", "token"),
                ("X-Captcha-Session-Id", "session"),
            ]
        );
        assert_eq!(
            Captcha::from_body(&json!({"captcha_key": ["x"], "captcha_service": "recaptcha"})),
            None
        );
        assert_eq!(Captcha::from_body(&json!({"message": "no"})), None);
    }

    #[tokio::test]
    async fn serves_the_page_and_takes_the_solution() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let page = Page::serve(&captcha()).await.unwrap();
        let url = page.url.clone();
        let solution = tokio::spawn(async move { page.solution().await });
        let client = reqwest::Client::new();
        let html = client.get(&url).send().await.unwrap().text().await.unwrap();
        assert!(html.contains("host=discord.com"), "{html}");
        assert!(html.contains(r#""data<\/script>""#), "{html}");
        let elsewhere = url.rsplit_once('/').unwrap().0.to_owned() + "/guess";
        let status = client.get(elsewhere).send().await.unwrap().status();
        assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
        client.post(&url).body("solved").send().await.unwrap();
        assert_eq!(solution.await.unwrap().unwrap(), "solved");
    }
}
