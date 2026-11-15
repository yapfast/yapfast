//! The iced client's desktop (`yapfast_iced::Desktop`), made of the shared
//! desktop modules the Slint client uses: the tray (`tray`), notifications
//! (`notify`), the single instance (`instance`), starting at sign-in
//! (`autostart`), Discord's links (`protocol`), copied pictures (`copy`),
//! the window's icon (`icon`), files dragged over it on Wayland (`drop`) and
//! updates (`update`), as the Slint client's `src/ui/updates.rs` drives them.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use yapfast_core::{chat::Alert, runtime::BackgroundRuntime};
use yapfast_iced::{Heard, Offered, Tell, TrayChoice, UpdateChoices, UpdateStatus, UpdatesShown};

use crate::update::{
    self, Outcome, Release, Version,
    install::{self, Prepared, Progress, Stage},
};

/// What the iced window asks of the desktop, for one start of Yapfast.
pub struct Shell {
    runtime: Arc<BackgroundRuntime>,
    /// The preview offers neither starting at sign-in nor Discord's links.
    preview: bool,
    instance: Mutex<crate::instance::Instance>,
    tell: OnceLock<Tell>,
    updates: Arc<Mutex<Updating>>,
}

/// What the last look at the releases found, and an update under way. The
/// tasks write their results here.
#[derive(Default)]
struct Updating {
    /// The releases GitHub listed, or why it couldn't.
    found: Option<Result<Vec<Release>, String>>,
    checking: bool,
    /// The last look's task and the last update's: dropping one stops it,
    /// and a finished one is replaced by the next.
    look: Option<yapfast_core::runtime::BackgroundTask<()>>,
    install: Option<yapfast_core::runtime::BackgroundTask<()>>,
    installing: Option<Installing>,
    install_error: String,
    /// An update that is ready for `main` to finish once the window closes.
    prepared: Option<Prepared>,
}

/// An update being downloaded.
struct Installing {
    progress: Arc<Progress>,
    /// The half-downloaded file, removed when the update is cancelled.
    part: PathBuf,
}

impl Shell {
    pub fn new(
        runtime: Arc<BackgroundRuntime>,
        preview: bool,
        instance: crate::instance::Instance,
    ) -> Arc<Self> {
        Arc::new(Self {
            runtime,
            preview,
            instance: Mutex::new(instance),
            tell: OnceLock::new(),
            updates: Arc::default(),
        })
    }

    /// The update downloaded and checked, for `main` to finish.
    pub fn take_prepared(&self) -> Option<Prepared> {
        self.lock_updates().prepared.take()
    }

    fn lock_updates(&self) -> std::sync::MutexGuard<'_, Updating> {
        self.updates
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn tell(&self, heard: Heard) {
        if let Some(tell) = self.tell.get() {
            tell(heard);
        }
    }
}

/// The tray, as `tray::Tray`.
struct Tray(crate::tray::Tray);

impl yapfast_iced::Tray for Tray {
    fn available(&self) -> bool {
        self.0.available()
    }

    fn show(&self, unread: bool, call: bool) {
        self.0.show(crate::tray::Shown { unread, call });
    }
}

struct Notifier(crate::notify::Notifier);

impl yapfast_iced::Notifier for Notifier {
    fn show(&self, alert: Alert, text: bool) {
        self.0.show(alert, crate::notify::Style { text });
    }
}

