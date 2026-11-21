# Privacy and accessibility settings

The Content & Social, Data & Privacy, Accessibility, Streamer Mode and
Activity Privacy pages live in `crates/yapfast-iced/src/settings/privacy.rs`.
They use native Yapfast form cards and the selected theme, without additional
images or dependencies. They keep no separate preference state. Each control
uses the existing handler, persistence key and core action of its original
settings page, so both pages show the same value immediately.

A page holds only controls that work. Yapfast does not copy a Discord
privacy permission it cannot change (who may send a friend request, the
sensitive-media and DM spam filters, Family Center, activity audiences), so
it has no page, button or card for them and sends nobody to discord.com.

## Controls

| Page | Controls in Yapfast |
| --- | --- |
| Content & Social | Link previews, whether replies mention their author, hiding messages from blocked people |
| Data & Privacy | Removing known tracking parameters from sent links, sending Discord read acknowledgements |
| Accessibility | Message text size, folding long messages, GIF/emoji/sticker playback, profile effects, animation of decorations and nameplates |
| Streamer Mode | Desktop notifications, message text in notifications, taskbar flashing and Yapfast interface sound volume |
| Activity Privacy | Typing indicators, game sharing and reporting idle presence |

Blocked-message hiding, Discord read acknowledgements, game sharing and idle
presence appear only with a linked Discord user outside the preview. The
other controls remain available without an account.

Game sharing dispatches the existing `Action::ShowGame` and
`DiscordCommand::ShowGame`, as Game Activity does. This is an account setting
kept by Discord; its existing failure event restores the previous value and
reports the error. The other switches are local preferences and are kept in
`layout.json`, including after Settings closes or Yapfast restarts.

Yapfast's Streamer Mode page has no automatic mode or master privacy switch:
names, messages and invite links remain visible. Hiding notification message
text still leaves sender and conversation names visible. Setting sound volume
to 0% silences interface sounds; call audio, streams and the soundboard have
separate controls. Hiding link previews does not filter sensitive media, and
disabling read sync does not erase acknowledgements already sent.

## Regression coverage

`crates/yapfast-iced/src/tests/settings_privacy.rs` drives the five pages
with headless fixtures. It checks navigation search, that no page sends
anyone to a browser, account gating, preference propagation and restart
persistence, and the existing game-sharing command and failure path. It never
signs in, starts a backend or opens a browser.
