# Matrix sessions

Yapfast supports password sign-in, restoration, recovery-key import and logout
through the CLI and native desktop. Launch `cargo run --locked` to sign in or
restore an account, or add `-- --preview` to try the layout locally first. The
desktop exposes masked password and recovery-key fields and a device-trust
check. Real text sending requires a verified device and an encrypted joined
room. SSO, OAuth/MAS and emoji verification are not implemented. A homeserver
without password sign-in receives an actionable error.

Run these commands inside the development shell. This example uses fish's hidden
prompt so the password stays out of shell history and process arguments:

```fish
read --silent --prompt-str 'Matrix password: ' yapfast_password
printf '%s\n' "$yapfast_password" | cargo run --locked --no-default-features --features matrix,discord -- session login --homeserver http://localhost:18008 --user alice --password-stdin --data-dir .local/manual-session
set --erase yapfast_password
cargo run --locked --no-default-features --features matrix,discord -- session status --data-dir .local/manual-session
```

The local `alice` account is provisioned by `scripts/test-stack check`. For an
existing account, use its actual HTTPS homeserver and username. HTTP is accepted
only on loopback. Homeserver URLs containing credentials are rejected.

Import an existing recovery key with the same hidden-prompt pattern:

```fish
read --silent --prompt-str 'Recovery key: ' yapfast_recovery_key
printf '%s\n' "$yapfast_recovery_key" | cargo run --locked --no-default-features --features matrix,discord -- session recover --key-stdin --data-dir .local/manual-session
set --erase yapfast_recovery_key
```

The CLI reads one secret line from stdin and refuses a terminal as input.
It does not print passwords, tokens, store passwords or recovery keys. Status
prints the account ID, device ID and current cross-signing trust result.

## Storage and lifecycle

Without `--data-dir`, Yapfast uses the platform's local application data directory.
Each directory has one active session and an exclusive process lock. Since
that lock keeps the stores to one process, the SDK's own cross-process store
lock is off; it would rewrite a lease in the crypto store every 50 ms. Corrupt
metadata, missing credentials and a missing crypto store stop restoration. They
never silently create a replacement device or reset its identity.

The OS credential store holds the access token and a randomly generated SDK
store password. Linux uses Secret Service; Windows uses Credential Manager.
When Secret Service is unavailable at first sign-in, Linux uses private files
and prints that storage notice. Directory permissions are 0700 and credential
files are 0600. The root and newly prepared store directories reject broad
permissions. Final-path symbolic links are rejected for the root, metadata,
credential files and newly prepared store directories. A saved keyring session
remains bound to that backend if it later
becomes unavailable. Windows has no plaintext-file fallback.

The pinned SDK receives the same password for its state, crypto, event-cache and
media SQLite stores. Its store code encrypts private values, including cached
events. This does not hide filesystem names, SQLite structure or all public
metadata. The fallback password is protected by file permissions, so someone
with access as the same Linux user can also read it.

Discord's token lives in the same credential store, with no file fallback,
and so does the key of what Yapfast keeps to show while it connects: the
account, recent messages, profiles, favourite GIFs and the previews of
attachments and links, each file encrypted with XChaCha20-Poly1305
(`crates/yapfast-discord/src/cache/sealed.rs`). The names of the messages'
and profiles' files carry channel and user ids. Avatars, emoji and other
pictures public on Discord's CDN are kept plain, as is the video playing,
deleted once it stops. Unlinking deletes the files and the key. With several
Discord accounts linked, each has its own entry in the credential store, and
the files belong to the account in use: switching accounts empties them for the
next, as another login's files read as empty.

Writes use a private temporary file, flush its contents and replace the prior
file. Store passwords are preserved separately from active login credentials.
Logout revokes the server session before clearing its local credentials; failed
revocation leaves them available for retry. It retains encrypted SDK stores and
their passwords, including unsent local data. This retention is deliberate;
there is no user-facing removal flow yet.

```sh
cargo run --locked --no-default-features --features matrix,discord -- session logout --data-dir .local/manual-session
```

The password flow does not request refresh tokens. Refresh-token persistence
and OAuth sessions are not implemented. New accounts may bootstrap
cross-signing through the SDK, which leaves an existing identity intact.
Recovery-key setup is exercised only on disposable experiment accounts; the CLI
imports an existing key. Room keys use the SDK's identity-based sharing policy.

The call experiment (`--call-experiment`) checks actual OS-store restoration,
message-key persistence and recovery against the local server. Unit tests
separately cover the Linux fallback's permissions, missing credentials,
metadata damage, retained store passwords and exclusive locking. Matrix storage
on Windows remains untested.
