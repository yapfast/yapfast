//! The interface's icons, coloured by the theme: [`icon`] draws one. The
//! list, [`Icon`], is `anomi_vm::icon`'s, so view-models can hold icons;
//! add an icon there (task F04).

use iced::widget::{Svg, svg};

use anomi_theme::catalog;
use anomi_theme::prelude::*;

pub use anomi_vm::icon::Icon;

/// An icon at a square size, muted until the pointer is over it (the svg
/// catalog's default); `.style(...)` paints it otherwise. `Verified` keeps its
/// own colours (green with a white check), as in the Slint interface.
pub fn icon<'a>(icon: Icon, size: f32) -> Svg<'a, Theme> {
    let svg = svg(icon.handle()).width(size).height(size);
    if icon == Icon::Verified {
        svg.style(catalog::svg::original)
    } else {
        svg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use resvg::{tiny_skia, usvg};

    /// Every SVG of `ui/icons` has a variant, and every variant's file exists.
    #[test]
    fn every_file_has_a_variant() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ui/icons");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| {
                let name = e.unwrap().file_name().into_string().unwrap();
                name.strip_suffix(".svg").map(str::to_owned)
            })
            .collect();
        files.sort();
        let mut have: Vec<String> = Icon::ALL.iter().map(|i| i.file().to_owned()).collect();
        have.sort();
        assert_eq!(files, have);
    }

    /// Each icon parses and, drawn at 20 px, puts something on the canvas.
    #[test]
    fn every_icon_draws_at_20px() {
        for icon in Icon::ALL {
            let tree = usvg::Tree::from_data(icon.bytes(), &usvg::Options::default())
                .unwrap_or_else(|e| panic!("{}: {e}", icon.file()));
            let size = tree.size();
            let scale = 20.0 / size.width().max(size.height());
            let mut pixmap = tiny_skia::Pixmap::new(20, 20).unwrap();
            resvg::render(
                &tree,
                tiny_skia::Transform::from_scale(scale, scale),
                &mut pixmap.as_mut(),
            );
            assert!(
                pixmap.pixels().iter().any(|p| p.alpha() > 0),
                "{} draws nothing at 20 px",
                icon.file()
            );
        }
    }
}
