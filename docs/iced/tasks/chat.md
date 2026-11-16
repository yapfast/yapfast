# The conversation: wave 3

The timeline, the message and all it holds, the composer, the pickers,
threads, forums, search and pins. Screens in `anomi-chat/src/<area>/`,
view-models in `anomi-vm/src/chat/<area>.rs`. See [the index](../tasks.md).

### C01 · The timeline (Opus · XL)
- **Owns:** `anomi-chat/src/timeline/`, `anomi-vm/src/chat/timeline.rs`.
- **After:** F07, F08, F13.
- **Ports:** `NoConversation`, `HistoryEdge`, `UnreadBar`, `PresentBar`, `ConversationStart` (`ui/messages.slint:2373`, `:2390`, `:2467`, `:2497`, `:2530`); the timeline of `ui/yapfast.slint` (following, jumping, reporting what shows); `src/ui/messages.rs:18` (`continues`), `:36` (`day`), `:95` (`show_timeline`); `src/ui/refresh.rs:4371` (`show_messages`); `src/ui/callbacks.rs:707`, `:960`, `:971`, `:1183`.
- **Shots:** `server/general_unread_divider.png`, `chan/history_plain_a.png`, `dm/person_chat.png` (start of a DM).
- **Done when:** grouping by author within 7 minutes, day and "New" dividers, loading older and newer pages at the edges, the jump bar and "back to present", visible rows reported to the app with six of margin at most every 50 ms; 1 000 messages scroll with memory flat. Rows are C02's `message::view` (F01's stub until C02 passes; C01 and C02 run side by side), so the shots compare once both passed. I07.

### C02 · The message (Opus · L)
- **Owns:** `anomi-chat/src/message/{mod,author,reply,notice,hover_bar}.rs`, `anomi-vm/src/chat/message.rs`.
- **After:** F08, F14, F15.
- **Ports:** `MessageBody`, `MessageContent`, `BotTag`, `ThreadChip` (`ui/messages.slint:2948`, `:3015`, `:2031`, `:1989`); `MessageRow` (about 85 fields) in `ui/messages.slint`; `src/ui/messages.rs:41` (`Marks`), `:54` (`Decoration`).
- **Shots:** `chan/history_plain_a.png`, `chan/gallery_hoverbar.png` (hover bar), `chan/spoiler_reply_blocked_linkembed.png` (reply line, blocked message), `dmcall/connected_bob_dm.png` (call notice).
- **Done when:** avatar or compact gutter, author in role colour with name style and tag, time on hover, replies with their spine, forwards, system notices, blocked messages, the 12-line fold, mentions highlighted, the hover bar with its quick reactions. I07.

### C03 · Reactions (Sonnet · M)
- **Owns:** `anomi-chat/src/reactions/`, `anomi-vm/src/chat/reactions.rs`.
- **After:** C02, F06.
- **Ports:** `ReactionToggle` (`ui/messages.slint:272`), the `ReactionHover` global, `ReactionsDialog` (`ui/dialogs.slint:797`); `src/ui/refresh.rs:1838` (`show_reactions`), `:2632`; `src/ui/callbacks.rs:1925`, `:1937`, `:1952`.
- **Shots:** `server/general_unread_divider.png` (chips), `chan/showcase_emoji_stickers_gif.png`.
- **Done when:** chips, custom emoji, super reactions marked, who reacted on hover, the Reactions window. I07.

### C04 · Attachments (Sonnet · M)
- **Owns:** `anomi-chat/src/attachments/`, `anomi-vm/src/chat/attachments.rs`.
- **After:** C02, F05, F13.
- **Ports:** `MessageGallery`, `FileCard`, `MessageStickers` (`ui/messages.slint:1615`, `:1750`, `:1683`); `src/ui/callbacks.rs:2056`, `:2572`, `:2635`.
- **Shots:** `chan/gallery_hoverbar.png`, `chan/showcase_emoji_stickers_gif.png`, `chan/spoiler_reply_blocked_linkembed.png` (spoiler picture).
- **Done when:** the gallery's geometry (tall left, two right), downloads, file cards, spoilers, stickers and animated stickers. I07.

