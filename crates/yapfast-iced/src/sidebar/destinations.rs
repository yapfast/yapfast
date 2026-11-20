//! The Nitro, Shop and Quests screens on the DM home: opened directly in
//! Yapfast in place of a conversation, with live account, cosmetic, boost
//! and activity controls plus optional handoffs to Discord's web pages.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Element, Length, Padding, Point,
    widget::{column, container, progress_bar, row, text},
};

use crate::{
    app::Message,
    controls::{Kind, action},
    ctx::Ctx,
    fonts::BOLD,
    form::{self, switch},
    settings::{Section, appearance::Cosmetic},
    style::scrollbar,
};

pub const NITRO_URL: &str = "https://discord.com/nitro";
pub const SHOP_URL: &str = "https://discord.com/shop";
pub const QUESTS_URL: &str = "https://discord.com/discovery/quests";

const LEVEL_BOOSTS: [u32; 3] = [2, 7, 14];
const METER_HEIGHT: f32 = 8.0;

fn page_frame<'a>(cx: Ctx<'a>, content: Element<'a, Message>) -> Element<'a, Message> {
    let theme = cx.look.theme;
    scroll_view(
        container(container(content).max_width(form::COLUMN))
            .padding(Padding {
                top: form::PAGE_TOP,
                right: form::PAGE_SIDE,
                bottom: form::PAGE_BOTTOM,
                left: form::PAGE_SIDE,
            })
            .center_x(Length::Fill),
    )
    .style(scrollbar(theme))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub fn nitro_screen<'a>(cx: Ctx<'a>) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let nitro = cx.app.discord_nitro();
    let status_headline = if nitro {
        "Discord Nitro is active on your account"
    } else {
        "Standard Discord account"
    };
    let status_note = if nitro {
        "Custom emoji everywhere, animated avatars, profile banners, custom themes and HD streaming are unlocked in Yapfast."
    } else {
        "Manage your profile look, review server boost progress across your servers, or open Discord's Nitro page for subscription management."
    };
    let overview = form::card(
        theme,
        form::page([
            form::heading(theme, "ACCOUNT STATUS"),
            text(status_headline)
                .size(18)
                .font(BOLD)
                .color(theme.text)
                .into(),
            form::note(theme, status_note),
            form::buttons([
                action(
                    theme,
                    "Edit Profile",
                    Kind::Primary,
                    Some(Message::Side(super::Event::EditProfile)),
                )
                .into(),
                action(
                    theme,
                    "Account & Billing Settings",
                    Kind::Plain,
                    Some(Message::Settings(crate::settings::Event::Open(
                        Section::Account,
                    ))),
                )
                .into(),
                action(
                    theme,
                    "Voice & Video Settings",
                    Kind::Plain,
                    Some(Message::Settings(crate::settings::Event::Open(
                        Section::Voice,
                    ))),
                )
                .into(),
                action(
                    theme,
                    "Open Nitro on discord.com",
                    Kind::Ghost,
                    Some(Message::Link(NITRO_URL.into(), Point::ORIGIN)),
                )
                .into(),
            ]),
        ]),
    );

    let discord_servers: Vec<_> = cx
        .state
        .servers
        .iter()
        .filter(|server| server.discord && !server.unavailable)
        .collect();
    let boosted_count = discord_servers
        .iter()
        .filter(|server| server.boosts.unwrap_or(0) > 0 || server.premium_tier.unwrap_or(0) > 0)
        .count();
    let total_boosts: u32 = discord_servers
        .iter()
        .filter_map(|server| server.boosts)
        .sum();

    let perks = form::card(
        theme,
        form::page([
            form::heading(theme, "PERKS IN YAPFAST"),
            form::note(
                theme,
                format!(
                    "{} joined Discord servers loaded ({} with active boosts, {} total reported boosts).",
                    discord_servers.len(),
                    boosted_count,
                    total_boosts
                ),
            ),
            form::note(
                theme,
                "Profile banners, two-tone profile themes, avatar decorations, custom server profiles, animated emoji and cross-server stickers work natively in Yapfast.",
            ),
        ]),
    );

    let mut boost_rows: Vec<Element<'a, Message>> = Vec::new();
    for server in discord_servers
        .into_iter()
        .filter(|server| server.boosts.is_some() || server.premium_tier.unwrap_or(0) > 0)
        .take(12)
    {
        let tier = server.premium_tier.unwrap_or(0);
        let boosts = server.boosts.unwrap_or(0);
        let target = LEVEL_BOOSTS[usize::from(tier.min(2))];
        let header = row![
            text(server.name.as_str())
                .size(15)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill),
            text(format!("Level {tier} · {boosts} boosts"))
                .size(13)
                .color(theme.secondary),
            action(
                theme,
                "Open Server",
                Kind::Plain,
                Some(Message::Rail(crate::rail::Event::Server(server.id.clone()))),
            )
            .small(),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let bar = progress_bar(0.0..=target as f32, boosts.min(target) as f32)
            .girth(METER_HEIGHT)
            .style(move |_| progress_bar::Style {
                background: theme.field.into(),
                bar: theme.accent.into(),
                border: crate::style::round(METER_HEIGHT / 2.0),
            });
        boost_rows.push(form::card(theme, column![header, bar].spacing(8)));
    }
    if boost_rows.is_empty() {
        boost_rows.push(form::card(
            theme,
            form::note(
                theme,
                "No server boost progress bars are currently reported among your joined servers.",
            ),
        ));
    }

    let boosts_section = form::page(
        std::iter::once(form::heading(theme, "SERVER BOOST PROGRESS")).chain(boost_rows),
    );

    page_frame(
        cx,
        form::page([form::title(theme, "Nitro"), overview, perks, boosts_section]),
    )
}

