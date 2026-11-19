//! The window's left column: the font and effect tiles and the colour row
//! (`ui/server.slint:1326`-`:1405`).

use iced::widget::{Row, button, column, container, mouse_area, row, space, stack, svg, text};
use iced::{Alignment, Background, Border, Color, Fill, Length, Padding, Radians, gradient};

use anomi_theme::fonts::{self, Name};
use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::kind::NameStyle as Style;
use anomi_vm::people::editor_preview;
use anomi_vm::people::name_style::{Intent, NameStyle, presets};
use anomi_widgets::icon::icon;
use anomi_widgets::name::styled_name;
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

pub const WIDTH: f32 = 424.0;
const GAP: f32 = 8.0;

pub fn colour(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// A tile: raised, the accent's wash and ring when chosen, the hover's when
/// pointed at. Slint also grows it by 4% under the pointer.
fn tile<'a>(
    content: Element<'a, Intent>,
    chosen: bool,
    height: f32,
    on: Intent,
) -> Element<'a, Intent> {
    let body = container(content)
        .width(Fill)
        .height(height)
        .center_x(Fill)
        .center_y(height)
        .clip(true);
    button(body)
        .padding(0)
        .width(Fill)
        .on_press(on)
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let fill = if chosen {
                Color {
                    a: 0.16,
                    ..palette.accent
                }
            } else if hovered {
                palette.hover
            } else {
                palette.raised
            };
            button::Style {
                background: Some(Background::Color(fill)),
                border: Border {
                    color: palette.accent,
                    width: if chosen { 2.0 } else { 0.0 },
                    radius: 12.0.into(),
                },
                ..catalog::button::bare(theme, status)
            }
        })
        .boxed()
}

fn heading(words: &str) -> Element<'_, Intent> {
    text(words)
        .size(13)
        .font(typography::SEMIBOLD)
        .style(catalog::text::secondary)
        .boxed()
}

fn grid<'a>(tiles: Vec<Element<'a, Intent>>, per: usize) -> Element<'a, Intent> {
    let mut rows = column![].spacing(GAP);
    let mut tiles = tiles.into_iter().peekable();
    while tiles.peek().is_some() {
        let mut line = row![].spacing(GAP);
        for _ in 0..per {
            line = line.push(tiles.next().unwrap_or_else(|| space().width(Fill).boxed()));
        }
        rows = rows.push(line);
    }
    rows.boxed()
}

fn faces(window: &NameStyle, motion: Motion) -> Element<'_, Intent> {
    let tiles = presets::FACES
        .iter()
        .map(|&(id, label)| {
            let font = Name::of(id).map_or(typography::MEDIUM, fonts::name);
            let glyphs = text("Gg").size(22).font(font).style(catalog::text::primary);
            let label: &str = label.unwrap_or(tr!("NameStyleWindow" => "Default"));
            let button = tile(glyphs.boxed(), window.font == id, 64.0, Intent::Font(id));
            tip(button, label, Side::Above, motion).boxed()
        })
        .collect();
    grid(tiles, 4)
}

fn effects(window: &NameStyle) -> Element<'_, Intent> {
    let tiles = presets::EFFECTS
        .iter()
        .map(|&id| {
            let label = match id {
                1 => tr!("Solid"),
                2 => tr!("Gradient"),
                3 => tr!("Neon"),
                4 => tr!("Toon"),
                5 => tr!("Pop"),
                8 => tr!("Gummy"),
                _ => tr!("Prism"),
            };
            let colours = presets::colours(presets::of(id)[0]);
            let name = styled_name(label, Style::new(11, id, &colours)).size(15.0);
            tile(name.boxed(), window.effect == id, 52.0, Intent::Effect(id))
        })
        .collect();
    grid(tiles, 3)
}

/// The colours as a left to right bar: evenly spread, or a gummy's stripes
/// (each letter takes one whole).
pub fn swatch(colours: &[u32], stripes: bool) -> Background {
    let colours: Vec<Color> = colours.iter().map(|&hex| colour(hex)).collect();
    match colours.as_slice() {
        [] => Background::Color(Color::TRANSPARENT),
        [one] => Background::Color(*one),
        many => {
            let mut bar = gradient::Linear::new(Radians(std::f32::consts::FRAC_PI_2));
            let count = many.len() as f32;
            for (at, colour) in many.iter().enumerate() {
                let at = at as f32;
                bar = if stripes {
                    bar.add_stop(at / count, *colour)
                        .add_stop((at + 1.0) / count, *colour)
                } else {
                    bar.add_stop(at / (count - 1.0), *colour)
                };
            }
            Background::Gradient(bar.into())
        }
    }
}

