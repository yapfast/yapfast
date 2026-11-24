//! Go Live's menu: screens, windows, cameras, audio and stream quality.

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
    widget::{button, column, container, operation, row, text},
};
use yapfast_core::{
    application::Action,
    stream_worker::Source,
    voice::{Command, Resolution, StreamAudio, StreamQuality},
};

use crate::call::{Event, Popup};
use crate::{
    app::{App, Message, NOBODY},
    controls::{self, Check, heading},
    fonts::BOLD,
    pictures::Icon,
};

pub(super) const SHARE_WIDE: f32 = 496.0;

pub(super) const SHARE_NARROW: f32 = 240.0;

/// A preview in the share menu, in pixels.
const GLANCE: (u32, u32) = (240, 135);

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
    audio_windows_open: bool,
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
    Audio(StreamAudio),
    ToggleAudioWindows,
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

pub(in crate::call) fn share_event(app: &mut App, event: ShareEvent) -> Task<Message> {
    match event {
        ShareEvent::Listed(count, listing) => {
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
            let audio = voice.stream_audio;
            let mut audio_options = vec![
                (StreamAudio::Off, "No audio".into()),
                (StreamAudio::All, "All applications".into()),
            ];
            #[cfg(windows)]
            audio_options.extend(windows.iter().filter_map(|listed| match listed.source {
                Source::Window(id) => Some((StreamAudio::Window(id), listed.title.clone())),
                _ => None,
            }));
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
                audio_windows_open: false,
            };
            app.call.share = Some(share);
            app.call.popup = Some(Popup::Share);
            return Task::batch([
                glance(count, screen_ids, window_ids, preview_stop),
                operation::focus(NOBODY),
            ]);
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
            }
        }
        ShareEvent::Audio(audio) => {
            if let Some(share) = &mut app.call.share {
                share.audio = audio;
                app.client
                    .dispatch(Action::Voice(Command::StreamAudio(audio)));
                return app.sync();
            }
        }
        ShareEvent::ToggleAudioWindows => {
            if let Some(share) = &mut app.call.share {
                share.audio_windows_open = !share.audio_windows_open;
            }
        }
        ShareEvent::Resolution(resolution) => {
            if let Some(share) = &mut app.call.share {
                share.quality.resolution = resolution;
            }
        }
        ShareEvent::Fps(fps) => {
            if let Some(share) = &mut app.call.share {
                share.quality.fps = fps;
            }
        }
        ShareEvent::HighQuality => {
            if let Some(share) = &mut app.call.share {
                share.quality.high_quality = !share.quality.high_quality;
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

/// The share menu scrolls all choices together above the Go Live or Switch button.
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
    choices = choices.push(heading(theme, "STREAM AUDIO", 8.0));
    if share.switching {
        choices = choices.push(
            text("Audio changes apply immediately")
                .size(12)
                .color(theme.secondary),
        );
    }
    let audio_choice = |audio, title: &'a str, subtitle| {
        crate::call::popups::popup_row(
            ctx,
            title,
            subtitle,
            None,
            if audio == share.audio {
                Check::Current
            } else {
                Check::None
            },
            Some(Message::Call(Event::Share(ShareEvent::Audio(audio)))),
        )
    };
    for (audio, title) in share.audio_options.iter().take(2) {
        let subtitle =
            (*audio == StreamAudio::All).then_some("Excludes Yapfast's own audio".into());
        choices = choices.push(audio_choice(*audio, title, subtitle));
    }
    let window_audio = &share.audio_options[2..];
    if !window_audio.is_empty() {
        let selected = window_audio
            .iter()
            .find_map(|(audio, title)| (*audio == share.audio).then_some(title.as_str().into()));
        choices = choices.push(crate::call::popups::popup_row(
            ctx,
            "Select audio from window",
            selected,
            Some(if share.audio_windows_open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            }),
            Check::None,
            Some(Message::Call(Event::Share(ShareEvent::ToggleAudioWindows))),
        ));
        if share.audio_windows_open {
            for (audio, title) in window_audio {
                choices = choices.push(audio_choice(*audio, title, None));
            }
        }
    }
    if !share.switching {
        let quality = share.quality;
        choices = choices.push(heading(theme, "RESOLUTION", 8.0));
        for (resolution, label) in [
            (Resolution::P720, "720p"),
            (Resolution::P1080, "1080p"),
            (Resolution::Source, "Source"),
        ] {
            let check = if quality.resolution == resolution {
                Check::Current
            } else {
                Check::None
            };
            choices = choices.push(crate::call::popups::popup_row(
                ctx,
                label,
                None,
                None,
                check,
                Some(Message::Call(Event::Share(ShareEvent::Resolution(
                    resolution,
                )))),
            ));
        }
        choices = choices.push(heading(theme, "FRAME RATE", 8.0));
        for fps in [15, 30, 60] {
            let check = if quality.fps == fps {
                Check::Current
            } else {
                Check::None
            };
            choices = choices.push(crate::call::popups::popup_row(
                ctx,
                format!("{fps} fps"),
                None,
                None,
                check,
                Some(Message::Call(Event::Share(ShareEvent::Fps(fps)))),
            ));
        }
        choices =
            choices
                .push(heading(theme, "ENCODING", 8.0))
                .push(crate::call::popups::popup_row(
                    ctx,
                    "Higher quality",
                    Some("Sharper, at twice the CPU".into()),
                    None,
                    Check::Box(quality.high_quality),
                    Some(Message::Call(Event::Share(ShareEvent::HighQuality))),
                ));
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
    // The tallest the choices get: from the window's top margin to
    // the control, less the button's room.
    let spot = ctx.call.spot.get();
    let tallest = (spot.y - 6.0 - 8.0 - 12.0 - 52.0).clamp(80.0, 496.0);
    column![
        container(scroll_view(choices).height(Length::Fill)).height(tallest),
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
