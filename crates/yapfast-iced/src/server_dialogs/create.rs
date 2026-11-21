//! Server creation using Yapfast's own dialog controls.

use std::path::PathBuf;

use yapfast_core::{
    application::{ServerCreation, valid_server_name},
    chat::ServerLayout,
};

use iced::widget::column;

use super::*;
use crate::fonts::MEDIUM;

const NAME_FIELD: &str = "new-server-name";

/// The picker width, row height and maximum list height.
const CREATE_WIDTH: f32 = 444.0;
const TEMPLATE_HEIGHT: f32 = 54.0;
const TEMPLATE_LIST: f32 = 322.0;

/// Starter layouts, with the caption and glyph shown in the picker.
const LAYOUTS: [(ServerLayout, &str, Icon, u32); 7] = [
    (
        ServerLayout::Basic,
        later("Create My Own"),
        Icon::Pen,
        0x57c98b,
    ),
    (
        ServerLayout::Gaming,
        later("Gaming"),
        Icon::Gamepad,
        0x8ea1e1,
    ),
    (
        ServerLayout::Friends,
        later("Friends"),
        Icon::Heart,
        0xf47fb3,
    ),
    (
        ServerLayout::Study,
        later("Study Group"),
        Icon::AcademicCap,
        0xf59f45,
    ),
    (
        ServerLayout::SchoolClub,
        later("School Club"),
        Icon::Book,
        0x5b7fe8,
    ),
    (
        ServerLayout::LocalCommunity,
        later("Local Community"),
        Icon::Community,
        0x51beac,
    ),
    (
        ServerLayout::Creators,
        later("Artists & Creators"),
        Icon::Palette,
        0xe88c74,
    ),
];

#[derive(Default)]
pub struct Draft {
    layout: Option<ServerLayout>,
    showing: bool,
    name: Option<String>,
    icon: Option<PathBuf>,
    picker: Option<u64>,
    picker_sequence: u64,
}

#[derive(Clone, Debug)]
pub enum Event {
    Layout(Option<ServerLayout>),
    Name(String),
    ChooseIcon,
    PickedIcon(u64, Option<PathBuf>),
    RemoveIcon,
    Confirm,
    Close,
}

fn create(event: Event) -> Message {
    message(super::Event::Create(event))
}

