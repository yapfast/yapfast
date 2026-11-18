# Integration and finishing: waves 2 to 5

The I tasks put the core behind the screens: each fills `anomi-app`'s
`project/<area>` (the core's `AppState` as the area's view-model, ported
from `src/ui/refresh.rs`) and `intent/<area>` (the area's intents as
`Action`s, ported from `src/ui/callbacks.rs`), and runs the effects the area
needs. They are Opus tasks in the one integration worktree, one at a time
(its first build costs minutes and several GiB; see the README). Each runs
once its screen tasks pass review, and is accepted on the real backend under
`scripts/ui-session --anomi` against the same shots. The Q tasks finish the
move. See [the index](../tasks.md).

**How a task plugs into the spine (I01's).** *Projection:* write `project/<area>`'s `show(state, [layout,] &mut vm.<area>, &mut shown.<area>)`, which rebuilds only when its key changed, and add its line to `project::refresh` under the `Reach`es that can change it (`project/reach.rs`: `STATE` always, then `TYPING`, `VOICE`, `PICTURES`, `EMOJI`, `PICKER`, `VIDEO`; a backend event shows only the areas it reaches) and its key to `Shown`. *Intents:* `intent::handle` already routes every `anomi_vm::Intent` to `intent/<area>`: a view-model change ends in `app.refresh()` (or `app.show(reach)`), a core change goes through `app.dispatch(action)`; one another area acts on is routed in `intent/mod.rs` ahead of its area's arm, and a layer the area opens closes through its arm in `intent/shell.rs`'s `close`. *Effects:* `effects::run` already hands each `Effect` to its module's `run(app, effect) -> Task`. Settings the core doesn't keep are `app.layout` (`layout/saved.rs`, the Slint `Layout` field for field): read through `Deref`, changed with `app.layout.edit(|saved| ...)`, written a second later; a wait of the app's own is `app::later(delay, message)`; `App::for_tests(folder)` gives a test an app without a window.

### I01 · The window's state and layout.json (Opus · L)
- **Owns:** `anomi-app/src/layout/` (the whole `Layout` of `src/ui.rs:1287`, read and written in the same format, the 1 s delayed write of `src/ui.rs:3296`), `anomi-app/src/project/{shell,reach}.rs`, `anomi-app/src/intent/shell.rs`, `anomi-app/src/zoom/`; and the spine every later I task adds its arms to in turn: `anomi-app/Cargo.toml`, `anomi-app/src/{lib,main,app,message,run,cli,executor,backend,logging,window}.rs`, `anomi-app/src/{effects,project,intent,platform}/mod.rs`, `anomi-app/src/project/rows.rs`, `anomi-app/src/{project,intent}/chat/mod.rs`.
- **After:** N01.
- **Ports:** `Layout` and `Adapter`'s window fields in `src/ui.rs`; what `Adapter::new` hands the core from the layout before anything starts (`src/ui.rs:2535`-`:2578`: reaction and sticker uses, the switches it dispatches as actions, pinned DMs) and `remember_channels` before each `Effect::Start` (`src/ui.rs:2954`), none of which the skeleton does yet; `src/ui/callbacks.rs:1964`-`:2012` (widths, toggles); `src/zoom.rs`.
- **Done when:** a `layout.json` the Slint app wrote opens unchanged and keeps every key it doesn't use; pinned DMs, the last channel of each server and the Slint app's switches (hidden channels, reply ping, read sync...) carry over from it; panel widths, zoom and the member list's switch survive a restart.
- **Keep:** `run.rs` calls `anomi_theme::fonts::install()` before `iced::daemon` (done; it replaces `.fonts(fonts::ALL)`, which draws emoji in the system's font and name faces at one weight). The app is the only caller of `anomi_widgets::picture::set_scale(scale_factor * zoom)`: call it when the window opens, on `window::Event::Rescaled` and when the zoom changes, before the next layout; pictures, drawn names, frames and the paragraph's emoji are copied at that scale (iced's `Rescaled` leaves the zoom out). `anomi-shots` sets it to its `--scale`.

### I02 · Places (Opus · L)
- **Owns:** `anomi-app/src/project/{rail,sidebar}/`, `anomi-app/src/intent/{rail,sidebar}.rs` (the sidebar's and Browse Channels' intents).
- **After:** N02, N03, N04, N05, N14.
- **Ports:** `src/ui/refresh.rs:1059` (`show_sidebar`, whole), `:5396`-`:5569`, `:5533` (`voice_places`), `:5558`, `:124`; `src/ui/callbacks.rs:2716`-`:3051`; the pictures of the rows that show (the core leaves list avatars to the interface: "Virtualized UI rows request their own avatars" in `request_pictures`, `yapfast-core/src/application.rs`), asked with `Action::LoadPictures` from F07's visible keys through F05.
- **Done when:** the rail, DM list and channel list match the Slint app on the fake's data, with folders, voice people, threads, folding, arranging and moving people.
- **Seams:** fill the `ServerHeader` fields from `src/ui/refresh.rs:1059` and handle its `Menu`, `Invite` and `Events` intents (N05). The rail's open folders and each server's collapsed categories are `app.layout.open_folders` and `collapsed` (I01): pass `&Layout` to their `show`s, key on them and change them with `app.layout.edit`. Voice people under channels and the rail's speaker follow `Reach::VOICE`, which already reaches `rail::show` and `sidebar::show`; key them so a speaking change rebuilds no rows. Escape while someone is dragged from a voice channel: give the focus scope `on_let_go` in `App::view` (I01).

### I03 · The account and status (Opus · M)
- **Owns:** `anomi-app/src/project/account.rs`, `anomi-app/src/intent/{account,status}.rs`.
- **After:** N06, P10.
- **Ports:** `src/ui/refresh.rs:1627`; `src/ui/callbacks.rs:1619`, `:3073`, `:3591`.
- **Done when:** mute and deafen outside a call, status and custom status set and shown.

### I04 · Header and inbox (Opus · M)
- **Owns:** `anomi-app/src/project/{header,inbox}.rs`, `anomi-app/src/intent/{header,inbox}.rs`.
- **After:** N07, N08.
- **Ports:** `src/ui/refresh.rs:2124`, `:2289`, `:1746`; `src/ui/callbacks.rs:1288`.
- **Done when:** every header's buttons act.
- **Seams:** The member list's and a DM's profile buttons: `Tools.members` shows `!app.layout.members_hidden` while `state.lists_members()`, and their `Members` and `ProfilePanel` intents are already routed to `intent::shell::toggle_members` and `toggle_profile`, which keep the switch (I01); key the header on `layout.members_hidden`.

### I05 · Signing in and linking Discord (Opus · M)
- **Owns:** `anomi-app/src/project/sign_in.rs`, `anomi-app/src/intent/sign_in.rs` (with the two-factor prompt's, S11), `anomi-app/src/effects/link.rs` (`LinkDiscord`, `LinkDiscordToken`, `StopDiscordLink` as an abortable task over `discord_link::start`'s progress).
- **After:** N09, S11.
- **Ports:** `src/ui.rs:2949` (`execute`, the linking arms); `src/ui/callbacks.rs:2685`, `:2704`, `:1308`.
- **Done when:** linking by QR code and by token against the fake works (`scripts/test-fake-discord links_by_qr_code` covers the core side); two-factor prompts answer.
- **Seams:** on Settings' `discord::Intent::Link`, start linking and show the later phases of N09's `sign_in/discord.rs` panel (QR code, Scanned, "Use a token", the error and "Try again") over Settings (S08). The conversation draws the verify banner while `vm.sign_in.verify.unverified` and the graphics notice from `vm.sign_in.graphics_notice` (`DismissNotice` clears it).

### I06 · Dialogs about places (Opus · M)
- **Owns:** `anomi-app/src/project/places.rs`, `anomi-app/src/intent/places.rs` (the switcher's, the new DM's and every `places` dialog's).
- **After:** N10, N11, N12, N13.
- **Ports:** `src/ui/refresh.rs:2717`-`:3149`, `:3530`, `:3566`, `:3738` (`show_dialogs`); `src/ui/callbacks.rs:1280`, `:1323`, `:1362`, `:1815`, `:1825`.
- **Done when:** each dialog's flow works against the fake.
- **Seams:** Ctrl+K already reaches `keys::switcher` through the focus scope `App::view` places (I01); close the switcher's and the dialogs' layers through their arms in `intent/shell.rs`'s `close`.

### I07 · Messages (Opus · XL)
- **Owns:** `anomi-app/src/project/chat/{timeline,message,attachments,embeds,reactions}.rs`, `anomi-app/src/intent/chat/timeline.rs`.
- **After:** C01, C02, C03, C04, C05.
- **Ports:** `src/ui/messages.rs` (all its row building: grouping, dividers, wording, embeds' layout), `src/ui/refresh.rs:4371`, `:1838`; `Action::VisibleMessages` and the picture requests of `src/ui/callbacks.rs:31`, `:707`.
- **Done when:** #general of the fake with `control showcase`, `embeds`, `poll`, `gallery` shows as the shots; scrolling reports visible rows and previews load and go; memory on an image-heavy channel measured against Slint's.
- **Seams:** Ask who reacted (`hover-reaction`) only once the pointer stays 500 ms on a chip: a chip sends `reactions::Intent::Hover(item, key)` on enter and `Left(item, key)` on leave, and `Left` clears the card only while that chip is still the hovered one (the next chip's `Hover` comes first) (C03). Fill `Chip::burst` from Discord's `burst_colors` and `count_details.burst` once `yapfast-core`'s `Reaction` carries them (C03). Fill `Attachments.lit`, `item`, `preview` and `undelivered` from the message's (`Message.lit`, its key, its delivery): until `lit` is, Save shows on the cell under the pointer only (C04).

### I08 · Components, polls, cards and bot forms (Opus · M)
- **Owns:** `anomi-app/src/project/chat/{components,poll,cards,bot_forms}.rs`, `anomi-app/src/intent/chat/bots.rs`.
- **After:** C06, C07, C08, C20.
- **Ports:** `src/ui/refresh.rs:785`, `:759`, `:948`; `src/ui/polls.rs`; `src/ui/callbacks.rs:812`-`:917`.
- **Done when:** `scripts/test-fake-discord` scenarios for components, polls and forms pass through the iced interface by hand.

### I09 · Media (Opus · L)
- **Owns:** `anomi-app/src/media/` (video frames through `yapfast_core::picture::set_frame_buffers`, the viewer, voice messages, and their intents).
- **After:** C09, C10, C11, F05.
- **Ports:** `src/ui/refresh.rs:5176`, `:5243`, `:5279`; `src/ui/callbacks.rs:4172`, `:1146`, `:1159`; `src/ui/pictures.rs` (frames).
- **Done when:** a video plays in its cell and full screen from the fake's media with CPU and memory measured against Slint's.
- **Seams:** Round a playing video's corners where its frames are made (`pictures/frames.rs`): clear the corner pixels in the frame buffer in place for the cell's radius (8 px in an attachment, 4 in an embed), as the player decodes at the cell's size; then `video::view` may take the radius for its controls' fade (C09). Fill each `VoiceMessage` as `show_voice_message` (`src/ui/refresh.rs:5243`): `enabled` from `attachments.acts()`, `active` while `state.voice_message.item` is this message, `speed` as `"{}x"`, `volume` and `muted` from `voice_tuning`, `progress` as position over duration, `time` counting down what's left once started (empty before), `error` from playback (C11).

### I10 · Menus, editing and forwarding (Opus · L)
- **Owns:** `anomi-app/src/project/menus.rs`, `anomi-app/src/intent/{menus,edit,forward}.rs`.
- **After:** C12, C13, C28, F06.
- **Ports:** `src/ui/menus.rs` (every menu's rows: message, member, channel, server, DM, thread), `src/ui/callbacks.rs:1195`, `:1222`, `:1265`, `:3966`.
- **Done when:** each menu of the golden `menu/` shots lists the same rows and each acts.
- **Seams:** Fill `Menu.rows` for `Layer::MessageMenu` with `anomi_chat::menu::build(&mut menu)`; `Menu::apps` stays empty for a message that takes no app commands (not `d:`, still sending, ephemeral, "Sending command...", `ui/messages.slint:1431`); `Intent::Apps(bool)` sets `apps_open`; set `Menu::flip` without room on the right and place the layer so the menu, the card's right part, sits at the pointer (C12). As an edit starts, `focus::into(anomi_chat::inline_edit::FIELD)` with the caret at the end, only then; set `Timeline::editing` to the message's key and its row's `Message::editing` to `InlineEdit::editing(key, text)`; fill `Field::marks` and `Field::suggestions` while that field was typed in last; after an emoji from the picker `Intent::Emoji` opened goes in, `composer::place_caret` and `focus::to(FIELD)` (C13). The places' menus arrive logged in `intent::rail` (`rail::Intent::Menu(id, spot)`: a server, folder or unread DM, Slint's `open-context-menu(id, false, …)`) and `intent::sidebar` (`dms::Intent::Menu(room, spot)`; `channels::Intent::Menu(room, spot)`; `CategoryMenu(row id, spot)`, its category from `intent::sidebar::category_of`; `PersonMenu { channel, user, at }`, Slint's voice target; `server_header::Intent::Menu(spot)`, Slint's `header:` + the server, and `Events(spot)`, `events:` + the server): open them there, and set the shown header's `ServerHeader::open` (`vm.sidebar`'s `Place::Server`) while its menu is open. In Discord's look Browse Channels opens only from those menus (`Action::BrowseChannels`); the channel list offers no link (I02). The header's bell and a thread's More ask `header::Intent::Notifications(spot)` and `More(spot)`, which `intent::header` leaves logged: open Slint's `notifications:` + the room and `thread-toolbar:` + the room there (I04).

### I11 · The composer (Opus · L)
- **Owns:** `anomi-app/src/project/chat/composer.rs`, `anomi-app/src/intent/chat/composer.rs`.
- **After:** C14, C15, C16, F10.
- **Ports:** `src/ui/refresh.rs:4161`, `:4337`, `:4359`; `src/ui/app_commands.rs`; `src/ui/callbacks.rs:940`, `:2537`-`:2570`, `:2798`-`:2908`; `src/paste.rs`, `src/drop.rs`.
- **Done when:** drafts per room never cross, mentions and emoji complete, slash commands run, pasted and dropped files stage, typing is sent unless the person turned it off.
- **Seams:** open the plus's menu with `vm.menu.narrowest = Some(chat::composer::PLUS_MENU)` (C14); route `composer::Intent::Command(i)` (a press on the written command's pills) as `chat::Intent::Commands(i)` (C16). The field takes the suggestions' keys while `suggesting` or `suggestions.showing()` (C15). The suggestions over an edited message's field come with C13. The composer and the thread's follow `Reach::TYPING` (who's typing reaches nothing else): add their `show` under `TYPING | STATE`. Typing anywhere goes to `focus::COMPOSER` while `App::show_keys` says the composer shows and may be written in; set `app.aside_last` where `keys::typing` says `aside-composer-last` changes (I01).

### I12 · Pickers (Opus · M)
- **Owns:** `anomi-app/src/project/chat/pickers.rs`, `anomi-app/src/intent/chat/pickers.rs`.
- **After:** C17, C18, C19.
- **Ports:** `src/ui/callbacks.rs:426`, `:557`, `:575`, `:1856`-`:1902`; `src/ui/gifs.rs`, `src/ui/stickers.rs`, `src/ui/emoji_grid.rs`, `src/ui/refresh.rs:487`, `:510`, `:668`.
- **Done when:** the three pickers fill from the account's emoji, GIFs and stickers, and let go of far pictures.
- **Seams:** when the GIF page changes, reset `pickers.gif.shown` to (0, 0), else the old page's scroll draws the next page's tiles empty for a frame (C18). The picker's width is `layout.picker_width` (I01): set it with `app.layout.edit` as the edge lets go, which writes once changes stop, as `picker-width-chosen` did. Picker pictures arrive as `Reach::PICKER` (GIFs, stickers) and `Reach::EMOJI` (custom emoji), shown 100 ms after the first of a run (`App::show_events`); the Slint emoji picker waited 32 ms. Status's `PickEmoji(spot)` opens `Layer::EmojiPicker` at the spot with `emoji_picker.purpose = Purpose::Status` (I03): fill the picker as for a reaction, and on `Pick` with that purpose call `intent::status::picked(app, &emoji)` and close the picker; the dialog's draft takes it.

### I13 · Threads, forums, search and pins (Opus · M)
- **Owns:** `anomi-app/src/project/chat/{threads,forum,search,pins}.rs`, `anomi-app/src/intent/chat/places.rs`.
- **After:** C21, C22, C23, C24, C25, C26, C27.
- **Ports:** `src/ui/refresh.rs:1789`, `:1921`, `:1995`, `:2065`, `:3979`, `:5779`, `:5812`; `src/ui/callbacks.rs:1642`-`:1795`.
- **Done when:** each flow of the golden `thread/`, `forum/` and `header/` shots works.
- **Seams:** the thread panel's timeline sets `Timeline::aside` (a short history under its header, rows ending 16 px in) and gives its messages' bars two frequent reactions (C24). A new forum post is `vm.chat.forum.composing` alone (C27). After `pin-message`, refresh `vm.chat.pins`: the popover has no unpin of its own, as Slint's (C22). Fill `vm.conversation.forum_post` (I01 projects the column's `body` and `beside`); the forum post bar's `shell::Intent::ReactToPost`, `FollowPost` and `CopyPostLink` are routed to `intent::chat::places::post_bar`. The header opens `Layer::Pins` and `Layer::Threads` at their buttons after `ShowPins`/`ShowThreads`, and `project::header` closes them as the core lets go (`state.pins`, `state.threads`); Escape or a press outside dispatches the close through `intent::header::close` (I04): fill only their contents. Pins opened from elsewhere (a pin's notice) open the layer without a spot, or keep the pin button's. Fill `vm.chat.search.placeholder` (`tr!("Search {}", …)` of `show_dm_tools`, `src/ui/refresh.rs:2338`) and set the field's `compact` from the conversation's width, as `anomi_screens::header::with_search` now takes `docking.room < header::NARROW` (I04).

### I14 · Tray, window and single instance (Opus · L)
- **Owns:** `anomi-app/src/platform/{tray,window,instance,placement}.rs` (`platform::window` takes F18's title bar intents).
- **After:** I01, F18.
- **Ports:** F18's title bar intents (drag, resize, minimize, maximize, close) as `iced::window` tasks; `src/tray.rs`, `src/ui/shell.rs` (closing to the tray gives memory back), `src/instance.rs`, `src/ui/placement.rs`, `src/autostart.rs`, `src/desktop_entry.rs`, `src/ui/move_loop.rs` (Windows).
- **Done when:** close to tray and open again (measure the memory returned with `testing/measure_tray.py --bin`), `--background`, a second start brings the window forward, the window opens where it was.
- **Seams:** `layout.json`'s `window` (the Slint `placement::Placement`), `close_to_tray`, `system_title_bar` and `multiple_windows` are in `layout::Saved`, `window` as the JSON it is until you type it (I01). Quit through `App::quit`, which writes what waits of the layout; closing to the tray should flush it too (`app.layout.flush()`). The bar's `chrome::Intent::ToggleInbox` is routed to `intent::inbox::toggle` ahead of the chrome's arm, which sets `vm.chrome.inbox_open` with `Layer::Inbox` (I04): keep that field when projecting the chrome.

### I15 · Notifications, sounds, links and the clipboard (Opus · M)
- **Owns:** `anomi-app/src/effects/{notify,sound,page,clipboard}.rs`, `anomi-app/src/platform/{drop,paste,links}.rs`.
- **After:** I01.
- **Ports:** `src/ui.rs:2949` (`Notify`, `Sound`, `OpenPage`, `CopyPicture`), `src/notify.rs`, `src/protocol.rs`, `src/copy.rs`, `src/drop.rs`, `src/paste.rs`, `src/ui/browser.rs`.
- **Done when:** a message behind another window notifies and flashes the taskbar, sounds follow the settings, links open, a picture copies, files drop on Wayland and X11.
- **Seams:** The sound and notification settings are in `layout::Saved`, `notification_sound` already settled at read (I01). `shell::Intent::DismissDrop` clears `vm.shell.drop`; the masked link's question turns the window's keys off through `keys::Covers::leaving_link` in `App::show_keys` (I01). The account popover's badges ask `account::Intent::Badge(badges::Intent::OpenLink(link))`, which `intent::account` leaves logged: open the link and close `Layer::Account` (I03). Muting or deafening outside a call runs `Effect::Sound(Mute | Unmute | Deafen | Undeafen)` through `effects::run`, as the Slint app's `play` did (I03). The topic in full asks `header::Intent::Link` for each link: channels, people and roles act in `intent::header::link`; web addresses, masked links and emoji cards arrive there logged: open them as a message's links do (I04).

### I16 · People (Opus · L)
- **Owns:** `anomi-app/src/project/people/`, `anomi-app/src/intent/people.rs`.
- **After:** P01-P09, P11-P13.
- **Ports:** `src/ui/profile.rs`, `src/ui/active_now.rs`, `src/ui/refresh.rs:2429`, `:2507`, `:3587`, `:5129`, `:6094`; `src/ui/callbacks.rs:986`-`:1062`, `:1237`, `:1249`, `:1383`, `:1398`, `:1804`, `:3037`.
- **Done when:** the golden `friends/`, `profile/` and member list shots match on the real backend; Active Now's cards keep their state while they stay.
- **Seams:** Leave `card.frame` empty when the window has no room above the full profile (Slint's `show-frame`, P09). Someone clicked under a voice channel asks `channels::Intent::Profile { channel, user, at }` (Slint's `open-voice-profile`: `Action::ShowProfile { user, large: false }`, the card beside their row), which `intent::sidebar` leaves logged: open the card there (I02). `project::account::profile` makes a profile card's banner colour (`accent`, with the avatar's dominant colour when the profile has none), `badges` and the pictures to want (`addresses`) from a core `Profile`, for the account's popover: reuse them for the cards (I03). The header's person button dispatches `ShowProfile` and opens `Layer::Profile` at its spot, a topic's mention `ShowProfile` alone (I04): fill `vm.people.profile.card` and close the layer as the core lets the profile go. The Friends tab is the window's own: `vm.people.friends.all`, which the header's `FriendsAll` sets (I04); keep it when projecting the Friends screen. The friends picker (`Layer::AddPeople`) opens and closes from the header as the pins do: fill `vm.people.add_people`.

### I17 · The profile editor (Opus · M)
- **Owns:** `anomi-app/src/project/editor.rs`, `anomi-app/src/intent/editor.rs`.
- **After:** P14, P15, P16.
- **Ports:** `src/ui/refresh.rs:3246`; `src/ui/name_style.rs`; `src/ui/callbacks.rs:1426`-`:1554`.
- **Done when:** editing and saving the main and a server profile as `scripts/test-fake-discord edits_the_main_profile_as_discords_client_does` expects.
- **Seams:** Escape or a click elsewhere ends editing a text with `Intent::Edit(None)` (the bio's Enter makes a new line); `field::focus` `anomi_people::editor::{NAME, PRONOUNS, BIO, COLOUR}` as `Intent::Edit` or `ColourBox` opens one; answer `TagMenu`, `PictureMenu` and `MoreMenu(Spot)` with `vm.menu` rows sending `ChooseTag(id or "")`, `Pick`, `Reset`, `CopyId`; "Member Since" in the app's language (P14). `Intent::Handle(at, y)`: stop editing, close the picker if `at` is open, else set `handle` and `handle_y` and start `preview.colour` with `Colour::start` on that colour's text; `CloseHandle` clears `handle`; a preview `Intent::Colour(change)` goes to `Colour::apply`, and a colour it returns is written to the open handle (`write(4 or 5, "#" + hex)`); fill `EditorPreview` from `ProfileData` (theme, activities, now, widgets, board and wishlist notes, wishlist), set `page` on `Choose` and ask Discord for the wishlist on `Choose(Page::Wishlist)` (P15).

### I18 · Voice (Opus · XL)
- **Owns:** `anomi-app/src/project/voice/`, `anomi-app/src/intent/voice.rs`, `anomi-app/src/effects/devices.rs` (`ListDevices`, `PreviewCamera`), `anomi-app/src/platform/keybinds.rs` (push to talk).
- **After:** V01-V12.
- **Ports:** `src/ui/refresh.rs:4541`, `:4592`, `:5013`, `:5052`; `src/ui/tiles.rs`, `src/ui/alone.rs`, `src/ui/call_timer.rs`, `src/ui/call_link.rs`, `src/keybinds.rs`, `src/ui/keybinds.rs`; `src/ui/callbacks.rs:3061`-`:3881`.
- **Done when:** joining, speaking rings, watching a stream, the camera, sharing a screen and the soundboard work against the fake; a call measured against Slint's memory (44 MB in `docs/measurements/README.md`).
- **Seams:** a participant's card (`Layer::ParticipantMenu`) and its rows (`vm.menu`) are one popup: the rows draw over the card, whose layer then has no backdrop, so a press outside closes the menu; close the card with it. Open the rows with `narrowest: Some(participant_menu::WIDTH)` and `headed: true`, and set `ParticipantMenu.width` to `anomi_widgets::menu::width(&rows, WIDTH)` (V11). The ten-person call to measure: `fixtures::voice::stage::ten()` / `crowd(10)` (V04). Fill `vm.conversation.call` and `description`, `vm.shell.voice_dock` and `pip` (I01 leaves them); the call's views follow `Reach::VOICE`, the video and voice message `Reach::VIDEO`. `push_to_talk_key` and `keybinds` are in `layout::Saved` as the JSON the Slint app wrote, to type as `keybinds::Shortcut` and `Binding`; give the focus scope `talk_keys` in `App::view`. The call's participants under its channel in the channel list are I02's (`project::sidebar::people::follow`, under `Reach::VOICE`); the call dock in the sidebar is yours. The account card's carets ask `account::Intent::InputMenu(spot)` and `OutputMenu(spot)`, which `intent::account` leaves logged: open the device menus there and set `vm.account.input_open` / `output_open` while they show (I03). `project::account::voice` already sets the card's `muted`, `deafened`, `in_call`, `ready`, `speaking` and `docked` under `Reach::VOICE`.

### I19 · The person's settings (Opus · L)
- **Owns:** `anomi-app/src/project/settings.rs`, `anomi-app/src/intent/settings.rs`, `anomi-app/src/language.rs`.
- **After:** S01-S10.
- **Ports:** `src/ui/appearance.rs`, `src/ui/updates.rs`, `src/ui/games.rs`, `src/custom_font.rs`, `src/woff.rs`; `src/ui/callbacks.rs:2193`-`:2510`.
- **Done when:** every switch of every page changes and persists what the Slint app's does.
- **Seams:** Games: `Save` saves from `vm.settings.games.{editing, executable_draft, text_draft}`, sets `invalid` when refused and else clears the drafts and `editing`; `Edit(i)` fills the drafts from the game and clears `invalid`; `Remove(i)` and `Cancel` clear `editing` and the drafts; `MoreBlocked` grows `blocked_shown` by `BLOCKED_STEP` (S08). About: clear `About.copied` 2 s after `CopyVersion`; `fresh_check` and `fresh_nightlies` come from `src/ui/new_tags.rs` ("check-updates", "update-nightlies"); `About.original` is `tr!("Original: {}", "Yapfast")` (S10). Appearance draws the theme editor while `appearance.editing` (S06). The entrance sound's chooser is inline, names only; Slint opened V12's soundboard in a popup under "Choose", which needs a layer of its own and the soundboard shared (S03). Every setting is a field of `layout::Saved` (I01): change it with `app.layout.edit` and, for the switches `layout::carry::give` hands the core at the start, dispatch the same action. Still to port at read, as the Slint `Layout::read` does: `first_version` (`new_tags::first_version`), `theme::tidy` of `custom_themes`; type `custom_themes` and `ui_fonts`, held as JSON. The animations switch sets `app.motion.never` (I01 reads it at the start). The account card's cog and the popover's User Settings open `Layer::Settings` on `Page::Account` (`intent::account::settings`, I03), which then shows the frame unfilled: fill it as it opens, and close the layer on the frame's `Close`.

### I20 · Server and channel settings (Opus · L)
- **Owns:** `anomi-app/src/project/server_settings/`, `anomi-app/src/intent/server_settings.rs`.
- **After:** X01-X15.
- **Ports:** `src/ui/server_settings.rs`, `src/ui/channel_settings.rs`, `src/ui/refresh.rs:2679`, `:3149`.
- **Done when:** `scripts/test-fake-discord changes_server_settings_as_discords_client_does`, `changes_roles_…` and `changes_channel_permissions_…` flows work by hand through the iced interface.
- **Seams:** a role is open while `vm.settings.server.title.is_some()`; fill `server.title`, `detail`, `fill`, `unsaved` and `loaded` (X01). Fill the channel's Permissions page in `vm.settings.channel.permissions` (X13). The settings' dialogs are layers of their own, open while their page holds one: `Layer::SecurityActions` (`safety.actions`, which Invites' Security Actions fills), `Ban` (`bans.open`), `RoleConfirm` and `RoleAdding` (`role_editor.confirm`, `.adding`), `EmojiUpload` (`emoji.upload`), `StickerForm` (`stickers.form`), `Ownership` (`ownership.kind` not `None`), `PermissionsConfirm` and `PermissionsAdd` (`channel.permissions.confirm`, `.adding`). Open the layer when the page's model gets one, close it when a dialog's own button lets it go, and clear the model on its `Gone`, so the card plays its way out. `shell::Intent::Close` of each asks its page's: `CloseActions`, `bans::Intent::Close`, `Cancel`, `AddingClose`, emoji's `Cancel`, stickers' `Cancel` unless `form.saving`, ownership's `Cancel` unless `sending` (the one close can't tell Escape, which Slint let through, from a press beside), permissions' `Cancel` and `CloseAdd`. Focus `emoji::NAME`, `ownership::NAME` or `CODE`, and permissions' `SEARCH` as their field appears (X05, X06, X09-X12, X14). Invites: subscribe to `time::every(1s)` only while `motion.front && invites.wants_ticks()` and give each `Tick(now)` to `invites.tick(now)`; `set_rows(rows, Instant::now())` on each page read; `Hover(code, inside)` to `invites.hover` (X04). `Hover(id, inside)` of the bans to `Bans::hover`, of the emoji clears `hovered` only while it is still `id`; emoji's `Rename(id)` acts only while that row is edited with a changed name, and clears it (X06, X10). `server.fill` is true on Emoji and on the role editor's Permissions and Members tabs (X09, X10).

### I21 · Clocks and presence (Opus · S)
- **Owns:** `anomi-app/src/clocks.rs`.
- **After:** I01.
- **Ports:** `src/ui/profile.rs:252` (`clock_interval`), `src/ui/away.rs`, `src/ui/new_tags.rs`, `activity_clock` (`src/ui/callbacks.rs:1297`).
- **Done when:** every clock is a `time::every` subscribed only while the window is in front and something shows the time; idle after 15 s sets `motion.idle`; a test checks `subscription().units()` in front and behind.
- **Seams:** Busy voice channels' rows say how long their call has gone on: while `motion.front && project::sidebar::timed(&vm.sidebar)`, tick every 15 s with `project::sidebar::tick(&core.state, &mut vm.sidebar, unix_seconds)` (I02).

## Finishing

### Q01 · The UI tests that move (Opus · M)
- **Owns:** `anomi-app/src/project/tests/` (one module per area, under `project` so it reaches the areas' `pub(super)` builders).
- **After:** the I task of each area.
- **Ports:** the 55 tests of `src/ui/tests.rs` that test row builders and wording (not the window).
- **Done when:** each passes against the ported builder.

### Q02 · Snapshot baselines (Sonnet · S)
- **Owns:** `anomi-{screens,chat,people,voice,settings}/tests/areas/snapshots.rs` (a module of each crate's test binary, which F01 lists) and `anomi-{screens,chat,people,voice,settings}/tests/snapshots/` (the baselines).
- **After:** an area's acceptance.
- **Done when:** each accepted shot is an `iced_test` snapshot compared on every `scripts/check-iced --review`.

### Q03 · Parity measurements (Opus · M)
- **Owns:** `docs/measurements/` records for the iced interface.
- **After:** I07, I14, I18.
- **Done when:** idle, image-heavy channel, call and scrolling (`testing/measure_scroll.py`) measured for both interfaces on the same data, as `docs/measurements/README.md` asks; and a busy server (typing, presence, messages every second) behind another window, where every batch of events costs the iced interface a view and a layout: its CPU and wakeups against Slint's.

### Q04 · The test session's commands (Opus · S)
- **Owns:** `anomi-app/src/control.rs`.
- **After:** I11, I14, I19.
- **Ports:** `test_control` in `src/ui.rs:1011` (`pick`, `stage`, `stage-aside`, `window open|close|hide|show|destroy|create|trim|stats`).
- **Done when:** every `scripts/ui-session do` command works with `--anomi`.

### Q05 · Switching over (Opus · L)
- **Owns:** `Cargo.toml`, `packaging/`, `scripts/`, the docs of the interface (`docs/architecture.md`, `docs/building.md`, `docs/translations.md`, `docs/themes.md`, `docs/iced/`); it runs after every other task.
- **After:** everything.
- **Done when:** `anomi` is the shipped binary, the Slint interface, `vendor/i-slint-*`, Skia and `crates/yapfast-ui` are gone, and the docs describe the iced interface alone.
