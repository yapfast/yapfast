//! Discord's Display Name Style window, opened from the editor's tile: the
//! name's font, its effect and its colours (Discord's presets for each
//! effect, one's own colour on a pad, or a gradient's two and a prism's
//! five, each on a handle of the bar), with how it looks on a profile, a
//! message and a member row, the message and row in Discord's light mode or
//! dark. Apply hands the style to the core (`Action::ChooseNameStyle`),
//! which keeps it with the profile's other edits until they're saved.
//!
//! A style's colours are kept as Discord's are written: hex joined by
//! commas, spread over five stops for a swatch.

use std::{cell::RefCell, collections::HashMap};

use iced::{
    Alignment, Background, Border, Color, Element, Length, Shadow, Task, Vector,
    advanced::graphics::text::{Version, font_system},
    gradient::Linear,
    widget::{Space, button, column, container, mouse_area, opaque, row, stack, text, tooltip},
};
use yapfast_core::{application::Action, chat::NameStyle};

use super::{Event as ProfileEvent, Shown, editor::Event as EditorEvent};
use crate::{
    app::Message,
    controls::{self, Kind, action, lights, placed},
    ctx::Cx,
    fonts::{BOLD, SEMIBOLD, name_font},
    form,
    looks::{self, family},
    paint::NamePicture,
    pictures::{Icon, Mark},
    popover::{Popover, Side, Width},
    style::{faded, hex, rounded},
    theme::{mix, rgb},
    widgets::{ColourPad, Hsv, Pic, hsv_rgb, rgb_hsv},
};

/// The window's most, and the room kept round it in a smaller one.
const WIDTH: f32 = 836.0;
const HEIGHT: f32 = 722.0;
const ROOM: f32 = 32.0;
/// The choices' column, its padding, and its tiles: four to a row.
const CHOICES: f32 = 424.0;
const PAD: f32 = 24.0;
const TILE_GAP: f32 = 8.0;
const TILE: f32 = (CHOICES - 2.0 * PAD - 3.0 * TILE_GAP) / 4.0;
const TILE_HEIGHT: f32 = 68.0;
/// A colour's square, and the bar of a gradient's or a prism's handles.
const SWATCH: f32 = 40.0;
const BAR: f32 = 136.0;
/// The foot under the previews: its padding round the buttons.
const FOOT: f32 = 16.0 + 36.0 + 16.0;
/// The colour code's field.
const CODE: &str = "name-style-code";
/// What the message's preview says.
const SAMPLE: &str = "does anyone read this?";
/// The light preview's ground and ink, Discord's light mode.
const LIGHT_FILL: u32 = 0xffffff;
const LIGHT_INK: u32 = 0x313338;

/// Discord's fonts, by `font_id`, with the names their tiles show.
const FACES: [(u8, &str); 12] = [
    (11, "Default"),
    (12, "Tempo"),
    (3, "Sakura"),
    (4, "Jellybean"),
    (6, "Modern"),
    (7, "Medieval"),
    (8, "8Bit"),
    (10, "Vampyre"),
    (13, "Monkey Bars"),
    (14, "Mainframe"),
    (15, "Headbang"),
    (16, "Journal"),
];
/// Discord's effects, by `effect_id`, in its order.
const EFFECTS: [(u8, &str); 7] = [
    (1, "Solid"),
    (2, "Gradient"),
    (3, "Neon"),
    (4, "Toon"),
    (5, "Pop"),
    (8, "Gummy"),
    (7, "Prism"),
];
/// The usual font's weight on its tile and in the effects' names.
const DEFAULT_WEIGHT: u16 = 500;

