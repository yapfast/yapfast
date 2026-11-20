//! Server creation using Yapfast's own dialog controls.

use std::path::PathBuf;

use yapfast_core::{
    application::{ServerCreation, valid_server_name},
    chat::ServerLayout,
};

use super::*;

const NAME_FIELD: &str = "new-server-name";

const LAYOUTS: [(ServerLayout, &str); 5] = [
    (ServerLayout::Basic, "Basic"),
    (ServerLayout::Gaming, "Gaming"),
    (ServerLayout::Friends, "Friends"),
    (ServerLayout::Study, "Study Group"),
    (ServerLayout::SchoolClub, "School Club"),
];

#[derive(Default)]
pub struct Draft {
    layout: Option<ServerLayout>,
    showing: bool,
    name: String,
    icon: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub enum Event {
    Layout(Option<ServerLayout>),
    Name(String),
    ChooseIcon,
    PickedIcon(PathBuf),
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
            *self = Self::default();
            return None;
        };
        let showing = state.join.is_none();
        let changed = self.showing != showing || self.layout != dialog.layout;
        self.showing = showing;
        self.layout = dialog.layout;
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
            Event::Name(name) if ready => self.name = name,
            Event::ChooseIcon if ready => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Choose a server icon")
                            .add_filter("Pictures", &["png", "jpg", "jpeg", "webp"])
                            .pick_file()
                            .await
                            .map(|file| file.path().to_owned())
                    },
                    |path| path.map_or(Message::Nothing, |path| create(Event::PickedIcon(path))),
                );
            }
            Event::PickedIcon(path) if ready => self.icon = Some(path),
            Event::RemoveIcon if ready => self.icon = None,
            Event::Confirm if ready => cx.dispatch(Action::CreateServer {
                name: self.name.clone(),
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
            let mut body = vec![
                look.dialog_head("Add a server", close.clone()),
                form::note(
                    theme,
                    "Choose a starting layout, or join a server with an invite.",
                ),
            ];
            body.extend(LAYOUTS.iter().map(|&(layout, caption)| {
                action(
                    theme,
                    caption,
                    Kind::Plain,
                    Some(create(Event::Layout(Some(layout)))),
                )
                .wide(form::BOX)
                .into()
            }));
            body.push(buttons([action(
                theme,
                "Join with an invite",
                Kind::Primary,
                Some(message(super::Event::OpenJoinFromAdd)),
            )
            .into()]));
            return look.framed(body, NARROW, close);
        };

        let ready = !dialog.creating();
        let channels = layout.text_channels();
        let summary = if channels.is_empty() {
            "Starts with Discord's default channels.".to_owned()
        } else {
            format!("Starts with #{} and a voice channel.", channels.join(", #"))
        };
        let title = if ready {
            look.dialog_head("Set up your server", close.clone())
        } else {
            text("Set up your server").size(18).color(theme.text).into()
        };
        let mut body = vec![
            title,
            form::note(theme, summary),
            form::heading(theme, "SERVER NAME"),
            form::field(theme, NAME_FIELD, "My server", &self.name)
                .padding([8.0, 10.0])
                .on_input_maybe(ready.then_some(|name| create(Event::Name(name))))
                .on_submit_maybe(ready.then(|| create(Event::Confirm)))
                .into(),
        ];
        let icon_button = action(
            theme,
            if self.icon.is_some() {
                "Change icon"
            } else {
                "Choose icon"
            },
            Kind::Plain,
            ready.then(|| create(Event::ChooseIcon)),
        );
        let mut icon_row = row![icon_button].spacing(8).align_y(Alignment::Center);
        if let Some(name) = self.icon.as_ref().and_then(|file| file.file_name()) {
            icon_row = icon_row.push(action(
                theme,
                "Remove icon",
                Kind::Ghost,
                ready.then(|| create(Event::RemoveIcon)),
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
                "Back",
                Kind::Ghost,
                ready.then(|| create(Event::Layout(None))),
            )
            .into(),
            action(
                theme,
                if ready {
                    "Create server"
                } else {
                    "Creating server..."
                },
                Kind::Primary,
                (ready && valid_server_name(&self.name)).then(|| create(Event::Confirm)),
            )
            .into(),
        ]));
        look.framed(body, NARROW, close)
    }
}
