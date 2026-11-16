//! Files that go with the next message, as the Slint client takes them:
//! chosen in the system's file dialog (through the desktop's portal on
//! Linux), pasted (files copied in a file manager, or a picture), or
//! dropped on the window. They wait over the composer's text as tiles
//! (`StagedTile`): a picture's preview or a file's icon, the name and size,
//! a button that marks it as a spoiler where the room has them, and one
//! that takes it out. A drag over the window shows Discord's card saying
//! where they'd go, and a drop that can't go says why until Got It.
//!
//! The core keeps the files (`Action::Stage`) and checks each against the
//! room's limits; the previews are made off the window's thread, at 240 px
//! at most, and go when their file does.

use crate::widgets::scroll_view;
use std::{path::PathBuf, sync::Arc, time::Duration};

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Size, Task,
    widget::{button, column, container, row, scrollable, stack, text},
};
use yapfast_core::{
    application::Action,
    chat::{self, Picture},
};

use crate::{
    after,
    app::{App, Message},
    composer::{self, Event as Write},
    controls::{self, lit_style},
    fonts::{BOLD, MEDIUM},
    media::{Fit, Shot},
    pictures::Icon,
    style::{card_shadow, rounded},
};

/// The tray's height over the text, and a tile's size and picture.
pub const TRAY: f32 = 152.0;
const TILE: Size = Size::new(120.0, 140.0);
const TILE_PICTURE: f32 = 86.0;
/// The tiles' round buttons, on a dark disc.
const TILE_BUTTON: f32 = 28.0;
const DISC: Color = Color::from_rgba8(0, 0, 0, 0xb3 as f32 / 255.0);
/// The drop card's width, and the dim under it: the Slint client's, a
/// shade lighter than a dialog's.
const CARD: f32 = 310.0;
const DROP_DIM: Color = Color::from_rgba8(0, 0, 0, 0xa6 as f32 / 255.0);
/// How long after the first file of a drop the drop is taken whole: a
/// drop comes a file at a time.
const DROP_GATHER: Duration = Duration::from_millis(30);

#[derive(Clone, Debug)]
pub enum Event {
    /// The file dialog, to attach; what it chose.
    Attach,
    Chosen(Vec<PathBuf>),
    Unstage(usize),
    Spoiler(usize),
    /// A paste found files or a picture, or nothing to attach (text).
    Clipboard(Option<Vec<PathBuf>>),
    /// The same for the thread beside the channel's message.
    ChosenAside(Vec<PathBuf>),
    UnstageAside(usize),
    SpoilerAside(usize),
    ClipboardAside(Option<Vec<PathBuf>>),
    /// A file's preview was made, or couldn't be.
    Preview(PathBuf, Option<Arc<Picture>>),
    /// Files dragged over the window, dropped (one at a time), the drop
    /// gathered, the drag gone.
    Hovered,
    /// Where they are, in the window, where the desktop says (Wayland).
    Moved(iced::Point),
    /// Where winit says nothing of where (X11, Windows): the platform's
    /// pointer, made as files come over the window, and a look at it.
    Pointer(Following),
    Follow,
    Dropped(PathBuf),
    Drop,
    Left,
    /// The refusal's Got It, or a click on it.
    Understood,
}

/// The platform's pointer over the window, while files hover over it.
#[derive(Clone)]
pub struct Following(pub(crate) Option<Arc<dyn crate::desktop::PointerAt>>);

impl std::fmt::Debug for Following {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(if self.0.is_some() {
            "Following"
        } else {
            "Following(none)"
        })
    }
}

/// How often the pointer is asked where it is while files hover over the
/// window on X11 and Windows, as the Slint client asks it.
const FOLLOW: std::time::Duration = std::time::Duration::from_millis(40);

/// What files over the window would do, and why a drop didn't.
/// Files waiting to go and dragged over the window.
#[derive(Default)]
pub struct Uploads {
    /// Previews of the staged pictures, by path (none while being made),
    /// and files dragged over the window.
    pub thumbnails: std::collections::HashMap<PathBuf, Option<Arc<Picture>>>,
    /// The staged lists the previews were last made for, by place and
    /// length: the channel's and the thread's.
    pub staged_seen: [usize; 4],
    pub drop: Drop,
}

