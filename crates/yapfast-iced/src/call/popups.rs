//! What opens from the call's controls, above the control: the device
//! menus (`QuickDeviceMenu`, with the microphone's `InputLevel` and noise
//! suppression), the share menu (`ShareMenu`), the soundboard
//! (`Soundboard`); and someone's voice menu, their volumes (`VolumeMenu`)
//! over their rows. As the Slint client's.

use std::{borrow::Cow, sync::Arc};

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Point, Task,
    advanced::image::Handle,
    widget::{
        Space, button, column, container, mouse_area, opaque, operation, row, scrollable, text,
    },
};
use yapfast_core::{
    application::MenuChoice,
    chat::MemberChange,
    meter::InputMeter,
    noise::Suppressor,
    voice::{self, Command, Resolution, StreamQuality},
};

use super::{Event, Popup};
use crate::{
    app::{App, Message, NOBODY},
    controls::{self, Check, Item, Light, heading, menu_panel, separator},
    fonts::BOLD,
    menus::{Menu, MenuAct, MenuRow},
    pictures::Icon,
    style::rounded,
    widgets::{Anchor, Hover, Pic, Place},
};

/// The device menus' width, the share menu's with screens and without,
/// the soundboard's.
const DEVICE_WIDTH: f32 = 280.0;
const SHARE_WIDE: f32 = 336.0;
const SHARE_NARROW: f32 = 240.0;
pub(crate) const SOUNDBOARD_WIDTH: f32 = 372.0;
const SOUNDBOARD_TALLEST: f32 = 440.0;
/// A soundboard row, and a sound's button in it.
const SOUND_ROW: f32 = 42.0;
const SOUND: f32 = 36.0;
/// A device menu's rows before its list scrolls.
const DEVICES_SHOWN: usize = 5;
/// The microphone's level, in segments.
const SEGMENTS: usize = 24;
/// A screen's picture in the share menu, in pixels.
const GLANCE: (u32, u32) = (160, 90);

/// The share menu's sources and choices.
pub struct Share {
    /// The screens with their pictures once taken, the windows, the
    /// cameras; then every source in the menu's order (screens, windows,
    /// the system's dialog, cameras).
    pub screens: Vec<(String, Option<Handle>)>,
    pub windows: Vec<String>,
    pub cameras: Vec<String>,
    pub sources: Vec<yapfast_core::stream_worker::Source>,
    /// The system's dialog picks a screen or window.
    pub dialog: bool,
    pub pick: usize,
    pub quality: StreamQuality,
    /// It switches what's shared, at the stream's quality.
    pub switching: bool,
}

/// What's listed to share, as it comes from the thread that lists it.
pub struct Listing(pub Vec<yapfast_core::stream_worker::Shareable>);

impl std::fmt::Debug for Listing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} shareable", self.0.len())
    }
}

