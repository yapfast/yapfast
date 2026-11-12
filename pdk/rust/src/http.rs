//! HTTP through Yapfast's network gate.
//!
//! A plugin has no sockets. Every request goes through Yapfast, which checks it
//! against the hosts the manifest asked for, shows the person a consent dialog
//! the first time, and enforces the response cap. A host that isn't in the
//! manifest is refused whatever the plugin does, so this returns an error
//! rather than a failure to connect.
//!
//! A fetch either answers at once or is left pending, because the person may
//! have to allow the host first. A pending one is completed in a later call,
//! which is why the request carries a tag of the plugin's own.

use crate::call::{self, Pending, method};
use crate::wire;
use crate::{Error, Result};

pub use crate::wire::{HttpRequest, HttpResponse};

/// What a fetch came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetched {
    /// The answer arrived while the call was open.
    Ready(HttpResponse),
    /// The answer is coming in a later call; hold the `Pending` and ask
    /// [`Context::answer`](crate::plugin::Context::answer) for it.
    Pending(Pending),
}

impl Fetched {
    /// The answer, when it came at once.
    pub fn ready(self) -> Option<HttpResponse> {
        match self {
            Fetched::Ready(response) => Some(response),
            Fetched::Pending(_) => None,
        }
    }
}

impl HttpRequest {
    /// A GET with nothing else set.
    pub fn get(url: impl Into<String>) -> Self {
        HttpRequest {
            method: "GET".to_owned(),
            url: url.into(),
            headers: Vec::new(),
            body_base64: None,
            tag: None,
        }
    }

    /// A POST with nothing else set.
    pub fn post(url: impl Into<String>) -> Self {
        HttpRequest {
            method: "POST".to_owned(),
            ..HttpRequest::get(url)
        }
    }

    /// The same request with one more header.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// The same request with a body, which is encoded for the wire.
    pub fn with_body(mut self, body: impl AsRef<[u8]>) -> Self {
        self.body_base64 = Some(wire::base64::encode(body.as_ref()));
        self
    }

    /// The same request with the plugin's own name for it, which its answer
    /// echoes back so a plugin can tell two requests to one URL apart.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// The body, decoded; `None` when the request had none.
    pub fn body(&self) -> Option<Vec<u8>> {
        self.body_base64.as_deref().and_then(wire::base64::decode)
    }
}

impl HttpResponse {
    /// Whether the status was in the 2xx range.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The body, decoded; `None` when it was too large to fit in one answer.
    pub fn body(&self) -> Option<Vec<u8>> {
        self.body_base64.as_deref().and_then(wire::base64::decode)
    }

    /// The body as text, when it was there and is UTF-8.
    pub fn text(&self) -> Option<String> {
        String::from_utf8(self.body()?).ok()
    }

    /// One header by name, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// One request through the gate.
pub fn fetch(request: &HttpRequest) -> Result<Fetched> {
    match call::send(method::HTTP_FETCH, request)? {
        wire::HostReturn::Pending { tag } => Ok(Fetched::Pending(Pending::from_tag(tag))),
        answer => {
            let value = call::value(method::HTTP_FETCH, answer)?;
            Ok(Fetched::Ready(call::parse(method::HTTP_FETCH, value)?))
        }
    }
}

/// One GET, whose answer must arrive while the call is open.
///
/// For a host the person has already allowed, this is all a plugin needs; when
/// the answer is deferred, this is the refusal explained rather than a request
/// lost.
pub fn get(url: &str) -> Result<HttpResponse> {
    ready(fetch(&HttpRequest::get(url))?)
}

/// The answer of a fetch that must not be deferred, or an error saying so.
pub fn ready(fetched: Fetched) -> Result<HttpResponse> {
    match fetched {
        Fetched::Ready(response) => Ok(response),
        Fetched::Pending(pending) => Err(Error::msg(format!(
            "the request was left pending under `{}`; use `fetch` to wait for it",
            pending.tag()
        ))),
    }
}
