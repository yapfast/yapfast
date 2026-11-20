//! `net.listen`: a small HTTP endpoint on this computer's loopback, for a
//! plugin that serves something local apps call — an MCP server Claude
//! connects to is the case it was made for.
//!
//! The parent listens, never the helper. It binds `127.0.0.1` and `::1` and
//! nothing else: the addresses are constants here, so no manifest, setting or
//! guest can make it listen on a network a stranger is on. Every request must
//! carry the plugin's token, `Authorization: Bearer <token>`, which the person
//! copies from the plugin's page into the app they connect; one without it is
//! answered 401 without the plugin ever hearing of it. The `Host` header must
//! name loopback too, so a web page that rebinds a name of its own to
//! 127.0.0.1 can't reach the endpoint through a person's browser.
//!
//! One request a connection, a head of at most 16 KiB and a body of at most
//! 1 MiB, eight at a time, and an answer within 30 seconds or a 504: a local
//! endpoint is a door, and these are its hinges.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, oneshot};
use yapfast_plugin_api::call::Incoming;

/// The only addresses an endpoint binds.
pub const LOOPBACK: [IpAddr; 2] = [
    IpAddr::V4(Ipv4Addr::LOCALHOST),
    IpAddr::V6(Ipv6Addr::LOCALHOST),
];

const MAX_HEAD_BYTES: usize = 16 << 10;
pub const MAX_BODY_BYTES: usize = 1 << 20;
const MAX_AT_ONCE: usize = 8;
const READ_TIMEOUT: Duration = Duration::from_secs(10);
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

/// What the plugin answers one request with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Hands one request to the plugin; the reply comes back on the sender.
pub type Deliver = Arc<dyn Fn(Incoming, oneshot::Sender<Reply>) + Send + Sync>;

/// A running endpoint: its port, and the tasks that serve it, which stop when
/// this is dropped.
pub struct Endpoint {
    pub port: u16,
    pub addresses: Vec<SocketAddr>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// Binds one loopback address, for the runtime this is called inside.
fn bind(address: SocketAddr) -> std::io::Result<TcpListener> {
    let listener = std::net::TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;
    TcpListener::from_std(listener)
}

/// Starts serving on loopback, on `port` or one the system picks. Called
/// inside a Tokio runtime, whose tasks then serve it.
pub fn start(port: Option<u16>, token: String, deliver: Deliver) -> std::io::Result<Endpoint> {
    let v4 = bind(SocketAddr::new(LOOPBACK[0], port.unwrap_or(0)))?;
    let port = v4.local_addr()?.port();
    // IPv6 loopback on the same port where the system has it; a machine with
    // IPv6 off still serves on 127.0.0.1.
    let v6 = bind(SocketAddr::new(LOOPBACK[1], port)).ok();
    let token: Arc<str> = token.into();
    let gate = Arc::new(Semaphore::new(MAX_AT_ONCE));
    let next = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let mut addresses = vec![v4.local_addr()?];
    let mut tasks = Vec::new();
    for listener in std::iter::once(v4).chain(v6) {
        if let Ok(address) = listener.local_addr()
            && !addresses.contains(&address)
        {
            addresses.push(address);
        }
        let token = Arc::clone(&token);
        let deliver = Arc::clone(&deliver);
        let gate = Arc::clone(&gate);
        let next = Arc::clone(&next);
        tasks.push(tokio::spawn(async move {
            loop {
                let Ok((stream, from)) = listener.accept().await else {
                    continue;
                };
                // Only this computer can reach a loopback socket; a peer that
                // somehow isn't one is not served.
                if !from.ip().is_loopback() {
                    continue;
                }
                let Ok(permit) = Arc::clone(&gate).try_acquire_owned() else {
                    let _ = answer(stream, status(503, "busy")).await;
                    continue;
                };
                let token = Arc::clone(&token);
                let deliver = Arc::clone(&deliver);
                let id = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tokio::spawn(async move {
                    let _permit = permit;
                    serve(stream, &token, deliver, id).await;
                });
            }
        }));
    }
    Ok(Endpoint {
        port,
        addresses,
        tasks,
    })
}

fn status(code: u16, text: &str) -> Reply {
    Reply {
        status: code,
        headers: vec![("content-type".into(), "text/plain; charset=utf-8".into())],
        body: text.as_bytes().to_vec(),
    }
}

/// Compares two secrets in time that doesn't depend on where they differ.
fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Whether a `Host` header names this computer.
fn loopback_host(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        host.rsplit_once(':').map_or(host, |(name, _)| name)
    };
    matches!(
        name.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
}

async fn serve(mut stream: TcpStream, token: &str, deliver: Deliver, id: u64) {
    let request = match tokio::time::timeout(READ_TIMEOUT, read_request(&mut stream)).await {
        Ok(Ok(request)) => request,
        Ok(Err(reply)) => {
            let _ = answer(stream, reply).await;
            return;
        }
        Err(_) => return,
    };
    let header = |name: &str| {
        request
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    };
    if !header("host").is_some_and(loopback_host) {
        let _ = answer(
            stream,
            status(403, "this endpoint answers this computer only"),
        )
        .await;
        return;
    }
    let presented = header("authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !same_secret(presented.trim(), token) {
        let mut reply = status(
            401,
            "the plugin's token is needed: Authorization: Bearer <token>",
        );
        reply
            .headers
            .push(("www-authenticate".into(), "Bearer".into()));
        let _ = answer(stream, reply).await;
        return;
    }
    let incoming = Incoming {
        request: id,
        method: request.method,
        path: request.path,
        headers: request
            .headers
            .into_iter()
            .filter(|(name, _)| !name.eq_ignore_ascii_case("authorization"))
            .collect(),
        body_base64: yapfast_plugin_api::call::base64::encode(&request.body),
    };
    let (sender, receiver) = oneshot::channel();
    deliver(incoming, sender);
    let reply = match tokio::time::timeout(ANSWER_TIMEOUT, receiver).await {
        Ok(Ok(reply)) => reply,
        Ok(Err(_)) => status(503, "the plugin isn't running"),
        Err(_) => status(504, "the plugin didn't answer in time"),
    };
    let _ = answer(stream, reply).await;
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

async fn read_request(stream: &mut TcpStream) -> Result<Request, Reply> {
    let mut buffer = Vec::with_capacity(1024);
    let head_end = loop {
        let mut chunk = [0u8; 4096];
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|_| status(400, "unreadable"))?;
        if read == 0 {
            return Err(status(400, "incomplete request"));
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(at) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break at;
        }
        if buffer.len() > MAX_HEAD_BYTES {
            return Err(status(431, "the request's head is too large"));
        }
    };
    let head = std::str::from_utf8(&buffer[..head_end]).map_err(|_| status(400, "bad head"))?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let method = first.next().unwrap_or_default().to_owned();
    let path = first.next().unwrap_or_default().to_owned();
    if method.is_empty() || !path.starts_with('/') {
        return Err(status(400, "bad request line"));
    }
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect();
    if headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("transfer-encoding"))
    {
        return Err(status(411, "send a Content-Length"));
    }
    let length: usize = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.parse().map_err(|_| status(400, "bad length")))
        .transpose()?
        .unwrap_or(0);
    if length > MAX_BODY_BYTES {
        return Err(status(413, "a request's body is at most 1 MiB"));
    }
    let mut body = buffer[head_end + 4..].to_vec();
    while body.len() < length {
        let mut chunk = vec![0u8; (length - body.len()).min(64 << 10)];
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|_| status(400, "unreadable"))?;
        if read == 0 {
            return Err(status(400, "incomplete body"));
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(length);
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}