impl Draft {
    /// Focus the name when entering the form, including returning from Join.
    pub(super) fn sync(&mut self, state: &AppState) -> Option<&'static str> {
        let Some(dialog) = &state.server_creation else {
            *self = Self {
                picker_sequence: self.picker_sequence,
                ..Self::default()
            };
            return None;
        };
        let showing = state.join.is_none();
        let changed = self.showing != showing || self.layout != dialog.layout;
        self.showing = showing;
        self.layout = dialog.layout;
        if changed {
            self.picker = None;
        }
        if showing
            && dialog.layout.is_some()
            && self.name.is_none()
            && let Some(account) = state
                .account_name
                .as_deref()
                .filter(|name| !name.is_empty())
        {
            let name = i18n::fill(tr("{}’s server"), &[&account]);
            self.name = Some(name.chars().take(100).collect());
        }
        (showing && changed).then_some(if dialog.layout.is_some() {
            NAME_FIELD
        } else {
            NOBODY
        })
    }

    pub(super) fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        let ready = cx
            .state()
            .server_creation
            .as_ref()
            .is_some_and(|dialog| dialog.layout.is_some() && !dialog.creating())
            && cx.state().join.is_none();
        match event {
            Event::Layout(layout) => cx.dispatch(Action::ChooseServerLayout(layout)),
            Event::Name(name) if ready => self.name = Some(name),
            Event::ChooseIcon if ready && self.picker.is_none() => {
                self.picker_sequence = self.picker_sequence.wrapping_add(1);
                let picker = self.picker_sequence;
                self.picker = Some(picker);
                let title = tr("Choose a server icon");
                let pictures = tr("Pictures");
                return Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title(title)
                            .add_filter(pictures, &["png", "jpg", "jpeg", "webp"])
                            .pick_file()
                            .await
                            .map(|file| file.path().to_owned())
                    },
                    move |path| create(Event::PickedIcon(picker, path)),
                );
            }
            Event::PickedIcon(picker, path) if self.picker == Some(picker) => {
                self.picker = None;
                if ready && let Some(path) = path {
                    self.icon = Some(path);
                }
            }
            Event::RemoveIcon if ready => self.icon = None,
            Event::Confirm if ready && self.picker.is_none() => cx.dispatch(Action::CreateServer {
                name: self.name.clone().unwrap_or_default(),
                icon: self.icon.clone(),
            }),
            Event::Close => cx.dispatch(Action::CloseAddServer),
            _ => {}
        }
        Task::none()
    }

    pub(super) fn view<'a>(
        &'a self,
        cx: Ctx<'a>,
        dialog: &'a ServerCreation,
    ) -> Element<'a, Message> {
        let look = cx.look;
        let theme = look.theme;
        let close = create(Event::Close);
        let Some(layout) = dialog.layout else {
            return templates(look, close);
        };

        let ready = !dialog.creating();
        let can_create = ready && self.picker.is_none();
        let name = self.name.as_deref().unwrap_or_default();
        let channels = layout.text_channels();
        let summary = if channels.is_empty() {
            tr("Starts with the default text and voice channels.").to_owned()
        } else {
            let channels = format!("#{}", channels.join(", #"));
            i18n::fill(tr("Starts with {} and a voice channel."), &[&channels])
        };
        let title = tr("Customize your server");
        let title = if ready {
            look.dialog_head(title, close.clone())
        } else {
            text(title).size(18).color(theme.text).into()
        };
        let mut body = vec![
            title,
            form::note(theme, summary),
            form::heading(theme, tr("Server name *")),
            form::field(theme, NAME_FIELD, tr("My server"), name)
                .padding([8.0, 10.0])
                .on_input_maybe(ready.then_some(|name| create(Event::Name(name))))
                .on_submit_maybe(
                    (can_create && valid_server_name(name)).then(|| create(Event::Confirm)),
                )
                .into(),
        ];
        let icon_button = action(
            theme,
            if self.icon.is_some() {
                tr("Change icon")
            } else {
                tr("Choose icon")
            },
            Kind::Plain,
            can_create.then(|| create(Event::ChooseIcon)),
        );
        let mut icon_row = row![icon_button].spacing(8).align_y(Alignment::Center);
        if let Some(name) = self.icon.as_ref().and_then(|file| file.file_name()) {
            icon_row = icon_row.push(action(
                theme,
                tr("Remove icon"),
                Kind::Ghost,
                can_create.then(|| create(Event::RemoveIcon)),
            ));
            body.push(form::note(theme, name.to_string_lossy().into_owned()));
        }
        body.push(icon_row.into());
        if let Some(error) = &dialog.error {
            body.push(form::note_in(error.as_str(), theme.danger));
        }
        body.push(buttons([
            action(
                theme,
                tr("Back"),
                Kind::Ghost,
                ready.then(|| create(Event::Layout(None))),
            )
            .into(),
            action(
                theme,
                if ready {
                    tr("Create server")
                } else {
                    tr("Creating server...")
                },
                Kind::Primary,
                (can_create && valid_server_name(name)).then(|| create(Event::Confirm)),
            )
            .into(),
        ]));
        look.framed(body, NARROW, close)
    }
}

