//! Plugins' overlays: a free-floating layer of a plugin's own pictures over
//! the window, as a cat that follows the pointer is drawn.
//!
//! Cheap by construction. The layer is drawn only while a plugin shows one,
//! from the core's `plugin_overlays`; a click passes through it to whatever is
//! under it, except on a sprite its plugin made `clickable`. Its pace is an
//! empty widget's: on each redraw it hands every plugin that asked for frames
//! its next one through the ticker, a send on a channel that publishes no
//! message and so redraws nothing, and asks for the redraw after next at the
//! fastest pace any plugin asked for. A layer whose plugins asked for no
//! frames asks for no redraw at all, and a window that isn't shown draws
//! nothing and so ticks nothing.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, renderer, widget::tree};
use iced::widget::{container, image, mouse_area, pin, stack};
use iced::{Element, Length, Rectangle, Size, Theme, mouse};

use crate::widgets::Renderer;

use crate::app::{App, Message};

/// The overlays over everything else, or nothing while no plugin shows one.
pub fn layer(app: &App) -> Option<Element<'_, Message>> {
    let state = app.client.state();
    let overlays = &state.plugin_overlays;
    if overlays.is_empty() {
        return None;
    }
    let mut layers = stack![];
    let paced: Vec<(String, u8)> = overlays
        .iter()
        .filter(|(_, layer)| layer.fps > 0)
        .map(|(plugin, layer)| (plugin.clone(), layer.fps))
        .collect();
    // A layer at no pace that asked for the pointer is given a frame when it
    // moves, and none while it stays.
    let pointed: Vec<String> = overlays
        .iter()
        .filter(|(_, layer)| layer.pointer)
        .map(|(plugin, _)| plugin.clone())
        .collect();
    if let (false, Some(ticker)) = (
        paced.is_empty() && pointed.is_empty(),
        state.plugin_overlay_ticker.clone(),
    ) {
        layers = layers.push(Pacer {
            paced,
            pointed,
            ticker,
        });
    }
    for (plugin, layer) in overlays {
        for sprite in &layer.sprites {
            let key = crate::plugin_tree::image_key(plugin, &sprite.image);
            let Some(Some(handle)) = app.plugin_kept.images.get(&key) else {
                continue;
            };
            let mut picture = image(handle.clone())
                .filter_method(image::FilterMethod::Nearest)
                .opacity(sprite.opacity.unwrap_or(1.0).clamp(0.0, 1.0));
            if let Some([x, y, width, height]) = sprite.crop {
                picture = picture.crop(Rectangle {
                    x,
                    y,
                    width,
                    height,
                });
            }
            if let Some(width) = sprite.width {
                picture = picture.width(width);
            }
            if let Some(height) = sprite.height {
                picture = picture.height(height);
            }
            let shown: Element<'_, Message> = if sprite.clickable {
                mouse_area(picture)
                    .on_press(Message::Plugin(
                        yapfast_core::plugins::Input::OverlayClick {
                            plugin: plugin.clone(),
                            id: sprite.id.clone(),
                        },
                    ))
                    .interaction(mouse::Interaction::Pointer)
                    .into()
            } else {
                picture.into()
            };
            layers = layers.push(
                pin(shown)
                    .x(sprite.x)
                    .y(sprite.y)
                    .width(Length::Fill)
                    .height(Length::Fill),
            );
        }
    }
    Some(
        container(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

/// The overlays' frame clock: draws nothing and takes no event.
struct Pacer {
    /// Each plugin that asked for frames, and how many a second.
    paced: Vec<(String, u8)>,
    /// Each plugin that asked for a frame when the pointer moves.
    pointed: Vec<String>,
    ticker: yapfast_core::plugins::bridge::OverlayTicker,
}

/// When each plugin is due its next frame.
#[derive(Default)]
struct Due(BTreeMap<String, Instant>);

impl<M> Widget<M, Theme, Renderer> for Pacer {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Due>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Due::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut tree::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn update(
        &mut self,
        tree: &mut tree::Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        if let iced::Event::Mouse(mouse::Event::CursorMoved { .. }) = event {
            // The ticker sends nothing while a frame is unanswered, so a fast
            // pointer costs the plugin its own pace.
            let (at_ms, pointer, size) = Self::now(layout.bounds(), cursor);
            for plugin in &self.pointed {
                self.ticker.frame(plugin, at_ms, pointer, size);
            }
            return;
        }
        let iced::Event::Window(iced::window::Event::RedrawRequested(now)) = event else {
            return;
        };
        let due = tree.state.downcast_mut::<Due>();
        if let Some(next) = self.tick(due, *now, layout.bounds(), cursor) {
            shell.request_redraw_at(next);
        }
    }

    fn draw(
        &self,
        _tree: &tree::Tree,
        _renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
    }
}

impl Pacer {
    /// The time, where the pointer is over the window, and its size.
    fn now(bounds: Rectangle, cursor: mouse::Cursor) -> (u64, Option<(i32, i32)>, (u32, u32)) {
        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as u64);
        let pointer = cursor
            .position_in(bounds)
            .map(|point| (point.x.round() as i32, point.y.round() as i32));
        let size = (bounds.width.round() as u32, bounds.height.round() as u32);
        (at_ms, pointer, size)
    }

    /// Hands each plugin that is due its frame, and says when the next is.
    fn tick(
        &self,
        due: &mut Due,
        now: Instant,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Instant> {
        let (at_ms, pointer, size) = Self::now(bounds, cursor);
        let mut next: Option<Instant> = None;
        due.0
            .retain(|plugin, _| self.paced.iter().any(|(paced, _)| paced == plugin));
        for (plugin, fps) in &self.paced {
            let period = Duration::from_secs(1) / u32::from((*fps).max(1));
            let at = due.0.entry(plugin.clone()).or_insert(now);
            if now >= *at {
                self.ticker.frame(plugin, at_ms, pointer, size);
                // From when it was due, so the pace holds; from now when it
                // fell more than a frame behind, so it doesn't race to catch up.
                *at = if now.duration_since(*at) > period {
                    now + period
                } else {
                    *at + period
                };
            }
            next = Some(next.map_or(*at, |next| next.min(*at)));
        }
        next
    }
}

impl<'a, M: 'a> From<Pacer> for Element<'a, M, Theme, Renderer> {
    fn from(pacer: Pacer) -> Self {
        Element::new(pacer)
    }
}
