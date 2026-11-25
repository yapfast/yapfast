# Themes

Settings → Appearance offers the themes Yapfast comes with and the person's
own as quick choices. Settings → Themes is the dedicated theme-management
destination. It currently manages installed packages and custom themes, their
options, content permission, performance and reduced motion. Its Marketplace
and Add from URL tabs fetch public GitHub themes through the core's bounded,
verified package pipeline, show a stock-themed permission/cost review, and do
not persist anything until Install. An
installed theme shown in Appearance links
back to that page. Built-in and custom
themes are a full set of the colours of `Theme` in
`yapfast_core::theme` (`crates/yapfast-core/src/theme.rs`), plus whether it's
dark, which sets the scrollbars' shade and the colours worked out from the
others. The core holds the built-in ones as `BUILT_IN` and reads custom
themes and hex colours; `crates/yapfast-iced/src/themes.rs` makes the chosen
one the kit's `Theme` (`crates/yapfast-iced-kit/src/theme.rs`), which every
widget paints with. An installed package is read from the core theme store,
resolved with `Package::resolve_with`, and converted at that same boundary;
see [the client's theme](client/window.md#theme). The
choice is kept in `layout.json` as `theme`, the theme's id, and applied
before the window's first frame, and again to a window made anew. With none
kept, the window shows Bonfire. An id Yapfast doesn't know, or a deleted
theme's, shows Yap. The window-control command `theme ID` chooses and keeps
one as Appearance and Themes do.

## Installed themes

Settings → Themes reads installed packages from `<data>/themes` through
`yapfast_core::themes::install::Themes`. Their layout ids are
`installed:<package-id>`, keeping them separate from built-in and `custom:`
ids. The page reads as the other settings pages: a title, what themes are,
and underlined tabs (Installed, Marketplace, Add from URL, Create). Installed
shows cards two to a row, or a list (toggled on the INSTALLED line), each with
its preview, modes tag, authors and cost in plain words with Details. A card
offers **Apply**, or **Turn off theme** on the applied one (ringed and tagged),
and behind **…** **Try for 30 seconds**, **Roll back** when core has retained an
earlier revision, and **Remove**. The applied theme's own settings (options,
people's content, sounds) follow under its name, then Performance and Motion.
A trial shows as a pill over Settings' foot. Try shows the package for 30
seconds without changing `layout.json`; its banner offers **Keep** and
**Revert**, and expiry reverts automatically. A reviewed remote package can be
tried from checked files in a private temporary folder and is installed only
if Keep is chosen. The applied row offers **Turn off theme**, which keeps
`theme: null` and returns to Bonfire immediately and on
the next start. Each actual change also keeps the replaced id as
`previous_theme`; Settings → Themes offers **Back to _name_** while that
theme still exists. Re-choosing the current theme leaves the existing Back
target intact, and restoring a theme after Turn off consumes that target.
`Ctrl+Shift+T` is the emergency turn-off shortcut for a theme package (a
colour-only theme stays as it is), held keys counting once, and is
handled above settings and other overlays, and while a field such as the
composer has the keys, so a problematic package cannot trap the person
behind themed UI. Sign-in, a link's confirmation (leaving Yapfast, an app's
authorization, a gift) and a theme's review always show the whole window in
the stock look, with no theme's colours, faces, icons, art, overlays, tint or
particles, so a theme can't dress up or draw over what it asks the person to
judge. Removing the theme on trial ends the trial. Themes can't set the
danger colours; danger text is darkened or lightened, keeping its hue, until
it reads at 4.5:1 on the theme's chat. Started with `--default-theme`, Yapfast shows the default
look for that run only, keeping the theme chosen (Back offers it). A package missing its preferred dark
variant uses its light variant. A dual-variant package's Auto choice follows
iced's live system light/dark mode without changing its persisted package id.

