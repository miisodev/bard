//! Deterministic state machines for Bard's fixed keyboard gestures.
//!
//! Ctrl+Ctrl is not an operating-system shortcut: a shortcut is one modifier
//! set plus one non-modifier key, while this gesture is two physical presses
//! of a modifier. It therefore lives beside the Shift gestures in the raw
//! input coordinator.
//!
//! One convention runs through all of them: Shift captures, Ctrl dictates,
//! Left Alt draws, and Right Alt opens Settings. Alt × 2 behaves like the Shift
//! taps — a discrete action on the second release — and inherits the same clean-
//! sequence rules, so AltGr, which arrives with a synthetic Control press on
//! the layouts that have it, never reads as a double tap.

use std::time::{Duration, Instant};

/// The second tap must begin within this window of the first tap's release.
pub const TAP_WINDOW: Duration = Duration::from_millis(400);
/// A first tap may not be held like a normal modifier.
pub const TAP_HOLD_LIMIT: Duration = Duration::from_millis(500);
/// Once the second Control press is seen, wait briefly for a following key to
/// invalidate the chord before starting dictation.
pub const ACTIVATION_GRACE: Duration = Duration::from_millis(50);
/// A held second Control press uses push-to-talk after this duration.
pub const DICTATION_HOLD_LIMIT: Duration = Duration::from_millis(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureKey {
    LeftShift,
    RightShift,
    LeftControl,
    RightControl,
    LeftAlt,
    RightAlt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureAction {
    Capture,
    TogglePanel,
    ToggleDraw,
    ToggleSettings,
    PasteItem { slot: u8 },
    DictationPress { token: u64 },
    DictationRelease { token: u64, held: Duration },
}

/// Result of consuming the optional number after Right Shift × 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemSlotResult {
    Unchanged,
    Paste(u8),
    TogglePanel,
    Cancelled,
}

impl GestureKey {
    fn action(self) -> GestureAction {
        match self {
            Self::LeftShift => GestureAction::Capture,
            Self::RightShift => GestureAction::TogglePanel,
            Self::LeftAlt => GestureAction::ToggleDraw,
            Self::RightAlt => GestureAction::ToggleSettings,
            Self::LeftControl | Self::RightControl => {
                // This variant is never returned directly. Control uses the
                // ownership token below so a late release cannot stop a take
                // started by another input source.
                unreachable!("Control gestures have a stateful action")
            }
        }
    }

    fn index(self) -> usize {
        match self {
            Self::LeftShift => 0,
            Self::RightShift => 1,
            Self::LeftControl => 2,
            Self::RightControl => 3,
            Self::LeftAlt => 4,
            Self::RightAlt => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct KeyState {
    pressed_at: Option<Instant>,
    dirty: bool,
}

#[derive(Clone, Copy, Debug)]
struct PendingTap {
    key: GestureKey,
    released_at: Instant,
}

#[derive(Clone, Copy, Debug)]
struct ControlGesture {
    key: GestureKey,
    token: u64,
    pressed_at: Instant,
    released_at: Option<Instant>,
    activated: bool,
}

/// Tracks the two Shift gestures and the two Control gestures independently.
///
/// A pending tap is consumed by any intervening key press. Control gets an
/// additional activation grace so a normal Ctrl+Space or Ctrl+Shift+Space is
/// not turned into dictation after its second Control tap.
#[derive(Clone, Debug)]
pub struct GestureState {
    keys: [KeyState; 6],
    pending: Option<PendingTap>,
    /// A Right Shift × 2 sequence is held briefly for 1/2/3 item paste.
    /// If no slot arrives, poll() falls back to the normal panel toggle.
    item_slot: Option<Instant>,
    control: Option<ControlGesture>,
    foreign_down: bool,
    /// Set when `foreign_down` last became true.
    ///
    /// `None` means the clock is not running, because the press that set the flag
    /// was stamped with an explicit instant supplied by the caller. A real
    /// `Instant::now()` here would be a different timeline from every other
    /// transition in this machine, and the tests drive a synthetic clock.
    foreign_since: Option<Instant>,
    next_token: u64,
}

impl Default for GestureState {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureState {
    pub fn new() -> Self {
        Self {
            keys: [KeyState::default(); 6],
            pending: None,
            item_slot: None,
            control: None,
            foreign_down: false,
            foreign_since: None,
            next_token: 0,
        }
    }

    /// Forget all physical-key state. Used after a synthetic input burst.
    pub fn reset(&mut self) {
        self.keys = [KeyState::default(); 6];
        self.pending = None;
        self.item_slot = None;
        self.control = None;
        self.foreign_down = false;
    }

    /// Set whether any non-gesture key or mouse button is currently held.
    /// The adapter owns the physical-key set; this method only needs the
    /// aggregate state to make a first gesture dirty.
    ///
    /// `now` stamps the belief when it starts. Without it a synthetic clock
    /// could never be used to drive this state, and the tests would be asserting
    /// against wall-clock time instead of the timeline the machine runs on.
    #[cfg(test)]
    pub fn set_foreign_down_at(&mut self, down: bool, now: Instant) {
        if down && !self.foreign_down {
            self.invalidate();
            self.foreign_since = Some(now);
        }
        self.foreign_down = down;
    }

    /// Set whether any non-gesture key or mouse button is currently held.
    ///
    /// The adapter owns the physical-key set; this method only needs the
    /// aggregate state to make a first gesture dirty.
    pub fn set_foreign_down(&mut self, down: bool) {
        if down && !self.foreign_down {
            self.invalidate();
            self.foreign_since = Some(Instant::now());
        }
        self.foreign_down = down;
    }

    /// How long a `foreign_down` belief has stood, if one is standing.
    ///
    /// The age is measured against an explicit `now` rather than by calling
    /// `Instant::elapsed()` internally, because every other transition in this
    /// state machine is driven from a timestamp the caller supplies — the tests
    /// advance a synthetic clock, and a real `elapsed()` would ignore that
    /// entirely.
    pub fn foreign_held_for(&self, now: Instant) -> Option<Duration> {
        if !self.foreign_down {
            return None;
        }
        // A belief with no stamp can only come from a path that did not supply a
        // clock. Treating it as brand new means the recovery is driven by the
        // caller, which is the only place that knows the timeline.
        self.foreign_since
            .map(|since| now.saturating_duration_since(since))
    }

    /// Drop a `foreign_down` belief that has lasted longer than a hold should.
    ///
    /// Returns whether it changed, so the adapter can also drop the physical-key
    /// entries that no longer correspond to anything.
    pub fn expire_stale_foreign(&mut self, now: Instant) -> bool {
        let stale = self
            .foreign_held_for(now)
            .is_some_and(|held| held > TAP_HOLD_LIMIT);
        if stale {
            self.foreign_down = false;
        }
        stale
    }

    /// Record a non-gesture key/button press. It invalidates both a held
    /// gesture and a tap waiting for its second press.
    pub fn invalidate(&mut self) {
        for key in &mut self.keys {
            if key.pressed_at.is_some() {
                key.dirty = true;
            }
        }
        self.pending = None;
        self.item_slot = None;
        if self.control.is_some_and(|control| !control.activated) {
            self.control = None;
        }
    }

    /// Record a gesture key press. Auto-repeat is ignored; it must not turn a
    /// single physical press into several taps.
    pub fn press(&mut self, key: GestureKey, now: Instant) -> Vec<GestureAction> {
        let index = key.index();
        if self.keys[index].pressed_at.is_some() {
            return Vec::new();
        }

        // Any gesture key after the Right Shift × 2 prefix ends the optional
        // item-paste sequence; the next gesture starts cleanly.
        self.item_slot = None;

        // Any other held gesture is an intervening key. In particular, holding
        // both Control keys must never count as Ctrl+Ctrl.
        for (other_index, other) in self.keys.iter_mut().enumerate() {
            if other_index != index && other.pressed_at.is_some() {
                other.dirty = true;
            }
        }
        self.pending = self.pending.filter(|pending| pending.key == key);
        self.control = self
            .control
            .filter(|control| control.key == key && !control.activated);

        self.keys[index].pressed_at = Some(now);
        self.keys[index].dirty = self.foreign_down;

        // A second same-side Control press inside the tap window enters the
        // activation grace. It does not invoke Tauri yet; poll() does that
        // after any following foreign key has had a chance to cancel it.
        if matches!(key, GestureKey::LeftControl | GestureKey::RightControl) {
            if let Some(pending) = self.pending {
                let gap = now
                    .checked_duration_since(pending.released_at)
                    .unwrap_or_default();
                if pending.key == key && gap <= TAP_WINDOW && !self.foreign_down {
                    self.pending = None;
                    self.next_token = self.next_token.wrapping_add(1).max(1);
                    self.control = Some(ControlGesture {
                        key,
                        token: self.next_token,
                        pressed_at: now,
                        released_at: None,
                        activated: false,
                    });
                }
            }
        }
        Vec::new()
    }

    /// Record a gesture key release. Shift produces a discrete action; Control
    /// produces a release only after its corresponding press has activated.
    pub fn release(&mut self, key: GestureKey, now: Instant) -> Vec<GestureAction> {
        let index = key.index();
        let Some(pressed_at) = self.keys[index].pressed_at.take() else {
            return Vec::new();
        };
        let was_dirty = self.keys[index].dirty;
        self.keys[index].dirty = false;
        let held_for = now.checked_duration_since(pressed_at).unwrap_or_default();

        if let Some(control) = self.control {
            if control.key == key {
                if was_dirty {
                    self.control = None;
                    return Vec::new();
                }
                if control.activated {
                    self.control = None;
                    return vec![GestureAction::DictationRelease {
                        token: control.token,
                        held: held_for,
                    }];
                }
                if let Some(active) = self.control.as_mut() {
                    active.released_at = Some(now);
                }
                return Vec::new();
            }
        }

        if was_dirty || held_for > TAP_HOLD_LIMIT {
            self.pending = None;
            return Vec::new();
        }

        {
            if let Some(pending) = self.pending {
                let gap = now
                    .checked_duration_since(pending.released_at)
                    .unwrap_or_default();
                if pending.key == key && gap <= TAP_WINDOW && !self.foreign_down {
                    self.pending = None;
                    if key == GestureKey::RightShift {
                        self.item_slot = Some(now);
                        return Vec::new();
                    }
                    if !matches!(key, GestureKey::LeftControl | GestureKey::RightControl) {
                        return vec![key.action()];
                    }
                    self.next_token = self.next_token.wrapping_add(1).max(1);
                    self.control = Some(ControlGesture {
                        key,
                        token: self.next_token,
                        pressed_at: now,
                        released_at: None,
                        activated: false,
                    });
                    return Vec::new();
                }
            }
            self.pending = Some(PendingTap {
                key,
                released_at: now,
            });
        }
        Vec::new()
    }

    /// Consume the optional 1/2/3 suffix after Right Shift × 2. Returning
    /// `Cancelled` means the sequence was consumed by an unsupported number;
    /// the caller should not let that number reach the focused application as
    /// part of a normal shortcut.
    pub fn consume_item_slot(&mut self, slot: u8, now: Instant) -> ItemSlotResult {
        let Some(released_at) = self.item_slot.take() else {
            return ItemSlotResult::Unchanged;
        };
        if now.checked_duration_since(released_at).unwrap_or_default() > TAP_WINDOW {
            return ItemSlotResult::TogglePanel;
        }
        if (1..=3).contains(&slot) {
            ItemSlotResult::Paste(slot)
        } else {
            ItemSlotResult::Cancelled
        }
    }

    /// Poll time-based transitions. The raw-input worker calls this on a short
    /// timer even when no keyboard event arrives.
    pub fn poll(&mut self, now: Instant) -> Vec<GestureAction> {
        // Recover from a lost release before anything else, so the rest of this
        // tick sees a coordinator that can still recognise a gesture.
        self.expire_stale_foreign(now);

        if self.pending.is_some_and(|pending| {
            now.checked_duration_since(pending.released_at)
                .unwrap_or_default()
                > TAP_WINDOW
        }) {
            self.pending = None;
        }
        if self.item_slot.is_some_and(|released_at| {
            now.checked_duration_since(released_at).unwrap_or_default() > TAP_WINDOW
        }) {
            self.item_slot = None;
            return vec![GestureAction::TogglePanel];
        }

        let Some(control) = self.control else {
            return Vec::new();
        };
        if control.activated
            || now
                .checked_duration_since(control.pressed_at)
                .unwrap_or_default()
                < ACTIVATION_GRACE
        {
            return Vec::new();
        }

        let mut actions = vec![GestureAction::DictationPress {
            token: control.token,
        }];
        let mut next = control;
        next.activated = true;
        if next.released_at.is_some() {
            actions.push(GestureAction::DictationRelease {
                token: next.token,
                held: next
                    .released_at
                    .and_then(|released_at| released_at.checked_duration_since(next.pressed_at))
                    .unwrap_or_default(),
            });
            self.control = None;
        } else {
            self.control = Some(next);
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(state: &mut GestureState, key: GestureKey, at: Instant) -> Vec<GestureAction> {
        state.press(key, at)
    }

    fn release(state: &mut GestureState, key: GestureKey, at: Instant) -> Vec<GestureAction> {
        state.release(key, at)
    }

    fn shift_tap(
        state: &mut GestureState,
        key: GestureKey,
        base: Instant,
        offset: Duration,
    ) -> Vec<GestureAction> {
        let at = base + offset;
        press(state, key, at);
        release(state, key, at + Duration::from_millis(80))
    }

    #[test]
    fn alt_double_tap_opens_the_matching_secondary_window() {
        let base = Instant::now();
        let mut draw = GestureState::new();
        assert!(shift_tap(&mut draw, GestureKey::LeftAlt, base, Duration::ZERO).is_empty());
        assert_eq!(
            shift_tap(
                &mut draw,
                GestureKey::LeftAlt,
                base,
                Duration::from_millis(250)
            ),
            vec![GestureAction::ToggleDraw]
        );

        let mut settings = GestureState::new();
        assert!(shift_tap(&mut settings, GestureKey::RightAlt, base, Duration::ZERO).is_empty());
        assert_eq!(
            shift_tap(
                &mut settings,
                GestureKey::RightAlt,
                base,
                Duration::from_millis(250)
            ),
            vec![GestureAction::ToggleSettings]
        );
    }

    #[test]
    fn altgr_with_its_synthetic_control_is_not_a_tap() {
        let mut state = GestureState::new();
        let base = Instant::now();
        for offset in [0u64, 250] {
            let at = base + Duration::from_millis(offset);
            press(&mut state, GestureKey::LeftControl, at);
            press(&mut state, GestureKey::RightAlt, at);
            release(
                &mut state,
                GestureKey::RightAlt,
                at + Duration::from_millis(80),
            );
            let actions = release(
                &mut state,
                GestureKey::LeftControl,
                at + Duration::from_millis(80),
            );
            assert!(actions.is_empty());
        }
        assert!(state.poll(base + Duration::from_secs(1)).is_empty());
    }

    #[test]
    fn left_then_right_alt_is_not_a_gesture() {
        let mut state = GestureState::new();
        let base = Instant::now();
        shift_tap(&mut state, GestureKey::LeftAlt, base, Duration::ZERO);
        assert!(shift_tap(
            &mut state,
            GestureKey::RightAlt,
            base,
            Duration::from_millis(200)
        )
        .is_empty());
    }

    fn control_press(
        state: &mut GestureState,
        key: GestureKey,
        base: Instant,
        second_at: Instant,
    ) -> Vec<GestureAction> {
        press(state, key, base);
        release(state, key, base + Duration::from_millis(80));
        press(state, key, second_at)
    }

    #[test]
    fn left_shift_double_tap_captures() {
        let base = Instant::now();
        let mut state = GestureState::new();
        assert!(shift_tap(&mut state, GestureKey::LeftShift, base, Duration::ZERO).is_empty());
        assert_eq!(
            shift_tap(
                &mut state,
                GestureKey::LeftShift,
                base,
                Duration::from_millis(180)
            ),
            vec![GestureAction::Capture]
        );
    }

    #[test]
    fn right_shift_double_tap_toggles_panel_after_slot_window() {
        let base = Instant::now();
        let mut state = GestureState::new();
        shift_tap(&mut state, GestureKey::RightShift, base, Duration::ZERO);
        assert!(shift_tap(
            &mut state,
            GestureKey::RightShift,
            base,
            Duration::from_millis(200)
        )
        .is_empty());
        assert_eq!(
            state.poll(base + Duration::from_millis(700)),
            vec![GestureAction::TogglePanel]
        );
    }

    #[test]
    fn right_shift_slot_pastes_without_opening_the_panel() {
        let base = Instant::now();
        let mut state = GestureState::new();
        assert!(shift_tap(&mut state, GestureKey::RightShift, base, Duration::ZERO).is_empty());
        assert!(shift_tap(
            &mut state,
            GestureKey::RightShift,
            base,
            Duration::from_millis(200)
        )
        .is_empty());
        assert_eq!(
            state.consume_item_slot(1, base + Duration::from_millis(300)),
            ItemSlotResult::Paste(1)
        );
        assert!(state.poll(base + Duration::from_millis(500)).is_empty());
    }

    #[test]
    fn right_shift_without_a_slot_still_toggles_the_panel() {
        let base = Instant::now();
        let mut state = GestureState::new();
        shift_tap(&mut state, GestureKey::RightShift, base, Duration::ZERO);
        shift_tap(
            &mut state,
            GestureKey::RightShift,
            base,
            Duration::from_millis(200),
        );
        assert_eq!(
            state.poll(base + Duration::from_millis(700)),
            vec![GestureAction::TogglePanel]
        );
    }

    #[test]
    fn control_press_waits_for_activation_grace() {
        let base = Instant::now();
        let mut state = GestureState::new();
        assert!(control_press(
            &mut state,
            GestureKey::LeftControl,
            base,
            base + Duration::from_millis(180)
        )
        .is_empty());
        assert_eq!(
            state.poll(base + Duration::from_millis(230)),
            vec![GestureAction::DictationPress { token: 1 }]
        );
        assert_eq!(
            release(
                &mut state,
                GestureKey::LeftControl,
                base + Duration::from_millis(400)
            ),
            vec![GestureAction::DictationRelease {
                token: 1,
                held: Duration::from_millis(220),
            }]
        );
    }

    #[test]
    fn a_short_second_release_presses_then_releases_after_grace() {
        let base = Instant::now();
        let mut state = GestureState::new();
        control_press(
            &mut state,
            GestureKey::RightControl,
            base,
            base + Duration::from_millis(180),
        );
        release(
            &mut state,
            GestureKey::RightControl,
            base + Duration::from_millis(200),
        );
        assert_eq!(
            state.poll(base + Duration::from_millis(231)),
            vec![
                GestureAction::DictationPress { token: 1 },
                GestureAction::DictationRelease {
                    token: 1,
                    held: Duration::from_millis(20),
                },
            ]
        );
    }

    #[test]
    fn an_intervening_key_cancels_control_before_activation() {
        let base = Instant::now();
        let mut state = GestureState::new();
        control_press(
            &mut state,
            GestureKey::LeftControl,
            base,
            base + Duration::from_millis(180),
        );
        state.invalidate();
        assert!(state.poll(base + Duration::from_millis(260)).is_empty());
    }

    #[test]
    fn a_held_shift_does_not_trigger() {
        let base = Instant::now();
        let mut state = GestureState::new();
        press(&mut state, GestureKey::LeftShift, base);
        assert!(release(
            &mut state,
            GestureKey::LeftShift,
            base + Duration::from_millis(501)
        )
        .is_empty());
        assert!(shift_tap(
            &mut state,
            GestureKey::LeftShift,
            base,
            Duration::from_millis(600)
        )
        .is_empty());
    }

    #[test]
    fn auto_repeat_is_one_press() {
        let base = Instant::now();
        let mut state = GestureState::new();
        press(&mut state, GestureKey::LeftControl, base);
        press(
            &mut state,
            GestureKey::LeftControl,
            base + Duration::from_millis(20),
        );
        assert!(release(
            &mut state,
            GestureKey::LeftControl,
            base + Duration::from_millis(80)
        )
        .is_empty());
    }

    #[test]
    fn left_and_right_control_do_not_cross_trigger() {
        let base = Instant::now();
        let mut state = GestureState::new();
        shift_tap(&mut state, GestureKey::LeftControl, base, Duration::ZERO);
        assert!(shift_tap(
            &mut state,
            GestureKey::RightControl,
            base,
            Duration::from_millis(180)
        )
        .is_empty());
    }

    #[test]
    fn a_late_second_press_starts_a_new_sequence() {
        let base = Instant::now();
        let mut state = GestureState::new();
        shift_tap(&mut state, GestureKey::LeftControl, base, Duration::ZERO);
        assert!(shift_tap(
            &mut state,
            GestureKey::LeftControl,
            base,
            Duration::from_millis(500)
        )
        .is_empty());
        assert_eq!(state.poll(base + Duration::from_millis(600)), Vec::new());
    }

    #[test]
    fn a_foreign_key_held_before_the_tap_keeps_it_dirty() {
        let base = Instant::now();
        let mut state = GestureState::new();
        state.set_foreign_down(true);
        press(&mut state, GestureKey::LeftControl, base);
        assert!(release(
            &mut state,
            GestureKey::LeftControl,
            base + Duration::from_millis(80)
        )
        .is_empty());
    }

    #[test]
    fn a_foreign_key_within_the_hold_limit_keeps_the_coordinator_dirty() {
        // The positive half of the recovery below: a genuinely held key must
        // still disarm the gesture, or the fix would be worse than the bug.
        let base = Instant::now() + Duration::from_secs(1);
        let mut state = GestureState::new();
        state.set_foreign_down_at(true, base);
        assert!(!state.expire_stale_foreign(base + Duration::from_millis(200)));
        press(&mut state, GestureKey::LeftShift, base);
        assert!(release(
            &mut state,
            GestureKey::LeftShift,
            base + Duration::from_millis(80)
        )
        .is_empty());
    }

    #[test]
    fn a_foreign_key_never_released_stops_disarming_every_gesture() {
        // The defect this covers: a release lost to a re-armed hook or a dropped
        // event left `foreign_down` true forever, and every later gesture key
        // press was then "dirty" — so no shortcut worked for the rest of the
        // session, with nothing in any log. A left Shift double-tap must recover.
        let mut state = GestureState::new();
        let base = Instant::now() + Duration::from_secs(1);
        state.set_foreign_down_at(true, base);

        // Nothing happens while the belief is fresh.
        assert!(release(
            &mut state,
            GestureKey::LeftShift,
            base + Duration::from_millis(80)
        )
        .is_empty());
        assert!(!state.expire_stale_foreign(base + Duration::from_millis(200)));

        // Once it has been held implausibly long, the belief is dropped.
        let after = base + TAP_HOLD_LIMIT + Duration::from_millis(1);
        assert!(state.expire_stale_foreign(after));
        assert!(!state.foreign_down);

        // And the gesture works again. Both taps, because the first one is only
        // the pending half of a double-tap.
        let start = after + Duration::from_millis(10);
        assert!(
            shift_tap(&mut state, GestureKey::LeftShift, start, Duration::ZERO).is_empty(),
            "the first tap arms"
        );
        assert_eq!(
            shift_tap(
                &mut state,
                GestureKey::LeftShift,
                start,
                Duration::from_millis(250)
            ),
            vec![GestureAction::Capture],
            "the second tap captures"
        );
    }

    #[test]
    fn poll_recovers_from_a_stale_foreign_belief() {
        // The recovery has to happen on the poll path, because that is the only
        // thing that runs when no keyboard event arrives.
        let base = Instant::now() + Duration::from_secs(1);
        let mut state = GestureState::new();
        state.set_foreign_down_at(true, base);
        let recovered = base + TAP_HOLD_LIMIT + Duration::from_millis(1);
        state.poll(recovered);
        assert!(!state.foreign_down);

        let start = recovered + Duration::from_millis(10);
        assert!(shift_tap(&mut state, GestureKey::LeftShift, start, Duration::ZERO).is_empty());
        assert_eq!(
            shift_tap(
                &mut state,
                GestureKey::LeftShift,
                start,
                Duration::from_millis(250)
            ),
            vec![GestureAction::Capture]
        );
    }

    #[test]
    fn reset_clears_the_foreign_belief() {
        let mut state = GestureState::new();
        state.set_foreign_down(true);
        state.reset();
        assert!(!state.foreign_down);
        assert!(state.foreign_held_for(Instant::now()).is_none());
    }
}
