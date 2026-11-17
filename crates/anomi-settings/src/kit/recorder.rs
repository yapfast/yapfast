//! A keybind's keys and the button that records them: `KeyRecorder`
//! (`ui/settings.slint:713`), for Voice & Video's push to talk and the
//! Keybinds page. The well shows the keys, or what is pressed so far while
//! recording, on the sunken ground with the accent's rim.

use iced::widget::{container, row, text};
use iced::{Border, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

/// The well's height and inset, and its gap to the button.
const HEIGHT: f32 = 36.0;
const INSET: f32 = 12.0;
const GAP: f32 = 8.0;

/// The well `width` wide and its button: `keys` set, `recorded` so far
/// while `recording`; `on_record(on)` is what the button asks for.
pub fn recorder<'a, M: Clone + 'a>(
    keys: &'a str,
    recording: bool,
    recorded: &'a str,
    on_record: impl Fn(bool) -> M,
    width: impl Into<Length>,
    motion: Motion,
) -> Element<'a, M> {
    let set = !keys.is_empty() && !recording;
    let shown = if recording {
        if recorded.trim_end().is_empty() {
            tr!("Press a key...")
        } else {
            recorded.trim_end()
        }
    } else if keys.is_empty() {
        tr!("No keybind set")
    } else {
        keys
    };
    let label = text(shown)
        .size(typography::size::BODY)
        .font(if set {
            typography::SEMIBOLD
        } else {
            typography::UI
        })
        .style(if set {
            catalog::text::primary
        } else {
            catalog::text::muted
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    let well = container(label)
        .width(width)
        .height(HEIGHT)
        .padding([0.0, INSET])
        .align_y(alignment::Vertical::Center)
        .clip(true)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.sunken.into()),
                border: Border::default()
                    .rounded(metrics::RADIUS_CARD)
                    .width(if recording { 2.0 } else { 1.0 })
                    .color(if recording {
                        palette.accent
                    } else {
                        palette.line
                    }),
                ..container::Style::default()
            }
        });
    let (button, kind) = if recording {
        (tr!("Stop recording"), Kind::Danger)
    } else if keys.is_empty() {
        (tr!("Record keybind"), Kind::Secondary)
    } else {
        (tr!("Edit keybind"), Kind::Secondary)
    };
    row![
        well,
        action_button(button, kind, motion)
            .on_press(on_record(!recording))
            .boxed(),
    ]
    .spacing(GAP)
    .align_y(alignment::Vertical::Center)
    .boxed()
}
