//! The pictures and sounds a plugin carries in its package.
//!
//! A manifest names them under `assets`, by a short name of its own and a path
//! inside the plugin's folder. They are read at install, measured and sniffed
//! here, and written into the store beside the module: a plugin never fetches
//! one, never names a path at run time, and is handed one only by the name its
//! manifest gave it. Both processes check a file with [`check`], so a package
//! the helper read out of git and a folder the parent read from disk are held
//! to the same caps.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The most assets one manifest may name.
pub const MAX_ASSETS: usize = 128;

/// The largest one asset may be: a picture a row or a card draws, or a short
/// sound, and the same cap `audio.play` holds a sound a plugin brings to.
pub const MAX_ASSET_BYTES: u64 = 512 << 10;

/// The most a package's assets may weigh together.
pub const MAX_ASSETS_TOTAL_BYTES: u64 = 16 << 20;

/// The longest name an asset may have, in bytes.
pub const MAX_ASSET_NAME_BYTES: usize = 32;

/// What an asset is, read from its first bytes rather than its file name, so a
/// file that calls itself `.png` and isn't one is refused at install.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Png,
    Jpeg,
    Gif,
    Webp,
    Wav,
    Ogg,
    Mp3,
}

impl AssetKind {
    /// Whether this is a picture, which a widget tree or an emoji may use.
    pub fn is_picture(self) -> bool {
        matches!(
            self,
            AssetKind::Png | AssetKind::Jpeg | AssetKind::Gif | AssetKind::Webp
        )
    }

    /// Whether this is a sound, which `audio.play` may play.
    pub fn is_sound(self) -> bool {
        !self.is_picture()
    }

    /// The media type a file of this kind is.
    pub fn media_type(self) -> &'static str {
        match self {
            AssetKind::Png => "image/png",
            AssetKind::Jpeg => "image/jpeg",
            AssetKind::Gif => "image/gif",
            AssetKind::Webp => "image/webp",
            AssetKind::Wav => "audio/wav",
            AssetKind::Ogg => "audio/ogg",
            AssetKind::Mp3 => "audio/mpeg",
        }
    }

    /// What a file's first bytes say it is, or nothing for a kind the
    /// interface can't draw or play.
    pub fn sniff(bytes: &[u8]) -> Option<AssetKind> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(AssetKind::Png)
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            Some(AssetKind::Jpeg)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(AssetKind::Gif)
        } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            Some(AssetKind::Webp)
        } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
            Some(AssetKind::Wav)
        } else if bytes.starts_with(b"OggS") {
            Some(AssetKind::Ogg)
        } else if bytes.starts_with(b"ID3")
            || (bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0)
        {
            Some(AssetKind::Mp3)
        } else {
            None
        }
    }
}

impl fmt::Display for AssetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.media_type())
    }
}

/// One asset as `assets.list` answers it and the review lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetInfo {
    pub name: String,
    pub kind: AssetKind,
    pub bytes: u64,
}

/// Whether `name` is one a manifest may give an asset: 1 to 32 bytes of
/// `a-z`, `0-9`, `-`, `_` and `.`, starting with a letter or digit. A name is
/// a key, never a path, so it can't climb out of the store.
pub fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_ASSET_NAME_BYTES
        && name
            .bytes()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
}

/// Why an asset was refused at install, in a sentence a person reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetError(pub String);

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AssetError {}

/// Checks one asset's bytes: its size and what it is.
pub fn check(name: &str, bytes: &[u8]) -> Result<AssetKind, AssetError> {
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err(AssetError(format!(
            "the asset `{name}` is {} bytes, and an asset is at most {} KiB",
            bytes.len(),
            MAX_ASSET_BYTES >> 10
        )));
    }
    AssetKind::sniff(bytes).ok_or_else(|| {
        AssetError(format!(
            "the asset `{name}` isn't a PNG, JPEG, GIF or WebP picture or a WAV, Ogg or MP3 sound"
        ))
    })
}

/// Checks a whole package's assets together, after each was checked alone.
pub fn check_total(sizes: impl IntoIterator<Item = u64>) -> Result<(), AssetError> {
    let total: u64 = sizes.into_iter().sum();
    if total > MAX_ASSETS_TOTAL_BYTES {
        return Err(AssetError(format!(
            "the assets weigh {total} bytes together, and a plugin carries at most {} MiB",
            MAX_ASSETS_TOTAL_BYTES >> 20
        )));
    }
    Ok(())
}

/// A line for the review, such as `3 pictures and 1 sound, 412 KiB`.
pub fn summary(assets: &[AssetInfo]) -> Option<String> {
    if assets.is_empty() {
        return None;
    }
    let pictures = assets
        .iter()
        .filter(|asset| asset.kind.is_picture())
        .count();
    let sounds = assets.len() - pictures;
    let count = |n: usize, one: &str, many: &str| match n {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {many}")),
    };
    let parts: Vec<String> = [
        count(pictures, "picture", "pictures"),
        count(sounds, "sound", "sounds"),
    ]
    .into_iter()
    .flatten()
    .collect();
    let bytes: u64 = assets.iter().map(|asset| asset.bytes).sum();
    let size = if bytes < 1024 {
        format!("{bytes} bytes")
    } else {
        format!("{} KiB", bytes.div_ceil(1024))
    };
    Some(format!("Carries {}, {size}", parts.join(" and ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_what_its_bytes_say_and_not_what_its_name_says() {
        assert_eq!(check("logo", b"\x89PNG\r\n\x1a\nrest"), Ok(AssetKind::Png));
        assert_eq!(check("beep", b"RIFF\0\0\0\0WAVEfmt "), Ok(AssetKind::Wav));
        assert_eq!(check("beep", b"OggS\0"), Ok(AssetKind::Ogg));
        assert!(check("logo", b"<svg></svg>").is_err(), "an SVG is code");
        assert!(check("x", b"").is_err());
    }

    #[test]
    fn an_asset_past_its_cap_is_refused_with_the_cap_in_the_sentence() {
        let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
        big.resize(MAX_ASSET_BYTES as usize + 1, 0);
        let refusal = check("big", &big).unwrap_err().to_string();
        assert!(refusal.contains("512 KiB"), "{refusal}");
        assert!(check_total([MAX_ASSETS_TOTAL_BYTES, 1]).is_err());
        assert!(check_total([MAX_ASSET_BYTES; 8]).is_ok());
    }

    #[test]
    fn a_name_is_a_key_and_never_a_path() {
        assert!(name_ok("logo"));
        assert!(name_ok("beep-2.wav"));
        assert!(!name_ok(""));
        assert!(!name_ok("../logo"));
        assert!(!name_ok("a/b"));
        assert!(!name_ok(".hidden"));
        assert!(!name_ok("Logo"));
        assert!(!name_ok(&"a".repeat(MAX_ASSET_NAME_BYTES + 1)));
    }

    #[test]
    fn the_review_says_what_a_package_carries() {
        let assets = [
            AssetInfo {
                name: "logo".into(),
                kind: AssetKind::Png,
                bytes: 2048,
            },
            AssetInfo {
                name: "beep".into(),
                kind: AssetKind::Wav,
                bytes: 1000,
            },
        ];
        assert_eq!(
            summary(&assets).as_deref(),
            Some("Carries 1 picture and 1 sound, 3 KiB")
        );
        assert_eq!(summary(&[]), None);
    }
}
