//! Adding people to a DM, the popover under the header's add button. Task
//! P13 (docs/iced/tasks/people.md). The Slint side: `AddPeoplePopover`
//! (`ui/header.slint:539`) in `Popover` (`:243`).
//!
//! A heading (the add icon, "Select Friends", how many more fit, Close)
//! over a search field, the friends to pick from, why adding failed, and
//! the button: "Create Group DM" from a one-to-one DM, else "Add to Group".
//! The shell places it, closes it on Escape and a press outside, and gives
//! it its way in; the app focuses the field as it opens (`query_id`).

mod row;

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::{column, container, text};
use iced::{Alignment, Background, Border, Color, Fill, Padding, Shadow, Vector, border};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::people::add_people::{AddPeople, Intent};
use anomi_widgets::button::{Kind, Side, action_button, icon_button};
use anomi_widgets::field::{self, field};
use anomi_widgets::icon::icon;
use anomi_widgets::list::list;
use yapfast_lang::{tr, trn};

/// The popover's width (`width: 440px`) and its height at most (`min(520px,
/// ...)`, the shell keeps it in the window).
const WIDTH: f32 = 440.0;
const HEIGHT: f32 = 520.0;
const RADIUS: f32 = 10.0;
/// The heading's height with and without its second line.
const HEADING: f32 = 64.0;
/// The body's padding and spacing (`padding: 12px; spacing: 8px`), and the
/// search field's height and size.
const PAD: f32 = metrics::SPACE_M;
const GAP: f32 = metrics::SPACE_S;
const FIELD_PAD: f32 = 10.0;
const ROWS_GAP: f32 = 2.0;
/// The heading's icon (`:260`), its title's spacing, and the gap between
/// icon, title and Close (`spacing: 10px`, `:262`).
const ICON: f32 = 20.0;
const TITLE_GAP: f32 = 2.0;
const HEAD_GAP: f32 = 10.0;
/// The heading's right padding (`padding-right: 10px`).
const HEAD_END: f32 = 10.0;
/// Close and its glyph (`size: 28px; glyph: 18px`, `:266`).
const CLOSE: f32 = 28.0;
const CLOSE_GLYPH: f32 = 18.0;
/// The popover's shadow (`#00000066`, 24 px blur, 8 px down; `:250`).
const SHADOW: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.4,
};
const SHADOW_BLUR: f32 = 24.0;
const SHADOW_DOWN: f32 = 8.0;
/// The border's width, which the content stays inside.
const BORDER: f32 = 1.0;

/// The add button's id, to focus it from a test or a task.
pub const ADD: &str = "add-people-add";

/// The search field's id: the app focuses it as the popover opens (`init`,
/// `:555`) with `anomi_widgets::field::focus`.
pub fn query_id() -> field::Id {
    field::Id::new("add-people-query")
}

pub fn view(add_people: &AddPeople, motion: Motion) -> impl View<Intent> + '_ {
    let mut search = field(tr!("Type the name of a friend"), &add_people.query)
        .id(query_id())
        .on_input(Intent::Query)
        .text_size(typography::size::BODY)
        .pad(FIELD_PAD)
        .rim(false)
        .motion(motion);
    if add_people.may_add() {
        search = search.on_submit(Intent::Add);
    }
    let label = if add_people.adding {
        tr!("Adding...")
    } else if add_people.one_to_one {
        tr!("Create Group DM")
    } else {
        tr!("Add to Group")
    };
    let mut add = action_button(label, Kind::Primary, motion)
        .id(iced::advanced::widget::Id::new(ADD))
        .width(Fill)
        .enabled(add_people.may_add());
    if add_people.may_add() {
        add = add.on_press(Intent::Add);
    }
    let mut parts: Vec<Element<'_, Intent>> = vec![search.view(), people(add_people, motion)];
    if !add_people.note.is_empty() {
        parts.push(
            text(add_people.note.as_str())
                .size(typography::size::LABEL)
                .width(Fill)
                .style(catalog::text::danger)
                .boxed(),
        );
    }
    parts.push(add.boxed());
    let body = column(parts).spacing(GAP).padding(PAD).height(Fill);
    container(column![heading(add_people, motion), body])
        .padding(BORDER)
        .width(WIDTH)
        .height(HEIGHT)
        .clip(true)
        .style(card)
}

