//! What the helper costs, measured on the real binary: how long it takes to
//! start, to activate a module cold and from the cache, to answer a hook, an
//! event and a decoration one at a time, and to get through a burst of a
//! thousand; and what it holds and spends while it sits idle.
//!
//! These are measurements rather than checks, so they are ignored by default and
//! print a table instead of asserting a threshold a busy machine could miss:
//!
//! ```sh
//! nix develop path:./nix --command cargo test --profile fast \
//!   -p yapfast-plugin-host --test perf -- --ignored --nocapture --test-threads 1
//! ```
//!
//! `echo.wasm` and `filter.wasm` are hand-written fixtures that answer at once,
//! so a number for them is the helper's own overhead. `plugins/ping` is a Rust
//! plugin built with the PDK, which parses what it is given as a real guest
//! does.

use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;
use yapfast_plugin_api::API_VERSION;
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::event::{EventEnvelope, EventKind, Network};
use yapfast_plugin_api::frame::{Frame, FrameKind, read_frame, write_frame};
use yapfast_plugin_api::hook::{Draft, HookKind, HookRequest, HookSubject, TypingPending};
use yapfast_plugin_api::message::{Activation, DecorateRequest, FromHelper, ToHelper};

const BUDGET: Duration = Duration::from_secs(30);
const PAGES: u32 = 256;
const SAMPLES: usize = 2000;
const BURST: usize = 1000;

#[test]
#[ignore = "a measurement; run with --ignored --nocapture"]
fn perf_helper() {
    let mut table = Table::default();
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!("load average: {}", load.trim());

    // Start: spawn to Ready.
    let mut starts = Vec::new();
    for round in 0..10 {
        let started = Instant::now();
        let mut helper = Helper::start(&format!("perf-start-{round}"));
        helper.send(FrameKind::Hello, 1, &ToHelper::Hello { api: API_VERSION });
        helper.wait(|message| matches!(message, FromHelper::Ready { .. }));
        starts.push(started.elapsed());
    }
    table.row("spawn to Ready", &mut starts);

    let cache = std::env::temp_dir().join("yapfast-plugins-perf/cache");
    let _ = std::fs::remove_dir_all(&cache);

    // Activation of the PDK plugin: cold, then from the compile cache in a new
    // helper.
    let ping = plugin("ping");
    let ping_wasm = repository().join("plugins/ping/plugin.wasm");
    let mut cold = Vec::new();
    let mut warm = Vec::new();
    for round in 0..6 {
        let mut helper = Helper::start_with_cache(&cache);
        let started = Instant::now();
        helper.activate(&ping, &ping_wasm);
        helper.wait(|message| matches!(message, FromHelper::Activated { .. }));
        if round == 0 {
            cold.push(started.elapsed());
        } else {
            warm.push(started.elapsed());
        }
    }
    table.row("activate ping (cold compile)", &mut cold);
    table.row("activate ping (compile cache)", &mut warm);

    let mut helper = Helper::start_with_cache(&cache);
    helper.send(FrameKind::Hello, 1, &ToHelper::Hello { api: API_VERSION });
    helper.wait(|message| matches!(message, FromHelper::Ready { .. }));
    let echo = plugin("echo");
    let filter = plugin("filter");
    for (id, wasm) in [
        (&echo, fixtures().join("echo.wasm")),
        (&filter, fixtures().join("filter.wasm")),
        (&ping, ping_wasm.clone()),
    ] {
        helper.activate(id, &wasm);
        helper.wait(|message| matches!(message, FromHelper::Activated { .. }));
    }

    // One at a time.
    for (name, chain) in [
        ("send hook, echo", vec![echo.clone()]),
        ("send hook, ping (PDK)", vec![ping.clone()]),
        (
            "send hook, chain of 3",
            vec![echo.clone(), filter.clone(), ping.clone()],
        ),
    ] {
        let mut times = helper.round_trips(SAMPLES, |helper, id| {
            helper.send(
                FrameKind::Request,
                id,
                &ToHelper::Hook {
                    chain: chain.clone(),
                    request: a_hook(),
                },
            );
        });
        table.row(name, &mut times);
    }
    let mut times = helper.round_trips(SAMPLES, |helper, id| {
        helper.send(
            FrameKind::Request,
            id,
            &ToHelper::Hook {
                chain: vec![echo.clone()],
                request: HookRequest {
                    hook: HookKind::TypingPreSend,
                    subject: HookSubject::Typing(TypingPending {
                        room: "room-1".into(),
                    }),
                },
            },
        );
    });
    table.row("typing hook, echo", &mut times);
    let mut times = helper.round_trips(SAMPLES, |helper, id| {
        helper.send(
            FrameKind::Request,
            id,
            &ToHelper::Decorate {
                plugin: echo.clone(),
                request: a_row(id),
            },
        );
    });
    table.row("decorate, echo", &mut times);
    for (name, target) in [("event, filter", &filter), ("event, ping (PDK)", &ping)] {
        let mut times = helper.round_trips(SAMPLES, |helper, id| {
            helper.send(
                FrameKind::Request,
                id,
                &ToHelper::Event {
                    plugin: target.clone(),
                    events: vec![an_event(id)],
                    topics: Vec::new(),
                },
            );
        });
        table.row(name, &mut times);
    }

    // Bursts: a thousand written back to back, then every answer awaited.
    for (name, target) in [
        ("1000 events, filter", &filter),
        ("1000 events, ping (PDK)", &ping),
    ] {
        let per = helper.burst(BURST, |helper, id| {
            helper.send(
                FrameKind::Request,
                id,
                &ToHelper::Event {
                    plugin: target.clone(),
                    events: vec![an_event(id)],
                    topics: Vec::new(),
                },
            );
        });
        table.single(name, per);
    }
    let per = helper.burst(BURST, |helper, id| {
        helper.send(
            FrameKind::Request,
            id,
            &ToHelper::Decorate {
                plugin: echo.clone(),
                request: a_row(id),
            },
        );
    });
    table.single("1000 decorates, echo", per);

    // Idle: what the helper holds with three plugins up and nothing to do.
    thread::sleep(Duration::from_millis(500));
    let pid = helper.child.id();
    let before = Usage::of(pid);
    thread::sleep(Duration::from_secs(5));
    let after = Usage::of(pid);
    println!(
        "helper idle 5 s, 3 plugins: rss {:.1} MiB, cpu {:.3}%, {} wakeups/s over {} threads",
        after.rss_kib as f64 / 1024.0,
        (after.cpu_ticks - before.cpu_ticks) as f64 / 5.0,
        (after.switches - before.switches) / 5,
        after.threads,
    );
    table.print();
}

