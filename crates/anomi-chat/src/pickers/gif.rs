//! The GIF picker. Task C18 (docs/iced/tasks/chat.md), porting `GifPicker`
//! (`ui/gifs.slint:68`) and `FavoriteStar` (`:32`), as `show_gifs` and its
//! masonry (`src/ui/gifs.rs`) fill them: a search over Klipy, a home of the
//! Favorites tile, "Trending GIFs" and the trending terms, and the GIFs found
//! in columns ([`grid`], [`tile`]), a star on each to keep it among the
//! favourites. A click sends a GIF, Shift and a click puts it in the message.
//!
//! [`view`] is what shows under the picker's tabs ([`super`]). The app
//! focuses [`FIELD`] as it shows, and searches once typing has paused
//! ([`Intent::Edit`]). The card's dragged edge and the keys ([`keys`]) are
//! here too, as the tabs share them.

mod edge;
mod grid;
mod keys;
#[cfg(feature = "shots")]
mod shots;
mod tile;

use iced::widget::{Id, container, row, stack, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::chat::gif_picker::{GifPicker, Intent, Page};
use anomi_widgets::button::icon_button;
use anomi_widgets::field::field;
use anomi_widgets::icon::Icon;
use yapfast_lang::tr;

pub use edge::edge;
#[cfg(feature = "shots")]
pub use shots::shots;

/// The search field's id, which the app focuses as the picker shows (the
/// `Timer` of `ui/gifs.slint:104`).
pub const FIELD: &str = "gif-picker-field";

/// The search row: 16 px either side and 12 under, 8 between; the back
/// button 34 px, the field 34 px high with 15 px text 10 px in (`:121`-`:158`).
const BAR_SIDE: f32 = 16.0;
const BAR_FOOT: f32 = 12.0;
const BAR_SPACING: f32 = 8.0;
const CONTROL: f32 = 34.0;
const TEXT: f32 = 15.0;
const PAD: f32 = 10.0;

/// What shows under the tabs, in a card `panel` wide: the search over the
/// grid.
pub fn view<'a>(picker: &'a GifPicker, panel: f32, motion: Motion) -> impl View<Intent> + 'a {
    let page = stack![
        iced::widget::column![bar(picker, motion), grid::view(picker, panel, motion)]
            .width(Fill)
            .height(Fill),
        keys::keys(
            (picker.page != Page::Home).then_some(Intent::Back),
            Intent::Close,
            Intent::Shift,
        ),
    ];
    page.width(Fill).height(Fill)
}

/// The back button, the "Trending GIFs" title or the search field.
fn bar(picker: &GifPicker, motion: Motion) -> Element<'_, Intent> {
    let mut bar = row![].spacing(BAR_SPACING).align_y(Alignment::Center);
    if picker.page != Page::Home {
        bar = bar.push(
            icon_button(Icon::ChevronLeft, tr!("Back"), motion)
                .size(CONTROL)
                .on_press(Intent::Back)
                .view(),
        );
    }
    if picker.page == Page::Trending {
        // The field is out of sight here, where typing still searches.
        bar = bar.push(
            text(tr!("Trending GIFs"))
                .size(typography::size::TITLE)
                .font(typography::BOLD)
                .width(Fill),
        );
    } else {
        let favorites = picker.page == Page::Favorites;
        let placeholder = if favorites {
            tr!("Search Favorites")
        } else {
            tr!("Search Klipy")
        };
        let field = field(placeholder, &picker.query)
            .id(Id::new(FIELD))
            .on_input(move |text| {
                // Favorites filter as it is typed; a search waits for a pause.
                if favorites {
                    Intent::Search(text)
                } else {
                    Intent::Edit(text)
                }
            })
            .on_submit(Intent::Search(picker.query.clone()))
            .text_size(TEXT)
            .pad(PAD)
            .height(CONTROL)
            .motion(motion);
        bar = bar.push(container(field.view()).width(Fill));
    }
    container(bar)
        .padding(Padding {
            top: 0.0,
            right: BAR_SIDE,
            bottom: BAR_FOOT,
            left: BAR_SIDE,
        })
        .width(Fill)
        .boxed()
}
