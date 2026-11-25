# Yapfast dependency patch

This is davey 0.2.0 from crates.io under the
original MIT license. The published release uses OpenMLS 0.8 and P-256 0.13.
Yapfast's patch pins OpenMLS to the revision using HPKE 0.8 and updates
P-256 to 0.14 so Cargo resolves fixed crypto dependencies. Keep the
OpenMLS revision in `crates/yapfast-discord/Cargo.toml` in sync for the tests.

The patch adapts signing key generation, commit processing and MLS secret
export to those versions. It also removes traces containing media key material.
The local crate only exposes the Rust API used by Yapfast. It omits the
Node.js and Python binding hooks and the unused numeric proposal conversion.
Remove it when a compatible davey release includes the dependency updates.
