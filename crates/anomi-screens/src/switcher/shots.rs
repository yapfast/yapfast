//! The switcher's shots, named after the golden screenshots
//! (`switcher/empty.png`, `switcher/query.png`).

use anomi_vm::fixtures::switcher;

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    vec![
        Shot {
            area: "switcher",
            name: "empty",
            size: GOLDEN,
            theme: "bonfire",
            vm: switcher::opened,
        },
        Shot {
            area: "switcher",
            name: "query",
            size: GOLDEN,
            theme: "bonfire",
            vm: switcher::searched,
        },
    ]
}
