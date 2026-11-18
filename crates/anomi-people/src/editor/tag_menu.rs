//! The server tag's chooser (`tag-menu`, `ui/server.slint:2077`): 4 px
//! under the tag's chip, 270 px wide and as tall as its rows up to 290, a
//! card in the raised colour with a search over its rows: No Server Tag,
//! then each server that has a tag, with its icon, name and tag, the one
//! chosen lit. The search keeps the servers whose name or tag holds it. A
//! press beside it closes it, as Escape does before the editor (the app's
//! `intent::editor::escape`).

use iced::widget::{
    Column, button, container, mouse_area, opaque, pin, row, scrollable, space, stack, text,
};
use iced::{Alignment, Background, Border, Fill, Size};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::people::editor::{Choice, Editor, Intent as Edit, TagMenu};
use anomi_widgets::avatar::portrait;
use anomi_widgets::field::field;
use anomi_widgets::menu::place::MARGIN;
use anomi_widgets::tag::server_tag;
use yapfast_lang::tr;

/// The search field's id: it has the keys as the chooser opens.
pub const SEARCH: &str = "profile-editor-tag-search";

const WIDTH: f32 = 270.0;
const TALLEST: f32 = 290.0;
/// Its padding and spacing, and what they and the search take above the
/// rows (`56px + 36px * rows`).
const PAD: f32 = 8.0;
const SPACING: f32 = 6.0;
const ABOVE: f32 = 56.0;
const SEARCH_HEIGHT: f32 = 32.0;
const SEARCH_TEXT: f32 = 13.0;
/// A row, its corners and inset, its icon (`:2090`, `:2104`).
const ROW: f32 = 36.0;
const ROW_RADIUS: f32 = 6.0;
const INSET: f32 = 8.0;
const ICON: f32 = 20.0;
const TEXT: f32 = 13.0;
/// Below the chip (`y: 28px` from the 24 px chip), and the card's corners.
const BELOW: f32 = 4.0;
const RADIUS: f32 = 10.0;

/// The chooser over the editor's `room`, which the layer lays out `MARGIN`
/// in from the window, over a catcher that closes it.
pub fn view<'a>(
    editor: &'a Editor,
    menu: &'a TagMenu,
    room: Size,
    motion: Motion,
) -> Element<'a, Edit> {
    let height = TALLEST.min(ABOVE + ROW * (editor.tags.len() + 1) as f32);
    let x = (menu.at.x - MARGIN)
        .min(room.width - WIDTH - MARGIN)
        .max(0.0);
    let below = menu.at.y + menu.at.height + BELOW - MARGIN;
    let y = below.min(room.height - height - MARGIN).max(0.0);
    let search = field(tr!("Search"), &menu.search)
        .id(SEARCH)
        .on_input(Edit::TagSearch)
        .text_size(SEARCH_TEXT)
        .height(SEARCH_HEIGHT)
        .radius(8.0)
        .motion(motion)
        .view();
    let mut rows = Column::new().push(none(editor.tag_chosen.is_empty()));
    for choice in editor
        .tags
        .iter()
        .filter(|choice| choice.found(&menu.search))
    {
        rows = rows.push(server(choice, choice.id == editor.tag_chosen, motion));
    }
    let card = container(
        Column::new()
            .push(search)
            .push(scrollable(rows).height(Fill).boxed())
            .spacing(SPACING),
    )
    .padding(PAD)
    .width(WIDTH)
    .height(height)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().raised)),
        border: Border::default()
            .rounded(RADIUS)
            .width(1.0)
            .color(theme.palette().edge),
        ..container::Style::default()
    });
    let catcher = mouse_area(space().width(Fill).height(Fill)).on_press(Edit::CloseTagMenu);
    stack![catcher, pin(opaque(card)).x(x).y(y)]
        .width(Fill)
        .height(Fill)
        .boxed()
}

/// No Server Tag, in italic and muted, lit while no tag is worn, as the
/// Slint chooser shows it.
fn none<'a>(chosen: bool) -> Element<'a, Edit> {
    let words = text(tr!("No Server Tag"))
        .size(TEXT)
        .font(iced::Font {
            style: iced::font::Style::Italic,
            ..typography::UI
        })
        .style(catalog::text::muted);
    choice_row(
        container(words).padding([0.0, INSET]).center_y(ROW).boxed(),
        chosen,
        Edit::ChooseTag(String::new()),
    )
}

/// A server: its icon, its name cut to fit, its tag.
fn server<'a>(choice: &'a Choice, chosen: bool, motion: Motion) -> Element<'a, Edit> {
    let name = text(&choice.name)
        .size(TEXT)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Fill);
    let line = row![
        portrait(&choice.icon, ICON).view(),
        name,
        server_tag(&choice.tag, motion).view(),
    ]
    .spacing(INSET)
    .align_y(Alignment::Center)
    .padding([0.0, INSET])
    .height(ROW);
    choice_row(line.boxed(), chosen, Edit::ChooseTag(choice.id.clone()))
}

/// A row that lights under the pointer, and stays lit while chosen.
fn choice_row<'a>(content: Element<'a, Edit>, chosen: bool, intent: Edit) -> Element<'a, Edit> {
    button(content)
        .padding(0)
        .width(Fill)
        .height(ROW)
        .on_press(intent)
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let fill = match status {
                button::Status::Hovered | button::Status::Pressed => Some(palette.hover),
                _ if chosen => Some(palette.active),
                _ => None,
            };
            button::Style {
                background: fill.map(Background::Color),
                text_color: palette.text,
                border: Border::default().rounded(ROW_RADIUS),
                ..button::Style::default()
            }
        })
        .boxed()
}
