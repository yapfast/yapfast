//! A theme's `chrome` drawn around iced surfaces, as the Slint build
//! draws it: panes framed with box-drawing lines and a title in the top
//! edge, file-tree lines before list rows, marks before members by their
//! group, and a status line across the conversation's foot.
//!
//! Nothing here takes input or changes a surface's behaviour.

use iced::advanced::{Layout, Widget, layout, mouse, renderer, widget::Tree};
use iced::{Color, Element, Font, Length, Point, Rectangle, Size, Theme};
use yapfast_core::themes::chrome::{self, Chrome, Pane, TreeInk, TreeList};

use crate::app::Message;

/// The face the chrome's box lines and titles take: the theme's code face.
pub fn mono(ctx: crate::ctx::Ctx<'_>) -> Font {
    ctx.look.theme.font("code", Font::MONOSPACE)
}

fn chrome<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<&'a Chrome> {
    ctx.themes.chrome(&ctx.look.theme.id)
}

fn token(theme: &crate::theme::Theme, ink: TreeInk) -> Color {
    match ink {
        TreeInk::Faint => theme.faint,
        TreeInk::Muted => theme.muted,
        TreeInk::Secondary => theme.secondary,
        TreeInk::Text => theme.text,
        TreeInk::Accent => theme.accent,
    }
}

/// The colour of a theme's tree lines: its colour, its token, else faint.
pub fn tree_ink(ctx: crate::ctx::Ctx<'_>) -> Color {
    let theme = ctx.look.theme;
    let Some(chrome) = chrome(ctx) else {
        return theme.faint;
    };
    chrome
        .tree_colour
        .map(crate::themes::argb)
        .or_else(|| chrome.tree_ink.map(|ink| token(theme, ink)))
        .unwrap_or(theme.faint)
}

/// Whether a theme draws `list` as a file tree.
pub fn tree(ctx: crate::ctx::Ctx<'_>, list: TreeList) -> bool {
    chrome(ctx).is_some_and(|chrome| chrome.tree.contains(&list))
}

/// Whether a theme leaves `what` out.
pub fn hides(ctx: crate::ctx::Ctx<'_>, what: chrome::Hide) -> bool {
    chrome(ctx).is_some_and(|chrome| chrome.hide.contains(&what))
}

/// The box frame's character cell: a character's width and a line's
/// height in the box face, which the framed pane keeps clear on each side.
fn cell(size: f32) -> (f32, f32) {
    (size * 0.6, (size * 1.3).round())
}

/// A pane framed as a terminal window, when the theme frames `pane`:
/// `┌─[ title ]───┐` along its top, its sides and foot in the box lines,
/// the content kept a character clear of them (two lines under a title
/// over the conversation, as in Slint).
pub fn boxed<'a>(
    content: impl Into<Element<'a, Message>>,
    ctx: crate::ctx::Ctx<'a>,
    pane: Pane,
    name: &str,
    n: Option<usize>,
) -> Element<'a, Message> {
    let content = content.into();
    // A pane of a fixed width (the member list) keeps it, the frame inside
    // it, rather than taking a share of the row.
    let width = match content.as_widget().size().width {
        Length::Fixed(width) => Length::Fixed(width),
        _ => Length::Fill,
    };
    let Some(frame) = chrome(ctx)
        .and_then(|chrome| chrome.box_frame.as_ref())
        .filter(|frame| frame.panes.contains(&pane))
    else {
        return content;
    };
    let theme = ctx.look.theme;
    let ink = frame
        .colour
        .map(crate::themes::argb)
        .or_else(|| frame.ink.map(|ink| token(theme, ink)))
        .unwrap_or_else(|| tree_ink(ctx));
    let template = frame.title_for(pane);
    let count = n.map(|n| n.to_string()).unwrap_or_default();
    let title = if template.is_empty() {
        String::new()
    } else {
        chrome::fill(template, &[("name", name), ("n", count.as_str())])
    };
    let (cell, line) = cell(frame.size);
    let top = if pane == Pane::Chat && !title.is_empty() {
        line * 2.0
    } else {
        line
    };
    let padded = iced::widget::container(content)
        .padding(iced::Padding {
            top,
            right: cell,
            bottom: line,
            left: cell,
        })
        .width(Length::Fill)
        .height(Length::Fill);
    let font = mono(ctx);
    let lines = Element::new(BoxLines {
        ink,
        cell,
        line,
        title,
        title_ink: theme.text,
        font,
        size: frame.size,
        title_width: 0.0,
    });
    iced::widget::container(
        iced::widget::stack![padded, lines]
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(width)
    .height(Length::Fill)
    .clip(true)
    .into()
}

