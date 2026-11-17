//! The strip under a card: "This emoji is from", then the server with its
//! icon, name and counts, which opens on a click (`ui/messages.slint:1079`,
//! `:957`).

use iced::widget::{Column, Row, button, container, text};
use iced::{Alignment, Background, Border, Fill, Font};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::cards::{Intent, Origin};
use anomi_vm::kind::Expected;
use anomi_widgets::picture::{Fit, picture};

const ICON: f32 = 32.0;
const NAME_LINE: f32 = 20.0;
const COUNT_LINE: f32 = 16.0;

/// Which card the strip is under: they draw the server's icon apart.
#[derive(Clone, Copy)]
pub enum Look {
    /// A square with 8 px corners, the icon covering it and the letter bold
    /// on the field's ground (`EmojiCardView`).
    Emoji,
    /// The icon whole in its square, the letter semibold in a circle
    /// (`MessageStickerCardView`).
    Sticker,
}

impl Look {
    fn radius(self) -> f32 {
        match self {
            Look::Emoji => metrics::RADIUS_CARD,
            Look::Sticker => ICON / 2.0,
        }
    }

    fn initial_font(self) -> Font {
        match self {
            Look::Emoji => typography::BOLD,
            Look::Sticker => typography::SEMIBOLD,
        }
    }
}

pub fn view<'a>(
    origin: &'a Origin,
    heading: &'a str,
    look: Look,
    motion: Motion,
) -> Element<'a, Intent> {
    let icon: Element<'a, Intent> = match (&origin.icon, look) {
        (Expected::Ready(handle), Look::Emoji) => picture(handle.clone())
            .side(ICON)
            .fit(Fit::Cover)
            .radius(metrics::RADIUS_CARD)
            .motion(motion)
            .boxed(),
        (Expected::Ready(handle), Look::Sticker) => picture(handle.clone())
            .side(ICON)
            .fit(Fit::Contain)
            .motion(motion)
            .boxed(),
        _ => container(
            text(origin.initial.as_str())
                .size(typography::size::SMALL)
                .font(look.initial_font())
                .style(catalog::text::primary),
        )
        .center(ICON)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().field)),
            border: Border::default().rounded(look.radius()),
            ..container::Style::default()
        })
        .boxed(),
    };
    let one_line = |line: f32, content: iced::widget::Text<'a, Theme>| {
        container(
            content
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End),
        )
        .width(Fill)
        .height(line)
        .align_y(Alignment::Center)
    };
    let counts = origin.counts();
    let mut lines = Column::<Element<'a, Intent>>::new()
        .spacing(2)
        .width(Fill)
        .push(
            one_line(
                NAME_LINE,
                text(origin.name.as_str())
                    .size(typography::size::TITLE)
                    .font(typography::SEMIBOLD)
                    .style(catalog::text::primary),
            )
            .boxed(),
        );
    if !counts.is_empty() {
        lines = lines.push(
            one_line(
                COUNT_LINE,
                text(counts)
                    .size(typography::size::CAPTION)
                    .style(catalog::text::primary),
            )
            .boxed(),
        );
    }
    // The icon centres against the two lines beside it; the hover sits on
    // them and no more.
    let row = button(
        Row::<Element<'a, Intent>>::new()
            .push(icon)
            .push(lines.boxed())
            .spacing(metrics::SPACE_S)
            .align_y(Alignment::Center)
            .width(Fill),
    )
    .width(Fill)
    .padding(0)
    .on_press(Intent::OpenServer(origin.id.clone()))
    .style(catalog::button::plain)
    .boxed();
    container(
        Column::<Element<'a, Intent>>::new()
            .push(
                text(heading)
                    .size(typography::size::CAPTION)
                    .font(typography::MEDIUM)
                    .style(catalog::text::muted)
                    .wrapping(text::Wrapping::None)
                    .ellipsis(text::Ellipsis::End)
                    .boxed(),
            )
            .push(row)
            .spacing(metrics::SPACE_S),
    )
    .width(Fill)
    .padding(metrics::SPACE_L)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().panel)),
        border: Border {
            radius: iced::border::bottom(metrics::RADIUS_CARD - 1.0),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .boxed()
}
