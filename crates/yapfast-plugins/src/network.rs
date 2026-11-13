//! The network gate: every request a plugin makes is decided and performed
//! here.
//!
//! A plugin has no socket of its own. The helper has no networking at all, and
//! a guest asks the parent for `http.fetch`, so this module is the only way out
//! of a plugin. It decides whether the request may happen and, through
//! [`Transport`], performs it. The same checks run for every hop of a redirect
//! chain: a server that answers 302 is as able to point a request at a private
//! address as a plugin is, and it isn't the plugin's fault that the first host
//! was allowed.
//!
//! Five things are refused outright:
//!
//! - anything but HTTPS. A plain request travels in the clear, and a grant for
//!   a host says nothing about the computers in between;
//! - Discord's own API, gateway, voice and CDN names. A plugin reaches Discord
//!   through Yapfast's curated actions and `media.get`, never with the
//!   account's credentials, so a plugin has no business talking to Discord
//!   itself;
//! - a host the manifest doesn't list and nobody has allowed;
//! - an address that isn't a public one — loopback, a home or office network, a
//!   carrier's range, link-local, a cloud metadata address, the unspecified
//!   address, multicast — because each of those reaches a machine that never
//!   expected a plugin to be able to reach it;
//! - anything past the plugin's response cap, or past its share of concurrent
//!   requests.
//!
//! Addresses are what gets checked, not names: the name is resolved here and
//! the transport connects to the addresses this module checked, so a resolver
//! that answers differently on the second lookup — DNS rebinding — finds that
//! nobody is left to connect anywhere.
//!
//! Loopback is the one deliberate exception, and it is its own capability: a
//! plugin that declared `network.local` and named one port may reach that port
//! on this computer and nothing else, which is how a plugin talks to a program
//! someone runs beside Yapfast.

use std::collections::BTreeMap;
use std::io::Write;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use anyhow::{Result, bail};
use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{HttpRequest, HttpResponse, base64};
use yapfast_plugin_api::capability::{Capability, HostPattern, Permissions};
use yapfast_plugin_api::frame::MAX_FRAME;
use yapfast_plugin_api::limits;

use super::grants::{Ask, Grants};
use super::permissions::{Denial, check};

/// The `User-Agent` of every plugin request.
///
/// One fixed line for all plugins, and never Discord's client: a server that
/// sees this knows a plugin in Yapfast asked, and a plugin can't borrow the
/// browser identity Yapfast's own Discord traffic uses.
pub const USER_AGENT: &str = "Yapfast-Plugin/1";

/// Redirect hops one request may follow.
///
/// Each hop is a fresh decision, so the cost of allowing more is another dialog
/// at worst; five is above what a well-behaved service does and low enough that
/// a chain can't be used to walk a request around a limit.
pub const MAX_REDIRECTS: usize = 5;

/// The largest request body a plugin may send.
///
/// A body arrives base64 inside one frame, so this is what a frame carries once
/// the encoding is undone; asking for more would be asking for a frame that
/// can't be written.
pub const MAX_REQUEST_BODY_BYTES: u64 = (MAX_FRAME as u64) * 3 / 4;

/// The largest body that travels back inside one answer.
///
/// The answer crosses the pipe in a frame of [`MAX_FRAME`] bytes with base64
/// and JSON around it, leaving room for the head and the plugin's own fields. A
/// body above this is streamed to a file, and the answer says where.
pub const MAX_INLINE_BODY_BYTES: u64 = (MAX_FRAME as u64) * 3 / 4 - 16 * 1024;

/// Discord's own names: the API, the gateway, voice and the CDN.
const DISCORD_HOSTS: [&str; 6] = [
    "discord.com",
    "discordapp.com",
    "discordapp.net",
    "discord.gg",
    "discord.dev",
    "discord.media",
];

/// The methods a plugin may use.
///
/// A fixed set rather than any token: `CONNECT` would ask a server to open a
/// tunnel, and `TRACE` echoes the request back, and neither is something a
/// plugin needs to talk to an API.
const ALLOWED_METHODS: [&str; 7] = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];

