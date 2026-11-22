//! `net.socket`: a raw TCP connection out, with TLS over it or a WebSocket on
//! it, that the parent opens and runs for a plugin.
//!
//! The guest still never holds a socket. `net.connect` is answered once it is
//! open, and everything the connection hears comes to the plugin as further
//! answers to that call ([`SocketNews`]); `net.send` and `net.close` go the
//! other way. A connection is a task here, so a slow far end waits on nothing
//! but itself.
//!
//! What is checked is what keeps Yapfast and the person's machine standing,
//! not what a plugin may say: every address a name resolves to is checked
//! before any is dialled, so a name that answers differently the second time
//! finds nothing to reach; this computer and its network are reached only when
//! the manifest says `socket-local`, which the red install box then says in
//! words; and Discord's own hosts are refused, as the network gate refuses
//! them, because Discord is reached through Yapfast. How many bytes, how fast
//! and for how long are the plugin's business: the install review says it can
//! open raw connections to any server, and the person agreed to that.

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use yapfast_plugin_api::call::{NetConnect, SocketNews};
use yapfast_plugin_api::limits;

/// The largest WebSocket message a connection takes: its base64 has to fit one
/// answer on the pipe, which is a frame of [`yapfast_plugin_api::frame::MAX_FRAME`].
pub const MAX_WEBSOCKET_MESSAGE: usize = 512 << 10;

/// What the plugin asks of a connection it holds.
#[derive(Debug)]
pub enum Out {
    Bytes(Vec<u8>),
    Text(String),
    Close,
}

/// One open connection, as the bridge holds it. Dropping it stops the task.
pub struct Handle {
    pub out: mpsc::UnboundedSender<Out>,
    task: tokio::task::AbortHandle,
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// How a connection tells its plugin what happened.
pub type Deliver = Arc<dyn Fn(SocketNews) + Send + Sync>;

/// Whether `address` may be dialled: a public one always, and this computer
/// or its network only for a plugin whose manifest says `socket-local`.
pub fn reachable(address: IpAddr, local: bool) -> Result<(), String> {
    match crate::network::address_problem(address) {
        None => Ok(()),
        Some(_) if local => Ok(()),
        Some(why) => Err(format!(
            "{address} {why}, and this plugin's manifest doesn't say `socket-local`, which \
             reaching this computer or its network needs"
        )),
    }
}

/// Opens a connection for a plugin on a task of its own, and answers through
/// `deliver`: [`SocketNews::Connected`] first, then what it hears, and
/// [`SocketNews::Closed`] last, whatever happens.
pub fn open(socket: u64, request: NetConnect, local: bool, deliver: Deliver) -> Handle {
    let (out, mut asked) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let wait = Duration::from_secs(limits::SOCKET_CONNECT_SECONDS);
        let reason = match tokio::time::timeout(wait, connect(&request, local)).await {
            Err(_) => format!(
                "it didn't open in {} seconds",
                limits::SOCKET_CONNECT_SECONDS
            ),
            Ok(Err(why)) => why,
            Ok(Ok((connection, address))) => {
                deliver(SocketNews::Connected { socket, address });
                run(socket, connection, &mut asked, &deliver).await
            }
        };
        deliver(SocketNews::Closed { socket, reason });
    });
    Handle {
        out,
        task: task.abort_handle(),
    }
}

/// A byte stream, plain or under TLS.
trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

enum Connection {
    Raw(Box<dyn Stream>),
    WebSocket(Box<tokio_tungstenite::WebSocketStream<Box<dyn Stream>>>),
}

/// The host a request names, checked to be a name it can dial.
pub fn checked_host(host: &str) -> Result<String, String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.len() > 253 || host.contains(['/', '@', ' ']) {
        return Err(format!("`{host}` isn't a host name or an address"));
    }
    if crate::network::is_discord_host(&host) {
        return Err(
            "Discord's own servers are reached through Yapfast, never through a plugin's socket"
                .to_owned(),
        );
    }
    Ok(host)
}

