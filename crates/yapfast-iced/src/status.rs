//! The account's popover and the custom status's dialog, as the Slint
//! client's `AccountPopover` (`ui/yapfast/account-popover.slint`, its
//! Discord look) and `StatusDialog` (`ui/dialogs.slint`). The popover opens
//! over the account card when its picture or name is clicked: the account
//! as Discord's own card shows it, Edit Profile, the Discord status to
//! choose, the custom status, Copy User ID and User Settings. The dialog
//! sets the custom status: an emoji from the picker, words, and when it
//! clears.

use iced::{
    Alignment, Border, Element, Length, Padding, Point, Task,
    widget::{Space, button, column, container, opaque, operation, row, stack, text},
};
use yapfast_core::{
    application::{Action, ClearAfter},
    chat::Presence,
};

use crate::{
    app::{App, Message},
    controls::{self, Check, Item, Kind, Light, action, lights, placed},
    dialogs::dialog,
    fonts::BOLD,
    form,
    picker::{Tab, Target},
    pictures::{Icon, Mark},
    style::{corners, hex, rounded},
    theme::rgb,
    widgets::{Anchor, Pic, Place},
};

/// The popover's width, the dialog's, and its field's id.
const POPOVER: f32 = 300.0;
const DIALOG: f32 = 460.0;
pub const FIELD: &str = "status-text";
/// What Clear After offers, as minutes where it's a time.
const CLEAR: [&str; 5] = ["Never", "30 min", "1 hour", "4 hours", "Today"];
/// Discord's statuses: its key, name and what it means.
const STATUSES: [(&str, &str, &str); 4] = [
    ("online", "Online", ""),
    ("idle", "Idle", ""),
    (
        "dnd",
        "Do Not Disturb",
        "You won't get message notifications.",
    ),
    ("invisible", "Invisible", "You'll appear offline."),
];

#[derive(Clone, Debug)]
pub enum Event {
    /// The account card's picture or name: the popover opens, or closes.
    Toggle,
    Close,
    /// The status row: its choices show or go.
    Expand,
    Choose(&'static str),
    EditProfile,
    CustomStatus,
    CopyId,
    Settings,
    // The dialog.
    Text(String),
    /// The emoji's button, where it was pressed.
    Emoji,
    RemoveEmoji,
    ClearAfter(usize),
    Save,
    Clear,
    Cancel,
}

fn message(event: Event) -> Message {
    Message::Status(event)
}

/// The popover, while it's open.
#[derive(Default)]
pub struct Popover {
    expanded: bool,
}

/// What the dialog holds while it's open: the core keeps only its saving.
#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub text: String,
    /// A Unicode emoji, or a custom one's `name:id`, as the picker gives it.
    pub emoji: Option<String>,
    pub clear: usize,
}

