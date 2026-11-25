//! The Nitro, Shop and Quests screens on the DM home: opened in Yapfast in
//! place of a conversation, as Discord's are. The Quests and the Shop list
//! what Discord offers, with its pictures, and Quests are accepted here;
//! what needs Discord's own checkout (paying, subscribing, claiming a
//! reward, which may ask for a captcha) opens discord.com from a button.

use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding, Point, Size, Task, border,
    gradient,
    widget::{Column, Space, button, container, row, stack, text},
};
use yapfast_core::application::{Action, DmScreen};

use crate::{
    app::{App, Message},
    controls::{self, Kind, action, bar},
    ctx::Ctx,
    fonts::{MEDIUM, SEMIBOLD},
    pictures::Icon,
    style::{hex, rounded, scrollbar},
    widgets::{Pic, scroll_view},
};

mod nitro;
mod quests;
mod shop;

pub const NITRO_URL: &str = "https://discord.com/nitro";
pub const SHOP_URL: &str = "https://discord.com/shop";
pub const QUESTS_URL: &str = "https://discord.com/discovery/quests";

/// The search field's id in the Shop's header.
pub const SEARCH: &str = "shop-search";

/// The room each side of a page, the widest its content gets, and the
/// room between its cards.
const SIDE: f32 = 24.0;
const MOST: f32 = 1090.0;
const GAP: f32 = 16.0;

/// The tab each screen shows, and the Shop's search and category.
#[derive(Default)]
pub struct Pages {
    pub quests: QuestTab,
    pub shop: ShopTab,
    pub nitro: NitroTab,
    /// What the Shop's search field holds.
    pub search: String,
    /// The category the Shop's Browse tab shows alone, by its SKU.
    pub category: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuestTab {
    #[default]
    All,
    Claimed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShopTab {
    #[default]
    Featured,
    Browse,
    Orbs,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NitroTab {
    #[default]
    Home,
    Perks,
    Plans,
    Compare,
}

#[derive(Clone, Debug)]
pub enum Event {
    Quests(QuestTab),
    Shop(ShopTab),
    Nitro(NitroTab),
    /// A category of the Shop, shown alone in Browse; `None` shows them all.
    Category(Option<String>),
    Search(String),
    /// Accepts the quest with this id.
    Accept(String),
    /// Asks Discord for the open screen's list again.
    Refresh,
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Quests(tab) => app.pages.quests = tab,
        Event::Shop(tab) => {
            app.pages.shop = tab;
            app.pages.category = None;
            app.pages.search.clear();
        }
        Event::Category(category) => {
            app.pages.shop = ShopTab::Browse;
            app.pages.category = category;
            app.pages.search.clear();
        }
        Event::Nitro(tab) => app.pages.nitro = tab,
        Event::Search(query) => app.pages.search = query,
        Event::Accept(id) => app.client.dispatch(Action::AcceptQuest(id)),
        Event::Refresh => match crate::friends::dm_screen(app) {
            Some(DmScreen::Quests) => app.client.dispatch(Action::RefreshQuests),
            Some(DmScreen::Shop) => app.client.dispatch(Action::RefreshShop),
            _ => {}
        },
    }
    let synced = app.sync();
    Task::batch([synced, app.ask()])
}

/// The screen for `screen`, below its header.
pub fn page<'a>(cx: Ctx<'a>, screen: DmScreen) -> Element<'a, Message> {
    match screen {
        DmScreen::Nitro => nitro::screen(cx),
        DmScreen::Shop => shop::screen(cx),
        _ => quests::screen(cx),
    }
}

/// The pictures the open screen shows, which the window asks for.
pub fn pictures(app: &App) -> Vec<String> {
    let state = app.client.state();
    match crate::friends::dm_screen(app) {
        Some(DmScreen::Quests) => quests::pictures(&state.quests, app.pages.quests),
        Some(DmScreen::Shop) => shop::pictures(&state.shop, &app.pages),
        _ => Vec::new(),
    }
}

