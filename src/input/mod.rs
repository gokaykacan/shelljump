//! Input as the simulation sees it. This module is terminal-free; the crossterm
//! event pump lives in [`terminal`] and only calls into [`InputCollector`].

pub mod terminal;

/// Logical game actions. Physical keys are mapped at the terminal boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameKey {
    Left,
    Right,
    Jump,
    Run,
}

const KEY_COUNT: usize = 4;

/// Every key the collector tracks held state for.
const ALL_KEYS: [GameKey; KEY_COUNT] = [GameKey::Left, GameKey::Right, GameKey::Jump, GameKey::Run];

/// How key-release information reaches us.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldMode {
    /// The terminal reports real press/repeat/release events (Kitty keyboard
    /// protocol), so held state is authoritative.
    Explicit,
    /// The terminal only ever reports presses and auto-repeats, so a key counts
    /// as held until its repeats stop arriving.
    Timeout,
}

/// How long a key may go without an event of its own before [`HoldMode::Timeout`]
/// infers that it was released.
///
/// The binding constraint is not the auto-repeat interval but repeat *ownership*:
/// the terminal delivers a single repeat stream, so pressing a second key moves
/// repeats to it and starves the first key's refresh for as long as the second
/// key stays down. A plain per-key window is therefore never long enough on its
/// own — holding Jump through a jump starves the direction key for the whole
/// flight.
///
/// So every key carries a durable absolute deadline instead. Its own event buys
/// one window; being masked by another key's press pushes its deadline out to
/// *twice* the window from that press, so a starved key always retains a full
/// window measured from the point where the masking key's own deadline lapses.
/// That gives the terminal a fair chance to resume the masked key's repeats
/// before the key is written off, without tying the two keys' fates together
/// the way a single global silence timer does.
pub const HOLD_TIMEOUT: f64 = 0.25;

/// One frame of input, consumed by the simulation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputState {
    pub move_left: bool,
    pub move_right: bool,
    /// Level state, not an edge: raises the horizontal speed cap while down.
    pub run_held: bool,
    pub jump_held: bool,
    /// Latched rising edge; survives frames in which no fixed step ran.
    pub jump_pressed: bool,
    /// Latched falling edge; drives the variable-height jump cut.
    pub jump_released: bool,
    pub quit_requested: bool,
    /// Latest terminal size seen this frame, in (columns, rows).
    pub resized: Option<(u16, u16)>,
}

impl InputState {
    /// Clears one-shot edges so repeated fixed steps within a single frame do
    /// not act on the same press or release more than once.
    pub fn consume_edges(&mut self) {
        self.jump_pressed = false;
        self.jump_released = false;
    }
}

/// Folds a stream of key activity into per-frame [`InputState`] values.
#[derive(Debug)]
pub struct InputCollector {
    mode: HoldMode,
    held: [bool; KEY_COUNT],
    /// Per-key recency, used only to break direction conflicts.
    last_seen: [f64; KEY_COUNT],
    /// Absolute per-key deadline past which a hold is inferred to have ended.
    /// Consulted only in [`HoldMode::Timeout`]; see [`HOLD_TIMEOUT`].
    expiry_at: [f64; KEY_COUNT],
    jump_pressed_latch: bool,
    jump_released_latch: bool,
    quit: bool,
    resized: Option<(u16, u16)>,
}

impl InputCollector {
    pub fn new(mode: HoldMode) -> Self {
        Self {
            mode,
            held: [false; KEY_COUNT],
            last_seen: [f64::NEG_INFINITY; KEY_COUNT],
            expiry_at: [f64::NEG_INFINITY; KEY_COUNT],
            jump_pressed_latch: false,
            jump_released_latch: false,
            quit: false,
            resized: None,
        }
    }

    fn set_held(&mut self, key: GameKey, down: bool) {
        let index = key as usize;
        if self.held[index] == down {
            return;
        }
        self.held[index] = down;
        if key == GameKey::Jump {
            if down {
                self.jump_pressed_latch = true;
            } else {
                self.jump_released_latch = true;
            }
        }
    }

