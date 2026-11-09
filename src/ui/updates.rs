//! Settings' About page: whether a newer Yapfast is out. The decisions are in
//! [`crate::update`]; this keeps what the last look found, asks for another
//! when a person turns the check on or presses Check now (and once at start
//! if the check is on), and shows the result in the `Updates` global. It opens
//! the release's page in the browser and installs nothing.
//!
//! With the check on, it looks at start and every few hours after, and tells
//! of a newer release once, in a notification that opens About.

use super::*;
use crate::update::{self, Outcome, Release, Version};

/// The "room" of the notification that tells of a release; opening it shows About.
pub(super) const ABOUT: &str = "yapfast:about";

/// How long between looks while Yapfast runs. GitHub allows an address 60
/// requests an hour without an account.
const EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// The project's pages, which About's buttons open.
const WEBSITE: &str = "https://yapfast.com";
const GITHUB: &str = "https://github.com/yapfast/yapfast";

/// What the last look at the releases found.
#[derive(Default)]
pub(super) struct State {
    /// Looks again every [`EVERY`], while the check is on.
    timer: slint::Timer,
    /// The releases GitHub listed, or why it couldn't.
    found: Option<Result<Vec<Release>, String>>,
    checking: bool,
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
                latest = release.tag;
                UpdateStatus::Available
            }
            Some(Outcome::Skipped(release)) => {
                latest = release.tag;
                UpdateStatus::Skipped
            }
        },
    };
    shown.set_status(status);
    shown.set_latest(latest.into());
    shown.set_error(error.into());
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
            .unwrap_or_else(|_| Err("The look was cut short.".into()));
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
                title: format!("Yapfast {} is out", release.tag),
                body: "Open About to see it.".into(),
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
    shown.on_open_website(|| browser::open(WEBSITE));
    shown.on_open_github(|| browser::open(GITHUB));
}
