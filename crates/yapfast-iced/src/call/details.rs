//! The call's connection in detail: the ping as the signal's tooltip, and a
//! click on the signal (or the dock's status) opens the popout: Connection
//! (a graph of the last five minutes of pings, the average and last ping,
//! the packets lost), Privacy (the voice privacy code, while the call is
//! end-to-end encrypted with someone else in it) and Stream (how this
//! account's screen goes out, while it's live), over how the call is
//! secured.
//!
//! The call measures all along; what the popout shows is copied out of it
//! only while the popout shows or the pointer is on the signal, again each
//! second of the call's clock.

use iced::{
    Alignment, Color, Element, Length, Point, Rectangle, Size, Task, Theme,
    advanced::{Layout, Widget, layout, mouse, renderer, widget::Tree},
    widget::{Space, column, container, mouse_area, row, stack, text, tooltip},
};
use yapfast_core::voice::LinkStats;

use yapfast_core::call_details as words;

use super::{Event, Popup};
use crate::{
    app::{App, Message},
    controls::{self, Kind},
    fonts::BOLD,
    pictures::Icon,
    style::{faded, rounded},
    widgets::Renderer,
};
use yapfast_core::i18n::tr;

/// The popout's width, and its graph's height.
pub const WIDTH: f32 = 320.0;
const GRAPH_HEIGHT: f32 = 80.0;
/// How long "Copied" shows after the code is copied.
const COPIED_FOR: std::time::Duration = std::time::Duration::from_secs(2);

/// The popout's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Connection,
    Privacy,
    Stream,
}

/// What the details show, while they show.
#[derive(Default)]
pub struct Details {
    pub stats: Option<LinkStats>,
    pub tab: Tab,
    pub copied: bool,
    /// The pointer is on the signal (or the dock's status).
    pub pointed: bool,
}

/// What's done in the details.
#[derive(Clone, Debug)]
pub enum LinkEvent {
    Pointed(bool),
    Tab(Tab),
    Copy,
    CopiedDone,
}

/// The details, or the signal's tooltip, show.
pub fn link_shown(app: &App) -> bool {
    app.call.popup == Some(Popup::Link) || app.call.details.pointed
}

/// The details read again from the call, while they show.
pub(super) fn read_link(app: &mut App) {
    let shown = link_shown(app);
    app.call.details.stats = shown
        .then(|| {
            app.client
                .state()
                .voice
                .link
                .as_ref()
                .map(|link| link.read())
        })
        .flatten();
}

pub(super) fn link_event(app: &mut App, event: LinkEvent) -> Task<Message> {
    match event {
        LinkEvent::Pointed(pointed) => {
            app.call.details.pointed = pointed;
            read_link(app);
        }
        LinkEvent::Tab(tab) => app.call.details.tab = tab,
        LinkEvent::Copy => {
            let Some(code) = privacy_code(app.ctx()) else {
                return Task::none();
            };
            app.call.details.copied = true;
            return Task::batch([
                iced::clipboard::write(code.join(" ")),
                crate::after(
                    COPIED_FOR,
                    Message::Call(Event::Link(LinkEvent::CopiedDone)),
                ),
            ]);
        }
        LinkEvent::CopiedDone => app.call.details.copied = false,
    }
    Task::none()
}

/// The note under Connection, with the loss's when it's known.
fn connection_note(loss_known: bool) -> String {
    let mut note = "You may notice delayed audio at 250 ms or higher. If the problem persists, disconnect and try again.".to_owned();
    if loss_known {
        note.push_str(tr(
            " You may sound robotic if your packet loss rate is over 10%.",
        ));
    }
    note
}

const STREAM_NOTE: &str = "The bitrate follows what Discord estimates your connection carries. Fast motion at a low bitrate looks blocky to viewers.";
const PRIVACY_NOTE: &str = "A new code is generated when people join or leave this call. Compare it with the others in the call: everyone sees the same numbers.";
const PRIVATE_NOTE: &str = "Only you and the others in this call can hear you. Nobody else, not even Discord, can listen in.";

/// A line of steps drawn across its box (a graph of the call's pings): each
/// step's run at its height, joined by risers, in a box of its own units
/// stretched to the widget's, the line `stroke` px wide whatever the
/// stretch. Its lines are axis-aligned, so they're quads: no path to fill.
pub struct Steps {
    steps: Vec<yapfast_core::call_details::Step>,
    units: Size,
    ink: Color,
    stroke: f32,
    height: f32,
}

impl Steps {
    pub fn new(
        steps: Vec<yapfast_core::call_details::Step>,
        units: Size,
        ink: Color,
        height: f32,
    ) -> Self {
        Self {
            steps,
            units,
            ink,
            stroke: 2.0,
            height,
        }
    }
}

