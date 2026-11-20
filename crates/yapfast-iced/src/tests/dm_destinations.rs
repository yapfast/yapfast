//! The DM sidebar's Nitro, Shop and Quests screens: opened in Yapfast from
//! the DM navigation rows alongside Friends and Requests, each with its own
//! tabs; the Quests accepted here, the Shop searched, and the purchase and
//! the claim handed to discord.com.

use iced::{Settings, widget::Id};
use iced_test::Simulator;
use yapfast_core::{
    application::{Action, DmScreen, Navigation},
    storefront::{
        Category, Product, Quest, QuestReward, QuestRewardKind, QuestStatus, QuestsPage, ShopPage,
    },
};

use super::{change, interact, preview, shows};
use crate::{
    app::{App, Message},
    browser,
    sidebar::{
        self,
        destinations::{NITRO_URL, NitroTab, QUESTS_URL, QuestTab, SEARCH, SHOP_URL, ShopTab},
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
                    products: vec![product("p3", "Rain")],
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
fn a_finished_quest_is_claimed_on_discord_com() {
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
    assert_eq!(
        browser::take_opened(),
        [QUESTS_URL],
        "a claim may ask for a captcha, which only discord.com shows"
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
fn the_shop_shows_categories_searches_and_opens_products_on_discord_com() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    stocked(&mut app);
    open(&mut app, "Shop");
    assert!(shows(&app, "Vice City"), "the first category is the banner");
    assert!(shows(&app, "550"), "the account's Orbs");
    assert!(shows(&app, "Palm"));

    interact(&mut app, |ui| {
        ui.click("Palm").expect("a product");
    });
    assert_eq!(browser::take_opened(), [format!("{SHOP_URL}#itemSkuId=p1")]);

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
fn the_nitro_screen_has_its_tabs_and_hands_the_checkout_to_discord_com() {
    let (mut app, _folder) = preview();
    dm_home(&mut app);
    open(&mut app, "Nitro");
    assert!(shows(&app, "UNLOCK A WORLD OF PERKS WITH NITRO"));
    assert!(browser::take_opened().is_empty());
    interact(&mut app, |ui| {
        ui.click("Upgrade").expect("the hero's button");
    });
    assert_eq!(browser::take_opened(), [NITRO_URL]);

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
    }
}
