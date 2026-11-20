# Privacy and accessibility settings

The Content & Social, Data & Privacy, Family Center, Friend Requests,
Accessibility, Streamer Mode and Activity Privacy pages live in
`crates/yapfast-iced/src/settings/privacy.rs`. They use native Yapfast form
cards and the selected theme, without additional images or dependencies.
They keep no separate preference state. Each native control uses the
existing handler, persistence key and core action of its original settings
page, so both pages show the same value immediately.

## Native controls

| Page | Controls in Yapfast |
| --- | --- |
| Content & Social | Link previews, whether replies mention their author, hiding messages from blocked people |
| Data & Privacy | Removing known tracking parameters from sent links, sending Discord read acknowledgements |
| Family Center | Official Discord browser handoff only |
| Friend Requests | Open the existing Requests screen to review incoming and sent requests |
| Accessibility | Message text size, folding long messages, GIF/emoji/sticker playback, profile effects, animation of decorations and nameplates |
| Streamer Mode | Desktop notifications, message text in notifications, taskbar flashing and Yapfast interface sound volume |
| Activity Privacy | Typing indicators, game sharing and reporting idle presence |

Blocked-message hiding, Discord read acknowledgements, the Requests
shortcut, game sharing and idle presence appear only with a linked Discord
user outside the preview. Other local controls and the official handoffs
remain available without an account. The Requests shortcut closes Settings
and navigates to DMs before opening Requests, including when invoked from a
server conversation.

Game sharing dispatches the existing `Action::ShowGame` and
`DiscordCommand::ShowGame`, as Game Activity does. This is an account setting
kept by Discord; its existing failure event restores the previous value and
reports the error. The other switches are local preferences and are kept in
`layout.json`, including after Settings closes or Yapfast restarts.

## Official Discord handoffs

Each page separates its native controls from an **In Discord** card. **Open
Discord in browser** opens `https://discord.com/channels/@me` in the system
browser. The card gives the next User Settings path; it does not claim to
open a particular settings page directly. Authentication, account choice
and confirmation happen in Discord. **Read Discord's guide** opens that
area's public help article. Data & Privacy instead offers **Read Discord's
privacy policy**.

The following controls are completed in the official Discord app:

- Content & Social: sensitive media, DM spam filters and messaging permissions.
- Data & Privacy: data use, personalization, profile privacy and data requests.
- Family Center: setup, connections and activity summaries.
- Friend Requests: who is allowed to send a request, under Content & Social.
- Accessibility: Discord's own text-to-speech, contrast and other app settings.
- Streamer Mode: Discord's identity and invite masking and streaming-software detection.
- Activity Privacy: audiences, per-server sharing, connected services and activity history.

Changing Discord's own accessibility or streamer settings does not
configure Yapfast. Yapfast's Streamer Mode page has no automatic mode or
master privacy switch: names, messages and invite links remain visible.
Hiding notification message text still leaves sender and conversation
names visible. Setting sound volume to 0% silences interface sounds; call
audio, streams and the soundboard have separate controls. Hiding link
previews does not filter sensitive media, and disabling read sync does not
erase acknowledgements already sent.

Public references:
[blocking and privacy](https://support.discord.com/hc/en-us/articles/217916488-Blocking-Privacy-Settings),
[privacy policy](https://discord.com/privacy/policy/),
[Family Center](https://support.discord.com/hc/en-us/articles/14155043715735-Family-Center-for-Parents-and-Guardians),
[accessibility](https://support.discord.com/hc/en-us/articles/1500010454681-Accessibility-Settings-Tab),
[streamer mode](https://support.discord.com/hc/en-us/articles/218485407-Streamer-Mode-101),
[activity sharing](https://support.discord.com/hc/en-us/articles/7931156448919-Activity-Sharing-on-Discord-FAQ).

## Regression coverage

`crates/yapfast-iced/src/tests/settings_privacy.rs` drives all seven pages
with headless fixtures. It checks navigation search, the button messages
and official destinations, account gating, preference propagation and
restart persistence, the existing game-sharing command and failure path,
and opening Requests from a server. It never signs in, starts a backend or
opens a browser. Integration and build gates are run by the lead.
