# The shell and the places: wave 3

The window's frame, the rail, the sidebar's lists, the conversation's
header and the dialogs about servers and DMs. Screens go in
`anomi-screens/src/<area>/`, view-models in `anomi-vm/src/<area>/`. Each
task's integration (projection and intents in `anomi-app`) is the I task it
names. See [the index](../tasks.md).

### N01 · The window and its layers (Opus · L)
- **Owns:** `anomi-screens/src/shell/` (the layout of every area, panels whose edges drag, the layers each area opens, the zoom note), `anomi-screens/src/conversation/` (the column: the header above what the open place shows), `anomi-vm/src/shell.rs`, `anomi-vm/src/conversation.rs`.
- **After:** F06, F07.
- **Ports:** `YapfastWindow` (`ui/yapfast.slint:95`): the left column, the conversation column, the 40 layers over it and how they open; `src/ui/callbacks.rs:1964`, `:1976`, `:1983` (panel widths); `src/zoom.rs`.
- **Shots:** every shot's frame; `small/*.png` for the narrow window (the member list hides).
- **Done when:** at 1400x860 and 1000x640 the frame matches; dragging the sidebar and member list edges keeps their bounds; any area's layer opens over the conversation with its backdrop. Every area's view is placed here, F01's stub until its task passes (`.map(Intent::from)` takes whatever `Intent` it returns), and which layers are open is the shell's own state, so an area's shot opens its layer from its fixture. I01.

### N02 · The rail (Sonnet · M)
- **Owns:** `anomi-screens/src/rail/`, `anomi-vm/src/rail.rs`, `anomi-widgets/src/{rail_item,brand}.rs`.
- **After:** F05, F14.
- **Ports:** `ServerRail`, `RailItem`, `FolderItem`, `CallBubble` (`ui/rail.slint:1011`, `:121`, `:250`, `:105`); `src/ui/refresh.rs:5396` (`rail`), `:5412` (`rail_rows`), `:5569` (`server_row`); `src/ui/callbacks.rs:2716`, `:2725`.
- **Shots:** `server/general_unread_divider.png` (rail), `voice/joined_sidebar_dock.png` (call bubble), `menu/server_icon_context.png`.
- **Done when:** folders open and close, unread DMs show above the servers, the call bubble and mention badges sit as in the shots, a right click opens the server's menu (F06). I02.

### N03 · The DM list (Sonnet · M)
- **Owns:** `anomi-screens/src/sidebar/dms/`, `anomi-vm/src/sidebar/dms.rs`.
- **After:** F07, F14, F15.
- **Ports:** `DmHeading` (`ui/yapfast/dm-heading.slint:4`), `DmScreenLinks` (`ui/rail.slint:1114`), `RoomList` (`ui/rail.slint:1150`, the DM rows); `src/ui/refresh.rs:1059` (`show_sidebar`, the DM half), `src/ui.rs:3455` (`dm_detail`).
- **Shots:** `dm/conversation_profile.png`, `dm/person_chat.png`, `dm/group.png` (left column).
- **Done when:** pinned DMs first, name styles and nameplates, status emoji, the Wave button for an empty DM, call marks; the list is virtual. I02.

### N04 · The channel list (Opus · L)
- **Owns:** `anomi-screens/src/sidebar/channels/`, `anomi-vm/src/sidebar/channels.rs`.
- **After:** F07, F14.
- **Ports:** `ChannelRow`, `CategoryHeading`, `BrowseLink`, `DropLine`, `PersonTouch`, `VoiceCardView` (`ui/rail.slint:397`, `:840`, `:1138`, `:787`, `:799`, `:741`); `src/ui/refresh.rs:1059` (the channel half), `:1575` (`show_moving_person`), `:1605` (`show_arranging`); `src/ui/callbacks.rs:2927`-`:3027` (folding, arranging, moving people).
- **Shots:** `server/general_unread_divider.png`, `voice/joined_sidebar_dock.png`, `thread/book_club_not_joined.png`, `menu/channel_context.png`.
- **Done when:** categories fold, pinned channels head the list, threads hang under their channel, voice channels list their people with LIVE and mute marks, channels and categories drag to reorder and people drag into voice channels (through F07). I02.

### N05 · The server's header (Sonnet · S)
- **Owns:** `anomi-screens/src/sidebar/server_header.rs`, `anomi-vm/src/sidebar/server_header.rs`.
- **After:** F06.
- **Ports:** `ServerHeader` (`ui/rail.slint:946`); the events and boost rows; `src/ui/refresh.rs:1059` (header fields).
- **Shots:** `menu/server_header.png`, `menu/server_notifications.png`.
- **Done when:** the name, banner, verified tick, invite button and the dropdown with its notification submenu match. I02.

