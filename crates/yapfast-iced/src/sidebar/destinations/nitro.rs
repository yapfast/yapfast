//! The Nitro screen: what Nitro gives, which plan the account is on, the
//! boosts it has put into servers, and Free against Nitro side by side.
//! Subscribing needs Discord's checkout, so its buttons open discord.com.

use iced::{
    Alignment, Element, Length,
    alignment::Horizontal,
    widget::{Column, Space, column, container, progress_bar, row, text},
};

use super::{GAP, NITRO_URL, NitroTab, columns, fade, frame, grid, handoff, heading, pill, room};
use crate::{
    app::Message,
    controls::{Kind, action, bar},
    ctx::Ctx,
    fonts::{BOLD, SEMIBOLD},
    pictures::Icon,
    settings::Section,
    style::{hex, round, rounded},
};

/// The boosts that lift a server to its next level, from level 0.
const LEVEL_BOOSTS: [u32; 3] = [2, 7, 14];
const METER: f32 = 8.0;
/// The least a perk's card is wide, and how many stand across.
const PERK_LEAST: f32 = 300.0;
const PERKS: usize = 3;
/// A column of the comparison, between the perk and the plans.
const PLAN: f32 = 120.0;

pub(super) fn screen<'a>(cx: Ctx<'a>) -> Element<'a, Message> {
    let width = room(cx);
    let body = match cx.pages.nitro {
        NitroTab::Home => home(cx, width),
        NitroTab::Perks => perks(cx),
        NitroTab::Plans => plans(cx),
        NitroTab::Compare => compare(cx),
    };
    frame(cx, body)
}

/// A card: `content` on the raised ground, 16 px in.
fn card<'a>(cx: Ctx<'a>, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let ground = cx.look.theme.raised;
    container(content)
        .padding(16)
        .width(Length::Fill)
        .style(move |_| rounded(ground, 12))
        .into()
}

/// The page's first screen: the offer on the purple banner and what it
/// holds.
fn home<'a>(cx: Ctx<'a>, width: f32) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let nitro = cx.app.discord_nitro();
    let mut offer = column![
        text("Unlock a world of perks with Nitro".to_uppercase())
            .size(if width < 560.0 { 28 } else { 44 })
            .font(BOLD)
            .color(theme.channel_white)
            .width(Length::Fill)
            .align_x(Horizontal::Center),
    ]
    .spacing(18)
    .align_x(Alignment::Center)
    .width(Length::Fill);
    if nitro {
        offer = offer.push(pill(cx, "Nitro is active on your account"));
    }
    let main: Element<'a, Message> = if nitro {
        action(
            theme,
            "Manage your subscription",
            Kind::Plain,
            Some(Message::Settings(crate::settings::Event::Open(
                Section::Account,
            ))),
        )
        .into()
    } else {
        handoff(cx, "Upgrade", NITRO_URL, Kind::Primary)
    };
    offer = offer
        .push(
            row![main, handoff(cx, "Gift Nitro", NITRO_URL, Kind::Plain)]
                .spacing(12)
                .align_y(Alignment::Center),
        )
        .push(
            text("Discord Nitro is a subscription that renews itself. You can cancel it any time in your settings.")
            .size(12)
            .color(theme.channel_white)
            .align_x(Horizontal::Center),
        );
    let hero = container(offer)
        .padding([48, 24])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(fade(hex(0xa04de0), hex(0x4a3aa7))),
            border: round(16),
            ..container::Style::default()
        });
    let (count, _) = columns(width, PERK_LEAST, PERKS);
    column![
        hero,
        heading(cx, "What Nitro gives you"),
        grid(
            gifts(cx).into_iter().map(|gift| card(cx, gift)).collect(),
            count
        ),
    ]
    .spacing(24)
    .into()
}

/// What Nitro gives, as small cards: an icon, a name and a line.
fn gifts<'a>(cx: Ctx<'a>) -> Vec<Element<'a, Message>> {
    let theme = cx.look.theme;
    [
        (
            Icon::Smile,
            "Emoji everywhere",
            "Use custom and animated emoji in every server and chat.",
        ),
        (
            Icon::Sticker,
            "Stickers everywhere",
            "Send the stickers of your servers in any server.",
        ),
        (
            Icon::Image,
            "Bigger uploads",
            "Send files up to 500 MB, and longer messages.",
        ),
        (
            Icon::Video,
            "Better streams",
            "Share your screen in a higher quality.",
        ),
        (
            Icon::Palette,
            "Your own look",
            "An animated avatar, a profile banner and custom themes.",
        ),
        (
            Icon::Gem,
            "Server Boosts",
            "Two Server Boosts, and a discount on more.",
        ),
    ]
    .into_iter()
    .map(|(icon, name, words)| {
        row![
            cx.look.glyph(icon, 24, theme.accent),
            column![
                text(name).size(16).font(SEMIBOLD).color(theme.text),
                text(words).size(14).color(theme.secondary),
            ]
            .spacing(4)
            .width(Length::Fill),
        ]
        .spacing(14)
        .into()
    })
    .collect()
}