pub fn shop_screen<'a>(cx: Ctx<'a>) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let prefs = &cx.layout.prefs;
    let cosmetics = prefs.cosmetics();
    let has_decoration = cx.state.account_decoration.is_some();
    let has_nameplate = cx.state.account_nameplate.is_some();
    let worn_summary = match (has_decoration, has_nameplate) {
        (true, true) => "Your account is currently wearing an avatar decoration and a nameplate.",
        (true, false) => "Your account is currently wearing an avatar decoration.",
        (false, true) => "Your account is currently wearing a nameplate.",
        (false, false) => {
            "No avatar decoration or nameplate is currently equipped on your account card."
        }
    };

    let worn_card = form::card(
        theme,
        form::page([
            form::heading(theme, "YOUR PROFILE LOOK & COSMETICS"),
            form::note(theme, worn_summary),
            form::buttons([
                action(
                    theme,
                    "Edit Profile",
                    Kind::Primary,
                    Some(Message::Side(super::Event::EditProfile)),
                )
                .into(),
                action(
                    theme,
                    "Themes & Appearance",
                    Kind::Plain,
                    Some(Message::Settings(crate::settings::Event::Open(
                        Section::Appearance,
                    ))),
                )
                .into(),
                action(
                    theme,
                    "Open Shop on discord.com",
                    Kind::Ghost,
                    Some(Message::Link(SHOP_URL.into(), Point::ORIGIN)),
                )
                .into(),
            ]),
        ]),
    );

    let toggle = |label: &'static str, description: &'static str, on: bool, which: Cosmetic| {
        switch(
            label,
            on,
            Some(Message::Side(super::Event::Cosmetic(which, !on))),
        )
        .description(description)
        .view(cx.look)
    };

    let switches_card = form::card(
        theme,
        form::group([
            form::heading(theme, "COSMETICS SHOWN IN YAPFAST"),
            toggle(
                "Avatar decorations",
                "Show decorations worn around avatars in chats, DMs and member lists.",
                cosmetics.decorations,
                Cosmetic::Decorations,
            ),
            toggle(
                "Profile effects",
                "Play animated effects over profile cards.",
                cosmetics.effects,
                Cosmetic::Effects,
            ),
            toggle(
                "Profile frames",
                "Draw decorative frames around profile cards.",
                cosmetics.frames,
                Cosmetic::Frames,
            ),
            toggle(
                "Nameplates",
                "Show themed nameplates behind member and DM rows.",
                !cx.state.hide_nameplates,
                Cosmetic::Nameplates,
            ),
            toggle(
                "Animate decorations and nameplates",
                "Play animated artwork when hovered or focused.",
                cosmetics.moving,
                Cosmetic::Moving,
            ),
            toggle(
                "Display name styles",
                "Render custom display name fonts and gradients.",
                cosmetics.name_styles,
                Cosmetic::NameStyles,
            ),
            toggle(
                "Server tags",
                "Show server tag badges beside display names.",
                cosmetics.server_tags,
                Cosmetic::ServerTags,
            ),
        ]),
    );

    let decorated_friends: Vec<_> = cx
        .state
        .friends
        .iter()
        .filter(|friend| friend.decoration.is_some())
        .take(10)
        .collect();
    let mut friend_rows: Vec<Element<'a, Message>> = Vec::new();
    for friend in decorated_friends {
        friend_rows.push(
            row![
                cx.avatar(friend.avatar.as_deref(), &friend.name, 32.0, 16.0, 1.0),
                text(friend.name.as_str())
                    .size(15)
                    .font(BOLD)
                    .color(theme.text)
                    .width(Length::Fill),
                action(
                    theme,
                    "Message",
                    Kind::Plain,
                    Some(Message::Friends(crate::friends::Event::Message(
                        friend.id.clone()
                    ))),
                )
                .small(),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
        );
    }
    let friends_card = if friend_rows.is_empty() {
        form::card(
            theme,
            form::group([
                form::heading(theme, "FRIENDS WEARING COSMETICS"),
                form::note(
                    theme,
                    "Friends wearing avatar decorations appear here automatically as your friend list syncs.",
                ),
            ]),
        )
    } else {
        form::card(
            theme,
            form::page(
                std::iter::once(form::heading(theme, "FRIENDS WEARING COSMETICS"))
                    .chain(friend_rows),
            ),
        )
    };

    page_frame(
        cx,
        form::page([
            form::title(theme, "Shop"),
            worn_card,
            switches_card,
            friends_card,
        ]),
    )
}

