//! The switcher session: one open overlay, from the shortcut that opened it to the commit or
//! cancel that closes it (data-model.md → Switcher Session).
//!
//! Pure, so every transition in the state diagram is unit-testable without a compositor. The
//! Wayland shell owns *when* these methods are called; it owns none of what they decide.
//!
//! Two properties are load-bearing and easy to lose:
//!
//! - **A session commits at most once.** Hyprland may emit the same `modifiers` state twice in a
//!   row (research.md R4), so every transition out of [`Outcome::Open`] is guarded rather than
//!   assumed to happen once.
//! - **The entries are a snapshot.** The world moves on while the overlay is open, which is why
//!   the committed target is re-checked against the live world at commit time (FR-027).

use crate::model::MonitorName;
use crate::ordering::Entry;
use crate::state::World;

/// A depressed-modifier mask, as `wl_keyboard.modifiers` reports it.
pub type ModMask = u32;

/// Where a session ended up. `Open` until exactly one of the other two is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Open,
    /// The user selected this workspace id.
    Committed(i32),
    /// Escape, a lost connection, or an empty entry list. Dispatches nothing and leaves the
    /// activation history untouched (US1-AS5).
    Cancelled,
}

/// Whether the overlay has keyboard focus yet, which is what distinguishes the ordinary
/// hold-and-release path from the fast tap (research.md R4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusState {
    /// Opened; `wl_keyboard.enter` has not arrived. The overlay may not be painted yet.
    AwaitingFocus,
    /// Focused, `initial_mods` recorded.
    Focused,
    /// The shortcut was released before focus ever arrived — the fast-tap path. The overlay
    /// never maps at all (FR-005).
    NeverFocused,
}

/// One thing an in-overlay key press asks for (FR-004a, contracts/shortcuts.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Next,
    Previous,
    Cancel,
    /// Only reachable in sticky mode; harmless otherwise (research.md R15).
    Commit,
}

/// The transient state of one open overlay. At most one exists at a time (FR-028).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// Snapshot taken when the session opened; never refreshed while open.
    pub entries: Vec<Entry>,
    /// Index into `entries`. Wraps in both directions (FR-003, FR-004).
    pub highlight: usize,
    /// The focused monitor when the session opened.
    pub origin_monitor: MonitorName,
    /// Modifiers depressed at keyboard focus. Empty means sticky mode (research.md R15).
    pub initial_mods: ModMask,
    pub focus_state: FocusState,
    pub outcome: Outcome,
}

impl Session {
    /// Open a session on a snapshot of the world's entries.
    ///
    /// `None` when there is nothing to show: an overlay listing no workspaces would take the
    /// user's keyboard for no reason.
    #[must_use]
    pub fn open(
        entries: Vec<Entry>,
        highlight: usize,
        origin_monitor: MonitorName,
    ) -> Option<Self> {
        if entries.is_empty() {
            return None;
        }
        Some(Self {
            highlight: highlight.min(entries.len() - 1),
            entries,
            origin_monitor,
            initial_mods: 0,
            focus_state: FocusState::AwaitingFocus,
            outcome: Outcome::Open,
        })
    }

