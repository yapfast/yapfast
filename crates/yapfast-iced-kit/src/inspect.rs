//! The theme inspector: in theme dev mode, the named part under the pointer
//! is outlined and labelled with its name, the states it's in and the theme
//! rule that styled it, and a click copies its name. See docs/themes.md,
//! "Named parts".
//!
//! Widgets that are a named part wrap themselves in [`tag`]; it hands the
//! element back untouched unless the inspector is on ([`set_on`]), so with
//! it off a part costs one thread-local read as the view is built and
//! nothing as it's drawn. The window wraps itself in [`Inspector`], which draws the
//! label over everything and takes the clicks while it's on.

use std::cell::{Cell, RefCell};

use iced::{
    Border, Color, Element, Font, Pixels, Point, Rectangle, Size,
    advanced::{
        Renderer as _, Shell, mouse, renderer,
        text::{self, Renderer as _},
    },
    widget::text::{LineHeight, Wrapping},
};

use crate::{
    parts::{PartLook, States},
    widgets::{At, Hooks, Inner, Stateless, Wrap},
};

type Renderer = iced::Renderer;

thread_local! {
    /// Whether the inspector is on, on the window's thread, which builds
    /// and draws the view.
    static ON: Cell<bool> = const { Cell::new(false) };
}

/// Whether the inspector is on.
pub fn on() -> bool {
    ON.with(Cell::get)
}

/// Turns the inspector on or off; the app builds its view again after.
pub fn set_on(on: bool) {
    ON.with(|cell| cell.set(on));
}

/// The part under the pointer, as the last frame drew it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Under {
    pub name: &'static str,
    pub states: States,
    pub look: Option<PartLook>,
    pub bounds: Rectangle,
}

impl Under {
    /// The states it's in, by name, in the order they're laid: `hover +
    /// selected`, or `rest`.
    pub fn states_words(&self) -> String {
        let names: Vec<&str> = STATE_NAMES
            .iter()
            .filter(|(bit, _)| self.states & bit != 0)
            .map(|(_, name)| *name)
            .collect();
        if names.is_empty() {
            "rest".to_owned()
        } else {
            names.join(" + ")
        }
    }

    /// The theme rule that styled it: `parts.menu.row`, with the states
    /// whose values were laid over it, or that nothing did.
    pub fn rule_words(&self) -> String {
        let Some(look) = self
            .look
            .filter(|look| look.from_base || look.from_states != 0)
        else {
            return "no parts rule: the theme's other layers or the stock look".to_owned();
        };
        let mut rules = Vec::new();
        if look.from_base {
            rules.push(format!("parts.{}", self.name));
        }
        for (bit, state) in STATE_NAMES {
            if look.from_states & bit != 0 {
                rules.push(format!("parts.{}.{state}", self.name));
            }
        }
        rules.join(", ")
    }
}

/// The states by their name in `theme.json`, in the order they're laid.
const STATE_NAMES: [(States, &str); 6] = [
    (crate::parts::UNREAD, "unread"),
    (crate::parts::MUTED, "muted"),
    (crate::parts::HOVER, "hover"),
    (crate::parts::FOCUSED, "focused"),
    (crate::parts::PRESSED, "pressed"),
    (crate::parts::SELECTED, "selected"),
];

thread_local! {
    /// The innermost part under the pointer in the frame being drawn:
    /// each [`tag`] under it writes itself here before its content, so an
    /// inner part's replaces its outer one's.
    static UNDER: RefCell<Option<Under>> = const { RefCell::new(None) };
}

/// The part under the pointer, as the last frame drew it.
pub fn under() -> Option<Under> {
    UNDER.with(|under| *under.borrow())
}

/// `content` as the named part `name` in `states`, for the inspector to
/// find: `content` itself while the inspector is off.
pub fn tag<'a, M: 'a>(
    theme: &crate::theme::Theme,
    name: &'static str,
    states: States,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    if !on() {
        return content.into();
    }
    Wrap::new(
        content,
        Tag {
            name,
            states,
            look: theme.part(name, states),
        },
    )
    .into()
}

/// [`tag`] for a part whose hover comes from the pointer over it (a button,
/// a menu row): under the pointer it's labelled hovered, with that look.
pub fn tag_pointed<'a, M: 'a>(
    theme: &crate::theme::Theme,
    name: &'static str,
    states: States,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    if !on() {
        return content.into();
    }
    Wrap::new(
        content,
        Tag {
            name,
            states: states | crate::parts::HOVER,
            look: theme.part(name, states | crate::parts::HOVER),
        },
    )
    .into()
}

/// [`tag`] with the part's look already looked up.
pub fn tag_with<'a, M: 'a>(
    name: &'static str,
    states: States,
    look: Option<PartLook>,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    if !on() {
        return content.into();
    }
    Wrap::new(content, Tag { name, states, look }).into()
}

/// [`tag_pointed`] for a part whose look under the pointer was looked up
/// already: `hovered`.
pub fn tag_looked<'a, M: 'a>(
    name: &'static str,
    states: States,
    hovered: Option<PartLook>,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    if !on() {
        return content.into();
    }
    Wrap::new(
        content,
        Tag {
            name,
            states: states | crate::parts::HOVER,
            look: hovered,
        },
    )
    .into()
}

struct Tag {
    name: &'static str,
    states: States,
    look: Option<PartLook>,
}

impl<M> Hooks<M> for Tag {
    type State = Stateless;

    fn draw(
        &self,
        _: &Self::State,
        renderer: &mut Renderer,
        at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        if at.cursor.is_over(at.bounds) && at.viewport.intersects(&at.bounds) {
            UNDER.with(|under| {
                *under.borrow_mut() = Some(Under {
                    name: self.name,
                    states: self.states,
                    look: self.look,
                    bounds: at.bounds,
                });
            });
        }
        child(renderer);
    }
}

