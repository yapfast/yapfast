//! The Quests and Shop screens' data: quests, accepting one, and the
//! Shop's categories.

use yapfast_discord::{
    quests::{QUEST_HOME_DESKTOP, RewardKind, Status, Task},
    shop::Kind,
};

use super::*;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn lists_and_accepts_quests() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("lists_and_accepts_quests").await;

    let (offered, claimed) = discord.quests().await.unwrap();
    assert_eq!(offered.len(), 3);
    let first = &offered[0];
    assert_eq!(first.name, "Kill the Aliens");
    assert_eq!(first.game, "Cyberalien 2077");
    assert_eq!(first.publisher, "Alien Studios");
    assert_eq!(first.status, Status::Available);
    assert_eq!((first.task, first.target), (Task::Play, 900));
    let reward = first.reward.as_ref().unwrap();
    assert_eq!((reward.kind, reward.orbs), (RewardKind::Orbs, Some(700)));
    assert!(
        first
            .hero
            .as_deref()
            .unwrap()
            .ends_with("/quests/9100000000000000001/hero.png")
    );
    // The way Discord names them: the dark game tile and logo, not the
    // placeholders beside them, and the strip over a card.
    for (picture, file) in [
        (&first.bar, "bar.png"),
        (&first.tile, "tile.png"),
        (&first.logo, "logo.png"),
    ] {
        let picture = picture.as_deref().expect("a picture");
        assert!(picture.ends_with(&format!("/quests/9100000000000000001/{file}")));
    }
    assert_eq!(first.button.as_deref(), Some("Play Now"));

    // One is under way and one is done, unclaimed.
    let watching = &offered[1];
    assert_eq!((watching.status, watching.progress), (Status::Accepted, 20));
    assert_eq!(watching.task, Task::Watch);
    assert_eq!(offered[2].status, Status::Completed);
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].status, Status::Claimed);
    assert_eq!(
        claimed[0].reward.as_ref().map(|reward| reward.kind),
        Some(RewardKind::Decoration)
    );

    // Accepting names where it was done from, and the quest then shows as
    // accepted.
    discord.enroll_quest(&first.id).await.unwrap();
    let (offered, _) = discord.quests().await.unwrap();
    assert_eq!(offered[0].status, Status::Accepted);
    let enrollments: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/quest-enrollments", "")).unwrap();
    let enrollments = enrollments.as_array().unwrap();
    assert_eq!(enrollments.len(), 1);
    assert_eq!(enrollments[0]["id"], first.id.as_str());
    assert_eq!(enrollments[0]["location"], QUEST_HOME_DESKTOP);

    // A quest already accepted, or one Discord doesn't know, is refused.
    assert!(discord.enroll_quest(&first.id).await.is_err());
    assert!(discord.enroll_quest("1").await.is_err());

    // A quest that is done is claimed, and moves to the claimed ones; one
    // that is not done is refused with Discord's reason.
    let done = offered
        .iter()
        .find(|quest| quest.status == Status::Completed)
        .expect("a finished quest")
        .id
        .clone();
    assert!(discord.claim_quest(&first.id, |_| {}).await.is_err());
    discord.claim_quest(&done, |_| {}).await.unwrap();
    let claims: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/quest-claims", "")).unwrap();
    let claims = claims.as_array().unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0]["id"], done.as_str());
    assert_eq!(claims[0]["location"], QUEST_HOME_DESKTOP);
    assert!(discord.claim_quest(&done, |_| {}).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn lists_the_shop_with_prices_and_orbs() {
    let Fixture { _fake, discord, .. } =
        Fixture::start("lists_the_shop_with_prices_and_orbs").await;

    let (categories, orbs) = discord.shop().await.unwrap();
    assert_eq!(orbs, Some(550));
    assert_eq!(categories.len(), 2);

    let vice = &categories[0];
    assert_eq!(vice.name, "Vice City");
    assert!(!vice.orbs_only);
    assert!(
        vice.banner
            .as_deref()
            .unwrap()
            .ends_with("/shop-assets/banner-1.png")
    );
    assert_eq!(vice.products.len(), 3);
    let palm = &vice.products[0];
    assert_eq!(
        (palm.name.as_str(), palm.kind),
        ("Palm Sunset", Kind::Decoration)
    );
    assert_eq!(palm.price.as_ref().unwrap().label(), "\u{20ac}8.99");
    assert_eq!(palm.orbs, None);
    assert!(palm.picture.as_deref().unwrap().ends_with(".png"));
    assert_eq!(vice.products[1].kind, Kind::Nameplate);
    assert_eq!(vice.products[2].kind, Kind::Bundle);

    let lofi = &categories[1];
    assert!(lofi.orbs_only);
    assert_eq!(lofi.products[0].orbs, Some(1200));
    assert!(lofi.products[0].price.is_none());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn buys_a_product_with_orbs_and_the_orbs_fall() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("buys_a_product_with_orbs_and_the_orbs_fall").await;

    let (categories, orbs) = discord.shop().await.unwrap();
    assert_eq!(orbs, Some(550));
    let star = &categories[1].products[2];
    assert_eq!((star.name.as_str(), star.orbs), ("Tiny Star", Some(300)));
    let cosy = &categories[1].products[0];
    assert_eq!(cosy.orbs, Some(1200));

    // Too dear for the Orbs the account has: Discord refuses, nothing is spent.
    assert!(discord.buy_with_orbs(&cosy.sku).await.is_err());
    // A SKU Discord doesn't know is refused too.
    assert!(discord.buy_with_orbs("1").await.is_err());
    let (_, orbs) = discord.shop().await.unwrap();
    assert_eq!(orbs, Some(550));

    discord.buy_with_orbs(&star.sku).await.unwrap();
    let (_, orbs) = discord.shop().await.unwrap();
    assert_eq!(orbs, Some(250));
    let bought: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/orb-purchases", "")).unwrap();
    let bought = bought.as_array().unwrap();
    assert_eq!(bought.len(), 1);
    assert_eq!(bought[0]["sku"], star.sku.as_str());
    assert_eq!(bought[0]["checkout"], true);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn gives_a_server_the_free_boost_and_only_that_one() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("gives_a_server_the_free_boost_and_only_that_one").await;
    const SERVER: &str = "1400000000000000100";

    // One boost is free, one rests after a move and one is on another server.
    let slots = discord.boost_slots(SERVER).await.unwrap();
    assert_eq!(slots.free, ["9300000000000000001"]);
    assert_eq!((slots.here, slots.elsewhere, slots.resting), (0, 1, 1));

    discord.boost_server(SERVER).await.unwrap();
    let slots = discord.boost_slots(SERVER).await.unwrap();
    assert!(slots.free.is_empty());
    assert_eq!((slots.here, slots.elsewhere, slots.resting), (1, 1, 1));
    let placed: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/boosts", "")).unwrap();
    let placed = placed.as_array().unwrap();
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0]["guild"], SERVER);
    assert_eq!(placed[0]["slot"], "9300000000000000001");

    // Nothing is left to give, and Discord is not asked.
    let error = discord.boost_server(SERVER).await.unwrap_err();
    assert!(error.to_string().contains("no free boost"), "{error:#}");
    let placed: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/boosts", "")).unwrap();
    assert_eq!(placed.as_array().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn lists_searches_and_joins_discoverable_servers() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("lists_searches_and_joins_discoverable_servers").await;

    // The first page holds 24 of the 26, the categories come with it, and
    // the next page holds the rest.
    let (first, categories) = discord.discover("", None, 0, true).await.unwrap();
    assert_eq!((first.servers.len(), first.total), (24, 26));
    let categories = categories.expect("the categories");
    assert_eq!(
        categories
            .iter()
            .map(|category| (category.id, category.name.as_str()))
            .collect::<Vec<_>>(),
        [(1, "Gaming"), (3, "Music")]
    );
    let rust = &first.servers[0];
    assert_eq!(rust.name, "Rustaceans");
    assert_eq!(rust.description.as_deref(), Some("All about Rustaceans."));
    assert_eq!((rust.members, rust.online), (Some(1001), Some(101)));
    assert_eq!(rust.category.as_deref(), Some("Gaming"));
    assert!(rust.icon.as_deref().unwrap().contains("/icons/"));
    assert!(
        first.servers[1]
            .splash
            .as_deref()
            .unwrap()
            .contains("/discovery-splashes/")
    );
    let (rest, none) = discord.discover("", None, 24, false).await.unwrap();
    assert_eq!(rest.servers.len(), 2);
    assert!(none.is_none());

    // A category narrows the list, and so do words, which count the matches.
    let (music, _) = discord.discover("", Some(3), 0, false).await.unwrap();
    assert_eq!(music.total, 13);
    assert!(
        music
            .servers
            .iter()
            .all(|s| s.category.as_deref() == Some("Music"))
    );
    let (found, _) = discord.discover("  synth ", None, 0, false).await.unwrap();
    assert_eq!(
        found
            .servers
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["Synthwave"]
    );
    let (none, _) = discord.discover("zzz", Some(1), 0, false).await.unwrap();
    assert_eq!((none.servers.len(), none.total), (0, 0));

    // Joining sends where it came from, once; a second join is refused.
    discord
        .join_discoverable(&rust.id, |_| panic!("no captcha"))
        .await
        .unwrap();
    let joins: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/discovery-joins", "")).unwrap();
    let joins = joins.as_array().unwrap();
    assert_eq!(joins.len(), 1);
    assert_eq!(joins[0]["guild"], rust.id.as_str());
    assert_eq!(joins[0]["location"], "Guild Discovery");
    assert_eq!(joins[0]["lurker"], "false");
    let again = discord
        .join_discoverable(&rust.id, |_| panic!("no captcha"))
        .await
        .unwrap_err();
    assert!(
        format!("{again:#}").contains("400 Bad Request"),
        "{again:#}"
    );
    let joins: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/discovery-joins", "")).unwrap();
    assert_eq!(joins.as_array().unwrap().len(), 1);

    // A captcha is shown first, and the join goes through once it is solved.
    post_control(server, "/_control/discovery-captcha", "");
    let mut pages = Vec::new();
    discord
        .join_discoverable(&first.servers[1].id, |page| {
            pages.push(page.to_owned());
            let page = page.to_owned();
            tokio::spawn(async move {
                reqwest::Client::new()
                    .post(page)
                    .body("solved")
                    .send()
                    .await
            });
        })
        .await
        .unwrap();
    assert_eq!(pages.len(), 1);
    let joins: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/discovery-joins", "")).unwrap();
    assert_eq!(joins.as_array().unwrap().len(), 2);
}
