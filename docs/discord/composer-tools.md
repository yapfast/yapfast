# Composer gifts and apps

The composer's **Gift** and **Apps** icon buttons sit where Discord's do (the
gift before the GIF button, the apps after the emoji) and draw Yapfast's own
icons (Solar's gift and apps), not Discord's artwork. They appear in writable
Discord chats, including DMs, group DMs and threads, once the timeline is
ready. They do not appear in Matrix rooms or unanswered message requests.
Both run inside Yapfast: nothing here opens a browser.

| Button | What it does |
| --- | --- |
| Gift | Opens a menu of the gifts the account bought and can still give, each by its plan's name (such as "Nitro Monthly"). Choosing one puts its link, `https://discord.gift/<code>`, in the message at the caret, with a space before it when the text before it does not end in one. The person sends the message; nothing is sent by the menu. |
| Apps | Opens a search and list of apps with usable commands in the current chat. Selecting an app lists its commands. Picking a command prepares it; Enter runs it through the existing command implementation ([Apps' commands](messages.md#apps-commands)). |

## The gift menu

The menu reads `GET /users/@me/entitlements/gifts` each time it opens
(`Action::LoadGifts`, one read at a time, `AppState.gifts`). This returns
giftable entitlements. For each distinct SKU and subscription plan it then
reads `GET /users/@me/entitlements/gift-codes?sku_id=…`, including the plan ID
when available. Only existing, unexpired codes with remaining uses appear.
The menu shows loading, empty or error text as appropriate (`Menu.gifts`,
`gifts_sync`). HTTP failures include the status. Reopening retries the read.

Choosing a gift checks that the room is still the one the menu opened over
and still writable before it inserts the link, so a stale menu cannot write
into another chat. The link is the one Discord's clients show as a gift card
and Yapfast shows as a gift in the timeline ([gift links](messages.md)).

## The Apps button

Apps opens the browser above the composer and focuses search. It uses
the same room index and permission checks as the typed `/` picker.
Search matches app names, command names and descriptions. Selecting an app
narrows the list to its commands. Selecting a command prepares it for Enter
when the composer is empty or contains a slash query. An ordinary draft or
a command already being written can stay in place while browsing.

## Behaviour shared by both

Escape or clicking outside dismisses either popup. Opening one closes the
expression picker first. Drafts stay in place while browsing.

## Unverified

The entitlement and code response shapes follow the
[entitlements reference](https://docs.discord.food/resources/entitlement)
and the per-SKU query follows
[discord.py-self's implementation](https://github.com/dolfies/discord.py-self/blob/master/discord/http.py).
Live account behavior, including whether all SKUs with existing codes still
appear in the giftable inventory, is unverified.

## Tests

`tests/composer_tools.rs` and `composer/tools.rs` in `yapfast-iced`: the
menu's loading, list, empty and error lines, the inserted link, the stale-room
guard, the Apps browser and its draft protection, unavailable contexts, Escape
and closing the expression picker. `application/tests/storefront.rs` covers
`LoadGifts` and `GiftsLoaded`. The fake-server test `lists_the_gifts_the_account_can_still_give`
(`scripts/test-fake-discord`) runs the read against the fake.