### C05 · Embeds (Sonnet · L)
- **Owns:** `anomi-chat/src/embeds/`, `anomi-vm/src/chat/embeds.rs`.
- **After:** C02, F05.
- **Ports:** `EmbedView` (`ui/messages.slint:2108`); `src/ui/messages.rs:302` (`embed_title`).
- **Shots:** `chan/bot_embed_card_poll.png`, `chan/youtube_gif_embed.png`, `chan/spoiler_reply_blocked_linkembed.png`.
- **Done when:** the colour stripe, author, title, fields inline and not, thumbnail, image, video card with its play button, footer. I07.

### C06 · Bot components (Sonnet · L)
- **Owns:** `anomi-chat/src/components/`, `anomi-vm/src/chat/components.rs`.
- **After:** C02, F11.
- **Ports:** `CardView`, `CardButtonView` (`ui/messages.slint:2257`, `:2225`); `src/ui/refresh.rs:785` (`show_bot_interactions`); `src/ui/callbacks.rs:812`, `:827`.
- **Shots:** `dm/discord_components_notice.png`.
- **Done when:** containers, sections with thumbnails, link and action buttons, the failed-interaction line. I08.

### C07 · Polls (Sonnet · M)
- **Owns:** `anomi-chat/src/poll/`, `anomi-vm/src/chat/poll.rs`.
- **After:** C02, F12.
- **Ports:** `PollView` (`ui/messages.slint:690`), `PollDialog` (`ui/dialogs.slint:932`); `src/ui/polls.rs`.
- **Shots:** `chan/bot_embed_card_poll.png`, `dialog/create_poll.png`.
- **Done when:** voting, results, remaining time, ending early; the create dialog with its counters and duration. I08.

### C08 · Emoji and sticker cards (Sonnet · S)
- **Owns:** `anomi-chat/src/cards/`, `anomi-vm/src/chat/cards.rs`.
- **After:** C02, F06.
- **Ports:** `EmojiCardView`, `MessageStickerCardView` (`ui/messages.slint:1079`, `:957`); `src/ui/refresh.rs:759`, `:773`, `:948`.
- **Done when:** clicking a custom emoji or a sticker opens its card with its server (no golden shot: compare by hand). I08.

### C09 · Video in a message and full screen (Opus · L)
- **Owns:** `anomi-chat/src/video/`, `anomi-vm/src/chat/video.rs`.
- **After:** C04, F05.
- **Ports:** `VideoCell`, `VideoControls`, `VideoViewer` (`ui/messages.slint:524`, `:412`, `:2731`); `src/ui/refresh.rs:5176` (`show_video`); `src/ui/callbacks.rs:4172` (`place_videos`).
- **Shots:** `chan/youtube_gif_embed.png`.
- **Done when:** a video plays in its cell and goes full screen without restarting; frames come as new handles without a copy per frame (measure); nothing plays while still. I09.

### C10 · Image viewer (Sonnet · M)
- **Owns:** `anomi-chat/src/viewer/`, `anomi-vm/src/chat/viewer.rs`.
- **After:** F05, F06.
- **Ports:** `ImageViewer`, `ViewerMenu` (`ui/messages.slint:2610`, `:2563`); `src/ui/refresh.rs:5279`; `src/ui/callbacks.rs:1146`, `:1159`.
- **Done when:** opens over the window, steps through a message's pictures, zooms, copies and saves. I09.

### C11 · Voice messages (Sonnet · S)
- **Owns:** `anomi-chat/src/voice_message.rs`.
- **After:** C04.
- **Ports:** `VoicePlayButton` (`ui/messages.slint:1709`); `src/ui/refresh.rs:5243`.
- **Done when:** play, pause, the waveform's progress, speed (no golden shot). I09.

