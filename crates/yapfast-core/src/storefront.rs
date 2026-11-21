//! What the Quests and Shop screens show, as the core holds it: Discord's
//! quests with the account's progress, and the Shop's categories with
//! their products. Discord's answers are read in `yapfast_discord::quests`
//! and `yapfast_discord::shop`; these are the same facts without Discord's
//! crate, so the window builds without it.

/// How far this account is with a quest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuestStatus {
    /// Not accepted.
    #[default]
    Available,
    /// Accepted, and not yet done.
    Accepted,
    /// Done, and its reward not claimed.
    Completed,
    /// Done and claimed.
    Claimed,
}

/// What a quest's reward is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuestRewardKind {
    #[default]
    Other,
    Code,
    InGame,
    Decoration,
    Orbs,
    Nitro,
}

/// What a quest asks of the account.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuestTask {
    /// Play the game on a computer.
    #[default]
    Play,
    /// Play it and stream it.
    Stream,
    /// Watch a video.
    Watch,
    /// Play an activity inside Discord.
    Activity,
    /// Something the game reports itself.
    Achievement,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestReward {
    pub kind: QuestRewardKind,
    /// Its name, such as "Cybernetic Headgear Decoration".
    pub name: String,
    /// The Orbs it gives, for Orbs.
    pub orbs: Option<u64>,
    pub picture: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Quest {
    pub id: String,
    pub name: String,
    pub game: String,
    pub publisher: String,
    /// The wide picture of the quest's banner.
    pub hero: Option<String>,
    /// The picture over a quest's card, narrower than the banner's.
    pub bar: Option<String>,
    pub tile: Option<String>,
    pub logo: Option<String>,
    /// The gradient the quest's card takes, as `0xRRGGBB`.
    pub colors: Option<[u32; 2]>,
    /// When it ends, in seconds since 1970.
    pub expires: Option<i64>,
    pub reward: Option<QuestReward>,
    pub task: QuestTask,
    /// What the task asks, in seconds for a task of time.
    pub target: u64,
    /// How much of it is done, in the same unit.
    pub progress: u64,
    pub status: QuestStatus,
    /// The game's page or store, and what its button says.
    pub link: Option<String>,
    pub button: Option<String>,
}

impl Quest {
    /// The share of the task done, from 0 to 1.
    pub fn fraction(&self) -> f32 {
        match self.status {
            QuestStatus::Completed | QuestStatus::Claimed => 1.0,
            _ if self.target == 0 => 0.0,
            _ => (self.progress as f32 / self.target as f32).clamp(0.0, 1.0),
        }
    }

    /// Whole minutes of the task still to do, rounded up.
    pub fn minutes_left(&self) -> u64 {
        self.target.saturating_sub(self.progress).div_ceil(60)
    }
}

/// The Quests screen's data: what Discord lists, loaded when it opens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestsPage {
    pub offered: Vec<Quest>,
    pub claimed: Vec<Quest>,
    /// Asked for and not answered.
    pub loading: bool,
    /// Answered once, so an empty list means none, not none yet.
    pub loaded: bool,
    pub error: Option<String>,
    /// The quests being accepted, until Discord answers.
    pub accepting: Vec<String>,
    /// The quests whose rewards are being claimed, until Discord answers,
    /// which may wait on a captcha.
    pub claiming: Vec<String>,
    /// The page with the captcha Discord asked for before a claim, to open
    /// again if the browser tab was closed.
    pub captcha: Option<String>,
}

/// What a product is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProductKind {
    #[default]
    Other,
    Decoration,
    Effect,
    Nameplate,
    Frame,
    Bundle,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Product {
    pub sku: String,
    pub name: String,
    pub summary: String,
    pub kind: ProductKind,
    pub picture: Option<String>,
    /// Its price in money, as it reads, such as "$8.99".
    pub price: Option<String>,
    /// What it costs in Orbs, if Orbs buy it.
    pub orbs: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Category {
    pub sku: String,
    pub name: String,
    pub summary: String,
    pub banner: Option<String>,
    pub logo: Option<String>,
    /// Every product costs Orbs.
    pub orbs_only: bool,
    pub products: Vec<Product>,
}

impl Category {
    /// The picture on this category's card: its banner, or its first
    /// product's picture.
    pub fn cover(&self) -> Option<&str> {
        self.banner.as_deref().or_else(|| {
            self.products
                .iter()
                .find_map(|product| product.picture.as_deref())
        })
    }
}

/// What the account's boosts are doing, for one server.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoostSlots {
    /// The boosts that can be given to a server now.
    pub free: u32,
    /// The boosts the account has given this server.
    pub here: u32,
    /// The boosts the account has given other servers.
    pub elsewhere: u32,
    /// The free boosts that are resting after being moved.
    pub resting: u32,
}

/// The boosts the account can give, read for the server whose progress is
/// open, and what giving one came to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoostPage {
    /// The server these answers are for, by its id.
    pub server: Option<String>,
    /// What Discord said of the account's boosts.
    pub slots: Option<BoostSlots>,
    /// Asked for and not answered.
    pub loading: bool,
    /// A boost is being given, until Discord answers.
    pub applying: bool,
    /// Why the last asking or giving failed.
    pub error: Option<String>,
    /// A boost was given to the server in this run.
    pub given: bool,
}

