//! A server tag's card, 300 px wide as Discord's, headed as a mutual
//! server's: the server's icon over a banner, its name and the tag, its
//! counts, when it was made, its description, and a way in when this
//! account is in it. What Discord hasn't said yet shows as skeletons
//! (`TagCardView`, `ui/people.slint:974`).

use iced::widget::text::LineHeight;
use iced::widget::{Column, Row, button, container, text};
use iced::{Alignment, Fill, Padding};
use iced::{Length, Pixels};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::kind::Expected;
use anomi_vm::people::cards::{Intent, ServerTag, WIDTH};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::skeleton::skeleton;
use anomi_widgets::tag::server_tag;
use yapfast_lang::tr;

use super::{head, surface};

const SIDE: f32 = metrics::SPACE_L;
const SMALL: Pixels = Pixels(typography::size::SMALL);
const GAP: f32 = metrics::SPACE_M;
const NAME: f32 = typography::size::DIALOG_TITLE;
/// The name's skeleton while Discord hasn't answered, and the one for a
/// line of text (`Skeleton { width: 140px; height: 20px }`, `height: 14px`).
const NAME_SKELETON: (f32, f32) = (140.0, 20.0);
const LINE_SKELETON: f32 = 14.0;
const SKELETON_RADIUS: f32 = 4.0;
/// The space between the counts, and between the name and the chip
/// (`ui/people.slint:1031`, `:1027`).
const COUNT_GAP: f32 = 14.0;
const TITLE_GAP: f32 = 6.0;
/// The description's lines before it is cut (`max-lines: 4`).
const DESCRIPTION_LINES: f32 = 4.0;

pub fn view(card: &ServerTag, motion: Motion) -> Element<'_, Intent> {
    let stand = if card.icon.ready().is_none() && (card.limited || card.failed) {
        head::Stand::Glyph(if card.failed {
            Icon::TriangleAlert
        } else {
            Icon::Lock
        })
    } else {
        head::Stand::Initial(&card.initial)
    };
    let mut body = Column::<Element<'_, Intent>>::new()
        .push(title(card, motion))
        .spacing(GAP)
        .padding(Padding {
            left: SIDE,
            right: SIDE,
            bottom: SIDE,
            ..Padding::ZERO
        });
    if !card.online.is_empty() || !card.members.is_empty() {
        let mut counts = Row::<Element<'_, Intent>>::new()
            .spacing(COUNT_GAP)
            .align_y(Alignment::Center);
        if !card.online.is_empty() {
            counts = counts.push(head::count(|palette| palette.live, &card.online));
        }
        if !card.members.is_empty() {
            counts = counts.push(head::count(|palette| palette.faint, &card.members));
        }
        body = body.push(counts.boxed());
    }
    if card.loading {
        body = body.push(
            skeleton(motion)
                .width(Fill)
                .height(LINE_SKELETON)
                .radius(SKELETON_RADIUS)
                .boxed(),
        );
    }
    if !card.established.is_empty() {
        body = body.push(
            text(card.established.as_str())
                .size(typography::size::LABEL)
                .style(catalog::text::secondary)
                .boxed(),
        );
    }
    if !card.description.is_empty() {
        body = body.push(
            container(
                text(card.description.as_str())
                    .size(typography::size::SMALL)
                    .style(catalog::text::primary)
                    .ellipsis(text::Ellipsis::End),
            )
            .width(Fill)
            .height(
                Length::Shrink.max(DESCRIPTION_LINES * LineHeight::default().to_absolute(SMALL).0),
            )
            .boxed(),
        );
    }
    if !card.note.is_empty() {
        body = body.push(note(card));
    }
    if card.joined {
        body = body.push(
            action_button(tr!("Go to Server"), Kind::Primary, motion)
                .width(Fill)
                .on_press(Intent::OpenServer(card.server.clone()))
                .boxed(),
        );
    }
    surface(
        Column::<Element<'_, Intent>>::new()
            .push(head::view(&Expected::None, &card.icon, stand, motion))
            .push(body.boxed())
            .boxed(),
        WIDTH,
        metrics::RADIUS_DIALOG,
        |theme| theme.palette().sidebar,
    )
}

/// The name, which opens the server when this account is in it, or its
/// skeleton while Discord hasn't answered; the tag's chip under it.
fn title(card: &ServerTag, motion: Motion) -> Element<'_, Intent> {
    let mut lines = Column::<Element<'_, Intent>>::new().spacing(TITLE_GAP);
    if !card.name.is_empty() {
        let name = text(card.name.as_str())
            .size(NAME)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End);
        lines = lines.push(
            button(name)
                .padding(0)
                .on_press_maybe(card.joined.then(|| Intent::OpenServer(card.server.clone())))
                .style(|theme: &Theme, _| button::Style {
                    text_color: theme.palette().text,
                    ..button::Style::default()
                })
                .boxed(),
        );
    } else if card.loading {
        lines = lines.push(
            skeleton(motion)
                .width(NAME_SKELETON.0)
                .height(NAME_SKELETON.1)
                .radius(SKELETON_RADIUS)
                .boxed(),
        );
    }
    lines
        .push(
            Row::<Element<'_, Intent>>::new()
                .push(server_tag(&card.tag, motion).view().boxed())
                .boxed(),
        )
        .width(Fill)
        .boxed()
}

/// Why there's no more to show; a failure's note tries again when clicked,
/// as in Discord's client, and brightens under the pointer.
fn note(card: &ServerTag) -> Element<'_, Intent> {
    let words = text(card.note.as_str()).size(typography::size::LABEL);
    if !card.failed {
        return words.style(catalog::text::muted).width(Fill).boxed();
    }
    button(words)
        .padding(0)
        .on_press(Intent::Retry(card.server.clone()))
        .style(|theme: &Theme, status| {
            let lit = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let palette = theme.palette();
            button::Style {
                text_color: if lit { palette.text } else { palette.muted },
                ..button::Style::default()
            }
        })
        .boxed()
}
