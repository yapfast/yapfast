//! The title bar Yapfast draws in place of the desktop's, as the Slint
//! client's `WindowBar` (`ui/yapfast/window-bar.slint`) with its Discord
//! look: Back and Forward, where Yapfast is in the middle, Inbox and Help,
//! then minimize, maximize and close. Pressing and moving it moves the
//! window, a double click maximizes it, and its edges resize it. The Inbox
//! opens its popout under it (`ui/yapfast/inbox.slint`): the channels and
//! conversations with unread messages, or those that mention this account.
//!
//! Whether Yapfast draws it is the desktop's way (`frame.rs`, which the
//! Slint client reads too) until Appearance's "Use the system title bar"
//! chooses. A window is made with the frame or without; changing it makes
//! the window again, as the Slint client does for a maximized one.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Element, Length, Point, Size, Task,
    mouse::Interaction,
    widget::{Space, button, column, container, mouse_area, row, stack, text, tooltip},
    window,
};

use crate::{
    app::{App, Message},
    controls::{self, lit_style},
    elide::elide,
    fonts::BOLD,
    pictures::Icon,
    style::scrollbar,
    widgets::{Hover, Rows},
};

// The root crate's word on which desktops frame their windows, which the
// Slint window reads too: one file, compiled into both.
#[path = "../../../src/frame.rs"]
mod frame;

/// The bar's height, which moves the window's content down.
pub const HEIGHT: f32 = 32.0;

/// A caption button's width, and its glyph's size.
const CAPTION: f32 = 36.0;
const CAPTION_GLYPH: u32 = 12;

/// The inbox popout's width and tallest height, as Discord's.
const INBOX_WIDTH: f32 = 440.0;
const INBOX_MOST: f32 = 560.0;
const INBOX_ROW: f32 = 56.0;
/// Round the inbox's rows.
const INBOX_PAD: f32 = 4.0;

/// Where Discord's help opens.
const HELP: &str = "https://support.discord.com";

/// How far the pointer moves with the button down before the window moves,
/// so a double click still maximizes it.
const DRAG_AFTER: f32 = 3.0;

/// The edges' and corners' widths that resize a window without a frame.
const EDGE: f32 = 5.0;
const CORNER: f32 = 8.0;

/// Whether Yapfast draws the title bar, as `system` (Appearance's choice)
/// or else the desktop's way has it: on Linux where the desktop frames no
/// windows, and on Windows, where Discord's look draws its own.
pub fn own(system: Option<bool>) -> bool {
    offered() && !system.unwrap_or_else(system_by_default)
}

/// Where Yapfast can draw its own title bar.
pub fn offered() -> bool {
    cfg!(any(target_os = "linux", windows))
}

/// Whether the desktop draws the title bar unless the person chose: on a
/// desktop not known to give windows one, it's the desktop's, which may be
/// none at all, as a tiling compositor's. The tests lay the window out
/// without a bar wherever they run, unless one asks for it.
fn system_by_default() -> bool {
    cfg!(test) || (!cfg!(windows) && !frame::desktop_frames())
}

#[derive(Clone, Debug)]
pub enum Event {
    Back,
    Forward,
    Inbox,
    InboxTab(Tab),
    InboxOpen(String),
    InboxScrolled(crate::sync::Scroll),
    CloseInbox,
    Help,
    /// The bar pressed, the pointer moved on it, the button let go.
    Press,
    Moved(Point),
    Release,
    Maximize,
    Minimize,
    Close,
    /// An edge or corner pressed, by the direction it resizes.
    Resize(window::Direction),
    /// Whether the window is maximized now, and full screen (as a kiosk
    /// makes it).
    State(bool, bool),
}

/// The inbox's tabs, as Discord's first two.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Unreads,
    Mentions,
}

/// The inbox popout while it's open: its tab and its rows.
#[derive(Default)]
pub struct Inbox {
    pub tab: Tab,
    pub rows: Vec<InboxRow>,
    /// Its list's viewport: only the rows near it are built.
    pub scroll: crate::sync::Scroll,
}

