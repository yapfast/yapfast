//! What a plugin draws: a tree of rows of cells, for a slot in the window or
//! for a widget setting on its page.
//!
//! The tree is the plugin's own data (`yapfast_core::plugins::ui`): a cell
//! names a colour by tone and nothing else, so it is drawn with the theme's
//! tokens and the kit's widgets. Interactive cells report what the person did
//! with them by their own `id`, which the caller turns into the message that
//! carries it back to the plugin.
//!
//! What the window keeps of a drawing between the plugin's own draws is a
//! [`Local`]: the text typed into its fields, which tab shows, which section is
//! folded and which drop-down is open. A field draws what was typed at once,
//! not when the plugin draws again, and a tab or a fold costs the plugin
//! nothing.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use iced::{
    Alignment, Color, Element, Length, font,
    widget::{Space, button, column, container, image, rich_text, row, text, tooltip},
};
use yapfast_core::plugins::ui::{Cell, Kind, Row, Tone, Tree, parse_colour};

use crate::{
    controls::{self, Kind as ActionKind},
    fonts::{BOLD, ITALIC, REGULAR},
    form,
    look::Look,
    style,
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
    /// Something only the window keeps changed: a tab chosen, a section
    /// folded, a drop-down opened (`key` names which, `value` its state).
    Local { key: String, value: usize },
}

/// What the window keeps of one drawing between the plugin's draws.
#[derive(Default)]
pub struct Local {
    /// What a field holds, by its cell's id, until the plugin draws again.
    pub drafts: BTreeMap<String, String>,
    /// The editors of the text areas, by their cell's id; made before the
    /// drawing is ([`Local::prepare`]), since the view only borrows them.
    pub areas: BTreeMap<String, crate::field::Draft>,
    /// Which tab shows (`tab:<id>`), whether a section is open (`fold:<id>`)
    /// and whether a drop-down is (`open:<id>`).
    pub open: BTreeMap<String, usize>,
    /// Values the window holds for cells by their id, over the plugin's
    /// own: a plugin's settings page shows each setting's saved value in the
    /// cell named by its key.
    pub bound: BTreeMap<String, String>,
}

impl std::fmt::Debug for Local {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Local")
            .field("drafts", &self.drafts)
            .field("open", &self.open)
            .finish_non_exhaustive()
    }
}

impl Local {
    /// Makes an editor for each text area `tree` has, holding the text the
    /// plugin gave it, and drops those of areas it no longer has.
    pub fn prepare(&mut self, tree: &Tree) {
        let areas: Vec<&Cell> = tree
            .cells()
            .into_iter()
            .filter(|cell| cell.kind == Kind::Textarea)
            .collect();
        self.areas
            .retain(|id, _| areas.iter().any(|cell| &cell.id == id));
        for cell in areas {
            let text = self.bound.get(&cell.id).unwrap_or(&cell.value);
            self.areas.entry(cell.id.clone()).or_insert_with(|| {
                let draft = crate::field::Draft::default();
                draft.reset(text);
                draft
            });
        }
    }

    /// Keeps what `act` changes here; true when it is the plugin's to hear
    /// of too.
    pub fn apply(&mut self, act: &Act) -> bool {
        match act {
            // An empty key is a text area's caret moving: nothing to keep.
            Act::Local { key, .. } if key.is_empty() => false,
            Act::Local { key, value } => {
                self.open.insert(key.clone(), *value);
                false
            }
            Act::Edit { id, value } => {
                self.drafts.insert(id.clone(), value.clone());
                true
            }
            Act::Set { id, .. } => {
                // A choice closes the list it was made in, and the field
                // shows what the plugin settles on from now on.
                self.open.remove(&format!("open:{id}"));
                true
            }
        }
    }

    fn shown(&self, key: &str) -> Option<usize> {
        self.open.get(key).copied()
    }

    /// What a cell shows as its value: the text typed into it, else what the
    /// window holds for it, else the plugin's.
    fn value<'a>(&'a self, cell: &'a Cell) -> &'a str {
        self.drafts
            .get(&cell.id)
            .or_else(|| self.bound.get(&cell.id))
            .map_or(cell.value.as_str(), String::as_str)
    }

    /// Whether a toggle shows on.
    fn checked(&self, cell: &Cell) -> bool {
        self.bound
            .get(&cell.id)
            .map_or(cell.checked, |value| value == "true")
    }
}

