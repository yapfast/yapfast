//! The rules the network gate and the capability check hold to.
//!
//! Each test here is one rule, written so that removing the rule makes it fail:
//! a private address, a rebinding name, Discord's own hosts, plain HTTP, a bare
//! wildcard, a grant and its revocation, a response over its cap, the
//! concurrency limits, the one loopback port, and an event a plugin without
//! `messages.read` never sees.
//!
//! Almost all of it runs without a network. The gate's decisions are separated
//! from the socket on purpose: the resolver and the transport are traits, so a
//! test says what a name resolves to and what a server answers, and the rule
//! being tested is the gate's own. The one test that needs the outside world is
//! `#[ignore]`d.

#![cfg(feature = "plugins")]

use std::collections::{BTreeMap, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use futures_util::stream;
use tokio::sync::Semaphore;
use yapfast_core::plugins::grants::{Ask, Grants, Lasts};
use yapfast_core::plugins::network::{
    Answer, ConnectPlan, MAX_INLINE_BODY_BYTES, NetworkGate, Outcome, Resolver, Served, Transport,
    address_problem, is_discord_host, read_cap,
};
use yapfast_core::plugins::permissions::{self, SendPath};
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::PluginManifest;
use yapfast_plugin_api::call::HttpRequest;
use yapfast_plugin_api::capability::{Capability, HostPattern, Permissions};
use yapfast_plugin_api::event::{EventEnvelope, EventKind, Network, SubscriptionFilter};
use yapfast_plugin_api::limits;

/// The plugin every fixture declares.
const PLUGIN: &str = "com.example.weather";

fn plugin() -> PluginId {
    PluginId::parse(PLUGIN).expect("a test id")
}

fn host(text: &str) -> HostPattern {
    HostPattern::parse(text).expect("a test host")
}

fn policy(capabilities: &[Capability], hosts: &[&str], local_port: Option<u16>) -> Permissions {
    Permissions {
        capabilities: capabilities.to_vec(),
        network: hosts.iter().map(|text| host(text)).collect(),
        local_port,
        schemes: Vec::new(),
    }
}

/// The runtime the async tests run on. `fetch` needs a reactor for the system
/// resolver, and a test may be the first thing in the process to build one.
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a test runtime")
}

/// One answer a fake server gives.
#[derive(Clone, Debug)]
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    chunks: Vec<Vec<u8>>,
}

impl Reply {
    fn ok(chunks: &[&[u8]]) -> Self {
        Reply {
            status: 200,
            headers: Vec::new(),
            chunks: chunks.iter().map(|chunk| chunk.to_vec()).collect(),
        }
    }

    fn redirect(to: &str) -> Self {
        Reply {
            status: 302,
            headers: vec![("location".to_owned(), to.to_owned())],
            chunks: Vec::new(),
        }
    }
}

/// A transport that records what it was asked to send and answers what the test
/// told it to.
///
/// It is the whole of what a real transport would do that a test needs: it
/// keeps the plans, which is how a test proves a refused request never reached
/// a socket, and it can hold a request open, which is how the concurrency limits
/// are tested without racing.
#[derive(Clone)]
struct Recorder {
    plans: Arc<Mutex<Vec<ConnectPlanRecord>>>,
    replies: Arc<Mutex<VecDeque<Reply>>>,
    started: Arc<AtomicUsize>,
    gate: Arc<Semaphore>,
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder {
            plans: Arc::default(),
            replies: Arc::default(),
            started: Arc::default(),
            // No permits: a request waits in the transport until a test lets it
            // through, which is what makes the concurrency limits testable.
            gate: Arc::new(Semaphore::new(0)),
        }
    }
}

/// What a test remembers about one plan, without the body it may have carried.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ConnectPlanRecord {
    host: String,
    port: u16,
    path: String,
    addresses: Vec<IpAddr>,
    headers: Vec<(String, String)>,
    body_len: usize,
}

impl Recorder {
    /// A recorder that answers at once, with `replies` in that order.
    fn new(replies: Vec<Reply>) -> Self {
        let recorder = Recorder::default();
        recorder.release(1024);
        recorder.answer(replies);
        recorder
    }

    /// A recorder that holds every request until the test releases it.
    fn holding(replies: Vec<Reply>) -> Self {
        let recorder = Recorder::default();
        recorder.answer(replies);
        recorder
    }