/// A header tab: its label, whether it is the open one, and what it opens.
type Tab = (&'static str, bool, Event);

/// The header over a screen: its icon, its tabs as underlined words, and
/// what Discord puts at its end.
pub fn header<'a>(cx: Ctx<'a>, screen: DmScreen) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let pages = cx.pages;
    let (icon, tabs, end): (Icon, Vec<Tab>, Element<'a, Message>) = match screen {
        DmScreen::Nitro => (
            Icon::Gem,
            [
                ("Home", NitroTab::Home),
                ("Perks", NitroTab::Perks),
                ("Plans", NitroTab::Plans),
                ("Compare", NitroTab::Compare),
            ]
            .into_iter()
            .map(|(label, tab)| (label, pages.nitro == tab, Event::Nitro(tab)))
            .collect(),
            action(
                theme,
                "Gift Nitro",
                Kind::Plain,
                Some(Message::Link(NITRO_URL.into(), Point::ORIGIN)),
            )
            .small()
            .into(),
        ),
        DmScreen::Shop => (
            Icon::Shop,
            [
                ("Featured", ShopTab::Featured),
                ("Browse", ShopTab::Browse),
                ("Orbs Shop", ShopTab::Orbs),
            ]
            .into_iter()
            .map(|(label, tab)| (label, pages.shop == tab, Event::Shop(tab)))
            .collect(),
            shop::header_end(cx),
        ),
        _ => (
            Icon::Trophy,
            [
                ("All Quests", QuestTab::All),
                ("Claimed Quests", QuestTab::Claimed),
            ]
            .into_iter()
            .map(|(label, tab)| (label, pages.quests == tab, Event::Quests(tab)))
            .collect(),
            Space::new().into(),
        ),
    };
    let mut head = row![container(cx.look.glyph(icon, 22, theme.channel_read)).padding([0, 8])]
        .align_y(Alignment::Center);
    for (label, chosen, event) in tabs {
        head = head.push(tab(cx, label, chosen, event));
    }
    head = head.push(Space::new().width(Length::Fill)).push(end);
    container(head)
        .padding([0, 16])
        .width(Length::Fill)
        .height(crate::header::HEIGHT)
        .align_y(Alignment::Center)
        .into()
}

/// A tab of the header: its word, and under it a line while it's the one
/// shown.
fn tab<'a>(cx: Ctx<'a>, label: &'static str, chosen: bool, event: Event) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let (ink, secondary) = (theme.text, theme.secondary);
    let word = container(text(label).size(14).font(MEDIUM))
        .padding([0, 12])
        .center_y(Length::Fill);
    let line = container(bar(
        if chosen { ink } else { Color::TRANSPARENT },
        Length::Fill,
        2,
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(Alignment::End);
    button(stack![word, line])
        .padding(0)
        .height(crate::header::HEIGHT)
        .on_press(Message::Pages(event))
        .style(move |_, status| button::Style {
            background: None,
            text_color: if chosen || controls::lights(status) {
                ink
            } else {
                secondary
            },
            border: Border::default(),
            ..Default::default()
        })
        .into()
}

/// How wide the page's content is: the window less the navigation and the
/// room each side, at most [`MOST`].
fn room(cx: Ctx<'_>) -> f32 {
    (cx.look.window.width - crate::conversation::navigation_ctx(cx) - 2.0 * SIDE)
        .clamp(280.0, MOST)
        .floor()
}

/// How many cards of at least `least` px fit across `room`, up to `most`,
/// and how wide each then is.
fn columns(room: f32, least: f32, most: usize) -> (usize, f32) {
    let count = (((room + GAP) / (least + GAP)).floor() as usize).clamp(1, most);
    let width = ((room - GAP * (count - 1) as f32) / count as f32).floor();
    (count, width)
}

/// `content` in the page's scrolling column, centred and [`room`] wide.
fn frame<'a>(cx: Ctx<'a>, content: Element<'a, Message>) -> Element<'a, Message> {
    scroll_view(
        container(container(content).width(room(cx)))
            .padding(Padding {
                top: SIDE,
                right: SIDE,
                bottom: 2.0 * SIDE,
                left: SIDE,
            })
            .center_x(Length::Fill),
    )
    .style(scrollbar(cx.look.theme))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// `items` in rows of `columns`, each row's cards [`GAP`] apart.
fn grid<'a>(items: Vec<Element<'a, Message>>, columns: usize) -> Element<'a, Message> {
    let mut rows = Column::new().spacing(GAP);
    let mut items = items.into_iter();
    loop {
        let line: Vec<Element<'a, Message>> = items.by_ref().take(columns).collect();
        if line.is_empty() {
            break;
        }
        rows = rows.push(row(line).spacing(GAP));
    }
    rows.into()
}

/// A rounded rectangle's corners, as Discord's cards round only the top of
/// a picture.
fn corners(radii: [f32; 4]) -> border::Radius {
    border::Radius {
        top_left: radii[0],
        top_right: radii[1],
        bottom_right: radii[2],
        bottom_left: radii[3],
    }
}

