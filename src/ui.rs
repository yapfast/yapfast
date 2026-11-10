//! Slint presentation and event-loop adapter for the shared application.
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use slint::{
    ComponentHandle, Model, ModelRc, SharedString, VecModel, winit_030::WinitWindowAccessor,
};
use yapfast_core::{
    application::{
        Action, AddFriend, AppState, Application, BAN_HISTORY, ChannelAsk, ChannelInput,
        ClearAfter, Compose, Cosmetics, DmScreen, Effect, INVITE_AGES, INVITE_USES, ListedCategory,
        MenuChoice, Navigation, NewDm, PersonAsk, Place, ProfileText, ProfileView, Relation,
        RoleInput, Search, StickerUse, TIMEOUT_TIMES,
    },
    backend::Backend,
    chat::{
        ChannelKind, ChannelSettings, ChatMessage, Command, Delivery, EmbedField, Event, Picture,
        Placement, Presence, ProfilePicture, ProfileRole, Reacts, Role, VideoCommand,
    },
    clock, discord, links, mention,
    meter::{self, InputMeter},
    network::Network,
    runtime::BackgroundRuntime,
    sounds::{self, Sound, SoundSet},
    spoiler,
    voice::{self, Phase},
};

use crate::drop::Drag;
use crate::{autostart, custom_emoji, emoji, emoji_text, graphics, zoom};

mod appearance;
mod away;
mod browser;
mod call_link;
mod call_timer;
mod callbacks;
mod channel_settings;
mod emoji_animation;
mod emoji_grid;
mod games;
mod gifs;
mod menus;
mod messages;
#[cfg(windows)]
mod move_loop;
mod name_style;
mod new_tags;
mod pictures;
mod placement;
mod polls;
mod profile;
mod refresh;
mod server_settings;
mod shell;
mod stickers;
mod theme;
mod tiles;
mod title_bar;
mod updates;
mod yapfast;

use callbacks::bind_callbacks;
use messages::*;
pub(crate) use pictures::picture as slint_image;
use pictures::*;
use profile::*;
use refresh::{AccountCardShown, Reach, RosterShown, SidebarShown, SwitcherShown};
use tiles::{Focus, Pip, Stage, Tiles};
use yapfast_ui::*;

/// How the desktop window ended.
pub enum Exit {
    Closed,
    /// GPU rendering failed; restart with Skia Software for this reason.
    UseSoftware(String),
    /// The renderer setting changed; restart to use it.
    Restart,
    /// An update was downloaded and checked; finish it and start the new program.
    Update(crate::update::install::Prepared),
}

/// Slint releases the first window after at most 500 ms of desktop appearance
/// queries; this leaves time for the window, its GPU surface and first frames.
/// Skia disables partial repainting while a rendering notifier is installed,
/// so startup is confirmed by the native window rather than by a frame.
const GPU_START_CONFIRMATION: Duration = Duration::from_secs(3);

pub fn run(
    runtime: BackgroundRuntime,
    root: PathBuf,
    preview: bool,
    renderer: Option<graphics::Renderer>,
    gpu_failure: Option<String>,
    background: bool,
    open_link: Option<String>,
) -> Result<Exit> {
    let (plan, placement, system_title_bar, mut instance) = {
        let mut layout = Layout::read(&root);
        let instance = if preview {
            crate::instance::Instance::default()
        } else {
            match crate::instance::claim(
                &root,
                !layout.multiple_windows,
                background,
                open_link.clone(),
            ) {
                crate::instance::Start::Run(instance) => instance,
                crate::instance::Start::Handed => return Ok(Exit::Closed),
            }
        };
        let plan = graphics::plan(renderer, layout.gpu_rendering, gpu_failure);
        if plan.forget_gpu {
            layout.gpu_rendering = false;
            layout.write(&root);
        }
        title_bar::choose(layout.system_title_bar);
        (plan, layout.window, layout.system_title_bar, instance)
    };
    decode_into_slint_buffers();
    pictures::make_frames_in_slint_buffers();
    // Request the graphics API explicitly so Slint cannot silently replace a
    // failed GPU surface; Yapfast owns the fallback and tells the user about it.
    // Windows draws the GPU setting with Direct3D 12; see [`graphics`].
    let selector = slint::BackendSelector::new().backend_name("winit".into());
    let (selector, renderer) = match plan.renderer {
        graphics::Renderer::Gpu if cfg!(windows) => (
            selector.renderer_name("skia".into()).require_d3d(),
            "skia-direct3d",
        ),
        graphics::Renderer::Gpu => (selector.renderer_name("skia-opengl".into()), "skia-opengl"),
        graphics::Renderer::Software => (
            selector.renderer_name("skia-software".into()),
            "skia-software",
        ),
    };
    selector
        .with_winit_window_attributes_hook(|attributes| {
            crate::instance::attributes(title_bar::attributes(crate::icon::attributes(attributes)))
        })
        .select()
        .context("could not select the Slint desktop backend")?;
    slint::set_xdg_app_id(crate::icon::APP_ID).context("could not set Yapfast's app ID")?;
    let gpu = plan.renderer == graphics::Renderer::Gpu;
    let failed = |error: slint::PlatformError, context: &'static str| -> Result<Exit> {
        let reason = error.to_string();
        if gpu {
            tracing::error!(%reason, "GPU rendering failed; restarting with Skia Software");
            Ok(Exit::UseSoftware(reason))
        } else {
            Err(anyhow::Error::new(error).context(context))
        }
    };
    // Started at sign-in, Yapfast goes to the tray with the saved session.
    // Without one there's nothing to do there but sign in, which needs the window.
    let hidden =
        background && open_link.is_none() && !preview && yapfast_core::chat::can_restore(&root);
    // Before the window maps, as the desktop looks up its icon then.
    #[cfg(target_os = "linux")]
    if !preview {
        crate::desktop_entry::install();
    }
    let window = match yapfast::window() {
        Ok(window) => window,
        Err(error) => return failed(error, "could not create the Yapfast window"),
    };
    title_bar::show(&window, system_title_bar);
    if let Some(placement) = &placement {
        placement::place(&window, placement);
    }
    if !hidden && let Err(error) = window.show() {
        return failed(error, "could not show Yapfast");
    }
    if !preview {
        AUTOSTART.set(autostart::SUPPORTED);
        PROTOCOL.set(cfg!(feature = "discord") && crate::protocol::SUPPORTED);
        if PROTOCOL.get() {
            crate::protocol::refresh(&root);
        }
        autostart::refresh();
    }
    prepare(&window);
    let adapter = Adapter::new(Arc::new(runtime), root, &window)?;
    bind_callbacks(&window, &adapter);
    updates::start(&adapter, &window);
    new_tags::start(&adapter);
    watch_window(&window, &adapter);
    adapter.borrow_mut().watch_away();
    adapter.borrow_mut().watch_talk_key(&window);
    adapter.borrow().show_protocol(&window);
    if !preview {
        adapter.borrow_mut().pending_link = open_link;
    }
    adapter.borrow_mut().refresh(&window);
    if preview {
        adapter.borrow_mut().action(&window, Action::Preview);
    } else if yapfast_core::chat::can_restore(&adapter.borrow().root) {
        adapter.borrow_mut().action(&window, Action::Restore);
    } else {
        window.invoke_focus_sign_in();
    }
    let notice = SharedString::from(plan.notice.unwrap_or_default());
    window.set_graphics_notice(notice.clone());
    window.set_gpu_running(gpu);
    window.global::<Popup>().set_fade(gpu);
    // Slint 1.18 keeps its event loop running when it creates a window late and
    // the GPU surface fails, leaving no window. Check for the native window.
    let missing_window = Rc::new(Cell::new(false));
    if gpu && !hidden {
        let missing_window = missing_window.clone();
        let weak = window.as_weak();
        // A window already closed to the tray had appeared.
        slint::Timer::single_shot(GPU_START_CONFIRMATION, move || {
            if weak
                .upgrade()
                .is_some_and(|window| !window.window().has_winit_window())
            {
                missing_window.set(true);
                let _ = slint::quit_event_loop();
            }
        });
    }
    let shell = shell::Shell::new(window, adapter.clone(), notice, gpu);
    if hidden {
        shell.start_in_tray();
    }
    instance.listen(|launch| {
        let _ = slint::invoke_from_event_loop(move || shell::launched(launch));
    });
    tracing::info!(renderer, hidden, "starting the Slint desktop window");
    let _control = test_control().map(|path| control(path, shell.clone()));
    // Until quit: closing the window to the tray doesn't end Yapfast.
    let result = slint::run_event_loop_until_quit();
    let restart = shell.restarting();
    let late_gpu_failure = shell.gpu_failure();
    drop(shell);
    let mut adapter = adapter.borrow_mut();
    let updated = adapter.updates.prepared.take();
    placement::flush(&adapter);
    // The icon goes before Yapfast does, not when Windows next looks at it.
    adapter.tray = None;
    adapter.stop_backend();
    // A window made late, as from the tray after starting in it, failed on the GPU.
    if result.is_ok()
        && let Some(reason) = late_gpu_failure
    {
        tracing::error!(%reason, "GPU rendering failed; restarting with Skia Software");
        return Ok(Exit::UseSoftware(reason));
    }
    match result {
        Err(error) => failed(error, "Slint's desktop event loop failed"),
        Ok(()) if missing_window.get() => failed(
            "the window did not appear".into(),
            "Yapfast's window did not appear",
        ),
        Ok(()) => Ok(match updated {
            Some(prepared) => Exit::Update(prepared),
            None if restart => Exit::Restart,
            None => Exit::Closed,
        }),
    }
}

/// Only the private test compositor can request a different initial size.
fn isolated_display() -> bool {
    std::env::var("YAPFAST_FRAMEWORK_ISOLATED_DISPLAY").as_deref() == Ok("1")
}

fn prepare(window: &YapfastWindow) {
    // The entry is the setting, so each window reads it as it is.
    let autostart = AUTOSTART.get();
    window.set_autostart_available(autostart);
    window.set_start_on_sign_in(autostart && autostart::enabled());
    // Without Matrix everything is Discord's, so its badge would say nothing.
    window
        .global::<Badges>()
        .set_discord(cfg!(feature = "matrix"));
    if isolated_display()
        && let Ok(size) = std::env::var("YAPFAST_UI_TEST_SIZE")
        && let Some((width, height)) = size.split_once('x')
        && let (Ok(width), Ok(height)) = (width.parse::<u32>(), height.parse::<u32>())
        && width >= 800
        && height >= 560
    {
        window
            .window()
            .set_size(slint::PhysicalSize::new(width, height));
    }
    window.set_emoji_font(emoji::family().into());
    emoji_text::register();
    window
        .global::<Twemoji>()
        .on_text(|text| emoji_text::text(&text).into());
    copy_whole_messages(window);
    mark_autoscrolls(window);
    type_anywhere(window);
}

/// The most backend events applied together before the window refreshes, so
/// a long burst still lets the window keep up.
const EVENT_BATCH: usize = 256;

/// How long the zoom level shows after it changes.
const ZOOM_NOTE: Duration = Duration::from_millis(1200);

/// What the mouse's back and forward buttons do, as Alt+Left and Alt+Right
/// do.
fn side_button_action(button: slint::winit_030::winit::event::MouseButton) -> Option<Action> {
    use slint::winit_030::winit::event::MouseButton;
    match button {
        MouseButton::Back => Some(Action::NavigateBack),
        MouseButton::Forward => Some(Action::NavigateForward),
        _ => None,
    }
}

/// Carries out `action` once the event that asked for it is done.
fn navigate(
    adapter: &std::rc::Weak<RefCell<Adapter>>,
    window: &slint::Weak<YapfastWindow>,
    action: Action,
) {
    let (adapter, window) = (adapter.clone(), window.clone());
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let (Some(adapter), Some(window)) = (adapter.upgrade(), window.upgrade()) {
            adapter.borrow_mut().action(&window, action);
        }
    });
}

