//! The scope as a widget: a wrapper as wide as what it holds, which has
//! every event before it (the window's shortcuts) and what it left after it
//! (Tab, Escape, typing anywhere). See the module's documentation for the
//! rules.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::operation::Outcome;
use iced::advanced::widget::{self, Id, Operation, Tree, Widget as _, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::{Event, Length, Rectangle, Size, Vector, keyboard};

use anomi_theme::{Renderer, Theme};

use super::Scope;
use super::reach::{Caret, Reach};
use super::shortcut::{self, Shortcut};
use super::unclaimed::{self, Unclaimed};
use super::walk::{self, Walk};

/// The scope's place in the tree: it keeps no state of its own.
struct Marker;

/// What the scope works on while it handles an event: the content's tree
/// and layout, and what the content's `operate` needs besides.
struct Inside<'t> {
    tree: &'t mut Tree,
    layout: Layout,
    renderer: &'t Renderer,
    viewport: &'t Rectangle,
}

impl<M> Scope<'_, M> {
    /// Runs `operation` over the content.
    fn operate_inside(&mut self, inside: &mut Inside<'_>, operation: &mut dyn Operation) {
        self.content.operate(
            inside.tree,
            inside.layout,
            inside.viewport,
            inside.renderer,
            operation,
        );
    }

    /// Whether `target` may take the focus here, and whether it has it.
    fn find(&mut self, inside: &mut Inside<'_>, target: &Id, caret: Caret) -> Reach {
        let mut reach = Reach::new(target.clone(), caret);
        self.operate_inside(inside, &mut reach);
        reach
    }

    /// Focuses `target` with its caret at `caret`, if it's in reach.
    fn reach(&mut self, inside: &mut Inside<'_>, target: &Id, caret: Caret) -> bool {
        let reach = self.find(inside, target, caret);
        if reach.found {
            self.operate_inside(inside, &mut reach.take());
        }
        reach.found
    }
}

impl<M: Clone> Scope<'_, M> {
    /// The window's shortcuts, before anything else has the event; whether
    /// it took it.
    fn before(&mut self, event: &Event, inside: &mut Inside<'_>, shell: &mut Shell<'_, M>) -> bool {
        let Some(taken) = shortcut::of(event) else {
            return false;
        };
        let message = match taken.shortcut {
            Shortcut::Switcher => &self.switcher,
            Shortcut::Back => &self.back,
            Shortcut::Forward => &self.forward,
            Shortcut::Escape => &self.let_go,
            Shortcut::Search => {
                let Some(search) = self.search.clone() else {
                    return false;
                };
                if !self.reach(inside, &search, Caret::All) {
                    return false;
                }
                shell.request_redraw();
                shell.capture_event();
                return true;
            }
        };
        let Some(message) = message else {
            return false;
        };
        if taken.act {
            shell.publish(message.clone());
        }
        shell.capture_event();
        true
    }

    /// Tab, Escape and typing anywhere, for a key nothing took.
    fn after(
        &mut self,
        event: &Event,
        inside: &mut Inside<'_>,
        cursor: mouse::Cursor,
        shell: &mut Shell<'_, M>,
    ) {
        let Some(unclaimed) = unclaimed::of(event) else {
            return;
        };
        match unclaimed {
            Unclaimed::Escape => {
                if let Some(escape) = &self.escape {
                    shell.publish(escape.clone());
                    shell.capture_event();
                }
            }
            Unclaimed::Tab { back } => {
                let mut walk = Walk::new(back);
                self.operate_inside(inside, &mut walk);
                // With nothing focused, Tab goes where typing goes.
                let typing = (!back && walk.focused.is_none())
                    .then(|| self.type_into.clone())
                    .flatten();
                let typed = typing.is_some_and(|target| self.reach(inside, &target, Caret::End));
                if !typed && let Outcome::Chain(next) = walk.finish() {
                    let Self { content, .. } = self;
                    walk::run(next, &mut |operation| {
                        content.operate(
                            inside.tree,
                            inside.layout,
                            inside.viewport,
                            inside.renderer,
                            operation,
                        );
                    });
                }
                shell.request_redraw();
                shell.capture_event();
            }
            Unclaimed::Text { plain } => {
                let Some(target) = self.type_into.clone() else {
                    return;
                };
                if let Event::Keyboard(keyboard::Event::KeyPressed { physical_key, .. }) = event
                    && self.talking.contains(physical_key)
                {
                    return;
                }
                if !plain
                    && let Some(browse) = self.browse.clone()
                    && self.find(inside, &browse, Caret::Kept).focused
                {
                    return;
                }
                // Out of reach, or focused and had the key already.
                let reach = self.find(inside, &target, Caret::End);
                if !reach.found || reach.focused {
                    return;
                }
                self.operate_inside(inside, &mut reach.take());
                shell.request_redraw();
                // The key goes on to the field now focused.
                self.content.update(
                    inside.tree,
                    event,
                    inside.layout,
                    cursor,
                    inside.renderer,
                    shell,
                    inside.viewport,
                );
            }
        }
    }
}

impl<M> widget::Meta for Scope<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Scope<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Marker>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Marker)
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.content.layout(&mut tree.children[0], renderer, limits);
        tree.size = tree.children[0].size;
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let (layout, tree) = layout.iter(&tree.children).next().expect("the content");
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds(), viewport);
        operation.traverse(&mut |operation| {
            let (layout, tree) = layout
                .iter_mut(&mut tree.children)
                .next()
                .expect("the content");
            self.content
                .operate(tree, layout, viewport, renderer, operation);
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let (layout, tree) = layout
            .iter_mut(&mut tree.children)
            .next()
            .expect("the content");
        let mut inside = Inside {
            tree,
            layout,
            renderer,
            viewport,
        };
        if self.keys_taken && matches!(event, Event::Keyboard(_)) {
            shell.capture_event();
            return;
        }
        if self.before(event, &mut inside, shell) {
            return;
        }
        self.content.update(
            inside.tree,
            event,
            layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
        if !shell.is_event_captured() {
            self.after(event, &mut inside, cursor, shell);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, tree) = layout.iter(&tree.children).next().expect("the content");
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let (layout, tree) = layout
            .iter_mut(&mut tree.children)
            .next()
            .expect("the content");
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
