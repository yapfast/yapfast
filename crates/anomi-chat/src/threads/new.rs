//! A new thread: Discord's "New Thread" form, beside the channel in place of
//! its member list (`NewThreadPanel`, `ui/thread.slint:452`): a header with
//! its cross, the message it starts from, the name, the private switch, why
//! it failed, and at the bottom the channel's composer for the first
//! message, which Enter sends with the thread. Closing it after writing asks
//! first (`leave`). The glue is `src/ui/refresh.rs:1789`.
//!
//! The panel takes its room at once and the question appears at once: Slint
//! fades both with `PopupMotion`, which the shell's layers give a dialog but
//! a card inside a panel doesn't have (see `motion::Popup`).

pub mod leave;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::{Toggler, column, container, mouse_area, row, rule, svg, text};
use iced::{Alignment, Fill, Length, Padding, alignment};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::new_thread::{Intent, NewThread};
use anomi_vm::chat::{self};
use anomi_vm::icon::Icon;
use anomi_widgets::button::icon_button;
use anomi_widgets::composer::composer as body_field;
use anomi_widgets::field::{Id, field};
use anomi_widgets::focus;
use anomi_widgets::icon::icon;
use anomi_widgets::tip::Side;

use crate::composer::frame;
use anomi_vm::chat::search::Intent as SearchIntent;

use crate::search;

#[cfg(feature = "shots")]
pub use shots::shots;

/// The name field's id, which a test focuses and the app focuses on opening.
pub fn name_id() -> Id {
    Id::new("new-thread-name")
}

/// The cross's and the send button's ids, which tests press them by.
pub const CLOSE_ID: &str = "new-thread-close";
pub const CREATE_ID: &str = "new-thread-create";

/// The header: `height: 48px`, `padding-left: 16px; padding-right: 10px;
/// spacing: 10px`; the thread's icon (18 px) and the title (15 px, bold).
const HEAD: f32 = 48.0;
const HEAD_LEFT: f32 = 16.0;
const HEAD_RIGHT: f32 = 10.0;
const HEAD_GAP: f32 = 10.0;
const HEAD_ICON: f32 = 18.0;
const HEAD_TITLE: f32 = 15.0;
/// The close button: 28 px with an 18 px glyph.
const CLOSE: f32 = 28.0;
/// The form's `padding: 16px; spacing: 16px`; a label's `spacing: 8px`.
const PAD: f32 = metrics::SPACE_L;
const GAP: f32 = metrics::SPACE_L;
const LABEL_GAP: f32 = metrics::SPACE_S;
/// The label over the name: 12 px, bold, `letter-spacing: 0.6px`.
const LABEL: f32 = typography::size::CAPTION;
/// The switch's label (15 px), its row's gap (`spacing: 12px`) and what it
/// does below (`spacing: 4px`).
const SWITCH_TEXT: f32 = typography::size::BODY;
const SWITCH_GAP: f32 = metrics::SPACE_M;
const SWITCH_BELOW: f32 = metrics::SPACE_XS;
/// The error line: 14 px.
const ERROR: f32 = typography::size::SMALL;
/// `ClientStyle.composer-gutter` (the sides and below), the field's row
/// (`padding-left: 10px; padding-right: 8px; spacing: 6px`) and
/// `composer-button-inset`.
const GUTTER: f32 = 8.0;
const ROW_LEFT: f32 = 10.0;
const ROW_RIGHT: f32 = 8.0;
const ROW_GAP: f32 = 6.0;
const BUTTON_INSET: f32 = 14.0;

pub fn view(new_thread: &NewThread, motion: Motion) -> impl View<chat::Intent> + '_ {
    let form = column![
        head(motion),
        container(rule::horizontal(1).style(catalog::rule::line)),
        form(new_thread, motion),
        first_message(new_thread, motion),
    ]
    .width(Fill)
    .height(Fill);
    let panel = row![rule::vertical(1).style(catalog::rule::outline_chat), form];
    let panel = container(panel)
        .width(Fill)
        .height(Fill)
        .style(catalog::container::chat);
    leave::over(panel, new_thread, motion).map(chat::Intent::from)
}

/// The header: the thread's icon, the title and the cross.
fn head(motion: Motion) -> Element<'static, Intent> {
    container(
        row![
            icon(Icon::Thread, HEAD_ICON).style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().muted),
            }),
            text(tr!("New Thread"))
                .size(HEAD_TITLE)
                .font(typography::BOLD)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .width(Fill)
                .style(catalog::text::primary),
            icon_button(Icon::X, tr!("Close"), motion)
                .size(CLOSE)
                .glyph(HEAD_ICON)
                .on_press(Intent::Close)
                .face()
                .id(Id::new(CLOSE_ID))
                .tipped(tr!("Close"), Side::Below),
        ]
        .spacing(HEAD_GAP)
        .align_y(Alignment::Center),
    )
    .padding(Padding {
        top: 0.0,
        right: HEAD_RIGHT,
        bottom: 0.0,
        left: HEAD_LEFT,
    })
    .height(HEAD)
    .width(Fill)
    .align_y(alignment::Vertical::Center)
    .boxed()
}