    /// Records a press or auto-repeat at `now` seconds since application start.
    pub fn on_press(&mut self, key: GameKey, now: f64) {
        self.last_seen[key as usize] = now;
        // Every other held key just lost the repeat stream to this one, so grant
        // it a full window beyond where this key's own deadline lands. `max`
        // keeps same-timestamp or out-of-order events from walking a deadline
        // backwards.
        for other in ALL_KEYS {
            let index = other as usize;
            if other != key && self.held[index] {
                self.expiry_at[index] = self.expiry_at[index].max(now + 2.0 * HOLD_TIMEOUT);
            }
        }
        self.expiry_at[key as usize] = self.expiry_at[key as usize].max(now + HOLD_TIMEOUT);
        self.set_held(key, true);
    }

    /// Records a real release. Ignored in [`HoldMode::Timeout`], where releases
    /// are inferred instead and a stray release would cut movement short.
    pub fn on_release(&mut self, key: GameKey) {
        if self.mode == HoldMode::Explicit {
            self.set_held(key, false);
        }
    }

    pub fn request_quit(&mut self) {
        self.quit = true;
    }

    pub fn on_resize(&mut self, columns: u16, rows: u16) {
        // Drag-resizes arrive as a burst; only the final size matters.
        self.resized = Some((columns, rows));
    }

    /// Drops each inferred hold once its own deadline has elapsed. Deadlines are
    /// per-key and durable rather than a shared silence timer: see
    /// [`HOLD_TIMEOUT`].
    fn expire_stale_holds(&mut self, now: f64) {
        if self.mode != HoldMode::Timeout {
            return;
        }
        for key in ALL_KEYS {
            let index = key as usize;
            if self.held[index] && now >= self.expiry_at[index] {
                // Deliberately bypasses `set_held`: an inferred expiry is a
                // guess, not an observed release, and must never latch the
                // jump cut.
                self.held[index] = false;
            }
        }
    }

    /// Both directions reading as held would otherwise cancel to a dead stop:
    /// in [`HoldMode::Timeout`] because a tap lingers for [`HOLD_TIMEOUT`], and
    /// in [`HoldMode::Explicit`] because the player really is holding both. The
    /// most recently seen key wins in either case.
    fn resolve_move_keys(&self) -> (bool, bool) {
        let left = self.held[GameKey::Left as usize];
        let right = self.held[GameKey::Right as usize];
        if left && right {
            let left_seen = self.last_seen[GameKey::Left as usize];
            let right_seen = self.last_seen[GameKey::Right as usize];
            if left_seen >= right_seen {
                (true, false)
            } else {
                (false, true)
            }
        } else {
            (left, right)
        }
    }

    /// Produces this frame's input. Edge latches persist until
    /// [`InputCollector::acknowledge_edges`] is called.
    pub fn finish_frame(&mut self, now: f64) -> InputState {
        self.expire_stale_holds(now);
        let (move_left, move_right) = self.resolve_move_keys();
        InputState {
            move_left,
            move_right,
            run_held: self.held[GameKey::Run as usize],
            jump_held: self.held[GameKey::Jump as usize],
            jump_pressed: self.jump_pressed_latch,
            jump_released: self.jump_released_latch,
            quit_requested: self.quit,
            resized: self.resized.take(),
        }
    }

    /// Clears the edge latches. Call only once the simulation has actually run
    /// a step on them, otherwise a press during a step-less frame is lost.
    pub fn acknowledge_edges(&mut self) {
        self.jump_pressed_latch = false;
        self.jump_released_latch = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_mode_tracks_real_press_and_release() {
        let mut collector = InputCollector::new(HoldMode::Explicit);
        collector.on_press(GameKey::Right, 0.0);
        let state = collector.finish_frame(0.0);
        assert!(state.move_right);

        collector.acknowledge_edges();
        collector.on_release(GameKey::Right);
        let state = collector.finish_frame(0.016);
        assert!(!state.move_right);
    }

    #[test]
    fn explicit_mode_keeps_holding_without_repeat_events() {
        let mut collector = InputCollector::new(HoldMode::Explicit);
        collector.on_press(GameKey::Right, 0.0);
        // Far beyond HOLD_TIMEOUT: no release arrived, so the key is still down.
        let state = collector.finish_frame(10.0);
        assert!(state.move_right);
    }

    #[test]
    fn timeout_mode_infers_release_when_repeats_stop() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        // Press at t=0 so the elapsed comparison lands exactly on the boundary.
        collector.on_press(GameKey::Left, 0.0);
        assert!(collector.finish_frame(0.0).move_left);
        assert!(collector.finish_frame(HOLD_TIMEOUT - 0.01).move_left);
        assert!(!collector.finish_frame(HOLD_TIMEOUT).move_left);
    }