    fn answer(&self, replies: Vec<Reply>) {
        let mut queue = self.replies.lock().unwrap_or_else(PoisonError::into_inner);
        queue.extend(replies);
    }

    /// Lets the held requests finish.
    fn release(&self, requests: usize) {
        self.gate.add_permits(requests);
    }

    fn plans(&self) -> Vec<ConnectPlanRecord> {
        self.plans
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn started(&self) -> usize {
        self.started.load(Ordering::SeqCst)
    }
}

impl Transport for Recorder {
    fn send(&self, plan: ConnectPlan) -> BoxFuture<'static, anyhow::Result<Served>> {
        let plans = Arc::clone(&self.plans);
        let replies = Arc::clone(&self.replies);
        let started = Arc::clone(&self.started);
        let gate = Arc::clone(&self.gate);
        Box::pin(async move {
            started.fetch_add(1, Ordering::SeqCst);
            plans
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(ConnectPlanRecord {
                    host: plan.host.clone(),
                    port: plan.port,
                    path: plan.path.clone(),
                    addresses: plan.addresses.clone(),
                    headers: plan.headers.clone(),
                    body_len: plan.body.as_ref().map(|body| body.len()).unwrap_or(0),
                });
            let _held = gate.acquire().await;
            let reply = {
                let mut queue = replies.lock().unwrap_or_else(PoisonError::into_inner);
                match queue.len() {
                    0 => Reply::ok(&[]),
                    1 => queue.front().cloned().unwrap_or_else(|| Reply::ok(&[])),
                    _ => queue.pop_front().unwrap_or_else(|| Reply::ok(&[])),
                }
            };
            let chunks: Vec<anyhow::Result<Vec<u8>>> = reply.chunks.into_iter().map(Ok).collect();
            Ok(Served {
                status: reply.status,
                headers: reply.headers,
                url: format!("https://{}{}", plan.host, plan.path),
                body: stream::iter(chunks).boxed(),
            })
        })
    }
}

/// A resolver a test decides the answers for.
///
/// A name may be given several answers, handed out in order, which is what a
/// rebinding name is: the same question answered differently the second time.
#[derive(Clone, Default)]
struct Table {
    answers: Arc<Mutex<BTreeMap<String, VecDeque<Vec<IpAddr>>>>>,
}

impl Table {
    fn fixed(host: &str, addresses: &[&str]) -> Arc<Self> {
        let table = Table::default();
        for address in addresses {
            table.push(host, address);
        }
        Arc::new(table)
    }

    /// A table with one answer for each name it is given.
    fn of(entries: &[(&str, &[&str])]) -> Arc<Self> {
        let table = Table::default();
        for (name, addresses) in entries {
            for address in *addresses {
                table.push(name, address);
            }
        }
        Arc::new(table)
    }

    fn rebinding(host: &str, first: &str, then: &str) -> Arc<Self> {
        let table = Table::default();
        table.push(host, first);
        table.push(host, then);
        Arc::new(table)
    }

    fn push(&self, host: &str, address: &str) {
        let address: IpAddr = address.parse().expect("a test address");
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(host.to_owned())
            .or_default()
            .push_back(vec![address]);
    }
}

impl Resolver for Table {
    fn resolve(&self, host: String, _port: u16) -> BoxFuture<'static, anyhow::Result<Vec<IpAddr>>> {
        let table = self.clone();
        Box::pin(async move {
            let mut answers = table.answers.lock().unwrap_or_else(PoisonError::into_inner);
            let queue = answers
                .get_mut(&host)
                .ok_or_else(|| anyhow::anyhow!("the test gave no answer for {host}"))?;
            // The last answer stands, so a name asked about more than the test
            // listed keeps answering the same way.
            Ok(if queue.len() > 1 {
                queue.pop_front().unwrap_or_default()
            } else {
                queue.front().cloned().unwrap_or_default()
            })
        })
    }
}

