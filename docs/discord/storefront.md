# Nitro, Shop and Quests

The three screens under Friends and Requests on the DM home. Each opens in
the conversation's place, as Discord's do, from a row of the DM list
(`Action::OpenDmScreen`), with its own header: the screen's icon, its tabs
as words with a line under the open one, and what Discord puts at the end.
They are in the back and forward history like Friends and Requests, and need
a linked account (they are absent in the preview and a server).

What needs Discord's own checkout is not done here. Buying, subscribing and
claiming a reward (which may ask for a captcha, as [linking](README.md#account-linking)
does) open discord.com from a button; everything else is native.

| Screen | Tabs | Native | Handed to discord.com |
| --- | --- | --- | --- |
| Quests | All Quests, Claimed Quests | The list with the first quest as the wide banner, each card's picture, game, sponsor, end date, reward, task, progress and its game's link; accepting a quest | Claiming a reward |
| Shop | Featured, Browse, Orbs Shop | Categories with banners, their products with pictures and prices, the account's Orbs, a search, one category alone | Buying: a product's card opens its page |
| Nitro | Home, Perks, Plans, Compare | The offer, what Nitro gives, whether the account has it, each server's boost level and progress, Free against Nitro | Upgrading, gifting, the plans |

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
| `GET /collectibles-categories` | The Shop's categories and products |
| `GET /users/@me/virtual-currency/balance` | The Orbs in the Shop's header |

Both parsers read defensively: a field Discord has changed or left out loses
that field (a price, a picture), not the list. A product's price is shown as
Discord words it; Yapfast keeps none of its own, and the Nitro screen shows
no price either, as it varies by country.

A quest being accepted is in `QuestsPage::accepting` until Discord answers,
so a second press does nothing and its button reads "Accepting". On success
the quest turns `Accepted` with Discord's own progress; on failure the page
keeps its list and shows the reason with a Retry.

## The screens

`sidebar/destinations/` holds them: `mod.rs` (the header, the tabs, the
shared cards and helpers), `quests.rs`, `shop.rs`, `nitro.rs`. The tab each
screen shows, the Shop's search and its open category are `App::pages`
(`Pages`), changed by `Message::Pages`. The pictures a screen shows are asked
for through `destinations::pictures`, like the friends list's, and a card
holds a gradient (the quest's own colours, else the theme's greys) until its
picture arrives. Cards stand 1 to 3 across by the window's width, in a
column at most 1090 px wide.

Until a list has come, a screen says it is loading; on an error it says so
with a Retry; unlinked or in the preview it asks to link the account.

## Tests

- `yapfast-core`: `storefront.rs` (progress, search), `application/tests/storefront.rs`
  (loading on open, accepting, the double press, errors).
- `yapfast-discord`: the parsers' unit tests and `tests/fake_server/storefront.rs`,
  against `testing/fake_discord.ts` (three quests, one claimed, two shop
  categories, 550 Orbs; `/_control/quest-enrollments` shows what was accepted).
- `yapfast-iced`: `tests/dm_destinations.rs` clicks through every screen: the
  tabs, accepting a quest, the claim and a product's hand-off to the right
  URL, the Shop's search, and the sidebar at each width.

## Not done

Paying, subscribing and gifting (Discord's checkout), claiming a reward
(captcha), and the price and Orbs shapes Discord has not been checked
against a real account for. The Shop shows no product page of its own.