/// The headers a request may not carry, whatever the plugin wrote.
///
/// `host` and `user-agent` are Yapfast's to set; `connection`, `content-length`,
/// `transfer-encoding` and `expect` describe a connection the transport owns;
/// and cookies and proxy credentials belong to no plugin, since nothing a
/// plugin may reach is a session it holds.
const WITHHELD_HEADERS: [&str; 11] = [
    "host",
    "user-agent",
    "connection",
    "content-length",
    "transfer-encoding",
    "expect",
    "te",
    "trailer",
    "upgrade",
    "cookie",
    "proxy-authorization",
];

/// One checked request, ready to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectPlan {
    pub method: String,
    pub host: String,
    pub port: u16,
    /// The request target: the path and query, as a server reads them.
    pub path: String,
    /// Every address the host resolved to, all of them checked. The transport
    /// connects to one of these and no other, which is what makes a second
    /// lookup unable to redirect the connection.
    pub addresses: Vec<IpAddr>,
    /// The headers to send, exactly these and in this order.
    pub headers: Vec<(String, String)>,
    /// The body, shared rather than copied so a redirect that keeps the method
    /// doesn't duplicate it.
    pub body: Option<Arc<[u8]>>,
}

/// A response's head, and its body as it arrives.
pub struct Served {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// The URL that answered, for the plugin to see after redirects.
    pub url: String,
    /// The body in the pieces it arrived in. Dropping it abandons the rest,
    /// which is how a body over its cap is stopped.
    pub body: BoxStream<'static, Result<Vec<u8>>>,
}

/// The one place a socket is opened for a plugin.
///
/// The gate hands over a request it has already decided and resolved, so an
/// implementation has nothing left to interpret and must do exactly what the
/// plan says: connect to one of `plan.addresses`, use `plan.host` as the TLS
/// server name, send `plan.headers` as they are, and open one connection per
/// request, keeping nothing between them — no cookie jar, no connection pool,
/// no proxy, no reuse of a connection Discord's own traffic is on. A plugin
/// must not be able to touch Yapfast's session, or another plugin's, through
/// the transport.
pub trait Transport: Send + Sync {
    /// Sends one planned request.
    fn send(&self, plan: ConnectPlan) -> BoxFuture<'static, Result<Served>>;
}

/// Turns a name into the addresses a connection would use.
///
/// It is a trait because the check that matters is on the addresses, and a test
/// that needed a resolver to answer a particular way would otherwise need the
/// internet to be arranged to agree.
pub trait Resolver: Send + Sync {
    /// The addresses `host` has, in the order the system would try them.
    fn resolve(&self, host: String, port: u16) -> BoxFuture<'static, Result<Vec<IpAddr>>>;
}

/// The system's resolver, as every other request Yapfast makes uses.
#[derive(Debug, Default)]
pub struct SystemResolver;

impl Resolver for SystemResolver {
    fn resolve(&self, host: String, port: u16) -> BoxFuture<'static, Result<Vec<IpAddr>>> {
        Box::pin(async move {
            let mut addresses = Vec::new();
            for address in tokio::net::lookup_host((host.as_str(), port)).await? {
                if !addresses.contains(&address.ip()) {
                    addresses.push(address.ip());
                }
            }
            Ok(addresses)
        })
    }
}

/// What `fetch` decided.
#[derive(Debug)]
pub enum Outcome {
    /// Sent and answered, with the body read inside its cap.
    Done(Answer),
    /// The host isn't in the manifest and nobody has answered about it. The
    /// request is not sent: the caller keeps it and asks about `host`, then
    /// makes the request again once there is an answer.
    AwaitingConsent { host: String, asking: Asking },
}

/// What a request for a host nobody has answered about is waiting on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asking {
    /// No dialog about this host is up, so one is needed before anything can
    /// be sent.
    Opened,
    /// A dialog about this host is already up, and its answer completes this
    /// request too.
    Joined,
    /// A dialog about another host for this plugin is up. Only one is open at a
    /// time, so this request has to wait for the person to finish with that
    /// one, and the plugin is told to ask again.
    Busy,
}

