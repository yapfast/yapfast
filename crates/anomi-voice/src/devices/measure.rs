//! How wide the noise options are, which the menu fits itself to
//! (`noise-measure`, `ui/yapfast.slint:2052`, `:2071`): each option's bold
//! words, measured again when the language or the fonts change.

use std::cell::Cell;

use iced::advanced::text::paragraph::Plain;
use iced::advanced::text::{Alignment, Ellipsis, LineHeight, Shaping, Text, Wrapping};
use iced::{Pixels, Size, alignment};

use anomi_theme::fonts::{self, Version};
use anomi_theme::typography;
use anomi_vm::voice::devices::Noise;

use super::choices::name;

type Paragraph = Plain<<anomi_theme::Renderer as iced::advanced::text::Renderer>::Paragraph>;

/// The room the menu adds to the options' bold words (`+ 108px`) and its
/// least width.
const EXTRA: f32 = 108.0;
const LEAST: f32 = 280.0;

thread_local! {
    /// The widths with the `Off` name they were measured for, the only
    /// option whose words change with the language, and the fonts they were
    /// measured in.
    static BOLD: Cell<Option<(&'static str, Version, [f32; 3])>> = const { Cell::new(None) };
}

/// Each option's bold words' width, in [`Noise::ALL`] order.
pub fn bold() -> [f32; 3] {
    bold_for(name(Noise::Off))
}

/// The widths with `off` as the `Off` name, cached by that word and the
/// fonts.
fn bold_for(off: &'static str) -> [f32; 3] {
    let fonts = fonts::version();
    if let Some((measured, kept, widths)) = BOLD.get()
        && measured == off
        && kept == fonts
    {
        return widths;
    }
    let widths = Noise::ALL.map(|noise| {
        let words = if noise == Noise::Off {
            off
        } else {
            name(noise)
        };
        Paragraph::new(Text {
            content: words.to_owned(),
            bounds: Size::new(f32::INFINITY, 28.0),
            size: Pixels(typography::size::SMALL),
            line_height: LineHeight::default(),
            font: typography::BOLD,
            align_x: Alignment::Center,
            align_y: alignment::Vertical::Center,
            shaping: Shaping::Advanced,
            wrapping: Wrapping::None,
            ellipsis: Ellipsis::None,
            hint_factor: None,
        })
        .min_width()
    });
    BOLD.set(Some((off, fonts, widths)));
    widths
}

/// The input menu's width with noise suppression to choose.
pub fn card() -> f32 {
    LEAST.max((bold().iter().sum::<f32>() + EXTRA).floor())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_word_measures_again_and_the_same_word_comes_from_the_cache() {
        let short = bold_for("Off");
        let long = bold_for("Switched off entirely");
        assert_ne!(short[2], long[2]);
        assert_eq!(short[0], long[0]);
        // The same word again is the cached array.
        assert_eq!(bold_for("Switched off entirely"), long);
        assert_eq!(
            BOLD.get().map(|(word, _, _)| word),
            Some("Switched off entirely")
        );
    }
}