/// The frame's lines and title, over the pane.
struct BoxLines {
    ink: Color,
    cell: f32,
    line: f32,
    title: String,
    title_ink: Color,
    font: Font,
    size: f32,
    /// The title's width, measured at layout.
    title_width: f32,
}

impl BoxLines {
    fn text(&self, content: &str) -> iced::advanced::text::Text<String, Font> {
        iced::advanced::text::Text {
            content: content.to_owned(),
            bounds: Size::new(f32::INFINITY, self.line),
            size: iced::Pixels(self.size),
            line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(self.line)),
            font: self.font,
            align_x: iced::advanced::text::Alignment::Left,
            align_y: iced::alignment::Vertical::Center,
            shaping: iced::advanced::text::Shaping::Advanced,
            wrapping: iced::advanced::text::Wrapping::None,
        }
    }
}

impl<M> Widget<M, Theme, iced::Renderer> for BoxLines {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        use iced::advanced::text::Paragraph as _;
        self.title_width = if self.title.is_empty() {
            0.0
        } else {
            let text = self.text(&self.title);
            <iced::Renderer as iced::advanced::text::Renderer>::Paragraph::with_text(
                iced::advanced::text::Text {
                    content: text.content.as_str(),
                    bounds: text.bounds,
                    size: text.size,
                    line_height: text.line_height,
                    font: text.font,
                    align_x: text.align_x,
                    align_y: text.align_y,
                    shaping: text.shaping,
                    wrapping: text.wrapping,
                },
            )
            .min_width()
        };
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        use iced::advanced::text::Renderer as _;
        let bounds = layout.bounds();
        if bounds.width < self.cell * 4.0 || bounds.height < self.line * 2.0 {
            return;
        }
        // The middle of the first and last character cells, where box
        // characters draw their lines.
        let left = (bounds.x + self.cell / 2.0).round();
        let right = (bounds.x + bounds.width - self.cell / 2.0).round();
        let top = (bounds.y + self.line / 2.0).round();
        let bottom = (bounds.y + bounds.height - self.line / 2.0).round();
        let fill = |renderer: &mut iced::Renderer, x: f32, y: f32, w: f32, h: f32| {
            if w > 0.0 && h > 0.0 {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle::new(Point::new(x, y), Size::new(w, h)),
                        ..Default::default()
                    },
                    self.ink,
                );
            }
        };
        // The title in the top edge, two cells in, and the line either side.
        let title_from = bounds.x + self.cell * 2.0;
        let room = (right - title_from - self.cell).max(0.0);
        let title_width = self.title_width.min(room);
        if title_width > 0.0 {
            fill(renderer, left, top, title_from - left, 1.0);
            fill(
                renderer,
                title_from + title_width,
                top,
                right + 1.0 - title_from - title_width,
                1.0,
            );
            renderer.fill_text(
                iced::advanced::text::Text {
                    bounds: Size::new(title_width, self.line),
                    ..self.text(&self.title)
                },
                Point::new(title_from, bounds.y + self.line / 2.0),
                self.title_ink,
                *viewport,
            );
        } else {
            fill(renderer, left, top, right + 1.0 - left, 1.0);
        }
        fill(renderer, left, bottom, right + 1.0 - left, 1.0);
        fill(renderer, left, top, 1.0, bottom - top);
        fill(renderer, right, top, 1.0, bottom - top);
    }
}

/// A list row after its file-tree line, `├──` or, ending its group, `└──`,
/// in the tree's ink and the code face.
pub fn tree_row<'a>(
    ctx: crate::ctx::Ctx<'a>,
    ends: bool,
    row: Element<'a, Message>,
) -> Element<'a, Message> {
    let glyph = if ends { "└──" } else { "├──" };
    iced::widget::row![
        iced::widget::text(glyph)
            .font(mono(ctx))
            .size(14)
            .color(tree_ink(ctx)),
        row
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center)
    .into()
}

/// Letters a name is cut into; the rest is one plain scrap, as in Slint.
const MOST_LETTERS: usize = 12;

