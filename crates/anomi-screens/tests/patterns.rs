//! The iced patterns docs/iced/iced-notes.md shows, compiled against the
//! pinned iced and the interface's theme, so the notes can't drift from the
//! API. Each function is one pattern; the tests run the ones that do
//! something headless.

use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::advanced::text::Target;
use iced::advanced::widget::Operation as _;
use iced::advanced::widget::operation::{self, Outcome};
use iced::font::Weight;
use iced::keyboard::{Key, key};
use iced::widget::image;
use iced::widget::scrollable::{AbsoluteOffset, Viewport};
use iced::widget::text::Span;
use iced::widget::text_editor::{self, Binding, Content, KeyPress};
use iced::widget::{
    Id, button, column, container, keyed_column, mouse_area, rich_text, row, span, stack, text,
    text_input, tooltip,
};
use iced::{
    Background, Border, Fill, Font, Padding, Point, Size, Subscription, Task, event, time, window,
};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, motion, typography};

// The patterns build messages; nothing here reads them back.
#[allow(dead_code)]
#[derive(Clone, Debug)]
enum Message {
    Open(String),
    Link(String),
    Edit(text_editor::Action),
    Send,
    Typed(String),
    Scrolled(Viewport),
    Menu(String),
    Focused(bool),
    Tick(Instant),
}

/// Pattern 1: A view returns `impl View<Message> + '_` and borrows what it shows.
/// Branches of different widget types meet in `.boxed()`.
fn title(name: Option<&str>) -> impl View<Message> + '_ {
    match name {
        Some(name) => text(name).font(typography::SEMIBOLD).boxed(),
        None => iced::widget::space().boxed(),
    }
}

/// Pattern 2: A style closure reads the palette; no colour is written in a view.
fn card<'a>(content: impl View<Message> + 'a) -> impl View<Message> + 'a {
    container(content)
        .padding(metrics::SPACE_M)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().raised)),
            border: Border::default()
                .rounded(metrics::RADIUS_CARD)
                .width(1)
                .color(theme.palette().edge),
            ..container::Style::default()
        })
}

/// Pattern 3: A row that is a button, with a named style from the catalog.
fn row_button(id: &str, label: &str) -> impl View<Message> {
    button(text(label.to_owned()))
        .on_press(Message::Open(id.to_owned()))
        .style(catalog::button::plain)
}

/// Pattern 4: Rich text: a mention pill, a link, bold. Spans borrow the text.
fn message_text<'a>(author: &'a str, url: &'a str) -> impl View<Message> + 'a {
    let mention: Span<'a, String> = span(author)
        .font(Font {
            weight: Weight::Semibold,
            ..typography::UI
        })
        .background(Background::Color(iced::Color::from_rgba8(
            88, 101, 242, 0.15,
        )))
        .border(Border::default().rounded(4))
        .padding(Padding::default().left(2).right(2));
    rich_text([
        mention,
        span(" see "),
        span(url).underline(true).link(url.to_owned()),
    ])
    .on_link_click(Message::Link)
    .selectable(true)
    .size(typography::size::BODY)
}

/// Pattern 5: A field: its text lives in the view-model; each keystroke is an
/// intent; Enter submits; an id lets a task focus it.
fn search(value: &str) -> impl View<Message> + '_ {
    text_input("Find", value)
        .id(Id::new("search"))
        .on_input(Message::Typed)
        .on_submit(Message::Send)
        .style(catalog::text_input::search)
}

fn focus_search() -> Task<Message> {
    iced::widget::operation::focus(Id::new("search"))
}

/// Pattern 6: The editor: `Content` holds text and undo; Enter sends, Shift+Enter
/// breaks the line; every other key falls back to the default binding.
fn composer(draft: &Content) -> impl View<Message> + '_ {
    iced::widget::text_editor(draft)
        .placeholder("Message")
        .on_action(Message::Edit)
        .key_binding(bindings)
        .padding([10, 12])
}

fn bindings(press: KeyPress) -> Option<Binding<Message>> {
    match press.key.as_ref() {
        Key::Named(key::Named::Enter) if press.is_focused && !press.modifiers.shift() => {
            Some(Binding::Custom(Message::Send))
        }
        _ => Binding::from_key_press(press),
    }
}

/// Pattern 7: A scrollable with an id, kept at its end, reporting where it is; and
/// the tasks that move it.
fn timeline<'a>(rows: impl Iterator<Item = &'a str>) -> impl View<Message> + 'a {
    iced::widget::scrollable(column(rows.map(|row| text(row).boxed())).spacing(4))
        .id(Id::new("timeline"))
        .anchor_bottom()
        .on_scroll(|scroll| Message::Scrolled(scroll.viewport))
        .height(Fill)
}

fn to_top() -> Task<Message> {
    let offset = AbsoluteOffset {
        x: None,
        y: Some(0.0),
    };
    iced::widget::operation::scrollable::scroll_to(
        Id::new("timeline"),
        offset,
        iced::widget::operation::Animation::default(),
    )
}

/// Pattern 8: A keyed column keeps each row's widget state with its key, so rows
/// that come and go don't hand state to their neighbours.
fn rooms<'a>(rooms: &'a [(String, String)]) -> impl View<Message> + 'a {
    keyed_column(
        rooms
            .iter()
            .map(|(id, name)| (key(id), row_button(id, name).boxed())),
    )
}

