# Adding a Discord feature

A Discord feature runs through every layer, from the HTTP call to the
Slint window. Take them in this order, and build and test each layer that
has tests before the next. "List the people you blocked"
(5ebf7bc) and "Leave a group DM" (a5d1904) are small commits that show the
whole path.

1. **Find what Discord does.** The endpoint, payload and gateway event are
   on https://docs.discord.food. What its client shows and sends around
   them (which menu lists it, defaults, dialogs, query flags such as
   `silent`) is only in Discord's web bundle; see the area doc for what
   Yapfast already matches.
2. **The call and the state, in `crates/yapfast-discord`.** The request
   goes in `src/api.rs`, wrapped by a method on `Discord` in
   `src/service.rs`. Anything the UI shows from the gateway is a field of
   `Snapshot` (`src/service.rs`), filled by the tracker
   (`src/service/tracker.rs`), with a test in the tracker's `mod tests`.
3. **The fake.** Give `testing/fake_discord.ts` the route, in Discord's
   shape, and the gateway event it dispatches. Record what the client sent
   in a list that a `GET /_control/...` route returns, and describe both in
   the header comment at the top of the file. Then test it in
   `crates/yapfast-discord/tests/fake_server.rs`, in the test of its area,
   and run that test alone, as
   `scripts/test-fake-discord makes_group_dms`.
4. **The command and its answer, in `crates/yapfast-core/src/chat.rs`.**
   Add a `DiscordCommand` variant, and an `Event` that says whether it
   worked. `discord_text/commands.rs` passes a command through each module's
   `command` in turn; handle it in the module of its area
   (`discord_text/account.rs`, `people.rs`, `server_settings.rs`...), which
   spawns the call and sends the event.
5. **The app, in `crates/yapfast-core/src/application`.** An `Action` (in
   `application.rs`) for what the person does, state in `AppState` for a
   dialog or list, and the handling of the event, in the module of its area.
   A snapshot field reaches `AppState` in `discord_account.rs`. Test it in
   `application/tests.rs`, which drives `Application` with snapshots and
   actions.
6. **A menu item, if it's one.** A new `MenuChoice` needs six places in
   `src/ui/menus.rs`: its group for separators, red if it destroys, its
   label and icon, the action in `choose`, `CHOICES` (and its length), and
   `choice_id`. The variant and which menus offer it are in the core's
   `application/rooms.rs`, tested by
   `menus_offer_what_fits_their_conversation_or_server` in
   `application/tests.rs`.
7. **The window.** A component in `ui/*.slint`, a property or callback on
   `YapfastWindow` in `ui/yapfast.slint` passed down to it, its rows filled
   in `src/ui/refresh.rs` (a list model goes in `Models` in `src/ui.rs`;
   a sidebar list joins `SidebarShown::lists`, or its rows never refresh),
   and its callback bound to the action in `src/ui/callbacks.rs`. Before
   adding a field to a Slint struct, grep `StructName {` in `src/`: every
   literal of it needs the field. `docs/architecture.md` has the path in
   full; `scripts/ui-session` shows it running against the fake.
8. **The docs, in the same commit.** The section of the area doc
   (`docs/discord/*.md`), the status table in [README.md](README.md), and
   the root README's feature list, taking it out of "Not there yet" if it
   was there.

Something the fake's built-in people don't show, such as a blocked person,
can be seeded with `--env` and a file typed by `testing/fake_discord_env.ts`.
