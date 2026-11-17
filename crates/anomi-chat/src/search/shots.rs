//! Search's shots: the panel beside #general, as the golden
//! `header/search_results.png`, and each state it can be in. The header's
//! field (`header/search_focus.png`) has no shot until the header's row
//! places it.

use anomi_vm::fixtures::chat::search as sample;

use crate::shots::{GOLDEN, SMALL, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm: fn() -> anomi_vm::Vm| Shot {
        area: "header",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("search_results", sample::beside),
        shot("search_kinds", || sample::beside_with(sample::kinds())),
        shot("search_loading", || sample::beside_with(sample::loading())),
        shot("search_more", || sample::beside_with(sample::more())),
        shot("search_waiting", || sample::beside_with(sample::waiting())),
        shot("search_nothing", || sample::beside_with(sample::nothing())),
        Shot {
            size: SMALL,
            ..shot("search_results_small", sample::beside)
        },
    ]
}
