//! The settings frame as a part any settings window builds on: the card
//! with the sections on its left and, on its right, a bar with the page's
//! title and the close button over the page's column, the bar for unsaved
//! changes floating over that column. The person's settings ([`super::window`])
//! and the server's and channel's (X01, X13) are all this part with their
//! own sections and pages. The Slint side: `SettingsFrame`
//! (`ui/settings.slint:408`).

use iced::widget::{column, container, row, stack};
use iced::{Length, Padding};

use anomi_theme::prelude::*;
use anomi_vm::settings::frame::Unsaved;
use anomi_widgets::list::List;

use super::bar::{self, COLUMN, SIDES, WIDE};
use crate::kit::{card, unsaved_bar};

/// The room under the page's last line, and over it where the bar for
/// unsaved changes lies (`bottom: 56px`; the bar is 60 tall).
const BOTTOM: f32 = 56.0;
const TOP: f32 = 24.0;
/// The bar for unsaved changes stands this far above the card's bottom.
const FLOAT: f32 = 24.0;
/// Its left inset, the Slint default (`UnsavedBar.pad`).
const BAR_PAD: f32 = 12.0;

type Part<'a, M> = Box<dyn Fn() -> Element<'a, M> + 'a>;

/// The frame: `sections` and `body` are built on each draw of the card, as
/// the card is laid out in the room it is given.
pub struct Pane<'a, M> {
    sections: Part<'a, M>,
    body: Part<'a, M>,
    title: &'a str,
    detail: &'a str,
    close_tip: &'a str,
    on_close: M,
    wide: bool,
    fill: bool,
    key: u64,
    unsaved: Option<(&'a Unsaved, M, M)>,
    motion: Motion,
}

/// A frame whose sections are `sections()` and whose page, titled `title`,
/// is `body()`. `close_tip` is the close button's tooltip ("Close
/// Settings"); `on_close` is sent by it and by a press on the card's gutter.
pub fn pane<'a, M: Clone + 'a>(
    sections: impl Fn() -> Element<'a, M> + 'a,
    body: impl Fn() -> Element<'a, M> + 'a,
    title: &'a str,
    close_tip: &'a str,
    on_close: M,
    motion: Motion,
) -> Pane<'a, M> {
    Pane {
        sections: Box::new(sections),
        body: Box::new(body),
        title,
        detail: "",
        close_tip,
        on_close,
        wide: false,
        fill: false,
        key: 0,
        unsaved: None,
        motion,
    }
}

impl<'a, M: Clone + 'a> Pane<'a, M> {
    /// Quieter words after the title, such as a server's boost level.
    pub fn detail(mut self, detail: &'a str) -> Self {
        self.detail = detail;
        self
    }

    /// The page is a table: its column is up to 1040 px, not 696.
    pub fn wide(mut self, wide: bool) -> Self {
        self.wide = wide;
        self
    }

    /// The page takes the frame's height instead of scrolling in it, for a
    /// list with its own scroll.
    pub fn fill(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }

    /// Names the page: a new key opens the page at its top, where the same
    /// key keeps the scroll.
    pub fn key(mut self, key: u64) -> Self {
        self.key = key;
        self
    }

    /// The bar for unsaved changes over the page's bottom while `bar` is
    /// `Some`; `undo` and `save` are what its buttons send.
    pub fn unsaved(mut self, bar: Option<&'a Unsaved>, undo: M, save: M) -> Self {
        self.unsaved = bar.map(|bar| (bar, undo, save));
        self
    }

    pub fn view(self) -> Element<'a, M> {
        let close = self.on_close.clone();
        let this = self;
        card(
            move |_size| row![(this.sections)(), this.content()].boxed(),
            close,
        )
    }

    /// The bar over the page and the page.
    fn content(&self) -> Element<'a, M> {
        let column_max = if self.wide { WIDE } else { COLUMN };
        let bar = bar::bar(
            self.title,
            self.detail,
            self.close_tip,
            self.on_close.clone(),
            column_max,
            self.motion,
        );
        let room = Padding {
            top: TOP,
            bottom: BOTTOM,
            left: SIDES,
            right: SIDES,
        };
        let page = if self.fill {
            container((self.body)()).height(Length::Fill)
        } else {
            container((self.body)())
        };
        let column_of = container(page.width(Length::Fill.max(column_max)))
            .padding(room)
            .center_x(Length::Fill);
        let scroll = if self.fill {
            column_of.height(Length::Fill).boxed()
        } else {
            // A page opens at its top: each page is its own row in the scroll.
            List::with_rows([(self.key, column_of.boxed())], self.motion)
                .report_pixels(true)
                .boxed()
        };
        let mut layers: Vec<Element<'a, M>> = vec![scroll];
        if let Some((unsaved, undo, save)) = &self.unsaved {
            let notice = unsaved_bar(unsaved, BAR_PAD, undo.clone(), save.clone(), self.motion);
            // Over the page's column, as wide as it.
            layers.push(
                container(container(notice).width(Length::Fill.max(column_max)))
                    .padding(Padding {
                        left: SIDES,
                        right: SIDES,
                        bottom: FLOAT,
                        ..Padding::ZERO
                    })
                    .center_x(Length::Fill)
                    .align_bottom(Length::Fill)
                    .boxed(),
            );
        }
        column![bar, stack(layers).height(Length::Fill)]
            .width(Length::Fill)
            .boxed()
    }
}
