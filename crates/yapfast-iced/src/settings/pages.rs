//! Settings' pages of switches and choices: My account, Messages,
//! Notifications, Sounds, Advanced, About, Discord and Game Activity, in the
//! Slint client's wording; and what their preferences do when changed.

use iced::{
    Alignment, Element, Length, Task,
    widget::{column, container, row, text},
};
use yapfast_core::{
    application::Action,
    chat::RelationshipChange,
    sounds::{Sound, SoundSet},
};

use super::{Event, Settings, build_tint};
use crate::{
    app::{App, Message},
    controls::{Kind, action, choices, slider},
    fonts::BOLD,
    form::{self, switch},
    layout::Animate,
    people::StatusInk,
    pictures::Icon,
};

/// The choices of Messages' animations, of the date's order, of the sound
/// set and of the text's size, in the order they show.
const ANIMATES: [Animate; 3] = [Animate::Focused, Animate::Always, Animate::Never];
const DATE_FORMATS: [&str; 3] = ["Day/Month/Year", "Month/Day/Year", "Year-Month-Day"];
const SOUND_SETS: [&str; 6] = [
    "New Kalimba",
    "Felt piano",
    "Soft marimba",
    "Mellow Rhodes",
    "Glow",
    "Flute",
];
/// Each of Yapfast's sounds, in `Sound::ALL`'s order.
const SOUND_NAMES: [&str; 12] = [
    "Someone joins the call",
    "Someone leaves the call",
    "Muting",
    "Unmuting",
    "Deafening",
    "Undeafening",
    "A message",
    "A mention",
    "Someone starts watching a stream",
    "Someone stops watching a stream",
    "Someone starts streaming",
    "Someone stops streaming",
];

/// The Discord account's blocked list shows five more at a time.
const BLOCKED_STEP: usize = 5;
/// A custom game's name, at its longest.
const GAME_NAME: usize = 128;

/// A preference changed, by a switch, a choice or a slider (with whether
/// the slider was let go, when it's kept).
#[derive(Clone, Debug)]
pub enum Pref {
    TwelveHour(bool),
    Seconds(bool),
    DateFormat(usize),
    TextSize(usize),
    Animate(usize),
    LinkPreviews(bool),
    CollapseLong(bool),
    BoostGems(bool),
    ShareTyping(bool),
    ReplyPing(bool),
    StripTracking(bool),
    Notifications(bool),
    NotificationText(bool),
    FlashTaskbar(bool),
    SoundSet(usize),
    SoundVolume(u32, bool),
    AdjustEach(bool),
    EachSound(usize, u32, bool),
    PlaySound(usize),
    GpuRendering(bool),
    GpuEncoding(bool),
    CloseToTray(bool),
    MultipleWindows(bool),
    DeveloperMode(bool),
    ShowHiddenChannels(bool),
    SyncReads(bool),
    ShowIdle(bool),
    HideBlocked(bool),
    CheckUpdates(bool),
    Nightlies(bool),
    StartOnSignIn(bool),
    DiscordLinks(bool),
    SystemTitleBar(bool),
}

