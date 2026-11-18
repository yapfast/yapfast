//! The gallery (`MessageGallery`, `ui/messages.slint:1615`): cells placed in
//! a block of the size the app laid out, each with its picture, a spoiler's
//! cover, a video's play button, a GIF's star while the pointer is on it and
//! Save while it is on the message.

use iced::widget::{container, hover, mouse_area, responsive, stack};
use iced::{Color, Fill, Length, Padding, Shrink, Size, alignment, mouse::Interaction};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_vm::chat::attachments::{Attachments, Cell, Intent};
use anomi_vm::kind::Expected;
use anomi_vm::menu::Spot;
use anomi_widgets::button::icon_button;
use anomi_widgets::cover;
use anomi_widgets::icon::Icon;
use anomi_widgets::menu::opener;
use anomi_widgets::picture::{Fit, blurred, picture};
use anomi_widgets::skeleton::skeleton;

use crate::pickers::gif::star;
use crate::video;

/// A cell's corners (`border-radius: 8px`).
const RADIUS: f32 = 8.0;
/// Under the message's text (`padding-top: 4px`).
const TOP: f32 = 4.0;
/// The star's and Save's distance from the cell's corner.
const INSET: f32 = 8.0;
/// Save's chip: 32 px, `#000000b3`, radius 6.
const SAVE: f32 = 32.0;
const SAVE_GROUND: Color = Color::from_rgba(0.0, 0.0, 0.0, 179.0 / 255.0);

pub fn view<'a>(attachments: &'a Attachments, motion: Motion) -> Element<'a, Intent> {
    let gallery = &attachments.gallery;
    // As laid out when there is room; narrower, it shrinks to the width it
    // gets, keeping its shape (`scale`), so a narrow timeline doesn't cut it.
    let block = responsive(move |room: Size| -> Element<'a, Intent> {
        let scale = scale(gallery.width, room.width);
        let cells = gallery
            .cells
            .iter()
            .map(|cell| placed(cell, scale, cell_view(attachments, cell, motion)));
        stack(cells)
            .width(gallery.width as f32 * scale)
            .height(gallery.height as f32 * scale)
            .boxed()
    })
    .height(Shrink);
    container(block).padding(Padding::ZERO.top(TOP)).boxed()
}

/// How much a gallery `width` wide shrinks to fit in `room`; it never grows.
fn scale(width: u32, room: f32) -> f32 {
    if width == 0 || !room.is_finite() {
        return 1.0;
    }
    (room / width as f32).clamp(0.0, 1.0)
}

/// `content` at the cell's place, of its size, both `scale`d.
fn placed<'a>(cell: &Cell, scale: f32, content: Element<'a, Intent>) -> Element<'a, Intent> {
    let at = |px: u32| px as f32 * scale;
    container(
        container(content)
            .width(at(cell.width))
            .height(at(cell.height)),
    )
    .padding(Padding::ZERO.left(at(cell.x)).top(at(cell.y)))
    .width(Fill)
    .height(Fill)
    .boxed()
}

fn cell_view<'a>(
    attachments: &'a Attachments,
    cell: &'a Cell,
    motion: Motion,
) -> Element<'a, Intent> {
    let item = attachments.item;
    let index = cell.index;
    let acts = attachments.acts();
    let face = face(cell, motion);
    let face = if cell.spoiler {
        cover::spoiler(face, true, RADIUS, Intent::Reveal { item, index })
    } else {
        mouse_area(face)
            .on_press(Intent::Open { item, index })
            .interaction(Interaction::Pointer)
            .boxed()
    };
    let mut layers = vec![face];
    if let Some(playing) = cell
        .video
        .as_ref()
        .filter(|_| !cell.spoiler && !attachments.preview && cell.picture != Expected::Waiting)
    {
        layers.push(video::view(playing, motion).map(Intent::Video).boxed());
    }
    let menu = opener(stack(layers)).on_right_press(move |at| Intent::Menu {
        item,
        index,
        video: cell.video.is_some(),
        spot: Spot::pointer(at),
    });
    let mut cell_view: Element<'a, Intent> = menu.boxed();
    if let Some(gif) = cell.gif.filter(|_| !cell.spoiler && acts) {
        let chip = star(gif.favorite, Intent::Favorite { item, index }, motion);
        cell_view = hover(cell_view, corner(chip, alignment::Horizontal::Left)).boxed();
    }
    if acts && !cell.spoiler {
        let save = corner(
            save(item, index, &cell.name, motion),
            alignment::Horizontal::Right,
        );
        cell_view = if attachments.lit {
            stack![cell_view, save].boxed()
        } else {
            hover(cell_view, save).boxed()
        };
    }
    cell_view
}

/// The picture, a skeleton while it comes, or its ground when there is none;
/// a spoiler's picture blurred.
fn face<'a>(cell: &Cell, motion: Motion) -> Element<'a, Intent> {
    match &cell.picture {
        Expected::Waiting => skeleton(motion)
            .width(Fill)
            .height(Fill)
            .radius(RADIUS)
            .boxed(),
        Expected::None => ground(),
        Expected::Ready(handle) => {
            let handle = if cell.spoiler {
                match blurred(handle) {
                    Some(blur) => blur,
                    None => return ground(),
                }
            } else {
                handle.clone()
            };
            picture(handle)
                .width(Fill)
                .height(Fill)
                .fit(Fit::Cover)
                .radius(RADIUS)
                .motion(motion)
                .boxed()
        }
    }
}

fn ground<'a>() -> Element<'a, Intent> {
    container(iced::widget::space())
        .width(Fill)
        .height(Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().sunken.into()),
            border: iced::Border::default().rounded(RADIUS),
            ..container::Style::default()
        })
        .boxed()
}

/// `content` in the cell's top corner, `INSET` in.
fn corner<'a>(content: Element<'a, Intent>, side: alignment::Horizontal) -> Element<'a, Intent> {
    container(content)
        .padding(INSET)
        .align_x(side)
        .align_y(alignment::Vertical::Top)
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed()
}

fn save<'a>(item: u64, index: usize, name: &str, motion: Motion) -> Element<'a, Intent> {
    icon_button(Icon::Download, tr!("Save {}", name), motion)
        .size(SAVE)
        .radius(6.0)
        .tint(|_| Color::WHITE)
        .fill(|_| SAVE_GROUND)
        .on_press(Intent::Save { item, index })
        .view()
        .boxed()
}
