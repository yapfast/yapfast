//! The program: its state and its `update`, `view`, `subscription`, `theme`,
//! `title` and `scale_factor`, which `run` hands iced.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use iced::{Subscription, Task, window};
use yapfast_core::application::{Action, Application};
use yapfast_core::runtime::BackgroundRuntime;

use anomi_theme::{Motion, Theme, View, builtin, metrics};
use anomi_vm::Vm;
use anomi_vm::shell::Layer;
use anomi_widgets::focus;

use crate::backend::Connection;
use crate::layout::{self, Animate, Layout};
use crate::message::{Key, Message};
use crate::project::Reach;
use crate::zoom::{self, Zoom};
use crate::{backend, cli, control, effects, intent, keys, platform, project};

/// How long pictures that arrived wait for those that follow them closely,
/// to show together (`PICTURE_BATCH` in `src/ui/refresh.rs`).
const PICTURE_BATCH: Duration = Duration::from_millis(100);

/// The whole program's state. Fields drop in order: the backend's events
/// and the backend itself go before the runtime they run on.
pub struct App {
    pub(crate) backend: Connection,
    /// Linking Discord, while it runs on the runtime.
    pub(crate) linking: effects::link::Linking,
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
    pub(crate) zoom: Zoom,
    /// Which of the window's keys are on, and where typing anywhere goes.
    pub(crate) keys: keys::Keys,
    typing: Option<iced::advanced::widget::Id>,
    /// The thread beside the channel had its composer used last, rather
    /// than the channel (`aside-composer-last`; see `keys::typing`).
    pub(crate) aside_last: bool,
    /// Pictures that arrived and wait to show, while they do.
    pictures_due: Option<Reach>,
    /// Pictures the rows on screen show and the core hasn't got, to ask
    /// for (`crate::pictures::Requests`), and whether a wait for that runs.
    pub(crate) pictures: crate::pictures::Requests,
    pub(crate) pictures_asking: bool,
    /// The window's size in the view's pixels, which menus open within.
    pub(crate) size: iced::Size,
    /// Shift is held: a menu's Delete deletes at once.
    pub(crate) shift: bool,
    /// The tray, the single instance, a link waiting, the title.
    pub(crate) desktop: platform::Desktop,
    /// No window shows (started in the tray, or closed to it): nothing is
    /// projected until one does (`platform::forget`).
    pub(crate) hidden: bool,
}

impl App {
    pub fn new(runtime: Arc<BackgroundRuntime>, root: PathBuf, layout: Layout) -> Self {
        let theme = Theme::built_in(layout.theme());
        let mut vm = Vm::default();
        let shell = &mut vm.shell;
        if let Some(width) = layout.sidebar_width {
            shell.sidebar_width = width.clamp(metrics::SIDEBAR_MIN, metrics::SIDEBAR_MAX);
        }
        shell.members_width = layout.members_width;
        shell.aside_width = layout.aside_width;
        shell.call_height = layout.call_height;
        let mut core = Application::default();
        // Behind until the window says it's in front (`window::focused`), as `motion`
        // is: the core takes it for in front otherwise; going behind asks for nothing.
        drop(core.dispatch(Action::WindowFocused(false)));
        layout::carry::give(&layout, &mut core);
        let opening = crate::window::settings().size;
        let zoom = Zoom::new(layout.zoom, opening);
        let size = opening * (1.0 / zoom.level());
        // Until the display says its scale, as the window opens.
        anomi_widgets::picture::set_scale(zoom.level());
        let motion = Motion {
            never: layout.animate == Animate::Never,
            // In front once the window says so; until then, still.
            ..Motion::default()
        };
        Self {
            backend: Connection::default(),
            linking: effects::link::Linking::default(),
            runtime,
            root,
            core,
            vm,
            shown: project::Shown::default(),
            layout,
            theme,
            motion,
            window: None,
            zoom,
            keys: keys::Keys::default(),
            typing: None,
            aside_last: false,
            pictures_due: None,
            pictures: crate::pictures::Requests::default(),
            pictures_asking: false,
            size,
            shift: false,
            desktop: platform::Desktop::default(),
            hidden: false,
        }
    }