/// Follows the native window's focus, for notifications, zooms the window
/// with Ctrl and +, - and 0 or the scroll wheel, keeping the level in the
/// layout, opens the quick switcher with Ctrl+K, and takes files dropped on
/// it.
fn watch_window(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    use slint::winit_030::{
        EventResult,
        winit::{
            event::{ElementState, KeyEvent, MouseButton, WindowEvent},
            keyboard::{Key, KeyCode, NamedKey, PhysicalKey},
        },
    };
    /// The K key: the letter K in the keyboard's layout, or in a layout
    /// without Latin letters, the key in its place.
    fn is_k(key: &KeyEvent) -> bool {
        match &key.logical_key {
            Key::Character(text) if text.chars().all(|letter| letter.is_ascii_alphabetic()) => {
                text.eq_ignore_ascii_case("k")
            }
            _ => key.physical_key == PhysicalKey::Code(KeyCode::KeyK),
        }
    }
    let saved = zoom::saved(adapter.borrow().layout.zoom);
    let adapter = Rc::downgrade(adapter);
    let (to, weak) = (adapter.clone(), window.as_weak());
    DRAGGED.set(Some(Box::new(move |drag| {
        if let (Some(adapter), Some(window)) = (to.upgrade(), weak.upgrade()) {
            adapter.borrow_mut().drag(&window, drag);
        }
    })));
    // The winit window exists once the event loop runs.
    let weak = window.as_weak();
    slint::Timer::single_shot(Duration::ZERO, move || {
        let Some(window) = weak.upgrade() else { return };
        // A window that opens focused gets no event saying so, and animation
        // would wait for one; ask it instead.
        let focused = {
            use slint::winit_030::WinitWindowAccessor as _;
            window
                .window()
                .with_winit_window(|winit| winit.has_focus())
                .unwrap_or(false)
        };
        FOCUSED.set(focused);
        show_motion(&window);
        // The display's own scale factor, which the zoom multiplies.
        let base = Rc::new(Cell::new(window.window().scale_factor()));
        let level = Rc::new(Cell::new(
            saved
                .min(zoom::most(window.window().size(), base.get()))
                .max(zoom::LEVELS[0]),
        ));
        zoom::apply(window.window(), base.get(), level.get());
        let note = Rc::new(slint::Timer::default());
        let mut keys = zoom::Keys::default();
        let mut cursor = None;
        // The winit window exists by the first event it sends.
        #[cfg(target_os = "linux")]
        let mut watching = false;
        // Files dropped together arrive one at a time, then go as one drop.
        let dropped = Rc::new(RefCell::new(Vec::new()));
        let mut hovering = false;
        // winit doesn't say where files hover, so the pointer is asked.
        let pointer: Rc<RefCell<Option<crate::drop::Pointer>>> = Rc::default();
        let following = Rc::new(slint::Timer::default());
        let mut made = false;
        let placing = Rc::new(Cell::new(false));
        let weak = window.as_weak();
        window
            .window()
            .on_winit_window_event(move |slint_window, event| {
                // Slint gives the window its display's scale as winit makes it,
                // which can come after the zoom above, so the zoom goes back on
                // top with the window's first event. On Wayland a scale other
                // than 1 also comes as ScaleFactorChanged, below; 1 never does.
                if !made {
                    made = true;
                    let (weak, base, level) = (weak.clone(), base.clone(), level.clone());
                    slint::Timer::single_shot(Duration::ZERO, move || {
                        let Some(window) = weak.upgrade() else { return };
                        if std::env::var("SLINT_SCALE_FACTOR").is_err()
                            && let Some(scale) = window
                                .window()
                                .with_winit_window(|winit| winit.scale_factor() as f32)
                        {
                            base.set(scale);
                        }
                        zoom::apply(window.window(), base.get(), level.get());
                    });
                }
                #[cfg(target_os = "linux")]
                if !watching {
                    watching = true;
                    let weak = weak.clone();
                    slint::Timer::single_shot(Duration::ZERO, move || {
                        if let Some(window) = weak.upgrade() {
                            watch_wayland_drops(&window);
                        }
                    });
                }
                // winit takes drops on X11 and Windows; see watch_wayland_drops.
                match event {
                    WindowEvent::HoveredFile(_) if !hovering => {
                        hovering = true;
                        let found = slint_window
                            .with_winit_window(crate::drop::Pointer::of)
                            .flatten();
                        if let Some(moved) = found.as_ref().and_then(crate::drop::Pointer::at) {
                            dragged(moved);
                        }
                        if found.is_some() {
                            *pointer.borrow_mut() = found;
                            let pointer = pointer.clone();
                            following.start(
                                slint::TimerMode::Repeated,
                                Duration::from_millis(40),
                                move || {
                                    let moved = pointer.borrow().as_ref().and_then(|p| p.at());
                                    if let Some(moved) = moved {
                                        dragged(moved);
                                    }
                                },
                            );
                        }
                        dragged(Drag::Entered);
                    }
                    WindowEvent::HoveredFileCancelled => {
                        hovering = false;
                        following.stop();
                        pointer.take();
                        dragged(Drag::Left);
                    }
                    WindowEvent::DroppedFile(path) => {
                        hovering = false;
                        following.stop();
                        // Where it was let go.
                        if let Some(moved) = pointer.take().as_ref().and_then(|p| p.at()) {
                            dragged(moved);
                        }
                        if dropped.borrow().is_empty() {
                            let dropped = dropped.clone();
                            slint::Timer::single_shot(Duration::ZERO, move || {
                                dragged(Drag::Dropped(dropped.take()));
                            });
                        }
                        dropped.borrow_mut().push(path.clone());
                    }
                    _ => {}
                }
                if away::is_use(event)
                    && let (Some(adapter), Some(window)) = (adapter.upgrade(), weak.upgrade())
                    && let Ok(mut adapter) = adapter.try_borrow_mut()
                {
                    adapter.used(&window);
                }
                if let WindowEvent::Focused(focused) = event {
                    FOCUSED.set(*focused);
                    if let Some(window) = weak.upgrade() {
                        window.set_window_focused(*focused);
                        show_motion(&window);
                        if *focused {
                            attention(&window, false);
                        }
                        if let Some(adapter) = adapter.upgrade()
                            && let Ok(mut adapter) = adapter.try_borrow_mut()
                        {
                            adapter.action(&window, Action::WindowFocused(*focused));
                        }
                    }
                }
                if matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
                    #[cfg(windows)]
                    move_loop::moved(&weak);
                    // Once for the events a move or resize sends together.
                    if !placing.replace(true) {
                        let (weak, adapter, placing) =
                            (weak.clone(), adapter.clone(), placing.clone());
                        slint::Timer::single_shot(Duration::ZERO, move || {
                            placing.set(false);
                            if let (Some(adapter), Some(window)) =
                                (adapter.upgrade(), weak.upgrade())
                            {
                                window.set_window_maximized(window.window().is_maximized());
                                placement::moved(&mut adapter.borrow_mut(), &window);
                            }
                        });
                    }
                }
                // Moving to a display of another density: Slint takes its scale,
                // and then the zoom goes back on top.
                if let WindowEvent::ScaleFactorChanged { scale_factor, .. } = event
                    && std::env::var("SLINT_SCALE_FACTOR").is_err()
                {
                    base.set(*scale_factor as f32);
                    let (weak, base, level) = (weak.clone(), base.clone(), level.clone());
                    slint::Timer::single_shot(Duration::ZERO, move || {
                        if let Some(window) = weak.upgrade() {
                            zoom::apply(window.window(), base.get(), level.get());
                        }
                    });
                }
                crate::push_to_talk::window_event(event);
                // Windows' two side buttons are recordable shortcuts too.
                #[cfg(windows)]
                if let Some(window) = weak.upgrade()
                    && window.get_talk_recording()
                    && let Some(recorded) = crate::push_to_talk::Keybind::recorded_mouse(event)
                {
                    window.set_talk_recording(false);
                    let (adapter, weak) = (adapter.clone(), weak.clone());
                    slint::Timer::single_shot(Duration::ZERO, move || {
                        if let (Some(adapter), Some(window)) = (adapter.upgrade(), weak.upgrade()) {
                            adapter.borrow_mut().set_talk_key(&window, recorded);
                        }
                    });
                    return EventResult::PreventDefault;
                }
                // While recording the push-to-talk key, the next key becomes
                // it, and Escape stops; either way only the recorder sees it.
                if let WindowEvent::KeyboardInput { event: key, .. } = event
                    && key.state == ElementState::Pressed
                    && let Some(window) = weak.upgrade()
                    && window.get_talk_recording()
                {
                    if !key.repeat {
                        window.set_talk_recording(false);
                        let recorded = (key.logical_key != Key::Named(NamedKey::Escape))
                            .then(|| crate::push_to_talk::Keybind::recorded(key))
                            .flatten();
                        if let Some(recorded) = recorded {
                            let (adapter, weak) = (adapter.clone(), weak.clone());
                            slint::Timer::single_shot(Duration::ZERO, move || {
                                if let (Some(adapter), Some(window)) =
                                    (adapter.upgrade(), weak.upgrade())
                                {
                                    adapter.borrow_mut().set_talk_key(&window, recorded);
                                }
                            });
                        }
                    }
                    return EventResult::PreventDefault;
                }
                // Escape lets go of someone dragged from a voice channel,
                // wherever the focus is, and does nothing else; see RoomList.
                if let WindowEvent::KeyboardInput { event: key, .. } = event
                    && key.state == ElementState::Pressed
                    && key.logical_key == Key::Named(NamedKey::Escape)
                    && let Some(window) = weak.upgrade()
                    && window.get_person_moving()
                {
                    window.set_person_moving(false);
                    let (adapter, weak) = (adapter.clone(), weak.clone());
                    slint::Timer::single_shot(Duration::ZERO, move || {
                        if let (Some(adapter), Some(window)) = (adapter.upgrade(), weak.upgrade()) {
                            adapter.borrow_mut().stop_moving_person(&window);
                        }
                    });
                    return EventResult::PreventDefault;
                }
                // Someone dragged whose row went away, as when the list
                // scrolled or their channel's people changed, can be let go
                // where no row hears the button go up. Once Slint has taken
                // the release, which drops them if it can, nothing is dragged.
                if let WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } = event
                    && let Some(window) = weak.upgrade()
                    && window.get_person_moving()
                {
                    let (adapter, weak) = (adapter.clone(), weak.clone());
                    slint::Timer::single_shot(Duration::ZERO, move || {
                        if let (Some(adapter), Some(window)) = (adapter.upgrade(), weak.upgrade()) {
                            adapter.borrow_mut().stop_moving_person(&window);
                        }
                    });
                }
                // Ctrl+K opens the quick switcher, or closes it, wherever the
                // focus is, as in Discord; not over the settings or a link
                // that asks before it opens, which the window keeps. Those
                // the application keeps, it checks itself.
                if let WindowEvent::KeyboardInput { event: key, .. } = event
                    && key.state == ElementState::Pressed
                    && keys.control_only()
                    && is_k(key)
                    && let Some(window) = weak.upgrade()
                    && window.get_workspace()
                    && !window.get_settings_open()
                    && window.get_leaving_link().is_empty()
                {
                    if !key.repeat {
                        let (adapter, weak) = (adapter.clone(), weak.clone());
                        slint::Timer::single_shot(Duration::ZERO, move || {
                            if let (Some(adapter), Some(window)) =
                                (adapter.upgrade(), weak.upgrade())
                            {
                                adapter.borrow_mut().action(&window, Action::ToggleSwitcher);
                            }
                        });
                    }
                    return EventResult::PreventDefault;
                }
                let outcome = keys.event(event);
                // Shift turns a message's hover bar into its quick actions,
                // as in Discord's client, wherever the focus is.
                match event {
                    WindowEvent::CursorMoved { position, .. } => cursor = Some(*position),
                    WindowEvent::CursorLeft { .. } => cursor = None,
                    _ => {}
                }
                if let Some(window) = weak.upgrade()
                    && window.get_shift_held() != keys.shift()
                {
                    window.set_shift_held(keys.shift());
                    // The buttons change under a pointer that stays put, and
                    // Slint looks again only when it moves; ask it to now, so
                    // the tooltip and highlight belong to what a click does.
                    // Not while the keyboard walks the messages, whose choice
                    // a row newly under the pointer would take.
                    if let Some(position) = cursor
                        && !window.get_browsing()
                    {
                        let position =
                            position.to_logical::<f32>(f64::from(slint_window.scale_factor()));
                        let position = slint::LogicalPosition::new(position.x, position.y);
                        let weak = weak.clone();
                        slint::Timer::single_shot(Duration::ZERO, move || {
                            if let Some(window) = weak.upgrade() {
                                window.window().dispatch_event(
                                    slint::platform::WindowEvent::PointerMoved { position },
                                );
                            }
                        });
                    }
                }
                // Alt+Left and Alt+Right go back and forward through the
                // conversations visited, as in Discord.
                if let WindowEvent::KeyboardInput { event: key, .. } = event
                    && key.state == ElementState::Pressed
                    && keys.alt_only()
                    && let Key::Named(named @ (NamedKey::ArrowLeft | NamedKey::ArrowRight)) =
                        key.logical_key
                    && let Some(window) = weak.upgrade()
                    && window.get_workspace()
                    && !window.get_settings_open()
                {
                    if !key.repeat {
                        navigate(
                            &adapter,
                            &weak,
                            if named == NamedKey::ArrowLeft {
                                Action::NavigateBack
                            } else {
                                Action::NavigateForward
                            },
                        );
                    }
                    return EventResult::PreventDefault;
                }
                // So do the mouse's back and forward buttons. The release is
                // kept from the window too, so that it isn't taken for a click.
                if let WindowEvent::MouseInput { state, button, .. } = event
                    && let Some(action) = side_button_action(*button)
                    && let Some(window) = weak.upgrade()
                    && window.get_workspace()
                    && !window.get_settings_open()
                {
                    if *state == ElementState::Pressed {
                        navigate(&adapter, &weak, action);
                    }
                    return EventResult::PreventDefault;
                }
                let step = match outcome {
                    zoom::Outcome::Pass => return EventResult::Propagate,
                    zoom::Outcome::Taken => return EventResult::PreventDefault,
                    zoom::Outcome::Step(step) => step,
                };
                let most = zoom::most(slint_window.size(), base.get());
                let next = zoom::stepped(level.get(), step, most);
                if let Some(window) = weak.upgrade() {
                    window.set_zoom_note(format!("{}%", (next * 100.).round()).into());
                    let weak = weak.clone();
                    note.start(slint::TimerMode::SingleShot, ZOOM_NOTE, move || {
                        if let Some(window) = weak.upgrade() {
                            window.set_zoom_note(SharedString::default());
                        }
                    });
                }
                if next != level.get() {
                    level.set(next);
                    zoom::apply(slint_window, base.get(), next);
                    if let Some(adapter) = adapter.upgrade() {
                        let mut adapter = adapter.borrow_mut();
                        adapter.layout.zoom = Some(next);
                        adapter.layout.write(&adapter.root);
                    }
                }
                EventResult::PreventDefault
            });
    });
}

/// Files dragged over the window, which the current window takes.
fn dragged(drag: Drag) {
    DRAGGED.with_borrow(|dragged| {
        if let Some(dragged) = dragged {
            dragged(drag);
        }
    });
}

/// On Wayland, where winit takes no drops, a data device on winit's own
/// connection does, once for every window, as it's the seat's.
#[cfg(target_os = "linux")]
fn watch_wayland_drops(window: &YapfastWindow) {
    use slint::winit_030::WinitWindowAccessor;
    use slint::winit_030::winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
    thread_local! {
        static WATCHING: Cell<bool> = const { Cell::new(false) };
    }
    if WATCHING.get() {
        return;
    }
    let found = window.window().with_winit_window(|window| {
        match window.display_handle().map(|handle| handle.as_raw()) {
            Ok(RawDisplayHandle::Wayland(handle)) => Some(handle.display),
            _ => None,
        }
    });
    let Some(Some(display)) = found else {
        return;
    };
    WATCHING.set(true);
    // SAFETY: winit's display stays connected while its event loop runs,
    // which is as long as Yapfast does.
    unsafe {
        crate::drop::wayland::watch(display, |drag| {
            let _ = slint::invoke_from_event_loop(move || dragged(drag));
        });
    }
}

thread_local! {
    /// Takes files dragged over the window; see [`dragged`].
    static DRAGGED: RefCell<Option<TakeDrag>> = const { RefCell::new(None) };
    /// Whether Yapfast's window has the keyboard focus, so notifications can wait.
    static FOCUSED: Cell<bool> = const { Cell::new(false) };
    /// Settings offer starting at sign-in: the platform can, and this isn't the preview.
    static AUTOSTART: Cell<bool> = const { Cell::new(false) };
    /// Settings offer claiming Discord's protocol links: the platform
    /// can, this build speaks Discord, and this isn't the preview.
    static PROTOCOL: Cell<bool> = const { Cell::new(false) };
    /// When animations play, as the settings say.
    static ANIMATE: Cell<Animate> = const { Cell::new(Animate::Focused) };
    /// Opens a conversation from a clicked notification, in the current window.
    static OPEN: RefCell<Option<OpenRoom>> = const { RefCell::new(None) };
}

type OpenRoom = Box<dyn Fn(String)>;
type TakeDrag = Box<dyn Fn(Drag)>;

fn open_from_notification(room: String) {
    OPEN.with(|open| {
        if let Some(open) = open.borrow().as_ref() {
            open(room);
        }
    });
}

/// Sends keys nothing took, such as after a menu closes, to the composer, as Discord does.
fn type_anywhere(window: &YapfastWindow) {
    let window = window.as_weak();
    i_slint_core::yapfast_typing::on_unclaimed(move |text| {
        let Some(window) = window.upgrade() else {
            return false;
        };
        if text.chars().eq([char::from(slint::platform::Key::Escape)]) {
            return window.invoke_escape_composing();
        }
        // Stepping through the messages, only letters and digits leave them.
        if window.get_browsing() && !text.chars().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }
        // Holding the key to talk shouldn't also write it.
        if window.get_voice_active()
            && !window.get_muted()
            && crate::push_to_talk::pressed_talk_key()
        {
            return false;
        }
        window.invoke_type_into_composer()
    });
}

/// Lets a selection of text copy the messages it covers whole, including those scrolled out of
/// view, which the list no longer shows.
fn copy_whole_messages(window: &YapfastWindow) {
    let window = window.as_weak();
    i_slint_core::yapfast_selection::set_row_text(move |row| {
        Some(message_text(
            &window.upgrade()?.get_messages().row_data(row)?,
        ))
    });
}

/// Marks where a middle click started scrolling, until the scroll ends.
fn mark_autoscrolls(window: &YapfastWindow) {
    let window = window.as_weak();
    i_slint_core::yapfast_autoscroll::on_marker(move |scrolled, marker| {
        let Some(window) = window.upgrade() else {
            return;
        };
        if !std::ptr::eq(scrolled, window.window()) {
            return;
        }
        let shown = window.global::<Autoscroll>();
        if let Some(marker) = marker {
            shown.set_x(marker.position.x);
            shown.set_y(marker.position.y);
            shown.set_vertical(marker.vertical);
            shown.set_horizontal(marker.horizontal);
        }
        shown.set_shown(marker.is_some());
    });
}

/// A message as a selection covering it copies it, reading as its row shows it: texts beside
/// each other joined by a space, others by line breaks, and each picture as `[Picture]`.
fn message_text(row: &MessageRow) -> String {
    let plain = i_slint_core::yapfast_selection::plain;
    let mut lines = Vec::new();
    if row.first {
        lines.push(format!(
            "{} {}",
            plain(&row.author_styled),
            plain(&row.time_styled)
        ));
    }
    // Text in blocks reads as each block's.
    let text = |styled: &slint::StyledText, blocks: &ModelRc<TextBlock>| {
        if blocks.row_count() == 0 {
            return plain(styled);
        }
        let blocks: Vec<String> = blocks.iter().map(|block| plain(&block.text)).collect();
        blocks.join("\n")
    };
    if !row.body.is_empty() {
        lines.push(text(&row.styled, &row.blocks));
    }
    let pictures = |count: usize| std::iter::repeat_n("[Picture]".to_string(), count);
    lines.extend(pictures(row.images.row_count()));
    for embed in row.embeds.iter() {
        let mut card = Vec::new();
        let texts = [
            (embed.has_provider, &embed.provider),
            (embed.has_author, &embed.author),
            (embed.has_title, &embed.title),
        ];
        card.extend(
            texts
                .iter()
                .filter(|(shown, _)| *shown)
                .map(|(_, text)| plain(text)),
        );
        if embed.has_description {
            card.push(text(&embed.description, &embed.description_blocks));
        }
        for fields in embed.field_rows.iter() {
            let (names, values): (Vec<_>, Vec<_>) = fields
                .fields
                .iter()
                .map(|field| (plain(&field.name), text(&field.value, &field.value_blocks)))
                .unzip();
            card.push(names.join(" "));
            card.push(values.join(" "));
        }
        card.extend(pictures(embed.images.row_count()));
        if embed.has_footer {
            card.push(plain(&embed.footer));
        }
        // The thumbnail sits level with the card's first text, to its right.
        if embed.has_thumbnail {
            card.insert(card.len().min(1), "[Picture]".into());
        }
        lines.extend(card);
    }
    for card in row.cards.iter() {
        for each in card.rows.iter() {
            lines.extend(each.blocks.iter().map(|block| plain(&block.text)));
            if each.has_thumbnail {
                lines.push("[Picture]".into());
            }
            lines.extend(pictures(each.images.row_count()));
            lines.extend(each.files.iter().map(|file| file.name.to_string()));
            let buttons = each
                .buttons
                .iter()
                .chain(each.has_button.then_some(each.button.clone()));
            let labels: Vec<_> = buttons.map(|button| button.label.to_string()).collect();
            if !labels.is_empty() {
                lines.push(labels.join(" "));
            }
            if !each.select.is_empty() {
                lines.push(each.select.to_string());
            }
        }
    }
    lines.join("\n")
}