    /// Whether the session is still accepting input.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.outcome == Outcome::Open
    }

    /// Whether the overlay should ever be painted. False on the fast-tap path, which commits
    /// without showing anything (FR-005).
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.focus_state != FocusState::NeverFocused
    }

    /// Sticky mode: the shortcut was bound without a modifier, so no release can ever commit and
    /// Enter does it instead (research.md R15).
    ///
    /// Only knowable once focus has arrived, because that is when the held set is observed.
    #[must_use]
    pub fn is_sticky(&self) -> bool {
        self.focus_state == FocusState::Focused && self.initial_mods == 0
    }

    /// The entry the user would commit right now.
    #[must_use]
    pub fn highlighted(&self) -> &Entry {
        // `open` rejects an empty list and `highlight` only ever moves modulo the length.
        &self.entries[self.highlight]
    }

    /// Keyboard focus arrived; record the modifiers held at that instant (research.md R4).
    ///
    /// A second `enter` does not re-record: the first observation is the gesture the user began.
    pub fn focused(&mut self, mods: ModMask) {
        if self.focus_state == FocusState::AwaitingFocus {
            self.initial_mods = mods;
            self.focus_state = FocusState::Focused;
        }
    }

    /// A `modifiers` event arrived. Commits when any modifier held at focus has been released,
    /// which is the whole hold-and-release interaction (FR-002, FR-005).
    pub fn modifiers_changed(&mut self, current: ModMask) {
        if self.is_open()
            && self.focus_state == FocusState::Focused
            && self.initial_mods != 0
            && current & self.initial_mods != self.initial_mods
        {
            self.commit();
        }
    }

    /// The switcher shortcut was released.
    ///
    /// Only meaningful before focus ever arrived: the user tapped and let go faster than the
    /// overlay could map, and the initial highlight is what they meant (FR-005). Once focused,
    /// this event is noise — Hyprland fires bind release on the bind's *key*, not its modifier
    /// (research.md R4), so acting on it would commit on the first Tab of an Alt-Tab-Tab.
    pub fn shortcut_released(&mut self) {
        if self.is_open() && self.focus_state == FocusState::AwaitingFocus {
            self.focus_state = FocusState::NeverFocused;
            self.commit();
        }
    }

    /// Apply an in-overlay key action (FR-004a).
    pub fn apply(&mut self, action: Action) {
        if !self.is_open() {
            return;
        }
        match action {
            Action::Next => self.next(),
            Action::Previous => self.previous(),
            Action::Cancel => self.cancel(),
            // Enter is inert outside sticky mode: with a modifier held, its release is what
            // commits, and honouring Enter as well would commit the wrong entry mid-gesture.
            Action::Commit => {
                if self.is_sticky() {
                    self.commit();
                }
            }
        }
    }

    /// Advance the highlight, wrapping to the first entry (FR-003, FR-004).
    ///
    /// Also what a repeat `switcher` press does, since the compositor consumes the bind's key and
    /// it never reaches the overlay (research.md R5, FR-028).
    pub fn next(&mut self) {
        if self.is_open() {
            self.highlight = (self.highlight + 1) % self.entries.len();
        }
    }

    /// Step back, wrapping to the last entry (FR-004, FR-004a).
    pub fn previous(&mut self) {
        if self.is_open() {
            self.highlight = (self.highlight + self.entries.len() - 1) % self.entries.len();
        }
    }

    /// Close with no workspace change and no history change (FR-006).
    pub fn cancel(&mut self) {
        if self.is_open() {
            self.outcome = Outcome::Cancelled;
        }
    }

    /// The compositor went away; close without committing (FR-026a).
    pub fn connection_lost(&mut self) {
        self.cancel();
    }

    fn commit(&mut self) {
        if self.is_open() {
            self.outcome = Outcome::Committed(self.highlighted().workspace_id);
        }
    }

    /// The workspace to act on, re-checked against the live world (FR-027).
    ///
    /// `None` for a session that did not commit, or whose target has been destroyed since the
    /// snapshot was taken — the entries are a snapshot, so this is a real case rather than a
    /// defensive one. A target that survives on a monitor that has since gone is *not* handled
    /// here: [`crate::actions::plan`] resolves its monitor from the current world and degrades to
    /// plain activation, which is the FR-027 case that must not cancel.
    #[must_use]
    pub fn target(&self, world: &World) -> Option<i32> {
        let Outcome::Committed(id) = self.outcome else {
            return None;
        };
        world.workspace(id).map(|workspace| workspace.id)
    }
}

/// The fixed in-overlay key map (FR-004a, contracts/shortcuts.md).
///
/// Fixed, not configurable, and deliberately total: any key not named here is ignored while the
/// overlay holds keyboard focus. Keysyms are the raw xkb values, so this stays free of the
/// Wayland stack and is testable on its own.
#[must_use]
pub fn action_for(keysym: u32, shift: bool) -> Option<Action> {
    match keysym {
        // xkb hands back `ISO_Left_Tab` for Shift+Tab, but not every layout does, so the shift
        // state is honoured on plain Tab too.
        keysyms::TAB if shift => Some(Action::Previous),
        keysyms::TAB | keysyms::RIGHT | keysyms::DOWN => Some(Action::Next),
        keysyms::ISO_LEFT_TAB | keysyms::LEFT | keysyms::UP => Some(Action::Previous),
        keysyms::ESCAPE => Some(Action::Cancel),
        keysyms::RETURN | keysyms::KP_ENTER => Some(Action::Commit),
        _ => None,
    }
}

