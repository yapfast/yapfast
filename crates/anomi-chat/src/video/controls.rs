//! A playing video's controls (`VideoControls`, `ui/messages.slint:412`):
//! play or pause, its time, the bar to move through it, mute with its
//! volume, and full screen. A narrow video, such as one filmed upright,
//! leaves out the time and then mute, so full screen stays in it. A press
//! between the controls is theirs: it doesn't play or pause the video
//! under them.

use iced::widget::{opaque, responsive, row, text};
use iced::{Alignment, Fill, Padding};
use yapfast_lang::{text_in, tr};

use anomi_theme::prelude::*;
use anomi_theme::typography::size;
use anomi_vm::chat::video::{Intent, Player};
use anomi_vm::icon::Icon;
use anomi_widgets::button::icon_button;
use anomi_widgets::tip::Side;

use super::look::INK;
use super::scrub::scrub;
use super::sound::{SLOT, sound};

/// The buttons' glyph (`:422`).
const GLYPH: f32 = 16.0;
/// The padding and spacing of the row (`:420`).
const LEFT: f32 = 4.0;
const RIGHT: f32 = 6.0;
const SPACING: f32 = 6.0;
/// The bar keeps 32 px whatever else shows (`:414`).
const BAR_LEAST: f32 = 32.0;
/// Play, full screen, the padding and their spacing, with the bar's least.
const FIXED: f32 = LEFT + SLOT + SPACING + BAR_LEAST + SPACING + SLOT + RIGHT;
/// What mute takes, with its spacing.
const MUTE: f32 = SLOT + SPACING;
/// The time's type size, and roughly how wide its figures and the rest of
/// its characters are at it in the interface's face.
const TIME: f32 = size::CAPTION;
const FIGURE: f32 = 7.0;
const OTHER: f32 = 3.5;

/// What fits in a bar `width` wide: mute (with `sound`), then the time.
pub fn fits(width: f32, sound: bool, time: &str) -> (bool, bool) {
    let mute = sound && width >= FIXED + MUTE;
    let time = width >= FIXED + if mute { MUTE } else { 0.0 } + time_width(time) + SPACING;
    (mute, time)
}

/// About how wide `time` draws: the controls choose by it before it's laid
/// out, as Slint measures its hidden copy (`clock`, `:416`).
fn time_width(time: &str) -> f32 {
    time.chars()
        .map(|c| if c.is_ascii_digit() { FIGURE } else { OTHER })
        .sum()
}

/// The controls of `player`; `expanded` full screen, where the last button
/// brings it back.
pub fn controls<'a>(player: &'a Player, expanded: bool, motion: Motion) -> Element<'a, Intent> {
    let bar = responsive(move |room| bar(player, expanded, room.width, motion))
        .width(Fill)
        .height(Fill);
    opaque(bar).boxed()
}

fn bar<'a>(player: &'a Player, expanded: bool, width: f32, motion: Motion) -> Element<'a, Intent> {
    let (mute, time) = fits(width, player.sound, &player.time);
    let (glyph, label) = if player.playing {
        (Icon::Pause, tr!("Pause"))
    } else {
        (Icon::Play, tr!("Play"))
    };
    let play = control(glyph, label, Side::Above, Intent::Toggle, motion);
    let clock = time.then(|| {
        text(player.time.as_str())
            .size(TIME)
            .color(INK)
            .wrapping(text::Wrapping::None)
            .boxed()
    });
    let mute = mute.then(|| {
        let (glyph, label) = if player.muted {
            (Icon::VolumeX, text_in("VideoControls", "Unmute"))
        } else {
            (Icon::Volume, text_in("VideoControls", "Mute"))
        };
        let button = control(
            glyph,
            label,
            Side::Below,
            Intent::Mute(!player.muted),
            motion,
        );
        sound(button, player.shown_volume(), motion).boxed()
    });
    let (glyph, label) = if expanded {
        (Icon::Minimize, tr!("Exit full screen"))
    } else {
        (Icon::Maximize, tr!("Full screen"))
    };
    let expand = control(glyph, label, Side::Above, Intent::Expand(!expanded), motion);
    row![play]
        .extend(clock)
        .push(scrub(player.progress).boxed())
        .extend(mute)
        .push(expand)
        .spacing(SPACING)
        .padding(Padding::ZERO.left(LEFT).right(RIGHT))
        .align_y(Alignment::Center)
        .width(Fill)
        .height(Fill)
        .boxed()
}

/// A white 28 px button with its tooltip.
fn control<'a>(
    glyph: Icon,
    label: &'a str,
    side: Side,
    intent: Intent,
    motion: Motion,
) -> Element<'a, Intent> {
    icon_button(glyph, label, motion)
        .size(SLOT)
        .glyph(GLYPH)
        .tint(|_| INK)
        .tip_side(side)
        .on_press(intent)
        .view()
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_narrow_video_leaves_out_the_time_then_mute() {
        let time = "0:12 / 1:30";
        assert_eq!(fits(400.0, true, time), (true, true));
        assert_eq!(fits(180.0, true, time), (true, false));
        assert_eq!(fits(140.0, true, time), (false, false));
        assert_eq!(fits(180.0, false, time), (false, true));
    }
}
