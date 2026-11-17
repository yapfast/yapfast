//! Forwarding a message (`ForwardDialog`, `ui/dialogs.slint:231`): a search of
//! the places, up to five chosen with a check each, the message quoted, a note
//! to add and Send. The shell gives it the dialog's backdrop, Escape and the
//! way in; the card is drawn here.

use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Fill, Length, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::forward::{Forward, Intent};
use anomi_vm::icon::Icon;
use anomi_widgets::button::icon_button;
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::{Id, field};
use anomi_widgets::list::list;
use anomi_widgets::tip::Side;
use yapfast_lang::{tr, trn};

mod target;

/// The card: `min(440px, ..)` by `min(600px, ..)`; the shell keeps it inside
/// a small window.
const WIDTH: f32 = 440.0;
const HEIGHT: f32 = 600.0;
/// `padding: 20px; spacing: 12px`.
const PAD: f32 = 20.0;
const GAP: f32 = metrics::SPACE_M;
const TITLE: f32 = 18.0;
const SEARCH: f32 = 36.0;
const NOTE: f32 = 40.0;
/// The quote's bar (`width: 3px`) and the line it stands by.
const BAR: f32 = 3.0;
const QUOTE_LINE: f32 = 20.0;
/// The gap between rows (`spacing: 2px`).
const ROW_GAP: f32 = 2.0;
/// The title and its hint (`spacing: 2px`, `ui/dialogs.slint:271`).
const HEAD_GAP: f32 = 2.0;
/// The close button (`:278`): 28 px with an 18 px glyph.
const CLOSE_SIZE: f32 = 28.0;
const CLOSE_GLYPH: f32 = 18.0;
/// The fields' inner padding (`:282`, `:342`).
const FIELD_PAD: f32 = 10.0;
/// The quote's bar and its line (`spacing: 10px`, `:337`).
const QUOTE_GAP: f32 = 10.0;
/// The card's shadow (`:258`): `#00000066`, 12 px down, 32 px blur.
const SHADOW_ALPHA: f32 = 0x66 as f32 / 255.0;
const SHADOW_DROP: f32 = 12.0;
const SHADOW_BLUR: f32 = 32.0;

/// The Send button's id, to focus it from a test.
pub const SEND: &str = "forward-send";
/// The close button's id.
pub const CLOSE: &str = "forward-close";

/// The search field's id: the app focuses it as the dialog opens (`init`,
/// `ui/dialogs.slint:262`) with `anomi_widgets::field::focus`.
pub fn query_id() -> Id {
    Id::new("forward-query")
}

/// The note field's id.
pub fn note_id() -> Id {
    Id::new("forward-note")
}

pub fn view(forward: &Forward, motion: Motion) -> impl View<Intent> + '_ {
    let note = field(tr!("Add an optional message..."), &forward.note)
        .id(note_id())
        .on_input(Intent::Note)
        .text_size(typography::size::BODY)
        .pad(FIELD_PAD)
        .height(NOTE)
        .motion(motion);
    let note = if forward.can_send() {
        note.on_submit(Intent::Send)
    } else {
        note
    };
    let mut body = column![
        head(forward, motion),
        search(forward, motion),
        places(forward, motion),
        quote(forward),
        note.view()
    ]
    .spacing(GAP);
    if !forward.note.is_empty() && !forward.slowmode.is_empty() {
        body = body.push(typography::label(forward.slowmode.as_str()).boxed());
    }
    if !forward.error.is_empty() {
        body = body.push(
            typography::label(forward.error.as_str())
                .style(catalog::text::danger)
                .boxed(),
        );
    }
    body = body.push(send(forward, motion));
    container(body.padding(PAD).height(Fill))
        .width(Length::Fixed(WIDTH))
        .height(Length::Fixed(HEIGHT))
        .style(card)
}