/// The whole window, with the inspector's outline and label over the part
/// under the pointer while it's on, and a click there sending `copy` with
/// the part's name instead of reaching the part.
pub fn inspector<'a, M: Clone + 'a>(
    content: impl Into<Element<'a, M>>,
    copy: fn(&'static str) -> M,
) -> Element<'a, M> {
    if !on() {
        return content.into();
    }
    Wrap::new(content, Inspector { copy }).into()
}

struct Inspector<M> {
    copy: fn(&'static str) -> M,
}

/// The label's text size and the room round it.
const SIZE: f32 = 12.0;
const LINE: f32 = 16.0;
const PAD: f32 = 6.0;
const INK: Color = Color::WHITE;
const GROUND: Color = Color {
    r: 0.07,
    g: 0.07,
    b: 0.09,
    a: 0.94,
};
const EDGE: Color = Color {
    r: 1.0,
    g: 0.73,
    b: 0.2,
    a: 1.0,
};

impl<M: Clone> Hooks<M> for Inspector<M> {
    type State = Stateless;

    fn update(
        &mut self,
        _: &mut Self::State,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        match event {
            // The label follows the pointer.
            iced::Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                shell.request_redraw();
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if at.cursor.is_over(at.bounds) =>
            {
                if let Some(under) = under() {
                    shell.publish((self.copy)(under.name));
                    shell.capture_event();
                    return;
                }
            }
            // Its release goes nowhere either, so nothing under it acts on
            // half a click.
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if under().is_some() && at.cursor.is_over(at.bounds) =>
            {
                shell.capture_event();
                return;
            }
            _ => {}
        }
        child.update(shell);
    }

    fn draw(
        &self,
        _: &Self::State,
        renderer: &mut Renderer,
        at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        UNDER.with(|under| *under.borrow_mut() = None);
        child(renderer);
        let (Some(under), Some(cursor)) = (under(), at.cursor.position()) else {
            return;
        };
        let lines = [
            under.name.to_owned(),
            format!("state: {}", under.states_words()),
            format!("rule: {}", under.rule_words()),
            "click copies the name".to_owned(),
        ];
        renderer.with_layer(at.viewport, |renderer| {
            // The part's outline.
            renderer.fill_quad(
                renderer::Quad {
                    bounds: under.bounds,
                    border: Border {
                        color: EDGE,
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                },
                Color::TRANSPARENT,
            );
            // The label, below and right of the pointer, kept in the window.
            let width = lines
                .iter()
                .map(|line| line.chars().count() as f32 * SIZE * 0.6)
                .fold(0.0, f32::max)
                + PAD * 2.0;
            let height = LINE * lines.len() as f32 + PAD * 2.0;
            let mut origin = Point::new(cursor.x + 14.0, cursor.y + 18.0);
            let room = at.viewport;
            if origin.x + width > room.x + room.width {
                origin.x = (cursor.x - 14.0 - width).max(room.x);
            }
            if origin.y + height > room.y + room.height {
                origin.y = (cursor.y - 12.0 - height).max(room.y);
            }
            let label = Rectangle::new(origin, Size::new(width, height));
            renderer.fill_quad(
                renderer::Quad {
                    bounds: label,
                    border: Border {
                        color: EDGE,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                },
                GROUND,
            );
            for (index, line) in lines.iter().enumerate() {
                let at = Point::new(origin.x + PAD, origin.y + PAD + LINE * index as f32);
                renderer.fill_text(
                    text::Text {
                        content: line.clone(),
                        bounds: Size::new(width, LINE),
                        size: Pixels(SIZE),
                        line_height: LineHeight::Absolute(Pixels(LINE)),
                        font: if index == 0 {
                            Font {
                                weight: iced::font::Weight::Bold,
                                ..Font::MONOSPACE
                            }
                        } else {
                            Font::MONOSPACE
                        },
                        align_x: text::Alignment::Default,
                        align_y: iced::alignment::Vertical::Top,
                        shaping: text::Shaping::Basic,
                        wrapping: Wrapping::None,
                    },
                    at,
                    if index == 0 { EDGE } else { INK },
                    label,
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_names_the_state_and_the_rule_that_styled_it() {
        let under = Under {
            name: "sidebar.channel-row",
            states: crate::parts::HOVER | crate::parts::SELECTED,
            look: Some(PartLook {
                from_base: true,
                from_states: crate::parts::SELECTED,
                ..PartLook::default()
            }),
            bounds: Rectangle::default(),
        };
        assert_eq!(under.states_words(), "hover + selected");
        assert_eq!(
            under.rule_words(),
            "parts.sidebar.channel-row, parts.sidebar.channel-row.selected"
        );
        let stock = Under {
            look: None,
            states: 0,
            ..under
        };
        assert_eq!(stock.states_words(), "rest");
        assert!(stock.rule_words().starts_with("no parts rule"));
    }

    #[test]
    fn a_tag_is_its_content_while_the_inspector_is_off() {
        set_on(false);
        let theme = crate::theme::Theme::new("inspect-off", true).worked_out();
        let content: Element<'_, ()> = iced::widget::Space::new().into();
        let tagged = tag(&theme, "menu.row", 0, content);
        // No wrapper: the same widget's tree tag as the content's.
        let bare: Element<'_, ()> = iced::widget::Space::new().into();
        assert_eq!(
            tagged.as_widget().tag(),
            bare.as_widget().tag(),
            "a tag adds nothing while the inspector is off"
        );
    }
}
