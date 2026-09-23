//! Deterministic state machines for Bard's fixed keyboard gestures.
//!
//! Ctrl+Ctrl is not an operating-system shortcut: a shortcut is one modifier
//! set plus one non-modifier key, while this gesture is two physical presses
//! of a modifier. It therefore lives beside the Shift gestures in the raw
//! input coordinator.

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureAction {
    Capture,
    TogglePanel,
    DictationPress { token: u64 },
    DictationRelease { token: u64, held: Duration },
}

impl GestureKey {
    fn action(self) -> GestureAction {
        match self {
            Self::LeftShift => GestureAction::Capture,
            Self::RightShift => GestureAction::TogglePanel,
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
    keys: [KeyState; 4],
    pending: Option<PendingTap>,
    control: Option<ControlGesture>,
    foreign_down: bool,
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
            keys: [KeyState::default(); 4],
            pending: None,
            control: None,
            foreign_down: false,
            next_token: 0,
        }
    }

    /// Forget all physical-key state. Used while the shortcut editor is open
    /// or after a synthetic input burst.
    pub fn reset(&mut self) {
        self.keys = [KeyState::default(); 4];
        self.pending = None;
        self.control = None;
        self.foreign_down = false;
    }

    /// Set whether any non-gesture key or mouse button is currently held.
    /// The adapter owns the physical-key set; this method only needs the
    /// aggregate state to make a first gesture dirty.
    pub fn set_foreign_down(&mut self, down: bool) {
        if down && !self.foreign_down {
            self.invalidate();
        }
        self.foreign_down = down;
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

        if matches!(
            key,
            GestureKey::LeftShift
                | GestureKey::RightShift
                | GestureKey::LeftControl
                | GestureKey::RightControl
        ) {
            if let Some(pending) = self.pending {
                let gap = now
                    .checked_duration_since(pending.released_at)
                    .unwrap_or_default();
                if pending.key == key && gap <= TAP_WINDOW && !self.foreign_down {
                    self.pending = None;
                    if matches!(key, GestureKey::LeftShift | GestureKey::RightShift) {
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

    /// Poll time-based transitions. The raw-input worker calls this on a short
    /// timer even when no keyboard event arrives.
    pub fn poll(&mut self, now: Instant) -> Vec<GestureAction> {
        if self.pending.is_some_and(|pending| {
            now.checked_duration_since(pending.released_at)
                .unwrap_or_default()
                > TAP_WINDOW
        }) {
            self.pending = None;
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
    fn right_shift_double_tap_toggles_panel() {
        let base = Instant::now();
        let mut state = GestureState::new();
        shift_tap(&mut state, GestureKey::RightShift, base, Duration::ZERO);
        assert_eq!(
            shift_tap(
                &mut state,
                GestureKey::RightShift,
                base,
                Duration::from_millis(200)
            ),
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
}
