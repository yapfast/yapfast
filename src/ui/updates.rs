//! Settings' About page: whether a newer Yapfast is out. The decisions are in
//! [`crate::update`]; this keeps what the last look found, asks for another
//! when a person turns the check on or presses Check now (and once at start
//! if the check is on), and shows the result in the `Updates` global. Update
//! and restart downloads the release ([`update::install`]) while About shows
//! its progress, then ends the window with [`Exit::Update`] so `main` can
//! finish and start the new program. Nothing installs unless it is pressed.
//!
//! With the check on, it looks at start and every few hours after, and tells
//! of a newer release once, in a notification that opens About.

use super::*;
use crate::update::{
    self, Outcome, Release, Version,
    install::{self, Progress, Stage},
};

/// The "room" of the notification that tells of a release; opening it shows About.
pub(super) const ABOUT: &str = "yapfast:about";

/// How long between looks while Yapfast runs. GitHub allows an address 60
/// requests an hour without an account.
const EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// The project's pages, which About's buttons open.
const WEBSITE: &str = "https://yapfast.com";
const GITHUB: &str = "https://github.com/yapfast/yapfast";
/// The address the website sends to the project's Discord server, as the README's badge does.
const DISCORD: &str = "https://yapfast.com/discord-invite";

/// What the last look at the releases found.
#[derive(Default)]
pub(super) struct State {
    /// Looks again every [`EVERY`], while the check is on.
    timer: slint::Timer,
    /// The releases GitHub listed, or why it couldn't.
    found: Option<Result<Vec<Release>, String>>,
    checking: bool,
    /// An update under way: its waiting task, and what it has got to.
    installing: Option<Installing>,
    /// Why the last update failed.
    install_error: String,
    /// An update that is ready for `main` to finish once the window closes.
    pub(super) prepared: Option<install::Prepared>,
}

/// An update being downloaded. Dropping it stops the download.
struct Installing {
    task: slint::JoinHandle<()>,
    progress: Arc<Progress>,
    /// Reads `progress` into the window, a few times a second.
    timer: slint::Timer,
    /// The half-downloaded file, removed when the update is cancelled.
    part: std::path::PathBuf,
}

/// What the releases mean for this build under `layout`'s choices.
fn outcome(layout: &Layout, found: &[Release]) -> Option<Outcome> {
    let build = crate::build_info::current();
    let current = Version::parse(build.version())?;
    Some(update::decide(
        found,
        &current,
        build.channel(),
        layout.update_nightlies,
        layout.skipped_update.as_deref(),
    ))
}

/// Puts the choices and what was found in the window.
pub(super) fn show(window: &YapfastWindow, layout: &Layout, state: &State) {
    let shown = window.global::<Updates>();
    shown.set_check(layout.check_for_updates);
    shown.set_nightlies(layout.update_nightlies);
    shown.set_manager(update::source().manager().unwrap_or_default().into());
    let mut latest = String::new();
    let mut error = String::new();
    let mut notes = String::new();
    let mut can_install = false;
    let mut install_note = String::new();
    let status = match &state.found {
        _ if state.checking => UpdateStatus::Checking,
        None => UpdateStatus::Idle,
        Some(Err(why)) => {
            error = why.clone();
            UpdateStatus::Failed
        }
        Some(Ok(found)) => match outcome(layout, found) {
            None | Some(Outcome::UpToDate) => UpdateStatus::Current,
            Some(Outcome::FromSource) => UpdateStatus::Source,
            Some(Outcome::Available(release)) => {
                (can_install, install_note) = install_offer(&release);
                latest = release.tag;
                notes = release.notes;
                UpdateStatus::Available
            }
            Some(Outcome::Skipped(release)) => {
                latest = release.tag;
                notes = release.notes;
                UpdateStatus::Skipped
            }
        },
    };
    shown.set_status(status);
    shown.set_latest(latest.into());
    shown.set_error(error.into());
    shown.set_notes(notes.into());
    shown.set_can_install(can_install && state.installing.is_none());
    shown.set_install_note(install_note.into());
    shown.set_install_error(state.install_error.clone().into());
    shown.set_installing(state.installing.is_some() || state.prepared.is_some());
}

/// Whether Update and restart is offered for `release`, and the line under it:
/// what it will do, or why this copy can't update itself. A package manager's
/// copy says so elsewhere on the page.
fn install_offer(release: &Release) -> (bool, String) {
    match install::target(release) {
        Ok(target) => {
            let megabytes = target.asset().size.div_ceil(1_000_000);
            (
                true,
                format!("{megabytes} MB. Yapfast restarts when it's done."),
            )
        }
        Err(_) if update::source().manager().is_some() => (false, String::new()),
        Err(reason) => (false, reason),
    }
}