/// A thousand events cost per event, and the time a single answer waits.
#[derive(Default)]
struct Table {
    rows: Vec<String>,
}

impl Table {
    fn row(&mut self, name: &str, times: &mut [Duration]) {
        times.sort();
        let at = |q: f64| times[((times.len() - 1) as f64 * q).round() as usize];
        self.rows.push(format!(
            "| {name:<32} | {:>9.1} | {:>9.1} | {:>9.1} | {:>5} |",
            micros(at(0.5)),
            micros(at(0.99)),
            micros(*times.last().unwrap()),
            times.len()
        ));
    }

    fn single(&mut self, name: &str, per: Duration) {
        self.rows.push(format!(
            "| {name:<32} | {:>9.1} | {:>9} | {:>9} | {:>5} |",
            micros(per),
            "per item",
            "",
            BURST
        ));
    }

    fn print(&self) {
        println!(
            "| {:<32} | {:>9} | {:>9} | {:>9} | {:>5} |",
            "what", "p50 µs", "p99 µs", "max µs", "n"
        );
        for row in &self.rows {
            println!("{row}");
        }
    }
}

fn micros(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e6
}

struct Usage {
    rss_kib: u64,
    cpu_ticks: u64,
    switches: u64,
    threads: usize,
}

impl Usage {
    fn of(pid: u32) -> Usage {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
        let rss_kib = status
            .lines()
            .find_map(|line| line.strip_prefix("VmRSS:"))
            .and_then(|value| value.split_whitespace().next()?.parse().ok())
            .unwrap_or(0);
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        let fields: Vec<&str> = stat
            .rsplit_once(')')
            .map(|(_, rest)| rest.split_whitespace().collect())
            .unwrap_or_default();
        let cpu_ticks = fields
            .get(11)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
            + fields
                .get(12)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
        let mut switches = 0;
        let mut threads = 0;
        if let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) {
            for task in tasks.flatten() {
                threads += 1;
                let status =
                    std::fs::read_to_string(task.path().join("status")).unwrap_or_default();
                for line in status.lines() {
                    if let Some(value) = line
                        .strip_prefix("voluntary_ctxt_switches:")
                        .or_else(|| line.strip_prefix("nonvoluntary_ctxt_switches:"))
                    {
                        switches += value.trim().parse::<u64>().unwrap_or(0);
                    }
                }
            }
        }
        Usage {
            rss_kib,
            cpu_ticks,
            switches,
            threads,
        }
    }
}

fn plugin(name: &str) -> PluginId {
    PluginId::parse(&format!("com.example.{name}")).expect("a valid plugin id")
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository")
}

fn fixtures() -> PathBuf {
    repository().join("testing/plugins")
}

