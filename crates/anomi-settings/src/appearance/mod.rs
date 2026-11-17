//! The Appearance settings page: the window's colours from the twelve themes
//! Yapfast comes with and the person's own, the language, a font of the
//! person's own, the title bar, and the switches for how people's looks
//! show. A click shows a theme at once; New theme opens the editor (S07's
//! `editor::view`, drawn instead of the page while a theme is edited). The
//! Slint side: `AppearanceSettings` (`ui/settings.slint:1371`).

pub mod editor;
mod font_row;
mod theme_tile;

use iced::widget::{column, container, row, rule, space, text};
use iced::{Border, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::appearance::{Appearance, Intent, Look};
use anomi_vm::settings::{self, theme_editor::ThemeEditor};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

use crate::kit::{heading, link, picker, switch};

/// Between the page's parts and its switches (`spacing: 16px`), and between a
/// group's heading and what is under it (`8px`).
const SPACING: f32 = 16.0;
const GROUP: f32 = 8.0;
/// The language picker's width.
const PICKER: f32 = 260.0;

/// The page, which `frame::page` places. While a theme is edited it draws
/// nothing: the window's owner calls [`with_editor`] then (see the gap S06
/// left in `~/.cache/anomi-iced/state/gaps/S06.md`).
pub fn view(appearance: &Appearance, motion: Motion) -> impl View<settings::Intent> + '_ {
    if appearance.editing {
        space().boxed()
    } else {
        page(appearance, motion)
            .map(settings::Intent::Appearance)
            .boxed()
    }
}

/// The page, or the editor in its place while a theme is edited.
pub fn with_editor<'a>(
    appearance: &'a Appearance,
    editor: &'a ThemeEditor,
    motion: Motion,
) -> impl View<settings::Intent> + 'a {
    if appearance.editing {
        self::editor::view(editor, motion)
            .map(settings::Intent::ThemeEditor)
            .boxed()
    } else {
        view(appearance, motion).boxed()
    }
}

/// The page's own parts, with the editor out of it.
pub fn page(appearance: &Appearance, motion: Motion) -> Element<'_, Intent> {
    let mut parts = vec![
        themes(appearance, motion),
        language(appearance),
        fonts(appearance, motion),
    ];
    if appearance.title_bar_choice {
        parts.push(
            switch(
                tr!("Use the system title bar"),
                appearance.system_title_bar,
                motion,
            )
            .description(tr!(
                "Your desktop draws the window's title bar and buttons, as on its other windows. Off, Yapfast draws its own."
            ))
            .on_toggle(Intent::SystemTitleBar)
            .view(),
        );
    }
    parts.push(looks(appearance, motion));
    column(parts).spacing(SPACING).width(Length::Fill).boxed()
}

/// The heading with an action or two at its end.
fn heading_row<'a>(words: &'a str, actions: Vec<Element<'a, Intent>>) -> Element<'a, Intent> {
    let mut line = row![container(heading(words)).width(Length::Fill)]
        .spacing(GROUP)
        .align_y(alignment::Vertical::Center);
    for action in actions {
        line = line.push(action);
    }
    line.boxed()
}

fn note(words: &str, wrong: bool) -> Element<'_, Intent> {
    text(words)
        .size(13)
        .width(Length::Fill)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            iced::widget::text::Style {
                color: Some(if wrong {
                    palette.danger_text()
                } else {
                    palette.secondary
                }),
                ..Default::default()
            }
        })
        .boxed()
}

fn themes(page: &Appearance, motion: Motion) -> Element<'_, Intent> {
    let import = action_button(tr!("Import"), Kind::Secondary, motion)
        .small()
        .leading(Icon::Folder)
        .on_press(Intent::Import);
    let new = action_button(tr!("New theme"), Kind::Secondary, motion)
        .small()
        .leading(Icon::CirclePlus)
        .on_press(Intent::NewTheme);
    let mut parts = vec![heading_row(tr!("THEME"), vec![import.boxed(), new.boxed()])];
    if !page.note.is_empty() {
        parts.push(note(&page.note, page.note_wrong));
    }
    let current = page.themes.iter().position(|card| card.id == page.theme_id);
    let tiles = page
        .themes
        .iter()
        .enumerate()
        .map(|(index, card)| theme_tile::tile(card, current == Some(index), motion))
        .collect();
    parts.push(theme_tile::grid(tiles, current, motion));
    column(parts).spacing(GROUP).width(Length::Fill).boxed()
}

