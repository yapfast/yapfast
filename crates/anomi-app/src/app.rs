//! The program: its state and its `update`, `view`, `subscription`, `theme`
//! and `title`, which `run` hands iced.

use std::path::PathBuf;
use std::sync::Arc;

use iced::{Subscription, Task, window};
use yapfast_core::application::{Action, Application};
use yapfast_core::runtime::BackgroundRuntime;

use anomi_theme::{Motion, Theme, View, builtin};
use anomi_vm::Vm;

use crate::backend::Connection;
use crate::layout::Layout;
use crate::message::Message;
use crate::{backend, cli, control, effects, intent, project};

/// The whole program's state. Fields drop in order: the backend's events
/// and the backend itself go before the runtime they run on.
pub struct App {
    pub(crate) backend: Connection,
    pub(crate) runtime: Arc<BackgroundRuntime>,
    /// The data folder: the session, `layout.json`.
    pub(crate) root: PathBuf,
    /// The core: navigation, drafts, everything the networks keep.
    pub(crate) core: Application,
    /// What the screens show, made from the core's state.
    pub(crate) vm: Vm,
    pub(crate) shown: project::Shown,
    pub(crate) layout: Layout,
    pub(crate) theme: Theme,
    pub(crate) motion: Motion,
    pub(crate) window: Option<window::Id>,
}

impl App {
    pub fn new(runtime: Arc<BackgroundRuntime>, root: PathBuf, layout: Layout) -> Self {
        let theme = Theme::built_in(layout.theme());
        let mut vm = Vm::default();
        if let Some(width) = layout.sidebar_width() {
            vm.shell.sidebar_width = width.clamp(
                anomi_theme::metrics::SIDEBAR_MIN,
                anomi_theme::metrics::SIDEBAR_MAX,
            );
        }
        Self {
            backend: Connection::default(),
            runtime,
            root,
            core: Application::default(),
            vm,
            shown: project::Shown::default(),
            layout,
            theme,
            // In front once the window says so; until then, still.
            motion: Motion::default(),
            window: None,
        }
    }

    /// Opens the window and starts the session, as the Slint app does: the
    /// preview, or the saved session.
    pub fn boot(mut self, start: &cli::Start) -> (Self, Task<Message>) {
        let (_id, opened) = window::open(crate::window::settings());
        let first = if start.preview {
            Action::Preview
        } else {
            // With no saved session the core says so, and the sign-in
            // screen's task shows it.
            Action::Restore
        };
        let started = self.dispatch(first);
        let control = control::path().map_or_else(Task::none, control::watch);
        (
            self,
            Task::batch([opened.map(Message::Opened), started, control]),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Intent(intent) => intent::handle(self, intent),
            Message::Backend { run, events } => backend::receive(self, run, events),
            Message::BackendClosed(run) => backend::closed(self, run),
            Message::Opened(id) => {
                self.window = Some(id);
                Task::none()
            }
            Message::Focused(front) => {
                // The background rule: behind another window nothing moves.
                tracing::debug!(front, "the window's focus");
                self.motion.front = front;
                Task::none()
            }
            // No tray yet (I14): closing the window quits.
            Message::CloseRequested(_) => iced::exit(),
            Message::Control(line) => control::run(self, &line),
        }
    }

    pub fn view(&self, _window: window::Id) -> impl View<Message> + '_ {
        use iced::Widget as _;
        anomi_screens::window(&self.vm, self.motion).map(Message::Intent)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        crate::window::subscription()
    }

    pub fn theme(&self, _window: window::Id) -> Theme {
        self.theme.clone()
    }

    pub fn title(&self, _window: window::Id) -> String {
        yapfast_lang::PRODUCT.to_owned()
    }

    /// Hands the core an action, runs its effects and shows what changed.
    pub fn dispatch(&mut self, action: Action) -> Task<Message> {
        let effects = self.core.dispatch(action);
        let task = effects::run(self, effects);
        self.refresh();
        task
    }

    /// Shows the core's state in every area whose inputs changed.
    pub fn refresh(&mut self) {
        project::refresh(
            &self.core.state,
            &self.layout,
            &mut self.vm,
            &mut self.shown,
        );
    }

    /// Switches to a built-in theme by id, and keeps it. Custom themes come
    /// with S07.
    pub fn choose_theme(&mut self, id: &str) -> bool {
        let Some(chosen) = builtin::find(id) else {
            return false;
        };
        self.theme = Theme::new(chosen.id, chosen.name, chosen.palette);
        self.layout.set("theme", chosen.id);
        true
    }
}
