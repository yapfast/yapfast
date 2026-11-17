//! About a person (`PersonDialog`, `ui/dialogs.slint:705`): a title with its
//! X, what it explains, choices to pick one of under a heading (a timeout's
//! length), a field (a note, a nickname, a reason), a box to tick, an error,
//! and Cancel beside the button that answers. The shell gives it the
//! backdrop, Escape and the way in; the card is drawn here.
//!
//! With no field, Enter answers (`:728`): the app focuses [`CONFIRM`] as the
//! dialog opens, as it focuses the field's [`field_id`] for one that asks for
//! text (`init`, `:743`), and Enter pressed on it answers.

use iced::advanced::widget::Id as WidgetId;
use iced::widget::{column, container, row, text};
use iced::{Alignment, Background, Border, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::people::person::{Intent, Person};
use anomi_widgets::button::{Kind, action_button, icon_button};
use anomi_widgets::field::{Id, field};
use anomi_widgets::menu::item;
use yapfast_lang::tr;

/// The card's width (`min(440px, root.width - 48px)`, `:718`).
const WIDTH: f32 = 440.0;
/// `padding: 20px; spacing: 12px`.
const PAD: f32 = 20.0;
const GAP: f32 = 12.0;
/// The title's size (`font-size: 18px`) and the line the X sets.
const TITLE_SIZE: f32 = 18.0;
const TITLE: f32 = 28.0;
/// The choices' rows sit 2 px apart under their heading (`:737`).
const OPTIONS_GAP: f32 = 2.0;
/// The heading's size (`font-size: 12px`, `letter-spacing: 0.6px`).
const HEADING: f32 = typography::size::CAPTION;
/// The field's height, its text and padding (`:746`).
const FIELD: f32 = 36.0;
const FIELD_PAD: f32 = 10.0;
/// The space between the buttons (`spacing: 8px`).
const BESIDE: f32 = 8.0;

/// The X's box and glyph (`ui/dialogs.slint:736`).
const CLOSE: f32 = 28.0;
const CLOSE_GLYPH: f32 = 18.0;
/// Cancel's id, to focus it from a test.
pub const CANCEL: &str = "person-cancel";

/// The button's id, for the app to focus in a dialog without a field.
pub const CONFIRM: &str = "person-confirm";

/// The field's id, which the app focuses as the dialog opens
/// (`anomi_widgets::field::focus`).
pub fn field_id() -> Id {
    Id::new("person-field")
}

pub fn view(person: &Person, motion: Motion) -> impl View<Intent> + '_ {
    let mut card = column![title(person, motion)].spacing(GAP).padding(PAD);
    if !person.text.is_empty() {
        card = card.push(typography::label(person.text.as_str()).boxed());
    }
    if !person.options.is_empty() {
        card = card.push(options(person, motion));
    }
    if person.has_field() {
        card = card.push(input(person, motion));
    }
    if let Some(tick) = &person.tick {
        card = card.push(item(tick, motion).boxed());
    }
    if !person.error.is_empty() {
        card = card.push(
            text(person.error.as_str())
                .size(typography::size::SMALL)
                .width(Fill)
                // Slint's PersonDialog draws it in Theme.danger (ui/dialogs.slint:765),
                // not the lighter danger_text.
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.palette().danger),
                    ..text::Style::default()
                })
                .boxed(),
        );
    }
    card = card.push(buttons(person, motion));
    container(card).width(WIDTH).style(card_style)
}

fn title<'a>(person: &'a Person, motion: Motion) -> Element<'a, Intent> {
    row![
        text(person.title.as_str())
            .size(TITLE_SIZE)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .width(Fill),
        icon_button(Icon::X, tr!("Close"), motion)
            .size(CLOSE)
            .glyph(CLOSE_GLYPH)
            .on_press(Intent::Close)
            .view(),
    ]
    .align_y(Alignment::Center)
    .height(TITLE)
    .boxed()
}

/// The heading and the choices, the picked one checked.
fn options<'a>(person: &'a Person, motion: Motion) -> Element<'a, Intent> {
    let mut list = column![
        text(person.options_heading.as_str())
            .size(HEADING)
            .font(typography::BOLD)
            .style(catalog::text::muted)
    ]
    .spacing(OPTIONS_GAP);
    for choice in &person.options {
        list = list.push(item(choice, motion).boxed());
    }
    list.boxed()
}

/// The field; while its text is on its way it says "Loading..." and takes
/// nothing, as a field without `on_input` does.
fn input<'a>(person: &'a Person, motion: Motion) -> Element<'a, Intent> {
    let placeholder = if person.loading {
        tr!("Loading...")
    } else {
        person.field.as_str()
    };
    let mut input = field(placeholder, &person.text_now)
        .id(field_id())
        .text_size(typography::size::BODY)
        .pad(FIELD_PAD)
        .height(FIELD)
        .motion(motion);
    if !person.loading {
        input = input.on_input(Intent::Text);
        if person.may_answer() {
            input = input.on_submit(Intent::Answer(person.text_now.clone()));
        }
    }
    input.view()
}

fn buttons<'a>(person: &'a Person, motion: Motion) -> Element<'a, Intent> {
    let cancel = action_button(tr!("Cancel"), Kind::Ghost, motion)
        .id(WidgetId::new(CANCEL))
        .on_press(Intent::Close);
    let kind = if person.danger {
        Kind::Danger
    } else {
        Kind::Primary
    };
    let mut confirm = action_button(person.confirm.as_str(), kind, motion)
        .id(WidgetId::new(CONFIRM))
        .enabled(person.may_answer());
    if person.may_answer() {
        confirm = confirm.on_press(Intent::Answer(person.text_now.clone()));
    }
    container(
        row![cancel, confirm]
            .spacing(BESIDE)
            .align_y(Alignment::Center),
    )
    .align_right(Fill)
    .boxed()
}

/// `background: Theme.raised; border: edge 1px; radius 12px;` and the
/// dialog's shadow (32 px blur, 12 px down, `#00000066`).
fn card_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.raised)),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: catalog::float::dialog(theme).shadow,
        ..container::Style::default()
    }
}

/// Its shots for `anomi-shots`: the whole window over the sample dialog.
/// There is no golden screenshot; these are compared with the Slint app's
/// dialog under `scripts/ui-session`.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::Vm;
    use anomi_vm::fixtures::people::person;
    use anomi_vm::fixtures::shell;
    use anomi_vm::shell::Layer;

    use crate::shots::{GOLDEN, Shot};

    fn over(person: Person) -> Vm {
        let mut vm = shell::open(shell::dm_home(), Layer::Person);
        vm.people.person = person;
        vm
    }
    let shot = |name, vm| Shot {
        area: "person",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("note", || over(person::note())),
        shot("note_typed", || over(person::note_typed())),
        shot("note_loading", || over(person::loading())),
        shot("note_failed", || over(person::failed())),
        shot("nickname", || over(person::nickname())),
        shot("call", || over(person::call())),
        shot("unfriend", || over(person::unfriend())),
        shot("timeout", || over(person::timeout())),
        shot("ban", || over(person::ban())),
    ]
}