/// My account's and the Discord page's buttons.
#[derive(Clone, Debug)]
pub enum AccountEvent {
    Reconnect,
    CustomStatus,
    SignOut,
    Unlink,
    Unblock(String),
    MoreBlocked,
    Open(&'static str),
}

/// Game Activity's draft: the executable and name being written, the game
/// being edited (by its place), and whether the last save was refused.
#[derive(Default)]
pub struct GameDraft {
    pub executable: String,
    pub name: String,
    pub editing: Option<usize>,
    pub invalid: bool,
}

#[derive(Clone, Debug)]
pub enum GameEvent {
    Show(bool),
    Executable(String),
    Name(String),
    Edit(usize),
    Remove(usize),
    Save,
    Cancel,
}

fn pref(pref: Pref) -> Message {
    Message::Settings(Event::Pref(pref))
}

impl App {
    pub(super) fn pref(&mut self, pref: Pref) -> Task<Message> {
        let prefs = &mut self.layout.prefs;
        match pref {
            Pref::TwelveHour(on) => {
                prefs.twelve_hour_clock = on;
                prefs.apply_clock();
                self.keep("twelve_hour_clock", on);
                return self.clock_changed();
            }
            Pref::Seconds(on) => {
                prefs.clock_seconds = on;
                self.keep("clock_seconds", on);
                return self.clock_changed();
            }
            Pref::DateFormat(index) => {
                prefs.date_format = Some(index as i32);
                prefs.apply_clock();
                self.keep("date_format", index);
                return self.clock_changed();
            }
            Pref::TextSize(index) => {
                prefs.text_size = Some(index as i32);
                self.keep("text_size", index);
                self.relook();
            }
            Pref::Animate(index) => {
                let animate = ANIMATES[index.min(2)];
                prefs.animate = animate;
                self.keep("animate", animate);
            }
            Pref::LinkPreviews(on) => {
                prefs.link_previews = Some(on);
                self.keep("link_previews", on);
                self.relook();
            }
            Pref::CollapseLong(on) => {
                self.layout.collapse_long = on;
                self.keep("collapse_long", on);
                self.relook();
            }
            Pref::BoostGems(on) => {
                prefs.boost_gems = Some(on);
                self.keep("boost_gems", on);
                self.show_roster(false);
            }
            Pref::ShareTyping(on) => {
                prefs.share_typing = Some(on);
                self.client.share_typing = on;
                self.keep("share_typing", on);
            }
            Pref::ReplyPing(on) => {
                prefs.reply_ping = Some(on);
                self.keep("reply_ping", on);
                self.client.dispatch(Action::ReplyPing(on));
            }
            Pref::StripTracking(on) => {
                prefs.strip_tracking = Some(on);
                self.keep("strip_tracking", on);
                self.client.dispatch(Action::StripTracking(on));
            }
            Pref::Notifications(on) => {
                prefs.notifications = Some(on);
                self.keep("notifications", on);
            }
            Pref::NotificationText(on) => {
                prefs.notification_text = Some(on);
                self.keep("notification_text", on);
            }
            Pref::FlashTaskbar(on) => {
                prefs.flash_taskbar = Some(on);
                self.keep("flash_taskbar", on);
            }
            Pref::SoundSet(index) => {
                let set = SoundSet::ALL[index.min(SoundSet::ALL.len() - 1)];
                self.layout.voice.sound_set = set;
                self.keep("sound_set", set);
                self.try_sound(Sound::ALL[0]);
            }
            Pref::SoundVolume(value, done) => {
                self.layout.voice.sound_volume = Some(value.min(200) as u8);
                if done {
                    self.keep("sound_volume", value.min(200));
                    self.try_sound(Sound::Message);
                }
            }
            Pref::AdjustEach(on) => {
                self.layout.voice.adjust_each_sound = on;
                self.keep("adjust_each_sound", on);
            }
            Pref::EachSound(index, value, done) => {
                let Some(&sound) = Sound::ALL.get(index) else {
                    return Task::none();
                };
                let each = &mut self.layout.voice.each_sound;
                if value >= 100 {
                    each.remove(&sound);
                } else {
                    each.insert(sound, value as u8);
                }
                if done {
                    self.keep("each_sound", &self.layout.voice.each_sound);
                    self.try_sound(sound);
                }
            }
            Pref::PlaySound(index) => {
                if let Some(&sound) = Sound::ALL.get(index) {
                    self.try_sound(sound);
                }
            }
            Pref::GpuRendering(on) => {
                prefs.gpu_rendering = on;
                self.keep("gpu_rendering", on);
            }
            Pref::GpuEncoding(on) => {
                self.layout.voice.gpu_encoding = Some(on);
                yapfast_core::stream_worker::set_hardware_encoding(on);
                self.keep("gpu_encoding", on);
            }
            Pref::CloseToTray(on) => {
                prefs.close_to_tray = Some(on);
                self.keep("close_to_tray", on);
            }
            Pref::MultipleWindows(on) => {
                prefs.multiple_windows = on;
                self.keep("multiple_windows", on);
            }
            Pref::DeveloperMode(on) => {
                self.layout.developer_mode = on;
                self.keep("developer_mode", on);
            }
            Pref::ShowHiddenChannels(on) => {
                prefs.show_hidden_channels = on;
                self.keep("show_hidden_channels", on);
                self.client.dispatch(Action::ShowHiddenChannels(on));
            }
            Pref::SyncReads(on) => {
                prefs.sync_discord_reads = Some(on);
                self.keep("sync_discord_reads", on);
                self.client.dispatch(Action::SyncDiscordReads(on));
            }
            Pref::ShowIdle(on) => {
                prefs.show_idle = Some(on);
                self.keep("show_idle", on);
                self.idle_setting_changed();
            }
            Pref::HideBlocked(on) => {
                self.layout.hide_blocked = on;
                self.keep("hide_blocked", on);
                self.client.dispatch(Action::HideBlocked(on));
            }
            Pref::CheckUpdates(on) => {
                prefs.check_for_updates = on;
                self.keep("check_for_updates", on);
                if on {
                    self.look_for_updates(true);
                }
            }
            Pref::Nightlies(on) => {
                prefs.update_nightlies = on;
                self.keep("update_nightlies", on);
            }
            Pref::StartOnSignIn(on) => {
                match crate::desktop::desktop().set_autostart(on) {
                    Ok(()) => tracing::info!(start = on, "starting at sign-in changed"),
                    Err(error) => tracing::warn!(%error, "could not change starting at sign-in"),
                }
                self.read_platform_choices();
            }
            Pref::DiscordLinks(on) => {
                let root = self.client.root();
                match crate::desktop::desktop().set_discord_links(root, on) {
                    Ok(()) => tracing::info!(discord_links = on, "Discord link handling changed"),
                    Err(error) => tracing::warn!(%error, "could not change Discord link handling"),
                }
                self.read_platform_choices();
            }
            Pref::SystemTitleBar(on) => {
                self.layout.system_title_bar = Some(on);
                self.keep("system_title_bar", on);
                return self.choose_title_bar(on);
            }
        }
        self.sync()
    }

