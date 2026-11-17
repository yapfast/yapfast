//! Checks a release's `SHA256SUMS.txt` against the minisign signature
//! published beside it, `SHA256SUMS.txt.minisig`, before any of its sums is
//! trusted.
//!
//! Two kinds of key sign releases, and each is accepted only for its own kind:
//! the stable keys, kept off GitHub on the maintainer's machine, for every
//! version that isn't a nightly; the nightly key, which the Nightly workflow
//! holds, for nightlies only. Someone with the nightly key can reach only the
//! people who chose nightlies. The signature's trusted comment names the
//! release's tag, so a signed file can't be passed off as another release's,
//! an older one say. See docs/packaging.md.

use minisign_verify::{PublicKey, Signature};

use super::Release;
use yapfast_core::i18n::tr;

/// The signature's name on a release.
pub const FILE: &str = "SHA256SUMS.txt.minisig";

/// The largest signature read; minisign's are about 300 bytes.
pub const LIMIT: u64 = 4 << 10;

/// The keys of the two kinds of release, as `minisign -G` writes them.
pub struct Keys {
    /// For stable releases and every prerelease but a nightly.
    pub stable: &'static [&'static str],
    pub nightly: &'static [&'static str],
}

/// The keys built in. The stable list has the key in use and a backup kept
/// apart from it, for when the first is lost.
#[cfg(not(test))]
pub const KEYS: Keys = Keys {
    stable: &[
        include_str!("../../packaging/keys/stable.pub"),
        include_str!("../../packaging/keys/stable-backup.pub"),
    ],
    nightly: &[include_str!("../../packaging/keys/nightly.pub")],
};

/// The tests' own keys, which `testdata/sign` signs with.
#[cfg(test)]
pub const KEYS: Keys = Keys {
    stable: &[include_str!("testdata/stable.pub")],
    nightly: &[include_str!("testdata/nightly.pub")],
};

/// The trusted comment a signature of `tag`'s sums carries.
pub fn comment(tag: &str) -> String {
    format!("yapfast {tag} SHA256SUMS.txt")
}

/// Whether `signature` is one of `keys` for `release`'s kind over `sums`, with
/// the release's own tag in its trusted comment. The reason it isn't is what
/// About shows.
pub fn verify(keys: &Keys, sums: &[u8], signature: &str, release: &Release) -> Result<(), String> {
    const BAD: &str = "The release's signature doesn't check out, so nothing was installed.";
    let signature = Signature::decode(signature).map_err(|_| BAD)?;
    let keys = if release.version.is_nightly() {
        keys.nightly
    } else {
        keys.stable
    };
    // Only a prehashed signature, which minisign has made by default since 0.8.
    let signed = keys.iter().any(|key| {
        PublicKey::decode(key).is_ok_and(|key| key.verify(sums, &signature, false).is_ok())
    });
    if !signed {
        return Err(BAD.into());
    }
    // The trusted comment is signed too, so it's checked only once the
    // signature is.
    if signature.trusted_comment() != comment(&release.tag) {
        return Err(tr(
            "The release's signature is for another release, so nothing was installed.",
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::Version;

    fn release(tag: &str) -> Release {
        Release {
            version: Version::parse(tag).unwrap(),
            tag: tag.into(),
            page: String::new(),
            notes: String::new(),
            assets: Vec::new(),
        }
    }

    const SUMS: &[u8] = include_bytes!("testdata/sums.txt");

    #[test]
    fn each_kind_of_release_takes_only_its_own_key() {
        let stable = release("v0.4.1");
        let nightly = release("v0.5.0-nightly.20261114.3");
        let signed = include_str!("testdata/stable.minisig");
        assert_eq!(verify(&KEYS, SUMS, signed, &stable), Ok(()));
        let signed = include_str!("testdata/nightly.minisig");
        assert_eq!(verify(&KEYS, SUMS, signed, &nightly), Ok(()));
        // The nightly key's signature, with a stable release's tag, still
        // isn't the stable key's.
        let signed = include_str!("testdata/nightly-as-stable.minisig");
        assert!(
            verify(&KEYS, SUMS, signed, &stable)
                .unwrap_err()
                .contains("doesn't check out")
        );
    }

    #[test]
    fn a_signature_holds_only_for_its_file_and_its_release() {
        let signed = include_str!("testdata/stable.minisig");
        let changed = [SUMS, b"0000  extra\n"].concat();
        assert!(
            verify(&KEYS, &changed, signed, &release("v0.4.1"))
                .unwrap_err()
                .contains("doesn't check out")
        );
        // The same file, signed for v0.4.1, offered as v0.4.2 or v0.4.0.
        for tag in ["v0.4.2", "v0.4.0"] {
            assert!(
                verify(&KEYS, SUMS, signed, &release(tag))
                    .unwrap_err()
                    .contains("another release")
            );
        }
        assert!(verify(&KEYS, SUMS, "not a signature", &release("v0.4.1")).is_err());
        assert!(verify(&KEYS, SUMS, "", &release("v0.4.1")).is_err());
    }

    /// The keys a release build carries, which the tests above don't use.
    #[test]
    fn the_built_in_keys_are_keys_and_the_kinds_share_none() {
        let built_in = [
            include_str!("../../packaging/keys/stable.pub"),
            include_str!("../../packaging/keys/stable-backup.pub"),
            include_str!("../../packaging/keys/nightly.pub"),
        ];
        let keys: Vec<PublicKey> = built_in
            .iter()
            .map(|key| PublicKey::decode(key).expect("a minisign public key"))
            .collect();
        assert!(keys[0] != keys[1] && keys[1] != keys[2] && keys[0] != keys[2]);
    }
}
