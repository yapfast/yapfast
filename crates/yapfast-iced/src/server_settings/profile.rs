//! Server Profile, Engagement and Boost Perks: what the server is called
//! and looks like, where system messages go and which, how members who set
//! nothing are notified, where idle members move, and the pictures and
//! progress bar that boosting unlocks. Each edits the core's draft, which
//! the bar saves.

use iced::{
    Alignment, Element, Length,
    widget::{column, row, text},
};
use yapfast_core::{
    application::{
        AFK_TIMEOUTS, BOOSTED, BoostPage, DESCRIPTION_LIMIT, EngagementPage, ProfilePage,
        SETUP_TIPS, ServerSettings, SettingsChoice, SettingsText, SettingsToggle, WELCOME,
        WELCOME_STICKER,
    },
    chat::ServerPicture,
};

use super::View;
use super::{DESCRIPTION, Drop, Event, Screen, message, picture_box};
use crate::{
    app::Message,
    controls::{Kind, action},
    fonts::{BOLD, SEMIBOLD},
    form,
};

/// The inactive timeouts' names, as [`AFK_TIMEOUTS`] lists them.
const TIMEOUTS: [&str; 5] = [
    crate::i18n::tr_mark!("1 minute"),
    crate::i18n::tr_mark!("5 minutes"),
    crate::i18n::tr_mark!("15 minutes"),
    crate::i18n::tr_mark!("30 minutes"),
    crate::i18n::tr_mark!("1 hour"),
];

/// Engagement's switches: each system message's bit and words.
const SYSTEM_MESSAGES: [(u64, &str); 4] = [
    (
        WELCOME,
        crate::i18n::tr_mark!("Send a random welcome message when someone joins this server."),
    ),
    (
        WELCOME_STICKER,
        crate::i18n::tr_mark!("Prompt members to reply to welcome messages with a sticker."),
    ),
    (
        BOOSTED,
        crate::i18n::tr_mark!("Send a message when someone boosts this server."),
    ),
    (
        SETUP_TIPS,
        crate::i18n::tr_mark!("Send helpful tips for server setup."),
    ),
];