/// The pictures a plugin's image cells show, by `<plugin>\u{1f}<src>`, made
/// from the plugin's own files; one that couldn't be read is `None`.
pub type Images = BTreeMap<String, Option<image::Handle>>;

/// The key a picture is kept under.
pub fn image_key(plugin: &str, src: &str) -> String {
    format!("{plugin}\u{1f}{src}")
}

/// What drawing a tree reads besides the tree.
#[derive(Clone, Copy)]
pub struct Paint<'a> {
    pub look: Look<'a>,
    pub local: &'a Local,
    pub images: &'a Images,
    /// The plugin that drew it, whose pictures it may show.
    pub plugin: &'a str,
}

type Sender<'a, M> = std::rc::Rc<dyn Fn(Act) -> M + 'a>;

/// Draws `tree`, its fields showing what `paint.local` keeps for them.
pub fn draw<'a, M: Clone + 'a>(
    paint: Paint<'a>,
    tree: &'a Tree,
    message: impl Fn(Act) -> M + 'a,
) -> Element<'a, M> {
    let message: Sender<'a, M> = std::rc::Rc::new(message);
    rows_view(paint, &tree.rows, &message)
}

fn rows_view<'a, M: Clone + 'a>(
    paint: Paint<'a>,
    lines: &'a [Row],
    message: &Sender<'a, M>,
) -> Element<'a, M> {
    let theme = paint.look.theme;
    let mut rows = column![].spacing(6).width(Length::Fill);
    for line in lines {
        // A row's cells sit side by side while they fit, and the ones that
        // don't move to a line of their own: every cell keeps its natural
        // width, so a narrow card stacks a row rather than crushing its text
        // into a column a letter wide. A container takes the row's width.
        let wide = line.cells.iter().any(|cell| wide(cell.kind));
        let drawn: Element<'a, M> = if wide {
            let mut cells = column![].spacing(6).width(Length::Fill);
            for cell in &line.cells {
                cells = cells.push(cell_view(paint, cell, message));
            }
            cells.into()
        } else {
            let mut cells = row![]
                .spacing(8)
                .align_y(Alignment::Center)
                .width(Length::Fill);
            for cell in &line.cells {
                cells = cells.push(cell_view(paint, cell, message));
            }
            cells.wrap().vertical_spacing(6).into()
        };
        let mut drawn = drawn;
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

/// A cell that takes its row's whole width rather than its own.
fn wide(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Tabs | Kind::Section | Kind::Grid | Kind::Table | Kind::Textarea | Kind::List
    )
}

/// Draws `tree` on one line at its natural width, every row's cells after
/// each other: for the slots that hold only buttons (beside the composer, on
/// a message's hover bar), which sit in a row of Yapfast's own buttons and
/// must neither stretch nor be squeezed by it.
pub fn draw_inline<'a, M: Clone + 'a>(
    paint: Paint<'a>,
    tree: &'a Tree,
    message: impl Fn(Act) -> M + 'a,
) -> Element<'a, M> {
    let message: Sender<'a, M> = std::rc::Rc::new(message);
    let mut cells = row![].spacing(4).align_y(Alignment::Center);
    for cell in tree.rows.iter().flat_map(|line| &line.cells) {
        cells = cells.push(cell_view(paint, cell, &message));
    }
    cells.into()
}

/// One cell, with its tooltip when it has one.
fn cell_view<'a, M: Clone + 'a>(
    paint: Paint<'a>,
    cell: &'a Cell,
    message: &Sender<'a, M>,
) -> Element<'a, M> {
    let drawn = cell_body(paint, cell, message);
    if cell.tooltip.is_empty() {
        return drawn;
    }
    tooltip(
        drawn,
        controls::tip(paint.look.theme, cell.tooltip.as_str()),
        tooltip::Position::Top,
    )
    .into()
}

