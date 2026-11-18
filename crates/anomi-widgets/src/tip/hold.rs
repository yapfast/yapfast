//! A card that takes the pointer (`Tip::holding`): it stays while the
//! pointer is on what it names or on the card, and [`GRACE`] after it left
//! both, as Slint's card of who reacted stays (`ReactionHover`'s 200 ms
//! timer, `ui/yapfast.slint:5440`), so the pointer crosses the gap between
//! the chip and the card. One wake at the end of the wait, no ticking.

use std::time::{Duration, Instant};

use iced::advanced::{Shell, mouse};
use iced::{Event, window};

use anomi_theme::Motion;
use anomi_theme::motion::{Pace, Popup};

use super::State;

/// How long the card stays once the pointer left it and what it names.
pub(super) const GRACE: Duration = Duration::from_millis(200);

/// Follows the pointer for a holding card: `over` is the pointer on what it
/// names; whether it's on the card, its overlay noted first.
pub(super) fn update<M>(
    state: &mut State,
    event: &Event,
    over: bool,
    motion: Motion,
    on_leave: &dyn Fn() -> M,
    shell: &mut Shell<'_, M>,
) {
    let held = over || (state.on_card && state.hovered && !state.hidden);
    match event {
        Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
            if held {
                state.left = None;
                if !state.hovered {
                    state.hovered = true;
                    state.hidden = false;
                    state.entry = Popup::entering(Pace::Popover, motion, Instant::now());
                    shell.invalidate_overlay();
                    shell.request_redraw();
                }
            } else if state.hovered && state.left.is_none() {
                let now = Instant::now();
                state.left = Some(now);
                shell.request_redraw_at(now + GRACE);
            }
        }
        // A press on what it names hides it; one on the card is the card's,
        // which acts as it's let go.
        Event::Mouse(mouse::Event::ButtonPressed(_)) if over && !state.hidden => {
            state.hidden = true;
            shell.invalidate_overlay();
            shell.request_redraw();
        }
        Event::Window(window::Event::RedrawRequested(now)) => {
            state.now = *now;
            if state.entry.is_animating(*now) {
                shell.request_redraw();
            }
            match state.left {
                Some(left) if *now >= left + GRACE => {
                    state.left = None;
                    state.hovered = false;
                    state.hidden = false;
                    state.on_card = false;
                    state.entry = Popup::closed(Pace::Popover);
                    shell.invalidate_overlay();
                    shell.publish(on_leave());
                }
                Some(left) => shell.request_redraw_at(left + GRACE),
                None => {}
            }
        }
        _ => {}
    }
}
