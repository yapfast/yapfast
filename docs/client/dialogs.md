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
  the custom status, then live Rich Presence cards: the application's large
  picture with its small badge, name, details and state. A game's elapsed
  clock is green beside a controller and updates each second without
  rebuilding the view; music with start and end times shows a progress bar.
  Activity changes refresh these cards while the menu stays open, without
  another profile request. Their artwork is requested only while open;
  the plain detected game remains a fallback when no rich activity exists.
  The menu scrolls when its cards and actions exceed the window's height.
  Its 40 px rows follow: Edit Profile, the Discord status (its row shows the
  four to choose), Set or Change custom status, Switch Accounts, Copy User ID
  and User Settings. Switch Accounts shows once a Discord account is linked:
  with the pointer on its row, a 220 px list opens beside the card (the
  `Place::beside` of the card's second group), each linked account with its
  picture and username (the core's `discord_accounts`) and a tick on the one
  in use, then Manage Accounts; choosing one switches to it
  (`Action::SwitchDiscordAccount`). Pressing the row itself opens Manage
  Accounts, as Discord's client does.
- Manage Accounts (`accounts.rs`), from that list or the row
  (`manage_accounts` in the core's state), under "Switch accounts, sign in,
  sign out, go wild.": a 64 px card for each linked account with its picture,
  username and "Active account" under the one in use, Switch on the others,
  and a button for its menu, which opens under the card with Log out alone.
  "Add an account" swaps the accounts for Add Account's step: "Logging in to
  another account will let you easily switch between accounts on this
  device." over the sign-in screen's linking steps (`signin::link_panel`: the
  warning, the QR code, the token's field) with Back; once another account is
  linked the step ends. The words are Discord's client's. Escape closes the
  menu, then the step, then the dialog; closing stops any linking and lets go
  of the token pasted.
- Set a custom status (`status.rs`): an emoji from the picker (opened over
  the dialog, beside its button; its badge takes it off), the words, and
  Clear After; it starts from the status set, and Save, Clear status and
  Cancel go to the core, which says while it saves and why it couldn't.
- The quick switcher (`switcher.rs`), from Ctrl+K wherever the keys are
  (the key in K's place where the layout has no Latin letters, as every
  shortcut's letter the window takes):
  the core's `switcher_places` for what's typed (PREVIOUS CHANNELS while
  nothing is), each with its picture or channel icon, where it is and its
  mentions; Up, Down and Tab move to the place Enter goes to.
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
  with its icon and who is online, whether it is looking the invite up or
  the account is in the server already, and Find Invite, Join in Browser
  or Open Server; Join in Browser goes through the core to `browser.rs`.
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
