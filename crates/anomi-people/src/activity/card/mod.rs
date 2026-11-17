//! One of someone's activities as a profile shows it (`ActivityCard`,
//! `ui/people.slint:780`): its heading ("PLAYING", "LISTENING TO SPOTIFY")
//! with the more button of Discord's menu, a 64 px picture with a small one
//! over its corner, the words ([`words`]) beside it, and "Watch stream" for a
//! stream. It is drawn on whatever the profile's theme sets ([`Look`]), and
//! counts its times from the `now` it is given.

mod words;

use iced::widget::{Column, Row, container, hover, pin, row, stack, text};
use iced::{Alignment, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, metrics, typography};
use anomi_vm::kind::Expected;
use anomi_vm::menu::Spot;
use anomi_vm::people::activity::card::{Card, Intent, Look};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::menu::opener;
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

use super::picture;

const HEADING: f32 = 16.0;
const MORE: f32 = 20.0;
const MORE_GLYPH: f32 = 14.0;
const LARGE: f32 = 64.0;
const SMALL: f32 = 22.0;
/// The small picture's ring, and where it sits on the large one.
const RING: f32 = 26.0;
const RING_AT: f32 = 44.0;
/// Between the heading and the line under it (`ui/people.slint:805`).
const BODY_GAP: f32 = 8.0;
/// Between the heading's text and its more button (`:807`).
const HEADING_GAP: f32 = 6.0;
/// Between the large picture and the words (`:817`); the ring hangs
/// `RING_AT + RING - LARGE` = 6 px of it, so the stack is spaced by the rest.
const LINE_GAP: f32 = 12.0;
const RING_HANG: f32 = RING_AT + RING - LARGE;

/// The colours a card is drawn in once the profile's [`Look`] has had its
/// say.
#[derive(Clone, Copy)]
pub(super) struct Inks {
    pub ink: iced::Color,
    pub ink2: iced::Color,
    pub ink3: iced::Color,
    pub ground: iced::Color,
    pub track: iced::Color,
}

impl Inks {
    fn new(look: &Look, palette: &Palette) -> Self {
        Self {
            ink: look.ink.unwrap_or(palette.text),
            ink2: look.ink2.unwrap_or(palette.secondary),
            ink3: look.ink3.unwrap_or(palette.muted),
            ground: look.ground.unwrap_or(palette.sidebar),
            track: look.track.unwrap_or(palette.raised),
        }
    }
}

/// The card at `now` (seconds as `Card::start` counts them). `menu` is
/// Discord's more button, on anyone's card but a bot's, never in a preview.
/// Colours come from the theme but where `look` sets them: each part reads
/// them through `Inks::new` on the theme that draws it.
pub fn view<'a>(
    card: &'a Card,
    now: i64,
    look: &'a Look,
    menu: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut body = Column::<Element<'a, Intent>>::new().spacing(BODY_GAP);
    body = body.push(heading(card, look, menu, motion));
    let mut line = Row::<Element<'a, Intent>>::new()
        .spacing(LINE_GAP - RING_HANG)
        .align_y(Alignment::Start);
    if !matches!(card.large, Expected::None) {
        line = line.push(large(card, look, motion));
    }
    line = line.push(
        // Beside the picture at its top; centred when shorter than it.
        container(words::view(card, now, look, motion))
            .width(Fill)
            .height(Length::Shrink.min(LARGE))
            .align_y(Alignment::Center)
            .boxed(),
    );
    body = body.push(line.boxed());
    if !card.url.is_empty() {
        body = body.push(
            action_button(tr!("Watch stream"), Kind::Secondary, motion)
                .small()
                .on_press(Intent::Link(card.url.clone()))
                .boxed(),
        );
    }
    body.width(Fill).boxed()
}

/// The heading in the third ink, with the more button at its end.
fn heading<'a>(card: &'a Card, look: &'a Look, menu: bool, motion: Motion) -> Element<'a, Intent> {
    let look = *look;
    let label = text(card.heading.as_str())
        .size(typography::size::CAPTION)
        .font(typography::BOLD)
        .style(move |theme: &Theme| text::Style {
            color: Some(Inks::new(&look, theme.palette()).ink3),
            ..text::Style::default()
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Fill);
    let mut line = row![label.boxed()]
        .spacing(HEADING_GAP)
        .align_y(Alignment::Center);
    if menu {
        line = line.push(more(look, motion));
    }
    container(line).height(HEADING).center_y(HEADING).boxed()
}

/// Discord's more button: 20 px with a 14 px dots glyph, lit under the
/// pointer. The opener tells where it is; the button sends nothing itself.
fn more<'a>(look: Look, motion: Motion) -> Element<'a, Intent> {
    let face = move |hovered: bool| {
        let glyph = icon(Icon::Ellipsis, MORE_GLYPH).style(move |theme: &Theme, _| {
            iced::widget::svg::Style {
                color: Some(Inks::new(&look, theme.palette()).ink2),
            }
        });
        container(glyph)
            .center(MORE)
            .style(move |theme: &Theme| container::Style {
                background: hovered.then(|| theme.palette().lift(0.12).into()),
                border: iced::Border::default().rounded(metrics::RADIUS_FIELD),
                ..container::Style::default()
            })
    };
    tip(
        opener(hover(face(false), face(true))).on_press(|at| Intent::More(Spot::below(at))),
        tr!("More"),
        Side::Above,
        motion,
    )
    .boxed()
}

/// The large picture with the small one ringed over its corner.
fn large<'a>(card: &'a Card, look: &Look, motion: Motion) -> Element<'a, Intent> {
    let look = *look;
    let large = picture::view(&card.large, LARGE, LARGE / 6.0, motion);
    if matches!(card.small, Expected::None) {
        return container(large)
            .width(LARGE + RING_HANG)
            .height(LARGE)
            .boxed();
    }
    let small = container(picture::view(&card.small, SMALL, SMALL / 2.0, motion))
        .center(RING)
        .style(move |theme: &Theme| container::Style {
            background: Some(Inks::new(&look, theme.palette()).ground.into()),
            border: iced::Border::default().rounded(RING / 2.0),
            ..container::Style::default()
        });
    // The ring hangs past the picture, so the stack is room for all of it;
    // the box is the picture's height, which the words centre against.
    let stack = stack![large, pin(small).x(RING_AT).y(RING_AT)]
        .width(Length::Fixed(LARGE + RING_HANG))
        .height(Length::Fixed(LARGE + RING_HANG));
    container(stack)
        .width(LARGE + RING_HANG)
        .height(LARGE)
        .align_y(Alignment::Start)
        .boxed()
}

pub use anomi_vm::people::activity::card::wants_clock;