/// What's done in the share menu.
#[derive(Clone, Debug)]
pub enum ShareEvent {
    Listed(Arc<Listing>),
    /// A screen's picture, by the screen's id.
    Picture(u64, Handle),
    Pick(usize),
    Resolution(Resolution),
    Fps(u32),
    HighQuality,
    Go,
}

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
    pub(super) fn open_popup(&mut self, popup: Popup) -> Task<Message> {
        if self.call.popup == Some(popup) {
            self.close_popup();
            return Task::none();
        }
        self.close_popup();
        match popup {
            Popup::Input | Popup::Output => {
                self.client
                    .dispatch(yapfast_core::application::Action::Voice(Command::Devices));
                if popup == Popup::Input {
                    let microphone = self.client.state().voice.microphone.clone();
                    self.call.meter = Some(InputMeter::start(microphone.as_deref()));
                }
                self.call.popup = Some(popup);
            }
            Popup::Soundboard => {
                let pictures: Vec<String> = self
                    .client
                    .state()
                    .voice
                    .soundboard
                    .iter()
                    .flat_map(|groups| groups.iter())
                    .flat_map(|group| &group.sounds)
                    .filter_map(|sound| sound.picture.clone())
                    .collect();
                if !pictures.is_empty() {
                    self.client
                        .dispatch(yapfast_core::application::Action::LoadPictures(pictures));
                }
                self.call.popup = Some(popup);
            }
            // Shown once its sources are listed, so it opens at its height.
            Popup::Share => return list_sources(),
            Popup::Link => {
                self.call.popup = Some(popup);
                self.read_link();
                return Task::none();
            }
        }
        Task::batch([self.sync(), operation::focus(NOBODY)])
    }

    pub fn close_popup(&mut self) {
        let link = self.call.popup == Some(Popup::Link);
        self.call.popup = None;
        if link {
            self.call.details.tab = super::LinkTab::Connection;
            self.read_link();
        }
        self.call.share = None;
        self.call.listing = false;
        self.call.meter = None;
        self.call.level = 0.0;
    }

    pub(super) fn share_event(&mut self, event: ShareEvent) -> Task<Message> {
        match event {
            ShareEvent::Listed(listing) => {
                use yapfast_core::stream_worker::Source;
                let (mut screens, mut windows, mut cameras) = (vec![], vec![], vec![]);
                for listed in &listing.0 {
                    match listed.source {
                        Source::Display(_) => screens.push(listed),
                        Source::Camera(_) => cameras.push(listed),
                        _ => windows.push(listed),
                    }
                }
                let ids: Vec<u64> = screens
                    .iter()
                    .filter_map(|listed| match listed.source {
                        Source::Display(id) => Some(id),
                        _ => None,
                    })
                    .collect();
                // Where the system's dialog picks a screen or window, the
                // menu's first choice asks it, before the cameras.
                let dialog = yapfast_core::stream_worker::portal_picks();
                let sources = screens
                    .iter()
                    .chain(&windows)
                    .map(|listed| listed.source.clone())
                    .chain(dialog.then_some(Source::Screen))
                    .chain(cameras.iter().map(|listed| listed.source.clone()))
                    .collect();
                let voice = &self.client.state().voice;
                self.call.share = Some(Share {
                    screens: screens
                        .iter()
                        .map(|listed| (listed.title.clone(), None))
                        .collect(),
                    windows: windows.iter().map(|listed| listed.title.clone()).collect(),
                    cameras: cameras.iter().map(|listed| listed.title.clone()).collect(),
                    sources,
                    dialog,
                    pick: 0,
                    quality: voice.quality,
                    switching: voice.sharing.is_some(),
                });
                self.call.popup = Some(Popup::Share);
                return Task::batch([glance(ids), operation::focus(NOBODY)]);
            }
            ShareEvent::Picture(id, handle) => {
                if let Some(share) = &mut self.call.share {
                    let at = share.sources.iter().position(|source| {
                        *source == yapfast_core::stream_worker::Source::Display(id)
                    });
                    if let Some((_, picture)) = at.and_then(|at| share.screens.get_mut(at)) {
                        *picture = Some(handle);
                    }
                }
            }
            ShareEvent::Pick(pick) => {
                if let Some(share) = &mut self.call.share {
                    share.pick = pick;
                }
            }
            ShareEvent::Resolution(resolution) => {
                if let Some(share) = &mut self.call.share {
                    share.quality.resolution = resolution;
                }
            }
            ShareEvent::Fps(fps) => {
                if let Some(share) = &mut self.call.share {
                    share.quality.fps = fps;
                }
            }
            ShareEvent::HighQuality => {
                if let Some(share) = &mut self.call.share {
                    share.quality.high_quality = !share.quality.high_quality;
                }
            }
            ShareEvent::Go => {
                let Some(share) = self.call.share.take() else {
                    return Task::none();
                };
                let picked = share.sources.get(share.pick).cloned();
                let command = if share.switching {
                    if let Some(picked) = picked {
                        yapfast_core::stream_worker::pick(picked);
                    }
                    Command::SwitchLive
                } else {
                    yapfast_core::stream_worker::pick(
                        picked.unwrap_or(yapfast_core::stream_worker::Source::Screen),
                    );
                    Command::GoLive(share.quality)
                };
                self.close_popup();
                self.client
                    .dispatch(yapfast_core::application::Action::Voice(command));
                return Task::batch([self.sync(), self.refocus()]);
            }
        }
        Task::none()
    }

    /// A device menu's volume: how loud calls send the microphone or play,
    /// kept as the drag lets go.
    pub(super) fn device_volume(&mut self, input: bool, value: u32, done: bool) {
        let value = value.min(200) as u8;
        let voice = &mut self.layout.voice;
        let (kept, key) = if input {
            (&mut voice.microphone_volume, "microphone_volume")
        } else {
            (&mut voice.speaker_volume, "speaker_volume")
        };
        *kept = Some(value);
        voice.play_volumes();
        if done {
            crate::layout::store(self.client.root(), key, value);
        }
    }

    /// Someone's voice or stream volume from their menu: within a step of
    /// the default is the default, as Discord snaps it; the drag's last is
    /// kept.
    pub(super) fn person_volume(&mut self, stream: bool, value: u32, done: bool) {
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
    pub(super) fn open_voice_menu(
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

    /// The popup open from a control, above it.
    pub fn call_popup_view(&self) -> Option<Element<'_, Message>> {
        let popup = self.call.popup?;
        let (panel, width) = match popup {
            Popup::Input => (self.device_menu(true), DEVICE_WIDTH),
            Popup::Output => (self.device_menu(false), DEVICE_WIDTH),
            Popup::Share => {
                let share = self.call.share.as_ref()?;
                let width = if share.screens.is_empty() {
                    SHARE_NARROW
                } else {
                    SHARE_WIDE
                };
                (self.share_menu(share, width), width)
            }
            Popup::Soundboard => (self.soundboard(), SOUNDBOARD_WIDTH),
            // Under its signal, or above it without room, as the Slint
            // client's `call-link-menu`.
            Popup::Link => {
                let spot = self.call.spot.get();
                return Some(
                    Place::new(opaque(self.link_card()), spot.position(), 8.0)
                        .anchor(Anchor::Span(spot.height))
                        .into(),
                );
            }
        };
        let spot = self.call.spot.get();
        let panel = container(panel).width(width).style(menu_panel(&self.theme));
        Some(
            Place::new(opaque(panel), Point::new(spot.x - 6.0, spot.y - 6.0), 8.0)
                .anchor(Anchor::Bottom)
                .into(),
        )
    }

    /// A row of a popup, as the Slint client's `MenuItem`, lit under the
    /// pointer.
    fn popup_row<'a>(
        &self,
        label: impl Into<Cow<'a, str>>,
        subtitle: Option<Cow<'a, str>>,
        icon: Option<Icon>,
        check: Check,
        press: Option<Message>,
    ) -> Element<'a, Message> {
        let item = Item {
            label: label.into(),
            icon,
            danger: false,
            slot: false,
            disabled: press.is_none(),
            subtitle,
            check,
        };
        self.menu_item(item, Light::Pointer, press.unwrap_or(Message::Nothing))
    }

    /// A device menu: the device in use, which opens the list of them; how
    /// loud; for the microphone its level and noise suppression; Voice
    /// Settings.
    fn device_menu(&self, input: bool) -> Element<'_, Message> {
        let theme = &self.theme;
        let voice = &self.client.state().voice;
        let (devices, chosen) = if input {
            (&voice.microphones, voice.microphone.as_deref())
        } else {
            (&voice.speakers, voice.speaker.as_deref())
        };
        let (rows, current) = device_rows(devices, chosen);
        let kind = if input { "Input" } else { "Output" };
        let subtitle = if devices.is_empty() {
            "None found".to_owned()
        } else {
            current
                .and_then(|current| rows.get(current).cloned())
                .unwrap_or_else(|| "System default".to_owned())
        };
        let mut menu = column![
            row![
                container(self.popup_row(
                    format!("{kind} Device"),
                    Some(subtitle.into()),
                    None,
                    Check::None,
                    Some(Message::Call(Event::List)),
                ))
                .width(Length::Fill),
            ]
            .push(
                container(self.glyph(Icon::ChevronRight, 24, theme.text))
                    .height(44)
                    .align_y(Alignment::Center)
            )
        ]
        .spacing(4)
        .padding(6);
        if self.call.listing {
            if rows.is_empty() {
                let empty = if input {
                    "No microphones found. Connect one, then reopen this menu."
                } else {
                    "No speakers found. Connect one, then reopen this menu."
                };
                menu =
                    menu.push(container(text(empty).size(13).color(theme.muted)).padding([0, 8]));
            } else {
                let mut list = column![];
                for (index, label) in rows.into_iter().enumerate() {
                    let check = if current == Some(index) {
                        Check::Current
                    } else {
                        Check::None
                    };
                    let press = (index < devices.len())
                        .then_some(Message::Call(Event::Device(input, index)));
                    list = list.push(self.popup_row(label, None, None, check, press));
                }
                let shown = devices.len().clamp(1, DEVICES_SHOWN) as f32;
                menu = menu.push(scrollable(list).height(shown * controls::ITEM + 4.0));
            }
        }
        let volume = if input {
            self.layout.voice.microphone_volume
        } else {
            self.layout.voice.speaker_volume
        }
        .unwrap_or(100);
        let volume = u32::from(volume);
        menu = menu.push(separator(theme)).push(
            container(
                controls::slider(format!("{kind} Volume"), volume, 200, move |value| {
                    Message::Call(Event::DeviceVolume(input, value, false))
                })
                .on_release(Message::Call(Event::DeviceVolume(input, volume, true)))
                .view(theme),
            )
            .padding([4, 8]),
        );
        if input {
            menu = menu.push(separator(theme)).push(self.input_level());
            if self.client.state().discord_user.is_some() {
                let current = match self.layout.voice.noise_suppression {
                    Suppressor::DeepFilterNet => 0,
                    Suppressor::RNNoise => 1,
                    Suppressor::Off => 2,
                };
                menu = menu.push(separator(theme)).push(
                    column![
                        heading(theme, "NOISE SUPPRESSION", 0.0),
                        controls::choices(
                            theme,
                            &["DeepFilterNet", "RNNoise", "Off"],
                            current,
                            |index| Message::Call(Event::Suppressor(match index {
                                0 => Suppressor::DeepFilterNet,
                                1 => Suppressor::RNNoise,
                                _ => Suppressor::Off,
                            })),
                        ),
                    ]
                    .spacing(2)
                    .padding(Padding {
                        left: 8.0,
                        right: 8.0,
                        bottom: 4.0,
                        top: 0.0,
                    }),
                );
            }
        }
        menu.push(separator(theme))
            .push(self.popup_row(
                "Voice Settings",
                None,
                Some(Icon::Settings),
                Check::None,
                Some(Message::Settings(crate::settings::Event::Open(
                    crate::settings::Section::Voice,
                ))),
            ))
            .into()
    }

    /// The microphone's level as a row of segments, as the Slint client's
    /// `InputLevel`: muted, it still shows, in a faded green, heard here and
    /// sent nowhere.
    fn input_level(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let voice = &self.client.state().voice;
        let level = self.call.level;
        let muted = voice.muted;
        let mut segments = row![].spacing(3).height(10);
        for segment in 0..SEGMENTS {
            let lit = (segment as f32 + 0.5) / SEGMENTS as f32 <= level;
            let ground = if !lit {
                theme.line
            } else if muted {
                crate::style::faded(theme.live, 0.4)
            } else {
                theme.live
            };
            segments = segments.push(
                container(Space::new())
                    .width(Length::Fill)
                    .height(10)
                    .style(move |_| rounded(ground, 2)),
            );
        }
        let note = if !yapfast_core::meter::AVAILABLE {
            Some("The level meter isn't available on this system yet.")
        } else if self.call.meter.as_ref().is_some_and(InputMeter::failed) {
            Some(
                "Yapfast can't read this microphone. Check that it's connected, or choose another.",
            )
        } else if muted {
            Some("Muted. Nothing you say is sent.")
        } else {
            None
        };
        let mut level = column![
            text("INPUT LEVEL").size(12).font(BOLD).color(theme.muted),
            segments
        ]
        .spacing(8)
        .padding([4, 8]);
        if let Some(note) = note {
            level = level.push(text(note).size(13).color(theme.muted));
        }
        level.into()
    }

    /// The share menu: what to share (screens as pictures two to a row,
    /// windows, the system's dialog, cameras), then the quality, and Go
    /// Live; while sharing, what to switch to, and Switch. The choices
    /// scroll in a window too short for them, over the button, which stays.
    fn share_menu<'a>(&'a self, share: &'a Share, width: f32) -> Element<'a, Message> {
        let theme = &self.theme;
        let pick = |index: usize| Message::Call(Event::Share(ShareEvent::Pick(index)));
        let checked = |index: usize| {
            if share.pick == index {
                Check::Current
            } else {
                Check::None
            }
        };
        let mut choices = column![].spacing(2);
        if !share.screens.is_empty() {
            choices = choices.push(heading(theme, "SCREENS", 4.0));
            let card = (width - 12.0 - 4.0) / 2.0;
            let mut cards = Vec::new();
            for (index, (title, picture)) in share.screens.iter().enumerate() {
                cards.push(self.screen_card(
                    title,
                    picture.as_ref(),
                    card,
                    share.pick == index,
                    pick(index),
                ));
            }
            let mut rows = column![].spacing(4);
            let mut cards = cards.into_iter();
            loop {
                let Some(first) = cards.next() else { break };
                let pair = match cards.next() {
                    Some(second) => row![first, second].spacing(4),
                    None => row![first],
                };
                rows = rows.push(container(pair).center_x(Length::Fill));
            }
            choices = choices.push(rows);
        }
        if !share.windows.is_empty() {
            let top = if share.screens.is_empty() { 4.0 } else { 8.0 };
            choices = choices.push(heading(theme, "WINDOWS", top));
            let first = share.screens.len();
            for (index, title) in share.windows.iter().enumerate() {
                let at = first + index;
                choices = choices.push(self.popup_row(
                    title.as_str(),
                    None,
                    None,
                    checked(at),
                    Some(pick(at)),
                ));
            }
        }
        let dialog_index = share.screens.len() + share.windows.len();
        let asks_dialog = share.dialog && !share.cameras.is_empty();
        if asks_dialog {
            choices = choices
                .push(heading(theme, "SCREEN", 4.0))
                .push(self.popup_row(
                    "Screen or window",
                    Some("Picked in the system's dialog".into()),
                    None,
                    checked(dialog_index),
                    Some(pick(dialog_index)),
                ));
        }
        if !share.cameras.is_empty() {
            let top = if dialog_index > 0 || asks_dialog {
                8.0
            } else {
                4.0
            };
            choices = choices.push(heading(theme, "CAMERAS", top));
            let first = dialog_index + usize::from(share.dialog);
            for (index, title) in share.cameras.iter().enumerate() {
                let at = first + index;
                choices = choices.push(self.popup_row(
                    title.as_str(),
                    None,
                    None,
                    checked(at),
                    Some(pick(at)),
                ));
            }
        }
        if !share.switching {
            let lists = dialog_index + share.cameras.len() > 0;
            let quality = share.quality;
            choices = choices.push(heading(theme, "RESOLUTION", if lists { 8.0 } else { 4.0 }));
            for (resolution, label) in [
                (Resolution::P720, "720p"),
                (Resolution::P1080, "1080p"),
                (Resolution::Source, "Source"),
            ] {
                let check = if quality.resolution == resolution {
                    Check::Current
                } else {
                    Check::None
                };
                choices = choices.push(self.popup_row(
                    label,
                    None,
                    None,
                    check,
                    Some(Message::Call(Event::Share(ShareEvent::Resolution(
                        resolution,
                    )))),
                ));
            }
            choices = choices.push(heading(theme, "FRAME RATE", 8.0));
            for fps in [15, 30, 60] {
                let check = if quality.fps == fps {
                    Check::Current
                } else {
                    Check::None
                };
                choices = choices.push(self.popup_row(
                    format!("{fps} fps"),
                    None,
                    None,
                    check,
                    Some(Message::Call(Event::Share(ShareEvent::Fps(fps)))),
                ));
            }
            choices = choices
                .push(heading(theme, "ENCODING", 8.0))
                .push(self.popup_row(
                    "Higher quality",
                    Some("Sharper, at twice the CPU".into()),
                    None,
                    Check::Box(quality.high_quality),
                    Some(Message::Call(Event::Share(ShareEvent::HighQuality))),
                ));
        }
        let label = if share.switching { "Switch" } else { "Go Live" };
        let start: Element<'_, Message> = controls::action(
            theme,
            label,
            controls::Kind::Primary,
            Some(Message::Call(Event::Share(ShareEvent::Go))),
        )
        .icon(self.icon(Icon::ScreenShare, 18, crate::theme::rgb(theme.on_accent)))
        .wide(38.0)
        .into();
        // The tallest the choices get: from the window's top margin to
        // the control, less the button's room.
        let spot = self.call.spot.get();
        let tallest = (spot.y - 6.0 - 8.0 - 12.0 - 52.0).max(80.0);
        column![
            container(scrollable(choices).height(Length::Shrink)).max_height(tallest),
            container(start).padding(Padding {
                top: 10.0,
                left: 4.0,
                right: 4.0,
                bottom: 4.0,
            }),
        ]
        .padding(6)
        .into()
    }

    /// A screen in the share menu: its picture, once taken, over its name,
    /// as the Slint client's `ScreenCard`.
    fn screen_card<'a>(
        &'a self,
        title: &'a str,
        picture: Option<&Handle>,
        width: f32,
        checked: bool,
        press: Message,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let inner = width - 8.0;
        let height = inner * 9.0 / 16.0;
        let shown: Element<'a, Message> = match picture {
            Some(handle) => iced::widget::image(handle.clone())
                .width(inner)
                .height(height)
                .content_fit(iced::ContentFit::Contain)
                .into(),
            None => container(self.glyph(Icon::ScreenShare, 24, theme.faint))
                .center(Length::Fill)
                .into(),
        };
        let (sunken, edge) = (
            theme.sunken,
            if checked { theme.accent } else { theme.line },
        );
        let frame = container(shown)
            .width(inner)
            .height(height)
            .clip(true)
            .style(move |_| container::Style {
                background: Some(sunken.into()),
                border: Border {
                    color: edge,
                    width: if checked { 2.0 } else { 1.0 },
                    radius: 4.0.into(),
                },
                ..Default::default()
            });
        let (ink, lit_ink) = if checked {
            (theme.text, theme.text)
        } else {
            (theme.secondary, theme.text)
        };
        let active = theme.active;
        button(
            column![
                frame,
                text(title)
                    .size(13)
                    .font(if checked { BOLD } else { crate::fonts::REGULAR })
                    .wrapping(text::Wrapping::None)
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
            ]
            .spacing(4),
        )
        .padding(4)
        .width(width)
        .on_press(press)
        .style(move |_, status| {
            let lit = controls::lights(status);
            button::Style {
                background: lit.then(|| active.into()),
                text_color: if lit { lit_ink } else { ink },
                border: Border::default().rounded(6),
                ..Default::default()
            }
        })
        .into()
    }

    /// Discord's soundboard in the call: the favourites, the call's
    /// server's sounds, others', Discord's own, three to a row. A click
    /// plays one for everyone; the pointer on one offers hearing it first
    /// and starring it. While deafened or server muted, why none plays.
    fn soundboard(&self) -> Element<'_, Message> {
        let voice = &self.client.state().voice;
        let server_muted = voice
            .participants
            .iter()
            .any(|person| person.local && person.server_muted);
        let note = if voice.deafened {
            Some("Soundboard disabled when deafened")
        } else if server_muted {
            Some("Soundboard disabled when server muted")
        } else {
            None
        };
        let groups = voice.soundboard.as_deref().unwrap_or(&[]);
        self.sound_board("Soundboard", groups, note, true, |sound| {
            Message::Call(Event::Play(sound.id.clone()))
        })
    }

    /// A board of sounds, as the Slint client's `Soundboard`: `title`, a
    /// note (while there is one, no sound can be chosen), the groups'
    /// sounds three to a row under their names, `chosen`'s message for a
    /// click on one, and with `extras` hearing it first and its star under
    /// the pointer. The call's soundboard, and the entrance sound's choice
    /// in Settings.
    pub(crate) fn sound_board<'a>(
        &'a self,
        title: &'a str,
        groups: &'a [voice::SoundGroup],
        note: Option<&'a str>,
        extras: bool,
        chosen: impl Fn(&voice::SoundChoice) -> Message,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let mut top = column![text(title).size(16).font(BOLD).color(theme.text)]
            .spacing(4)
            .padding(Padding {
                top: 14.0,
                left: 14.0,
                right: 14.0,
                bottom: 4.0,
            });
        if let Some(note) = note {
            top = top.push(text(note).size(13).color(theme.muted));
        }
        if groups.is_empty() {
            top = top.push(text("Loading sounds...").size(13).color(theme.muted));
        }
        let cell = (SOUNDBOARD_WIDTH - 20.0 - 12.0) / 3.0;
        let mut rows = column![].padding([0, 10]);
        let mut count = 0;
        for group in groups {
            rows = rows.push(
                container(
                    text(group.name.to_uppercase())
                        .size(12)
                        .font(BOLD)
                        .color(theme.muted)
                        .wrapping(text::Wrapping::None),
                )
                .height(SOUND_ROW)
                .padding(Padding {
                    bottom: 8.0,
                    ..Padding::ZERO
                })
                .align_y(Alignment::End),
            );
            count += 1;
            for chunk in group.sounds.chunks(3) {
                let mut line = row![].spacing(6);
                for sound in chunk {
                    let press = note.is_none().then(|| chosen(sound));
                    line = line.push(self.sound_button(sound, cell, press, extras));
                }
                rows = rows.push(container(line).height(SOUND_ROW).align_y(Alignment::Center));
                count += 1;
            }
        }
        let listed = (count.max(1) as f32) * SOUND_ROW + 10.0;
        let room = (SOUNDBOARD_TALLEST - 80.0).max(SOUND_ROW);
        column![
            top,
            scrollable(rows).height(if listed > room {
                Length::Fixed(room)
            } else {
                Length::Shrink
            })
        ]
        .width(SOUNDBOARD_WIDTH)
        .padding(Padding {
            bottom: 10.0,
            ..Padding::ZERO
        })
        .into()
    }

    /// A sound to play, as the Slint client's `SoundButton`: its emoji and
    /// name; with the pointer on it, a button to hear it first and its star.
    fn sound_button<'a>(
        &'a self,
        sound: &'a voice::SoundChoice,
        width: f32,
        press: Option<Message>,
        extras: bool,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
        let enabled = press.is_some();
        let picture = match &sound.picture {
            Some(url) => self.picture(url, 18, 18, false),
            None => {
                emojis::get(&sound.emoji).and_then(|emoji| self.emoji_picture(emoji.as_str(), 18))
            }
        };
        let mut face = row![].spacing(8).align_y(Alignment::Center);
        if let Some(picture) = picture {
            face = face.push(Pic::square(picture, 18.0));
        } else if sound.picture.is_some() {
            face = face.push(Space::new().width(18).height(18));
        }
        face = face.push(
            text(sound.name.as_str())
                .size(14)
                .color(theme.text)
                .wrapping(text::Wrapping::None)
                .width(Length::Fill),
        );
        let (rest, lit) = (theme.button_default, theme.button_default_hover);
        let opacity = if enabled { 1.0 } else { 0.5 };
        let base = button(
            container(face)
                .height(Length::Fill)
                .align_y(Alignment::Center)
                .clip(true)
                .style(move |_| container::Style {
                    text_color: None,
                    ..Default::default()
                }),
        )
        .padding([0, 10])
        .width(width)
        .height(SOUND)
        .on_press_maybe(press)
        .style(move |_, status| button::Style {
            background: Some(
                crate::style::faded(
                    if enabled && controls::lights(status) {
                        lit
                    } else {
                        rest
                    },
                    opacity,
                )
                .into(),
            ),
            border: Border::default().rounded(8),
            ..Default::default()
        });
        let small = |icon, label: String, tint, ground: Color, message| {
            let face = button(self.glyph(icon, 15, tint).boxed(26))
                .padding(0)
                .width(26)
                .height(26)
                .on_press(message)
                .style(move |_, _| button::Style {
                    background: Some(ground.into()),
                    border: Border::default().rounded(6),
                    ..Default::default()
                });
            iced::widget::tooltip(
                face,
                controls::tip(theme, label),
                iced::widget::tooltip::Position::Top,
            )
            .gap(6)
        };
        let star = |ground| {
            let (icon, tint, label) = if sound.favorite {
                (
                    Icon::StarFilled,
                    crate::style::hex(crate::style::CROWN),
                    "Remove from Favorites",
                )
            } else {
                (Icon::Star, theme.secondary, "Add to Favorites")
            };
            small(
                icon,
                label.to_owned(),
                tint,
                ground,
                Message::Call(Event::Favorite(sound.id.clone(), !sound.favorite)),
            )
        };
        let corner = |content: Element<'a, Message>| {
            container(content)
                .padding([0, 4])
                .width(width)
                .height(SOUND)
                .align_x(Alignment::End)
                .align_y(Alignment::Center)
        };
        if !extras {
            return base.into();
        }
        // A favourite's star stays.
        let base: Element<'a, Message> = if sound.favorite {
            iced::widget::stack![base, corner(star(rest).into())].into()
        } else {
            base.into()
        };
        let over = corner(
            row![
                small(
                    Icon::Volume,
                    format!("Preview {}", sound.name),
                    theme.text,
                    lit,
                    Message::Call(Event::Preview(sound.id.clone())),
                ),
                star(lit),
            ]
            .spacing(4)
            .into(),
        );
        mouse_area(Hover::new(base, over)).into()
    }
}

