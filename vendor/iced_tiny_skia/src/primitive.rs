use crate::core::Rectangle;

#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    /// A path filled with some paint.
    Fill {
        /// The path to fill.
        path: tiny_skia::Path,
        /// The paint to use.
        paint: tiny_skia::Paint<'static>,
        /// The fill rule to follow.
        rule: tiny_skia::FillRule,
    },
    /// A path stroked with some paint.
    Stroke {
        /// The path to stroke.
        path: tiny_skia::Path,
        /// The paint to use.
        paint: tiny_skia::Paint<'static>,
        /// The stroke settings.
        stroke: tiny_skia::Stroke,
    },
}

impl Primitive {
    /// Returns the visible bounds of the [`Primitive`].
    pub fn visible_bounds(&self) -> Rectangle {
        let bounds = match self {
            Primitive::Fill { path, .. } => path.bounds(),
            Primitive::Stroke { path, .. } => path.bounds(),
        };

        Rectangle {
            x: bounds.x(),
            y: bounds.y(),
            width: bounds.width(),
            height: bounds.height(),
        }
    }

    /// Yapfast port (patch 27): the pixels the [`Primitive`] can touch: a
    /// stroke's path bounds grown by half its width, and either grown by a
    /// pixel for anti-aliasing. A straight stroke's path bounds have no
    /// height (or width) and were dropped from a layer's extent.
    pub fn drawn_bounds(&self) -> Rectangle {
        let reach = match self {
            Primitive::Fill { .. } => 0.0,
            Primitive::Stroke { stroke, .. } => stroke.width / 2.0,
        };

        self.visible_bounds().expand(reach + 1.0)
    }
}
