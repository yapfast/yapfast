//! A server's role list, as Discord's Roles page has it: @everyone's
//! Default Permissions, the search with Create Role, and the roles in their
//! order, which dragging a role's handle changes. Task X08's
//! (docs/iced/tasks/settings.md). The Slint side: `RolesSection`'s list
//! (`ui/roles.slint:367`-`:430`); a role's row is `item`'s, its handle
//! `handle`'s and its More menu `menu`'s. The editor a role opens to is
//! `editor`'s (X09).
//!
//! The page scrolls in the frame as one piece, as Slint's: the roles are a
//! [`list`] as tall as all of them (a server holds at most 250), so the
//! list drags them, draws the line where one would drop and the copy that
//! follows the pointer on the raised ground, and keeps each row's state by
//! its role.

mod handle;
mod item;
pub mod menu;

use iced::widget::{column, container, row, space, stack, text};
use iced::{Alignment, Border, Color, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::roles::{Intent, Roles};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::list::{Drag, Fill, Ground, list};
use yapfast_lang::tr;

use crate::kit::{Ink, Leaf, heading, mix};
use item::glyph;

/// Between the page's parts, and between the search and Create Role.
const GAP: f32 = 16.0;
const BESIDE: f32 = 12.0;
/// The search's height, a button's.
const CONTROL: f32 = 36.0;
/// Default Permissions: its card, its corners and sides, the circle with
/// the people, the chevron, the lift under the pointer
/// (`Theme.raised.brighter(0.1)`).
const CARD: f32 = 64.0;
const CARD_RADIUS: f32 = 8.0;
const CARD_SIDES: f32 = 16.0;
const CIRCLE: f32 = 36.0;
const PEOPLE: f32 = 20.0;
const CHEVRON: f32 = 18.0;
const BRIGHTER: f32 = 0.1;
/// Under the columns' names, before their line.
const NAMES_BELOW: f32 = 6.0;

/// A line of the page's quieter words (`Label`: 13 px, muted, wrapped).
fn label<'a>(words: impl text::IntoFragment<'a>) -> Element<'a, Intent> {
    typography::label(words).width(Length::Fill).boxed()
}

/// `colour` as Slint's `brighter(factor)`: its value raised by `factor`,
/// its hue and saturation kept.
fn brighter(colour: Color, factor: f32) -> Color {
    let most = colour.r.max(colour.g).max(colour.b);
    if most <= 0.0 {
        return colour;
    }
    let scale = (most * (1.0 + factor)).min(1.0) / most;
    Color {
        r: colour.r * scale,
        g: colour.g * scale,
        b: colour.b * scale,
        a: colour.a,
    }
}

/// @everyone's card: Default Permissions, which applies to all members.
fn everyone<'a>(roles: &'a Roles, motion: Motion) -> Element<'a, Intent> {
    let mut backdrop = Leaf::new("", motion, |theme: &Theme, lit: f32| {
        let palette = theme.palette();
        Ink {
            fill: mix(palette.raised, brighter(palette.raised, BRIGHTER), lit),
            text: palette.text,
            glyph: palette.text,
        }
    })
    .on_press(Intent::Open(roles.everyone.clone()));
    backdrop.stretch = true;
    backdrop.height = CARD;
    backdrop.radius = CARD_RADIUS;
    let circle = container(glyph(Icon::Users, PEOPLE, |theme| {
        theme.palette().secondary
    }))
    .center(CIRCLE)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().field.into()),
        border: Border::default().rounded(CIRCLE / 2.0),
        ..container::Style::default()
    });
    let words = column![
        text(tr!("Default Permissions"))
            .size(typography::size::BODY)
            .font(typography::SEMIBOLD)
            .style(catalog::text::primary)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .boxed(),
        text(tr!(
            "{} • applies to all server members",
            roles.everyone_name
        ))
        .size(typography::size::LABEL)
        .style(catalog::text::secondary)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .boxed(),
    ]
    .spacing(2)
    .width(Length::Fill);
    let parts = row![
        circle.boxed(),
        words.boxed(),
        glyph(Icon::ChevronRight, CHEVRON, |theme| theme
            .palette()
            .secondary),
    ]
    .spacing(BESIDE)
    .padding(Padding::ZERO.left(CARD_SIDES).right(CARD_SIDES))
    .height(CARD)
    .align_y(Alignment::Center);
    stack![backdrop, parts].boxed()
}