pub fn quests_screen<'a>(cx: Ctx<'a>) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let share_game = cx.state.discord_show_game;
    let current_game = cx
        .state
        .discord_playing
        .as_ref()
        .map(|playing| playing.name.as_str())
        .unwrap_or("No active game detected on this device.");

    let status_card = form::card(
        theme,
        form::page([
            form::heading(theme, "YOUR GAME & QUEST ACTIVITY"),
            text(current_game)
                .size(16)
                .font(BOLD)
                .color(theme.text)
                .into(),
            switch(
                "Share the game you play",
                share_game,
                Some(Message::Side(super::Event::ShareGame(!share_game))),
            )
            .description("Broadcast your active game to Discord so friends and Quest progress can track your session.")
            .view(cx.look),
            form::buttons([
                action(
                    theme,
                    "Game Activity Settings",
                    Kind::Primary,
                    Some(Message::Settings(crate::settings::Event::Open(
                        Section::Games,
                    ))),
                )
                .into(),
                action(
                    theme,
                    "Activity Privacy",
                    Kind::Plain,
                    Some(Message::Settings(crate::settings::Event::Open(
                        Section::ActivityPrivacy,
                    ))),
                )
                .into(),
                action(
                    theme,
                    "Open Quests on discord.com",
                    Kind::Ghost,
                    Some(Message::Link(QUESTS_URL.into(), Point::ORIGIN)),
                )
                .into(),
            ]),
        ]),
    );

    let active_friends: Vec<_> = cx
        .state
        .friends
        .iter()
        .filter(|friend| {
            friend.activity.as_ref().is_some_and(|act| !act.is_empty())
                || !friend.activities.is_empty()
        })
        .take(12)
        .collect();

    let mut activity_rows: Vec<Element<'a, Message>> = Vec::new();
    for friend in active_friends {
        let detail = friend
            .activities
            .first()
            .map(
                |act| match act.details.as_deref().filter(|d| !d.is_empty()) {
                    Some(details) => format!("{} · {details}", act.name),
                    None => act.name.to_string(),
                },
            )
            .or_else(|| friend.activity.clone())
            .unwrap_or_default();
        activity_rows.push(form::card(
            theme,
            row![
                cx.avatar(friend.avatar.as_deref(), &friend.name, 36.0, 18.0, 1.0),
                column![
                    text(friend.name.as_str())
                        .size(15)
                        .font(BOLD)
                        .color(theme.text),
                    text(detail).size(13).color(theme.secondary),
                ]
                .spacing(2)
                .width(Length::Fill),
                action(
                    theme,
                    "Message",
                    Kind::Plain,
                    Some(Message::Friends(crate::friends::Event::Message(
                        friend.id.clone()
                    ))),
                )
                .small(),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        ));
    }

    if activity_rows.is_empty() {
        activity_rows.push(form::card(
            theme,
            form::note(
                theme,
                "None of your friends are broadcasting a live game or activity right now.",
            ),
        ));
    }

    let live_section = form::page(
        std::iter::once(form::heading(theme, "FRIENDS PLAYING & STREAMING NOW"))
            .chain(activity_rows),
    );

    page_frame(
        cx,
        form::page([form::title(theme, "Quests"), status_card, live_section]),
    )
}