#[derive(Default)]
pub struct Drop {
    pub dragging: bool,
    /// The platform's pointer, on X11 and Windows while files hover.
    pointer: Option<Arc<dyn crate::desktop::PointerAt>>,
    asked: bool,
    /// They're over the thread beside the channel, and go to it.
    pub aside: bool,
    pub gathered: Vec<PathBuf>,
    pub refusal: Option<(String, String)>,
}

/// The system's file dialog for files to attach: `to` the files chosen,
/// or `cancelled`.
pub fn pick_files(
    to: impl Fn(Vec<PathBuf>) -> Message + Send + 'static,
    cancelled: Message,
) -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .set_title("Attach files")
                .pick_files()
                .await
                .map(|files| files.iter().map(|file| file.path().to_owned()).collect())
        },
        move |files| match files {
            Some(files) => to(files),
            None => cancelled.clone(),
        },
    )
}

impl App {
    /// The drop's mark follows the pointer: over the thread beside, or
    /// the channel.
    fn follow_drop(&mut self) {
        if let Some((x, y)) = self
            .uploads
            .drop
            .pointer
            .as_ref()
            .and_then(|pointer| pointer.at())
        {
            let zoom = f64::from(self.shell.zoom);
            let at = iced::Point::new((x / zoom) as f32, (y / zoom) as f32);
            self.uploads.drop.aside = self.over_aside(at);
        }
    }

