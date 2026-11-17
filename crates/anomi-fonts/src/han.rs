//! The fonts that draw Chinese, Japanese and Korean, so a character is never
//! a box. Han unification gives the three one set of code points, and a font
//! for one language lacks characters only another has, such as simplified
//! Chinese's 费, 戏, 领 and 组 in a Japanese font (issue #192). So the system's
//! language picks the order, and every language's fonts follow for what its
//! own lack. cosmic-text asks these for a script before its common fallback,
//! as `anomi_theme::fonts` sets it up.
//!
//! Ports `src/han_fallback.rs`; the Windows symbol fonts are `src/emoji.rs`'s
//! `WINDOWS_FALLBACK`.

use std::sync::LazyLock;

use unicode_script::Script;

/// What the system's language asks to be drawn first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Region {
    Simplified = 0,
    Traditional = 1,
    Japanese = 2,
    Korean = 3,
}

impl Region {
    /// The region of a BCP 47 `locale` such as "zh-TW" or "ja-JP"; simplified
    /// Chinese, whose characters the other fonts lack most, for any other.
    pub fn of(locale: &str) -> Self {
        let locale = locale.to_ascii_lowercase().replace('_', "-");
        let mut parts = locale.split(['-', '.', '@']);
        match parts.next() {
            Some("ja") => Self::Japanese,
            Some("ko") => Self::Korean,
            Some("zh") => {
                if parts.any(|part| ["tw", "hk", "mo", "hant"].contains(&part)) {
                    Self::Traditional
                } else {
                    Self::Simplified
                }
            }
            _ => Self::Simplified,
        }
    }
}

const SIMPLIFIED: &[&str] = &[
    "Microsoft YaHei UI",
    "Noto Sans CJK SC",
    "Source Han Sans SC",
    "Source Han Sans CN",
    "WenQuanYi Micro Hei",
    "WenQuanYi Zen Hei",
];
const TRADITIONAL: &[&str] = &[
    "Microsoft JhengHei UI",
    "Noto Sans CJK TC",
    "Noto Sans CJK HK",
    "Source Han Sans TC",
    "Source Han Sans HK",
];
const JAPANESE: &[&str] = &[
    "Yu Gothic UI",
    "Noto Sans CJK JP",
    "Source Han Sans JP",
    "Source Han Sans",
];
const KOREAN: &[&str] = &["Malgun Gothic", "Noto Sans CJK KR", "Source Han Sans KR"];
/// What draws the rarest characters, after every language's own.
const LAST: &[&str] = &[
    "SimSun-ExtB",
    "MingLiU-ExtB",
    "Droid Sans Fallback",
    "Unifont",
];

/// The families that draw Han on Windows and Linux, the region's own first, so
/// a character the languages share is drawn as the system's language draws
/// it, then the others for what they alone have. Made once, for cosmic-text
/// to keep a borrow of.
fn ordered(region: Region) -> &'static [&'static str] {
    static ORDERS: LazyLock<[Vec<&'static str>; 4]> = LazyLock::new(|| {
        [
            [SIMPLIFIED, TRADITIONAL, JAPANESE, KOREAN],
            [TRADITIONAL, SIMPLIFIED, JAPANESE, KOREAN],
            [JAPANESE, SIMPLIFIED, TRADITIONAL, KOREAN],
            [KOREAN, SIMPLIFIED, TRADITIONAL, JAPANESE],
        ]
        .map(|lists| lists.into_iter().chain([LAST]).flatten().copied().collect())
    });
    &ORDERS[region as usize]
}

/// The families to ask for `script` in text of a system set to `locale`, or
/// none for a script these don't draw: Han in the language's order, kana in
/// Japanese's, Hangul in Korean's, Bopomofo in traditional Chinese's.
pub fn families(script: Script, locale: &str) -> Option<&'static [&'static str]> {
    let region = match script {
        Script::Han => Region::of(locale),
        Script::Hiragana | Script::Katakana => Region::Japanese,
        Script::Hangul => Region::Korean,
        Script::Bopomofo => Region::Traditional,
        _ => return None,
    };
    Some(ordered(region))
}

/// The fonts Windows ships for the "Common" script's symbols and the styled
/// letters of Unicode's mathematical alphabets that names use, such as 𝓯𝓪𝓷𝓬𝔂:
/// on Windows no font is picked for that script, so they go in the common
/// fallback itself, after Twemoji.
pub const WINDOWS_SYMBOLS: [&str; 4] = [
    "Segoe UI",
    "Segoe UI Symbol",
    "Cambria Math",
    "Segoe UI Historic",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_systems_language_picks_the_region() {
        for (locale, region) in [
            ("zh-CN", Region::Simplified),
            ("zh_Hans_CN", Region::Simplified),
            ("zh-TW", Region::Traditional),
            ("zh_HK.UTF-8", Region::Traditional),
            ("zh-Hant-TW", Region::Traditional),
            ("ja-JP", Region::Japanese),
            ("ko_KR", Region::Korean),
            ("ru-RU", Region::Simplified),
            ("en-US", Region::Simplified),
            ("", Region::Simplified),
        ] {
            assert_eq!(Region::of(locale), region, "{locale}");
        }
    }

    #[test]
    fn each_region_leads_with_its_own_font_and_every_region_has_all() {
        let first = |script, locale| families(script, locale).unwrap()[0];
        assert_eq!(first(Script::Han, "zh-CN"), "Microsoft YaHei UI");
        assert_eq!(first(Script::Han, "zh-TW"), "Microsoft JhengHei UI");
        assert_eq!(first(Script::Han, "ja"), "Yu Gothic UI");
        assert_eq!(first(Script::Han, "ko-KR"), "Malgun Gothic");
        assert_eq!(first(Script::Hiragana, "en-US"), "Yu Gothic UI");
        assert_eq!(first(Script::Hangul, "ru"), "Malgun Gothic");
        let mut all = ordered(Region::Japanese).to_vec();
        let mut other = ordered(Region::Simplified).to_vec();
        assert_eq!(all.len(), 22);
        all.sort_unstable();
        other.sort_unstable();
        assert_eq!(all, other);
        assert_eq!(families(Script::Latin, "zh-CN"), None);
        assert_eq!(families(Script::Cyrillic, "ja"), None);
    }
}
