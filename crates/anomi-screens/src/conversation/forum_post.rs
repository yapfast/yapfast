//! A forum post's bar, as Discord shows it above the post open as the
//! conversation: React to post at its left, Follow (or Following) and Copy
//! link at its right, on the chat's ground over a line. The Slint side:
//! `ui/yapfast.slint:3554`-`:3614`.
//!
//! Each is a [`pill`] 31 px tall with 8 px corners on the raised ground,
//! fading to `Theme.field` under the pointer, an 18 px icon and 14 px
//! semibold text 12 px from its sides, in its middle.

mod pill;

use iced::widget::{column, container, row, rule, space, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::conversation::ForumPost;
use anomi_vm::{Intent, shell};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::menu::opener;
use yapfast_lang::tr;

use pill::pill;

/// The bar's height with its line, the pills' height, their inset from the
/// bar's top and ends, the gap between the right ones, and their padding
/// and parts (`:3555`, `:3559`, `:3564`, `:3582`, `:3589`, `:3602`). The
/// pills sit 7 px down, not in the middle of the 44 px over the line, so
/// their edges fall on whole pixels.
pub(super) const BAR: f32 = 45.0;
const PILL: f32 = 31.0;
const TOP: f32 = 7.0;
const INSET: f32 = 16.0;
const GAP: f32 = 4.0;
const PAD: f32 = 12.0;
const PARTS: f32 = 8.0;
const PARTS_FOLLOW: f32 = 6.0;
const GLYPH: f32 = 18.0;
const LINK: f32 = 32.0;

/// The bar over a forum post.
pub fn view<'a>(post: ForumPost, motion: Motion) -> impl View<Intent> + 'a {
    // The opener tells where the pill is, for the picker to open by it;
    // it acts for the pill, which only looks the part.
    let react = post.reacts.then(|| {
        let face = pill(parts(Icon::Smile, tr!("React to post"), PARTS), motion);
        opener(face)
            .on_press(|bounds| Intent::Shell(shell::Intent::ReactToPost(bounds)))
            .boxed()
    });
    let follow = post.following.map(|following| {
        let (glyph, label) = if following {
            (Icon::Check, tr!("Following"))
        } else {
            (Icon::Plus, tr!("Follow"))
        };
        pill(parts(glyph, label, PARTS_FOLLOW), motion)
            .on_press(Intent::Shell(shell::Intent::FollowPost(!following)))
            .boxed()
    });
    let link = pill(
        container(tinted(Icon::Link)).center_x(LINK).center_y(PILL),
        motion,
    )
    .on_press(Intent::Shell(shell::Intent::CopyPostLink));
    let right = row![]
        .extend(follow)
        .push(link.boxed())
        .spacing(GAP)
        .align_y(Alignment::Center);
    let bar = row![]
        .extend(react)
        .push(space().width(Fill).boxed())
        .push(right.boxed())
        .padding(Padding::new(INSET).top(TOP).bottom(0.0))
        .align_y(Alignment::Center);
    column![
        container(bar).height(BAR - 1.0),
        rule::horizontal(1).style(catalog::rule::outline_chat),
    ]
    .width(Fill)
}

/// A pill's icon and words, `gap` apart, 12 px in from its sides and in
/// the middle of its height.
fn parts<'a>(glyph: Icon, label: &'a str, gap: f32) -> impl View<Intent> + 'a {
    let parts = row![
        tinted(glyph),
        text(label)
            .size(typography::size::SMALL)
            .font(typography::SEMIBOLD)
            .style(catalog::text::primary),
    ]
    .spacing(gap)
    .align_y(Alignment::Center);
    container(parts).padding([0.0, PAD]).center_y(PILL)
}

fn tinted<'a>(glyph: Icon) -> impl View<Intent> + 'a {
    icon(glyph, GLYPH).style(|theme: &Theme, _status| iced::widget::svg::Style {
        color: Some(theme.palette().text),
    })
}