/// One cell, drawn for its kind alone.
#[expect(
    clippy::too_many_lines,
    reason = "one arm per kind of cell reads as the widget set's list"
)]
fn cell_body<'a, M: Clone + 'a>(
    paint: Paint<'a>,
    cell: &'a Cell,
    message: &Sender<'a, M>,
) -> Element<'a, M> {
    let theme = paint.look.theme;
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
        Kind::Text => prose(theme, &cell.text, 14.0, ink, false),
        Kind::Muted => prose(theme, &cell.text, 13.0, theme.muted, false),
        Kind::Heading => prose(theme, &cell.text, 15.0, ink, true),
        Kind::Code => {
            let ground = theme.sunken;
            container(
                text(cell.text.clone())
                    .size(13)
                    .font(iced::Font::MONOSPACE)
                    .color(ink),
            )
            .padding(8)
            .style(move |_| style::rounded(ground, 6))
            .into()
        }
        Kind::Badge => badge(theme, &cell.text, ink),
        Kind::Icon | Kind::Image => picture(paint, cell),
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
            button(
                text(cell.text.clone())
                    .size(14)
                    .color(accent)
                    .wrapping(text::Wrapping::None),
            )
            .padding(0)
            .on_press_maybe(cell.enabled.then(|| set(cell.id.clone())))
            .style(controls::ink_style(accent, lit))
            .into()
        }
        Kind::Toggle => {
            let checked = paint.local.checked(cell);
            let on = Some(set(if checked { "false" } else { "true" }.to_owned()));
            let toggle = form::toggle(theme, checked, cell.enabled.then_some(on).flatten());
            labelled(theme, &cell.text, toggle)
        }
        Kind::Select => {
            // A plugin's own list, as buttons with the chosen one accented.
            let mut choices = row![].spacing(4).align_y(Alignment::Center);
            let value = paint.local.value(cell);
            for option in &cell.options {
                let chosen = option == value;
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
        Kind::Dropdown => {
            let key = format!("open:{}", cell.id);
            let open = paint.local.shown(&key) == Some(1);
            let value = paint.local.value(cell);
            let current = cell.options.iter().position(|option| option == value);
            let options = cell.options.clone();
            let toggle = message(Act::Local {
                key,
                value: usize::from(!open),
            });
            let pick = {
                let options = options.clone();
                move |index: usize| set(options.get(index).cloned().unwrap_or_default())
            };
            let list = form::dropdown(options, current, open, toggle, pick)
                .width(Length::Fixed(FIELD_WIDTH))
                .enabled(cell.enabled)
                .view(paint.look);
            labelled_above(theme, &cell.text, list)
        }
        Kind::Input => {
            let id = cell.id.clone();
            let value = paint.local.value(cell);
            let message = message.clone();
            let pressed = id.clone();
            let submitted = set(value.to_owned());
            form::field(theme, format!("plugin-cell-{id}"), &cell.text, value)
                .on_input(move |text| {
                    message(Act::Edit {
                        id: pressed.clone(),
                        value: text,
                    })
                })
                .on_submit(submitted)
                .width(Length::Fill)
                .into()
        }
        Kind::Number => number(paint, cell, message),
        Kind::Textarea => {
            let Some(draft) = paint.local.areas.get(&cell.id) else {
                // Not made yet: the plugin's text, until the next frame.
                return prose(theme, &cell.value, 14.0, theme.muted, false);
            };
            let id = cell.id.clone();
            let message = message.clone();
            let colors = crate::field::Colors {
                ground: theme.sunken,
                text: if cell.enabled {
                    theme.text
                } else {
                    theme.muted
                },
                placeholder: theme.muted,
                selection: theme.selection,
            };
            let field = crate::field::Field::new(
                draft,
                iced::widget::Id::from(format!("plugin-area-{id}")),
                String::new(),
                move |event| match event {
                    crate::field::Event::Edited(text, _) => message(Act::Edit {
                        id: id.clone(),
                        value: text,
                    }),
                    _ => message(Act::Local {
                        key: String::new(),
                        value: 0,
                    }),
                },
                colors,
            )
            .enabled(cell.enabled)
            .breaks()
            .size(14.0)
            .line_height(20.0)
            .lines(4.0)
            .padding(iced::Padding::from([8.0, 10.0]))
            .radius(6.0)
            .ring(style::faded(theme.accent, 0.8));
            let sunken = theme.sunken;
            let area = container(field)
                .width(Length::Fill)
                .style(move |_| style::rounded(sunken, 6));
            labelled_above(theme, &cell.text, area.into())
        }
        Kind::Color => colour(paint, cell, message),
        Kind::Slider => {
            let span = (cell.max - cell.min).max(f32::EPSILON);
            let position =
                ((paint.local.value(cell).parse::<f32>().unwrap_or(cell.min) - cell.min) / span)
                    .clamp(0.0, 1.0);
            const STEPS: u32 = 1000;
            let current = (position * STEPS as f32).round() as u32;
            let (min, max) = (f64::from(cell.min), f64::from(cell.max));
            let label = cell.text.clone();
            let message = message.clone();
            let id = cell.id.clone();
            // The plugin's own number, not the share of the track: a slider
            // from 0 to 10 at 6 says 6.
            let shown = trim(min + (max - min) * f64::from(position));
            container(
                controls::slider(label, current, STEPS, move |chosen| {
                    let value = min + (max - min) * f64::from(chosen) / f64::from(STEPS);
                    message(Act::Set {
                        id: id.clone(),
                        value: trim(value),
                    })
                })
                .shown(shown)
                .mark(false)
                .view(theme),
            )
            .width(Length::Fixed(SLIDER_WIDTH))
            .into()
        }
        Kind::Progress => {
            let span = (cell.max - cell.min).max(f32::EPSILON);
            let done =
                ((cell.value.parse::<f32>().unwrap_or(cell.min) - cell.min) / span).clamp(0.0, 1.0);
            let fill = if cell.tone == Tone::Normal {
                theme.accent
            } else {
                ink
            };
            let ground = theme.sunken;
            let bar = container(
                container(Space::new())
                    .width(Length::Fixed(200.0 * done))
                    .height(Length::Fixed(6.0))
                    .style(move |_| style::rounded(fill, 3.0)),
            )
            .width(Length::Fixed(200.0))
            .height(Length::Fixed(6.0))
            .style(move |_| style::rounded(ground, 3.0));
            if cell.text.is_empty() {
                bar.into()
            } else {
                column![text(cell.text.clone()).size(12).color(theme.muted), bar]
                    .spacing(4)
                    .into()
            }
        }
        Kind::Spinner => spinner(theme, &cell.text, ink),
        Kind::Divider => {
            let line = theme.line;
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fixed(1.0))
                .style(move |_| style::fill(line))
                .into()
        }
        Kind::Spacer => Space::new().width(Length::Fixed(8.0)).into(),
        Kind::List => {
            let mut items = column![].spacing(4).width(Length::Fill);
            for (index, item) in cell.items.iter().enumerate() {
                let mark = if cell.ordered {
                    format!("{}.", index + 1)
                } else {
                    "\u{2022}".to_owned()
                };
                items = items.push(
                    row![
                        text(mark)
                            .size(14)
                            .color(theme.muted)
                            .width(Length::Fixed(22.0)),
                        container(prose(theme, item, 14.0, ink, false)).width(Length::Fill),
                    ]
                    .spacing(4),
                );
            }
            items.into()
        }
        Kind::Table => table(theme, cell, ink),
        Kind::Tabs => tabs(paint, cell, message),
        Kind::Section => section(paint, cell, message),
        Kind::Grid => {
            let columns = usize::from(cell.columns.max(1));
            let mut grid = column![].spacing(8).width(Length::Fill);
            for chunk in cell.cells.chunks(columns) {
                let mut line = row![].spacing(8).width(Length::Fill);
                for inner in chunk {
                    line = line.push(
                        container(cell_view(paint, inner, message))
                            .width(Length::FillPortion(1))
                            .clip(true),
                    );
                }
                // A short last line keeps its cells the width of the rest.
                for _ in chunk.len()..columns {
                    line = line.push(Space::new().width(Length::FillPortion(1)));
                }
                grid = grid.push(line);
            }
            grid.into()
        }
    }
}

