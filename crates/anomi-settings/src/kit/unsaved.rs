//! Discord's bar once a draft differs from what Discord has: Reset puts it
//! back, and Save Changes sends it. Each time leaving is refused for the
//! changes, `nudge` goes up, and the bar shakes red for a moment, as
//! Discord's does (`anomi_widgets::shake`). The Slint
//! side: `UnsavedBar` (`ui/settings.slint:311`): 60 px tall, the raised
//! ground under a line, its words 14 px semibold, a ghost Reset and a
//! primary Save Changes.
//!
//! The bar's coming up from its edge (`PopupEntry`) isn't drawn: iced has no
//! layer transform, so it appears whole.

use iced::widget::{container, row, text};
use iced::{Color, Length, Padding, Shadow, Vector, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::settings::frame::Unsaved;
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::shake::shake;
use yapfast_lang::tr;

const HEIGHT: f32 = 60.0;
/// The bar's insets (`padding: 12px`) and the gap between its parts.
const INSET: f32 = 12.0;
const GAP: f32 = 8.0;
/// The bar floats over the page: 8 px corners and a 16 px blur, `#00000066`,
/// 4 px down (`ui/server_settings.slint:464`).
const RADIUS: f32 = 8.0;
const SHADOW_BLUR: f32 = 16.0;
const SHADOW_DROP: f32 = 4.0;
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 102.0 / 255.0);

/// The bar for `bar`; `pad` is its left inset, 12 px unless a page lines it
/// up with its column. `undo` and `save` are what the buttons send.
pub fn unsaved_bar<'a, M: Clone + 'a>(
    bar: &'a Unsaved,
    pad: f32,
    undo: M,
    save: M,
    motion: Motion,
) -> Element<'a, M> {
    let words = text(tr!("Careful — you have unsaved changes!"))
        .size(typography::size::SMALL)
        .font(typography::SEMIBOLD)
        .wrapping(text::Wrapping::WordOrGlyph)
        .style(catalog::text::inherit);
    let reset = action_button(tr!("Reset"), Kind::Ghost, motion)
        .enabled(bar.may_reset && !bar.saving)
        .on_press(undo);
    let saving = if bar.saving {
        tr!("Saving...")
    } else {
        tr!("Save Changes")
    };
    let save = action_button(saving, Kind::Primary, motion)
        .enabled(bar.may_save && !bar.saving)
        .on_press(save);
    let content = row![
        container(words)
            .clip(true)
            .width(Length::Fill)
            .align_y(alignment::Vertical::Center)
            .height(Length::Fill),
        reset,
        save,
    ]
    .spacing(GAP)
    .align_y(alignment::Vertical::Center)
    .padding(Padding {
        left: pad,
        ..Padding::new(INSET)
    })
    .height(HEIGHT);
    shake(content.boxed(), bar.nudge, motion)
        .height(HEIGHT)
        .radius(RADIUS)
        .shadow(Shadow {
            color: SHADOW,
            offset: Vector::new(0.0, SHADOW_DROP),
            blur_radius: SHADOW_BLUR,
        })
        .boxed()
}