### C12 · The message menu (Sonnet · M)
- **Owns:** `anomi-chat/src/menu.rs`, `anomi-vm/src/chat/menu.rs`.
- **After:** F06, C02.
- **Ports:** `MessageMenu` (`ui/messages.slint:1394`); `src/ui/menus.rs:110` (the message's rows), `:560` (apps).
- **Shots:** `menu/message_context.png`, `menu/apps_submenu.png`.
- **Done when:** the four quick emoji, every row and icon in Discord's order, the apps submenu, red rows. I10.

### C13 · Editing in place (Opus · M)
- **Owns:** `anomi-chat/src/inline_edit.rs`.
- **After:** C02, F10.
- **Ports:** `InlineEditor` (`ui/messages.slint:2873`); `src/ui/refresh.rs:4125`; `src/ui/callbacks.rs:3966`, `:4035`.
- **Done when:** the row turns into an editor that survives the list rebuilding the row, Escape cancels, Enter saves. I10.

### C14 · The composer's frame (Sonnet · M)
- **Owns:** `anomi-chat/src/composer/{mod,staged,request_bar}.rs`, `anomi-vm/src/chat/composer.rs`.
- **After:** F10, F11.
- **Ports:** `ComposerFrame`, `StagedTile`, `RequestBar` (`ui/composer.slint:68`, `:12`, `:196`), with F11's ping toggle; `src/ui/refresh.rs:4161` (`show_composer`); `src/ui/callbacks.rs:940`, `:2537`-`:2570`.
- **Shots:** `composer/reply.png`, `composer/reply_attachment_mention.png`, `menu/composer_plus.png`, `dm/person_chat.png`.
- **Done when:** the reply bar, staged files with spoiler and remove, the plus menu, GIF, sticker and emoji buttons, who is typing, slow mode, a message request's bar. I11.

### C15 · Suggestions (Sonnet · M)
- **Owns:** `anomi-chat/src/composer/suggestions.rs`.
- **After:** C14.
- **Ports:** `SuggestionFrame`, `SuggestionHeading`, `MentionList`, `EmojiSuggestions` (`ui/composer.slint:104`, `:113`, `:119`, `:163`); `src/ui/refresh.rs:4337`; `src/ui/callbacks.rs:2806`-`:2900`.
- **Shots:** `composer/reply_attachment_mention.png`, `composer/emoji_autocomplete.png`.
- **Done when:** members, roles and channels for `@` and `#`, emoji for `:`, arrows and Tab choose. I11.

### C16 · Slash commands (Sonnet · M)
- **Owns:** `anomi-chat/src/composer/commands.rs`, `anomi-vm/src/chat/commands.rs`.
- **After:** C14.
- **Ports:** `CommandFrame`, `CommandHeading`, `CommandList`, `CommandPills` (`ui/app_commands.slint:22`, `:31`, `:40`, `:117`); `src/ui/app_commands.rs`.
- **Shots:** `composer/slash_suggestions.png`.
- **Done when:** the list by app, options as pills, required options marked. I11.

### C17 · The emoji picker (Opus · L)
- **Owns:** `anomi-chat/src/pickers/emoji/`, `anomi-vm/src/chat/emoji_picker.rs`.
- **After:** F05, F07, F06.
- **Ports:** `EmojiPicker`, `EmojiFace`, `PickerRail`, `EmojiButton` (`ui/messages.slint:1223`, `:64`, `:116`, `:179`); `src/ui/emoji_grid.rs`, `src/ui/callbacks.rs:426` (`show_emoji`), `:1856`-`:1902`, `:575`.
- **Shots:** `picker/reaction_emoji.png`, `picker/composer_emoji.png`.
- **Done when:** sections, the category rail, favourites and frequent, locked emoji, search, pictures loaded three screens ahead and let go behind; opens at the composer and at a message. I12.

### C18 · The GIF picker (Sonnet · M)
- **Owns:** `anomi-chat/src/pickers/gif.rs`, `anomi-vm/src/chat/gif_picker.rs`.
- **After:** C17.
- **Ports:** `GifPicker`, `PickerTab`, `FavoriteStar` (`ui/gifs.slint:68`, `:235`, `:32`); `src/ui/gifs.rs` (the masonry).
- **Shots:** `picker/gif.png`.
- **Done when:** category tiles, search results in masonry, favourite stars. I12.

### C19 · The sticker picker (Sonnet · M)
- **Owns:** `anomi-chat/src/pickers/sticker.rs`, `anomi-vm/src/chat/sticker_picker.rs`.
- **After:** C17.
- **Ports:** `StickerPicker` (`ui/stickers.slint:31`); `src/ui/stickers.rs`.
- **Shots:** `picker/sticker.png`.
- **Done when:** sections by server and default packs, animated stickers only while shown and playing. I12.

### C20 · Bot forms and menus (Sonnet · M)
- **Owns:** `anomi-chat/src/bot_forms/`, `anomi-vm/src/chat/bot_forms.rs`.
- **After:** F06, F12.
- **Ports:** `BotModalDialog`, `BotSelectDropdown`, `Mark`, `MarkRow`, `RadioRow`, `CheckRow` (`ui/interactions.slint:155`, `:24`, `:116`, `:128`, `:152`, `:153`); `BotSelectBox` (`ui/bot_select.slint:15`); `src/ui/callbacks.rs:843`-`:917`.
- **Done when:** a bot's form with text, choices, checkboxes and files submits; a select menu picks several with emoji (no golden shot). I08.

### C21 · Search (Sonnet · M)
- **Owns:** `anomi-chat/src/search/`, `anomi-vm/src/chat/search.rs`.
- **After:** F07, F12.
- **Ports:** `HeaderSearch`, `SearchPanel`, `FoundCard` (`ui/header.slint:65`, `:647`, `:158`); `src/ui/refresh.rs:5779`, `:5812`; `src/ui/callbacks.rs:1642`.
- **Shots:** `header/search_focus.png`, `header/search_results.png`.
- **Done when:** the field grows when focused, results group by channel, a card jumps to its message. I13.

### C22 · Pins (Sonnet · S)
- **Owns:** `anomi-chat/src/pins.rs`.
- **After:** F06, C02.
- **Ports:** `PinsPopover` (`ui/header.slint:275`); `src/ui/callbacks.rs:1657`.
- **Shots:** `header/pins.png`.
- **Done when:** pinned messages as cards, jump and unpin. I13.

### C23 · The threads popover (Sonnet · S)
- **Owns:** `anomi-chat/src/threads/popover.rs`, `anomi-vm/src/chat/threads.rs`.
- **After:** F06, F14.
- **Ports:** `ThreadsPopover`, `ThreadEntry` (`ui/header.slint:406`, `:319`); `src/ui/refresh.rs:1921` (`show_threads`); `src/ui/callbacks.rs:1667`, `:1773`.
- **Shots:** `thread/list_popover.png`.
- **Done when:** search, Create, joined, other active and older sections, avatars and counts. I13.

### C24 · The thread beside a channel (Sonnet · M)
- **Owns:** `anomi-chat/src/threads/panel.rs`.
- **After:** C01, C14.
- **Ports:** `ThreadPanel` (`ui/thread.slint:13`); `src/ui/refresh.rs:3979` (`show_aside`); `src/ui/callbacks.rs:755`, `:1131`, `:2738`.
- **Shots:** `forum/post_open.png`, `thread/panel_menu.png`, `thread/panel.png`.
- **Done when:** its own timeline and composer beside the channel, the "…" menu, expand and close. I13.

### C25 · A new thread (Sonnet · S)
- **Owns:** `anomi-chat/src/threads/new.rs`.
- **After:** C14.
- **Ports:** `NewThreadPanel` (`ui/thread.slint:452`); `src/ui/refresh.rs:1789`; `src/ui/callbacks.rs:1677`-`:1709`, `:1761`.
- **Done when:** name, first message, private toggle, create (no golden shot). I13.

### C26 · A forum (Sonnet · M)
- **Owns:** `anomi-chat/src/forum/{list,card}.rs`, `anomi-vm/src/chat/forum.rs`.
- **After:** F07, F05.
- **Ports:** `ForumPanel`, `PostCard`, `TagPill`, `PostEmoji`, `StoredImage` (`ui/forum.slint:473`, `:92`, `:74`, `:61`, `:43`); `src/ui/refresh.rs:1995` (`show_forum`); `src/ui/callbacks.rs:1785`, `:1795`.
- **Shots:** `forum/list.png`, `small/forum.png`, `light/forum.png`.
- **Done when:** pinned post, tags with emoji, NEW badge, preview picture, counts, older posts. I13.

### C27 · A new forum post (Sonnet · M)
- **Owns:** `anomi-chat/src/forum/new_post.rs`.
- **After:** C26, C14.
- **Ports:** `NewPostForm` (`ui/forum.slint:212`); `src/ui/refresh.rs:2065`; `src/ui/callbacks.rs:1723`-`:1751`.
- **Shots:** `forum/new_post.png`, `forum/new_post_tags.png`.
- **Done when:** title, tags chooser, guidelines card that folds, message, Publish. I13.

### C28 · Forwarding (Sonnet · S)
- **Owns:** `anomi-chat/src/forward.rs`.
- **After:** F06, F12.
- **Ports:** `ForwardDialog` (`ui/dialogs.slint:231`); `src/ui/callbacks.rs:1278`, `:1842`.
- **Done when:** search places, pick several, add a note, send (no golden shot). I10.