/// A command file through which the private test compositor can hide, destroy
/// and recreate the window while Yapfast keeps running, close it as its close
/// button does and open it as the tray does, and add a file to the next
/// message with `stage PATH`, as a paste would: the private display has no
/// clipboard, file chooser or window decorations to do it with.
fn test_control() -> Option<PathBuf> {
    isolated_display()
        .then(|| std::env::var_os("YAPFAST_UI_TEST_CONTROL"))
        .flatten()
        .map(PathBuf::from)
}

/// Run each new line of the command file.
fn control(path: PathBuf, shell: Rc<shell::Shell>) -> slint::Timer {
    let handled = Cell::new(0);
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(500),
        move || {
            let Ok(text) = std::fs::read_to_string(&path) else {
                return;
            };
            for command in text.lines().skip(handled.get()) {
                handled.set(handled.get() + 1);
                tracing::info!(command, "test window control");
                let window = shell.window();
                let adapter = &shell.adapter;
                let result = match command {
                    "hide" => window.map_or(Ok(()), |window| window.hide()),
                    "show" => window.map_or(Ok(()), |window| window.show()),
                    // Drops the window without giving the memory back, which
                    // `trim` then does.
                    "destroy" => {
                        drop(window);
                        shell.drop_window();
                        Ok(())
                    }
                    // As the window's close button: to the tray, or quit.
                    "close" => {
                        drop(window);
                        shell.close();
                        Ok(())
                    }
                    // As the tray's Open Yapfast.
                    "create" | "open" => {
                        drop(window);
                        shell.open();
                        Ok(())
                    }
                    "trim" => {
                        trim();
                        Ok(())
                    }
                    "stats" => {
                        allocator_stats();
                        Ok(())
                    }
                    // `theme ID` chooses a theme, built in or custom, as Appearance does.
                    command if command.starts_with("theme ") => {
                        let id = &command["theme ".len()..];
                        match window {
                            Some(window) if !adapter.borrow_mut().choose_theme(&window, id) => {
                                Err("no such theme".into())
                            }
                            _ => Ok(()),
                        }
                    }
                    "quit" => slint::quit_event_loop().map_err(|error| error.to_string().into()),
                    // `pick avatar PATH` or `pick banner PATH` chooses a
                    // picture in the server profile editor, `pick icon PATH`
                    // a server's icon in its settings, and `pick emoji` and
                    // `pick sticker` a new emoji's or sticker's.
                    command if command.starts_with("pick ") => {
                        let (kind, path) =
                            command["pick ".len()..].split_once(' ').unwrap_or_default();
                        let path = PathBuf::from(path);
                        let action = match kind {
                            "icon" => Action::PickServerPicture {
                                kind: yapfast_core::chat::ServerPicture::Icon,
                                path,
                            },
                            "server-banner" => Action::PickServerPicture {
                                kind: yapfast_core::chat::ServerPicture::Banner,
                                path,
                            },
                            "splash" => Action::PickServerPicture {
                                kind: yapfast_core::chat::ServerPicture::Splash,
                                path,
                            },
                            "role-icon" => Action::PickServerPicture {
                                kind: yapfast_core::chat::ServerPicture::RoleIcon,
                                path,
                            },
                            "emoji" => Action::PickServerPicture {
                                kind: yapfast_core::chat::ServerPicture::Emoji,
                                path,
                            },
                            "sticker" => Action::PickServerPicture {
                                kind: yapfast_core::chat::ServerPicture::Sticker,
                                path,
                            },
                            "banner" => Action::PickProfilePicture {
                                kind: ProfilePicture::Banner,
                                path,
                            },
                            _ => Action::PickProfilePicture {
                                kind: ProfilePicture::Avatar,
                                path,
                            },
                        };
                        if let Some(window) = window {
                            adapter.borrow_mut().action(&window, action);
                        }
                        Ok(())
                    }
                    command if command.starts_with("stage ") => {
                        if let Some(window) = window {
                            let path = PathBuf::from(&command["stage ".len()..]);
                            adapter
                                .borrow_mut()
                                .action(&window, Action::Stage(vec![path]));
                        }
                        Ok(())
                    }
                    // A file for the thread beside's next message.
                    command if command.starts_with("stage-aside ") => {
                        if let Some(window) = window {
                            let path = PathBuf::from(&command["stage-aside ".len()..]);
                            adapter
                                .borrow_mut()
                                .action(&window, Action::StageAside(vec![path]));
                        }
                        Ok(())
                    }
                    _ => Ok(()),
                };
                if let Err(error) = result {
                    tracing::error!(command, %error, "test window control failed");
                }
            }
        },
    );
    timer
}

/// Return freed heap pages to the system, to separate memory still in use from
/// memory glibc keeps for reuse.
fn trim() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> std::ffi::c_int;
        }
        // SAFETY: malloc_trim only releases free memory held by the allocator.
        unsafe { malloc_trim(0) };
    }
    // Windows keeps what a burst of allocations freed, ready for the next
    // burst, in heaps the low-fragmentation heap makes for those sizes.
    // `HeapCompact` joins freed blocks but returns none of the heaps;
    // `HeapOptimizeResources` gives back what they hold, which is what lets a
    // closed picker's memory go.
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Memory::{HeapOptimizeResources, HeapSetInformation};
        // HEAP_OPTIMIZE_RESOURCES_INFORMATION, which windows-sys doesn't name.
        #[repr(C)]
        struct OptimizeResources {
            version: u32,
            flags: u32,
        }
        // HEAP_OPTIMIZE_RESOURCES_CURRENT_VERSION.
        const CURRENT_VERSION: u32 = 1;
        let info = OptimizeResources {
            version: CURRENT_VERSION,
            flags: 0,
        };
        // SAFETY: the information is the one HeapOptimizeResources takes, a
        // null heap handle means every heap of the process, and optimizing
        // them moves nothing in use.
        unsafe {
            HeapSetInformation(
                std::ptr::null_mut(),
                HeapOptimizeResources,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of::<OptimizeResources>(),
            );
        }
    }
}

/// A picker of many pictures closed: once the frame without it is drawn,
/// Skia lets go of what it kept to draw them, and then the heap gives back
/// what that and the picker freed. Without this the memory stays with the
/// process until something else needs it.
pub(super) fn let_go_after_picker(window: &YapfastWindow) {
    let weak = window.as_weak();
    slint::Timer::single_shot(Duration::from_secs(1), move || {
        i_slint_renderer_skia::yapfast_let_go_of_caches();
        if let Some(window) = weak.upgrade() {
            window.window().request_redraw();
        }
        trim_over(&[500, 2_000, 5_000]);
    });
}

/// Trim now, then again after each of `delays` milliseconds: the last
/// downloads, decodes and animations of a closed picker free their memory as
/// they finish, after the trim the close itself made.
fn trim_over(delays: &'static [u64]) {
    let Some((&delay, rest)) = delays.split_first() else {
        return;
    };
    slint::Timer::single_shot(Duration::from_millis(delay), move || {
        trim();
        trim_over(rest);
    });
}

/// Print the allocator's per-arena totals to standard error: memory held from
/// the system against memory in use.
fn allocator_stats() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_stats();
        }
        // SAFETY: malloc_stats only reads allocator state and writes to stderr.
        unsafe { malloc_stats() };
    }
}

/// The layout a person chose, such as the sidebar width, kept beside the session.
const LAYOUT: &str = "layout.json";

/// A sticker this device kept, as `layout.json` keeps it.
#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct KeptSticker {
    id: String,
    name: String,
    /// Discord's `format_type`, which its picture's address takes.
    format: u64,
    /// Its picture at the picker's tile size.
    picture: String,
}

/// A sticker this device sent, and how often, as `layout.json` keeps it.
#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct KeptStickerUse {
    id: String,
    name: String,
    /// Discord's `format_type`, which its picture's address takes.
    format: u64,
    /// Its picture at the picker's tile size.
    picture: String,
    uses: u32,
}

#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Layout {
    sidebar_width: Option<f32>,
    /// The width of a Discord thread shown beside its channel.
    aside_width: Option<f32>,
    /// The expression picker's width, the GIFs and the emoji alike, as its
    /// left edge last dragged it wider. `gif_picker_width` named it while only
    /// the GIFs took it.
    #[serde(alias = "gif_picker_width")]
    picker_width: Option<f32>,
    /// The member list shows until someone closes it.
    members_hidden: bool,
    /// So does the profile beside a one-to-one DM.
    profile_panel_hidden: bool,
    /// How far the whole window is zoomed, as one of [`zoom::LEVELS`]; 100%
    /// unless changed.
    zoom: Option<f32>,
    /// Each server's last selected channel, by server ID.
    channels: HashMap<String, String>,
    /// Each server's collapsed channel categories, by server ID: a Discord
    /// category by its room id, any other by its heading.
    collapsed: HashMap<String, std::collections::BTreeSet<String>>,
    /// The rail's open folders, by folder ID.
    open_folders: std::collections::BTreeSet<String>,
    /// How often this device added each reaction, for the frequent reactions.
    reactions: HashMap<String, u32>,
    /// The stickers this device sent, and how often, for Frequently Used.
    sticker_uses: Vec<KeptStickerUse>,
    /// The stickers this device keeps, for Favorites.
    sticker_favorites: Vec<KeptSticker>,
    /// List Discord channels hidden from this account, locked.
    show_hidden_channels: bool,
    /// Reading in Yapfast marks Discord channels read on Discord; on unless turned off.
    sync_discord_reads: Option<bool>,
    /// Discord shows this account idle after 10 minutes nobody uses Yapfast;
    /// on unless turned off.
    show_idle: Option<bool>,
    /// Games this device names itself: an executable, by file name or full
    /// path, and the game to show for it.
    game_executables: Vec<(String, String)>,
    /// Messages from people this account blocked don't show at all.
    hide_blocked: bool,
    /// A call from someone's menu starts without asking first.
    skip_call_prompt: bool,
    /// The member list's Activity panel shows only its header.
    activity_cards_hidden: bool,
    /// A message that notifies while Yapfast isn't in front flashes the
    /// taskbar; on unless turned off.
    flash_taskbar: Option<bool>,
    /// DMs pinned to the top of the DM list, by room ID, the first pinned first.
    pinned: Vec<String>,
    /// Message times as 9:41 PM rather than 21:41.
    twelve_hour_clock: bool,
    /// Message times with their seconds, as 21:41:05.
    clock_seconds: bool,
    /// The order of day, month and year, as an index into
    /// [`yapfast_core::clock::DateFormat::ALL`]; follows the clock unless chosen.
    date_format: Option<i32>,
    /// Message text: 0 small, 1 default, 2 large.
    text_size: Option<i32>,
    /// Link previews under messages; on unless turned off.
    link_previews: Option<bool>,
    /// A gem by each server booster in the member list; on unless turned off.
    boost_gems: Option<bool>,
    /// Replies mention the person they reply to; on unless turned off.
    reply_ping: Option<bool>,
    /// Tracking comes out of links in sent messages; on unless turned off.
    strip_tracking: Option<bool>,
    /// Nameplates show; on unless turned off.
    nameplates: Option<bool>,
    /// Long messages fold to a few lines, with Show more; off unless turned
    /// on.
    collapse_long: bool,
    /// When animations play.
    animate: Animate,
    /// The profile switches, each as [`Cosmetics`] starts it unless changed.
    profile_effects: Option<bool>,
    decorations: Option<bool>,
    moving_cosmetics: Option<bool>,
    name_styles: Option<bool>,
    server_tags: Option<bool>,
    /// Joining a call turns the microphone on; on unless turned off.
    unmute_on_join: Option<bool>,
    /// Joining a call deafens.
    deafen_on_join: bool,
    /// Your voice is evened out in calls; on unless turned off.
    level_voice: Option<bool>,
    /// What takes out noise in Discord calls, and how far; DeepFilterNet,
    /// at Medium, unless changed.
    noise_suppression: yapfast_core::noise::Suppressor,
    noise_strength: yapfast_core::noise::Strength,
    /// Discord calls take the room's reverb out of the microphone.
    reduce_room_echo: bool,
    /// Windows runs its microphone effects on Discord calls' microphone;
    /// off, Yapfast skips them. See [`yapfast_core::set_windows_effects`].
    windows_microphone_effects: bool,
    /// The quietest speech Discord calls send, in dBFS; -60 unless changed.
    input_sensitivity: Option<i32>,
    /// Whether calls send the voice the gate hears or only while a key is
    /// held, the key, and how long the microphone sends after it comes up,
    /// in ms; Discord's 20 unless changed.
    input_mode: InputMode,
    push_to_talk_key: Option<crate::push_to_talk::Keybind>,
    push_to_talk_release_delay: Option<u32>,
    /// How loud every call sends the microphone and plays, as sliders'
    /// percentages from 0% to 200%; 100% unless changed.
    microphone_volume: Option<u8>,
    speaker_volume: Option<u8>,
    /// How loud people sound in calls, and their streams, by Discord or
    /// Matrix user id; see [`yapfast_core::volume`]. Unlisted people are as they
    /// come.
    volumes: std::collections::BTreeMap<String, f32>,
    stream_volumes: std::collections::BTreeMap<String, f32>,
    /// Those muted for this account alone, from their menu in a call.
    muted_people: std::collections::BTreeSet<String>,
    /// Those whose soundboard sounds this account doesn't hear, from the
    /// same menu.
    soundboard_muted_people: std::collections::BTreeSet<String>,
    /// Others see when this account is typing; on unless turned off.
    share_typing: Option<bool>,
    /// Desktop notifications, and whether they say what messages say; on unless
    /// turned off.
    notifications: Option<bool>,
    notification_text: Option<bool>,
    /// Whether notifications played the desktop's sound, before Yapfast had its own;
    /// read once, to keep messages quiet for someone who turned it off.
    #[serde(skip_serializing)]
    notification_sound: Option<bool>,
    /// The instrument Yapfast's sounds play on, and how loud they are as a slider's
    /// percentage from 0% to 200%; [`DEFAULT_SOUND_VOLUME`] unless changed.
    sound_set: SoundSet,
    sound_volume: Option<u8>,
    /// Each sound has its own volume too, against that one; [`Self::each_sound`]
    /// holds them.
    adjust_each_sound: bool,
    /// Each sound's own volume as a percentage up to 100%, if it isn't 100%.
    each_sound: std::collections::BTreeMap<Sound, u8>,
    /// Yapfast draws with Skia GPU from its next start; see [`graphics`].
    gpu_rendering: bool,
    /// Screen sharing encodes on the GPU where it has an encoder; on unless
    /// turned off.
    gpu_encoding: Option<bool>,
    /// The theme chosen, by its id; Yap unless chosen.
    theme: Option<String>,
    /// The person's own themes, in the order Appearance shows them.
    custom_themes: Vec<theme::Custom>,
    /// Closing the window leaves Yapfast in the system tray; on unless turned off.
    close_to_tray: Option<bool>,
    /// Starting Yapfast again on this folder starts another, rather than
    /// showing the running one's window; see [`crate::instance`].
    multiple_windows: bool,
    /// Menus offer Copy ID, as Discord's Developer Mode does; off unless
    /// turned on.
    developer_mode: bool,
    /// Settings' About page asks GitHub for the releases when Yapfast starts;
    /// off unless turned on, and then Yapfast makes no request of its own.
    /// See [`updates`].
    check_for_updates: bool,
    /// Nightly builds count as newer releases; off unless turned on.
    update_nightlies: bool,
    /// The tag of the release that Skip this version hid.
    skipped_update: Option<String>,
    /// The tag of the release a notification last told of, so each release is
    /// announced once.
    announced_update: Option<String>,
    /// The version whose settings this person counts as having met: a fresh
    /// install's own, or the release before NEW tags for an older layout. A
    /// setting that arrived after it is NEW; see [`new_tags`].
    first_version: Option<String>,
    /// The NEW settings this person has looked at.
    seen_new: Vec<String>,
    /// The desktop draws the window's title bar; unset, Yapfast draws its own
    /// on desktops that give windows one. See [`title_bar`].
    system_title_bar: Option<bool>,
    /// Where the window was when Yapfast last ran, and how big.
    window: Option<placement::Placement>,
}