/// Everything one test needs: a gate, its grants, and what the transport saw.
struct Fixture {
    gate: NetworkGate,
    grants: Arc<Grants>,
    recorder: Recorder,
    plugin: PluginId,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn build(declared: Permissions, resolver: Arc<Table>, recorder: Recorder) -> Self {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let grants = Arc::new(
            Grants::load(directory.path().join("grants.json")).expect("a fresh grants file"),
        );
        let gate = NetworkGate::new(
            Arc::clone(&grants),
            Arc::new(recorder.clone()),
            directory.path().join("scratch"),
        )
        .with_resolver(resolver);
        let plugin = plugin();
        gate.declare(&plugin, declared);
        Fixture {
            gate,
            grants,
            recorder,
            plugin,
            _directory: directory,
        }
    }

    /// A fixture whose one transport answers `Reply::ok` with `chunks`.
    fn open(declared: Permissions, resolver: Arc<Table>) -> Self {
        Fixture::build(declared, resolver, Recorder::new(vec![Reply::ok(&[b"hi"])]))
    }

    fn fetch(&self, url: &str) -> anyhow::Result<Outcome> {
        runtime().block_on(self.gate.fetch(&self.plugin, HttpRequest::get(url), 0))
    }

    fn fetch_capped(&self, url: &str, cap: u64) -> anyhow::Result<Outcome> {
        runtime().block_on(self.gate.fetch(&self.plugin, HttpRequest::get(url), cap))
    }

    /// The answer's body, for the tests that compare bytes.
    fn done(&self, url: &str) -> Answer {
        match self.fetch(url).expect("the request should be allowed") {
            Outcome::Done(answer) => answer,
            Outcome::AwaitingConsent { host } => {
                panic!("expected an answer, not a dialog about {host}")
            }
        }
    }
}

/// The text of a refusal, however it was wrapped.
fn refusal(error: &anyhow::Error) -> String {
    format!("{error:#}")
}

#[test]
fn plain_http_is_refused_even_for_a_declared_host() {
    let fixture = Fixture::open(
        policy(&[Capability::Network], &["api.example.com"], None),
        Table::fixed("api.example.com", &["93.184.216.34"]),
    );
    let error = fixture
        .fetch("http://api.example.com/messages")
        .expect_err("HTTP is not HTTPS");
    assert!(refusal(&error).contains("HTTPS"), "{}", refusal(&error));
    assert!(
        fixture.recorder.plans().is_empty(),
        "a refused request never reaches a socket"
    );
}

#[test]
fn discord_hosts_are_refused_though_the_manifest_names_them() {
    for host_name in [
        "discord.com",
        "api.discord.com",
        "gateway.discord.gg",
        "cdn.discordapp.com",
        "media.discordapp.net",
        "discordapp.com",
    ] {
        assert!(is_discord_host(host_name), "{host_name} is Discord's");
        // The manifest declares the very host, and the request is still
        // refused: a plugin reaches Discord through Yapfast's actions.
        let fixture = Fixture::open(
            policy(&[Capability::Network], &["*.discord.com"], None),
            Table::fixed(host_name, &["93.184.216.34"]),
        );
        let error = fixture
            .fetch(&format!("https://{host_name}/api/v9/users/@me"))
            .expect_err("Discord's own host is never a plugin's to call");
        assert!(
            refusal(&error).contains("Discord"),
            "{host_name}: {}",
            refusal(&error)
        );
        assert!(fixture.recorder.plans().is_empty());
    }
    assert!(!is_discord_host("discord.com.evil.test"));
    assert!(!is_discord_host("notdiscord.com"));
}

#[test]
fn every_private_shape_is_refused() {
    // The plain function, so the table is exhaustive and readable.
    for (address, expected) in [
        ("127.0.0.1", "loopback"),
        ("::1", "loopback"),
        ("10.1.2.3", "private network"),
        ("172.16.0.1", "private network"),
        ("192.168.1.10", "private network"),
        ("100.64.0.1", "carrier"),
        ("169.254.1.1", "link-local"),
        ("169.254.169.254", "cloud metadata"),
        ("::ffff:169.254.169.254", "cloud metadata"),
        ("fd00:ec2::254", "private network"),
        ("fc00::1", "private network"),
        ("fe80::1", "link-local"),
        ("0.0.0.0", "unspecified"),
        ("::", "unspecified"),
        ("224.0.0.1", "multicast"),
        ("ff02::1", "multicast"),
    ] {
        let parsed: IpAddr = address.parse().expect("a test address");
        let problem = address_problem(parsed).expect("this address is not a public one");
        assert!(
            problem.contains(expected),
            "{address} should be refused as {expected}, and was refused as {problem}"
        );
    }
    // And the sentinel: public addresses are allowed, or the whole gate is a
    // wall and the tests above would prove nothing.
    for address in ["93.184.216.34", "1.1.1.1", "2606:4700:4700::1111"] {
        let parsed: IpAddr = address.parse().expect("a test address");
        assert_eq!(address_problem(parsed), None, "{address} is public");
    }
}