/// A response the gate let through.
#[derive(Debug)]
pub struct Answer {
    pub response: HttpResponse,
    /// Where the body was streamed, when it was too large to travel in one
    /// answer. Nothing deletes it today: the answer carries the byte count but
    /// not the path, so the plugin reads `body_base64: null` and the file stays
    /// behind. Removing it is the gate's job, not the guest's.
    pub body: Option<PathBuf>,
}

/// A URL, split into the parts the gate checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    host: String,
    port: u16,
    path: String,
}

impl Target {
    /// Splits `url`, refusing anything that isn't a plain HTTPS URL.
    ///
    /// The parse is deliberately narrow. A URL that needs a rule from the RFC
    /// Yapfast doesn't implement — user information, a percent-encoded host, a
    /// fragment, an international name written as itself — is refused rather
    /// than guessed at, because the guess would be made by the checker and used
    /// by the transport, and the two would disagree about which host is meant.
    pub fn parse(url: &str) -> Result<Self, Denial> {
        let (scheme, rest) = url.split_once("://").ok_or_else(|| {
            Denial::new("a plugin's request needs a URL, as in https://api.example.com/x")
        })?;
        let scheme = scheme.to_ascii_lowercase();
        if scheme != "https" {
            return Err(Denial::new(format!(
                "a plugin's request must be HTTPS, and this one is `{scheme}`"
            )));
        }
        let (authority, path) = match rest.find(['/', '?', '#']) {
            Some(cut) => (&rest[..cut], &rest[cut..]),
            None => (rest, "/"),
        };
        if authority.is_empty() {
            return Err(Denial::new("a plugin's request must name a host"));
        }
        if authority.contains('@') {
            return Err(Denial::new(
                "a plugin's request may not carry a user name or password in its URL",
            ));
        }
        let (host, port) = split_authority(authority)?;
        let path = match path.find('#') {
            Some(cut) => &path[..cut],
            None => path,
        };
        let path = if path.starts_with('/') { path } else { "/" };
        Ok(Target {
            host,
            port,
            path: path.to_owned(),
        })
    }

    /// The host, in lowercase, as a request and a grant name it.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port, 443 when the URL doesn't say.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The path and query, as a server reads them.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The URL as it would be written back out.
    pub fn url(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == 443 {
            format!("https://{host}{}", self.path)
        } else {
            format!("https://{host}:{}{}", self.port, self.path)
        }
    }