/// The raw xkb keysyms the overlay recognises.
pub mod keysyms {
    pub const TAB: u32 = 0xff09;
    pub const ISO_LEFT_TAB: u32 = 0xfe20;
    pub const RETURN: u32 = 0xff0d;
    pub const KP_ENTER: u32 = 0xff8d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Order;
    use crate::model::{Monitor, Workspace};

    /// An arbitrary non-empty mask standing in for "ALT is held".
    const ALT: ModMask = 1 << 3;
    const SHIFT: ModMask = 1 << 0;

    fn entry(id: i32) -> Entry {
        Entry {
            workspace_id: id,
            label: id.to_string(),
            windows: Vec::new(),
            monitor: "eDP-1".to_owned(),
            monitor_position: (0, 0),
            monitor_size: (1920, 1080),
            is_active: id == 1,
        }
    }

    /// A session on three entries, highlight starting on the second, as MRU opens it.
    fn session() -> Session {
        Session::open(vec![entry(1), entry(2), entry(3)], 1, "eDP-1".to_owned())
            .expect("three entries")
    }

    /// A focused session in the ordinary hold-a-modifier case.
    fn held() -> Session {
        let mut session = session();
        session.focused(ALT);
        session
    }

    fn world_with(ids: &[i32]) -> World {
        let mut world = World::default();
        world.rebuild(
            vec![Monitor {
                id: 0,
                name: "eDP-1".to_owned(),
                position: (0, 0),
                size: (1920, 1080),
                scale: 1.0,
                active_workspace: ids.first().copied().unwrap_or(1),
                focused: true,
            }],
            ids.iter()
                .map(|id| Workspace {
                    id: *id,
                    name: id.to_string(),
                    monitor: "eDP-1".to_owned(),
                    window_count: 0,
                })
                .collect(),
            vec![],
        );
        world
    }

    // --- Opening -----------------------------------------------------------

    #[test]
    fn opening_with_no_entries_yields_no_session() {
        assert_eq!(Session::open(Vec::new(), 0, "eDP-1".to_owned()), None);
    }

    #[test]
    fn a_new_session_is_open_awaiting_focus_on_the_given_highlight() {
        let session = session();
        assert_eq!(session.outcome, Outcome::Open);
        assert_eq!(session.focus_state, FocusState::AwaitingFocus);
        assert_eq!(session.highlight, 1);
        assert_eq!(session.highlighted().workspace_id, 2);
        assert!(session.is_visible(), "an ordinary session paints");
    }

    #[test]
    fn an_out_of_range_highlight_clamps_to_the_last_entry() {
        let session = Session::open(vec![entry(1), entry(2)], 9, "eDP-1".to_owned()).unwrap();
        assert_eq!(session.highlight, 1);
    }

    /// The ordering module produces the pair this consumes, so the two agree by construction.
    #[test]
    fn a_session_opens_on_whatever_ordering_chose() {
        let world = world_with(&[1, 2, 3]);
        let (entries, highlight) = crate::ordering::entries(&world, Order::Compositor);
        let session = Session::open(entries, highlight, "eDP-1".to_owned()).unwrap();
        assert_eq!(
            session.highlighted().workspace_id,
            1,
            "the active workspace"
        );
    }

    // --- Focus -------------------------------------------------------------

    #[test]
    fn keyboard_focus_records_the_modifiers_held_at_that_instant() {
        let session = held();
        assert_eq!(session.focus_state, FocusState::Focused);
        assert_eq!(session.initial_mods, ALT);
        assert!(!session.is_sticky());
    }

    #[test]
    fn a_second_enter_does_not_re_record_the_initial_modifiers() {
        let mut session = held();
        session.focused(SHIFT);
        assert_eq!(session.initial_mods, ALT, "the first observation stands");
    }

    #[test]
    fn no_modifier_held_at_focus_is_sticky_mode() {
        // research.md R15: a bare-key bind has no modifier whose release could commit.
        let mut session = session();
        session.focused(0);
        assert!(session.is_sticky());
    }

    #[test]
    fn stickiness_is_unknown_before_focus_arrives() {
        assert!(!session().is_sticky(), "nothing has been observed yet");
    }

    // --- Navigation --------------------------------------------------------

