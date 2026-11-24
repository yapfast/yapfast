//! Go Live's menu: the screens, windows and cameras to share, then a row
//! for each of the stream's settings (its audio, resolution and frame rate)
//! that names what's chosen and opens its choices beside it, as a context
//! menu's submenu, so the menu keeps its size and place.

use crate::widgets::scroll_view;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use iced::{
    Alignment, Border, Element, Length, Padding, Task,
    advanced::image::Handle,
    widget::{button, column, container, mouse_area, operation, row, text},
};
use yapfast_core::{
    application::Action,
    stream_worker::Source,
    voice::{Command, Resolution, StreamAudio, StreamQuality},
};

use crate::call::{Event, Popup};
use crate::{
    app::{App, Message, NOBODY},
    controls::{self, Check, Item, Light, heading},
    fonts::BOLD,
    menus::{PAD, SUB_WIDTH},
    pictures::Icon,
};

pub(super) const SHARE_WIDE: f32 = 496.0;

pub(super) const SHARE_NARROW: f32 = 240.0;

/// A preview in the share menu, in pixels.
const GLANCE: (u32, u32) = (240, 135);

/// The tallest the sources get before they scroll: two rows of screens
/// or windows and the top of a third.
const SOURCES_TALLEST: f32 = 400.0;
/// The tallest a setting's choices get before they scroll, as with many
/// windows to take the audio from.
const SIDE_TALLEST: f32 = 400.0;
/// The line between the sources and the settings, as a menu's separator.
const SEPARATOR: f32 = 17.0;
/// Between two of the settings' rows.
const ROW_GAP: f32 = 2.0;
/// Higher quality's row, with its second line.
const TOGGLE: f32 = 44.0;
/// The settings' rows among the menu's parts: the sources, the line, the
/// rows, the button.
const SETTINGS: usize = 2;

/// A setting of the stream whose row opens its choices beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Audio,
    Resolution,
    FrameRate,
}

const RESOLUTIONS: [(Resolution, &str); 3] = [
    (Resolution::P720, "720p"),
    (Resolution::P1080, "1080p"),
    (Resolution::Source, "Source"),
];
const FRAME_RATES: [u32; 3] = [15, 30, 60];

/// The share menu's sources and choices.
pub struct Share {
    /// The screens with their pictures once taken, the windows, the
    /// cameras; then every source in the menu's order (screens, windows,
    /// the system's dialog, cameras).
    pub screens: Vec<(String, Option<Handle>)>,
    pub windows: Vec<(String, Option<Handle>)>,
    pub cameras: Vec<String>,
    pub sources: Vec<yapfast_core::stream_worker::Source>,
    /// The system's dialog picks a screen or window.
    pub dialog: bool,
    pub pick: Option<usize>,
    pub quality: StreamQuality,
    /// It switches what's shared, at the stream's quality.
    pub switching: bool,
    count: u64,
    preview_stop: Arc<AtomicBool>,
    audio: StreamAudio,
    audio_options: Vec<(StreamAudio, String)>,
    /// The setting whose choices show beside its row.
    pub side: Option<Setting>,
}

impl Share {
    /// The settings with a row, in the rows' order: a stream that's live
    /// keeps its quality, so only its audio changes.
    fn settings(&self) -> &'static [Setting] {
        if self.switching {
            &[Setting::Audio]
        } else {
            &[Setting::Audio, Setting::Resolution, Setting::FrameRate]
        }
    }

    /// How tall the settings' rows are together, the line over them too.
    fn settings_height(&self) -> f32 {
        let rows = self.settings().len() as f32;
        let toggle = if self.switching {
            0.0
        } else {
            ROW_GAP + TOGGLE
        };
        SEPARATOR + rows * controls::ITEM + (rows - 1.0) * ROW_GAP + toggle
    }

    /// What `setting` is set to, as its row names it.
    fn value(&self, setting: Setting) -> std::borrow::Cow<'_, str> {
        match setting {
            Setting::Audio => self
                .audio_options
                .iter()
                .find(|(audio, _)| *audio == self.audio)
                .map_or("", |(_, title)| title.as_str())
                .into(),
            Setting::Resolution => RESOLUTIONS
                .iter()
                .find(|(resolution, _)| *resolution == self.quality.resolution)
                .map_or("", |(_, label)| label)
                .into(),
            Setting::FrameRate => format!("{} fps", self.quality.fps).into(),
        }
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        self.preview_stop.store(true, Ordering::Relaxed);
    }
}

