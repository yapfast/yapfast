# Import accounts from the Discord desktop app

Windows builds offer **Import accounts from the Discord app** on the sign-in
screen and under **Manage Accounts > Add Account**. Import can be retried after
an error. QR login and pasted tokens remain available. After an import failure,
the QR action says **Show the code**; after a QR failure, it says **Try again**.

For a newly linked account, import reuses a saved Discord session. It does not
create an independent login: logging that session out in Discord can invalidate
it in Yapfast too. An already linked account keeps its existing session, including
an independent QR login; import updates only its name and username. The
button reads only the current Windows user's Stable, PTB and Canary profiles
under `%APPDATA%`. Browser profiles and other OS users are outside its scope.

## Storage and validation

`yapfast-discord/src/import.rs` streams the files needed for LevelDB recovery into
memory and opens that copy with `rusty-leveldb`. The source files are only read.
The reader handles compressed tables and fragmented write-ahead logs. Values
returned by replay are searched for Discord's encrypted `v10`
sessions; Windows DPAPI unwraps the profile key and AES-GCM authenticates each
candidate. Unsupported encryption schemes are ignored.

Recovery is best effort. In `rusty-leveldb` 4.0.1, a bad log record silently stops
replay of that log, so later updates or deletions may be absent from the result.
The importer accepts this limitation; it cannot count an unreported truncated
replay as an unreadable profile. API validation establishes whether a candidate
session is usable, not whether every current saved account was discovered.

Reads compare file sizes and modification times, including the directory's file
list before and after the copy. A file changing or disappearing during copying
restarts the copy, with up to three attempts and backoffs of 25 and 50 ms. Local
State reads also retry changes. A profile that still cannot be copied can fail
without discarding candidates from another readable profile. Only a profile
that keeps changing suggests closing Discord and retrying. Missing Local State,
key access, file access and size-limit failures have their own explanations and
offer QR login. A worker failure asks the user to retry the import.

Limits per profile are 128 storage files, 8 MiB per file and 16 MiB total.
Files stream into the memory copy without a second full-file read buffer.
These limits bound copied source bytes; replay and caches need additional memory.
Local State is limited to 2 MiB. At most 64 distinct session candidates are
returned across the profiles, with at most 2,048 encoded bytes per candidate.
Filesystem, DPAPI and credential-store work run on the blocking pool, outside
the network workers.
Candidate debug formatting redacts the token.

`yapfast-core/src/discord_link/importing.rs` validates candidates before storing
them, with up to four validations in flight. Results are saved in discovery
order, regardless of response timing; duplicate user IDs keep the first usable
session in that order. The web client's build number is
learned once before the batch. Each request to `/users/@me` has a 30-second
deadline. Deduplication uses the authenticated user ID.
Existing unrelated saved accounts cannot turn a failed import into a success.
The result reports added accounts, already linked accounts and partial failures.
Only HTTP 401 counts as a signed-out session. Network failures, timeouts, other
HTTP statuses, local client construction and credential-store failures retain
their own causes and retry guidance. Discovery, validation and save failures log their error chains without
logging candidate tokens. On first sign-in, the summary remains visible while
the imported account connects.

Import preserves an existing active account and its cached profile picture.
The first usable account in discovery order becomes active when none was linked.
A connected client refreshes its account list without reconnecting or clearing the current
conversation.

Cancellation stops validation and prevents further saves. A credential save
already in progress gets a ten-second grace period after the cancellation
request. If it still has not returned, the screen reports **Import cancelled.
A save may still finish.** and allows linking again. The blocking save keeps
running; it cannot be forcibly stopped. The UI avoids credential-store reads
while any import save is pending, including after the deadline. When the save
finishes, the connected account list refreshes, even if another linking flow
has started.

Accounts saved before the request or by an in-flight save remain linked.
Cancelling a first import stays on the sign-in screen, reports confirmed saved
accounts and offers **Continue with saved account** once the store is available.
That account is also available to restore on the next launch. Cancellation and
success notices use neutral text; error notices use the error colour. Closing
the application can abort reporting while a blocking save still runs.
