//! The Discover screen: the servers anyone may join, as Discord's Discover
//! page lists them. A search over their names and words (asked of Discord
//! when it is submitted), Discord's categories as chips, a card for each
//! server with its picture, name, words and numbers, a Join button that
//! joins it here, or Open for a server the account is in, and Show more for
//! the next page.

use iced::{
    Alignment, Element, Length, Size,
    widget::{Column, Space, column, container, row, text},
};
use yapfast_core::{
    discord::server_id,
    emoji::card::grouped,
    i18n::{fill, tr},
    storefront::{DiscoverPage, DiscoverServer},
};

use super::{DISCOVER_SEARCH, Event, Pages, columns, cover, frame, grid, room, status};
use crate::{
    app::Message,
    controls::{self, Kind, action},
    ctx::Ctx,
    fonts::BOLD,
    pictures::Icon,
    style::rounded,
};

/// A card's picture strip, the server's icon on it, and the least a card is
/// wide, and how many stand across.
const STRIP: f32 = 110.0;
const ICON: f32 = 48.0;
const CARD_LEAST: f32 = 280.0;
const CARDS: usize = 3;
/// How tall a card's words are, two lines.
const WORDS: f32 = 40.0;
/// The size of the dot before a count.
const DOT: f32 = 8.0;
/// The most pictures the window asks for at once.
const PICTURES: usize = 96;

fn search<'a>(cx: Ctx<'a>, page: &'a DiscoverPage) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let field = controls::field(
        theme,
        DISCOVER_SEARCH,
        tr("Search for servers"),
        &cx.pages.discover,
        false,
        true,
    )
    .size(14)
    .padding([6, 10])
    .width(Length::Fill)
    .on_input(|query| Message::Pages(Event::Discover(query)))
    .on_submit(Message::Pages(Event::Find));
    let mut toolbar = row![
        field,
        action(
            theme,
            tr("Search"),
            Kind::Primary,
            Some(Message::Pages(Event::Find))
        )
        .small(),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if !cx.pages.discover.is_empty() || !page.query.is_empty() {
        toolbar = toolbar.push(
            action(
                theme,
                tr("Clear search"),
                Kind::Plain,
                Some(Message::Pages(Event::ClearDiscover)),
            )
            .small(),
        );
    }
    toolbar.into()
}

pub(super) fn screen<'a>(cx: Ctx<'a>) -> Element<'a, Message> {
    let page: &'a DiscoverPage = &cx.state.discover;
    if !page.loaded {
        return status(cx, Icon::Compass, page.loading, page.error.as_deref());
    }
    let theme = cx.look.theme;
    let width = room(cx);
    let mut content = Column::new().spacing(20).width(Length::Fill);
    content = content.push(search(cx, page));
    if !page.categories.is_empty() {
        content = content.push(category_filters(cx, page));
    }
    if let Some(error) = &page.error {
        content = content.push(
            row![
                text(fill(tr("Could not refresh the list: {}"), &[error]))
                    .size(14)
                    .color(theme.danger)
                    .width(Length::Fill),
                action(
                    theme,
                    tr("Retry"),
                    Kind::Plain,
                    Some(Message::Pages(Event::Refresh)),
                )
                .small(),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        );
    }
    if let Some(refused) = &page.refused {
        content = content.push(text(refused.as_str()).size(14).color(theme.danger));
    }
    if page.servers.is_empty() {
        if page.loading {
            content = content.push(text(tr("Loading")).size(14).color(theme.secondary));
        } else if page.error.is_none() {
            content = content.push(text(empty(page)).size(14).color(theme.secondary));
        }
        return frame(cx, content.into());
    }
    let (count, each) = columns(width, CARD_LEAST, CARDS);
    content = content.push(grid(
        page.servers
            .iter()
            .map(|server| card(cx, server, each))
            .collect(),
        count,
    ));
    if page.more() {
        content = content.push(
            container(action(
                theme,
                if page.loading {
                    tr("Loading")
                } else {
                    tr("Show more")
                },
                Kind::Plain,
                (!page.loading).then_some(Message::Pages(Event::MoreServers)),
            ))
            .center_x(Length::Fill),
        );
    }
    frame(cx, content.into())
}