Only the themes showing (the chosen one, one on trial, the creator's
preview) have their fonts, icons, sounds and art loaded; every other
installed theme keeps its name, options, cost and colours, and loads the
rest when it's applied or tried, letting it go again when it no longer
shows. `--default-theme` loads none of the theme it escapes, and a restart
(a renderer fallback, a restart from Settings, an update) keeps both
`--default-theme` and `--theme-dev`.
The gallery's thumbnails are each package's first preview shrunk to at most
672 by 378 and kept as PNGs in `<data>/theme-thumbs/`, named for the store
folder (its files' digest, so one never goes stale); a start reads those,
making any missing ones side by side, and drops those of removed packages.

Installed rows identify the package's authors, version, available dark/light
modes and description, and name any package fields this Yapfast left out.
An entry whose stored manifest is missing, changed or otherwise refused by
the core remains visible under **Themes that couldn't be opened**, together
with the verification error; it is never silently treated as uninstalled.

The page also keeps `theme_performance` (`full`, `balanced`, or `light`) and
`reduce_motion` in `layout.json`. Balanced is the default. These settings are
shared by installed themes; Light still permits colours, fonts, shapes and
spacing, while package animation is disabled. Reduce motion independently
turns off package flourish; both choices keep the stock look's short transitions
so menus and dialogs do not lose their ordinary opening and closing feedback.

Resolved package metrics travel inside the kit `Theme` (`with_metrics`), so a
theme value draws the same wherever it's copied, and a colour-only theme has
none. Surface radii, outlines and shadows now reach the
shared buttons, fields, composer, toggles, chips, menus, dialogs, tooltips,
cards, code blocks, embeds, message/list rows, rail icons, call tiles, pickers,
popouts, toasts and scrollbars. Header and list-label grounds, text and case,
profile-section frames and title bars, card top insets, and message-row inset
and stripe settings use the same path. Values a package does not provide keep
the exact stock style.

Spacing controls timeline message and group gaps, avatar size, chat padding,
channel and member row heights, and the sidebar and member-pane widths. A pane
width the person resized explicitly in `layout.json` wins over the theme's
suggested width. Popup duration, curve, opening and closing style apply to the
app's animated layers; hover and press choices apply to shared action and icon
buttons and rail icons. Performance and Reduce motion are applied when these
motion values are resolved, without a second settings store.

Installed layouts can dock the composer above or below the history, move or
hide the member pane, dock the server rail/navigation on the left or right,
hide the server rail, centre the message lane at
`chat_max_width`, and hide message avatars. Flat, bubbles, IRC and the
non-rotated part of cards are drawn by the existing virtual timeline. Bubble
grounds (including linear gradients), own-message ink, radius, padding and
maximum share are honoured; IRC time/name columns and separator are honoured;
cards use their ground, padding and shadow. Missing layout fields keep
the stock renderer. A top rail scrolls the same themed server tiles across
the window. Tree mode (`sidebar/tree.rs`) lays the sidebar out as one virtual
file tree: the root, `dm/`, then each folder and server, every place opening
and closing on its own. A place's mark (`▸`/`▾`, drawn as pixel triangles when
the theme keeps those glyphs) or a category's line opens and closes it; a
place's name goes there, or, there already, opens and closes it. The place
the window is in is open and the others closed unless changed; changes are
kept per place in `layout.json` (`tree_open`), and going to a place opens it
again. Categories fold through the stock `collapsed` state, and opening a
channel (a click, a notification, a mention jump) unfolds its category and
opens its folder by the one/many folder rule. A closed place or category
shows its mentions (`counts.mentions`), or else that it's unread
(`counts.unread`), on its own line. Only open places' rows are made, once per
list change, and the list builds only rows near its viewport; branch and
guide lines are drawn as 1 px lines, so `├──`/`└──` stay joined across rows of
any height. A click on a row gives the tree the keys until a press elsewhere:
Up/Down move, Right opens or steps into the first child, Left closes or steps
out to the parent, Enter/Space act as a click, Home/End go to the first and
last row, and Escape returns the keys to the composer; the row is outlined in
the accent once a key moves it. Opening is instant. Grid
members use the package's column count, tile size and member row styling. A
collapsed member pane is a 56 px avatar strip and expands to the
full list while pointed at.

