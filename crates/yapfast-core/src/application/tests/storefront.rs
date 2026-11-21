//! The Quests and Shop screens: loading their data when they open, and
//! accepting a quest, and the boosts the account can give a server.

use yapfast_discord::service::{Snapshot, Status};

use super::*;
use crate::storefront::{BoostSlots, Category, Product, Quest, QuestStatus};

fn online() -> Application {
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::DirectMessages));
    app
}

fn quest(id: &str, status: QuestStatus) -> Quest {
    Quest {
        id: id.into(),
        name: format!("Quest {id}"),
        status,
        target: 900,
        ..Quest::default()
    }
}

fn asks(effects: &[Effect], wanted: fn(&DiscordCommand) -> bool) -> bool {
    effects.iter().any(
        |effect| matches!(effect, Effect::Backend(Command::Discord(command)) if wanted(command)),
    )
}

#[test]
fn opening_quests_loads_them_once_at_a_time() {
    let mut app = online();
    let effects = app.dispatch(Action::OpenDmScreen(DmScreen::Quests));
    assert!(asks(&effects, |c| matches!(c, DiscordCommand::LoadQuests)));
    assert!(app.state.quests.loading);

    // Opening it again while Discord hasn't answered asks nothing more.
    let again = app.dispatch(Action::OpenDmScreen(DmScreen::Quests));
    assert!(!asks(&again, |c| matches!(c, DiscordCommand::LoadQuests)));
    assert!(app.dispatch(Action::RefreshQuests).is_empty());

    app.apply(Event::QuestsLoaded {
        result: Ok((
            vec![quest("1", QuestStatus::Available)],
            vec![quest("9", QuestStatus::Claimed)],
        )),
    });
    let page = &app.state.quests;
    assert!(page.loaded && !page.loading && page.error.is_none());
    assert_eq!((page.offered.len(), page.claimed.len()), (1, 1));

    // Coming back to the screen from another one refreshes what it shows.
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    let back = app.dispatch(Action::OpenDmScreen(DmScreen::Quests));
    assert!(asks(&back, |c| matches!(c, DiscordCommand::LoadQuests)));
}

#[test]
fn quests_that_fail_to_load_say_why_and_keep_the_last_list() {
    let mut app = online();
    app.dispatch(Action::OpenDmScreen(DmScreen::Quests));
    app.apply(Event::QuestsLoaded {
        result: Ok((vec![quest("1", QuestStatus::Available)], vec![])),
    });
    app.dispatch(Action::RefreshQuests);
    app.apply(Event::QuestsLoaded {
        result: Err("Discord is down.".into()),
    });
    let page = &app.state.quests;
    assert_eq!(page.error.as_deref(), Some("Discord is down."));
    assert!(!page.loading);
    assert_eq!(page.offered.len(), 1);
}

#[test]
fn the_screens_load_nothing_without_discord_or_in_the_preview() {
    let mut offline = Application::default();
    offline.dispatch(Action::Navigate(Navigation::DirectMessages));
    for screen in [DmScreen::Quests, DmScreen::Shop] {
        let effects = offline.dispatch(Action::OpenDmScreen(screen));
        assert!(!asks(&effects, |c| matches!(
            c,
            DiscordCommand::LoadQuests | DiscordCommand::LoadShop
        )));
    }
    assert!(!offline.state.quests.loading && !offline.state.shop.loading);

    // Nitro and the Friends screen have nothing to load.
    let mut app = online();
    let effects = app.dispatch(Action::OpenDmScreen(DmScreen::Nitro));
    assert!(!asks(&effects, |c| matches!(
        c,
        DiscordCommand::LoadQuests | DiscordCommand::LoadShop
    )));
}

#[test]
fn accepts_a_quest_that_is_on_offer() {
    let mut app = online();
    app.dispatch(Action::OpenDmScreen(DmScreen::Quests));
    app.apply(Event::QuestsLoaded {
        result: Ok((
            vec![
                quest("1", QuestStatus::Available),
                quest("2", QuestStatus::Accepted),
            ],
            vec![],
        )),
    });

    // One that is already accepted, or unknown, is left alone.
    assert!(app.dispatch(Action::AcceptQuest("2".into())).is_empty());
    assert!(app.dispatch(Action::AcceptQuest("7".into())).is_empty());

    assert!(matches!(
        app.dispatch(Action::AcceptQuest("1".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::AcceptQuest { id }))] if id == "1"
    ));
    assert_eq!(app.state.quests.accepting, ["1"]);
    assert!(
        app.dispatch(Action::AcceptQuest("1".into())).is_empty(),
        "one answer at a time"
    );

    // Accepted, it shows as such at once, and Discord's own numbers are
    // asked for.
    let effects = app.apply(Event::QuestAccepted {
        id: "1".into(),
        result: Ok(()),
    });
    assert!(asks(&effects, |c| matches!(c, DiscordCommand::LoadQuests)));
    assert!(app.state.quests.accepting.is_empty());
    assert_eq!(app.state.quests.offered[0].status, QuestStatus::Accepted);
}