/// One line of the inbox: a channel or conversation with something waiting.
#[derive(Clone, Debug, PartialEq)]
pub struct InboxRow {
    pub id: String,
    /// As the row shows it (a channel's with its `#`), and the name its
    /// avatar's initial and tint are taken from.
    pub name: String,
    pub avatar_name: String,
    pub place: String,
    pub picture: Option<String>,
    pub mentions: u64,
}

/// The press that may move the window, and where it was.
#[derive(Default)]
pub struct Drag {
    pub armed: Option<Option<Point>>,
}

/// Alt+Left and Alt+Right, and the mouse's back and forward buttons, go
/// back and forward through the conversations visited, as in Discord and
/// whichever bar the window has.
pub fn history_keys() -> iced::Subscription<Message> {
    use iced::keyboard::{self, Key, Modifiers, key::Named};
    iced::event::listen_with(|event, _, _| {
        let back = match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(named @ (Named::ArrowLeft | Named::ArrowRight)),
                modifiers: Modifiers::ALT,
                repeat: false,
                ..
            }) => named == Named::ArrowLeft,
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(button)) => match button {
                iced::mouse::Button::Back => true,
                iced::mouse::Button::Forward => false,
                _ => return None,
            },
            _ => return None,
        };
        Some(Message::Bar(if back {
            Event::Back
        } else {
            Event::Forward
        }))
    })
}

impl App {
    /// The room under the title bar is what the window's content lays out
    /// in (`Below`), so it's the window's size to everything under it.
    pub fn fit_window(&mut self) {
        let frame = self.shell.frame;
        let room = Size::new(frame.width, (frame.height - self.bar_height()).max(0.0));
        if room != self.window {
            self.window = room;
        }
    }

    /// The room the title bar takes at the window's top.
    pub fn bar_height(&self) -> f32 {
        if self.shell.own_title_bar && !self.full_screen() {
            HEIGHT
        } else {
            0.0
        }
    }

    /// Something full screen covers the window and the bar, or the window
    /// is full screen itself.
    fn full_screen(&self) -> bool {
        self.shell.full_screen || self.playback.clip_full || self.full_shown()
    }

    /// Asks the window whether it's maximized or full screen, as its size
    /// changed.
    pub fn read_window_state(&self) -> Task<Message> {
        let Some(id) = self.shell.window else {
            return Task::none();
        };
        window::is_maximized(id).then(move |maximized| {
            window::mode(id).map(move |mode| {
                Message::Bar(Event::State(maximized, mode == window::Mode::Fullscreen))
            })
        })
    }

    pub fn bar_event(&mut self, event: Event) -> Task<Message> {
        let id = self.shell.window;
        match event {
            Event::Back | Event::Forward if !self.workspace() || self.win.settings.is_open() => {
                Task::none()
            }
            Event::Back | Event::Forward => {
                self.client.dispatch(match event {
                    Event::Back => yapfast_core::application::Action::NavigateBack,
                    _ => yapfast_core::application::Action::NavigateForward,
                });
                self.sync()
            }
            Event::Inbox => {
                self.inbox = match self.inbox.take() {
                    Some(_) => None,
                    None => Some(Inbox {
                        scroll: crate::sync::Scroll::new(INBOX_MOST),
                        ..Inbox::default()
                    }),
                };
                self.inbox_sync();
                Task::none()
            }
            Event::InboxTab(tab) => {
                if let Some(inbox) = &mut self.inbox {
                    inbox.tab = tab;
                }
                self.inbox_sync();
                Task::none()
            }
            Event::InboxOpen(room) => {
                self.inbox = None;
                self.client
                    .dispatch(yapfast_core::application::Action::SwitchTo(room));
                self.sync()
            }
            Event::InboxScrolled(scroll) => {
                if let Some(inbox) = &mut self.inbox {
                    inbox.scroll = scroll;
                }
                Task::none()
            }
            Event::CloseInbox => {
                self.inbox = None;
                Task::none()
            }
            Event::Help => {
                crate::browser::open(HELP);
                Task::none()
            }
            Event::Press => {
                self.bar_drag.armed = Some(None);
                Task::none()
            }
            Event::Moved(at) => {
                let Some(armed) = &mut self.bar_drag.armed else {
                    return Task::none();
                };
                let Some(from) = *armed else {
                    *armed = Some(at);
                    return Task::none();
                };
                if (at.x - from.x).abs() > DRAG_AFTER || (at.y - from.y).abs() > DRAG_AFTER {
                    // The desktop takes the pointer for the move, and the
                    // window may not hear the button let go.
                    self.bar_drag.armed = None;
                    return id.map_or_else(Task::none, window::drag);
                }
                Task::none()
            }
            Event::Release => {
                self.bar_drag.armed = None;
                Task::none()
            }
            Event::Maximize => {
                self.bar_drag.armed = None;
                id.map_or_else(Task::none, window::toggle_maximize)
            }
            Event::Minimize => id.map_or_else(Task::none, |id| window::minimize(id, true)),
            // As the desktop's close button: to the tray, or quit.
            Event::Close => self.close_requested(),
            Event::Resize(direction) => {
                id.map_or_else(Task::none, |id| window::drag_resize(id, direction))
            }
            Event::State(maximized, full) => {
                self.shell.maximized = maximized;
                self.shell.full_screen = full;
                Task::none()
            }
        }
    }

