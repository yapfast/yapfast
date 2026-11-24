//! Privacy and accessibility pages reuse the preferences' existing owners.
//! Discord account controls without native support have explicit browser
//! handoffs. No local switch stands in for a Discord privacy permission.

use iced::Element;
use yapfast_core::application::{Action, DmScreen, Navigation};

use super::{
    Event as SettingsEvent, Page,
    appearance::{Cosmetic, Event as ThemeEvent},
    pages::{GameEvent, Pref},
};
use crate::{
    app::Message,
    controls::{Kind, action, choices, slider},
    ctx::{Cx, Request, Screen},
    form::{self, switch},
    layout::Animate,
};

/// Public Discord destinations, opened only on an explicit button press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handoff {
    Discord,
    ContentSocial,
    DataPrivacy,
    FamilyCenter,
    FriendRequests,
    Accessibility,
    StreamerMode,
    ActivityPrivacy,
}

impl Handoff {
    pub(crate) fn url(self) -> &'static str {
        match self {
            Self::Discord => "https://discord.com/channels/@me",
            Self::ContentSocial | Self::FriendRequests => {
                "https://support.discord.com/hc/en-us/articles/217916488-Blocking-Privacy-Settings"
            }
            Self::DataPrivacy => "https://discord.com/privacy/policy/",
            Self::FamilyCenter => {
                "https://support.discord.com/hc/en-us/articles/14155043715735-Family-Center-for-Parents-and-Guardians"
            }
            Self::Accessibility => {
                "https://support.discord.com/hc/en-us/articles/1500010454681-Accessibility-Settings-Tab"
            }
            Self::StreamerMode => {
                "https://support.discord.com/hc/en-us/articles/218485407-Streamer-Mode-101"
            }
            Self::ActivityPrivacy => {
                "https://support.discord.com/hc/en-us/articles/7931156448919-Activity-Sharing-on-Discord-FAQ"
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Open(Handoff),
    Requests,
}

fn event(event: Event) -> Message {
    crate::app::settings(SettingsEvent::Privacy(event))
}

fn pref(pref: Pref) -> Message {
    crate::app::settings(SettingsEvent::Pref(pref))
}

pub(super) fn update(event: Event, cx: &mut Cx<'_>) {
    match event {
        Event::Open(destination) => crate::browser::open(destination.url()),
        Event::Requests if can_use_discord(cx.state()) => {
            cx.request(Request::Close(Screen::Settings));
            cx.dispatch(Action::Navigate(Navigation::DirectMessages));
            cx.dispatch(Action::OpenDmScreen(DmScreen::Requests));
        }
        Event::Requests => {}
    }
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

    /// Discord has no supported settings-page URL here. Open its web app
    /// and give the exact next step, rather than inventing a deep link.
    fn discord_handoff(self, guide: Handoff, instructions: &'static str) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        self.privacy_card(
            "IN DISCORD",
            [
                form::note(theme, instructions),
                form::buttons([
                    action(
                        theme,
                        "Open Discord in browser",
                        Kind::Plain,
                        Some(event(Event::Open(Handoff::Discord))),
                    )
                    .into(),
                    action(
                        theme,
                        if guide == Handoff::DataPrivacy {
                            "Read Discord's privacy policy"
                        } else {
                            "Read Discord's guide"
                        },
                        Kind::Ghost,
                        Some(event(Event::Open(guide))),
                    )
                    .into(),
                ]),
            ],
        )
    }

    pub(super) fn content_social_page(self) -> Element<'a, Message> {
        let prefs = &self.cx.layout.prefs;
        let mut controls = vec![
            self.privacy_flip(
                "Show link previews",
                "Show cards for links, videos and bots in Yapfast. Linked pictures still show. This does not filter sensitive media.",
                prefs.link_previews.unwrap_or(true),
                Pref::LinkPreviews,
            ),
            self.privacy_flip(
                "Mention people you reply to",
                "Notify the person you reply to by default. Each reply can override this.",
                prefs.reply_ping.unwrap_or(true),
                Pref::ReplyPing,
            ),
        ];
        if can_use_discord(self.cx.state) {
            controls.push(self.privacy_flip(
                "Hide messages from people you blocked",
                "Leave their messages out of Yapfast's timeline. Your Discord block list is unchanged.",
                self.cx.layout.hide_blocked,
                Pref::HideBlocked,
            ));
        }
        form::page([
            form::title(self.cx.look.theme, "Content & Social"),
            self.privacy_card("IN YAPFAST", controls),
            self.discord_handoff(
                Handoff::ContentSocial,
                "For sensitive media, DM spam filters and who can message you, open Discord, then User Settings > Content & Social. Yapfast does not edit these account permissions.",
            ),
        ])
    }