/// The title with what it says beneath, and the close button.
fn head(forward: &Forward, motion: Motion) -> Element<'_, Intent> {
    let hint = if forward.chosen < forward.limit {
        String::from(tr!("Select where you want to share this message."))
    } else {
        trn!(
            "Maximum {n} place at once.",
            "Maximum {n} places at once.",
            forward.limit
        )
    };
    row![
        column![
            text(tr!("Forward To"))
                .size(TITLE)
                .font(typography::BOLD)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End),
            text(hint)
                .size(typography::size::SMALL)
                .style(catalog::text::secondary)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End),
        ]
        .spacing(HEAD_GAP)
        .width(Fill),
        container(
            icon_button(Icon::X, tr!("Close"), motion)
                .size(CLOSE_SIZE)
                .glyph(CLOSE_GLYPH)
                .on_press(Intent::Close)
                .face()
                .id(Id::new(CLOSE))
                .tipped(tr!("Close"), Side::Above)
        )
        .center_y(Length::Shrink),
    ]
    .align_y(Alignment::Center)
    .boxed()
}

fn search(forward: &Forward, motion: Motion) -> Element<'_, Intent> {
    field(tr!("Search"), &forward.query)
        .id(query_id())
        .on_input(Intent::Query)
        .text_size(typography::size::BODY)
        .pad(FIELD_PAD)
        .height(SEARCH)
        .motion(motion)
        .view()
}

/// The places as a list that scrolls, or why there are none.
fn places(forward: &Forward, motion: Motion) -> Element<'_, Intent> {
    if forward.targets.is_empty() {
        return container(typography::label(forward.empty.as_str()))
            .padding(metrics::SPACE_S)
            .width(Fill)
            .height(Fill)
            .boxed();
    }
    list(
        forward.targets.len(),
        |index| index as u64,
        move |index| target::view(forward, index, motion),
        motion,
    )
    .estimate(target::HEIGHT)
    .spacing(ROW_GAP)
    .height(Fill)
    .boxed()
}

/// The message as a quote: a bar and one line.
fn quote(forward: &Forward) -> Element<'_, Intent> {
    row![
        container("")
            .width(BAR)
            .height(Fill)
            .style(|theme: &Theme| container::Style {
                background: Some(theme.palette().line.into()),
                border: Border::default().rounded(BAR / 2.0),
                ..container::Style::default()
            }),
        text(forward.quote.as_str())
            .size(typography::size::BODY)
            .style(catalog::text::secondary)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .width(Fill),
    ]
    .spacing(QUOTE_GAP)
    .height(QUOTE_LINE)
    .boxed()
}

/// Send, with how many places it goes to once there are two or more (the
/// words are `Forward::send`).
fn send(forward: &Forward, motion: Motion) -> Element<'_, Intent> {
    let button = action_button(&forward.send, Kind::Primary, motion)
        .id(Id::new(SEND))
        .enabled(forward.can_send());
    let button = if forward.can_send() {
        button.on_press(Intent::Send)
    } else {
        button
    };
    container(button).align_right(Fill).boxed()
}

/// `background: Theme.raised; border: edge 1px; radius 12px;` and the
/// dialog's shadow (32 px blur, 12 px down, `#00000066`).
fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, SHADOW_ALPHA),
            offset: Vector::new(0.0, SHADOW_DROP),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}

/// Its shots for `anomi-shots`: the whole window over the dialog, one for each
/// of the Slint one's states (no golden shot of it).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::Vm;
    use anomi_vm::fixtures::chat::forward as sample;
    use anomi_vm::fixtures::shell;
    use anomi_vm::shell::Layer;

    use crate::shots::{GOLDEN, Shot};

    fn over(forward: Forward) -> Vm {
        let mut vm = shell::open(shell::server_home(), Layer::Forward);
        vm.chat.forward = forward;
        vm
    }
    let shot = |name, vm| Shot {
        area: "forward",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("open", || over(sample::open())),
        shot("chosen", || over(sample::chosen_two())),
        shot("one", || over(sample::chosen_one())),
        shot("limit", || over(sample::at_limit())),
        shot("found", || over(sample::found())),
        shot("nothing", || over(sample::nothing_found())),
        shot("nowhere", || over(sample::nowhere())),
        shot("slowmode", || over(sample::slowmode())),
        shot("failed", || over(sample::failed())),
    ]
}
