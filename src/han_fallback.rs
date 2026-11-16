//! The fonts that draw Chinese, so a character is never a box. Han
//! unification gives the Chinese, Japanese and Korean scripts one set of code
//! points, and fontique's system fallback names one font for the whole script
//! without asking for a language: where that is a Japanese font (Yu Gothic on
//! Windows, IPA or VL Gothic on Linux), the characters only simplified Chinese
//! has, such as 费, 戏, 领 and 组, are missing from it and draw as boxes
//! (issue #192).

use slint::fontique_012::{fontique, shared_collection};

/// What the system's language asks to be drawn first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Region {
    Simplified,
    Traditional,
    Japanese,
    Korean,
}

impl Region {
    /// The region of a BCP 47 `locale` such as "zh-TW" or "ja-JP"; simplified
    /// Chinese, whose characters the other fonts lack most, for any other.
    fn of(locale: &str) -> Self {
        let locale = locale.to_ascii_lowercase().replace('_', "-");
        let mut parts = locale.split(['-', '.', '@']);
        match parts.next() {
            Some("ja") => Self::Japanese,
            Some("ko") => Self::Korean,
            Some("zh") => {
                let rest: Vec<_> = parts.collect();
                if rest
                    .iter()
                    .any(|part| ["tw", "hk", "mo", "hant"].contains(part))
                {
                    Self::Traditional
                } else {
                    Self::Simplified
                }
            }
            _ => Self::Simplified,
        }
    }
}

/// The families that draw Han on Windows and Linux, in the order `region`
/// wants them: its own first, so a character the languages share is drawn as
/// the system's language draws it, then the others for what they alone have.
fn families(region: Region) -> Vec<&'static str> {
    let sc = [
        "Microsoft YaHei UI",
        "Noto Sans CJK SC",
        "Source Han Sans SC",
        "Source Han Sans CN",
        "WenQuanYi Micro Hei",
        "WenQuanYi Zen Hei",
    ];
    let tc = [
        "Microsoft JhengHei UI",
        "Noto Sans CJK TC",
        "Noto Sans CJK HK",
        "Source Han Sans TC",
        "Source Han Sans HK",
    ];
    let jp = [
        "Yu Gothic UI",
        "Noto Sans CJK JP",
        "Source Han Sans JP",
        "Source Han Sans",
    ];
    let kr = ["Malgun Gothic", "Noto Sans CJK KR", "Source Han Sans KR"];
    let last = [
        "SimSun-ExtB",
        "MingLiU-ExtB",
        "Droid Sans Fallback",
        "Unifont",
    ];
    let order: [&[&'static str]; 4] = match region {
        Region::Simplified => [&sc, &tc, &jp, &kr],
        Region::Traditional => [&tc, &sc, &jp, &kr],
        Region::Japanese => [&jp, &sc, &tc, &kr],
        Region::Korean => [&kr, &sc, &tc, &jp],
    };
    order.into_iter().flatten().copied().chain(last).collect()
}

/// Adds the installed ones of `names` to the end of both generic families text
/// falls back through after the UI typeface, in their order, those already in
/// one left where they are. Text asks them for a character, in order, only
/// when the typeface and what stands before lack it, so Latin and Cyrillic
/// keep their fonts, and a font is loaded when a character needs it.
fn append_families(collection: &mut fontique::Collection, names: &[&str]) {
    let wanted: Vec<_> = names
        .iter()
        .filter_map(|name| collection.family_by_name(name))
        .map(|family| family.id())
        .collect();
    for generic in [
        fontique::GenericFamily::SansSerif,
        fontique::GenericFamily::SystemUi,
    ] {
        let mut list: Vec<_> = collection.generic_families(generic).collect();
        let missing: Vec<_> = wanted
            .iter()
            .copied()
            .filter(|id| !list.contains(id))
            .collect();
        if !missing.is_empty() {
            list.extend(missing);
            collection.set_generic_families(generic, list.into_iter());
        }
    }
}

/// Puts the fonts for Chinese at the end of the fallback, in the order of the
/// system's language. Call after selecting the Slint backend.
pub fn register() {
    let region = sys_locale::get_locale().map_or(Region::Simplified, |locale| Region::of(&locale));
    append_families(&mut shared_collection(), &families(region));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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
        ] {
            assert_eq!(Region::of(locale), region, "{locale}");
        }
    }

    #[test]
    fn each_region_leads_with_its_own_font_and_every_region_has_all() {
        assert_eq!(families(Region::Simplified)[0], "Microsoft YaHei UI");
        assert_eq!(families(Region::Traditional)[0], "Microsoft JhengHei UI");
        assert_eq!(families(Region::Japanese)[0], "Yu Gothic UI");
        assert_eq!(families(Region::Korean)[0], "Malgun Gothic");
        let mut all = families(Region::Japanese);
        let mut other = families(Region::Simplified);
        all.sort_unstable();
        other.sort_unstable();
        assert_eq!(all, other);
    }

    #[test]
    fn the_installed_fonts_follow_what_the_lists_hold_and_none_repeats() {
        let mut collection = fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let mut register = |name: &str| {
            let font = include_bytes!("../ui/fonts/AtkinsonHyperlegibleNext-Regular.ttf");
            let registered = collection.register_fonts(
                fontique::Blob::new(Arc::new(font.to_vec())),
                Some(fontique::FontInfoOverride {
                    family_name: Some(name),
                    ..Default::default()
                }),
            );
            registered.first().expect("a family").0
        };
        let (arial, japanese, simplified) = (
            register("Arial"),
            register("Yu Gothic UI"),
            register("Microsoft YaHei UI"),
        );
        let sans = fontique::GenericFamily::SansSerif;
        let ui = fontique::GenericFamily::SystemUi;
        collection.set_generic_families(sans, [arial, japanese].into_iter());
        collection.set_generic_families(ui, [arial].into_iter());

        append_families(
            &mut collection,
            &["Not installed", "Microsoft YaHei UI", "Yu Gothic UI"],
        );

        let order = |collection: &mut fontique::Collection, generic| {
            collection.generic_families(generic).collect::<Vec<_>>()
        };
        assert_eq!(order(&mut collection, sans), [arial, japanese, simplified]);
        assert_eq!(order(&mut collection, ui), [arial, simplified, japanese]);
    }
}
