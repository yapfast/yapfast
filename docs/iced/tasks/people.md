# People: wave 3

Friends, Active Now, the member list, profiles, status and the profile
editor. Screens in `anomi-people/src/<area>/`, view-models in
`anomi-vm/src/people/<area>.rs`. See [the index](../tasks.md).

### P01 · Friends (Sonnet · M)
- **Owns:** `anomi-people/src/friends/{mod,row,tabs}.rs`, `anomi-vm/src/people/friends.rs`.
- **After:** F07, F14.
- **Ports:** `FriendsScreen`, `FriendItem`, `HeaderTab` (`ui/friends.slint:247`, `:170`, `:313`); `src/ui/refresh.rs:1059` (friend rows); `src/ui/callbacks.rs:3037`.
- **Shots:** `friends/online_tab_active_now.png`, `friends/all_tab.png`, `small/friends.png`, `light/friends.png`.
- **Done when:** tabs Online, All, Pending, Blocked with counts, rows with activity, voice place and chat button, search. I16.

### P02 · Requests (Sonnet · M)
- **Owns:** `anomi-people/src/friends/requests.rs`, `anomi-vm/src/people/requests.rs`.
- **After:** P01.
- **Ports:** `RequestsScreen`, `RequestItem`, `RequestMessage`, `RequestsHeading` (`ui/friends.slint:345`, `:82`, `:21`, `:150`); `src/ui/refresh.rs:6094` (`request_rows`); `src/ui/callbacks.rs:1062`, `:979`.
- **Shots:** `friends/requests.png`.
- **Done when:** friend requests with Accept and Ignore, message requests with their preview and New badge, spam folded. I16.

### P03 · Add a friend (Sonnet · S)
- **Owns:** `anomi-people/src/friends/add.rs`.
- **After:** F06, F12.
- **Ports:** `AddFriendDialog` (`ui/friends.slint:420`); `src/ui/callbacks.rs:1039`.
- **Shots:** `dialog/add_friend.png`.
- **Done when:** the field, its hint, sending and Discord's answers. I16.

### P04 · Active Now (Sonnet · M)
- **Owns:** `anomi-people/src/active_now/`, `anomi-vm/src/people/active_now.rs`.
- **After:** F14, F05.
- **Ports:** `ActiveNowColumn`, `ActiveCard`, `ActiveRow`, `Faces`, `RowIcon` (`ui/active_now.slint:186`, `:140`, `:108`, `:71`, `:88`); `src/ui/active_now.rs`; `src/ui/refresh.rs:1059` (cards kept while they stay).
- **Shots:** `friends/online_tab_active_now.png`.
- **Done when:** Spotify, game and voice cards with their times ticking only while the window is in front. I16.