impl Widget<Message, Theme, Renderer> for Steps {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.height))
    }

    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fill, self.height, Size::ZERO))
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        let (sx, sy) = (
            bounds.width / self.units.width,
            bounds.height / self.units.height,
        );
        let half = self.stroke / 2.0;
        let mut line = |x: f32, y: f32, width: f32, height: f32| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(Point::new(x, y), Size::new(width, height)),
                    ..Default::default()
                },
                self.ink,
            );
        };
        let mut last: Option<f32> = None;
        for step in &self.steps {
            let x0 = bounds.x + step.x0 * sx;
            let x1 = bounds.x + step.x1 * sx;
            let y = bounds.y + step.y * sy;
            if let Some(was) = last {
                let (top, bottom) = (was.min(y), was.max(y));
                line(
                    x0 - half,
                    top - half,
                    self.stroke,
                    bottom - top + self.stroke,
                );
            }
            line(x0 - half, y - half, x1 - x0 + self.stroke, self.stroke);
            last = Some(y);
        }
    }
}

impl<'a> From<Steps> for Element<'a, Message> {
    fn from(steps: Steps) -> Self {
        Element::new(steps)
    }
}

/// The call measures its connection, as a Discord call does: its
/// signal opens the details.
pub fn link_measured(ctx: crate::ctx::Ctx<'_>) -> bool {
    let voice = &ctx.state.voice;
    voice.active() && voice.link.is_some()
}

/// `child` as the signal's touch: the ping as its tooltip, `below` it
/// or above, and a click opens the details from it. Only while the
/// call measures.
pub fn link_touch<'a>(
    ctx: crate::ctx::Ctx<'a>,
    child: impl Into<Element<'a, Message>>,
    below: bool,
) -> Element<'a, Message> {
    let child = child.into();
    if !link_measured(ctx) {
        return child;
    }
    let touch = mouse_area(child)
        .on_enter(Message::Call(Event::Link(LinkEvent::Pointed(true))))
        .on_exit(Message::Call(Event::Link(LinkEvent::Pointed(false))))
        .on_press(Message::Call(Event::Open(Popup::Link)))
        .interaction(iced::mouse::Interaction::Pointer);
    let ping = (ctx.call.popup != Some(Popup::Link))
        .then(|| ctx.call.details.stats.as_ref().and_then(words::last_ping))
        .flatten();
    let touch = ctx.spot(touch);
    match ping {
        Some(ping) => tooltip(
            touch,
            controls::tip(ctx.look.theme, ping),
            if below {
                tooltip::Position::Bottom
            } else {
                tooltip::Position::Top
            },
        )
        .gap(6)
        .into(),
        None => touch,
    }
}

/// The popout, as wide as [`WIDTH`].
pub(super) fn link_card<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let details = &ctx.call.details;
    let empty = LinkStats::default();
    let stats = details.stats.as_ref().unwrap_or(&empty);
    let code = privacy_code(ctx);
    let streaming = stats.stream.is_some();
    // Connection shows while the one picked isn't there.
    let shown = match details.tab {
        Tab::Privacy if code.is_some() => Tab::Privacy,
        Tab::Stream if streaming => Tab::Stream,
        _ => Tab::Connection,
    };
    let mut card = column![].spacing(10).padding(16);
    if code.is_some() || streaming {
        let tab = |label, tab| {
            controls::tab(
                controls::Tab {
                    label,
                    chosen: shown == tab,
                    ink: theme.text,
                    rest: theme.muted,
                    bar: theme.accent,
                    height: 26.0,
                    rest_font: BOLD,
                },
                Message::Call(Event::Link(LinkEvent::Tab(tab))),
            )
        };
        let mut tabs = row![tab(tr("Connection"), Tab::Connection)].spacing(16);
        if code.is_some() {
            tabs = tabs.push(tab(tr("Privacy"), Tab::Privacy));
        }
        if streaming {
            tabs = tabs.push(tab(tr("Stream"), Tab::Stream));
        }
        card = card.push(tabs);
    }
    card = card.push(match (shown, code) {
        (Tab::Privacy, Some(code)) => privacy_page(ctx, code),
        (Tab::Stream, _) => stream_page(ctx, stats),
        _ => connection_page(ctx, stats),
    });
    let secured = if ctx.state.voice.end_to_end {
        tr("End-to-end encrypted")
    } else {
        tr("Secure connection")
    };
    let live = theme.live;
    card = card.push(
        container(
            row![
                ctx.look.glyph(Icon::Lock, 16, live),
                text(secured).size(13).font(BOLD).color(live)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(8)
        .style(move |_| rounded(faded(live, 0.12), 6)),
    );
    let (ground, edge) = (theme.raised, theme.edge);
    container(card)
        .width(WIDTH)
        .style(move |_| crate::style::panel(ground, edge, 8.0))
        .into()
}

fn connection_page<'a>(ctx: crate::ctx::Ctx<'a>, stats: &LinkStats) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (steps, top) = words::graph(&stats.pings);
    let (line, muted) = (theme.line, theme.muted);
    let rule = move |y: f32| {
        controls::placed(
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(move |_| rounded(line, 0)),
            0.0,
            y,
        )
    };
    let label =
        move |words: String, y: f32| controls::placed(text(words).size(11).color(muted), 6.0, y);
    let units = Size::new(words::GRAPH.0, words::GRAPH.1);
    let graph = container(
        stack![
            Steps::new(steps, units, theme.link, GRAPH_HEIGHT),
            rule(0.0),
            rule(GRAPH_HEIGHT / 2.0),
            label(words::milliseconds(top), 4.0),
            label(words::milliseconds(top / 2), GRAPH_HEIGHT / 2.0 + 4.0),
        ]
        .width(Length::Fill)
        .height(GRAPH_HEIGHT),
    )
    .width(Length::Fill)
    .height(GRAPH_HEIGHT)
    .clip(true)
    .style({
        let sunken = theme.sunken;
        move |_| rounded(sunken, 6)
    });
    let average = words::average(&stats.pings).map(words::milliseconds);
    let mut page = column![
        graph,
        detail_row(
            ctx,
            tr("Average ping:"),
            average.unwrap_or_else(|| tr("Unknown").to_owned())
        )
    ]
    .spacing(8);
    if let Some(ping) = words::last_ping(stats) {
        page = page.push(detail_row(ctx, tr("Last ping:"), ping));
    }
    let loss = words::loss(stats);
    let known = loss.is_some();
    if let Some(loss) = loss {
        page = page.push(detail_row(ctx, tr("Outbound packet loss rate:"), loss));
    }
    page.push(detail_note(connection_note(known), theme.muted))
        .into()
}

