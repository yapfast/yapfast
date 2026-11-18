//! `scroll`: #general's long history with pictures, scrolled up and down.

use std::time::Instant;

use iced::Event;
use iced::advanced::mouse;
use iced::widget::image;
use iced_runtime::user_interface;

use anomi_screens::shots;
use anomi_vm::chat::attachments::{Attachments, Cell, Gallery};
use anomi_vm::chat::{self, message};
use anomi_vm::fixtures::chat::message::lit;
use anomi_vm::fixtures::chat::timeline;
use anomi_vm::kind::Expected;
use anomi_vm::{Intent, Vm};

use super::{Options, Window, pace};

const MESSAGES: usize = 400;
/// Frames each way: two seconds at 60 a second, as the measurement turns.
const TURN: usize = 120;

/// The members shot's window (#general, the list beside it) with a history
/// of `MESSAGES` messages, every fourth with a picture of its own.
pub fn vm() -> Vm {
    let shot = shots::all()
        .into_iter()
        .find(|shot| shot.area == "members" && shot.name == "general_unread_divider")
        .expect("the members shot");
    let mut vm = (shot.vm)();
    let mut history = timeline::long(MESSAGES);
    for (index, row) in history.rows.iter_mut().enumerate() {
        if index.is_multiple_of(4) {
            row.message.attachments = Attachments {
                item: row.key,
                gallery: Gallery::mosaic(vec![Cell {
                    index: 0,
                    name: format!("photo-{index}.png"),
                    picture: Expected::Ready(photo(index)),
                    width: 480,
                    height: 320,
                    ..Cell::default()
                }]),
                ..Attachments::default()
            };
        }
    }
    vm.chat.timeline = history;
    vm
}

/// The fake's picture for history message `index`: a gradient of its shade.
fn photo(index: usize) -> image::Handle {
    let (width, height) = (480u32, 320u32);
    let shade = [(index * 37) % 200, (index * 71) % 200, 160];
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend([
                (shade[0] + (55 * x / width) as usize) as u8,
                (shade[1] + (55 * y / height) as usize) as u8,
                shade[2] as u8,
                255,
            ]);
        }
    }
    image::Handle::from_rgba(width, height, rgba)
}

/// What the app does with the window's messages here: the message the
/// pointer is on lies lit, with its bar (`intent/chat/timeline.rs`
/// `light`); the rest change nothing this view shows.
pub fn take(vm: &mut Vm, messages: Vec<Intent>) {
    for message in messages {
        let Intent::Chat(chat::Intent::Message(message::Intent::Point(key))) = message else {
            continue;
        };
        for row in &mut vm.chat.timeline.rows {
            if row.message.lit && row.key != key {
                row.message.lit = false;
                row.message.bar = None;
            } else if row.key == key && !row.message.lit {
                row.message = lit(std::mem::take(&mut row.message));
            }
        }
    }
}

/// Where a frame goes on after the view was built again.
#[derive(PartialEq)]
enum Stage {
    Events,
    Redraw,
}

pub fn run(window: &mut Window, options: &Options) {
    let mut vm = vm();
    let mut cache = user_interface::Cache::default();
    let moved = Event::Mouse(mouse::Event::CursorMoved {
        position: window.cursor.position().unwrap_or_default(),
    });
    // Settled first: the pictures of the rows in view copied, as the window
    // has them by the time a person scrolls.
    for _ in 0..3 {
        let mut ui = window.build(&vm, cache);
        let _ = window.events(&mut ui, std::slice::from_ref(&moved));
        let _ = window.redraw(&mut ui, Instant::now());
        window.draw(&mut ui);
        cache = ui.into_cache();
        take(&mut vm, std::mem::take(&mut window.inbox));
    }
    window.totals = super::Totals::default();
    let _ = iced_tiny_skia::stats::take();
    let started = Instant::now();
    let mut frame = 0;
    let mut stage = Stage::Events;
    let mut redraws = 0;
    // A view lives until a message changes the view-model; the frame goes
    // on with the new one, as iced's loop runs `update` and `view` within it.
    'views: while frame < options.frames {
        take(&mut vm, std::mem::take(&mut window.inbox));
        let mut ui = window.build(&vm, cache);
        loop {
            if stage == Stage::Events {
                // Up first, as the measurement scrolls.
                let sign = if (frame / TURN).is_multiple_of(2) {
                    1.0
                } else {
                    -1.0
                };
                let delta = if options.lines {
                    mouse::ScrollDelta::Lines { x: 0.0, y: sign }
                } else {
                    mouse::ScrollDelta::Pixels {
                        x: 0.0,
                        y: sign * options.delta,
                    }
                };
                let events =
                    vec![Event::Mouse(mouse::Event::WheelScrolled { delta }); options.events];
                stage = Stage::Redraw;
                if window.events(&mut ui, &events) {
                    cache = ui.into_cache();
                    continue 'views;
                }
            }
            // iced's loop gives up after three redraws that each asked again.
            redraws += 1;
            if window.redraw(&mut ui, Instant::now()) && redraws < 3 {
                cache = ui.into_cache();
                continue 'views;
            }
            redraws = 0;
            window.draw(&mut ui);
            pace(started, frame, options.fps);
            frame += 1;
            stage = Stage::Events;
            if frame == options.frames {
                break 'views;
            }
        }
    }
}