#[test]
fn a_refused_quest_stays_on_offer_and_says_why() {
    let mut app = online();
    app.dispatch(Action::OpenDmScreen(DmScreen::Quests));
    app.apply(Event::QuestsLoaded {
        result: Ok((vec![quest("1", QuestStatus::Available)], vec![])),
    });
    app.dispatch(Action::AcceptQuest("1".into()));
    app.apply(Event::QuestAccepted {
        id: "1".into(),
        result: Err("Discord refused.".into()),
    });
    let page = &app.state.quests;
    assert_eq!(page.error.as_deref(), Some("Discord refused."));
    assert!(page.accepting.is_empty());
    assert_eq!(page.offered[0].status, QuestStatus::Available);
    // And it can be tried again.
    assert!(!app.dispatch(Action::AcceptQuest("1".into())).is_empty());
}

#[test]
fn claims_the_reward_of_a_finished_quest_and_opens_its_captcha() {
    let mut app = online();
    app.dispatch(Action::OpenDmScreen(DmScreen::Quests));
    app.apply(Event::QuestsLoaded {
        result: Ok((
            vec![
                quest("1", QuestStatus::Completed),
                quest("2", QuestStatus::Accepted),
                quest("3", QuestStatus::Available),
            ],
            vec![],
        )),
    });

    // Only a finished quest can be claimed.
    assert!(app.dispatch(Action::ClaimQuest("2".into())).is_empty());
    assert!(app.dispatch(Action::ClaimQuest("3".into())).is_empty());
    assert!(app.dispatch(Action::ClaimQuest("7".into())).is_empty());

    assert!(matches!(
        app.dispatch(Action::ClaimQuest("1".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ClaimQuest { id }))] if id == "1"
    ));
    assert_eq!(app.state.quests.claiming, ["1"]);
    assert!(
        app.dispatch(Action::ClaimQuest("1".into())).is_empty(),
        "one answer at a time"
    );

    // A captcha's page opens in the browser, and is kept.
    let effects = app.apply(Event::QuestCaptcha {
        id: "1".into(),
        page: "http://127.0.0.1:9/".into(),
    });
    assert!(matches!(
        effects.as_slice(),
        [Effect::OpenPage(page)] if page == "http://127.0.0.1:9/"
    ));
    assert_eq!(
        app.state.quests.captcha.as_deref(),
        Some("http://127.0.0.1:9/")
    );
    // One for a quest that isn't being claimed is ignored.
    assert!(
        app.apply(Event::QuestCaptcha {
            id: "2".into(),
            page: "http://127.0.0.1:9/".into(),
        })
        .is_empty()
    );

    // Claimed, it shows as such at once, and Discord's lists are asked for.
    let effects = app.apply(Event::QuestClaimed {
        id: "1".into(),
        result: Ok(()),
    });
    assert!(asks(&effects, |c| matches!(c, DiscordCommand::LoadQuests)));
    let page = &app.state.quests;
    assert!(page.claiming.is_empty());
    assert_eq!(page.captcha, None);
    assert_eq!(page.offered[0].status, QuestStatus::Claimed);
}

#[test]
fn a_refused_claim_stays_claimable_and_says_why() {
    let mut app = online();
    app.dispatch(Action::OpenDmScreen(DmScreen::Quests));
    app.apply(Event::QuestsLoaded {
        result: Ok((vec![quest("1", QuestStatus::Completed)], vec![])),
    });
    app.dispatch(Action::ClaimQuest("1".into()));
    app.apply(Event::QuestClaimed {
        id: "1".into(),
        result: Err("Couldn't claim the reward: no.".into()),
    });
    let page = &app.state.quests;
    assert_eq!(
        page.error.as_deref(),
        Some("Couldn't claim the reward: no.")
    );
    assert!(page.claiming.is_empty());
    assert_eq!(page.offered[0].status, QuestStatus::Completed);
    assert!(!app.dispatch(Action::ClaimQuest("1".into())).is_empty());
}

