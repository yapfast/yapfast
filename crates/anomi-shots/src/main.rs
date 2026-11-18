//! Renders each screen of the iced interface from its sample view-model to a
//! PNG, without a display, for reviewers to hold beside the reference
//! screenshots of the Slint app. Several reviewers can run it at once.
//!
//!     anomi-shots [OUT_DIR] [--scale 1] [--language ru|en] [--only TEXT] [--time FRAMES]
//!
//! OUT_DIR defaults to `target/anomi-shots`; each shot goes to
//! `OUT_DIR/<area>/<name>.png`. The language is Russian unless given, as the
//! app's is. `--only` keeps the shots whose `area/name` contains TEXT.
//! `--time` has the renderer draw each shot whole FRAMES times more and
//! prints the milliseconds a frame took, its buffer and the conversion to
//! RGBA included: what the renderer costs, without a window (measure with
//! the `fast` profile or a faster one).

mod render;

use std::path::PathBuf;

use anomi_screens::shots;
use anomi_theme::{Motion, Theme};

fn main() {
    let mut out = PathBuf::from("target/anomi-shots");
    let mut scale = 1.0f32;
    let mut language = yapfast_lang::DEFAULT.to_owned();
    let mut only = String::new();
    let mut time = 0usize;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let mut value = || arguments.next().unwrap_or_else(|| usage(&argument));
        match argument.as_str() {
            "--scale" => scale = value().parse().unwrap_or_else(|_| usage("--scale")),
            "--language" => language = value(),
            "--only" => only = value(),
            "--time" => time = value().parse().unwrap_or_else(|_| usage("--time")),
            "--help" | "-h" => usage(""),
            path if !path.starts_with('-') => out = PathBuf::from(path),
            other => usage(other),
        }
    }
    yapfast_lang::choose(&language);
    // Drawn as a window in front would be, with nothing still moving.
    let motion = Motion {
        front: true,
        never: true,
        idle: false,
    };
    let mut written = 0;
    for shot in shots::all() {
        let id = format!("{}/{}", shot.area, shot.name);
        if !id.contains(&only) {
            continue;
        }
        let vm = (shot.vm)();
        let theme = Theme::built_in(Some(shot.theme));
        let path = out.join(format!("{id}.png"));
        let view = anomi_screens::window(&vm, motion);
        let drawn = render::png(view, shot.size, scale, &theme, &path, time);
        match drawn {
            Some(per_frame) => println!("{} {:.3} ms/frame", path.display(), per_frame * 1e3),
            None => println!("{}", path.display()),
        }
        written += 1;
    }
    if written == 0 {
        eprintln!("no shot matches {only:?}");
        std::process::exit(1);
    }
}

fn usage(argument: &str) -> ! {
    if !argument.is_empty() {
        eprintln!("unexpected argument {argument:?}");
    }
    eprintln!(
        "usage: anomi-shots [OUT_DIR] [--scale 1] [--language ru|en] [--only TEXT] [--time FRAMES]"
    );
    std::process::exit(2)
}
