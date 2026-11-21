//! The Quests and Shop screens: loading their data when they open, and
//! accepting a quest, the boosts the account can give a server, the servers
//! of the Discover list, and the gifts the composer offers to give.

use yapfast_discord::service::{Snapshot, Status};

use super::*;
use crate::storefront::{
    BoostSlots, Category, DiscoverBatch, DiscoverCategory, DiscoverServer, GiftToGive, Product,
    Quest, QuestStatus, QuestTask, QuestVideo,
};

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

/// A quest of watching with a video, `progress` seconds of it done.
fn watchable(id: &str, status: QuestStatus, progress: u64) -> Quest {
    Quest {
        task: QuestTask::Watch,
        video: Some(QuestVideo {
            url: format!("https://cdn.discordapp.com/quests/{id}/video.mp4"),
            thumbnail: None,
        }),
        progress,
        ..quest(id, status)
    }
}

fn quest_video(id: &str, ended: bool) -> chat::Video {
    chat::Video {
        room: chat::QUEST_VIDEO_ROOM.into(),
        item: id.into(),
        index: 0,
        frames: None,
        duration_ms: Some(900_400),
        size: Some((1280, 720)),
        loading: false,
        playing: !ended,
        ended,
        sound: true,
        muted: false,
        volume: 1.0,
        error: None,
    }
}

#[test]
fn plays_the_video_of_an_accepted_quest_and_tells_discord_how_much_was_watched() {
    let mut app = online();
    app.dispatch(Action::OpenDmScreen(DmScreen::Quests));
    app.apply(Event::QuestsLoaded {
        result: Ok((
            vec![
                watchable("1", QuestStatus::Accepted, 20),
                watchable("2", QuestStatus::Available, 0),
                quest("3", QuestStatus::Accepted),
            ],
            vec![],
        )),
    });
    let play = |id: &str| Action::PlayQuestVideo {
        id: id.into(),
        width: 1280,
        height: 720,
    };

    // Only an accepted quest that has a video plays.
    for id in ["2", "3", "7"] {
        assert!(app.dispatch(play(id)).is_empty(), "quest {id}");
    }
    assert!(matches!(
        app.dispatch(play("1")).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::PlayQuestVideo {
            id, url, width: 1280, ..
        }))] if id == "1" && url.ends_with("/1/video.mp4")
    ));
    assert_eq!(app.state.quests.watching, Some(("1".into(), 20)));

    // The video belongs to the Quests screen, no conversation.
    app.apply(Event::Video(Some(quest_video("1", false))));
    assert!(app.state.video.is_some());

    // Discord hears of it every five seconds past what it knew, not
    // before.
    assert!(app.dispatch(Action::QuestVideoAt(23_000)).is_empty());
    assert!(matches!(
        app.dispatch(Action::QuestVideoAt(26_500)).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ReportQuestVideo {
            id, seconds: 26, target: 900,
        }))] if id == "1"
    ));
    assert!(
        app.dispatch(Action::QuestVideoAt(30_000)).is_empty(),
        "four seconds on"
    );
    assert!(
        !app.dispatch(Action::QuestVideoAt(31_000)).is_empty(),
        "five seconds on"
    );

    // At its end, all of it is told, once.
    let effects = app.apply(Event::Video(Some(quest_video("1", true))));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::ReportQuestVideo { seconds: 901, .. }
        ))]
    ));
    assert!(
        app.apply(Event::Video(Some(quest_video("1", true))))
            .is_empty()
    );
}