/// A name cut as a ransom note, when the theme cuts names: each of its
/// first letters on a scrap of its own, from the theme's faces in turn,
/// the rest as one scrap in the first. A name with letters outside Latin
/// script is one scrap whole, in the stock face. Each letter's scrap leans
/// a few degrees by its seed, as Slint's.
pub fn ransom<'a>(ctx: crate::ctx::Ctx<'a>, name: &str, size: f32) -> Option<Element<'a, Message>> {
    ransom_in(ctx, name, size, None)
}

/// [`ransom`] with its scraps `height` tall, as a line that holds them
/// gives Slint's (a message's author line).
pub fn ransom_in<'a>(
    ctx: crate::ctx::Ctx<'a>,
    name: &str,
    size: f32,
    height: Option<f32>,
) -> Option<Element<'a, Message>> {
    let ransom = ctx.themes.ransom(&ctx.look.theme.id)?;
    if ctx.look.theme.is_readable_text() {
        return None;
    }
    let (first_paper, first_ink) = ransom.papers.first().copied()?;
    let tall = height.unwrap_or_else(|| (size * ransom.line).round());
    // Each scrap stands as tall as the name's tallest face. A letter's
    // scrap and the letter on it turn together by its seed, as Slint's (up
    // to 5.4° either way): iced 0.14 can't turn text, so the scrap is
    // drawn once as a picture (kept) and the picture turns. A longer run
    // (the rest of a long name, a name outside Latin script) is a straight
    // scrap of text.
    let scrap =
        |words: String, font: Option<Font>, paper: Color, ink: Color, pad: f32, turn: f32| {
            if words.chars().count() == 1
                && let Some(font) = font
                && let Some((picture, width)) =
                    letter_picture(&words, font, size, ink, paper, pad, tall)
            {
                return Element::from(
                    crate::widgets::Pic::new(picture, Size::new(width, tall)).turn(turn),
                );
            }
            let mut words = iced::widget::text(words).size(size).color(ink);
            if let Some(font) = font {
                words = words.font(font);
            }
            let words: Element<'a, Message> = iced::widget::container(words)
                .padding([0.0, pad])
                .height(tall)
                .align_y(iced::Alignment::Center)
                .into();
            crate::theme_art::card_ground(
                words,
                Some(paper),
                iced::Shadow::default(),
                turn,
                -tall / 2.0,
                0.0,
                crate::theme_art::CardMarks::default(),
            )
        };
    // Balanced and Light draw a name as one straight scrap in the theme's
    // heading face (brief §10): no letters to cut or turn.
    let (performance, _) = ctx.themes.art_motion(&ctx.look.theme.id);
    if performance != yapfast_core::themes::Performance::Full {
        return Some(scrap(
            name.to_owned(),
            Some(ctx.look.theme.font("headings", crate::fonts::BOLD)),
            first_paper,
            first_ink,
            2.0,
            0.0,
        ));
    }
    let latin = name
        .chars()
        .all(|letter| letter.is_ascii() || ('\u{a0}'..='\u{24f}').contains(&letter));
    let mut row = iced::widget::row![]
        .spacing(1)
        .align_y(iced::Alignment::Center);
    if ransom.fonts.is_empty() || !latin {
        return Some(scrap(
            name.to_owned(),
            None,
            first_paper,
            first_ink,
            2.0,
            0.0,
        ));
    }
    for (index, (at, letter)) in name.char_indices().enumerate() {
        if index == MOST_LETTERS {
            row = row.push(scrap(
                name[at..].to_owned(),
                Some(ransom.fonts[0]),
                first_paper,
                first_ink,
                2.0,
                0.0,
            ));
            break;
        }
        if letter == ' ' {
            // Slint's word gap: a space scrap, the window face's space
            // and its 4 px.
            row = row.push(iced::widget::Space::new().width(size * 0.6 + 4.0));
            continue;
        }
        let seed = letter_seed(name, index);
        let (paper, ink) = ransom.papers[(seed as usize / 7) % ransom.papers.len()];
        // A letter whose case would read as another letter ("Alice" as
        // "AIIce") never takes a face that draws small letters as capitals:
        // the next face in turn that keeps its case.
        let mut face = index % ransom.fonts.len();
        if letter.is_lowercase() && yapfast_core::themes::layout::case_sensitive_letter(letter) {
            face = (0..ransom.fonts.len())
                .map(|step| (face + step) % ransom.fonts.len())
                .find(|&at| !ransom.capitals.get(at).copied().unwrap_or(false))
                .unwrap_or(face);
        }
        row = row.push(scrap(
            letter.to_string(),
            Some(ransom.fonts[face]),
            paper,
            ink,
            2.0,
            ((seed % 13) as f32 - 6.0) * 0.9,
        ));
    }
    Some(row.into())
}