/// What the form asks above the composer, which takes the rest of the room.
fn form(vm: &NewThread, motion: Motion) -> Element<'_, Intent> {
    let mut parts: Vec<Element<'_, Intent>> = Vec::new();
    if let Some(starter) = &vm.starter {
        parts.push(
            Element::from(search::card(starter, motion))
                .map(|intent| match intent {
                    SearchIntent::Jump { room, item } => Intent::Jump { room, item },
                    SearchIntent::Link(link) => Intent::Link(link),
                    other => unreachable!("a found card asks for {other:?}"),
                })
                .boxed(),
        );
    }
    parts.push(name(vm, motion));
    if vm.can_private {
        parts.push(private(vm, motion));
    }
    if !vm.error.is_empty() {
        parts.push(
            text(vm.error.as_str())
                .size(ERROR)
                .style(catalog::text::danger)
                .width(Fill)
                .boxed(),
        );
    }
    let form = iced::widget::column(parts)
        .spacing(GAP)
        .padding(PAD)
        .width(Fill);
    container(form).width(Fill).height(Fill).boxed()
}

/// The label over the name's field, and the field.
fn name(vm: &NewThread, motion: Motion) -> Element<'_, Intent> {
    let label = if vm.name_optional() {
        tr!("THREAD NAME (OPTIONAL)")
    } else {
        tr!("THREAD NAME")
    };
    let placeholder = if vm.placeholder.is_empty() {
        tr!("New Thread")
    } else {
        vm.placeholder.as_str().into()
    };
    let mut name = field(&placeholder, &vm.name)
        .id(name_id())
        .motion(motion)
        .on_submit(Intent::ToBody);
    if !vm.starting {
        name = name.on_input(Intent::Name);
    }
    column![
        text(label)
            .size(LABEL)
            .font(typography::BOLD)
            .style(catalog::text::muted)
            .width(Fill),
        name.view(),
    ]
    .spacing(LABEL_GAP)
    .boxed()
}

/// The private switch (`SettingsSwitch`): its label and track on one line,
/// and what it does under them; a click on the label flips it too.
fn private(vm: &NewThread, _motion: Motion) -> Element<'_, Intent> {
    let enabled = !vm.only_private && !vm.starting;
    let says = if vm.private {
        tr!(
            "Only people you invite and moderators can see. You can invite new people by @mentioning them."
        )
    } else {
        tr!("Only people you invite and moderators can see")
    };
    let label = text(tr!("Private Thread"))
        .size(SWITCH_TEXT)
        .style(if enabled {
            catalog::text::primary
        } else {
            catalog::text::muted
        })
        .width(Fill);
    let label = if enabled {
        mouse_area(label)
            .on_press(Intent::Private(!vm.private))
            .interaction(iced::mouse::Interaction::Pointer)
            .boxed()
    } else {
        label.boxed()
    };
    let track = Toggler::new(vm.private)
        .size(metrics::TOGGLE_HEIGHT)
        .on_toggle_maybe(enabled.then_some(Intent::Private));
    column![
        row![label, track]
            .spacing(SWITCH_GAP)
            .align_y(Alignment::Center),
        typography::label(says).width(Fill),
    ]
    .spacing(SWITCH_BELOW)
    .boxed()
}

/// The channel's composer's gutter and box, with the send button on the
/// field's last line.
fn first_message(vm: &NewThread, motion: Motion) -> Element<'_, Intent> {
    let mut field = body_field(&vm.body, motion)
        .key(vm.key)
        .id(focus::ASIDE_COMPOSER)
        .placeholder(tr!("Enter a message to start the conversation!"))
        .pad(metrics::COMPOSER_PADDING)
        .min_height(metrics::COMPOSER_HEIGHT);
    if !vm.starting {
        field = field
            .on_edit(|text, caret| Intent::Body { text, caret })
            .on_send(Intent::Create)
            .on_focus(Intent::Focus);
    }
    let label = if vm.starting {
        tr!("Creating...")
    } else {
        tr!("Create Thread")
    };
    let mut send = icon_button(Icon::SendHorizontal, label, motion)
        .enabled(vm.can_create())
        .emphasis(vm.emphasised());
    if vm.can_create() {
        send = send.on_press(Intent::Create);
    }
    let send = container(
        send.face()
            .id(Id::new(CREATE_ID))
            .tipped(label, Side::Above),
    )
    .padding(Padding {
        top: 0.0,
        right: 0.0,
        bottom: BUTTON_INSET,
        left: 0.0,
    })
    .align_y(alignment::Vertical::Bottom);
    let line = row![field.boxed(), send]
        .spacing(ROW_GAP)
        .padding(Padding {
            top: 0.0,
            right: ROW_RIGHT,
            bottom: 0.0,
            left: ROW_LEFT,
        })
        .align_y(alignment::Vertical::Bottom)
        .width(Fill);
    container(frame(line.boxed(), vm.focused, false, motion))
        .padding(Padding {
            top: 0.0,
            right: GUTTER,
            bottom: GUTTER,
            left: GUTTER,
        })
        .width(Length::Fill)
        .boxed()
}
