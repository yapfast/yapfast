//! What a plugin draws: a tree of rows of cells, for a slot in the window or
//! for a widget setting on its page.
//!
//! The tree is the plugin's own data (`yapfast_core::plugins::ui`): a cell
//! names a colour by tone and nothing else, so it is drawn with the theme's
//! tokens and the kit's widgets. Interactive cells report what the person did
//! with them by their own `id`, which the caller turns into the message that
//! carries it back to the plugin.

use std::collections::BTreeMap;

use iced::{
    Alignment, Color, Element, Length,
    widget::{Space, button, column, container, row, text},
};
use yapfast_core::plugins::ui::{Cell, Kind, Tone, Tree};

use crate::{
    controls::{self, Kind as ActionKind},
    fonts::BOLD,
    form, style,
    theme::Theme,
};

/// What a person did with a cell, for whoever draws the tree to carry to the
/// plugin that drew it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// A button or a link was chosen, or a toggle, a select or a slider
    /// settled on a value: `value` says which.
    Set { id: String, value: String },
    /// A text cell took a keystroke, which its owner keeps until it submits.
    Edit { id: String, value: String },
}

/// Draws `tree`, its input cells showing what `drafts` keeps for them.
pub fn draw<'a, M: Clone + 'a>(
    theme: &Theme,
    tree: &'a Tree,
    drafts: &'a BTreeMap<String, String>,
    message: impl Fn(Act) -> M + 'a,
) -> Element<'a, M> {
    let message: std::rc::Rc<dyn Fn(Act) -> M> = std::rc::Rc::new(message);
    let mut rows = column![].spacing(6).width(Length::Fill);
    for line in &tree.rows {
        let mut cells = row![].spacing(8).align_y(Alignment::Center);
        for cell in &line.cells {
            cells = cells.push(cell_view(theme, cell, drafts, message.clone()));
        }
        let mut drawn: Element<'a, M> = cells.into();
        if line.framed {
            let ground = theme.raised;
            drawn = container(drawn)
                .padding(10)
                .width(Length::Fill)
                .style(move |_| style::rounded(ground, 8))
                .into();
        } else if line.indent > 0 {
            let indent = f32::from(line.indent.min(3)) * 16.0;
            drawn = row![Space::new().width(indent), drawn].into();
        }
        rows = rows.push(drawn);
    }
    rows.into()
}