/// How wide a field, a drop-down or a slider is drawn: wide enough to use,
/// narrow enough to sit beside another cell.
const SLIDER_WIDTH: f32 = 200.0;
const FIELD_WIDTH: f32 = 200.0;

/// A small mark in its tone on the raised ground.
fn badge<'a, M: Clone + 'a>(theme: &Theme, words: &str, ink: Color) -> Element<'a, M> {
    let ground = theme.raised;
    container(text(words.to_owned()).size(11).font(BOLD).color(ink))
        .padding([2, 6])
        .style(move |_| style::rounded(ground, 999))
        .into()
}

/// A control with its label before it, the label taking what the control
/// leaves and wrapping, so the control keeps its size however long it is.
fn labelled<'a, M: Clone + 'a>(
    theme: &Theme,
    label: &str,
    control: Element<'a, M>,
) -> Element<'a, M> {
    if label.is_empty() {
        return control;
    }
    row![
        text(label.to_owned())
            .size(14)
            .color(theme.text)
            .width(Length::Fill)
            .wrapping(text::Wrapping::WordOrGlyph),
        control
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

/// A control with its label over it, as the kit's forms put a field's.
fn labelled_above<'a, M: Clone + 'a>(
    theme: &Theme,
    label: &str,
    control: Element<'a, M>,
) -> Element<'a, M> {
    if label.is_empty() {
        return control;
    }
    column![
        text(label.to_owned())
            .size(12)
            .font(BOLD)
            .color(theme.muted),
        control
    ]
    .spacing(4)
    .into()
}