fn language(page: &Appearance) -> Element<'_, Intent> {
    column![
        heading(tr!("LANGUAGE")),
        container(
            container(picker(
                &page.languages,
                page.language,
                Some(Intent::Language as fn(usize) -> Intent),
            ))
            .width(PICKER)
        )
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Left),
    ]
    .spacing(GROUP)
    .boxed()
}

fn fonts(page: &Appearance, motion: Motion) -> Element<'_, Intent> {
    let adding = page.font_importing;
    let add = action_button(
        if adding {
            tr!("Adding…")
        } else {
            tr!("Add font files")
        },
        Kind::Secondary,
        motion,
    )
    .small()
    .leading(Icon::CirclePlus)
    .enabled(!adding)
    .on_press(Intent::AddFonts);
    let mut parts = vec![
        heading_row(tr!("FONT"), vec![add.boxed()]),
        typography::label(tr!(
            "Draw the whole interface in a font of your own: add its TTF, OTF, TTC or WOFF files, each weight and italic you have. Code and emoji keep their fonts."
        ))
        .width(Length::Fill)
        .boxed(),
    ];
    if !page.font_files.is_empty() {
        // The files in a rimmed card, a line between each.
        let mut list: iced::widget::Column<Element<'_, Intent>> = column![];
        for (index, file) in page.font_files.iter().enumerate() {
            if index > 0 {
                list = list.push(rule::horizontal(1).style(catalog::rule::line).boxed());
            }
            list = list.push(font_row::font_row(file, index, motion));
        }
        parts.push(
            container(list)
                .width(Length::Fill)
                .style(|theme: &Theme| {
                    container::Style::default().border(Border {
                        width: 1.0,
                        color: theme.palette().line,
                        radius: 8.into(),
                    })
                })
                .boxed(),
        );
    }
    if !page.font_note.is_empty() {
        parts.push(note(&page.font_note, true));
    }
    if !page.font_files.is_empty() {
        parts.push(
            link(tr!("Back to the bundled font"), motion)
                .small(true)
                .on_press(Intent::ResetFont)
                .view(),
        );
    }
    column(parts).spacing(GROUP).width(Length::Fill).boxed()
}

/// The heading 8 px over its switches, which keep the page's 16 px apart.
fn looks(page: &Appearance, motion: Motion) -> Element<'_, Intent> {
    let one = |look: Look, on: bool, name: &'static str, said: &'static str| {
        switch(name, on, motion)
            .description(said)
            .on_toggle(move |on| Intent::Look(look, on))
            .view()
    };
    let switches = column![
        one(
            Look::ProfileEffects,
            page.profile_effects,
            tr!("Play profile effects"),
            tr!("The animations some people put over their profile cards."),
        ),
        one(
            Look::Decorations,
            page.decorations,
            tr!("Show avatar decorations"),
            tr!("The pictures around people's avatars."),
        ),
        one(
            Look::Nameplates,
            page.nameplates,
            tr!("Show nameplates"),
            tr!(
                "The pictures Discord members equip behind their rows in the member list and on your account card."
            ),
        ),
        one(
            Look::Moving,
            page.moving,
            tr!("Animate decorations and nameplates"),
            tr!("They play when pointed at, as in Discord. Off, they hold still."),
        ),
        one(
            Look::NameStyles,
            page.name_styles,
            tr!("Show name styles"),
            tr!("The fonts and colours people give their display names."),
        ),
        one(
            Look::ServerTags,
            page.server_tags,
            tr!("Show server tags"),
            tr!("The tag and its badge beside people's names."),
        ),
        one(
            Look::ProfileFrames,
            page.profile_frames,
            tr!("Show profile frames"),
            tr!("The frames some people put around their profile cards."),
        ),
    ]
    .spacing(SPACING);
    column![heading(tr!("PROFILES")), switches]
        .spacing(GROUP)
        .boxed()
}

/// Its shots for `anomi-shots`: `settings/appearance.png` and
/// `light/settings_appearance.png` as the Slint app showed them, and the page
/// with a theme of the person's own and two font files. A shot can't scroll
/// the page, so `appearance_scrolled` and `appearance_scrolled_more` have
/// none (see `~/.cache/anomi-iced/state/gaps/S06.md`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::settings::appearance::{latte, page, with_fonts};

    use crate::shots::{GOLDEN, Shot};

    [
        (
            "settings",
            "appearance",
            "bonfire",
            page as fn() -> anomi_vm::Vm,
        ),
        ("settings", "appearance_fonts", "bonfire", with_fonts),
        ("light", "settings_appearance", "catppuccin-latte", latte),
    ]
    .into_iter()
    .map(|(area, name, theme, vm)| Shot {
        area,
        name,
        size: GOLDEN,
        theme,
        vm,
    })
    .collect()
}