/// What's listed to share, as it comes from the thread that lists it.
pub struct Listing(pub Vec<yapfast_core::stream_worker::Shareable>);

impl std::fmt::Debug for Listing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} shareable", self.0.len())
    }
}

/// What's done in the share menu.
#[derive(Clone, Debug)]
pub enum ShareEvent {
    /// The sources listed, by the listing's count.
    Listed(u64, Arc<Listing>),
    /// A picture from this listing, keyed by source kind and id.
    Picture(u64, Source, Handle),
    Pick(usize),
    /// A setting's choices open beside its row, or close.
    Side(Option<Setting>),
    Audio(StreamAudio),
    Resolution(Resolution),
    Fps(u32),
    HighQuality,
    Go,
}

/// Lists what can be shared beside the window, as listing starts a
/// capturer for screens and one for windows and looks at each camera; the
/// menu opens when it's done.
pub(super) fn list_sources(count: u64) -> Task<Message> {
    Task::done(()).then(move |()| {
        let (sender, receiver) = iced::futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            sender.send(yapfast_core::stream_worker::shareable()).ok();
        });
        Task::perform(receiver, move |listed| {
            Message::Call(Event::Share(ShareEvent::Listed(
                count,
                Arc::new(Listing(listed.unwrap_or_default())),
            )))
        })
    })
}

/// Pictures of the screens and windows to share, from a worker, so capturing them
/// takes memory only until they're in.
fn glance(
    count: u64,
    screens: Vec<u64>,
    windows: Vec<u64>,
    stop: Arc<AtomicBool>,
) -> Task<Message> {
    if screens.is_empty() && windows.is_empty() {
        return Task::none();
    }
    // The worker starts as the task runs, not as it's made.
    Task::done((screens, windows, stop)).then(move |(screens, windows, stop)| {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            use yapfast_core::stream_worker::{Report, Worker};
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let (_worker, mut reports) = match Worker::glance(screens, windows, GLANCE.0, GLANCE.1)
            {
                Ok(started) => started,
                Err(error) => {
                    tracing::warn!("no pictures of the sources to share: {error:#}");
                    return;
                }
            };
            while !stop.load(Ordering::Relaxed) {
                let report = match reports.try_recv() {
                    Ok(report) => report,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                        std::thread::sleep(Duration::from_millis(25));
                        continue;
                    }
                };
                match report {
                    Report::Screen {
                        id,
                        window,
                        width,
                        height,
                        rgba,
                    } => {
                        let handle = Handle::from_rgba(width, height, rgba);
                        let source = if window {
                            Source::Window(id)
                        } else {
                            Source::Display(id)
                        };
                        if sender.unbounded_send((source, handle)).is_err() {
                            break;
                        }
                    }
                    Report::Ended { error } => {
                        if let Some(error) = error {
                            tracing::warn!("no pictures of the sources to share: {error}");
                        }
                        break;
                    }
                    _ => {}
                }
            }
        });
        Task::run(receiver, move |(source, handle)| {
            Message::Call(Event::Share(ShareEvent::Picture(count, source, handle)))
        })
    })
}

