//! The Shop screen: Discord's categories of avatar decorations, profile
//! effects, nameplates and bundles, with their pictures and prices, in
//! the tabs Featured, Browse and Orbs Shop, and a search over them. A
//! product that costs Orbs is bought here, after a second press to confirm
//! it; Yapfast has no checkout for money, so a product priced only in money
//! is listed with its price and nothing to press.

use iced::{
    Alignment, Border, Color, Element, Length, Size,
    widget::{Column, Space, column, container, row, stack, text},
};
use yapfast_core::{
    i18n::{fill, tr},
    storefront::{Category, Product, ProductKind, ShopPage},
};

use super::{
    Event, Pages, SEARCH, ShopTab, columns, contain, corners, cover, fade, frame, grid, heading,
    room, status,
};
use crate::{
    app::Message,
    controls::{self, Kind, action},
    ctx::Ctx,
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    pictures::Icon,
    style::rounded,
};

/// The least a product's card is wide, and how many stand across.
const CARD_LEAST: f32 = 180.0;
const CARDS: usize = 4;
/// Featured is a preview. Browse and Search cover the complete catalog.
const FEATURED_CATEGORIES: usize = 8;
/// The room a product's picture has in its card.
const ART: f32 = 120.0;
/// The most pictures the window asks for at once.
const PICTURES: usize = 120;

/// What the header shows at its end: the search field, and the Orbs the
/// account has when Discord said.
pub(super) fn header_end<'a>(cx: Ctx<'a>) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let search = controls::field(
        theme,
        SEARCH,
        tr("Search the Shop"),
        &cx.pages.search,
        false,
        true,
    )
    .size(14)
    .padding([6, 10])
    .width(220)
    .on_input(|query| Message::Pages(Event::Search(query)));
    let mut end = row![search].spacing(8).align_y(Alignment::Center);
    if let Some(orbs) = cx.state.shop.orbs {
        let ground = theme.raised;
        end = end.push(
            container(
                row![
                    cx.look.glyph(Icon::Orbs, 16, theme.text),
                    text(orbs.to_string())
                        .size(14)
                        .font(SEMIBOLD)
                        .color(theme.text),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([6, 12])
            .style(move |_| rounded(ground, 6)),
        );
    }
    end.into()
}

pub(super) fn screen<'a>(cx: Ctx<'a>) -> Element<'a, Message> {
    let page: &'a ShopPage = &cx.state.shop;
    if !page.loaded {
        return status(cx, Icon::Shop, page.loading, page.error.as_deref());
    }
    if page.categories.is_empty() {
        return container(cx.look.empty_screen(
            Icon::Shop,
            tr("The Shop is empty"),
            tr("Discord lists nothing to buy for your account at the moment."),
            Some((tr("Refresh"), Message::Pages(Event::Refresh))),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .into();
    }
    let width = room(cx);
    let theme = cx.look.theme;
    let mut content = Column::new().spacing(32).width(Length::Fill);
    if let Some(error) = &page.error {
        content = content.push(
            row![
                text(fill(tr("Could not refresh the Shop: {}"), &[error]))
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
    let query = cx.pages.search.trim();
    if !query.is_empty() {
        let found = page.search(query);
        if found.is_empty() {
            content = content.push(note(
                cx,
                fill(tr("Nothing in the Shop matches {}."), &[&query]),
            ));
        } else {
            content = content
                .push(heading(cx, fill(tr("Results for {}"), &[&query])))
                .push(products(cx, found, width, false));
        }
    } else {
        content = match cx.pages.shop {
            ShopTab::Featured => featured(cx, content, page, width),
            ShopTab::Browse => browse(cx, content, page, width, cx.pages.category.as_deref()),
            ShopTab::Orbs => orbs(cx, content, page, width),
        };
    }
    if let Some(refused) = &page.refused {
        content = content.push(text(refused.as_str()).size(14).color(theme.danger));
    }
    content = content.push(note(
        cx,
        tr("Products that cost Orbs are bought here. Products priced in money are for looking at.")
            .to_owned(),
    ));
    frame(cx, content.into())
}

/// The pictures the open tab shows.
pub(super) fn pictures(page: &ShopPage, pages: &Pages) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    let query = pages.search.trim();
    if !query.is_empty() {
        urls.extend(
            page.search(query)
                .into_iter()
                .filter_map(|p| p.picture.clone()),
        );
    } else {
        match pages.shop {
            ShopTab::Featured => {
                for (index, category) in
                    page.categories.iter().take(FEATURED_CATEGORIES).enumerate()
                {
                    // Only the first category is drawn as a banner: the
                    // others' banners are large and would only be fetched.
                    if index == 0 {
                        urls.extend(category.cover().map(str::to_owned));
                        urls.extend(category.logo.clone());
                    }
                    urls.extend(
                        category
                            .products
                            .iter()
                            .take(CARDS)
                            .filter_map(|p| p.picture.clone()),
                    );
                }
            }
            ShopTab::Browse => {
                let chosen = pages.category.as_deref();
                for category in page
                    .categories
                    .iter()
                    .filter(|c| chosen.is_none_or(|sku| c.sku == sku))
                {
                    if chosen.is_some() {
                        urls.extend(category.cover().map(str::to_owned));
                        urls.extend(category.logo.clone());
                    }
                    urls.extend(category.products.iter().filter_map(|p| p.picture.clone()));
                }
            }
            ShopTab::Orbs => urls.extend(
                page.categories
                    .iter()
                    .flat_map(|category| &category.products)
                    .filter(|product| product.orbs.is_some())
                    .filter_map(|product| product.picture.clone()),
            ),
        }
    }
    urls.truncate(PICTURES);
    urls
}

/// A line of small words on the page.
fn note<'a>(cx: Ctx<'a>, words: String) -> Element<'a, Message> {
    text(words).size(13).color(cx.look.theme.secondary).into()
}

/// The first category as the wide banner, its first products beneath, and
/// a selection of categories as shelves.
fn featured<'a>(
    cx: Ctx<'a>,
    content: Column<'a, Message>,
    page: &'a ShopPage,
    width: f32,
) -> Column<'a, Message> {
    let mut content = content;
    for (index, category) in page.categories.iter().take(FEATURED_CATEGORIES).enumerate() {
        if index == 0 {
            content = content.push(banner(cx, category, width, false));
        }
        let shelf = shelf(cx, category, width, false, false, index > 0);
        if let Some(shelf) = shelf {
            content = content.push(shelf);
        }
    }
    content
}