### N06 · The account card and its popover (Sonnet · M)
- **Owns:** `anomi-screens/src/account/`, `anomi-vm/src/account.rs`.
- **After:** F06, F14, F15.
- **Ports:** the account card of `ui/yapfast.slint`; `AccountPopover` (`ui/yapfast/account-popover.slint:11`); `src/ui/refresh.rs:1627` (`show_account`).
- **Shots:** `status/menu.png`, `dm/person_chat.png` (card), `voice/mic_device_menu.png` (its arrows).
- **Done when:** the nameplate behind the card, the username on hover, mute and deafen, the popover with the status submenu (P10 draws the menu itself). I03.

### N07 · The conversation header (Sonnet · M)
- **Owns:** `anomi-screens/src/header/{mod,title,tools}.rs`, `anomi-vm/src/header.rs`.
- **After:** F11.
- **Ports:** the header of `ui/yapfast.slint` (title, topic, buttons); `TopicPopover`, `Popover` (`ui/header.slint:511`, `:243`); `src/ui/refresh.rs:2124` (`show_header`), `:2289` (`show_dm_tools`).
- **Shots:** `dm/person_chat.png`, `server/general_unread_divider.png`, `thread/panel.png`, `small/server_general.png` (topic cut short).
- **Done when:** DM, group, channel, thread and voice headers each show their buttons; the topic popover opens. I04.

### N08 · Inbox (Sonnet · S)
- **Owns:** `anomi-screens/src/inbox/`, `anomi-vm/src/inbox.rs`.
- **After:** F06.
- **Ports:** `InboxTab`, `InboxLine`, `InboxPopout` (`ui/yapfast/inbox.slint:12`, `:35`, `:76`); `src/ui/refresh.rs:1746`.
- **Done when:** the popout lists mentions and unreads by tab (no golden shot: compare with the Slint app by hand). I04.

### N09 · Sign-in (Sonnet · M)
- **Owns:** `anomi-screens/src/sign_in/`, `anomi-vm/src/sign_in.rs`.
- **After:** F12.
- **Ports:** `SignInScreen`, `VerifyBanner` (`ui/sign-in.slint:8`, `:119`); `DiscordLinkPanel` (`ui/discord.slint:29`, the QR code and token paste); `src/ui/callbacks.rs:2685`, `:2704`, `:1308`; `src/ui/pictures.rs:13` (`qr_code`).
- **Done when:** a data folder without a session shows sign-in; QR linking and token linking go through; the verify banner shows for an unverified device. I05.

### N10 · Quick switcher (Sonnet · S)
- **Owns:** `anomi-screens/src/switcher.rs`, `anomi-vm/src/switcher.rs`.
- **After:** F06, F14.
- **Ports:** `QuickSwitcher` (`ui/dialogs.slint:364`); `src/ui/callbacks.rs:1280`.
- **Shots:** `switcher/empty.png`, `switcher/query.png`.
- **Done when:** Ctrl+K opens it, arrows move, Enter opens, sections and badges match. I06.

### N11 · New DM and group DM (Sonnet · S)
- **Owns:** `anomi-screens/src/new_dm.rs`, `anomi-vm/src/new_dm.rs`.
- **After:** F06, F12.
- **Ports:** `NewDmDialog` (`ui/dialogs.slint:137`); `src/ui/callbacks.rs:1815`, `:1825`.
- **Shots:** `dialog/new_dm.png`.
- **Done when:** search, pick several, "Write" buttons. I06.

### N12 · Invite, join, event, gift, authorize (Sonnet · L)
- **Owns:** `anomi-screens/src/places/{invite,join,event,gift,oauth}.rs`, `anomi-vm/src/places/{invite,join,event,gift,oauth}.rs`.
- **After:** F06, F12.
- **Ports:** `InviteDialog`, `JoinDialog`, `EventDialog`, `GiftDialog`, `OAuthDialog` (`ui/server.slint:27`, `:139`, `:237`, `:311`, `:395`); `src/ui/refresh.rs:2717`, `:2775`, `:2860`, `:2934`, `:2996`; `src/ui/callbacks.rs:1323`, `:1362`.
- **Shots:** `dialog/invite_to_server.png`, `dialog/add_server.png`.
- **Done when:** each dialog's states (loading, answer, error) match; links copy. I06.

### N13 · Leaving a server or a link (Sonnet · S)
- **Owns:** `anomi-screens/src/places/{leave,leaving_link}.rs`, `anomi-vm/src/places/{leave,leaving_link}.rs`.
- **After:** F06.
- **Ports:** `LeavingDialog` (`ui/dialogs.slint:518`); the leave and move-sync confirmations (`src/ui/refresh.rs:3530`, `:3566`); `src/ui/callbacks.rs:2180`, `:2186`.
- **Done when:** both ask, in red where destructive, and Escape cancels. I06.

### N14 · Browse channels (Sonnet · S)
- **Owns:** `anomi-screens/src/browse.rs`, `anomi-vm/src/browse.rs`.
- **After:** F07, F12.
- **Ports:** `BrowseItem`, `BrowseScreen` (`ui/browse.slint:26`, `:102`); `src/ui/refresh.rs:124` (`browse_rows`); `src/ui/callbacks.rs:3044`, `:3051`.
- **Done when:** a Community server's Browse Channels lists, filters and follows. I02.