/// Yapfast's sounds start as they're made: 12 dB under full scale at the loudest.
const DEFAULT_SOUND_VOLUME: u8 = 100;

/// The noise suppression settings' choices, in the order they show.
const SUPPRESSORS: [yapfast_core::noise::Suppressor; 3] = [
    yapfast_core::noise::Suppressor::DeepFilterNet,
    yapfast_core::noise::Suppressor::RNNoise,
    yapfast_core::noise::Suppressor::Off,
];
const STRENGTHS: [yapfast_core::noise::Strength; 3] = [
    yapfast_core::noise::Strength::Full,
    yapfast_core::noise::Strength::Medium,
    yapfast_core::noise::Strength::Light,
];

/// How calls decide when to send the microphone, as Discord's Input Mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum InputMode {
    #[default]
    VoiceActivity,
    PushToTalk,
}

const INPUT_MODES: [InputMode; 2] = [InputMode::VoiceActivity, InputMode::PushToTalk];

/// When GIFs, animated emoji and stickers play. Held still, an animation
/// decodes no frame past the one showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Animate {
    /// While Yapfast's window has the focus, as in Discord, so they hold still
    /// on another screen.
    #[default]
    Focused,
    Always,
    Never,
}

/// The animation settings' choices, in the order they show.
const ANIMATES: [Animate; 3] = [Animate::Focused, Animate::Always, Animate::Never];

/// Asks the desktop to flash Yapfast's taskbar entry, or stops it: an urgency
/// hint on X11, an activation request on Wayland, a flash on Windows.
fn attention(window: &YapfastWindow, wanted: bool) {
    use slint::winit_030::winit::window::UserAttentionType;
    window.window().with_winit_window(|winit| {
        winit.request_user_attention(wanted.then_some(UserAttentionType::Informational));
    });
}

/// Plays animations or holds them still, as the setting and the focus say.
fn show_motion(window: &YapfastWindow) {
    let playing = match ANIMATE.get() {
        Animate::Focused => FOCUSED.get(),
        Animate::Always => true,
        Animate::Never => false,
    };
    window.global::<Motion>().set_playing(playing);
    window
        .global::<Motion>()
        .set_never(ANIMATE.get() == Animate::Never);
    // Animated emoji hold still with everything else, at once rather than at
    // the next refresh.
    i_slint_core::yapfast_emoji::set_playing(playing);
}

/// Where a setting's value is among its choices.
fn choice<T: PartialEq>(choices: &[T], value: T) -> i32 {
    choices
        .iter()
        .position(|choice| *choice == value)
        .unwrap_or(0) as i32
}

/// Calls send the microphone and play at the volume sliders' values.
fn play_call_volumes(window: &YapfastWindow) {
    let gain = |value: i32| yapfast_core::volume::from_slider(value.clamp(0, 200) as f32);
    yapfast_core::volume::set_microphone(gain(window.get_microphone_volume()));
    yapfast_core::volume::set_speakers(gain(window.get_speaker_volume()));
}

/// The choice a setting shows at an index; the first if there's none.
fn chosen<T: Copy>(choices: &[T], index: i32) -> T {
    choices
        .get(usize::try_from(index).unwrap_or(0))
        .copied()
        .unwrap_or(choices[0])
}

impl Layout {
    /// Hands the date format and the 12-hour clock to what formats times.
    fn apply_date_format(&self) {
        clock::set_date_format(self.date_format.and_then(|index| {
            clock::DateFormat::ALL
                .get(usize::try_from(index).ok()?)
                .copied()
        }));
        // After the date format, which Discord's timestamps follow.
        clock::set_twelve_hour_clock(self.twelve_hour_clock);
    }

    /// The voice gate's sensitivity, in whole dBFS from -80 to 0.
    fn sensitivity(&self) -> i32 {
        self.input_sensitivity
            .unwrap_or(yapfast_core::noise::DEFAULT_SENSITIVITY as i32)
            .clamp(-80, 0)
    }

    /// How long the microphone sends after the push-to-talk key comes up, in ms.
    fn release_delay(&self) -> u32 {
        self.push_to_talk_release_delay
            .unwrap_or(yapfast_core::push_to_talk::DEFAULT_RELEASE_DELAY)
            .min(yapfast_core::push_to_talk::MOST_RELEASE_DELAY)
    }

    fn read(root: &std::path::Path) -> Self {
        let saved = std::fs::read(root.join(LAYOUT)).ok();
        let fresh = saved.is_none();
        let mut layout: Self = saved
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        if layout.first_version.is_none() {
            layout.first_version = Some(new_tags::first_version(fresh));
        }
        if layout.notification_sound.take() == Some(false) {
            layout.adjust_each_sound = true;
            layout.each_sound.insert(Sound::Message, 0);
            layout.each_sound.insert(Sound::Mention, 0);
        }
        theme::tidy(&mut layout.custom_themes);
        layout
    }

    /// The profile switches, as saved.
    fn cosmetics(&self) -> Cosmetics {
        let default = Cosmetics::default();
        Cosmetics {
            effects: self.profile_effects.unwrap_or(default.effects),
            decorations: self.decorations.unwrap_or(default.decorations),
            moving: self.moving_cosmetics.unwrap_or(default.moving),
            name_styles: self.name_styles.unwrap_or(default.name_styles),
            server_tags: self.server_tags.unwrap_or(default.server_tags),
        }
    }

    /// Sets Settings' profile switches as saved, and whether collectibles'
    /// animated pictures may be asked for.
    fn show_cosmetics(&self, window: &YapfastWindow) {
        let cosmetics = self.cosmetics();
        let looks = window.global::<Looks>();
        looks.set_profile_effects(cosmetics.effects);
        looks.set_decorations(cosmetics.decorations);
        looks.set_moving(cosmetics.moving);
        looks.set_name_styles(cosmetics.name_styles);
        looks.set_server_tags(cosmetics.server_tags);
        pictures::set_moving_collectibles(cosmetics.moving);
    }

    /// Reads Settings' profile switches.
    fn read_cosmetics(&mut self, window: &YapfastWindow) {
        let looks = window.global::<Looks>();
        self.profile_effects = Some(looks.get_profile_effects());
        self.decorations = Some(looks.get_decorations());
        self.moving_cosmetics = Some(looks.get_moving());
        self.name_styles = Some(looks.get_name_styles());
        self.server_tags = Some(looks.get_server_tags());
        pictures::set_moving_collectibles(self.cosmetics().moving);
    }

    /// Shows the custom games, and has detection use them.
    fn apply_custom_games(&self, window: &YapfastWindow) {
        window.set_game_executables(ModelRc::new(VecModel::from(
            self.game_executables
                .iter()
                .map(|(executable, text)| GameExecutable {
                    executable: executable.as_str().into(),
                    text: text.as_str().into(),
                })
                .collect::<Vec<_>>(),
        )));
        discord::set_custom_games(&self.game_executables);
    }

    /// A sound's own volume as a percentage: 100% unless adjusted.
    fn each_sound(&self, sound: Sound) -> u8 {
        self.each_sound
            .get(&sound)
            .copied()
            .filter(|_| self.adjust_each_sound)
            .unwrap_or(100)
    }

    /// How loud a sound plays, as a gain.
    fn sound_gain(&self, sound: Sound) -> f32 {
        sounds::gain(self.sound_volume.unwrap_or(DEFAULT_SOUND_VOLUME))
            * sounds::gain(self.each_sound(sound))
    }

    // Appearance checks can apply these preferences without changing global audio gains.
    fn apply_appearance(&self, window: &YapfastWindow) {
        // Before the first frame, which the event loop draws. A window made
        // again, after closing to the tray, keeps the theme this way too.
        let default_theme = window
            .global::<ClientStyle>()
            .get_enabled()
            .then_some("bonfire");
        let (id, chosen) =
            theme::chosen(self.theme.as_deref().or(default_theme), &self.custom_themes);
        chosen.apply(window);
        window.set_theme_id(id.into());
        window.set_themes(theme::cards(&self.custom_themes));
        if let Some(width) = self.sidebar_width {
            window.set_sidebar_width(width);
        }
    }

    fn apply(&self, window: &YapfastWindow) {
        self.apply_custom_games(window);
        window.set_show_idle(self.show_idle.unwrap_or(true));
        window.set_hide_blocked(self.hide_blocked);
        window.set_skip_call_prompt(self.skip_call_prompt);
        window.set_activity_cards_hidden(self.activity_cards_hidden);
        window.set_flash_taskbar(self.flash_taskbar.unwrap_or(true));
        self.apply_appearance(window);
        if let Some(width) = self.aside_width {
            window.set_aside_width(width);
        }
        if let Some(width) = self.picker_width {
            window.set_picker_width(width);
        }
        window.set_member_list(!self.members_hidden);
        window.set_profile_panel(!self.profile_panel_hidden);
        window.set_twelve_hour_clock(self.twelve_hour_clock);
        window.set_clock_seconds(self.clock_seconds);
        window.set_date_format(self.date_format.unwrap_or(-1));
        self.apply_date_format();
        window.set_text_size(self.text_size.unwrap_or(1).clamp(0, 2));
        window.set_link_previews(self.link_previews.unwrap_or(true));
        window.set_boost_gems(self.boost_gems.unwrap_or(true));
        window.set_reply_ping_default(self.reply_ping.unwrap_or(true));
        window.set_strip_tracking(self.strip_tracking.unwrap_or(true));
        window.set_show_nameplates(self.nameplates.unwrap_or(true));
        window.set_collapse_long(self.collapse_long);
        window.set_animate(choice(&ANIMATES, self.animate));
        ANIMATE.set(self.animate);
        self.show_cosmetics(window);
        show_motion(window);
        window.set_unmute_on_join(self.unmute_on_join.unwrap_or(true));
        window.set_deafen_on_join(self.deafen_on_join);
        window.set_level_voice(self.level_voice.unwrap_or(true));
        yapfast_core::leveling::set_enabled(self.level_voice.unwrap_or(true));
        window.set_noise_suppression(choice(&SUPPRESSORS, self.noise_suppression));
        window.set_noise_strength(choice(&STRENGTHS, self.noise_strength));
        yapfast_core::noise::set(self.noise_suppression);
        yapfast_core::noise::set_strength(self.noise_strength);
        window.set_dereverb(self.reduce_room_echo);
        yapfast_core::noise::set_dereverb(self.reduce_room_echo);
        window.set_windows_effects_choice(cfg!(windows));
        window.set_windows_effects(self.windows_microphone_effects);
        yapfast_core::set_windows_effects(self.windows_microphone_effects);
        let sensitivity = self.sensitivity();
        window.set_sensitivity(sensitivity);
        yapfast_core::noise::set_sensitivity(sensitivity as f32);
        window.set_input_mode(choice(&INPUT_MODES, self.input_mode));
        yapfast_core::push_to_talk::set_on(self.input_mode == InputMode::PushToTalk);
        window.set_talk_key(
            self.push_to_talk_key
                .as_ref()
                .map(|key| key.name.as_str())
                .unwrap_or_default()
                .into(),
        );
        window.set_talk_delay(self.release_delay() as i32);
        yapfast_core::push_to_talk::set_release_delay(self.release_delay());
        window.set_microphone_volume(self.microphone_volume.unwrap_or(100).into());
        window.set_speaker_volume(self.speaker_volume.unwrap_or(100).into());
        play_call_volumes(window);
        for (person, volume) in &self.volumes {
            yapfast_core::volume::set(yapfast_core::volume::Kind::Voice, person, *volume);
        }
        for (person, volume) in &self.stream_volumes {
            yapfast_core::volume::set(yapfast_core::volume::Kind::Stream, person, *volume);
        }
        for person in &self.muted_people {
            yapfast_core::volume::mute(person, true);
        }
        for person in &self.soundboard_muted_people {
            yapfast_core::volume::mute_soundboard(person, true);
        }
        window.set_share_typing(self.share_typing.unwrap_or(true));
        window.set_notifications(self.notifications.unwrap_or(true));
        window.set_notification_text(self.notification_text.unwrap_or(true));
        window.set_sound_set(choice(&SoundSet::ALL, self.sound_set));
        window.set_sound_volume(self.sound_volume.unwrap_or(DEFAULT_SOUND_VOLUME).into());
        window.set_adjust_each_sound(self.adjust_each_sound);
        window.set_gpu_rendering(self.gpu_rendering);
        window.set_gpu_encoding(self.gpu_encoding.unwrap_or(true));
        #[cfg(feature = "discord")]
        yapfast_core::stream_worker::set_hardware_encoding(self.gpu_encoding.unwrap_or(true));
        window.set_close_to_tray(self.close_to_tray.unwrap_or(true));
        window.set_multiple_windows(self.multiple_windows);
        window.set_developer_mode(self.developer_mode);
        title_bar::show(window, self.system_title_bar);
        let each: Vec<i32> = Sound::ALL
            .iter()
            .map(|sound| self.each_sound.get(sound).copied().unwrap_or(100).into())
            .collect();
        window.set_each_sound(ModelRc::new(VecModel::from(each)));
    }

    /// A message's row as these preferences show it: its time on the chosen clock, with its
    /// date when it's from another day, its link previews only when they're on, and its
    /// author's name style and server tag only as the profile switches let them show.
    fn restyle(&self, message: &ChatMessage, mut row: MessageRow) -> MessageRow {
        let cosmetics = self.cosmetics();
        if !cosmetics.name_styles {
            row.name_style = ModelRc::default();
        }
        if !cosmetics.server_tags {
            row.tag = SharedString::default();
            row.tag_badge = SharedString::default();
        }
        if message.timestamp != 0 {
            let (twelve_hour, with_seconds) = (self.twelve_hour_clock, self.clock_seconds);
            let now = yapfast_core::clock::now_ms() / 1000;
            let sent = yapfast_core::clock::sent(message.timestamp, now, twelve_hour, with_seconds);
            // The gutter beside a grouped message is narrow, so it shows only the clock time,
            // without AM and PM.
            let time = yapfast_core::clock::time_with(message.timestamp, twelve_hour, with_seconds);
            row.short_time = time.split_whitespace().next().unwrap_or_default().into();
            if let Some(notice) = &message.notice
                && !message.blocked
            {
                row.styled = notice_text(message, notice, &sent);
            }
            row.time_styled = slint::StyledText::from_plain_text(&sent);
            row.time = sent.into();
        }
        if !self.link_previews.unwrap_or(true) {
            row.embeds = ModelRc::default();
            // A bot's card can be the whole message; say it's there.
            if message.body.trim().is_empty()
                && !message.attachments.iter().any(|attachment| {
                    matches!(
                        attachment.placement,
                        Placement::Message | Placement::Sticker
                    )
                })
                && let Some(embed) = message.embeds.first()
            {
                row.hidden_card = if embed.title.is_empty() {
                    "A card, hidden while link previews are off.".into()
                } else {
                    format!(
                        "A card, \"{}\", hidden while link previews are off.",
                        embed.title
                    )
                    .into()
                };
            }
        }
        row
    }

