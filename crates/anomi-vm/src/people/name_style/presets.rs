//! The name style window's presets: Discord's colours for each effect, each
//! written as hex joined by commas (`ui/server.slint:1155`), the faces and
//! the effects it offers.

/// The faces: Discord's `font_id`, its label (none for the usual font,
/// which a screen words itself).
pub const FACES: [(u8, Option<&str>); 12] = [
    (11, None),
    (12, Some("Tempo")),
    (3, Some("Sakura")),
    (4, Some("Jellybean")),
    (6, Some("Modern")),
    (7, Some("Medieval")),
    (8, Some("8Bit")),
    (10, Some("Vampyre")),
    (13, Some("Monkey Bars")),
    (14, Some("Mainframe")),
    (15, Some("Headbang")),
    (16, Some("Journal")),
];

/// The effects offered, in the window's order: Discord's `effect_id`s.
pub const EFFECTS: [u8; 7] = [1, 2, 3, 4, 5, 8, 7];

const SOLID: [&str; 7] = [
    "f0f0f2", "26e36e", "1f99ec", "c32cfe", "da034a", "f93824", "c6a617",
];
const NEON: [&str; 7] = [
    "6b20ee", "26e36e", "1f99ec", "c32cfe", "da034a", "f93824", "c6a617",
];
const TOON: [&str; 7] = [
    "f52399", "26e36e", "1f99ec", "c32cfe", "da034a", "f93824", "c6a617",
];
const POP: [&str; 7] = [
    "12d088", "26e36e", "1f99ec", "c32cfe", "da034a", "f93824", "c6a617",
];

const GRADIENT: [&str; 5] = [
    "2dafa7,fec491",
    "ca63fa,417ae4",
    "7c61c4,6bba9f",
    "edaab1,e577da",
    "f39054,e764cf",
];
const GUMMY: [&str; 7] = [
    "eaaad6,aadef1,baa8ef,baebcc",
    "fe70c2,f550a8,fe9bd6,da6cb8",
    "ffb67c,ff9571,ff7e7a,f06c8b",
    "c4ed66,98de61,6dcd64,46bc67",
    "7db8fe,8fe0d0,9d95f5,67cff0",
    "d77cfe,b56cfe,e45de5,8c6dfe",
    "d83b60,c94ccb,8c5dfe,b06dfe",
];
const PRISM: [&str; 4] = [
    "fe9444,37ee80,fee048,ff4ed3,5966f2",
    "90c464,68e971,fed84f,fe63bd,b458e0",
    "7ed8ed,7dbbfe,828bfe,ad72fe,d17bed",
    "a5160f,d32f15,f45e18,fc8d05,feb122",
];

/// The presets of an effect; solid ones for any this window doesn't offer.
pub fn of(effect: u8) -> &'static [&'static str] {
    match effect {
        2 => &GRADIENT,
        3 => &NEON,
        4 => &TOON,
        5 => &POP,
        7 => &PRISM,
        8 => &GUMMY,
        _ => &SOLID,
    }
}

/// A key's colours: each hex part that reads.
pub fn colours(key: &str) -> Vec<u32> {
    key.split(',').filter_map(parse).collect()
}

/// One hex colour, with or without `#`: six digits.
pub fn parse(text: &str) -> Option<u32> {
    let digits = text.trim().trim_start_matches('#');
    (digits.len() == 6)
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten()
}
