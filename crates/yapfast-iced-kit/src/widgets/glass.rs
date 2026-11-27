//! Themes' fake glass: the window's art, reduced and blurred once,
//! cut to each pane and laid under its ground, rim, highlight and inset.
//! It never samples other widgets; only the art shows through.

use std::sync::{Arc, Mutex};

use iced::{
    Color, Point, Rectangle, Size, Vector,
    advanced::{Renderer as _, image::Handle, renderer},
};

use super::{Renderer, draw_picture, gradient_quad};

/// Pieces cut for panes, by the art pixels they cover, the latest last.
type Cuts = Arc<Mutex<Vec<([u32; 4], Handle)>>>;

/// The reduced, preblurred window art, and the pieces of it cut for the
/// panes now shown.
#[derive(Clone)]
pub struct BlurredWindow {
    pixels: Arc<(u32, u32, Vec<u8>)>,
    crops: Cuts,
    /// The window size, in logical pixels, the art was laid out for.
    pub window: (u32, u32),
}

impl std::fmt::Debug for BlurredWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlurredWindow")
            .field("size", &(self.pixels.0, self.pixels.1))
            .field("window", &self.window)
            .finish_non_exhaustive()
    }
}

impl BlurredWindow {
    /// `pixels` are `width` x `height` RGBA, already in the renderer's byte
    /// order, for a window of `window` logical pixels.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>, window: (u32, u32)) -> Self {
        debug_assert_eq!(pixels.len(), width as usize * height as usize * 4);
        Self {
            pixels: Arc::new((width, height, pixels)),
            crops: Arc::default(),
            window,
        }
    }

    /// The same blurred art stretched over a window of another size, shown
    /// while a resize's own blur is baked off the window's thread.
    #[must_use]
    pub fn stretched_to(&self, window: (u32, u32)) -> Self {
        Self {
            pixels: self.pixels.clone(),
            // Cuts are by the art's own pixels, so they still serve.
            crops: self.crops.clone(),
            window,
        }
    }

    /// The blurred art under `pane` (window coordinates), as a picture to
    /// stretch over exactly that rectangle.
    pub fn under(&self, pane: Rectangle) -> Option<Handle> {
        let (width, height, pixels) = &*self.pixels;
        if *width == 0 || *height == 0 {
            return None;
        }
        let (sx, sy) = (
            *width as f32 / self.window.0.max(1) as f32,
            *height as f32 / self.window.1.max(1) as f32,
        );
        let x0 = ((pane.x * sx).floor().max(0.0) as u32).min(width - 1);
        let y0 = ((pane.y * sy).floor().max(0.0) as u32).min(height - 1);
        let x1 = (((pane.x + pane.width) * sx).ceil().max(0.0) as u32).clamp(x0 + 1, *width);
        let y1 = (((pane.y + pane.height) * sy).ceil().max(0.0) as u32).clamp(y0 + 1, *height);
        let key = [x0, y0, x1, y1];
        let mut crops = self.crops.lock().ok()?;
        // A pane a fraction of a pixel taller one frame than the next (a
        // layout settling) keeps its cut: a cut a pixel off either way
        // serves. A new picture each frame made the CPU renderer draw the
        // whole pane again, every time.
        let near = |at: &[u32; 4]| at.iter().zip(key.iter()).all(|(a, b)| a.abs_diff(*b) <= 1);
        if let Some(index) = crops.iter().position(|(at, _)| near(at)) {
            // Most recently used last.
            let found = crops.remove(index);
            let handle = found.1.clone();
            crops.push(found);
            return Some(handle);
        }
        let row = *width as usize * 4;
        let run = (x1 - x0) as usize * 4;
        let mut cut = Vec::with_capacity(run * (y1 - y0) as usize);
        for y in y0..y1 {
            let start = y as usize * row + x0 as usize * 4;
            cut.extend_from_slice(&pixels[start..start + run]);
        }
        let handle = Handle::from_rgba(x1 - x0, y1 - y0, cut);
        // A few panes show at once; a resize leaves old cuts to drop.
        if crops.len() >= 24 {
            crops.remove(0);
        }
        crops.push((key, handle.clone()));
        Some(handle)
    }
}