    /// A lost layout only costs the person their arrangement; it never stops Yapfast.
    fn write(&self, root: &std::path::Path) {
        let result = std::fs::create_dir_all(root).and_then(|()| {
            std::fs::write(
                root.join(LAYOUT),
                serde_json::to_vec(self).expect("layout serializes"),
            )
        });
        if let Err(error) = result {
            tracing::warn!(%error, "could not store the window layout");
        }
    }
}

#[derive(Default)]
struct Models {
    servers: Rc<VecModel<ServerRow>>,
    rooms: Rc<VecModel<RoomRow>>,
    /// Discord friend requests and message requests, on the requests screen.
    requests_incoming: Rc<VecModel<RequestRow>>,
    requests_sent: Rc<VecModel<RequestRow>>,
    requests_spam: Rc<VecModel<RequestRow>>,
    messages: Rc<VecModel<MessageRow>>,
    /// The messages of the thread beside the open channel.
    aside_messages: Rc<VecModel<MessageRow>>,
    roster: Rc<VecModel<MemberRow>>,
    /// What the members are doing, in the panel above the member list.
    roster_activity: Rc<VecModel<MemberActivityRow>>,
    /// What waits in the title bar's inbox, while it is open.
    inbox: Rc<VecModel<InboxRow>>,
    mentions: Rc<VecModel<MentionRow>>,
    new_dm_people: Rc<VecModel<ContactRow>>,
    /// The open DM's pinned messages, what its search found, and the
    /// friends its picker offers.
    pins: Rc<VecModel<FoundRow>>,
    /// The open channel's list of its threads.
    thread_rows: Rc<VecModel<ThreadRow>>,
    /// The open forum's posts.
    post_rows: Rc<VecModel<PostRow>>,
    /// The form for a post's chosen tags, and those it offers.
    post_chosen_tags: Rc<VecModel<PostTagChoice>>,
    post_tag_choices: Rc<VecModel<PostTagChoice>>,
    /// The Reactions window's reactions, and who reacted with the chosen one.
    reaction_pills: Rc<VecModel<ReactionPill>>,
    reactor_rows: Rc<VecModel<ReactorRow>>,
    /// The answers of the poll being made.
    poll_answers: Rc<VecModel<PollAnswerDraft>>,
    search_results: Rc<VecModel<FoundRow>>,
    add_people: Rc<VecModel<PickRow>>,
    role_members: Rc<VecModel<MemberRow>>,
    bar_reactions: Rc<VecModel<EmojiCell>>,
    menu_reactions: Rc<VecModel<EmojiCell>>,
    emoji_rows: Rc<VecModel<EmojiRow>>,
    /// What the picker's grid draws of `emoji_rows`: the emoji and headings
    /// of the rows in sight and a row or two around them; see `emoji_grid`.
    emoji_tiles: Rc<VecModel<EmojiTile>>,
    emoji_headings: Rc<VecModel<EmojiRow>>,
    /// The picker's sidebar, built with the grid it jumps through.
    emoji_sections: Rc<VecModel<EmojiSection>>,
    emoji_suggestions: Rc<VecModel<EmojiCell>>,
    forward_targets: Rc<VecModel<PlaceRow>>,
    switcher_places: Rc<VecModel<PlaceRow>>,
    participants: Rc<VecModel<PersonRow>>,
    /// The GIF picker's tiles.
    gif_tiles: Rc<VecModel<GifTile>>,
    /// The sticker picker's tiles: its stickers and their collections'
    /// headings.
    sticker_tiles: Rc<VecModel<StickerGridTile>>,
    /// The collections down each picker's left, which its rail jumps to: the
    /// emoji picker's and the sticker picker's, the same rail's.
    sticker_sections: Rc<VecModel<RailSection>>,
    emoji_rail: Rc<VecModel<RailSection>>,
    /// Who watches this account's stream, and the one it watches, as rows of
    /// `participants`.
    share_viewers: Rc<VecModel<PersonRow>>,
    watch_viewers: Rc<VecModel<PersonRow>>,
    /// Each participant's newest camera picture, in the same order.
    participant_frames: Rc<VecModel<slint::Image>>,
    /// The streams in tiles of their own, as indexes of their participants.
    stream_tiles: Rc<VecModel<i32>>,
    /// Who is in the open DM's call, shown above its messages.
    call_people: Rc<VecModel<PersonRow>>,
    /// The call's soundboard: headings and rows of sounds.
    soundboard_rows: Rc<VecModel<SoundRow>>,
    /// The sounds Settings offers as an entrance sound, likewise, and the
    /// servers to choose one for.
    entrance_rows: Rc<VecModel<SoundRow>>,
    entrance_servers: Rc<VecModel<SharedString>>,
    /// Discord friends: all of them, and those around.
    friends: Rc<VecModel<FriendRow>>,
    friends_online: Rc<VecModel<FriendRow>>,
    /// Those the Discord account blocked, for Settings.
    blocked: Rc<VecModel<BlockedRow>>,
    /// Browse Channels' rows, after its search.
    browse_rows: Rc<VecModel<BrowseRow>>,
    microphones: Rc<VecModel<SharedString>>,
    speakers: Rc<VecModel<SharedString>>,
    cameras: Rc<VecModel<SharedString>>,
    /// Each device's name as last listed, by id, for a chosen device that
    /// isn't connected; see [`device_rows`].
    device_names: HashMap<String, SharedString>,
    staged: Rc<VecModel<StagedFile>>,
    aside_staged: Rc<VecModel<StagedFile>>,
}

impl Models {
    fn attach(&self, window: &YapfastWindow) {
        window.set_servers(ModelRc::from(self.servers.clone()));
        window.set_rooms(ModelRc::from(self.rooms.clone()));
        window.set_requests_incoming(ModelRc::from(self.requests_incoming.clone()));
        window.set_requests_sent(ModelRc::from(self.requests_sent.clone()));
        window.set_requests_spam(ModelRc::from(self.requests_spam.clone()));
        window.set_messages(ModelRc::from(self.messages.clone()));
        window.set_aside_messages(ModelRc::from(self.aside_messages.clone()));
        window.set_roster(ModelRc::from(self.roster.clone()));
        window.set_roster_activity(ModelRc::from(self.roster_activity.clone()));
        window.set_inbox_rows(ModelRc::from(self.inbox.clone()));
        window.set_mentions(ModelRc::from(self.mentions.clone()));
        window.set_new_dm_people(ModelRc::from(self.new_dm_people.clone()));
        window.set_pins(ModelRc::from(self.pins.clone()));
        window.set_thread_rows(ModelRc::from(self.thread_rows.clone()));
        window.set_post_rows(ModelRc::from(self.post_rows.clone()));
        window.set_post_chosen_tags(ModelRc::from(self.post_chosen_tags.clone()));
        window.set_post_tag_choices(ModelRc::from(self.post_tag_choices.clone()));
        window.set_reaction_pills(ModelRc::from(self.reaction_pills.clone()));
        window.set_reactor_rows(ModelRc::from(self.reactor_rows.clone()));
        window.set_poll_answers(ModelRc::from(self.poll_answers.clone()));
        window.set_search_results(ModelRc::from(self.search_results.clone()));
        window.set_add_people(ModelRc::from(self.add_people.clone()));
        window.set_role_members(ModelRc::from(self.role_members.clone()));
        window.set_bar_reactions(ModelRc::from(self.bar_reactions.clone()));
        window.set_menu_reactions(ModelRc::from(self.menu_reactions.clone()));
        window.set_emoji_rows(ModelRc::from(self.emoji_rows.clone()));
        window.set_emoji_tiles(ModelRc::from(self.emoji_tiles.clone()));
        window.set_emoji_headings(ModelRc::from(self.emoji_headings.clone()));
        window.set_emoji_sections(ModelRc::from(self.emoji_sections.clone()));
        window.set_emoji_suggestions(ModelRc::from(self.emoji_suggestions.clone()));
        window.set_forward_targets(ModelRc::from(self.forward_targets.clone()));
        window.set_switcher_places(ModelRc::from(self.switcher_places.clone()));
        window.set_participants(ModelRc::from(self.participants.clone()));
        window.set_gif_tiles(ModelRc::from(self.gif_tiles.clone()));
        window.set_sticker_tiles(ModelRc::from(self.sticker_tiles.clone()));
        window.set_sticker_sections(ModelRc::from(self.sticker_sections.clone()));
        window.set_emoji_rail_sections(ModelRc::from(self.emoji_rail.clone()));
        window.set_share_viewers(ModelRc::from(self.share_viewers.clone()));
        window.set_watch_viewers(ModelRc::from(self.watch_viewers.clone()));
        window.set_participant_frames(ModelRc::from(self.participant_frames.clone()));
        window.set_stream_tiles(ModelRc::from(self.stream_tiles.clone()));
        window.set_call_people(ModelRc::from(self.call_people.clone()));
        window.set_soundboard_rows(ModelRc::from(self.soundboard_rows.clone()));
        window.set_entrance_rows(ModelRc::from(self.entrance_rows.clone()));
        window.set_entrance_servers(ModelRc::from(self.entrance_servers.clone()));
        window.set_friends(ModelRc::from(self.friends.clone()));
        window.set_friends_online(ModelRc::from(self.friends_online.clone()));
        window.set_blocked_people(ModelRc::from(self.blocked.clone()));
        window.set_browse_rows(ModelRc::from(self.browse_rows.clone()));
        window.set_microphones(ModelRc::from(self.microphones.clone()));
        window.set_speakers(ModelRc::from(self.speakers.clone()));
        window.set_cameras(ModelRc::from(self.cameras.clone()));
        window.set_staged(ModelRc::from(self.staged.clone()));
        window.set_aside_staged(ModelRc::from(self.aside_staged.clone()));
    }
}

/// A kind of Discord channel's icon, alone and with a lock or a warning
/// sign at its top right, as Discord's client marks a channel @everyone is
/// kept out of and an age-restricted one.
struct ChannelIcon {
    plain: slint::Image,
    private: slint::Image,
    age_restricted: slint::Image,
}

impl ChannelIcon {
    fn load(plain: &[u8], private: &[u8], age_restricted: &[u8]) -> Result<Self> {
        Ok(Self {
            plain: slint::Image::load_from_svg_data(plain)?,
            private: slint::Image::load_from_svg_data(private)?,
            age_restricted: slint::Image::load_from_svg_data(age_restricted)?,
        })
    }
}

struct Icons {
    dm: slint::Image,
    chat: slint::Image,
    text: ChannelIcon,
    /// A boxed ID, for Copy ID.
    id: slint::Image,
    thread: slint::Image,
    forum: ChannelIcon,
    voice: ChannelIcon,
    announcement: ChannelIcon,
    media: ChannelIcon,
    /// A stage channel's, which Discord doesn't mark age-restricted.
    stage: ChannelIcon,
    rules: slint::Image,
    lock: slint::Image,
    read: slint::Image,
    call: slint::Image,
    profile: slint::Image,
    copy: slint::Image,
    befriend: slint::Image,
    cancel: slint::Image,
    pin: slint::Image,
    unpin: slint::Image,
    note: slint::Image,
    nickname: slint::Image,
    unfriend: slint::Image,
    ignore: slint::Image,
    show: slint::Image,
    block: slint::Image,
    mute: slint::Image,
    unmute: slint::Image,
    // A server member's menu.
    mention: slint::Image,
    rename: slint::Image,
    roles: slint::Image,
    timeout: slint::Image,
    kick: slint::Image,
    ban: slint::Image,
    /// A Discord server's Audit Log, Members and Invites.
    history: slint::Image,
    members: slint::Image,
    invites: slint::Image,
    /// A Discord server's Emoji; its Stickers have `note`.
    emoji: slint::Image,
    /// Transfer Ownership in a member's menu.
    crown: slint::Image,
    // A Discord server's and channel's menus.
    create: slint::Image,
    create_category: slint::Image,
    settings: slint::Image,
    edit: slint::Image,
    duplicate: slint::Image,
    delete: slint::Image,
    leave: slint::Image,
    // Someone's menu in a voice channel.
    mic_off: slint::Image,
    soundboard: slint::Image,
    deafen: slint::Image,
    move_to: slint::Image,
    disconnect: slint::Image,
}