fn an_event(at: u64) -> EventEnvelope {
    EventEnvelope {
        kind: EventKind::MessageCreate,
        network: Network::Discord,
        at,
        room: Some("discord:123456789012345678:234567890123456789".into()),
        author: Some("345678901234567890".into()),
        own: false,
        payload: json!({
            "id": format!("{at}"),
            "body": "a message of an ordinary length, with a link https://example.com/a/b and an emoji 🙂",
            "author": { "id": "345678901234567890", "name": "Somebody", "username": "somebody" },
            "attachments": [],
            "mentions": [],
            "reply_to": null,
        }),
    }
}

fn a_hook() -> HookRequest {
    HookRequest {
        hook: HookKind::MessagePreSend,
        subject: HookSubject::Draft(Draft {
            room: "discord:123456789012345678:234567890123456789".into(),
            body: "a message of an ordinary length, about to be sent".into(),
            reply_to: None,
            attachments: Vec::new(),
        }),
    }
}

fn a_row(id: u64) -> DecorateRequest {
    DecorateRequest {
        room: "discord:123456789012345678:234567890123456789".into(),
        message: format!("message-{id}"),
        author: "345678901234567890".into(),
        body: "a message of an ordinary length, with a link https://example.com/a/b".into(),
        own: false,
    }
}

struct Helper {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    frames: Receiver<Frame>,
    next: u64,
}

impl Helper {
    fn start(name: &str) -> Helper {
        let cache = std::env::temp_dir().join(format!("yapfast-plugins-perf/{name}"));
        let _ = std::fs::remove_dir_all(&cache);
        Helper::start_with_cache(&cache)
    }

    fn start_with_cache(cache: &Path) -> Helper {
        std::fs::create_dir_all(cache).expect("a cache directory for the helper");
        // A copy of the binary can be named, so a measurement isn't of whatever
        // another build left in a shared target directory.
        let binary = std::env::var_os("YAPFAST_PLUGINS")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_yapfast-plugins").into());
        let mut child = Command::new(binary)
            .arg("run")
            .arg("--cache")
            .arg(cache)
            // As the parent starts it: with the store named, so the OS sandbox
            // is on and its cost is in the numbers.
            .arg("--store")
            .arg(repository())
            .env_clear()
            .env("RAYON_NUM_THREADS", "2")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the helper binary starts");
        let stdin = BufWriter::new(child.stdin.take().expect("piped"));
        let stdout = child.stdout.take().expect("piped");
        let (sender, frames) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(frame)) = read_frame(&mut reader) {
                if sender.send(frame).is_err() {
                    return;
                }
            }
        });
        Helper {
            child,
            stdin,
            frames,
            next: 1,
        }
    }

    fn next_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    fn send(&mut self, kind: FrameKind, id: u64, message: &ToHelper) {
        let payload = serde_json::to_vec(message).expect("a ToHelper is JSON");
        write_frame(&mut self.stdin, &Frame::new(kind, id, payload)).expect("the pipe takes it");
    }

    fn activate(&mut self, plugin: &PluginId, wasm: &Path) {
        let id = self.next_id();
        self.send(
            FrameKind::Request,
            id,
            &ToHelper::Activate {
                plugin: plugin.clone(),
                load: Some(Activation {
                    wasm: wasm.to_path_buf(),
                    memory_pages: PAGES,
                    wasi: false,
                    state: None,
                }),
            },
        );
    }

    fn wait(&mut self, mut wanted: impl FnMut(&FromHelper) -> bool) -> (u64, FromHelper) {
        let deadline = Instant::now() + BUDGET;
        loop {
            let frame = self
                .frames
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("the helper answers in time");
            let message: FromHelper = serde_json::from_slice(&frame.payload).expect("a FromHelper");
            if let FromHelper::Fault { plugin, fault } = &message {
                panic!("`{plugin}` faulted: {fault:?}");
            }
            if wanted(&message) {
                return (frame.id, message);
            }
        }
    }

    /// Whether this is the answer a request is waited on for.
    fn answers(message: &FromHelper) -> bool {
        matches!(
            message,
            FromHelper::HookDone { .. }
                | FromHelper::EventDone { .. }
                | FromHelper::Decorated { .. }
        )
    }

    fn round_trips(
        &mut self,
        samples: usize,
        mut ask: impl FnMut(&mut Helper, u64),
    ) -> Vec<Duration> {
        // A warm-up, so the first instantiation isn't in the numbers.
        for _ in 0..50 {
            let id = self.next_id();
            ask(self, id);
            self.wait(Helper::answers);
        }
        let mut times = Vec::with_capacity(samples);
        for _ in 0..samples {
            let id = self.next_id();
            let started = Instant::now();
            ask(self, id);
            self.wait(Helper::answers);
            times.push(started.elapsed());
        }
        times
    }

    fn burst(&mut self, count: usize, mut ask: impl FnMut(&mut Helper, u64)) -> Duration {
        let started = Instant::now();
        for _ in 0..count {
            let id = self.next_id();
            ask(self, id);
        }
        for _ in 0..count {
            self.wait(Helper::answers);
        }
        started.elapsed() / count as u32
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
