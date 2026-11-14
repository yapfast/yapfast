//! Three dialogs of the Slint client's `ui/dialogs.slint` over the dimmed
//! window: the quick switcher (`QuickSwitcher`, Ctrl+K), New message
//! (`NewDmDialog`), and Create a Poll (`PollDialog`). What each lists or
//! makes is the core's (`switcher_places`, `new_dm`, `poll_draft`); they
//! keep only which place Enter goes to, and Escape or a click outside
//! closes each.

use iced::{
    Alignment, Border, Element, Length, Padding, Task,
    widget::{Space, button, column, container, operation, row, scrollable, stack, text},
};
use yapfast_core::{
    application::{Action, POLL_HOURS, Place, PollEdit, QUESTION_LIMIT},
    chat::Contact,
};

use crate::{
    app::{App, Message},
    controls::{self, Kind, action, lights, placed},
    dialogs::dialog,
    fonts::{BOLD, MEDIUM},
    form,
    picker::{Tab, Target},
    pictures::Icon,
    style::{rounded, scrollbar},
};

/// The fields, for focus.
pub const SWITCHER: &str = "switcher-search";
pub const NEW_DM: &str = "new-dm-search";
pub const QUESTION: &str = "poll-question";
/// A switcher row's height.
const PITCH: f32 = 40.0;
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