/// One cell, drawn for its kind alone.
fn cell_view<'a, M: Clone + 'a>(
    theme: &Theme,
    cell: &'a Cell,
    drafts: &'a BTreeMap<String, String>,
    message: std::rc::Rc<dyn Fn(Act) -> M + 'a>,
) -> Element<'a, M> {
    let ink = tone_colour(theme, cell.tone);
    let set = {
        let message = message.clone();
        let id = cell.id.clone();
        move |value: String| {
            message(Act::Set {
                id: id.clone(),
                value,
            })
        }
    };
    match cell.kind {
        Kind::Text => prose(&cell.text, 14.0, ink),
        Kind::Muted => prose(&cell.text, 13.0, theme.muted),
        Kind::Heading => prose(&cell.text, 15.0, ink),
        Kind::Code => {
            let ground = theme.sunken;
            container(
                text(cell.text.clone())
                    .size(13)
                    .font(iced::Font::MONOSPACE)
                    .color(ink),
            )
            .padding(8)
            .width(Length::Fill)
            .style(move |_| style::rounded(ground, 6))
            .into()
        }
        Kind::Badge => {
            let ground = theme.raised;
            container(text(cell.text.clone()).size(11).font(BOLD).color(ink))
                .padding([2, 6])
                .style(move |_| style::rounded(ground, 999))
                .into()
        }
        Kind::Icon | Kind::Image => {
            // The picture is the plugin's asset, which the runtime fetches;
            // until it has one the cell keeps its room and shows its label.
            let square = cell.kind == Kind::Icon;
            let (width, height) = if square { (16.0, 16.0) } else { (120.0, 68.0) };
            let ground = theme.raised;
            container(text(cell.text.clone()).size(11).color(theme.muted))
                .width(width)
                .height(height)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .style(move |_| style::rounded(ground, 6))
                .into()
        }
        Kind::Button => controls::action(
            theme,
            cell.text.clone(),
            ActionKind::Plain,
            cell.enabled.then(|| set(cell.id.clone())),
        )
        .into(),
        Kind::Link => {
            let accent = theme.accent;
            let lit = theme.text;
            button(text(cell.text.clone()).size(14).color(accent))
                .padding(0)
                .on_press_maybe(cell.enabled.then(|| set(cell.id.clone())))
                .style(controls::ink_style(accent, lit))
                .into()
        }
        Kind::Toggle => {
            let on = Some(set(if cell.checked { "false" } else { "true" }.to_owned()));
            let toggle = form::toggle(theme, cell.checked, cell.enabled.then_some(on).flatten());
            if cell.text.is_empty() {
                toggle
            } else {
                row![
                    text(cell.text.clone()).size(14).color(theme.text),
                    Space::new().width(Length::Fill),
                    toggle
                ]
                .align_y(Alignment::Center)
                .into()
            }
        }
        Kind::Select => {
            // A plugin's own list, as buttons with the chosen one accented.
            let mut choices = row![].spacing(4).align_y(Alignment::Center);
            for option in &cell.options {
                let chosen = *option == cell.value;
                let on_press = cell.enabled.then(|| set(option.clone()));
                choices = choices.push(
                    button(text(option.clone()).size(13))
                        .padding([4, 8])
                        .on_press_maybe(on_press)
                        .style(controls::ink_style(
                            if chosen { theme.accent } else { theme.muted },
                            theme.text,
                        )),
                );
            }
            choices.into()
        }
        Kind::Input => {
            let id = cell.id.clone();
            let value = drafts.get(&id).map_or(cell.value.as_str(), String::as_str);
            let message = message.clone();
            let pressed = id.clone();
            form::field(theme, format!("plugin-cell-{id}"), &cell.text, value)
                .on_input(move |text| {
                    message(Act::Edit {
                        id: pressed.clone(),
                        value: text,
                    })
                })
                .width(Length::Fill)
                .into()
        }
        Kind::Slider => {
            let span = (cell.max - cell.min).max(f32::EPSILON);
            let position =
                ((cell.value.parse::<f32>().unwrap_or(cell.min) - cell.min) / span).clamp(0.0, 1.0);
            const STEPS: u32 = 1000;
            let current = (position * STEPS as f32).round() as u32;
            let (min, max) = (f64::from(cell.min), f64::from(cell.max));
            let label = cell.text.clone();
            let message = message.clone();
            let id = cell.id.clone();
            controls::slider(label, current, STEPS, move |chosen| {
                let value = min + (max - min) * f64::from(chosen) / f64::from(STEPS);
                message(Act::Set {
                    id: id.clone(),
                    value: trim(value),
                })
            })
            .view(theme)
        }
        Kind::Progress => {
            let span = (cell.max - cell.min).max(f32::EPSILON);
            let done =
                ((cell.value.parse::<f32>().unwrap_or(cell.min) - cell.min) / span).clamp(0.0, 1.0);
            let (ground, fill) = (theme.sunken, theme.accent);
            container(
                container(Space::new())
                    .width(Length::Fixed(200.0 * done))
                    .height(Length::Fixed(6.0))
                    .style(move |_| style::rounded(fill, 3.0)),
            )
            .width(Length::Fixed(200.0))
            .height(Length::Fixed(6.0))
            .style(move |_| style::rounded(ground, 3.0))
            .into()
        }
        Kind::Divider => {
            let line = theme.line;
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fixed(1.0))
                .style(move |_| style::fill(line))
                .into()
        }
        Kind::Spacer => Space::new().width(Length::Fixed(8.0)).into(),
    }
}

fn prose<'a, M: Clone + 'a>(words: &str, size: f32, ink: Color) -> Element<'a, M> {
    text(words.to_owned())
        .size(size)
        .color(ink)
        .width(Length::Fill)
        .wrapping(text::Wrapping::WordOrGlyph)
        .into()
}

/// A number as a plugin reads it: no trailing zeros.
fn trim(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A cell's tone as the theme's colour.
pub fn tone_colour(theme: &Theme, tone: Tone) -> Color {
    match tone {
        Tone::Normal => theme.text,
        Tone::Muted => theme.muted,
        Tone::Accent => theme.accent,
        Tone::Success => theme.live,
        Tone::Warning => theme.loud,
        Tone::Danger => theme.danger,
    }
}
