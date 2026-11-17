//! The fake Discord's nameplates, drawn in code as `testing/fake_discord.ts`
//! draws them (`collectiblePicture`), so sample rows carry the plates the
//! reference screenshots show: a 448 by 84 strip that fades in from its
//! left, white as a still and moving between white and pink, and the
//! colours of the palettes the fake hands out, as
//! `crates/yapfast-discord/src/collectibles.rs` knows them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use iced::Color;
use iced::widget::image;

use crate::picture::{Frames, register};

const WIDTH: u32 = 448;
const HEIGHT: u32 = 84;

thread_local! {
    static MADE: RefCell<HashMap<String, image::Handle>> = RefCell::default();
}

/// The strip the fake serves for the nameplate `sku`, as a still that
/// moves: made once a thread, so its handle keeps its id.
pub fn strip(sku: &str) -> image::Handle {
    if let Some(still) = MADE.with_borrow(|made| made.get(sku).cloned()) {
        return still;
    }
    let frame = |shade: u8| {
        let mut rgba = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
        for _ in 0..HEIGHT {
            for x in 0..WIDTH {
                let alpha = (255.0 * x as f32 / WIDTH as f32).round() as u8;
                rgba.extend([255, shade, 255, alpha]);
            }
        }
        rgba
    };
    let frames = vec![frame(255), frame(120)];
    let still = image::Handle::from_rgba(WIDTH, HEIGHT, frames[0].clone());
    // The fake's APNGs play ten frames a second.
    let moving = Frames::new(WIDTH, HEIGHT, frames, vec![100, 100], 0);
    register(&still, Arc::new(moving));
    MADE.with_borrow_mut(|made| made.insert(sku.to_owned(), still.clone()));
    still
}

/// A palette's colours on a dark theme and on a light one, as Discord's
/// client tints with them; none for one Anomi doesn't know, as the fake's
/// `sunset`.
pub fn palette(name: &str) -> Option<[Color; 2]> {
    let [dark, light] = match name {
        "crimson" => [0x90_00_07, 0xe7_04_0f],
        "berry" => [0x89_3a_99, 0xb1_1f_cf],
        "sky" => [0x00_80_b7, 0x56_cc_ff],
        "teal" => [0x08_64_60, 0x7d_ee_d7],
        "forest" => [0x2d_54_01, 0x6a_a6_24],
        "bubble_gum" => [0xdc_3e_97, 0xf9_57_b3],
        "violet" => [0x73_0b_c8, 0x97_2f_ed],
        "cobalt" => [0x01_31_c2, 0x42_78_ff],
        "clover" => [0x04_7b_20, 0x63_cd_5a],
        "lemon" => [0xf6_cd_12, 0xfe_d4_00],
        "white" => [0xff_ff_ff, 0xff_ff_ff],
        "black" => [0x00_00_00, 0x00_00_00],
        _ => return None,
    };
    Some([anomi_theme::rgb(dark), anomi_theme::rgb(light)])
}