/// An icon or an image: the plugin's own picture once it is read, and until
/// then (or for a picture from the web, which the runtime fetches) its room
/// with its label.
fn picture<'a, M: Clone + 'a>(paint: Paint<'a>, cell: &'a Cell) -> Element<'a, M> {
    let theme = paint.look.theme;
    let square = cell.kind == Kind::Icon;
    let (width, height) = if square {
        (16.0, 16.0)
    } else {
        (IMAGE_WIDTH, IMAGE_HEIGHT)
    };
    let found = (!cell.src.is_empty())
        .then(|| paint.images.get(&image_key(paint.plugin, &cell.src)))
        .flatten()
        .and_then(Option::as_ref);
    if let Some(handle) = found {
        return image(handle.clone())
            .width(width)
            .height(height)
            .content_fit(iced::ContentFit::Contain)
            .into();
    }
    let ground = theme.raised;
    // A picture from the web that hasn't come yet says so, where the plugin
    // gave it no words of its own.
    let loading = !paint
        .images
        .contains_key(&image_key(paint.plugin, &cell.src))
        && matches!(
            yapfast_core::plugins::ui::source(&cell.src),
            Some(yapfast_core::plugins::ui::Source::Web { .. })
        );
    let label = if square {
        String::new()
    } else if loading && cell.text.is_empty() {
        "Loading picture…".to_owned()
    } else {
        cell.text.clone()
    };
    container(text(label).size(11).color(theme.muted))
        .width(width)
        .height(height)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| style::rounded(ground, 6))
        .into()
}

/// The largest an image cell draws: wide enough to read a chart, small
/// enough for a card.
pub const IMAGE_WIDTH: f32 = 240.0;
pub const IMAGE_HEIGHT: f32 = 135.0;

/// A number field with steppers: the typed text shows at once, and the
/// plugin hears a number once it is one and in range.
fn number<'a, M: Clone + 'a>(
    paint: Paint<'a>,
    cell: &'a Cell,
    message: &Sender<'a, M>,
) -> Element<'a, M> {
    let theme = paint.look.theme;
    let id = cell.id.clone();
    let value = paint.local.value(cell);
    let ranged = cell.min < cell.max && !(cell.min == 0.0 && cell.max == 1.0);
    let clamp = move |number: f64| {
        if ranged {
            number.clamp(f64::from(cell.min), f64::from(cell.max))
        } else {
            number
        }
    };
    let step = if cell.step > 0.0 {
        f64::from(cell.step)
    } else {
        1.0
    };
    let now = value.trim().parse::<f64>().ok();
    let stepped = |by: f64| {
        let next = clamp(now.unwrap_or(0.0) + by);
        cell.enabled.then(|| {
            message(Act::Set {
                id: id.clone(),
                value: trim(next),
            })
        })
    };
    let (down, up) = (stepped(-step), stepped(step));
    let edit = {
        let message = message.clone();
        let id = id.clone();
        move |typed: String| {
            // Only what could become a number is kept.
            let typed: String = typed
                .chars()
                .filter(|c| c.is_ascii_digit() || matches!(c, '-' | '.'))
                .collect();
            message(Act::Edit {
                id: id.clone(),
                value: typed,
            })
        }
    };
    let submitted = now.map(|number| {
        message(Act::Set {
            id: id.clone(),
            value: trim(clamp(number)),
        })
    });
    let mut field =
        form::field(theme, format!("plugin-number-{id}"), "0", value).width(Length::Fixed(96.0));
    if cell.enabled {
        field = field.on_input(edit);
        if let Some(submitted) = submitted {
            field = field.on_submit(submitted);
        }
    }
    let control = row![
        controls::action(theme, "\u{2212}", ActionKind::Plain, down).small(),
        field,
        controls::action(theme, "+", ActionKind::Plain, up).small(),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    labelled(theme, &cell.text, control.into())
}

/// A few colours to choose with one click, beside the code's field.
const SWATCHES: [u32; 8] = [
    0xe5_484d, 0xf7_6b15, 0xf5_d90a, 0x46_a758, 0x12_a594, 0x00_90ff, 0x8e_4ec6, 0x8b_8d98,
];

