//! A server's rows: headings, links, channels with their threads under
//! them, and their faces and inks.

use iced::{
    Alignment, Color, Element, Length, Padding, Point, Rectangle, Size,
    widget::{Space, column, container, mouse_area, row, stack, text},
};

use crate::{
    app::Message,
    controls::{self, ROW_COUNT, bar, count_pill},
    elide::elide,
    fonts::{BOLD, MEDIUM, REGULAR, SEMIBOLD},
    pictures::Icon,
    style::{corners, faded, rounded},
    widgets::{Drags, Pic, RightPress},
};

use super::{CHAT_BUTTON, Event, GAP, HEADING, INSET, ROW, Room, SideRow, VOICE_CARD_GAP};

fn themed_count_pill<'a>(
    theme: &crate::theme::Theme,
    count: &str,
    badge: yapfast_core::themes::lists::MentionBadge,
) -> Element<'a, Message> {
    if badge.ground.is_none() && badge.ink.is_none() && badge.radius.is_none() {
        return count_pill(theme, count, ROW_COUNT, None);
    }
    let ground = badge
        .ground
        .map(crate::themes::argb)
        .unwrap_or(theme.danger);
    let ink = badge
        .ink
        .map(crate::themes::argb)
        .unwrap_or(theme.on_danger);
    let radius = badge.radius.unwrap_or(ROW_COUNT.height / 2.0);
    let number = text(count.to_owned())
        .size(ROW_COUNT.size)
        .font(BOLD)
        .color(ink)
        .wrapping(text::Wrapping::None);
    let mut pill = container(number)
        .padding([0.0, ROW_COUNT.pad])
        .height(ROW_COUNT.height)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);
    if count.chars().count() < 2 {
        pill = pill.width(ROW_COUNT.height);
    }
    pill.style(move |_| rounded(ground, radius)).into()
}

const THREAD_INDENT: f32 = 28.0;
/// The stem's left edge within the row, aligned under the channel icon.
const THREAD_STEM: f32 = 16.0;
/// Space between the parent row and the top of its thread connector.
const THREAD_STEM_INSET: f32 = 1.0;

