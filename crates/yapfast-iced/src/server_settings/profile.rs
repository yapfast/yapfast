//! Server Profile, Engagement and Boost Perks: what the server is called
//! and looks like, where system messages go and which, how members who set
//! nothing are notified, where idle members move, and the pictures and
//! progress bar that boosting unlocks. Each edits the core's draft, which
//! the bar saves.

use iced::{
    Alignment, Background, Color, Element, Length, Padding,
    widget::{button, column, container, row, stack, text},
};
use yapfast_core::{
    application::{
        AFK_TIMEOUTS, BOOSTED, BoostPage, DESCRIPTION_LIMIT, EngagementPage, ProfilePage,
        SETUP_TIPS, ServerSettings, SettingsChoice, SettingsText, SettingsToggle, TRAITS, WELCOME,
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
    "1 minute",
    "5 minutes",
    "15 minutes",
    "30 minutes",
    "1 hour",
];

/// Discord's ten banner colours, as Server Profile offers them, each by
/// the hex string Discord keeps and its name.
pub const BANNERS: [(&str, &str); 10] = [
    ("#0f0f0f", "Black"),
    ("#ff2f9d", "Pink"),
    ("#eb2f2c", "Red"),
    ("#eb7c2c", "Orange"),
    ("#eccb41", "Yellow"),
    ("#804699", "Purple"),
    ("#1daefd", "Blue"),
    ("#62e7d3", "Teal"),
    ("#507614", "Green"),
    ("#363636", "Grey"),
];
/// The swatches across, one's height, and the preview's size.
const SWATCHES_ACROSS: usize = 4;
const SWATCH: f32 = 48.0;
const PREVIEW: (f32, f32) = (320.0, 148.0);
/// The traits across.
const TRAITS_ACROSS: usize = 3;

/// A hex string such as "#ff73fa", as RGB.
fn hex_rgb(hex: &str) -> Option<u32> {
    let digits = hex.strip_prefix('#')?;
    (digits.len() == 6)
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten()
}

/// Discord's banner ground for `tint`: darker at the top, lighter at the
/// bottom, as it draws the swatches and the banner.
fn banner_ground(tint: Color) -> Background {
    let scale = |color: Color, by: f32| Color {
        r: (color.r * by).min(1.0),
        g: (color.g * by).min(1.0),
        b: (color.b * by).min(1.0),
        a: 1.0,
    };
    Background::Gradient(
        iced::gradient::Linear::new(std::f32::consts::PI)
            .add_stop(0.0, scale(tint, 1.0 / 1.3))
            .add_stop(1.0, scale(tint, 1.25))
            .into(),
    )
}

/// Engagement's switches: each system message's bit and words.
const SYSTEM_MESSAGES: [(u64, &str); 4] = [
    (
        WELCOME,
        "Send a random welcome message when someone joins this server.",
    ),
    (
        WELCOME_STICKER,
        "Prompt members to reply to welcome messages with a sticker.",
    ),
    (BOOSTED, "Send a message when someone boosts this server."),
    (SETUP_TIPS, "Send helpful tips for server setup."),
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
                "Change Server Icon",
                Kind::Plain,
                (ready && !reading).then(|| message(Event::Pick(ServerPicture::Icon))),
            )
            .into(),
        ];
        if url.is_some() || new.is_some() {
            buttons.push(
                action(
                    theme,
                    "Remove Icon",
                    Kind::Ghost,
                    ready.then(|| message(Event::Remove(ServerPicture::Icon))),
                )
                .into(),
            );
        }
        let left = DESCRIPTION_LIMIT as i64 - page.description.trim().chars().count() as i64;
        form::page([
            form::title(theme, "Server Profile"),
            form::note(
                theme,
                "Customize how your server appears in invite links and, if enabled, in Server Discovery and Announcement Channel messages.",
            ),
            form::group([
                form::heading(theme, "NAME"),
                form::entry(theme, "server-name", "", &page.name)
                    .on_input_maybe(
                        ready.then_some(|text| message(Event::Write(SettingsText::Name, text))),
                    )
                    .into(),
            ]),
            form::group([
                form::heading(theme, "ICON"),
                row![
                    icon,
                    column![
                        form::note(
                            theme,
                            if reading {
                                "Reading the picture..."
                            } else {
                                "We recommend an image of at least 512x512."
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
                form::heading(theme, "BANNER"),
                self.banner_preview(page, url, new),
                self.banner_swatches(page, ready),
            ]),
            form::group([
                form::heading(theme, "TRAITS"),
                form::note(
                    theme,
                    "Add up to 5 traits to show off your server's interests and personality.",
                ),
                self.trait_fields(page, ready),
            ]),
            form::group([
                form::heading(theme, "DESCRIPTION"),
                form::area(
                    theme,
                    &screen.description,
                    DESCRIPTION,
                    "Tell people what your server is about.",
                    left,
                    ready,
                    |event| message(Event::Description(event)),
                ),
            ]),
        ])
    }

    /// The colour picked on a card, as invites show the server, before it's
    /// saved: its banner in Discord's gradient, its icon and its name; a
    /// Yapfast addition.
    fn banner_preview(
        self,
        page: &'a ProfilePage,
        url: Option<&'a str>,
        new: Option<&'a std::sync::Arc<yapfast_core::chat::Picture>>,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let tint = page.brand_color.as_deref().and_then(hex_rgb);
        let banner_ground = match tint {
            Some(rgb) => banner_ground(crate::style::hex(rgb)),
            None => Background::Color(theme.raised),
        };
        let banner = container(iced::widget::Space::new())
            .width(Length::Fill)
            .height(64)
            .style(move |_| container::Style {
                background: Some(banner_ground),
                ..container::Style::default()
            });
        let icon: Element<'a, Message> = match new {
            Some(picture) => picture_box(
                self.cx,
                None,
                Some(picture),
                52.0,
                52.0,
                14.0,
                column![].into(),
            ),
            None => self.cx.avatar(url, &page.name, 52.0, 14.0, 1.0),
        };
        let sidebar = theme.sidebar;
        let ring = container(icon)
            .padding(4)
            .style(move |_| crate::style::rounded(sidebar, 18));
        let words = column![
            crate::elide::elide(page.name.as_str())
                .size(16)
                .font(BOLD)
                .color(theme.text),
            text("Preview").size(12).color(theme.muted),
        ]
        .spacing(2);
        let (ground, line) = (theme.sidebar, theme.line);
        container(stack![
            column![banner],
            crate::controls::placed(ring, 12.0, 36.0),
            crate::controls::placed(container(words).width(PREVIEW.0 - 32.0), 16.0, 102.0),
        ])
        .width(PREVIEW.0)
        .height(PREVIEW.1)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(Background::Color(ground)),
            border: iced::Border {
                color: line,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..container::Style::default()
        })
        .into()
    }

    /// Discord's ten banner colours, four across, each drawn as it draws
    /// them; the one picked outlined in the accent.
    fn banner_swatches(self, page: &'a ProfilePage, ready: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let picked = page.brand_color.as_deref();
        let mut rows = column![].spacing(form::TIGHT);
        for line in BANNERS.chunks(SWATCHES_ACROSS) {
            let mut cells = row![].spacing(form::TIGHT);
            for &(hex, name) in line {
                let ground = hex_rgb(hex).map(|rgb| banner_ground(crate::style::hex(rgb)));
                let chosen = picked == Some(hex);
                let (accent, ink) = (theme.accent, theme.text);
                let swatch =
                    button(iced::widget::Space::new())
                        .width(Length::FillPortion(1))
                        .height(SWATCH)
                        .on_press_maybe(ready.then(|| {
                            message(Event::Write(SettingsText::BrandColor, hex.to_owned()))
                        }))
                        .style(move |_, status| button::Style {
                            background: ground,
                            border: iced::Border {
                                color: if chosen {
                                    accent
                                } else {
                                    Color { a: 0.5, ..ink }
                                },
                                width: if chosen {
                                    2.0
                                } else if status == button::Status::Hovered {
                                    1.0
                                } else {
                                    0.0
                                },
                                radius: 8.0.into(),
                            },
                            ..button::Style::default()
                        });
                cells = cells.push(
                    iced::widget::tooltip(
                        swatch,
                        crate::controls::tip(theme, name),
                        iced::widget::tooltip::Position::Top,
                    )
                    .gap(4),
                );
            }
            // A short last row keeps its cells the others' width.
            for _ in line.len()..SWATCHES_ACROSS {
                cells = cells.push(iced::widget::Space::new().width(Length::FillPortion(1)));
            }
            rows = rows.push(cells);
        }
        container(rows).max_width(PREVIEW.0 * 1.5).into()
    }

    /// The five traits' fields, three across, each by an id of its place,
    /// so the one typed in keeps the keys as the page is built again.
    fn trait_fields(self, page: &'a ProfilePage, ready: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut rows = column![].spacing(form::TIGHT);
        for start in (0..TRAITS).step_by(TRAITS_ACROSS) {
            let mut cells = row![].spacing(form::TIGHT);
            for index in start..(start + TRAITS_ACROSS) {
                let Some(written) = page.traits.get(index) else {
                    cells = cells.push(iced::widget::Space::new().width(Length::FillPortion(1)));
                    continue;
                };
                cells = cells.push(
                    container(
                        form::entry(theme, trait_id(index), "Trait", written.as_str())
                            .on_input_maybe(ready.then_some(move |text| {
                                message(Event::Write(SettingsText::Trait(index), text))
                            })),
                    )
                    .width(Length::FillPortion(1)),
                );
            }
            rows = rows.push(cells);
        }
        container(rows).padding(Padding::ZERO).into()
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
            "No System Messages",
            &text_channels,
            page.system_channel.as_deref(),
        );
        let afk = channel_list(
            "No Inactive Channel",
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
            form::title(theme, "Engagement"),
            form::group([
                form::heading(theme, "SYSTEM MESSAGES CHANNEL"),
                form::note(
                    theme,
                    "This is the channel we send system event messages to. These can be turned off at any time.",
                ),
                system_drop,
            ]),
        ];
        let sends = page.system_channel.is_some();
        for (bit, words) in SYSTEM_MESSAGES {
            parts.push(
                form::switch(
                    words,
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
            TIMEOUTS.iter().map(|name| (*name).to_owned()).collect(),
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
                form::heading(theme, "DEFAULT NOTIFICATION SETTINGS"),
                form::note(
                    theme,
                    "This will determine whether members who have not explicitly set their notification settings receive a notification for every message sent in this server or not.",
                ),
                crate::controls::choices(
                    theme,
                    &["All Messages", "Only @mentions"],
                    usize::from(page.default_notifications.min(1)),
                    |index| message(Event::Choose(SettingsChoice::Notifications(index as u8))),
                ),
            ]),
            form::group([
                form::heading(theme, "INACTIVE CHANNEL"),
                form::note(
                    theme,
                    "Automatically move members to this channel and mute them when they have been idle for longer than the inactive timeout. This does not affect browsers.",
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
            0 => "No Level".to_owned(),
            tier => format!("Level {tier}"),
        };
        form::page([
            row![
                iced::widget::container(form::title(theme, "Boost Perks")).width(Length::Fill),
                text(level).size(14).font(BOLD).color(theme.muted)
            ]
            .align_y(Alignment::Center)
            .into(),
            form::switch(
                "Show Boost progress bar",
                page.progress_bar,
                (!core.saving).then(|| message(Event::Toggle(SettingsToggle::ProgressBar))),
            )
            .description(
                "This progress bar will display in your channel list, attached to your server name (or server banner if you have one set).",
            )
            .view(self.cx.look),
            self.boost_background(
                page,
                ServerPicture::Banner,
                2,
                "SERVER BANNER BACKGROUND",
                "The recommended minimum size is 960x540 and recommended aspect ratio is 16:9.",
                ready,
            ),
            self.boost_background(
                page,
                ServerPicture::Splash,
                1,
                "SERVER INVITE BACKGROUND",
                "Servers who reach Boosting Level 1 and above get unique invite embeds with custom Server Invite Backgrounds. The recommended minimum size is 1920x1080 and recommended aspect ratio is 16:9.",
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
            "No image".to_owned()
        } else {
            format!("Unlocks at Level {level}")
        })
        .size(13)
        .font(SEMIBOLD)
        .color(theme.muted);
        let mut buttons = column![action(
            theme,
            "Upload Background",
            Kind::Plain,
            (ready && unlocked).then(|| message(Event::Pick(kind))),
        )]
        .spacing(form::TIGHT);
        if url.is_some() || new.is_some() {
            buttons = buttons.push(action(
                theme,
                "Remove Background",
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

/// The id of the trait field at `index`, steady while the page is built
/// again, so the one typed in keeps its focus.
fn trait_id(index: usize) -> iced::widget::Id {
    const IDS: [&str; TRAITS] = [
        "server-trait-0",
        "server-trait-1",
        "server-trait-2",
        "server-trait-3",
        "server-trait-4",
    ];
    iced::widget::Id::new(IDS[index.min(TRAITS - 1)])
}