/// Opens the menu with what was listed. `applications` is whether the
/// stream's audio can be one application's, picked by its window: on
/// Windows.
pub(crate) fn list(
    app: &mut App,
    count: u64,
    listing: &Listing,
    applications: bool,
) -> Task<Message> {
    // Only the listing the menu waits for, in a call.
    if app.call.share_listing != Some(count) || !app.client.state().voice.active() {
        return Task::none();
    }
    app.call.share_listing = None;
    let (mut screens, mut windows, mut cameras) = (vec![], vec![], vec![]);
    for listed in &listing.0 {
        match listed.source {
            Source::Display(_) => screens.push(listed),
            Source::Camera(_) => cameras.push(listed),
            Source::Window(_) => windows.push(listed),
            _ => {}
        }
    }
    let screen_ids = screens
        .iter()
        .filter_map(|listed| match listed.source {
            Source::Display(id) => Some(id),
            _ => None,
        })
        .collect();
    let window_ids = windows
        .iter()
        .filter_map(|listed| match listed.source {
            Source::Window(id) => Some(id),
            _ => None,
        })
        .collect();
    // Where the system's dialog picks a screen or window, the
    // menu's first choice asks it, before the cameras.
    let dialog = yapfast_core::stream_worker::portal_picks();
    let sources = screens
        .iter()
        .chain(&windows)
        .map(|listed| listed.source.clone())
        .chain(dialog.then_some(Source::Screen))
        .chain(cameras.iter().map(|listed| listed.source.clone()))
        .collect();
    let voice = &app.client.state().voice;
    let switching = voice.sharing.is_some();
    let mut audio = voice.stream_audio;
    let mut audio_options = vec![
        (StreamAudio::Off, "No audio".into()),
        (StreamAudio::All, "All applications".into()),
    ];
    if applications {
        audio_options.extend(windows.iter().filter_map(|listed| match listed.source {
            Source::Window(id) => Some((StreamAudio::Window(id), listed.title.clone())),
            _ => None,
        }));
    }
    // The application chosen earlier, in this run or one before, is
    // found again by a window it has now, unless one of its windows is
    // the choice already. Not while live: a stream's audio changes only
    // when asked.
    let chosen = matches!(audio, StreamAudio::Window(_))
        && audio_options.iter().any(|(option, _)| *option == audio);
    let found = voice
        .stream_app
        .as_ref()
        .filter(|_| applications && !switching && !chosen)
        .and_then(|program| {
            windows.iter().find_map(|listed| match listed.source {
                Source::Window(id) if listed.program.as_ref() == Some(program) => {
                    Some(StreamAudio::Window(id))
                }
                _ => None,
            })
        });
    if let Some(found) = found {
        audio = found;
    }
    if !audio_options.iter().any(|(option, _)| *option == audio) {
        audio_options.push((audio, "Unavailable application".into()));
    }
    let preview_stop = Arc::new(AtomicBool::new(false));
    let share = Share {
        screens: screens
            .iter()
            .map(|listed| (listed.title.clone(), None))
            .collect(),
        windows: windows
            .iter()
            .map(|listed| (listed.title.clone(), None))
            .collect(),
        cameras: cameras.iter().map(|listed| listed.title.clone()).collect(),
        sources,
        dialog,
        pick: (!switching).then_some(0),
        quality: voice.quality,
        switching,
        count,
        preview_stop: preview_stop.clone(),
        audio,
        audio_options,
        side: None,
    };
    app.call.share = Some(share);
    app.call.popup = Some(Popup::Share);
    let mut tasks = vec![
        glance(count, screen_ids, window_ids, preview_stop),
        operation::focus(NOBODY),
    ];
    if let Some(found) = found {
        app.client
            .dispatch(Action::Voice(Command::StreamAudio(found)));
        tasks.push(app.sync());
    }
    Task::batch(tasks)
}