/// A colour: its swatch, the code's field, and a row of colours to pick.
fn colour<'a, M: Clone + 'a>(
    paint: Paint<'a>,
    cell: &'a Cell,
    message: &Sender<'a, M>,
) -> Element<'a, M> {
    let theme = paint.look.theme;
    let id = cell.id.clone();
    let typed = paint.local.value(cell);
    let parsed = parse_colour(typed);
    let shown = parsed.map_or(theme.sunken, argb);
    let edit = {
        let message = message.clone();
        let id = id.clone();
        move |typed: String| {
            message(Act::Edit {
                id: id.clone(),
                value: typed,
            })
        }
    };
    let edge = theme.line;
    let swatch = container(Space::new())
        .width(24)
        .height(24)
        .style(move |_| iced::widget::container::Style {
            background: Some(shown.into()),
            border: style::edge(edge, 6.0),
            ..Default::default()
        });
    let wrong = !typed.is_empty() && parsed.is_none();
    let mut field = form::field(theme, format!("plugin-colour-{id}"), "#rrggbb", typed)
        .width(Length::Fixed(100.0));
    if cell.enabled {
        field = field.on_input(edit);
        if let Some(colour) = parsed {
            field = field.on_submit(message(Act::Set {
                id: id.clone(),
                value: hex(colour),
            }));
        }
    }
    let mut picks = row![].spacing(4);
    for rgb in SWATCHES {
        let colour = argb(0xff00_0000 | rgb);
        let chosen = parsed.is_some_and(|value| value & 0x00ff_ffff == rgb);
        let ring = if chosen {
            theme.text
        } else {
            Color::TRANSPARENT
        };
        let code = format!("#{rgb:06x}");
        picks = picks.push(
            button(Space::new().width(16).height(16))
                .padding(0)
                .on_press_maybe(cell.enabled.then(|| {
                    message(Act::Set {
                        id: id.clone(),
                        value: code.clone(),
                    })
                }))
                .style(move |_, _| button::Style {
                    background: Some(colour.into()),
                    border: iced::Border {
                        color: ring,
                        width: 2.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                }),
        );
    }
    let mut body = column![
        row![swatch, field, picks]
            .spacing(8)
            .align_y(Alignment::Center)
    ]
    .spacing(2);
    if wrong {
        body = body.push(text("Not a colour").size(12).color(theme.danger));
    }
    labelled_above(theme, &cell.text, body.into())
}

/// An ARGB colour as iced's.
fn argb(colour: u32) -> Color {
    let [a, r, g, b] = colour.to_be_bytes();
    Color::from_rgba8(r, g, b, f32::from(a) / 255.0)
}

/// An ARGB colour as the `#rrggbb` a plugin reads.
fn hex(colour: u32) -> String {
    format!("#{:06x}", colour & 0x00ff_ffff)
}

/// Something going on: eight dots turning, beside what is going on.
fn spinner<'a, M: Clone + 'a>(theme: &Theme, words: &str, ink: Color) -> Element<'a, M> {
    let (faint, lit) = (
        theme.line,
        if ink == theme.text { theme.accent } else { ink },
    );
    let started = *SPUN;
    let turning = crate::widgets::gauge(Duration::from_millis(100), move |renderer, bounds| {
        use iced::advanced::{Renderer as _, renderer::Quad};
        const DOTS: usize = 8;
        let lead = (started.elapsed().as_millis() / 100) as usize % DOTS;
        let centre = bounds.center();
        let radius = bounds.width.min(bounds.height) / 2.0 - 2.0;
        for dot in 0..DOTS {
            let angle = dot as f32 / DOTS as f32 * std::f32::consts::TAU;
            let behind = (lead + DOTS - dot) % DOTS;
            let colour = if behind == 0 {
                lit
            } else {
                style::faded(lit, 1.0 - behind as f32 / DOTS as f32 * 0.8)
            };
            let colour = if behind > DOTS / 2 { faint } else { colour };
            renderer.fill_quad(
                Quad {
                    bounds: iced::Rectangle {
                        x: centre.x + radius * angle.cos() - 1.5,
                        y: centre.y + radius * angle.sin() - 1.5,
                        width: 3.0,
                        height: 3.0,
                    },
                    border: style::round(2),
                    ..Default::default()
                },
                colour,
            );
        }
    })
    .width(16)
    .height(16);
    if words.is_empty() {
        return turning.into();
    }
    row![turning, text(words.to_owned()).size(13).color(theme.muted)]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}