/// How far each server the account is in has come up its levels, and what
/// Yapfast shows of Nitro by itself.
fn perks<'a>(cx: Ctx<'a>) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let servers: Vec<_> = cx
        .state
        .servers
        .iter()
        .filter(|server| server.discord && !server.unavailable)
        .collect();
    let boosted = servers
        .iter()
        .filter(|server| server.boosts.unwrap_or(0) > 0 || server.premium_tier.unwrap_or(0) > 0)
        .count();
    let total: u32 = servers.iter().filter_map(|server| server.boosts).sum();
    let mut page = Column::new().spacing(16).width(Length::Fill);
    page = page.push(heading(cx, "In Yapfast"));
    page = page.push(card(
        cx,
        column![
            text("Profile banners, profile themes, avatar decorations, server profiles, animated emoji and stickers from other servers all work in Yapfast.")
            .size(14)
            .color(theme.secondary),
            text(format!("{} servers loaded, {} of them boosted, {} boosts in all.", servers.len(), boosted, total))
            .size(14)
            .color(theme.secondary),
        ]
        .spacing(8),
    ));
    page = page.push(heading(cx, "Server Boosts"));
    let mut any = false;
    for server in servers
        .iter()
        .filter(|server| server.boosts.is_some() || server.premium_tier.unwrap_or(0) > 0)
        .take(12)
    {
        any = true;
        let tier = server.premium_tier.unwrap_or(0);
        let boosts = server.boosts.unwrap_or(0);
        let target = LEVEL_BOOSTS[usize::from(tier.min(2))];
        let head = row![
            text(server.name.as_str())
                .size(15)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill),
            text(format!("Level {}, {} boosts", tier, boosts))
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
        let meter = progress_bar(0.0..=target as f32, boosts.min(target) as f32)
            .girth(METER)
            .style(move |_| progress_bar::Style {
                background: theme.field.into(),
                bar: theme.accent.into(),
                border: round(METER / 2.0),
            });
        page = page.push(card(cx, column![head, meter].spacing(8)));
    }
    if !any {
        page = page.push(card(
            cx,
            text("None of your servers has a Server Boost yet.")
                .size(14)
                .color(theme.secondary),
        ));
    }
    page.into()
}

/// Which plan the account is on, and the plans to move to.
fn plans<'a>(cx: Ctx<'a>) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let nitro = cx.app.discord_nitro();
    let (name, words) = if nitro {
        (
            "Nitro",
            "Your account has Nitro. Billing and cancelling are in Settings.",
        )
    } else {
        (
            "Free",
            "Your account has no Nitro. The plans below add perks.",
        )
    };
    let status = card(
        cx,
        column![
            text("Your plan")
                .size(12)
                .font(SEMIBOLD)
                .color(theme.secondary),
            text(name).size(24).font(BOLD).color(theme.text),
            text(words).size(14).color(theme.secondary),
        ]
        .spacing(4),
    );
    let plan = |name: &'static str, words: &'static str, label: &'static str, kind: Kind| {
        card(
            cx,
            column![
                text(name).size(20).font(BOLD).color(theme.text),
                text(words).size(14).color(theme.secondary),
                Space::new().height(8),
                handoff(cx, label, NITRO_URL, kind),
            ]
            .spacing(6)
            .width(Length::Fill),
        )
    };
    column![
        status,
        row![
            plan(
                "Nitro Basic",
                "A lighter plan with custom emoji and bigger uploads.",
                "See Nitro Basic",
                Kind::Plain,
            ),
            plan(
                "Nitro",
                "Every perk, with Server Boosts and a look of your own.",
                "Get Nitro",
                Kind::Primary,
            ),
        ]
        .spacing(GAP),
        text("Discord shows the price for your country when you subscribe.")
            .size(13)
            .color(theme.secondary),
    ]
    .spacing(16)
    .into()
}

/// A cell of the comparison.
enum Cell {
    Yes,
    No,
    Words(&'static str),
}

/// Free and Nitro side by side.
fn compare<'a>(cx: Ctx<'a>) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let cell = |cell: &Cell| -> Element<'a, Message> {
        let content: Element<'a, Message> = match cell {
            Cell::Yes => cx.look.glyph(Icon::Check, 18, theme.live).into(),
            Cell::No => cx.look.glyph(Icon::Minus, 18, theme.faint).into(),
            Cell::Words(words) => text(*words).size(14).color(theme.text).into(),
        };
        container(content).center_x(PLAN).into()
    };
    let rows: [(&'static str, Cell, Cell); 8] = [
        ("Custom emoji in every server", Cell::No, Cell::Yes),
        ("Stickers from every server", Cell::No, Cell::Yes),
        ("Upload size", Cell::Words("10 MB"), Cell::Words("500 MB")),
        ("Message length", Cell::Words("2,000"), Cell::Words("4,000")),
        ("Animated avatar and profile banner", Cell::No, Cell::Yes),
        ("Custom client themes", Cell::No, Cell::Yes),
        ("Higher stream quality", Cell::No, Cell::Yes),
        ("Server Boosts", Cell::No, Cell::Words("2, and a discount")),
    ];
    let head = row![
        Space::new().width(Length::Fill),
        container(text("Free").size(14).font(SEMIBOLD).color(theme.secondary)).center_x(PLAN),
        container(text("Nitro").size(14).font(SEMIBOLD).color(theme.text)).center_x(PLAN),
    ]
    .align_y(Alignment::Center);
    let mut table = Column::new().push(head);
    for (perk, free, nitro) in &rows {
        table = table.push(bar(theme.line, Length::Fill, 1)).push(
            container(
                row![
                    text(*perk).size(14).color(theme.text).width(Length::Fill),
                    cell(free),
                    cell(nitro),
                ]
                .align_y(Alignment::Center),
            )
            .padding([12, 0]),
        );
    }
    card(cx, table)
}
