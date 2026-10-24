//! The window, which closing to the tray drops and opening makes again, and
//! the tray icon that keeps Yapfast running meanwhile, with its session, calls,
//! notifications and sounds.

use std::rc::Weak;

use super::*;
use crate::tray::{Choice, Shown, Tray};

thread_local! {
    /// Where the tray's choices go.
    static SHELL: RefCell<Weak<Shell>> = const { RefCell::new(Weak::new()) };
}

/// Yapfast's window while it's open, and what makes it again.
pub(super) struct Shell {
    window: RefCell<Option<YapfastWindow>>,
    pub(super) adapter: Rc<RefCell<Adapter>>,
    /// Shown in every window: why Yapfast draws with Skia Software, if the GPU failed.
    notice: SharedString,
    gpu: bool,
    /// The renderer setting changed and Yapfast restarts to use it.
    restart: Cell<bool>,
    /// Why a window made after the first failed on the GPU; Yapfast restarts
    /// with Skia Software.
    gpu_failure: RefCell<Option<String>>,
    /// What the closed window showed that the application doesn't keep.
    view: RefCell<Option<View>>,
}

/// Where a closed window's timeline was scrolled to; the layout keeps where
/// the window was.
struct View {
    /// The message at the top of the timeline, unless the newest showed.
    top: Option<SharedString>,
}

impl Shell {
    /// Takes the first window, bound and refreshed, and puts Yapfast in the tray.
    pub(super) fn new(
        window: YapfastWindow,
        adapter: Rc<RefCell<Adapter>>,
        notice: SharedString,
        gpu: bool,
    ) -> Rc<Self> {
        let shell = Rc::new(Self {
            window: RefCell::new(None),
            adapter,
            notice,
            gpu,
            restart: Cell::new(false),
            gpu_failure: RefCell::new(None),
            view: RefCell::new(None),
        });
        shell.watch(&window);
        *shell.window.borrow_mut() = Some(window);
        SHELL.with(|current| *current.borrow_mut() = Rc::downgrade(&shell));
        OPEN.with(|open| {
            *open.borrow_mut() = Some(Box::new(|room| {
                if let Some(shell) = SHELL.with(|shell| shell.borrow().upgrade()) {
                    shell.open_room(room);
                }
            }));
        });
        let tray = Tray::start(chosen);
        let mut adapter = shell.adapter.borrow_mut();
        adapter.tray = tray;
        adapter.show_tray();
        drop(adapter);
        shell
    }

    /// Whether Yapfast ends to restart with another renderer.
    pub(super) fn restarting(&self) -> bool {
        self.restart.get()
    }

    /// Why a window made late failed on the GPU, if one did.
    pub(super) fn gpu_failure(&self) -> Option<String> {
        self.gpu_failure.borrow_mut().take()
    }

    /// Goes to the tray, as starting at sign-in does: the first window, never
    /// shown, goes, and the next opens where a first one would.
    pub(super) fn start_in_tray(self: &Rc<Self>) {
        // Windows knows at once whether the icon went in. On Linux the bus says
        // a moment later, and without a tray `chosen` opens the window.
        let missing = match self.adapter.borrow().tray.as_ref() {
            None => true,
            Some(tray) => cfg!(windows) && !tray.available(),
        };
        if missing {
            self.show_first();
            return;
        }
        if let Some(window) = self.window.borrow_mut().take() {
            self.adapter.borrow_mut().detach();
            drop(window);
        }
        tracing::info!("started in the tray");
        slint::Timer::single_shot(Duration::from_millis(500), release_memory);
    }

    /// Shows the first window, which started hidden for a tray there wasn't.
    fn show_first(self: &Rc<Self>) {
        let Some(window) = self.window() else {
            return;
        };
        match window.show() {
            Ok(()) => self.confirm_gpu(&window),
            Err(error) => self.fail_on_gpu(error.to_string()),
        }
    }

    /// As for the first window shown at start: Slint can go on with no window
    /// when the GPU surface fails, so check for the native one.
    fn confirm_gpu(self: &Rc<Self>, window: &YapfastWindow) {
        if !self.gpu {
            return;
        }
        let shell = Rc::downgrade(self);
        let weak = window.as_weak();
        slint::Timer::single_shot(GPU_START_CONFIRMATION, move || {
            if let Some(shell) = shell.upgrade()
                && weak
                    .upgrade()
                    .is_some_and(|window| !window.window().has_winit_window())
            {
                shell.fail_on_gpu("the window did not appear".to_owned());
            }
        });
    }

    /// A window failed to appear: with the GPU, Yapfast restarts with Skia
    /// Software, which shows it.
    fn fail_on_gpu(&self, reason: String) {
        tracing::error!(%reason, "could not show the window");
        if self.gpu {
            *self.gpu_failure.borrow_mut() = Some(reason);
            let _ = slint::quit_event_loop();
        }
    }