/// When the spinners started, so every one turns in step.
static SPUN: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

/// A table: its column names over its rows, each cell markdown.
fn table<'a, M: Clone + 'a>(theme: &Theme, cell: &'a Cell, ink: Color) -> Element<'a, M> {
    let columns = cell
        .header
        .len()
        .max(cell.table.iter().map(Vec::len).max().unwrap_or(0))
        .max(1);
    let mut lines = column![].width(Length::Fill);
    if !cell.header.is_empty() {
        let mut names = row![].spacing(8).width(Length::Fill);
        for index in 0..columns {
            let name = cell.header.get(index).cloned().unwrap_or_default();
            names = names.push(
                text(name)
                    .size(12)
                    .font(BOLD)
                    .color(theme.muted)
                    .width(Length::FillPortion(1)),
            );
        }
        lines = lines.push(container(names).padding([6, 10]));
    }
    for (index, values) in cell.table.iter().enumerate() {
        let mut line = row![].spacing(8).width(Length::Fill);
        for column_index in 0..columns {
            let value = values.get(column_index).map_or("", String::as_str);
            line = line.push(
                container(prose(theme, value, 13.0, ink, false)).width(Length::FillPortion(1)),
            );
        }
        // Every other row on the hover ground, so a wide table reads across.
        let striped = index % 2 == 1;
        let stripe = theme.hover;
        lines = lines.push(
            container(line)
                .padding([6, 10])
                .width(Length::Fill)
                .style(move |_| {
                    if striped {
                        style::fill(stripe)
                    } else {
                        iced::widget::container::Style::default()
                    }
                }),
        );
    }
    let ground = theme.raised;
    container(lines)
        .width(Length::Fill)
        .clip(true)
        .style(move |_| style::rounded(ground, 8))
        .into()
}

/// Tabs: a row of their labels, the chosen one raised, over the chosen
/// tab's rows. Which one is chosen is the window's to keep.
fn tabs<'a, M: Clone + 'a>(
    paint: Paint<'a>,
    cell: &'a Cell,
    message: &Sender<'a, M>,
) -> Element<'a, M> {
    let theme = paint.look.theme;
    let key = format!("tab:{}", cell.id);
    let chosen = paint
        .local
        .shown(&key)
        .unwrap_or(0)
        .min(cell.tabs.len().saturating_sub(1));
    let mut labels = row![].spacing(4).align_y(Alignment::Center);
    for (index, pane) in cell.tabs.iter().enumerate() {
        let on = index == chosen;
        let ink = if on { theme.text } else { theme.muted };
        let ground = theme.raised;
        labels = labels.push(
            button(
                container(text(pane.label.clone()).size(13).font(BOLD).color(ink))
                    .padding([5, 10])
                    .style(move |_| {
                        if on {
                            style::rounded(ground, 6)
                        } else {
                            iced::widget::container::Style::default()
                        }
                    }),
            )
            .padding(0)
            .on_press(message(Act::Local {
                key: key.clone(),
                value: index,
            }))
            .style(controls::ink_style(Color::TRANSPARENT, Color::TRANSPARENT)),
        );
    }
    let line = theme.line;
    let body = cell.tabs.get(chosen).map_or_else(
        || Space::new().into(),
        |pane| rows_view(paint, &pane.rows, message),
    );
    column![
        labels.wrap(),
        container(Space::new())
            .width(Length::Fill)
            .height(1)
            .style(move |_| style::fill(line)),
        body
    ]
    .spacing(8)
    .width(Length::Fill)
    .into()
}

/// A section: its heading, which folds it, over its rows while it is open.
fn section<'a, M: Clone + 'a>(
    paint: Paint<'a>,
    cell: &'a Cell,
    message: &Sender<'a, M>,
) -> Element<'a, M> {
    let theme = paint.look.theme;
    let key = format!("fold:{}", cell.id);
    let open = paint
        .local
        .shown(&key)
        .map_or(!cell.collapsed, |open| open == 1);
    let mark = paint.look.glyph(
        if open {
            crate::icons::Icon::ChevronDown
        } else {
            crate::icons::Icon::ChevronRight
        },
        14,
        theme.muted,
    );
    let heading = button(
        row![
            container(mark).width(14),
            text(cell.text.clone())
                .size(14)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill)
                .wrapping(text::Wrapping::WordOrGlyph),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([4, 0])
    .width(Length::Fill)
    .on_press(message(Act::Local {
        key,
        value: usize::from(!open),
    }))
    .style(controls::ink_style(theme.text, theme.text));
    let mut drawn = column![heading].spacing(6).width(Length::Fill);
    if open {
        drawn = drawn.push(row![
            Space::new().width(20),
            rows_view(paint, &cell.rows, message)
        ]);
    }
    drawn.into()
}

