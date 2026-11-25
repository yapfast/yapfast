# Yapfast stream group fix

This is davey 0.1.4 from crates.io under its original MIT license
(LICENSE, as the crate's manifest declares; the published crate ships no
licence file).

davey names a session's MLS group after the voice channel it's given, and
the only way to change it, `reinit`, clears the session's storage. A Go Live
stream's group isn't its voice channel's, and Yapfast learns the real id only
from the voice server's first proposals or commit. By then it has sent a key
package, and clearing storage throws away that package's private keys, so the
welcome built from it fails with `NoMatchingKeyPackage`.

Changes, marked `Yapfast:` in the source:

- `DaveSession::set_channel_id` changes the group id and keeps the signing key
  and stored key packages. A pending group starts again under the new id; a
  joined group is left as it is.
- `read_leb128` stops at ten bytes rather than shifting past a `u64`, which
  panics in a debug build when a received frame's nonce runs that long.

This build pins OpenMLS to the revision that uses HPKE 0.8 and P-256 0.14.
`DaveSession::process_commit` handles the resulting `OwnPendingCommit` as a
processed message, and the media ratchet reads the bytes of the exported secret.
