# Dialogs

Over the dimmed window, in the frame `dialog::dialog` draws; Escape or a
click outside closes
each, and what each lists or makes is the core's. The frame stays 24 px
inside the window each way, and a dialog built of parts (`Look::framed`)
scrolls them when the window is shorter than they are.

- The account's popover (`status.rs`), from the account card's picture
  or name: the account
  as Discord's own card shows it (the core's `account_profile`, asked for
  as it opens, its card made while the popover shows and let go as it
  closes): the banner's picture, else its colour, the theme's, or with
  neither the avatar's own colour, the avatar with its decoration and
  status, the name in its style with the server tag, and the badges; then
  the custom status, the game played and for how long (to the minute),
  and its 40 px rows: Edit Profile, the Discord status (its row shows the
  four to choose), Set or Change custom status, Copy User ID and User
  Settings.
- Set a custom status (`status.rs`): an emoji from the picker (opened over
  the dialog, beside its button; its badge takes it off), the words, and
  Clear After; it starts from the status set, and Save, Clear status and
  Cancel go to the core, which says while it saves and why it couldn't.
- The quick switcher (`switcher.rs`), from Ctrl+K wherever the keys are
  (the key in K's place where the layout has no Latin letters, as every
  shortcut's letter the window takes):
  the core's `switcher_places` for what's typed (Previous channels while
  nothing is), each in a 34 px row with its picture or channel icon, its
  name with the channel's category dim after it, where it is and its
  mentions; the search is 70 px tall with a plain 1 px edge; Up, Down and Tab move to the place Enter goes to.
- New message, from the DM list's pencil: the people the core finds for
  what's typed, each with Message, and what the search came to.
- Create a Poll, from the composer's + menu: the question with its count,
  the answers each with an emoji from the picker and a bin, Add another
  answer, the duration, Allow Multiple Answers, what's too long, and Post.

The dialogs a server's and a channel's menus lead to are
`server_dialogs.rs`'s.
An opening one's field takes the keys, else nothing under it does, and
Escape closes it.

- Invite: "Invite friends to" the server and the channel the link lands
  people in, the link with Copy (Copied once copied), how long it lasts
  and how often it works, why it couldn't be made, and Expire after and
  Max number of uses, starting from the link's, for Generate a new link.
- Join a Server, from the rail's button or a clicked invite link: the
  field (Join's alone; Enter finds the invite), the server once found
  with its icon and who is online, how joining goes (looking up, joining,
  already in it, a captcha or a form to finish in the browser), and Find
  Invite, Join Server or Open Server; Open the captcha again and Open in
  browser go through the core to `browser.rs`.
- Create channel (Text or Voice) and Create category, in the server or
  the category the menu was for, Duplicate channel with its name to change,
  and Delete channel or category in red, which Delete Channel in the
  channel's settings opens too; Enter does what the button does, and the
  core says while it saves and why it couldn't.
- Leave a server or group, in red: a group's offers to leave without
  notifying its other members; Enter leaves.

A clicked event, gift or authorization link opens its dialog
(`link_dialogs.rs`), saying what the core's `event_prompt`, `gift_prompt`
and `oauth_prompt` say. Each has its title
with a close button, what it found on the sunken ground, how it goes, why
it couldn't, and Close, or Cancel beside its main button.

- An event: its name, when and where it is, who made it and how many are
  interested, what it says, and Interested or Remove interest.
- A gift: what it grants, its plan and who sent it, and Redeem Gift, or
  Open in Browser for one only Discord redeems.
- An application's authorization: the application with its picture, in
  red what of it can take over the account or a server, what it asks
  for, the server it's added to (a list to choose from, or the one its
  link names), the address Discord sends on to with Copy where Yapfast
  won't open it, and Authorize.