    /// Puts the icon in the tray, opens the window and starts the session,
    /// as the Slint app does: the preview, or the saved session. Started at
    /// sign-in with a session to restore and a tray to sit in, the window
    /// waits for the tray's Open.
    pub fn boot(mut self, start: &cli::Start) -> (Self, Task<Message>) {
        let (desktop, opened) = platform::start(&mut self, start);
        let first = if start.preview {
            Action::Preview
        } else {
            // With no saved session the core says so, and the sign-in
            // screen's task shows it.
            Action::Restore
        };
        let started = self.dispatch(first);
        let control = control::path().map_or_else(Task::none, control::watch);
        (self, Task::batch([desktop, opened, started, control]))
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = match message {
            Message::Intent(intent) => intent::handle(self, intent),
            Message::Backend { run, events } => backend::receive(self, run, events),
            Message::BackendClosed(run) => backend::closed(self, run),
            Message::PicturesDue => {
                if let Some(reach) = self.pictures_due.take() {
                    self.show(reach);
                }
                Task::none()
            }
            Message::LayoutDue(wait) => {
                self.layout.due(wait);
                Task::none()
            }
            Message::AskPictures => {
                self.pictures_asking = false;
                Task::none()
            }
            message @ (Message::Opened(_)
            | Message::Placed(..)
            | Message::Resized(_)
            | Message::Maximized(_)
            | Message::Desktop(_)
            | Message::Release
            | Message::CloseRequested(_)) => platform::update(self, message),
            Message::Focused(front) => crate::window::focused(self, front),
            Message::Shift(held) => {
                self.shift = held;
                Task::none()
            }
            Message::Rescaled(base) => zoom::rescaled(self, base),
            Message::Zoom(step) => zoom::step(self, step),
            Message::Zoomed(step, size) => zoom::zoomed(self, step, size),
            Message::ZoomNoteGone(note) => {
                zoom::note_gone(self, note);
                Task::none()
            }
            Message::Control(line) => control::run(self, &line),
            Message::Key(Key::Switcher) => keys::switcher(self),
            Message::Key(Key::Back) => keys::back(self),
            Message::Key(Key::Forward) => keys::forward(self),
            Message::Key(Key::Escape) => keys::escape(self),
            Message::Timeline(tick) => intent::chat::tick(self, tick),
            Message::Composer(done) => intent::chat::composer::done(self, done),
            Message::Pickers(due) => intent::chat::pickers::due(self, due),
            #[cfg(feature = "discord")]
            Message::Linking(run, step) => effects::link::progress(self, run, step),
        };
        // The timeline's list does what its projection asked, and loads
        // more at an edge it nears (I07).
        let timeline = intent::chat::after(self);
        let link = platform::links::open_waiting(self);
        let task = Task::batch([
            task,
            timeline,
            link,
            crate::pictures::ask(self),
            self.take_focus(),
        ]);
        // A change to the layout reaches the file once changes stop.
        match self.layout.wait() {
            Some(wait) => Task::batch([task, later(layout::WRITE_AFTER, Message::LayoutDue(wait))]),
            None => task,
        }
    }

    /// The field a projection asked to take the keys, as it showed: a
    /// prompt's code, the sign-in's first field.
    fn take_focus(&mut self) -> Task<Message> {
        self.shown
            .focus
            .take()
            .map_or_else(Task::none, anomi_widgets::field::focus)
    }

