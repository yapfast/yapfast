//! The DM sidebar's Nitro, Shop and Quests screens: opened in Yapfast from
//! the DM navigation rows alongside Friends and Requests, each with its own
//! tabs; the Quests accepted and claimed here, the Shop searched.

use iced::{
    Settings,
    keyboard::{Key, key},
    widget::Id,
};
use iced_test::Simulator;
use yapfast_core::{
    application::{Action, DmScreen, Navigation},
    storefront::{
        Category, DiscoverCategory, DiscoverPage, DiscoverServer, Product, Quest, QuestReward,
        QuestRewardKind, QuestStatus, QuestsPage, ShopPage,
    },
};

use super::{change, interact, preview, shows};
use crate::{
    app::{App, Message},
    browser,
    sidebar::{
        self,
        destinations::{DISCOVER_SEARCH, GIFT_CODE, NitroTab, QuestTab, SEARCH, ShopTab},
    },
};

/// Each row of the sidebar, its screen, and a tab of that screen's header.
const SCREENS: [(&str, DmScreen, &str); 3] = [
    ("Nitro", DmScreen::Nitro, "Compare"),
    ("Shop", DmScreen::Shop, "Orbs Shop"),
    ("Quests", DmScreen::Quests, "Claimed Quests"),
];

fn dm_home(app: &mut App) {
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    change(app, |state| {
        state.account = Some("1".into());
        state.preview = false;
        state.discord_linked = true;
    });
    browser::take_opened();
}

fn quest(id: &str, game: &str, status: QuestStatus) -> Quest {
    Quest {
        id: id.into(),
        name: format!("{game} quest"),
        game: game.into(),
        publisher: "Studio".into(),
        reward: Some(QuestReward {
            kind: QuestRewardKind::Orbs,
            name: "Orbs".into(),
            orbs: Some(700),
            picture: None,
        }),
        target: 900,
        progress: if status == QuestStatus::Accepted {
            300
        } else {
            0
        },
        status,
        ..Quest::default()
    }
}

fn product(sku: &str, name: &str) -> Product {
    Product {
        sku: sku.into(),
        name: name.into(),
        price: Some("$8.99".into()),
        ..Product::default()
    }
}

/// A product that costs Orbs and no money.
fn orbs_product(sku: &str, name: &str, cost: u64) -> Product {
    Product {
        sku: sku.into(),
        name: name.into(),
        orbs: Some(cost),
        ..Product::default()
    }
}

/// Discord's answers, as the core holds them once they have come.
fn stocked(app: &mut App) {
    change(app, |state| {
        state.quests = QuestsPage {
            offered: vec![
                quest("q1", "Zenith Arena", QuestStatus::Available),
                quest("q2", "Hollow Peaks", QuestStatus::Accepted),
            ],
            loaded: true,
            ..QuestsPage::default()
        };
        state.shop = ShopPage {
            categories: vec![
                Category {
                    sku: "c1".into(),
                    name: "Vice City".into(),
                    products: vec![product("p1", "Palm"), product("p2", "Neon plate")],
                    ..Category::default()
                },
                Category {
                    sku: "c2".into(),
                    name: "Lo-fi vibes".into(),
                    products: vec![orbs_product("p3", "Rain", 400)],
                    ..Category::default()
                },
            ],
            orbs: Some(550),
            loaded: true,
            ..ShopPage::default()
        };
    });
}

fn open(app: &mut App, row: &str) {
    interact(app, |ui| {
        ui.click(row).expect("the DM sidebar row");
    });
}

#[test]
fn dm_screens_open_natively_in_yapfast_from_the_sidebar() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    for (label, screen, tab) in SCREENS {
        open(&mut app, label);
        assert_eq!(app.client.state().dm_screen, Some(screen));
        assert_eq!(app.client.state().selected, None);
        assert!(shows(&app, tab), "{label} screen shows its tab {tab}");
        assert!(
            browser::take_opened().is_empty(),
            "clicking {label} opens in Yapfast, not the browser"
        );
    }
}

#[test]
fn dm_screens_participate_in_back_and_forward_navigation_and_select_rooms() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    open(&mut app, "Friends");
    assert_eq!(app.client.state().dm_screen, Some(DmScreen::Friends));

    for (label, screen, tab) in SCREENS {
        open(&mut app, label);
        assert_eq!(app.client.state().dm_screen, Some(screen));
        assert!(shows(&app, tab));
    }

    app.client.dispatch(Action::NavigateBack);
    assert_eq!(app.client.state().dm_screen, Some(DmScreen::Shop));
    app.client.dispatch(Action::NavigateForward);
    assert_eq!(app.client.state().dm_screen, Some(DmScreen::Quests));

    interact(&mut app, |ui| {
        ui.click("Sam").expect("Sam's DM");
    });
    assert_eq!(app.client.state().navigation, Navigation::DirectMessages);
    assert_eq!(app.client.state().dm_screen, None);
    assert!(app.client.state().selected.is_some());
}