/// Every category's products, or one category alone with its banner.
fn browse<'a>(
    cx: Ctx<'a>,
    content: Column<'a, Message>,
    page: &'a ShopPage,
    width: f32,
    chosen: Option<&str>,
) -> Column<'a, Message> {
    let mut content = content;
    if let Some(category) = chosen.and_then(|sku| page.categories.iter().find(|c| c.sku == sku)) {
        content = content
            .push(
                action(
                    cx.look.theme,
                    tr("All categories"),
                    Kind::Ghost,
                    Some(Message::Pages(Event::Category(None))),
                )
                .small(),
            )
            .push(banner(cx, category, width, true));
        let all: Vec<&Product> = category.products.iter().collect();
        if !all.is_empty() {
            content = content.push(products(cx, all, width, false));
        }
        return content;
    }
    for category in &page.categories {
        if let Some(shelf) = shelf(cx, category, width, true, false, true) {
            content = content.push(shelf);
        }
    }
    content
}

/// The products Orbs buy, by category.
fn orbs<'a>(
    cx: Ctx<'a>,
    content: Column<'a, Message>,
    page: &'a ShopPage,
    width: f32,
) -> Column<'a, Message> {
    let mut content = content;
    let mut any = false;
    for category in &page.categories {
        if let Some(shelf) = shelf(cx, category, width, true, true, true) {
            any = true;
            content = content.push(shelf);
        }
    }
    if !any {
        content = content.push(note(
            cx,
            tr("No product costs Orbs at the moment.").to_owned(),
        ));
    }
    content
}

/// A category's heading with a button into it, and its products in a
/// row, or all of them with `all`; `orbs` keeps those Orbs buy. `None`
/// when it has none to show.
fn shelf<'a>(
    cx: Ctx<'a>,
    category: &'a Category,
    width: f32,
    all: bool,
    orbs: bool,
    titled: bool,
) -> Option<Element<'a, Message>> {
    let (count, _) = columns(width, CARD_LEAST, CARDS);
    let list: Vec<&'a Product> = category
        .products
        .iter()
        .filter(|product| !orbs || product.orbs.is_some())
        .take(if all { usize::MAX } else { count })
        .collect();
    if list.is_empty() {
        return None;
    }
    let mut shelf = Column::new().spacing(16).width(Length::Fill);
    if titled {
        let mut head = row![
            heading(cx, category.name.as_str()),
            Space::new().width(Length::Fill)
        ]
        .align_y(Alignment::Center);
        if !all {
            head = head.push(
                action(
                    cx.look.theme,
                    fill(tr("Discover {}"), &[&category.name]),
                    Kind::Plain,
                    Some(Message::Pages(Event::Category(Some(category.sku.clone())))),
                )
                .small(),
            );
        }
        shelf = shelf.push(head);
    }
    Some(
        shelf
            .push(products(cx, list, width, orbs || category.orbs_only))
            .into(),
    )
}

/// Products in a grid of cards.
fn products<'a>(
    cx: Ctx<'a>,
    list: Vec<&'a Product>,
    width: f32,
    orbs: bool,
) -> Element<'a, Message> {
    let (count, each) = columns(width, CARD_LEAST, CARDS);
    grid(
        list.into_iter()
            .map(|product| card(cx, product, each, orbs))
            .collect(),
        count,
    )
}

