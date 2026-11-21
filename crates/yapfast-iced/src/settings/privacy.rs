//! Privacy and accessibility pages reuse the preferences' existing owners.
//! Every control here changes something Yapfast does or the Discord setting
//! Yapfast can write; nothing stands in for a Discord privacy permission.

use iced::Element;
use yapfast_core::i18n::tr;

use super::{
    Event as SettingsEvent, Page,
    appearance::{Cosmetic, Event as ThemeEvent},
    pages::{GameEvent, Pref},
};
use crate::{
    app::Message,
    controls::{choices, slider},
    form::{self, switch},
    layout::Animate,
};

fn pref(pref: Pref) -> Message {
    Message::Settings(SettingsEvent::Pref(pref))
}

fn can_use_discord(state: &yapfast_core::application::AppState) -> bool {
    !state.preview && state.discord_user.is_some() && super::discord_linked(state)
}

impl<'a> Page<'a> {
    /// A Yapfast settings card, using the person's theme and native controls.
    fn privacy_card(
        self,
        title: &'static str,
        parts: impl IntoIterator<Item = Element<'a, Message>>,
    ) -> Element<'a, Message> {
        form::card(
            self.cx.look.theme,
            form::group(std::iter::once(form::heading(self.cx.look.theme, title)).chain(parts)),
        )
    }

    fn privacy_flip(
        self,
        label: &'static str,
        description: &'static str,
        on: bool,
        make: fn(bool) -> Pref,
    ) -> Element<'a, Message> {
        switch(label, on, Some(pref(make(!on))))
            .description(description)
            .view(self.cx.look)
    }

    pub(super) fn content_social_page(self) -> Element<'a, Message> {
        let prefs = &self.cx.layout.prefs;
        let mut controls = vec![
            self.privacy_flip(
                tr("Show link previews"),
                tr("Show cards for links, videos and bots in Yapfast. Linked pictures still show. This does not filter sensitive media."),
                prefs.link_previews.unwrap_or(true),
                Pref::LinkPreviews,
            ),
            self.privacy_flip(
                tr("Mention people you reply to"),
                tr("Notify the person you reply to by default. Each reply can override this."),
                prefs.reply_ping.unwrap_or(true),
                Pref::ReplyPing,
            ),
        ];
        if can_use_discord(self.cx.state) {
            controls.push(self.privacy_flip(
                tr("Hide messages from people you blocked"),
                tr("Leave their messages out of Yapfast's timeline. Your Discord block list is unchanged."),
                self.cx.layout.hide_blocked,
                Pref::HideBlocked,
            ));
        }
        form::page([
            form::title(self.cx.look.theme, tr("Content & Social")),
            self.privacy_card(tr("IN YAPFAST"), controls),
        ])
    }

    pub(super) fn data_privacy_page(self) -> Element<'a, Message> {
        let prefs = &self.cx.layout.prefs;
        let mut controls = vec![self.privacy_flip(
            tr("Remove tracking from links you send"),
            tr("Remove known tracking parameters from links sent through Yapfast."),
            prefs.strip_tracking.unwrap_or(true),
            Pref::StripTracking,
        )];
        if can_use_discord(self.cx.state) {
            controls.push(self.privacy_flip(
                tr("Mark messages read on Discord"),
                tr("Send read acknowledgements for channels you read in Yapfast. Turning this off does not erase read state already sent."),
                prefs.sync_discord_reads.unwrap_or(true),
                Pref::SyncReads,
            ));
        }
        form::page([
            form::title(self.cx.look.theme, tr("Data & Privacy")),
            self.privacy_card(tr("IN YAPFAST"), controls),
        ])
    }

    pub(super) fn accessibility_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let cosmetics = prefs.cosmetics();
        let cosmetic = |label, description, on: bool, which| {
            switch(
                label,
                on,
                Some(Message::Settings(SettingsEvent::Theme(
                    ThemeEvent::Profile(which, !on),
                ))),
            )
            .description(description)
            .view(self.cx.look)
        };
        let animate = match prefs.animate {
            Animate::Focused => 0,
            Animate::Always => 1,
            Animate::Never => 2,
        };
        form::page([
            form::title(theme, tr("Accessibility")),
            self.privacy_card(
                tr("READING IN YAPFAST"),
                [
                    form::heading(theme, tr("MESSAGE TEXT SIZE")),
                    choices(
                        theme,
                        &[tr("Small"), tr("Default"), tr("Large")],
                        prefs.text_size.unwrap_or(1).clamp(0, 2) as usize,
                        |index| pref(Pref::TextSize(index)),
                    ),
                    self.privacy_flip(
                        tr("Collapse long messages"),
                        tr("Fold long messages and code, with Show more to expand them."),
                        self.cx.layout.collapse_long,
                        Pref::CollapseLong,
                    ),
                ],
            ),
            self.privacy_card(
                tr("MOTION IN YAPFAST"),
                [
                    form::heading(theme, tr("PLAY GIFS, EMOJI AND STICKERS")),
                    choices(
                        theme,
                        &[tr("While focused"), tr("Always"), tr("Never")],
                        animate,
                        |index| pref(Pref::Animate(index)),
                    ),
                    cosmetic(
                        tr("Play profile effects"),
                        tr("Allow animations over profile cards."),
                        cosmetics.effects,
                        Cosmetic::Effects,
                    ),
                    cosmetic(
                        tr("Animate decorations and nameplates"),
                        tr("Off keeps these cosmetics still, including on hover."),
                        cosmetics.moving,
                        Cosmetic::Moving,
                    ),
                ],
            ),
        ])
    }

    pub(super) fn streamer_mode_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let volume = u32::from(self.cx.layout.voice.sound_volume.unwrap_or(100));
        form::page([
            form::title(theme, tr("Streamer Mode")),
            form::note(theme, tr("Prepare Yapfast for sharing your screen. Names, messages and invite links remain visible; there is no automatic streamer mode.")),
            self.privacy_card(tr("NOTIFICATIONS AND SOUNDS IN YAPFAST"), [
                self.privacy_flip(
                    tr("Show desktop notifications"),
                    tr("Allow Yapfast to show message notifications while it is in the background."),
                    prefs.notifications.unwrap_or(true),
                    Pref::Notifications,
                ),
                self.privacy_flip(
                    tr("Show message text in notifications"),
                    tr("Off hides message text. Sender and conversation names still appear when desktop notifications are enabled."),
                    prefs.notification_text.unwrap_or(true),
                    Pref::NotificationText,
                ),
                self.privacy_flip(
                    tr("Flash the taskbar"),
                    tr("Flash Yapfast for a message that notifies while the window is in the background."),
                    prefs.flash_taskbar.unwrap_or(true),
                    Pref::FlashTaskbar,
                ),
                slider(tr("Yapfast sound volume"), volume, 200, |value| pref(Pref::SoundVolume(value, false)))
                    .on_release(pref(Pref::SoundVolume(volume, true)))
                    .view(theme),
                form::note(theme, tr("0% silences Yapfast's interface sounds. Call audio, streams and the soundboard have separate volume controls.")),
            ]),
        ])
    }

    pub(super) fn activity_privacy_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let mut controls = vec![self.privacy_flip(
            tr("Let others see when you're typing"),
            tr("Send typing indicators while you write in Yapfast. Others' typing indicators still appear."),
            prefs.share_typing.unwrap_or(true),
            Pref::ShareTyping,
        )];
        if can_use_discord(self.cx.state) {
            let shown = self.cx.state.discord_show_game;
            controls.push(
                switch(
                    tr("Share the game you play"),
                    shown,
                    Some(Message::Settings(SettingsEvent::Game(GameEvent::Show(!shown)))),
                )
                .description(tr("Uses the same Discord account setting as Game Activity. Changes apply to Discord too; a failed save restores the previous value."))
                .view(self.cx.look),
            );
            controls.push(self.privacy_flip(
                tr("Show me as idle when I'm away"),
                tr("Let Yapfast report idle presence after inactivity. This does not hide your online presence."),
                prefs.show_idle.unwrap_or(true),
                Pref::ShowIdle,
            ));
        }
        form::page([
            form::title(theme, tr("Activity Privacy")),
            self.privacy_card(tr("SHARING FROM YAPFAST"), controls),
        ])
    }
}