#[test]
fn a_name_that_resolves_privately_is_refused() {
    let fixture = Fixture::open(
        policy(&[Capability::Network], &["*.example.com"], None),
        Table::fixed("internal.example.com", &["10.1.2.3"]),
    );
    let error = fixture
        .fetch("https://internal.example.com/status")
        .expect_err("a private address is not reachable");
    assert!(
        refusal(&error).contains("private network"),
        "{}",
        refusal(&error)
    );
    assert!(fixture.recorder.plans().is_empty());
}

#[test]
fn a_name_that_rebinds_never_reaches_the_address_it_changed_to() {
    let table = Table::rebinding("api.example.com", "93.184.216.34", "127.0.0.1");
    let fixture = Fixture::open(
        policy(&[Capability::Network], &["api.example.com"], None),
        Arc::clone(&table),
    );
    // The first answer is public, so the request goes out — to exactly the
    // address that was checked, which is what makes a second lookup useless.
    let answer = fixture.done("https://api.example.com/x");
    assert_eq!(answer.response.bytes, 2);
    let plans = fixture.recorder.plans();
    assert_eq!(plans.len(), 1);
    assert_eq!(
        plans[0].addresses,
        vec!["93.184.216.34".parse::<IpAddr>().expect("an address")],
        "the transport is given the addresses the gate checked, not the name"
    );

    // The second lookup answers with loopback, and the request stops there.
    let error = fixture
        .fetch("https://api.example.com/x")
        .expect_err("the second answer is not reachable");
    assert!(refusal(&error).contains("loopback"), "{}", refusal(&error));
    assert_eq!(
        fixture.recorder.plans().len(),
        1,
        "nothing was sent to the address it changed to"
    );
}

#[test]
fn every_redirect_hop_is_checked_again() {
    let recorder = Recorder::new(vec![
        Reply::redirect("https://cdn.example.com/real"),
        Reply::ok(&[b"there"]),
    ]);
    let fixture = Fixture::build(
        policy(
            &[Capability::Network],
            &["api.example.com", "cdn.example.com"],
            None,
        ),
        Table::of(&[
            ("api.example.com", &["93.184.216.34"]),
            ("cdn.example.com", &["93.184.216.35"]),
        ]),
        recorder,
    );
    let answer = fixture.done("https://api.example.com/go");
    assert_eq!(answer.response.url, "https://cdn.example.com/real");
    assert_eq!(answer.response.body_base64.as_deref(), Some("dGhlcmU="));
    let plans = fixture.recorder.plans();
    assert_eq!(plans.len(), 2, "both hops went out");
    assert_eq!(plans[1].host, "cdn.example.com");
}

#[test]
fn a_redirect_to_a_private_address_is_refused() {
    let recorder = Recorder::new(vec![
        Reply::redirect("https://internal.example.com/secret"),
        Reply::ok(&[b"never"]),
    ]);
    let fixture = Fixture::build(
        policy(
            &[Capability::Network],
            &["api.example.com", "internal.example.com"],
            None,
        ),
        Table::of(&[
            ("api.example.com", &["93.184.216.34"]),
            ("internal.example.com", &["10.0.0.9"]),
        ]),
        recorder,
    );
    // The first host is public, so the first hop is sent; the second is where a
    // server tries to walk the request into a home network.
    let error = fixture
        .fetch("https://api.example.com/go")
        .expect_err("the second hop is private");
    assert!(refusal(&error).contains("private"), "{}", refusal(&error));
    assert_eq!(
        fixture.recorder.plans().len(),
        1,
        "only the first, public hop was sent"
    );
}

