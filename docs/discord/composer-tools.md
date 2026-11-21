# Composer gifts and apps

The composer's **Gift** and **Apps** icon buttons open Yapfast menus. They sit
where Discord's do (the gift before the GIF button, the apps after the emoji)
and draw Yapfast's own icons (Solar's gift and apps), not Discord's artwork.
They use the client's native menu widgets, theme and keyboard navigation.
They contain no copied launcher or checkout. They appear in
writable Discord chats, including DMs, group DMs and threads, once the timeline
is ready. They do not appear in Matrix rooms or unanswered message requests.

| Action | Where it runs | What it does |
| --- | --- | --- |
| Gift: Open chat for gifting | System browser | Opens `https://discord.com/channels/<guild>/<channel>` (`@me` for DMs). Select Gift Nitro in Discord and complete the purchase there. |
| Gift: Nitro gifting guide | System browser | Opens [Discord's Nitro Gifting guide](https://support.discord.com/hc/en-us/articles/360020877112-Nitro-Gifting). |
| Apps: Commands in Yapfast | Native | Opens the existing slash-command picker, or resumes the command already being written. Picking a command prepares it; Enter runs it through the existing command implementation. |
| Apps: Open chat for activities | System browser | Opens the same chat on discord.com. Select Discord's App Launcher there, choose an activity, and authorize or launch it there. |
| Apps: Find apps on discord.com | System browser | Opens [Discord's App Directory](https://discord.com/discovery/applications). Browse and add an app there, completing any installation or OAuth prompts there. |

Every browser row says **Browser** in its subtitle; the command row says
**Native** when available. Commands in Yapfast is unavailable while an ordinary
message draft is nonempty, with a reminder to finish or clear it first. A
choice checks the room and draft again before opening the picker, so a stale
menu cannot replace a newer draft. Gift and Apps menus leave drafts untouched.
Escape or clicking outside closes the menu. Opening one closes the expression
picker.

Browser actions use `browser::open` directly, as **Open Link** in a message's
menu does. They deliberately bypass Yapfast's handling of Discord links, so a
channel link reaches the browser instead of reopening the channel in Yapfast.
The channel captured when the menu opens stays in the handoff URL. These links
carry no Yapfast credentials, OAuth scopes or automatic launch flags. The
browser's Discord session may belong to another account or need sign-in.

These are handoffs to entry pages, not one-click gift or activity launches.
Discord owns the purchase, app installation, authorization and activity
session. Yapfast makes no purchase, grant, redemption or activity interaction
from these menus. Native activity hosting and gift checkout are not
implemented. No new Discord API routes, commands or fake-server endpoints are
needed; native slash commands reuse [Apps' commands](messages.md#apps-commands).

## References and link routing

[Nitro Gifting](https://support.discord.com/hc/en-us/articles/360020877112-Nitro-Gifting)
documents gifting from DMs, group chats and server channels. Nitro Basic
gifts require Discord's Nitro page instead, which the linked guide explains.
[How to Use Apps](https://support-apps.discord.com/hc/en-us/articles/26593412574359-How-to-Use-Apps)
documents commands, activities, installation and authorization in Discord.

`https://discord.com/application-directory` redirects to
`https://discord.com/discovery/applications`. The latter is the menu's target.
It serves Discord's JavaScript application. The channel route follows
Yapfast's existing `network_link` format and Discord's channel-link parser;
only decimal server and channel ids, or `@me` with a decimal channel id, are
accepted. Public routing checks establish destinations, not signed-in access
or availability of a particular gift or activity.

Regression coverage lives in `tests/composer_tools.rs` and
`composer/tools.rs` in `yapfast-iced`: menu actions and destinations, draft
protection, native command selection and submission, unavailable contexts,
Escape and closing the expression picker. The browser helper suppresses
opening a real browser during tests.
