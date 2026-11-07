# Privacy

RatFast is an unofficial third-party Discord client. It authenticates with your
account token, which Discord's Terms of Service prohibit; using it can get the
account limited or terminated. Nothing on this page changes that.

## What RatFast stores locally

| Path (under the data folder) | Contents | Protection |
| --- | --- | --- |
| OS keyring, service `io.github.yapfast.Yapfast` | Discord token, Matrix session, cache key | The OS credential store |
| `discord-account.json` | Servers, channels, members, settings | XChaCha20-Poly1305, keyed per token |
| `discord-messages/*.json` | Loaded timelines | Same, sealed |
| `discord-previews/*` | Decoded picture previews | Same, sealed |
| `discord-pictures/*` | Public CDN pictures (avatars, emoji, stickers) | Plain files, hash-named |
| `discord-cookies.json` | `__dcfduid`, `__cf_bm` device cookies | Plain file, `0600` on Unix |
| `layout.json` | Window, theme, appearance, plugins | Plain JSON |
| `yapfast.log` | Diagnostics, `0600`, up to 4 MiB | Plain, truncated at the cap |

The data folder is `%LOCALAPPDATA%\Yapfast\Yapfast\data` on Windows, or
whatever `--data-dir` names. On Windows those files inherit the profile ACL;
on Unix they are created owner-only.

## What the logs contain

`YAPFAST_LOG` filters what is written; the default is `INFO` for RatFast's own
crates and nothing from dependencies.

Gateway payloads are no longer logged whole. Voice and stream gateway logs
record the opcode and the payload size only, so a debug log no longer carries
session descriptions, early DMs or endpoint IPs. Message bodies, tokens and
passwords are not logged at any level.

## What RatFast does not do

- No analytics, telemetry, crash reporting or advertising SDK.
- No unsolicited uploads of logs or screenshots.
- No background connections beyond Discord itself: the gateway, the remote-auth
  gateway, the CDN and media allowlist, and the GIF providers you open.
- No third-party plugin code today; see [plugins](plugins.md).

## Network endpoints

`https://discord.com/api/v9`, `wss://gateway.discord.gg`,
`wss://remote-auth-gateway.discord.gg`, the Discord CDN and media proxy, and —
only when you use those features — Tenor, Giphy, Klipy, Spotify, YouTube and
Twitch thumbnail hosts. Tokens are only sent to Discord; gateways refuse
`ws://` everywhere except loopback.

## What you control

- **Notifications** can hide message text, so the lock screen shows only that
  something arrived.
- **Game activity** (Rich Presence) is off unless you turn it on, and names the
  games RatFast detects.
- **Rich presence over Discord's RPC socket** only exposes the activity you
  choose.
- **Deleting data**: signing out revokes the session, clears the keyring entry
  and removes the sealed cache. Deleting the data folder removes everything
  else, including themes and plugin settings.

## Limits, stated plainly

- Your account identity, IP address, gateway metadata and activity are visible
  to Discord. A native client does not hide them.
- The cache key is derived from the token, so two backups of the same account
  share a header value. Someone with both files can tell they are the same
  account without decrypting either.
- Public CDN pictures are stored unsealed. A backup reader learns which avatars
  and emoji you have seen from the hashes, though not their names.
- The keyring is the trust boundary. If another process running as you can read
  it, it can read the session.
- Windows keeps the profile ACL, not per-file modes; a data folder on shared
  storage is readable by whoever that storage trusts.