    pub(super) fn window(&self) -> Option<YapfastWindow> {
        self.window
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
    }

    /// What every window does beyond its callbacks: closes to the tray, and
    /// restarts Yapfast.
    fn watch(self: &Rc<Self>, window: &YapfastWindow) {
        let shell = Rc::downgrade(self);
        window.window().on_close_requested(move || {
            let shell = shell.clone();
            // Not from inside the window's own handler, which drops it.
            slint::Timer::single_shot(Duration::ZERO, move || {
                if let Some(shell) = shell.upgrade() {
                    shell.close();
                }
            });
            slint::CloseRequestResponse::KeepWindowShown
        });
        let shell = Rc::downgrade(self);
        window.on_restart(move || {
            if let Some(shell) = shell.upgrade() {
                shell.restart.set(true);
            }
            let _ = slint::quit_event_loop();
        });
    }

    /// The window's close button: Yapfast goes on in the tray, or without a tray
    /// to show it, or with the setting off, quits.
    pub(super) fn close(&self) {
        let to_tray = {
            let adapter = self.adapter.borrow();
            adapter.layout.close_to_tray.unwrap_or(true)
                && adapter.tray.as_ref().is_some_and(Tray::available)
        };
        if !to_tray {
            tracing::info!("window closed without a tray to close to; quitting");
            let _ = slint::quit_event_loop();
            return;
        }
        self.drop_window();
        // Slint lets go of the window's last pieces as the event loop goes on.
        slint::Timer::single_shot(Duration::from_millis(500), release_memory);
    }

    /// Drops the window, its component, models' views and renderer, keeping
    /// where it was for the next.
    pub(super) fn drop_window(&self) {
        let Some(window) = self.window.borrow_mut().take() else {
            return;
        };
        let top = window.get_top_item();
        *self.view.borrow_mut() = Some(View {
            top: (!window.invoke_timeline_at_end() && !top.is_empty()).then_some(top),
        });
        let mut adapter = self.adapter.borrow_mut();
        placement::moved(&mut adapter, &window);
        adapter.detach();
        drop(adapter);
        // A shown window keeps itself alive; hide it, then drop the last handle.
        let weak = window.as_weak();
        if let Err(error) = window.hide() {
            tracing::warn!(%error, "could not hide the window");
        }
        drop(window);
        FOCUSED.set(false);
        tracing::info!(released = weak.upgrade().is_none(), "window closed");
    }

    /// Brings the window to the front, making it again if it was closed.
    pub(super) fn open(self: &Rc<Self>) -> Option<YapfastWindow> {
        if let Some(window) = self.window() {
            let _ = window.show();
            window.window().set_minimized(false);
            window.window().with_winit_window(|winit| {
                winit.focus_window();
                #[cfg(target_os = "linux")]
                {
                    use slint::winit_030::winit::{
                        raw_window_handle::{HasDisplayHandle, RawDisplayHandle},
                        window::UserAttentionType,
                    };
                    // On Wayland focus_window is a no-op; winit requests
                    // xdg_activation through request_user_attention instead.
                    if winit.display_handle().is_ok_and(|display| {
                        matches!(display.as_raw(), RawDisplayHandle::Wayland(_))
                    }) {
                        winit.request_user_attention(Some(UserAttentionType::Informational));
                    }
                }
            });
            return Some(window);
        }
        match self.make() {
            Ok(window) => Some(window),
            Err(error) => {
                self.fail_on_gpu(error.to_string());
                None
            }
        }
    }

    /// Opens the window on a conversation, as a clicked notification does.
    fn open_room(self: &Rc<Self>, room: String) {
        if let Some(window) = self.open() {
            window.set_settings_open(false);
            self.adapter
                .borrow_mut()
                .action(&window, Action::OpenChat(room));
        }
    }

    /// Makes the window again, as the closed one was: the same size and place,
    /// the conversation scrolled to the same message, and everything else from
    /// the application's state.
    fn make(self: &Rc<Self>) -> Result<YapfastWindow, slint::PlatformError> {
        let window = YapfastWindow::new()?;
        let view = self.view.borrow_mut().take();
        let adapter = self.adapter.borrow();
        title_bar::show(&window, adapter.layout.system_title_bar);
        if let Some(placement) = &adapter.layout.window {
            placement::place(&window, placement);
        }
        drop(adapter);
        window.show()?;
        self.confirm_gpu(&window);
        prepare(&window);
        window.set_graphics_notice(self.notice.clone());
        window.set_gpu_running(self.gpu);
        self.adapter.borrow_mut().attach(&window);
        bind_callbacks(&window, &self.adapter);
        watch_window(&window, &self.adapter);
        self.watch(&window);
        self.adapter.borrow_mut().refresh(&window);
        let adapter = self.adapter.borrow();
        let state = &adapter.app.state;
        if let Some(top) = view.and_then(|view| view.top)
            && let Some(index) = state
                .messages
                .iter()
                .position(|message| message.item == top.as_str())
        {
            show_message(&window, index, &top, false);
        }
        if state.shows_text() {
            window.invoke_focus_composer();
        }
        drop(adapter);
        *self.window.borrow_mut() = Some(window.clone_strong());
        tracing::info!("window opened again");
        Ok(window)
    }

