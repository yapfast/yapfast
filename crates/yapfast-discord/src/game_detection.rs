//! Lightweight game detection on Linux, Windows and macOS. Only executable
//! identity is refreshed; no CPU, memory, I/O, environment or thread metrics.

use crate::api::Api;
use anyhow::Result;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot, watch};

mod catalog;
pub mod custom_games;
mod processes;
mod steam;
use catalog::{Catalog, GameInfo};

const STEAM_INTERVAL: Duration = Duration::from_secs(60);

/// How many games [`Detector::detected`] gives, for a menu to list.
const DETECTED_MOST: usize = 50;

/// How often the running processes are looked at: every 15 seconds, or
/// every half second against the test server, where detection runs only
/// for tests that ask for it, so they don't wait out the scans.
fn interval() -> Duration {
    if crate::client::test_server().is_some() {
        Duration::from_millis(500)
    } else {
        Duration::from_secs(15)
    }
}

pub(crate) fn start(
    api: Arc<Api>,
    rescan: mpsc::UnboundedReceiver<oneshot::Sender<()>>,
) -> watch::Receiver<Option<Arc<Value>>> {
    let (shown, receiver) = watch::channel(None);
    let custom = std::env::var_os("YAPFAST_DISCORD_GAMES_FILE").map(PathBuf::from);
    let isolated_cache = std::env::var_os("YAPFAST_DISCORD_GAMES_CACHE").is_some();
    let disabled = std::env::var_os("YAPFAST_DISCORD_GAME_DETECTION").is_some_and(|v| v == "0")
        || (crate::client::test_server().is_some() && custom.is_none() && !isolated_cache);
    if disabled || !cfg!(any(target_os = "linux", windows, target_os = "macos")) {
        return receiver;
    }
    let detected = custom_games::Detection::start();
    tokio::spawn(async move {
        let (games, blocked) = (custom_games::subscribe(), custom_games::subscribe_blocked());
        if let Err(error) = run(api, custom, &shown, games, blocked, rescan, &detected).await {
            tracing::warn!(%error, "automatic game detection stopped");
        }
        shown.send_replace(None);
    });
    receiver
}