/// Each effect's presets, as Discord's window offers them.
const SOLID: [&str; 7] = [
    "f0f0f2", "26e36e", "1f99ec", "c32cfe", "da034a", "f93824", "c6a617",
];
const NEON: [&str; 7] = [
    "6b20ee", "26e36e", "1f99ec", "c32cfe", "da034a", "f93824", "c6a617",
];
const TOON: [&str; 7] = [
    "f52399", "26e36e", "1f99ec", "c32cfe", "da034a", "f93824", "c6a617",
];
const POP: [&str; 7] = [
    "12d088", "26e36e", "1f99ec", "c32cfe", "da034a", "f93824", "c6a617",
];
const GRADIENT: [&str; 5] = [
    "2dafa7,fec491",
    "ca63fa,417ae4",
    "7c61c4,6bba9f",
    "edaab1,e577da",
    "f39054,e764cf",
];
const GUMMY: [&str; 7] = [
    "eaaad6,aadef1,baa8ef,baebcc",
    "fe70c2,f550a8,fe9bd6,da6cb8",
    "ffb67c,ff9571,ff7e7a,f06c8b",
    "c4ed66,98de61,6dcd64,46bc67",
    "7db8fe,8fe0d0,9d95f5,67cff0",
    "d77cfe,b56cfe,e45de5,8c6dfe",
    "d83b60,c94ccb,8c5dfe,b06dfe",
];
const PRISM: [&str; 4] = [
    "fe9444,37ee80,fee048,ff4ed3,5966f2",
    "90c464,68e971,fed84f,fe63bd,b458e0",
    "7ed8ed,7dbbfe,828bfe,ad72fe,d17bed",
    "a5160f,d32f15,f45e18,fc8d05,feb122",
];

pub fn presets(effect: u8) -> &'static [&'static str] {
    match effect {
        2 => &GRADIENT,
        3 => &NEON,
        4 => &TOON,
        5 => &POP,
        7 => &PRISM,
        8 => &GUMMY,
        _ => &SOLID,
    }
}

/// A style's colours as written, and spread over five stops.
#[derive(Clone, Debug, PartialEq)]
pub struct Paint {
    pub key: String,
    stops: [u32; 5],
}

impl Paint {
    fn of(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            stops: spread(&colours(key)),
        }
    }

    fn solid(rgb: u32) -> Self {
        Self {
            key: format!("{rgb:06x}"),
            stops: [rgb; 5],
        }
    }
}

/// The colours of a key, hex joined by commas.
fn colours(key: &str) -> Vec<u32> {
    key.split(',')
        .filter_map(|hex| u32::from_str_radix(hex.trim().trim_start_matches('#'), 16).ok())
        .collect()
}

/// Five stops evenly over however many colours there are.
fn spread(colours: &[u32]) -> [u32; 5] {
    let (Some(&first), Some(&last)) = (colours.first(), colours.last()) else {
        return [0; 5];
    };
    std::array::from_fn(|index| {
        let at = index as f32 / 4.0 * (colours.len() - 1) as f32;
        let low = at.floor() as usize;
        let (from, to) = (
            colours.get(low).copied().unwrap_or(first),
            colours.get(low + 1).copied().unwrap_or(last),
        );
        blend(from, to, at.fract())
    })
}

