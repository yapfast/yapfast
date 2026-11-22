//! Three dialogs over the dimmed window: the quick switcher (Ctrl+K), New
//! message, and Create a Poll. What each lists or makes is the core's
//! (`switcher_places`, `new_dm`, `poll_draft`); they keep only which place
//! Enter goes to, and Escape or a click outside closes each.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Border, Element, Length, Padding, Task,
    widget::{Space, button, column, container, operation, row, scrollable, stack, text},
};
use yapfast_core::{
    application::{Action, AppState, POLL_HOURS, Place, PollEdit, QUESTION_LIMIT},
    chat::Contact,
};

use crate::{
    app::{App, Message},
    controls::{self, Kind, action, lights, placed},
    ctx::{Ctx, Cx, Focus, Request},
    dialog::dialog,
    fonts::{BOLD, MEDIUM},
    form,
    pictures::Icon,
    style::{rounded, scrollbar},
};

/// The fields, for focus.
pub const SWITCHER: &str = "switcher-search";
pub const NEW_DM: &str = "new-dm-search";
pub const QUESTION: &str = "poll-question";
/// A switcher row's height, and its list's id, for keeping the row the
/// keys light in view.
const PITCH: f32 = 40.0;
const LIST: &str = "switcher-list";
/// What a poll may last, as the dialog offers it.
const DURATIONS: [&str; 7] = [
    "1 hour", "4 hours", "8 hours", "1 day", "3 days", "1 week", "2 weeks",
];

#[derive(Clone, Debug)]
pub enum Event {
    /// Ctrl+K: the switcher opens, or closes.
    Toggle,
    Search(String),
    /// Up or Down, Tab or Shift+Tab: the place Enter goes to.
    Step(i32),
    /// The list scrolled this far down.
    Scrolled(f32),
    Go,
    GoTo(String),
    Close,
    // New message.
    OpenNewDm,
    NewDmQuery(String),
    Start(Contact),
    CloseNewDm,
    // Create a Poll.
    OpenPoll,
    Poll(PollEdit),
    /// An answer's emoji from the picker, beside its button.
    PollEmoji(usize),
    Post,
    ClosePoll,
}

fn message(event: Event) -> Message {
    Message::Switcher(event)
}

/// The quick switcher's places as the core ranked them for its search,
/// worked out once per search and room list rather than per view, so the
/// view, the arrows and Enter all go by the same list.
#[derive(Default)]
pub struct Quick {
    /// The place Enter goes to.
    pub chosen: usize,
    pub places: Vec<Found>,
    /// The search and the room lists they were found for.
    made_for: Option<(String, u64)>,
    /// How far down the list is scrolled.
    pub offset: f32,
}

/// A place the switcher found, as its row shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub id: String,
    pub name: String,
    /// Where it is: its server, "Direct message", "Channel" or "Server".
    pub place: String,
    pub unread: bool,
    pub mentions: u64,
    pub face: Face,
}

/// What a found place shows before its name.
#[derive(Clone, Debug, PartialEq)]
pub enum Face {
    /// A DM's or a server's picture, by its name's initial.
    Avatar(Option<String>),
    /// A channel's icon.
    Icon(Icon),
}

