//! A server's Stickers settings: Discord's words, then a card per boost
//! level with its stickers as tiles and Upload Sticker after them. Task
//! X11's (docs/iced/tasks/settings.md). The Slint side: `StickersSection`
//! (`ui/server_expressions.slint:286`); the tile is [`tile`], the form is
//! [`dialog`].

mod dialog;
mod tile;

use iced::widget::{column, container, responsive, row, space, svg, text};
use iced::{Alignment, Border, Length, Size};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::stickers::{Card, Intent, Stickers};
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

pub use dialog::dialog;

const GAP: f32 = 16.0;
const INSIDE: f32 = 12.0;
const PAD: f32 = 16.0;
/// The narrowest a tile gets before the row takes one fewer.
const NARROWEST: f32 = 140.0;

/// The page: it scrolls in the frame, as Slint's did.
pub fn view(stickers: &Stickers, motion: Motion) -> impl View<Intent> + '_ {
    let mut page = column![
        text(tr!(
            "Stickers can be static (JPG, PNG) or animated (APNG, GIF). Stickers must be exactly 320 x 320 pixels and no larger than 512 KB. We will automatically resize static JPG, PNG and animated GIF stickers for you."
        ))
        .size(typography::size::LABEL)
        .style(catalog::text::muted),
    ]
    .spacing(GAP)
    .width(Length::Fill);
    for card in &stickers.cards {
        page = page.push(card_view(stickers, card, motion));
    }
    page
}

fn ground(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.overlay.scale_alpha(0.03).into()),
        border: Border::default()
            .rounded(metrics::RADIUS_CARD)
            .width(1)
            .color(palette.line),
        ..container::Style::default()
    }
}

/// A level's card: its title and slots over its tiles.
fn card_view<'a>(stickers: &'a Stickers, card: &'a Card, motion: Motion) -> Element<'a, Intent> {
    let mut title: Vec<Element<'a, Intent>> = Vec::with_capacity(4);
    if !card.unlocked {
        title.push(
            icon(Icon::Lock, 16.0)
                .style(|theme: &Theme, _| svg::Style {
                    color: Some(theme.palette().secondary),
                })
                .boxed(),
        );
    }
    title.push(
        text(&card.title)
            .size(16)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary)
            .boxed(),
    );
    title.push(space().width(Length::Fill).boxed());
    title.push(
        text(&card.status)
            .size(typography::size::SMALL)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::secondary)
            .boxed(),
    );
    let title = row(title).spacing(8).align_y(Alignment::Center);
    let upload = card.next && stickers.may_upload;
    let mut inner = column![title.boxed()].spacing(INSIDE);
    if !card.tiles.is_empty() || upload {
        inner = inner.push(
            responsive(move |room: Size| grid(stickers, card, upload, room.width, motion))
                .height(Length::Shrink)
                .boxed(),
        );
    }
    container(inner)
        .padding(PAD)
        .width(Length::Fill)
        .style(ground)
        .boxed()
}

/// The tiles in rows, at least [`NARROWEST`] each, sharing the card's width
/// so the last of a row ends on the margin the first starts at; Upload
/// Sticker after them.
fn grid<'a>(
    stickers: &'a Stickers,
    card: &'a Card,
    upload: bool,
    width: f32,
    motion: Motion,
) -> Element<'a, Intent> {
    let columns = (((width + INSIDE) / (NARROWEST + INSIDE)).floor() as usize).max(1);
    let side = (width - (columns - 1) as f32 * INSIDE) / columns as f32;
    let mut cells = card
        .tiles
        .iter()
        .map(|sticker| tile::tile(stickers, sticker, side, motion))
        .chain(upload.then(|| tile::upload(stickers, side, motion)));
    let mut rows: Vec<Element<'a, Intent>> = Vec::new();
    loop {
        let line: Vec<Element<'a, Intent>> = cells.by_ref().take(columns).collect();
        if line.is_empty() {
            break;
        }
        rows.push(row(line).spacing(INSIDE).boxed());
    }
    column(rows).spacing(INSIDE).boxed()
}

/// Its shots for `anomi-shots`: the cards, Bounce moving as the fake's
/// animated sticker does.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::Vm;
    use anomi_vm::fixtures::settings::stickers::{BOUNCE, page};
    use anomi_vm::kind::Expected;
    use anomi_widgets::picture::sample::bouncing;

    fn moving() -> Vm {
        let mut vm = page();
        let tiles = vm
            .settings
            .stickers
            .cards
            .iter_mut()
            .flat_map(|card| &mut card.tiles);
        for tile in tiles.filter(|tile| tile.id == BOUNCE) {
            tile.picture = Expected::Ready(bouncing(80, 10, [124, 196, 255]));
        }
        vm
    }
    vec![Shot {
        area: "ssettings",
        name: "stickers",
        size: GOLDEN,
        theme: "bonfire",
        vm: moving,
    }]
}