Layout borders can replace the ordinary solid edge with solid, dashed or
dotted strokes at ten declared places: composer, card, menu, message, embed,
code, tile, divider, actions and pane edges, and dialogs and tooltips. Their
dash/gap, optional width and colour come from the resolved theme;
otherwise the surface's shape border and stock colour remain the fallback.

The non-rotating list subset also covers rail width, tile size/radius/gap/pad,
inside or below labels, lift hover, pill mode, divider size,
and mention-badge colours. Channel and member rows use package height and gap,
text size/case/ink, solid separators, zebra stripes and foot reserve; channel
rows additionally use icon size/gap/ink, selected underlines, voice meta and
unread glyph/badge modes. Member rows use avatar size/gap, presence text or
glyphs, activity/presence meta, online badges, row margins/insets, header
templates and metrics, and the Activity switch. Rail, channel and member-row
frames and selected overlays use the package's density-aware nine-slice list
art. Header, separator, footer, selected-prefix, unread/status glyph, rail
ground/below/between/foot and member-avatar frame slots use that same renderer;
avatar frame windows, presence rings/dots and the channel-header rule are also
honoured. List-art sprite sheets follow Performance and Reduce motion just as
surface art does. Rail tiles take their reflections and lean (see the cards
below); magnify list motion is not applied.

Package icon replacements are decoded once and flow through the kit's existing
glyph calls. Package app/ring sounds replace the corresponding core samples;
menu, popup, toggle, send, reaction and theme-applied cues use the interface
sound slots. Settings → Themes separately lets each applied package replace
notification sounds, enable its interface sounds and set their interface
volume. The defaults (notifications on, interface sounds off, 50% interface
volume) need no `theme_sounds` entry; only differences are kept. The package's
built-in `theme-sounds` option still participates in its own resolved sound
levels. `theme_transitions` keeps a separate Transitions between channels
permission. When enabled in Full performance without Reduce motion,
the resolved theme fade or dissolve transition is drawn over the retained
previous chat surface. Iced 0.14 has no public live-window snapshot API, so
the transition covers the conversation rather than the whole window. Theme art layers are decoded once, with their blur, blend and dim baked
over the surface ground, and can cover, contain, stretch or tile the window, rail,
sidebar, account, chat, header, members, aside, profile pane, call, Settings,
menu, actions, embed, code, label, picker, toast, selected rows, cards,
dialogs, popouts and the lightbox. The chat's
ground, art and glass lie under the whole conversation (its bar and member
list included), and the bar is bare unless the theme gives it a ground, art
or glass of its own. Card-over layers stay above an unthemed
profile card's content, and Footer art keeps its declared height (64 px when
unspecified) at the member list's foot. Image overlays are drawn above their
declared window or chat scope. Tooltip art and glass draw over a tooltip's
ground; tooltip art plays its frames and drift.
Procedural scanline, LCD-grid, grain, vignette, rolling-band and flare overlays
use the same scope and performance rules. LCD `ghost` keeps the grid but not
the previous conversation image: capturing that image needs a renderer hook
iced 0.14 doesn't offer, and isn't among the vendored renderer's patches yet. Declared
drift runs at 60 Hz in Full and 30 Hz in Balanced (heavy layers rest in
Balanced), and rests in Light or Reduce motion. A sprite sheet that doesn't
drift asks for a frame only when its picture changes, grain once a period, and
ambient particles alone at most 30 times a second, so a theme at rest stays
near idle. Grain changes the whole window each step, which costs the CPU
renderer two whole-window draws, so it holds still there (as on Balanced) and
moves on the GPU. Ambient and
theme-applied, sent-message and reaction particles draw at their declared
depth; Light, Reduce motion and heavy-effect rules keep them still or omit
them. Glass surfaces use one reduced, preblurred copy of the resolved window
art and the package's ground, rim, highlight, gap and radius settings. This is
fake glass, not live backdrop blur. Each pane draws only
the piece of the blurred art under it, cut once and kept while the pane stays
put, rounded like the pane, with its ground, inset and highlight gradients and
rim above it. A glass `gap` floats the navigation and the conversation that far
apart and from the window's edge over the window's art; each is one rounded
pane whose surfaces round only the corners they share with it, and a glass account card floats `gap` inside the navigation's foot. The
art is rebaked after the window resizes so panes stay aligned to the current
window pixels.
Text-role shadow, glow, outline and emboss are extra glyph passes behind
message bodies, author/time text, headings, header text and member labels, so
links and cross-message selection keep the stock look. Appearance's Draw text
glow switch drops the glow's copies alone, which are the many of the four: the
shadow, the outline and the ink stay. Channel rows and cards draw
declared click ripples, hover gloss and selected highlighters. Foil bands
cross profile cards and member-grid tiles; they follow the pointer or sweep
on the art clock, and rest halfway across in Light or Reduce motion. Tiny highlighter and foil
rotations are left out with the other rotation styles.
Sidebar, member, chat and composer art reserve insets keep ordinary content
clear of decorations at pane edges.
`art.keys` puts a key under the composer's `plus`, `gif`, `sticker` and
`emoji` buttons, each `{ "image", "pressed", "size", "travel" }`: the key is
centred on its button at `size` logical pixels (half the picture's own when
left out, for a 2x picture), the button's hover wash gives way to it, and it
rises a pixel under the pointer unless the theme's hover is `none`. While held,
the `pressed` picture shows and the glyph moves down `travel` pixels (2 when
left out, at most 6); a key without a `pressed` picture moves down with it.
Bubblegum's candy keys are drawn this way, so they go down when pressed
rather than lying painted on the composer.
A surface's ground and art take the same round corner as the border round
it: a rim picture's own outer arc (measured from the picture, stretched with
it), the corners it shares with a rounded floating pane, or its own radius;
a picture that overhangs the surface is cut to its box with those corners.