pub(in crate::call) fn share_event(app: &mut App, event: ShareEvent) -> Task<Message> {
    match event {
        ShareEvent::Listed(count, listing) => {
            return list(app, count, &listing, cfg!(windows));
        }
        ShareEvent::Picture(count, source, handle) => {
            if let Some(share) = &mut app.call.share
                && share.count == count
                && let Some(at) = share.sources.iter().position(|listed| *listed == source)
            {
                let row = match source {
                    Source::Display(_) => share.screens.get_mut(at),
                    Source::Window(_) => share.windows.get_mut(at - share.screens.len()),
                    _ => None,
                };
                if let Some((_, picture)) = row {
                    *picture = Some(handle);
                }
            }
        }
        ShareEvent::Pick(pick) => {
            if let Some(share) = &mut app.call.share {
                share.pick = Some(pick);
                share.side = None;
            }
        }
        ShareEvent::Side(side) => {
            if let Some(share) = &mut app.call.share {
                share.side = side.filter(|setting| share.settings().contains(setting));
            }
        }
        // A choice made beside a row closes its list. The audio's is
        // the call's at once, live or not, and remembered.
        ShareEvent::Audio(audio) => {
            if let Some(share) = &mut app.call.share {
                share.audio = audio;
                share.side = None;
                app.client
                    .dispatch(Action::Voice(Command::StreamAudio(audio)));
                return app.sync();
            }
        }
        ShareEvent::Resolution(resolution) => {
            if let Some(share) = &mut app.call.share {
                share.quality.resolution = resolution;
                share.side = None;
            }
        }
        ShareEvent::Fps(fps) => {
            if let Some(share) = &mut app.call.share {
                share.quality.fps = fps;
                share.side = None;
            }
        }
        ShareEvent::HighQuality => {
            if let Some(share) = &mut app.call.share {
                share.quality.high_quality = !share.quality.high_quality;
                share.side = None;
            }
        }
        ShareEvent::Go => {
            let Some(share) = app.call.share.take() else {
                return Task::none();
            };
            let picked = share.pick.and_then(|at| share.sources.get(at)).cloned();
            let command = if share.switching {
                picked.map(|source| {
                    yapfast_core::stream_worker::pick(source);
                    Command::SwitchLive
                })
            } else {
                yapfast_core::stream_worker::pick(
                    picked.unwrap_or(yapfast_core::stream_worker::Source::Screen),
                );
                Some(Command::GoLive(share.quality))
            };
            crate::call::popups::close_popup(app);
            if let Some(command) = command {
                app.client.dispatch(Action::Voice(command));
            }
            return Task::batch([app.sync(), crate::composer::refocus(app)]);
        }
    }
    Task::none()
}

