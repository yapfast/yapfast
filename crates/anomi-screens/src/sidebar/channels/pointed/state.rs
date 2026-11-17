//! A [`Pointed`](super::Pointed) row's tree state: the pointer, the focus,
//! the card's dismissal, its animations, and the faded theme its body is
//! drawn in.

use std::cell::RefCell;
use std::time::Instant;

use iced::Vector;
use iced::advanced::widget::operation::Focusable;
use iced::animation::{Animation, Easing};

use anomi_theme::motion::{ARRIVE, Pace, Popup, QUICK};
use anomi_theme::{Motion, Palette, Theme};

pub(super) struct State {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    /// A click, a right click, a drag or a scroll hid the card until the
    /// pointer leaves.
    pub dismissed: bool,
    /// The trailing part was laid out.
    pub trailing: bool,
    /// Where the chevron goes, from the row's left: after the body.
    pub chevron_x: f32,
    /// How far the window is from the row's layout, as overlays see it.
    pub translation: Vector,
    pub now: Instant,
    /// How lit the row is under the pointer; Slint's `animate background`
    /// and `animate ink` (`ui/rail.slint:510`, `:518`), linear.
    pub lit: Animation<bool>,
    /// The chevron's quarter turn while folded, on `ARRIVE` (`:875`).
    pub turn: Animation<bool>,
    /// The card growing in, as `PopupEntry` has it.
    pub card: Popup,
    faded: RefCell<Option<(Theme, f32, Theme)>>,
}

impl State {
    pub fn new(folded: bool) -> Self {
        Self {
            hovered: false,
            pressed: false,
            focused: false,
            dismissed: false,
            trailing: false,
            chevron_x: 0.0,
            translation: Vector::ZERO,
            now: Instant::now(),
            lit: Animation::new(false),
            turn: Animation::new(folded),
            card: Popup::closed(Pace::Popover),
            faded: RefCell::new(None),
        }
    }

    /// The pointer came or went: the card enters or goes at once, and a
    /// card hidden by a click shows again on the next visit.
    pub fn point(&mut self, over: bool, motion: Motion, now: Instant) {
        self.hovered = over;
        if !over {
            self.dismissed = false;
            self.pressed = false;
        }
        self.card = if over {
            Popup::entering(Pace::Popover, motion, now)
        } else {
            Popup::closed(Pace::Popover)
        };
        self.advance(now, None, motion);
    }

    /// Moves the animations toward what the state asks for at `now`; true
    /// while one still plays and a frame is due.
    pub fn advance(&mut self, now: Instant, folded: Option<bool>, motion: Motion) -> bool {
        self.now = now;
        self.lit = step(&self.lit, self.hovered, Easing::Linear, motion, now);
        if let Some(folded) = folded {
            self.turn = step(&self.turn, folded, ARRIVE, motion, now);
        }
        if motion.still() {
            self.card.land();
        }
        self.lit.is_animating(now) || self.turn.is_animating(now) || self.card.is_animating(now)
    }

    pub fn lit(&self) -> f32 {
        self.lit.interpolate(0.0f32, 1.0, self.now)
    }

    pub fn turned(&self) -> f32 {
        self.turn.interpolate(0.0f32, 1.0, self.now)
    }

    /// `theme` with every colour at `alpha` of its strength, made once
    /// while both stay the same.
    pub fn faded(&self, theme: &Theme, alpha: f32) -> Theme {
        if alpha >= 1.0 {
            return theme.clone();
        }
        let mut kept = self.faded.borrow_mut();
        if let Some((from, at, faint)) = kept.as_ref()
            && *at == alpha
            && from == theme
        {
            return faint.clone();
        }
        let mut palette = *theme.palette();
        for index in 0..Palette::NAMES.len() {
            if let Some(colour) = palette.colour_mut(index) {
                *colour = colour.scale_alpha(alpha);
            }
        }
        let faint = Theme::new(theme.id(), theme.name(), palette);
        *kept = Some((theme.clone(), alpha, faint.clone()));
        faint
    }
}

/// An animation that lands at once while still, and otherwise eases from
/// where it is to `target` over `QUICK`.
fn step(
    animation: &Animation<bool>,
    target: bool,
    easing: Easing,
    motion: Motion,
    now: Instant,
) -> Animation<bool> {
    if motion.still() {
        Animation::new(target)
    } else if animation.value() != target {
        animation
            .clone()
            .duration(motion.duration(QUICK))
            .easing(easing)
            .go(target, now)
    } else {
        animation.clone()
    }
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}