#[test]
fn a_quest_finished_by_its_video_waits_to_be_claimed() {
    let mut app = online();
    app.dispatch(Action::OpenDmScreen(DmScreen::Quests));
    app.apply(Event::QuestsLoaded {
        result: Ok((vec![watchable("1", QuestStatus::Accepted, 800)], vec![])),
    });

    // A count that says less than the list did moves nothing back.
    let effects = app.apply(Event::QuestProgress {
        id: "1".into(),
        result: Ok((QuestStatus::Available, 0)),
    });
    assert!(effects.is_empty());
    let quest = &app.state.quests.offered[0];
    assert_eq!((quest.status, quest.progress), (QuestStatus::Accepted, 800));

    let effects = app.apply(Event::QuestProgress {
        id: "1".into(),
        result: Ok((QuestStatus::Accepted, 850)),
    });
    assert!(effects.is_empty());
    assert_eq!(app.state.quests.offered[0].progress, 850);

    // Done, it can be claimed, and the list is read again.
    let effects = app.apply(Event::QuestProgress {
        id: "1".into(),
        result: Ok((QuestStatus::Completed, 900)),
    });
    assert!(asks(&effects, |c| matches!(c, DiscordCommand::LoadQuests)));
    assert_eq!(app.state.quests.offered[0].status, QuestStatus::Completed);

    app.apply(Event::QuestProgress {
        id: "1".into(),
        result: Err("Couldn't count the video.".into()),
    });
    assert_eq!(
        app.state.quests.error.as_deref(),
        Some("Couldn't count the video.")
    );
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

fn listed(id: &str) -> DiscoverServer {
    DiscoverServer {
        id: id.into(),
        name: format!("Server {id}"),
        ..DiscoverServer::default()
    }
}

fn batch(ids: &[&str], total: u64) -> DiscoverBatch {
    DiscoverBatch {
        servers: ids.iter().map(|id| listed(id)).collect(),
        total,
        categories: None,
    }
}

fn discovering() -> Application {
    let mut app = online();
    app.dispatch(Action::OpenDmScreen(DmScreen::Discover));
    app.apply(Event::DiscoverLoaded {
        query: String::new(),
        category: None,
        offset: 0,
        result: Ok(DiscoverBatch {
            categories: Some(vec![DiscoverCategory {
                id: 1,
                name: "Gaming".into(),
            }]),
            ..batch(&["10", "20"], 3)
        }),
    });
    app
}

#[test]
fn opening_discover_loads_the_list_and_its_categories_once_at_a_time() {
    let mut app = online();
    let effects = app.dispatch(Action::OpenDmScreen(DmScreen::Discover));
    assert!(asks(&effects, |command| matches!(
        command,
        DiscordCommand::LoadDiscover {
            query,
            category: None,
            offset: 0,
            categories: true,
        } if query.is_empty()
    )));
    assert!(app.state.discover.loading);
    let again = app.dispatch(Action::OpenDmScreen(DmScreen::Discover));
    assert!(!asks(&again, |c| matches!(
        c,
        DiscordCommand::LoadDiscover { .. }
    )));

    app.apply(Event::DiscoverLoaded {
        query: String::new(),
        category: None,
        offset: 0,
        result: Ok(DiscoverBatch {
            categories: Some(vec![DiscoverCategory {
                id: 1,
                name: "Gaming".into(),
            }]),
            ..batch(&["10", "20"], 3)
        }),
    });
    let page = &app.state.discover;
    assert!(page.loaded && !page.loading && page.error.is_none());
    assert_eq!((page.servers.len(), page.total), (2, 3));
    assert_eq!(page.categories.len(), 1);
    assert!(page.more());

    // The categories are known now, so a new asking leaves them out.
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    let back = app.dispatch(Action::OpenDmScreen(DmScreen::Discover));
    assert!(asks(&back, |c| matches!(
        c,
        DiscordCommand::LoadDiscover {
            categories: false,
            offset: 0,
            ..
        }
    )));
}

#[test]
fn discover_asks_nothing_in_the_preview_or_offline() {
    let mut offline = Application::default();
    offline.dispatch(Action::Navigate(Navigation::DirectMessages));
    let effects = offline.dispatch(Action::OpenDmScreen(DmScreen::Discover));
    assert!(!asks(&effects, |c| matches!(
        c,
        DiscordCommand::LoadDiscover { .. }
    )));
    assert!(!offline.state.discover.loading);

    let mut app = online();
    app.state.preview = true;
    assert!(
        app.dispatch(Action::LoadDiscover {
            query: "rust".into(),
            category: None,
        })
        .is_empty()
    );
    assert!(app.dispatch(Action::JoinDiscovered("10".into())).is_empty());
}

#[test]
fn a_search_trims_its_words_and_goes_back_to_the_first_page() {
    let mut app = discovering();
    let effects = app.dispatch(Action::LoadDiscover {
        query: "  rust  ".into(),
        category: Some(1),
    });
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::LoadDiscover {
            query,
            category: Some(1),
            offset: 0,
            categories: false,
        }))] if query == "rust"
    ));
    let page = &app.state.discover;
    assert_eq!((page.query.as_str(), page.category), ("rust", Some(1)));
    assert!(page.loading);
    assert!(
        page.servers.is_empty(),
        "the old results belong to another filter"
    );
    assert_eq!(page.total, 0);

    app.apply(Event::DiscoverLoaded {
        query: "rust".into(),
        category: Some(1),
        offset: 0,
        result: Ok(batch(&["30"], 1)),
    });
    let page = &app.state.discover;
    assert_eq!(page.servers.len(), 1);
    assert!(!page.more());
}