    /// The host as a `Host` header writes it.
    fn authority(&self) -> String {
        if self.port == 443 {
            self.host.clone()
        } else if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// Where a redirect sent the request.
    ///
    /// An absolute URL, a path on the same host, or the `//host/path` form that
    /// keeps the scheme; a relative path is refused, since resolving one means
    /// picking a base directory rule and a plugin's request should name where it
    /// goes.
    pub fn resolve_location(&self, location: &str) -> Result<Self, Denial> {
        let location = location.trim();
        if location.is_empty() {
            return Err(Denial::new("a redirect must say where to go"));
        }
        if let Some(rest) = location.strip_prefix("//") {
            return Target::parse(&format!("https://{rest}"));
        }
        if location.contains("://") {
            return Target::parse(location);
        }
        if !location.starts_with('/') {
            return Err(Denial::new(format!(
                "this redirect names `{location}`, which isn't a URL or an absolute path"
            )));
        }
        let path = match location.find('#') {
            Some(cut) => &location[..cut],
            None => location,
        };
        Ok(Target {
            host: self.host.clone(),
            port: self.port,
            path: path.to_owned(),
        })
    }
}

/// Splits an authority into a checked host and port.
fn split_authority(authority: &str) -> Result<(String, u16), Denial> {
    let (host, port) = match authority.strip_prefix('[') {
        Some(rest) => {
            let (host, after) = rest
                .split_once(']')
                .ok_or_else(|| Denial::new("an IPv6 address in a URL is written in brackets"))?;
            let port =
                match after {
                    "" => None,
                    other => Some(other.strip_prefix(':').ok_or_else(|| {
                        Denial::new("a URL's port follows its host after a colon")
                    })?),
                };
            (host.to_owned(), port)
        }
        None => {
            if authority.matches(':').count() > 1 {
                return Err(Denial::new(
                    "an IPv6 address in a URL is written in brackets",
                ));
            }
            match authority.rsplit_once(':') {
                Some((host, port)) => (host.to_owned(), Some(port)),
                None => (authority.to_owned(), None),
            }
        }
    };
    if host.is_empty() {
        return Err(Denial::new("a plugin's request must name a host"));
    }
    if !host.is_ascii() {
        return Err(Denial::new(
            "a host name must be written in ASCII; an international name is written as punycode",
        ));
    }
    let host = host.to_ascii_lowercase();
    let is_address = host.parse::<IpAddr>().is_ok();
    if host.len() > HostPattern::MAX_LEN
        || host.starts_with('.')
        || host.ends_with('.')
        || host.contains("..")
        || (!is_address
            && !host
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.'))
    {
        return Err(Denial::new(format!("`{host}` isn't a host name")));
    }
    let port = match port {
        Some(text) => text
            .parse::<u16>()
            .map_err(|_| Denial::new(format!("`{text}` isn't a port")))?,
        None => 443,
    };
    if port == 0 {
        return Err(Denial::new("a request may not go to port 0"));
    }
    Ok((host, port))
}

/// Whether `host` is one of Discord's own names, or below one.
pub fn is_discord_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    DISCORD_HOSTS
        .iter()
        .any(|name| host == *name || host.ends_with(&format!(".{name}")))
}

/// Whether `host` names this computer, by the names reserved for it.
///
/// These are the only hosts `network.local` reaches, and they are recognised
/// before any grant: a person is never asked to allow a request to a machine
/// that answers with whatever is behind it.
pub fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(address) => match address {
            IpAddr::V4(v4) => v4.is_loopback(),
            IpAddr::V6(v6) => v6.is_loopback(),
        },
        Err(_) => false,
    }
}

/// Why an address can't be connected to, in a phrase a person can read.
///
/// `None` means the address is a public one. The list is written out rather
/// than left to a library's idea of "global", because this is the rule that
/// keeps a plugin away from the machine it runs on and from the metadata
/// service of the cloud that machine is rented from.
pub fn address_problem(address: IpAddr) -> Option<&'static str> {
    match address {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            if v4.is_loopback() {
                Some("is this computer's loopback")
            } else if v4.is_unspecified() {
                Some("is the unspecified address")
            } else if v4.is_link_local() {
                if octets == [169, 254, 169, 254] {
                    Some("is a cloud metadata address")
                } else {
                    Some("is link-local")
                }
            } else if v4.is_private() {
                Some("is on a private network")
            } else if v4.is_broadcast() || v4.is_multicast() {
                Some("is a broadcast or multicast address")
            } else if octets[0] == 100 && (octets[1] & 0xc0) == 64 {
                Some("is on a carrier's private range")
            } else if octets[0] == 0 {
                Some("is on the reserved 0.0.0.0/8 range")
            } else {
                None
            }
        }
        IpAddr::V6(v6) => {
            // An address that is really an IPv4 one, written the IPv6 way, is
            // checked as what it reaches: `::ffff:169.254.169.254` is the
            // metadata service however it is spelt.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return address_problem(IpAddr::V4(v4));
            }
            let segments = v6.segments();
            if v6.is_loopback() {
                Some("is this computer's loopback")
            } else if v6.is_unspecified() {
                Some("is the unspecified address")
            } else if v6.is_multicast() {
                Some("is a multicast address")
            } else if segments[..6] == [0, 0, 0, 0, 0, 0] {
                Some("is an IPv4 address written the old IPv6 way")
            } else if segments[0] & 0xfe00 == 0xfc00 {
                // fc00::/7: unique local, the IPv6 answer to a home network.
                Some("is on a private network")
            } else if segments[0] & 0xffc0 == 0xfe80 {
                Some("is link-local")
            } else {
                None
            }
        }
    }
}