/// A preset's square, ringed in the text's colour when chosen.
fn square<'a>(fill: Background, chosen: bool, on: Intent) -> Element<'a, Intent> {
    button(space())
        .width(40.0)
        .height(40.0)
        .on_press(on)
        .style(move |theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(fill),
                border: Border {
                    color: theme.palette().text,
                    width: if chosen { 3.0 } else { 0.0 },
                    radius: 12.0.into(),
                },
                shadow: iced::Shadow::default(),
                ..catalog::button::bare(
                    theme,
                    if hovered {
                        status
                    } else {
                        button::Status::Active
                    },
                )
            }
        })
        .boxed()
}

/// The bar of one's own colours with a handle on each.
fn bar(window: &NameStyle) -> Element<'_, Intent> {
    let colours: Vec<u32> = window
        .handles()
        .iter()
        .filter_map(|h| presets::parse(h))
        .collect();
    let wide = if window.effect == 2 { 24.0 } else { 20.0 };
    let mut handles = Row::<Element<'_, Intent>>::new().align_y(Alignment::Center);
    for (at, hex) in window.handles().iter().enumerate() {
        let number = at as u8 + 1;
        let chosen = window.handle == number;
        let fill = colour(presets::parse(hex).unwrap_or(0));
        if at > 0 {
            handles = handles.push(space().width(Fill).boxed());
        }
        handles = handles.push(
            button(space())
                .width(wide)
                .height(24.0)
                .padding(0)
                .on_press(Intent::Handle(number))
                .style(move |_: &Theme, _| button::Style {
                    background: Some(Background::Color(fill)),
                    border: Border {
                        color: Color::WHITE,
                        width: if chosen { 3.0 } else { 2.0 },
                        radius: 7.0.into(),
                    },
                    shadow: iced::Shadow {
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                        blur_radius: 4.0,
                        ..iced::Shadow::default()
                    },
                    ..button::Style::default()
                })
                .boxed(),
        );
    }
    container(handles)
        .width(136.0)
        .height(40.0)
        .padding(Padding::from([8.0, 5.0]))
        .style(move |_: &Theme| container::Style {
            background: Some(swatch(&colours, false)),
            border: Border::default().rounded(12.0),
            ..container::Style::default()
        })
        .boxed()
}

fn eyedropper(window: &NameStyle) -> Element<'_, Intent> {
    let open = window.hex_open;
    let glyph = icon(Icon::Eyedropper, 16.0).style(|theme: &Theme, _| svg::Style {
        color: Some(theme.palette().text),
    });
    button(container(glyph).center(Fill))
        .width(40.0)
        .height(40.0)
        .padding(0)
        .on_press(Intent::Hex)
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let hovered =
                open || matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(if hovered {
                    palette.hover
                } else {
                    palette.raised
                })),
                border: Border::default().rounded(12.0),
                ..catalog::button::bare(theme, status)
            }
        })
        .boxed()
}

fn colours(window: &NameStyle) -> Element<'_, Intent> {
    let mut line = row![].spacing(GAP).height(40.0).align_y(Alignment::Center);
    line = line.push(if window.bar() {
        bar(window)
    } else {
        eyedropper(window)
    });
    for key in presets::of(window.effect) {
        let swatch = swatch(&presets::colours(key), window.effect == 8);
        let chosen = window.key == *key;
        line = line.push(square(swatch, chosen, Intent::Preset((*key).to_owned())));
    }
    line.boxed()
}

/// The colour picker (P15's popover card), its left 24 px in and its foot
/// 8 px over the colour row, which ends the column 24 px above its own
/// (`ui/server.slint:1524`). A press on it closes nothing.
fn picker(window: &NameStyle, motion: Motion) -> Element<'_, Intent> {
    let card = crate::editor::colour::view(&window.pick, motion).boxed();
    let card = Element::map(card, |asked| match asked {
        editor_preview::Intent::Colour(change) => Intent::Pick(change),
        _ => Intent::Ignored,
    });
    container(mouse_area(card).on_press(Intent::Ignored))
        .padding(Padding {
            left: 24.0,
            bottom: 24.0 + 40.0 + GAP,
            ..Padding::ZERO
        })
        .height(Fill)
        .align_bottom(Fill)
        .boxed()
}

/// The whole column.
pub fn view(window: &NameStyle, motion: Motion) -> Element<'_, Intent> {
    let list = column![
        text(tr!("Change Display Name Style"))
            .size(22)
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .width(Fill),
        heading(tr!("Font")),
        faces(window, motion),
        heading(tr!("Effect")),
        effects(window),
        heading(tr!("Colour")),
        colours(window),
    ]
    .spacing(12.0)
    .padding(24.0)
    .width(Length::Fixed(WIDTH));
    // One shape open or shut, so the column keeps its widgets' state.
    let mut layers = stack![list];
    if window.picking() {
        layers = layers.push(picker(window, motion));
    }
    layers.boxed()
}