#[test]
fn an_answer_for_words_that_were_left_is_dropped() {
    let mut app = discovering();
    app.dispatch(Action::LoadDiscover {
        query: "rust".into(),
        category: None,
    });
    // The words changed again before the first answer came.
    app.dispatch(Action::LoadDiscover {
        query: "chess".into(),
        category: None,
    });
    app.apply(Event::DiscoverLoaded {
        query: "rust".into(),
        category: None,
        offset: 0,
        result: Ok(batch(&["77"], 1)),
    });
    let page = &app.state.discover;
    assert!(page.loading, "the answer for chess is still awaited");
    assert!(page.servers.is_empty());

    // A page that doesn't follow the list is no part of it.
    app.apply(Event::DiscoverLoaded {
        query: "chess".into(),
        category: None,
        offset: 24,
        result: Ok(batch(&["88"], 40)),
    });
    assert!(app.state.discover.servers.is_empty());
}

#[test]
fn show_more_appends_the_next_page_without_repeating_a_server() {
    let mut app = discovering();
    let effects = app.dispatch(Action::MoreDiscover);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::LoadDiscover {
                offset: 2,
                categories: false,
                ..
            }
        ))]
    ));
    assert!(
        app.dispatch(Action::MoreDiscover).is_empty(),
        "one page at a time"
    );
    app.apply(Event::DiscoverLoaded {
        query: String::new(),
        category: None,
        offset: 2,
        result: Ok(batch(&["20", "30"], 3)),
    });
    let page = &app.state.discover;
    let ids: Vec<_> = page.servers.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["10", "20", "30"]);
    assert!(!page.more());
    assert!(app.dispatch(Action::MoreDiscover).is_empty());
}

#[test]
fn a_list_that_cannot_be_read_says_why_and_keeps_what_is_listed() {
    let mut app = discovering();
    app.dispatch(Action::LoadDiscover {
        query: String::new(),
        category: None,
    });
    app.apply(Event::DiscoverLoaded {
        query: String::new(),
        category: None,
        offset: 0,
        result: Err("Discord is down.".into()),
    });
    let page = &app.state.discover;
    assert_eq!(page.error.as_deref(), Some("Discord is down."));
    assert!(!page.loading);
    assert_eq!(page.servers.len(), 2);
}

#[test]
fn joins_a_listed_server_once_and_remembers_it() {
    let mut app = discovering();
    // Only a server of the list can be joined.
    assert!(app.dispatch(Action::JoinDiscovered("99".into())).is_empty());
    assert!(matches!(
        app.dispatch(Action::JoinDiscovered("10".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::JoinDiscovered { id }))] if id == "10"
    ));
    assert_eq!(app.state.discover.joining, ["10"]);
    assert!(
        app.dispatch(Action::JoinDiscovered("10".into())).is_empty(),
        "one answer at a time"
    );

    app.apply(Event::DiscoverJoined {
        id: "10".into(),
        result: Ok(()),
    });
    let page = &app.state.discover;
    assert!(page.joining.is_empty());
    assert_eq!(page.joined, ["10"]);
    assert!(app.dispatch(Action::JoinDiscovered("10".into())).is_empty());
}

