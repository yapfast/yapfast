//! A drop-down on a settings page: the chosen option on a face that opens a
//! list below it, or above where there is less room. The Slint side:
//! `Picker` (`ui/settings.slint:203`), the std-widgets `ComboBox` at the
//! settings pages' control height; the look is the theme's `pick_list` and
//! `menu` catalogs'.

use std::fmt;

use iced::widget::pick_list;
use iced::{Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;

/// A control's height with the text's own line (`min-height: 36px`).
const VERTICAL: f32 = 8.9;
const HORIZONTAL: f32 = 12.0;
/// Fluent's chevron (`combobox.slint:84`).
const CHEVRON: f32 = 12.0;

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
        .padding(Padding::from([VERTICAL, HORIZONTAL]))
        .text_size(typography::size::SMALL)
        .handle(pick_list::Handle::Arrow {
            size: Some(CHEVRON.into()),
        });
    if let Some(on_pick) = on_pick {
        list = list.on_select(move |pick: Pick<'_>| on_pick(pick.index));
    }
    list.boxed()
}
