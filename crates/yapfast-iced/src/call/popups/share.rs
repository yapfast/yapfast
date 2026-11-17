//! Go Live's menu (`ShareMenu`): the screens with a picture of each and
//! the windows, from a listing that comes as the menu opens.

use crate::widgets::scroll_view;
use std::sync::Arc;

use iced::{
    Alignment, Border, Element, Length, Padding, Task,
    advanced::image::Handle,
    widget::{button, column, container, operation, row, text},
};
use yapfast_core::voice::{Command, Resolution, StreamQuality};

use crate::call::{Event, Popup};
use crate::{
    app::{App, Message, NOBODY},
    controls::{self, Check, heading},
    fonts::BOLD,
    pictures::Icon,
};
use yapfast_core::i18n::{self, tr};

pub(super) const SHARE_WIDE: f32 = 336.0;

pub(super) const SHARE_NARROW: f32 = 240.0;

/// A screen's picture in the share menu, in pixels.
pub(super) const GLANCE: (u32, u32) = (160, 90);

/// The share menu's sources and choices.
pub struct Share {
    /// The screens with their pictures once taken, the windows, the
    /// cameras; then every source in the menu's order (screens, windows,
    /// the system's dialog, cameras).
    pub screens: Vec<(String, Option<Handle>)>,
    pub windows: Vec<String>,
    pub cameras: Vec<String>,
    pub sources: Vec<yapfast_core::stream_worker::Source>,
    /// The system's dialog picks a screen or window.
    pub dialog: bool,
    pub pick: usize,
    pub quality: StreamQuality,
    /// It switches what's shared, at the stream's quality.
    pub switching: bool,
}

impl Share {
    /// Switching where the system's dialog is the only choice, as no
    /// screen, window or camera is listed: the dialog opens with no menu.
    pub fn dialog_alone(&self) -> bool {
        self.switching
            && self.dialog
            && self.screens.is_empty()
            && self.windows.is_empty()
            && self.cameras.is_empty()
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
    /// A screen's picture, by the screen's id.
    Picture(u64, Handle),
    Pick(usize),
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

/// Pictures of the screens to share, from a worker, so capturing them
/// takes memory only until they're in.
pub(super) fn glance(ids: Vec<u64>) -> Task<Message> {
    if ids.is_empty() {
        return Task::none();
    }
    // The worker starts as the task runs, not as it's made.
    Task::done(ids).then(|ids| {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            use yapfast_core::stream_worker::{Report, Worker};
            let (_worker, mut reports) = match Worker::glance(ids, GLANCE.0, GLANCE.1) {
                Ok(started) => started,
                Err(error) => {
                    tracing::warn!("no pictures of the screens to share: {error:#}");
                    return;
                }
            };
            while let Some(report) = reports.blocking_recv() {
                match report {
                    Report::Screen {
                        id,
                        width,
                        height,
                        rgba,
                    } => {
                        let handle = Handle::from_rgba(width, height, rgba);
                        if sender.unbounded_send((id, handle)).is_err() {
                            break;
                        }
                    }
                    Report::Ended { error } => {
                        if let Some(error) = error {
                            tracing::warn!("no pictures of the screens to share: {error}");
                        }
                        break;
                    }
                    _ => {}
                }
            }
        });
        Task::run(receiver, |(id, handle)| {
            Message::Call(Event::Share(ShareEvent::Picture(id, handle)))
        })
    })
}