impl App {
    pub fn status_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Toggle => {
                if self.popover.take().is_none() {
                    self.popover = Some(Popover::default());
                    self.client.dispatch(Action::LoadOwnProfile);
                }
            }
            Event::Close => self.popover = None,
            Event::Expand => {
                if let Some(popover) = &mut self.popover {
                    popover.expanded = !popover.expanded;
                }
                return Task::none();
            }
            Event::Choose(status) => {
                self.popover = None;
                self.client.dispatch(Action::SetStatus(status.to_owned()));
            }
            Event::EditProfile => {
                self.popover = None;
                self.client.dispatch(Action::EditProfile);
            }
            Event::CustomStatus => {
                self.popover = None;
                self.client.dispatch(Action::OpenStatusEditor);
            }
            Event::CopyId => {
                self.popover = None;
                let id = self.client.state().discord_user.clone().unwrap_or_default();
                return iced::clipboard::write(id);
            }
            Event::Settings => {
                self.popover = None;
                return self.update(Message::Settings(crate::settings::Event::Open(
                    crate::settings::Section::Account,
                )));
            }
            Event::Text(text) => {
                if let Some(draft) = &mut self.status_draft {
                    draft.text = text;
                }
                return Task::none();
            }
            Event::Emoji => {
                let at = self.pressed.get();
                return self.open_picker(Target::Status(at), Tab::Emoji);
            }
            Event::RemoveEmoji => {
                if let Some(draft) = &mut self.status_draft {
                    draft.emoji = None;
                }
                return Task::none();
            }
            Event::ClearAfter(index) => {
                if let Some(draft) = &mut self.status_draft {
                    draft.clear = index;
                }
                return Task::none();
            }
            Event::Save => {
                let Some(draft) = self.status_draft.clone() else {
                    return Task::none();
                };
                self.client.dispatch(Action::SetCustomStatus {
                    emoji: draft.emoji,
                    text: draft.text,
                    clear_after: match draft.clear {
                        1 => ClearAfter::Minutes(30),
                        2 => ClearAfter::Minutes(60),
                        3 => ClearAfter::Minutes(240),
                        4 => ClearAfter::Today,
                        _ => ClearAfter::Never,
                    },
                });
            }
            Event::Clear => self.client.dispatch(Action::ClearCustomStatus),
            Event::Cancel => {
                self.client.dispatch(Action::CloseStatusEditor);
                return Task::batch([self.sync(), self.refocus()]);
            }
        }
        self.sync()
    }

    /// Takes in the core's dialog: opening it starts from the status there
    /// is, and its field takes the keys.
    pub fn status_sync(&mut self) -> Task<Message> {
        let state = self.client.state();
        match (&state.status_editor, &self.status_draft) {
            (Some(_), None) => {
                let current = state.custom_status.as_ref();
                self.status_draft = Some(Draft {
                    text: current
                        .map(|status| status.text.clone())
                        .unwrap_or_default(),
                    emoji: current.and_then(|status| match (&status.emoji, &status.emoji_id) {
                        (Some(name), Some(id)) => Some(format!("{name}:{id}")),
                        (Some(emoji), None) => Some(emoji.clone()),
                        _ => None,
                    }),
                    clear: 0,
                });
                operation::focus(FIELD)
            }
            (None, Some(_)) => {
                self.status_draft = None;
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// The emoji the picker gave, for the status.
    pub fn status_emoji(&mut self, key: String) {
        if let Some(draft) = &mut self.status_draft {
            draft.emoji = Some(key);
        }
    }

    /// The popover over the account card, its foot 4 px above it.
    pub fn popover_view(&self) -> Option<Element<'_, Message>> {
        let popover = self.popover.as_ref()?;
        let theme = &self.theme;
        let state = self.client.state();
        let account = &self.account;
        let profile = state.account_profile.as_deref();
        let inks = crate::profile::look::Inks::new(
            theme,
            profile.and_then(|profile| profile.theme),
            theme.panel,
        );
        let ink = inks.ink;
        // The banner: its colour, the theme's, or the sunken grey.
        let banner = profile
            .and_then(|profile| profile.accent)
            .map(hex)
            .or(inks.themed.then_some(inks.primary))
            .unwrap_or(theme.sunken);
        let ring = inks.ground;
        let mut face = stack![
            container(Space::new())
                .width(88)
                .height(88)
                .style(move |_| rounded(ring, 44)),
            container(self.avatar(account.avatar.as_deref(), &account.name, 80.0, 40.0, 1.0))
                .padding(4),
        ];
        let status = state.discord_presence.as_deref().unwrap_or("online");
        if let Some(dot) = self.mark(Mark::Dot(presence(status), rgb(ring))) {
            face = face.push(placed(Pic::square(dot, 16.0), 66.0, 66.0));
        }
        let header = stack![
            Space::new().width(Length::Fill).height(156),
            container(Space::new())
                .width(Length::Fill)
                .height(80)
                .style(move |_| rounded(banner, corners([8.0, 8.0, 0.0, 0.0]))),
            placed(face, 12.0, 36.0),
            placed(
                text(account.name.as_str())
                    .size(20)
                    .font(BOLD)
                    .color(ink)
                    .wrapping(text::Wrapping::None),
                16.0,
                128.0,
            ),
        ];
        let mut body = column![].spacing(12).padding(Padding {
            top: 8.0,
            right: 16.0,
            bottom: 16.0,
            left: 16.0,
        });
        let custom = state.custom_status.is_some();
        if custom {
            body = body.push(self.status_line(&account.status, 1.0));
        }
        let mut first =
            column![self.popover_row("Edit Profile", Icon::Pencil, message(Event::EditProfile))]
                .spacing(4);
        let line = theme.line;
        first = first.push(
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(move |_| rounded(line, 0)),
        );
        first = first.push(self.status_row(status, popover.expanded));
        if popover.expanded {
            let mut options = column![].spacing(2);
            for (key, name, note) in STATUSES {
                options = options.push(self.status_option(key, name, note, key == status));
            }
            first = first.push(options);
        }
        first = first.push(self.popover_row(
            if custom {
                "Change custom status"
            } else {
                "Set a custom status"
            },
            Icon::SmilePlus,
            message(Event::CustomStatus),
        ));
        let mut second = column![].spacing(4);
        if state.discord_user.is_some() {
            second =
                second.push(self.popover_row("Copy User ID", Icon::Copy, message(Event::CopyId)));
        }
        second = second.push(self.popover_row(
            "User Settings",
            Icon::Settings,
            message(Event::Settings),
        ));
        let raised = theme.raised;
        body = body.push(group(first, raised)).push(group(second, raised));
        let card = container(column![header, body])
            .width(POPOVER)
            .style(crate::profile::frame_style(inks, theme.line, 8.0, true));
        let bottom = self.window.height - crate::account::HEIGHT - crate::account::INSET - 4.0;
        Some(
            Place::new(opaque(card), Point::new(crate::account::INSET, bottom), 8.0)
                .anchor(Anchor::Bottom)
                .into(),
        )
    }

    /// A row of the popover's: its icon, its words.
    fn popover_row<'a>(
        &self,
        label: &'a str,
        icon: Icon,
        pressed: Message,
    ) -> Element<'a, Message> {
        self.menu_item(
            Item {
                label: label.into(),
                icon: Some(icon),
                danger: false,
                slot: false,
                disabled: false,
                subtitle: None,
                check: Check::None,
            },
            Light::Pointer,
            pressed,
        )
    }

    /// The status as it is, which shows the others to choose.
    fn status_row(&self, status: &str, expanded: bool) -> Element<'_, Message> {
        let theme = &self.theme;
        let name = STATUSES
            .iter()
            .find(|(key, ..)| *key == status)
            .map_or("Online", |(_, name, _)| name);
        let mut line = row![].spacing(8).align_y(Alignment::Center);
        if let Some(dot) = self.mark(Mark::Dot(presence(status), rgb(theme.raised))) {
            line = line.push(Pic::square(dot, 16.0));
        }
        line = line
            .push(
                text(name)
                    .size(15)
                    .color(theme.secondary)
                    .width(Length::Fill),
            )
            .push(self.glyph(
                if expanded {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                16,
                theme.muted,
            ));
        let active = theme.active;
        button(line)
            .padding([0, 8])
            .height(40)
            .width(Length::Fill)
            .on_press(message(Event::Expand))
            .style(move |_, status| button::Style {
                background: lights(status).then(|| active.into()),
                border: Border::default().rounded(4),
                ..Default::default()
            })
            .into()
    }

    /// One of Discord's statuses: its dot, its name and what it means, a
    /// tick on the one chosen.
    fn status_option<'a>(
        &self,
        key: &'static str,
        name: &'a str,
        note: &'a str,
        checked: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut words = column![
            text(name)
                .size(15)
                .font(if checked { BOLD } else { iced::Font::default() })
                .color(if checked { theme.text } else { theme.secondary })
        ]
        .spacing(2)
        .width(Length::Fill);
        if !note.is_empty() {
            words = words.push(text(note).size(12).color(theme.muted));
        }
        let mut line = row![].spacing(10).align_y(Alignment::Center);
        if let Some(dot) = self.mark(Mark::Dot(presence(key), rgb(theme.raised))) {
            line = line.push(Pic::square(dot, 16.0));
        }
        line = line.push(words);
        if checked {
            line = line.push(self.glyph(Icon::Check, 16, theme.accent));
        }
        let active = theme.active;
        button(line)
            .padding([6, 8])
            .width(Length::Fill)
            .on_press(message(Event::Choose(key)))
            .style(move |_, status| button::Style {
                background: lights(status).then(|| active.into()),
                border: Border::default().rounded(4),
                ..Default::default()
            })
            .into()
    }

    /// The custom status's dialog.
    pub fn status_dialog_view(&self) -> Option<Element<'_, Message>> {
        let editor = self.client.state().status_editor.as_ref()?;
        let draft = self.status_draft.as_ref()?;
        let theme = &self.theme;
        let head = row![
            text("Set a custom status")
                .size(18)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill),
            self.icon_button(Icon::X, "Close", 28.0, 18, message(Event::Cancel), true),
        ]
        .align_y(Alignment::Center);
        let face: Element<'_, Message> = match &draft.emoji {
            Some(key) => self.emoji_face(key, 22),
            None => self.glyph(Icon::SmilePlus, 20, theme.secondary).into(),
        };
        let (sunken, hover) = (theme.sunken, theme.hover);
        let emoji = button(container(face).center(40))
            .padding(0)
            .width(40)
            .height(40)
            .on_press(message(Event::Emoji))
            .style(move |_, status| button::Style {
                background: Some(if lights(status) { hover } else { sunken }.into()),
                border: Border::default().rounded(6),
                ..Default::default()
            });
        let mut emoji = stack![emoji];
        if draft.emoji.is_some() {
            // Taking the emoji off: a small badge on its corner.
            let (faint, danger, raised) = (theme.faint, theme.danger, theme.raised);
            let remove = button(container(self.glyph(Icon::X, 10, theme.text)).center(14))
                .padding(0)
                .width(18)
                .height(18)
                .on_press(message(Event::RemoveEmoji))
                .style(move |_, status| button::Style {
                    background: Some(if lights(status) { danger } else { faint }.into()),
                    border: Border {
                        color: raised,
                        width: 2.0,
                        radius: 9.0.into(),
                    },
                    ..Default::default()
                });
            emoji = emoji.push(placed(remove, 28.0, 0.0));
        }
        let field = form::field(theme, FIELD, "What's happening?", &draft.text)
            .padding(10)
            .on_input(|text| message(Event::Text(text)))
            .on_submit(message(Event::Save));
        let mut body = column![
            head,
            row![emoji, field].spacing(8).align_y(Alignment::Center),
            text("CLEAR AFTER").size(12).font(BOLD).color(theme.muted),
            controls::choices(theme, &CLEAR, draft.clear, |index| message(
                Event::ClearAfter(index)
            )),
        ]
        .spacing(12)
        .padding(20);
        if let Some(error) = &editor.error {
            body = body.push(text(error.as_str()).size(14).color(theme.danger));
        }
        let mut foot = row![].spacing(8).align_y(Alignment::Center);
        if self.client.state().custom_status.is_some() {
            foot = foot.push(action(
                theme,
                "Clear status",
                Kind::Ghost,
                (!editor.saving).then(|| message(Event::Clear)),
            ));
        }
        foot = foot
            .push(Space::new().width(Length::Fill))
            .push(action(
                theme,
                "Cancel",
                Kind::Ghost,
                Some(message(Event::Cancel)),
            ))
            .push(action(
                theme,
                if editor.saving { "Saving..." } else { "Save" },
                Kind::Primary,
                (!editor.saving).then(|| message(Event::Save)),
            ));
        body = body.push(container(foot).padding(Padding {
            top: 4.0,
            ..Padding::ZERO
        }));
        let width = DIALOG.min(self.window.width - 48.0);
        Some(dialog(theme, body.into(), width, message(Event::Cancel)))
    }
}

/// The dot for a Discord status's key.
fn presence(key: &str) -> Presence {
    match key {
        "idle" => Presence::Idle,
        "dnd" => Presence::DoNotDisturb,
        "invisible" => Presence::Offline,
        _ => Presence::Online,
    }
}

/// A group of the popover's rows, on its raised ground.
fn group(rows: iced::widget::Column<'_, Message>, raised: iced::Color) -> Element<'_, Message> {
    container(rows)
        .padding(8)
        .width(Length::Fill)
        .style(move |_| rounded(raised, 8))
        .into()
}