/// The search, a button's height, beside Create Role.
fn search<'a>(roles: &'a Roles, motion: Motion) -> Element<'a, Intent> {
    row![
        container(
            field(tr!("Search Roles"), roles.search.as_str())
                .on_input(Intent::Search)
                .text_size(typography::size::SMALL)
                .height(CONTROL)
                .motion(motion)
                .view()
        )
        .width(Length::Fill)
        .boxed(),
        action_button(tr!("Create Role"), Kind::Primary, motion)
            .enabled(!roles.saving && roles.may_create)
            .on_press(Intent::Create)
            .boxed(),
    ]
    .spacing(BESIDE)
    .align_y(Alignment::Center)
    .boxed()
}

/// The columns' names over the roles, laid as a row's parts are.
fn names(roles: &Roles) -> Element<'_, Intent> {
    // The count, Edit and More with the room between them.
    let after = item::MEMBERS + 2.0 * (item::BUTTON + item::SPACING);
    row![
        space().width(handle::WIDTH).boxed(),
        heading(tr!("ROLES – {}", roles.rows.len()))
            .width(Length::Fill)
            .boxed(),
        heading(tr!("MEMBERS")).width(after).boxed(),
    ]
    .spacing(item::SPACING)
    .padding(Padding {
        left: item::LEFT,
        right: item::RIGHT,
        bottom: NAMES_BELOW,
        top: 0.0,
    })
    .boxed()
}

/// The line under the columns' names.
fn rule<'a>() -> Element<'a, Intent> {
    container(space())
        .width(Length::Fill)
        .height(1)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().line.into()),
            ..container::Style::default()
        })
        .boxed()
}

/// What a drag of the role at `from` asks: it's held while it moves, and
/// it goes where it's dropped.
fn dragged(roles: &Roles, drag: Drag) -> Intent {
    let id = |at: usize| roles.rows.get(at).map(|role| role.id.clone());
    match drag {
        Drag::Started { from } | Drag::Over { from, .. } => Intent::Dragging(id(from)),
        Drag::Dropped { from, to } if from != to => match id(from) {
            Some(id) => Intent::Move { id, to },
            None => Intent::Dragging(None),
        },
        Drag::Dropped { .. } | Drag::Cancelled { .. } => Intent::Dragging(None),
    }
}

/// The roles, or "No roles" when the search leaves none.
fn table<'a>(roles: &'a Roles, motion: Motion) -> Element<'a, Intent> {
    if roles.rows.is_empty() {
        return label(tr!("No roles"));
    }
    list(
        roles.rows.len(),
        |index| roles.key(index),
        move |index| item::role(roles, index, motion),
        motion,
    )
    // As tall as its roles: the page scrolls, not the list.
    .height(Length::Shrink)
    .scrollbar(false)
    .estimate(item::HEIGHT)
    .reorder(
        |index| roles.may_move(index),
        |from, to| roles.may_drop(from, to),
        move |drag| dragged(roles, drag),
    )
    .drag_handle(item::GRIP)
    .drag_ground(|_| Ground {
        fill: Fill::Raised,
        left: 0.0,
        right: 0.0,
        radius: 6.0,
        alpha: 1.0,
        ..Ground::default()
    })
    .boxed()
}

pub fn view(roles: &Roles, motion: Motion) -> impl View<Intent> + '_ {
    column![
        label(tr!(
            "Use roles to group your server members and assign permissions."
        )),
        everyone(roles, motion),
        search(roles, motion),
        label(tr!(
            "Members use the color of the highest role they have on this list. Drag roles to reorder them."
        )),
        column![names(roles), rule(), table(roles, motion)],
    ]
    .spacing(GAP)
    .width(Length::Fill)
}

/// Its shots for `anomi-shots`: `ssettings/roles.png` as the Slint app
/// showed it, a role's More menu, every kind of mark, a search and a search
/// that finds nothing.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::roles;
    let shot = |name: &'static str, vm: fn() -> anomi_vm::Vm| Shot {
        area: "ssettings",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("roles", roles::open),
        shot("roles_menu", menu_open),
        shot("roles_marks", || roles::window(roles::marks())),
        shot("roles_search", || roles::window(roles::searched())),
        shot("roles_none", || roles::window(roles::none())),
    ]
}

/// The window with Crew's More menu open: Move Up can't go above the
/// locked roles.
#[cfg(feature = "shots")]
fn menu_open() -> anomi_vm::Vm {
    use anomi_vm::fixtures::settings::roles;
    use anomi_vm::menu::{Align, Spot};
    use iced::{Point, Rectangle, Size};
    let mut vm = roles::open();
    let more = Rectangle::new(Point::new(1141.0, 471.0), Size::new(32.0, 32.0));
    let at = Spot::below(more).align(Align::End);
    vm.menu = menu::menu(&vm.settings.roles, roles::CREW, at, 1);
    vm
}