    /// Appearance's switch: the window is made again with the desktop's
    /// frame or without it, where it was.
    pub fn choose_title_bar(&mut self, system: bool) -> Task<Message> {
        let own = own(Some(system));
        if own == self.shell.own_title_bar {
            return Task::none();
        }
        self.shell.own_title_bar = own;
        self.inbox = None;
        if self.shell.window.is_none() {
            return Task::none();
        }
        // iced could take the frame off the window as it is, but where the
        // desktop draws none winit draws its own, and a maximized window
        // taking it or losing it grows or shrinks past the screen.
        // Settings, which the switch is in, stay open as they are.
        let closed = self.drop_window_keeping(crate::app::Keep { settings: true });
        let opened = self.open_window();
        Task::batch([closed, opened])
    }

    /// The inbox's rows for its tab, from what the client already knows:
    /// the channels and conversations with unread messages, muted ones only
    /// for a mention, or those that mention this account.
    pub fn inbox_sync(&mut self) {
        let Some(inbox) = &mut self.inbox else {
            return;
        };
        let state = self.client.state();
        let mentions_only = inbox.tab == Tab::Mentions;
        // Each channel's server, found once, as the Slint client's
        // channel-to-server map.
        let places: std::collections::HashMap<&str, &str> = state
            .servers
            .iter()
            .flat_map(|server| {
                server
                    .channels
                    .iter()
                    .map(|channel| (channel.as_str(), server.name.as_str()))
            })
            .collect();
        let rows: Vec<InboxRow> = state
            .rooms
            .iter()
            .filter(|room| !room.request && !room.unlisted)
            .filter(|room| room.mentions > 0 || (!mentions_only && room.unread > 0 && !room.muted))
            .map(|room| InboxRow {
                id: room.id.clone(),
                name: if room.direct {
                    room.name.clone()
                } else {
                    format!("#{}", room.name)
                },
                avatar_name: room.name.clone(),
                place: places
                    .get(room.id.as_str())
                    .map(|place| (*place).to_owned())
                    .unwrap_or_default(),
                picture: room.direct.then(|| room.avatar.clone()).flatten(),
                mentions: room.mentions,
            })
            .collect();
        if rows != inbox.rows {
            inbox.rows = rows;
        }
    }

