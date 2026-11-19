//! A rail folder's settings, as Discord's dialog
//! (`discord-folder-settings-dialog.png` among the owner's references; the
//! Slint window has none): "Folder settings" with the X; "Folder name" over
//! a field whose placeholder is the name the rail gives the folder without
//! one; "Folder colour" over two large swatches, the default colour (the
//! one the rail draws a folder without its own in) and a custom one with a
//! pencil, which opens the profile editor's colour picker under it
//! ([`picker`], `Layer::FolderColour`); under them Discord's twenty in two
//! rows of a role's swatches (`anomi_settings`' role editor); and Done, as
//! wide as the card. The app focuses the field as it opens ([`NAME`]);
//! Enter is Done; Escape, the X and a press beside the card leave the
//! folder as it was. The shell draws it over its backdrop
//! (`Layer::FolderSettings`).

use iced::widget::{Id, column, container, row, space, stack, svg, text};
use iced::{Border, Color, Fill};

use anomi_people::editor::colour;
use anomi_settings::server::roles::editor::colors::square;
use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_vm::people::editor_preview;
use anomi_vm::rail::FOLDER_COLOR;
use anomi_vm::rail::folder::{FolderSettings, Intent, rgb};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::icon::icon;
use anomi_widgets::menu::opener;
use yapfast_lang::tr;

use crate::places::leave::{card, enter};

/// The name's field, which the app focuses as the dialog opens.
pub const NAME: &str = "folder-name";

/// Discord's dialog: 480 px wide, its parts 24 px in from its edges.
const WIDTH: f32 = 480.0;
const PAD: f32 = 24.0;
/// Under the title; between a label and what it names; between the name
/// and the colour; before Done.
const TITLE_GAP: f32 = 20.0;
const LABEL_GAP: f32 = 8.0;
const SECTION_GAP: f32 = 16.0;
const DONE_GAP: f32 = 24.0;
/// The field and Done are 40 px tall.
const FIELD: f32 = 40.0;
const DONE: f32 = 40.0;
/// The two large swatches, the twenty small ones and the room between.
const LARGE: (f32, f32) = (70.0, 50.0);
const SWATCH: f32 = 20.0;
const GAP: f32 = 10.0;
/// The pencil in the custom swatch's top right corner, this far in.
const PENCIL: f32 = 14.0;
const PENCIL_INSET: f32 = 5.0;
/// The custom swatch's corners and rim while it has no colour, as a
/// role's swatches have their corners.
const RADIUS: f32 = 4.0;
const RIM: f32 = 1.0;
/// Over a colour lighter than this, the pencil is dark.
const LIGHT: f32 = 0.6;

/// The dialog's card.
pub fn view(settings: &FolderSettings, motion: Motion) -> impl View<Intent> + '_ {
    let name = field(&settings.placeholder, &settings.name)
        .id(Id::new(NAME))
        .on_input(Intent::Name)
        .on_submit(Intent::Done)
        .height(FIELD)
        .motion(motion)
        .view();
    let done = action_button(tr!("Done"), Kind::Primary, motion)
        .width(Fill)
        .height(DONE)
        .on_press(Intent::Done);
    let body = column![
        card::title_row(tr!("Folder settings"), Intent::Close, motion),
        space().height(TITLE_GAP),
        label(tr!("Folder name")),
        space().height(LABEL_GAP),
        name,
        space().height(SECTION_GAP),
        label(tr!("Folder colour")),
        space().height(LABEL_GAP),
        large(settings),
        space().height(GAP),
        presets(settings),
        space().height(DONE_GAP),
        done,
    ]
    .width(Fill);
    let framed = container(body)
        .width(WIDTH)
        .padding(PAD)
        .style(card::dialog);
    enter(framed, Some(Intent::Done))
}

/// The custom colour's picker: the profile editor's card, under the custom
/// swatch (`Layer::FolderColour`).
pub fn picker(settings: &FolderSettings, motion: Motion) -> impl View<Intent> + '_ {
    colour::view(&settings.pick, motion).map(|asked| match asked {
        editor_preview::Intent::Colour(change) => Intent::Pick(change),
        _ => Intent::Ignored,
    })
}

/// A label over a part, as Discord's.
fn label(words: &str) -> Element<'_, Intent> {
    text(words)
        .size(typography::size::SMALL)
        .font(typography::MEDIUM)
        .style(catalog::text::secondary)
        .boxed()
}

/// The default colour, ticked while the folder has none, and the custom
/// one.
fn large(settings: &FolderSettings) -> Element<'_, Intent> {
    let default = square(
        FOLDER_COLOR,
        LARGE,
        true,
        settings.colour.is_none(),
        Some(Intent::Colour(None)),
    );
    let custom = opener(custom(settings)).on_press(|at| Intent::Custom(Spot::below(at)));
    row![default, custom].spacing(GAP).boxed()
}

/// The custom swatch: the colour picked, ticked while it's the one chosen,
/// or the sunken ground in a rim before one is; the pencil in its corner.
/// The opener around it takes the press.
fn custom<'a>(settings: &FolderSettings) -> Element<'a, Intent> {
    let (width, height) = LARGE;
    let face = match settings.custom {
        Some(value) => square::<Intent>(rgb(value), LARGE, true, settings.custom_chosen(), None),
        None => container(space())
            .width(width)
            .height(height)
            .style(|theme: &Theme| {
                let palette = theme.palette();
                container::Style {
                    background: Some(palette.sunken.into()),
                    border: Border::default()
                        .rounded(RADIUS)
                        .width(RIM)
                        .color(palette.edge),
                    ..container::Style::default()
                }
            })
            .boxed(),
    };
    let ink = settings.custom.map(|value| ink_on(rgb(value)));
    let pencil = icon(Icon::Pencil, PENCIL).style(move |theme: &Theme, _| svg::Style {
        color: Some(ink.unwrap_or(theme.palette().text)),
    });
    let corner = container(pencil)
        .padding(PENCIL_INSET)
        .align_right(Fill)
        .height(Fill);
    stack![face, corner].boxed()
}

/// The pencil's ink over `colour`: dark over a light one.
fn ink_on(colour: Color) -> Color {
    let light = 0.299 * colour.r + 0.587 * colour.g + 0.114 * colour.b;
    if light > LIGHT {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

/// Discord's twenty: ten bright, then ten dark.
fn presets(settings: &FolderSettings) -> Element<'_, Intent> {
    let (bright, dark) = settings.presets.split_at(settings.presets.len().min(10));
    column![
        swatches(bright, settings.colour),
        swatches(dark, settings.colour)
    ]
    .spacing(GAP)
    .boxed()
}

/// One row of swatches, `chosen` ticked.
fn swatches<'a>(colours: &[u32], chosen: Option<u32>) -> Element<'a, Intent> {
    row(colours.iter().map(|&value| {
        let press = Some(Intent::Colour(Some(value)));
        square(
            rgb(value),
            (SWATCH, SWATCH),
            false,
            chosen == Some(value),
            press,
        )
    }))
    .spacing(GAP)
    .boxed()
}