    pub fn uploads_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Attach => self.attach(),
            Event::Chosen(files) => {
                self.client.dispatch(Action::Stage(files));
                Task::batch([self.sync(), self.refocus()])
            }
            Event::Unstage(index) => {
                self.client.dispatch(Action::Unstage(index));
                self.sync()
            }
            Event::Spoiler(index) => {
                self.client.dispatch(Action::ToggleSpoiler(index));
                self.sync()
            }
            Event::Clipboard(Some(files)) => {
                self.client.dispatch(Action::Stage(files));
                self.sync()
            }
            Event::Clipboard(None) => iced::clipboard::read()
                .map(|text| Message::Write(Write::Pasted(composer::Writing::Draft, text))),
            Event::ChosenAside(files) | Event::ClipboardAside(Some(files)) => {
                if !files.is_empty() {
                    self.client.dispatch(Action::StageAside(files));
                }
                Task::batch([
                    self.sync(),
                    iced::widget::operation::focus(crate::thread::COMPOSER),
                ])
            }
            Event::ClipboardAside(None) => iced::clipboard::read()
                .map(|text| Message::Write(Write::Pasted(composer::Writing::Aside, text))),
            Event::UnstageAside(index) => {
                self.client.dispatch(Action::UnstageAside(index));
                self.sync()
            }
            Event::SpoilerAside(index) => {
                self.client.dispatch(Action::ToggleAsideSpoiler(index));
                self.sync()
            }
            Event::Preview(path, picture) => {
                // Unless it was taken out meanwhile.
                if let Some(slot) = self.uploads.thumbnails.get_mut(&path) {
                    *slot = picture;
                }
                Task::none()
            }
            Event::Hovered => {
                self.uploads.drop.dragging = true;
                self.uploads.drop.refusal = None;
                self.follow_pointer()
            }
            Event::Moved(at) => {
                self.uploads.drop.aside = self.over_aside(at);
                Task::none()
            }
            Event::Pointer(Following(pointer)) => {
                // Unless the drag ended meanwhile.
                if self.uploads.drop.dragging {
                    self.uploads.drop.pointer = pointer;
                    self.follow_drop();
                }
                Task::none()
            }
            Event::Follow => {
                self.follow_drop();
                Task::none()
            }
            Event::Dropped(path) => {
                // Where it was let go.
                self.follow_drop();
                self.uploads.drop.dragging = false;
                self.uploads.drop.pointer = None;
                self.uploads.drop.asked = false;
                self.uploads.drop.gathered.push(path);
                if self.uploads.drop.gathered.len() == 1 {
                    return after(DROP_GATHER, Message::Write(Write::Uploads(Event::Drop)));
                }
                Task::none()
            }
            Event::Drop => self.dropped(),
            Event::Left => {
                self.uploads.drop.dragging = false;
                self.uploads.drop.pointer = None;
                self.uploads.drop.asked = false;
                Task::none()
            }
            Event::Understood => {
                self.uploads.drop.refusal = None;
                Task::none()
            }
        }
    }

    /// The files of a drop go with the next message, all of them, or none
    /// and why, as in Discord's client.
    fn dropped(&mut self) -> Task<Message> {
        let paths = std::mem::take(&mut self.uploads.drop.gathered);
        if paths.is_empty() {
            return Task::none();
        }
        let state = self.client.state();
        // Over the thread beside the channel, they go to it.
        let aside = self.uploads.drop.aside && self.ctx().thread_shown();
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
            self.client.dispatch(stage(paths));
            return self.sync();
        }
        if let Some(refusal) = refusal {
            self.uploads.drop.refusal =
                Some((refusal.title().to_owned(), refusal.help().to_owned()));
            return Task::none();
        }
        self.client.dispatch(stage(paths));
        let focus = if aside {
            iced::widget::operation::focus(crate::thread::COMPOSER)
        } else {
            self.refocus()
        };
        Task::batch([self.sync(), focus])
    }

    /// Whether a point of the window is over the thread beside the
    /// channel, below the header, as the Slint client's `over-aside`.
    fn over_aside(&self, at: iced::Point) -> bool {
        let bar = self.shell.frame.height - self.window.height;
        self.ctx().thread_shown()
            && at.x >= self.shell.frame.width - self.aside_width()
            && at.y >= bar + crate::header::HEIGHT
    }

    /// Files came over the window: on X11 and Windows, where winit says
    /// nothing of where they are, the platform's pointer is asked for once
    /// per drag, to be asked where it is while they hover and as they're
    /// let go.
    fn follow_pointer(&mut self) -> Task<Message> {
        let Some(id) = self.shell.window.filter(|_| !self.uploads.drop.asked) else {
            return Task::none();
        };
        self.uploads.drop.asked = true;
        iced::window::scale_factor(id)
            .then(move |scale| {
                iced::window::run(id, move |window| {
                    crate::desktop::desktop()
                        .pointer(window, f64::from(scale))
                        .map(Arc::from)
                })
            })
            .map(|pointer| Message::Write(Write::Uploads(Event::Pointer(Following(pointer)))))
    }

    /// The pointer asked where it is while files hover, where the
    /// platform doesn't say (X11, Windows).
    pub fn follow_subscription(&self) -> Option<iced::Subscription<Message>> {
        (self.uploads.drop.dragging && self.uploads.drop.pointer.is_some()).then(|| {
            iced::time::every(FOLLOW).map(|_| Message::Write(Write::Uploads(Event::Follow)))
        })
    }

    /// The system's file dialog, as the Slint client's `attach` opens it.
    pub fn attach(&mut self) -> Task<Message> {
        if !self.client.state().can_attach() {
            return Task::none();
        }
        pick_files(
            |files| Message::Write(Write::Uploads(Event::Chosen(files))),
            Message::Nothing,
        )
    }

    /// Makes the previews of staged pictures that have none yet, off the
    /// window's thread, and lets go of those of files no longer staged.
    pub fn previews(&mut self) -> Task<Message> {
        let state = self.client.state();
        let (staged, aside) = (state.staged(), state.aside_staged());
        self.uploads
            .thumbnails
            .retain(|path, _| staged.iter().chain(aside).any(|file| &file.path == path));
        let mut tasks = Vec::new();
        for file in staged.iter().chain(aside).filter(|file| file.image) {
            if self.uploads.thumbnails.contains_key(&file.path) {
                continue;
            }
            self.uploads.thumbnails.insert(file.path.clone(), None);
            let path = file.path.clone();
            tasks.push(Task::perform(
                {
                    let path = path.clone();
                    async move {
                        tokio::task::spawn_blocking(move || {
                            std::fs::read(&path)
                                .ok()
                                .and_then(|data| chat::decode_picture(&data, 240, 240))
                                .map(Arc::new)
                        })
                        .await
                        .ok()
                        .flatten()
                    }
                },
                move |picture| {
                    Message::Write(Write::Uploads(Event::Preview(path.clone(), picture)))
                },
            ));
        }
        Task::batch(tasks)
    }
}

