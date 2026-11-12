//! A theme's mouse cursors on the window (`docs/themes.md`, "Cursors").
//! [`load`] decodes each picture once, when a theme is shown; [`show`] hands
//! them to Slint's core (`yapfast_cursors` in `vendor/i-slint-core`), which
//! gives the window's adapter a theme cursor in place of a system one of the
//! same kind, and the winit backend shows it as a custom cursor on Windows,
//! X11, Wayland and macOS alike. [`hide`] puts the system's cursors back, as
//! `theme_look`'s reset does for the rest of a theme, so a protected dialog
//! shows them.
//!
//! A theme without cursors sets nothing: the core's hook stays empty and
//! every cursor is the system's, as before.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use i_slint_core::items::BuiltInMouseCursor;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use yapfast_core::themes::cursors::{self, Kind};
use yapfast_core::themes::package::Resolved;

/// One kind's cursor: each size's picture and width, smallest first, and
/// its hotspot on each.
struct Sizes(Vec<(Image, u32, (i32, i32))>);

#[derive(Default)]
struct State {
    cursors: Rc<Vec<(BuiltInMouseCursor, Sizes)>>,
    /// The person's switch for the shown theme.
    off: bool,
    /// The theme is painted: no protected dialog hides it.
    painted: bool,
    /// The core has the theme's cursors.
    given: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The Slint cursor of a kind.
fn builtin(kind: Kind) -> BuiltInMouseCursor {
    use BuiltInMouseCursor as Cursor;
    match kind {
        Kind::Default => Cursor::Default,
        Kind::Pointer => Cursor::Pointer,
        Kind::Text => Cursor::Text,
        Kind::Grab => Cursor::Grab,
        Kind::Grabbing => Cursor::Grabbing,
        Kind::NotAllowed => Cursor::NotAllowed,
        Kind::NoDrop => Cursor::NoDrop,
        Kind::Wait => Cursor::Wait,
        Kind::Progress => Cursor::Progress,
        Kind::Help => Cursor::Help,
        Kind::Crosshair => Cursor::Crosshair,
        Kind::Move => Cursor::Move,
        Kind::Copy => Cursor::Copy,
        Kind::Alias => Cursor::Alias,
        Kind::ColResize => Cursor::ColResize,
        Kind::RowResize => Cursor::RowResize,
        Kind::EwResize => Cursor::EwResize,
        Kind::NsResize => Cursor::NsResize,
        Kind::NeswResize => Cursor::NeswResize,
        Kind::NwseResize => Cursor::NwseResize,
        Kind::NResize => Cursor::NResize,
        Kind::EResize => Cursor::EResize,
        Kind::SResize => Cursor::SResize,
        Kind::WResize => Cursor::WResize,
        Kind::NeResize => Cursor::NeResize,
        Kind::NwResize => Cursor::NwResize,
        Kind::SeResize => Cursor::SeResize,
        Kind::SwResize => Cursor::SwResize,
    }
}

/// Decodes the shown theme's cursors, each file once. A cursor whose
/// pictures don't all load keeps the system's, so a theme never loses its
/// pointer to a bad file.
pub fn load(resolved: &Resolved, folder: &Path) {
    let mut decoded: BTreeMap<&PathBuf, Option<(Image, u32, u32)>> = BTreeMap::new();
    let mut cursors = Vec::new();
    for (kind, cursor) in &resolved.cursors {
        let mut sizes = Vec::new();
        for path in &cursor.images {
            let picture = decoded.entry(path).or_insert_with(|| {
                let full = folder.join(path);
                let picture = std::fs::read(&full)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| cursors::decode(&bytes));
                match picture {
                    Ok(picture) => {
                        let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                            &picture.rgba,
                            picture.width,
                            picture.height,
                        );
                        Some((Image::from_rgba8(buffer), picture.width, picture.height))
                    }
                    Err(error) => {
                        tracing::warn!(file = %full.display(), %error, "a theme cursor didn't load");
                        None
                    }
                }
            });
            sizes.push(picture.clone());
        }
        let Some(sizes) = sizes.into_iter().collect::<Option<Vec<_>>>() else {
            continue;
        };
        let first = (sizes[0].1, sizes[0].2);
        let sizes = sizes
            .into_iter()
            .map(|(image, width, height)| {
                let (x, y) = cursors::scaled_hotspot(cursor.hotspot, first, (width, height));
                (image, width, (x as i32, y as i32))
            })
            .collect();
        cursors.push((builtin(*kind), Sizes(sizes)));
    }
    STATE.with_borrow_mut(|state| state.cursors = Rc::new(cursors));
    update();
}

/// Whether the shown theme has cursors.
pub fn has_cursors() -> bool {
    STATE.with_borrow(|state| !state.cursors.is_empty())
}

/// Shows the theme's cursors, unless the person turned them off.
pub fn show() {
    STATE.with_borrow_mut(|state| state.painted = true);
    update();
}

/// Puts the system's cursors back until [`show`].
pub fn hide() {
    STATE.with_borrow_mut(|state| state.painted = false);
    update();
}

/// The person's switch for the shown theme's cursors.
pub fn set_on(on: bool) {
    STATE.with_borrow_mut(|state| state.off = !on);
    update();
}

/// Forgets the theme's cursors.
pub fn forget() {
    STATE.with_borrow_mut(|state| {
        state.cursors = Rc::default();
        state.painted = false;
    });
    update();
}

/// Gives the core the theme's cursors, or takes them back, when that
/// changes.
fn update() {
    let wanted = STATE.with_borrow(|state| {
        (state.painted && !state.off && !state.cursors.is_empty()).then(|| state.cursors.clone())
    });
    let given = STATE.with_borrow(|state| state.given);
    if wanted.is_none() && !given {
        return;
    }
    STATE.with_borrow_mut(|state| state.given = wanted.is_some());
    let hook = wanted.map(|cursors| -> i_slint_core::yapfast_cursors::Hook {
        Box::new(move |kind, scale| {
            let (_, Sizes(sizes)) = cursors.iter().find(|(shown, _)| *shown == kind)?;
            let widths: Vec<u32> = sizes.iter().map(|(_, width, _)| *width).collect();
            let (image, _, (x, y)) = &sizes[cursors::pick(&widths, scale)];
            Some((image.clone(), *x, *y))
        })
    });
    i_slint_core::yapfast_cursors::set(hook);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_its_slint_cursor() {
        let mut seen = Vec::new();
        for kind in Kind::ALL {
            let cursor = builtin(kind);
            assert!(!seen.contains(&cursor), "{kind:?}");
            seen.push(cursor);
        }
    }
}
