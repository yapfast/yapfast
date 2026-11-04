# Caching every favourite GIF

Goal: every one of the account's favourite GIFs has its thumbnail on disk, so the
picker's Favourites view shows them all at once — offline too — instead of
filling in as its tiles are drawn. The fetching happens in the background, off
the thread that draws, and holds nothing in memory.

Out of scope: trending and search results (they follow the user and rotate), the
picker's look and masonry, and anything about emoji. This branch is off `main`:
the emoji branch's prefetch shares no callable code with this (different
command, transport, cache directory and rate-limit regime), only the shape.

## What exists today

- **The list** (`chat::FavoriteGif { url, src, video, width, height, order }`):
  Discord's `FrecencyUserSettings.favorite_gifs`, read from
  `discord-favorite-gifs.json` at start (`crates/yapfast-discord/src/gifs.rs`,
  `cache.rs`) before Discord answers, and replaced when Discord's settings
  change. So the whole list is known at start, without an API call.
- **The cache** (`cache.rs`): `discord-gifs/` holds a thumbnail per favourite,
  named by the hash of its picture's address — "a thumbnail of each favourite
  GIF *shown*" — under `GIFS_SIZE = 50 MB`; a thumbnail is "14-300 KB, 40 KB at
  the median", so about 1250 fit. The least lately shown go first, and
  `kept_gif` touches a file when it is read so what is looked at stays.
  `keep_gif(url, KeptGif::File | KeptGif::Bytes)`, `kept_gif(url)`.
- **The fetches**: the picker asks for a tile's picture through
  `DiscordCommand::GifPicture { url, width, height, favorite }`, and only a
  favourite's is kept (`application/gifs.rs::gifs_shown`, `src/ui/gifs.rs::
  ask_gif_pictures`). That is why favourites cache in the first place — but only
  once their tile is drawn, which is what this change removes.
- **The pictures are not Discord's**: Klipy and Tenor (`media.tenor.com`,
  `static.klipy.com`), so no Discord API or CDN rate limit; the favourites list
  itself comes from the settings call Yapfast already makes.
- **The fake** (`testing/fake_discord.ts`) starts Alice with two favourite GIFs
  and serves `/_gifs/…`, so the cold and warm paths are testable end to end.

## Design

**1. One command: `DiscordCommand::PrefetchFavorites`.**
Carrying the thumbnails to fetch, `(url, width, height)` for each favourite
whose `kept_gif(url)` is absent. Handled on the Discord worker: four at a time
(these are bigger than emoji pictures, and the user is not waiting on them),
with a pause between batches and the fetcher's backoff for a failure. Nothing
is decoded, nothing is sent to the UI, nothing is held — the bytes go to
`discord-gifs/` and are dropped.

**2. When it runs.**
- On start, once the favourites list is known (from the disk cache: no API
  call, so this can happen early).
- Again when Discord's settings replace the list, for the entries that are new.
- Never twice for the same URL: `kept_gif` is the check, and an in-flight set
  covers what the first pass has not finished.

**3. What size a thumbnail is fetched at.**
The tile a favourite shows in is `masonry`'s width for the picker's width: about
200 px wide, its height from the favourite's own aspect. Prefetch at the
picker's default width (`Masonry::new(474.0)`), and let a wider picker ask again
for the ones on screen — `AskedPicture::outgrown` already decides that — with
`keep_gif` overwriting the file. One size keeps the cache from filling with the
same GIF at five widths.

**4. The budget must fit the list.**
`GIFS_SIZE = 50 MB` sounds generous against the code's "40 KB at the median",
but a real cache measured on this machine held **114 favourite thumbnails in
37.2 MB — 334 KB each**, so 50 MB is about 150 of them and prefetching the list
would start evicting favourites. Size the budget by the list, not by a constant. Make the budget follow
the list rather than a constant: keep every favourite, and prune only *beyond*
the list — i.e. a thumbnail whose favourite is gone (unfavourited) goes, and the
size cap becomes a backstop that, if it is ever hit, evicts the oldest `order`
favourites first and says so in a log line. Also worth measuring first: if the
account's favourites are 50, this is moot; the design should not assume that.

**5. Memory stays where it is.**
`gifs_shown` keeps the pictures of the tiles on screen, `close` clears them; the
prefetch touches neither. A thumbnail is decoded when its tile is drawn, from
the disk.

## Work items

1. `crates/yapfast-discord/src/cache.rs`: `favorite_gif_thumbnails()` — the
   cache's view of the list: which of `src` are kept, their sizes and bytes —
   and a prune that removes thumbnails whose favourite is no longer in the list.
   Tests with a temp cache.
2. `crates/yapfast-discord/src/gifs.rs`: the thumbnail fetch that checks
   `kept_gif`, downloads at the given size, and keeps it (`keep_gif`), with no
   decode and no event. `DiscordCommand::PrefetchFavorites` in
   `crates/yapfast-core/src/chat.rs`, handled in `discord_text.rs` beside
   `GifPicture`.
3. `crates/yapfast-core/src/application/gifs.rs`: `prefetch_favorites()` — from
   `state.favorite_gifs`, the `(src, width, height)` of those not kept, and the
   effect. Called when the list arrives or changes, and once at start.
4. `src/ui/gifs.rs`: nothing changes for drawing; a tile whose thumbnail is kept
   already shows without a request. Check that a prefetched size is the size a
   tile asks for, so it is not fetched again on open.
5. Docs: `docs/chat-ui.md` and `docs/discord/messages.md#gifs` say the
   favourites are kept and fetched ahead; `docs/building.md` if the cache's
   pruning rule changes.

Acceptance: with a cold `discord-gifs/`, the Favourites view is complete the
first time it opens, and every favourite's file is there after a start; with a
warm one, a start fetches nothing; unfavouriting removes the file at the next
prune; memory is only the tiles on screen.

## Measurements to record

How many favourites an account has and what they take (the fake's two, then a
real list), the time a cold prefetch takes (8 at a time, ~40 KB a thumbnail),
and the first open of Favourites cold versus warm. The emoji numbers to compare
with: 4805 pictures, 9 s, 31 MB, 0.043 ms a read.

## Risks

- **The budget.** Measured: 334 KB a thumbnail on this machine, 114 of them
  already 37 MB — so 50 MB covers ~150 and the cap must grow or follow the list.
  Log when the backstop bites rather than silently dropping a favourite.
- **Video favourites.** `FavoriteGif.video` means the picture is a video; the
  thumbnail is a frame of it, which the existing fetch already produces for a
  tile — check the prefetch path asks for the same thing, and that a video
  favourite is not left out.
- **Provider terms.** Keeping a favourite's thumbnail is what Yapfast already
  does; prefetching the whole list widens it from "what was shown" to "what was
  favourited". Both are the user's own choices, and the cache is local and
  bounded, but it is worth a line in the docs.
- **Unfavourited files.** Without the prune above, `discord-gifs/` would keep
  thumbnails for GIFs no longer favourited until the size cap evicts them.