/// What the list says when Discord has no server for what was asked.
fn empty(page: &DiscoverPage) -> String {
    if page.query.is_empty() {
        tr("Discord lists no servers here at the moment.").to_owned()
    } else {
        fill(tr("No server matches {}."), &[&page.query])
    }
}

/// Keep the grid visible above the fold until the category choices are needed.
fn category_filters<'a>(cx: Ctx<'a>, page: &'a DiscoverPage) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let expanded = cx.pages.discover_categories;
    let selected = page
        .categories
        .iter()
        .find(|category| Some(category.id) == page.category)
        .map_or_else(|| tr("All"), |category| category_label(&category.name));
    let label = if expanded {
        tr("Hide categories")
    } else {
        tr("Show categories")
    };
    let mut filters = column![
        row![
            action(
                theme,
                label,
                Kind::Plain,
                Some(Message::Pages(Event::ToggleDiscoverCategories)),
            )
            .small(),
            text(selected).size(14).color(theme.secondary),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
    ]
    .spacing(12);
    if expanded {
        filters = filters.push(chips(cx, page));
    }
    filters.into()
}

/// Available server categories with a visible selection.
fn chips<'a>(cx: Ctx<'a>, page: &'a DiscoverPage) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let chip = |label: &'a str, chosen: bool, category: Option<u32>| {
        action(
            theme,
            label,
            if chosen { Kind::Primary } else { Kind::Plain },
            Some(Message::Pages(Event::Within(category))),
        )
        .small()
        .into()
    };
    let mut all: Vec<Element<'a, Message>> = vec![chip(tr("All"), page.category.is_none(), None)];
    all.extend(page.categories.iter().map(|category| {
        chip(
            category_label(&category.name),
            page.category == Some(category.id),
            Some(category.id),
        )
    }));
    row(all).spacing(8).wrap().vertical_spacing(8).into()
}

/// Localize known category names and retain new names supplied by the server.
fn category_label(name: &str) -> &str {
    match name {
        "Anime & Manga" => tr("Anime & Manga"),
        "Art" => tr("Art"),
        "Books" => tr("Books"),
        "Bots" => tr("Bots"),
        "Business" => tr("Business"),
        "Charity & Nonprofit" => tr("Charity & Nonprofit"),
        "Collaboration" => tr("Collaboration"),
        "Comics & Cartoons" => tr("Comics & Cartoons"),
        "Console" => tr("Console"),
        "Content Creator" => tr("Content Creator"),
        "Crafts, DIY, & Making" => tr("Crafts, DIY, & Making"),
        "Creative Arts" => tr("Creative Arts"),
        "Cryptocurrency" => tr("Cryptocurrency"),
        "Customer Support" => tr("Customer Support"),
        "Education" => tr("Education"),
        "Emoji" => tr("Emoji"),
        "Entertainment" => tr("Entertainment"),
        "Esports" => tr("Esports"),
        "Events" => tr("Events"),
        "Fandom" => tr("Fandom"),
        "Fashion & Beauty" => tr("Fashion & Beauty"),
        "Finance" => tr("Finance"),
        "Fitness & Health" => tr("Fitness & Health"),
        "Game Developer" => tr("Game Developer"),
        "Gaming" => tr("Gaming"),
        "General" => tr("General"),
        "General Chatting" => tr("General Chatting"),
        "Investing" => tr("Investing"),
        "LFG" => tr("LFG"),
        "Local Group" => tr("Local Group"),
        "Memes" => tr("Memes"),
        "Mobile" => tr("Mobile"),
        "Movies & TV" => tr("Movies & TV"),
        "Music" => tr("Music"),
        "News & Current Events" => tr("News & Current Events"),
        "Other" => tr("Other"),
        "Podcasts" => tr("Podcasts"),
        "Programming" => tr("Programming"),
        "Relationships & Identity" => tr("Relationships & Identity"),
        "Roleplay" => tr("Roleplay"),
        "Science & Tech" => tr("Science & Tech"),
        "Sports" => tr("Sports"),
        "Studying & Teaching" => tr("Studying & Teaching"),
        "Subreddit" => tr("Subreddit"),
        "Tabletop Games" => tr("Tabletop Games"),
        "Theorycraft" => tr("Theorycraft"),
        "Travel & Food" => tr("Travel & Food"),
        "Wiki & Guide" => tr("Wiki & Guide"),
        "Writing" => tr("Writing"),
        _ => name,
    }
}

