//! Times parsing servers from guild objects, as READY and GUILD_CREATE give
//! them, and counts the allocations. Ignored: run it with the measure profile,
//!
//! ```sh
//! cargo test --profile measure -p yapfast-discord --test server_parse -- --ignored --nocapture
//! ```
//!
//! and `SERVER_PARSE_READY=ready.json` for a READY payload's `d` saved from
//! `testing/fake_discord.ts --servers 100` as well.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use yapfast_discord::ready::{self, Server};

struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: every call goes straight to the system allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size() as u64, Relaxed);
        // SAFETY: the caller keeps GlobalAlloc's contract, which this passes on.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller keeps GlobalAlloc's contract, which this passes on.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(size as u64, Relaxed);
        // SAFETY: the caller keeps GlobalAlloc's contract, which this passes on.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Allocations and bytes asked for while `f` runs.
fn counted<T>(f: impl FnOnce() -> T) -> (T, u64, u64) {
    let (count, bytes) = (ALLOCATIONS.load(Relaxed), BYTES.load(Relaxed));
    let value = f();
    (
        value,
        ALLOCATIONS.load(Relaxed) - count,
        BYTES.load(Relaxed) - bytes,
    )
}

/// A deterministic shuffle's random numbers.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// A guild with `count` channels, a tenth of them categories (at most 50,
/// Discord's cap) and a fifth of the rest voice channels, in a shuffled
/// order, with `threads` active threads this account joined, and 50 roles
/// and emoji as the fake's filler servers have.
fn guild(seed: u64, count: usize, threads: usize) -> Value {
    let mut random = Lcg(seed);
    let base = 1_200_000_000_000_000_000u64 + seed * 10_000_000_000;
    let id = |n: u64| (base + n * 4_194_304).to_string();
    let guild_id = id(0);
    let categories = (count / 10).clamp(1, 50);
    let mut channels = Vec::with_capacity(count);
    for n in 0..categories {
        channels.push(json!({
            "id": id(1 + n as u64), "type": 4, "name": format!("Group {n}"),
            "position": n, "parent_id": null, "guild_id": guild_id, "flags": 0,
            "permission_overwrites": [],
        }));
    }
    let rest = count - categories;
    let mut text = Vec::new();
    for n in 0..rest {
        let channel = id(100 + n as u64);
        let voice = n % 5 == 4;
        // One in twenty sits outside any category.
        let parent = (n % 20 != 19).then(|| id(1 + (random.next() as usize % categories) as u64));
        if !voice {
            text.push(channel.clone());
        }
        channels.push(json!({
            "id": channel, "type": if voice { 2 } else { 0 },
            "name": format!("channel-{n}"), "position": random.next() % 50,
            "parent_id": parent, "guild_id": guild_id, "flags": 0,
            "topic": (n % 2 == 0).then(|| format!("What channel {n} is for")),
            "nsfw": false, "rate_limit_per_user": 0, "last_message_id": null,
            "permission_overwrites": [
                {"id": id(900 + n as u64 % 50), "type": 0, "allow": "2048", "deny": "0"},
                {"id": id(900 + (n as u64 + 7) % 50), "type": 0, "allow": "0", "deny": "1024"},
            ],
        }));
    }
    for at in (1..channels.len()).rev() {
        channels.swap(at, random.next() as usize % (at + 1));
    }
    let threads: Vec<Value> = (0..threads)
        .map(|n| {
            json!({
                "id": id(2000 + (n as u64 * 7919) % 100_003), "type": 11,
                "name": format!("thread {n}"), "guild_id": guild_id,
                "parent_id": text[random.next() as usize % text.len()],
                "owner_id": "1000", "message_count": 40, "member_count": 5,
                "last_message_id": id(200_000 + n as u64), "rate_limit_per_user": 0,
                "thread_metadata": {
                    "archived": false, "locked": false, "auto_archive_duration": 1440,
                    "archive_timestamp": "2026-10-20T10:00:00.000000+00:00",
                },
                "member": {"flags": 1, "join_timestamp": "2026-10-20T10:00:00.000000+00:00"},
            })
        })
        .collect();
    json!({
        "id": guild_id,
        "properties": {"id": guild_id, "name": format!("Guild {seed}"), "owner_id": "1000"},
        "channels": channels,
        "threads": threads,
        "roles": (0..50).map(|n| json!({
            "id": if n == 0 { guild_id.clone() } else { id(900 + n) }, "name": format!("Role {n}"),
            "permissions": "0", "position": n, "color": n * 1000, "hoist": false,
            "managed": false, "mentionable": n % 3 == 0, "flags": 0,
        })).collect::<Vec<_>>(),
        "emojis": (0..50).map(|n| json!({
            "id": id(5000 + n), "name": format!("emoji_{n}"), "animated": false, "available": true,
        })).collect::<Vec<_>>(),
        "member_count": 1000,
    })
}

