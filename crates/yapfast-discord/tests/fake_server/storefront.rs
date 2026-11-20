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
    assert!(first.hero.as_deref().unwrap().ends_with("/quests/9100000000000000001/hero.png"));
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
        serde_json::from_str(&control(server, "GET", "/_control/quest-enrollments", ""))
            .unwrap();
    let enrollments = enrollments.as_array().unwrap();
    assert_eq!(enrollments.len(), 1);
    assert_eq!(enrollments[0]["id"], first.id.as_str());
    assert_eq!(enrollments[0]["location"], QUEST_HOME_DESKTOP);

    // A quest already accepted, or one Discord doesn't know, is refused.
    assert!(discord.enroll_quest(&first.id).await.is_err());
    assert!(discord.enroll_quest("1").await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn lists_the_shop_with_prices_and_orbs() {
    let Fixture {
        _fake, discord, ..
    } = Fixture::start("lists_the_shop_with_prices_and_orbs").await;

    let (categories, orbs) = discord.shop().await.unwrap();
    assert_eq!(orbs, Some(550));
    assert_eq!(categories.len(), 2);

    let vice = &categories[0];
    assert_eq!(vice.name, "Vice City");
    assert!(!vice.orbs_only);
    assert!(vice.banner.as_deref().unwrap().ends_with("/shop-assets/banner-1.png"));
    assert_eq!(vice.products.len(), 3);
    let palm = &vice.products[0];
    assert_eq!((palm.name.as_str(), palm.kind), ("Palm Sunset", Kind::Decoration));
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