    #[test]
    fn next_advances_and_wraps_to_the_first_entry() {
        let mut session = session();
        session.next();
        assert_eq!(session.highlighted().workspace_id, 3);
        session.next();
        assert_eq!(session.highlighted().workspace_id, 1, "wrapped");
    }

    #[test]
    fn previous_steps_back_and_wraps_to_the_last_entry() {
        let mut session = session();
        session.previous();
        assert_eq!(session.highlighted().workspace_id, 1);
        session.previous();
        assert_eq!(session.highlighted().workspace_id, 3, "wrapped");
    }

    #[test]
    fn tapping_past_the_end_and_reversing_returns_to_where_it_was() {
        // US1-AS8: tap past the last entry, then Shift+Tab.
        let mut session = session();
        let start = session.highlight;
        for _ in 0..7 {
            session.next();
        }
        for _ in 0..7 {
            session.previous();
        }
        assert_eq!(session.highlight, start);
    }

    #[test]
    fn navigation_on_a_single_entry_stays_put() {
        let mut session = Session::open(vec![entry(1)], 0, "eDP-1".to_owned()).unwrap();
        session.next();
        session.previous();
        assert_eq!(session.highlight, 0);
    }

    #[test]
    fn a_closed_session_ignores_navigation() {
        let mut session = session();
        session.cancel();
        session.next();
        assert_eq!(session.highlight, 1, "unchanged after cancelling");
    }

    // --- Commit on release -------------------------------------------------

    #[test]
    fn releasing_a_modifier_held_at_focus_commits_the_highlighted_entry() {
        // FR-002, FR-005: the whole hold-and-release interaction.
        let mut session = held();
        session.next();
        session.modifiers_changed(0);
        assert_eq!(session.outcome, Outcome::Committed(3));
    }

    #[test]
    fn releasing_only_one_of_several_held_modifiers_still_commits() {
        let mut session = session();
        session.focused(ALT | SHIFT);
        session.modifiers_changed(ALT);
        assert_eq!(
            session.outcome,
            Outcome::Committed(2),
            "any modifier from the initial set leaving is the release"
        );
    }

    #[test]
    fn an_unrelated_modifier_arriving_does_not_commit() {
        // The user holding ALT and pressing SHIFT for Shift+Tab must not commit.
        let mut session = held();
        session.modifiers_changed(ALT | SHIFT);
        assert_eq!(session.outcome, Outcome::Open);
    }

    #[test]
    fn the_same_modifier_state_arriving_twice_commits_once() {
        // research.md R4: Hyprland may repeat a modifiers event.
        let mut session = held();
        session.modifiers_changed(0);
        assert_eq!(session.outcome, Outcome::Committed(2));
        session.next();
        session.modifiers_changed(0);
        assert_eq!(
            session.outcome,
            Outcome::Committed(2),
            "the second event changes nothing"
        );
    }

    #[test]
    fn modifiers_before_focus_never_commit() {
        let mut session = session();
        session.modifiers_changed(0);
        assert_eq!(session.outcome, Outcome::Open);
    }

    #[test]
    fn in_sticky_mode_no_modifier_event_can_commit() {
        let mut session = session();
        session.focused(0);
        session.modifiers_changed(0);
        session.modifiers_changed(ALT);
        assert_eq!(session.outcome, Outcome::Open);
    }

    // --- The fast-tap path -------------------------------------------------

    #[test]
    fn a_release_before_focus_commits_the_initial_highlight_without_showing_the_overlay() {
        // FR-005: press and release faster than the overlay can map.
        let mut session = session();
        session.shortcut_released();
        assert_eq!(session.outcome, Outcome::Committed(2), "the initial entry");
        assert_eq!(session.focus_state, FocusState::NeverFocused);
        assert!(!session.is_visible(), "the overlay never appears");
    }

    #[test]
    fn a_release_after_focus_is_ignored() {
        // research.md R4: the bind's release fires on its *key*, 400 ms before the modifier.
        let mut session = held();
        session.next();
        session.shortcut_released();
        assert_eq!(
            session.outcome,
            Outcome::Open,
            "Alt-Tab-Tab must not commit on the first Tab release"
        );
        assert_eq!(session.focus_state, FocusState::Focused);
    }

    #[test]
    fn focus_arriving_after_a_fast_tap_commit_does_not_reopen_anything() {
        let mut session = session();
        session.shortcut_released();
        session.focused(ALT);
        assert_eq!(session.outcome, Outcome::Committed(2));
        assert_eq!(session.focus_state, FocusState::NeverFocused);
    }