pub(in crate::call) fn share_event(app: &mut App, event: ShareEvent) -> Task<Message> {
    match event {
        ShareEvent::Listed(count, listing) => {
            use yapfast_core::stream_worker::Source;
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
                    _ => windows.push(listed),
                }
            }
            let ids: Vec<u64> = screens
                .iter()
                .filter_map(|listed| match listed.source {
                    Source::Display(id) => Some(id),
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
            let share = Share {
                screens: screens
                    .iter()
                    .map(|listed| (listed.title.clone(), None))
                    .collect(),
                windows: windows.iter().map(|listed| listed.title.clone()).collect(),
                cameras: cameras.iter().map(|listed| listed.title.clone()).collect(),
                sources,
                dialog,
                pick: 0,
                quality: voice.quality,
                switching: voice.sharing.is_some(),
            };
            let alone = share.dialog_alone();
            app.call.share = Some(share);
            if alone {
                return share_event(app, ShareEvent::Go);
            }
            app.call.popup = Some(Popup::Share);
            return Task::batch([glance(ids), operation::focus(NOBODY)]);
        }
        ShareEvent::Picture(id, handle) => {
            if let Some(share) = &mut app.call.share {
                let at = share
                    .sources
                    .iter()
                    .position(|source| *source == yapfast_core::stream_worker::Source::Display(id));
                if let Some((_, picture)) = at.and_then(|at| share.screens.get_mut(at)) {
                    *picture = Some(handle);
                }
            }
        }
        ShareEvent::Pick(pick) => {
            if let Some(share) = &mut app.call.share {
                share.pick = pick;
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
            let picked = share.sources.get(share.pick).cloned();
            let command = if share.switching {
                if let Some(picked) = picked {
                    yapfast_core::stream_worker::pick(picked);
                }
                Command::SwitchLive
            } else {
                yapfast_core::stream_worker::pick(
                    picked.unwrap_or(yapfast_core::stream_worker::Source::Screen),
                );
                Command::GoLive(share.quality)
            };
            crate::call::popups::close_popup(app);
            app.client
                .dispatch(yapfast_core::application::Action::Voice(command));
            return Task::batch([app.sync(), crate::composer::refocus(app)]);
        }
    }
    Task::none()
}

/// The share menu: what to share (screens as pictures two to a row,
/// windows, the system's dialog, cameras), then the quality, and Go
/// Live; while sharing, what to switch to, and Switch. The choices
/// scroll in a window too short for them, over the button, which stays.
pub(super) fn share_menu<'a>(
    ctx: crate::ctx::Ctx<'a>,
    share: &'a Share,
    width: f32,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let pick = |index: usize| Message::Call(Event::Share(ShareEvent::Pick(index)));
    let checked = |index: usize| {
        if share.pick == index {
            Check::Current
        } else {
            Check::None
        }
    };
    let mut choices = column![].spacing(2);
    if !share.screens.is_empty() {
        choices = choices.push(heading(theme, tr("SCREENS"), 4.0));
        let card = (width - 12.0 - 4.0) / 2.0;
        let mut cards = Vec::new();
        for (index, (title, picture)) in share.screens.iter().enumerate() {
            cards.push(screen_card(
                ctx,
                title,
                picture.as_ref(),
                card,
                share.pick == index,
                pick(index),
            ));
        }
        let mut rows = column![].spacing(4);
        let mut cards = cards.into_iter();
        loop {
            let Some(first) = cards.next() else { break };
            let pair = match cards.next() {
                Some(second) => row![first, second].spacing(4),
                None => row![first],
            };
            rows = rows.push(container(pair).center_x(Length::Fill));
        }
        choices = choices.push(rows);
    }
    if !share.windows.is_empty() {
        let top = if share.screens.is_empty() { 4.0 } else { 8.0 };
        choices = choices.push(heading(theme, tr("WINDOWS"), top));
        let first = share.screens.len();
        for (index, title) in share.windows.iter().enumerate() {
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
    let dialog_index = share.screens.len() + share.windows.len();
    let asks_dialog = share.dialog && !share.cameras.is_empty();
    if asks_dialog {
        choices =
            choices
                .push(heading(theme, tr("SCREEN"), 4.0))
                .push(crate::call::popups::popup_row(
                    ctx,
                    tr("Screen or window"),
                    Some(tr("Picked in the system's dialog").into()),
                    None,
                    checked(dialog_index),
                    Some(pick(dialog_index)),
                ));
    }
    if !share.cameras.is_empty() {
        let top = if dialog_index > 0 || asks_dialog {
            8.0
        } else {
            4.0
        };
        choices = choices.push(heading(theme, tr("CAMERAS"), top));
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
    if !share.switching {
        let lists = dialog_index + share.cameras.len() > 0;
        let quality = share.quality;
        choices = choices.push(heading(
            theme,
            tr("RESOLUTION"),
            if lists { 8.0 } else { 4.0 },
        ));
        for (resolution, label) in [
            (Resolution::P720, "720p"),
            (Resolution::P1080, "1080p"),
            (Resolution::Source, tr("Source")),
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
        choices = choices.push(heading(theme, tr("FRAME RATE"), 8.0));
        for fps in [15, 30, 60] {
            let check = if quality.fps == fps {
                Check::Current
            } else {
                Check::None
            };
            choices = choices.push(crate::call::popups::popup_row(
                ctx,
                i18n::fill(tr("{} fps"), &[&fps]),
                None,
                None,
                check,
                Some(Message::Call(Event::Share(ShareEvent::Fps(fps)))),
            ));
        }
        choices =
            choices
                .push(heading(theme, tr("ENCODING"), 8.0))
                .push(crate::call::popups::popup_row(
                    ctx,
                    tr("Higher quality"),
                    Some(tr("Sharper, at twice the CPU").into()),
                    None,
                    Check::Box(quality.high_quality),
                    Some(Message::Call(Event::Share(ShareEvent::HighQuality))),
                ));
    }
    let label = if share.switching {
        tr("Switch")
    } else {
        tr("Go Live")
    };
    let start: Element<'a, Message> = controls::action(
        theme,
        label,
        controls::Kind::Primary,
        Some(Message::Call(Event::Share(ShareEvent::Go))),
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
    let tallest = (spot.y - 6.0 - 8.0 - 12.0 - 52.0).max(80.0);
    column![
        container(scroll_view(choices).height(Length::Shrink)).max_height(tallest),
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

/// A screen in the share menu: its picture, once taken, over its name.
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