impl<'a> super::View<'a> {
    pub(super) fn side_row(
        self,
        row: &'a SideRow,
        place: Option<usize>,
        after_room: bool,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let row_gap = self
            .cx
            .themes
            .channel_rows(&theme.id)
            .and_then(|rows| rows.gap)
            .unwrap_or(GAP);
        let inner: Element<'a, Message> = match row {
            SideRow::Screens {
                friends,
                requests,
                badge,
            } => column![
                self.link_row(
                    ("screen:friends", Icon::Users, "Friends"),
                    *friends,
                    "",
                    Message::Side(Event::Friends),
                    true
                ),
                self.link_row(
                    ("screen:requests", Icon::Inbox, "Requests"),
                    *requests,
                    badge,
                    Message::Side(Event::Requests),
                    true
                ),
            ]
            .spacing(GAP)
            .padding(Padding {
                top: 9.0,
                bottom: 6.0,
                ..Padding::ZERO
            })
            .into(),
            SideRow::DmHeading => row![
                text("Direct Messages")
                    .size(12.0)
                    .font(SEMIBOLD)
                    .color(theme.muted)
                    .width(Length::Fill),
                self.cx
                    .look
                    .icon_button(
                        Icon::UserPlus,
                        "Add a friend",
                        Message::Friends(crate::friends::Event::OpenAdd)
                    )
                    .size(24.0, 16)
                    .below(),
                self.cx
                    .look
                    .icon_button(
                        Icon::SquarePen,
                        "New message",
                        Message::Switcher(crate::switcher::Event::OpenNewDm)
                    )
                    .size(24.0, 16)
                    .below(),
            ]
            .spacing(4)
            .padding(Padding {
                left: 8.0,
                ..Padding::ZERO
            })
            .height(40)
            .align_y(Alignment::Center)
            .into(),
            SideRow::Browse { open } => container(self.link_row(
                ("screen:browse", Icon::Hash, "Browse channels"),
                *open,
                "",
                Message::Side(Event::Browse),
                false,
            ))
            .padding([6, 0])
            .into(),
            SideRow::Pinned => container(
                text("PINNED CHANNELS")
                    .size(12.0)
                    .font(BOLD)
                    .color(theme.muted),
            )
            .padding(Padding {
                left: 16.0,
                ..Padding::ZERO
            })
            .height(36)
            .align_y(Alignment::Center)
            .into(),
            SideRow::PinnedEnd => container(bar(theme.line, Length::Fill, 1))
                .height(9)
                .align_y(Alignment::Center)
                .into(),
            SideRow::Heading {
                name,
                fold,
                collapsed,
            } => self.heading(name, fold, *collapsed, place),
            SideRow::Room(room) => self.room_rows(room, place),
            SideRow::Empty(words) => container(
                text(*words)
                    .size(theme.type_size("body", 14.0))
                    .font(theme.font("body", REGULAR))
                    .color(theme.muted),
            )
            .padding(8)
            .into(),
        };
        let (inner, gap) = match row {
            // A heading sits 2 px over its first row, whatever the gap.
            SideRow::Heading { .. } => (inner, GAP),
            // The connector takes up the space through the row's gap.
            SideRow::Room(room) if room.thread => (inner, 0.0),
            // The stem starts a pixel under its channel, rounded at its
            // top, and runs on through the gap to the first thread.
            SideRow::Room(room) if room.has_threads && row_gap > THREAD_STEM_INSET => {
                let ink = theme.reply_spine;
                let icon = self
                    .cx
                    .look
                    .icon(Icon::ThreadCap, 2, crate::theme::rgb(ink));
                let cap = Pic::new(icon, Size::new(2.0, 1.0))
                    .at(Rectangle::with_size(Size::new(2.0, 2.0)))
                    .opacity(ink.a);
                let below = row_gap - THREAD_STEM_INSET - 1.0;
                let stem: Element<'a, Message> = if below > 0.0 {
                    column![cap, bar(ink, 2.0, below)].into()
                } else {
                    cap.into()
                };
                (
                    column![
                        inner,
                        controls::placed(stem, THREAD_STEM, THREAD_STEM_INSET)
                    ]
                    .into(),
                    0.0,
                )
            }
            _ => (inner, row_gap),
        };
        let inset = container(inner)
            .padding(Padding {
                left: INSET,
                right: INSET,
                bottom: gap,
                top: 0.0,
            })
            .width(Length::Fill);
        let stripe = match row {
            SideRow::Room(_) => place.and_then(|index| {
                theme
                    .surface_stripe("row")
                    .filter(|stripe| stripe.channels && index % 2 == 1)
                    .map(|stripe| stripe.colour)
            }),
            _ => None,
        };
        let inset: Element<'a, Message> = match stripe {
            Some(ground) => container(inset)
                .width(Length::Fill)
                .style(move |_| crate::style::fill(ground))
                .into(),
            None => inset.into(),
        };
        // A row that may be dragged: where it would land marked by a line
        // across its top (dragged up) or its foot (dragged down).
        let inset: Element<'a, Message> = match self.s.drop_line(place) {
            Some(below) => {
                // From 8 px into the row to its end.
                let line = container(self.drop_line_view()).padding(Padding {
                    left: 2.0 * INSET,
                    right: INSET,
                    ..Padding::ZERO
                });
                let y = if below {
                    row.height(theme, row_gap) - crate::arrange::LINE
                } else {
                    0.0
                };
                stack![inset, controls::placed(line, 0.0, y)].into()
            }
            None => inset,
        };
        // Where someone dragged from a voice channel would drop: outlined
        // in the drop line's green.
        let inset: Element<'a, Message> = match row {
            SideRow::Room(room) if self.s.move_aimed(&room.id) => {
                let live = theme.live;
                let outline = container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(move |_| iced::widget::container::Style {
                        border: iced::Border {
                            color: live,
                            width: 2.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    });
                stack![
                    inset,
                    container(outline).padding(Padding {
                        left: INSET,
                        right: INSET,
                        bottom: GAP,
                        top: 0.0,
                    })
                ]
                .into()
            }
            _ => inset,
        };
        // While someone is dragged, a voice channel's row says when the
        // pointer comes onto it, a folded category's busy one too.
        let onto = match row {
            SideRow::Room(room) if room.voice => Some(room.id.as_str()),
            _ => None,
        };
        let moving = self.s.moving_person.is_some();
        let over = move |place: Option<usize>, over: bool| match (onto, place) {
            (Some(room), _) if moving => {
                Message::Arrange(crate::arrange::Event::Onto(room.to_owned(), over))
            }
            (_, Some(place)) if !moving => {
                Message::Arrange(crate::arrange::Event::Over(place, over))
            }
            _ => Message::Nothing,
        };
        let inset: Element<'a, Message> = match place {
            Some(place) => {
                let click = match row {
                    SideRow::Heading { fold, .. } => Message::Side(Event::Toggle(fold.clone())),
                    SideRow::Room(room) if room.voice => {
                        Message::Side(Event::Voice(room.id.clone()))
                    }
                    SideRow::Room(room) => Message::Side(Event::Select(room.id.clone())),
                    _ => return inset,
                };
                let head = match row {
                    SideRow::Room(room) => room.row_height(theme),
                    _ => HEADING,
                };
                Drags::new(inset)
                    .click(click)
                    .head(head)
                    .drag(move |grab| Message::Arrange(crate::arrange::Event::Start(place, grab)))
                    .tracking(self.s.dragging(), move |on| over(Some(place), on))
                    .into()
            }
            None if onto.is_some() => Drags::new(inset)
                .tracking(moving, move |on| over(None, on))
                .into(),
            None => inset,
        };
        let inset = match row {
            // Unread activity is marked at the sidebar's edge, as on the
            // rail, a thread's too.
            SideRow::Room(room)
                if room.unread
                    && !room.selected
                    && self
                        .cx
                        .themes
                        .channel_rows(&theme.id)
                        .and_then(|rows| rows.meta.unread)
                        .is_none() =>
            {
                let ink = theme.text;
                let mark = container(Space::new())
                    .width(4)
                    .height(8)
                    .style(move |_| rounded(ink, corners([0.0, 4.0, 4.0, 0.0])));
                stack![
                    inset,
                    controls::placed(mark, 0.0, (room.row_height(theme) - 8.0) / 2.0)
                ]
                .into()
            }
            _ => inset,
        };
        if matches!(row, SideRow::Room(_))
            && after_room
            && let Some(separator) = self
                .cx
                .themes
                .channel_rows(&theme.id)
                .and_then(|rows| rows.separator.as_ref())
        {
            let separator: Option<(Element<'a, Message>, f32)> = match separator {
                yapfast_core::themes::lists::Separator::Line {
                    colour,
                    width,
                    inset: (left, right),
                } => Some((
                    // Its inset is from the row, which sits INSET in.
                    container(bar(crate::themes::argb(*colour), Length::Fill, *width))
                        .padding(Padding {
                            left: *left + INSET,
                            right: *right + INSET,
                            ..Padding::ZERO
                        })
                        .into(),
                    *width,
                )),
                yapfast_core::themes::lists::Separator::Art { art, height } => {
                    crate::theme_art::tile_art(
                        self.cx,
                        yapfast_core::themes::package::art::ArtSurface::Sidebar,
                        art,
                        super::width(self.cx.layout, theme),
                        *height,
                    )
                    .map(|art| (art, *height))
                }
            };
            if let Some((separator, _)) = separator {
                return stack![inset, controls::placed(separator, 0.0, 0.0)].into();
            }
        }
        inset
    }

    /// A category's heading: 17 px above its name, the chevron after it
    /// turned while folded; a click folds it or opens it, and a Discord
    /// category's opens its menu on a right click.
    pub(super) fn heading(
        self,
        name: &'a str,
        fold: &'a str,
        collapsed: bool,
        place: Option<usize>,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let plain_name = name;
        let name = self.cx.themes.label(
            &theme.id,
            yapfast_core::themes::chrome::Label::Category,
            name,
            &[("name", name)],
        );
        let rows = self.cx.themes.channel_rows(&theme.id);
        let headers = rows.map(|rows| &rows.headers);
        // `{n}`: the category's place among the list's categories, from 1.
        let ordinal = self
            .s
            .rows
            .iter()
            .filter(|row| matches!(row, super::SideRow::Heading { .. }))
            .position(|row| matches!(row, super::SideRow::Heading { fold: at, .. } if at == fold))
            .map(|index| index + 1)
            .or(place.map(|place| place + 1));
        let place_text = ordinal.map_or_else(String::new, |n| n.to_string());
        // The list's own heading words take the category's plain name (the
        // chrome's `category` label would mark it twice).
        let name = if let Some(headers) = headers.filter(|headers| !headers.text.is_empty()) {
            yapfast_core::themes::chrome::fill(
                &headers.text,
                &[("name", plain_name), ("n", place_text.as_str())],
            )
        } else {
            name
        };
        let name = crate::themes::list_case(headers.and_then(|headers| headers.case), &name);
        let name = theme.surface_words("label", &name).into_owned();
        // A theme's categories as directories, `▾ TEXT CHANNELS/`, with no
        // chevron of their own.
        let folders = self
            .cx
            .themes
            .chrome(&theme.id)
            .and_then(|chrome| chrome.folders.as_ref());
        let name = match folders {
            Some(folders) => format!(
                "{} {name}{}",
                if collapsed {
                    &folders.closed
                } else {
                    &folders.open
                },
                folders.suffix
            ),
            None => name,
        };
        let id = format!("heading:{fold}");
        let pointed = self.cx.pointed.side.as_deref() == Some(id.as_str());
        let stock_ink = if pointed {
            theme.secondary
        } else {
            theme.channel_read
        };
        // A theme's label bar (`shape.label`) under a stock heading: its
        // words in the bar's ink, as Slint's `Trim.label`.
        let label_bar = headers
            .is_none_or(|headers| *headers == Default::default())
            .then(|| theme.surface_ground("label"))
            .flatten();
        let ink = headers
            .and_then(|headers| headers.colour)
            .map(crate::themes::argb)
            .or_else(|| {
                label_bar
                    .is_some()
                    .then(|| theme.surface_text("label"))
                    .flatten()
            })
            .unwrap_or(stock_ink);
        // While a drag is on, as a text channel's row.
        let ink = if self.s.arrange_dimmed(None, false) {
            faded(ink, crate::arrange::DIM)
        } else {
            ink
        };
        // As Slint's category heading: the label face only on a theme's
        // label bar (`Trim.label`), else the window's.
        let header_font = crate::themes::list_font(
            theme,
            headers.and_then(|h| h.font),
            if label_bar.is_some() {
                "labels"
            } else {
                "body"
            },
            headers.and_then(|h| h.weight),
            SEMIBOLD,
        );
        let header_shadow = crate::themes::offset_shadow(headers.and_then(|h| h.shadow));
        let framed_words = headers.is_some_and(|headers| headers.frame.is_some());
        let plain_heading = name.clone();
        let name = elide(name);
        // A frame hugs the words, so they take their own width.
        let name = if framed_words { name.shrink() } else { name };
        let size = headers.and_then(|headers| headers.size).unwrap_or_else(|| {
            if label_bar.is_some() {
                theme.type_size("labels", 14.0)
            } else {
                14.0
            }
        });
        // A theme's ransom-note headings, as Slint cuts them.
        let ransom = headers
            .filter(|headers| headers.style == yapfast_core::themes::lists::HeaderStyle::Ransom)
            .and_then(|_| crate::theme_chrome::ransom(self.cx, &plain_heading, size));
        // A ransom heading stands in the middle of its height, as Slint's.
        let centred = ransom.is_some();
        let words: Element<'a, Message> = match ransom {
            Some(ransom) => ransom,
            None => name
                .size(size)
                .font(header_font)
                .color(ink)
                .light(header_shadow)
                .into(),
        };
        let mut words = row![words].spacing(4).align_y(Alignment::Center);
        if headers.and_then(|headers| headers.chevron) != Some(false) && folders.is_none() {
            words = words.push(self.cx.look.glyph(
                if collapsed {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                },
                12,
                ink,
            ));
        }
        // Discord's "+", while the pointer is over a category this account
        // may make channels in.
        let plus = (pointed && self.cx.app.may_create_channel(fold)).then(|| {
            self.cx
                .look
                .icon_button(
                    Icon::Add,
                    "Create Channel",
                    Message::Side(Event::CreateIn(fold.to_owned())),
                )
                .size(CHAT_BUTTON, 16)
        });
        let border = theme.surface_border("label", 0.0, 0.0, Color::TRANSPARENT);
        let shadow = theme.surface_shadow("label", iced::Shadow::default());
        let surface = yapfast_core::themes::package::art::ArtSurface::Label;
        let decorated = self.cx.themes.decorates(&theme.id, surface);
        let background = (!decorated)
            .then(|| theme.surface_ground("label"))
            .flatten();
        let height = headers
            .and_then(|headers| headers.height)
            .unwrap_or(HEADING);
        let fill_width = headers.is_some_and(|h| h.fill);
        let inset_left = headers
            .and_then(|headers| headers.inset)
            .unwrap_or(if fill_width { 6.0 } else { 8.0 });
        let body: Element<'a, Message> =
            if let Some(frame) = headers.and_then(|headers| headers.frame.as_ref()) {
                // As Slint: a frame 22 px tall hugging the words and what
                // follows them, the words 12 px in (6 px into one across the
                // list), centred in the heading.
                let framed = crate::theme_art::framed_heading(
                    self.cx,
                    yapfast_core::themes::package::art::ArtSurface::Sidebar,
                    frame,
                    words.into(),
                    // Slint lays the words in the heading's last 21 px.
                    if fill_width { height } else { height.min(21.0) },
                    22.0,
                    fill_width,
                    Padding {
                        top: 0.0,
                        right: if fill_width { 6.0 } else { 10.0 },
                        bottom: 0.0,
                        left: if fill_width { 6.0 } else { 12.0 },
                    },
                );
                let mut line = row![framed].align_y(Alignment::Center);
                if let Some(plus) = plus {
                    line = line.push(plus);
                }
                container(line)
                    .padding(Padding {
                        left: inset_left,
                        right: inset_left,
                        top: if fill_width {
                            0.0
                        } else {
                            (height - 21.0).max(0.0)
                        },
                        ..Padding::ZERO
                    })
                    .height(height)
                    .width(Length::Fill)
                    .align_y(Alignment::Center)
                    .into()
            } else {
                // The words take their own width, the chevron right after them
                // (a Fill spacer beside them would halve the room the words
                // may take); the "+" goes to the far end.
                let face = match plus {
                    Some(plus) => row![words, Space::new().width(Length::Fill), plus],
                    None => row![words],
                }
                .align_y(Alignment::Center);
                let words = container(face)
                    .padding(Padding {
                        left: inset_left,
                        right: if fill_width { 6.0 } else { 8.0 },
                        top: if centred {
                            0.0
                        } else if fill_width {
                            4.0
                        } else {
                            17.0
                        },
                        bottom: 0.0,
                    })
                    .height(height)
                    .width(Length::Fill)
                    .align_y(Alignment::Center);
                match label_bar {
                    // As Slint: the bar 8 px in, 15 px down and 22 px tall,
                    // under the words, not the whole heading.
                    Some(bar_ground) => {
                        let bar_ = container(Space::new())
                            .width(Length::Fill)
                            .height(22)
                            .style(move |_| container::Style {
                                background: Some(bar_ground),
                                border,
                                shadow,
                                ..Default::default()
                            });
                        stack![
                            container(bar_).padding(Padding {
                                top: 15.0,
                                left: 8.0,
                                right: 8.0,
                                bottom: 0.0,
                            }),
                            words
                        ]
                        .height(height)
                        .into()
                    }
                    None => words
                        .style(move |_| container::Style {
                            background,
                            border,
                            shadow,
                            ..Default::default()
                        })
                        .into(),
                }
            };
        let body = crate::theme_art::surface(body, self.cx, surface);
        let area = mouse_area(body)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_enter(Message::Side(Event::Point(id.clone(), true)))
            .on_exit(Message::Side(Event::Point(id, false)));
        // One that may be dragged folds on a click, not a press (`Drags`).
        let area = if place.is_none() {
            area.on_press(Message::Side(Event::Toggle(fold.to_owned())))
        } else {
            area
        };
        if !self.cx.state.is_category(fold) {
            return area.into();
        }
        // Where in the window it was pressed: the list's own space is under
        // the header, and scrolled.
        let (room, spot) = (fold.to_owned(), self.cx.pressed);
        RightPress::new(area, move |_| {
            Message::Side(Event::Menu(room.clone(), spot.get()))
        })
        .into()
    }

    /// A row that opens one of the DM home's screens or Browse Channels; the
    /// screens' rows are `tall`, Discord's 38 px with its own greys.
    pub(super) fn link_row(
        self,
        (id, icon, label): (&'static str, Icon, &'static str),
        selected: bool,
        badge: &str,
        message: Message,
        tall: bool,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let pointed = self.cx.pointed.side.as_deref() == Some(id);
        let (open, read) = if tall {
            (theme.nav_open, theme.nav_read)
        } else {
            (theme.text, theme.muted)
        };
        let ink = if selected {
            open
        } else if pointed {
            theme.secondary
        } else {
            read
        };
        let icon_ink = match (selected, tall) {
            (true, _) => open,
            (false, true) => theme.nav_read,
            (false, false) => theme.faint,
        };
        let mut face = row![
            self.cx.look.glyph(icon, 20, icon_ink),
            text(label)
                .size(theme.type_size("body", 15.0))
                .font(theme.font("body", MEDIUM))
                .color(ink)
                .width(Length::Fill)
        ]
        .spacing(if tall { 13 } else { 8 })
        .align_y(Alignment::Center);
        if !badge.is_empty() {
            face = face.push(count_pill(theme, badge, ROW_COUNT, None));
        }
        let height = super::link_height(tall);
        self.row_face(
            id.into(),
            face.into(),
            height,
            selected,
            Some(message),
            None,
        )
    }

    /// A row's face: lit while selected or pointed at, a press (unless a
    /// click is the row's `Drags`) and a right click on it.
    pub(super) fn row_face(
        self,
        id: String,
        face: Element<'a, Message>,
        height: f32,
        selected: bool,
        message: Option<Message>,
        menu: Option<String>,
    ) -> Element<'a, Message> {
        self.row_face_listed(id, face, height, selected, message, menu, true)
    }

    /// [`row_face`](Self::row_face), dressed by a theme's channel rows only
    /// when `listed`: a DM's row keeps its own look, as Slint's (Zine's label
    /// tape is for channels).
    #[allow(
        clippy::too_many_arguments,
        reason = "each part it draws is its own input"
    )]
    pub(super) fn row_face_listed(
        self,
        id: String,
        face: Element<'a, Message>,
        height: f32,
        selected: bool,
        message: Option<Message>,
        menu: Option<String>,
        listed: bool,
    ) -> Element<'a, Message> {
        listed_row_face(
            self.cx, id, face, height, selected, message, menu, listed, None,
        )
    }

    /// What a row's face takes: the pointer over it, a press (unless a
    /// click is the row's `Drags`) and a right click on it.
    pub(super) fn row_presses(
        self,
        id: String,
        body: Element<'a, Message>,
        message: Option<Message>,
        menu: Option<String>,
    ) -> Element<'a, Message> {
        let area = mouse_area(body)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_enter(Message::Side(Event::Point(id.clone(), true)))
            .on_exit(Message::Side(Event::Point(id, false)));
        let area = match message {
            Some(message) => area.on_press(message),
            None => area,
        };
        match menu {
            Some(room) => {
                let spot = self.cx.pressed;
                RightPress::new(area, move |_| {
                    Message::Side(Event::Menu(room.clone(), spot.get()))
                })
                .into()
            }
            None => area.into(),
        }
    }

    /// A conversation's row, with its thread line, and who is in it for a
    /// busy voice channel.
    pub(super) fn room_rows(self, room: &'a Room, place: Option<usize>) -> Element<'a, Message> {
        let line = self.room_row(room, place.is_some());
        if room.thread {
            return row![self.thread_line(room), line].into();
        }
        if room.people.is_empty() {
            return line;
        }
        let mut people = column![].padding(Padding {
            left: 32.0,
            bottom: 4.0,
            ..Padding::ZERO
        });
        // The call this account is in shows who speaks, in the call's
        // order: this account first.
        let voice = &self.cx.state.voice;
        let call = voice.active() && voice.room.as_deref() == Some(room.id.as_str());
        let mut listed: Vec<_> = room
            .people
            .iter()
            .map(|person| {
                let live = call
                    .then(|| {
                        voice
                            .participants
                            .iter()
                            .position(|live| live.person == person.id || live.id == person.id)
                    })
                    .flatten();
                (live, person)
            })
            .collect();
        if call {
            listed.sort_by_key(|(live, _)| live.unwrap_or(usize::MAX));
        }
        for (live, person) in listed {
            let live = live.map(|place| &voice.participants[place]);
            people = people.push(self.voice_person(&room.id, person, live));
        }
        column![line, people].spacing(GAP).into()
    }

    /// The line into a thread and, while another follows, down through
    /// the gap to the next row: the connector's 34 px square with its arm
    /// at the row's middle, and the stem drawn on through what a theme's
    /// taller row or wider gap leaves above or below it. The square's
    /// right edge is transparent.
    fn thread_line(self, room: &Room) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let height = room.row_height(theme);
        let gap = self
            .cx
            .themes
            .channel_rows(&theme.id)
            .and_then(|rows| rows.gap)
            .unwrap_or(GAP);
        let icon = if room.thread_continues {
            Icon::ThreadBranch
        } else {
            Icon::ThreadEnd
        };
        let side = ROW + GAP;
        let ink = theme.reply_spine;
        let handle = self.cx.look.icon(icon, side as u32, crate::theme::rgb(ink));
        let total = height + gap;
        // The arm is at the middle of the square's 32 px row.
        let top = (height - ROW) / 2.0;
        let mut layers = stack![
            Pic::new(handle, Size::new(THREAD_INDENT, total))
                .at(Rectangle::new(Point::new(0.0, top), Size::new(side, side)))
                .opacity(ink.a)
        ]
        .width(THREAD_INDENT)
        .height(total);
        let stem =
            |from: f32, to: f32| controls::placed(bar(ink, 2.0, to - from), THREAD_STEM, from);
        if top > 0.0 {
            layers = layers.push(stem(0.0, top));
        }
        if room.thread_continues && top + side < total {
            layers = layers.push(stem(top + side, total));
        }
        layers.into()
    }

    /// A conversation's row; one that may be `dragged` is clicked through
    /// its `Drags`.
    pub(super) fn room_row(self, room: &'a Room, dragged: bool) -> Element<'a, Message> {
        if room.direct {
            return self.dm_row(room);
        }
        let theme = self.cx.look.theme;
        let list = self.cx.themes.channel_rows(&theme.id);
        let pointed = self.cx.pointed.side.as_deref() == Some(room.id.as_str());
        let (ink, opacity) = self.row_ink(room, pointed);
        // While a drag is on, rows it can't land among dim.
        let (ink, opacity) = if self.s.arrange_dimmed(Some(&room.id), room.voice) {
            (
                faded(ink, crate::arrange::DIM),
                opacity * crate::arrange::DIM,
            )
        } else {
            (ink, opacity)
        };
        let icon_size = list.and_then(|rows| rows.icon.size).unwrap_or(20.0);
        let icon_gap = list.and_then(|rows| rows.icon.gap).unwrap_or(8.0);
        let mut face = row![].spacing(icon_gap).align_y(Alignment::Center);
        if room.selected
            && let Some(prefix_art) = list.and_then(|rows| rows.selected.prefix_art.as_ref())
            && let Some(prefix) = crate::theme_art::tile_art(
                self.cx,
                yapfast_core::themes::package::art::ArtSurface::Sidebar,
                prefix_art,
                icon_size,
                icon_size,
            )
        {
            face = face.push(prefix);
        } else if room.selected
            && let Some(prefix) = list
                .map(|rows| rows.selected.prefix.as_str())
                .filter(|prefix| !prefix.is_empty())
        {
            let colour = list
                .and_then(|rows| rows.selected.prefix_colour)
                .map(crate::themes::argb)
                .unwrap_or(ink);
            face = face.push(text(prefix.to_owned()).size(icon_size).color(colour));
        }
        if !room.thread {
            let stock_icon = if room.selected {
                theme.channel_white
            } else if room.unread {
                theme.secondary
            } else {
                theme.channel_read
            };
            let icon_ink = list.map_or(stock_icon, |rows| {
                if rows.icon.follow_ink {
                    ink
                } else if room.selected {
                    rows.icon
                        .selected
                        .map(crate::themes::argb)
                        .unwrap_or(stock_icon)
                } else if pointed {
                    rows.icon
                        .hover
                        .map(crate::themes::argb)
                        .unwrap_or(stock_icon)
                } else {
                    rows.icon
                        .colour
                        .map(crate::themes::argb)
                        .unwrap_or(stock_icon)
                }
            });
            // A list's own icon colours take even a coloured icon pack's
            // icons, as Slint draws them.
            let themed_ink = list.is_some_and(|rows| {
                rows.icon.follow_ink
                    || if room.selected {
                        rows.icon.selected.is_some()
                    } else {
                        rows.icon.colour.is_some()
                    }
            });
            let glyph = if themed_ink {
                crate::look::Look::tinted_glyph
            } else {
                crate::look::Look::glyph
            };
            face = face.push(glyph(
                self.cx.look,
                room.icon,
                icon_size.round() as u32,
                faded(icon_ink, opacity),
            ));
        }
        let marked = self.cx.themes.marked(
            &theme.id,
            yapfast_core::themes::chrome::Place::Channel,
            &room.name,
        );
        let name = crate::themes::list_case(list.and_then(|rows| rows.case), &marked);
        let row_role = list
            .and_then(|rows| rows.font)
            .map_or("body", yapfast_core::themes::lists::FontRole::name);
        let row_font = crate::themes::list_font(
            theme,
            list.and_then(|rows| rows.font),
            "body",
            list.and_then(|rows| rows.weight),
            MEDIUM,
        );
        let selected_shadow = if room.selected {
            crate::themes::offset_shadow(
                list.and_then(|rows| rows.ink.selected_shadow)
                    .map(|c| (c, 0.0, 1.0)),
            )
        } else {
            crate::piece::TextLight::default()
        };
        let name = elide(name.into_owned())
            .size(
                list.and_then(|rows| rows.size)
                    .unwrap_or_else(|| theme.type_size(row_role, 15.0)),
            )
            .font(row_font)
            .color(ink)
            .light(selected_shadow);
        // A theme's frame fitted to the name takes only the name's width.
        let fit = list.is_some_and(|rows| rows.fit_text);
        // As Slint's `ListedName`: the selected underline is as wide as the
        // words, so a selected name with one hugs its words.
        let underlined =
            room.selected && list.is_some_and(|rows| rows.selected.underline.is_some());
        let hug = fit || underlined;
        let name = if hug { name.shrink() } else { name };
        let mut name = column![name];
        if room.selected
            && let Some(underline) = list.and_then(|rows| rows.selected.underline)
        {
            name = name.spacing(underline.offset).push(bar(
                crate::themes::argb(underline.colour),
                Length::Fill,
                underline.width,
            ));
        }
        // Set after the bar: a filling child would widen the column.
        let name = name.width(if hug { Length::Shrink } else { Length::Fill });
        face = face.push(name);
        if let Some(started) = room.started {
            let lasted = yapfast_core::clock::lasted(started, self.cx.minute);
            face = face.push(
                text(lasted)
                    .size(theme.type_size("small", 12.0))
                    .font(theme.font("small", crate::fonts::REGULAR))
                    .color(theme.muted),
            );
        }
        if room.voice
            && let Some(meta) = list.map(|rows| &rows.meta)
            && !meta.voice.is_empty()
        {
            let count = room.people.len().to_string();
            let words = yapfast_core::themes::chrome::fill(&meta.voice, &[("n", count.as_str())]);
            let meta_role = meta
                .font
                .map_or("small", yapfast_core::themes::lists::FontRole::name);
            face = face.push(
                text(words)
                    .size(
                        meta.size
                            .unwrap_or_else(|| theme.type_size(meta_role, 11.0)),
                    )
                    .font(theme.font(meta_role, crate::fonts::REGULAR))
                    .color(meta.colour.map(crate::themes::argb).unwrap_or(theme.muted)),
            );
        }
        if !room.badge.is_empty() {
            let badge = list.map(|rows| rows.meta.badge).unwrap_or_default();
            face = face.push(themed_count_pill(theme, &room.badge, badge));
        } else if room.unread
            && list.and_then(|rows| rows.meta.unread)
                == Some(yapfast_core::themes::lists::UnreadMark::Badge)
        {
            let badge = list.map(|rows| rows.meta.badge).unwrap_or_default();
            face = face.push(themed_count_pill(theme, "•", badge));
        } else if room.unread
            && list.and_then(|rows| rows.meta.unread)
                == Some(yapfast_core::themes::lists::UnreadMark::Glyph)
            && let Some(glyph) = list.and_then(|rows| rows.meta.unread_glyph.as_ref())
        {
            let side = list.and_then(|rows| rows.meta.size).unwrap_or(11.0);
            match glyph {
                yapfast_core::themes::lists::Glyph::Text(glyph) => {
                    face = face.push(
                        text(glyph.clone()).size(side).color(
                            list.and_then(|rows| rows.meta.colour)
                                .map(crate::themes::argb)
                                .unwrap_or(theme.muted),
                        ),
                    );
                }
                yapfast_core::themes::lists::Glyph::Art(art) => {
                    if let Some(art) = crate::theme_art::tile_art(
                        self.cx,
                        yapfast_core::themes::package::art::ArtSurface::Sidebar,
                        art,
                        side,
                        side,
                    ) {
                        face = face.push(art);
                    }
                }
            }
        } else if !room.posts.is_empty() {
            // The forum's unread posts, a bold number in the nav grey.
            face = face.push(
                text(room.posts.as_str())
                    .size(11)
                    .font(BOLD)
                    .color(theme.nav_read),
            );
        }
        // Discord's invite button on a channel's row, under the pointer or
        // while it's selected, for one who may invite.
        if self.s.header.invite && !room.thread && (pointed || room.selected) {
            face = face.push(
                self.cx
                    .look
                    .icon_button(
                        Icon::UserPlus,
                        "Create invite",
                        Message::Side(Event::InviteTo(room.id.clone())),
                    )
                    .size(CHAT_BUTTON, 16),
            );
        }
        // A voice channel's chat opens without joining it, from a button
        // shown under the pointer or while it's selected, as in Discord.
        if room.chat && (pointed || room.selected) {
            face = face.push(
                self.cx
                    .look
                    .icon_button(
                        Icon::Forum,
                        "Open chat",
                        Message::Side(Event::OpenChat(room.id.clone())),
                    )
                    .size(CHAT_BUTTON, 16),
            );
        }
        let message = if room.voice {
            Message::Side(Event::Voice(room.id.clone()))
        } else {
            Message::Side(Event::Select(room.id.clone()))
        };
        let row = self.row_face(
            room.id.clone(),
            face.into(),
            room.row_height(theme),
            room.selected,
            (!dragged).then_some(message),
            Some(room.id.clone()),
        );
        // A busy voice channel's card beside it, under the pointer.
        if pointed && room.voice && !room.people.is_empty() && !self.s.voice_card_off {
            return iced::widget::tooltip(
                row,
                self.voice_channel_card(&room.people),
                iced::widget::tooltip::Position::Right,
            )
            .gap(VOICE_CARD_GAP)
            .snap_within_viewport(true)
            .into();
        }
        row
    }

    /// A row's ink, and its opacity: a muted one is faint until it's
    /// selected or pointed at.
    pub(super) fn row_ink(self, room: &Room, pointed: bool) -> (Color, f32) {
        let theme = self.cx.look.theme;
        let stock = if room.locked {
            if room.selected {
                theme.muted
            } else {
                theme.faint
            }
        } else if room.selected {
            // Discord's channel list inks; a DM keeps the text colours.
            theme.token_colour(
                "row-selected-text",
                if room.direct {
                    theme.text
                } else {
                    theme.channel_white
                },
            )
        } else if room.unread {
            if room.direct {
                theme.text
            } else {
                theme.channel_white
            }
        } else if pointed {
            theme.secondary
        } else if room.direct {
            theme.muted
        } else {
            theme.channel_read
        };
        let ink = self
            .cx
            .themes
            .channel_rows(&theme.id)
            // A DM's row isn't a channel row: its own inks, as Slint's.
            .filter(|_| !room.direct)
            .and_then(|rows| {
                // In Slint's order: selected, unread, then a muted row's own
                // ink if the list has one, the pointed one's, the read one.
                // A muted row without an ink of its own takes the read ink,
                // faded below, not the window's grey (on Zine's pale tape it
                // was unreadable).
                if room.selected {
                    rows.ink.selected
                } else if room.unread {
                    rows.ink.unread
                } else if room.muted && rows.ink.muted.is_some() {
                    rows.ink.muted
                } else if pointed && rows.ink.hover.is_some() {
                    rows.ink.hover
                } else {
                    rows.ink.read
                }
            })
            .map(crate::themes::argb)
            .unwrap_or(stock);
        let opacity = if room.muted && !room.selected && !pointed {
            0.5
        } else {
            1.0
        };
        (faded(ink, opacity), opacity)
    }
}