    /// Does what a button in the window would, with the window or without it.
    fn act(&self, action: Action) {
        let mut adapter = self.adapter.borrow_mut();
        match self.window() {
            Some(window) => adapter.action(&window, action),
            None => {
                let effects = adapter.app.dispatch(action);
                adapter.execute(effects);
                adapter.show_tray();
            }
        }
    }
}

/// Makes the window again where it was, on Appearance as the setting that
/// asked for it, for a change Slint can't make to the shown window.
pub(super) fn remake_window() {
    let Some(shell) = SHELL.with(|shell| shell.borrow().upgrade()) else {
        return;
    };
    shell.drop_window();
    if let Some(window) = shell.open() {
        window.set_settings_section(7);
        window.set_settings_open(true);
    }
}

/// What the person chose in the tray.
fn chosen(choice: Choice) {
    let Some(shell) = SHELL.with(|shell| shell.borrow().upgrade()) else {
        return;
    };
    match choice {
        Choice::Open => {
            shell.open();
        }
        Choice::Disconnect => shell.act(Action::Voice(voice::Command::Leave)),
        Choice::Quit => {
            let _ = slint::quit_event_loop();
        }
        Choice::Available => {
            let available = shell
                .adapter
                .borrow()
                .tray
                .as_ref()
                .is_some_and(Tray::available);
            match shell.window() {
                Some(window) => window.set_tray_available(available),
                // Without a tray to open it from, the window can't stay closed.
                None if !available => {
                    tracing::info!("the tray went; opening the window");
                    shell.open();
                }
                None => {}
            }
        }
    }
}

impl Adapter {
    /// The window closed: what only it used stops with it. The models stay, so
    /// the next window shows them without building them again.
    pub(super) fn detach(&mut self) {
        self.meter = None;
        #[cfg(feature = "discord")]
        {
            self.gate_meter = None;
        }
        self.camera_test = None;
        self.viewer_image = None;
        if let Some(dialog) = self.dialog.take() {
            dialog.abort();
        }
        self.window = slint::Weak::default();
    }

    /// Rings for a DM call that rings this account, except in Do Not Disturb,
    /// which shows the call without ringing.
    pub(super) fn ring(&mut self) {
        #[cfg(feature = "discord")]
        {
            let state = &self.app.state;
            if ringing(state).next().is_none() || state.do_not_disturb() {
                self.ringer = None;
            } else if self.ringer.is_none() {
                self.ringer = yapfast_core::ring::Ringer::start(
                    yapfast_core::ring::Tone::Incoming,
                    state.voice.speaker.as_deref(),
                )
                .inspect_err(|error| tracing::warn!(%error, "could not ring"))
                .ok();
            }
        }
    }

    /// The window's title, counting the mentions waiting, as a browser
    /// tab's does; every message in a DM counts as one.
    pub(super) fn show_title(&self, window: &YapfastWindow) {
        let waiting: u64 = self.app.state.rooms.iter().map(|room| room.mentions).sum();
        let title = match waiting {
            0 => "Yapfast".to_owned(),
            1..=99 => format!("({waiting}) Yapfast"),
            _ => "(99+) Yapfast".to_owned(),
        };
        if window.get_window_title() != title.as_str() {
            window.set_window_title(title.into());
        }
    }

    /// The tray icon's unread dot, for a mention or an unread DM as Discord
    /// counts them, and Disconnect in its menu during a call.
    pub(super) fn show_tray(&self) {
        let state = &self.app.state;
        let Some(tray) = &self.tray else { return };
        tray.show(Shown {
            unread: state
                .rooms
                .iter()
                .any(|room| room.mentions > 0 || (room.direct && !room.muted && room.unread > 0)),
            call: state.voice.active(),
            speaking: state.voice.active()
                && state.voice.participants.iter().any(|person| {
                    person.local && person.speaking && !person.muted && person.connected
                })
                && self.layout.tray_voice_activity.unwrap_or(true),
            icon_style: self.layout.icon_style.unwrap_or(0).clamp(0, 2) as u8,
            custom_icon: self.custom_icon.clone(),
            idle_icon: self.tray_idle_icon.clone(),
            speaking_icon: self.tray_speaking_icon.clone(),
        });
    }
}

/// Gives back memory freed with the window. On Linux glibc keeps freed heap
/// for reuse until trimmed. On Windows the heap is compacted, which releases
/// its free blocks, and the working set emptied: Task Manager shows the
/// private working set, which pages the window used would otherwise stay in.
pub(super) fn release_memory() {
    trim();
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
