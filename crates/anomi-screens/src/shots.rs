//! The screens to render headless from sample view-models, for reviewers to
//! hold beside the reference screenshots of the Slint app
//! (`~/.cache/anomi-iced/golden`). `anomi-shots` draws each to a PNG.
//!
//! Each area adds its shots to [`all`] from a `shots()` of its own module,
//! named after the golden screenshot it should match where there is one.

use anomi_vm::{Vm, fixtures};

/// One picture: the whole window in a theme at a size, from a view-model.
pub struct Shot {
    /// The area's folder, as `shell` or `dm`.
    pub area: &'static str,
    /// The file name, without `.png`.
    pub name: &'static str,
    /// The window's size in logical pixels.
    pub size: (f32, f32),
    /// A built-in theme's id.
    pub theme: &'static str,
    pub vm: fn() -> Vm,
}

/// The golden screenshots' window size, and their small one.
pub const GOLDEN: (f32, f32) = (1400.0, 860.0);
pub const SMALL: (f32, f32) = (1000.0, 640.0);

pub fn all() -> Vec<Shot> {
    let mut shots = shell();
    shots.sort_by_key(|shot| (shot.area, shot.name));
    shots
}

/// The shell: rail, sidebar, account card and an empty conversation.
fn shell() -> Vec<Shot> {
    let shot = |name, size, theme, vm| Shot {
        area: "shell",
        name,
        size,
        theme,
        vm,
    };
    vec![
        shot(
            "dm_home",
            GOLDEN,
            "bonfire",
            fixtures::dm_home as fn() -> Vm,
        ),
        shot("server_home", GOLDEN, "bonfire", fixtures::server_home),
        shot(
            "dm_home_latte",
            GOLDEN,
            "catppuccin-latte",
            fixtures::dm_home,
        ),
        shot("dm_home_small", SMALL, "bonfire", fixtures::dm_home),
        shot("connecting", GOLDEN, "bonfire", fixtures::connecting),
    ]
}