/// How a glass pane is finished over the art.
#[derive(Clone, Copy, Debug)]
pub struct GlassLook {
    /// The surface's own ground, a colour or a gradient, as the Slint
    /// build lays `Theme.<surface>` over the blur.
    pub ground: iced::Background,
    /// How much of the ground covers the art, 0 to 1.
    pub opacity: f32,
    /// Each corner's radius: top left, top right, bottom right, bottom left.
    pub radii: [f32; 4],
    pub rim: Option<(Color, f32)>,
    pub highlight: Option<Color>,
    /// The inset's colour and reach.
    pub inset: Option<(Color, f32)>,
}

thread_local! {
    /// The pane being drawn, and its corners' radius: the Slint build
    /// clips each pane to its rounded rectangle, so a surface inside one
    /// rounds only the corners it shares with it.
    static PANE: std::cell::Cell<Option<(Rectangle, f32)>> = const { std::cell::Cell::new(None) };
}

/// The radii a surface at `bounds` takes inside the pane being drawn, or
/// `own` on every corner outside any.
pub fn pane_radii(bounds: Rectangle, own: f32) -> [f32; 4] {
    let Some((pane, radius)) = PANE.get() else {
        return [own; 4];
    };
    let near = |a: f32, b: f32| (a - b).abs() < 1.5;
    let left = near(bounds.x, pane.x);
    let top = near(bounds.y, pane.y);
    let right = near(bounds.x + bounds.width, pane.x + pane.width);
    let bottom = near(bounds.y + bounds.height, pane.y + pane.height);
    [
        if top && left { radius } else { 0.0 },
        if top && right { radius } else { 0.0 },
        if bottom && right { radius } else { 0.0 },
        if bottom && left { radius } else { 0.0 },
    ]
}

/// Marks its child as one rounded pane for [`pane_radii`] while it draws.
pub struct Pane {
    pub radius: f32,
}

impl<M> super::Hooks<M> for Pane {
    // Not `()`: a tree made for a stateless widget (the same pane before a
    // theme with glass was applied) has `()`'s tag, and applying such a
    // theme panicked downcasting its missing state.
    type State = super::Stateless;

    fn draw(
        &self,
        _state: &super::Stateless,
        renderer: &mut Renderer,
        at: super::At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        let outer = PANE.replace(Some((at.bounds, self.radius)));
        child(renderer);
        PANE.set(outer);
    }
}

/// Draws a glass pane in `bounds`, as the Slint build `ArtLayer` does:
/// the blurred art under it, its ground over that, then the inset, the
/// highlight and the rim.
pub fn draw_glass(renderer: &mut Renderer, pane: Rectangle, art: &BlurredWindow, look: &GlassLook) {
    if pane.width <= 0.0 || pane.height <= 0.0 {
        return;
    }
    let most = pane.width.min(pane.height) / 2.0;
    let radii = look.radii.map(|radius| radius.clamp(0.0, most));
    let radius = border_radius(radii);
    // Only the art under this pane, cut to it and rounded like it: the whole
    // window's art drawn here would cover every pane drawn before this one.
    let opaque = match look.ground {
        iced::Background::Color(colour) => look.opacity * colour.a >= 1.0,
        iced::Background::Gradient(_) => false,
    };
    if !opaque && let Some(handle) = art.under(pane) {
        draw_picture(renderer, &handle, pane, radii, 1.0);
    }
    // Within one layer quads draw before pictures, so the ground and the
    // edges go in a layer of their own, over the art.
    renderer.with_layer(pane, |renderer| {
        renderer.fill_quad(
            renderer::Quad {
                bounds: pane,
                border: iced::Border {
                    radius,
                    ..Default::default()
                },
                ..Default::default()
            },
            look.ground.scale_alpha(look.opacity.clamp(0.0, 1.0)),
        );
        let rim_width = look.rim.map_or(0.0, |(_, width)| width.max(0.0));
        let inner = Rectangle::new(
            pane.position() + Vector::new(rim_width, rim_width),
            Size::new(
                (pane.width - rim_width * 2.0).max(0.0),
                (pane.height - rim_width * 2.0).max(0.0),
            ),
        );
        let inner_radii = radii.map(|radius| (radius - rim_width).max(0.0));
        if let Some((colour, reach)) = look.inset {
            glass_inset(renderer, inner, inner_radii, reach.max(1.0), colour);
        }
        if let Some(highlight) = look.highlight
            && highlight.a > 0.0
        {
            glass_highlight(renderer, inner, inner_radii, highlight);
        }
        if let Some((rim, width)) = look.rim
            && width > 0.0
        {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: pane,
                    border: iced::Border {
                        color: rim,
                        width,
                        radius,
                    },
                    ..Default::default()
                },
                Color::TRANSPARENT,
            );
        }
    });
}