struct Adapter {
    // Cancel the UI event receiver before releasing the actor and runtime.
    events: Option<slint::JoinHandle<()>>,
    backend: Option<Backend>,
    runtime: Arc<BackgroundRuntime>,
    root: PathBuf,
    layout: Layout,
    /// What About knows of newer releases.
    updates: updates::State,
    /// The rows of the emoji picker's grid that may hold pictures it drew, first
    /// and last: see `fill_shown_glyphs`.
    emoji_rows_filled: (usize, usize),
    /// Which of the picker's rows it shows, and which emoji each of its
    /// buttons is showing.
    emoji_grid: emoji_grid::EmojiGrid,
    app: Application,
    models: Models,
    icons: Icons,
    weak: std::rc::Weak<RefCell<Self>>,
    window: slint::Weak<YapfastWindow>,
    away: away::Away,
    call_timer: call_timer::CallTimer,
    /// Ticks ActivityClock while a profile on screen times an activity.
    activity_clock: slint::Timer,
    navigation: Navigation,
    /// Whether the forward dialog was open at the last refresh.
    forwarding: bool,
    selection_version: u64,
    /// The conversation just shown was still loading, when its composer is
    /// disabled and takes no focus; it takes it once the messages come.
    focus_when_ready: bool,
    reset_version: u64,
    /// The timeline shown, which tells the messages that changed since.
    messages: Arc<Vec<Arc<ChatMessage>>>,
    /// The rows built for those messages, as the list shows them.
    rows: Vec<MessageRow>,
    /// The same for the thread beside the open channel.
    aside_messages: Arc<Vec<Arc<ChatMessage>>>,
    aside_rows: Vec<MessageRow>,
    /// The width the thread beside the channel last fitted its rows to.
    aside_fit: i32,
    /// The thread's rows are built again at its next showing, as the
    /// timeline's were.
    aside_restyle: bool,
    /// What the sidebar showed last, so it's built again only once that changes.
    sidebar: Option<SidebarShown>,
    /// What the member list showed last, so it's built again only once that changes.
    roster_shown: Option<RosterShown>,
    /// Runs for [`ROSTER_PAUSE`] after the member list was built; see
    /// `show_members`.
    roster_due: slint::Timer,
    /// Whether the member list changed while `roster_due` ran, to show
    /// once it ends.
    roster_held: bool,
    /// The account's own profile its card showed last, so the card is built
    /// again only once that or one of its pictures changes.
    account_profile_shown: Option<AccountCardShown>,
    /// What the quick switcher listed last, while it's open.
    switcher: Option<SwitcherShown>,
    /// The room ids of the rows a Discord server's channel list drags and
    /// drops, by their places; see RoomRow.
    places: Vec<String>,
    /// The channel or category being dragged there, by room id.
    arranging: Option<String>,
    /// Someone being dragged from a voice channel to another, by the
    /// channel's room id and their user id.
    moving_person: Option<(String, String)>,
    /// The voice channel of the call the rail last showed, so a voice event
    /// builds the rail again only when the call moves.
    rail_call: Option<String>,
    /// A hash of what the friends picker's rows were built from, so they're
    /// built again only once that changes.
    picker_shown: Option<u64>,
    /// The bot picker rows last shown; unchanged syncs keep their widgets.
    bot_select_shown: Option<u64>,
    /// The same for the list of threads, and the minute it was built in,
    /// as its rows say how long ago each was active.
    threads_shown: Option<u64>,
    /// The same for the open forum's posts.
    forum_shown: Option<u64>,
    /// The topic the header shows, so it's styled again only once it changes.
    topic_shown: Option<Arc<yapfast_core::chat::Topic>>,
    /// The forum's guidelines as the form for a post last showed them, and
    /// the tags it offered and had.
    guidelines_shown: Option<Arc<yapfast_core::chat::Topic>>,
    post_tags_shown: Option<(Vec<yapfast_core::chat::ForumTag>, Vec<String>)>,
    /// Shows pictures that arrived, together with those that follow them closely.
    pictures_due: slint::Timer,
    /// A custom emoji's picture arrived for the emoji picker, shown with
    /// those that come a few frames after it.
    emoji_due: slint::Timer,
    /// A layout change waiting to be written; see [`Adapter::write_layout_soon`].
    layout_due: slint::Timer,
    layout_dirty: bool,
    /// Writes the layout once the window holds still after moving or resizing.
    placement_due: slint::Timer,
    /// Stream ticks without a new picture of a video that isn't playing,
    /// after which they stop until the video changes.
    video_quiet: Cell<u32>,
    /// The size, in pixels, of the cell the video playing shows in when it
    /// isn't full screen.
    video_cell: Cell<(u32, u32)>,
    /// Server icons and avatars already handed to Slint, by address.
    pictures: HashMap<String, slint::Image>,
    /// The emoji the open card shows, so the card is built again as what
    /// Discord says of its server arrives.
    emoji_card: Option<emoji_text::Target>,
    /// Pictures a picker's tiles draw, shown together once they settle.
    picker_due: slint::Timer,
    /// The sticker whose card is open, by its message's item and its
    /// attachment's index.
    sticker_card: Option<(String, usize)>,
    /// The animated emoji on screen: the frames they need and the clock they
    /// draw from, which stops while the window is unfocused, idle or minimised.
    animations: emoji_animation::Animations,
    /// `AppState::picture_changes` when the message rows were last built.
    pictures_seen: u64,
    /// `AppState::picture_changes` when `PictureStore` last caught up with
    /// it; none before the first refresh.
    pictures_sourced: Option<u64>,
    /// The last jump to a message the timeline made; see `AppState::jump`.
    jumped: Option<(String, u64)>,
    /// `AppState::unread_jump` when the timeline last brought the first
    /// unread message to its top.
    unread_jumped: u64,
    /// The newest messages are on their way in place of a window of older
    /// ones, and show at the end the moment they come. Following the
    /// window's end until then would load earlier pages as they go in.
    latest_due: bool,
    /// Spoilers clicked, and what was picked or shown on polls; see
    /// `message_row`.
    revealed: HashSet<String>,
    /// The minute the timeline was last checked for a poll's time left, and
    /// for a new day, which moves its messages' times to yesterday.
    clock_minute: u64,
    /// The preferences that shape message rows changed, so they're built again.
    restyle: bool,
    /// Appearance's theme editor, while it's open.
    theme_editor: Option<theme::Editor>,
    /// Where the `@` of the word being mentioned is in the composer text.
    mention_at: Option<usize>,
    /// What the composer's mentions were last coloured for.
    mentions_marked: [Option<refresh::MentionsMarked>; i_slint_core::yapfast_marks::SLOTS],
    /// Suggestions for the `@` here were closed with Escape; they stay closed until
    /// the cursor leaves the word.
    mention_dismissed: Option<usize>,
    /// The `:name` being typed in the composer: where its `:` is, and the name.
    emoji_word: Option<(usize, String)>,
    /// Like `mention_dismissed`, for the `:` here.
    emoji_dismissed: Option<usize>,
    /// The suggestions are for the composer of the thread beside the channel,
    /// typed in last, rather than the channel's.
    suggesting_aside: bool,
    /// The room whose composer last reported its cursor, and the cursor's byte
    /// offset, where the picker's emoji go.
    caret: Option<(String, usize)>,
    notifier: crate::notify::Notifier,
    /// The viewer's image, converted for Slint while it shows.
    viewer_image: Option<(Arc<Picture>, slint::Image)>,
    /// The size the viewer opened an image at, by item and attachment, kept
    /// so the full image replaces the preview without the image moving.
    viewer_size: Option<(String, usize, u32, u32)>,
    /// One open or save dialog at a time; dropping it cancels the dialog's task.
    dialog: Option<slint::JoinHandle<()>>,
    /// Reads the microphone while the input menu shows its level.
    meter: Option<InputMeter>,
    /// Films with the chosen camera while voice settings try it.
    camera_test: Option<(yapfast_core::camera::Camera, yapfast_core::picture::Feed)>,
    /// Films for a preview call with the camera on.
    preview_camera: Option<yapfast_core::camera::Camera>,
    /// Whose camera picture each place in the call's grid holds.
    frame_ids: Vec<String>,
    /// The soundboard's sounds as its rows show them, and the pictures'
    /// revision then, so the rows are made again only when either changes.
    soundboard_shown: Option<(Arc<[voice::SoundGroup]>, u64)>,
    /// The same for the entrance sounds Settings offers; the servers it
    /// offers them for, "All servers" first as none; and the soundboard
    /// volume shown, so the slider moves only when Discord's does.
    entrance_shown: Option<(Arc<[voice::SoundGroup]>, u64)>,
    entrance_servers: Vec<Option<String>>,
    soundboard_volume_shown: Option<f32>,
    /// Where the call's tiles are, as the grid last said.
    tiles: Option<Tiles>,
    /// The call's tile shown large, and whose stream was watched when the
    /// call last showed, so a stream starts large.
    call_focus: Option<tiles::Focus>,
    watched: Option<String>,
    /// Runs the microphone through Discord's processing while voice settings
    /// show the voice gate's sensitivity.
    #[cfg(feature = "discord")]
    gate_meter: Option<yapfast_core::discord_audio::GateMeter>,
    /// Watches the push-to-talk key while push to talk is on and it's set,
    /// with or without the window.
    talk_key: Option<crate::push_to_talk::Watcher>,
    /// Previews of the pictures waiting in the composer, by file; `None` while
    /// one is read, or when it can't be.
    thumbnails: HashMap<PathBuf, Option<pictures::Thumbnail>>,
    /// Where pasted pictures are saved until they're sent; removed as Yapfast closes.
    pasted: Option<tempfile::TempDir>,
    /// Files are dragged over the window.
    drag_over: bool,
    /// Ringing for a DM call that rings this account.
    #[cfg(feature = "discord")]
    ringer: Option<yapfast_core::ring::Ringer>,
    /// Yapfast's icon in the system tray, where the window can close to.
    tray: Option<crate::tray::Tray>,
    /// Linking a Discord account by QR code; dropping it stops the linking.
    #[cfg(feature = "discord")]
    linking: Option<yapfast_core::runtime::BackgroundTask<()>>,
    /// Counts linking runs, so a replaced run's last words are ignored.
    #[cfg(feature = "discord")]
    link_run: u64,
    /// The code shown for linking, by the address it carries.
    #[cfg(feature = "discord")]
    link_code: Option<(String, slint::Image)>,
    /// What the GIF picker's grid shows.
    gifs: gifs::Shown,
    /// What the sticker picker's grid shows.
    stickers: stickers::Shown,
    /// A Discord link handed to this start before Discord is online.
    pending_link: Option<String>,
}

impl Adapter {
    /// What the channel's composer holds, or the thread's beside it with `aside`.
    fn composer_text(&self, aside: bool) -> String {
        let state = &self.app.state;
        if aside {
            state.aside_draft()
        } else {
            state.draft()
        }
        .to_owned()
    }

    /// Puts `text` in the channel's composer, or the thread's beside it.
    fn set_composer_text(&mut self, window: &YapfastWindow, aside: bool, text: String) {
        let action = if aside {
            Action::SetAsideDraft(text)
        } else {
            Action::SetDraft(text)
        };
        self.action(window, action);
    }

    fn new(
        runtime: Arc<BackgroundRuntime>,
        root: PathBuf,
        window: &YapfastWindow,
    ) -> Result<Rc<RefCell<Self>>> {
        let icons = Icons {
            dm: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/message-circle.svg"))?,
            chat: slint::Image::load_from_svg_data(include_bytes!(
                "../ui/icons/message-square-text.svg"
            ))?,
            text: ChannelIcon::load(
                include_bytes!("../ui/icons/hash.svg"),
                include_bytes!("../ui/icons/hash-lock.svg"),
                include_bytes!("../ui/icons/hash-warning.svg"),
            )?,
            id: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/id-filled.svg"))?,
            thread: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/thread.svg"))?,
            forum: ChannelIcon::load(
                include_bytes!("../ui/icons/message-square-text.svg"),
                include_bytes!("../ui/icons/message-square-text-lock.svg"),
                include_bytes!("../ui/icons/message-square-text-warning.svg"),
            )?,
            voice: ChannelIcon::load(
                include_bytes!("../ui/icons/volume-2.svg"),
                include_bytes!("../ui/icons/volume-2-lock.svg"),
                include_bytes!("../ui/icons/volume-2-warning.svg"),
            )?,
            announcement: ChannelIcon::load(
                include_bytes!("../ui/icons/megaphone.svg"),
                include_bytes!("../ui/icons/megaphone-lock.svg"),
                include_bytes!("../ui/icons/megaphone-warning.svg"),
            )?,
            media: ChannelIcon::load(
                include_bytes!("../ui/icons/image.svg"),
                include_bytes!("../ui/icons/image-lock.svg"),
                include_bytes!("../ui/icons/image-warning.svg"),
            )?,
            stage: ChannelIcon::load(
                include_bytes!("../ui/icons/stage.svg"),
                include_bytes!("../ui/icons/stage-lock.svg"),
                include_bytes!("../ui/icons/stage.svg"),
            )?,
            rules: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/rules.svg"))?,
            lock: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/lock.svg"))?,
            read: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/check.svg"))?,
            call: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/phone.svg"))?,
            profile: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/user.svg"))?,
            copy: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/copy.svg"))?,
            befriend: slint::Image::load_from_svg_data(include_bytes!(
                "../ui/icons/user-plus.svg"
            ))?,
            cancel: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/x.svg"))?,
            pin: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/pin.svg"))?,
            unpin: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/pin-off.svg"))?,
            note: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/sticky-note.svg"))?,
            nickname: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/user-pen.svg"))?,
            unfriend: slint::Image::load_from_svg_data(include_bytes!(
                "../ui/icons/user-minus.svg"
            ))?,
            ignore: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/eye-off.svg"))?,
            show: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/eye.svg"))?,
            block: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/ban.svg"))?,
            mute: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/bell-off.svg"))?,
            unmute: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/bell.svg"))?,
            mention: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/at-sign.svg"))?,
            rename: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/pencil.svg"))?,
            roles: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/shield.svg"))?,
            timeout: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/clock.svg"))?,
            kick: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/user-x.svg"))?,
            ban: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/gavel.svg"))?,
            history: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/history.svg"))?,
            members: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/users.svg"))?,
            invites: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/link.svg"))?,
            emoji: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/smile.svg"))?,
            crown: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/crown.svg"))?,
            create: slint::Image::load_from_svg_data(include_bytes!(
                "../ui/icons/circle-plus.svg"
            ))?,
            create_category: slint::Image::load_from_svg_data(include_bytes!(
                "../ui/icons/folder-plus.svg"
            ))?,
            settings: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/settings.svg"))?,
            edit: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/pencil.svg"))?,
            duplicate: slint::Image::load_from_svg_data(include_bytes!(
                "../ui/icons/copy-plus.svg"
            ))?,
            delete: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/trash-2.svg"))?,
            leave: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/log-out.svg"))?,
            mic_off: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/mic-off.svg"))?,
            soundboard: slint::Image::load_from_svg_data(include_bytes!(
                "../ui/icons/soundboard.svg"
            ))?,
            deafen: slint::Image::load_from_svg_data(include_bytes!(
                "../ui/icons/headphone-off.svg"
            ))?,
            move_to: slint::Image::load_from_svg_data(include_bytes!("../ui/icons/forward.svg"))?,
            disconnect: slint::Image::load_from_svg_data(include_bytes!(
                "../ui/icons/phone-off.svg"
            ))?,
        };
        let models = Models::default();
        models.attach(window);
        let layout = Layout::read(&root);
        layout.apply(window);
        let mut app = Application::default();
        app.remember_reaction_uses(layout.reactions.clone());
        app.remember_sticker_favorites(
            layout
                .sticker_favorites
                .iter()
                .map(|kept| yapfast_core::chat::Sticker {
                    id: kept.id.clone(),
                    name: kept.name.clone(),
                    format: kept.format,
                    picture: kept.picture.clone(),
                    usable: true,
                })
                .collect(),
        );
        app.remember_sticker_uses(
            layout
                .sticker_uses
                .iter()
                .map(|used| StickerUse {
                    sticker: yapfast_core::chat::Sticker {
                        id: used.id.clone(),
                        name: used.name.clone(),
                        format: used.format,
                        picture: used.picture.clone(),
                        usable: true,
                    },
                    uses: used.uses,
                })
                .collect(),
        );
        app.dispatch(Action::ShowHiddenChannels(layout.show_hidden_channels));
        app.dispatch(Action::ShowProfilePanel(!layout.profile_panel_hidden));
        app.dispatch(Action::SyncDiscordReads(
            layout.sync_discord_reads.unwrap_or(true),
        ));
        app.dispatch(Action::ReplyPing(layout.reply_ping.unwrap_or(true)));
        app.dispatch(Action::HideBlocked(layout.hide_blocked));
        app.dispatch(Action::SkipCallPrompt(layout.skip_call_prompt));
        app.dispatch(Action::ActivityCardsHidden(layout.activity_cards_hidden));
        app.dispatch(Action::SetCosmetics(layout.cosmetics()));
        app.dispatch(Action::StripTracking(layout.strip_tracking.unwrap_or(true)));
        app.dispatch(Action::HideNameplates(!layout.nameplates.unwrap_or(true)));
        app.dispatch(Action::SetPinned(layout.pinned.clone()));
        let notifier = crate::notify::Notifier::start(open_from_notification, &root);
        Ok(Rc::new_cyclic(|weak| {
            RefCell::new(Self {
                events: None,
                backend: None,
                runtime,
                root,
                layout,
                updates: Default::default(),
                emoji_rows_filled: (0, 0),
                emoji_grid: Default::default(),
                app,
                models,
                icons,
                weak: weak.clone(),
                window: window.as_weak(),
                away: away::Away::default(),
                call_timer: call_timer::CallTimer::default(),
                activity_clock: slint::Timer::default(),
                navigation: Navigation::default(),
                forwarding: false,
                selection_version: 0,
                focus_when_ready: false,
                reset_version: 0,
                messages: Arc::new(Vec::new()),
                rows: Vec::new(),
                sidebar: None,
                roster_shown: None,
                roster_due: slint::Timer::default(),
                roster_held: false,
                account_profile_shown: None,
                switcher: None,
                places: Vec::new(),
                arranging: None,
                moving_person: None,
                rail_call: None,
                picker_shown: None,
                bot_select_shown: None,
                threads_shown: None,
                forum_shown: None,
                topic_shown: None,
                guidelines_shown: None,
                post_tags_shown: None,
                pictures_due: slint::Timer::default(),
                emoji_due: slint::Timer::default(),
                layout_due: slint::Timer::default(),
                layout_dirty: false,
                placement_due: slint::Timer::default(),
                video_quiet: Cell::new(0),
                video_cell: Cell::new((0, 0)),
                pictures: HashMap::new(),
                emoji_card: None,
                picker_due: slint::Timer::default(),
                sticker_card: None,
                animations: emoji_animation::Animations::new(),
                pictures_seen: 0,
                pictures_sourced: None,
                jumped: None,
                unread_jumped: 0,
                latest_due: false,
                revealed: HashSet::new(),
                clock_minute: 0,
                restyle: false,
                theme_editor: None,
                aside_messages: Arc::new(Vec::new()),
                aside_fit: 0,
                aside_rows: Vec::new(),
                aside_restyle: false,
                mention_at: None,
                mentions_marked: [None, None],
                mention_dismissed: None,
                emoji_word: None,
                emoji_dismissed: None,
                suggesting_aside: false,
                caret: None,
                notifier,
                viewer_image: None,
                viewer_size: None,
                dialog: None,
                meter: None,
                camera_test: None,
                preview_camera: None,
                frame_ids: vec![],
                soundboard_shown: None,
                entrance_shown: None,
                entrance_servers: vec![],
                soundboard_volume_shown: None,
                tiles: None,
                call_focus: None,
                watched: None,
                #[cfg(feature = "discord")]
                gate_meter: None,
                talk_key: None,
                thumbnails: HashMap::new(),
                pasted: None,
                drag_over: false,
                #[cfg(feature = "discord")]
                ringer: None,
                tray: None,
                #[cfg(feature = "discord")]
                linking: None,
                #[cfg(feature = "discord")]
                link_run: 0,
                #[cfg(feature = "discord")]
                link_code: None,
                gifs: gifs::Shown::default(),
                stickers: stickers::Shown::default(),
                pending_link: None,
            })
        }))
    }

