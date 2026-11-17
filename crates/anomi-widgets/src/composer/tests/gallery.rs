//! The field in a frame like the channel's, drawn to PNGs for a reviewer
//! to hold beside `composer/reply.png` and C14 to build from, as widgets
//! have no screen of their own:
//!
//! ```sh
//! ANOMI_GALLERY=/tmp/composer cargo test -p anomi-widgets composer::tests::gallery -- --ignored
//! ```
//!
//! Empty with its placeholder, written with a mention marked, five lines
//! grown, the text selected, and off; at 1x and 2x, in Bonfire and Latte.
//! `gallery_menu` adds the edit menu open over a selection.

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker as TaskWaker};

use iced::advanced::renderer::{Headless as _, Renderer as _, Style};
use iced::theme::Base as _;
use iced::widget::container;
use iced::{Background, Border};

use anomi_theme::fonts;
use anomi_theme::metrics::{COMPOSER_HEIGHT, COMPOSER_PADDING};
use anomi_theme::prelude::*;

use super::*;

/// The frame's width in `composer/reply.png` (385 to 1127), less its
/// plus button and the three on the right.
pub(super) const FRAME: f32 = 742.0;

pub(super) fn renderer() -> Renderer {
    fonts::install();
    let settings = Look {
        font: typography::UI,
        ..Look::default()
    };
    let mut future = pin!(<Renderer as iced::advanced::renderer::Headless>::new(
        settings, None
    ));
    let mut context = Context::from_waker(TaskWaker::noop());
    loop {
        if let Poll::Ready(renderer) = future.as_mut().poll(&mut context) {
            return renderer.expect("a headless renderer");
        }
    }
}

/// The field framed as `ComposerFrame` (`ui/composer.slint:68`) draws it,
/// with the room its buttons take at either side.
pub(super) fn framed(field: Composer<'_, Asked>) -> Element<'_, Asked> {
    let field = field.pad(COMPOSER_PADDING).min_height(COMPOSER_HEIGHT);
    container(field)
        .padding(iced::Padding {
            left: 47.0,
            right: 120.0,
            ..iced::Padding::ZERO
        })
        .width(FRAME)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(palette.composer())),
                border: Border::default()
                    .rounded(10)
                    .width(1)
                    .color(palette.outline_field),
                ..container::Style::default()
            }
        })
        .boxed()
}

#[test]
#[ignore = "writes PNGs for review"]
fn gallery() {
    let Some(dir) = std::env::var_os("ANOMI_GALLERY") else {
        return;
    };
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).expect("the gallery's folder");
    let marks = [
        (6..10, None),
        (15..23, Some(iced::Color::from_rgb8(0xe6, 0x7e, 0x22))),
    ];
    let five = "one\ntwo\nthree\nfour\nfive";
    let mut renderer = renderer();
    for built_in in ["bonfire", "catppuccin-latte"] {
        let theme = Theme::built_in(Some(built_in));
        let views: [(&str, Element<'_, Asked>, bool); 5] = [
            (
                "empty",
                framed(field("", 1).placeholder("Write a reply")),
                false,
            ),
            (
                "marked",
                framed(field("hello @Bob and @Mods ok", 1).marks(&marks)),
                false,
            ),
            ("grown", framed(field(five, 1)), false),
            ("selected", framed(field("hello there", 1)), true),
            (
                "off",
                framed(composer("", front()).placeholder("Message #general")),
                false,
            ),
        ];
        for (name, mut view, select) in views {
            let window = Size::new(FRAME, 200.0);
            let mut tree = Tree::empty();
            tree.diff(&mut view);
            view.layout(&mut tree, &renderer, &Limits::new(Size::ZERO, window));
            let mut bus = Bus::new();
            let mut shell = Shell::new(&window::Headless, Waker::noop(), &mut bus);
            let viewport = Rectangle::with_size(window);
            let layout = Layout::new(tree.size);
            if select {
                view.operate(
                    &mut tree,
                    layout,
                    &viewport,
                    &renderer,
                    &mut focusable::focus::<()>(Id::new(FIELD)),
                );
                view.update(
                    &mut tree,
                    &ctrl("a", Code::KeyA),
                    layout,
                    mouse::Cursor::Unavailable,
                    &renderer,
                    &mut shell,
                    &viewport,
                );
            }
            for scale in [1.0, 2.0] {
                let base = theme.base();
                renderer.reset(viewport);
                view.draw(
                    &tree,
                    &mut renderer,
                    &theme,
                    &Style {
                        text_color: base.text_color,
                    },
                    Layout::new(tree.size),
                    mouse::Cursor::Unavailable,
                    &viewport,
                );
                let size = Size::new((FRAME * scale) as u32, (tree.size.height * scale) as u32);
                let rgba = renderer.screenshot(size, scale, theme.palette().chat);
                let pixels = image::RgbaImage::from_raw(size.width, size.height, rgba)
                    .expect("a pixel for each");
                let path = dir.join(format!("{built_in}-{name}@{scale}x.png"));
                pixels.save(path).expect("saved");
            }
        }
    }
}