fn border_radius(
    [top_left, top_right, bottom_right, bottom_left]: [f32; 4],
) -> iced::border::Radius {
    iced::border::Radius {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    }
}

/// The four edges' straight runs between their corners, each with the
/// angle its gradient runs inward: top, bottom, left, right.
fn runs(inner: Rectangle, [tl, tr, br, bl]: [f32; 4], size: f32) -> [(Rectangle, f32); 4] {
    let (x, y, w, h) = (inner.x, inner.y, inner.width, inner.height);
    [
        (
            Rectangle::new(
                Point::new(x + tl, y),
                Size::new((w - tl - tr).max(0.0), size),
            ),
            180.0,
        ),
        (
            Rectangle::new(
                Point::new(x + bl, y + h - size),
                Size::new((w - bl - br).max(0.0), size),
            ),
            0.0,
        ),
        (
            Rectangle::new(
                Point::new(x, y + tl),
                Size::new(size, (h - tl - bl).max(0.0)),
            ),
            90.0,
        ),
        (
            Rectangle::new(
                Point::new(x + w - size, y + tr),
                Size::new(size, (h - tr - br).max(0.0)),
            ),
            270.0,
        ),
    ]
}

/// The Slint build's glass inset: along each inside edge, the colour fading to
/// clear over `size` (Slint's four edge gradients, clipped to the corners).
fn glass_inset(
    renderer: &mut Renderer,
    inner: Rectangle,
    radii: [f32; 4],
    size: f32,
    colour: Color,
) {
    let size = size.min(inner.width / 2.0).min(inner.height / 2.0);
    if size <= 0.0 || colour.a <= 0.0 {
        return;
    }
    // Concentric rounded rings, a pixel apart and fading inward, draw the
    // edge gradients and their round corners in one pass. (Runs with the
    // corners clipped each into a layer of their own cost a layer per
    // corner per step: dozens a window, every frame.)
    let steps = size.ceil().max(1.0) as usize;
    let most = inner.width.min(inner.height) / 2.0;
    for step in 0..steps {
        let at = step as f32;
        let share = 1.0 - (at + 0.5) / size;
        if share <= 0.0 {
            break;
        }
        let ring = Rectangle::new(
            inner.position() + Vector::new(at, at),
            Size::new(
                (inner.width - at * 2.0).max(0.0),
                (inner.height - at * 2.0).max(0.0),
            ),
        );
        if ring.width <= 0.0 || ring.height <= 0.0 {
            break;
        }
        let radii = radii.map(|radius| (radius - at).clamp(0.0, most));
        renderer.fill_quad(
            renderer::Quad {
                bounds: ring,
                border: iced::Border {
                    color: Color {
                        a: colour.a * share,
                        ..colour
                    },
                    width: 1.0,
                    radius: border_radius(radii),
                },
                ..Default::default()
            },
            Color::TRANSPARENT,
        );
    }
}