/// A row's key: a hash of its id, as keys are `Copy` and `'static`.
fn key(id: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

/// Pattern 9: A tooltip, and a right click through `mouse_area`.
fn with_menu<'a>(label: &'a str) -> impl View<Message> + 'a {
    let tip = container(text(label))
        .padding([6, 10])
        .style(catalog::container::tooltip);
    let item = mouse_area(text(label)).on_right_press(Message::Menu(label.to_owned()));
    tooltip(item, tip).position(tooltip::Position::Right)
}

/// Pattern 10: Layers: `stack` draws its children over each other in order.
fn badge_over(content: impl View<Message> + 'static) -> impl View<Message> {
    stack![
        content,
        container(text("1")).align_right(Fill).align_bottom(Fill)
    ]
}

/// Pattern 11: Tasks: one future, one stream, several at once, one that can be
/// cancelled through its handle, one after another.
fn tasks() -> (Task<Message>, iced::task::Handle) {
    let once = Task::perform(async { "done".to_owned() }, Message::Open);
    let stream = iced::futures::stream::iter(["a", "b"].map(str::to_owned));
    let (many, handle) = Task::run(stream, Message::Typed).abortable();
    let next = Task::done(Message::Send);
    (
        Task::batch([once, many]).chain(next),
        handle.abort_on_drop(),
    )
}

/// Pattern 12: Subscriptions: the window's focus, and a clock that ticks only while
/// the window is in front and something on screen shows the time.
fn subscription(front: bool, clock_shown: bool) -> Subscription<Message> {
    let focus = event::listen_with(|event, _status, _window| match event {
        iced::Event::Window(window::Event::Focused) => Some(Message::Focused(true)),
        iced::Event::Window(window::Event::Unfocused) => Some(Message::Focused(false)),
        _ => None,
    });
    if front && clock_shown {
        Subscription::batch([
            focus,
            time::every(Duration::from_secs(1)).map(Message::Tick),
        ])
    } else {
        focus
    }
}

/// Pattern 13: An animation in the state: its duration from `Motion`, interpolated
/// at the frame time `update` keeps.
fn panel_height(open: &iced::Animation<bool>, now: Instant) -> f32 {
    open.interpolate(0.0, 96.0, now)
}

fn open_panel(motion: Motion, now: Instant) -> iced::Animation<bool> {
    motion
        .animation(false, motion::SLIDE, motion::ARRIVE)
        .go(true, now)
}

/// Pattern 14: A picture from shared pixels, without a copy. `Bytes` is the
/// `bytes` crate's; iced doesn't re-export it.
fn picture(width: u32, height: u32, rgba: Arc<[u8]>) -> image::Handle {
    image::Handle::from_rgba(width, height, bytes::Bytes::from_owner(rgba))
}

#[test]
fn every_pattern_builds() {
    let draft = Content::with_text("hi");
    let rooms_list = vec![("a".to_owned(), "Alice".to_owned())];
    let _views: Vec<Element<'_, Message>> = vec![
        title(Some("Bob")).boxed(),
        card(text("card")).boxed(),
        message_text("@Bob", "https://example.org").boxed(),
        search("bo").boxed(),
        composer(&draft).boxed(),
        timeline(["one", "two"].into_iter()).boxed(),
        rooms(&rooms_list).boxed(),
        with_menu("Bob").boxed(),
        badge_over(text("avatar")).boxed(),
        row![text("a"), text("b")].boxed(),
    ];
    let _ = (focus_search(), to_top(), tasks(), subscription(true, true));
    let motion = Motion {
        front: true,
        ..Motion::default()
    };
    let now = Instant::now();
    assert_eq!(panel_height(&open_panel(Motion::default(), now), now), 96.0);
    assert!(panel_height(&open_panel(motion, now), now) < 96.0);
    let pixels: Arc<[u8]> = Arc::from(vec![0u8; 4]);
    assert!(matches!(picture(1, 1, pixels), image::Handle::Rgba { .. }));
}

#[test]
fn a_clock_ticks_only_in_front() {
    assert_eq!(subscription(true, true).units(), 2);
    assert_eq!(subscription(false, true).units(), 1);
}

#[test]
fn rich_text_selects_and_copies() {
    let settings = iced::Settings {
        fonts: anomi_theme::fonts::ALL
            .iter()
            .map(|font| std::borrow::Cow::Borrowed(*font))
            .collect(),
        font: typography::UI,
        ..iced::Settings::default()
    };
    let view = message_text("@Bob", "https://example.org");
    let mut ui = iced_test::Simulator::with_size(settings, Size::new(400.0, 100.0), view);
    ui.draw(&Theme::default());
    let mut select = operation::text::select(
        Point::new(0.0, 0.0),
        Point::new(399.0, 99.0),
        Target::Character,
    );
    ui.operate(&mut select);
    let mut copy = operation::text::copy();
    ui.operate(&mut operation::black_box(&mut copy));
    let Outcome::Some(copied) = copy.finish() else {
        panic!("nothing copied")
    };
    assert!(copied.contains("@Bob"), "{copied}");
}
