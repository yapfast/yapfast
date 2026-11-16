//! Draws a view headless and writes it as a PNG: iced's `UserInterface` and
//! its headless renderer, at any scale (`iced_test`'s snapshots are always
//! at 2x and write only when missing).

use std::borrow::Cow;
use std::path::Path;
use std::sync::Once;
use std::time::Instant;

use iced::advanced::renderer::{self, Headless};
use iced::advanced::{graphics, mouse, shell};
use iced::theme::Base;
use iced::{Event, Size, window};
use iced_runtime::{UserInterface, user_interface};

use anomi_theme::{Renderer, Theme, View, fonts, typography};

/// Loads the interface's fonts into the global font system, once.
fn load_fonts() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let mut system = graphics::text::font_system()
            .write()
            .expect("the font system");
        for font in fonts::ALL {
            system.load_font(Cow::Borrowed(font));
        }
    });
}

fn renderer() -> Renderer {
    let settings = renderer::Settings {
        font: typography::UI,
        ..renderer::Settings::default()
    };
    let backend = std::env::var("ICED_TEST_BACKEND").ok();
    block_on(<Renderer as Headless>::new(settings, backend.as_deref()))
        .expect("a headless renderer")
}

/// Waits for a future on this thread. The headless renderers are ready at
/// once or after a little work; nothing here waits on I/O.
fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::yield_now();
    }
}

/// Lays `view` out at `size` logical pixels, draws it once settled, and
/// writes the RGBA picture at `scale` to `path`.
pub fn png<Message>(
    view: impl View<Message>,
    size: (f32, f32),
    scale: f32,
    theme: &Theme,
    path: &Path,
) {
    load_fonts();
    let size = Size::new(size.0, size.1);
    let mut renderer = renderer();
    let mut ui = UserInterface::build(view, size, user_interface::Cache::default(), &mut renderer);
    let mut messages = shell::Bus::new();
    let redraw = Event::Window(window::Event::RedrawRequested(Instant::now()));
    let cursor = mouse::Cursor::Unavailable;
    let _ = ui.update(
        &window::Headless,
        &shell::Waker::noop(),
        &[redraw],
        cursor,
        &mut renderer,
        &mut messages,
    );
    let base = theme.base();
    ui.draw(
        &mut renderer,
        theme,
        &renderer::Style {
            text_color: base.text_color,
        },
        cursor,
    );
    let physical = Size::new(
        (size.width * scale).round() as u32,
        (size.height * scale).round() as u32,
    );
    let rgba = renderer.screenshot(physical, scale, base.background_color);
    write(path, physical, &rgba);
}

fn write(path: &Path, size: Size<u32>, rgba: &[u8]) {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).expect("the shots folder");
    }
    let file = std::io::BufWriter::new(std::fs::File::create(path).expect("the PNG file"));
    let mut encoder = png::Encoder::new(file, size.width, size.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("a PNG header");
    writer.write_image_data(rgba).expect("the PNG's pixels");
}