    // --- Cancelling --------------------------------------------------------

    #[test]
    fn escape_cancels_and_commits_nothing() {
        let mut session = held();
        session.apply(Action::Cancel);
        assert_eq!(session.outcome, Outcome::Cancelled);
        assert_eq!(session.target(&world_with(&[1, 2, 3])), None);
    }

    #[test]
    fn a_modifier_release_after_escape_does_not_resurrect_the_commit() {
        // US1-AS5: cancelling leaves the state alone even though the user still lets go of ALT.
        let mut session = held();
        session.apply(Action::Cancel);
        session.modifiers_changed(0);
        assert_eq!(session.outcome, Outcome::Cancelled);
    }

    #[test]
    fn losing_the_connection_cancels_without_committing() {
        // FR-026a.
        let mut session = held();
        session.connection_lost();
        assert_eq!(session.outcome, Outcome::Cancelled);
    }

    // --- Enter -------------------------------------------------------------

    #[test]
    fn enter_commits_in_sticky_mode() {
        let mut session = session();
        session.focused(0);
        session.apply(Action::Next);
        session.apply(Action::Commit);
        assert_eq!(session.outcome, Outcome::Committed(3));
    }

    #[test]
    fn enter_is_inert_while_a_modifier_is_held() {
        let mut session = held();
        session.apply(Action::Commit);
        assert_eq!(
            session.outcome,
            Outcome::Open,
            "the modifier release is what commits"
        );
    }

    // --- The vanished target (FR-027) --------------------------------------

    #[test]
    fn a_committed_target_that_still_exists_is_the_workspace_to_act_on() {
        let mut session = held();
        session.modifiers_changed(0);
        assert_eq!(session.target(&world_with(&[1, 2, 3])), Some(2));
    }

    #[test]
    fn a_committed_target_destroyed_since_the_snapshot_resolves_to_nothing() {
        // FR-027: the entries are a snapshot, so the world really does move underneath them.
        let mut session = held();
        session.modifiers_changed(0);
        assert_eq!(session.target(&world_with(&[1, 3])), None);
    }

    #[test]
    fn an_open_session_has_no_target_yet() {
        assert_eq!(held().target(&world_with(&[1, 2, 3])), None);
    }

    // --- The key map (FR-004a) ---------------------------------------------

    #[test]
    fn tab_right_and_down_advance() {
        for key in [keysyms::TAB, keysyms::RIGHT, keysyms::DOWN] {
            assert_eq!(action_for(key, false), Some(Action::Next), "{key:#x}");
        }
    }

    #[test]
    fn shift_tab_left_and_up_step_back() {
        assert_eq!(action_for(keysyms::TAB, true), Some(Action::Previous));
        assert_eq!(
            action_for(keysyms::ISO_LEFT_TAB, false),
            Some(Action::Previous),
            "xkb reports Shift+Tab as ISO_Left_Tab on most layouts"
        );
        for key in [keysyms::LEFT, keysyms::UP] {
            assert_eq!(action_for(key, false), Some(Action::Previous), "{key:#x}");
        }
    }

    #[test]
    fn escape_cancels_and_enter_commits() {
        assert_eq!(action_for(keysyms::ESCAPE, false), Some(Action::Cancel));
        assert_eq!(action_for(keysyms::RETURN, false), Some(Action::Commit));
        assert_eq!(action_for(keysyms::KP_ENTER, false), Some(Action::Commit));
    }

    #[test]
    fn every_other_key_is_ignored() {
        // FR-004a: the map is closed. `a`, `1`, F1, Home, Space.
        for key in [0x0061, 0x0031, 0xffbe, 0xff50, 0x0020] {
            assert_eq!(action_for(key, false), None, "{key:#x}");
            assert_eq!(action_for(key, true), None, "{key:#x} with shift");
        }
    }

    #[test]
    fn holding_shift_does_not_change_the_arrow_keys() {
        // Shift+Alt+Tab is the documented reverse gesture; the arrows keep their meaning.
        assert_eq!(action_for(keysyms::RIGHT, true), Some(Action::Next));
        assert_eq!(action_for(keysyms::UP, true), Some(Action::Previous));
    }
}