    pub(super) fn data_privacy_page(self) -> Element<'a, Message> {
        let prefs = &self.cx.layout.prefs;
        let mut controls = vec![self.privacy_flip(
            "Remove tracking from links you send",
            "Remove known tracking parameters from links sent through Yapfast.",
            prefs.strip_tracking.unwrap_or(true),
            Pref::StripTracking,
        )];
        if can_use_discord(self.cx.state) {
            controls.push(self.privacy_flip(
                "Mark messages read on Discord",
                "Send read acknowledgements for channels you read in Yapfast. Turning this off does not erase read state already sent.",
                prefs.sync_discord_reads.unwrap_or(true),
                Pref::SyncReads,
            ));
        }
        form::page([
            form::title(self.cx.look.theme, "Data & Privacy"),
            self.privacy_card("IN YAPFAST", controls),
            self.discord_handoff(
                Handoff::DataPrivacy,
                "For Discord data use, personalization, profile privacy or a copy of your Discord data, open Discord, then User Settings > Data & Privacy. These choices are managed by Discord.",
            ),
        ])
    }

    pub(super) fn family_center_page(self) -> Element<'a, Message> {
        form::page([
            form::title(self.cx.look.theme, "Family Center"),
            self.discord_handoff(
                Handoff::FamilyCenter,
                "Family Center setup, connections and activity summaries are available in Discord. Open Discord, then User Settings > Family Center. Yapfast does not display or manage family connections.",
            ),
        ])
    }

    pub(super) fn friend_requests_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut parts = vec![form::title(theme, "Friend Requests")];
        if can_use_discord(self.cx.state) {
            parts.push(
                self.privacy_card(
                    "IN YAPFAST",
                    [
                        form::note(
                            theme,
                            "Review incoming and sent requests on Yapfast's Requests screen.",
                        ),
                        action(
                            theme,
                            "Review requests in Yapfast",
                            Kind::Plain,
                            Some(event(Event::Requests)),
                        )
                        .into(),
                    ],
                ),
            );
        }
        parts.push(self.discord_handoff(
            Handoff::FriendRequests,
            "To choose who can send you friend requests, open Discord, then User Settings > Content & Social > Friend Requests. Yapfast does not edit these permissions.",
        ));
        form::page(parts)
    }

    pub(super) fn accessibility_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let cosmetics = prefs.cosmetics();
        let cosmetic = |label, description, on: bool, which| {
            switch(
                label,
                on,
                Some(crate::app::settings(SettingsEvent::Theme(
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
            form::title(theme, "Accessibility"),
            self.privacy_card("READING IN YAPFAST", [
                form::heading(theme, "MESSAGE TEXT SIZE"),
                choices(theme, &["Small", "Default", "Large"], prefs.text_size.unwrap_or(1).clamp(0, 2) as usize, |index| pref(Pref::TextSize(index))),
                self.privacy_flip(
                    "Collapse long messages",
                    "Fold long messages and code, with Show more to expand them.",
                    self.cx.layout.collapse_long,
                    Pref::CollapseLong,
                ),
            ]),
            self.privacy_card("MOTION IN YAPFAST", [
                form::heading(theme, "PLAY GIFS, EMOJI AND STICKERS"),
                choices(theme, &["While focused", "Always", "Never"], animate, |index| pref(Pref::Animate(index))),
                cosmetic("Play profile effects", "Allow animations over profile cards.", cosmetics.effects, Cosmetic::Effects),
                cosmetic("Animate decorations and nameplates", "Off keeps these cosmetics still, including on hover.", cosmetics.moving, Cosmetic::Moving),
            ]),
            self.discord_handoff(
                Handoff::Accessibility,
                "These controls affect Yapfast only. For Discord's own accessibility options, open Discord, then User Settings > Accessibility. Discord's text-to-speech and contrast settings do not configure Yapfast.",
            ),
        ])
    }

    pub(super) fn streamer_mode_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let volume = u32::from(self.cx.layout.voice.sound_volume.unwrap_or(100));
        form::page([
            form::title(theme, "Streamer Mode"),
            form::note(theme, "Prepare Yapfast for sharing your screen. Names, messages and invite links remain visible; there is no automatic streamer mode."),
            self.privacy_card("NOTIFICATIONS AND SOUNDS IN YAPFAST", [
                self.privacy_flip(
                    "Show desktop notifications",
                    "Allow Yapfast to show message notifications while it is in the background.",
                    prefs.notifications.unwrap_or(true),
                    Pref::Notifications,
                ),
                self.privacy_flip(
                    "Show message text in notifications",
                    "Off hides message text. Sender and conversation names still appear when desktop notifications are enabled.",
                    prefs.notification_text.unwrap_or(true),
                    Pref::NotificationText,
                ),
                self.privacy_flip(
                    "Flash the taskbar",
                    "Flash Yapfast for a message that notifies while the window is in the background.",
                    prefs.flash_taskbar.unwrap_or(true),
                    Pref::FlashTaskbar,
                ),
                slider("Yapfast sound volume", volume, 200, |value| pref(Pref::SoundVolume(value, false)))
                    .on_release(pref(Pref::SoundVolume(volume, true)))
                    .view(theme),
                form::note(theme, "0% silences Yapfast's interface sounds. Call audio, streams and the soundboard have separate volume controls."),
            ]),
            self.discord_handoff(
                Handoff::StreamerMode,
                "To configure streamer mode for the official Discord app, open Discord, then User Settings > Streamer Mode. Discord's identity and invite masking and streaming-software detection do not apply to Yapfast.",
            ),
        ])
    }

    pub(super) fn activity_privacy_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let mut controls = vec![self.privacy_flip(
            "Let others see when you're typing",
            "Send typing indicators while you write in Yapfast. Others' typing indicators still appear.",
            prefs.share_typing.unwrap_or(true),
            Pref::ShareTyping,
        )];
        if can_use_discord(self.cx.state) {
            let shown = self.cx.state.discord_show_game;
            controls.push(
                switch(
                    "Share the game you play",
                    shown,
                    Some(crate::app::settings(SettingsEvent::Game(GameEvent::Show(!shown)))),
                )
                .description("Uses the same Discord account setting as Game Activity. Changes apply to Discord too; a failed save restores the previous value.")
                .view(self.cx.look),
            );
            controls.push(self.privacy_flip(
                "Show me as idle when I'm away",
                "Let Yapfast report idle presence after inactivity. This does not hide your online presence.",
                prefs.show_idle.unwrap_or(true),
                Pref::ShowIdle,
            ));
        }
        form::page([
            form::title(theme, "Activity Privacy"),
            self.privacy_card("SHARING FROM YAPFAST", controls),
            self.discord_handoff(
                Handoff::ActivityPrivacy,
                "For activity audiences, per-server sharing, connected services and recent activity history, open Discord, then User Settings > Activity Privacy. Yapfast does not edit those controls.",
            ),
        ])
    }
}