/// The heading: the icon, the title with how many more fit under it, Close.
fn heading(add_people: &AddPeople, motion: Motion) -> Element<'_, Intent> {
    let left = add_people.left();
    let detail = if left > 0 {
        trn!(
            "You can add {n} more friend.",
            "You can add {n} more friends.",
            left
        )
    } else {
        tr!("This group has no room for more.").to_string()
    };
    let title = column![
        text(tr!("Select Friends"))
            .size(typography::size::TITLE)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary),
        text(detail)
            .size(typography::size::LABEL)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::muted),
    ]
    .spacing(TITLE_GAP)
    .width(Fill);
    let content = container(
        iced::widget::row![
            icon(Icon::UserPlus, ICON).style(|theme: &Theme, _status| iced::widget::svg::Style {
                color: Some(theme.palette().secondary),
            }),
            title,
            icon_button(Icon::X, tr!("Close"), motion)
                .size(CLOSE)
                .glyph(CLOSE_GLYPH)
                .tip_side(Side::Below)
                .on_press(Intent::Close)
                .view(),
        ]
        .spacing(HEAD_GAP)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([0.0, HEAD_END]).left(metrics::SPACE_L))
    .height(HEADING - BORDER - 1.0)
    .center_y(HEADING - BORDER - 1.0)
    .width(Fill);
    // The line is the heading's last row (`:268`); the heading covers the
    // popover's top from inside its border, so its top corners follow it.
    let line = container("")
        .width(Fill)
        .height(1)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().edge)),
            ..container::Style::default()
        });
    container(column![content, line])
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().panel)),
            border: Border {
                radius: border::top(RADIUS - BORDER),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .boxed()
}

/// The friends to pick from, or why there are none.
fn people(add_people: &AddPeople, motion: Motion) -> Element<'_, Intent> {
    if add_people.people.is_empty() {
        let words = if add_people.query.is_empty() {
            tr!("All your friends are here already.")
        } else {
            tr!("No friends match.")
        };
        return container(
            text(words)
                .size(typography::size::LABEL)
                .style(catalog::text::muted),
        )
        .padding(metrics::SPACE_S)
        .height(Fill)
        .boxed();
    }
    let people = &add_people.people;
    list(
        people.len(),
        |index| {
            let mut hasher = DefaultHasher::new();
            people[index].id.hash(&mut hasher);
            hasher.finish()
        },
        move |index| row::view(&people[index], add_people.may_choose(&people[index])),
        motion,
    )
    .estimate(row::HEIGHT)
    .spacing(ROWS_GAP)
    .height(Fill)
    .boxed()
}

/// `Theme.sidebar` with the popover's edge, corners and shadow (24 px
/// blur, 8 px down).
fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.sidebar)),
        border: Border::default()
            .rounded(RADIUS)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: SHADOW,
            offset: Vector::new(0.0, SHADOW_DOWN),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}

/// Its shots for `anomi-shots`: the whole window over the popover. No
/// golden screenshot: compare with the Slint app (header button "Add
/// friends to DM").
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::Vm;
    use anomi_vm::fixtures::people::add_people as fixtures;
    use anomi_vm::fixtures::shell;
    use anomi_vm::menu::Spot;
    use anomi_vm::shell::Layer;

    use crate::shots::{GOLDEN, Shot};

    fn over(add_people: AddPeople) -> Vm {
        let mut vm = shell::open_at(
            shell::dm_home(),
            Layer::AddPeople,
            // The header's add button; the popover's right edge is its.
            Spot::below(iced::Rectangle::new(
                iced::Point::new(1310.0, 8.0),
                iced::Size::new(32.0, 32.0),
            ))
            .align(anomi_vm::menu::Align::End),
        );
        if let anomi_vm::header::Header::Open(place) = &mut vm.header
            && let Some(tool) = place.tools.add_people.as_mut()
        {
            tool.open = true;
        }
        vm.people.add_people = add_people;
        vm
    }
    let shot = |name, vm| Shot {
        area: "header",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("add_people", || over(fixtures::empty())),
        shot("add_people_chosen", || over(fixtures::chosen())),
        shot("add_people_limit", || over(fixtures::at_limit())),
        shot("add_people_searched", || over(fixtures::searched())),
        shot("add_people_none", || over(fixtures::no_match())),
        shot("add_people_adding", || over(fixtures::adding())),
        shot("add_people_failed", || over(fixtures::failed())),
    ]
}