impl Quick {
    /// What's done in the three dialogs; Toggle with Settings open and an
    /// answer's emoji are the root's (`App::switcher_event`).
    pub fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            Event::Toggle => {
                *self = Quick::default();
                cx.dispatch(Action::ToggleSwitcher);
                if cx.state().switcher.is_some() {
                    return operation::focus(SWITCHER);
                }
                cx.request(Request::Focus(Focus::Default));
            }
            Event::Search(query) => {
                self.chosen = 0;
                cx.dispatch(Action::SearchSwitcher(query));
            }
            Event::Step(by) => {
                let count = self.places.len();
                if count > 0 {
                    let at = self.chosen.min(count - 1) as i32 + by;
                    self.chosen = at.rem_euclid(count as i32) as usize;
                }
                return self.keep_chosen_in_view(cx.window.height);
            }
            Event::Scrolled(offset) => {
                self.offset = offset;
                return Task::none();
            }
            Event::Go => {
                let places = &self.places;
                let Some(place) = places.get(self.chosen.min(places.len().max(1) - 1)) else {
                    return Task::none();
                };
                let id = place.id.clone();
                return self.update(Event::GoTo(id), cx);
            }
            Event::GoTo(id) => {
                cx.dispatch(Action::SwitchTo(id));
                cx.request(Request::Focus(Focus::Default));
            }
            Event::Close => {
                cx.dispatch(Action::CloseSwitcher);
                cx.request(Request::Focus(Focus::Default));
            }
            Event::OpenNewDm => {
                cx.dispatch(Action::OpenNewDm);
                return operation::focus(NEW_DM);
            }
            Event::NewDmQuery(query) => cx.dispatch(Action::NewDmQuery(query)),
            Event::Start(contact) => cx.dispatch(Action::StartDm(contact)),
            Event::CloseNewDm => {
                cx.dispatch(Action::CloseNewDm);
                cx.request(Request::Focus(Focus::Default));
            }
            Event::OpenPoll => {
                cx.dispatch(Action::OpenPollDraft);
                return operation::focus(QUESTION);
            }
            Event::Poll(edit) => cx.dispatch(Action::EditPoll(edit)),
            // The root opens the picker for it.
            Event::PollEmoji(_) => {}
            Event::Post => cx.dispatch(Action::PostPoll),
            Event::ClosePoll => {
                cx.dispatch(Action::ClosePollDraft);
                cx.request(Request::Focus(Focus::Default));
            }
        }
        Task::none()
    }

    /// The switcher's keys: Escape closes it, the arrows and Tab move
    /// through its places.
    pub fn key(
        &mut self,
        named: Option<iced::keyboard::key::Named>,
        shift: bool,
        cx: &mut Cx<'_>,
    ) -> Task<Message> {
        use iced::keyboard::key::Named;
        match named {
            Some(Named::Escape) => self.update(Event::Close, cx),
            Some(Named::ArrowDown) => self.update(Event::Step(1), cx),
            Some(Named::ArrowUp) => self.update(Event::Step(-1), cx),
            Some(Named::Tab) => self.update(Event::Step(if shift { -1 } else { 1 }), cx),
            Some(Named::Enter) => self.update(Event::Go, cx),
            _ => Task::none(),
        }
    }

    /// Takes in the core's places for the switcher's search, when the
    /// search or the room lists changed.
    pub fn sync(&mut self, state: &AppState) {
        let Some(query) = state.switcher.as_ref() else {
            if self.made_for.is_some() {
                *self = Quick::default();
            }
            return;
        };
        let made_for = (query.clone(), state.list_changes);
        if self.made_for.as_ref() == Some(&made_for) {
            return;
        }
        // Each channel's server, found once.
        let servers: std::collections::HashMap<&str, &str> = state
            .servers
            .iter()
            .flat_map(|server| {
                server
                    .channels
                    .iter()
                    .map(|channel| (channel.as_str(), server.name.as_str()))
            })
            .collect();
        self.places = state
            .switcher_places()
            .iter()
            .map(|place| match place {
                Place::Room(room) => Found {
                    id: room.id.clone(),
                    name: room.name.clone(),
                    place: if room.direct {
                        "Direct message".to_owned()
                    } else {
                        servers
                            .get(room.id.as_str())
                            .map_or_else(|| "Channel".to_owned(), |name| (*name).to_owned())
                    },
                    unread: room.unread > 0 && !room.muted,
                    mentions: room.mentions,
                    face: if room.direct {
                        Face::Avatar(room.avatar.clone())
                    } else {
                        Face::Icon(crate::sidebar::room_icon(room))
                    },
                },
                Place::Server(server) => Found {
                    id: server.id.clone(),
                    name: server.name.clone(),
                    place: "Server".to_owned(),
                    unread: false,
                    mentions: 0,
                    face: Face::Avatar(server.icon.clone()),
                },
            })
            .collect();
        self.made_for = Some(made_for);
    }

    /// The list scrolled so the row the keys light shows.
    fn keep_chosen_in_view(&mut self, window: f32) -> Task<Message> {
        let height = list_height(window);
        let top = self.chosen as f32 * PITCH;
        let offset = if top < self.offset {
            top
        } else if top + PITCH > self.offset + height {
            top + PITCH - height
        } else {
            return Task::none();
        };
        self.offset = offset;
        operation::scroll_to(
            LIST,
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(offset),
            },
        )
    }
}

/// The list's height in a window `window` tall: the dialog at most
/// 440 px, less its padding, the search and the hint under the list.
fn list_height(window: f32) -> f32 {
    ((window - 48.0).min(440.0) - 2.0 * 16.0 - 52.0 - 60.0).max(PITCH)
}

/// The quick switcher: a large search, PREVIOUS CHANNELS while it's
/// empty, the places found, and how a search can be narrowed.
pub fn switcher_view<'a>(quick: &'a Quick, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    View { s: quick, cx }.switcher()
}