    /// The bar across the window's top, when Yapfast draws it.
    pub fn window_bar(&self) -> Option<Element<'_, Message>> {
        if self.bar_height() == 0.0 {
            return None;
        }
        let theme = &self.theme;
        let workspace = self.workspace();
        let state = self.client.state();
        let settings_open = self.win.settings.is_open();
        let place = if !workspace {
            "Yapfast".to_owned()
        } else if self.sidebar.header.server.is_none() {
            "Direct Messages".to_owned()
        } else {
            self.sidebar.header.name.clone()
        };
        let ink = if self.shell.focused {
            theme.secondary
        } else {
            theme.faint
        };
        let middle = row![
            self.look().glyph(Icon::Dm, 16, theme.secondary).boxed(16.0),
            elide(place).size(12).font(BOLD).color(ink).shrink()
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let back = workspace && !settings_open && state.can_go_back();
        let forward = workspace && !settings_open && state.can_go_forward();
        let left = row![
            self.bar_button(Icon::ArrowLeft, "Back", back.then_some(Event::Back), false),
            self.bar_button(
                Icon::ArrowRight,
                "Forward",
                forward.then_some(Event::Forward),
                false
            ),
        ]
        .spacing(4)
        .padding(iced::Padding {
            left: 8.0,
            ..iced::Padding::ZERO
        })
        .align_y(Alignment::Center);
        let mut right = row![].align_y(Alignment::Center);
        if workspace {
            right = right.push(
                row![
                    self.bar_button(
                        Icon::Inbox,
                        "Inbox",
                        Some(Event::Inbox),
                        self.inbox.is_some()
                    ),
                    self.bar_button(Icon::Help, "Help", Some(Event::Help), false),
                ]
                .spacing(4)
                .padding(iced::Padding {
                    right: 12.0,
                    ..iced::Padding::ZERO
                }),
            );
        }
        let maximized = self.shell.maximized;
        right = right
            .push(self.caption_button(Icon::Minus, Event::Minimize, false))
            .push(self.caption_button(
                if maximized {
                    Icon::Restore
                } else {
                    Icon::Square
                },
                Event::Maximize,
                false,
            ))
            .push(self.caption_button(Icon::X, Event::Close, true));
        // The bar's ground moves the window; its buttons sit over it. Its
        // moves are messages only once pressed: a pointer passing over it
        // sends none.
        let mut ground = mouse_area(container(Space::new()).width(Length::Fill).height(HEIGHT))
            .interaction(Interaction::Idle)
            .on_press(Message::Bar(Event::Press))
            .on_release(Message::Bar(Event::Release))
            .on_double_click(Message::Bar(Event::Maximize));
        if self.bar_drag.armed.is_some() {
            ground = ground.on_move(|at| Message::Bar(Event::Moved(at)));
        }
        let rail = theme.rail;
        let bar = stack![
            ground,
            container(middle)
                .center_x(Length::Fill)
                .height(HEIGHT)
                .align_y(Alignment::Center)
                .padding([0, 230]),
            row![left, Space::new().width(Length::Fill), right]
                .height(HEIGHT)
                .align_y(Alignment::Center),
        ];
        Some(
            container(bar)
                .width(Length::Fill)
                .height(HEIGHT)
                .style(move |_| crate::style::fill(rail))
                .into(),
        )
    }

