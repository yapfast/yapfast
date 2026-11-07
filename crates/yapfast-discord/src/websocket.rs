//! Opening WebSockets to Discord: linking, the gateway and voice. They trust
//! the certificates the system trusts, as HTTPS requests do, so a CA added
//! for a reverse proxy works for both.

use std::sync::{Arc, OnceLock};

use rustls::ClientConfig;
use rustls_platform_verifier::ConfigVerifierExt;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    Connector, MaybeTlsStream, WebSocketStream, connect_async_tls_with_config,
    tungstenite::{self, client::IntoClientRequest, error::TlsError, handshake::client::Response},
};

/// Opens a WebSocket to `request`.
pub(crate) async fn connect(
    request: impl IntoClientRequest + Unpin,
) -> tungstenite::Result<(WebSocketStream<MaybeTlsStream<TcpStream>>, Response)> {
    let connector = Connector::Rustls(config().map_err(TlsError::Rustls)?);
    connect_async_tls_with_config(request, None, false, Some(connector)).await
}

/// The one TLS setup every socket shares: on Linux, making it reads and parses
/// the system's certificates. Without one, every reconnect would.
fn config() -> Result<Arc<ClientConfig>, rustls::Error> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    if let Some(config) = CONFIG.get() {
        return Ok(config.clone());
    }
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let config = Arc::new(ClientConfig::with_platform_verifier()?);
    Ok(CONFIG.get_or_init(|| config).clone())
}