async fn run(
    api: Arc<Api>,
    custom: Option<PathBuf>,
    shown: &watch::Sender<Option<Arc<Value>>>,
    mut custom_games: watch::Receiver<Vec<custom_games::CustomGame>>,
    mut blocked_games: watch::Receiver<Vec<String>>,
    mut rescan: mpsc::UnboundedReceiver<oneshot::Sender<()>>,
    detected: &custom_games::Detection,
) -> Result<()> {
    let cache = std::env::var_os("YAPFAST_DISCORD_GAMES_CACHE")
        .map(PathBuf::from)
        .or_else(crate::cache::games_file);
    let path = custom.clone().or_else(|| cache.clone());
    let (initial, age) = tokio::task::spawn_blocking(move || {
        let age = path.as_deref().and_then(catalog::age);
        let catalog = path.as_ref().and_then(|p| match Catalog::read(p) {
            Ok(catalog) => Some(catalog),
            Err(error) => {
                tracing::debug!(%error, "no cached game catalogue");
                None
            }
        });
        let age = age.filter(|_| catalog.is_some());
        (catalog.unwrap_or_default(), age)
    })
    .await?;
    let mut detector = Detector::new(initial, steam::roots());
    detector.set_executables(&custom_games.borrow_and_update());
    detector.set_blocked(&blocked_games.borrow_and_update());
    let mut ticks = tokio::time::interval(interval());
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // A list from the cache is asked for again once it's a day old, and
    // comes again only if it changed.
    let mut cached = age.is_some();
    let mut due = custom.is_none() && age.is_none_or(|age| age >= catalog::MAX_AGE);
    let mut refresh = Box::pin(catalog::refresh(&api, cache.clone(), cached));
    let mut next_refresh =
        Instant::now() + age.map_or(catalog::MAX_AGE, |age| catalog::MAX_AGE.saturating_sub(age));
    let mut scanned_for_rpc = None;
    loop {
        tokio::select! {
            biased;
            () = shown.closed() => return Ok(()),
            request = rescan.recv() => {
                let Some(reply) = request else { return Ok(()); };
                scanned_for_rpc = Some(reply);
                detector.selection.verify_exit();
                ticks.reset_immediately();
            }
            result = custom_games.changed() => {
                if result.is_err() { return Ok(()); }
                detector.set_executables(&custom_games.borrow_and_update());
                ticks.reset_immediately();
            }
            result = blocked_games.changed() => {
                if result.is_err() { return Ok(()); }
                detector.set_blocked(&blocked_games.borrow_and_update());
                ticks.reset_immediately();
            }
            result = &mut refresh, if due => {
                let success = result.is_ok();
                match result {
                    Ok(catalog::Refreshed::New(catalog)) => {
                        detector.replace_catalog(catalog);
                        cached = true;
                    }
                    Ok(catalog::Refreshed::Unchanged) => {}
                    Err(error) => tracing::warn!(%error, "could not refresh game catalogue; using cache"),
                }
                due = false;
                next_refresh = Instant::now() + if success { catalog::MAX_AGE } else { Duration::from_secs(3600) };
            }
            _ = ticks.tick() => {
                if custom.is_none() && !due && Instant::now() >= next_refresh {
                    refresh = Box::pin(catalog::refresh(&api, cache.clone(), cached));
                    due = true;
                }
                let scanned = tokio::task::spawn_blocking(move || {
                    let activity = detector.scan(crate::service::now_ms());
                    (detector, activity)
                }).await?;
                detector = scanned.0;
                detected.publish(detector.detected());
                shown.send_if_modified(|previous| {
                    if *previous == scanned.1 { false } else { *previous = scanned.1; true }
                });
                if let Some(reply) = scanned_for_rpc.take() {
                    let _ = reply.send(());
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    pid: u32,
    started: u64,
}

struct Known {
    started: u64,
    fingerprint: u64,
    game: Option<Arc<GameInfo>>,
    generation: u64,
}

struct Found {
    identity: Identity,
    game: Arc<GameInfo>,
}

struct Active {
    found: Found,
    missed: u8,
    activity: Arc<Value>,
}

#[derive(Default)]
struct Selection {
    active: Option<Active>,
}

impl Selection {
    /// A vanished RPC source warrants checking its fallback now. One
    /// completed scan can clear it; a process still present keeps its clock.
    fn verify_exit(&mut self) {
        if let Some(active) = &mut self.active {
            active.missed = 1;
        }
    }

    fn update(
        &mut self,
        candidates: impl IntoIterator<Item = Found>,
        now_ms: u64,
    ) -> Option<Arc<Value>> {
        let mut current_found = false;
        let next = candidates
            .into_iter()
            .inspect(|found| {
                current_found |= self.active.as_ref().is_some_and(|active| {
                    found.identity == active.found.identity && found.game == active.found.game
                });
            })
            .max_by_key(|f| (f.identity.started, f.identity.pid));
        if let Some(active) = &mut self.active {
            if current_found {
                active.missed = 0;
                return Some(active.activity.clone());
            }
            active.missed += 1;
            if active.missed < 2 {
                return Some(active.activity.clone());
            }
        }
        self.active = next.map(|found| {
            let started = found
                .identity
                .started
                .checked_mul(1000)
                .filter(|t| *t > 0 && *t <= now_ms)
                .unwrap_or(now_ms);
            let mut value = json!({"type":0,"name":found.game.name,"timestamps":{"start":started}});
            if let Some(id) = &found.game.id {
                value["application_id"] = json!(id.to_string());
            }
            Active {
                found,
                missed: 0,
                activity: Arc::new(value),
            }
        });
        self.active.as_ref().map(|active| active.activity.clone())
    }
}

struct Detector {
    /// How many processes the last scan saw.
    seen: usize,
    catalog: Catalog,
    steam: steam::Steam,
    steam_roots: Vec<PathBuf>,
    next_steam: Instant,
    known: HashMap<u32, Known>,
    selection: Selection,
    generation: u64,
    executables: Vec<(String, Arc<GameInfo>)>,
    /// What this device never shows as the game played: executables and names.
    blocked: Vec<String>,
}

/// A path as Windows compares it: lower case, with forward slashes and
/// without the `\\?\` prefix.
fn normalize_path(path: &str) -> String {
    let mut path = path.replace('\\', "/").to_lowercase();
    if path.starts_with("//?/unc/") {
        path.replace_range(..8, "//");
    } else if path.starts_with("//?/") {
        path.drain(..4);
    }
    path
}

/// Whether `path` is the executable `entry` names: by file name, or by the
/// whole path when `entry` has a separator.
fn names_executable(entry: &str, path: &str, windows: bool) -> bool {
    let (expected, actual) = if windows {
        (
            std::borrow::Cow::Owned(normalize_path(entry)),
            std::borrow::Cow::Owned(normalize_path(path)),
        )
    } else {
        (
            std::borrow::Cow::Borrowed(entry),
            std::borrow::Cow::Borrowed(path),
        )
    };
    if expected.contains('/') {
        expected == actual
    } else {
        actual.rsplit('/').next() == Some(expected.as_ref())
    }
}

fn custom_match(
    entries: &[(String, Arc<GameInfo>)],
    path: &str,
    windows: bool,
) -> Option<Arc<GameInfo>> {
    entries.iter().find_map(|(executable, game)| {
        names_executable(executable, path, windows).then(|| game.clone())
    })
}

/// Whether the blocked list names the program at `path`, by its executable,
/// or the game `name` shown for it, which ignores case.
fn blocked_match(blocked: &[String], path: &str, name: &str, windows: bool) -> bool {
    blocked
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case(name) || names_executable(entry, path, windows))
}

fn fingerprint(process: &processes::Process) -> u64 {
    let mut hash = DefaultHasher::new();
    process.name.hash(&mut hash);
    process.exe.hash(&mut hash);
    hash.finish()
}

fn is_wine(process: &processes::Process) -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    let name = process
        .exe
        .as_deref()
        .and_then(|path| path.file_name())
        .unwrap_or(&process.name)
        .to_string_lossy();
    name.starts_with("wine")
        || process
            .name
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(".exe")
}

fn ignored(path: &str) -> bool {
    let file = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase();
    matches!(
        file.as_str(),
        "steam"
            | "steam.exe"
            | "steamwebhelper"
            | "steamwebhelper.exe"
            | "wallpaper32.exe"
            | "wallpaper64.exe"
            | "wallpaperui.exe"
            | "webwallpaper32.exe"
            | "wine"
            | "wine64"
            | "wine-preloader"
            | "wine64-preloader"
            | "wineserver"
            | "crashpad_handler"
            | "crashpad_handler.exe"
            | "unitycrashhandler64.exe"
            | "unitycrashhandler32.exe"
            | "explorer.exe"
            | "services.exe"
            | "winedevice.exe"
            | "rundll32.exe"
            | "conhost.exe"
            | "start.exe"
    ) || path
        .rsplit(['/', '\\'])
        .skip(1)
        .any(|directory| directory.eq_ignore_ascii_case("wallpaper_engine"))
        || file.starts_with("easyanticheat")
        || file.starts_with("eac_launcher")
        || file.starts_with("battleye")
        || file.starts_with("beservice")
}

fn wine_executable(args: &[std::ffi::OsString]) -> Option<String> {
    args.iter()
        .map(|arg| arg.to_string_lossy())
        .find_map(|arg| {
            let path = arg.trim_matches('"');
            (!path.starts_with('-') && path.to_ascii_lowercase().ends_with(".exe"))
                .then(|| path.to_owned())
        })
}

impl Detector {
    fn new(catalog: Catalog, steam_roots: Vec<PathBuf>) -> Self {
        Self {
            seen: 0,
            catalog,
            steam: steam::Steam::default(),
            steam_roots,
            next_steam: Instant::now(),
            known: HashMap::new(),
            selection: Selection::default(),
            generation: 0,
            executables: Vec::new(),
            blocked: Vec::new(),
        }
    }

    fn replace_catalog(&mut self, catalog: Catalog) {
        self.catalog = catalog;
        self.generation = self.generation.wrapping_add(1);
        // Steam's games are looked up again in the new catalogue.
        self.steam = steam::Steam::default();
        self.next_steam = Instant::now();
    }

    fn set_executables(&mut self, entries: &[custom_games::CustomGame]) {
        self.executables = entries
            .iter()
            .filter(|game| !game.executable.trim().is_empty() && !game.name.trim().is_empty())
            .map(|game| {
                (
                    game.executable.trim().to_owned(),
                    Arc::new(GameInfo {
                        id: None,
                        name: game.name.trim().into(),
                    }),
                )
            })
            .collect();
        self.generation = self.generation.wrapping_add(1);
        self.selection = Selection::default();
    }

    /// The games the running programs are taken for, each by the name that
    /// would show after Playing: once each, in alphabetical order, as many as
    /// a menu lists. What the blocked list leaves out isn't one.
    fn detected(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .known
            .values()
            .filter_map(|known| known.game.as_ref())
            .map(|game| game.name.to_string())
            .collect();
        names.sort_by_cached_key(|name| name.to_lowercase());
        names.dedup();
        names.truncate(DETECTED_MOST);
        names
    }

    /// Takes the list of what is never shown as the game played. Every
    /// process is looked at again, as for a changed list of custom games.
    fn set_blocked(&mut self, entries: &[String]) {
        self.blocked = entries
            .iter()
            .map(|entry| entry.trim())
            .filter(|entry| !entry.is_empty())
            .map(str::to_owned)
            .collect();
        self.generation = self.generation.wrapping_add(1);
        self.selection = Selection::default();
    }

    fn scan(&mut self, now_ms: u64) -> Option<Arc<Value>> {
        if Instant::now() >= self.next_steam {
            if let Some(steam) = steam::Steam::load(&self.steam_roots, &self.catalog, &self.steam) {
                self.steam = steam;
                self.generation = self.generation.wrapping_add(1);
            }
            self.next_steam = Instant::now() + STEAM_INTERVAL;
        }
        let processes = match processes::scan() {
            Ok(processes) => processes,
            Err(error) => {
                tracing::debug!(%error, "could not scan processes");
                return None;
            }
        };
        self.known.retain(|pid, _| processes.contains_key(pid));
        let mut due = Vec::new();
        self.seen = processes.len();
        for (pid, process) in processes {
            let started = process.started;
            let fingerprint = fingerprint(&process);
            if self.known.get(&pid).is_some_and(|known| {
                known.started == started
                    && known.fingerprint == fingerprint
                    && known.generation == self.generation
            }) {
                continue;
            }
            let wine = is_wine(&process);
            let needs_arguments = wine
                || process
                    .exe
                    .as_ref()
                    .is_some_and(|path| self.catalog.needs_arguments(&path.to_string_lossy()));
            let args = if needs_arguments {
                processes::arguments(pid, started)
            } else {
                Vec::new()
            };
            let path = if wine {
                wine_executable(&args)
            } else {
                process.exe.map(|path| {
                    path.into_os_string()
                        .into_string()
                        .unwrap_or_else(|path| path.to_string_lossy().into_owned())
                })
            };
            due.push((pid, started, fingerprint, wine, path, args));
        }
        // One read of the catalogue for all the names it has rules for.
        let windows = cfg!(target_os = "windows");
        self.catalog
            .prepare(due.iter().filter_map(|(_, _, _, wine, path, _)| {
                path.as_deref()
                    .filter(|path| !ignored(path))
                    .map(|path| (path, *wine || windows))
            }));
        for (pid, started, fingerprint, wine, path, args) in due {
            let game = path.and_then(|path| {
                let game = custom_match(&self.executables, &path, wine || windows).or_else(|| {
                    if ignored(&path) {
                        return None;
                    }
                    self.catalog
                        .matches(&path, &args, wine || windows)
                        .or_else(|| self.steam.matches(&path))
                        .or_else(|| {
                            let cwd = wine.then(|| processes::cwd(pid, started)).flatten()?;
                            self.steam.matches(&cwd.to_string_lossy())
                        })
                });
                // What the device never shows isn't a game, whatever matched.
                game.filter(|game| {
                    !blocked_match(&self.blocked, &path, &game.name, wine || windows)
                })
            });
            self.known.insert(
                pid,
                Known {
                    started,
                    fingerprint,
                    game,
                    generation: self.generation,
                },
            );
        }
        self.selection.update(
            self.known.iter().filter_map(|(&pid, known)| {
                known.game.as_ref().map(|game| Found {
                    identity: Identity {
                        pid,
                        started: known.started,
                    },
                    game: game.clone(),
                })
            }),
            now_ms,
        )
    }
}

/// A standalone diagnostic used to measure real scans, without a Discord
/// connection, CPU sampling, background sleeps or printing the process list.
#[derive(Serialize)]
pub struct Measurement {
    pub rules: usize,
    pub processes: usize,
    pub samples: usize,
    pub cold_scan_ms: f64,
    pub warm_mean_ms: f64,
    pub warm_p95_ms: f64,
    /// The heap in use before the catalogue loads and after the scans, as
    /// glibc counts it.
    pub heap_before_kib: Option<u64>,
    pub heap_after_kib: Option<u64>,
}

fn heap_kib() -> Option<u64> {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: mallinfo2 only reads the allocator's counters.
        let info = unsafe { libc::mallinfo2() };
        Some(((info.uordblks + info.hblkhd) / 1024) as u64)
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        None
    }
}

pub fn measure(path: &Path, samples: usize) -> Result<Measurement> {
    anyhow::ensure!(
        (1..=1000).contains(&samples),
        "samples must be between 1 and 1000"
    );
    let heap_before_kib = heap_kib();
    let catalog = Catalog::read(path)?;
    let rules = catalog.rule_count();
    let mut detector = Detector::new(catalog, steam::roots());
    let started = Instant::now();
    detector.scan(crate::service::now_ms());
    let cold_scan_ms = started.elapsed().as_secs_f64() * 1000.;
    let mut timings = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        detector.scan(crate::service::now_ms());
        timings.push(started.elapsed().as_secs_f64() * 1000.);
    }
    timings.sort_by(f64::total_cmp);
    Ok(Measurement {
        rules,
        processes: detector.seen,
        samples,
        cold_scan_ms,
        warm_mean_ms: timings.iter().sum::<f64>() / samples as f64,
        warm_p95_ms: timings[(samples * 95 / 100).min(samples - 1)],
        heap_before_kib,
        heap_after_kib: heap_kib(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_executables_match_names_or_exact_paths() {
        let title = Arc::new(GameInfo {
            id: None,
            name: "Custom".into(),
        });
        let names = vec![("Game.exe".into(), title.clone())];
        assert!(custom_match(&names, "C:\\Games\\GAME.EXE", true).is_some());
        assert!(custom_match(&names, "/games/game.exe", false).is_none());
        assert!(custom_match(&names, "/games/OtherGame.exe", true).is_none());
        let paths = vec![("C:\\Games\\Game.exe".into(), title.clone())];
        assert!(custom_match(&paths, "\\\\?\\C:\\Games\\GAME.exe", true).is_some());
        assert!(custom_match(&paths, "C:\\Other\\Game.exe", true).is_none());
        let unc = vec![("\\\\server\\games\\game.exe".into(), title.clone())];
        assert!(custom_match(&unc, "\\\\?\\UNC\\SERVER\\games\\GAME.exe", true).is_some());
        let unix = vec![("/games/game".into(), title)];
        assert!(custom_match(&unix, "/games/game", false).is_some());
        assert!(custom_match(&unix, "/other/games/game", false).is_none());
    }

    #[test]
    fn blocked_programs_match_by_executable_or_by_the_name_shown() {
        let blocked: Vec<String> = ["Spotify.exe", "/usr/bin/browser", "Visual Studio Code"]
            .map(String::from)
            .to_vec();
        // By file name, in any case and folder on Windows.
        assert!(blocked_match(
            &blocked,
            "C:\\Music\\SPOTIFY.EXE",
            "Anything",
            true
        ));
        // By full path, which is exact elsewhere.
        assert!(blocked_match(
            &blocked,
            "/usr/bin/browser",
            "Browser",
            false
        ));
        assert!(!blocked_match(&blocked, "/opt/browser", "Browser", false));
        // By the name shown after Playing, whatever its case.
        assert!(blocked_match(
            &blocked,
            "/opt/code/code",
            "visual studio code",
            false
        ));
        // What isn't named stays, and nothing is blocked from an empty list.
        assert!(!blocked_match(
            &blocked,
            "/games/factorio",
            "Factorio",
            false
        ));
        assert!(!blocked_match(&[], "/games/factorio", "Factorio", false));
    }

    #[test]
    fn the_blocked_list_is_trimmed_and_drops_empty_entries() {
        let mut detector = Detector::new(Catalog::default(), Vec::new());
        detector.set_blocked(&["  Spotify ".into(), String::new(), "  ".into()]);
        assert_eq!(detector.blocked, ["Spotify"]);
    }

    #[test]
    fn detected_games_are_named_once_in_order_and_programs_not_taken_for_games_are_left_out() {
        let mut detector = Detector::new(Catalog::default(), Vec::new());
        let game = |name: &str| {
            Some(Arc::new(GameInfo {
                id: None,
                name: name.into(),
            }))
        };
        for (pid, game) in [
            (1, game("Zelda")),
            (2, game("apex")),
            (3, game("Zelda")),
            (4, None),
        ] {
            detector.known.insert(
                pid,
                Known {
                    started: 0,
                    fingerprint: 0,
                    game,
                    generation: 0,
                },
            );
        }
        assert_eq!(detector.detected(), ["apex", "Zelda"]);
    }

    fn found(pid: u32, started: u64, name: &str) -> Found {
        Found {
            identity: Identity { pid, started },
            game: Arc::new(GameInfo {
                id: Some(42),
                name: name.into(),
            }),
        }
    }

    #[test]
    fn detection_is_stable_tolerates_one_miss_and_distinguishes_reused_pids() {
        let mut state = Selection::default();
        let first = state.update(vec![found(10, 100, "Game")], 300_000).unwrap();
        assert_eq!(first["timestamps"]["start"], 100_000);
        let unchanged = state
            .update(
                vec![found(10, 100, "Game"), found(20, 200, "Other")],
                300_000,
            )
            .unwrap();
        assert!(
            Arc::ptr_eq(&first, &unchanged),
            "no new JSON while the game is unchanged"
        );
        assert_eq!(state.update(vec![], 300_000), Some(first.clone()));
        assert_eq!(
            state.update(vec![found(10, 100, "Game")], 300_000),
            Some(first.clone())
        );
        assert_eq!(
            state.update(vec![found(10, 200, "Game")], 300_000),
            Some(first)
        );
        let restarted = state.update(vec![found(10, 200, "Game")], 300_000).unwrap();
        assert_eq!(restarted["timestamps"]["start"], 200_000);
        assert!(state.update(vec![], 300_000).is_some());
        assert!(state.update(vec![], 300_000).is_none());
    }

    #[test]
    fn an_rpc_exit_checks_the_fallback_without_waiting_for_two_periodic_scans() {
        let mut state = Selection::default();
        let first = state.update([found(10, 100, "Editor")], 300_000).unwrap();
        state.verify_exit();
        let still_running = state.update([found(10, 100, "Editor")], 300_000).unwrap();
        assert!(
            Arc::ptr_eq(&first, &still_running),
            "a live fallback keeps its clock"
        );
        state.verify_exit();
        let other = state.update([found(20, 200, "Music")], 300_000).unwrap();
        assert_eq!(
            other["name"], "Music",
            "a closed app is replaced on this scan"
        );
        state.verify_exit();
        assert!(state.update([], 300_000).is_none());
    }

    #[test]
    fn ignores_runtime_helpers_launchers_and_anti_cheat() {
        for path in [
            "/usr/bin/wine64",
            "C:\\Windows\\explorer.exe",
            "/games/game/EasyAntiCheat_EOS.exe",
            "/games/game/UnityCrashHandler64.exe",
            "C:\\Steam\\steamapps\\common\\wallpaper_engine\\launcher.exe",
            "/steam/steamapps/common/Wallpaper_Engine/bin/ui32.exe",
        ] {
            assert!(ignored(path), "{path}");
        }
        assert!(!ignored("/games/Game/game.exe"));
        assert!(!ignored("/games/Factorio/bin/x64/factorio"));
    }

    #[test]
    fn wine_uses_the_game_executable_and_preserves_spaces() {
        let args = [
            "/usr/bin/wine64",
            "--debug=helper.exe",
            "\"C:\\Program Files\\Game\\Game.EXE\"",
            "--fullscreen",
        ]
        .map(std::ffi::OsString::from);
        assert_eq!(
            wine_executable(&args).as_deref(),
            Some("C:\\Program Files\\Game\\Game.EXE")
        );
        assert!(wine_executable(&[std::ffi::OsString::from("/usr/bin/wineserver")]).is_none());
    }

    #[test]
    #[ignore = "child process of the game detection tests"]
    fn fixture_process() {
        std::env::var_os("YAPFAST_TEST_GAME_FIXTURE").expect("test harness only");
        use std::io::Read;
        let _ = std::io::stdin().read_exact(&mut [0_u8]);
    }

    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn custom(executable: &str, name: &str) -> custom_games::CustomGame {
        custom_games::CustomGame {
            executable: executable.into(),
            name: name.into(),
        }
    }

    #[test]
    fn scans_an_actual_game_process() {
        // Hard-link the test binary so no shell, external game or duplicate
        // large executable is needed. The fixture waits on its stdin pipe.
        let executable = std::env::current_exe().unwrap();
        let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
        let name = if cfg!(target_os = "windows") {
            "yapfast_fixture_game.exe"
        } else {
            "yapfast_fixture_game"
        };
        let game = directory.path().join(name);
        std::fs::hard_link(&executable, &game).unwrap();
        let mut child = Child(
            std::process::Command::new(&game)
                .args([
                    "--exact",
                    "game_detection::tests::fixture_process",
                    "--ignored",
                ])
                .env("YAPFAST_TEST_GAME_FIXTURE", "1")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let rules = directory.path().join("rules.json");
        std::fs::write(
            &rules,
            serde_json::to_vec(&json!([{"id":"42", "name":"Native Fixture",
            "executables":[{"os":catalog::host_os(),"name":name,"arguments":"--exact"}]}]))
            .unwrap(),
        )
        .unwrap();
        let mut detector = Detector::new(Catalog::default(), Vec::new());
        assert!(detector.scan(crate::service::now_ms()).is_none());
        // A weekly catalogue update can add argument-dependent rules for a
        // process that was already cached before its game became recognizable.
        detector.replace_catalog(Catalog::read(&rules).unwrap());
        let shown = detector.scan(crate::service::now_ms()).unwrap();
        assert_eq!(shown["name"], "Native Fixture");
        assert_eq!(shown["application_id"], "42");
        detector.set_executables(&[custom(name, "My custom title")]);
        let shown = detector.scan(crate::service::now_ms()).unwrap();
        assert_eq!(shown["name"], "My custom title");
        assert!(shown.get("application_id").is_none());
        detector.set_executables(&[custom(&game.to_string_lossy(), "Renamed live")]);
        assert_eq!(
            detector.scan(crate::service::now_ms()).unwrap()["name"],
            "Renamed live"
        );
        detector.set_executables(&[]);
        assert_eq!(
            detector.scan(crate::service::now_ms()).unwrap()["name"],
            "Native Fixture"
        );
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        assert!(detector.scan(crate::service::now_ms()).is_some());
        assert!(detector.scan(crate::service::now_ms()).is_none());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn recognizes_a_launcher_execing_a_game_without_changing_pid() {
        use std::io::Write;
        let executable = std::env::current_exe().unwrap();
        let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
        let game = directory.path().join("yf_exec_game");
        std::fs::hard_link(&executable, &game).unwrap();
        let mut child = Child(
            // exec from a single-threaded shell, as real launch scripts do.
            // proot crashes when libtest's worker thread execs a new image.
            std::process::Command::new("sh")
                .args([
                    "-c",
                    "read -r ready; exec \"$1\" --exact game_detection::tests::fixture_process --ignored",
                    "yapfast-test-launcher",
                ])
                .arg(&game)
                .env("YAPFAST_TEST_GAME_FIXTURE", "1")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let rules = directory.path().join("rules.json");
        std::fs::write(
            &rules,
            r#"[{"name":"Exec Game","executables":[{"os":"linux","name":"yf_exec_game"}]}]"#,
        )
        .unwrap();
        let mut detector = Detector::new(Catalog::read(&rules).unwrap(), Vec::new());
        assert!(detector.scan(crate::service::now_ms()).is_none());
        child
            .0
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"ready\n")
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(activity) = detector.scan(crate::service::now_ms()) {
                assert_eq!(activity["name"], "Exec Game");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "did not refresh after exec(): child={:?}, kernel_exe={:?}, cmdline={:?}",
                child.0.try_wait(),
                std::fs::read_link(format!("/proc/{}/exe", child.0.id())),
                std::fs::read(format!("/proc/{}/cmdline", child.0.id()))
                    .map(|s| String::from_utf8_lossy(&s).into_owned())
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
