//! The Quests and Shop screens' data: loaded from Discord when the screen
//! opens, refreshed when asked, and the one thing the Quests screen does
//! itself: accepting a quest, and claiming a finished quest's reward, which
//! may ask for a captcha that the browser shows, as joining a server does.
//! The Shop's products that cost Orbs are bought here too. What needs
//! Discord's own checkout, paying money, Yapfast doesn't offer.

use super::*;
use crate::storefront::{BoostPage, BoostSlots, Category, Quest, QuestStatus};

impl Application {
    /// What opening a screen of the DM list loads, if it shows Discord's
    /// quests or Shop.
    pub(super) fn load_screen(&mut self, screen: DmScreen) -> Vec<Effect> {
        match screen {
            DmScreen::Quests => self.load_quests(),
            DmScreen::Shop => self.load_shop(),
            DmScreen::Friends | DmScreen::Requests | DmScreen::Nitro => vec![],
        }
    }

    pub(super) fn load_quests(&mut self) -> Vec<Effect> {
        if self.state.preview || !self.state.discord_online || self.state.quests.loading {
            return vec![];
        }
        self.state.quests.loading = true;
        self.state.quests.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::LoadQuests,
        ))]
    }

    pub(super) fn quests_loaded(
        &mut self,
        result: Result<(Vec<Quest>, Vec<Quest>), String>,
    ) -> Vec<Effect> {
        let page = &mut self.state.quests;
        page.loading = false;
        match result {
            Ok((offered, claimed)) => {
                page.offered = offered;
                page.claimed = claimed;
                page.loaded = true;
                page.error = None;
            }
            Err(error) => page.error = Some(error),
        }
        vec![]
    }

    /// Accepts a quest that is on offer, once at a time.
    pub(super) fn accept_quest(&mut self, id: String) -> Vec<Effect> {
        let page = &mut self.state.quests;
        if self.state.preview
            || page.accepting.contains(&id)
            || !page
                .offered
                .iter()
                .any(|quest| quest.id == id && quest.status == QuestStatus::Available)
        {
            return vec![];
        }
        page.accepting.push(id.clone());
        page.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::AcceptQuest { id },
        ))]
    }

    pub(super) fn quest_accepted(&mut self, id: String, result: Result<(), String>) -> Vec<Effect> {
        let page = &mut self.state.quests;
        page.accepting.retain(|accepting| *accepting != id);
        match result {
            Ok(()) => {
                if let Some(quest) = page.offered.iter_mut().find(|quest| quest.id == id) {
                    quest.status = QuestStatus::Accepted;
                }
                // Discord's own numbers replace the guess.
                self.load_quests()
            }
            Err(error) => {
                page.error = Some(error);
                vec![]
            }
        }
    }

    /// Claims the reward of a quest that is done, once at a time.
    pub(super) fn claim_quest(&mut self, id: String) -> Vec<Effect> {
        let page = &mut self.state.quests;
        if self.state.preview
            || page.claiming.contains(&id)
            || !page
                .offered
                .iter()
                .any(|quest| quest.id == id && quest.status == QuestStatus::Completed)
        {
            return vec![];
        }
        page.claiming.push(id.clone());
        page.error = None;
        page.captcha = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ClaimQuest { id },
        ))]
    }

    /// Opens the page with the captcha Discord asked for before a claim.
    pub(super) fn quest_captcha(&mut self, id: String, page: String) -> Vec<Effect> {
        let quests = &mut self.state.quests;
        if !quests.claiming.contains(&id) {
            return vec![];
        }
        quests.captcha = Some(page.clone());
        vec![Effect::OpenPage(page)]
    }

    pub(super) fn quest_claimed(&mut self, id: String, result: Result<(), String>) -> Vec<Effect> {
        let page = &mut self.state.quests;
        page.claiming.retain(|claiming| *claiming != id);
        page.captcha = None;
        match result {
            Ok(()) => {
                if let Some(quest) = page.offered.iter_mut().find(|quest| quest.id == id) {
                    quest.status = QuestStatus::Claimed;
                }
                // The claimed list now holds it.
                self.load_quests()
            }
            Err(error) => {
                page.error = Some(error);
                vec![]
            }
        }
    }

    /// Buys a product that costs Orbs, once at a time and only with Orbs
    /// enough: Discord's number is the account's, and when it hasn't been
    /// read the purchase is left to Discord to refuse.
    pub(super) fn buy_product(&mut self, sku: String) -> Vec<Effect> {
        let page = &mut self.state.shop;
        let Some(cost) = page.cost(&sku) else {
            return vec![];
        };
        if self.state.preview || page.buying.contains(&sku) || page.bought.contains(&sku) {
            return vec![];
        }
        if page.orbs.is_some_and(|orbs| orbs < cost) {
            page.refused = Some("You don't have enough Orbs.".to_owned());
            return vec![];
        }
        page.buying.push(sku.clone());
        page.refused = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::BuyProduct { sku },
        ))]
    }

    pub(super) fn product_bought(
        &mut self,
        sku: String,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let page = &mut self.state.shop;
        page.buying.retain(|buying| *buying != sku);
        match result {
            Ok(()) => {
                page.bought.push(sku);
                // The Orbs left are Discord's number.
                self.load_shop()
            }
            Err(error) => {
                page.refused = Some(error);
                vec![]
            }
        }
    }

    /// Reads the boosts the account can give the server, when the server's
    /// boost progress opens. What was read for another server is dropped.
    pub(super) fn load_boosts(&mut self, server: String) -> Vec<Effect> {
        if self.state.preview
            || !self.state.discord_online
            || crate::discord::parse_server(&server).is_none()
        {
            return vec![];
        }
        let page = &mut self.state.boosts;
        if page.server.as_deref() != Some(server.as_str()) {
            *page = BoostPage {
                server: Some(server.clone()),
                ..BoostPage::default()
            };
        }
        if page.loading {
            return vec![];
        }
        page.loading = true;
        page.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::LoadBoosts { server },
        ))]
    }

    pub(super) fn boosts_loaded(
        &mut self,
        server: String,
        result: Result<BoostSlots, String>,
    ) -> Vec<Effect> {
        let page = &mut self.state.boosts;
        if page.server.as_deref() != Some(server.as_str()) {
            return vec![];
        }
        page.loading = false;
        match result {
            Ok(slots) => {
                page.slots = Some(slots);
                page.error = None;
            }
            Err(error) => page.error = Some(error),
        }
        vec![]
    }

    /// Gives the server a boost, once at a time and only when the account
    /// has a free one.
    pub(super) fn boost_server(&mut self, server: String) -> Vec<Effect> {
        let page = &mut self.state.boosts;
        if self.state.preview
            || page.server.as_deref() != Some(server.as_str())
            || page.applying
            || page.slots.is_none_or(|slots| slots.free == 0)
        {
            return vec![];
        }
        page.applying = true;
        page.given = false;
        page.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::BoostServer { server },
        ))]
    }

    pub(super) fn server_boosted(
        &mut self,
        server: String,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let page = &mut self.state.boosts;
        if page.server.as_deref() != Some(server.as_str()) {
            return vec![];
        }
        page.applying = false;
        match result {
            Ok(()) => {
                page.given = true;
                // What is left to give is Discord's number.
                self.load_boosts(server)
            }
            Err(error) => {
                page.error = Some(error);
                vec![]
            }
        }
    }

    pub(super) fn load_shop(&mut self) -> Vec<Effect> {
        if self.state.preview || !self.state.discord_online || self.state.shop.loading {
            return vec![];
        }
        self.state.shop.loading = true;
        self.state.shop.error = None;
        vec![Effect::Backend(Command::Discord(DiscordCommand::LoadShop))]
    }

    pub(super) fn shop_loaded(
        &mut self,
        result: Result<(Vec<Category>, Option<u64>), String>,
    ) -> Vec<Effect> {
        let page = &mut self.state.shop;
        page.loading = false;
        match result {
            Ok((categories, orbs)) => {
                page.categories = categories;
                page.orbs = orbs;
                page.loaded = true;
                page.error = None;
            }
            Err(error) => page.error = Some(error),
        }
        vec![]
    }
}