    /// Starts, changes or stops watching the push-to-talk key as settings
    /// say, and shows where it works.
    fn watch_talk_key(&mut self, window: &YapfastWindow) {
        let wanted = self
            .layout
            .push_to_talk_key
            .as_ref()
            .filter(|_| self.layout.input_mode == InputMode::PushToTalk);
        if self
            .talk_key
            .as_ref()
            .map(crate::push_to_talk::Watcher::key)
            != wanted
        {
            // The old one goes first, ending its portal session.
            self.talk_key = None;
            self.talk_key = wanted.cloned().map(|key| {
                let weak = window.as_weak();
                crate::push_to_talk::Watcher::start(key, &self.runtime, move |reach| {
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        window.set_talk_note(reach.note().into());
                    });
                })
            });
        }
        let note = self.talk_key.as_ref().map(|watcher| watcher.reach().note());
        window.set_talk_note(note.unwrap_or_default().into());
    }

    /// Makes a recorded key the push-to-talk key.
    fn set_talk_key(&mut self, window: &YapfastWindow, key: crate::push_to_talk::Keybind) {
        tracing::info!(key = key.name, "push-to-talk key recorded");
        window.set_talk_key(key.name.as_str().into());
        self.layout.push_to_talk_key = Some(key);
        self.layout.write(&self.root);
        self.watch_talk_key(window);
    }

    /// Shows whether Discord protocol links open here, as the desktop
    /// has it rather than as a setting remembers it.
    fn show_protocol(&self, window: &YapfastWindow) {
        let offered = PROTOCOL.get();
        window.set_discord_links_available(offered);
        window.set_discord_links(offered && crate::protocol::discord_enabled(&self.root));
    }

    /// Present the existing models and state in a new window.
    fn attach(&mut self, window: &YapfastWindow) {
        self.models.attach(window);
        self.layout.apply(window);
        updates::show(window, &self.layout, &self.updates);
        self.watch_talk_key(window);
        self.show_protocol(window);
        // The theme being edited goes unsaved with the old window, and rows
        // may carry its mention colour; the refresh after attaching builds
        // them again.
        if self.theme_editor.take().is_some() {
            self.restyle_mentions();
        }
        // These tell what the old window already showed, which the new one
        // doesn't: the sidebar's title and search placeholder, the topic and
        // the linking code are set only when they change.
        self.sidebar = None;
        self.topic_shown = None;
        #[cfg(feature = "discord")]
        {
            self.link_code = None;
        }
        self.window = window.as_weak();
    }

    /// Ctrl+V in the composer, or with `aside` the thread beside's: files and
    /// pictures on the clipboard wait to go with the message, and anything
    /// else is pasted as text.
    fn paste(&mut self, aside: bool) {
        if self.pasted.is_none() {
            self.pasted = tempfile::Builder::new()
                .prefix("yapfast-pasted-")
                .tempdir()
                .inspect_err(
                    |error| tracing::warn!(%error, "could not make a folder for pasted pictures"),
                )
                .ok();
        }
        let folder = self.pasted.as_ref().map(|dir| dir.path().to_owned());
        let reading = self
            .runtime
            .spawn_blocking(move || match crate::paste::read() {
                crate::paste::Clipboard::Files(files) => Some(files),
                crate::paste::Clipboard::Picture(bytes, extension) => {
                    // A folder for each paste, so each picture is image.png, as in Discord.
                    let folder = tempfile::Builder::new()
                        .prefix("")
                        .tempdir_in(folder?)
                        .ok()?
                        .keep();
                    let path = folder.join(format!("image.{extension}"));
                    std::fs::write(&path, bytes).ok()?;
                    Some(vec![path])
                }
                crate::paste::Clipboard::Other => None,
            });
        let weak = self.weak.clone();
        let task = slint::spawn_local(async move {
            let files = reading.await.ok().flatten();
            let Some(adapter) = weak.upgrade() else {
                return;
            };
            let window = adapter.borrow().window.upgrade();
            let Some(window) = window else {
                return;
            };
            match files {
                Some(files) if aside => adapter
                    .borrow_mut()
                    .action(&window, Action::StageAside(files)),
                Some(files) => adapter.borrow_mut().action(&window, Action::Stage(files)),
                None if aside => window.set_aside_paste_tick(window.get_aside_paste_tick() + 1),
                None => window.invoke_paste_composer_text(),
            }
        });
        if let Err(error) = task {
            tracing::error!(%error, "could not read the clipboard");
        }
    }

    /// Files dragged over the window go with the next message, as they do
    /// in Discord's client: all of them, or none and why. Those over the
    /// thread beside the channel go with its message.
    fn drag(&mut self, window: &YapfastWindow, drag: Drag) {
        let state = &self.app.state;
        match drag {
            Drag::Entered => {
                self.drag_over = true;
                window.set_drop_refusal(SharedString::default());
                self.show_drop_target(window);
            }
            Drag::Moved { x, y } => {
                // From the platform's logical pixels to the zoomed window's.
                let platform = window
                    .window()
                    .with_winit_window(|winit| winit.scale_factor())
                    .unwrap_or(1.);
                let zoom = f64::from(window.window().scale_factor()) / platform;
                let aside = window.invoke_over_aside((x / zoom) as f32, (y / zoom) as f32);
                if aside != window.get_drop_aside() {
                    window.set_drop_aside(aside);
                    if self.drag_over {
                        self.show_drop_target(window);
                    }
                }
            }
            Drag::Left => {
                self.drag_over = false;
                window.set_dragging(false);
            }
            Drag::Dropped(paths) => {
                self.drag_over = false;
                window.set_dragging(false);
                let aside = window.get_drop_aside();
                let (can_attach, refusal) = if aside {
                    (state.aside_can_attach(), state.aside_drop_refusal(&paths))
                } else {
                    (state.can_attach(), state.drop_refusal(&paths))
                };
                let stage = if aside {
                    Action::StageAside
                } else {
                    Action::Stage
                };
                // Staging says why it can't, as in a message being edited.
                if !can_attach {
                    self.action(window, stage(paths));
                } else if let Some(refusal) = refusal {
                    window.set_drop_refusal(refusal.title().into());
                    window.set_drop_refusal_help(refusal.help().into());
                } else {
                    self.action(window, stage(paths));
                    if aside {
                        window.invoke_focus_aside_composer();
                    } else {
                        window.invoke_focus_composer();
                    }
                }
            }
        }
    }

    /// Where files dragged over the window would go, and whether they can.
    fn show_drop_target(&self, window: &YapfastWindow) {
        let state = &self.app.state;
        if window.get_drop_aside() {
            let destination = state.aside_room().map(|room| state.destination(room));
            window.set_drop_destination(destination.unwrap_or_default().into());
            window.set_dragging(state.aside_can_attach());
        } else {
            window.set_drop_destination(state.upload_destination().into());
            window.set_dragging(state.can_attach());
        }
    }

    /// Run a native file dialog on Slint's executor, replacing any open one.
    fn dialog(&mut self, task: impl std::future::Future<Output = ()> + 'static) {
        if let Some(previous) = self.dialog.take() {
            previous.abort();
        }
        match slint::spawn_local(task) {
            Ok(handle) => self.dialog = Some(handle),
            Err(error) => {
                tracing::error!(%error, "could not open the file dialog");
                self.app.state.notice = Some("Yapfast could not open a file dialog.".into());
            }
        }
    }

    fn stop_backend(&mut self) {
        if let Some(events) = self.events.take() {
            events.abort();
        }
        self.backend = None;
    }

    fn execute(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Start(start) => {
                    self.stop_backend();
                    self.app.remember_channels(self.layout.channels.clone());
                    let (backend, mut events) =
                        Backend::start(self.runtime.clone(), self.root.clone(), start);
                    self.backend = Some(backend);
                    let weak = self.weak.clone();
                    // Network work stays on Tokio. Only channel delivery runs on
                    // Slint's executor, which wakes on an event and never polls idle.
                    // Events still apply without a window, so the call and chat go on.
                    match slint::spawn_local(async move {
                        while let Some(event) = events.recv().await {
                            let Some(adapter) = weak.upgrade() else {
                                return;
                            };
                            let window = adapter.borrow().window.upgrade();
                            let mut adapter = adapter.borrow_mut();
                            let mut reach = adapter.apply(event);
                            // What else has arrived meanwhile applies too, before
                            // one refresh: a burst, such as a page of avatars,
                            // lays the window out once rather than per event.
                            for _ in 1..EVENT_BATCH {
                                match events.try_recv() {
                                    Ok(event) => reach = reach.and(adapter.apply(event)),
                                    Err(_) => break,
                                }
                            }
                            // Without a window, a call still rings, and the tray shows
                            // what's unread and offers Disconnect in a call.
                            match window {
                                Some(window) => {
                                    adapter.show(&window, reach);
                                    adapter.open_pending_link(&window);
                                }
                                None => adapter.ring(),
                            }
                            adapter.show_tray();
                        }
                        if let Some(adapter) = weak.upgrade() {
                            let window = adapter.borrow().window.upgrade();
                            let mut adapter = adapter.borrow_mut();
                            adapter.app.stopped();
                            adapter.backend = None;
                            if let Some(window) = window {
                                adapter.refresh(&window);
                            }
                        }
                    }) {
                        Ok(task) => self.events = Some(task),
                        Err(error) => {
                            tracing::error!(%error, "could not connect backend events to Slint");
                            self.backend = None;
                            self.app.stopped();
                        }
                    }
                }
                Effect::PreviewCamera(filming) => {
                    self.preview_camera = None;
                    if let Some((camera, feed)) = filming {
                        let sink = Box::new(feed.mirrored_sink());
                        match yapfast_core::camera::Camera::open(camera.as_deref(), sink) {
                            Ok(camera) => self.preview_camera = Some(camera),
                            Err(error) => self.app.state.voice.error = Some(error.to_string()),
                        }
                    }
                }
                Effect::ListDevices => {
                    let weak = self.weak.clone();
                    // Listing starts the platform's audio service, which can block.
                    let listing = self.runtime.spawn_blocking(|| {
                        voice::list_devices().map_err(|error| error.to_string())
                    });
                    let task = slint::spawn_local(async move {
                        let devices = listing
                            .await
                            .unwrap_or_else(|_| Err("Could not list audio devices.".into()));
                        if let Some(adapter) = weak.upgrade() {
                            let window = adapter.borrow().window.upgrade();
                            let mut adapter = adapter.borrow_mut();
                            adapter.app.preview_devices(devices);
                            if let Some(window) = window {
                                adapter.refresh(&window);
                            }
                        }
                    });
                    if let Err(error) = task {
                        tracing::error!(%error, "could not list audio devices for the preview");
                    }
                }
                Effect::LinkDiscord => {
                    #[cfg(feature = "discord")]
                    self.link_discord(None);
                }
                #[cfg_attr(not(feature = "discord"), allow(unused_variables))]
                Effect::LinkDiscordToken(token) => {
                    #[cfg(feature = "discord")]
                    self.link_discord(Some(token));
                }
                Effect::OpenPage(page) => browser::open(&page),
                Effect::CopyPicture(picture) => crate::copy::picture(picture),
                Effect::StopDiscordLink => {
                    #[cfg(feature = "discord")]
                    {
                        self.linking = None;
                        self.link_run += 1;
                    }
                }
                Effect::Notify(alert) => {
                    // A message sounds unless it's in the conversation in front;
                    // a call rings instead.
                    let reading =
                        FOCUSED.get() && self.app.state.selected.as_ref() == Some(&alert.room);
                    if !alert.call && !reading {
                        self.sound(if alert.mention {
                            Sound::Mention
                        } else {
                            Sound::Message
                        });
                    }
                    // Only while Yapfast isn't in front: there the lists show what's new.
                    if !FOCUSED.get()
                        && !alert.call
                        && self.layout.flash_taskbar.unwrap_or(true)
                        && let Some(window) = self.window.upgrade()
                    {
                        attention(&window, true);
                    }
                    if self.layout.notifications.unwrap_or(true) && !FOCUSED.get() {
                        let style = crate::notify::Style {
                            text: self.layout.notification_text.unwrap_or(true),
                        };
                        self.notifier.show(alert, style);
                    }
                }
                Effect::Sound(sound) => self.sound(sound),
                Effect::Backend(Command::Typing { .. })
                    if !self.layout.share_typing.unwrap_or(true) => {}
                Effect::Backend(command) => {
                    let result = match &self.backend {
                        Some(backend) => backend.send(command),
                        None => Err(command),
                    };
                    if let Err(command) = result {
                        self.app.backend_failed(&command);
                    }
                }
            }
        }
    }

    /// Starts linking a Discord account, by QR code or with a pasted token,
    /// replacing any linking under way. The work runs on Tokio; its progress
    /// reaches the app on Slint's executor.
    #[cfg(feature = "discord")]
    fn link_discord(&mut self, token: Option<String>) {
        self.link_run += 1;
        let run = self.link_run;
        let (task, mut progress) = yapfast_core::discord_link::start(&self.runtime, token);
        self.linking = Some(task);
        let weak = self.weak.clone();
        let task = slint::spawn_local(async move {
            while let Some(step) = progress.recv().await {
                let Some(adapter) = weak.upgrade() else {
                    return;
                };
                let window = adapter.borrow().window.upgrade();
                let mut adapter = adapter.borrow_mut();
                if adapter.link_run != run {
                    return;
                }
                let effects = adapter.app.linking(step);
                adapter.execute(effects);
                if let Some(window) = window {
                    adapter.refresh(&window);
                }
            }
        });
        if let Err(error) = task {
            tracing::error!(%error, "could not follow linking Discord");
            self.linking = None;
        }
    }

    /// Plays one of Yapfast's sounds as the person set them. While deafened, only
    /// deafening's own sounds play.
    fn sound(&self, sound: Sound) {
        let voice = &self.app.state.voice;
        if voice.active() && voice.deafened && !matches!(sound, Sound::Deafen | Sound::Undeafen) {
            return;
        }
        self.play(sound);
    }

    /// Out of a call, mutes or deafens the next one, or stops doing so, with
    /// the sound it makes in a call. Unmuting undeafens, as in a call.
    fn toggle_on_join(&mut self, window: &YapfastWindow, deafen: bool) {
        let layout = &mut self.layout;
        let sound = if deafen {
            layout.deafen_on_join = !layout.deafen_on_join;
            if layout.deafen_on_join {
                Sound::Deafen
            } else {
                Sound::Undeafen
            }
        } else if layout.deafen_on_join {
            (layout.deafen_on_join, layout.unmute_on_join) = (false, Some(true));
            Sound::Undeafen
        } else {
            let unmute = !layout.unmute_on_join.unwrap_or(true);
            layout.unmute_on_join = Some(unmute);
            if unmute { Sound::Unmute } else { Sound::Mute }
        };
        window.set_unmute_on_join(layout.unmute_on_join.unwrap_or(true));
        window.set_deafen_on_join(layout.deafen_on_join);
        layout.write(&self.root);
        self.play(sound);
    }

    /// Plays a sound on the call's speakers, at the volume set for it.
    fn play(&self, sound: Sound) {
        sounds::play(
            self.layout.sound_set,
            sound,
            self.layout.sound_gain(sound),
            self.app.state.voice.speaker.as_deref(),
        );
    }

    /// Opens a Discord link handed to this start once Discord is online
    /// and no dialog is open. Any app or web page can hand Yapfast a
    /// link, so one never takes an open dialog's place: an Authorize or
    /// Join click meant for what the dialog showed would otherwise land
    /// on what the link opened instead. The opening is scheduled, as its
    /// callback borrows this adapter.
    fn open_pending_link(&mut self, window: &YapfastWindow) {
        if !self.app.state.discord_online || self.app.state.dialog_open() {
            return;
        }
        let Some(link) = self.pending_link.take() else {
            return;
        };
        let window = window.as_weak();
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(window) = window.upgrade() {
                window.invoke_open_link(link.into());
            }
        });
    }

    fn action(&mut self, window: &YapfastWindow, action: Action) {
        // A keystroke in the composer changes only the composer, and one in
        // the profile editor only the editor; the rest of the window
        // stays as it is.
        let composing = matches!(action, Action::SetDraft(_) | Action::MentionQuery(_));
        let bot_input = matches!(
            action,
            Action::SetModalField { .. } | Action::SearchSelect(_) | Action::ToggleSelect(_)
        );
        let profiling = matches!(action, Action::WriteProfile { .. });
        // Typing on Roles changes only the settings, too.
        let setting = matches!(
            action,
            Action::WriteServerSetting { .. }
                | Action::WriteMfa(_)
                | Action::Roles(
                    RoleInput::Name(_)
                        | RoleInput::Search(_)
                        | RoleInput::PermissionSearch(_)
                        | RoleInput::MemberSearch(_)
                        | RoleInput::FindCandidates(_)
                )
        );
        // As is typing in a channel's settings.
        let channel_setting = matches!(
            action,
            Action::ChannelSettings(
                ChannelInput::Name(_) | ChannelInput::Topic(_) | ChannelInput::FindMembers(_)
            )
        );
        // Pictures asked for change nothing on screen until they come.
        let quiet = matches!(
            action,
            Action::LoadPicturesFirst(_)
                | Action::KeepEmojiPictures(_)
                | Action::CacheEmojiPictures
        );
        let effects = self.app.dispatch(action);
        self.execute(effects);
        if quiet {
            return;
        }
        if channel_setting {
            self.show_channel_settings(window);
        } else if composing {
            self.show_composer(window);
        } else if bot_input {
            self.show_bot_interactions(window);
        } else if profiling {
            self.show_profile_editor(window);
        } else if setting {
            self.show_server_settings(window);
            self.show_mfa(window);
        } else {
            self.refresh(window);
        }
        // A handed link may have waited for the dialog this closed.
        self.open_pending_link(window);
    }

    /// Applies a backend event; the caller shows what it can have changed after.
    fn apply(&mut self, event: Event) -> Reach {
        let reach = Reach::of(&event);
        let effects = self.app.apply(event);
        // What it set off, such as opening a DM that just started, can change
        // anything; a sound changes nothing on screen.
        let reach = if effects
            .iter()
            .all(|effect| matches!(effect, Effect::Sound(_)))
        {
            reach
        } else {
            Reach::EVERYTHING
        };
        self.execute(effects);
        reach
    }
}