/// Writes how far an update has got into the window.
fn show_progress(window: &YapfastWindow, progress: &Progress) {
    let (stage, fraction) = progress.read();
    let (text, value) = match (stage, fraction) {
        (Stage::Starting, _) => ("Preparing…".to_owned(), -1.0),
        (Stage::Downloading, Some(done)) => (format!("Downloading {:.0}%", done * 100.0), done),
        (Stage::Downloading, None) => ("Downloading".to_owned(), -1.0),
        (Stage::Testing, _) => ("Checking the update…".to_owned(), -1.0),
        (Stage::Installing, _) => ("Restarting…".to_owned(), 1.0),
    };
    let shown = window.global::<Updates>();
    shown.set_install_text(text.into());
    shown.set_install_progress(value);
    shown.set_can_cancel(matches!(stage, Stage::Starting | Stage::Downloading));
}

/// Downloads the newest release for this install, and ends the window once it
/// is ready to finish. A failure is shown in About and leaves everything as it was.
fn start_install(adapter: &Rc<RefCell<Adapter>>) {
    let mut guard = adapter.borrow_mut();
    if guard.updates.installing.is_some() || guard.updates.prepared.is_some() {
        return;
    }
    let Some(Ok(found)) = &guard.updates.found else {
        return;
    };
    let Some(Outcome::Available(release)) = outcome(&guard.layout, found) else {
        return;
    };
    guard.updates.install_error.clear();
    let started = install::target(&release).and_then(|target| {
        let client = update::client()?;
        let progress = Arc::new(Progress::default());
        let runtime = guard.runtime.clone();
        let work = {
            let (target, progress) = (target.clone(), progress.clone());
            guard.runtime.spawn(async move {
                install::prepare(&runtime, &client, &release, &target, progress).await
            })
        };
        Ok((target, progress, work))
    });
    let (target, progress, work) = match started {
        Ok(started) => started,
        Err(why) => {
            guard.updates.install_error = why;
            if let Some(window) = guard.window.upgrade() {
                show(&window, &guard.layout, &guard.updates);
            }
            return;
        }
    };
    let weak = Rc::downgrade(adapter);
    let task = slint::spawn_local(async move {
        let result = work
            .await
            .unwrap_or_else(|_| Err("The update stopped partway.".into()));
        let Some(adapter) = weak.upgrade() else {
            return;
        };
        let mut adapter = adapter.borrow_mut();
        let installing = adapter.updates.installing.take();
        match result {
            Ok(prepared) => {
                tracing::info!(?prepared, "the update is ready; restarting to finish it");
                adapter.updates.prepared = Some(prepared);
                if let (Some(window), Some(installing)) = (adapter.window.upgrade(), installing) {
                    show_progress(&window, &installing.progress);
                }
                if let Err(error) = slint::quit_event_loop() {
                    tracing::error!(%error, "could not end the window to finish the update");
                    adapter.updates.prepared = None;
                    adapter.updates.install_error =
                        "Couldn't restart. Please reopen Yapfast.".into();
                }
            }
            Err(error) => {
                tracing::warn!(%error, "the update failed");
                adapter.updates.install_error = error;
            }
        }
        if let Some(window) = adapter.window.upgrade() {
            show(&window, &adapter.layout, &adapter.updates);
        }
    });
    let task = match task {
        Ok(task) => task,
        Err(error) => {
            tracing::error!(%error, "could not wait for the update");
            guard.updates.install_error = "The update couldn't start.".into();
            return;
        }
    };
    let timer = slint::Timer::default();
    let (shown, reading) = (guard.window.clone(), progress.clone());
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(200),
        move || {
            if let Some(window) = shown.upgrade() {
                show_progress(&window, &reading);
            }
        },
    );
    guard.updates.installing = Some(Installing {
        task,
        progress: progress.clone(),
        timer,
        part: target.part(),
    });
    if let Some(window) = guard.window.upgrade() {
        show_progress(&window, &progress);
        show(&window, &guard.layout, &guard.updates);
    }
}

/// Stops a download that is still going, and removes what it had written.
fn cancel(adapter: &Rc<RefCell<Adapter>>) {
    let mut adapter = adapter.borrow_mut();
    let Some(installing) = adapter.updates.installing.take() else {
        return;
    };
    let (stage, _) = installing.progress.read();
    if !matches!(stage, Stage::Starting | Stage::Downloading) {
        // Past the download: the new version is being tried, and goes on.
        adapter.updates.installing = Some(installing);
        return;
    }
    let Installing {
        task, timer, part, ..
    } = installing;
    drop(timer);
    task.abort();
    // The download's file closes as its task stops, a moment from now.
    slint::Timer::single_shot(std::time::Duration::from_millis(500), move || {
        let _ = std::fs::remove_file(&part);
    });
    if let Some(window) = adapter.window.upgrade() {
        show(&window, &adapter.layout, &adapter.updates);
    }
}