impl yapfast_iced::Desktop for Shell {
    fn app_id(&self) -> &'static str {
        crate::APP_ID
    }

    fn start(&self, tell: Tell) {
        let _ = self.tell.set(tell.clone());
        self.instance
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .listen(move |launch| {
                tell(Heard::Launched {
                    token: launch.token,
                    link: launch.link,
                })
            });
    }

    fn tray(&self, tell: Tell) -> Option<Box<dyn yapfast_iced::Tray>> {
        let tray = crate::tray::Tray::start(Arc::new(move |choice| {
            tell(Heard::Tray(match choice {
                crate::tray::Choice::Open => TrayChoice::Open,
                crate::tray::Choice::Disconnect => TrayChoice::Disconnect,
                crate::tray::Choice::Quit => TrayChoice::Quit,
                crate::tray::Choice::Available => TrayChoice::Available,
            }))
        }))?;
        Some(Box::new(Tray(tray)))
    }

    fn tray_token(&self) -> Option<String> {
        crate::tray::take_activation_token()
    }

    fn notifier(&self, root: &Path, tell: Tell) -> Option<Box<dyn yapfast_iced::Notifier>> {
        let notifier =
            crate::notify::Notifier::start(Arc::new(move |room| tell(Heard::Opened(room))), root);
        Some(Box::new(Notifier(notifier)))
    }

    fn activate(&self, window: &dyn yapfast_iced::Window, token: &str) {
        #[cfg(target_os = "linux")]
        crate::instance::activate(window, token);
        #[cfg(not(target_os = "linux"))]
        let _ = (window, token);
    }

    fn decorate(&self, window: &dyn yapfast_iced::Window, tell: Tell) {
        #[cfg(windows)]
        if let Ok(handle) = window.window_handle()
            && let raw_window_handle::RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            crate::icon::set_on(handle.hwnd.get());
        }
        #[cfg(target_os = "linux")]
        watch_wayland_drops(window, tell);
        #[cfg(not(target_os = "linux"))]
        let _ = tell;
        #[cfg(not(any(windows, target_os = "linux")))]
        let _ = window;
    }

    fn window_icon(&self) -> Option<(Vec<u8>, u32, u32)> {
        #[cfg(target_os = "linux")]
        {
            let icon = crate::icon::window_icon()?;
            Some((icon.pixels, icon.width, icon.height))
        }
        #[cfg(not(target_os = "linux"))]
        None
    }

    fn autostart(&self) -> Option<bool> {
        (crate::autostart::SUPPORTED && !self.preview).then(crate::autostart::enabled)
    }

    fn set_autostart(&self, on: bool) -> Result<(), String> {
        crate::autostart::set(on).map_err(|error| error.to_string())
    }

    fn discord_links(&self, root: &Path) -> Option<bool> {
        (crate::protocol::SUPPORTED && !self.preview)
            .then(|| crate::protocol::discord_enabled(root))
    }

    fn set_discord_links(&self, root: &Path, on: bool) -> Result<(), String> {
        crate::protocol::set_discord_enabled(root, on).map_err(|error| error.to_string())
    }

    fn copy_picture(&self, bytes: Vec<u8>) {
        crate::copy::picture(bytes);
    }

    fn release_memory(&self) {
        release_memory();
    }

    fn updates(&self) -> Option<&dyn yapfast_iced::Updates> {
        Some(self)
    }
}

/// Files dragged over a Wayland window, which winit doesn't follow there,
/// through a data device of the shared `drop` module's own.
#[cfg(target_os = "linux")]
fn watch_wayland_drops(window: &dyn yapfast_iced::Window, tell: Tell) {
    use raw_window_handle::RawDisplayHandle;
    static WATCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let Ok(display) = window.display_handle() else {
        return;
    };
    let RawDisplayHandle::Wayland(display) = display.as_raw() else {
        return;
    };
    // One device for the seat serves every window made again.
    if WATCHING.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    // SAFETY: winit's display stays connected while its event loop runs,
    // which is as long as Yapfast does.
    unsafe {
        crate::drop::wayland::watch(display.display, move |drag| {
            use yapfast_iced::Drag;
            let drag = match drag {
                crate::drop::Drag::Entered => Drag::Entered,
                crate::drop::Drag::Left => Drag::Left,
                crate::drop::Drag::Dropped(files) => Drag::Dropped(files),
                crate::drop::Drag::Moved { .. } => return,
            };
            tell(Heard::Drag(drag));
        });
    }
}

/// Gives back memory freed with the window, as the Slint client does when
/// its window closes to the tray: glibc's free heap on Linux; the heap
/// compacted and the working set emptied on Windows, where Task Manager
/// shows the private working set.
pub fn release_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> std::ffi::c_int;
        }
        // SAFETY: malloc_trim only returns free memory to the system.
        unsafe { malloc_trim(0) };
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::{
            Memory::{GetProcessHeap, HeapCompact},
            Threading::{GetCurrentProcess, SetProcessWorkingSetSize},
        };
        // SAFETY: both act on this process's own heap and working set; -1 for
        // both sizes asks Windows to take out every page it can.
        unsafe {
            HeapCompact(GetProcessHeap(), 0);
            SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
        }
    }
    tracing::info!("memory released");
}