/// A row's face as a theme's channel list draws it (see
/// [`super::View::row_face_listed`]); `seed` picks its frame by place, as
/// Slint's `ChannelRow.seed` (a settings section's is 0), else its id.
#[allow(
    clippy::too_many_arguments,
    reason = "each part it draws is its own input"
)]
pub(crate) fn listed_row_face<'a>(
    cx: crate::ctx::Ctx<'a>,
    id: String,
    face: Element<'a, Message>,
    height: f32,
    selected: bool,
    message: Option<Message>,
    menu: Option<String>,
    listed: bool,
    seed: Option<usize>,
) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let pointed = cx.pointed.side.as_deref() == Some(id.as_str());
    let rows = cx.themes.channel_rows(&theme.id).filter(|_| listed);
    let frame = rows.and_then(|rows| {
        if selected {
            rows.selected.frame.as_ref()
        } else {
            None
        }
        .or_else(|| match seed {
            Some(seed) if !rows.frame.is_empty() => Some(&rows.frame[seed % rows.frame.len()]),
            Some(_) => None,
            None => crate::theme_art::picked_tile(&rows.frame, &id),
        })
    });
    let selected_plain =
        frame.is_none() && rows.is_none_or(|rows| rows.selected.underline.is_none());
    let selected_surface = yapfast_core::themes::package::art::ArtSurface::RowSelected;
    let selected_decorated =
        selected && selected_plain && cx.themes.decorates(&theme.id, selected_surface);
    // As Slint: in a theme's list the highlighter marks the selected
    // row only when the list has no frame or underline of its own.
    let event_highlighter = selected
        && (rows.is_none() || selected_plain)
        && cx
            .themes
            .event_effect(
                &theme.id,
                yapfast_core::themes::package::art::EventSlot::Select,
            )
            .is_some();
    let ground = if selected {
        if event_highlighter {
            Color::TRANSPARENT
        } else if selected_plain {
            if selected_decorated {
                Color::TRANSPARENT
            } else {
                theme.token_colour("row-selected", theme.active)
            }
        } else {
            Color::TRANSPARENT
        }
    } else if pointed {
        match rows.and_then(|rows| rows.hover_ground).unwrap_or_else(|| {
            if rows.is_some_and(|rows| !rows.frame.is_empty()) {
                yapfast_core::themes::lists::HoverGround::None
            } else {
                yapfast_core::themes::lists::HoverGround::Hover
            }
        }) {
            yapfast_core::themes::lists::HoverGround::None => Color::TRANSPARENT,
            yapfast_core::themes::lists::HoverGround::Hover => theme.hover,
            yapfast_core::themes::lists::HoverGround::RowHover => theme.row_hover,
        }
    } else {
        Color::TRANSPARENT
    };
    let mut border = theme.surface_border("row", 6.0, 0.0, Color::TRANSPARENT);
    border.radius = theme.row_radius(6.0).into();
    let shadow = theme.surface_shadow("row", iced::Shadow::default());
    let inset = cx
        .themes
        .channel_rows(&theme.id)
        .and_then(|rows| rows.inset)
        .unwrap_or(8.0);
    // A frame fitted to the name hugs it, as Slint's `fit_text` does.
    let fit = rows.is_some_and(|rows| rows.fit_text) && frame.is_some();
    let body: Element<'a, Message> = container(face)
        .padding([0.0, inset])
        .height(height)
        .width(if fit { Length::Shrink } else { Length::Fill })
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(ground.into()),
            border,
            shadow,
            ..Default::default()
        })
        .into();
    let mut body = iced::widget::Stack::with_children([body])
        .width(if fit { Length::Shrink } else { Length::Fill })
        .height(height);
    if let Some(frame) = frame
        && let Some(art) = crate::theme_art::tile_art_opacity(
            cx,
            yapfast_core::themes::package::art::ArtSurface::Sidebar,
            frame,
            if fit {
                f32::INFINITY
            } else {
                super::width(cx.layout, theme)
            },
            height,
            if pointed && !selected {
                rows.and_then(|rows| rows.hover_frame_opacity)
                    .unwrap_or(1.0)
            } else {
                1.0
            },
            0.0,
        )
    {
        body = if fit {
            body.push_under(art)
        } else {
            iced::widget::stack![art, body]
                .width(Length::Fill)
                .height(height)
        };
    }
    if selected
        && let Some(after) = rows.and_then(|rows| rows.selected.after.as_ref())
        && let Some(art) = crate::theme_art::tile_art(
            cx,
            yapfast_core::themes::package::art::ArtSurface::Sidebar,
            after,
            super::width(cx.layout, theme),
            height,
        )
    {
        body = body.push(art);
    }
    let body: Element<'a, Message> = body.into();
    let body = if selected && selected_plain {
        crate::theme_art::surface(body, cx, selected_surface)
    } else {
        body
    };
    let body = crate::theme_effects::selected_row(
        body,
        cx,
        selected && (rows.is_none() || selected_plain),
    );
    let area = mouse_area(body)
        .interaction(iced::mouse::Interaction::Pointer)
        .on_enter(Message::Side(Event::Point(id.clone(), true)))
        .on_exit(Message::Side(Event::Point(id, false)));
    let area = match message {
        Some(message) => area.on_press(message),
        None => area,
    };
    match menu {
        Some(room) => {
            let spot = cx.pressed;
            RightPress::new(area, move |_| {
                Message::Side(Event::Menu(room.clone(), spot.get()))
            })
            .into()
        }
        None => area.into(),
    }
}