    /// One of Yapfast's sounds, to try a setting, on the call's speakers.
    fn try_sound(&self, sound: Sound) {
        let voice = &self.client.state().voice;
        yapfast_core::sounds::play(
            self.layout.voice.sound_set,
            sound,
            self.layout.voice.sound_gain(sound),
            voice.speaker.as_deref(),
        );
    }

    /// Message times read on another clock: the timeline's rows are worked
    /// out again.
    fn clock_changed(&mut self) -> Task<Message> {
        self.relook();
        self.sync()
    }

    /// Works every row of the timeline out again, as a setting it shows
    /// changed.
    fn relook(&mut self) {
        let none = crate::app::Messages::default();
        self.measure(&none, true);
    }

    pub(super) fn account_settings_event(&mut self, event: AccountEvent) -> Task<Message> {
        let action = match event {
            AccountEvent::Reconnect => Action::RetrySync,
            AccountEvent::CustomStatus => {
                let task = self.close_settings();
                self.client.dispatch(Action::OpenStatusEditor);
                return Task::batch([task, self.sync()]);
            }
            AccountEvent::SignOut => {
                let task = self.close_settings();
                self.client.dispatch(Action::Logout);
                return Task::batch([task, self.sync()]);
            }
            AccountEvent::Unlink => Action::UnlinkDiscord,
            AccountEvent::Unblock(user) => Action::ChangeRelationship {
                user,
                change: RelationshipChange::Unblock,
                location: "ContextMenu",
            },
            AccountEvent::MoreBlocked => {
                if let Some(settings) = &mut self.settings {
                    settings.blocked_shown += BLOCKED_STEP;
                }
                return Task::none();
            }
            AccountEvent::Open(page) => {
                crate::browser::open(page);
                return Task::none();
            }
        };
        self.client.dispatch(action);
        self.sync()
    }

    pub(super) fn game_event(&mut self, event: GameEvent) -> Task<Message> {
        let Some(settings) = &mut self.settings else {
            return Task::none();
        };
        let draft = &mut settings.game;
        match event {
            GameEvent::Show(on) => {
                self.client.dispatch(Action::ShowGame(on));
                return self.sync();
            }
            GameEvent::Executable(text) => draft.executable = text,
            GameEvent::Name(text) => draft.name = text,
            GameEvent::Edit(index) => {
                if let Some((executable, name)) = self.layout.prefs.game_executables.get(index) {
                    *draft = GameDraft {
                        executable: executable.clone(),
                        name: name.clone(),
                        editing: Some(index),
                        invalid: false,
                    };
                }
            }
            GameEvent::Remove(index) => {
                *draft = GameDraft::default();
                let games = &mut self.layout.prefs.game_executables;
                if index < games.len() {
                    games.remove(index);
                    self.games_changed();
                }
            }
            GameEvent::Cancel => *draft = GameDraft::default(),
            GameEvent::Save => {
                let editing = draft.editing;
                let saved = save_game(
                    &mut self.layout.prefs.game_executables,
                    editing,
                    &draft.executable,
                    &draft.name,
                );
                if saved {
                    *draft = GameDraft::default();
                    self.games_changed();
                } else {
                    draft.invalid = true;
                }
            }
        }
        Task::none()
    }

    /// Keeps the custom games, and detection takes them at once.
    fn games_changed(&self) {
        let games = &self.layout.prefs.game_executables;
        self.keep("game_executables", games);
        yapfast_core::discord::set_custom_games(games);
    }