/// What the releases mean for this build under the layout's choices.
fn outcome(choices: &UpdateChoices, found: &[Release]) -> Option<Outcome> {
    let build = crate::build_info::current();
    let current = Version::parse(build.version())?;
    Some(update::decide(
        found,
        &current,
        build.channel(),
        choices.nightlies,
        choices.skipped.as_deref(),
    ))
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

impl yapfast_iced::Updates for Shell {
    fn look(&self) {
        let mut updating = self.lock_updates();
        if updating.checking {
            return;
        }
        updating.checking = true;
        let (shared, tell) = (self.updates.clone(), self.tell.get().cloned());
        updating.look = Some(self.runtime.spawn(async move {
            let found = update::check().await;
            if let Err(error) = &found {
                tracing::warn!(%error, "could not look for a newer release");
            }
            let mut updating = shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            updating.checking = false;
            updating.found = Some(found);
            drop(updating);
            if let Some(tell) = tell {
                tell(Heard::Updates);
            }
        }));
        drop(updating);
        self.tell(Heard::Updates);
    }

    fn shown(&self, choices: &UpdateChoices) -> UpdatesShown {
        let updating = self.lock_updates();
        let mut shown = UpdatesShown {
            manager: update::source().manager().unwrap_or_default().to_owned(),
            install_error: updating.install_error.clone(),
            installing: updating.installing.is_some() || updating.prepared.is_some(),
            ..UpdatesShown::default()
        };
        shown.status = match &updating.found {
            _ if updating.checking => UpdateStatus::Checking,
            None => UpdateStatus::Idle,
            Some(Err(why)) => {
                shown.error = why.clone();
                UpdateStatus::Failed
            }
            Some(Ok(found)) => match outcome(choices, found) {
                None | Some(Outcome::UpToDate) => UpdateStatus::Current,
                Some(Outcome::FromSource) => UpdateStatus::Source,
                Some(Outcome::Available(release)) => {
                    let (can_install, note) = install_offer(&release);
                    shown.can_install = can_install && updating.installing.is_none();
                    shown.install_note = note;
                    shown.latest = release.tag;
                    shown.notes = release.notes;
                    UpdateStatus::Available
                }
                Some(Outcome::Skipped(release)) => {
                    shown.latest = release.tag;
                    shown.notes = release.notes;
                    UpdateStatus::Skipped
                }
            },
        };
        if let Some(installing) = &updating.installing {
            let (stage, fraction) = installing.progress.read();
            (shown.install_text, shown.install_progress) = match (stage, fraction) {
                (Stage::Starting, _) => ("Preparing…".to_owned(), -1.0),
                (Stage::Downloading, Some(done)) => {
                    (format!("Downloading {:.0}%", done * 100.0), done)
                }
                (Stage::Downloading, None) => ("Downloading".to_owned(), -1.0),
                (Stage::Testing, _) => ("Checking the update…".to_owned(), -1.0),
                (Stage::Installing, _) => ("Restarting…".to_owned(), 1.0),
            };
            shown.can_cancel = matches!(stage, Stage::Starting | Stage::Downloading);
        } else if updating.prepared.is_some() {
            (shown.install_text, shown.install_progress) = ("Restarting…".to_owned(), 1.0);
        }
        shown
    }

    fn offered(&self, choices: &UpdateChoices) -> Option<Offered> {
        let updating = self.lock_updates();
        let Some(Ok(found)) = &updating.found else {
            return None;
        };
        let (release, skipped) = match outcome(choices, found)? {
            Outcome::Available(release) => (release, false),
            Outcome::Skipped(release) => (release, true),
            Outcome::UpToDate | Outcome::FromSource => return None,
        };
        Some(Offered {
            tag: release.tag,
            page: release.page,
            skipped,
        })
    }

    fn install(&self, choices: &UpdateChoices) {
        let mut updating = self.lock_updates();
        if updating.installing.is_some() || updating.prepared.is_some() {
            return;
        }
        let Some(Ok(found)) = &updating.found else {
            return;
        };
        let Some(Outcome::Available(release)) = outcome(choices, found) else {
            return;
        };
        updating.install_error.clear();
        let started = install::target(&release)
            .and_then(|target| update::client().map(|client| (target, client)));
        let (target, client) = match started {
            Ok(started) => started,
            Err(why) => {
                updating.install_error = why;
                drop(updating);
                self.tell(Heard::Updates);
                return;
            }
        };
        let progress = Arc::new(Progress::default());
        let (runtime, shared, tell) = (
            self.runtime.clone(),
            self.updates.clone(),
            self.tell.get().cloned(),
        );
        let part = target.part();
        let reading = progress.clone();
        updating.install = Some(self.runtime.spawn(async move {
            let result = install::prepare(&runtime, &client, &release, &target, reading).await;
            let mut updating = shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            updating.installing = None;
            match result {
                Ok(prepared) => {
                    tracing::info!(?prepared, "the update is ready; restarting to finish it");
                    updating.prepared = Some(prepared);
                }
                Err(error) => {
                    tracing::warn!(%error, "the update failed");
                    updating.install_error = error;
                }
            }
            drop(updating);
            if let Some(tell) = tell {
                tell(Heard::Updates);
            }
        }));
        updating.installing = Some(Installing { progress, part });
        drop(updating);
        self.tell(Heard::Updates);
    }

    fn cancel(&self) {
        let mut updating = self.lock_updates();
        let Some(installing) = &updating.installing else {
            return;
        };
        let (stage, _) = installing.progress.read();
        // Past the download, the new version is being tried, and goes on.
        if !matches!(stage, Stage::Starting | Stage::Downloading) {
            return;
        }
        let part = installing.part.clone();
        updating.installing = None;
        updating.install = None;
        drop(updating);
        // The download's file closes as its task stops, a moment from now.
        let _ = std::thread::Builder::new()
            .name("yapfast-update-cancel".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let _ = std::fs::remove_file(&part);
            });
        self.tell(Heard::Updates);
    }

    fn ready(&self) -> bool {
        self.lock_updates().prepared.is_some()
    }
}