    #[test]
    fn run_is_level_state_with_no_edge_latch() {
        let mut collector = InputCollector::new(HoldMode::Explicit);
        collector.on_press(GameKey::Run, 0.0);
        let state = collector.finish_frame(0.0);
        assert!(state.run_held);
        assert!(!state.jump_pressed, "run must not latch a jump edge");

        collector.on_release(GameKey::Run);
        assert!(!collector.finish_frame(0.016).run_held);
        assert!(
            !collector.finish_frame(0.016).jump_released,
            "run must not latch a jump edge"
        );
    }

    #[test]
    fn timeout_mode_infers_a_run_release_when_repeats_stop() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Run, 0.0);
        assert!(collector.finish_frame(HOLD_TIMEOUT - 0.01).run_held);
        assert!(!collector.finish_frame(HOLD_TIMEOUT).run_held);
    }

    #[test]
    fn timeout_mode_ignores_spurious_release_events() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Right, 0.0);
        collector.on_release(GameKey::Right);
        assert!(collector.finish_frame(0.0).move_right);
    }

    #[test]
    fn a_tap_and_release_inside_one_frame_still_reports_a_jump() {
        let mut collector = InputCollector::new(HoldMode::Explicit);
        collector.on_press(GameKey::Jump, 0.0);
        collector.on_release(GameKey::Jump);
        let state = collector.finish_frame(0.0);
        assert!(!state.jump_held);
        assert!(state.jump_pressed, "the press must not be swallowed");
        assert!(state.jump_released);
    }

    #[test]
    fn a_burst_of_same_frame_events_folds_to_the_final_state() {
        let mut collector = InputCollector::new(HoldMode::Explicit);
        collector.on_press(GameKey::Jump, 0.0);
        collector.on_release(GameKey::Jump);
        collector.on_press(GameKey::Jump, 0.0);
        collector.on_press(GameKey::Left, 0.0);
        collector.on_press(GameKey::Right, 0.0);
        collector.on_release(GameKey::Left);
        collector.on_resize(10, 10);
        collector.on_resize(120, 40);

        let state = collector.finish_frame(0.0);
        assert!(state.jump_held);
        assert!(state.jump_pressed);
        assert!(state.jump_released);
        assert!(!state.move_left);
        assert!(state.move_right);
        assert_eq!(
            state.resized,
            Some((120, 40)),
            "resizes coalesce to the last"
        );
    }

    #[test]
    fn edges_persist_until_acknowledged() {
        let mut collector = InputCollector::new(HoldMode::Explicit);
        collector.on_press(GameKey::Jump, 0.0);
        assert!(collector.finish_frame(0.0).jump_pressed);
        assert!(
            collector.finish_frame(0.001).jump_pressed,
            "a frame that ran no fixed step must not drop the press"
        );
        collector.acknowledge_edges();
        assert!(!collector.finish_frame(0.002).jump_pressed);
    }

    #[test]
    fn resize_is_reported_once() {
        let mut collector = InputCollector::new(HoldMode::Explicit);
        collector.on_resize(80, 24);
        assert_eq!(collector.finish_frame(0.0).resized, Some((80, 24)));
        assert_eq!(collector.finish_frame(0.0).resized, None);
    }

    #[test]
    fn quit_is_sticky() {
        let mut collector = InputCollector::new(HoldMode::Explicit);
        collector.request_quit();
        assert!(collector.finish_frame(0.0).quit_requested);
        assert!(collector.finish_frame(1.0).quit_requested);
    }

    #[test]
    fn opposing_direction_taps_within_timeout_resolve_to_the_newer_key() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Left, 0.0);
        assert!(collector.finish_frame(0.0).move_left);

        collector.on_press(GameKey::Right, 0.02);
        let state = collector.finish_frame(0.02);
        assert!(state.move_right, "the newer key must win immediately");
        assert!(
            !state.move_left,
            "the stale key must not fight the newer one"
        );
    }

    #[test]
    fn opposing_direction_taps_resolve_to_the_newer_key_in_either_order() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Right, 0.0);
        assert!(collector.finish_frame(0.0).move_right);

        collector.on_press(GameKey::Left, 0.02);
        let state = collector.finish_frame(0.02);
        assert!(state.move_left);
        assert!(!state.move_right);
    }

    #[test]
    fn direction_conflict_resolution_applies_in_explicit_mode_too() {
        // Previously asserted the opposite: Explicit mode passed both directions
        // through and let the player controller cancel them to a dead stop. That
        // is the same freeze already fixed for Timeout mode, so the recency
        // tie-break now runs in both modes.
        let mut collector = InputCollector::new(HoldMode::Explicit);
        collector.on_press(GameKey::Left, 0.0);
        collector.on_press(GameKey::Right, 0.02);
        let state = collector.finish_frame(0.02);
        assert!(state.move_right, "the newer key must win immediately");
        assert!(
            !state.move_left,
            "a genuine simultaneous hold must not cancel to a dead stop"
        );
    }

    #[test]
    fn jump_activity_does_not_shorten_a_held_direction() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Left, 0.0);
        // The terminal moves its single auto-repeat stream to Jump, so Left gets
        // no further events even though it is still physically down.
        for now in [0.05, 0.10, 0.20] {
            collector.on_press(GameKey::Jump, now);
            assert!(collector.finish_frame(now).move_left);
        }
        assert!(
            collector.finish_frame(0.24).move_left,
            "the direction must survive an OS initial repeat delay"
        );
        // Left's deadline was pushed to 2 x HOLD_TIMEOUT past the Jump repeat at
        // t=0.20 that masked it, so it outlives Jump's own deadline at t=0.45
        // rather than expiring in lockstep with it (that lockstep was Bug A).
        assert!(
            collector
                .finish_frame(0.20 + 2.0 * HOLD_TIMEOUT - 0.01)
                .move_left
        );
        assert!(
            !collector.finish_frame(0.20 + 2.0 * HOLD_TIMEOUT).move_left,
            "the direction must still expire once input really falls silent"
        );
    }

    #[test]
    fn a_held_direction_survives_an_entire_jump_worth_of_jump_repeats() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Right, 0.0);

        // Jump is held down for the whole flight, so the terminal's single repeat
        // stream never returns to Right. 1.2s comfortably outlasts a real jump
        // arc (~0.68s to apex plus the descent).
        let mut step = 0;
        loop {
            let now = f64::from(step) * 0.04;
            if now > 1.2 {
                break;
            }
            collector.on_press(GameKey::Jump, now);
            assert!(
                collector.finish_frame(now).move_right,
                "direction lost at t={now}"
            );
            assert!(
                collector.finish_frame(now + 0.02).move_right,
                "direction lost between repeats at t={now}"
            );
            step += 1;
        }
    }

    #[test]
    fn a_direction_expires_once_all_input_falls_silent() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Right, 0.0);
        let mut last_jump = 0.0;
        for step in 0..13 {
            last_jump = f64::from(step) * 0.04;
            collector.on_press(GameKey::Jump, last_jump);
        }
        assert!(collector.finish_frame(last_jump).move_right);

        // Right's own last event was at t=0.0, far more than HOLD_TIMEOUT ago,
        // yet its deadline was carried forward by every Jump repeat that masked
        // it, so it lapses 2 x HOLD_TIMEOUT after Jump's final repeat.
        assert!(
            collector
                .finish_frame(last_jump + 2.0 * HOLD_TIMEOUT - 0.01)
                .move_right
        );
        assert!(
            !collector
                .finish_frame(last_jump + 2.0 * HOLD_TIMEOUT)
                .move_right
        );
    }

    #[test]
    fn a_released_direction_lingers_while_run_stays_active() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Right, 0.0);

        // Right is physically released right after the press, but Run keeps the
        // terminal's single repeat stream busy, so no silence is ever observed.
        // Nothing here is Jump-specific: Run is an ordinary auto-repeating key.
        let mut last_run = 0.0;
        for step in 0..=30 {
            last_run = f64::from(step) * 0.04;
            collector.on_press(GameKey::Run, last_run);
            let state = collector.finish_frame(last_run);
            assert!(
                state.move_right,
                "the released direction must keep reading as held at t={last_run}"
            );
            assert!(state.run_held);
        }
        assert!(
            last_run >= 1.0,
            "the window must outlast Right's own event by more than a second"
        );

        // Run was never masked, so its own deadline is one plain window past its
        // final repeat. That part is unchanged.
        assert!(
            collector
                .finish_frame(last_run + HOLD_TIMEOUT - 0.01)
                .run_held
        );
        let state = collector.finish_frame(last_run + HOLD_TIMEOUT);
        assert!(!state.run_held);
        // Right, however, was masked by every one of those repeats, so its
        // deadline sits a further window out and it does not die alongside Run.
        assert!(
            state.move_right,
            "a masked direction must not expire in lockstep with its masker"
        );
        assert!(
            collector
                .finish_frame(last_run + 2.0 * HOLD_TIMEOUT - 0.01)
                .move_right
        );
        assert!(
            !collector
                .finish_frame(last_run + 2.0 * HOLD_TIMEOUT)
                .move_right,
            "the direction must expire once Run also falls silent"
        );
    }

    #[test]
    fn timeout_mode_never_applies_a_jump_cut_from_expiry() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Jump, 0.0);
        assert!(collector.finish_frame(0.0).jump_pressed);
        collector.acknowledge_edges();

        let mut now = 0.0;
        let mut released_while_held = false;
        while now < HOLD_TIMEOUT * 4.0 {
            now += 1.0 / 120.0;
            let state = collector.finish_frame(now);
            if state.jump_released {
                released_while_held = true;
            }
        }
        assert!(
            !released_while_held,
            "an inferred expiry must not cut the jump short"
        );
        assert!(
            !collector.finish_frame(now).jump_held,
            "the hold itself must still lapse"
        );
    }

    #[test]
    fn a_masked_direction_survives_its_masking_key_going_silent() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Left, 0.0);
        // Jump owns the repeat stream, so Left gets no events of its own even
        // though it stays physically down.
        for now in [0.05, 0.10, 0.15, 0.20] {
            collector.on_press(GameKey::Jump, now);
            assert!(collector.finish_frame(now).move_left);
        }

        // Jump is released at t=0.20 and the terminal never resumes Left's
        // repeats. Under the old shared-silence timer both keys expired together
        // at t=0.45, dropping a direction the player was still holding.
        let state = collector.finish_frame(0.20 + HOLD_TIMEOUT);
        assert!(!state.jump_held, "Jump's own window ends here");
        assert!(
            state.move_left,
            "the still-held direction must not die with the key that masked it"
        );

        assert!(
            collector
                .finish_frame(0.20 + 2.0 * HOLD_TIMEOUT - 0.01)
                .move_left
        );
        assert!(!collector.finish_frame(0.20 + 2.0 * HOLD_TIMEOUT).move_left);
    }

    #[test]
    fn a_tapped_direction_hands_control_back_to_the_still_held_opposite() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Right, 0.0);
        assert!(collector.finish_frame(0.0).move_right);

        // A quick opposite tap while Right stays down. Right is masked from here
        // on, so its deadline moves to 0.10 + 2 x HOLD_TIMEOUT = 0.60.
        collector.on_press(GameKey::Left, 0.10);
        let state = collector.finish_frame(0.10);
        assert!(state.move_left, "the newer key wins the recency tie-break");
        assert!(!state.move_right);

        // Left is released immediately and neither key sends another event.
        assert!(collector.finish_frame(0.10 + HOLD_TIMEOUT - 0.01).move_left);
        let state = collector.finish_frame(0.10 + HOLD_TIMEOUT);
        assert!(
            !state.move_left,
            "the tap must stop outvoting Right after one window"
        );
        assert!(
            state.move_right,
            "Right reads through again with no fresh event of its own"
        );
        assert!(!collector.finish_frame(0.60).move_right);
    }

    #[test]
    fn a_tapped_direction_never_freezes_movement_while_a_third_key_repeats() {
        // Accepted, bounded limitation: with a third key repeating, both
        // directions keep getting their deadlines extended, so a released tap
        // can outvote a genuinely held opposite for longer than the two-key
        // case above. What must never happen is the total freeze of Bug A.
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Right, 0.0);
        collector.on_press(GameKey::Left, 0.10);
        assert!(collector.finish_frame(0.10).move_left);

        let mut step = 3;
        loop {
            let now = f64::from(step) * 0.05;
            if now > 1.5 {
                break;
            }
            collector.on_press(GameKey::Jump, now);
            let state = collector.finish_frame(now);
            assert!(
                state.move_left || state.move_right,
                "movement froze entirely at t={now}"
            );
            assert!(state.jump_held);
            step += 1;
        }
    }

    #[test]
    fn consume_edges_clears_one_shot_flags() {
        let mut state = InputState {
            jump_pressed: true,
            jump_released: true,
            jump_held: true,
            ..InputState::default()
        };
        state.consume_edges();
        assert!(!state.jump_pressed);
        assert!(!state.jump_released);
        assert!(state.jump_held, "held state is not an edge");
    }
}
