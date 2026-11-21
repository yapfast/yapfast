# Nitro, Shop, Quests and Discover

The three screens under Friends and Requests on the DM home, and Discover, which the rail's
compass tile opens. Each opens in
the conversation's place, as Discord's do, from a row of the DM list
(`Action::OpenDmScreen`), with its own header: the screen's icon, its tabs
as words with a line under the open one, and what Discord puts at the end.
They are in the back and forward history like Friends and Requests, and need
a linked account (they are absent in the preview and a server).

Everything on them that can be done without paying is native, and nothing
sends the user to discord.com. Yapfast has no checkout, so it does not sell
anything for money: the Nitro screen shows what Nitro gives and whether the
account has it and redeems a gift code or gift link typed into it (the gift's
own dialog, as for a link in a chat), and the Shop shows a product priced only in money with its
price and nothing to press. A product that costs Orbs is bought in Yapfast
with the account's Orbs. Claiming a quest's reward is asked of Discord by
Yapfast; when Discord wants a captcha first, its page opens in the browser as
[linking](README.md#account-linking) and joining a server do, and the claim
goes on once it is solved.

| Screen | Tabs | Native |
| --- | --- | --- |
| Quests | All Quests, Claimed Quests | The list with the first quest as the wide banner, each card's picture, game, sponsor, end date, reward, task, progress and its game's link; accepting a quest; claiming a finished quest's reward |
| Shop | Featured, Browse, Orbs Shop | Categories with banners, their products with pictures and prices, the account's Orbs, a search, one category alone; buying a product that costs Orbs |
| Discover | one tab, with a search field | The servers Discord lists as joinable, a card each with its splash, icon, name, category, words and how many are in it and online; Discord's categories as chips; a search; Show more; joining a server |
| Nitro | Home, Perks, Plans, Compare | What Nitro gives, whether the account has it, redeeming a gift code or link (`Action::OpenGiftLink`, `gift::from_text` reads the code out of what was typed), each server's boost level and progress, Free against Nitro |

## The data

`yapfast-discord/src/quests.rs` and `shop.rs` read Discord's answers into the
core's types (`yapfast_core::storefront`), so the window builds without
Discord's crate. They are loaded by the core when a screen opens
(`Application::load_screen`) and again on **Refresh** or **Retry**
(`Action::RefreshQuests`, `Action::RefreshShop`); the Nitro screen needs
none.

| Request | For |
| --- | --- |
| `GET /quests/@me` | The quests on offer, with the account's enrollment and progress |
| `GET /quests/@me/claimed` | The Claimed Quests tab |
| `POST /quests/{id}/enroll` | Accepting a quest (`Action::AcceptQuest`) |
| `POST /quests/{id}/claim-reward` | Claiming a finished quest's reward (`Action::ClaimQuest`); `errors` in the answer is the reason it failed |
| `GET /collectibles-categories` | The Shop's categories and products |
| `GET /users/@me/virtual-currency/balance` | The Orbs in the Shop's header |
| `POST /virtual-currency/skus/{sku}/redeem` | Buying a product with Orbs (`Action::BuyProduct`); no money is involved |
| `GET /discoverable-guilds?limit=24&offset=&categories=` | A page of the Discover list (`{guilds, total}`) |
| `GET /discoverable-guilds/search?query=&category_id=` | The same list narrowed to the words typed |
| `GET /discovery/categories` | The category chips, asked for once with the first page |
| `PUT /guilds/{id}/members/@me?lurker=false&location=Guild%20Discovery` | Joining a server of the list (`Action::JoinDiscovered`) |

Both parsers read defensively: a field Discord has changed or left out loses
that field (a price, a picture), not the list. A product's price is shown as
Discord words it; Yapfast keeps none of its own, and the Nitro screen shows
no price either, as it varies by country.

A quest being accepted is in `QuestsPage::accepting` until Discord answers,
so a second press does nothing and its button reads "Accepting". On success
the quest turns `Accepted` with Discord's own progress; on failure the page
keeps its list and shows the reason with a Retry.

A reward being claimed is in `QuestsPage::claiming` the same way, its button
reading "Claiming". Discord may ask for a captcha first: the page is served on
the loopback address and opened in the browser (`Event::QuestCaptcha`,
`DiscordCommand::ClaimQuest`, the same `captcha::solving` loop as joining a
server), kept in `QuestsPage::captcha`, and the claim is sent again with the
solution. Some rewards need more than a claim (a shipping address); Discord
then refuses with its reason, which the page shows.

A product that costs Orbs shows **Buy** under its price. Pressing it asks
once more (`Pages::confirm`, "Buy for N" with Cancel beside it), because the
Orbs are spent for good; the second press sends `Action::BuyProduct`. The core
refuses it, asking Discord nothing, when the account's Orbs, as Discord last
said, fall short (`ShopPage::refused` says so), when the SKU is already being
bought (`ShopPage::buying`, the button reading "Buying") or was bought in this
run (`ShopPage::bought`, "Bought"), and for a product that Orbs do not buy.
When Discord answers, the Shop is read again, so the Orbs shown are Discord's.
A product priced only in money is never offered for purchase.

## Discover

`yapfast-discord/src/discovery.rs` reads the list (`{guilds, total}`, or a
plain array) and the categories; the core keeps them in `DiscoverPage`
(`yapfast_core::storefront`). Opening the screen asks for the first page
(`Action::OpenDmScreen(DmScreen::Discover)`, `load_discover`), typing in the
search field changes nothing until Enter (`Action::LoadDiscover`, which trims
the words and starts again from the first page), a chip narrows the list to
its category, and **Show more** asks for the page after the servers listed
(`Action::MoreDiscover`; one page at a time, repeats are left out). An
answer for words or a category that were left, or for a page that does not
follow the list, is dropped (`Event::DiscoverLoaded` carries what it was
asked for).

**Join** on a card sends `Action::JoinDiscovered`. The core refuses it,
asking Discord nothing, in the preview, offline, for a server the account is
in (its card says **Open**, which opens that server), for one being joined
(the button reads "Joining") or joined in this run ("Joined"), and for a
server that is not in the list. Discord may ask for a captcha first: it is
served on the loopback address and opened in the browser, as for a quest
claim (`Event::DiscoverCaptcha`), and the join goes on once it is solved. A
refusal shows under the chips with Discord's reason, and the button can be
pressed again.

The rail's compass tile ("Explore Discoverable Servers") opens this screen on
the DM home; it no longer opens discord.com.

## The screens

`sidebar/destinations/` holds them: `mod.rs` (the header, the tabs, the
shared cards and helpers), `quests.rs`, `shop.rs`, `nitro.rs`, `discover.rs`. The tab each
screen shows, the Shop's search and its open category are `App::pages`
(`Pages`), changed by `Message::Pages`. The pictures a screen shows are asked
for through `destinations::pictures`, like the friends list's, and a card
holds a gradient (the quest's own colours, else the theme's greys) until its
picture arrives. Cards stand 1 to 3 across by the window's width, in a
column at most 1090 px wide.

Until a list has come, a screen says it is loading; on an error it says so
with a Retry; unlinked or in the preview it asks to link the account.

## Tests

- `yapfast-core`: `storefront.rs` (progress, search, whether Discover has more),
  `application/tests/storefront.rs` (loading on open, accepting, claiming with a
  captcha, the double press, errors; Discover's paging, stale answers, joining and
  its captcha).
- `yapfast-discord`: the parsers' unit tests and `tests/fake_server/storefront.rs`,
  against `testing/fake_discord.ts` (three quests, one claimed, two shop
  categories, 550 Orbs, three boost slots; `/_control/quest-enrollments` shows
  what was accepted, `/_control/orb-purchases` what was bought with Orbs,
  `/_control/boosts` the boosts placed on servers, and 26 discoverable servers,
  `/_control/discovery-joins` the joins from Discover and
  `/_control/discovery-captcha` a captcha before the next).
- `yapfast-iced`: `tests/dm_destinations.rs` clicks through every screen: the
  tabs, accepting a quest, the claim, the Shop's search, buying with Orbs after
  the second press, Discover's list, Join and Show more, its search on Enter, and the
  sidebar at each width.

## Not done

Paying money (Nitro, gifts, boosts, products priced in money): Yapfast has no
checkout and sends nobody to discord.com for one, so the screens only show
what Discord lists. The price and Orbs shapes, and the Orbs purchase request
(`POST /virtual-currency/skus/{sku}/redeem` with a `checkout_session_id`), have
not been checked against a real account, and which products Discord lets Orbs
buy is read from the prices it lists (a currency named like an Orb). The Shop
shows no product page of its own. Discover's list, search and categories
shapes, and the join request's parameters and captcha, have not been checked
against a real account: the categories come in Discord's language only
(English names). After a join the server's membership screening form is read
(`GET /guilds/{id}/member-verification`); a server that has one opens the
same native dialog as a joined invite (`docs/discord/servers.md`), and one
that has none, or whose form can't be read, shows nothing.
