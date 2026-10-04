use super::{PasteTarget, editable_and_focused};
use gpui_kit::{App, Entity, Window, component::input::InputState};
use std::ops::Range;

/// Survives replacement of the platform adapter by the guard's paint canvas.
#[derive(Default)]
pub(super) struct ImeRejection {
    pending: Option<PendingCleanup>,
}

struct PendingCleanup {
    target: PasteTarget,
    stamp: (u64, u64),
}

impl ImeRejection {
    pub(super) fn clear(&mut self) {
        self.pending = None;
    }

    /// Arm for a rejected insertion into an existing native composition.
    pub(super) fn record(
        &mut self,
        input: &Entity<InputState>,
        stamp: (u64, u64),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.clear();
        if !editable_and_focused(input, window, cx) {
            return;
        }
        let target = PasteTarget::read(input, window, cx);
        if target
            .marked
            .as_ref()
            .is_some_and(|range| !range.is_empty())
        {
            self.pending = Some(PendingCleanup { target, stamp });
        }
    }

    /// Consume the gate on the next commit callback, whether it matches or not.
    pub(super) fn rejects_cleanup(
        &mut self,
        input: &Entity<InputState>,
        replacement_range: Option<&Range<usize>>,
        text: &str,
        stamp: (u64, u64),
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let Some(pending) = self.pending.take() else {
            return false;
        };
        // Pinned Wayland CommitString inserts immediately; Done without preedit
        // then deletes Some(marked), "", or sends an empty marked preedit. The
        // adapter also passes the current mark here for empty preedit/unmark.
        // Public callbacks expose no transaction serial: an indistinguishable
        // immediate explicit deletion/cancel is suppressed once too. New valid
        // edits and later deletions/cancellations remain native.
        text.is_empty()
            && replacement_range.is_some()
            && replacement_range == pending.target.marked.as_ref()
            && stamp == pending.stamp
            && editable_and_focused(input, window, cx)
            && PasteTarget::read(input, window, cx) == pending.target
    }
}
