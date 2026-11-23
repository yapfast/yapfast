use super::*;

/// How long a plugin waits between two drawings of the interface: ten a
/// second, which is what the plan caps them at.
pub(super) const PAINT_PERIOD: Duration = Duration::from_millis(100);

/// What plugins drew since the interface was last given a frame.
///
/// A plugin can call `ui.set_slot` as often as it likes; what reaches the
/// interface is one drawing per slot (and per decorated message) each frame,
/// and at most ten frames a second. The first drawing of an idle plugin goes
/// straight through, so a notice appears when it's asked for.
#[derive(Default)]
pub(super) struct Painting {
    /// When the interface may be given what's queued; `None` with nothing
    /// queued, so the loop has no timer to wake for.
    pub(super) due: Option<Instant>,
    /// The earliest moment the next frame may go, while a plugin keeps
    /// drawing: what holds it to ten a second.
    pub(super) window: Option<Instant>,
    /// The queued drawings, the last one for a place replacing the one before
    /// it.
    pub(super) drawn: Vec<slots::Output>,
}

/// Whether two drawings go in the same place: one plugin's tree in one slot,
/// or what it draws under one message.
pub(super) fn same_place(one: &slots::Output, other: &slots::Output) -> bool {
    if let (Some(one), Some(other)) = (slot_place(one), slot_place(other)) {
        return one == other;
    }
    match (one, other) {
        (
            slots::Output::Decorate {
                plugin: one,
                message: one_message,
                ..
            },
            slots::Output::Decorate {
                plugin: other,
                message: other_message,
                ..
            },
        ) => one == other && one_message == other_message,
        _ => false,
    }
}

/// The plugin, the slot and, in a profile slot, the person a slot drawing
/// is for; none for a decoration.
fn slot_place(output: &slots::Output) -> Option<(&str, slots::Slot, Option<&str>)> {
    let (plugin, slot, subject) = match output {
        slots::Output::SetSlot { plugin, slot, tree } => (plugin, *slot, tree.subject.as_deref()),
        slots::Output::ClearSlot {
            plugin,
            slot,
            subject,
        } => (plugin, *slot, subject.as_deref()),
        _ => return None,
    };
    Some((plugin, slot, subject.filter(|_| slot.per_person())))
}

impl Painting {
    /// Adds a drawing to those waiting for the interface.
    pub(super) fn queue(&mut self, now: Instant, output: slots::Output) {
        if let Some(queued) = self
            .drawn
            .iter_mut()
            .find(|queued| same_place(queued, &output))
        {
            *queued = output;
        } else {
            self.drawn.push(output);
        }
        // The first drawing after the interface has caught up goes at once;
        // one that arrives inside the frame window waits for its moment.
        if self.due.is_none() {
            self.due = Some(self.window.map_or(now, |window| window.max(now)));
        }
    }

    /// Everything queued, and the moment the next frame may go.
    pub(super) fn take(&mut self, now: Instant) -> Vec<slots::Output> {
        let drawn = std::mem::take(&mut self.drawn);
        self.window = (!drawn.is_empty()).then(|| now + PAINT_PERIOD);
        self.due = None;
        drawn
    }
}