/// The cap a response is read under, from what the manifest asked for.
///
/// A manifest that asks for nothing gets the default, and one that asks for
/// more than consent may raise the cap to gets the ceiling: the number in the
/// manifest is a request, and the ceiling is the one Yapfast promised.
pub fn read_cap(asked: u64) -> u64 {
    match asked {
        0 => limits::MAX_HTTP_RESPONSE_BYTES,
        asked => asked.min(limits::MAX_HTTP_RESPONSE_CONSENTED_BYTES),
    }
}

/// Requests in flight, per plugin and in total.
#[derive(Debug, Default)]
struct InFlight {
    total: usize,
    per: BTreeMap<PluginId, usize>,
}

#[derive(Debug, Default)]
struct Slots {
    in_flight: Mutex<InFlight>,
}

impl Slots {
    fn acquire(self: &Arc<Self>, plugin: &PluginId) -> Result<Slot, Denial> {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let held = in_flight.per.get(plugin).copied().unwrap_or_default();
        if held >= limits::MAX_HTTP_IN_FLIGHT {
            return Err(Denial::new(format!(
                "this plugin already has {} requests in flight, which is its limit",
                limits::MAX_HTTP_IN_FLIGHT
            )));
        }
        if in_flight.total >= limits::MAX_HTTP_IN_FLIGHT_TOTAL {
            return Err(Denial::new(
                "Yapfast is already handling as many plugin requests as it allows at once",
            ));
        }
        in_flight.total += 1;
        *in_flight.per.entry(plugin.clone()).or_default() += 1;
        Ok(Slot {
            slots: Arc::clone(self),
            plugin: plugin.clone(),
        })
    }

    fn release(&self, plugin: &PluginId) {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        in_flight.total = in_flight.total.saturating_sub(1);
        if let Some(held) = in_flight.per.get_mut(plugin) {
            *held = held.saturating_sub(1);
            if *held == 0 {
                in_flight.per.remove(plugin);
            }
        }
    }
}

/// One request's place in the limits, given back when it is dropped.
///
/// It is given back by `Drop` rather than by each return path, so an error, a
/// refusal from the transport or a panic on the way out all free the place: a
/// plugin that fails its requests still can't wedge the limit shut.
struct Slot {
    slots: Arc<Slots>,
    plugin: PluginId,
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.slots.release(&self.plugin);
    }
}

/// The gate every plugin request goes through.
pub struct NetworkGate {
    grants: Arc<Grants>,
    transport: Arc<dyn Transport>,
    resolver: Arc<dyn Resolver>,
    /// Where a body too large for one answer is streamed.
    scratch: PathBuf,
    declared: RwLock<BTreeMap<PluginId, Permissions>>,
    slots: Arc<Slots>,
}

impl NetworkGate {
    /// A gate with the system's resolver.
    ///
    /// `scratch` is a directory Yapfast owns, under its own data directory:
    /// bodies that don't fit in one answer are written there rather than held
    /// in memory, and the answer says which file it is.
    pub fn new(grants: Arc<Grants>, transport: Arc<dyn Transport>, scratch: PathBuf) -> Self {
        NetworkGate {
            grants,
            transport,
            resolver: Arc::new(SystemResolver),
            scratch,
            declared: RwLock::new(BTreeMap::new()),
            slots: Arc::new(Slots::default()),
        }
    }

    /// Replaces the resolver, for a test that decides what a name resolves to.
    pub fn with_resolver(mut self, resolver: Arc<dyn Resolver>) -> Self {
        self.resolver = resolver;
        self
    }

    /// Records what a plugin's manifest declares, which is what every request
    /// from it is checked against.
    pub fn declare(&self, plugin: &PluginId, permissions: Permissions) {
        let mut declared = self
            .declared
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        declared.insert(plugin.clone(), permissions);
    }

    /// Forgets a plugin's declaration and its session grants, for when it is
    /// turned off.
    pub fn forget(&self, plugin: &PluginId) {
        let mut declared = self
            .declared
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        declared.remove(plugin);
        drop(declared);
        self.grants.forget(plugin);
    }