#[test]
fn opening_the_shop_loads_its_categories_and_orbs() {
    let mut app = online();
    let effects = app.dispatch(Action::OpenDmScreen(DmScreen::Shop));
    assert!(asks(&effects, |c| matches!(c, DiscordCommand::LoadShop)));
    assert!(
        app.dispatch(Action::RefreshShop).is_empty(),
        "already asked"
    );

    app.apply(Event::ShopLoaded {
        result: Ok((
            vec![Category {
                sku: "1".into(),
                name: "Vice City".into(),
                products: vec![Product {
                    sku: "11".into(),
                    name: "Palm".into(),
                    ..Product::default()
                }],
                ..Category::default()
            }],
            Some(550),
        )),
    });
    let shop = &app.state.shop;
    assert!(shop.loaded && !shop.loading);
    assert_eq!((shop.categories.len(), shop.orbs), (1, Some(550)));

    app.dispatch(Action::RefreshShop);
    app.apply(Event::ShopLoaded {
        result: Err("No shop today.".into()),
    });
    assert_eq!(app.state.shop.error.as_deref(), Some("No shop today."));
    assert_eq!(app.state.shop.categories.len(), 1, "the last list stays");
}

/// A Shop with one product that costs Orbs, and the account's Orbs.
fn stocked_shop(orbs: Option<u64>) -> Application {
    let mut app = online();
    app.dispatch(Action::OpenDmScreen(DmScreen::Shop));
    app.apply(Event::ShopLoaded {
        result: Ok((
            vec![Category {
                sku: "1".into(),
                name: "Vice City".into(),
                products: vec![
                    Product {
                        sku: "11".into(),
                        name: "Palm".into(),
                        orbs: Some(400),
                        ..Product::default()
                    },
                    Product {
                        sku: "12".into(),
                        name: "Neon".into(),
                        price: Some("$8.99".into()),
                        ..Product::default()
                    },
                ],
                ..Category::default()
            }],
            orbs,
        )),
    });
    app
}

#[test]
fn buys_a_product_that_costs_orbs_once_and_reads_the_orbs_again() {
    let mut app = stocked_shop(Some(550));
    let effects = app.dispatch(Action::BuyProduct("11".into()));
    assert!(asks(
        &effects,
        |c| matches!(c, DiscordCommand::BuyProduct { sku } if sku == "11")
    ));
    assert_eq!(app.state.shop.buying, ["11"]);

    // A second press while Discord hasn't answered buys nothing more.
    assert!(app.dispatch(Action::BuyProduct("11".into())).is_empty());

    let after = app.apply(Event::ProductBought {
        sku: "11".into(),
        result: Ok(()),
    });
    let shop = &app.state.shop;
    assert!(shop.buying.is_empty());
    assert_eq!(shop.bought, ["11"]);
    assert!(shop.refused.is_none());
    assert!(
        asks(&after, |c| matches!(c, DiscordCommand::LoadShop)),
        "Discord's own number of Orbs left replaces the old one"
    );

    // What is bought is not bought again.
    assert!(app.dispatch(Action::BuyProduct("11".into())).is_empty());
}

#[test]
fn a_purchase_without_the_orbs_for_it_is_not_asked_of_discord() {
    let mut app = stocked_shop(Some(399));
    assert!(app.dispatch(Action::BuyProduct("11".into())).is_empty());
    let shop = &app.state.shop;
    assert!(shop.buying.is_empty());
    assert_eq!(shop.refused.as_deref(), Some("You don't have enough Orbs."));
}

#[test]
fn a_product_priced_in_money_has_no_purchase_in_yapfast() {
    let mut app = stocked_shop(Some(5000));
    assert!(app.dispatch(Action::BuyProduct("12".into())).is_empty());
    assert!(
        app.dispatch(Action::BuyProduct("nothing".into()))
            .is_empty()
    );
    assert!(app.state.shop.buying.is_empty());
}

#[test]
fn a_refused_purchase_says_why_and_can_be_tried_again() {
    let mut app = stocked_shop(None);
    app.dispatch(Action::BuyProduct("11".into()));
    app.apply(Event::ProductBought {
        sku: "11".into(),
        result: Err("Discord didn't sell it.".into()),
    });
    let shop = &app.state.shop;
    assert_eq!(shop.refused.as_deref(), Some("Discord didn't sell it."));
    assert!(shop.buying.is_empty() && shop.bought.is_empty());
    assert!(!app.dispatch(Action::BuyProduct("11".into())).is_empty());
    assert!(
        app.state.shop.refused.is_none(),
        "asking again clears the old answer"
    );
}

const SERVER: &str = "discord:10";

fn slots(free: u32) -> BoostSlots {
    BoostSlots {
        free,
        ..BoostSlots::default()
    }
}

fn boosting(free: u32) -> Application {
    let mut app = online();
    app.dispatch(Action::LoadBoosts(SERVER.into()));
    app.apply(Event::BoostsLoaded {
        server: SERVER.into(),
        result: Ok(slots(free)),
    });
    app
}

