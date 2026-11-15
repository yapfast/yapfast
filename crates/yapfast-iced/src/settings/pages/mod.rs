//! Settings' pages of switches and choices: My account, Messages,
//! Notifications, Sounds, Advanced, About, Discord and Game Activity, in the
//! Slint client's wording; and what their preferences do when changed.

use iced::{Element, Task};
use yapfast_core::{
    application::Action,
    sounds::{Sound, SoundSet},
};

use super::Event;
use crate::{
    app::{App, Message},
    form::switch,
    layout::Animate,
};

mod about;
mod account;
mod advanced;
mod discord;
mod games;
mod messages;
mod notifications;
mod sounds;

/// The choices of Messages' animations, of the date's order, of the sound
/// set and of the text's size, in the order they show.
const ANIMATES: [Animate; 3] = [Animate::Focused, Animate::Always, Animate::Never];

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
}

fn account_message(event: AccountEvent) -> Message {
    Message::Settings(Event::Account(event))
}

/// Messages' text at a size: 14 px small, 15 default, 18 large.
pub fn message_size(text_size: Option<i32>) -> f32 {
    match text_size {
        Some(0) => 14.0,
        Some(2) => 18.0,
        _ => 15.0,
    }
}
