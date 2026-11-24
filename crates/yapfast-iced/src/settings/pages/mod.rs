//! Settings' pages of switches and choices: My account, Messages,
//! Notifications, Sounds, Advanced, About, Discord and Game Activity; and
//! what their preferences do when changed.

use iced::Element;
use yapfast_core::{
    application::Action,
    i18n::Language,
    sounds::{Sound, SoundSet},
};

use super::{Event, Page, Settings};
use crate::{
    app::Message,
    ctx::{Cx, Pref as Shown, Request},
    form::switch,
    layout::Animate,
};

mod about;
mod account;
pub(super) use account::BLOCKED_STEP;
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
    /// A language, by its place among those Appearance offers.
    Language(usize),
    TwelveHour(bool),
    Seconds(bool),
    DateFormat(usize),
    TextSize(usize),
    Animate(usize),
    LinkPreviews(bool),
    CollapseLong(bool),
    PeekChannels(bool),
    DmSearchSwitcher(bool),
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
    crate::app::settings(Event::Pref(pref))
}

impl Settings {
    pub(in crate::settings) fn pref(&mut self, pref: Pref, cx: &mut Cx<'_>) {
        let prefs = &mut cx.layout.prefs;
        match pref {
            Pref::Language(place) => {
                let Some(language) = Language::offered().get(place).copied() else {
                    return;
                };
                self.open = None;
                prefs.interface_language = Some(language.code().to_owned());
                // Every text, and the order and words of dates and times.
                prefs.apply_language();
                cx.keep("interface_language", language.code());
                cx.request(Request::Changed(Shown::Language));
            }
            Pref::TwelveHour(on) => {
                prefs.twelve_hour_clock = on;
                prefs.apply_clock();
                cx.keep("twelve_hour_clock", on);
                cx.request(Request::Changed(Shown::Clock));
            }
            Pref::Seconds(on) => {
                prefs.clock_seconds = on;
                cx.keep("clock_seconds", on);
                cx.request(Request::Changed(Shown::Clock));
            }
            Pref::DateFormat(index) => {
                prefs.date_format = Some(index as i32);
                prefs.apply_clock();
                cx.keep("date_format", index);
                cx.request(Request::Changed(Shown::Clock));
            }
            Pref::TextSize(index) => {
                prefs.text_size = Some(index as i32);
                cx.keep("text_size", index);
                cx.request(Request::Changed(Shown::Rows));
            }
            Pref::Animate(index) => {
                let animate = ANIMATES[index.min(2)];
                prefs.animate = animate;
                cx.keep("animate", animate);
            }
            Pref::LinkPreviews(on) => {
                prefs.link_previews = Some(on);
                cx.keep("link_previews", on);
                cx.request(Request::Changed(Shown::Rows));
            }
            Pref::CollapseLong(on) => {
                cx.layout.collapse_long = on;
                cx.keep("collapse_long", on);
                cx.request(Request::Changed(Shown::Rows));
            }
            Pref::PeekChannels(on) => {
                cx.layout.peek_channels = on;
                cx.keep("peek_channels", on);
            }
            Pref::DmSearchSwitcher(on) => {
                cx.layout.dm_search_switcher = on;
                cx.keep("dm_search_switcher", on);
            }
            Pref::BoostGems(on) => {
                prefs.boost_gems = Some(on);
                cx.keep("boost_gems", on);
                cx.request(Request::Changed(Shown::Roster));
            }
            Pref::ShareTyping(on) => {
                prefs.share_typing = Some(on);
                cx.client.share_typing = on;
                cx.keep("share_typing", on);
            }
            Pref::ReplyPing(on) => {
                prefs.reply_ping = Some(on);
                cx.keep("reply_ping", on);
                cx.dispatch(Action::ReplyPing(on));
            }
            Pref::StripTracking(on) => {
                prefs.strip_tracking = Some(on);
                cx.keep("strip_tracking", on);
                cx.dispatch(Action::StripTracking(on));
            }
            Pref::Notifications(on) => {
                prefs.notifications = Some(on);
                cx.keep("notifications", on);
            }
            Pref::NotificationText(on) => {
                prefs.notification_text = Some(on);
                cx.keep("notification_text", on);
            }
            Pref::FlashTaskbar(on) => {
                prefs.flash_taskbar = Some(on);
                cx.keep("flash_taskbar", on);
            }
            Pref::SoundSet(index) => {
                let set = SoundSet::ALL[index.min(SoundSet::ALL.len() - 1)];
                cx.layout.voice.sound_set = set;
                cx.keep("sound_set", set);
                try_sound(cx, Sound::ALL[0]);
            }
            Pref::SoundVolume(value, done) => {
                cx.layout.voice.sound_volume = Some(value.min(200) as u8);
                if done {
                    cx.keep("sound_volume", value.min(200));
                    try_sound(cx, Sound::Message);
                }
            }
            Pref::AdjustEach(on) => {
                cx.layout.voice.adjust_each_sound = on;
                cx.keep("adjust_each_sound", on);
            }
            Pref::EachSound(index, value, done) => {
                let Some(&sound) = Sound::ALL.get(index) else {
                    return;
                };
                let each = &mut cx.layout.voice.each_sound;
                if value >= 100 {
                    each.remove(&sound);
                } else {
                    each.insert(sound, value as u8);
                }
                if done {
                    cx.keep("each_sound", &cx.layout.voice.each_sound);
                    try_sound(cx, sound);
                }
            }
            Pref::PlaySound(index) => {
                if let Some(&sound) = Sound::ALL.get(index) {
                    try_sound(cx, sound);
                }
            }
            Pref::GpuRendering(on) => {
                prefs.gpu_rendering = on;
                cx.keep("gpu_rendering", on);
            }
            Pref::GpuEncoding(on) => {
                cx.layout.voice.gpu_encoding = Some(on);
                yapfast_core::stream_worker::set_hardware_encoding(on);
                cx.keep("gpu_encoding", on);
            }
            Pref::CloseToTray(on) => {
                prefs.close_to_tray = Some(on);
                cx.keep("close_to_tray", on);
            }
            Pref::MultipleWindows(on) => {
                prefs.multiple_windows = on;
                cx.keep("multiple_windows", on);
            }
            Pref::DeveloperMode(on) => {
                cx.layout.developer_mode = on;
                cx.keep("developer_mode", on);
            }
            Pref::ShowHiddenChannels(on) => {
                prefs.show_hidden_channels = on;
                cx.keep("show_hidden_channels", on);
                cx.dispatch(Action::ShowHiddenChannels(on));
            }
            Pref::SyncReads(on) => {
                prefs.sync_discord_reads = Some(on);
                cx.keep("sync_discord_reads", on);
                cx.dispatch(Action::SyncDiscordReads(on));
            }
            Pref::ShowIdle(on) => {
                prefs.show_idle = Some(on);
                cx.keep("show_idle", on);
                cx.request(Request::Changed(Shown::Idle));
            }
            Pref::HideBlocked(on) => {
                cx.layout.hide_blocked = on;
                cx.keep("hide_blocked", on);
                cx.dispatch(Action::HideBlocked(on));
            }
            Pref::CheckUpdates(on) => {
                prefs.check_for_updates = on;
                cx.keep("check_for_updates", on);
                if on {
                    cx.request(Request::LookForUpdates(true));
                }
            }
            Pref::Nightlies(on) => {
                prefs.update_nightlies = on;
                cx.keep("update_nightlies", on);
            }
            Pref::StartOnSignIn(on) => {
                match crate::desktop::desktop().set_autostart(on) {
                    Ok(()) => tracing::info!(start = on, "starting at sign-in changed"),
                    Err(error) => tracing::warn!(%error, "could not change starting at sign-in"),
                }
                self.read_platform_choices(cx);
            }
            Pref::DiscordLinks(on) => {
                let root = cx.client.root();
                match crate::desktop::desktop().set_discord_links(root, on) {
                    Ok(()) => tracing::info!(discord_links = on, "Discord link handling changed"),
                    Err(error) => tracing::warn!(%error, "could not change Discord link handling"),
                }
                self.read_platform_choices(cx);
            }
            Pref::SystemTitleBar(on) => {
                cx.layout.system_title_bar = Some(on);
                cx.keep("system_title_bar", on);
                cx.request(Request::TitleBar(on));
            }
        }
        cx.touch();
    }
}

/// One of Yapfast's sounds, to try a setting, on the call's speakers.
fn try_sound(cx: &Cx<'_>, sound: Sound) {
    let voice = &cx.state().voice;
    yapfast_core::sounds::play(
        cx.layout.voice.sound_set,
        sound,
        cx.layout.voice.sound_gain(sound),
        voice.speaker.as_deref(),
    );
}

impl<'a> Page<'a> {
    /// A switch that flips a preference.
    fn flip(
        self,
        label: &'a str,
        description: &'a str,
        on: bool,
        make: fn(bool) -> Pref,
    ) -> Element<'a, Message> {
        switch(label, on, Some(pref(make(!on))))
            .description(description)
            .view(self.cx.look)
    }
}

fn account_message(event: AccountEvent) -> Message {
    crate::app::settings(Event::Account(event))
}

/// Messages' text at a size: 14 px small, 15 default, 18 large.
pub fn message_size(text_size: Option<i32>) -> f32 {
    match text_size {
        Some(0) => 14.0,
        Some(2) => 18.0,
        _ => 15.0,
    }
}