async fn answer(mut stream: TcpStream, reply: Reply) -> std::io::Result<()> {
    let reason = match reply.status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        411 => "Length Required",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Status",
    };
    let mut head = format!("HTTP/1.1 {} {reason}\r\n", reply.status);
    for (name, value) in &reply.headers {
        // A header a plugin wrote is one line, or not written at all.
        if name.contains(['\r', '\n', ':']) || value.contains(['\r', '\n']) {
            continue;
        }
        if name.eq_ignore_ascii_case("content-length") || name.eq_ignore_ascii_case("connection") {
            continue;
        }
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!(
        "content-length: {}\r\nconnection: close\r\n\r\n",
        reply.body.len()
    ));
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&reply.body).await?;
    stream.shutdown().await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn ask(port: u16, request: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("a connection");
        stream.write_all(request.as_bytes()).await.expect("sent");
        let mut answer = String::new();
        stream.read_to_string(&mut answer).await.expect("an answer");
        answer
    }

    fn echo() -> Deliver {
        Arc::new(|incoming: Incoming, reply: oneshot::Sender<Reply>| {
            let _ = reply.send(Reply {
                status: 200,
                headers: vec![("content-type".into(), "application/json".into())],
                body: format!("{} {}", incoming.method, incoming.path).into_bytes(),
            });
        })
    }

    #[test]
    fn it_listens_on_loopback_only_and_wants_the_token() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let endpoint = start(None, "secret".into(), echo()).expect("listening");
            assert!(
                endpoint.addresses.iter().all(|address| address.ip().is_loopback()),
                "{:?}",
                endpoint.addresses
            );
            let port = endpoint.port;
            let none = ask(port, "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\n\r\n").await;
            assert!(none.starts_with("HTTP/1.1 401"), "{none}");
            let wrong = ask(
                port,
                "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer secreT\r\nContent-Length: 0\r\n\r\n",
            )
            .await;
            assert!(wrong.starts_with("HTTP/1.1 401"), "{wrong}");
            let rebound = ask(
                port,
                "POST /mcp HTTP/1.1\r\nHost: evil.example\r\nAuthorization: Bearer secret\r\nContent-Length: 0\r\n\r\n",
            )
            .await;
            assert!(rebound.starts_with("HTTP/1.1 403"), "{rebound}");
            let good = ask(
                port,
                "POST /mcp HTTP/1.1\r\nHost: localhost:1\r\nAuthorization: Bearer secret\r\nContent-Length: 2\r\n\r\n{}",
            )
            .await;
            assert!(good.starts_with("HTTP/1.1 200"), "{good}");
            assert!(good.ends_with("POST /mcp"), "{good}");
            let big = ask(
                port,
                &format!(
                    "POST / HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer secret\r\nContent-Length: {}\r\n\r\n",
                    MAX_BODY_BYTES + 1
                ),
            )
            .await;
            assert!(big.starts_with("HTTP/1.1 413"), "{big}");
            drop(endpoint);
        });
    }

    #[test]
    fn a_host_header_names_this_computer_or_is_refused() {
        assert!(loopback_host("localhost"));
        assert!(loopback_host("127.0.0.1:8123"));
        assert!(loopback_host("[::1]:8123"));
        assert!(!loopback_host("0.0.0.0"));
        assert!(!loopback_host("192.168.1.2:8123"));
        assert!(!loopback_host("localhost.evil.example"));
        assert!(
            LOOPBACK.iter().all(IpAddr::is_loopback),
            "no address but loopback is bound"
        );
    }
}