#[cfg(feature = "discord")]
#[test]
fn discover_allows_a_new_join_after_the_account_leaves() {
    use yapfast_discord::{
        ready::Server,
        service::{Snapshot, Status},
    };
    let mut app = discovering();
    app.dispatch(Action::JoinDiscovered("10".into()));
    app.apply(Event::DiscoverJoined {
        id: "10".into(),
        result: Ok(()),
    });
    let snapshot = |servers| {
        Event::Discord(Arc::new(Snapshot {
            status: Status::Online,
            servers,
            ..Default::default()
        }))
    };
    app.apply(snapshot(vec![MarkedServer::from(Server {
        id: "10".into(),
        name: "First".into(),
        ..Default::default()
    })]));
    assert!(app.state.discover.joined.is_empty());
    assert!(app.dispatch(Action::JoinDiscovered("10".into())).is_empty());
    app.apply(snapshot(vec![]));
    assert!(matches!(
        app.dispatch(Action::JoinDiscovered("10".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::JoinDiscovered { id }))] if id == "10"
    ));
}

#[test]
fn a_joined_server_that_has_a_form_shows_it_and_one_without_stays_quiet() {
    use crate::chat::{ScreeningField, ScreeningForm, ScreeningKind};
    let mut app = discovering();
    app.dispatch(Action::JoinDiscovered("10".into()));
    let effects = app.apply(Event::DiscoverJoined {
        id: "10".into(),
        result: Ok(()),
    });
    // The form of the joined server is asked for, by its server id.
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::LoadScreening { server, code }))]
            if server == "discord:10" && code.is_empty()
    ));
    // A server without a form answers with an error, or with no questions.
    app.apply(Event::ScreeningLoaded {
        server: "discord:10".into(),
        result: Err("Unknown form.".into()),
    });
    assert!(app.state.join.is_none());
    let form = |fields| ScreeningForm {
        version: "v1".into(),
        description: String::new(),
        fields,
    };
    app.apply(Event::ScreeningLoaded {
        server: "discord:10".into(),
        result: Ok(form(vec![])),
    });
    assert!(app.state.join.is_none());
    // One that has questions opens the dialog, as for a joined invite.
    let rules = ScreeningField {
        kind: ScreeningKind::Terms,
        label: "Rules".into(),
        description: None,
        required: true,
        values: vec!["Be kind".into()],
        agreed: false,
        text: String::new(),
        choice: None,
    };
    app.apply(Event::ScreeningLoaded {
        server: "discord:10".into(),
        result: Ok(form(vec![rules.clone()])),
    });
    let dialog = app
        .state
        .join
        .as_ref()
        .and_then(|join| join.form.as_ref())
        .expect("the form is shown");
    assert_eq!(dialog.server, "discord:10");
    assert_eq!(dialog.form.as_ref().map(|form| form.fields.len()), Some(1));
    // A form of a server that wasn't joined from the list is not opened.
    app.state.join = None;
    app.apply(Event::ScreeningLoaded {
        server: "discord:77".into(),
        result: Ok(form(vec![rules])),
    });
    assert!(app.state.join.is_none());
}

#[test]
fn a_server_the_account_is_in_is_not_joined_again() {
    let mut app = discovering();
    app.state.servers.push(Arc::new(ChatServer {
        id: "discord:10".into(),
        name: "Server 10".into(),
        ..Default::default()
    }));
    assert!(app.dispatch(Action::JoinDiscovered("10".into())).is_empty());
    assert!(app.state.discover.joining.is_empty());
}

