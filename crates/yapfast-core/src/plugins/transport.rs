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

use crate::plugins::network::{ConnectPlan, Served, Transport};

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
                .build()
                .context("the plugin's HTTP client couldn't be built")?;

            let url = format!("https://{}:{}{}", plan.host, plan.port, plan.path);
            let method = reqwest::Method::from_bytes(plan.method.as_bytes())
                .with_context(|| format!("`{}` isn't an HTTP method", plan.method))?;
            let mut request = client.request(method, url);
            for (name, value) in &plan.headers {
                // reqwest writes these from the request it is building, and a
                // plan's copy would either double them or contradict it.
                if name.eq_ignore_ascii_case("host") || name.eq_ignore_ascii_case("content-length") {
                    continue;
                }
                request = request.header(name.as_str(), value.as_str());
            }
            if let Some(body) = &plan.body {
                request = request.body(body.to_vec());
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
                .map(|chunk| chunk.map(|bytes| bytes.to_vec()).map_err(anyhow::Error::from))
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