#[test]
fn dm_screens_require_a_linked_account_and_fit_every_sidebar_width() {
    let (mut app, _folder) = preview();
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    for (label, ..) in SCREENS {
        assert!(!shows(&app, label), "not in the preview");
    }
    dm_home(&mut app);
    for width in [180.0, sidebar::WIDTH, 380.0] {
        app.layout.sidebar_width = Some(width);
        let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
        let _ = ui.find(Id::new(sidebar::LIST)).expect("the sidebar list");
        for (label, ..) in SCREENS {
            let found = ui.find(label).expect("the sidebar row label");
            assert!(found.visible_bounds().is_some());
        }
    }
    change(&mut app, |state| state.discord_linked = false);
    for (label, ..) in SCREENS {
        assert!(!shows(&app, label), "not when unlinked");
    }
}

#[test]
fn the_quests_screen_lists_discords_quests_and_accepts_one() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    stocked(&mut app);
    open(&mut app, "Quests");
    assert!(shows(&app, "ZENITH ARENA"), "the first quest is the banner");
    assert!(shows(&app, "Available Quests"));

    interact(&mut app, |ui| {
        ui.click("Accept quest").expect("the banner's button");
    });
    assert_eq!(app.client.state().quests.accepting, ["q1"]);
    assert!(shows(&app, "Accepting"), "no second press while it waits");
    assert!(
        browser::take_opened().is_empty(),
        "accepting stays in Yapfast"
    );

    interact(&mut app, |ui| {
        ui.click("Claimed Quests").expect("the second tab");
    });
    assert_eq!(app.pages.quests, QuestTab::Claimed);
    assert!(shows(&app, "No claimed quests yet"));
    interact(&mut app, |ui| {
        ui.click("All Quests").expect("the first tab");
    });
    assert_eq!(app.pages.quests, QuestTab::All);
}

#[test]
fn a_finished_quest_is_claimed_in_yapfast() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    stocked(&mut app);
    change(&mut app, |state| {
        state.quests.offered[0].status = QuestStatus::Completed;
    });
    open(&mut app, "Quests");
    interact(&mut app, |ui| {
        ui.click("Claim reward").expect("the claim button");
    });
    assert_eq!(app.client.state().quests.claiming, ["q1"]);
    assert!(shows(&app, "Claiming"), "no second press while it waits");
    assert!(
        browser::take_opened().is_empty(),
        "claiming is asked of Discord by Yapfast, not sent to discord.com"
    );
}

#[test]
fn the_quests_screen_says_when_discord_lists_none() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    change(&mut app, |state| {
        state.quests = QuestsPage {
            loaded: true,
            ..QuestsPage::default()
        };
    });
    open(&mut app, "Quests");
    assert!(shows(&app, "No quests right now"));
}

#[test]
fn the_shop_shows_categories_searches_and_lists_money_products_without_a_checkout() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    stocked(&mut app);
    open(&mut app, "Shop");
    assert!(shows(&app, "Vice City"), "the first category is the banner");
    assert!(shows(&app, "550"), "the account's Orbs");
    assert!(shows(&app, "Palm"));

    // A product priced in money has its price and nothing to press: Yapfast
    // has no checkout, and nothing sends the reader to discord.com either.
    interact(&mut app, |ui| {
        ui.click("Palm").expect("a product");
    });
    assert!(browser::take_opened().is_empty());

    interact(&mut app, |ui| {
        ui.click("Shop the collection")
            .expect("the banner's button");
    });
    assert_eq!(app.pages.shop, ShopTab::Browse);
    assert_eq!(app.pages.category.as_deref(), Some("c1"));
    assert!(!shows(&app, "Rain"), "the other category is not shown");

    interact(&mut app, |ui| {
        ui.click("All categories").expect("the way back");
    });
    assert_eq!(app.pages.category, None);
    assert!(shows(&app, "Rain"));

    interact(&mut app, |ui| {
        ui.click(Id::new(SEARCH)).expect("the search field");
        ui.typewrite("neon");
    });
    assert_eq!(app.pages.search, "neon");
    assert!(shows(&app, "Results for neon"));
    assert!(shows(&app, "Neon plate"));
    assert!(!shows(&app, "Rain"));
}