/// The Shop screen's data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShopPage {
    pub categories: Vec<Category>,
    /// The account's Orbs, when Discord said.
    pub orbs: Option<u64>,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
    /// The products being bought with Orbs, by SKU, until Discord answers.
    pub buying: Vec<String>,
    /// The products bought in this run, by SKU.
    pub bought: Vec<String>,
    /// Why the last purchase didn't go through.
    pub refused: Option<String>,
}

impl ShopPage {
    /// The Orbs a product costs, if Orbs buy it.
    pub fn cost(&self, sku: &str) -> Option<u64> {
        self.categories
            .iter()
            .flat_map(|category| &category.products)
            .find(|product| product.sku == sku)
            .and_then(|product| product.orbs)
    }

    /// The products whose name or summary holds `query`, in the order the
    /// categories list them; every product for an empty query.
    pub fn search(&self, query: &str) -> Vec<&Product> {
        let query = query.trim().to_lowercase();
        self.categories
            .iter()
            .flat_map(|category| &category.products)
            .filter(|product| {
                query.is_empty()
                    || product.name.to_lowercase().contains(&query)
                    || product.summary.to_lowercase().contains(&query)
            })
            .collect()
    }
}

/// A server on the Discover page: a card with its picture, name, words and
/// numbers, and what joining it comes to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscoverServer {
    /// Discord's id for the server, as joining it asks for.
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// The icon's address.
    pub icon: Option<String>,
    /// The picture that heads its card.
    pub splash: Option<String>,
    pub members: Option<u64>,
    pub online: Option<u64>,
    /// The name of the server's primary category.
    pub category: Option<String>,
}

/// One of Discord's categories of servers, a chip on the Discover page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscoverCategory {
    pub id: u32,
    pub name: String,
}

/// What one asking of the Discover list came back with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscoverBatch {
    pub servers: Vec<DiscoverServer>,
    /// How many servers match in all.
    pub total: u64,
    /// Discord's categories, when they were asked for and said.
    pub categories: Option<Vec<DiscoverCategory>>,
}

/// The Discover screen's data: the servers Discord lists for the words
/// searched and the category chosen, a page at a time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscoverPage {
    pub categories: Vec<DiscoverCategory>,
    pub servers: Vec<DiscoverServer>,
    /// How many servers match the query and category in all.
    pub total: u64,
    /// The words the list is for; the whole list for none.
    pub query: String,
    /// The category the list is within, by Discord's number.
    pub category: Option<u32>,
    /// A page is being asked for.
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
    /// The servers being joined, by id, until Discord answers.
    pub joining: Vec<String>,
    /// The servers joined in this run, by id.
    pub joined: Vec<String>,
    /// Why the last join didn't go through.
    pub refused: Option<String>,
    /// The address of the captcha page a join is waiting on.
    pub captcha: Option<String>,
}

impl DiscoverPage {
    /// Whether Discord has more servers for this query and category than
    /// are listed.
    pub fn more(&self) -> bool {
        self.loaded && (self.servers.len() as u64) < self.total
    }
}

/// A gift the account bought and can still give, by its code.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GiftToGive {
    pub code: String,
    /// What it grants, such as "Nitro Monthly".
    pub name: String,
}

/// The gifts the composer's Gift button offers to put in a chat, read when
/// it opens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GiftInventory {
    pub gifts: Vec<GiftToGive>,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(name: &str) -> Product {
        Product {
            sku: name.into(),
            name: name.into(),
            ..Product::default()
        }
    }

    #[test]
    fn a_quest_counts_its_task_in_minutes() {
        let mut quest = Quest {
            target: 900,
            progress: 301,
            status: QuestStatus::Accepted,
            ..Quest::default()
        };
        assert_eq!(quest.minutes_left(), 10, "599 s left rounds up");
        assert!((quest.fraction() - 301.0 / 900.0).abs() < 0.0001);
        quest.status = QuestStatus::Completed;
        assert_eq!(quest.fraction(), 1.0);
        assert_eq!(Quest::default().fraction(), 0.0, "no target, no progress");
    }

    #[test]
    fn the_shop_finds_products_by_name_or_summary() {
        let page = ShopPage {
            categories: vec![
                Category {
                    products: vec![product("Palm"), product("Neon plate")],
                    ..Category::default()
                },
                Category {
                    products: vec![Product {
                        summary: "A palm tree at night".into(),
                        ..product("Dusk")
                    }],
                    banner: Some("banner".into()),
                    ..Category::default()
                },
            ],
            ..ShopPage::default()
        };
        assert_eq!(page.search("").len(), 3);
        let found: Vec<_> = page
            .search(" PALM ")
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(found, ["Palm", "Dusk"]);
        assert!(page.search("zzz").is_empty());
        assert_eq!(page.categories[1].cover(), Some("banner"));
        assert_eq!(page.categories[0].cover(), None);
    }

    #[test]
    fn the_discover_list_has_more_until_it_holds_every_match() {
        let server = |id: &str| DiscoverServer {
            id: id.into(),
            name: id.into(),
            ..DiscoverServer::default()
        };
        let mut page = DiscoverPage {
            servers: vec![server("1"), server("2")],
            total: 3,
            ..DiscoverPage::default()
        };
        assert!(!page.more(), "nothing is offered before the list loaded");
        page.loaded = true;
        assert!(page.more());
        page.servers.push(server("3"));
        assert!(!page.more());
    }
}