#[test]
fn a_refused_join_says_why_and_can_be_tried_again() {
    let mut app = discovering();
    app.dispatch(Action::JoinDiscovered("20".into()));
    app.apply(Event::DiscoverJoined {
        id: "20".into(),
        result: Err("You are banned from this server.".into()),
    });
    let page = &app.state.discover;
    assert_eq!(
        page.refused.as_deref(),
        Some("You are banned from this server.")
    );
    assert!(page.joining.is_empty() && page.joined.is_empty());
    assert!(!app.dispatch(Action::JoinDiscovered("20".into())).is_empty());
    assert_eq!(app.state.discover.refused, None);
}

#[test]
fn a_join_that_needs_a_captcha_opens_its_page_in_the_browser() {
    let mut app = discovering();
    app.dispatch(Action::JoinDiscovered("10".into()));
    let effects = app.apply(Event::DiscoverCaptcha {
        id: "10".into(),
        page: "http://127.0.0.1:9/".into(),
    });
    assert!(matches!(
        effects.as_slice(),
        [Effect::OpenPage(page)] if page == "http://127.0.0.1:9/"
    ));
    assert_eq!(
        app.state.discover.captcha.as_deref(),
        Some("http://127.0.0.1:9/")
    );
    // A captcha for a server that isn't being joined is ignored.
    assert!(
        app.apply(Event::DiscoverCaptcha {
            id: "20".into(),
            page: "http://127.0.0.1:9/".into(),
        })
        .is_empty()
    );
    app.apply(Event::DiscoverJoined {
        id: "10".into(),
        result: Ok(()),
    });
    assert_eq!(app.state.discover.captcha, None);
}

fn to_give(code: &str) -> GiftToGive {
    GiftToGive {
        code: code.into(),
        name: "Nitro Monthly".into(),
    }
}

#[test]
fn the_gifts_to_give_are_read_once_at_a_time_and_again_when_asked() {
    let mut app = online();
    let effects = app.dispatch(Action::LoadGifts);
    assert!(asks(&effects, |c| matches!(c, DiscordCommand::LoadGifts)));
    assert!(app.state.gifts.loading);
    // Asking while Discord has not answered asks nothing more.
    assert!(app.dispatch(Action::LoadGifts).is_empty());

    app.apply(Event::GiftsLoaded {
        result: Ok(vec![to_give("AAAA")]),
    });
    let gifts = &app.state.gifts;
    assert!(gifts.loaded && !gifts.loading && gifts.error.is_none());
    assert_eq!(gifts.gifts, vec![to_give("AAAA")]);

    // Each opening of the menu reads again, so a gift just bought shows.
    let again = app.dispatch(Action::LoadGifts);
    assert!(asks(&again, |c| matches!(c, DiscordCommand::LoadGifts)));
}

#[test]
fn gifts_that_cannot_be_read_say_why_and_keep_the_last_list() {
    let mut app = online();
    app.dispatch(Action::LoadGifts);
    app.apply(Event::GiftsLoaded {
        result: Ok(vec![to_give("AAAA")]),
    });
    app.dispatch(Action::LoadGifts);
    app.apply(Event::GiftsLoaded {
        result: Err("Discord is down.".into()),
    });
    let gifts = &app.state.gifts;
    assert_eq!(gifts.error.as_deref(), Some("Discord is down."));
    assert!(!gifts.loading);
    assert_eq!(gifts.gifts.len(), 1);
    // The next try clears the error and asks again.
    let effects = app.dispatch(Action::LoadGifts);
    assert!(asks(&effects, |c| matches!(c, DiscordCommand::LoadGifts)));
    assert_eq!(app.state.gifts.error, None);
}

#[test]
fn gifts_are_not_read_in_the_preview_or_without_discord() {
    let mut offline = Application::default();
    assert!(offline.dispatch(Action::LoadGifts).is_empty());
    assert!(!offline.state.gifts.loading);

    let mut preview = online();
    preview.state.preview = true;
    assert!(preview.dispatch(Action::LoadGifts).is_empty());
    assert!(!preview.state.gifts.loading);
}