async fn connect(request: &NetConnect, local: bool) -> Result<(Connection, String), String> {
    let host = checked_host(&request.host)?;
    if request.port == 0 {
        return Err("port 0 can't be connected to".to_owned());
    }
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), request.port))
        .await
        .map_err(|error| format!("`{host}` couldn't be looked up: {error}"))?
        .collect();
    if addresses.is_empty() {
        return Err(format!("`{host}` has no address"));
    }
    // Every address is checked before one is dialled: a name that answers a
    // public address and a private one is refused whole.
    for address in &addresses {
        reachable(address.ip(), local)?;
    }
    let mut last = String::new();
    let mut opened = None;
    for address in &addresses {
        match tokio::net::TcpStream::connect(address).await {
            Ok(tcp) => {
                opened = Some((tcp, *address));
                break;
            }
            Err(error) => last = format!("{address}: {error}"),
        }
    }
    let (tcp, address) = opened.ok_or_else(|| format!("nothing answered at `{host}` ({last})"))?;
    let _ = tcp.set_nodelay(true);
    let stream: Box<dyn Stream> = if request.tls {
        let name = rustls::pki_types::ServerName::try_from(host.clone())
            .map_err(|error| format!("`{host}` can't be checked for TLS: {error}"))?;
        let tls = tokio_rustls::TlsConnector::from(tls_config()?)
            .connect(name, tcp)
            .await
            .map_err(|error| format!("TLS with `{host}` failed: {error}"))?;
        Box::new(tls)
    } else {
        Box::new(tcp)
    };
    let Some(path) = request.websocket.as_deref() else {
        return Ok((Connection::Raw(stream), address.to_string()));
    };
    if !path.starts_with('/') {
        return Err("a WebSocket's path starts with `/`".to_owned());
    }
    let scheme = if request.tls { "wss" } else { "ws" };
    let named = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.clone()
    };
    let url = format!("{scheme}://{named}:{}{path}", request.port);
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_WEBSOCKET_MESSAGE))
        .max_frame_size(Some(MAX_WEBSOCKET_MESSAGE));
    let (socket, _) = tokio_tungstenite::client_async_with_config(url, stream, Some(config))
        .await
        .map_err(|error| format!("the WebSocket at `{host}` didn't open: {error}"))?;
    Ok((Connection::WebSocket(Box::new(socket)), address.to_string()))
}

/// The one TLS setup every connection shares, trusting what the system trusts,
/// as Discord's WebSockets do.
fn tls_config() -> Result<Arc<rustls::ClientConfig>, String> {
    use rustls_platform_verifier::ConfigVerifierExt;
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    if let Some(config) = CONFIG.get() {
        return Ok(config.clone());
    }
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let config = Arc::new(
        rustls::ClientConfig::with_platform_verifier()
            .map_err(|error| format!("TLS couldn't be set up: {error}"))?,
    );
    Ok(CONFIG.get_or_init(|| config).clone())
}

fn data(socket: u64, bytes: &[u8]) -> SocketNews {
    SocketNews::Data {
        socket,
        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    }
}

