//! The video playing, large over everything (`VideoViewer`,
//! `ui/messages.slint:2731`), opened as `Layer::Video`, whose dim, way in and
//! Escape are the shell's: enlarged to fit between the title bar and its
//! controls, 16:9 until the file says. The bar has its name, Save (not for a
//! link's preview) and the button back to its cell; the controls lie under
//! it, at least 360 wide. A click on the video plays or pauses it and a
//! right click opens its menu; a click anywhere else brings it back to its
//! cell. The size it shows at goes to the player as it shows and changes.

use iced::widget::{container, mouse_area, pin, responsive, row, sensor, space, stack, text};
use iced::{Alignment, Background, Fill, Padding, Rectangle, Size};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::typography::{self, size};
use anomi_vm::chat::video::{FullScreen, Intent};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_widgets::button::icon_button;
use anomi_widgets::menu::opener;

use super::controls::controls;
use super::keys::keys;
use super::look::{self, GROUND, INK};

/// The title bar's height and padding (`:2808`).
pub const BAR: f32 = 64.0;
const LEFT: f32 = 20.0;
const RIGHT: f32 = 12.0;
const SPACING: f32 = 8.0;
/// What the room leaves around the video: 40 each side, the bar above and
/// as much below for the controls (`:2763`).
const SIDES: f32 = 80.0;
const ENDS: f32 = 128.0;
/// The controls: 40 high, 8 under the video, at least 360 wide (`:2803`).
const CONTROLS: f32 = 40.0;
const CONTROLS_GAP: f32 = 8.0;
const CONTROLS_LEAST: f32 = 360.0;

/// Where the video lies in a `room` of the layer's size: `natural` scaled
/// to fit between the bar and the controls, centred.
pub fn fitted(natural: (f32, f32), room: Size) -> Rectangle {
    let (width, height) = natural;
    let free = Size::new((room.width - SIDES).max(0.0), (room.height - ENDS).max(0.0));
    let scale = (free.width / width).min(free.height / height);
    let size = Size::new(width * scale, height * scale);
    Rectangle {
        x: (room.width - size.width) / 2.0,
        y: BAR + (free.height - size.height) / 2.0,
        width: size.width,
        height: size.height,
    }
}

/// Where the controls lie under the video at `screen`.
pub fn under(screen: Rectangle, room: Size) -> Rectangle {
    let width = screen.width.max(CONTROLS_LEAST);
    Rectangle {
        x: (room.width - width) / 2.0,
        y: screen.y + screen.height + CONTROLS_GAP,
        width,
        height: CONTROLS,
    }
}

pub fn full_screen(full: &FullScreen, motion: Motion) -> Element<'_, Intent> {
    let content = responsive(move |room| placed(full, room, motion));
    keys(content.boxed(), &full.player).boxed()
}

fn placed(full: &FullScreen, room: Size, motion: Motion) -> Element<'_, Intent> {
    let screen = fitted(full.player.natural(), room);
    let strip = under(screen, room);
    // A press on what isn't the video, the controls or the bar's buttons.
    let outside = mouse_area(space().width(Fill).height(Fill)).on_press(Intent::Expand(false));
    let controls = container(controls(&full.player, true, motion))
        .width(strip.width)
        .height(strip.height);
    stack![
        outside,
        pin(picture(full, screen.size())).x(screen.x).y(screen.y),
        pin(controls).x(strip.x).y(strip.y),
        bar(full, motion),
    ]
    .width(Fill)
    .height(Fill)
    .boxed()
}

/// The video at `size`: its frame on black, "Loading..." or why it can't
/// play over it.
fn picture(full: &FullScreen, size: Size) -> Element<'_, Intent> {
    let player = &full.player;
    let mut layers: Vec<Element<'_, Intent>> = vec![
        container(space())
            .width(Fill)
            .height(Fill)
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(GROUND)),
                ..container::Style::default()
            })
            .boxed(),
    ];
    if let Some(frame) = &player.frame {
        layers.push(look::frame(frame));
    }
    match &player.error {
        Some(why) => layers.push(look::failure(why, false)),
        None if player.loading => layers.push(look::loading()),
        None => {}
    }
    let shown = stack(layers).width(size.width).height(size.height);
    let resize = |size: Size| Intent::Resize {
        width: size.width,
        height: size.height,
    };
    let sensed = sensor(shown).on_show(resize).on_resize(resize);
    let mut screen = opener(sensed).on_press(|_| Intent::Toggle);
    if full.has_menu() {
        screen = screen.on_right_press(|at| Intent::Menu(Spot::pointer(at)));
    }
    screen.boxed()
}

/// The name, Save and the way back to the cell.
fn bar(full: &FullScreen, motion: Motion) -> Element<'_, Intent> {
    let name = text(full.name.as_str())
        .size(size::BODY)
        .font(typography::BOLD)
        .color(INK)
        .ellipsis(text::Ellipsis::End)
        .wrapping(text::Wrapping::None)
        .width(Fill);
    let save = (!full.preview).then(|| {
        icon_button(Icon::Download, full.save_tip.as_str(), motion)
            .tint(|_| INK)
            .on_press(Intent::Save {
                item: full.item,
                index: full.index,
            })
            .view()
            .boxed()
    });
    let close = icon_button(Icon::X, tr!("Exit full screen"), motion)
        .tint(|_| INK)
        .on_press(Intent::Expand(false))
        .view()
        .boxed();
    row![name]
        .extend(save)
        .push(close)
        .spacing(SPACING)
        .align_y(Alignment::Center)
        .width(Fill)
        .height(BAR)
        .padding(Padding::ZERO.left(LEFT).right(RIGHT))
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM: Size = Size::new(1400.0, 828.0);

    #[test]
    fn a_wide_video_fits_the_height_between_the_bar_and_the_controls() {
        let screen = fitted((1280.0, 720.0), ROOM);
        assert_eq!(screen.height, ROOM.height - ENDS);
        assert_eq!(screen.y, BAR);
        assert!((screen.x + screen.width / 2.0 - ROOM.width / 2.0).abs() < 0.01);
        let strip = under(screen, ROOM);
        assert_eq!(strip.y, screen.y + screen.height + CONTROLS_GAP);
        assert_eq!(strip.width, screen.width);
    }

    #[test]
    fn an_upright_video_keeps_its_controls_wide_enough() {
        let screen = fitted((360.0, 1280.0), ROOM);
        assert!(screen.width < CONTROLS_LEAST);
        assert_eq!(under(screen, ROOM).width, CONTROLS_LEAST);
    }
}
