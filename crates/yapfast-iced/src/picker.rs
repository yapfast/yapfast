//! The emoji picker, as the Slint client's `EmojiPicker`
//! (`ui/messages.slint`) and `src/emoji.rs` build it: a search field, a rail
//! of the categories down its left, the grid of Twemoji's pictures in
//! 56 px buttons under each category's heading, and the pointed emoji with
//! its name at the foot. From the composer's emoji button it opens over the
//! composer as the Slint client's expression picker (498 by 486, its Emoji
//! tab); from a message's Add Reaction, as its reaction popup (408 by 432)
//! at the menu.
//!
//! The grid is virtualized as the timeline is: only rows near the viewport
//! are built, and only their pictures are made, each once, at the size it
//! shows (36 px), from the Twemoji font's PNGs. The picker holds them alone;
//! closing it drops them all, as the Slint client lets go of its picker's.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use iced::{
    Alignment, Border, Element, Length, Padding, Point, Size,
    advanced::{graphics::text::cosmic_text, image::Handle},
    widget::{
        Id, Space, button, column, container, mouse_area, row, scrollable, scrollable::Viewport,
        text,
    },
};

use crate::{
    app::{Message, key},
    controls::lit_style,
    field::{self, Draft, Field},
    fonts::BOLD,
    paint, pictures,
    style::{raised_shadow, scrollbar},
    theme::Theme,
    timeline::MARGIN,
    widgets::{Pic, Rows},
};

/// The grid's scrollable and the search field, for the app's tasks.
pub const GRID: &str = "emoji-grid";
pub const FIELD: &str = "picker-field";
const CELL: f32 = 56.0;
const GLYPH: u32 = 36;
const HEADING: f32 = 28.0;
/// The rail and the grid's padding beside the cells, as `emoji::columns`.
const BESIDE: f32 = 64.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Composer,
    /// A reaction to the message with this item, the menu's place.
    React(String, Point),
}

