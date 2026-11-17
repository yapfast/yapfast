//! The dialog that makes a poll (`PollDialog`, `ui/dialogs.slint:932`): a
//! title with its X, the question with its counter, the answers, the
//! duration, "Allow Multiple Answers", what went wrong, and Cancel and Post.

use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Background, Border, Fill, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::poll::{DURATIONS, Dialog, Intent};
use anomi_vm::icon::Icon;
use anomi_widgets::button::{Kind, action_button, icon_button};
use anomi_widgets::field::{Id, field};
use anomi_widgets::icon::icon;
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

use super::choices::{choices, natural};
use super::drafts;
use super::link::link;
use super::widest::widest;

/// The card's least width (`max(500px, durations.natural + 40px)`).
const WIDTH: f32 = 500.0;
/// `padding: 20px`; the form's `spacing: 8px`.
const PAD: f32 = 20.0;
const GAP: f32 = 8.0;
const TITLE_SIZE: f32 = 18.0;
const TITLE: f32 = 28.0;
const GLYPH: f32 = 18.0;
const HEAD_BELOW: f32 = 12.0;
const FOOT_ABOVE: f32 = 12.0;
const FIELD: f32 = 40.0;
const FIELD_PAD: f32 = 10.0;
const HEADING: f32 = typography::size::CAPTION;
const HEADING_LINE: f32 = 16.0;
/// The multiple-answers checkbox (20 px, 2 px rim, 14 px check).
const BOX: f32 = 20.0;
const BOX_RIM: f32 = 2.0;
const BOX_CHECK: f32 = 14.0;
const BOX_GAP: f32 = 10.0;
const BOX_ABOVE: f32 = 4.0;
/// The card's shadow: `#00000066`, 12 px down, 32 px blur.
const SHADOW_ALPHA: f32 = 0.4;
const SHADOW_DOWN: f32 = 12.0;
const SHADOW_BLUR: f32 = 32.0;

/// The buttons' ids, to focus them from a test or a task.
pub const POST: &str = "poll-post";
pub const CANCEL: &str = "poll-cancel";

/// The question's field: the app focuses it as the dialog opens (`init`,
/// `:967`) with `anomi_widgets::field::focus`.
pub fn question_id() -> Id {
    Id::new("poll-question")
}

fn heading<'a>(words: &'a str) -> Element<'a, Intent> {
    text(words)
        .size(HEADING)
        .line_height(iced::Pixels(HEADING_LINE))
        .font(typography::BOLD)
        .wrapping(text::Wrapping::None)
        .style(catalog::text::muted)
        .boxed()
}

pub(super) fn view(poll: &Dialog, motion: Motion) -> impl View<Intent> + '_ {
    let head = row![
        text(tr!("Create a Poll"))
            .size(TITLE_SIZE)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary)
            .width(Fill),
        icon_button(Icon::X, tr!("Close"), motion)
            .size(TITLE)
            .glyph(GLYPH)
            .on_press(Intent::Close)
            .view(),
    ]
    .align_y(Alignment::Center)
    .height(TITLE);

    let question = field(tr!("What question do you want to ask?"), &poll.question)
        .id(question_id())
        .on_input(Intent::Question)
        .on_submit(Intent::Post)
        .text_size(typography::size::BODY)
        .pad(FIELD_PAD)
        .height(FIELD)
        .motion(motion);

    let mut form: Vec<Element<'_, Intent>> = vec![
        row![
            heading(tr!("QUESTION")),
            space().width(Fill),
            text(poll.question_count.as_str())
                .size(HEADING)
                .wrapping(text::Wrapping::None)
                .style(catalog::text::muted),
        ]
        .boxed(),
        question.view(),
    ];
    // Each answer keeps room above it for its badge's overhang, which Slint
    // draws over the gap: the gap is narrower by as much.
    let several = poll.answers.len() > 1;
    let mut answers = vec![heading(tr!("ANSWERS"))];
    answers.extend(
        poll.answers
            .iter()
            .enumerate()
            .map(|(index, draft)| drafts::view(index, draft, several, motion)),
    );
    form.push(column(answers).spacing(GAP - drafts::OVERHANG).boxed());
    if poll.can_add {
        form.push(link(
            tr!("Add another answer"),
            Icon::CirclePlus,
            Intent::AddAnswer,
        ));
    }
    form.push(heading(tr!("DURATION")));
    let labels: [&str; DURATIONS] = [
        tr!("1 hour"),
        tr!("4 hours"),
        tr!("8 hours"),
        tr!("1 day"),
        tr!("3 days"),
        tr!("1 week"),
        tr!("2 weeks"),
    ];
    form.push(choices(&labels, poll.duration, Intent::Duration));
    let probe = natural(&labels);
    form.push(multiple(poll.multiselect));
    let form = scroll_view(column(form).spacing(GAP).width(Fill), motion).height(
        iced::Length::Fluid(iced_runtime::core::length::Constraint::Max),
    );

    let mut foot: Vec<Element<'_, Intent>> = vec![space().height(FOOT_ABOVE - GAP).boxed()];
    if !poll.problem.is_empty() {
        foot.push(
            text(poll.problem.as_str())
                .size(typography::size::SMALL)
                .width(Fill)
                .style(catalog::text::danger)
                .boxed(),
        );
    }
    let post = action_button(
        if poll.posting {
            tr!("Posting...")
        } else {
            tr!("Post")
        },
        Kind::Primary,
        motion,
    )
    .id(iced::advanced::widget::Id::new(POST))
    .enabled(poll.can_post);
    foot.push(
        row![
            space().width(Fill),
            action_button(tr!("Cancel"), Kind::Ghost, motion)
                .id(iced::advanced::widget::Id::new(CANCEL))
                .on_press(Intent::Close),
            if poll.can_post {
                post.on_press(Intent::Post)
            } else {
                post
            },
        ]
        .spacing(GAP)
        .align_y(Alignment::Center)
        .boxed(),
    );
    let card = column![
        head,
        space().height(HEAD_BELOW),
        form,
        column(foot).spacing(GAP),
    ]
    .padding(PAD);
    widest(
        container(card).width(Fill).style(card_style).boxed(),
        probe,
        WIDTH,
        2.0 * PAD,
    )
}

/// Discord's "Allow Multiple Answers", a checkbox with its words.
fn multiple<'a>(on: bool) -> Element<'a, Intent> {
    let tick = container(if on {
        icon(Icon::Check, BOX_CHECK)
            .style(|theme: &Theme, _| iced::widget::svg::Style {
                color: Some(theme.palette().on_accent),
            })
            .boxed()
    } else {
        space().boxed()
    })
    .center(BOX)
    .style(move |theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            background: on.then_some(Background::Color(palette.accent)),
            border: Border::default()
                .rounded(metrics::RADIUS_MARK)
                .width(BOX_RIM)
                .color(if on { palette.accent } else { palette.faint }),
            ..container::Style::default()
        }
    });
    button(
        row![
            tick,
            text(tr!("Allow Multiple Answers"))
                .size(typography::size::BODY)
                .wrapping(text::Wrapping::None)
                .style(catalog::text::primary),
        ]
        .spacing(BOX_GAP)
        .align_y(Alignment::Center)
        .padding(iced::Padding::ZERO.top(BOX_ABOVE)),
    )
    .padding(0)
    .on_press(Intent::Multiselect(!on))
    .style(|_: &Theme, _| button::Style::default())
    .boxed()
}

/// `background: raised; border: edge 1px; radius 12px;` and the dialog's
/// shadow.
fn card_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.raised)),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, SHADOW_ALPHA),
            offset: Vector::new(0.0, SHADOW_DOWN),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}