fn stream_page<'a>(ctx: crate::ctx::Ctx<'a>, stats: &LinkStats) -> Element<'a, Message> {
    let Some(stream) = &stats.stream else {
        return Space::new().into();
    };
    column![
        detail_row(ctx, tr("Encoder:"), words::encoder(&stream.engine)),
        detail_row(ctx, tr("Codec:"), stream.codec.clone()),
        detail_row(ctx, tr("Resolution:"), words::picture(stream)),
        detail_row(ctx, tr("Sending:"), words::rate(stream.sent_kbps)),
        detail_row(ctx, tr("Aiming for:"), words::rate(stream.target_kbps)),
        detail_row(
            ctx,
            tr("Keyframes viewers asked for:"),
            stream.keyframe_requests.to_string()
        ),
        detail_row(ctx, tr("Packets resent:"), stream.resent.to_string()),
        detail_note(STREAM_NOTE.to_owned(), ctx.look.theme.muted),
    ]
    .spacing(8)
    .into()
}

fn privacy_page<'a>(ctx: crate::ctx::Ctx<'a>, code: Vec<String>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let mut rows = column![].spacing(6).padding(10);
    for groups in code.chunks(3) {
        let mut line = row![];
        for group in groups {
            line = line.push(
                container(text(group.clone()).size(18).font(BOLD).color(theme.text))
                    .center_x(Length::Fill),
            );
        }
        rows = rows.push(line);
    }
    let sunken = theme.sunken;
    let copied = ctx.call.details.copied;
    let copy = controls::action(
        theme,
        if copied { tr("Copied") } else { tr("Copy") },
        Kind::Plain,
        Some(Message::Call(Event::Link(LinkEvent::Copy))),
    )
    .small()
    .icon(ctx.look.icon(Icon::Copy, 16, crate::theme::rgb(theme.text)));
    column![
        text(tr("Voice Privacy Code"))
            .size(15)
            .font(BOLD)
            .color(theme.text),
        detail_note(PRIVACY_NOTE.to_owned(), theme.secondary),
        container(rows)
            .width(Length::Fill)
            .style(move |_| rounded(sunken, 6)),
        container(copy).center_x(Length::Fill),
        controls::bar(theme.line, Length::Fill, 1.0),
        detail_note(PRIVATE_NOTE.to_owned(), theme.secondary),
    ]
    .spacing(10)
    .into()
}

/// A label and its value in bold, as the popout's rows are.
fn detail_row<'a>(ctx: crate::ctx::Ctx<'a>, label: &'a str, value: String) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    row![
        text(label).size(14).color(theme.secondary),
        text(value).size(14).font(BOLD).color(theme.text),
    ]
    .spacing(4)
    .into()
}

fn detail_note<'a>(note: String, ink: iced::Color) -> Element<'a, Message> {
    text(note)
        .size(13)
        .color(ink)
        .wrapping(text::Wrapping::Word)
        .into()
}

/// The privacy code's groups, only while others are in the call to
/// compare it with, as Discord's client shows it.
fn privacy_code(ctx: crate::ctx::Ctx<'_>) -> Option<Vec<String>> {
    let others = ctx.state.voice.participants.len() > 1;
    let code = ctx.call.details.stats.as_ref()?.privacy_code.as_deref()?;
    (others && !code.is_empty()).then(|| words::groups(code))
}
