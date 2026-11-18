# Foundation: waves 0 to 2

What every screen stands on. Wave 0 is one task, done first and alone.
Wave 1 runs in parallel; wave 2 needs wave 1's list and pictures. Paths are
under `crates/`. See [the index](../tasks.md) for the fields.

## Wave 0

### F01 · Scaffold every area (Opus · S)
- **Owns:** the scaffold later tasks fill without adding a line to it. The area crates (`anomi-{chat,people,voice,settings}/Cargo.toml`, `anomi-{chat,people,voice,settings}/src/{lib,shots}.rs`; they join the workspace and both check scripts by their folder's name), the other manifests (`anomi-{theme,vm,widgets,screens}/Cargo.toml`, with `anomi-screens`' dependencies on the area crates) and module lists (`anomi-{theme,widgets}/src/lib.rs`), and `anomi-shots/`. In `anomi-vm`: `Vm`, `Intent` and the `parts!` macro (`anomi-vm/src/lib.rs`); the folders' parents, each area's field and `Intent` variant wired (`anomi-vm/src/{chat,people,voice,settings,sidebar,places}/mod.rs`; `sidebar/mod.rs` keeps `Sidebar`, `Place` and the nested `Intent` whose parts N03-N05 fill); the fixtures' parents (`anomi-vm/src/fixtures/{mod,chat/mod,people/mod,voice/mod,settings/mod,sidebar/mod,places/mod}.rs`, with `Shot`). In the screens crates: the module lists (`anomi-screens/src/{lib,shots,places/mod}.rs`, `anomi-chat/src/threads/mod.rs`, `anomi-people/src/profile/mod.rs`, `anomi-settings/src/server/roles/mod.rs`), `anomi-screens/src/sidebar/mod.rs` composing the parts N03-N05 fill, and each crate's one test binary (`anomi-{screens,chat,people,voice,settings}/tests/areas/main.rs`, with `anomi-screens/tests/areas/sidebar.rs`). Besides: every module, file and folder another task's **Owns** names, created as a stub with its `mod` line in its parent, its fixtures file, its test module and a `shots()` its crate's `shots.rs` collects; in `anomi-app` a stub for each I task's module with every intent routed to one; the skeleton's files moved where [the table](../tasks.md#what-the-skeleton-already-has) says.
- **After:** nothing.
- **Ports:** the layout of `ui/yapfast.slint` into area folders; `docs/iced/README.md`'s crate table.
- **Done when:** every later task only fills files it owns, and adds no line to a file it doesn't; `scripts/iced-coverage` finds no path owned twice and no file of the writers' crates without its one owner; `scripts/check-iced --review` passes; the skeleton looks as before (its shots unchanged to the byte).

## Wave 1

### F02 · The rest of the style catalog (Sonnet · M)
- **Owns:** `anomi-theme/src/catalog/` (F01 made `{checkbox,toggler,slider,radio,pick_list,menu,progress_bar,combo_box,float,qr_code}.rs` in it), `anomi-theme/src/{metrics,typography,palette}.rs`, `anomi-theme/src/builtin/`.
- **After:** F01.
- **Ports:** `Middle`, `Label` (`ui/theme.slint:154`, `:158`); every `Theme.*` use in `ui/controls.slint` for the default looks; `src/ui/theme.rs:1091` (mention, text, muted colours).
- **Shots:** `settings/appearance.png`, `settings/sounds.png` (toggles, sliders, segmented controls).
- **Done when:** each built-in widget compiles with `Theme` and looks like its Slint counterpart in all twelve themes; a test renders one of each in Bonfire and Latte.

### F03 · Fonts, Twemoji and name faces (Opus · M)
- **Owns:** the crate `anomi-fonts/` (F01 made it empty, and `anomi-theme` depends on it; a build script making the Twemoji CBDT font as the root `build.rs` does, cached in `OUT_DIR`), `anomi-theme/src/fonts.rs`.
- **After:** F01.
- **Ports:** the `Twemoji` global (`ui/theme.slint`), `src/twemoji.rs`, `src/emoji_font.rs`, `src/emoji_font_tables.rs`, `src/han_fallback.rs`, `ui/fonts/README.md` (the eleven display-name faces), `src/custom_font.rs` (fonts the person loads).
- **Done when:** emoji draw in colour with skin tones through the "Yapfast Twemoji" family; name faces load by family; a person's font replaces DM Sans; the writers' cold check grows by at most 3 s (measure it; the build script runs only in crates that need emoji).

### F04 · The whole icon set (Sonnet · S)
- **Owns:** `anomi-vm/src/icon.rs` (`Icon`: every SVG of `ui/icons`, one variant each, so view-models hold icons), `anomi-widgets/src/icon.rs` (drawing one, re-exporting `Icon`).
- **After:** F01.
- **Ports:** `ui/icons/README.md`, every `@image-url("icons/…")`.
- **Done when:** all 158 icons have a variant; a test renders each at 20 px.

### F05 · Pictures: store, rounding, animation (Opus · XL)
- **Owns:** `anomi-widgets/src/picture/` (round and rounded-square copies, animated frames with a shared clock, pause while still or off screen), `anomi-widgets/examples/picture_memory.rs` (what pictures hold once drawn, the store's and the renderer's, measured without a window), `anomi-vm/src/fixtures/pictures.rs` (the fake's stills for fixtures: avatars, icons, banners, emoji), `anomi-app/src/pictures/` (requests for what shows, the 8 MiB budget, frame buffers through `yapfast_core::picture::set_frame_buffers`).
- **After:** F01.
- **Ports:** `Picture` (`ui/controls.slint:423`); `src/ui/pictures.rs` (`PictureStore`, slots, `cached_picture:77`), `src/ui/emoji_animation.rs` (queue, cache, budget), `src/ui/callbacks.rs:31` (`fetch_shown_pictures`).
- **Shots:** `dm/conversation_profile.png`, `chan/showcase_emoji_stickers_gif.png` (animated stickers).
- **Done when:** avatars are round on any ground (replaces `picture/`'s corner cover); only shown pictures are requested; animated emoji and stickers play only while `motion.playing()`; fixtures can carry pictures (the fake's two-tone avatars drawn in code), so shots show pictures where the golden shots do; idle memory with 100 pictures measured against Slint's.

### F06 · Layers, popups, menus and the tooltip (Opus · XL)
- **Owns:** `anomi-widgets/src/layer/` (a stack of layers over the window: backdrop, way in and out by `Motion::popup`, Escape top down, focus kept inside and given back), `anomi-widgets/src/menu/` (anchored popups that flip and stay in the window, menus as wide as their widest row, submenus), `anomi-widgets/src/tip.rs`, `anomi-vm/src/menu.rs` (the context menu open over the window: its rows, each carrying the `anomi_vm::Intent` it sends, and where it opened; `vm.menu`), `anomi-theme/src/motion.rs`.
- **After:** F01, F02.
- **Ports:** `PopupMotion`, `PopupEntry` (`ui/theme.slint:216`, `:234`), the `Popup`, `Tip` and `BadgeTip` globals; `MenuItem`, `MenuRows`, `ContextMenu`, `TipIcon` (`ui/controls.slint:1156`, `:1287`, `:1332`, `:1895`); `src/ui/menus.rs:110` (`menu`) for the rows' shapes.
- **Shots:** `menu/message_context.png`, `menu/apps_submenu.png`, `menu/server_header.png`, `header/notifications.png`.
- **Done when:** a menu opened at a point and from a button looks as the shots; Escape closes the top layer only; leaving plays 120 ms and none while still; a test opens, closes and checks focus return.

### F07 · The virtual list (Opus · XL)
- **Owns:** `anomi-widgets/src/list/` (rows of any height built only near the viewport, a height cache with estimates, anchoring by row key, laying out from the end, interruptible smooth scrolling, a report of the visible keys with six rows of margin at most every 50 ms, middle-click autoscroll, dragging rows to reorder with an insertion line and edge scrolling), `anomi-widgets/src/scrollbar.rs`.
- **After:** F01.
- **Ports:** `ScrollBar`, `ScrollView`, `ListView`, `AutoscrollArrow`, `AutoscrollMarker` (`ui/scrollview.slint:18`, `:110`, `:168`, `:184`, `:195`); `vendor/i-slint-core` (`yapfast_anchor.rs`, the repeater's anchoring); the timeline's behaviour in `ui/yapfast.slint` (follow the newest, jump to a message at a third of the height, short history pinned to the composer); `src/ui/callbacks.rs:707` (`media_viewport`).
- **Done when:** 5 000 rows of mixed heights scroll with only visible rows built (memory flat), loading older rows above keeps the view still, a late-growing row at the bottom keeps the list at the end, a jump lands at a third; tests for each.

### F11 · Buttons and toggles (Sonnet · M)
- **Owns:** `anomi-widgets/src/button/` (one module each).
- **After:** F02.
- **Ports:** `ActionButton`, `PingToggle`, `IconButton`, `DeviceToggle`, `CallButton`, `HeaderToggle` (`ui/controls.slint:8`, `:84`, `:105`, `:179`, `:1070`, `:1674`).
- **Shots:** `composer/reply.png` (ping toggle), `voice/joined_sidebar_dock.png`, `dm/person_chat.png` (header buttons).
- **Done when:** each has hover, pressed, disabled and focus looks, a tooltip, Space and Enter; a test per widget.

### F12 · Fields, sliders and meters (Sonnet · M)
- **Owns:** `anomi-widgets/src/field.rs`, `anomi-widgets/src/slider/`.
- **After:** F02.
- **Ports:** `TextField`, `VolumeSlider`, `UprightVolumeSlider`, `SensitivitySlider`, `LevelBar` (`ui/controls.slint:987`, `:1526`, `:1737`, `:1818`, `:1798`).
- **Shots:** `settings/voice_video.png`, `voice/mic_device_menu.png`.
- **Done when:** fields keep a wrong draft without losing it; sliders snap and show their value; the level bar draws a value the app hands it and asks for no frames of its own.

### F13 · Loading and covers (Sonnet · S)
- **Owns:** `anomi-widgets/src/skeleton.rs`, `anomi-widgets/src/cover.rs`, `anomi-widgets/src/new_tag.rs`.
- **After:** F02.
- **Ports:** `Skeleton`, `MessagesSkeleton`, `SpoilerCover`, `Banner` (`ui/controls.slint:1108`, `:1123`, `:1146`, `:1722`); `NewTag` (`ui/yapfast/new-tags.slint:19`).
- **Done when:** the shimmer sweeps only while `motion.watched()`; a spoiler cover blurs and reveals on click.

### F14 · Identity: avatar, tag, badges (Sonnet · M)
- **Owns:** `anomi-widgets/src/avatar.rs` (decorations, animated avatars through F05), `anomi-widgets/src/badge.rs`, `anomi-widgets/src/tag.rs`, `anomi-widgets/src/presence.rs`, `anomi-vm/src/kind.rs` (the vocabulary several areas share, with `kind/` for the name style, the nameplate, and the profile effect and frame F15 and F16 draw; others ask for additions in their review).
- **After:** F02, F05.
- **Ports:** `Avatar`, `ServerTagChip`, `VoiceSpot`, `StatusText` (`ui/controls.slint:548`, `:472`, `:947`, `:891`); `StatusDot` (`ui/rail.slint:360`); `DiscordBadge` (`ui/discord.slint:6`).
- **Shots:** `dm/group.png`, `server/general_unread_divider.png` (member list).
- **Done when:** sizes 16 to 120 match the shots; presence shapes (mobile, idle moon, dnd bar) as Discord draws them.

### F15 · Names and their styles (Opus · L)
- **Owns:** `anomi-widgets/src/name/` (gradient, neon, toon, pop, gummy, prism and per-letter colours, drawn from glyph outlines), `anomi-widgets/src/nameplate.rs`.
- **After:** F03, F05.
- **Ports:** `StyledName`, `Nameplate` (`ui/controls.slint:731`, `:627`); `src/ui/name_style.rs`.
- **Shots:** `server/general_unread_divider.png` (member list), `dm/group.png`.
- **Done when:** every style of the fake's `--member-looks` matches; effects that move play only while `motion.playing()`.

### F16 · Profile effects and frames (Opus · M)
- **Owns:** `anomi-widgets/src/effect.rs`, `anomi-widgets/src/frame.rs`.
- **After:** F05.
- **Ports:** `EffectLayerView`, `ProfileEffect` (`ui/controls.slint:833`, `:860`); `ProfileFrameLayer` (`ui/people.slint:1153`).
- **Shots:** `profile/popout_member.png`, `profile/popout_carol.png`.
- **Done when:** effects play on hover once and hold still otherwise.

### F17 · Keyboard and focus (Opus · M)
- **Owns:** `anomi-app/src/keys.rs`, `anomi-widgets/src/focus.rs`.
- **After:** F06.
- **Ports:** the root `FocusScope`s and key handlers of `ui/yapfast.slint`; `src/ui.rs:331` (mouse back and forward), `src/ui.rs:748`; typing goes to the last composer (`type_anywhere` in `src/ui.rs`).
- **Done when:** Ctrl+K, Alt+Left/Right, Escape order, Tab through a dialog and typing anywhere behave as in the Slint app; tests drive them through the simulator.

### F18 · Window chrome (Sonnet · M)
- **Owns:** `anomi-screens/src/chrome/`, `anomi-vm/src/chrome.rs` (whether the frame is drawn, maximized, can go back; its intents go to I14).
- **After:** F11.
- **Ports:** `WindowButton`, `TitleBar`, `ResizeEdges` (`ui/title_bar.slint:4`, `:28`, `:87`); `CaptionButton`, `BarButton`, `WindowBar` (`ui/yapfast/window-bar.slint:4`, `:31`, `:74`); `src/ui/title_bar.rs`.
- **Done when:** the window draws its own frame when the system's is off, drags and resizes, and the bar's back and forward arrows send their intents. N01 places `chrome::view` in the window; until then its tests drive it alone, and its shots (whole windows) show it once N01 passed.

## Wave 2

### F08 · The message paragraph (Opus · XL)
- **Owns:** `anomi-widgets/src/paragraph/` (spans with bold, italic, code, strike, underline, links, mention pills, spoilers; inline custom emoji and Twemoji pictures; quotes with their bar and code blocks; at most N lines and its height; links, emoji and mentions under the pointer), `anomi-vm/src/text/` (the span model, and the Markdown to spans parser beside it, on the `pulldown-cmark` F01 added to `anomi-vm`).
- **After:** F03, F05.
- **Ports:** `TextBlockView`, `TextBlocks` (`ui/messages.slint:2041`, `:2092`); `src/ui/messages.rs:207` (`markdown`), `:215`, `:302`; `src/emoji_text.rs`, `src/custom_emoji.rs`; the `sharedparley` patches in `vendor/i-slint-core`.
- **Shots:** `chan/history_plain_a.png`, `chan/showcase_emoji_stickers_gif.png`, `chan/spoiler_reply_blocked_linkembed.png`.
- **Done when:** the showcase messages match; a 12-line clamp measures right; 1 000 paragraphs cost no more than the probe's 51 KB each.

### F09 · Selection across messages (Opus · L)
- **Owns:** `anomi-widgets/src/selection/`.
- **After:** F07, F08.
- **Ports:** `vendor/i-slint-core` `yapfast_selection.rs`; `copy_whole_messages`, `mark_autoscrolls` in `src/ui.rs`; `src/ui/callbacks.rs:2579`.
- **Done when:** a drag over messages scrolled out of the built window still copies them in reading order, pictures as `[Picture]`; double and triple click; the list scrolls at its edges while dragging.

### F10 · The composer field (Opus · XL)
- **Owns:** `anomi-widgets/src/composer/`.
- **After:** F08.
- **Ports:** `Composer` (`ui/composer-field.slint:14`); `yapfast_marks.rs`, `yapfast_typing.rs` in `vendor/i-slint-core`; `src/ui/callbacks.rs:4078` (`composer_caret`), `:2908` (`draft_edited`), `src/paste.rs`.
- **Shots:** `composer/reply_attachment_mention.png`, `composer/slash_suggestions.png`, `composer/emoji_autocomplete.png`.
- **Done when:** grows to ten lines then scrolls to the caret; reports the caret as a byte offset for `@`, `#` and `:`; Tab, arrows and Escape go to suggestions when they show; Enter sends, Shift+Enter breaks; pasted pictures and files stage; a room's draft never lands in another; IME works under fcitx5.