    pub(super) fn account_page(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let account = &self.account;
        let linked = self.discord_linked();
        let card = form::card(
            theme,
            row![
                self.face(crate::pictures::Face {
                    url: account.avatar.as_deref(),
                    name: &account.name,
                    key: None,
                    size: 64.0,
                    opacity: 1.0,
                    decoration: state.account_decoration.as_deref(),
                    play: false,
                    dot: None,
                }),
                column![
                    text(account.name.as_str())
                        .size(20)
                        .font(BOLD)
                        .color(theme.text)
                        .wrapping(text::Wrapping::None),
                    self.status_line(&account.status, StatusInk::under_name(&self.theme, 1.0))
                ]
                .spacing(2)
                .width(Length::Fill)
            ]
            .spacing(16)
            .align_y(Alignment::Center),
        );
        let mut parts = vec![form::title(theme, "My account"), card];
        if let Some(storage) = state.storage {
            parts.push(form::note(theme, storage));
        }
        let mut buttons = vec![];
        let ink = crate::theme::rgb(theme.text);
        if !state.preview && !state.online {
            buttons.push(
                action(
                    theme,
                    "Reconnect",
                    Kind::Plain,
                    (!state.busy).then_some(account_message(AccountEvent::Reconnect)),
                )
                .icon(self.icon(Icon::Refresh, 18, ink))
                .into(),
            );
        }
        if linked {
            buttons.push(
                action(
                    theme,
                    "Edit profile",
                    Kind::Plain,
                    Some(Message::Settings(Event::EditProfile)),
                )
                .icon(self.icon(Icon::UserPen, 18, ink))
                .into(),
            );
            buttons.push(
                action(
                    theme,
                    if state.custom_status.is_some() {
                        "Change custom status"
                    } else {
                        "Set a custom status"
                    },
                    Kind::Plain,
                    Some(account_message(AccountEvent::CustomStatus)),
                )
                .icon(self.icon(Icon::SmilePlus, 18, ink))
                .into(),
            );
        }
        let sign_out = if state.preview {
            action(
                theme,
                "Back to sign in",
                Kind::Plain,
                Some(account_message(AccountEvent::SignOut)),
            )
            .icon(self.icon(Icon::LogOut, 18, ink))
        } else {
            action(
                theme,
                "Sign out",
                Kind::Danger,
                (!state.busy && state.sending.is_none())
                    .then_some(account_message(AccountEvent::SignOut)),
            )
            .icon(self.icon(Icon::LogOut, 18, crate::theme::rgb(theme.on_danger)))
        };
        buttons.push(sign_out.into());
        parts.push(form::buttons(buttons));
        form::page(parts)
    }

