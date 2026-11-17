//! The cards for `anomi-shots`, each the whole window with one card open. No
//! golden screenshot has them; they are checked against the Slint app.

use anomi_vm::fixtures::chat::cards as samples;

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm| Shot {
        area: "emoji_card",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("emoji", || {
            samples::over(samples::showing_emoji(samples::emoji()))
        }),
        shot("emoji_unicode", || {
            samples::over(samples::showing_emoji(samples::emoji_unicode()))
        }),
        shot("emoji_late", || {
            samples::over(samples::showing_emoji(samples::emoji_late()))
        }),
        shot("sticker", || {
            samples::over(samples::showing_sticker(samples::sticker()))
        }),
        shot("sticker_default", || {
            samples::over(samples::showing_sticker(samples::sticker_default()))
        }),
    ]
}