    /// A button in the bar's own row, as the Slint client's `BarButton`: a
    /// 16 px glyph on a 4 px rounded ground, its label in a tip below.
    fn bar_button(
        &self,
        icon: Icon,
        label: &'static str,
        event: Option<Event>,
        active: bool,
    ) -> Element<'_, Message> {
        let theme = &self.theme;
        let enabled = event.is_some();
        let rest_ink = if !enabled {
            theme.faint
        } else if active {
            theme.text
        } else {
            theme.secondary
        };
        let base = self.look().glyph(icon, 16, rest_ink).boxed(16.0);
        let face: Element<'_, Message> = if enabled && !active {
            Hover::new(base, self.look().glyph(icon, 16, theme.text).boxed(16.0)).into()
        } else {
            base.into()
        };
        let rest = active.then(|| theme.wash(0.12));
        let lit = if !enabled {
            iced::Color::TRANSPARENT
        } else if active {
            theme.wash(0.12)
        } else {
            theme.wash(0.08)
        };
        let face = button(container(face).center(Length::Fill))
            .padding(0)
            .width(28)
            .height(24)
            .on_press_maybe(event.map(Message::Bar))
            .style(lit_style(rest, lit, 4.0));
        // What the click opens opens under the tip, which goes while it's open.
        if active {
            return face.into();
        }
        tooltip(face, controls::tip(theme, label), tooltip::Position::Bottom)
            .gap(6)
            .into()
    }

    /// The window's minimize, maximize and close, as the Slint client's
    /// `CaptionButton`: close lights red.
    fn caption_button(&self, icon: Icon, event: Event, close: bool) -> Element<'_, Message> {
        let theme = &self.theme;
        let base = self
            .look()
            .glyph(icon, CAPTION_GLYPH, theme.secondary)
            .boxed(12.0);
        let face: Element<'_, Message> = if close {
            Hover::new(
                base,
                self.look()
                    .glyph(icon, CAPTION_GLYPH, theme.on_danger)
                    .boxed(12.0),
            )
            .into()
        } else {
            base.into()
        };
        let lit = if close { theme.danger } else { theme.hover };
        button(container(face).center(Length::Fill))
            .padding(0)
            .width(CAPTION)
            .height(HEIGHT)
            .on_press(Message::Bar(event))
            .style(lit_style(None, lit, 0.0))
            .into()
    }

    /// The edges and corners that resize a window without the desktop's
    /// frame, while it isn't maximized: a layer over the window that only
    /// they take the pointer in.
    pub fn resize_edges(&self) -> Option<Element<'_, Message>> {
        if self.bar_height() == 0.0 || self.shell.maximized {
            return None;
        }
        use window::Direction;
        let grip = |direction: Direction, width: Length, height: Length| -> Element<'_, Message> {
            let cursor = match direction {
                Direction::North | Direction::South => Interaction::ResizingVertically,
                Direction::East | Direction::West => Interaction::ResizingHorizontally,
                Direction::NorthWest | Direction::SouthEast => Interaction::ResizingDiagonallyDown,
                Direction::NorthEast | Direction::SouthWest => Interaction::ResizingDiagonallyUp,
            };
            mouse_area(container(Space::new()).width(width).height(height))
                .interaction(cursor)
                .on_press(Message::Bar(Event::Resize(direction)))
                .into()
        };
        let fill = Length::Fill;
        let top = row![
            grip(
                Direction::NorthWest,
                Length::Fixed(CORNER),
                Length::Fixed(CORNER)
            ),
            column![
                grip(Direction::North, fill, Length::Fixed(EDGE)),
                Space::new().height(CORNER - EDGE)
            ]
            .width(fill),
            grip(
                Direction::NorthEast,
                Length::Fixed(CORNER),
                Length::Fixed(CORNER)
            ),
        ]
        .height(CORNER);
        let sides = row![
            grip(Direction::West, Length::Fixed(EDGE), fill),
            Space::new().width(fill).height(fill),
            grip(Direction::East, Length::Fixed(EDGE), fill),
        ]
        .height(fill);
        let bottom = row![
            grip(
                Direction::SouthWest,
                Length::Fixed(CORNER),
                Length::Fixed(CORNER)
            ),
            column![
                Space::new().height(CORNER - EDGE),
                grip(Direction::South, fill, Length::Fixed(EDGE))
            ]
            .width(fill),
            grip(
                Direction::SouthEast,
                Length::Fixed(CORNER),
                Length::Fixed(CORNER)
            ),
        ]
        .height(CORNER);
        Some(column![top, sides, bottom].into())
    }

    /// The inbox popout under the bar's Inbox button, as Discord's: 440 px
    /// wide, Unreads and Mentions, a click on a row going there.
    pub fn inbox_view(&self) -> Option<Element<'_, Message>> {
        let inbox = self.inbox.as_ref()?;
        if self.bar_height() == 0.0 || !self.workspace() {
            return None;
        }
        let theme = &self.theme;
        let tab = |label: &'static str, which: Tab, width: f32| -> Element<'_, Message> {
            let selected = inbox.tab == which;
            button(
                container(text(label).size(14).font(BOLD).color(if selected {
                    theme.text
                } else {
                    theme.secondary
                }))
                .center(Length::Fill),
            )
            .padding(0)
            .width(width)
            .height(32)
            .on_press(Message::Bar(Event::InboxTab(which)))
            .style(lit_style(
                selected.then(|| theme.wash(0.12)),
                theme.wash(if selected { 0.12 } else { 0.06 }),
                4.0,
            ))
            .into()
        };
        let head = row![
            text("Inbox").size(16).font(BOLD).color(theme.text),
            Space::new().width(Length::Fill),
            tab("Unreads", Tab::Unreads, 80.0),
            tab("Mentions", Tab::Mentions, 92.0),
        ]
        .spacing(8)
        .padding([0, 16])
        .height(48)
        .align_y(Alignment::Center);
        let body: Element<'_, Message> = if inbox.rows.is_empty() {
            container(
                text(match inbox.tab {
                    Tab::Unreads => "Unread messages from your unmuted channels show up here.",
                    Tab::Mentions => "Nothing mentions you right now.",
                })
                .size(14)
                .color(theme.muted)
                .align_x(Alignment::Center)
                .width(Length::Fill),
            )
            .padding(iced::Padding {
                top: 16.0,
                left: 16.0,
                right: 16.0,
                bottom: 0.0,
            })
            .height(136)
            .align_y(Alignment::Center)
            .into()
        } else {
            // Only the rows near the view are built: the Unreads can list
            // hundreds.
            let (first, last) = inbox.scroll.range(inbox.rows.len(), INBOX_ROW, INBOX_PAD);
            let mut rows = Rows::new(inbox.rows.len() as f32 * INBOX_ROW + INBOX_PAD * 2.0);
            for (index, entry) in inbox.rows.iter().enumerate().take(last).skip(first) {
                rows.push(
                    crate::app::key(&entry.id),
                    INBOX_PAD + index as f32 * INBOX_ROW,
                    INBOX_ROW,
                    container(self.inbox_line(entry)).padding([0.0, INBOX_PAD]),
                );
            }
            scroll_view(rows)
                .on_scroll(|viewport| {
                    Message::Bar(Event::InboxScrolled(crate::sync::Scroll::of(viewport)))
                })
                .style(scrollbar(theme))
                .height(Length::Fill)
                .into()
        };
        let (raised, line) = (theme.raised, theme.line);
        let panel = container(column![
            head,
            controls::bar(theme.line, Length::Fill, 1),
            body
        ])
        .width(INBOX_WIDTH)
        // `window` is the room under the bar already: the panel starts 4 px
        // under it and stops 12 px over the window's foot.
        .height((self.window.height - 4.0 - 12.0).min(INBOX_MOST))
        .style(move |_| iced::widget::container::Style {
            background: Some(raised.into()),
            border: crate::style::edge(line, 8.0),
            shadow: crate::style::raised_shadow(),
            ..Default::default()
        });
        let at = Point::new(self.window.width - INBOX_WIDTH - 8.0, HEIGHT + 4.0);
        Some(
            stack![
                controls::outside(Message::Bar(Event::CloseInbox), false),
                controls::placed(mouse_area(panel).on_press(Message::Nothing), at.x, at.y),
            ]
            .into(),
        )
    }

    fn inbox_line<'a>(&'a self, entry: &'a InboxRow) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut names = column![
            elide(entry.name.as_str())
                .size(14)
                .font(BOLD)
                .color(theme.text)
        ];
        if !entry.place.is_empty() {
            names = names.push(elide(entry.place.as_str()).size(12).color(theme.muted));
        }
        let mut line = row![
            self.ctx().avatar_tinted(
                entry.picture.as_deref(),
                &entry.avatar_name,
                &entry.id,
                32.0,
                16.0,
                1.0
            ),
            container(names.spacing(2)).width(Length::Fill)
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        if entry.mentions > 0 {
            line = line.push(controls::count_pill(
                theme,
                &controls::count_badge(entry.mentions),
                controls::INBOX_COUNT,
                None,
            ));
        }
        button(line)
            .padding(8)
            .width(Length::Fill)
            .height(INBOX_ROW)
            .on_press(Message::Bar(Event::InboxOpen(entry.id.clone())))
            .style(lit_style(None, theme.wash(0.06), 4.0))
            .into()
    }
}