/// Asks GitHub for the releases, unless a look is under way. `announce` tells
/// of a newer release in a notification, as a look nobody asked for does.
fn look(adapter: &Rc<RefCell<Adapter>>, announce: bool) {
    let mut guard = adapter.borrow_mut();
    if guard.updates.checking {
        return;
    }
    guard.updates.checking = true;
    let task = guard.runtime.spawn(update::check());
    if let Some(window) = guard.window.upgrade() {
        show(&window, &guard.layout, &guard.updates);
    }
    drop(guard);
    let weak = Rc::downgrade(adapter);
    let spawned = slint::spawn_local(async move {
        let found = task
            .await
            .unwrap_or_else(|_| Err("The check stopped partway.".into()));
        let Some(adapter) = weak.upgrade() else {
            return;
        };
        let mut adapter = adapter.borrow_mut();
        if let Err(error) = &found {
            tracing::warn!(%error, "could not look for a newer release");
        }
        adapter.updates.checking = false;
        adapter.updates.found = Some(found);
        if let Some(window) = adapter.window.upgrade() {
            show(&window, &adapter.layout, &adapter.updates);
        }
        if announce {
            adapter.announce();
        }
    });
    if let Err(error) = spawned {
        tracing::error!(%error, "could not wait for the look at the releases");
        adapter.borrow_mut().updates.checking = false;
    }
}

/// Shows the saved choices, and looks once if the check is on.
pub(super) fn start(adapter: &Rc<RefCell<Adapter>>, window: &YapfastWindow) {
    let checking = {
        let adapter = adapter.borrow();
        show(window, &adapter.layout, &adapter.updates);
        adapter.layout.check_for_updates
    };
    if checking {
        look(adapter, true);
    }
    let weak = Rc::downgrade(adapter);
    adapter
        .borrow()
        .updates
        .timer
        .start(slint::TimerMode::Repeated, EVERY, move || {
            if let Some(adapter) = weak.upgrade()
                && adapter.borrow().layout.check_for_updates
            {
                look(&adapter, true);
            }
        });
}

impl Adapter {
    /// Tells of the newest release in a notification, once for each release,
    /// unless notifications are off or the person skipped it.
    fn announce(&mut self) {
        let Some(Ok(found)) = &self.updates.found else {
            return;
        };
        let Some(Outcome::Available(release)) = outcome(&self.layout, found) else {
            return;
        };
        if self.layout.announced_update.as_deref() == Some(release.tag.as_str()) {
            return;
        }
        self.layout.announced_update = Some(release.tag.clone());
        self.layout.write(&self.root);
        if !self.layout.notifications.unwrap_or(true) {
            return;
        }
        self.notifier.show(
            yapfast_core::chat::Alert {
                room: ABOUT.into(),
                title: format!("Yapfast {} is available", release.tag),
                body: "Click to see what's new.".into(),
                picture: None,
                call: false,
                mention: false,
            },
            crate::notify::Style { text: true },
        );
    }
}

pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    let shown = window.global::<Updates>();
    let (state, weak) = (adapter.clone(), window.as_weak());
    shown.on_preferences_changed(move || {
        let Some(window) = weak.upgrade() else { return };
        let (check, nightlies) = {
            let shown = window.global::<Updates>();
            (shown.get_check(), shown.get_nightlies())
        };
        let turned_on = {
            let mut adapter = state.borrow_mut();
            let turned_on = check && !adapter.layout.check_for_updates;
            adapter.layout.check_for_updates = check;
            adapter.layout.update_nightlies = nightlies;
            adapter.layout.write(&adapter.root);
            show(&window, &adapter.layout, &adapter.updates);
            turned_on
        };
        if turned_on {
            look(&state, true);
        }
    });
    let state = adapter.clone();
    shown.on_check_now(move || look(&state, false));
    let (state, weak) = (adapter.clone(), window.as_weak());
    shown.on_skip(move || {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        let Some(Ok(found)) = &adapter.updates.found else {
            return;
        };
        let Some(Outcome::Available(release)) = outcome(&adapter.layout, found) else {
            return;
        };
        adapter.layout.skipped_update = Some(release.tag);
        adapter.layout.write(&adapter.root);
        show(&window, &adapter.layout, &adapter.updates);
    });
    let state = adapter.clone();
    shown.on_view(move || {
        let adapter = state.borrow();
        let Some(Ok(found)) = &adapter.updates.found else {
            return;
        };
        if let Some(Outcome::Available(release) | Outcome::Skipped(release)) =
            outcome(&adapter.layout, found)
        {
            browser::open(&release.page);
        }
    });
    let state = adapter.clone();
    shown.on_install(move || start_install(&state));
    let state = adapter.clone();
    shown.on_cancel_install(move || cancel(&state));
    shown.on_open_website(|| browser::open(WEBSITE));
    shown.on_open_github(|| browser::open(GITHUB));
    shown.on_open_discord(|| browser::open(DISCORD));
}