/// `from` moved `share` of the way to `to`, channel by channel.
fn blend(from: u32, to: u32, share: f32) -> u32 {
    let channel = |shift: u32| {
        let (a, b) = (((from >> shift) & 255) as f32, ((to >> shift) & 255) as f32);
        ((a + (b - a) * share) as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// A colour's code as six hex digits, if it's one.
fn parse(code: &str) -> Option<u32> {
    let hex = code.trim().trim_start_matches('#');
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

/// What the colour pad is open for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picker {
    /// One's own colour, for an effect of one colour.
    Own,
    /// The gradient's or prism's handle, from the left.
    Handle(usize),
}

#[derive(Clone, Debug)]
pub enum Event {
    Open,
    Close,
    Font(u8),
    Effect(u8),
    Preset(&'static str),
    /// The eyedropper: one's own colour's pad opens or closes.
    Own,
    Handle(usize),
    /// A click away from the pad.
    Away,
    Hsv(Hsv),
    Code(String),
    Light(bool),
    Surprise,
    Apply,
}

fn message(event: Event) -> Message {
    Message::Profile(ProfileEvent::Editor(EditorEvent::Style(event)))
}

/// The window while it's open: the style as chosen, and how it opened.
pub struct Window {
    start: (u8, u8, String),
    pub font: u8,
    pub effect: u8,
    pub paint: Paint,
    pub picker: Option<Picker>,
    pub light: bool,
    /// One's own gradient's two ends and prism's five colours.
    pub ends: [String; 2],
    pub prism: [String; 5],
    /// The pad's colour and its code as written.
    pub hsv: Hsv,
    pub code: String,
    names: RefCell<Names>,
}

/// The names drawn in their styles while the window shows, by what they
/// were drawn of; made again for a theme or a font loaded since.
#[derive(Default)]
struct Names {
    made: HashMap<NameKey, NamePicture>,
    stamp: Option<(bool, Version)>,
}

/// A name's font, effect, colours, text and size.
type NameKey = (u8, u8, String, String, u32);

impl Window {
    /// On the style as it stands: a saved gradient's or prism's colours
    /// fill its handles.
    pub fn new(font: u8, effect: u8, key: String) -> Self {
        let effect = effect.max(1);
        let mut window = Self {
            font,
            effect,
            paint: Paint::of(presets(effect)[0]),
            picker: None,
            light: false,
            ends: ["2dafa7".into(), "fec491".into()],
            prism: ["fe9444", "37ee80", "fee048", "ff4ed3", "5966f2"].map(String::from),
            hsv: [0.0, 0.0, 1.0],
            code: String::new(),
            names: RefCell::default(),
            start: (font, effect, key.clone()),
        };
        if !key.is_empty() {
            window.paint = Paint::of(&key);
            let part = |index: usize| key.split(',').nth(index).unwrap_or_default().to_owned();
            match effect {
                2 => window.ends = std::array::from_fn(part),
                7 => window.prism = std::array::from_fn(part),
                _ => {}
            }
        }
        window
    }

    /// Whether Apply would change anything.
    pub fn changed(&self) -> bool {
        let (font, effect, key) = &self.start;
        self.font != *font || self.effect != *effect || self.paint.key != *key
    }

    /// The bar of handles shows for a gradient and a prism.
    fn bar(&self) -> bool {
        matches!(self.effect, 2 | 7)
    }

    /// The handles' colours as written.
    fn handles(&self) -> &[String] {
        if self.effect == 2 {
            &self.ends
        } else {
            &self.prism
        }
    }

    fn update(&mut self, event: Event) {
        match event {
            Event::Font(font) => self.font = font,
            Event::Effect(effect) => {
                self.effect = effect;
                self.paint = Paint::of(presets(effect)[0]);
                self.picker = None;
            }
            Event::Preset(key) => {
                self.paint = Paint::of(key);
                self.picker = None;
            }
            Event::Own => {
                self.picker = (self.picker != Some(Picker::Own)).then_some(Picker::Own);
                self.load(self.paint.stops[0]);
            }
            Event::Handle(index) => {
                let open = Some(Picker::Handle(index));
                self.picker = (self.picker != open).then_some(Picker::Handle(index));
                self.own_paint();
                if self.picker.is_some() {
                    let rgb = self.handles().get(index).and_then(|code| parse(code));
                    self.load(rgb.unwrap_or(0));
                }
            }
            Event::Away => self.picker = None,
            Event::Hsv(hsv) => {
                self.hsv = hsv;
                let code = format!("{:06x}", hsv_rgb(hsv));
                self.use_code(&code);
                self.code = code;
            }
            Event::Code(code) => {
                if let Some(rgb) = parse(&code) {
                    self.hsv = rgb_hsv(rgb);
                    self.use_code(&format!("{rgb:06x}"));
                }
                self.code = code;
            }
            Event::Light(light) => self.light = light,
            Event::Surprise => {
                let tick = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |since| since.as_millis() as usize);
                self.font = FACES[tick % FACES.len()].0;
                self.effect = EFFECTS[(tick / 7) % EFFECTS.len()].0;
                let list = presets(self.effect);
                self.paint = Paint::of(list[(tick / 53) % list.len()]);
            }
            Event::Open | Event::Close | Event::Apply => {}
        }
        // The names drawn for other styles go.
        let tile = |key: &NameKey| key.0 == 11 && key.4 == TILE_NAME as u32;
        let (font, effect, paint) = (self.font, self.effect, &self.paint.key);
        self.names
            .get_mut()
            .made
            .retain(|key, _| tile(key) || (key.0 == font && key.1 == effect && key.2 == *paint));
    }

    /// The pad opens on `rgb`.
    fn load(&mut self, rgb: u32) {
        self.hsv = rgb_hsv(rgb);
        self.code = format!("{rgb:06x}");
    }

    /// A colour from the pad or its code: the open handle's, or the
    /// style's one colour.
    fn use_code(&mut self, code: &str) {
        match self.picker {
            Some(Picker::Handle(index)) => {
                if self.effect == 2 {
                    if let Some(end) = self.ends.get_mut(index) {
                        code.clone_into(end);
                    }
                } else if let Some(colour) = self.prism.get_mut(index) {
                    code.clone_into(colour);
                }
                self.own_paint();
            }
            _ => self.paint = Paint::solid(parse(code).unwrap_or(0)),
        }
    }

    /// The style's colours from the handles: a gradient's two ends with
    /// three stops between, or the prism's five.
    fn own_paint(&mut self) {
        let parsed = |code: &String| parse(code).unwrap_or(0);
        self.paint = if self.effect == 2 {
            let [a, b] = [parsed(&self.ends[0]), parsed(&self.ends[1])];
            Paint {
                key: self.ends.join(","),
                stops: [a, blend(a, b, 0.25), blend(a, b, 0.5), blend(a, b, 0.75), b],
            }
        } else {
            Paint {
                key: self.prism.join(","),
                stops: std::array::from_fn(|index| parsed(&self.prism[index])),
            }
        };
    }
}

/// An effect's name on its tile.
const TILE_NAME: f32 = 15.0;

/// What's done in the window, which opens over `editor`.
pub(super) fn update(
    editor: &mut super::editor::Editor,
    event: Event,
    cx: &mut Cx<'_>,
) -> Task<Message> {
    let start = cx.state().profile_editor.as_ref().map(|core| {
        let key = core
            .name_colours
            .iter()
            .map(|colour| format!("{colour:06x}"))
            .collect::<Vec<_>>()
            .join(",");
        (core.name_font.unwrap_or(11), core.name_effect, key)
    });
    match event {
        Event::Open => {
            let Some((font, effect, key)) = start else {
                return Task::none();
            };
            editor.stop_editing();
            editor.style = Some(Window::new(font, effect, key));
            return iced::widget::operation::focus(crate::app::NOBODY);
        }
        Event::Close => editor.style = None,
        Event::Apply => {
            if let Some(window) = editor.style.take() {
                cx.dispatch(Action::ChooseNameStyle {
                    font: window.font,
                    effect: window.effect,
                    colours: window.paint.key,
                });
            }
        }
        event => {
            if let Some(window) = &mut editor.style {
                window.update(event);
            }
        }
    }
    Task::none()
}

/// Escape: the pad first, then the window. Whether it took the key.
pub(super) fn escape(editor: &mut super::editor::Editor) -> bool {
    let Some(window) = &mut editor.style else {
        return false;
    };
    if window.picker.is_some() {
        window.picker = None;
    } else {
        editor.style = None;
    }
    true
}

/// The weight a font's tile shows it in: its own face's, or the usual
/// font's tile weight.
fn face_weight(font: u8) -> u16 {
    if family(font).is_some() {
        looks::weight(font)
    } else {
        DEFAULT_WEIGHT
    }
}

/// Tiles four to a row, 8 px apart.
fn grid(tiles: Vec<Element<'_, Message>>) -> Element<'_, Message> {
    let mut rows = column![].spacing(TILE_GAP);
    let mut tiles = tiles.into_iter().peekable();
    while tiles.peek().is_some() {
        rows = rows.push(row(tiles.by_ref().take(4)).spacing(TILE_GAP));
    }
    rows.into()
}

/// A preset's swatch: its colours across it, or gummy's stripes, as each
/// of its letters takes one whole.
fn swatch(key: &str, stripes: bool) -> Background {
    let mut fill = Linear::new(iced::Degrees(90.0));
    if stripes {
        let colours = colours(key);
        for index in 0..4 {
            let colour = hex(colours
                .get(index % colours.len().max(1))
                .copied()
                .unwrap_or(0));
            let from = index as f32 / 4.0;
            // A stop at the same place would replace the one before.
            fill = fill
                .add_stop(if index == 0 { 0.0 } else { from + 0.001 }, colour)
                .add_stop(from + 0.25, colour);
        }
    } else {
        for (index, colour) in spread(&colours(key)).into_iter().enumerate() {
            fill = fill.add_stop(index as f32 / 4.0, hex(colour));
        }
    }
    Background::Gradient(fill.into())
}

/// The window over the editor dimmed by half; a click on the dim
/// closes it.
pub(super) fn name_style_view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    window: &'a Window,
    shown: &'a Shown,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let width = WIDTH.min(ctx.look.window.width - ROOM);
    let height = HEIGHT.min(ctx.look.window.height - ROOM);
    let body = row![
        style_choices(ctx, window),
        style_previews(ctx, window, shown, width - CHOICES, height - 1.0 - FOOT),
    ]
    .height(Length::Fill);
    let foot = row![
        Space::new().width(Length::Fill),
        action(
            theme,
            "Surprise Me",
            Kind::Plain,
            Some(message(Event::Surprise))
        ),
        action(
            theme,
            "Apply",
            Kind::Primary,
            window.changed().then(|| message(Event::Apply))
        ),
    ]
    .spacing(8)
    .padding(16);
    let ground = theme.sidebar;
    let card = container(column![
        body,
        controls::bar(theme.line, Length::Fill, 1.0),
        foot
    ])
    .width(width)
    .height(height)
    .clip(true)
    .style(move |_| rounded(ground, 16).shadow(crate::style::window_shadow()));
    let close = ctx
        .look
        .icon_button(Icon::X, "Close", message(Event::Close))
        .size(32.0, 20)
        .below();
    let card = stack![card, placed(close, width - 32.0 - 12.0, 12.0)];
    let dim = Color {
        a: 0.5,
        ..Color::BLACK
    };
    let backdrop = mouse_area(
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| crate::style::fill(dim)),
    )
    .on_press(message(Event::Close));
    opaque(stack![
        opaque(backdrop),
        container(opaque(card)).center(Length::Fill)
    ])
}

/// The choices' column: the fonts, the effects and the colours.
fn style_choices<'a>(ctx: crate::ctx::Ctx<'a>, window: &'a Window) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let label = |words: &'static str| text(words).size(13).font(SEMIBOLD).color(theme.secondary);
    let faces = FACES.map(|(font, label)| {
        let face = text("Gg")
            .size(22)
            .font(name_font(family(font), face_weight(font)))
            .color(theme.text);
        let tile = style_tile(
            ctx,
            face.into(),
            window.font == font,
            message(Event::Font(font)),
        );
        tooltip(tile, controls::tip(theme, label), tooltip::Position::Top)
            .gap(6)
            .into()
    });
    let effects = EFFECTS.map(|(effect, label)| {
        let name = style_name(
            ctx,
            window,
            11,
            effect,
            presets(effect)[0],
            label,
            (TILE_NAME, DEFAULT_WEIGHT),
        );
        style_tile(
            ctx,
            name,
            window.effect == effect,
            message(Event::Effect(effect)),
        )
    });
    column![
        text("Change Display Name Style")
            .size(22)
            .font(BOLD)
            .color(theme.text),
        label("Font"),
        grid(faces.into_iter().collect()),
        label("Effect"),
        grid(effects.into_iter().collect()),
        label("Colour"),
        colour_choices(ctx, window),
    ]
    .spacing(12)
    .padding(PAD)
    .width(CHOICES)
    .into()
}

/// A font's or an effect's tile: raised, lit under the pointer, the
/// chosen one in the accent's wash and edge.
fn style_tile<'a>(
    ctx: crate::ctx::Ctx<'a>,
    face: Element<'a, Message>,
    chosen: bool,
    pressed: Message,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (raised, hover, accent) = (theme.raised, theme.hover, theme.accent);
    button(container(face).center(Length::Fill))
        .padding(0)
        .width(TILE)
        .height(TILE_HEIGHT)
        .on_press(pressed)
        .style(move |_, status| button::Style {
            background: Some(
                if chosen {
                    faded(accent, 0.16)
                } else if lights(status) {
                    hover
                } else {
                    raised
                }
                .into(),
            ),
            border: Border {
                color: accent,
                width: if chosen { 2.0 } else { 0.0 },
                radius: 12.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// A name drawn in a style, kept while the window shows it.
fn style_name<'a>(
    ctx: crate::ctx::Ctx<'a>,
    window: &Window,
    font: u8,
    effect: u8,
    key: &str,
    name: &str,
    (size, weight): (f32, u16),
) -> Element<'a, Message> {
    let dark = ctx.look.theme.dark;
    let stamp = (dark, font_system().read().expect("font system").version());
    let mut names = window.names.borrow_mut();
    if names.stamp != Some(stamp) {
        names.made.clear();
        names.stamp = Some(stamp);
    }
    let made = names
        .made
        .entry((font, effect, key.to_owned(), name.to_owned(), size as u32))
        .or_insert_with(|| {
            let colours = colours(key);
            let colours = if colours.is_empty() {
                vec![0xf0f0f2]
            } else {
                colours
            };
            let style = NameStyle::new(font, effect, &colours);
            let look =
                looks::name_look(Some(&style), name, false, dark).expect("a style with colours");
            NamePicture::new(&look, name, size, weight)
        });
    made.view()
}

/// The colours: one's own (the eyedropper), or the bar of a gradient's
/// or prism's handles; then the effect's presets. The pad opens over
/// the row.
fn colour_choices<'a>(ctx: crate::ctx::Ctx<'a>, window: &'a Window) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let mut line = row![].spacing(8).height(SWATCH);
    if window.bar() {
        line = line.push(handle_bar(window));
    } else {
        let open = window.picker == Some(Picker::Own);
        let (raised, hover) = (theme.raised, theme.hover);
        line = line.push(
            button(
                ctx.look
                    .glyph(Icon::Eyedropper, 16, theme.text)
                    .boxed(SWATCH as u32),
            )
            .padding(0)
            .width(SWATCH)
            .height(SWATCH)
            .on_press(message(Event::Own))
            .style(move |_, status| button::Style {
                background: Some(
                    if open || lights(status) {
                        hover
                    } else {
                        raised
                    }
                    .into(),
                ),
                border: crate::style::round(12),
                ..Default::default()
            }),
        );
    }
    let ink = theme.text;
    for &key in presets(window.effect) {
        let chosen = window.paint.key == key;
        let fill = swatch(key, window.effect == 8);
        line = line.push(
            button(Space::new())
                .width(SWATCH)
                .height(SWATCH)
                .on_press(message(Event::Preset(key)))
                .style(move |_, _| button::Style {
                    background: Some(fill),
                    border: Border {
                        color: ink,
                        width: if chosen { 3.0 } else { 0.0 },
                        radius: 12.0.into(),
                    },
                    ..Default::default()
                }),
        );
    }
    let pad = window.picker.map(|_| {
        form::colour_card(
            theme,
            ColourPad::new(window.hsv, |hsv| message(Event::Hsv(hsv))),
            form::field(theme, CODE, "#rrggbb", &window.code)
                .on_input(|code| message(Event::Code(code)))
                .on_submit(message(Event::Away)),
        )
    });
    Popover::new(line, pad, Side::Above)
        .width(Width::Own(form::COLOUR_CARD))
        .gap(8.0)
        .outside(message(Event::Away))
        .into()
}

/// One's own gradient or prism: the colours across a bar, a handle on
/// each, the open one's edge thicker.
fn handle_bar<'a>(window: &Window) -> Element<'a, Message> {
    let codes = window.handles();
    let count = codes.len();
    let colours: Vec<Color> = codes
        .iter()
        .map(|code| parse(code).map_or(Color::TRANSPARENT, hex))
        .collect();
    let mut fill = Linear::new(iced::Degrees(90.0));
    for (index, colour) in colours.iter().enumerate() {
        fill = fill.add_stop(index as f32 / (count - 1) as f32, *colour);
    }
    let fill = Background::Gradient(fill.into());
    let handle_width = if count == 2 { 24.0 } else { 20.0 };
    let mut bar = stack![
        container(Space::new())
            .width(BAR)
            .height(SWATCH)
            .style(move |_| container::Style {
                background: Some(fill),
                border: crate::style::round(12),
                ..Default::default()
            })
    ];
    for (index, colour) in colours.into_iter().enumerate() {
        let open = window.picker == Some(Picker::Handle(index));
        let x = 5.0 + index as f32 * (BAR - 10.0 - handle_width) / (count - 1) as f32;
        let handle = button(Space::new())
            .width(handle_width)
            .height(24)
            .on_press(message(Event::Handle(index)))
            .style(move |_, _| button::Style {
                background: Some(colour.into()),
                border: Border {
                    color: Color::WHITE,
                    width: if open { 3.0 } else { 2.0 },
                    radius: 7.0.into(),
                },
                shadow: Shadow {
                    color: Color {
                        a: 0.5,
                        ..Color::BLACK
                    },
                    offset: Vector::ZERO,
                    blur_radius: 4.0,
                },
                ..Default::default()
            });
        bar = bar.push(placed(handle, x, (SWATCH - 24.0) / 2.0));
    }
    bar.into()
}

/// How it looks, laid out as Discord's, each over the one before: the
/// profile's card, a message, a member row; the light or dark choice
/// and the note in the corner.
fn style_previews<'a>(
    ctx: crate::ctx::Ctx<'a>,
    window: &'a Window,
    shown: &'a Shown,
    width: f32,
    height: f32,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let profile = shown.look.profile();
    let name = profile.name.as_str();
    let (font, effect, key) = (window.font, window.effect, window.paint.key.as_str());
    let (sample_fill, sample_ink, sample_edge) = if window.light {
        (hex(LIGHT_FILL), hex(LIGHT_INK), Color::TRANSPARENT)
    } else {
        (theme.raised, theme.text, theme.edge)
    };
    let inks = super::look::Inks::new(theme, profile.theme, theme.raised);
    let card_shadow = |blur: f32, alpha: f32, y: f32| Shadow {
        color: Color {
            a: alpha,
            ..Color::BLACK
        },
        offset: Vector::new(0.0, y),
        blur_radius: blur,
    };
    // The profile's card.
    let mut avatar = stack![ctx.avatar(profile.avatar.as_deref(), name, 80.0, 40.0, 1.0)];
    if let Some(dot) = profile
        .presence
        .and_then(|presence| ctx.mark(Mark::Dot(presence, rgb(inks.start))))
    {
        let size = 80.0 * 0.291;
        avatar = avatar.push(placed(
            Pic::square(dot, size),
            40.0 + 80.0 * 0.305 - size / 2.0,
            40.0 + 80.0 * 0.31 - size / 2.0,
        ));
    }
    let handle_ink = if inks.themed { inks.ink } else { Color::WHITE };
    let mut handle = row![text(profile.handle.as_str()).size(13).color(handle_ink)]
        .spacing(6)
        .height(26)
        .align_y(Alignment::Center);
    for badge in profile.badges.iter().take(3) {
        if let Some(picture) =
            crate::profile::profile_picture(ctx, shown, &badge.icon, 20, 20, false)
        {
            handle = handle.push(Pic::square(picture, 20.0));
        }
    }
    let (start, end, edge) = (inks.start, inks.end, theme.edge);
    let card = container(stack![
        Space::new().width(305).height(216),
        placed(avatar, 16.0, 56.0),
        placed(
            style_name(ctx, window, font, effect, key, name, (21.0, 700)),
            18.0,
            145.0
        ),
        placed(handle, 18.0, 176.0),
    ])
    .clip(true)
    .style(move |_| container::Style {
        background: Some(Background::Gradient(
            Linear::new(iced::Degrees(180.0))
                .add_stop(0.0, start)
                .add_stop(1.0, end)
                .into(),
        )),
        border: crate::style::edge(edge, 14.0),
        shadow: card_shadow(24.0, 0.4, 8.0),
        ..Default::default()
    });
    // A message: its name stays plain, in the style's font.
    let family = family(font);
    let message_card = container(stack![
        Space::new().width(320).height(85),
        placed(
            ctx.avatar(profile.avatar.as_deref(), name, 40.0, 20.0, 1.0),
            20.0,
            22.0
        ),
        placed(
            text(name)
                .size(17)
                .font(name_font(family, 600))
                .color(sample_ink)
                .wrapping(text::Wrapping::None),
            72.0,
            20.0
        ),
        placed(text(SAMPLE).size(15).color(sample_ink), 72.0, 46.0),
    ])
    .style(move |_| container::Style {
        border: crate::style::edge(sample_edge, 12.0),
        shadow: card_shadow(18.0, 0.33, 6.0),
        ..rounded(sample_fill, 12)
    });
    // A member row.
    let member = container(stack![
        Space::new().width(282).height(50),
        placed(
            ctx.avatar(profile.avatar.as_deref(), name, 32.0, 16.0, 1.0),
            10.0,
            9.0
        ),
        placed(
            style_name(ctx, window, font, effect, key, name, (17.0, 600)),
            52.0,
            14.0
        ),
    ])
    .style(move |_| container::Style {
        border: crate::style::edge(sample_edge, 10.0),
        shadow: card_shadow(14.0, 0.33, 4.0),
        ..rounded(sample_fill, 10)
    });
    let note = container(
        text("Colours and effects don't show in servers, and may look different across Discord.")
            .size(12)
            .color(theme.muted),
    )
    .width((width - 56.0 - 92.0).max(0.0));
    let (sunken, wash) = (
        theme.sunken,
        mix(faded(theme.accent, 0.1), theme.sunken, 0.5),
    );
    container(stack![
        Space::new().width(Length::Fill).height(Length::Fill),
        placed(card, 40.0, 132.0),
        placed(message_card, 65.0, 338.0),
        placed(member, 40.0, 418.0),
        placed(
            light_choice(ctx, window.light),
            width - 76.0 - 24.0,
            height - 64.0
        ),
        placed(note, 28.0, height - 72.0),
    ])
    .width(Length::Fill)
    .height(Length::Fill)
    .clip(true)
    .style(move |_| container::Style {
        background: Some(Background::Gradient(
            Linear::new(iced::Degrees(160.0))
                .add_stop(0.0, sunken)
                .add_stop(1.0, wash)
                .into(),
        )),
        ..Default::default()
    })
    .into()
}