/// What a kind of product is called.
fn kind_label(kind: ProductKind) -> &'static str {
    match kind {
        ProductKind::Decoration => tr("Avatar decoration"),
        ProductKind::Effect => tr("Profile effect"),
        ProductKind::Nameplate => tr("Nameplate"),
        ProductKind::Frame => tr("Profile frame"),
        ProductKind::Bundle => tr("Bundle"),
        ProductKind::Other => "",
    }
}

/// A product's card: its picture, its name and kind, and its price, in
/// money or in Orbs. One that costs Orbs has a Buy button, which asks to be
/// pressed again to confirm before it spends them.
fn card<'a>(cx: Ctx<'a>, product: &'a Product, width: f32, orbs: bool) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let art = contain(
        cx,
        product.picture.as_deref(),
        Size::new(width - 24.0, ART),
        8.0,
    )
    .unwrap_or_else(|| Space::new().into());
    let sunken = theme.sunken;
    let picture = container(art)
        .center_x(Length::Fill)
        .center_y(ART)
        .style(move |_| rounded(sunken, 8));
    let price: Element<'a, Message> = match (&product.price, product.orbs) {
        (_, Some(cost)) if orbs || product.price.is_none() => row![
            cx.look.glyph(Icon::Orbs, 14, theme.text),
            text(cost.to_string()).size(14).font(BOLD).color(theme.text),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into(),
        (Some(price), _) => text(price.as_str())
            .size(14)
            .font(BOLD)
            .color(theme.text)
            .into(),
        _ => Space::new().into(),
    };
    let mut words = column![
        text(product.name.as_str())
            .size(14)
            .font(MEDIUM)
            .color(theme.text)
    ]
    .spacing(2);
    let kind = kind_label(product.kind);
    if !kind.is_empty() {
        words = words.push(text(kind).size(12).color(theme.secondary));
    }
    let mut body = column![picture, words, price].spacing(8);
    if let Some(cost) = product.orbs {
        body = body.push(buy(cx, product, cost));
    }
    let raised = theme.raised;
    container(body)
        .padding(12)
        .width(width)
        .style(move |_| rounded(raised, 12))
        .into()
}

/// What a product that costs Orbs offers under its price: Buy, then to
/// confirm or cancel, or what became of the purchase.
fn buy<'a>(cx: Ctx<'a>, product: &'a Product, cost: u64) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let page = &cx.state.shop;
    let sku = &product.sku;
    if page.bought.contains(sku) {
        return action(theme, tr("Bought"), Kind::Plain, None)
            .small()
            .into();
    }
    if page.buying.contains(sku) {
        return action(theme, tr("Buying"), Kind::Primary, None)
            .small()
            .into();
    }
    if page.orbs.is_some_and(|orbs| orbs < cost) {
        return action(theme, tr("Not enough Orbs"), Kind::Plain, None)
            .small()
            .into();
    }
    if cx.pages.confirm.as_deref() == Some(sku.as_str()) {
        return row![
            action(
                theme,
                fill(tr("Buy for {}"), &[&cost]),
                Kind::Success,
                Some(Message::Pages(Event::Buy(sku.clone()))),
            )
            .small(),
            action(
                theme,
                tr("Cancel"),
                Kind::Plain,
                Some(Message::Pages(Event::Confirm(None))),
            )
            .small(),
        ]
        .spacing(6)
        .into();
    }
    action(
        theme,
        tr("Buy"),
        Kind::Primary,
        Some(Message::Pages(Event::Confirm(Some(sku.clone())))),
    )
    .small()
    .into()
}

/// A category as the wide banner: its picture shaded on the left, its
/// logo or name, and what it says of itself; with `alone` it has no button
/// into itself, as it's the one shown.
fn banner<'a>(
    cx: Ctx<'a>,
    category: &'a Category,
    width: f32,
    alone: bool,
) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let height = (width * 0.36).clamp(220.0, 400.0).floor();
    let back = cover(
        cx,
        category.cover(),
        Size::new(width, height),
        [16.0; 4],
        None,
    );
    let shade = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(fade(
                Color {
                    a: 0.7,
                    ..Color::BLACK
                },
                Color::TRANSPARENT,
            )),
            border: Border {
                radius: corners([16.0; 4]),
                ..Border::default()
            },
            ..container::Style::default()
        });
    let name: Element<'a, Message> = contain(
        cx,
        category.logo.as_deref(),
        Size::new(width * 0.4, 72.0),
        0.0,
    )
    .unwrap_or_else(|| {
        text(category.name.as_str())
            .size(36)
            .font(BOLD)
            .color(theme.channel_white)
            .into()
    });
    let mut words = column![name]
        .spacing(12)
        .max_width((width * 0.5).max(240.0));
    if !category.summary.is_empty() {
        words = words.push(
            text(category.summary.as_str())
                .size(15)
                .color(theme.channel_white),
        );
    }
    if !alone {
        words = words.push(
            action(
                theme,
                tr("Shop the collection"),
                Kind::Plain,
                Some(Message::Pages(Event::Category(Some(category.sku.clone())))),
            )
            .height(40.0),
        );
    }
    let front = container(words)
        .padding(28)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::End);
    stack![back, shade, front].into()
}