impl App {
    pub fn switcher_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Toggle => {
                if self.settings.is_some() {
                    return Task::none();
                }
                self.switcher_chosen = 0;
                self.client.dispatch(Action::ToggleSwitcher);
                if self.client.state().switcher.is_some() {
                    return Task::batch([self.sync(), operation::focus(SWITCHER)]);
                }
                return Task::batch([self.sync(), self.refocus()]);
            }
            Event::Search(query) => {
                self.switcher_chosen = 0;
                self.client.dispatch(Action::SearchSwitcher(query));
            }
            Event::Step(by) => {
                let count = self.client.state().switcher_places().len();
                if count > 0 {
                    let at = self.switcher_chosen.min(count - 1) as i32 + by;
                    self.switcher_chosen = at.rem_euclid(count as i32) as usize;
                }
                return Task::none();
            }
            Event::Go => {
                let places = self.client.state().switcher_places();
                let Some(place) = places.get(self.switcher_chosen.min(places.len().max(1) - 1))
                else {
                    return Task::none();
                };
                let id = place_id(place).to_owned();
                return self.switcher_event(Event::GoTo(id));
            }
            Event::GoTo(id) => {
                self.client.dispatch(Action::SwitchTo(id));
                return Task::batch([self.sync(), self.refocus()]);
            }
            Event::Close => {
                self.client.dispatch(Action::CloseSwitcher);
                return Task::batch([self.sync(), self.refocus()]);
            }
            Event::OpenNewDm => {
                self.client.dispatch(Action::OpenNewDm);
                return Task::batch([self.sync(), operation::focus(NEW_DM)]);
            }
            Event::NewDmQuery(query) => self.client.dispatch(Action::NewDmQuery(query)),
            Event::Start(contact) => self.client.dispatch(Action::StartDm(contact)),
            Event::CloseNewDm => {
                self.client.dispatch(Action::CloseNewDm);
                return Task::batch([self.sync(), self.refocus()]);
            }
            Event::OpenPoll => {
                self.client.dispatch(Action::OpenPollDraft);
                return Task::batch([self.sync(), operation::focus(QUESTION)]);
            }
            Event::Poll(edit) => self.client.dispatch(Action::EditPoll(edit)),
            Event::PollEmoji(index) => {
                let at = self.pressed.get();
                return self.open_picker(Target::Poll(index, at), Tab::Emoji);
            }
            Event::Post => self.client.dispatch(Action::PostPoll),
            Event::ClosePoll => {
                self.client.dispatch(Action::ClosePollDraft);
                return Task::batch([self.sync(), self.refocus()]);
            }
        }
        self.sync()
    }

    /// The switcher's keys: Escape closes it, the arrows and Tab move
    /// through its places.
    pub fn switcher_key(
        &mut self,
        named: Option<iced::keyboard::key::Named>,
        shift: bool,
    ) -> Task<Message> {
        use iced::keyboard::key::Named;
        match named {
            Some(Named::Escape) => self.switcher_event(Event::Close),
            Some(Named::ArrowDown) => self.switcher_event(Event::Step(1)),
            Some(Named::ArrowUp) => self.switcher_event(Event::Step(-1)),
            Some(Named::Tab) => self.switcher_event(Event::Step(if shift { -1 } else { 1 })),
            Some(Named::Enter) => self.switcher_event(Event::Go),
            _ => Task::none(),
        }
    }

    /// The quick switcher: a large search, PREVIOUS CHANNELS while it's
    /// empty, the places found, and how a search can be narrowed.
    pub fn switcher_view(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        let query = state.switcher.as_ref()?;
        let theme = &self.theme;
        let places = state.switcher_places();
        let current = self.switcher_chosen.min(places.len().saturating_sub(1));
        let field = form::field(theme, SWITCHER, "Where would you like to go?", query)
            .size(20)
            .padding(14)
            .on_input(|query| message(Event::Search(query)))
            .on_submit(message(Event::Go));
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
        let height = (self.window.height - 48.0).min(440.0) - 2.0 * 16.0 - 52.0 - 60.0;
        body = body.push(
            scrollable(list)
                .height(height.max(PITCH))
                .style(scrollbar(theme)),
        );
        body = body.push(
            text("Start a search with @ for people, # for text channels, ! for voice channels or * for servers.")
                .size(12)
                .color(theme.muted),
        );
        let width = 570.0_f32.min(self.window.width - 48.0);
        Some(dialog(theme, body.into(), width, message(Event::Close)))
    }

    /// A place the switcher found: its picture or icon, its name (bold
    /// while unread), where it is, and its mentions.
    fn switcher_row<'a>(&'a self, place: &Place<'a>, current: bool) -> Element<'a, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let (face, name, where_, unread, mentions): (
            Element<'a, Message>,
            &str,
            String,
            bool,
            u64,
        ) = match place {
            Place::Room(room) => {
                let server = state
                    .servers
                    .iter()
                    .find(|server| !room.direct && server.channels.contains(&room.id));
                let face = if room.direct {
                    self.avatar(room.avatar.as_deref(), &room.name, 24.0, 12.0, 1.0)
                } else {
                    self.glyph(crate::sidebar::room_icon(room), 20, theme.faint)
                        .into()
                };
                let where_ = match server {
                    _ if room.direct => "Direct message".to_owned(),
                    Some(server) => server.name.clone(),
                    None => "Channel".to_owned(),
                };
                (
                    face,
                    &room.name,
                    where_,
                    room.unread > 0 && !room.muted,
                    room.mentions,
                )
            }
            Place::Server(server) => (
                self.avatar(server.icon.as_deref(), &server.name, 24.0, 12.0, 1.0),
                &server.name,
                "Server".to_owned(),
                false,
                0,
            ),
        };
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
            let (danger, on_danger) = (theme.danger, theme.on_danger);
            line = line.push(
                container(
                    text(crate::rail::count_badge(mentions))
                        .size(11)
                        .font(BOLD)
                        .color(on_danger),
                )
                .padding([0, 5])
                .height(18)
                .center_y(18)
                .style(move |_| rounded(danger, 9)),
            );
        }
        let (active, hover) = (theme.active, theme.hover);
        button(line)
            .padding([0, 8])
            .height(PITCH)
            .width(Length::Fill)
            .on_press(message(Event::GoTo(place_id(place).to_owned())))
            .style(move |_, status| button::Style {
                background: if current {
                    Some(active.into())
                } else {
                    lights(status).then(|| hover.into())
                },
                border: Border::default().rounded(6),
                ..Default::default()
            })
            .into()
    }

    /// New message: a search for people on Discord, each found with
    /// Message, and what the search came to.
    pub fn new_dm_view(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        let new_dm = state.new_dm.as_ref()?;
        let theme = &self.theme;
        let head = row![
            text("New message")
                .size(18)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill),
            self.icon_button(Icon::X, "Close", 28.0, 18, message(Event::CloseNewDm), true),
        ]
        .align_y(Alignment::Center);
        let placeholder = if cfg!(feature = "matrix") {
            "Find a friend, or type a Matrix id"
        } else {
            "Find a friend"
        };
        let field = form::field(theme, NEW_DM, placeholder, &new_dm.query)
            .padding(10)
            .on_input(|query| message(Event::NewDmQuery(query)));
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
        let height = (self.window.height - 48.0).min(520.0) - 40.0 - 28.0 - 36.0 - 24.0;
        let body = column![
            head,
            field,
            scrollable(list).height(height).style(scrollbar(theme)),
        ]
        .spacing(12)
        .padding(20);
        let width = 440.0_f32.min(self.window.width - 48.0);
        Some(dialog(
            theme,
            body.into(),
            width,
            message(Event::CloseNewDm),
        ))
    }

    /// Someone New message found: their picture (with Discord's badge),
    /// name and who they are, and Message.
    fn new_dm_row<'a>(&'a self, person: &'a Contact, starting: bool) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut face = stack![self.avatar(person.avatar.as_deref(), &person.name, 32.0, 16.0, 1.0)];
        if person.discord {
            let (accent, raised) = (theme.accent, theme.raised);
            face = face.push(placed(
                container(self.glyph(Icon::Discord, 9, iced::Color::WHITE))
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
            .style(move |_, status| button::Style {
                background: lights(status).then(|| hover.into()),
                border: Border::default().rounded(6),
                ..Default::default()
            })
            .into()
    }

    /// Create a Poll: the question, the answers each with an emoji, how long
    /// it lasts, whether several answers may be chosen, and Post.
    pub fn poll_draft_view(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        let draft = state.poll_draft.as_ref()?;
        let theme = &self.theme;
        let edit = |edit: PollEdit| message(Event::Poll(edit));
        let caps = |words: &'static str| text(words).size(12).font(BOLD).color(theme.muted);
        let head = row![
            text("Create a Poll")
                .size(18)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill),
            self.icon_button(Icon::X, "Close", 28.0, 18, message(Event::ClosePoll), true),
        ]
        .align_y(Alignment::Center);
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
            form::field(
                theme,
                QUESTION,
                "What question do you want to ask?",
                &draft.question
            )
            .padding(10)
            .on_input(move |text| edit(PollEdit::Question(text))),
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
                .icon(self.icon(Icon::Plus, 16, crate::theme::rgb(theme.secondary)))
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
            Element::from(self.glyph(Icon::Check, 14, theme.on_accent))
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
            foot = foot.push(text(problem).size(14).color(theme.danger));
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
        let room = (self.window.height - 48.0 - 40.0 - 40.0 - 60.0).max(120.0);
        let body = column![
            head,
            scrollable(form.padding(Padding {
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
        let width = 500.0_f32.min(self.window.width - 48.0);
        Some(dialog(theme, body.into(), width, message(Event::ClosePoll)))
    }

    /// An answer: its emoji's button, its text, and the bin while there's
    /// more than one.
    fn poll_answer<'a>(
        &'a self,
        index: usize,
        answer: &'a yapfast_core::application::DraftAnswer,
        several: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let face: Element<'a, Message> = match &answer.emoji {
            Some(key) => self.emoji_face(key, 22),
            None => self.glyph(Icon::SmilePlus, 20, theme.secondary).into(),
        };
        let (sunken, hover) = (theme.sunken, theme.hover);
        let emoji = button(container(face).center(40))
            .padding(0)
            .width(40)
            .height(40)
            .on_press(message(Event::PollEmoji(index)))
            .style(move |_, status| button::Style {
                background: Some(if lights(status) { hover } else { sunken }.into()),
                border: Border::default().rounded(6),
                ..Default::default()
            });
        let mut emoji = stack![emoji];
        if answer.emoji.is_some() {
            let (faint, danger, raised) = (theme.faint, theme.danger, theme.raised);
            emoji = emoji.push(placed(
                button(container(self.glyph(Icon::X, 10, theme.text)).center(14))
                    .padding(0)
                    .width(18)
                    .height(18)
                    .on_press(message(Event::Poll(PollEdit::Emoji { index, emoji: None })))
                    .style(move |_, status| button::Style {
                        background: Some(if lights(status) { danger } else { faint }.into()),
                        border: Border {
                            color: raised,
                            width: 2.0,
                            radius: 9.0.into(),
                        },
                        ..Default::default()
                    }),
                28.0,
                0.0,
            ));
        }
        let field = controls::field(
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
        .on_input(move |text| message(Event::Poll(PollEdit::Answer { index, text })));
        let mut line = row![emoji, field].spacing(8).align_y(Alignment::Center);
        if several {
            line = line.push(self.icon_button(
                Icon::Trash,
                "Remove answer",
                32.0,
                18,
                message(Event::Poll(PollEdit::RemoveAnswer(index))),
                true,
            ));
        }
        line.into()
    }

    /// The emoji the picker gave, for a poll's answer.
    pub fn poll_draft_emoji(&mut self, index: usize, key: String) {
        self.client.dispatch(Action::EditPoll(PollEdit::Emoji {
            index,
            emoji: Some(key),
        }));
    }
}

/// Where a place goes.
fn place_id<'a>(place: &Place<'a>) -> &'a str {
    match place {
        Place::Room(room) => &room.id,
        Place::Server(server) => &server.id,
    }
}