/// The Slint build's glass highlight: a 1 px line inside the edge, lit at the top
/// left and fading to nothing 55% of the way across (Slint's 135° border
/// gradient), with its corners rounded like the pane.
fn glass_highlight(renderer: &mut Renderer, inner: Rectangle, radii: [f32; 4], colour: Color) {
    let span = (inner.width + inner.height) * 0.55;
    // A point's share of the light along the 135° diagonal.
    let lit = |x: f32, y: f32| Color {
        a: colour.a * (1.0 - (x + y) / span).clamp(0.0, 1.0),
        ..colour
    };
    let (w, h) = (inner.width, inner.height);
    let [tl, tr, br, bl] = radii;
    let edges = runs(inner, radii, 1.0);
    let ends = [
        (lit(tl, 0.0), lit(w - tr, 0.0), 90.0),
        (lit(bl, h), lit(w - br, h), 90.0),
        (lit(0.0, tl), lit(0.0, h - bl), 180.0),
        (lit(w, tr), lit(w, h - br), 180.0),
    ];
    for ((bounds, _), (from, to, angle)) in edges.into_iter().zip(ends) {
        if bounds.width > 0.0 && bounds.height > 0.0 && (from.a > 0.0 || to.a > 0.0) {
            gradient_quad(renderer, bounds, 0.0, angle, &[(0.0, from), (1.0, to)]);
        }
    }
    for (corner, radius) in corners(inner, radii).into_iter().zip(radii) {
        if radius <= 0.0 {
            continue;
        }
        let middle = lit(
            corner.x - inner.x + radius / 2.0,
            corner.y - inner.y + radius / 2.0,
        );
        if middle.a > 0.004 {
            corner_ring(renderer, inner, radii, corner, radius, 1.0, middle);
        }
    }
}

/// The top-left points of the squares, each its corner's radius, in
/// `inner`'s corners.
fn corners(inner: Rectangle, [_, tr, br, bl]: [f32; 4]) -> [Point; 4] {
    let right = inner.x + inner.width;
    let bottom = inner.y + inner.height;
    [
        Point::new(inner.x, inner.y),
        Point::new(right - tr, inner.y),
        Point::new(right - br, bottom - br),
        Point::new(inner.x, bottom - bl),
    ]
}

/// A `width` rounded border of `inner`, shown only in the corner square
/// at `corner`, `radius` a side.
fn corner_ring(
    renderer: &mut Renderer,
    inner: Rectangle,
    radii: [f32; 4],
    corner: Point,
    radius: f32,
    width: f32,
    colour: Color,
) {
    renderer.with_layer(
        Rectangle::new(corner, Size::new(radius, radius)),
        |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: inner,
                    border: iced::Border {
                        color: colour,
                        width,
                        radius: border_radius(radii),
                    },
                    ..Default::default()
                },
                Color::TRANSPARENT,
            );
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pane_cuts_only_the_art_under_it_and_keeps_the_cut() {
        // A 100x50 blurred art for a 400x200 window: a quarter the size.
        let art = BlurredWindow::new(100, 50, vec![0; 100 * 50 * 4], (400, 200));
        let pane = Rectangle::new(Point::new(40.0, 20.0), Size::new(200.0, 100.0));
        let first = art.under(pane).expect("a cut");
        let again = art.under(pane).expect("the same cut");
        assert_eq!(
            first.id(),
            again.id(),
            "a pane that stays put reuses its cut"
        );
        // A pane past the art's edge still gets a cut of at least a pixel.
        let outside = Rectangle::new(Point::new(390.0, 190.0), Size::new(50.0, 50.0));
        assert!(art.under(outside).is_some());
    }

    #[test]
    fn a_surface_rounds_only_the_corners_it_shares_with_its_pane() {
        let pane = Rectangle::new(Point::ORIGIN, Size::new(300.0, 200.0));
        assert_eq!(
            pane_radii(pane, 7.0),
            [7.0; 4],
            "outside a pane, its own radius"
        );
        PANE.set(Some((pane, 20.0)));
        // The member list on the pane's right: its right corners only.
        let members = Rectangle::new(Point::new(200.0, 40.0), Size::new(100.0, 160.0));
        assert_eq!(pane_radii(members, 7.0), [0.0, 0.0, 20.0, 0.0]);
        assert_eq!(pane_radii(pane, 7.0), [20.0; 4]);
        PANE.set(None);
    }
}
