# Writing

## Writing

The composer (`composer.rs`) shows, in a message
request's place, its Accept and Ignore; over the field, the reply's bar
with "@ On" or "@ Off" (whether the reply mentions its author) and its
cancel; in a channel this account may not write in, Discord's one bar
saying so; under it, why it can't send (timed out until a time in the
clock Settings chose, Discord disconnected) when it can't, and in a build
with Matrix whether the room is end-to-end encrypted (with a lock if it
is; an unencrypted Matrix room is read only). The field writes the core's
draft for the open room, which
also tells the room this account is typing; its outline lightens while
it has the keys. Enter sends, Shift+Enter breaks the line, Up in an
empty composer edits the last message of one's own, Ctrl+E opens the
emoji picker, Ctrl+G the GIFs. In a field built on `field.rs` (the
composer, the edit, a thread's and a post's, the searches) a click with
Shift held extends the selection, text selected with the pointer becomes
the desktop's primary selection, and a middle click pastes it at the
pointer, taking no autoscroll; the field that took the keys last, by any
button, holds them alone.

Files go with the next message (`uploads.rs`) from the system's file
dialog (the + where nothing else can start, or the + menu's "Upload a
File", whose Create Thread shows greyed until threads come; its Create
Poll opens Create a Poll, see [dialogs](dialogs.md)), from Ctrl+V (files
copied in a file manager, or a picture, which
the root crate's `paste.rs` reads and saves as `image.<extension>` in a
folder removed as Yapfast closes; text otherwise; files are looked for
even with ten waiting, so the core says why they can't go, and not where
files never go, the preview or a channel not to write in, which take the
text), or dropped on the
window (winit's drops on X11 and Windows, the root crate's `drop.rs` on
Wayland, whose data device winit doesn't take). Files over the thread
beside below its head go to the thread instead (`uploads.rs`'s
`over_aside`): on Wayland the desktop says where the drag is; on X11 and
Windows, where winit says nothing of where, the root crate's
`drop::Pointer` asks the platform where the pointer is (`Desktop::pointer`,
made once as files come over the window) every 40 ms while they hover
and as they're let go. A drag
shows Discord's card, "Upload to #general", and a drop that can't go whole
says why until Got It. They wait over the text as
tiles: a picture's preview (made off the window's thread at 240 px and
let go with its file) or a file's icon, either under the SPOILER cover
once marked, the name and size, the spoiler button where the room has
spoilers, and the X.

A message of one's own is edited in its row (`edit.rs`), tinted with
Discord's edit tint: the text becomes a field of its own there, with the
emoji button beside it and "escape to cancel • enter to save" under it,
both links. The room's draft and a reply being written stay as they
were. Starting an edit gives the field the keys and scrolls the row into
view; Enter saves (the core asks whether to delete a message emptied),
Escape cancels.

Over the field typed in last (`suggest.rs`), an `@` or `#` word suggests
the people, roles and channels the core finds, and a `:name` word of two
letters or more the emoji whose names hold it, a server's own first
(`yapfast_core::emoji`). Up and Down
light one, Tab, Enter or a click takes it, Escape closes them until the
word is left.