    /// The window, inside its keys: the zoom's first, then the focus
    /// scope's (`anomi_widgets::focus`), each only while it's on.
    pub fn view(&self, _window: window::Id) -> impl View<Message> + '_ {
        use iced::Widget as _;
        let window = anomi_screens::window(&self.vm, self.motion).map(Message::Intent);
        let (keys, key) = (self.keys, |on: bool, key| on.then_some(Message::Key(key)));
        let scope = focus::scope(window)
            .on_switcher(key(keys.switcher, Key::Switcher))
            .on_back(key(keys.navigate, Key::Back))
            .on_forward(key(keys.navigate, Key::Forward))
            .on_escape(Message::Key(Key::Escape))
            .search(focus::SEARCH)
            .browse(focus::TIMELINE)
            .type_into(self.typing.clone());
        zoom::keys(scope)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            crate::window::subscription(),
            intent::account::subscription(self),
            intent::sign_in::subscription(self),
        ])
    }

    pub fn theme(&self, _window: window::Id) -> Theme {
        self.theme.clone()
    }

    /// The product's name, with the mentions waiting (`platform::show`).
    pub fn title(&self, _window: window::Id) -> String {
        self.desktop.title.clone()
    }

    /// The zoom, which iced multiplies with the display's scale factor.
    pub fn scale_factor(&self, _window: window::Id) -> f32 {
        self.zoom.level()
    }

    /// Hands the core an action, runs its effects and shows what the action
    /// can have changed ([`Reach::after`]): a keystroke in the composer
    /// shows again the composer alone.
    pub fn dispatch(&mut self, action: Action) -> Task<Message> {
        let reach = Reach::after(&action);
        let effects = self.core.dispatch(action);
        let task = effects::run(self, effects);
        if reach != Reach::NOTHING {
            self.show(reach);
        }
        task
    }

    /// Shows the core's state in every area whose inputs changed.
    pub fn refresh(&mut self) {
        self.show(Reach::EVERYTHING);
    }

    /// Shows the core's state in the areas `reach` reaches, and keeps in the
    /// layout what the core changed of it. Without a window it keeps the
    /// layout, the tray and the title alone.
    pub fn show(&mut self, reach: Reach) {
        if !self.hidden {
            let (vm, shown, pictures) = (&mut self.vm, &mut self.shown, &mut self.pictures);
            project::refresh(
                &self.core,
                &self.root,
                &self.layout,
                vm,
                shown,
                pictures,
                reach,
            );
        }
        if reach.touches(Reach::STATE) {
            layout::carry::follow(&mut self.layout, &self.core);
            platform::show(self);
            if !self.hidden {
                self.show_keys();
            }
        }
    }

    /// Which of the window's keys are on, and where typing goes: the
    /// channel's composer when it shows and may be written in.
    fn show_keys(&mut self) {
        let state = &self.core.state;
        let covers = keys::Covers {
            settings: self.vm.shell.is_open(Layer::Settings),
            // The question before a masked link opens (I15 opens it).
            leaving_link: self.vm.shell.is_open(Layer::LeavingLink),
        };
        self.keys = keys::keys(state, covers);
        let composer = self.vm.conversation.body.composes()
            && state
                .selected
                .as_deref()
                .is_some_and(|room| state.can_write_in(room));
        self.typing = keys::typing(state, composer, self.aside_last);
    }

    /// Shows what a batch of backend events reached. Pictures alone wait a
    /// moment for those that follow them closely, hundreds as a picker
    /// opens, and show together (`Adapter::show`, `src/ui/refresh.rs:430`);
    /// with anything else they show at once, as every area showing pictures
    /// follows the state too.
    pub fn show_events(&mut self, reach: Reach) -> Task<Message> {
        if reach.touches(Reach::STATE) {
            self.show(reach);
            return Task::none();
        }
        let now = reach.without(Reach::ANY_PICTURE);
        if now != Reach::NOTHING {
            self.show(now);
        }
        let pictures = reach.without(now);
        if pictures == Reach::NOTHING {
            return Task::none();
        }
        match &mut self.pictures_due {
            Some(due) => {
                *due = *due | pictures;
                Task::none()
            }
            None => {
                self.pictures_due = Some(pictures);
                later(PICTURE_BATCH, Message::PicturesDue)
            }
        }
    }

    /// Writes what waits of the layout, and quits.
    pub fn quit(&mut self) -> Task<Message> {
        self.layout.flush();
        iced::exit()
    }

    /// Switches to a built-in theme by id, and keeps it. Custom themes come
    /// with S07.
    pub fn choose_theme(&mut self, id: &str) -> bool {
        let Some(chosen) = builtin::find(id) else {
            return false;
        };
        self.theme = Theme::new(chosen.id, chosen.name, chosen.palette);
        self.layout
            .edit(|saved| saved.theme = Some(chosen.id.to_owned()));
        true
    }
}

#[cfg(test)]
impl App {
    /// An app on the layout in `root`, without a window or a backend, for
    /// the tests of intents and projections.
    pub(crate) fn for_tests(root: &std::path::Path) -> Self {
        // No test reaches the speakers or the microphone.
        yapfast_core::silence_audio();
        let runtime = BackgroundRuntime::new().expect("the core's runtime");
        Self::new(Arc::new(runtime), root.to_owned(), Layout::read(root))
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.layout.flush();
    }
}

/// `message` after `delay`: a wait of the app's own, such as the layout's
/// or a note's. It wakes nothing until it ends; the timer starts as the
/// executor runs it.
pub fn later(delay: Duration, message: Message) -> Task<Message> {
    Task::perform(async move { tokio::time::sleep(delay).await }, move |()| {
        message
    })
}