/// A gradient from `from` to `to`, left to right.
fn fade(from: Color, to: Color) -> Background {
    Background::Gradient(
        gradient::Linear::new(std::f32::consts::FRAC_PI_2)
            .add_stop(0.0, from)
            .add_stop(1.0, to)
            .into(),
    )
}

/// The picture at `url` cut to cover `size`, with `radii` for its corners;
/// until it has come, or without one, a gradient between `colors`, or the
/// theme's greys.
fn cover<'a>(
    cx: Ctx<'a>,
    url: Option<&str>,
    size: Size,
    radii: [f32; 4],
    colors: Option<[u32; 2]>,
) -> Element<'a, Message> {
    if let Some(handle) =
        url.and_then(|url| cx.picture(url, size.width as u32, size.height as u32, true))
    {
        return Pic::new(handle, size).radius_each(radii).into();
    }
    let theme = cx.look.theme;
    let background = match colors {
        Some([from, to]) => fade(hex(from), hex(to)),
        None => fade(theme.raised, theme.sunken),
    };
    container(Space::new())
        .width(size.width)
        .height(size.height)
        .style(move |_| container::Style {
            background: Some(background),
            border: Border {
                radius: corners(radii),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// The picture at `url` shrunk to fit within `within`, keeping its shape;
/// `None` until it has come.
fn contain<'a>(
    cx: Ctx<'a>,
    url: Option<&str>,
    within: Size,
    radius: f32,
) -> Option<Element<'a, Message>> {
    let url = url?;
    let source = cx.state.pictures.get(url)?;
    if source.width == 0 || source.height == 0 {
        return None;
    }
    let scale = (within.width / source.width as f32).min(within.height / source.height as f32);
    let width = (source.width as f32 * scale).round().max(1.0);
    let height = (source.height as f32 * scale).round().max(1.0);
    let handle = cx.picture(url, width as u32, height as u32, true)?;
    Some(
        Pic::new(handle, Size::new(width, height))
            .radius(radius)
            .into(),
    )
}

/// The small label on a card or banner: a rounded plate behind its words.
fn pill<'a>(cx: Ctx<'a>, label: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let ground = Color {
        a: 0.6,
        ..Color::BLACK
    };
    container(
        text(label)
            .size(12)
            .font(SEMIBOLD)
            .color(theme.channel_white),
    )
    .padding([3, 8])
    .style(move |_| rounded(ground, 6))
    .into()
}

/// A section's heading.
fn heading<'a>(cx: Ctx<'a>, label: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    text(label)
        .size(20)
        .font(SEMIBOLD)
        .color(cx.look.theme.text)
        .into()
}

/// A button that opens discord.com's page for what only Discord's own
/// checkout does.
fn handoff<'a>(
    cx: Ctx<'a>,
    label: impl Into<std::borrow::Cow<'a, str>>,
    url: impl Into<String>,
    kind: Kind,
) -> Element<'a, Message> {
    action(
        cx.look.theme,
        label,
        kind,
        Some(Message::Link(url.into(), Point::ORIGIN)),
    )
    .into()
}

/// What a screen says while its list loads, or when it can't: the same
/// words on every screen, with Retry when it failed.
fn status<'a>(
    cx: Ctx<'a>,
    icon: Icon,
    loading: bool,
    error: Option<&'a str>,
) -> Element<'a, Message> {
    let offer = Some(("Retry", Message::Pages(Event::Refresh)));
    let screen = match (loading, error) {
        (true, _) => cx
            .look
            .empty_screen(icon, "Loading", "Asking Discord for the list.", None),
        (false, Some(error)) => {
            cx.look
                .empty_screen(icon, "Could not load this page", error, offer)
        }
        (false, None) if cx.state.preview || !cx.state.discord_online => cx.look.empty_screen(
            icon,
            "Link your Discord account",
            "This page shows what Discord offers your account.",
            None,
        ),
        (false, None) => cx.look.empty_screen(
            icon,
            "Nothing here yet",
            "Discord has not listed anything for this page.",
            offer,
        ),
    };
    container(screen)
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .into()
}

/// `n` as a count of minutes, "15 minutes", from a task's seconds.
fn minutes(seconds: u64) -> String {
    let minutes = seconds.div_ceil(60).max(1);
    if minutes == 1 {
        "1 minute".to_owned()
    } else {
        format!("{} minutes", minutes)
    }
}