/// How long a layout change waits for the next before `layout.json` is
/// written, so changes that come together write the file once.
const LAYOUT_WRITE: Duration = Duration::from_millis(1000);

impl Adapter {
    /// Writes the layout once changes to it have stopped for [`LAYOUT_WRITE`]:
    /// each change starts the wait over. Dropping the adapter writes one
    /// still waiting.
    fn write_layout_soon(&mut self) {
        self.layout_dirty = true;
        let weak = self.weak.clone();
        self.layout_due
            .start(slint::TimerMode::SingleShot, LAYOUT_WRITE, move || {
                if let Some(adapter) = weak.upgrade() {
                    adapter.borrow_mut().flush_layout();
                }
            });
    }

    /// Writes a layout change still waiting, at once.
    fn flush_layout(&mut self) {
        self.layout_due.stop();
        if std::mem::take(&mut self.layout_dirty) {
            self.layout.write(&self.root);
        }
    }
}

impl Drop for Adapter {
    fn drop(&mut self) {
        self.flush_layout();
        self.stop_backend();
    }
}

/// The DM calls ringing this account: those it isn't in already.
fn ringing(
    state: &yapfast_core::application::AppState,
) -> impl Iterator<Item = &yapfast_core::chat::ChatRoom> {
    let voice = &state.voice;
    state
        .rooms
        .iter()
        .map(std::sync::Arc::as_ref)
        .filter(|room| room.incoming && !(voice.active() && voice.room.as_ref() == Some(&room.id)))
}

/// Shows a DM call ringing this account in another conversation over
/// everything.
fn show_incoming(
    window: &YapfastWindow,
    state: &yapfast_core::application::AppState,
    pictures: &mut HashMap<String, slint::Image>,
) {
    // The open DM's own call answers in its panel.
    let shown = ringing(state).find(|room| state.selected.as_ref() != Some(&room.id));
    window.set_incoming_open(shown.is_some());
    if let Some(room) = shown {
        window.set_incoming_room(room.id.as_str().into());
        window.set_incoming_name(room.name.as_str().into());
        window.set_incoming_detail(
            if room.person.is_some() || room.people.is_empty() {
                "Incoming call".into()
            } else {
                format!("{} calling", who_is_here(&room.people))
            }
            .into(),
        );
        window.set_incoming_initial(initial(&room.name));
        window.set_incoming_tint(tint(&room.id));
        window.set_incoming_picture(cached_picture(pictures, state, room.avatar.as_deref()));
    }
}

fn who_is_here(people: &[yapfast_core::chat::VoicePerson]) -> String {
    let names: Vec<&str> = people.iter().map(|person| person.name.as_str()).collect();
    match names.as_slice() {
        [one] => format!("{one} is"),
        [first, second] => format!("{first} and {second} are"),
        [first, second, third] => format!("{first}, {second} and {third} are"),
        [first, second, rest @ ..] => format!("{first}, {second} and {} others are", rest.len()),
        [] => String::new(),
    }
}

fn mention_badge(room: &yapfast_core::chat::ChatRoom) -> SharedString {
    count_badge(room.mentions)
}

/// A count as a badge shows it: nothing for none, and at most "99+".
fn count_badge(count: u64) -> SharedString {
    match count {
        0 => SharedString::default(),
        count if count > 99 => "99+".into(),
        count => count.to_string().into(),
    }
}

/// Avatar tints: mid tones that keep a white initial legible.
const TINTS: [(u8, u8, u8); 8] = [
    (0xc2, 0x55, 0x4d),
    (0xb8, 0x6e, 0x22),
    (0x3a, 0x93, 0x57),
    (0x2b, 0x8a, 0x98),
    (0x4a, 0x74, 0xc9),
    (0x7a, 0x5f, 0xc9),
    (0xb3, 0x4f, 0x8c),
    (0x64, 0x6b, 0x7a),
];

/// The same name always gets the same tint. People are keyed by display name,
/// which is all a reply quote carries, so a quote matches its author's avatar.
fn tint(key: &str) -> slint::Color {
    let hash = key
        .to_lowercase()
        .bytes()
        .fold(0x811c_9dc5_u32, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
        });
    // FNV-1a mixes its high bits far better than its low ones.
    let (red, green, blue) = TINTS[(hash >> 20) as usize % TINTS.len()];
    slint::Color::from_rgb_u8(red, green, blue)
}

fn display_part(name: &str) -> &str {
    name.strip_prefix('@')
        .and_then(|rest| rest.split_once(':'))
        .map_or(name, |(local, _)| local)
}

/// A server icon shows the first letter of up to three words, as in "Co-op crew" → "CC", in capitals like Discord.
fn initials(name: &str) -> SharedString {
    let mut words = name
        .split_whitespace()
        .filter_map(|word| word.chars().next());
    let mut text: String = words
        .next()
        .map(|first| first.to_uppercase().collect())
        .unwrap_or_else(|| "?".into());
    text.extend(words.take(2).flat_map(char::to_uppercase));
    text.into()
}

use yapfast_core::chat::file_size;

/// An attachment's place in a message, the attachment, and its preview once
/// it has arrived.
type Preview<'a> = (
    usize,
    &'a yapfast_core::chat::Attachment,
    Option<&'a Arc<Picture>>,
);

/// The size Discord shows stickers at, as their pictures are made.
const STICKER_SIZE: i32 = 160;

/// The line under a Discord DM in the list: the person's custom status or
/// what they're doing on one line, or a group's size; and the address of the
/// picture of the status's custom emoji.
fn dm_detail(room: &yapfast_core::chat::ChatRoom) -> (SharedString, SharedString) {
    if !room.direct || room.network != Network::Discord {
        return Default::default();
    }
    if room.person.is_none() {
        return (
            format!("{} members", room.members).into(),
            SharedString::default(),
        );
    }
    activity_line(room.activity.as_deref())
}

/// A count with its noun, such as "1 member" or "12 members".
fn count_of(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// `count` with its thousands marked, as Discord writes "3,999".
fn grouped(count: u64) -> String {
    let digits = count.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (at, digit) in digits.char_indices() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// A status on one line, as Discord shows it under a name; the profile keeps
/// its own line breaks.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How the UI names a presence; empty when there's none.
/// `new`, unless it has the same rows as `old`, which then stays. Slint tells
/// lists apart by identity, so a row or profile holding an equal but new list
/// would count as changed and be laid out again, losing its hover and focus.
fn same_list<T: Clone + PartialEq + 'static>(old: &ModelRc<T>, new: ModelRc<T>) -> ModelRc<T> {
    let equal = old.row_count() == new.row_count() && old.iter().eq(new.iter());
    if equal { old.clone() } else { new }
}

/// The width role chips fill in the profile card and the full profile: the
/// cards' widths less their padding.
const PROFILE_CARD_WIDTH: f32 = 300. - 2. * 16.;
const PROFILE_LARGE_WIDTH: f32 = 600. - 2. * 24.;

/// How many emoji the composer suggests for a `:name` word.
const EMOJI_SUGGESTIONS: usize = 10;

/// The link that shows a blocked message, followed by its item.
const BLOCKED_LINK: &str = "yapfast:blocked/";

fn initial(text: &str) -> SharedString {
    text.chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string()
        .into()
}

/// WebRTC names its default entry "default: Device"; show it as "Default (Device)".
/// How a PulseAudio device list's default entry's id starts: the default
/// device's name, as WebRTC lists it, after `default: `.
const DEFAULT_DEVICE: &str = "name:default: ";

fn device_label(name: &str) -> SharedString {
    match name.strip_prefix("default: ") {
        Some(device) => format!("Default ({device})").into(),
        None => name.into(),
    }
}

/// A device picker's rows, and the chosen one's index. Without a choice,
/// voice uses WebRTC's default device, which it lists first. A chosen device
/// that isn't connected, such as Bluetooth headphones that went away, shows
/// last as not connected, by the name it was last listed with, rather than
/// as no device or as another one. `names` keeps those names. The default
/// entry's id names the device that was the default, and the default it
/// plays on may have changed since, so it stays the default entry.
fn device_rows(
    devices: &[voice::Device],
    chosen: Option<&str>,
    names: &mut HashMap<String, SharedString>,
) -> (Vec<SharedString>, i32) {
    let mut rows: Vec<SharedString> = devices
        .iter()
        .map(|device| device_label(&device.name))
        .collect();
    for (device, row) in devices.iter().zip(&rows) {
        if names.get(&device.id) != Some(row) {
            names.insert(device.id.clone(), row.clone());
        }
    }
    let index = match chosen {
        None => (!devices.is_empty()).then_some(0),
        Some(chosen) => Some(
            devices
                .iter()
                .position(|device| device.id == chosen)
                .or_else(|| {
                    let default = |id: &str| id.starts_with(DEFAULT_DEVICE);
                    default(chosen)
                        .then(|| devices.iter().position(|device| default(&device.id)))
                        .flatten()
                })
                .unwrap_or_else(|| {
                    let name = names
                        .get(chosen)
                        .cloned()
                        .or_else(|| chosen.strip_prefix(voice::NAMED).map(device_label))
                        .unwrap_or_else(|| "Chosen device".into());
                    rows.push(format!("{name} (not connected)").into());
                    rows.len() - 1
                }),
        ),
    };
    let index = index
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or(-1);
    (rows, index)
}

/// Repaints the popup on top and the shadow around it, telling Slint they're
/// dirty: it repaints the regions it is told of otherwise, which a popup's
/// contents moving or resizing leaves short of the shadow they draw outside
/// it. A popup in a window of its own repaints the window whole.
fn repaint_popup(window: &YapfastWindow) {
    use i_slint_core::lengths::{LogicalPoint, LogicalRect, LogicalSize};
    use i_slint_core::window::{PopupWindowLocation, WindowInner};
    /// How far a popup's shadow reaches past it: its 24px blur and 8px drop.
    const SHADOW: f32 = 32.;
    let native = window.window();
    let inner = WindowInner::from_pub(native);
    let size = native.size().to_logical(native.scale_factor());
    let whole = LogicalRect::new(
        LogicalPoint::new(0., 0.),
        LogicalSize::new(size.width, size.height),
    );
    let dirty = inner
        .active_popups()
        .last()
        .and_then(|popup| match &popup.location {
            PopupWindowLocation::ChildWindow(place) => {
                let geometry = i_slint_core::item_tree::ItemTreeRc::borrow_pin(&popup.component)
                    .as_ref()
                    .item_geometry(0);
                Some(LogicalRect::new(*place, geometry.size).inflate(SHADOW, SHADOW))
            }
            PopupWindowLocation::TopLevel(_) => None,
        })
        .and_then(|popup| popup.intersection(&whole))
        .unwrap_or(whole);
    inner
        .window_adapter()
        .renderer()
        .mark_dirty_region(i_slint_core::partial_renderer::DirtyRegion::from(dirty));
    native.request_redraw();
}

fn update_model<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    let common = model.row_count().min(rows.len());
    for (index, row) in rows.iter().take(common).enumerate() {
        if model.row_data(index).as_ref() != Some(row) {
            model.set_row_data(index, row.clone());
        }
    }
    while model.row_count() > rows.len() {
        model.remove(model.row_count() - 1);
    }
    for row in rows.into_iter().skip(common) {
        model.push(row);
    }
}

/// Shows `rows` on the model's rows as on slots, each named by its key: a row
/// that a slot already shows stays in it, and is set only where it changed; one
/// new takes the slot of one gone. `slots` names the key each slot shows. So a
/// grid given the tiles in sight sets the slots of those that scrolled in, and
/// its elements stay, where a model shifted along would set every row, and a
/// list view would make the new ones and drop the old.
fn update_model_by_slot<K: Copy + Eq + std::hash::Hash, T: Clone + PartialEq + 'static>(
    model: &VecModel<T>,
    slots: &mut Vec<K>,
    rows: Vec<(K, T)>,
) {
    // A model set apart from the slots, as one emptied, starts them over.
    if slots.len() != model.row_count() {
        slots.clear();
        model.set_vec(Vec::new());
    }
    let wanted: HashMap<K, usize> = rows
        .iter()
        .enumerate()
        .map(|(index, (key, _))| (*key, index))
        .collect();
    let mut placed = vec![false; rows.len()];
    let mut free = Vec::new();
    for (slot, key) in slots.iter().enumerate() {
        match wanted.get(key) {
            Some(&index) if !placed[index] => {
                placed[index] = true;
                if model.row_data(slot).as_ref() != Some(&rows[index].1) {
                    model.set_row_data(slot, rows[index].1.clone());
                }
            }
            _ => free.push(slot),
        }
    }
    let mut free = free.into_iter();
    for ((key, row), placed) in rows.into_iter().zip(placed) {
        if placed {
            continue;
        }
        if let Some(slot) = free.next() {
            slots[slot] = key;
            model.set_row_data(slot, row);
        } else {
            slots.push(key);
            model.push(row);
        }
    }
    // Slots left over go from the end, the last ones moving into those left
    // empty before them.
    let mut left: std::collections::VecDeque<usize> = free.collect();
    while let Some(&hole) = left.front() {
        let last = slots.len() - 1;
        if left.back() == Some(&last) {
            left.pop_back();
        } else {
            slots[hole] = slots[last];
            if let Some(row) = model.row_data(last) {
                model.set_row_data(hole, row);
            }
            left.pop_front();
        }
        slots.pop();
        model.remove(last);
    }
}

/// Updates a list of rows that have ids, keeping each row in its place:
/// rows that came or went before or after the rest are inserted or removed,
/// so loading older messages above leaves the rows below untouched, and the
/// rest are set only where they changed.
fn update_model_by_id<T: Clone + PartialEq + 'static>(
    model: &VecModel<T>,
    rows: Vec<T>,
    id: impl Fn(&T) -> SharedString,
) {
    let shown: Vec<SharedString> = model.iter().map(|row| id(&row)).collect();
    let same = |old: usize, new: usize| shown[old] == id(&rows[new]);
    let common = shown.len().min(rows.len());
    let before = (0..common).take_while(|&index| same(index, index)).count();
    let after = (0..common - before)
        .take_while(|&back| same(shown.len() - 1 - back, rows.len() - 1 - back))
        .count();
    for _ in before..shown.len() - after {
        model.remove(before);
    }
    let added = rows.len() - after;
    for (index, row) in rows.into_iter().enumerate() {
        if (before..added).contains(&index) {
            model.insert(index, row);
        } else if model.row_data(index).as_ref() != Some(&row) {
            model.set_row_data(index, row);
        }
    }
}

#[cfg(test)]
mod tests;