#[test]
fn an_orbs_product_is_bought_in_yapfast_after_a_second_press() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    stocked(&mut app);
    open(&mut app, "Shop");
    assert!(!shows(&app, "Buy for 400"));
    interact(&mut app, |ui| {
        ui.click("Buy").expect("the Buy button of the Orbs product");
    });
    assert_eq!(app.pages.confirm.as_deref(), Some("p3"));
    assert!(
        app.client.state().shop.buying.is_empty(),
        "one press buys nothing"
    );
    assert!(shows(&app, "Buy for 400"));

    interact(&mut app, |ui| {
        ui.click("Cancel").expect("Cancel");
    });
    assert_eq!(app.pages.confirm, None);
    assert!(!shows(&app, "Buy for 400"));

    interact(&mut app, |ui| {
        ui.click("Buy").expect("Buy");
    });
    interact(&mut app, |ui| {
        ui.click("Buy for 400").expect("the confirming press");
    });
    assert_eq!(app.pages.confirm, None);
    assert_eq!(app.client.state().shop.buying, ["p3"]);
    assert!(shows(&app, "Buying"), "no second purchase while it waits");
    assert!(
        browser::take_opened().is_empty(),
        "buying is asked of Discord by Yapfast, not sent to discord.com"
    );
}

#[test]
fn a_product_costing_more_orbs_than_the_account_has_cannot_be_bought() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    stocked(&mut app);
    change(&mut app, |state| state.shop.orbs = Some(399));
    open(&mut app, "Shop");
    assert!(shows(&app, "Not enough Orbs"));
    assert!(!shows(&app, "Buy"));
    assert!(app.client.state().shop.buying.is_empty());
}

#[test]
fn the_shop_search_says_when_nothing_matches() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    stocked(&mut app);
    open(&mut app, "Shop");
    interact(&mut app, |ui| {
        ui.click(Id::new(SEARCH)).expect("the search field");
        ui.typewrite("zzz");
    });
    assert!(shows(&app, "Nothing in the Shop matches zzz."));
}

#[test]
fn the_nitro_screen_has_its_tabs_and_sends_nobody_to_discord_com() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    open(&mut app, "Nitro");
    assert!(shows(&app, "UNLOCK A WORLD OF PERKS WITH NITRO"));
    // Yapfast has no checkout, so no button here sells or opens a page.
    for sold in ["Upgrade", "Gift Nitro", "Get Nitro", "See Nitro Basic"] {
        assert!(!shows(&app, sold), "{sold} has no checkout to open");
    }
    assert!(browser::take_opened().is_empty());

    for (tab, event, heading) in [
        ("Perks", NitroTab::Perks, "In Yapfast"),
        ("Plans", NitroTab::Plans, "Your plan"),
        ("Compare", NitroTab::Compare, "Upload size"),
        ("Home", NitroTab::Home, "What Nitro gives you"),
    ] {
        interact(&mut app, |ui| {
            ui.click(tab).expect("a tab");
        });
        assert_eq!(app.pages.nitro, event);
        assert!(shows(&app, heading), "{tab} shows {heading}");
        for sold in ["Upgrade", "Gift Nitro", "Get Nitro", "See Nitro Basic"] {
            assert!(!shows(&app, sold), "{tab} offers {sold}");
        }
    }
    assert!(browser::take_opened().is_empty());
}

#[test]
fn a_gift_code_typed_on_the_nitro_screen_opens_its_gift_to_redeem() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    change(&mut app, |state| state.discord_online = true);
    open(&mut app, "Nitro");

    // Something that is no code leaves Redeem without a press.
    interact(&mut app, |ui| {
        ui.click(Id::new(GIFT_CODE)).expect("the gift code field");
        ui.typewrite("nope");
    });
    assert_eq!(app.pages.code, "nope");
    interact(&mut app, |ui| {
        let _ = ui.click("Redeem");
    });
    assert!(app.client.state().gift.is_none());

    // A gift link is read as its code, and its dialog takes over.
    app.pages.code.clear();
    interact(&mut app, |ui| {
        ui.click(Id::new(GIFT_CODE)).expect("the gift code field");
        ui.typewrite("https://discord.gift/2CG6SV9QtRxerJTgCYNDnU7M");
    });
    interact(&mut app, |ui| {
        ui.click("Redeem").expect("Redeem");
    });
    let gift = app.client.state().gift.clone().expect("the gift's dialog");
    assert_eq!(gift.code, "2CG6SV9QtRxerJTgCYNDnU7M");
    assert!(app.pages.code.is_empty(), "the field is emptied");
    assert!(browser::take_opened().is_empty());
}