#[test]
fn a_host_nobody_has_answered_about_is_pending_until_it_is() {
    let fixture = Fixture::open(
        policy(&[Capability::Network], &[], None),
        Table::fixed("api.example.com", &["93.184.216.34"]),
    );
    let url = "https://api.example.com/x";
    match fixture.fetch(url).expect("a dialog, not a refusal") {
        Outcome::AwaitingConsent { host } => assert_eq!(host, "api.example.com"),
        Outcome::Done(_) => panic!("nothing may be sent before the person answers"),
    }
    assert!(fixture.recorder.plans().is_empty(), "nothing was held open");

    // A second request for the same host joins the dialog that is up, rather
    // than opening another.
    assert_eq!(
        fixture
            .grants
            .ask(&fixture.plugin, &host("api.example.com")),
        Ask::Joined
    );

    fixture
        .grants
        .answer(
            &fixture.plugin,
            &host("api.example.com"),
            true,
            Lasts::Stored,
        )
        .expect("an answer");
    let answer = fixture.done(url);
    assert_eq!(answer.response.status, 200);
    assert_eq!(fixture.recorder.plans().len(), 1);
}

#[test]
fn a_grant_is_used_until_it_is_revoked_and_not_once_more() {
    let fixture = Fixture::open(
        policy(&[Capability::Network], &[], None),
        Table::fixed("api.example.com", &["93.184.216.34"]),
    );
    let plugin = fixture.plugin.clone();
    let api = host("api.example.com");
    fixture
        .grants
        .answer(&plugin, &api, true, Lasts::Session)
        .expect("an answer");
    assert!(fixture.fetch("https://api.example.com/x").is_ok());
    assert_eq!(fixture.recorder.plans().len(), 1);

    fixture.grants.revoke(&plugin, &api).expect("a revocation");
    let error = fixture
        .fetch("https://api.example.com/x")
        .expect_err("a revoked grant refuses the next request");
    assert!(
        refusal(&error).contains("api.example.com"),
        "{}",
        refusal(&error)
    );
    assert_eq!(
        fixture.recorder.plans().len(),
        1,
        "the request after the revocation never reached a socket"
    );
}

#[test]
fn a_bare_wildcard_is_refused_by_the_manifest() {
    assert!(HostPattern::parse("*").is_err());
    let manifest = r#"{
        "api": 1,
        "id": "com.example.anywhere",
        "name": "Anywhere",
        "version": "1.0.0",
        "wasm": "plugin.wasm",
        "permissions": { "capabilities": ["network"], "network": ["*"] }
    }"#;
    assert!(
        PluginManifest::parse(manifest).is_err(),
        "a plugin that may reach anywhere says nothing useful at install"
    );
}

#[test]
fn a_response_over_its_cap_is_stopped() {
    let recorder = Recorder::new(vec![Reply::ok(&[b"aaaa", b"bbbb", b"cccc"])]);
    let fixture = Fixture::build(
        policy(&[Capability::Network], &["api.example.com"], None),
        Table::fixed("api.example.com", &["93.184.216.34"]),
        recorder,
    );
    let error = fixture
        .fetch_capped("https://api.example.com/x", 10)
        .expect_err("12 bytes is over a 10 byte cap");
    assert!(refusal(&error).contains("10"), "{}", refusal(&error));

    // The sentinel: the same answer under a cap it fits is returned whole.
    let recorder = Recorder::new(vec![Reply::ok(&[b"aaaa", b"bbbb", b"cccc"])]);
    let fixture = Fixture::build(
        policy(&[Capability::Network], &["api.example.com"], None),
        Table::fixed("api.example.com", &["93.184.216.34"]),
        recorder,
    );
    let answer = fixture.done("https://api.example.com/x");
    assert_eq!(answer.response.bytes, 12);
    assert_eq!(
        answer.response.body_base64.as_deref(),
        Some("YWFhYWJiYmJjY2Nj")
    );
}

#[test]
fn the_manifest_s_cap_is_a_request_and_has_a_ceiling() {
    assert_eq!(read_cap(0), limits::MAX_HTTP_RESPONSE_BYTES);
    assert_eq!(read_cap(1024), 1024);
    assert_eq!(
        read_cap(limits::MAX_HTTP_RESPONSE_CONSENTED_BYTES + 1),
        limits::MAX_HTTP_RESPONSE_CONSENTED_BYTES
    );
    assert_eq!(
        read_cap(u64::MAX),
        limits::MAX_HTTP_RESPONSE_CONSENTED_BYTES
    );
}