/// A device menu's rows and the chosen one's: without a choice, voice uses
/// the system's default, listed first; a chosen device that isn't
/// connected shows last, as not connected, as the Slint client's
/// `device_rows`.
pub fn device_rows(
    devices: &[voice::Device],
    chosen: Option<&str>,
) -> (Vec<String>, Option<usize>) {
    let mut rows: Vec<String> = devices
        .iter()
        .map(|device| device_label(&device.name))
        .collect();
    let index = match chosen {
        None => (!devices.is_empty()).then_some(0),
        Some(chosen) => Some(
            devices
                .iter()
                .position(|device| device.id == chosen)
                .or_else(|| {
                    let default = |id: &str| id.starts_with(DEFAULT_DEVICE);
                    default(chosen)
                        .then(|| devices.iter().position(|device| default(&device.id)))
                        .flatten()
                })
                .unwrap_or_else(|| {
                    let name = chosen
                        .strip_prefix(voice::NAMED)
                        .map_or_else(|| "Chosen device".to_owned(), device_label);
                    rows.push(format!("{name} (not connected)"));
                    rows.len() - 1
                }),
        ),
    };
    (rows, index)
}

const DEFAULT_DEVICE: &str = "name:default: ";

fn device_label(name: &str) -> String {
    match name.strip_prefix("default: ") {
        Some(device) => format!("Default ({device})"),
        None => name.to_owned(),
    }
}