/// Carries a connection until either end closes it; the reason it ended.
async fn run(
    socket: u64,
    connection: Connection,
    asked: &mut mpsc::UnboundedReceiver<Out>,
    deliver: &Deliver,
) -> String {
    match connection {
        Connection::Raw(mut stream) => {
            let mut buffer = vec![0u8; limits::MAX_SOCKET_CHUNK_BYTES];
            loop {
                tokio::select! {
                    read = stream.read(&mut buffer) => match read {
                        Ok(0) => return "the other end closed it".to_owned(),
                        Ok(count) => deliver(data(socket, &buffer[..count])),
                        Err(error) => return format!("reading from it failed: {error}"),
                    },
                    out = asked.recv() => {
                        let bytes = match out {
                            None | Some(Out::Close) => {
                                let _ = stream.shutdown().await;
                                return "the plugin closed it".to_owned();
                            }
                            Some(Out::Bytes(bytes)) => bytes,
                            Some(Out::Text(text)) => text.into_bytes(),
                        };
                        if let Err(error) = stream.write_all(&bytes).await {
                            return format!("writing to it failed: {error}");
                        }
                    }
                }
            }
        }
        Connection::WebSocket(mut stream) => loop {
            tokio::select! {
                heard = stream.next() => match heard {
                    None => return "the other end closed it".to_owned(),
                    Some(Err(error)) => return format!("the WebSocket failed: {error}"),
                    Some(Ok(Message::Text(text))) => deliver(SocketNews::Text {
                        socket,
                        text: text.as_str().to_owned(),
                    }),
                    Some(Ok(Message::Binary(bytes))) => deliver(data(socket, &bytes)),
                    Some(Ok(Message::Close(frame))) => {
                        return match frame {
                            Some(frame) if !frame.reason.is_empty() => {
                                format!("the other end closed it: {}", frame.reason.as_str())
                            }
                            _ => "the other end closed it".to_owned(),
                        };
                    }
                    // Pings are answered by the library; pongs and raw frames
                    // say nothing a plugin asked for.
                    Some(Ok(_)) => {}
                },
                out = asked.recv() => {
                    let sent = match out {
                        None | Some(Out::Close) => {
                            let _ = SinkExt::close(&mut stream).await;
                            return "the plugin closed it".to_owned();
                        }
                        Some(Out::Bytes(bytes)) => stream.send(Message::Binary(bytes.into())).await,
                        Some(Out::Text(text)) => stream.send(Message::Text(text.into())).await,
                    };
                    if let Err(error) = sent {
                        return format!("sending on the WebSocket failed: {error}");
                    }
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn collect() -> (Deliver, Arc<Mutex<Vec<SocketNews>>>) {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&heard);
        (Arc::new(move |news| kept.lock().unwrap().push(news)), heard)
    }

    async fn until_closed(heard: &Arc<Mutex<Vec<SocketNews>>>) {
        for _ in 0..200 {
            if heard
                .lock()
                .unwrap()
                .iter()
                .any(|news| matches!(news, SocketNews::Closed { .. }))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the connection never closed");
    }

    #[test]
    fn this_computer_is_reached_only_when_the_manifest_says_so() {
        let loopback: IpAddr = "127.0.0.1".parse().unwrap();
        let private: IpAddr = "192.168.1.4".parse().unwrap();
        let public: IpAddr = "93.184.215.14".parse().unwrap();
        assert!(reachable(public, false).is_ok());
        let refused = reachable(loopback, false).unwrap_err();
        assert!(refused.contains("socket-local"), "{refused}");
        assert!(reachable(private, false).is_err());
        assert!(reachable(loopback, true).is_ok());
        assert!(reachable(private, true).is_ok());
    }

    #[test]
    fn discord_is_reached_through_yapfast() {
        assert!(checked_host("gateway.discord.gg").is_err());
        assert!(checked_host("Example.COM.").unwrap() == "example.com");
        assert!(checked_host("a b").is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_connection_carries_bytes_both_ways_and_says_when_it_closes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut peer, _) = listener.accept().await.unwrap();
            let mut got = [0u8; 4];
            peer.read_exact(&mut got).await.unwrap();
            peer.write_all(b"pong").await.unwrap();
        });
        let (deliver, heard) = collect();
        let request = NetConnect {
            host: "127.0.0.1".into(),
            port,
            tls: false,
            websocket: None,
        };
        let handle = open(7, request, true, deliver);
        handle.out.send(Out::Text("ping".into())).unwrap();
        until_closed(&heard).await;
        let heard = heard.lock().unwrap();
        assert!(matches!(heard[0], SocketNews::Connected { socket: 7, .. }));
        assert!(
            heard.iter().any(|news| *news == data(7, b"pong")),
            "{heard:?}"
        );
        assert!(matches!(
            heard.last(),
            Some(SocketNews::Closed { socket: 7, .. })
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn loopback_without_socket_local_is_refused_before_anything_is_dialled() {
        let (deliver, heard) = collect();
        let request = NetConnect {
            host: "127.0.0.1".into(),
            port: 9,
            tls: false,
            websocket: None,
        };
        let _handle = open(1, request, false, deliver);
        until_closed(&heard).await;
        let heard = heard.lock().unwrap();
        assert_eq!(heard.len(), 1, "never connected: {heard:?}");
        let SocketNews::Closed { reason, .. } = &heard[0] else {
            panic!("{heard:?}");
        };
        assert!(reason.contains("socket-local"), "{reason}");
    }
}