Apps' commands (`commands.rs`):
`/` in a Discord channel opens the list of the commands that fit it over
the composer (or says they're coming, or that none match), each with its
app's icon and name, eight rows in view and the rest a wheel away (only
the rows near the view are built; Up and Down scroll it to keep the lit
one in view); one taken, or its whole name and a space typed, shows
its options as pills over the text, which writes the focused one, and the
list offers that option's values. Tab and Shift+Tab move between the
pills, a click takes one (a file's opens the file dialog), Enter runs the
command (or asks for a file option's file), Escape closes the list and
then the command (at once where no list shows, emptying the composer),
and Backspace in the empty first option puts the
command's name back. What they show and do is the core's
(`application/slash.rs`): the draft carries the query and each option's
text, and `Action::Send` runs the command.

The expression picker (`picker/`): in a Discord
room the composer's GIF, sticker and emoji buttons open one panel of three
tabs over them (498 by 486; its left edge drags it wider, up to 8 px
right of the server rail or the window's edge, its grids following, the
width kept in `layout.json` as `picker_width`, and a double click puts it
back); elsewhere, and for the edit or a reaction,
the emoji alone at the reaction popup's size (408 by 432). The emoji tab
lists the Discord account's Favorites, its Frequently Used (for a
reaction, the emoji it reacts with most), each server's custom emoji
(those this account may not use here greyed with a lock), then Unicode's
categories; the rail jumps between them, a search lists the matches, the
foot names the emoji pointed at (or the rail's section, by its mark and
name), and each emoji's star keeps it among the
favourites as Discord's settings do. The GIFs tab is Discord's over Klipy:
a home of Favorites, Trending GIFs and the trending terms, a search a
quarter second after typing pauses, suggested terms under a search that
found few, the GIFs in a masonry of columns about 200 px wide; a click
sends one, Shift and a click puts it in the composer, its star ("Add to
Favorites", "Remove from Favorites") keeps it, Back goes home,
Escape goes home and then closes, and Ctrl+G closes: the one-line search
passes Ctrl+G on, and the window's keys hand it to `picker::gifs_key`
first. In Trending the search stays under the title, out of sight, so
typing still searches, and a search, which changes the header, asks for
the keys to stay in its field. The stickers tab lists Favorites,
Frequently Used, every server's own (the open server's first) and
Discord's packs in 96 px tiles; the foot names the sticker pointed at, or
the collection on the rail. A click anywhere in the picker leaves the keys
with its search (a press nothing in it takes comes to the frame as
`Event::Pressed`), so typing goes on searching. The tabs share
`picker::kit`; each holds
its layout while it shows and lets go of it, and of the pictures it made,
when another shows or the picker closes, and the GIFs' and stickers'
pictures are asked of the core only for the tiles in sight. The custom
emoji's pictures are the core's: as the emoji grid scrolls into other
rows (`emoji_reach`), those within three screens are asked for with
`Action::LoadPicturesFirst`, those in sight first, and the rest let go of
with `Action::KeepEmojiPictures`, which the root sends with every
picture the rest of the window shows (`Request::KeepEmoji`,
`App::pictures_shown`), so a status's or a profile's emoji stays, and
the one just picked for a status being written or a poll's answer;
those already asked for aren't asked for again; leaving
the tab lets go of them all the same way (the window going, with
`Action::UnloadEmojiPictures`, as no settle follows), and opening it starts
`Action::CacheEmojiPictures`.

Who else is typing shows just above the composer (`typing.rs`): three
dots under the avatars that rise in turn, drawn in their own widget at
twenty frames a second while they show, and "Sam is typing..." where the
messages' text starts, breathing between half and full
brightness under a veil of the chat's ground, drawn the same way. While
moving pictures don't play (`pictures::plays`) the dots stand level, the
words hold still, and neither asks for frames. Under a message of one's own that didn't go:
"Sending..." with Cancel while its files upload, "Not sent yet" while
the connection is down, "Not sent" with Retry; under an answer only this
account sees, "Only you can see this · Dismiss message".

Rows are built only near the view at heights worked out ahead; once built,
`Rows` measures them and the rows keep their real heights. While earlier
messages are read, the message at the view's top stays where it is as
rows change height, new messages come, or the oldest go; sending goes
back to the newest message.

## Apps' forms and commands

An app's form (`bot_modal.rs`) shows over
the dimmed window: its title, then its parts in order (Markdown to read,
a line or a paragraph to write, a select menu with its dropdown, radio
buttons, checkboxes, a checkbox of its own, files to upload with Choose
files and each file's Remove, or a note for a part Yapfast can't fill),
why it failed, and Cancel and Submit ("Submitting…" while Discord
answers). Escape, Cancel or a click outside closes it. What it holds is
the core's; the window keeps only the paragraphs' editors and the text to
read laid out, and lets go of them as it closes.

A message's menu has Apps after Pin message where the room's apps have
message commands: a submenu of them by name with their app's under it,
the chosen one run on the message. The menu asks for the room's command
index as it opens, and is built again with its Apps once the index comes.