impl<'a> View<'a> {
    /// Server Profile: the name, the icon with Change and Remove, and the
    /// description with how much of it is left.
    pub(super) fn server_profile_page(
        self,
        core: &'a ServerSettings,
        page: &'a ProfilePage,
        screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let ready = !core.saving;
        let reading = core.reading.is_some();
        let (url, new) = page.icon();
        let icon: Element<'a, Message> = match new {
            Some(picture) => picture_box(
                self.cx,
                None,
                Some(picture),
                80.0,
                80.0,
                40.0,
                column![].into(),
            ),
            None => self.cx.avatar(url, &page.name, 80.0, 40.0, 1.0),
        };
        let mut buttons = vec![
            action(
                theme,
                crate::i18n::tr!(self.cx.i18n, "Change Server Icon"),
                Kind::Plain,
                (ready && !reading).then(|| message(Event::Pick(ServerPicture::Icon))),
            )
            .into(),
        ];
        if url.is_some() || new.is_some() {
            buttons.push(
                action(
                    theme,
                    crate::i18n::tr!(self.cx.i18n, "Remove Icon"),
                    Kind::Ghost,
                    ready.then(|| message(Event::Remove(ServerPicture::Icon))),
                )
                .into(),
            );
        }
        let left = DESCRIPTION_LIMIT as i64 - page.description.trim().chars().count() as i64;
        form::page([
            form::title(theme, crate::i18n::tr!(self.cx.i18n, "Server Profile")),
            form::note(
                theme,
                crate::i18n::tr!(
                    self.cx.i18n,
                    "Customize how your server appears in invite links and, if enabled, in Server Discovery and Announcement Channel messages."
                ),
            ),
            form::group([
                form::heading(theme, crate::i18n::tr!(self.cx.i18n, "NAME")),
                form::entry(theme, "server-name", "", &page.name)
                    .on_input_maybe(
                        ready.then_some(|text| message(Event::Write(SettingsText::Name, text))),
                    )
                    .into(),
            ]),
            form::group([
                form::heading(theme, crate::i18n::tr!(self.cx.i18n, "ICON")),
                row![
                    icon,
                    column![
                        form::note(
                            theme,
                            if reading {
                                crate::i18n::tr!(self.cx.i18n, "Reading the picture...")
                            } else {
                                crate::i18n::tr!(
                                    self.cx.i18n,
                                    "We recommend an image of at least 512x512."
                                )
                            }
                        ),
                        form::buttons(buttons)
                    ]
                    .spacing(form::TIGHT)
                ]
                .spacing(form::GAP)
                .align_y(Alignment::Center)
                .into(),
            ]),
            form::group([
                form::heading(theme, crate::i18n::tr!(self.cx.i18n, "DESCRIPTION")),
                form::area(
                    theme,
                    &screen.description,
                    DESCRIPTION,
                    crate::i18n::tr!(self.cx.i18n, "Tell people what your server is about."),
                    left,
                    ready,
                    |event| message(Event::Description(event)),
                ),
            ]),
        ])
    }

    /// Engagement: the system messages' channel and switches (which wait
    /// for a channel), the default notifications, and the inactive channel
    /// with its timeout (which waits for one).
    pub(super) fn engagement_page(
        self,
        core: &'a ServerSettings,
        page: &'a EngagementPage,
        screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let ready = !core.saving;
        let app = &self.cx.app;
        let text_channels = app.settings_channels(&core.server, false);
        let voice_channels = app.settings_channels(&core.server, true);
        let system = channel_list(
            crate::i18n::tr!(self.cx.i18n, "No System Messages"),
            &text_channels,
            page.system_channel.as_deref(),
        );
        let afk = channel_list(
            crate::i18n::tr!(self.cx.i18n, "No Inactive Channel"),
            &voice_channels,
            page.afk_channel.as_deref(),
        );
        let open = screen.open;
        let system_channels = text_channels;
        let system_drop = form::dropdown(
            system.0,
            Some(system.1),
            open == Some(Drop::SystemChannel),
            message(Event::Drop(Drop::SystemChannel)),
            move |index| {
                let channel = index
                    .checked_sub(1)
                    .and_then(|index| system_channels.get(index))
                    .map(|(id, _)| id.clone());
                message(Event::Choose(SettingsChoice::SystemChannel(channel)))
            },
        )
        .enabled(ready)
        .view(self.cx.look);
        let mut parts = vec![
            form::title(theme, crate::i18n::tr!(self.cx.i18n, "Engagement")),
            form::group([
                form::heading(
                    theme,
                    crate::i18n::tr!(self.cx.i18n, "SYSTEM MESSAGES CHANNEL"),
                ),
                form::note(
                    theme,
                    crate::i18n::tr!(
                        self.cx.i18n,
                        "This is the channel we send system event messages to. These can be turned off at any time."
                    ),
                ),
                system_drop,
            ]),
        ];
        let sends = page.system_channel.is_some();
        for (bit, words) in SYSTEM_MESSAGES {
            parts.push(
                form::switch(
                    self.cx.i18n.text(words),
                    page.sends(bit),
                    (ready && sends)
                        .then(|| message(Event::Toggle(SettingsToggle::SystemMessage(bit)))),
                )
                .view(self.cx.look),
            );
        }
        let voice = voice_channels;
        let afk_drop = form::dropdown(
            afk.0,
            Some(afk.1),
            open == Some(Drop::AfkChannel),
            message(Event::Drop(Drop::AfkChannel)),
            move |index| {
                let channel = index
                    .checked_sub(1)
                    .and_then(|index| voice.get(index))
                    .map(|(id, _)| id.clone());
                message(Event::Choose(SettingsChoice::AfkChannel(channel)))
            },
        )
        .enabled(ready)
        .view(self.cx.look);
        // Discord's default, five minutes, for a timeout it doesn't offer.
        let timeout = AFK_TIMEOUTS
            .iter()
            .position(|seconds| *seconds == page.afk_timeout)
            .unwrap_or(1);
        let timeout_drop = form::dropdown(
            TIMEOUTS
                .iter()
                .map(|name| self.cx.i18n.text(name).to_owned())
                .collect(),
            Some(timeout),
            open == Some(Drop::AfkTimeout),
            message(Event::Drop(Drop::AfkTimeout)),
            |index| {
                message(Event::Choose(SettingsChoice::AfkTimeout(
                    AFK_TIMEOUTS[index],
                )))
            },
        )
        .enabled(ready && page.afk_channel.is_some())
        .view(self.cx.look);
        parts.extend([
            form::group([
                form::heading(theme, crate::i18n::tr!(self.cx.i18n, "DEFAULT NOTIFICATION SETTINGS")),
                form::note(
                    theme,
                    crate::i18n::tr!(self.cx.i18n, "This will determine whether members who have not explicitly set their notification settings receive a notification for every message sent in this server or not."),
                ),
                crate::controls::choices(
                    theme,
                    &[crate::i18n::tr!(self.cx.i18n, "All Messages"), crate::i18n::tr!(self.cx.i18n, "Only @mentions")],
                    usize::from(page.default_notifications.min(1)),
                    |index| message(Event::Choose(SettingsChoice::Notifications(index as u8))),
                ),
            ]),
            form::group([
                form::heading(theme, crate::i18n::tr!(self.cx.i18n, "INACTIVE CHANNEL")),
                form::note(
                    theme,
                    crate::i18n::tr!(self.cx.i18n, "Automatically move members to this channel and mute them when they have been idle for longer than the inactive timeout. This does not affect browsers."),
                ),
                row![
                    // The row is shared out 19 to 13.
                    iced::widget::container(afk_drop).width(Length::FillPortion(19)),
                    iced::widget::container(timeout_drop).width(Length::FillPortion(13))
                ]
                .spacing(form::GAP)
                .into(),
            ]),
        ]);
        form::page(parts)
    }

    /// Boost Perks: the level, the progress bar's switch, and the banner
    /// and invite background, each with the level that unlocks it.
    pub(super) fn boost_page(
        self,
        core: &'a ServerSettings,
        page: &'a BoostPage,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let ready = !core.saving && core.reading.is_none();
        let level = match page.original.premium_tier {
            0 => crate::i18n::tr!(self.cx.i18n, "No Level").to_owned(),
            tier => {
                crate::i18n::tr!(self.cx.i18n, "Level {tier}").replace("{tier}", &tier.to_string())
            }
        };
        form::page([
            row![
                iced::widget::container(form::title(theme, crate::i18n::tr!(self.cx.i18n, "Boost Perks"))).width(Length::Fill),
                text(level).size(14).font(BOLD).color(theme.muted)
            ]
            .align_y(Alignment::Center)
            .into(),
            form::switch(
                crate::i18n::tr!(self.cx.i18n, "Show Boost progress bar"),
                page.progress_bar,
                (!core.saving).then(|| message(Event::Toggle(SettingsToggle::ProgressBar))),
            )
            .description(
                crate::i18n::tr!(self.cx.i18n, "This progress bar will display in your channel list, attached to your server name (or server banner if you have one set)."),
            )
            .view(self.cx.look),
            self.boost_background(
                page,
                ServerPicture::Banner,
                2,
                "SERVER BANNER BACKGROUND",
                crate::i18n::tr!(self.cx.i18n, "The recommended minimum size is 960x540 and recommended aspect ratio is 16:9."),
                ready,
            ),
            self.boost_background(
                page,
                ServerPicture::Splash,
                1,
                "SERVER INVITE BACKGROUND",
                crate::i18n::tr!(self.cx.i18n, "Servers who reach Boosting Level 1 and above get unique invite embeds with custom Server Invite Backgrounds. The recommended minimum size is 1920x1080 and recommended aspect ratio is 16:9."),
                ready,
            ),
        ])
    }

    /// A banner or invite background: the picture at 16 by 9 (or the level
    /// that unlocks it), with Upload and Remove.
    fn boost_background(
        self,
        page: &'a BoostPage,
        kind: ServerPicture,
        level: u8,
        heading: &'a str,
        words: &'a str,
        ready: bool,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let (url, new) = page.picture(kind);
        let unlocked = page.unlocked(kind);
        let empty = text(if unlocked {
            crate::i18n::tr!(self.cx.i18n, "No image").to_owned()
        } else {
            crate::i18n::tr!(self.cx.i18n, "Unlocks at Level {level}")
                .replace("{level}", &level.to_string())
        })
        .size(13)
        .font(SEMIBOLD)
        .color(theme.muted);
        let mut buttons = column![action(
            theme,
            crate::i18n::tr!(self.cx.i18n, "Upload Background"),
            Kind::Plain,
            (ready && unlocked).then(|| message(Event::Pick(kind))),
        )]
        .spacing(form::TIGHT);
        if url.is_some() || new.is_some() {
            buttons = buttons.push(action(
                theme,
                crate::i18n::tr!(self.cx.i18n, "Remove Background"),
                Kind::Ghost,
                ready.then(|| message(Event::Remove(kind))),
            ));
        }
        form::group([
            form::heading(theme, heading),
            form::note(theme, words),
            row![
                picture_box(self.cx, url, new, 240.0, 135.0, 8.0, empty.into()),
                buttons
            ]
            .spacing(form::GAP)
            .align_y(Alignment::Center)
            .into(),
        ])
    }
}

/// A list of channels as a page's dropdown shows it, `none` first, and the
/// place of `chosen` in it.
fn channel_list(
    none: &str,
    channels: &[(String, String)],
    chosen: Option<&str>,
) -> (Vec<String>, usize) {
    let names = std::iter::once(none.to_owned())
        .chain(channels.iter().map(|(_, name)| name.clone()))
        .collect();
    let index = chosen
        .and_then(|chosen| channels.iter().position(|(id, _)| id == chosen))
        .map_or(0, |index| index + 1);
    (names, index)
}