/// The scrollable layout picker and the shortcut to join by invite.
fn templates<'a>(look: crate::look::Look<'a>, close: Message) -> Element<'a, Message> {
    let theme = look.theme;
    let title = container(
        text(tr("Create Your Server"))
            .size(24)
            .font(BOLD)
            .color(theme.text)
            .line_height(iced::Pixels(30.0))
            .wrapping(text::Wrapping::None)
            .align_x(Alignment::Center)
            .width(Length::Fill),
    )
    .padding(iced::Padding {
        top: 12.0,
        ..iced::Padding::ZERO
    });
    let corner = container(
        look.icon_button(Icon::X, tr("Close"), close.clone())
            .size(32.0, 28)
            .ink(theme.secondary)
            .below(),
    )
    .width(Length::Fill)
    .align_x(Alignment::End)
    .padding(iced::Padding {
        top: 12.0,
        right: 19.0,
        ..iced::Padding::ZERO
    });
    let head = container(stack![title, corner]).height(42);
    let about = container(
        text(tr(
            "Your server is where you and your friends hang out. Make yours and start talking.",
        ))
        .size(14)
        .line_height(iced::Pixels(20.0))
        .color(theme.secondary)
        .align_x(Alignment::Center)
        .width(Length::Fill),
    )
    .height(40)
    .padding(iced::Padding {
        left: 20.0,
        right: 20.0,
        ..iced::Padding::ZERO
    });

    let space = |height: f32| iced::widget::Space::new().height(height);
    let entry = |&(layout, caption, icon, ink): &(ServerLayout, &'static str, Icon, u32)| {
        template(look, tr(caption), icon, hex(ink), layout)
    };
    let mut list = column![entry(&LAYOUTS[0])]
        .push(space(17.0))
        .push(
            text(tr("Start from a template"))
                .size(12)
                .font(BOLD)
                .color(theme.text)
                .line_height(iced::Pixels(16.0))
                .wrapping(text::Wrapping::None),
        )
        .push(space(10.0));
    for (at, layout) in LAYOUTS[1..].iter().enumerate() {
        if at > 0 {
            list = list.push(space(8.0));
        }
        list = list.push(entry(layout));
    }
    let scroller = scroll_view(container(list).padding(iced::Padding {
        left: 24.0,
        right: 16.0,
        ..iced::Padding::ZERO
    }))
    .height(TEMPLATE_LIST)
    .style(scrollbar(theme));

    let invite = column![
        text(tr("Already have an invite?"))
            .size(20)
            .font(BOLD)
            .color(theme.text)
            .line_height(iced::Pixels(24.0))
            .wrapping(text::Wrapping::None)
            .align_x(Alignment::Center)
            .width(Length::Fill),
        action(
            theme,
            tr("Join a Server"),
            Kind::Plain,
            Some(message(super::Event::OpenJoinFromAdd)),
        )
        .wide(40.0),
    ]
    .spacing(8)
    .padding(iced::Padding {
        top: 16.0,
        right: 24.0,
        bottom: 16.0,
        left: 24.0,
    });
    let body = column![head, space(22.0), about, space(32.0), scroller, invite];
    dialog(theme, body.into(), CREATE_WIDTH, close)
}

/// One template: its picture, its name and a chevron, on a card that
/// lightens under the pointer, 54 px tall.
fn template<'a>(
    look: crate::look::Look<'a>,
    caption: &'static str,
    icon: Icon,
    ink: iced::Color,
    layout: ServerLayout,
) -> Element<'a, Message> {
    let theme = look.theme;
    let face = row![
        look.glyph(icon, 20, ink),
        text(caption)
            .size(16)
            .font(MEDIUM)
            .color(theme.text)
            .wrapping(text::Wrapping::None)
            .width(Length::Fill),
        look.glyph(Icon::ChevronRight, 28, theme.secondary),
    ]
    .spacing(16)
    .align_y(Alignment::Center);
    let (rest, lit, edge) = (theme.wash(0.041), theme.wash(0.08), theme.wash(0.10));
    button(
        container(face)
            .width(Length::Fill)
            .padding(iced::Padding {
                left: 17.0,
                right: 13.0,
                ..iced::Padding::ZERO
            })
            .height(TEMPLATE_HEIGHT)
            .align_y(Alignment::Center),
    )
    .padding(0)
    .width(Length::Fill)
    .on_press(create(Event::Layout(Some(layout))))
    .style(move |_, status| {
        let lit_now = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(if lit_now { lit } else { rest }.into()),
            border: crate::style::edge(edge, 8.0),
            ..Default::default()
        }
    })
    .into()
}
