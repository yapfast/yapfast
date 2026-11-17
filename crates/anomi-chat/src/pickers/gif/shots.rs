//! The expression picker's shots with its GIFs tab, named after the golden
//! screenshot (`picker/gif.png`: the home over the composer's button) and its
//! other pages and a wider card. The GIFs play the
//! fake's bouncing dot.

use iced::widget::image;

use anomi_vm::Vm;
use anomi_vm::fixtures::chat::{gif_picker as sample, pickers};
use anomi_widgets::picture::sample::bouncing;

use super::super::shots::open;
use crate::shots::{GOLDEN, Shot};

fn moving(_src: &str) -> image::Handle {
    bouncing(96, 10, [124, 196, 255])
}

fn home() -> Vm {
    open(pickers::gifs())
}

fn loading() -> Vm {
    open(pickers::with_gifs(sample::home_loading()))
}

fn error() -> Vm {
    open(pickers::with_gifs(sample::home_error()))
}

fn search() -> Vm {
    open(pickers::with_gifs(sample::search_with("cat", moving)))
}

fn searching() -> Vm {
    open(pickers::with_gifs(sample::searching("cat")))
}

fn nothing() -> Vm {
    open(pickers::with_gifs(sample::nothing("zzzz")))
}

fn trending() -> Vm {
    open(pickers::with_gifs(sample::trending_with(moving)))
}

fn favorites() -> Vm {
    open(pickers::with_gifs(sample::favorites_with(moving)))
}

fn favorites_empty() -> Vm {
    open(pickers::with_gifs(sample::favorites_empty()))
}

fn wide() -> Vm {
    let mut wide = pickers::wide();
    wide.gif = sample::search_with("cat", moving);
    open(wide)
}

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm| Shot {
        area: "picker",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("gif", home),
        shot("gif_loading", loading),
        shot("gif_error", error),
        shot("gif_search", search),
        shot("gif_searching", searching),
        shot("gif_nothing", nothing),
        shot("gif_trending", trending),
        shot("gif_favorites", favorites),
        shot("gif_favorites_empty", favorites_empty),
        shot("gif_wide", wide),
    ]
}