    /// Performs one request for `plugin`, or refuses it.
    ///
    /// `response_cap` is the manifest's `http_response_bytes`, clamped to what
    /// consent may raise it to. Every hop is decided again, and the place in
    /// the concurrency limits is taken only once a hop is allowed to be sent, so
    /// a request waiting on a dialog holds nothing.
    pub async fn fetch(
        &self,
        plugin: &PluginId,
        request: HttpRequest,
        response_cap: u64,
    ) -> Result<Outcome> {
        let permissions = self.declared(plugin)?;
        let cap = read_cap(response_cap);
        let mut method = checked_method(&request.method)?;
        let mut body = request_body(&request)?;
        let mut headers = sanitized_headers(&request.headers)?;
        let mut target = Target::parse(&request.url)?;
        let mut hops = 0;
        loop {
            let addresses = match self.check_target(plugin, &permissions, &target).await? {
                Checked::Go(addresses) => addresses,
                Checked::Awaiting(host, asking) => {
                    return Ok(Outcome::AwaitingConsent { host, asking });
                }
            };
            let plan = ConnectPlan {
                method: method.clone(),
                host: target.host().to_owned(),
                port: target.port(),
                path: target.path().to_owned(),
                addresses,
                headers: request_headers(&target, &headers),
                body: body.clone(),
            };
            let slot = self.slots.acquire(plugin)?;
            let served = self.transport.send(plan).await?;
            if !is_redirect(served.status) {
                let answer = self.read_answer(&target, served, cap).await?;
                drop(slot);
                return Ok(Outcome::Done(answer));
            }
            hops += 1;
            if hops > MAX_REDIRECTS {
                bail!("this request redirected more than {MAX_REDIRECTS} times");
            }
            let location = served
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("location"))
                .map(|(_, value)| value.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!("this server answered a redirect with no location")
                })?;
            // A 301, 302 or 303 on something with a body becomes a plain GET,
            // as every browser does; 307 and 308 keep the method and the body,
            // which is the whole reason they exist.
            if matches!(served.status, 301..=303) && method != "GET" && method != "HEAD" {
                method = "GET".to_owned();
                body = None;
            }
            let next = target.resolve_location(&location)?;
            if next.host() != target.host() || next.port() != target.port() {
                // A credential is for the server the plugin named, and not for
                // whatever that server points at next: forwarding one on a
                // cross-host redirect would hand it to a stranger. Which header
                // is a credential is the plugin's business — `x-api-key` is as
                // good as `authorization` — so everything beyond describing the
                // response the plugin wants goes.
                headers.retain(|(name, _)| {
                    matches!(
                        name.to_ascii_lowercase().as_str(),
                        "accept"
                            | "accept-encoding"
                            | "accept-language"
                            | "content-type"
                            | "user-agent"
                    )
                });
            }
            target = next;
            drop(served);
            drop(slot);
        }
    }

    /// What plugin `plugin`'s manifest declared, if it is enabled.
    fn declared(&self, plugin: &PluginId) -> Result<Permissions, Denial> {
        let declared = self.declared.read().unwrap_or_else(PoisonError::into_inner);
        declared
            .get(plugin)
            .cloned()
            .ok_or_else(|| Denial::new("this plugin isn't enabled, so it can't make requests"))
    }

    /// Whether `target` may be reached, and by which addresses.
    async fn check_target(
        &self,
        plugin: &PluginId,
        permissions: &Permissions,
        target: &Target,
    ) -> Result<Checked, Denial> {
        let host = target.host();
        if is_discord_host(host) {
            return Err(Denial::new(format!(
                "`{host}` is Discord's own host; a plugin reaches Discord through Yapfast's \
                 actions and `media.get`, never directly"
            )));
        }
        if is_loopback_host(host) {
            return self.check_local(permissions, target).await;
        }
        check(permissions, Capability::Network)?;
        let declared = permissions
            .network
            .iter()
            .any(|pattern| pattern.matches(host));
        if !declared {
            let pattern = HostPattern::parse(host).map_err(|_| {
                Denial::new(format!("`{host}` isn't a host Yapfast can ask you about"))
            })?;
            match self.grants.ask(plugin, &pattern) {
                Ask::Allowed => {}
                Ask::Denied { reason } => return Err(Denial::new(reason)),
                // Nothing is sent while the person reads a dialog: the request
                // is answered as pending and made again afterwards.
                Ask::Asked => return Ok(Checked::Awaiting(host.to_owned(), Asking::Opened)),
                Ask::Joined => return Ok(Checked::Awaiting(host.to_owned(), Asking::Joined)),
                Ask::Busy => return Ok(Checked::Awaiting(host.to_owned(), Asking::Busy)),
            }
        }
        let addresses = self.resolve(host, target.port()).await?;
        for address in &addresses {
            if let Some(problem) = address_problem(*address) {
                return Err(Denial::new(format!(
                    "`{host}` resolves to {address}, which {problem}, so this request was refused"
                )));
            }
        }
        Ok(Checked::Go(addresses))
    }

    /// Loopback, which `network.local` reaches on one port and no other.
    async fn check_local(
        &self,
        permissions: &Permissions,
        target: &Target,
    ) -> Result<Checked, Denial> {
        check(permissions, Capability::NetworkLocal)?;
        let Some(port) = permissions.local_port else {
            return Err(Denial::new(
                "this plugin declared `network.local` without naming a port, so there is \
                 nothing on this computer it may reach",
            ));
        };
        if target.port() != port {
            return Err(Denial::new(format!(
                "this plugin may reach the app on this computer at port {port}, and not port {}",
                target.port()
            )));
        }
        let addresses = self.resolve(target.host(), target.port()).await?;
        let mut loopback = Vec::with_capacity(addresses.len());
        for address in addresses {
            if address.is_loopback() {
                loopback.push(address);
            } else {
                return Err(Denial::new(format!(
                    "`{}` resolves to {address}, which isn't this computer",
                    target.host()
                )));
            }
        }
        Ok(Checked::Go(loopback))
    }

    /// The addresses `host` has, refusing a name that resolves to nothing.
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<IpAddr>, Denial> {
        // An address written in the URL needs no lookup, and a lookup of it
        // would be a second chance for something to answer differently.
        if let Ok(address) = host.parse::<IpAddr>() {
            return Ok(vec![address]);
        }
        let addresses = self
            .resolver
            .resolve(host.to_owned(), port)
            .await
            .map_err(|error| Denial::new(format!("`{host}` couldn't be looked up: {error}")))?;
        if addresses.is_empty() {
            return Err(Denial::new(format!(
                "`{host}` doesn't resolve to any address"
            )));
        }
        Ok(addresses)
    }

    /// Reads a body as it arrives, inside `cap`, keeping only what an answer can
    /// carry and streaming the rest to a file.
    async fn read_answer(&self, target: &Target, served: Served, cap: u64) -> Result<Answer> {
        let status = served.status;
        let headers = served.headers;
        let mut body = served.body;
        let mut total: u64 = 0;
        let mut inline: Vec<u8> = Vec::new();
        let mut spilled: Option<(std::fs::File, PathBuf)> = None;
        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            total = total.saturating_add(chunk.len() as u64);
            if total > cap {
                // The stream is dropped here rather than read to the end: the
                // point of the cap is to stop spending on the answer. A file the
                // body had already started into goes with it, since no answer
                // will name it.
                drop(body);
                if let Some((_, path)) = spilled.take() {
                    let _ = std::fs::remove_file(path);
                }
                bail!("this answer is larger than the {cap} bytes this plugin may receive");
            }
            if spilled.is_none() && total > MAX_INLINE_BODY_BYTES {
                let (mut file, path) = self.spill()?;
                write_chunk(&mut file, &inline)?;
                write_chunk(&mut file, &chunk)?;
                inline = Vec::new();
                spilled = Some((file, path));
            } else if let Some((file, _)) = spilled.as_mut() {
                write_chunk(file, &chunk)?;
            } else {
                inline.extend_from_slice(&chunk);
            }
        }
        let body_file = match spilled {
            Some((file, path)) => {
                file.sync_all()?;
                Some(path)
            }
            None => None,
        };
        let response = HttpResponse {
            status,
            headers,
            url: target.url(),
            bytes: total,
            body_base64: body_file
                .is_none()
                .then(|| base64::encode(inline.as_slice())),
        };
        Ok(Answer {
            response,
            body: body_file,
        })
    }

    /// Opens the file a large body is streamed into, in the scratch directory
    /// the gate was built with.
    fn spill(&self) -> Result<(std::fs::File, PathBuf)> {
        std::fs::create_dir_all(&self.scratch)?;
        let path = self
            .scratch
            .join(format!("{}.body", uuid::Uuid::new_v4().simple()));
        let file = std::fs::File::create(&path)?;
        Ok((file, path))
    }
}