#[derive(Clone, Debug)]
pub enum Event {
    Field(field::Event),
    Scrolled(Viewport),
    Hover(&'static str),
    Pick(&'static str),
    /// A category on the rail, by its place.
    Jump(usize),
}

pub enum Out {
    None,
    Scroll(f32),
    Close,
    Pick(&'static str),
}

/// A category: its rail icon, heading and emoji.
struct Category {
    icon: &'static str,
    name: &'static str,
    emoji: Vec<&'static str>,
}

/// Every category with the emoji Twemoji draws as one picture, in Unicode's
/// order, up to Unicode 15.0, as the Slint client offers them; worked out
/// once, the first time a picker opens.
fn catalog(system: &mut cosmic_text::FontSystem) -> &'static [Category] {
    static CATALOG: OnceLock<Vec<Category>> = OnceLock::new();
    if let Some(catalog) = CATALOG.get() {
        return catalog;
    }
    let newest = emojis::UnicodeVersion::new(15, 0);
    let catalog = emojis::Group::iter()
        .map(|group| {
            let (icon, name) = match group {
                emojis::Group::SmileysAndEmotion => ("😀", "Smileys & emotion"),
                emojis::Group::PeopleAndBody => ("👋", "People & body"),
                emojis::Group::AnimalsAndNature => ("🐻", "Animals & nature"),
                emojis::Group::FoodAndDrink => ("🍔", "Food & drink"),
                emojis::Group::TravelAndPlaces => ("✈️", "Travel & places"),
                emojis::Group::Activities => ("⚽", "Activities"),
                emojis::Group::Objects => ("💡", "Objects"),
                emojis::Group::Symbols => ("🔣", "Symbols"),
                emojis::Group::Flags => ("🏁", "Flags"),
            };
            let emoji = group
                .emojis()
                .filter(|emoji| {
                    emoji.unicode_version() <= newest && paint::has_emoji(system, emoji.as_str())
                })
                .map(emojis::Emoji::as_str)
                .collect();
            Category { icon, name, emoji }
        })
        .collect();
    CATALOG.get_or_init(|| catalog)
}

/// An emoji's name as the picker's foot shows it: "Grinning face".
fn name(key: &str) -> String {
    let name = emojis::get(key).map_or(key, emojis::Emoji::name);
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

enum Line {
    Heading(&'static str),
    /// Emoji of `Picker::shown`, from and to.
    Cells(usize, usize),
}

struct Row {
    top: f32,
    height: f32,
    line: Line,
}

pub struct Picker {
    pub target: Target,
    pub field: Draft,
    catalog: &'static [Category],
    query: String,
    /// The emoji the grid shows, in order, and its rows.
    shown: Vec<&'static str>,
    rows: Vec<Row>,
    /// Where each category's rows start and end, for the rail.
    sections: Vec<(f32, f32)>,
    total: f32,
    scrolled: f32,
    height: f32,
    pub hover: Option<&'static str>,
    /// Pictures of the rows built, by emoji and size; the rest go.
    pictures: RefCell<HashMap<(&'static str, u32), Handle>>,
    used: RefCell<HashSet<(&'static str, u32)>>,
}

impl Picker {
    pub fn new(target: Target) -> Self {
        // The font system's lock is let go of before the field's editor,
        // which takes it too, is made.
        let catalog = catalog(
            iced::advanced::graphics::text::font_system()
                .write()
                .expect("font system")
                .raw(),
        );
        let mut picker = Self {
            target,
            field: Draft::default(),
            catalog,
            query: String::new(),
            shown: Vec::new(),
            rows: Vec::new(),
            sections: Vec::new(),
            total: 0.0,
            scrolled: 0.0,
            height: 300.0,
            hover: None,
            pictures: Default::default(),
            used: Default::default(),
        };
        picker.build();
        picker
    }

    /// The panel's size: the expression picker's, or the reaction popup's.
    pub fn size(&self) -> Size {
        match self.target {
            Target::Composer => Size::new(498.0, 486.0),
            Target::React(..) => Size::new(408.0, 432.0),
        }
    }

    fn columns(&self) -> usize {
        (((self.size().width - BESIDE) / CELL).floor() as usize).max(1)
    }

    /// The grid for the query, as `emoji::rows` lays it out: a heading row
    /// per category, then its emoji a row of buttons at a time; a search
    /// shows the matches alone.
    fn build(&mut self) {
        let columns = self.columns();
        let query = self.query.trim().trim_matches(':').to_lowercase();
        self.shown.clear();
        self.rows.clear();
        self.sections.clear();
        let mut top = 0.0;
        let mut section =
            |shown: &mut Vec<&'static str>,
             rows: &mut Vec<Row>,
             heading: Option<&'static str>,
             emoji: &mut dyn Iterator<Item = &'static str>| {
                let start = top;
                let first = shown.len();
                shown.extend(emoji);
                if shown.len() == first {
                    return (start, start);
                }
                if let Some(heading) = heading {
                    rows.push(Row {
                        top,
                        height: HEADING,
                        line: Line::Heading(heading),
                    });
                    top += HEADING;
                }
                let mut at = first;
                while at < shown.len() {
                    let end = (at + columns).min(shown.len());
                    rows.push(Row {
                        top,
                        height: CELL,
                        line: Line::Cells(at, end),
                    });
                    top += CELL;
                    at = end;
                }
                (start, top)
            };
        if query.is_empty() {
            for category in self.catalog {
                let range = section(
                    &mut self.shown,
                    &mut self.rows,
                    Some(category.name),
                    &mut category.emoji.iter().copied(),
                );
                self.sections.push(range);
            }
        } else {
            let mut matches = self
                .catalog
                .iter()
                .flat_map(|category| category.emoji.iter().copied())
                .filter(|key| {
                    emojis::get(key).is_some_and(|emoji| {
                        emoji.name().contains(&query)
                            || emoji.shortcodes().any(|code| code.contains(&query))
                    })
                });
            section(&mut self.shown, &mut self.rows, None, &mut matches);
        }
        self.total = top;
    }

    pub fn update(&mut self, event: Event) -> Out {
        match event {
            Event::Field(field::Event::Edited(text, _)) => {
                self.query = text;
                // The pointed cell may be gone from the new grid.
                self.hover = None;
                self.build();
                // The grid built for it starts at its top.
                self.scrolled = 0.0;
                Out::Scroll(0.0)
            }
            Event::Field(field::Event::Send) => {
                // Enter picks the first match.
                match self.shown.first() {
                    Some(first) if !self.query.is_empty() => Out::Pick(first),
                    _ => Out::None,
                }
            }
            Event::Field(field::Event::Escape) => Out::Close,
            Event::Field(_) => Out::None,
            Event::Scrolled(viewport) => {
                self.scrolled = viewport.absolute_offset().y;
                self.height = viewport.bounds().height;
                Out::None
            }
            Event::Hover(key) => {
                self.hover = Some(key);
                Out::None
            }
            Event::Pick(key) => Out::Pick(key),
            Event::Jump(index) => {
                if !self.query.is_empty() {
                    self.query.clear();
                    self.field.reset("");
                    self.build();
                }
                let top = self.sections.get(index).map_or(0.0, |(top, _)| *top);
                self.scrolled = top;
                Out::Scroll(top)
            }
        }
    }

    /// An emoji's picture at `size`, made once while its row is built.
    fn picture(&self, key: &'static str, size: u32) -> Option<Handle> {
        self.used.borrow_mut().insert((key, size));
        if let Some(handle) = self.pictures.borrow().get(&(key, size)) {
            return Some(handle.clone());
        }
        let canvas = {
            let mut system = iced::advanced::graphics::text::font_system()
                .write()
                .expect("font system");
            paint::emoji(system.raw(), key, size)?
        };
        let handle = pictures::handle(canvas);
        self.pictures
            .borrow_mut()
            .insert((key, size), handle.clone());
        Some(handle)
    }

    fn face(&self, key: &'static str, side: f32, size: u32) -> Element<'_, Message> {
        match self.picture(key, size) {
            Some(handle) => Pic::square(handle, size as f32).boxed(side).into(),
            None => Space::new().width(side).height(side).into(),
        }
    }

    /// The picker in `theme`'s colours, as the Slint client's
    /// `EmojiPicker`: the raised ground, the search field sunken, the rail
    /// and the foot on the panel's ground.
    pub fn view(&self, theme: &Theme) -> Element<'_, Message> {
        self.used.borrow_mut().clear();
        let size = self.size();
        let (raised, panel, edge) = (theme.raised, theme.panel, theme.edge);
        let (wash, active) = (theme.wash(0.12), theme.active);
        let field = Field::new(
            &self.field,
            Id::new(FIELD),
            "Find an emoji".into(),
            |event| Message::Picker(Event::Field(event)),
            field::Colors {
                ground: theme.sunken,
                text: theme.text,
                placeholder: theme.muted,
                selection: theme.selection,
            },
        )
        .single()
        .size(15.0)
        .padding(Padding {
            top: 5.0,
            right: 10.0,
            bottom: 5.0,
            left: 10.0,
        });
        let field = container(field).padding(Padding {
            top: 10.0,
            right: 10.0,
            bottom: 6.0,
            left: 10.0,
        });
        // The rail: a button per category, the one the grid is in marked.
        let mut rail = column![]
            .spacing(4)
            .padding(Padding {
                top: 4.0,
                ..Padding::ZERO
            })
            .align_x(Alignment::Center)
            .width(48);
        for (index, category) in self.catalog.iter().enumerate() {
            let (top, end) = self.sections.get(index).copied().unwrap_or_default();
            let current =
                self.query.is_empty() && self.scrolled >= top - 1.0 && self.scrolled < end - 1.0;
            rail = rail.push(
                button(self.face(category.icon, 36.0, 32))
                    .padding(0)
                    .width(36)
                    .height(36)
                    .on_press(Message::Picker(Event::Jump(index)))
                    .style(lit_style(current.then_some(active), wash, 6.0)),
            );
        }
        let rail = container(rail)
            .width(48)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(panel.into()),
                ..Default::default()
            });
        // The grid's rows near what shows.
        let (from, to) = (self.scrolled - MARGIN, self.scrolled + self.height + MARGIN);
        let first = self
            .rows
            .partition_point(|row| row.top + row.height <= from);
        let mut rows = Rows::new(self.total);
        for (index, line) in self.rows.iter().enumerate().skip(first) {
            if line.top >= to {
                break;
            }
            let content: Element<'_, Message> = match line.line {
                Line::Heading(name) => container(
                    text(name.to_uppercase())
                        .size(13)
                        .font(BOLD)
                        .color(theme.secondary),
                )
                .padding(Padding {
                    top: 0.0,
                    right: 8.0,
                    bottom: 2.0,
                    left: 10.0,
                })
                .height(HEADING)
                .align_y(Alignment::End)
                .into(),
                Line::Cells(start, end) => {
                    let mut cells = row![].padding(Padding {
                        left: 8.0,
                        ..Padding::ZERO
                    });
                    for &key in &self.shown[start..end] {
                        cells = cells.push(
                            mouse_area(
                                button(self.face(key, CELL, GLYPH))
                                    .padding(0)
                                    .width(CELL)
                                    .height(CELL)
                                    .on_press(Message::Picker(Event::Pick(key)))
                                    .style(lit_style(None, wash, 6.0)),
                            )
                            .on_enter(Message::Picker(Event::Hover(key))),
                        );
                    }
                    cells.into()
                }
            };
            let key = key(&format!("{}:{index}", self.query));
            rows.push(key, line.top, line.height, content);
        }
        let grid: Element<'_, Message> = if self.shown.is_empty() {
            container(
                text(format!("No emoji match \"{}\".", self.query))
                    .size(13)
                    .color(theme.muted),
            )
            .padding(16)
            .width(Length::Fill)
            .center_x(Length::Fill)
            .into()
        } else {
            scrollable(rows)
                .id(Id::new(GRID))
                .on_scroll(|viewport| Message::Picker(Event::Scrolled(viewport)))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(scrollbar(theme))
                .into()
        };
        // The pointed emoji, larger, with its name.
        let foot: Element<'_, Message> = match self.hover {
            Some(key) => row![
                self.face(key, 30.0, 26),
                text(name(key)).size(15).font(BOLD).color(theme.text)
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
            None => text(match self.target {
                Target::Composer => "Pick an emoji for your message.",
                Target::React(..) => "Pick an emoji to react with.",
            })
            .size(15)
            .color(theme.muted)
            .into(),
        };
        let foot = container(foot)
            .padding([0, 14])
            .height(48)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .style(move |_| container::Style {
                background: Some(panel.into()),
                ..Default::default()
            });
        let picker = column![field, row![rail, grid].height(Length::Fill), foot];
        let mut content = column![];
        if self.target == Target::Composer {
            // The expression picker's tabs, the Emoji one chosen; GIFs and
            // Stickers aren't in the port.
            let tab = |label: &'static str, chosen: bool| {
                container(text(label).size(15).font(BOLD).color(if chosen {
                    theme.text
                } else {
                    theme.secondary
                }))
                .padding([4, 10])
                .style(move |_| container::Style {
                    background: chosen.then(|| active.into()),
                    border: Border::default().rounded(4),
                    ..Default::default()
                })
            };
            content = content.push(
                container(
                    row![
                        tab("GIFs", false),
                        tab("Emoji", true),
                        tab("Stickers", false)
                    ]
                    .spacing(8),
                )
                .padding(Padding {
                    top: 16.0,
                    right: 16.0,
                    bottom: 12.0,
                    left: 16.0,
                }),
            );
        }
        let content = content.push(picker);
        let shown = container(content)
            .width(size.width)
            .height(size.height)
            .clip(true)
            .style(move |_| container::Style {
                background: Some(raised.into()),
                border: Border {
                    color: edge,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                shadow: raised_shadow(),
                ..Default::default()
            });
        // Pictures of rows no longer built go.
        let used = self.used.borrow();
        self.pictures
            .borrow_mut()
            .retain(|key, _| used.contains(key));
        shown.into()
    }
}