/// Lists what can be shared beside the window, as listing starts a
/// capturer for screens and one for windows and looks at each camera; the
/// menu opens when it's done.
fn list_sources() -> Task<Message> {
    Task::done(()).then(|()| {
        let (sender, receiver) = iced::futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = sender.send(yapfast_core::stream_worker::shareable());
        });
        Task::perform(receiver, |listed| {
            Message::Call(Event::Share(ShareEvent::Listed(Arc::new(Listing(
                listed.unwrap_or_default(),
            )))))
        })
    })
}

/// Pictures of the screens to share, from a worker, so capturing them
/// takes memory only until they're in.
fn glance(ids: Vec<u64>) -> Task<Message> {
    if ids.is_empty() {
        return Task::none();
    }
    // The worker starts as the task runs, not as it's made.
    Task::done(ids).then(|ids| {
        let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            use yapfast_core::stream_worker::{Report, Worker};
            let (_worker, mut reports) = match Worker::glance(ids, GLANCE.0, GLANCE.1) {
                Ok(started) => started,
                Err(error) => {
                    tracing::warn!("no pictures of the screens to share: {error:#}");
                    return;
                }
            };
            while let Some(report) = reports.blocking_recv() {
                match report {
                    Report::Screen {
                        id,
                        width,
                        height,
                        rgba,
                    } => {
                        let handle = Handle::from_rgba(width, height, rgba);
                        if sender.unbounded_send((id, handle)).is_err() {
                            break;
                        }
                    }
                    Report::Ended { error } => {
                        if let Some(error) = error {
                            tracing::warn!("no pictures of the screens to share: {error}");
                        }
                        break;
                    }
                    _ => {}
                }
            }
        });
        Task::run(receiver, |(id, handle)| {
            Message::Call(Event::Share(ShareEvent::Picture(id, handle)))
        })
    })
}