#[test]
fn a_body_too_large_for_one_answer_is_streamed_rather_than_held() {
    let piece = vec![b'z'; 64 * 1024];
    let chunks: Vec<&[u8]> = (0..14).map(|_| piece.as_slice()).collect();
    let recorder = Recorder::new(vec![Reply::ok(&chunks)]);
    let fixture = Fixture::build(
        policy(&[Capability::Network], &["api.example.com"], None),
        Table::fixed("api.example.com", &["93.184.216.34"]),
        recorder,
    );
    let answer = fixture.done("https://api.example.com/big");
    let total = (64 * 1024 * 14) as u64;
    assert!(total > MAX_INLINE_BODY_BYTES);
    assert_eq!(answer.response.bytes, total);
    assert_eq!(
        answer.response.body_base64, None,
        "a body this size travels as a file, not inside the answer"
    );
    let path = answer.body.expect("the body was streamed to a file");
    let written = std::fs::metadata(&path).expect("the streamed file").len();
    assert_eq!(written, total);
}

#[test]
fn the_headers_a_plugin_writes_are_not_the_ones_that_go() {
    let fixture = Fixture::open(
        policy(&[Capability::Network], &["api.example.com"], None),
        Table::fixed("api.example.com", &["93.184.216.34"]),
    );
    let request = HttpRequest {
        method: "POST".to_owned(),
        url: "https://api.example.com/x".to_owned(),
        headers: vec![
            ("Cookie".to_owned(), "session=discord-token".to_owned()),
            ("User-Agent".to_owned(), "DiscordBot/1".to_owned()),
            ("Authorization".to_owned(), "Bearer plugin-own".to_owned()),
            ("Connection".to_owned(), "keep-alive".to_owned()),
        ],
        body_base64: Some("aGk=".to_owned()),
        tag: None,
    };
    let outcome = runtime()
        .block_on(fixture.gate.fetch(&fixture.plugin, request, 0))
        .expect("the request is allowed");
    assert!(matches!(outcome, Outcome::Done(_)));
    let plans = fixture.recorder.plans();
    let sent = &plans[0];
    let header = |name: &str| {
        sent.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(header("cookie"), None, "a plugin sends no cookies");
    assert_eq!(header("user-agent"), Some("Yapfast-Plugin/1"));
    assert_eq!(
        header("authorization"),
        Some("Bearer plugin-own"),
        "the only Authorization is the one the plugin wrote"
    );
    assert_eq!(header("connection"), Some("close"));
    assert_eq!(header("host"), Some("api.example.com"));
    assert_eq!(sent.body_len, 2);

    // A header value that carries a line break is refused, not sent: it would
    // be a second header smuggled into the request.
    let smuggled = HttpRequest {
        method: "GET".to_owned(),
        url: "https://api.example.com/x".to_owned(),
        headers: vec![("x-note".to_owned(), "a\r\ncookie: x".to_owned())],
        body_base64: None,
        tag: None,
    };
    assert!(
        runtime()
            .block_on(fixture.gate.fetch(&fixture.plugin, smuggled, 0))
            .is_err()
    );
}

#[test]
fn a_credential_does_not_follow_a_redirect_to_another_host() {
    let recorder = Recorder::new(vec![
        Reply::redirect("https://cdn.example.com/real"),
        Reply::ok(&[b"there"]),
    ]);
    let fixture = Fixture::build(
        policy(
            &[Capability::Network],
            &["api.example.com", "cdn.example.com"],
            None,
        ),
        Table::of(&[
            ("api.example.com", &["93.184.216.34"]),
            ("cdn.example.com", &["93.184.216.35"]),
        ]),
        recorder,
    );
    let request = HttpRequest {
        method: "GET".to_owned(),
        url: "https://api.example.com/go".to_owned(),
        headers: vec![("Authorization".to_owned(), "Bearer plugin-own".to_owned())],
        body_base64: None,
        tag: None,
    };
    let outcome = runtime()
        .block_on(fixture.gate.fetch(&fixture.plugin, request, 0))
        .expect("the redirect is allowed");
    assert!(matches!(outcome, Outcome::Done(_)));
    let plans = fixture.recorder.plans();
    let header = |plan: &'_ ConnectPlanRecord, name: &str| -> Option<String> {
        plan.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    assert_eq!(
        header(&plans[0], "authorization").as_deref(),
        Some("Bearer plugin-own"),
        "the plugin's own credential goes where the plugin pointed it"
    );
    assert_eq!(
        header(&plans[1], "authorization").as_deref(),
        None,
        "and not on to another host the first one chose"
    );
}

#[test]
fn loopback_is_reachable_only_through_network_local_on_its_own_port() {
    let resolver = || Table::fixed("localhost", &["127.0.0.1"]);
    let declared = policy(&[Capability::NetworkLocal], &[], Some(8_080));
    let fixture = Fixture::open(declared, resolver());
    let answer = fixture.done("https://localhost:8080/status");
    assert_eq!(answer.response.status, 200);
    assert_eq!(
        fixture.recorder.plans()[0].addresses,
        vec!["127.0.0.1".parse::<IpAddr>().expect("an address")]
    );

    // Another port on the same computer is another program.
    let error = fixture
        .fetch("https://localhost:9090/status")
        .expect_err("only the port the manifest named");
    assert!(refusal(&error).contains("8080"), "{}", refusal(&error));
    assert_eq!(fixture.recorder.plans().len(), 1);

    // Without the capability, loopback is out of reach even on that port.
    let fixture = Fixture::open(policy(&[Capability::Network], &[], None), resolver());
    let error = fixture
        .fetch("https://localhost:8080/status")
        .expect_err("network.local is not declared");
    assert!(
        refusal(&error).contains("network.local"),
        "{}",
        refusal(&error)
    );
    assert!(fixture.recorder.plans().is_empty());

    // And a name that answers with this computer is not a way around it.
    let fixture = Fixture::open(
        policy(
            &[Capability::NetworkLocal, Capability::Network],
            &["*.example.com"],
            Some(8_080),
        ),
        Table::fixed("sneaky.example.com", &["127.0.0.1"]),
    );
    let error = fixture
        .fetch("https://sneaky.example.com:8080/x")
        .expect_err("a name that resolves to loopback is still loopback");
    assert!(refusal(&error).contains("loopback"), "{}", refusal(&error));
}

#[test]
fn a_plugin_without_messages_read_never_receives_a_message_event() {
    let event = EventEnvelope {
        kind: EventKind::MessageCreate,
        network: Network::Discord,
        at: 0,
        room: Some("room-1".to_owned()),
        author: Some("person-1".to_owned()),
        own: false,
        payload: serde_json::json!({ "body": "hello" }),
    };
    let filter = SubscriptionFilter::default();
    let quiet = policy(&[Capability::Storage, Capability::Network], &[], None);
    let denial = permissions::allows_event(&quiet, &filter, &event)
        .expect_err("no messages.read means no message events");
    assert!(
        denial.reason().contains("messages.read"),
        "{}",
        denial.reason()
    );

    let reader = policy(&[Capability::MessagesRead], &[], None);
    assert!(permissions::allows_event(&reader, &filter, &event).is_ok());
    let own_only = SubscriptionFilter {
        room: None,
        exclude_own: true,
    };
    let mine = EventEnvelope { own: true, ..event };
    assert!(
        permissions::allows_event(&reader, &own_only, &mine).is_err(),
        "the plugin's own filter is a second gate, not a substitute for the first"
    );
}

#[test]
fn a_send_needs_the_background_capability_when_no_gesture_asked_for_it() {
    let granted = policy(&[Capability::MessagesSend], &[], None);
    assert!(permissions::check_send(&granted, SendPath::Gesture).is_ok());
    let denial = permissions::check_send(&granted, SendPath::Background)
        .expect_err("a timer is not a person");
    assert!(denial.reason().contains("messages.send.background"));
}

#[test]
fn the_request_over_the_concurrency_limit_is_refused() {
    let fixture = Fixture::build(
        policy(&[Capability::Network], &["api.example.com"], None),
        Table::fixed("api.example.com", &["93.184.216.34"]),
        Recorder::holding(vec![Reply::ok(&[b"ok"])]),
    );
    let limit = limits::MAX_HTTP_IN_FLIGHT;
    let batch = (0..limit)
        .map(|_| {
            fixture.gate.fetch(
                &fixture.plugin,
                HttpRequest::get("https://api.example.com/x"),
                0,
            )
        })
        .map(Box::pin)
        .collect::<Vec<_>>();
    let over = async {
        while fixture.recorder.started() < limit {
            tokio::task::yield_now().await;
        }
        let outcome = fixture
            .gate
            .fetch(
                &fixture.plugin,
                HttpRequest::get("https://api.example.com/x"),
                0,
            )
            .await;
        fixture.recorder.release(limit + 1);
        outcome
    };
    let (outcomes, over) = runtime().block_on(async {
        futures_util::future::join(futures_util::future::join_all(batch), over).await
    });
    assert!(
        refusal(&over.expect_err("over the limit")).contains("in flight"),
        "the request over the limit is refused, not queued"
    );
    for outcome in outcomes {
        assert!(outcome.is_ok(), "the ones inside the limit still ran");
    }
    assert_eq!(
        fixture.recorder.started(),
        limit,
        "the refused request never reached the transport"
    );
}

#[test]
fn the_total_limit_stops_the_seventeenth_request_across_plugins() {
    let fixture = Fixture::build(
        policy(&[Capability::Network], &["api.example.com"], None),
        Table::fixed("api.example.com", &["93.184.216.34"]),
        Recorder::holding(vec![Reply::ok(&[b"ok"])]),
    );
    // Four plugins with four requests each is the total; the fifth plugin's
    // first request is the one that finds nothing left.
    let plugins: Vec<PluginId> = (0..4)
        .map(|index| {
            let id = PluginId::parse(&format!("com.example.p{index}")).expect("a test id");
            fixture.gate.declare(
                &id,
                policy(&[Capability::Network], &["api.example.com"], None),
            );
            id
        })
        .collect();
    let fifth = PluginId::parse("com.example.last").expect("a test id");
    fixture.gate.declare(
        &fifth,
        policy(&[Capability::Network], &["api.example.com"], None),
    );
    let limit = limits::MAX_HTTP_IN_FLIGHT;
    let mut batch = Vec::new();
    for plugin in &plugins {
        for _ in 0..limit {
            batch.push(Box::pin(fixture.gate.fetch(
                plugin,
                HttpRequest::get("https://api.example.com/x"),
                0,
            )));
        }
    }
    assert_eq!(batch.len(), limits::MAX_HTTP_IN_FLIGHT_TOTAL);
    let release = batch.len() + 1;
    let over = async {
        while fixture.recorder.started() < limits::MAX_HTTP_IN_FLIGHT_TOTAL {
            tokio::task::yield_now().await;
        }
        let outcome = fixture
            .gate
            .fetch(&fifth, HttpRequest::get("https://api.example.com/x"), 0)
            .await;
        fixture.recorder.release(release);
        outcome
    };
    let _ = runtime()
        .block_on(async {
            futures_util::future::join(futures_util::future::join_all(batch), over).await
        })
        .1
        .expect_err("the total is reached, so the next request waits for nothing");
}

/// A request to a real name, through the system's resolver and a transport that
/// sends nothing.
///
/// It is ignored because it needs DNS and the outside world; run it with
/// `cargo test -- --ignored` when the resolver itself is in question.
#[test]
#[ignore = "needs DNS"]
fn a_real_name_resolves_to_addresses_the_gate_accepts() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let grants =
        Arc::new(Grants::load(directory.path().join("grants.json")).expect("a fresh file"));
    let recorder = Recorder::default();
    let gate = NetworkGate::new(
        grants,
        Arc::new(recorder.clone()),
        directory.path().join("scratch"),
    );
    let plugin = plugin();
    gate.declare(
        &plugin,
        policy(&[Capability::Network], &["example.com"], None),
    );
    let outcome = runtime()
        .block_on(gate.fetch(&plugin, HttpRequest::get("https://example.com/"), 0))
        .expect("example.com is public");
    assert!(matches!(outcome, Outcome::Done(_)));
    for plan in recorder.plans() {
        assert!(!plan.addresses.is_empty());
        for address in plan.addresses {
            assert_eq!(address_problem(address), None, "{address} is public");
        }
    }
}
