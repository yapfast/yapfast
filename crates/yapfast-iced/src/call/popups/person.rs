//! Someone's voice menu over their row, and their volumes
//! (`VolumeMenu`).

use iced::{
    Element, Point, Task,
    widget::{column, operation, text},
};
use yapfast_core::{application::MenuChoice, chat::MemberChange, voice::Command};

use crate::call::Event;
use crate::{
    app::{App, Message, NOBODY},
    controls::{self, Check},
    fonts::BOLD,
    menus::{Menu, MenuAct, MenuRow},
    pictures::Icon,
};

/// Someone's volumes over their voice menu.
#[derive(Clone, Debug)]
pub struct Volumes {
    /// Their network id, which volumes are kept by, and name.
    pub person: String,
    pub name: String,
    /// They stream, which has a volume of its own.
    pub live: bool,
    pub voice: u32,
    pub stream: u32,
}

impl App {
    /// Someone's voice or stream volume from their menu: within a step of
    /// the default is the default, as Discord snaps it; the drag's last is
    /// kept.
    pub(in crate::call) fn person_volume(&mut self, stream: bool, value: u32, done: bool) {
        use yapfast_core::volume::{self, Kind};
        let Some(volumes) = self.menu.as_mut().and_then(|menu| menu.volumes.as_mut()) else {
            return;
        };
        if stream {
            volumes.stream = value;
        } else {
            volumes.voice = value;
        }
        let person = volumes.person.clone();
        let kind = if stream { Kind::Stream } else { Kind::Voice };
        let default = kind.default_gain();
        let gain = if (value as f32 - volume::to_slider(default)).abs() < 1.0 {
            default
        } else {
            volume::from_slider(value as f32)
        };
        volume::set(kind, &person, gain);
        if done {
            let voice = &mut self.layout.voice;
            let (kept, key) = if stream {
                (&mut voice.stream_volumes, "stream_volumes")
            } else {
                (&mut voice.volumes, "volumes")
            };
            if gain == default {
                kept.remove(&person);
            } else {
                kept.insert(person, gain);
            }
            crate::layout::store(self.client.root(), key, &*kept);
        }
    }

    /// Someone's menu in a call: their volumes, while they're someone
    /// else, over what one may do with them there, as the Slint client's
    /// `open-voice-menu`.
    pub(in crate::call) fn open_voice_menu(
        &mut self,
        room: String,
        user: String,
        at: Point,
    ) -> Task<Message> {
        let state = self.client.state();
        let in_call = state
            .voice
            .participants
            .iter()
            .find(|person| person.person == user || person.id == user);
        let listed = self.client.app.voice_person(&room, &user);
        let person = in_call.map_or(user.clone(), |person| person.person.clone());
        let local = in_call.is_some_and(|person| person.local);
        let name = in_call
            .map(|person| person.user.clone())
            .or_else(|| listed.map(|person| person.name.clone()))
            .unwrap_or_default();
        let live = in_call.map_or(listed.is_some_and(|person| person.streaming), |person| {
            person.streaming
        });
        let server_muted = listed.is_some_and(|person| person.server_muted);
        let server_deafened = listed.is_some_and(|person| person.server_deafened);
        let slider = |kind| {
            yapfast_core::volume::to_slider(yapfast_core::volume::get(kind, &person)).round() as u32
        };
        let volumes = (!local).then(|| Volumes {
            person: person.clone(),
            name,
            live,
            voice: slider(yapfast_core::volume::Kind::Voice),
            stream: slider(yapfast_core::volume::Kind::Stream),
        });
        let app = &self.client.app;
        let mut choices = app.voice_person_choices(&room, &person);
        choices.sort_by_key(|choice| crate::person::discord_rank(*choice));
        let developer = self.layout.developer_mode;
        let voice = &state.voice;
        let mut group = None;
        let rows: Vec<MenuRow> = choices
            .into_iter()
            .filter(|choice| developer || *choice != MenuChoice::CopyId)
            .map(|choice| {
                let starts = crate::places::group_of(choice);
                let separated = group.replace(starts).is_some_and(|last| last != starts);
                let ticked = match choice {
                    MenuChoice::LocalMute => Some(yapfast_core::volume::muted(&person)),
                    MenuChoice::SoundboardMute => {
                        Some(yapfast_core::volume::soundboard_muted(&person))
                    }
                    MenuChoice::SelfMute => Some(voice.muted),
                    MenuChoice::SelfDeafen => Some(voice.deafened),
                    MenuChoice::ServerMute => Some(server_muted),
                    MenuChoice::ServerDeafen => Some(server_deafened),
                    _ => None,
                };
                let mut row = MenuRow {
                    icon: (choice == MenuChoice::CopyId).then_some(Icon::Id),
                    danger: crate::person::danger(choice),
                    separated,
                    check: ticked.map_or(Check::None, Check::Box),
                    ..MenuRow::plain(MenuAct::Person(choice), crate::places::label(choice))
                };
                crate::person::submenu(
                    app,
                    &mut row,
                    choice,
                    Some(&person),
                    state.selected.as_deref(),
                    Some((&room, &person)),
                );
                row.disabled = !(crate::person::acts(choice)
                    || ticked.is_some()
                    || choice == MenuChoice::Disconnect
                    || !row.submenu.is_empty());
                row
            })
            .collect();
        if rows.is_empty() && volumes.is_none() {
            return Task::none();
        }
        self.menu = Some(Menu {
            place: Some(person.clone()),
            voice: Some((room, person)),
            volumes,
            ..Menu::new(at, rows).width(crate::menus::FIELD_WIDTH)
        });
        operation::focus(NOBODY)
    }

