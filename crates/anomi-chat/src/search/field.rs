//! The search field of a conversation's header (`HeaderSearch`,
//! `ui/header.slint:65`): Discord's box, 240 px with a faint rim, 300 px
//! while it's used, and a magnifier alone, 32 px, in a header short of room
//! until it is. Enter searches; the cross ends the search; Escape in the
//! field leaves it for the composer.
//!
//! The header's row places it (N07 leaves it room: `Tools::search`).

use iced::widget::{container, stack, svg};
use iced::{Color, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::chat::search::{Intent, Search};
use anomi_widgets::button::icon_button;
use anomi_widgets::field::field;
use anomi_widgets::focus;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::tip::Side;
use yapfast_lang::tr;

use super::grow::Grow;

/// The box's height and corner (`ClientStyle.enabled`).
const HEIGHT: f32 = 30.0;
const RADIUS: f32 = 8.0;
/// Its width at rest, in use, and as the magnifier alone.
pub const REST: f32 = 240.0;
pub const OPEN: f32 = 300.0;
pub const COLLAPSED: f32 = 32.0;
/// The text's inset from the left (`x: 8px`).
const PAD: f32 = 8.0;
/// The cross: 24 px with a 14 px glyph, `26px` from the right end.
const CROSS: f32 = 24.0;
const CROSS_GLYPH: f32 = 14.0;
const CROSS_FROM_RIGHT: f32 = 2.0;
/// The magnifier: 20 px, `x: width - 28`, so 8 px from the end; centred
/// when collapsed (`ui/header.slint:139`).
const MAGNIFIER: f32 = 20.0;
const MAGNIFIER_FROM_RIGHT: f32 = 8.0;

/// The width the field is heading for (`natural`).
pub fn natural(search: &Search) -> f32 {
    if search.compact && !search.focused && search.text.is_empty() {
        COLLAPSED
    } else if search.focused || !search.text.is_empty() {
        OPEN
    } else {
        REST
    }
}

pub fn view(search: &Search, motion: Motion) -> impl View<Intent> + '_ {
    let collapsed = natural(search) == COLLAPSED;
    let typed = !search.text.is_empty();
    // The field's own trailing picture stays invisible: it only keeps the
    // text clear of the magnifier and the cross. The magnifier is a layer
    // of its own, placed as `HeaderSearch` places it.
    let tint = Some(Color::TRANSPARENT);
    let placeholder = if collapsed {
        ""
    } else {
        search.placeholder.as_str()
    };
    let input = field(placeholder, &search.text)
        .id(focus::SEARCH)
        .on_input(Intent::Edit)
        .on_submit(Intent::Submit)
        .on_focus(Intent::Focus)
        .text_size(typography::size::SMALL)
        .pad(PAD)
        .height(HEIGHT)
        .radius(RADIUS)
        .trailing(Icon::Search, tint)
        .motion(motion)
        .view();

    let mut layers: Vec<Element<'_, Intent>> = vec![input];
    if !typed {
        let glass = icon(Icon::Search, MAGNIFIER).style(|theme: &Theme, _| svg::Style {
            color: Some(theme.palette().secondary),
        });
        let at = container(glass).center_y(HEIGHT).width(Fill);
        layers.push(if collapsed {
            at.center_x(Fill).boxed()
        } else {
            at.align_right(Fill)
                .padding(Padding::default().right(MAGNIFIER_FROM_RIGHT))
                .boxed()
        });
    }
    if typed {
        layers.push(
            container(
                icon_button(Icon::X, tr!("Clear search"), motion)
                    .size(CROSS)
                    .glyph(CROSS_GLYPH)
                    .tip_side(Side::Below)
                    .on_press(Intent::Clear)
                    .view(),
            )
            .padding(Padding::default().right(CROSS_FROM_RIGHT))
            .align_right(Fill)
            .center_y(HEIGHT)
            .boxed(),
        );
    }
    let content: Element<'_, Intent> = stack(layers).width(Fill).height(HEIGHT).boxed();
    Grow::new(content, natural(search), motion).on_escape(
        Intent::Leave { cleared: true },
        Intent::Leave { cleared: false },
        typed,
    )
}