/// New message: a search over friends and DMs, and the people found.
pub fn new_dm_view<'a>(quick: &'a Quick, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    View { s: quick, cx }.new_dm()
}

/// Create a Poll: the question, the answers, how long it lasts and
/// whether several may be chosen.
pub fn poll_view<'a>(quick: &'a Quick, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    View { s: quick, cx }.poll_draft()
}

/// The switcher's places and what the dialogs read, as their parts draw
/// them.
#[derive(Clone, Copy)]
struct View<'a> {
    s: &'a Quick,
    cx: Ctx<'a>,
}

impl<'a> View<'a> {
    /// The quick switcher: a large search, PREVIOUS CHANNELS while it's
    /// empty, the places found, and how a search can be narrowed.
    fn switcher(self) -> Option<Element<'a, Message>> {
        let state = self.cx.state;
        let query = state.switcher.as_ref()?;
        let theme = self.cx.look.theme;
        let places = &self.s.places;
        let current = self.s.chosen.min(places.len().saturating_sub(1));
        let field = yapfast_iced_kit::parts::show(
            theme,
            "field",
            0,
            form::field(theme, SWITCHER, "Where would you like to go?", query)
                .size(20)
                .padding(14)
                .on_input(|query| message(Event::Search(query)))
                .on_submit(message(Event::Go)),
        );
        let mut body = column![field].spacing(12).padding(16);
        if query.is_empty() && !places.is_empty() {
            body = body.push(
                text("PREVIOUS CHANNELS")
                    .size(12)
                    .font(BOLD)
                    .color(theme.muted),
            );
        }
        let mut list = column![];
        for (index, place) in places.iter().enumerate() {
            list = list.push(self.switcher_row(place, index == current));
        }
        if places.is_empty() && !query.is_empty() {
            list = list.push(
                container(
                    text("Nothing goes by that name.")
                        .size(14)
                        .color(theme.secondary),
                )
                .padding(8),
            );
        }
        body = body.push(
            scroll_view(list)
                .id(LIST)
                .on_scroll(|viewport| message(Event::Scrolled(viewport.absolute_offset().y)))
                .height(list_height(self.cx.look.window.height))
                .style(scrollbar(theme)),
        );
        body = body.push(
            text("Start a search with @ for people, # for text channels, ! for voice channels or * for servers.")
                .size(12)
                .color(theme.muted),
        );
        let width = 570.0_f32.min(self.cx.look.window.width - 48.0);
        Some(dialog(theme, body.into(), width, message(Event::Close)))
    }

    /// A place the switcher found: its picture or icon, its name (bold
    /// while unread), where it is, and its mentions.
    fn switcher_row(self, found: &'a Found, current: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let face: Element<'a, Message> = match &found.face {
            Face::Avatar(picture) => {
                self.cx
                    .avatar(picture.as_deref(), &found.name, 24.0, 12.0, 1.0)
            }
            Face::Icon(icon) => self.cx.look.glyph(*icon, 20, theme.faint).into(),
        };
        let (name, where_, unread, mentions) = (
            found.name.as_str(),
            found.place.as_str(),
            found.unread,
            found.mentions,
        );
        let ink = if unread || current {
            theme.text
        } else {
            theme.secondary
        };
        let mut line = row![
            container(face).width(24).center_x(24),
            text(name)
                .size(16)
                .font(if unread { BOLD } else { MEDIUM })
                .color(ink)
                .wrapping(text::Wrapping::None),
            text(where_)
                .size(13)
                .color(theme.muted)
                .wrapping(text::Wrapping::None)
                .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if mentions > 0 {
            line = line.push(controls::count_pill(
                theme,
                &controls::count_badge(mentions),
                controls::ROW_COUNT,
                None,
            ));
        }
        let (active, hover) = (theme.active, theme.hover);
        button(line)
            .padding([0, 8])
            .height(PITCH)
            .width(Length::Fill)
            .on_press(message(Event::GoTo(found.id.clone())))
            .style(move |_, status| button::Style {
                background: if current {
                    Some(active.into())
                } else {
                    lights(status).then(|| hover.into())
                },
                border: crate::style::round(6),
                ..Default::default()
            })
            .into()
    }

    /// New message: a search for people on Discord, each found with
    /// Message, and what the search came to.
    fn new_dm(self) -> Option<Element<'a, Message>> {
        let state = self.cx.state;
        let new_dm = state.new_dm.as_ref()?;
        let theme = self.cx.look.theme;
        let head = self
            .cx
            .look
            .dialog_head("New message", message(Event::CloseNewDm));
        let placeholder = if cfg!(feature = "matrix") {
            "Find a friend, or type a Matrix id"
        } else {
            "Find a friend"
        };
        let field = yapfast_iced_kit::parts::show(
            theme,
            "field",
            0,
            form::field(theme, NEW_DM, placeholder, &new_dm.query)
                .padding(10)
                .on_input(|query| message(Event::NewDmQuery(query))),
        );
        let mut list = column![].spacing(2);
        let mut found = 0;
        for person in new_dm.people() {
            found += 1;
            list = list.push(self.new_dm_row(person, new_dm.starting));
        }
        let note = if let Some(error) = &new_dm.error {
            error.as_str()
        } else if new_dm.starting {
            "Starting the conversation..."
        } else if found == 0 && new_dm.query.trim().is_empty() {
            if cfg!(feature = "matrix") {
                "Type a name to find people you know on Discord, or a full Matrix id such as @name:example.org."
            } else {
                "Type a name to find people you know on Discord."
            }
        } else if found == 0 {
            if cfg!(feature = "matrix") {
                "No one found. For Matrix, type a full id such as @name:example.org."
            } else {
                "No one found."
            }
        } else {
            ""
        };
        if !note.is_empty() {
            list = list.push(container(form::note(theme, note)).padding(8));
        }
        let height = (self.cx.look.window.height - 48.0).min(520.0) - 40.0 - 28.0 - 36.0 - 24.0;
        let body = column![
            head,
            field,
            scroll_view(list).height(height).style(scrollbar(theme)),
        ]
        .spacing(12)
        .padding(20);
        let width = 440.0_f32.min(self.cx.look.window.width - 48.0);
        Some(dialog(
            theme,
            body.into(),
            width,
            message(Event::CloseNewDm),
        ))
    }

    /// Someone New message found: their picture (with Discord's badge),
    /// name and who they are, and Message.
    fn new_dm_row(self, person: &'a Contact, starting: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut face =
            stack![
                self.cx
                    .avatar(person.avatar.as_deref(), &person.name, 32.0, 16.0, 1.0)
            ];
        if person.discord {
            let (accent, raised) = (theme.accent, theme.raised);
            face = face.push(placed(
                container(self.cx.look.glyph(Icon::Discord, 9, iced::Color::WHITE))
                    .center(14)
                    .style(move |_| container::Style {
                        border: Border {
                            color: raised,
                            width: 2.0,
                            radius: 7.0.into(),
                        },
                        ..rounded(accent, 7)
                    }),
                18.0,
                16.0,
            ));
        }
        let start = (!starting).then(|| message(Event::Start(person.clone())));
        let line = row![
            container(face).width(36),
            column![
                text(person.name.as_str())
                    .size(16)
                    .font(MEDIUM)
                    .color(theme.text)
                    .wrapping(text::Wrapping::None),
                text(person.detail.as_str())
                    .size(13)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            ]
            .width(Length::Fill)
            .clip(true),
            action(theme, "Message", Kind::Plain, start.clone()).small(),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let hover = theme.hover;
        button(line)
            .padding([0, 8])
            .height(48)
            .width(Length::Fill)
            .on_press_maybe(start)
            .style(controls::lit_style(None, hover, 6.0))
            .into()
    }

    /// Create a Poll: the question, the answers each with an emoji, how long
    /// it lasts, whether several answers may be chosen, and Post.
    fn poll_draft(self) -> Option<Element<'a, Message>> {
        let state = self.cx.state;
        let draft = state.poll_draft.as_ref()?;
        let theme = self.cx.look.theme;
        let edit = |edit: PollEdit| message(Event::Poll(edit));
        let caps = |words: &'static str| text(words).size(12).font(BOLD).color(theme.muted);
        let head = self
            .cx
            .look
            .dialog_head("Create a Poll", message(Event::ClosePoll));
        let mut form = column![
            row![
                caps("QUESTION").width(Length::Fill),
                text(format!(
                    "{} / {QUESTION_LIMIT}",
                    draft.question.chars().count()
                ))
                .size(12)
                .color(theme.muted)
            ],
            yapfast_iced_kit::parts::show(
                theme,
                "field",
                0,
                form::field(
                    theme,
                    QUESTION,
                    "What question do you want to ask?",
                    &draft.question
                )
                .padding(10)
                .on_input(move |text| edit(PollEdit::Question(text)))
            ),
            caps("ANSWERS"),
        ]
        .spacing(8);
        let several = draft.answers.len() > 1;
        for (index, answer) in draft.answers.iter().enumerate() {
            form = form.push(self.poll_answer(index, answer, several));
        }
        if draft.can_add() {
            form = form.push(
                action(
                    theme,
                    "Add another answer",
                    Kind::Ghost,
                    Some(edit(PollEdit::AddAnswer)),
                )
                .icon(
                    self.cx
                        .look
                        .icon(Icon::Plus, 16, crate::theme::rgb(theme.secondary)),
                )
                .small(),
            );
        }
        form = form.push(caps("DURATION")).push(controls::choices_within(
            theme,
            &DURATIONS,
            draft.duration.min(POLL_HOURS.len() - 1),
            460.0,
            move |index| edit(PollEdit::Duration(index)),
        ));
        let ticked = draft.multiselect;
        let (accent, faint) = (theme.accent, theme.faint);
        let tick = container(if ticked {
            Element::from(self.cx.look.glyph(Icon::Check, 14, theme.on_accent))
        } else {
            Space::new().into()
        })
        .center(20)
        .style(move |_| container::Style {
            background: ticked.then(|| accent.into()),
            border: Border {
                color: faint,
                width: if ticked { 0.0 } else { 2.0 },
                radius: 5.0.into(),
            },
            ..Default::default()
        });
        form = form.push(
            button(
                row![
                    tick,
                    text("Allow Multiple Answers").size(15).color(theme.text)
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding(Padding {
                top: 4.0,
                ..Padding::ZERO
            })
            .on_press(edit(PollEdit::Multiselect(!ticked)))
            .style(|_, _| button::Style::default()),
        );
        let mut foot = column![].spacing(8).padding(Padding {
            top: 12.0,
            ..Padding::ZERO
        });
        if let Some(problem) = draft.error.as_deref().or(draft.problem()) {
            foot = foot.push(form::error(theme, problem));
        }
        foot = foot.push(
            row![
                Space::new().width(Length::Fill),
                action(
                    theme,
                    "Cancel",
                    Kind::Ghost,
                    Some(message(Event::ClosePoll))
                ),
                action(
                    theme,
                    if draft.posting { "Posting..." } else { "Post" },
                    Kind::Primary,
                    draft.can_post().then(|| message(Event::Post)),
                ),
            ]
            .spacing(8),
        );
        let room = (self.cx.look.window.height - 48.0 - 40.0 - 40.0 - 60.0).max(120.0);
        let body = column![
            head,
            scroll_view(form.padding(Padding {
                top: 12.0,
                right: 12.0,
                ..Padding::ZERO
            }))
            .height(Length::Shrink)
            .style(scrollbar(theme)),
            foot,
        ]
        .padding(20);
        let body = container(body).max_height(room + 140.0);
        let width = 500.0_f32.min(self.cx.look.window.width - 48.0);
        Some(dialog(theme, body.into(), width, message(Event::ClosePoll)))
    }

    /// An answer: its emoji's button, its text, and the bin while there's
    /// more than one.
    fn poll_answer(
        self,
        index: usize,
        answer: &'a yapfast_core::application::DraftAnswer,
        several: bool,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let emoji = crate::dialogs::emoji_slot(
            self.cx,
            answer.emoji.as_deref(),
            message(Event::PollEmoji(index)),
            message(Event::Poll(PollEdit::Emoji { index, emoji: None })),
        );
        let field = yapfast_iced_kit::parts::show(
            theme,
            "field",
            0,
            controls::field(
                theme,
                "poll-answer",
                "Type your answer",
                &answer.text,
                false,
                true,
            )
            .id(iced::widget::Id::from(format!("poll-answer-{}", answer.id)))
            .size(15)
            .padding(10)
            .on_input(move |text| message(Event::Poll(PollEdit::Answer { index, text }))),
        );
        let mut line = row![emoji, field].spacing(8).align_y(Alignment::Center);
        if several {
            line = line.push(
                self.cx
                    .look
                    .icon_button(
                        Icon::Trash,
                        "Remove answer",
                        message(Event::Poll(PollEdit::RemoveAnswer(index))),
                    )
                    .size(32.0, 18)
                    .below(),
            );
        }
        line.into()
    }
}

/// The emoji the picker gave, for a poll's answer.
pub fn poll_draft_emoji(app: &mut App, index: usize, key: String) {
    app.client.dispatch(Action::EditPoll(PollEdit::Emoji {
        index,
        emoji: Some(key),
    }));
}