/// The same scrap for a name's letter every time it's drawn.
fn letter_seed(text: &str, index: usize) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    index.hash(&mut hasher);
    hasher.finish()
}

/// Pictures of ransom letters on their scraps, kept: a name's letters are
/// drawn once, not each frame.
type LetterKey = (String, String, u32, [u8; 4], [u8; 4], u32, u32);

/// How many times finer than the window a letter's picture is drawn, so it
/// stays sharp turned.
const LETTER_SCALE: f32 = 2.0;

/// A letter on its scrap of paper, `pad` either side and `height` tall,
/// as a picture to turn, and the scrap's width.
fn letter_picture(
    letter: &str,
    font: Font,
    size: f32,
    ink: Color,
    paper: Color,
    pad: f32,
    height: f32,
) -> Option<(iced::advanced::image::Handle, f32)> {
    use iced::advanced::graphics::text::{cosmic_text, font_system, to_attributes};
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static LETTERS: RefCell<HashMap<LetterKey, (iced::advanced::image::Handle, f32)>> =
            RefCell::new(HashMap::new());
        static SWASH: RefCell<cosmic_text::SwashCache> = RefCell::new(cosmic_text::SwashCache::new());
    }
    let key: LetterKey = (
        letter.to_owned(),
        format!("{font:?}"),
        size.to_bits(),
        ink.into_rgba8(),
        paper.into_rgba8(),
        pad.to_bits(),
        height.to_bits(),
    );
    if let Some(found) = LETTERS.with_borrow(|letters| letters.get(&key).cloned()) {
        return Some(found);
    }
    let scale = LETTER_SCALE;
    let mut system = font_system().write().ok()?;
    let raw = system.raw();
    let metrics = cosmic_text::Metrics::new(size * scale, height * scale);
    let mut buffer = cosmic_text::Buffer::new(raw, metrics);
    buffer.set_size(raw, None, None);
    buffer.set_text(
        raw,
        letter,
        &to_attributes(font),
        cosmic_text::Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(raw, false);
    let advance = buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0f32, f32::max);
    let width = (advance / scale + 2.0 * pad).ceil().max(1.0);
    let (columns, rows) = (
        (width * scale).round() as u32,
        (height * scale).round().max(1.0) as u32,
    );
    let ground = paper.into_rgba8();
    let mut pixels: Vec<f32> = (0..columns * rows)
        .flat_map(|_| ground.map(|value| f32::from(value) / 255.0))
        .collect();
    let left = (pad * scale).round() as i32;
    let colour = ink.into_rgba8();
    SWASH.with_borrow_mut(|swash| {
        buffer.draw(
            raw,
            swash,
            cosmic_text::Color::rgba(colour[0], colour[1], colour[2], colour[3]),
            |x, y, _, _, shown| {
                let (x, y) = (x + left, y);
                if x < 0 || y < 0 || x as u32 >= columns || y as u32 >= rows {
                    return;
                }
                let at = (y as u32 * columns + x as u32) as usize * 4;
                let alpha = f32::from(shown.a()) / 255.0;
                for (channel, value) in [shown.r(), shown.g(), shown.b()].into_iter().enumerate() {
                    let under = pixels[at + channel];
                    pixels[at + channel] = under * (1.0 - alpha) + f32::from(value) / 255.0 * alpha;
                }
                pixels[at + 3] = pixels[at + 3] + alpha * (1.0 - pixels[at + 3]);
            },
        );
    });
    drop(system);
    let mut rgba: Vec<u8> = pixels
        .iter()
        .map(|value| (value * 255.0).round().clamp(0.0, 255.0) as u8)
        .collect();
    crate::paint::to_native(&mut rgba);
    let handle = iced::advanced::image::Handle::from_rgba(columns, rows, rgba);
    LETTERS.with_borrow_mut(|letters| {
        if letters.len() >= 1024 {
            letters.clear();
        }
        letters.insert(key, (handle.clone(), width));
    });
    Some((handle, width))
}
