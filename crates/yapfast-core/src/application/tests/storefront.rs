//! The Quests and Shop screens: loading their data when they open, and
//! accepting a quest.

use yapfast_discord::service::{Snapshot, Status};

use super::*;
use crate::storefront::{Category, Product, Quest, QuestStatus};

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

    // Coming back to the screen refreshes what it shows.
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