### P05 · The member list (Sonnet · L)
- **Owns:** `anomi-people/src/members/{mod,row,group}.rs`, `anomi-vm/src/people/members.rs`.
- **After:** F07, F14, F15.
- **Ports:** `MemberPanel` (the list half, `ui/people.slint:97`), `RoleIcon` (`:46`); `src/ui/refresh.rs:2429` (`show_members`), `:2507` (`show_roster`); `src/ui/callbacks.rs:1964` (width), `:2005`.
- **Shots:** `server/general_unread_divider.png`, `dm/group.png`, `menu/member_context.png`.
- **Done when:** groups by role with counts, rows of two heights with name styles, nameplates, crowns, activity lines; thousands of rows scroll with memory flat; a right click opens the member menu (F06 with I10's rows). I16.

### P06 · Activity cards (Sonnet · M)
- **Owns:** `anomi-people/src/activity/`, `anomi-vm/src/people/activity.rs`.
- **After:** F05, F14.
- **Ports:** `ActivityCard`, `ActivityPicture`, `ActivityTime` (`ui/people.slint:780`, `:64`, `:763`); the Activity panel of `MemberPanel`; `src/ui/profile.rs:53` (`activity_panel`), `:21`, `:34`.
- **Shots:** `server/general_unread_divider.png` (Activity), `profile/popout_carol.png` (Spotify with progress).
- **Done when:** game, music with progress, stream and voice cards; times tick only in front. I16.

### P07 · The profile card (Opus · L)
- **Owns:** `anomi-people/src/profile/card/`, `anomi-vm/src/people/profile.rs`.
- **After:** F14, F15, F16, P06.
- **Ports:** `ProfileCard`, `ProfileTitle`, `ProfileHeading`, `MemberSince`, `ProfileAction` (`ui/people.slint:1326`, `:603`, `:607`, `:613`, `:1230`); `src/ui/profile.rs` (`profile_data`, `role_rows`); `src/ui/refresh.rs:5129` (`show_profile`).
- **Shots:** `profile/popout_member.png`, `profile/popout_carol.png`, `profile/popout_menu.png`, `dm/conversation_profile.png` (the DM's panel).
- **Done when:** the card in its five places (popout, DM panel, account card, full profile's side, member hover) with banner, frames, decoration, theme gradient, status bubble, role chips, note and "Message @name". I16.

### P08 · Badges (Sonnet · S)
- **Owns:** `anomi-people/src/profile/badges.rs`.
- **After:** F06, F05.
- **Ports:** `ProfileBadge`, `BadgeTipCard`, `BadgeRows` (`ui/people.slint:651`, `:716`, `:1201`), the `BadgeTip` global.
- **Shots:** `profile/popout_member.png`.
- **Done when:** badges in rows, the levelled badge's tooltip with its progress. I16.

### P09 · The full profile (Sonnet · L)
- **Owns:** `anomi-people/src/profile/full/`.
- **After:** P07.
- **Ports:** `ProfileModal`, `ProfileTabBar`, `ProfileTabEmpty`, `ProfileListRow`, `ShowcasePicture`, `BoardCard`, `WishlistCard` (`ui/people.slint:2067`, `:1279`, `:2046`, `:1079`, `:528`, `:554`, `:584`); `src/ui/callbacks.rs:1008`, `:1249`.
- **Shots:** `profile/full.png`, `profile/full_mutual_friends.png`, `profile/full_mutual_servers.png`.
- **Done when:** the tabs, mutual friends and servers, connections, member since and friends since, roles. I16.

### P10 · Status (Sonnet · M)
- **Owns:** `anomi-people/src/status/`, `anomi-vm/src/people/status.rs`.
- **After:** F06, F12.
- **Ports:** `StatusMenu`, `StatusOption`, `StatusDialog` (`ui/dialogs.slint:621`, `:567`, `:27`); `src/ui/callbacks.rs:1619`.
- **Shots:** `status/menu.png`, `status/submenu.png`, `status/custom_dialog.png`.
- **Done when:** the status choices with their explanations, the custom status with emoji and clear-after. I03.

### P11 · About a person (Sonnet · S)
- **Owns:** `anomi-people/src/person.rs`.
- **After:** F06, F12.
- **Ports:** `PersonDialog` (`ui/dialogs.slint:705`); `src/ui/refresh.rs:3587`; `src/ui/callbacks.rs:1026`, `:1408`.
- **Done when:** nickname, note and the asks it makes (no golden shot). I16.

### P12 · Server, tag and role cards (Sonnet · M)
- **Owns:** `anomi-people/src/cards/`.
- **After:** F06, F14.
- **Ports:** `ServerCard`, `TagCardView`, `RoleCard` (`ui/people.slint:884`, `:974`, `:2327`); `src/ui/refresh.rs:773`, `:2679`; `src/ui/callbacks.rs:1398`.
- **Done when:** the cards a server tag, a server's emoji and a role chip open (no golden shot). I16.

### P13 · Adding people to a DM (Sonnet · S)
- **Owns:** `anomi-people/src/add_people.rs`.
- **After:** F06, F12.
- **Ports:** `AddPeoplePopover` (`ui/header.slint:539`); `src/ui/callbacks.rs:1804`.
- **Done when:** search friends, pick, add or make a group (no golden shot). I16.

### P14 · The profile editor (Sonnet · L)
- **Owns:** `anomi-people/src/editor/{mod,customize,slots}.rs`, `anomi-vm/src/people/editor.rs`.
- **After:** P07, F12.
- **Ports:** `ProfileEditorDialog`, `ProfileCustomize`, `CustomizeGroup`, `CustomizeCell`, `SlotPlus`, `EditBadge`, `HoverEdit`, `MenuCard` (`ui/server.slint:1585`, `:792`, `:731`, `:739`, `:761`, `:684`, `:697`, `:779`); `src/ui/refresh.rs:3246`; `src/ui/callbacks.rs:1521`-`:1554`.
- **Shots:** `profile/editor.png`.
- **Done when:** nameplate, avatar, decoration, name style, theme, banner, effect, frame rows with the live card beside. I17.

### P15 · The editor's preview and colours (Sonnet · M)
- **Owns:** `anomi-people/src/editor/{preview,colour}.rs`.
- **After:** P14.
- **Ports:** `ProfileTabs`, `ColourPopover`, `ThemeHandle` (`ui/server.slint:1062`, `:989`, `:766`); `src/ui/callbacks.rs:1472`-`:1512`.
- **Shots:** `profile/editor.png` (tabs Board, Activity, Wishlist).
- **Done when:** the preview's tabs, the colour popover with hex that keeps a wrong draft. I17.

### P16 · The name style window (Sonnet · M)
- **Owns:** `anomi-people/src/editor/name_style.rs`.
- **After:** F15, P14.
- **Ports:** `NameStyleWindow` (`ui/server.slint:1158`); `src/ui/name_style.rs`; `src/ui/callbacks.rs:1426`.
- **Done when:** every font and effect previews live on the person's name (no golden shot). I17.