    pub(super) fn messages_page(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let prefs = &self.layout.prefs;
        let size = message_size(prefs.text_size);
        let twelve = prefs.twelve_hour_clock;
        let clock = format!(
            "{}{}{}",
            if twelve { "9:41" } else { "21:41" },
            if prefs.clock_seconds { ":05" } else { "" },
            if twelve { " PM" } else { "" }
        );
        let edge = theme.line;
        let preview = container(
            row![
                container(self.avatar(None, "Sam", 40.0, 20.0, 1.0)).align_top(Length::Shrink),
                column![
                    row![
                        text("Sam").size(size).font(BOLD).color(theme.text),
                        text(clock).size(size - 3.0).color(theme.muted)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text("Thinking co-op after nine, if people are up for it.")
                        .size(size)
                        .color(theme.text)
                ]
                .spacing(3)
                .width(Length::Fill)
            ]
            .spacing(16),
        )
        .padding(16)
        .width(Length::Fill)
        .style(move |_| iced::widget::container::Style {
            border: iced::Border {
                color: edge,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
        let text_size = prefs.text_size.unwrap_or(1).clamp(0, 2) as usize;
        let date = prefs
            .date_format
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or(usize::from(twelve));
        let animate = ANIMATES
            .iter()
            .position(|kind| *kind == prefs.animate)
            .unwrap_or(0);
        let networks = if cfg!(feature = "matrix") && self.discord_linked() {
            "Discord and Matrix show"
        } else if cfg!(feature = "matrix") {
            "Matrix shows"
        } else {
            "Discord shows"
        };
        form::page([
            form::title(theme, "Messages"),
            preview.into(),
            form::group([
                form::heading(theme, "TEXT SIZE"),
                choices(theme, &["Small", "Default", "Large"], text_size, |index| {
                    pref(Pref::TextSize(index))
                }),
            ]),
            form::group([
                form::heading(theme, "CLOCK"),
                choices(theme, &["24-hour", "12-hour"], usize::from(twelve), |index| {
                    pref(Pref::TwelveHour(index == 1))
                }),
                switch(
                    "Show seconds",
                    prefs.clock_seconds,
                    Some(pref(Pref::Seconds(!prefs.clock_seconds))),
                )
                .description("Message times show them, as 21:41:05.")
                .view(self),
                form::heading(theme, "DATE FORMAT"),
                choices(theme, &DATE_FORMATS, date, |index| {
                    pref(Pref::DateFormat(index))
                }),
            ]),
            form::group([
                form::heading(theme, "PLAY GIFS, EMOJI AND STICKERS"),
                choices(
                    theme,
                    &["While Yapfast is focused", "Always", "Never"],
                    animate,
                    |index| pref(Pref::Animate(index)),
                ),
            ]),
            form::rule(theme),
            self.flip(
                "Show link previews",
                "Cards for links, videos and bots under messages. Linked pictures still show.",
                prefs.link_previews.unwrap_or(true),
                Pref::LinkPreviews,
            ),
            self.flip(
                "Collapse long messages",
                "Long messages and code show their first lines, with Show more to see the rest.",
                self.layout.collapse_long,
                Pref::CollapseLong,
            ),
            self.flip(
                "Show boost gems",
                "A gem by the name of each member boosting their Discord server, in the member list.",
                prefs.boost_gems.unwrap_or(true),
                Pref::BoostGems,
            ),
            switch(
                "Let others see when you're typing",
                prefs.share_typing.unwrap_or(true),
                Some(pref(Pref::ShareTyping(!prefs.share_typing.unwrap_or(true)))),
            )
            .description(format!(
                "{networks} \"typing...\" while you write. Others still show here when they type."
            ))
            .view(self),
            self.flip(
                "Mention people you reply to",
                "They get notified of your reply. Switch it for one reply with @ On on the reply bar.",
                prefs.reply_ping.unwrap_or(true),
                Pref::ReplyPing,
            ),
            self.flip(
                "Remove tracking from links you send",
                "Takes out tags like utm_source and fbclid, and share ids from YouTube, Spotify and X. The links still go to the same page.",
                prefs.strip_tracking.unwrap_or(true),
                Pref::StripTracking,
            ),
        ])
    }

    /// A switch that flips a preference.
    fn flip<'a>(
        &'a self,
        label: &'a str,
        description: &'a str,
        on: bool,
        make: fn(bool) -> Pref,
    ) -> Element<'a, Message> {
        switch(label, on, Some(pref(make(!on))))
            .description(description)
            .view(self)
    }

    pub(super) fn notifications_page(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let prefs = &self.layout.prefs;
        let on = prefs.notifications.unwrap_or(true);
        let rules = if cfg!(feature = "matrix") && self.discord_linked() {
            "Each Discord server's notification settings and your Matrix notification rules decide"
        } else if cfg!(feature = "matrix") {
            "Your Matrix notification rules decide"
        } else {
            "Each Discord server's notification settings decide"
        };
        let words = prefs.notification_text.unwrap_or(true);
        form::page([
            form::title(theme, "Notifications"),
            switch(
                "Show desktop notifications",
                on,
                Some(pref(Pref::Notifications(!on))),
            )
            .description(format!(
                "For new messages while Yapfast isn't in front. {rules} which: usually DMs and mentions. Clicking one opens the conversation."
            ))
            .view(self),
            switch(
                "Show what messages say",
                words,
                on.then(|| pref(Pref::NotificationText(!words))),
            )
            .description("Off shows only who wrote and where, for a screen others can see.")
            .view(self),
            self.flip(
                "Flash the taskbar",
                "For a message that notifies while Yapfast isn't in front. The window's title counts mentions and DMs waiting either way.",
                prefs.flash_taskbar.unwrap_or(true),
                Pref::FlashTaskbar,
            ),
        ])
    }

    pub(super) fn sounds_page(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let voice = &self.layout.voice;
        let set = SoundSet::ALL
            .iter()
            .position(|set| *set == voice.sound_set)
            .unwrap_or(0);
        let volume = u32::from(voice.sound_volume.unwrap_or(100));
        let mut parts = vec![
            form::title(theme, "Sounds"),
            form::note(
                theme,
                "Yapfast plays these as you and others join and leave a call, as you mute and deafen, as others start and stop streaming, as they start and stop watching your stream or the one you watch, and for messages that notify, except in the conversation in front of you. They play on the speakers chosen for calls.",
            ),
            form::group([
                form::heading(theme, "SOUND SET"),
                crate::controls::choices_within(theme, &SOUND_SETS, set, 520.0, |index| {
                    pref(Pref::SoundSet(index))
                }),
            ]),
            slider("Volume", volume, 200, |value| {
                pref(Pref::SoundVolume(value, false))
            })
            .on_release(pref(Pref::SoundVolume(volume, true)))
            .view(theme),
            self.flip(
                "Adjust each sound",
                "Set how loud each one is, as a share of the volume above. 0% turns a sound off.",
                voice.adjust_each_sound,
                Pref::AdjustEach,
            ),
        ];
        if voice.adjust_each_sound {
            let mut each = column![].spacing(12);
            for (index, (name, sound)) in SOUND_NAMES.iter().zip(Sound::ALL).enumerate() {
                let value = u32::from(voice.each_sound.get(&sound).copied().unwrap_or(100));
                each = each.push(
                    row![
                        container(
                            slider(*name, value, 100, move |value| {
                                pref(Pref::EachSound(index, value, false))
                            })
                            .on_release(pref(Pref::EachSound(index, value, true)))
                            .view(theme)
                        )
                        .width(Length::Fill),
                        self.icon_button(
                            Icon::Play,
                            "Play",
                            32.0,
                            18,
                            pref(Pref::PlaySound(index)),
                            false
                        )
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                );
            }
            parts.push(each.into());
        }
        form::page(parts)
    }

    pub(super) fn advanced_page(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let prefs = &self.layout.prefs;
        let mut parts = vec![form::title(theme, "Advanced")];
        let platform = self
            .settings
            .as_ref()
            .map(|settings| settings.platform)
            .unwrap_or_default();
        if let Some(start) = platform.autostart {
            parts.push(self.flip(
                "Start when you sign in",
                "Yapfast starts in the system tray when you sign in to your computer, so messages and calls reach you.",
                start,
                Pref::StartOnSignIn,
            ));
        }
        if let Some(links) = platform.discord_links {
            parts.push(self.flip(
                "Open Discord links in Yapfast",
                "discord:// links from other apps and shortcuts open here instead of Discord. Turning it off gives them back to the app that had them.",
                links,
                Pref::DiscordLinks,
            ));
        }
        let tray = self
            .shell
            .tray
            .as_ref()
            .is_some_and(|tray| tray.available());
        parts.extend([
            self.flip(
                "Close to tray",
                if tray {
                    "Closing the window keeps Yapfast running in the system tray, with any call. Quit from the tray icon's menu."
                } else {
                    "No system tray is running, so closing the window quits Yapfast."
                },
                prefs.close_to_tray.unwrap_or(true),
                Pref::CloseToTray,
            ),
            self.flip(
                "Allow multiple windows",
                "Starting Yapfast while it runs opens another Yapfast, with its own window and tray icon. Off, it brings the open window forward.",
                prefs.multiple_windows,
                Pref::MultipleWindows,
            ),
            self.flip(
                "Render with the GPU",
                "Hardware acceleration. Off, Yapfast draws on the processor, which uses about half the memory.",
                prefs.gpu_rendering,
                Pref::GpuRendering,
            ),
            self.renderer_note(),
            self.flip(
                "Encode streams with the GPU",
                "Screen sharing uses the graphics card's video encoder, which takes almost no processor time and looks better at low bitrates. Off, or without one, Yapfast encodes on the processor.",
                self.layout.voice.gpu_encoding.unwrap_or(true),
                Pref::GpuEncoding,
            ),
            self.flip(
                "Developer Mode",
                "Menus offer Copy ID for servers, channels and people, which helps when you write apps that use the Discord API.",
                self.layout.developer_mode,
                Pref::DeveloperMode,
            ),
            text("Icons from Solar by 480 Design (CC BY 4.0), many of them adapted, and from Lucide (ISC).")
                .size(12)
                .color(theme.faint)
                .into(),
        ]);
        form::page(parts)
    }

    /// What the renderer setting means for this build, which draws with the
    /// one renderer it was built with: the setting is kept for builds that
    /// can follow it.
    fn renderer_note(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let gpu = cfg!(feature = "gpu");
        let words = if self.layout.prefs.gpu_rendering == gpu {
            if gpu {
                "This build draws with the GPU (wgpu)."
            } else {
                "This build draws on the processor (tiny-skia)."
            }
        } else if gpu {
            "This build draws with the GPU (wgpu) whatever the setting says: it was built with --features iced-gpu."
        } else {
            "This build draws on the processor (tiny-skia) whatever the setting says: it was built with --features iced."
        };
        form::note(theme, words)
    }

    pub(super) fn about_page<'a>(&'a self, settings: &'a Settings) -> Element<'a, Message> {
        let theme = &self.theme;
        let build = &self.build;
        let prefs = &self.layout.prefs;
        let mut channel = row![
            text(build.channel.as_str())
                .size(13)
                .font(BOLD)
                .color(build_tint(theme, build.tone))
        ];
        if !build.details.is_empty() {
            channel = channel.push(
                text(format!(" · {}", build.details))
                    .size(13)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            );
        }
        let card = form::card(
            theme,
            form::beside(
                column![
                    text(build.headline.as_str())
                        .size(20)
                        .font(BOLD)
                        .color(theme.text)
                        .wrapping(text::Wrapping::None),
                    channel
                ]
                .spacing(4),
                action(
                    theme,
                    if settings.copied {
                        "Copied"
                    } else {
                        "Copy version"
                    },
                    Kind::Plain,
                    Some(Message::Settings(Event::CopyVersion)),
                ),
            ),
        );
        let mut parts = vec![form::title(theme, "About"), card];
        parts.extend(self.updates_part());
        parts.push(
            switch(
                "Check for updates",
                prefs.check_for_updates,
                Some(pref(Pref::CheckUpdates(!prefs.check_for_updates))),
            )
            .description("Checks for new versions every 6 hours.")
            .fresh(self.fresh("check-updates"))
            .view(self),
        );
        parts.push(
            switch(
                "Include nightly builds",
                prefs.update_nightlies,
                prefs
                    .check_for_updates
                    .then(|| pref(Pref::Nightlies(!prefs.update_nightlies))),
            )
            .description("Get the newest features before release. Expect some bugs.")
            .fresh(self.fresh("update-nightlies"))
            .view(self),
        );
        parts.push(
            switch("Install updates automatically", false, None)
                .description("Not available yet.")
                .view(self),
        );
        parts.push(form::heading(theme, "LINKS"));
        let ink = crate::theme::rgb(theme.text);
        let link = |label, icon, page| -> Element<'_, Message> {
            action(
                theme,
                label,
                Kind::Plain,
                Some(account_message(AccountEvent::Open(page))),
            )
            .icon(self.icon(icon, 18, ink))
            .into()
        };
        parts.push(form::buttons([
            link("Website", Icon::Logo, WEBSITE),
            link("GitHub", Icon::GitHub, GITHUB),
            link("Discord", Icon::Discord, DISCORD),
        ]));
        form::page(parts)
    }

    pub(super) fn discord_page<'a>(&'a self, settings: &'a Settings) -> Element<'a, Message> {
        let theme = &self.theme;
        if !self.discord_linked() {
            return form::page([
                form::title(theme, "Discord"),
                form::note(
                    theme,
                    "Link your Discord account to see its servers and DMs in Yapfast, and join its calls. The login stays on this computer, in the system's credential store.",
                ),
                self.link_panel(),
            ]);
        }
        let state = self.client.state();
        let prefs = &self.layout.prefs;
        let mut parts = vec![form::title(theme, "Discord")];
        if cfg!(feature = "matrix") {
            parts.push(form::beside(
                form::note(
                    theme,
                    "Unlinking signs Yapfast out of Discord on this computer.",
                ),
                action(
                    theme,
                    "Unlink Discord",
                    Kind::Danger,
                    Some(account_message(AccountEvent::Unlink)),
                )
                .icon(self.icon(
                    Icon::LogOut,
                    18,
                    crate::theme::rgb(theme.on_danger),
                )),
            ));
        }
        parts.push(self.flip(
            "Show channels you can't access",
            "They appear locked. Server admins hid them on purpose.",
            prefs.show_hidden_channels,
            Pref::ShowHiddenChannels,
        ));
        parts.push(self.flip(
            "Mark messages read on Discord",
            "Channels you read in Yapfast show as read in the Discord apps too.",
            prefs.sync_discord_reads.unwrap_or(true),
            Pref::SyncReads,
        ));
        parts.push(self.flip(
            "Show me as idle when I'm away",
            "After 10 minutes without using Yapfast, as in Discord. Either way, your phone gets notifications while you're away.",
            prefs.show_idle.unwrap_or(true),
            Pref::ShowIdle,
        ));
        parts.push(self.flip(
            "Hide messages from people you blocked",
            "Leaves them out entirely, rather than showing a line you can open.",
            self.layout.hide_blocked,
            Pref::HideBlocked,
        ));
        let blocked = &state.discord_blocked;
        if !blocked.is_empty() {
            let raised = theme.raised;
            let count = blocked.len();
            let mut list = column![
                row![
                    self.glyph(Icon::Ban, 20, theme.secondary)
                        .boxed(40)
                        .style(move |_| crate::style::rounded(raised, 20)),
                    column![
                        text("Blocked accounts")
                            .size(16)
                            .font(crate::fonts::SEMIBOLD)
                            .color(theme.text),
                        text(format!(
                            "{count} {}",
                            if count == 1 { "account" } else { "accounts" }
                        ))
                        .size(14)
                        .color(theme.secondary)
                    ]
                    .spacing(2)
                ]
                .spacing(12)
                .padding(iced::Padding {
                    top: 8.0,
                    ..iced::Padding::ZERO
                })
                .align_y(Alignment::Center)
            ]
            .spacing(8);
            let mut people = column![];
            for person in blocked.iter().take(settings.blocked_shown) {
                let mut names = column![
                    text(person.name.as_str())
                        .size(16)
                        .font(crate::fonts::SEMIBOLD)
                        .color(theme.text)
                        .wrapping(text::Wrapping::None)
                ]
                .spacing(2);
                if let Some(username) = person.username.as_deref() {
                    names = names.push(
                        text(username)
                            .size(14)
                            .color(theme.secondary)
                            .wrapping(text::Wrapping::None),
                    );
                }
                people = people.push(
                    row![
                        self.avatar(person.avatar.as_deref(), &person.name, 40.0, 20.0, 1.0),
                        container(names).width(Length::Fill).clip(true),
                        action(
                            theme,
                            "Unblock",
                            Kind::Plain,
                            Some(account_message(AccountEvent::Unblock(person.id.clone()))),
                        )
                        .small()
                    ]
                    .spacing(12)
                    .padding([6, 0])
                    .align_y(Alignment::Center),
                );
            }
            list = list.push(people);
            if count > settings.blocked_shown {
                let more = format!("See {} more", count - settings.blocked_shown);
                list = list.push(Element::from(
                    action(
                        theme,
                        more,
                        Kind::Ghost,
                        Some(account_message(AccountEvent::MoreBlocked)),
                    )
                    .small(),
                ));
            }
            parts.push(list.into());
        }
        form::page(parts)
    }

    pub(super) fn games_page<'a>(&'a self, settings: &'a Settings) -> Element<'a, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let linked = self.discord_linked();
        let draft = &settings.game;
        let shown = state.discord_show_game;
        let mut parts = vec![
            form::title(theme, "Game activity"),
            switch(
                "Share the game you play",
                shown,
                linked.then(|| Message::Settings(Event::Game(GameEvent::Show(!shown)))),
            )
            .description("Yapfast looks for games among the programs running, and on Linux takes the Rich Presence games send. Your Discord account keeps this setting, so it's the same in Discord's apps.")
            .locked("Link a Discord account first.")
            .view(self),
            form::heading(theme, "CUSTOM GAMES"),
            form::note(
                theme,
                "Name a game by its executable, a file name or a full path, and what to show after Playing. These come before the games Yapfast knows, but a game's Rich Presence comes first.",
            ),
        ];
        for (index, (executable, name)) in self.layout.prefs.game_executables.iter().enumerate() {
            parts.push(
                column![
                    row![
                        text(name.as_str())
                            .size(15)
                            .color(theme.text)
                            .width(Length::Fill)
                            .wrapping(text::Wrapping::None),
                        action(
                            theme,
                            "Edit",
                            Kind::Plain,
                            Some(game_message(GameEvent::Edit(index)))
                        )
                        .small(),
                        action(
                            theme,
                            "Remove",
                            Kind::Plain,
                            Some(game_message(GameEvent::Remove(index)))
                        )
                        .small()
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(executable.as_str()).size(12).color(theme.muted)
                ]
                .spacing(4)
                .into(),
            );
        }
        parts.push(
            form::field(
                theme,
                "settings-game-executable",
                "Executable name or full path",
                &draft.executable,
            )
            .on_input(|text| game_message(GameEvent::Executable(text)))
            .into(),
        );
        parts.push(
            form::field(
                theme,
                "settings-game-name",
                "Text after Playing (up to 128 characters)",
                &draft.name,
            )
            .on_input(|text| game_message(GameEvent::Name(text)))
            .into(),
        );
        if draft.invalid {
            parts.push(form::note(
                theme,
                "Enter an executable and a name of up to 128 characters, without line breaks.",
            ));
        }
        let ready = !draft.executable.is_empty() && !draft.name.is_empty();
        let mut buttons = vec![
            action(
                theme,
                if draft.editing.is_some() {
                    "Save game"
                } else {
                    "Add game"
                },
                Kind::Plain,
                ready.then(|| game_message(GameEvent::Save)),
            )
            .into(),
        ];
        if draft.editing.is_some() {
            buttons.push(
                action(
                    theme,
                    "Cancel",
                    Kind::Plain,
                    Some(game_message(GameEvent::Cancel)),
                )
                .into(),
            );
        }
        parts.push(form::buttons(buttons));
        form::page(parts)
    }
}

fn account_message(event: AccountEvent) -> Message {
    Message::Settings(Event::Account(event))
}

fn game_message(event: GameEvent) -> Message {
    Message::Settings(Event::Game(event))
}

/// Messages' text at a size: 14 px small, 15 default, 18 large.
pub fn message_size(text_size: Option<i32>) -> f32 {
    match text_size {
        Some(0) => 14.0,
        Some(2) => 18.0,
        _ => 15.0,
    }
}

/// Saves a game as the Slint client's Game Activity does: both parts
/// needed, without line breaks, the name at most 128 characters; another
/// entry for the same executable gives way to this one. False when it
/// can't be saved.
pub fn save_game(
    games: &mut Vec<(String, String)>,
    editing: Option<usize>,
    executable: &str,
    name: &str,
) -> bool {
    let (executable, name) = (executable.trim(), name.trim());
    if executable.is_empty()
        || name.is_empty()
        || executable.len() > 4096
        || name.chars().count() > GAME_NAME
        || executable.chars().any(char::is_control)
        || name.chars().any(char::is_control)
    {
        return false;
    }
    let editing = editing.filter(|&index| index < games.len());
    let same = games
        .iter()
        .position(|(other, _)| other == executable)
        .filter(|&same| Some(same) != editing);
    let entry = (executable.to_owned(), name.to_owned());
    match (editing, same) {
        (Some(index), same) => {
            games[index] = entry;
            if let Some(same) = same {
                games.remove(same);
            }
        }
        (None, Some(same)) => games[same] = entry,
        (None, None) => games.push(entry),
    }
    true
}

/// About's links.
const WEBSITE: &str = "https://yapfast.com";
const GITHUB: &str = "https://github.com/yapfast/yapfast";
const DISCORD: &str = "https://yapfast.com/discord-invite";