/// Dark or light, for the message and the member row.
fn light_choice<'a>(ctx: crate::ctx::Ctx<'a>, light: bool) -> Element<'a, Message> {
    let mut choice = row![].spacing(4).padding(4);
    for (mode, icon) in [(false, Icon::Moon), (true, Icon::Sun)] {
        let chosen = light == mode;
        let ink = if chosen {
            Color::WHITE
        } else {
            faded(Color::WHITE, 0.65)
        };
        choice = choice.push(
            button(ctx.look.glyph(icon, 16, ink).boxed(32))
                .padding(0)
                .width(32)
                .height(32)
                .on_press(message(Event::Light(mode)))
                .style(move |_, status| button::Style {
                    background: Some(
                        faded(
                            Color::WHITE,
                            if chosen {
                                0.15
                            } else if lights(status) {
                                0.08
                            } else {
                                0.0
                            },
                        )
                        .into(),
                    ),
                    border: crate::style::round(9),
                    ..Default::default()
                }),
        );
    }
    container(choice)
        .width(76)
        .height(40)
        .style(|_| {
            rounded(
                Color {
                    a: 0.35,
                    ..Color::BLACK
                },
                12,
            )
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_spreads_over_five_stops() {
        assert_eq!(
            Paint::of("2dafa7,fec491").stops,
            [0x2dafa7, 0x61b4a1, 0x95b99c, 0xc9be96, 0xfec491]
        );
        assert_eq!(Paint::of("f0f0f2").stops, [0xf0f0f2; 5]);
    }

    #[test]
    fn a_saved_gradient_fills_its_handles_and_opens_unchanged() {
        let window = Window::new(4, 2, "ff0000,00ff00".into());
        assert_eq!(window.ends, ["ff0000", "00ff00"]);
        assert_eq!(window.paint.key, "ff0000,00ff00");
        assert!(!window.changed());
    }

    #[test]
    fn a_handle_picked_writes_its_colour_into_the_gradient() {
        let mut window = Window::new(11, 1, "f0f0f2".into());
        window.update(Event::Effect(2));
        assert_eq!(window.paint.key, "2dafa7,fec491", "the first preset");
        window.update(Event::Handle(1));
        assert_eq!(window.picker, Some(Picker::Handle(1)));
        assert_eq!(window.code, "fec491", "the pad opens on the handle's");
        window.update(Event::Code("#0000ff".into()));
        assert_eq!(window.paint.key, "2dafa7,0000ff");
        // A code that isn't one is kept as written and changes nothing.
        window.update(Event::Code("00f".into()));
        assert_eq!(
            (window.code.as_str(), window.paint.key.as_str()),
            ("00f", "2dafa7,0000ff")
        );
        window.update(Event::Handle(1));
        assert_eq!(window.picker, None, "a second click closes it");
        assert!(window.changed());
    }

    #[test]
    fn ones_own_colour_and_a_preset_close_the_pad() {
        let mut window = Window::new(11, 3, String::new());
        assert_eq!(window.paint.key, "6b20ee", "neon's first preset");
        window.update(Event::Own);
        window.update(Event::Hsv([0.0, 1.0, 1.0]));
        assert_eq!(window.paint.key, "ff0000");
        window.update(Event::Preset("1f99ec"));
        assert_eq!((window.picker, window.paint.key.as_str()), (None, "1f99ec"));
    }
}