/// The share menu: the sources, which scroll, over the settings' rows and
/// the Go Live or Switch button, which stay in view.
pub(super) fn share_menu<'a>(
    ctx: crate::ctx::Ctx<'a>,
    share: &'a Share,
    width: f32,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let pick = |index: usize| Message::Call(Event::Share(ShareEvent::Pick(index)));
    let checked = |index: usize| {
        if share.pick == Some(index) {
            Check::Current
        } else {
            Check::None
        }
    };
    let mut choices = column![].spacing(2);
    let card_width = (width - 16.0) / 2.0;
    for (label, sources, first) in [
        ("SCREENS", &share.screens, 0),
        ("WINDOWS", &share.windows, share.screens.len()),
    ] {
        if sources.is_empty() {
            continue;
        }
        choices = choices.push(heading(theme, label, if first == 0 { 4.0 } else { 8.0 }));
        let mut rows = column![].spacing(4);
        for (row_index, pair) in sources.chunks(2).enumerate() {
            let mut cards = row![].spacing(4);
            for (index, (title, picture)) in pair.iter().enumerate() {
                let at = first + row_index * 2 + index;
                cards = cards.push(screen_card(
                    ctx,
                    title,
                    picture.as_ref(),
                    card_width,
                    share.pick == Some(at),
                    pick(at),
                ));
            }
            rows = rows.push(container(cards).center_x(Length::Fill));
        }
        choices = choices.push(rows);
    }
    let dialog_index = share.screens.len() + share.windows.len();
    if share.dialog {
        choices = choices
            .push(heading(theme, "SCREEN", 4.0))
            .push(crate::call::popups::popup_row(
                ctx,
                "Screen or window",
                Some("Picked in the system's dialog".into()),
                None,
                checked(dialog_index),
                Some(pick(dialog_index)),
            ));
    }
    if !share.cameras.is_empty() {
        let top = if dialog_index > 0 || share.dialog {
            8.0
        } else {
            4.0
        };
        choices = choices.push(heading(theme, "CAMERAS", top));
        let first = dialog_index + usize::from(share.dialog);
        for (index, title) in share.cameras.iter().enumerate() {
            let at = first + index;
            choices = choices.push(crate::call::popups::popup_row(
                ctx,
                title.as_str(),
                None,
                None,
                checked(at),
                Some(pick(at)),
            ));
        }
    }
    // Each setting in a row that names what's chosen; its choices open
    // beside it ([`side_list`]), so nothing here moves.
    let event = |event| Message::Call(Event::Share(event));
    let mut settings = column![].spacing(ROW_GAP);
    for &setting in share.settings() {
        let item = Item {
            label: match setting {
                Setting::Audio => "Audio",
                Setting::Resolution => "Resolution",
                Setting::FrameRate => "Frame rate",
            }
            .into(),
            icon: None,
            danger: false,
            slot: false,
            disabled: false,
            subtitle: None,
            check: Check::Submenu,
        };
        // Lit while its choices show, as a menu's row with its submenu.
        let light = if share.side == Some(setting) {
            Light::Lit(true)
        } else {
            Light::Pointer
        };
        let open = event(ShareEvent::Side(Some(setting)));
        settings = settings.push(
            mouse_area(
                ctx.look
                    .menu_item_valued(item, share.value(setting), light, open.clone()),
            )
            .on_enter(open),
        );
    }
    if !share.switching {
        settings = settings.push(
            mouse_area(crate::call::popups::popup_row(
                ctx,
                "Higher quality",
                Some("Sharper, at twice the CPU".into()),
                None,
                Check::Box(share.quality.high_quality),
                Some(event(ShareEvent::HighQuality)),
            ))
            .on_enter(event(ShareEvent::Side(None))),
        );
    }
    let label = if share.switching { "Switch" } else { "Go Live" };
    let start: Element<'a, Message> = controls::action(
        theme,
        label,
        controls::Kind::Primary,
        (!share.switching || share.pick.is_some())
            .then_some(Message::Call(Event::Share(ShareEvent::Go))),
    )
    .icon(
        ctx.look
            .icon(Icon::ScreenShare, 18, crate::theme::rgb(theme.on_accent)),
    )
    .wide(38.0)
    .into();
    // The tallest the sources get: from the window's top margin to the
    // control, less the room of what's under them.
    let spot = ctx.call.spot.get();
    let under = share.settings_height() + 52.0;
    let tallest = (spot.y - 6.0 - 8.0 - 12.0 - under).clamp(80.0, SOURCES_TALLEST);
    // The settings' rows are the part at [`SETTINGS`].
    column![
        container(scroll_view(choices).height(Length::Shrink)).max_height(tallest),
        controls::menu_separator(theme),
        settings,
        container(start).padding(Padding {
            top: 10.0,
            left: 4.0,
            right: 4.0,
            bottom: 4.0,
        }),
    ]
    .padding(6)
    .into()
}