fn discovered(id: &str, name: &str) -> DiscoverServer {
    DiscoverServer {
        id: id.into(),
        name: name.into(),
        description: Some(format!("All about {name}.")),
        members: Some(1200),
        online: Some(300),
        ..DiscoverServer::default()
    }
}

/// The Discover screen, opened from the rail, with three servers listed of
/// four that match, the second of them joined already.
fn discover(app: &mut App) {
    dm_home(app);
    change(app, |state| {
        state.discord_online = true;
        state.discover = DiscoverPage {
            categories: vec![
                DiscoverCategory {
                    id: 1,
                    name: "Gaming".into(),
                },
                DiscoverCategory {
                    id: 3,
                    name: "Music".into(),
                },
            ],
            servers: vec![
                discovered("10", "Rustaceans"),
                discovered("20", "Synthwave"),
                discovered("30", "Chess club"),
            ],
            total: 4,
            loaded: true,
            joined: vec!["20".into()],
            ..DiscoverPage::default()
        };
    });
    let _ = app.update(Message::Rail(crate::rail::Event::Discover));
    browser::take_opened();
}

#[test]
fn the_discover_screen_lists_servers_and_joins_one_in_yapfast() {
    let (mut app, _folder) = preview();
    discover(&mut app);
    assert_eq!(app.client.state().dm_screen, Some(DmScreen::Discover));
    for words in [
        "Rustaceans",
        "Synthwave",
        "All about Rustaceans.",
        "300 Online",
        "1,200 Members",
        "Gaming",
        "Music",
        "Joined",
        "Show more",
    ] {
        assert!(shows(&app, words), "the page shows {words}");
    }
    // Press Join on a server that is not joined yet; the joined one has
    // no Join of its own.
    interact(&mut app, |ui| {
        ui.click("Join").expect("a Join button");
    });
    assert_eq!(app.client.state().discover.joining, ["10"]);
    assert!(shows(&app, "Joining"), "no second join while it waits");
    assert!(
        browser::take_opened().is_empty(),
        "joining is asked of Discord by Yapfast, not sent to discord.com"
    );
}

#[test]
fn the_discover_list_pages_and_narrows_to_a_category() {
    let (mut app, _folder) = preview();
    discover(&mut app);
    interact(&mut app, |ui| {
        ui.click("Show more").expect("Show more");
    });
    let page = &app.client.state().discover;
    assert!(page.loading, "the next page is being asked for");
    assert_eq!(page.servers.len(), 3, "the list is kept until it comes");
    assert!(shows(&app, "Loading"));

    change(&mut app, |state| state.discover.loading = false);
    interact(&mut app, |ui| {
        ui.click("Music").expect("the Music chip");
    });
    let page = &app.client.state().discover;
    assert_eq!(page.category, Some(3));
    assert!(page.loading);
}

#[test]
fn a_discover_search_is_asked_when_it_is_submitted_not_while_it_is_typed() {
    let (mut app, _folder) = preview();
    discover(&mut app);
    interact(&mut app, |ui| {
        ui.click(Id::new(DISCOVER_SEARCH))
            .expect("the search field");
        ui.typewrite("rust");
    });
    assert_eq!(app.pages.discover, "rust");
    assert_eq!(app.client.state().discover.query, "", "nothing asked yet");
    assert!(!app.client.state().discover.loading);
    interact(&mut app, |ui| {
        ui.tap_key(Key::Named(key::Named::Enter));
    });
    let page = &app.client.state().discover;
    assert_eq!(page.query, "rust");
    assert!(page.loading);
}

#[test]
fn the_discover_screen_says_when_nothing_matches() {
    let (mut app, _folder) = preview();
    discover(&mut app);
    change(&mut app, |state| {
        state.discover.servers.clear();
        state.discover.total = 0;
        state.discover.query = "zzz".into();
    });
    assert!(shows(&app, "No server matches zzz."));
    assert!(!shows(&app, "Show more"));
}

#[test]
fn a_server_the_account_is_in_offers_open_instead_of_join() {
    let (mut app, _folder) = preview();
    discover(&mut app);
    change(&mut app, |state| {
        let mut known = (*state.servers[0]).clone();
        known.id = "discord:10".into();
        known.name = "Rustaceans".into();
        state.servers.push(std::sync::Arc::new(known));
    });
    assert!(shows(&app, "Open"), "a member opens the server");
    interact(&mut app, |ui| {
        ui.click("Open").expect("an Open button");
    });
    assert_eq!(
        app.client.state().navigation,
        Navigation::Server("discord:10".into())
    );
    assert!(browser::take_opened().is_empty());
}
