//! The sections of the person's settings: who is signed in with a link to
//! edit the profile, a search over the sections, the pages in their groups,
//! and the update and build lines at the foot. The Slint side:
//! `SettingsNavigation` (`ui/yapfast/settings-nav.slint:10`): 266 px on the
//! chat's ground, the sections scrolling between the search and the foot.

use iced::widget::{container, row, space, text};
use iced::{Length, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::frame::{Frame, Intent, Page};
use anomi_widgets::avatar::portrait;
use anomi_widgets::field::field;
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

use super::page::{icon, label};
use super::stamp;
use crate::kit::{Ink, Leaf, WIDTH, chat_ground, mix, nav_row};

/// The ground's top inset and the gap between the parts (`padding-top: 16px;
/// spacing: 16px`).
const TOP: f32 = 16.0;
const GAP: f32 = 16.0;
/// The account's line: 56 px tall, 24 px in on the left and 16 on the right,
/// a 48 px picture and 12 px beside it (`:25`).
const ACCOUNT: f32 = 56.0;
const ACCOUNT_LEFT: f32 = 24.0;
const ACCOUNT_RIGHT: f32 = 16.0;
const PICTURE: f32 = 48.0;
const BESIDE: f32 = 12.0;
/// What the picture's neighbours stand 4 px apart (`spacing: 4px`).
const NAME_GAP: f32 = 4.0;
const EDIT: f32 = 20.0;
const EDIT_SIZE: f32 = 13.0;
const PENCIL: f32 = 12.0;
const EDIT_GAP: f32 = 4.0;
/// The search's line: 40 px tall, 16 in on each side (`:56`).
const SEARCH_SIDES: f32 = 16.0;
const SEARCH_LINE: f32 = 40.0;
const SEARCH_TEXT: f32 = typography::size::SMALL;
/// The entries: 16 px in on the sides and below, a heading 8 px in, 24 px
/// tall for the first and 40 for the rest, which stand 16 px lower.
const ENTRIES: f32 = 16.0;
const HEADING_INSET: f32 = 8.0;
const HEADING_FIRST: f32 = 24.0;
const HEADING_NEXT: f32 = 40.0;
const HEADING_GAP: f32 = 16.0;

/// The groups of pages, under their headings.
fn groups() -> [(&'static str, &'static [Page]); 3] {
    [
        (tr!("Account"), &[Page::Account]),
        (
            tr!("Experience"),
            &[
                Page::Voice,
                Page::Messages,
                Page::Notifications,
                Page::Sounds,
                Page::Appearance,
            ],
        ),
        (
            tr!("App Settings"),
            &[
                Page::Discord,
                Page::Games,
                Page::Keybinds,
                Page::Advanced,
                Page::About,
            ],
        ),
    ]
}

/// Whether `text` is what the search asks for (case aside).
fn matches(query: &str, text: &str) -> bool {
    query.is_empty() || text.to_lowercase().contains(&query.to_lowercase())
}

/// The account's line: the picture, the name and the way to edit the profile.
fn account<'a>(frame: &'a Frame, motion: Motion) -> Element<'a, Intent> {
    let name = text(frame.name.as_str())
        .size(typography::size::BODY)
        .font(typography::BOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    let mut lines: Vec<Element<'a, Intent>> = vec![name.boxed()];
    if frame.may_edit_profile {
        let mut edit = Leaf::new(tr!("Edit Profile"), motion, |theme: &Theme, lit: f32| {
            let palette = theme.palette();
            Ink {
                fill: iced::Color::TRANSPARENT,
                text: mix(palette.muted, palette.text, lit),
                glyph: palette.muted,
            }
        })
        .on_press(Intent::EditProfile);
        edit.icon = Some(Icon::Pencil);
        edit.trailing = true;
        edit.glyph = PENCIL;
        edit.size = EDIT_SIZE;
        edit.font = typography::UI;
        edit.gap = EDIT_GAP;
        edit.height = EDIT;
        edit.radius = 0.0;
        lines.push(edit.boxed());
    }
    let words = iced::widget::Column::with_children(lines)
        .spacing(NAME_GAP)
        .align_x(alignment::Horizontal::Left);
    row![
        container(portrait(&frame.avatar, PICTURE).view()).center_y(Length::Fill),
        container(words).center_y(Length::Fill)
    ]
    .spacing(BESIDE)
    .padding(Padding {
        left: ACCOUNT_LEFT,
        right: ACCOUNT_RIGHT,
        ..Padding::ZERO
    })
    .height(ACCOUNT)
    .boxed()
}

/// The pages in their groups, the headings only while nothing is searched.
fn entries<'a>(frame: &'a Frame, motion: Motion) -> Element<'a, Intent> {
    let searching = !frame.query.is_empty();
    let mut rows: Vec<Element<'a, Intent>> = Vec::new();
    for (index, (name, pages)) in groups().into_iter().enumerate() {
        if !searching {
            rows.push(
                container(
                    text(name)
                        .size(typography::size::CAPTION)
                        .wrapping(text::Wrapping::None)
                        .style(catalog::text::muted),
                )
                .padding(Padding {
                    left: HEADING_INSET,
                    top: if index == 0 { 0.0 } else { HEADING_GAP },
                    ..Padding::ZERO
                })
                .height(if index == 0 {
                    HEADING_FIRST
                } else {
                    HEADING_NEXT
                })
                .align_y(alignment::Vertical::Center)
                .boxed(),
            );
        }
        for &page in pages {
            let words = label(page);
            if (page.needs_discord() && !frame.with_discord) || !matches(&frame.query, words) {
                continue;
            }
            rows.push(nav_row(
                icon(page),
                words,
                frame.page == page,
                false,
                frame.fresh.contains(&page),
                Intent::Choose(page),
                motion,
            ));
        }
    }
    let list = iced::widget::Column::with_children(rows).padding(Padding {
        left: ENTRIES,
        right: ENTRIES,
        bottom: ENTRIES,
        top: 0.0,
    });
    scroll_view(list, motion).boxed()
}

pub fn navigation<'a>(frame: &'a Frame, motion: Motion) -> Element<'a, Intent> {
    let search = container(
        field(tr!("Search settings"), &frame.query)
            .text_size(SEARCH_TEXT)
            .height(SEARCH_LINE)
            .motion(motion)
            .on_input(Intent::Search)
            .view(),
    )
    .padding(Padding::from([0.0, SEARCH_SIDES]))
    .height(SEARCH_LINE);
    let mut parts: Vec<Element<'a, Intent>> = vec![
        account(frame, motion),
        search.boxed(),
        entries(frame, motion),
    ];
    if let Some(latest) = &frame.update {
        parts.push(stamp::update(latest, motion));
    } else {
        // Slint's notice is 0 px tall but keeps its gap: the sections end that
        // much higher.
        parts.push(space().height(0.0).boxed());
    }
    parts.extend(stamp::stamp(frame, motion));
    container(
        iced::widget::Column::with_children(parts)
            .spacing(GAP)
            .width(Length::Fill)
            .padding(Padding::ZERO.top(TOP)),
    )
    .width(Length::Fixed(WIDTH))
    .height(Length::Fill)
    .style(chat_ground)
    .boxed()
}