/// What one hop's check decided.
enum Checked {
    /// The request may go to these addresses.
    Go(Vec<IpAddr>),
    /// Somebody has to answer for this host first, and what is on screen about
    /// it already.
    Awaiting(String, Asking),
}

/// Whether a status asks the client to fetch somewhere else.
fn is_redirect(status: u16) -> bool {
    matches!(status, 301..=303 | 307 | 308)
}

/// The method to send, in uppercase, if it is one a plugin may use.
fn checked_method(method: &str) -> Result<String, Denial> {
    let method = method.to_ascii_uppercase();
    if ALLOWED_METHODS.contains(&method.as_str()) {
        Ok(method)
    } else {
        Err(Denial::new(format!(
            "`{method}` isn't a method a plugin may use"
        )))
    }
}

/// The request's body, decoded and inside its cap.
fn request_body(request: &HttpRequest) -> Result<Option<Arc<[u8]>>, Denial> {
    let Some(encoded) = request.body_base64.as_deref() else {
        return Ok(None);
    };
    let body =
        base64::decode(encoded).ok_or_else(|| Denial::new("the request's body isn't base64"))?;
    if body.len() as u64 > MAX_REQUEST_BODY_BYTES {
        return Err(Denial::new(format!(
            "this request's body is larger than the {MAX_REQUEST_BODY_BYTES} bytes a plugin may send"
        )));
    }
    Ok(Some(body.into()))
}