/// The window's drag-and-drop events, where the platform has them (winit
/// takes drops on X11 and Windows, saying nothing of where).
pub fn drops(event: &iced::window::Event) -> Option<Message> {
    let event = match event {
        iced::window::Event::FileHovered(_) => Event::Hovered,
        iced::window::Event::FileDropped(path) => Event::Dropped(path.clone()),
        iced::window::Event::FilesHoveredLeft => Event::Left,
        _ => return None,
    };
    Some(Message::Write(Write::Uploads(event)))
}

impl<'a> crate::ctx::Ctx<'a> {
    /// Whether the thread beside the channel shows, to take files: not
    /// the New Thread form, nor while search results take its place.
    fn thread_shown(self) -> bool {
        self.lanes.aside.is_some() && self.open.aside && self.state.new_thread().is_none()
    }

    /// The files waiting, in a row that scrolls sideways; none without.
    pub fn tray(self) -> Option<Element<'a, Message>> {
        let state = self.state;
        self.tray_of(state.staged(), state.can_mark_spoilers(), false)
    }

    /// The files waiting in the thread beside the channel's composer.
    pub fn aside_tray(self) -> Option<Element<'a, Message>> {
        let state = self.state;
        self.tray_of(state.aside_staged(), state.aside_can_mark_spoilers(), true)
    }

    fn tray_of(
        self,
        staged: &'a [chat::Staged],
        can_spoil: bool,
        aside: bool,
    ) -> Option<Element<'a, Message>> {
        if staged.is_empty() {
            return None;
        }
        let mut tiles = row![].spacing(8);
        for (index, file) in staged.iter().enumerate() {
            tiles = tiles.push(self.staged_tile(index, file, can_spoil, aside));
        }
        Some(
            scroll_view(container(tiles).padding(Padding {
                top: 10.0,
                right: 10.0,
                bottom: 2.0,
                left: 10.0,
            }))
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(4).scroller_width(4),
            ))
            .width(Length::Fill)
            .height(TRAY)
            .style(crate::style::scrollbar(self.look.theme))
            .into(),
        )
    }

    /// A file waiting, as the Slint client's `StagedTile`.
    fn staged_tile(
        self,
        index: usize,
        file: &'a chat::Staged,
        can_spoil: bool,
        aside: bool,
    ) -> Element<'a, Message> {
        let theme = self.look.theme;
        let inner = TILE.width - 12.0;
        let picture: Element<'a, Message> = match self
            .uploads
            .thumbnails
            .get(&file.path)
            .and_then(Option::as_ref)
        {
            Some(picture) => self.shot(
                Shot::new(Some(picture), Size::new(inner, TILE_PICTURE))
                    .fit(Fit::Cover)
                    .radius(6.0)
                    .covered(file.spoiler),
            ),
            None => {
                let ground = theme.sidebar;
                container(self.look.glyph(Icon::File, 32, theme.muted))
                    .center_x(inner)
                    .center_y(TILE_PICTURE)
                    .style(move |_| rounded(ground, 6))
                    .into()
            }
        };
        let words = column![
            text(file.name.clone())
                .size(13)
                .font(MEDIUM)
                .color(theme.text)
                .wrapping(text::Wrapping::None),
            text(chat::file_size(file.size))
                .size(12)
                .color(theme.muted)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(2)
        .width(inner)
        .clip(true);
        let ground = theme.raised;
        let tile = container(column![picture, words].spacing(2))
            .padding(6)
            .width(TILE.width)
            .height(TILE.height)
            .style(move |_| rounded(ground, 8));
        let mut buttons = row![].spacing(4);
        if can_spoil {
            buttons = buttons.push(self.disc_button(
                if file.spoiler {
                    Icon::EyeOff
                } else {
                    Icon::Eye
                },
                if file.spoiler {
                    "Stop covering"
                } else {
                    "Mark as a spoiler"
                },
                Message::Write(Write::Uploads(if aside {
                    Event::SpoilerAside(index)
                } else {
                    Event::Spoiler(index)
                })),
            ));
        }
        buttons = buttons.push(self.disc_button(
            Icon::X,
            "Remove",
            Message::Write(Write::Uploads(if aside {
                Event::UnstageAside(index)
            } else {
                Event::Unstage(index)
            })),
        ));
        stack![
            tile,
            container(buttons)
                .width(TILE.width)
                .padding(4)
                .align_x(Alignment::End)
        ]
        .into()
    }

    /// A tile's round button, white on a dark disc.
    fn disc_button(
        self,
        icon: Icon,
        label: &'static str,
        message: Message,
    ) -> Element<'a, Message> {
        let face = self.look.glyph(icon, 16, Color::WHITE).boxed(TILE_BUTTON);
        let lit = crate::theme::mix(DISC, Color::WHITE, 0.85);
        let face = button(face)
            .padding(0)
            .width(TILE_BUTTON)
            .height(TILE_BUTTON)
            .on_press(message)
            .style(lit_style(Some(DISC), lit, TILE_BUTTON / 2.0));
        iced::widget::tooltip(
            face,
            controls::tip(self.look.theme, label),
            iced::widget::tooltip::Position::Top,
        )
        .gap(6)
        .into()
    }

    /// Discord's card over the dimmed window while files are dragged over
    /// it, or why a drop didn't go.
    pub fn drop_view(self) -> Option<Element<'a, Message>> {
        let theme = self.look.theme;
        let state = self.state;
        let aside = self.uploads.drop.aside && self.thread_shown();
        let dragging = self.uploads.drop.dragging
            && if aside {
                state.aside_can_attach()
            } else {
                state.can_attach()
            };
        if !dragging && self.uploads.drop.refusal.is_none() {
            return None;
        }
        let ink = theme.on_accent;
        let mut lines =
            column![container(self.look.glyph(Icon::File, 40, ink)).center_x(Length::Fill)]
                .spacing(8)
                .padding(Padding {
                    top: 24.0,
                    right: 20.0,
                    bottom: 20.0,
                    left: 20.0,
                })
                .align_x(Alignment::Center);
        if dragging {
            lines = lines
                .push(
                    row![
                        text("Upload to ").size(20).color(ink),
                        text(if aside {
                            state
                                .aside_room()
                                .map(|room| state.destination(room))
                                .unwrap_or_default()
                        } else {
                            state.upload_destination()
                        })
                        .size(20)
                        .font(BOLD)
                        .color(ink)
                        .wrapping(text::Wrapping::None),
                    ]
                    .width(Length::Shrink),
                )
                .push(
                    text("You can add comments before uploading.")
                        .size(15)
                        .color(ink)
                        .align_x(Alignment::Center),
                );
        } else if let Some((title, help)) = &self.uploads.drop.refusal {
            lines = lines
                .push(
                    text(title.clone())
                        .size(20)
                        .font(BOLD)
                        .color(ink)
                        .align_x(Alignment::Center),
                )
                .push(
                    text(help.clone())
                        .size(15)
                        .color(ink)
                        .align_x(Alignment::Center),
                )
                .push(
                    container(controls::action(
                        theme,
                        "Got It",
                        controls::Kind::Plain,
                        Some(Message::Write(Write::Uploads(Event::Understood))),
                    ))
                    .padding(Padding::ZERO.top(8)),
                );
        }
        let frame_ink = Color { a: 0.6, ..ink };
        let framed = container(lines)
            .width(Length::Fill)
            .style(move |_| container::Style {
                border: Border {
                    color: frame_ink,
                    width: 2.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            });
        let ground = if dragging { theme.accent } else { theme.danger };
        let card = container(framed)
            .padding(8)
            .width(CARD)
            .style(move |_| container::Style {
                shadow: card_shadow(),
                ..rounded(ground, 10)
            });
        Some(
            iced::widget::mouse_area(
                container(card)
                    .center(Length::Fill)
                    .style(|_| crate::style::fill(DROP_DIM)),
            )
            .on_press(Message::Write(Write::Uploads(Event::Understood)))
            .into(),
        )
    }
}