#[test]
fn reads_the_accounts_boosts_once_at_a_time() {
    let mut app = online();
    let effects = app.dispatch(Action::LoadBoosts(SERVER.into()));
    assert!(asks(&effects, |command| matches!(
        command,
        DiscordCommand::LoadBoosts { server } if server == SERVER
    )));
    assert!(app.state.boosts.loading);
    assert!(
        app.dispatch(Action::LoadBoosts(SERVER.into())).is_empty(),
        "a second asking waits for the first"
    );
    app.apply(Event::BoostsLoaded {
        server: SERVER.into(),
        result: Ok(slots(2)),
    });
    let page = &app.state.boosts;
    assert!(!page.loading && page.error.is_none());
    assert_eq!(page.slots, Some(slots(2)));
}

#[test]
fn boosts_are_not_read_in_the_preview_offline_or_for_a_plain_server() {
    let mut app = Application::default();
    assert!(app.dispatch(Action::LoadBoosts(SERVER.into())).is_empty());
    let mut app = online();
    assert!(
        app.dispatch(Action::LoadBoosts("local-1".into()))
            .is_empty()
    );
    app.state.preview = true;
    assert!(app.dispatch(Action::LoadBoosts(SERVER.into())).is_empty());
    assert!(app.state.boosts.server.is_none());
}

#[test]
fn boosts_read_for_another_server_are_dropped() {
    let mut app = boosting(1);
    app.dispatch(Action::LoadBoosts("discord:11".into()));
    assert_eq!(app.state.boosts.server.as_deref(), Some("discord:11"));
    assert!(
        app.state.boosts.slots.is_none(),
        "the old server's counts go"
    );
    // The slow answer of the first server finds the page on another.
    app.apply(Event::BoostsLoaded {
        server: SERVER.into(),
        result: Ok(slots(5)),
    });
    assert!(app.state.boosts.slots.is_none());
    assert!(app.state.boosts.loading);
}

#[test]
fn a_boost_that_cannot_be_read_says_why_and_can_be_asked_again() {
    let mut app = online();
    app.dispatch(Action::LoadBoosts(SERVER.into()));
    app.apply(Event::BoostsLoaded {
        server: SERVER.into(),
        result: Err("Couldn't read your boosts: no answer".into()),
    });
    let page = &app.state.boosts;
    assert!(!page.loading && page.slots.is_none());
    assert_eq!(
        page.error.as_deref(),
        Some("Couldn't read your boosts: no answer")
    );
    assert!(!app.dispatch(Action::LoadBoosts(SERVER.into())).is_empty());
    assert!(app.state.boosts.error.is_none());
}

#[test]
fn gives_the_server_a_free_boost_once_and_reads_the_boosts_again() {
    let mut app = boosting(2);
    let effects = app.dispatch(Action::BoostServer(SERVER.into()));
    assert!(asks(&effects, |command| matches!(
        command,
        DiscordCommand::BoostServer { server } if server == SERVER
    )));
    assert!(app.state.boosts.applying);
    assert!(
        app.dispatch(Action::BoostServer(SERVER.into())).is_empty(),
        "a second press waits for Discord"
    );

    let effects = app.apply(Event::ServerBoosted {
        server: SERVER.into(),
        result: Ok(()),
    });
    let page = &app.state.boosts;
    assert!(!page.applying && page.given);
    assert!(
        asks(&effects, |command| matches!(
            command,
            DiscordCommand::LoadBoosts { .. }
        )),
        "what is left to give is Discord's number"
    );
}

#[test]
fn a_boost_is_not_given_without_a_free_one() {
    let mut app = boosting(0);
    assert!(app.dispatch(Action::BoostServer(SERVER.into())).is_empty());
    assert!(!app.state.boosts.applying);

    // Nor to a server whose boosts were not read.
    let mut app = boosting(3);
    assert!(
        app.dispatch(Action::BoostServer("discord:11".into()))
            .is_empty()
    );
}

#[test]
fn a_refused_boost_says_why_and_the_press_can_be_tried_again() {
    let mut app = boosting(1);
    app.dispatch(Action::BoostServer(SERVER.into()));
    app.apply(Event::ServerBoosted {
        server: SERVER.into(),
        result: Err("Couldn't boost the server: not allowed".into()),
    });
    let page = &app.state.boosts;
    assert!(!page.applying && !page.given);
    assert_eq!(
        page.error.as_deref(),
        Some("Couldn't boost the server: not allowed")
    );
    assert!(!app.dispatch(Action::BoostServer(SERVER.into())).is_empty());
    assert!(app.state.boosts.error.is_none());
}
