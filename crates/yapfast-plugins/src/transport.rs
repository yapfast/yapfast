//! The socket a plugin's request actually opens.
//!
//! The gate decides and resolves; this does what the plan says and nothing else.
//! That is the whole point of the split: the address is chosen once, checked,
//! and then pinned, so a second lookup — the one a DNS-rebinding server is
//! waiting for — can't move the connection somewhere the gate never saw.
//!
//! One connection per request, nothing kept between them: no cookie jar, no
//! pool, and no proxy, because a proxy picks the address and the gate already
//! did.

use std::net::SocketAddr;

use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;

use crate::network::{ConnectPlan, Served, Transport};

/// How long a plugin's request may take to connect.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// How long a plugin's request may take altogether, body included: a 16 MiB
/// answer, the most a manifest may ask for, at well under 1 Mbit/s.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// How much of a streamed file is read from disk at once.
const FILE_PIECE: usize = 256 * 1024;

/// A file body as it goes: what comes before it, the file a piece at a time,
/// each piece read off the runtime's threads, and what comes after. At most
/// one piece is in memory, and no more than the size the gate was told is
/// sent, so a file that grows while it uploads can't outrun its length.
fn file_stream(
    body: std::sync::Arc<crate::network::FileBody>,
) -> BoxStream<'static, std::io::Result<Vec<u8>>> {
    use std::io::Read;
    let before = futures_util::stream::iter([Ok(body.before.clone())]);
    let after = futures_util::stream::iter([Ok(body.after.clone())]);
    let start = (body.path.clone(), None::<std::fs::File>, body.size);
    let pieces = futures_util::stream::unfold(Some(start), |state| async move {
        let (path, file, left) = state?;
        if left == 0 {
            return None;
        }
        let read = tokio::task::spawn_blocking(move || {
            let mut file = match file {
                Some(file) => file,
                None => std::fs::File::open(&path)?,
            };
            let mut piece = vec![0; FILE_PIECE.min(left as usize)];
            let read = file.read(&mut piece)?;
            if read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "the file got shorter while it was being sent",
                ));
            }
            piece.truncate(read);
            Ok((path, file, piece))
        })
        .await;
        match read {
            Ok(Ok((path, file, piece))) => {
                let left = left - piece.len() as u64;
                Some((Ok(piece), Some((path, Some(file), left))))
            }
            Ok(Err(error)) => Some((Err(error), None)),
            Err(error) => Some((Err(std::io::Error::other(error)), None)),
        }
    });
    before.chain(pieces).chain(after).boxed()
}

/// Opens one connection per request, to the address the gate resolved.
#[derive(Debug, Default)]
pub struct PinnedTransport;

impl Transport for PinnedTransport {
    fn send(&self, plan: ConnectPlan) -> BoxFuture<'static, Result<Served>> {
        Box::pin(async move {
            // One of the addresses the gate checked, and no other: the plan
            // says which, and connecting to a name instead is the bug this
            // transport exists to make impossible.
            let address = *plan
                .addresses
                .first()
                .ok_or_else(|| anyhow!("`{}` has no address to connect to", plan.host))?;
            let client = reqwest::Client::builder()
                // A proxy would resolve the name again and pick the address.
                .no_proxy()
                // The gate follows redirects itself, one checked hop at a time.
                .redirect(reqwest::redirect::Policy::none())
                .resolve(plan.host.as_str(), SocketAddr::new(address, plan.port))
                .https_only(true)
                .pool_max_idle_per_host(0)
                // A server that accepts and never answers would otherwise hold
                // one of the plugin's four places, and one of sixteen in all,
                // for as long as the app runs.
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .build()
                .context("the plugin's HTTP client couldn't be built")?;

            let url = format!("https://{}:{}{}", plan.host, plan.port, plan.path);
            let method = reqwest::Method::from_bytes(plan.method.as_bytes())
                .with_context(|| format!("`{}` isn't an HTTP method", plan.method))?;
            let mut request = client.request(method, url);
            // A form made here says its own type, with its boundary.
            let own_type = plan
                .file
                .as_ref()
                .is_some_and(|file| file.content_type.is_some());
            for (name, value) in &plan.headers {
                // reqwest writes these from the request it is building, and a
                // plan's copy would either double them or contradict it.
                if name.eq_ignore_ascii_case("host")
                    || name.eq_ignore_ascii_case("content-length")
                    || own_type && name.eq_ignore_ascii_case("content-type")
                {
                    continue;
                }
                request = request.header(name.as_str(), value.as_str());
            }
            if let Some(body) = &plan.body {
                request = request.body(body.to_vec());
            }
            if let Some(file) = &plan.file {
                // A stream says no length of its own, and many file hosts
                // refuse a chunked upload, so the length goes as a header.
                request = request.header(reqwest::header::CONTENT_LENGTH, file.length());
                if let Some(kind) = &file.content_type {
                    request = request.header(reqwest::header::CONTENT_TYPE, kind.as_str());
                }
                request = request.body(reqwest::Body::wrap_stream(file_stream(file.clone())));
            }

            let response = request.send().await.context("the request failed")?;
            let status = response.status().as_u16();
            // What answered, which after a redirect is the last hop's name.
            let url = response.url().to_string();
            let headers = response
                .headers()
                .iter()
                .filter_map(|(name, value)| {
                    value
                        .to_str()
                        .ok()
                        .map(|value| (name.to_string(), value.to_owned()))
                })
                .collect();
            // Streamed, so a body over its cap is stopped by dropping this
            // rather than by having already held all of it.
            let body: BoxStream<'static, Result<Vec<u8>>> = response
                .bytes_stream()
                .map(|chunk| {
                    chunk
                        .map(|bytes| bytes.to_vec())
                        .map_err(anyhow::Error::from)
                })
                .boxed();
            Ok(Served {
                status,
                headers,
                url,
                body,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file body goes as the bytes before it, the file in pieces and the
    /// bytes after, and no more of the file than the size it was given.
    #[test]
    fn a_file_body_streams_its_form_around_the_file() {
        let folder = tempfile::tempdir().expect("a folder");
        let path = folder.path().join("big.bin");
        let file: Vec<u8> = (0..FILE_PIECE * 2 + 10).map(|n| n as u8).collect();
        std::fs::write(&path, &file).expect("a file");
        let body = std::sync::Arc::new(crate::network::FileBody {
            path,
            size: (FILE_PIECE * 2 + 5) as u64,
            before: b"<".to_vec(),
            after: b">".to_vec(),
            content_type: None,
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let pieces: Vec<Vec<u8>> = runtime.block_on(async {
            file_stream(body)
                .map(|piece| piece.expect("a piece"))
                .collect()
                .await
        });
        let sent: Vec<u8> = pieces.concat();
        assert_eq!(sent.len(), FILE_PIECE * 2 + 5 + 2);
        assert_eq!(sent[0], b'<');
        assert_eq!(&sent[1..sent.len() - 1], &file[..FILE_PIECE * 2 + 5]);
        assert_eq!(*sent.last().unwrap(), b'>');
        assert!(pieces.iter().all(|piece| piece.len() <= FILE_PIECE));
    }
}
