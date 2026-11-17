//! The edit menu open over a selection, drawn to PNGs for a reviewer to
//! hold beside the Slint app's (no golden shot has it):
//!
//! ```sh
//! ANOMI_GALLERY=/tmp/composer cargo test -p anomi-widgets composer::tests::gallery_menu -- --ignored
//! ```
//!
//! The field at the foot of the picture, the menu above the pointer; at 1x
//! and 2x, in Bonfire and Latte.

use iced::advanced::renderer::{Headless as _, Renderer as _, Style};
use iced::theme::Base as _;
use iced::widget::container;
use iced::{Length, Vector};

use anomi_theme::prelude::*;

use super::gallery::{FRAME, framed, renderer};
use super::*;

/// Room above the field for the menu to open into.
const HEIGHT: f32 = 240.0;

#[test]
#[ignore = "writes PNGs for review"]
fn gallery_menu() {
    let Some(dir) = std::env::var_os("ANOMI_GALLERY") else {
        return;
    };
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).expect("the gallery's folder");
    let mut renderer = renderer();
    let window = Size::new(FRAME, HEIGHT);
    let viewport = Rectangle::with_size(window);
    for built_in in ["bonfire", "catppuccin-latte"] {
        let theme = Theme::built_in(Some(built_in));
        // Held still, so the menu shows fully open.
        let mut still = field("hello there", 1);
        still.motion = Motion {
            never: true,
            ..front()
        };
        let mut view: Element<'_, Asked> = container(framed(still))
            .height(Length::Fill)
            .align_bottom(Length::Fill)
            .boxed();
        let mut tree = Tree::empty();
        tree.diff(&mut view);
        view.layout(&mut tree, &renderer, &Limits::new(Size::ZERO, window));
        let layout = Layout::new(tree.size);
        view.operate(
            &mut tree,
            layout,
            &viewport,
            &renderer,
            &mut focusable::focus::<()>(Id::new(FIELD)),
        );
        let pointer = mouse::Cursor::Available(Point::new(260.0, HEIGHT - 28.0));
        let right = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
        for (event, cursor) in [
            (ctrl("a", Code::KeyA), mouse::Cursor::Unavailable),
            (right, pointer),
        ] {
            let mut bus = Bus::new();
            let mut shell = Shell::new(&window::Headless, Waker::noop(), &mut bus);
            view.update(
                &mut tree, &event, layout, cursor, &renderer, &mut shell, &viewport,
            );
        }
        for scale in [1.0, 2.0] {
            let style = Style {
                text_color: theme.base().text_color,
            };
            renderer.reset(viewport);
            let none = mouse::Cursor::Unavailable;
            view.draw(
                &tree,
                &mut renderer,
                &theme,
                &style,
                layout,
                none,
                &viewport,
            );
            let overlays = view.overlay(
                &mut tree,
                layout,
                &renderer,
                &viewport,
                Vector::ZERO,
                window,
            );
            assert_eq!(overlays.len(), 1, "the menu is open");
            for overlay in &overlays {
                overlay
                    .as_overlay()
                    .draw(&mut renderer, &theme, &style, none);
            }
            drop(overlays);
            let size = Size::new((FRAME * scale) as u32, (HEIGHT * scale) as u32);
            let rgba = renderer.screenshot(size, scale, theme.palette().chat);
            let pixels = image::RgbaImage::from_raw(size.width, size.height, rgba)
                .expect("a pixel for each");
            let path = dir.join(format!("{built_in}-menu@{scale}x.png"));
            pixels.save(path).expect("saved");
        }
    }
}