/// The plugin's headers, with the ones Yapfast owns taken out.
///
/// Nothing is added on the plugin's behalf but the `User-Agent` and the `Host`,
/// so the only `Authorization` a request can carry is the one the plugin wrote
/// itself: Yapfast's own Discord token never has a path into a plugin request.
fn sanitized_headers(headers: &[(String, String)]) -> Result<Vec<(String, String)>, Denial> {
    let mut out = Vec::with_capacity(headers.len());
    for (name, value) in headers {
        let lower = name.to_ascii_lowercase();
        if !is_header_name(&lower) {
            return Err(Denial::new(format!("`{name}` isn't a header name")));
        }
        if value.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(Denial::new(format!(
                "the `{lower}` header carries a control character, so it was refused"
            )));
        }
        if WITHHELD_HEADERS.contains(&lower.as_str()) {
            continue;
        }
        out.push((lower, value.clone()));
    }
    Ok(out)
}

/// Whether every byte of `name` may appear in a header name.
fn is_header_name(name: &str) -> bool {
    const EXTRA: &[u8] = b"!#$%&'*+-.^_`|~";
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || EXTRA.contains(&byte))
}

/// The headers a request goes out with.
fn request_headers(target: &Target, headers: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(headers.len() + 4);
    out.push(("host".to_owned(), target.authority()));
    out.push(("user-agent".to_owned(), USER_AGENT.to_owned()));
    // The cap counts bytes, so the bytes that arrive are the bytes that count:
    // asking for a compressed body would let a small compressed answer expand
    // past what the plugin was allowed to receive.
    out.push(("accept-encoding".to_owned(), "identity".to_owned()));
    out.extend(headers.iter().cloned());
    // Every request opens and closes its own connection, so nothing is shared
    // with Discord's traffic or with the next request.
    out.push(("connection".to_owned(), "close".to_owned()));
    out
}

/// Writes one piece of a body to the file it is streaming into.
fn write_chunk(file: &mut std::fs::File, chunk: &[u8]) -> Result<()> {
    file.write_all(chunk)?;
    Ok(())
}

/// The scratch directory a gate writes large bodies into, under `data_dir`.
pub fn scratch_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("plugins-http")
}