/// Median, lowest and highest of 15 batches, per call, of each of `runs`
/// taken in turn, a batch lasting about 30 ms.
fn time(runs: &[&dyn Fn()]) -> Vec<(Duration, Duration, Duration)> {
    let iterations: Vec<u32> = runs
        .iter()
        .map(|run| {
            let start = Instant::now();
            run();
            let once = start.elapsed().max(Duration::from_nanos(100));
            (Duration::from_millis(30).as_nanos() / once.as_nanos()).clamp(1, 100_000) as u32
        })
        .collect();
    let mut samples = vec![Vec::new(); runs.len()];
    for _ in 0..15 {
        for (at, run) in runs.iter().enumerate() {
            let start = Instant::now();
            for _ in 0..iterations[at] {
                run();
            }
            samples[at].push(start.elapsed() / iterations[at]);
        }
    }
    samples
        .into_iter()
        .map(|mut batch| {
            batch.sort();
            (batch[7], batch[0], batch[14])
        })
        .collect()
}

fn micros(duration: Duration) -> String {
    format!("{:.1}", duration.as_secs_f64() * 1e6)
}

fn report(label: &str, parse: &dyn Fn() -> Vec<Server>) {
    let [(median, low, high)] = time(&[&|| drop(black_box(parse()))])[..] else {
        unreachable!()
    };
    let (_, count, bytes) = counted(parse);
    println!(
        "| {label} | {} | {}–{} | {count} | {:.1} |",
        micros(median),
        micros(low),
        micros(high),
        bytes as f64 / 1024.0
    );
}

#[test]
#[ignore = "a measurement"]
fn server_parse() {
    println!("| guild | median µs | range µs | allocations | KiB asked |");
    println!("|---|---|---|---|---|");
    for count in [50, 100, 250, 500] {
        for threads in [0, 20, 100] {
            let guild = guild(count as u64 * 1000 + threads as u64, count, threads);
            report(&format!("{count} channels, {threads} threads"), &|| {
                vec![ready::server(&guild).unwrap()]
            });
        }
    }
    if let Ok(path) = std::env::var("SERVER_PARSE_READY") {
        let bytes = std::fs::read(&path).unwrap();
        let start = Instant::now();
        let (ready, count, allocated) =
            counted(|| serde_json::from_slice::<Value>(&bytes).unwrap());
        println!(
            "\nREADY of {:.1} MiB: serde_json parse {:.1} ms, {count} allocations, {:.1} MiB asked",
            bytes.len() as f64 / 1048576.0,
            start.elapsed().as_secs_f64() * 1e3,
            allocated as f64 / 1048576.0
        );
        let guilds = ready["guilds"].as_array().unwrap();
        report(&format!("READY, {} servers", guilds.len()), &|| {
            guilds.iter().filter_map(ready::server).collect()
        });
        // The same with three of them 500-channel servers, 100 threads each.
        let mut big = guilds.clone();
        for (at, guild) in big.iter_mut().take(3).enumerate() {
            *guild = self::guild(77 + at as u64, 500, 100);
        }
        report("the same, 3 of them 500 channels", &|| {
            big.iter().filter_map(ready::server).collect()
        });
    }
}

/// A CHANNEL_UPDATE in a 500-channel server whose published snapshot shares
/// it, as the tracker's `Arc::make_mut` meets it: the server's copy, then
/// `put_channel`; and the same without a snapshot.
#[test]
#[ignore = "a measurement"]
fn channel_update() {
    let guild = guild(9, 500, 20);
    let server = Arc::new(ready::server(&guild).unwrap());
    let mut update = guild["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|channel| channel["type"] == 0)
        .unwrap()
        .clone();
    update["name"] = json!("renamed");
    let copy = || {
        let snapshot = server.clone();
        let mut mine = snapshot.clone();
        Arc::make_mut(&mut mine);
        black_box((snapshot, mine));
    };
    let shared = || {
        let snapshot = server.clone();
        let mut mine = snapshot.clone();
        Arc::make_mut(&mut mine).put_channel(&update);
        black_box((snapshot, mine));
    };
    let alone = std::cell::RefCell::new(Server::clone(&server));
    let runs: [(&str, &dyn Fn()); 3] = [
        ("the server's copy alone", &copy),
        ("shared with a snapshot", &shared),
        ("not shared", &|| alone.borrow_mut().put_channel(&update)),
    ];
    let times = time(&runs.iter().map(|(_, run)| *run).collect::<Vec<_>>());
    println!("| CHANNEL_UPDATE, 500 ch | median µs | range µs | allocations | KiB asked |");
    println!("|---|---|---|---|---|");
    for ((name, run), (median, low, high)) in runs.iter().zip(times) {
        let ((), count, bytes) = counted(run);
        println!(
            "| {name} | {} | {}–{} | {count} | {:.1} |",
            micros(median),
            micros(low),
            micros(high),
            bytes as f64 / 1024.0
        );
    }
}
