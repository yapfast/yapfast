//! The panel's header (`ui/people.slint:136`-`:196`): 32 px high, 4 px
//! round, "Activity — {n}" at the left, the eye that hides the cards and the
//! chevron that opens the rest. Under the pointer it lifts, but only while it
//! has something to open (`TouchArea.enabled`). The eye sits over it: a click
//! on the eye is the eye's alone.

use iced::widget::{button, container, row, space, text};
use iced::{Alignment, Fill};

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_theme::{Palette, metrics, typography};
use anomi_vm::people::activity::{Activity, Intent};
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::{Icon, icon};
use yapfast_lang::tr;

const HEIGHT: f32 = 32.0;
const RADIUS: f32 = 4.0;
const INSET: f32 = 8.0;
const GAP: f32 = 4.0;
/// The eye's button and its glyph, the chevron (`:155`-`:186`).
const EYE: f32 = 20.0;
const GLYPH: f32 = 16.0;

pub fn view(activity: &Activity, motion: Motion) -> Element<'_, Intent> {
    let hidden = activity.hidden;
    let label = text(tr!("Activity — {}", activity.rows.len()))
        .size(typography::size::SMALL)
        .font(typography::MEDIUM)
        .style(catalog::text::muted)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    let eye = icon_button(
        if hidden { Icon::EyeOff } else { Icon::Eye },
        if hidden {
            tr!("Show Activity Cards")
        } else {
            tr!("Hide Activity Cards")
        },
        motion,
    )
    .size(EYE)
    .glyph(GLYPH)
    .radius(metrics::RADIUS_FIELD)
    .tint(|palette| palette.muted)
    .on_press(Intent::Hide(!hidden))
    .view();
    let mut line = row![label.boxed(), space().width(Fill).boxed(), eye.boxed()]
        .spacing(GAP)
        .align_y(Alignment::Center);
    if activity.opens() {
        // Down while the rest shows. The glyph is swapped, not turned: the
        // rasteriser cannot draw an SVG turned a quarter.
        let glyph = if activity.open && !hidden {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        line = line.push(
            icon(glyph, GLYPH)
                .style(|theme: &Theme, _| iced::widget::svg::Style {
                    color: Some(theme.palette().muted),
                })
                .boxed(),
        );
    }
    let body = container(line)
        .padding([0.0, INSET])
        .height(HEIGHT)
        .width(Fill)
        .center_y(HEIGHT);
    // What a click does: bring the cards back, or fold the rest.
    let press = if hidden {
        Some(Intent::Hide(false))
    } else if activity.opens() {
        Some(Intent::Expand(!activity.open))
    } else {
        None
    };
    button(body)
        .padding(0)
        .on_press_maybe(press)
        .style(|theme: &Theme, status| {
            let palette: &Palette = theme.palette();
            button::Style {
                background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                    .then(|| palette.lift(0.06).into()),
                border: iced::Border::default().rounded(RADIUS),
                ..button::Style::default()
            }
        })
        .boxed()
}