/// The pictures the open list shows.
pub(super) fn pictures(page: &DiscoverPage, _pages: &Pages) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    for server in &page.servers {
        urls.extend(server.splash.clone());
        urls.extend(server.icon.clone());
    }
    urls.truncate(PICTURES);
    urls
}

/// A small count with its dot: green for those online, grey for all.
fn count<'a>(cx: Ctx<'a>, number: u64, online: bool, words: &'static str) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let colour = if online { theme.live } else { theme.faint };
    row![
        container(Space::new())
            .width(DOT)
            .height(DOT)
            .style(move |_| rounded(colour, DOT / 2.0)),
        text(fill(words, &[&grouped(number)]))
            .size(12)
            .color(theme.secondary),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

/// A server's card: its picture, its icon, its name and category, its
/// words, how many are in it and online, and the button that joins it, or
/// opens it when the account is in it.
fn card<'a>(cx: Ctx<'a>, server: &'a DiscoverServer, width: f32) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let page = &cx.state.discover;
    let strip = cover(
        cx,
        server.splash.as_deref(),
        Size::new(width, STRIP),
        [12.0, 12.0, 0.0, 0.0],
        None,
    );
    let icon = cx.avatar(server.icon.as_deref(), &server.name, ICON, 12.0, 1.0);
    let mut names = column![
        text(server.name.as_str())
            .size(16)
            .font(BOLD)
            .color(theme.text)
    ]
    .spacing(2)
    .width(Length::Fill);
    if let Some(category) = &server.category {
        names = names.push(
            text(category_label(category))
                .size(12)
                .color(theme.secondary),
        );
    }
    let mut body = column![row![icon, names].spacing(12).align_y(Alignment::Center),]
        .spacing(10)
        .padding(16)
        .width(Length::Fill);
    body = body.push(
        container(
            text(server.description.as_deref().unwrap_or_default())
                .size(14)
                .color(theme.secondary),
        )
        .height(WORDS)
        .clip(true),
    );
    let mut numbers = row![].spacing(12).align_y(Alignment::Center);
    if let Some(online) = server.online {
        numbers = numbers.push(count(cx, online, true, tr("{} Online")));
    }
    if let Some(members) = server.members {
        numbers = numbers.push(count(cx, members, false, tr("{} Members")));
    }
    body = body.push(numbers).push(join(cx, page, server));
    let raised = theme.raised;
    container(column![strip, body])
        .width(width)
        .style(move |_| rounded(raised, 12))
        .into()
}

/// A card's button: Open for a server the account is in, Joining while
/// Discord answers, otherwise Join.
fn join<'a>(cx: Ctx<'a>, page: &DiscoverPage, server: &DiscoverServer) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let local = server_id(&server.id);
    let member = cx.state.servers.iter().any(|known| known.id == local);
    let button = if member {
        action(
            theme,
            tr("Open"),
            Kind::Plain,
            Some(Message::Rail(crate::rail::Event::Server(local))),
        )
    } else if page.joining.contains(&server.id) {
        action(theme, tr("Joining"), Kind::Primary, None)
    } else if page.joined.contains(&server.id) {
        action(theme, tr("Joined"), Kind::Plain, None)
    } else {
        action(
            theme,
            tr("Join"),
            Kind::Primary,
            (cx.state.discord_online && !cx.state.preview)
                .then(|| Message::Pages(Event::Join(server.id.clone()))),
        )
    };
    button.wide(32.0).into()
}
