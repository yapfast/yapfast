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
| Apps | Opens the native slash-command picker (`/` and the room's commands), or resumes the command already being written. Picking a command prepares it; Enter runs it through the existing command implementation ([Apps' commands](messages.md#apps-commands)). |

## The gift menu

The menu reads `GET /users/@me/entitlements/gift-codes` each time it opens
(`Action::LoadGifts`, one read at a time, `AppState.gifts`), so a gift bought
a moment ago shows. It opens at once with one disabled line, "Loading your
gifts", and follows the answer without closing (`Menu.gifts`, `gifts_sync`).
Gifts used up by anyone are left out (`gift::Owned::list`). When there is
nothing to give the menu says "You have no gifts to give"; when Discord could
not be asked it says "Could not load your gifts" with the reason, and opening
the menu again asks again. A failed read keeps the list read before.

Yapfast buys no gift. The gifts listed were bought in Discord's apps, since a
purchase takes a payment source Yapfast has no screen for. This is why the
menu has no "Buy a gift" row and no link to a store: a button that only sends
the person to discord.com is not offered.

Choosing a gift checks that the room is still the one the menu opened over
and still writable before it inserts the link, so a stale menu cannot write
into another chat. The link is the one Discord's clients show as a gift card
and Yapfast shows as a gift in the timeline ([gift links](messages.md)).

## The Apps button

Apps opens the picker without a menu in between. While an ordinary message
draft is nonempty it is dim, and its tooltip says "Apps: finish or clear your
message first": the picker would replace the draft, so the draft is never
touched. With a command being written, or the picker already open, pressing
it leaves the text as it is and focuses the composer.

Discord's activities and its App Directory are not in Yapfast: an activity
runs a web page inside the call, and the directory is a store with OAuth
installs. Neither is offered as a stub or a link to discord.com.

## Behaviour shared by both

Escape or clicking outside closes the gift menu. Opening it closes the
expression picker first. Neither button changes a draft by itself.

## Unverified

The shape of `GET /users/@me/entitlements/gift-codes` is read as a list of
gift code objects, as `docs.discord.food` documents, and checked only against
the fake server (`testing/fake_discord.ts`); it has not been run against
Discord. A code is listed as given away when its `uses` reached `max_uses`.

## Tests

`tests/composer_tools.rs` and `composer/tools.rs` in `yapfast-iced`: the
menu's loading, list, empty and error lines, the inserted link, the stale-room
guard, the Apps picker and its draft protection, unavailable contexts, Escape
and closing the expression picker. `application/tests/storefront.rs` covers
`LoadGifts` and `GiftsLoaded`. The fake-server test `lists_the_gifts_the_account_can_still_give`
(`scripts/test-fake-discord`) runs the read against the fake.
