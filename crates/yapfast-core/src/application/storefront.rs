//! The Quests and Shop screens' data: loaded from Discord when the screen
//! opens, refreshed when asked, and the one thing the Quests screen does
//! itself: accepting a quest, and claiming a finished quest's reward, which
//! may ask for a captcha that the browser shows, as joining a server does.
//! The Shop's products that cost Orbs are bought here too. What needs
//! Discord's own checkout, paying money, Yapfast doesn't offer. The Discover
//! screen lists the servers anyone may join, a page at a time, and joins
//! them.

use super::*;
use crate::storefront::{
    BoostPage, BoostSlots, Category, DiscoverBatch, GiftToGive, Quest, QuestStatus,
};

/// How many seconds of a quest's video Discord is told of at a time.
const REPORT_EVERY: u64 = 5;

impl Application {
    /// What opening a screen of the DM list loads, if it shows Discord's
    /// quests or Shop.
    pub(super) fn load_screen(&mut self, screen: DmScreen) -> Vec<Effect> {
        match screen {
            DmScreen::Quests => self.load_quests(),
            DmScreen::Shop => self.load_shop(),
            DmScreen::Discover => {
                let page = &self.state.discover;
                let (query, category) = (page.query.clone(), page.category);
                self.load_discover(query, category, 0)
            }
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
                // A quest that Discord refuses may have ended since the list
                // came: the list is read again, and says why it was refused.
                let reload = self.load_quests();
                self.state.quests.error = Some(error);
                reload
            }
        }
    }

    /// Plays the video of a quest of watching that was accepted, at most
    /// `width` by `height` pixels. What it has watched so far, Discord
    /// already counts.
    pub(super) fn play_quest_video(&mut self, id: String, width: u32, height: u32) -> Vec<Effect> {
        let page = &mut self.state.quests;
        if self.state.preview {
            return vec![];
        }
        let Some((url, progress)) = page
            .offered
            .iter()
            .find(|quest| quest.id == id && quest.status == QuestStatus::Accepted)
            .and_then(|quest| Some((quest.video.as_ref()?.url.clone(), quest.progress)))
        else {
            return vec![];
        };
        page.watching = Some((id.clone(), progress));
        page.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::PlayQuestVideo {
                id,
                url,
                width,
                height,
                speakers: self.state.voice.speaker.clone(),
            },
        ))]
    }

    /// The quest's video is `at_ms` into its playing: Discord hears of it
    /// every few seconds, as a person's watching counts.
    pub(super) fn quest_video_at(&mut self, at_ms: u64) -> Vec<Effect> {
        self.report_quest_video(at_ms / 1000, REPORT_EVERY)
    }

    /// Tells Discord the quest's video playing has reached `seconds`, if
    /// that's `least` seconds past what it was last told.
    fn report_quest_video(&mut self, seconds: u64, least: u64) -> Vec<Effect> {
        let Some(id) = self
            .state
            .video
            .as_ref()
            .filter(|video| video.room == crate::chat::QUEST_VIDEO_ROOM)
            .map(|video| video.item.clone())
        else {
            return vec![];
        };
        let page = &mut self.state.quests;
        let Some((_, reported)) = page
            .watching
            .as_mut()
            .filter(|(watching, reported)| *watching == id && seconds >= reported + least)
        else {
            return vec![];
        };
        let Some(target) = page
            .offered
            .iter()
            .find(|quest| quest.id == id && quest.status == QuestStatus::Accepted)
            .map(|quest| quest.target)
        else {
            return vec![];
        };
        *reported = seconds;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ReportQuestVideo {
                id,
                seconds,
                target,
            },
        ))]
    }

    /// The quest's video played to its end: Discord hears of all of it.
    pub(super) fn quest_video_ended(&mut self) -> Vec<Effect> {
        let Some(seconds) = self
            .state
            .video
            .as_ref()
            .filter(|video| video.room == crate::chat::QUEST_VIDEO_ROOM && video.ended)
            .and_then(|video| video.duration_ms)
        else {
            return vec![];
        };
        self.report_quest_video(seconds.div_ceil(1000), 1)
    }

    /// What Discord counts of a quest's video watched.
    pub(super) fn quest_progress(
        &mut self,
        id: String,
        result: Result<(QuestStatus, u64), String>,
    ) -> Vec<Effect> {
        let page = &mut self.state.quests;
        let (status, done) = match result {
            Ok(counted) => counted,
            Err(error) => {
                page.error = Some(error);
                return vec![];
            }
        };
        let Some(quest) = page.offered.iter_mut().find(|quest| quest.id == id) else {
            return vec![];
        };
        quest.progress = quest.progress.max(done);
        // Only forward: an answer that says less of a quest than the list
        // did, is not taken for the quest having gone back.
        if matches!(status, QuestStatus::Completed | QuestStatus::Claimed)
            && quest.status == QuestStatus::Accepted
        {
            quest.status = QuestStatus::Completed;
            // Discord's own numbers and the reward's state.
            return self.load_quests();
        }
        vec![]
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

    /// Asks for a page of the discoverable servers: the first for a new
    /// query or category, which replaces the list when it arrives, or the
    /// next for the same ones. The categories are asked for with the first
    /// page until they have been read.
    pub(super) fn load_discover(
        &mut self,
        query: String,
        category: Option<u32>,
        offset: u32,
    ) -> Vec<Effect> {
        let query = query.trim().to_owned();
        let page = &mut self.state.discover;
        if self.state.preview
            || !self.state.discord_online
            || (page.loading && page.query == query && page.category == category)
        {
            return vec![];
        }
        if page.query != query || page.category != category {
            page.servers.clear();
            page.total = 0;
        }
        page.query = query.clone();
        page.category = category;
        page.loading = true;
        page.error = None;
        let categories = offset == 0 && page.categories.is_empty();
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::LoadDiscover {
                query,
                category,
                offset,
                categories,
            },
        ))]
    }

    /// Asks for the page after the servers listed, when Discord has more.
    pub(super) fn more_discover(&mut self) -> Vec<Effect> {
        let page = &self.state.discover;
        if page.loading || !page.more() {
            return vec![];
        }
        let (query, category) = (page.query.clone(), page.category);
        let offset = u32::try_from(page.servers.len()).unwrap_or(u32::MAX);
        self.load_discover(query, category, offset)
    }

    pub(super) fn discover_loaded(
        &mut self,
        query: String,
        category: Option<u32>,
        offset: u32,
        result: Result<DiscoverBatch, String>,
    ) -> Vec<Effect> {
        let page = &mut self.state.discover;
        // An answer for words or a category that were left, or for a page
        // that doesn't follow the list, is dropped.
        let follows = if offset == 0 {
            true
        } else {
            usize::try_from(offset).is_ok_and(|offset| offset == page.servers.len())
        };
        if page.query != query || page.category != category || !follows {
            return vec![];
        }
        page.loading = false;
        match result {
            Ok(batch) => {
                if offset == 0 {
                    page.servers = batch.servers;
                } else {
                    for server in batch.servers {
                        if !page.servers.iter().any(|shown| shown.id == server.id) {
                            page.servers.push(server);
                        }
                    }
                }
                page.total = batch.total;
                if let Some(categories) = batch.categories {
                    page.categories = categories;
                }
                page.loaded = true;
                page.error = None;
            }
            Err(error) => page.error = Some(error),
        }
        vec![]
    }

    /// Joins a server of the list, once at a time, unless the account is in
    /// it already.
    pub(super) fn join_discovered(&mut self, id: String) -> Vec<Effect> {
        let member = self
            .state
            .servers
            .iter()
            .any(|server| server.id == crate::discord::server_id(&id));
        let page = &mut self.state.discover;
        if self.state.preview
            || !self.state.discord_online
            || member
            || page.joining.contains(&id)
            || page.joined.contains(&id)
            || !page.servers.iter().any(|server| server.id == id)
        {
            return vec![];
        }
        page.joining.push(id.clone());
        page.refused = None;
        page.captcha = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::JoinDiscovered { id },
        ))]
    }

    /// Opens the page with the captcha Discord asked for before a join.
    pub(super) fn discover_captcha(&mut self, id: String, page: String) -> Vec<Effect> {
        let discover = &mut self.state.discover;
        if !discover.joining.contains(&id) {
            return vec![];
        }
        discover.captcha = Some(page.clone());
        vec![Effect::OpenPage(page)]
    }

    pub(super) fn discover_joined(
        &mut self,
        id: String,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let page = &mut self.state.discover;
        page.joining.retain(|joining| *joining != id);
        page.captcha = None;
        match result {
            Ok(()) => {
                let server = crate::discord::server_id(&id);
                if !self.state.servers.iter().any(|known| known.id == server) {
                    page.joined.push(id);
                }
                // A server may ask new members to fill in its form first;
                // it's shown if it has one.
                vec![Effect::Backend(Command::Discord(
                    DiscordCommand::LoadScreening {
                        server,
                        code: String::new(),
                    },
                ))]
            }
            Err(error) => {
                page.refused = Some(error);
                vec![]
            }
        }
    }

    /// Reads the gifts the account can still give, once at a time.
    pub(super) fn load_gifts(&mut self) -> Vec<Effect> {
        let gifts = &mut self.state.gifts;
        if self.state.preview || !self.state.discord_online || gifts.loading {
            return vec![];
        }
        gifts.loading = true;
        gifts.error = None;
        vec![Effect::Backend(Command::Discord(DiscordCommand::LoadGifts))]
    }

    pub(super) fn gifts_loaded(&mut self, result: Result<Vec<GiftToGive>, String>) -> Vec<Effect> {
        let gifts = &mut self.state.gifts;
        gifts.loading = false;
        match result {
            Ok(list) => {
                gifts.gifts = list;
                gifts.loaded = true;
                gifts.error = None;
            }
            Err(error) => gifts.error = Some(error),
        }
        vec![]
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