/// The open setting's choices, the one chosen ticked, and the place of its
/// row among the menu's parts, which they stand beside. Built only while
/// they show.
pub(super) fn side_list<'a>(
    ctx: crate::ctx::Ctx<'a>,
    share: &'a Share,
) -> Option<(Element<'a, Message>, f32, Vec<usize>)> {
    let setting = share.side?;
    let row = share.settings().iter().position(|&row| row == setting)?;
    let theme = ctx.look.theme;
    let choice = |label: std::borrow::Cow<'a, str>, subtitle, chosen: bool, event| {
        let item = Item {
            label,
            icon: None,
            danger: false,
            slot: true,
            disabled: false,
            subtitle,
            check: if chosen { Check::Current } else { Check::None },
        };
        ctx.look
            .context_item(item, Light::Pointer, Message::Call(Event::Share(event)))
    };
    let mut rows = column![].padding(PAD);
    match setting {
        Setting::Audio => {
            if share.switching {
                rows = rows.push(
                    container(
                        text("Changes apply immediately")
                            .size(12)
                            .color(theme.secondary),
                    )
                    .padding(Padding {
                        top: 2.0,
                        right: 8.0,
                        bottom: 6.0,
                        left: 8.0,
                    }),
                );
            }
            for (audio, title) in &share.audio_options {
                let subtitle =
                    (*audio == StreamAudio::All).then_some("Excludes Yapfast's own audio".into());
                rows = rows.push(choice(
                    title.as_str().into(),
                    subtitle,
                    *audio == share.audio,
                    ShareEvent::Audio(*audio),
                ));
            }
        }
        Setting::Resolution => {
            for (resolution, label) in RESOLUTIONS {
                rows = rows.push(choice(
                    label.into(),
                    None,
                    resolution == share.quality.resolution,
                    ShareEvent::Resolution(resolution),
                ));
            }
        }
        Setting::FrameRate => {
            for fps in FRAME_RATES {
                rows = rows.push(choice(
                    format!("{fps} fps").into(),
                    None,
                    fps == share.quality.fps,
                    ShareEvent::Fps(fps),
                ));
            }
        }
    }
    let tallest = SIDE_TALLEST.min(ctx.look.window.height - 16.0);
    let list = container(scroll_view(rows).height(Length::Shrink))
        .max_height(tallest)
        .into();
    // As wide as a menu's submenu, whichever the setting: all three open
    // on the same side.
    Some((list, SUB_WIDTH, vec![SETTINGS, row]))
}

/// Closes the open setting's choices, as Escape does before it closes the
/// menu. Whether any were open.
pub fn close_side(app: &mut App) -> bool {
    app.call
        .share
        .as_mut()
        .is_some_and(|share| share.side.take().is_some())
}

/// A screen or window in the share menu: its picture, once taken, over its name.
pub(super) fn screen_card<'a>(
    ctx: crate::ctx::Ctx<'a>,
    title: &'a str,
    picture: Option<&Handle>,
    width: f32,
    checked: bool,
    press: Message,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let inner = width - 8.0;
    let height = inner * 9.0 / 16.0;
    let shown: Element<'a, Message> = match picture {
        Some(handle) => iced::widget::image(handle.clone())
            .width(inner)
            .height(height)
            .content_fit(iced::ContentFit::Contain)
            .into(),
        None => container(ctx.look.glyph(Icon::ScreenShare, 24, theme.faint))
            .center(Length::Fill)
            .into(),
    };
    let (sunken, edge) = (
        theme.sunken,
        if checked { theme.accent } else { theme.line },
    );
    let frame = container(shown)
        .width(inner)
        .height(height)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(sunken.into()),
            border: Border {
                color: edge,
                width: if checked { 2.0 } else { 1.0 },
                radius: 4.0.into(),
            },
            ..Default::default()
        });
    let (ink, lit_ink) = if checked {
        (theme.text, theme.text)
    } else {
        (theme.secondary, theme.text)
    };
    let active = theme.active;
    button(
        column![
            frame,
            text(title)
                .size(13)
                .font(if checked { BOLD } else { crate::fonts::REGULAR })
                .wrapping(text::Wrapping::None)
                .width(Length::Fill)
                .align_x(Alignment::Center),
        ]
        .spacing(4),
    )
    .padding(4)
    .width(width)
    .on_press(press)
    .style(move |_, status| {
        let lit = controls::lights(status);
        button::Style {
            background: lit.then(|| active.into()),
            text_color: if lit { lit_ink } else { ink },
            border: crate::style::round(6),
            ..Default::default()
        }
    })
    .into()
}
