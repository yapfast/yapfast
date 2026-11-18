//! A drop-down on a settings page: the chosen option on a face that opens a
//! list below it, or above where there is less room. The Slint side:
//! `Picker` (`ui/settings.slint:203`), the std-widgets `ComboBox` at the
//! settings pages' control height; the look is the theme's `pick_list` and
//! `menu` catalogs'.

use std::fmt;

use iced::widget::{container, pick_list, stack, text};
use iced::{Length, Padding, Pixels, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_widgets::icon::{Icon, icon};

/// The settings pages' control height (`min-height: 36px`): the text's
/// line takes what the padding leaves.
const HEIGHT: f32 = 36.0;
const VERTICAL: f32 = 8.25;
const HORIZONTAL: f32 = 12.0;
/// Fluent's chevron is 12 px wide (`combobox.slint:84`); Lucide's path
/// spans half its box, so the box is twice that, less its stroke.
const CHEVRON: f32 = 20.0;
/// Its box's end lies this far in from the face's, so the glyph's end
/// lies 11 px in, as Fluent's (`padding-right: 11px`).
const CHEVRON_END: f32 = 7.0;
/// The text stops before the chevron: Fluent's padding, its icon and the
/// layout's spacing.
const RIGHT: f32 = 11.0 + 12.0 + 8.0;

/// An option of the list: its place, which is what is sent, and its words,
/// which are what shows.
#[derive(Clone, PartialEq)]
struct Pick<'a> {
    index: usize,
    words: &'a str,
}

impl fmt::Display for Pick<'_> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(self.words)
    }
}

/// A drop-down of `options` showing `chosen`; a pick sends `on_pick(index)`.
/// Without `on_pick` it is off.
pub fn picker<'a, M: Clone + 'a, S: AsRef<str>>(
    options: &'a [S],
    chosen: usize,
    on_pick: Option<impl Fn(usize) -> M + 'a>,
) -> Element<'a, M> {
    let picks: Vec<Pick<'a>> = options
        .iter()
        .enumerate()
        .map(|(index, words)| Pick {
            index,
            words: words.as_ref(),
        })
        .collect();
    let shown = picks.get(chosen).cloned();
    let mut list = pick_list(shown, picks, |pick: &Pick<'_>| pick.words.to_owned())
        .width(Length::Fill)
        .padding(Padding {
            right: RIGHT,
            ..Padding::from([VERTICAL, HORIZONTAL])
        })
        .text_size(typography::size::BODY)
        .line_height(text::LineHeight::Absolute(Pixels(HEIGHT - 2.0 * VERTICAL)))
        .handle(pick_list::Handle::None);
    let enabled = on_pick.is_some();
    if let Some(on_pick) = on_pick {
        list = list.on_select(move |pick: Pick<'_>| on_pick(pick.index));
    }
    let status = if enabled {
        pick_list::Status::Active
    } else {
        pick_list::Status::Disabled
    };
    let chevron =
        icon(Icon::ChevronDown, CHEVRON).style(move |theme: &Theme, _| iced::widget::svg::Style {
            color: Some(catalog::pick_list::default(theme, status).handle_color),
        });
    let over = container(chevron)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            right: CHEVRON_END,
            ..Padding::ZERO
        })
        .align_x(alignment::Horizontal::Right)
        .align_y(alignment::Vertical::Center);
    stack![list.boxed(), over.boxed()]
        .width(Length::Fill)
        .boxed()
}