/// A text cell: markdown, as the API says (`ui::Cell::text`), drawn in the
/// app's own markdown with bold, italic, struck and code runs; a link's
/// words show in the accent, since a slot's links are its link cells.
/// It wraps at words at the width its row gives it.
fn prose<'a, M: Clone + 'a>(
    theme: &Theme,
    words: &str,
    size: f32,
    ink: Color,
    heading: bool,
) -> Element<'a, M> {
    let body = crate::markdown::parse(words, &[], 0, "plugin", |_| false);
    // Words with no styling are plain text, which finds and reads as such.
    let plain = body.parts.len() == 1
        && body.parts[0]
            .runs
            .iter()
            .all(|run| !(run.bold || run.italic || run.strike || run.code || run.link.is_some()));
    if plain {
        return text(body.plain())
            .size(size)
            .color(ink)
            .font(if heading { BOLD } else { REGULAR })
            .wrapping(text::Wrapping::WordOrGlyph)
            .into();
    }
    let mut spans: Vec<text::Span<'static, (), iced::Font>> = Vec::new();
    for (index, part) in body.parts.iter().enumerate() {
        if index > 0 {
            spans.push(text::Span::new("\n"));
        }
        for run in &part.runs {
            let mut face = if run.code {
                iced::Font::MONOSPACE
            } else if run.bold || heading {
                BOLD
            } else if run.italic {
                ITALIC
            } else {
                REGULAR
            };
            if run.italic && (run.bold || heading || run.code) {
                face.style = font::Style::Italic;
            }
            let mut span = text::Span::new(run.text.clone())
                .font(face)
                .color(if run.link.is_some() {
                    theme.accent
                } else {
                    ink
                })
                .strikethrough(run.strike);
            if run.link.is_some() {
                span = span.underline(true);
            }
            spans.push(span);
        }
    }
    rich_text(spans)
        .size(size)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A text cell's markdown is drawn styled: the asterisks a plugin wrote
    /// are its bold and italic, not characters on screen.
    #[test]
    fn a_text_cell_draws_its_markdown_without_the_marks() {
        let body = crate::markdown::parse(
            "Your **three** channels, and what's *new*.",
            &[],
            0,
            "plugin",
            |_| false,
        );
        let plain = body.plain();
        assert!(!plain.contains('*'), "{plain}");
        let runs = &body.parts[0].runs;
        assert!(runs.iter().any(|run| run.bold && run.text == "three"));
        assert!(runs.iter().any(|run| run.italic && run.text == "new"));
    }

    /// What the window keeps of a drawing: a typed draft is the plugin's to
    /// hear of, a chosen tab isn't, and a choice closes its list.
    #[test]
    fn local_state_keeps_drafts_tabs_and_folds() {
        let mut local = Local::default();
        assert!(local.apply(&Act::Edit {
            id: "name".into(),
            value: "Ad".into()
        }));
        assert_eq!(local.drafts["name"], "Ad");
        assert!(!local.apply(&Act::Local {
            key: "tab:pages".into(),
            value: 2
        }));
        assert_eq!(local.shown("tab:pages"), Some(2));
        local.apply(&Act::Local {
            key: "open:pick".into(),
            value: 1,
        });
        assert!(local.apply(&Act::Set {
            id: "pick".into(),
            value: "b".into()
        }));
        assert_eq!(local.shown("open:pick"), None, "a choice closes its list");
        let tree = Tree::parse(
            r#"{ "rows": [{ "cells": [{ "kind": "textarea", "id": "notes", "value": "one\ntwo" }] }] }"#,
        )
        .unwrap();
        local.prepare(&tree);
        assert_eq!(local.areas["notes"].text(), "one\ntwo");
        local.prepare(&Tree::default());
        assert!(local.areas.is_empty(), "an area gone takes its editor");
    }

    #[test]
    fn colours_read_and_write_as_a_plugin_does() {
        let red = parse_colour("#ff0000").unwrap();
        assert_eq!(hex(red), "#ff0000");
        assert_eq!(argb(red), Color::from_rgb8(255, 0, 0));
    }
}