    /// A row of someone's voice menu: a checkbox's new state, or what the
    /// person menu does.
    pub fn choose_voice(
        &mut self,
        room: String,
        user: String,
        choice: MenuChoice,
        ticked: Option<bool>,
    ) -> Task<Message> {
        use yapfast_core::application::Action;
        let root = self.client.root().to_owned();
        let member = |change| Action::ChangeMember {
            place: room.clone(),
            user: user.clone(),
            change,
        };
        let action = match (choice, ticked) {
            (MenuChoice::LocalMute, Some(muted)) => {
                yapfast_core::volume::mute(&user, muted);
                let people = &mut self.layout.voice.muted_people;
                if muted {
                    people.insert(user.clone());
                } else {
                    people.remove(&user);
                }
                crate::layout::store(&root, "muted_people", &*people);
                None
            }
            (MenuChoice::SoundboardMute, Some(muted)) => {
                yapfast_core::volume::mute_soundboard(&user, muted);
                let people = &mut self.layout.voice.soundboard_muted_people;
                if muted {
                    people.insert(user.clone());
                } else {
                    people.remove(&user);
                }
                crate::layout::store(&root, "soundboard_muted_people", &*people);
                None
            }
            (MenuChoice::SelfMute, Some(muted)) => Some(Action::Voice(Command::Mute(muted))),
            (MenuChoice::SelfDeafen, Some(deafened)) => {
                Some(Action::Voice(Command::Deafen(deafened)))
            }
            (MenuChoice::ServerMute, Some(mute)) => Some(member(MemberChange::ServerMute(mute))),
            (MenuChoice::ServerDeafen, Some(deaf)) => {
                Some(member(MemberChange::ServerDeafen(deaf)))
            }
            (MenuChoice::Disconnect, _) => Some(member(MemberChange::Move(None))),
            _ => return self.choose_person(user, crate::person::CONTEXT_MENU, choice),
        };
        match action {
            Some(action) => {
                self.client.dispatch(action);
                self.sync()
            }
            None => Task::none(),
        }
    }

    /// Someone's volumes, as the Slint client's `VolumeMenu`: their name,
    /// then their voice's slider, and their stream's while they share.
    pub fn volumes_view<'a>(&'a self, volumes: &'a Volumes) -> Element<'a, Message> {
        let theme = &self.theme;
        let slider = |label, stream: bool, value| {
            controls::slider(label, value, 200, move |value| {
                Message::Call(Event::Volume(stream, value, false))
            })
            .on_release(Message::Call(Event::Volume(stream, value, true)))
            .view(theme)
        };
        let mut panel = column![
            text(volumes.name.as_str())
                .size(15)
                .font(BOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None),
            slider("User volume", false, volumes.voice),
        ]
        .spacing(12)
        .padding(12);
        if volumes.live {
            panel = panel.push(slider("Stream volume", true, volumes.stream));
        }
        panel.into()
    }
}
