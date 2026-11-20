//! The Quests and Shop screens' data: loaded from Discord when the screen
//! opens, refreshed when asked, and the one thing the Quests screen does
//! itself, accepting a quest. What needs Discord's own checkout, buying,
//! and claiming a reward, which may ask for a captcha, opens discord.com.

use super::*;
use crate::storefront::{Category, Quest, QuestStatus};

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