Chrome label substitutions, prefix/suffix marks and time marks are applied to
ordinary navigation, timeline and composer labels (IRC times and authors
included). An installed top bar shows workspace buttons at its left, its centre
text in its middle and its right text at its right, in the code face. A theme's
box frames draw the sidebar, the conversation and the member list in
box-drawing lines with a title in the top edge (`[ #general ]`); its file trees
draw the server tree (root, `dm/`, servers and folders as directories after
`├──`/`└──`, each open place's categories and channels under it), channels
and members after their branch, and categories as directories (`▾ NAME/`).
Members take marks by their group (`@`, `+`) and a status column by presence;
`hide` leaves out member avatars, the account's avatar, or the bar's buttons
(shown while the pointer is on the bar). The status line runs under or over the
composer, looking at the clock every ten seconds, and the composer shows the
theme's prompt. Message cards hold a whole group: the
`message` shape's corners and border on the card's outer edges, its shadow,
and its pictures (a torn edge, a tab) straddling its top or bottom edge; the
group's rows run on with no gap and the theme's `cards.gap` lies between cards.
It leans by its group's seed within `cards.tilt`, its rows turning about one
point so they line up as one scrap, its ground, shadow and pictures turned and
its words straight; hover, highlighter and mention marks lie on the card. Seeds
are FNV-1a held in a 32-bit float, so frames, tapes and leans pick the same
way on every start. Tape painted on a card's top-edge picture
is cut out as it loads and lies across the torn edge, half on the board; a
surface's `tape…` picture at its top (card, popout, dialog) lies across its top
edge the same way, outside its clip. Ransom-note names cut authors, members,
profile cards and ransom-style category headings into letters on scraps, from
the theme's faces on its papers, each letter and its scrap turned by its seed;
a small i, l or o never takes a face that draws small letters as capitals.
Balanced and Light draw a name as one straight scrap in the heading face. A
role a theme doesn't dress takes its body face.
Bubbles hug their words up to the theme's share of the lane. Theme content is opt-in per
package in Themes and currently controls avatar shape/ring/decorations and
theme or palette name colours. Media recolouring and dithering apply to still
and animated avatars, icons, emoji, stickers, attachments and previews. Light
performance uses the dither palette as a gradient instead of running its pixel
pattern.

## Making a theme

A theme is a folder: `theme.json` and the files it names. Four commands work
on one with no window (`crates/yapfast-iced/src/theme_cli.rs`):

- `yapfast theme new NAME [--light] [--from THEME] [--author NAME] [--id ID]
  [--dir FOLDER] [--data-dir PATH]` makes `NAME`'s folder from `themes/template/`, or with
  `--from` from a built-in theme, one of the person's own or an installed
  one, checks it and says what to do next.
- `yapfast --theme-dev FOLDER` shows the theme in `FOLDER` over the theme
  chosen and shows it again each time a file in it is saved (looked at twice
  a second, two folders deep, off the window's thread). What fails the checks
  is logged and the look before stays, and the folder isn't checked again
  until the next save. Ctrl+Shift+T turns the dev theme off for the rest of
  the run.
- `yapfast theme check FOLDER [--marketplace]` prints the checks an install
  runs, and with `--marketplace` those for a listing, and on a pass the
  `sha256` a catalog entry pins. A theme gets into the marketplace with a pull
  request adding its entry to `catalog.json` in
  [yapfast/themes](https://github.com/yapfast/themes), whose README says how.
- `yapfast theme install FOLDER [--data-dir PATH]` installs a checked local
  folder into the store, as Add from URL does a repository's.
- `yapfast theme export CUSTOM-THEME FOLDER [--author NAME] [--data-dir PATH]`
  writes one of
  the person's own themes as a package, as the creator's Export does, only
  into a folder that's empty or not there yet.

On Windows the commands print into the PowerShell or cmd window they're run
from. Yapfast is a window program there, so the prompt may come back before
the output; piping it (`yapfast theme check .\mytheme | Out-Host`) waits for
it.

Themes don't change the mouse cursor: the system's cursor always shows. An
older theme's `cursors` layer (and its `cursors/` folder) is ignored, and
`theme check` notes it rather than failing, so it still installs; the cursor
files aren't copied, named or counted in the cost line.

## Custom themes

Create theme opens the guided creator on a copy of the theme shown. It
fills the settings frame: a bar with the name, Undo, Start over, Cancel and
Save; the steps; the controls scrolling beside a live preview of the window
(above them where the frame is narrow); Export and Delete.
Its Start, Colours, Shape, Fonts and Advanced steps derive the package through
`yapfast_core::themes::creator`: accent, light/dark or a chosen picture,
picture blur and dim, message layout, corners, density, outlines, glass and
built-in or imported body/heading fonts. Advanced lists every derived colour
by its token name, swatch and hex. Undo restores the preceding recipe and Start
over restores the recipe that opened. Colours offers up to five of the newest
pictures from Pictures and its Wallpapers folders beside the file chooser.
Every draft is resolved through the same
temporary package registry as installed themes, so colours, shapes, fonts,
picture art and glass preview across the iced window immediately without
becoming a saved theme.

Save writes the creator recipe and package folder, keeps the custom theme and
chooses it. Export writes a shareable package folder including its manifest,
licence, picture and fonts. The person remains responsible for permission to
share pictures and imported fonts. Cancel, leaving Themes or closing settings
shows the theme from before and removes the draft. The separate Colour editor
keeps the earlier palette-only workflow: each colour's swatch opens a
`ColourPad` card under it (keeping any alpha on `#rrggbbaa` tokens), and its
hex field accepts `#rgb`, `#rrggbb` or `#rrggbbaa` and marks anything else. A saved theme's card has a
pencil that opens it again to rename, change or delete it; deleting the chosen
one shows Yap. Closing the window to the
[tray](tray.md) closes Settings, so it drops the unsaved edit too.

`layout.json` keeps them in `custom_themes`, in the order Appearance shows
them after the built-in ones and Themes shows them under Your themes:

```json
{
  "id": "custom:1",
  "name": "Plum",
  "base": "yap",
  "dark": true,
  "colours": { "rail": "#111215", "chat": "#2a1a3a", "row-hover": "#1a1b20" },
  "creator": { "accent": 16760647, "messages": "cards", "corners": 80 },
  "folder": "themes/creator/1"
}
```

Besides the colours every theme has, a custom theme can colour two parts
of the window on its own, by the names a theme package gives them: `pill`,
the pills beside the rail's tiles (open, pointed at, unread), which a
package sets as its `pill` colour, and `header`, the ground of the
conversation's header row, which a package sets as `shape.header.ground`.
Each follows a colour of the theme until it's set, as a package's extra
colours follow theirs: the pill the text, the header the chat, whose ground
then shows under the header as it does for every built-in theme. The colour
editor lists them after the other colours; while one isn't set, its field
shows the colour it follows and moves with it, so a new chat moves the
header and a new text the pills. `layout.json` keeps one only once it's
set, so a theme saved before they existed, or one that never set them,
draws as before. Exported (`yapfast theme export`), and in a theme's
folder, they're written by those package names, over any the folder gives.

`base` is the built-in theme the first copy came from (a copy of a custom
theme takes its base). Yapfast writes every colour, but a colour that's missing,
such as one added in a later version, or that isn't a colour, takes the
base's value, and a name Yapfast doesn't know is ignored, so themes survive new
colours. Guided themes additionally keep their `creator` recipe and a relative
`folder` containing their package assets. Ids are `custom:` and a number, and no built-in id has a colon, so
they can't collide; on reading, Yapfast gives a fresh one to any custom theme
whose id isn't of that form or repeats another's. The editor's temporary fields
exist only while it's open.

At startup, a missing folder under Yapfast's exact managed
`themes/creator/<id>` path can be regenerated from its saved creator recipe
when it uses only reproducible built-in assets. Arbitrary custom folder paths,
and recipes whose imported font or picture bytes are gone, are never rewritten.

| Theme | Id | Dark | Accent | From |
| --- | --- | --- | --- | --- |
| Yap | `yap` | yes | marigold | Yapfast's own |
| Bonfire | `bonfire` | yes | blurple | Discord's Dark theme, the default |
| Catppuccin Mocha | `catppuccin-mocha` | yes | mauve | [Catppuccin](https://github.com/catppuccin/catppuccin) |
| Dracula | `dracula` | yes | purple | [Dracula](https://draculatheme.com/contribute) |
| Nord | `nord` | yes | frost nord8 | [Nord](https://www.nordtheme.com/docs/colors-and-palettes) |
| Gruvbox Dark | `gruvbox-dark` | yes | orange | [Gruvbox](https://github.com/morhetz/gruvbox) |
| Tokyo Night | `tokyo-night` | yes | blue | [Tokyo Night](https://github.com/tokyo-night/tokyo-night-vscode-theme), Night style |
| Catppuccin Latte | `catppuccin-latte` | no | mauve | [Catppuccin](https://github.com/catppuccin/catppuccin) |
| Rosé Pine Dawn | `rose-pine-dawn` | no | rose | [Rosé Pine](https://rosepinetheme.com/palette/) |
| Sakura | `sakura` | no | blossom pink | Yapfast's own |

The palettes of Catppuccin, Dracula, Nord, Gruvbox, Tokyo Night and Rosé Pine
are published under the MIT licence; thanks to their authors. For Tokyo
Night, the steps between its grounds and text (bg_highlight, fg_gutter, dark3,
dark5) are named as folke's tokyonight.nvim names them. Yapfast takes their
colours, not their names for UI parts, so each mapping is Yapfast's:

- The ground steps go rail, sidebar, chat from the deepest to the one text
  sits on, with raised surfaces (popovers, reactions, stage tiles) and the
  composer one step off the chat. Where a palette has no shade below its
  background (Nord's nord0, Tokyo Night's bg_dark), the rail, panel and edge
  darken it, and hover and some row grounds are halfway mixes of two palette
  colours.
- Text, secondary, muted and faint take the palette's text ramp; faint keeps
  at least 2.5:1 against the sidebar, near Yap's 2.8:1.
- Content on the accent and on red (on-accent, on-danger) takes the palette's
  darkest or lightest colour, whichever reads; on pastel accents, as in
  Catppuccin and Tokyo Night, that's dark.
- Light themes wash hovers with a dark overlay (black, or the palette's text
  colour) instead of white, sweep skeletons with a white band, and draw the
  tooltip light, since its text is the theme's text colour.

Bonfire maps Discord's Dark theme from the CSS variables of its web client
(`.theme-dark` with `.theme-darker`, which is what its settings call Dark;
`.theme-dark` alone is Ash, `.theme-midnight` Onyx), read from the public
bundle: the rail and sidebar are `--background-base-lowest` (#121214), the chat
`--background-base-lower` (#1a1a1e), the account panel and composer
`--background-base-low` (#202024), raised surfaces and the tooltip
`--background-surface-high` (#242429), text `--text-default`, muted
`--text-muted`, links `--text-link`, the accent
`--control-primary-background-default` (blurple, #5865f2, hover #4452bb, white
on it), red `--control-critical-primary-background-default` (#d22d39), live
`--status-positive`, and hovers, selections, lines and secondary buttons the
`--interactive-background-*`, `--border-subtle` and
`--control-secondary-background-*` washes laid over their grounds. It carries
no Discord name or logo.

A few things that Discord's client draws off its palette, Bonfire draws in
the inks measured from it (`Theme::bonfire` in the core, `bonfire` on the
kit's theme): a mention's text, the channel list's names and glyphs, a
message's time, the header's search box and the Jump to present pill. A
custom theme made from Bonfire keeps them while its link colour is
Bonfire's; every other theme draws those from its own colours.

Discord role colours stay as servers set them, and some read faintly on a
light theme; Yapfast doesn't adjust them yet.

## Sharing themes

Import, beside New theme, opens a file picker and keeps the theme in the file
among the person's own, then chooses it, as Save does. Export, in the editor
beside Save and Cancel, saves the theme as the editor shows it to a file, so
a new theme can be shared before it is saved, and a built-in theme through a
copy of it. Copy, beside Export, puts the same text on the clipboard, and
Paste, beside Import, takes it from there as Import takes a file, so a theme
can be shared in a chat message. A line under the title says how each went,
or why a file wasn't taken.

This is the colours alone. A theme from the creator, with its pictures, fonts
and shapes, is shared as a package instead (its Export, or `yapfast theme
export`). The editor owns the window's colours while it is open, so a file
picked meanwhile (the picker can outlast opening the editor) is turned away
with a note.

A theme file is JSON, the format `layout.json` already uses for a custom
theme, so there is nothing new to learn or parse, and people can edit one in
any text editor. It is named after the theme, `Plum.yapfast-theme.json`; the
picker lists every `.json` file, so a renamed file loads as well.

```json
{
  "format": "yapfast-theme",
  "version": 1,
  "name": "Plum",
  "base": "yap",
  "dark": true,
  "colours": { "rail": "#111215", "chat": "#2a1a3a", "row-hover": "#1a1b20" }
}
```

It differs from a custom theme in `layout.json` in two ways. `format` and
`version` come first: a file without `"format": "yapfast-theme"` is turned
away, so that no other JSON, `layout.json`'s own entries included, loads as
a theme by accident, and one with a higher `version` than the app knows is
turned away with a note to update. `version` grows only for a change an older
Yapfast would read wrongly; a colour added later doesn't need it, as a missing
colour takes the base's. And there is no `id`: Yapfast gives the imported
theme the next free `custom:` id, so importing a file twice makes two themes
and a shared file never collides with the reader's.

Reading follows the same rules as `layout.json`. A colour that is missing or
isn't a colour takes the base's value, and a name Yapfast doesn't know is
ignored, so a file from a newer or older version loads; the note after the
import counts the colours that weren't colours. A `pill` or `header` the
file names is the theme's own part, as in `layout.json`; one it doesn't name
follows the theme's colour. A file with no colour name
Yapfast knows isn't a theme, and is turned away. The name loses control
characters and stops at 48 characters; a file without one imports as
"Imported theme". A file over 64 KiB (a theme is about 2 KiB) isn't read. A
theme's colours are the only data in it, and nothing in a file is run or
fetched.

## Fonts

Settings → Appearance has a FONT section for drawing all of Yapfast in the
person's own font. Choose font files opens the system's file dialog, which
takes several files at once; the list shows each file by name with the faces
it gave ("Regular, Bold Italic") and a Remove button. Choosing again adds to
the files already loaded, and Reset to default removes them all and goes back
to DM Sans. The monospace code font and the emoji font are not changed.

The window draws every text in the family named DM Sans, so the files' faces
are put in the font database under that name, not the names the files carry
(the weights of one typeface often have a family name each), and DM Sans's own
faces leave it while they're in. cosmic-text then picks the face for a bold or
italic request through fontdb's query, from the weight and style each file
declares in its own tables, so a family given as one file per weight and a
variable font both work; a variable font is one face, drawn at each weight
the text asks for, as DM Sans is. A weight the files don't have takes the
nearest one they do, and without an italic file emphasis draws upright; neither
is synthesised.

While the files are loaded they also stand in for the body face an installed
theme brings: its colours and other faces stay, and Reset to default
gives its body face back.

The formats are TrueType and OpenType (TTF, OTF), collections (TTC, which
register all of their faces) and WOFF. WOFF2 isn't supported, and says so in
the list. A file is told by its first four bytes, not its extension. A file
that isn't a font, or gives no face, is refused with a short reason and isn't
kept. TTF, OTF and TTC files are memory-mapped by fontdb, not read, so loading
costs no heap and the operating system pages in the glyphs that draw. A WOFF
file's tables are compressed, so it is unwrapped into a TrueType or OpenType
font in memory (`crates/yapfast-iced-kit/src/woff.rs`) and the compressed file
is released; that is the only copy a font makes. At most 64 files load.

`layout.json` keeps the files in `ui_font_files`, as absolute paths in the
order they were added. They are loaded at startup, before the window opens. A
path that is missing or isn't a usable font is skipped with a note under the
list, the rest are used, and when none loads DM Sans stays. A missing file
stays saved, in case its drive comes back; a file refused when chosen is not
saved.

Changing the files takes the earlier set's faces out of the font database and
puts the new set in, then moves the font system's version so iced shapes all
its text again (`fonts::shape_again`); the timeline's rows are measured again,
as when the message text size changes. The text engine's cached fonts of the
faces taken out are let go then too (`forget_removed_fonts`, patch 4 in
`vendor/cosmic-text/README.yapfast.md`), with the files' mappings and an
unwrapped WOFF's memory.

Known limitation: the text keeps the sizes the window's design gives it, so a
font with large or small proportions reads larger or smaller than DM Sans
does, and the emoji font's vertical metrics are tuned to DM Sans, so emoji can
sit a little high or low against the text of a font with other proportions.
