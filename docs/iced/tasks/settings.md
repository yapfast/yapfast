# Settings: wave 3

The person's settings (S) and a server's and a channel's (X). Screens in
`anomi-settings/src/<area>/`, view-models in `anomi-vm/src/settings/`.
Integration is I19 (the person's) and I20 (servers and channels). See
[the index](../tasks.md).

## The person's settings

### S01 · The settings frame and its parts (Sonnet · L)
- **Owns:** `anomi-settings/src/frame/`, `anomi-settings/src/kit/` (switch, choices, picker, area, link button, unsaved bar, card, pressable, the reach of a table's rows), `anomi-vm/src/settings/frame.rs` (which page shows, the unsaved bar).
- **After:** F06, F11, F12.
- **Ports:** `SettingsScreen`, `SettingsFrame`, `SectionsNavigation`, `SettingsTitle`, `SettingsHeading`, `SettingsSwitch`, `Choices`, `Picker`, `SettingsArea`, `LinkButton`, `UnsavedBar`, `SettingsCard` (`ui/settings.slint:582`, `:408`, `:368`, `:17`, `:21`, `:27`, `:132`, `:203`, `:214`, `:259`, `:311`, `:555`); `SettingsNavigation` (`ui/yapfast/settings-nav.slint:10`); `SettingsToggle` (`ui/yapfast/settings-toggle.slint:8`).
- **Shots:** `settings/account.png`, `small/settings.png`.
- **Done when:** the window over the app, the navigation that scrolls in a small window, Escape closes, every part looks as in the shots; the frame shows each page's `view` (S02-S10's, F01's stubs until they pass). I19.

### S02 · Account (Sonnet · S)
- **Owns:** `anomi-settings/src/account.rs`, `anomi-vm/src/settings/account.rs`.
- **After:** S01.
- **Ports:** `AccountSettings` (`ui/settings.slint:654`).
- **Shots:** `settings/account.png`.
- **Done when:** matches (the blocked list is `DiscordSettings`', S08). I19.

### S03 · Voice and video (Sonnet · L)
- **Owns:** `anomi-settings/src/voice.rs`, `anomi-vm/src/settings/voice.rs`.
- **After:** S01, F12.
- **Ports:** `VoiceSettings` (`ui/settings.slint:826`); `src/ui/callbacks.rs:2296`, `:3828`, `:3861`.
- **Shots:** `settings/voice_video.png`.
- **Done when:** devices, volumes, the gate's sensitivity with its live meter, noise suppression, the camera's preview; meters run only while the page shows in front. I19.

### S04 · Messages (Sonnet · M)
- **Owns:** `anomi-settings/src/messages.rs`, `anomi-vm/src/settings/messages.rs`.
- **After:** S01.
- **Ports:** `MessageSettings` (`ui/settings.slint:1634`); `src/ui/callbacks.rs:2303`.
- **Shots:** `settings/messages.png`.
- **Done when:** compact display, date format, 12-hour clock, the rest of the page. I19.

### S05 · Notifications and sounds (Sonnet · M)
- **Owns:** `anomi-settings/src/{notifications,sounds}.rs`, `anomi-vm/src/settings/{notifications,sounds}.rs`.
- **After:** S01, F12.
- **Ports:** `NotificationSettings`, `SoundSettings` (`ui/settings.slint:1786`, `:1821`); `src/ui/callbacks.rs:2400`-`:2438`.
- **Shots:** `settings/notifications.png`, `settings/sounds.png`.
- **Done when:** both pages match; each sound plays its preview. I19.

### S06 · Appearance (Sonnet · L)
- **Owns:** `anomi-settings/src/appearance/{mod,theme_tile,font_row}.rs`, `anomi-vm/src/settings/appearance.rs`.
- **After:** S01.
- **Ports:** `AppearanceSettings`, `ThemeTile`, `FontFileRow` (`ui/settings.slint:1371`, `:1209`, `:1318`); `src/ui/appearance.rs`; `src/ui/theme.rs:1125` (`cards`); `src/ui/callbacks.rs:2193`-`:2239`.
- **Shots:** `settings/appearance.png`, `settings/appearance_scrolled.png`, `settings/appearance_scrolled_more.png`, `light/settings_appearance.png`.
- **Done when:** the twelve theme tiles with live previews, the language list (Russian first), fonts added and reset, the looks' switches; the page shows S07's `appearance::editor::view` while a theme is edited (F01's stub until S07 passes). I19.

### S07 · The theme editor (Sonnet · M)
- **Owns:** `anomi-settings/src/appearance/editor.rs`, `anomi-vm/src/settings/theme_editor.rs`, `anomi-theme/src/custom.rs` (custom themes: storage in `layout.json`, import and export of `yapfast-theme` files), `anomi-theme/src/theme.rs` (what custom themes need of `Theme`).
- **After:** S06.
- **Ports:** `ThemeColourRow` (`ui/settings.slint:1285`); `src/ui/theme.rs:727`-`:1225` (parse, hex, `Custom`, import, export, `Editor`); `src/ui/callbacks.rs:2246`-`:2290`.
- **Shots:** `settings/theme_editor.png` (not its quirk: cancelling restores the theme).
- **Done when:** name, base, dark switch, 37 colours with hex fields that keep a wrong draft, save, export and import; a file written by the Slint app imports. I19.

### S08 · Discord and game activity (Sonnet · M)
- **Owns:** `anomi-settings/src/{discord,games}.rs`, `anomi-vm/src/settings/{discord,games}.rs`.
- **After:** S01.
- **Ports:** `DiscordSettings`, `DiscordLinkSettings`, `GameActivitySettings` (`ui/settings.slint:2195`, `:2110`, `:2134`); `src/ui/games.rs`.
- **Shots:** `settings/discord.png`, `settings/game_activity.png`.
- **Done when:** both pages match. I19.

### S09 · Keybinds (Sonnet · M)
- **Owns:** `anomi-settings/src/keybinds.rs`, `anomi-vm/src/settings/keybinds.rs`.
- **After:** S01.
- **Ports:** `KeybindSettings`, `KeyRecorder` (`ui/settings.slint:756`, `:713`); `src/ui/keybinds.rs`.
- **Shots:** `settings/keybinds.png`.
- **Done when:** recording a key, push to talk, the list. I19.

### S10 · Advanced, About and updates (Sonnet · M)
- **Owns:** `anomi-settings/src/{advanced,about}.rs`, `anomi-vm/src/settings/{advanced,about}.rs`.
- **After:** S01.
- **Ports:** `AdvancedSettings`, `AboutSettings` (`ui/settings.slint:1881`, `:1962`); `UpdateNotice`, `BuildStamp` (`ui/yapfast/build-info.slint:78`, `:108`); `src/ui/updates.rs`.
- **Shots:** `settings/advanced.png`, `settings/about.png`.
- **Done when:** both pages match; the update notice shows a prepared update. I19.

### S11 · Two-factor prompts (Sonnet · S)
- **Owns:** `anomi-settings/src/mfa.rs`, `anomi-vm/src/settings/mfa.rs`.
- **After:** F06, F12.
- **Ports:** `MfaDialog` (`ui/mfa.slint:36`).
- **Done when:** a code or password asked for a held request, with its errors (no golden shot). I05.

## A server's and a channel's settings

### X01 · Server settings frame and profile (Sonnet · M)
- **Owns:** `anomi-settings/src/server/{mod,profile}.rs`, `anomi-vm/src/settings/server.rs`.
- **After:** S01.
- **Ports:** `ServerSettingsScreen`, `ServerProfileSection` (`ui/server_settings.slint:320`, `:100`); `src/ui/server_settings.rs:26`-`:114`.
- **Shots:** `ssettings/profile.png`.
- **Done when:** the sections' navigation and the profile page with its pictures; the frame shows each section's `view` (X02-X12's, F01's stubs until they pass). I20.

### X02 · Engagement and boosts (Sonnet · M)
- **Owns:** `anomi-settings/src/server/{engagement,boosts}.rs`, `anomi-vm/src/settings/{engagement,boosts}.rs`.
- **After:** X01.
- **Ports:** `EngagementSection`, `BoostSection`, `BoostBackground` (`ui/server_settings.slint:159`, `:280`, `:237`).
- **Shots:** `ssettings/engagement.png`, `ssettings/boosts.png`.
- **Done when:** both pages match. I20.

### X03 · Members (Sonnet · L)
- **Owns:** `anomi-settings/src/server/members.rs`, `anomi-vm/src/settings/members.rs`.
- **After:** X01, F07.
- **Ports:** `MembersSection`, `RoleChip` (`ui/server_people.slint:117`, `:98`); `src/ui/server_settings.rs:506`.
- **Shots:** `ssettings/members.png`.
- **Done when:** the virtual table of thousands of members with roles, sorting and search. I20.

### X04 · Invites (Sonnet · S)
- **Owns:** `anomi-settings/src/server/invites.rs`, `anomi-vm/src/settings/invites.rs`.
- **After:** X01.
- **Ports:** `InvitesSection`, `Countdown` (`ui/server_people.slint:295`, `:283`).
- **Shots:** `ssettings/invites.png`.
- **Done when:** the table with uses and expiry counting down only in front. I20.

### X05 · Safety (Sonnet · M)
- **Owns:** `anomi-settings/src/server/safety.rs`, `anomi-vm/src/settings/safety.rs`.
- **After:** X01.
- **Ports:** `SafetySection`, `SafetyChoices` (`ui/moderation.slint:143`, `:97`), `SecurityActionsDialog` (`ui/server_people.slint:400`).
- **Shots:** `ssettings/security.png`.
- **Done when:** the page and the actions dialog match. I20.

### X06 · Bans (Sonnet · S)
- **Owns:** `anomi-settings/src/server/bans.rs`, `anomi-vm/src/settings/bans.rs`.
- **After:** X01.
- **Ports:** `BansSection`, `BanDialog` (`ui/moderation.slint:193`, `:265`).
- **Shots:** `ssettings/bans.png`.
- **Done when:** the list, search, unban, the ban dialog. I20.

### X07 · Audit log (Sonnet · M)
- **Owns:** `anomi-settings/src/server/audit.rs`, `anomi-vm/src/settings/audit.rs`.
- **After:** X01, F07, F08.
- **Ports:** `AuditLogSection`, `AuditFilter` (`ui/moderation.slint:330`, `:311`).
- **Shots:** `ssettings/audit_log.png`.
- **Done when:** a virtual list of formatted entries with filters. I20.

### X08 · The role list (Opus · M)
- **Owns:** `anomi-settings/src/server/roles/list.rs`, `anomi-vm/src/settings/roles.rs`.
- **After:** X01, F07.
- **Ports:** `RolesSection`, `RoleListRow`, `RoleMark` (`ui/roles.slint:341`, `:143`, `:127`); `src/ui/server_settings.rs:518` (`roles_prompt`).
- **Shots:** `ssettings/roles.png`.
- **Done when:** roles reorder by dragging their handle within what the account may move. I20.

### X09 · The role editor (Sonnet · L)
- **Owns:** `anomi-settings/src/server/roles/{editor,members}.rs`, `anomi-vm/src/settings/role_editor.rs`.
- **After:** X08.
- **Ports:** `RoleTabButton`, `RoleMemberLine`, `RoleConfirmDialog`, `AddMembersDialog` (`ui/roles.slint:295`, `:312`, `:664`, `:713`); `src/ui/server_settings.rs:756` (`hex_color`).
- **Shots:** `ssettings/role_editor.png`.
- **Done when:** the locked notice, tabs, colour grid with hex, icon, the permission toggles, members added and removed. I20.

### X10 · Emoji (Sonnet · M)
- **Owns:** `anomi-settings/src/server/emoji.rs`, `anomi-vm/src/settings/emoji.rs`.
- **After:** X01, F05.
- **Ports:** `EmojiSection`, `EmojiUploadDialog`, `UrlPicture` (`ui/server_expressions.slint:90`, `:182`, `:81`).
- **Shots:** `ssettings/emoji.png`.
- **Done when:** the table of emoji and uploading one. I20.

### X11 · Stickers (Sonnet · M)
- **Owns:** `anomi-settings/src/server/stickers.rs`, `anomi-vm/src/settings/stickers.rs`.
- **After:** X01, F05.
- **Ports:** `StickersSection`, `StickerTileView`, `StickerDialog` (`ui/server_expressions.slint:286`, `:246`, `:341`).
- **Shots:** `ssettings/stickers.png`.
- **Done when:** the tiles and uploading one. I20.

### X12 · Ownership (Sonnet · S)
- **Owns:** `anomi-settings/src/server/ownership.rs`, `anomi-vm/src/settings/ownership.rs`.
- **After:** X01.
- **Ports:** `OwnershipFrame`, `DeleteServerDialog`, `TransferOwnershipDialog` (`ui/server_ownership.slint:36`, `:69`, `:104`).
- **Done when:** both dialogs ask in red and need the server's name (no golden shot). I20.

### X13 · Channel settings: overview (Sonnet · M)
- **Owns:** `anomi-settings/src/channel/{mod,overview}.rs`, `anomi-vm/src/settings/channel.rs`.
- **After:** S01.
- **Ports:** `ChannelSettingsScreen`, `OverviewSection` (`ui/channel_settings.slint:466`, `:158`); `src/ui/channel_settings.rs`; `src/ui/callbacks.rs:1584`.
- **Shots:** `csettings/overview.png`, `dialog/delete_channel_confirm.png`.
- **Done when:** name, topic, slow mode stepper, age toggle, delete; the frame shows X14's `channel::permissions::view` on its Permissions section (F01's stub until X14 passes). I20.

### X14 · Channel permissions (Sonnet · M)
- **Owns:** `anomi-settings/src/channel/permissions.rs`, `anomi-vm/src/settings/permissions.rs`.
- **After:** X13.
- **Ports:** `PermissionsSection`, `ThreeState`, `ThreeStateLine`, `OverwriteConfirmDialog`, `AddOverwriteDialog` (`ui/channel_settings.slint:221`, `:80`, `:113`, `:322`, `:383`).
- **Shots:** `csettings/permissions.png`.
- **Done when:** synced notice, private toggle, roles with tri-state buttons, adding a role or member. I20.

### X15 · Making a channel or a category (Sonnet · S)
- **Owns:** `anomi-settings/src/channel/create.rs`, `anomi-vm/src/settings/create_channel.rs`.
- **After:** F06, F12.
- **Ports:** `ChannelDialog` (`ui/server.slint:516`); `src/ui/refresh.rs:3149`.
- **Shots:** `dialog/create_channel.png`, `dialog/create_category.png`.
- **Done when:** the type segments, name, create (Slint's `ChannelDialog` has no private toggle). I20.
