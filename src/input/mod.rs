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

/// How long a key's *first* event keeps it held in [`HoldMode::Timeout`].
///
/// Must outlast the OS "delay until repeat" (macOS ~375ms, GNOME ~500ms,
/// X11 ~660ms) or a genuinely held key expires before its first auto-repeat
/// ever arrives.
///
/// The price is ghost movement. With no release event to read, a direction key
/// the player really did let go of keeps reading as held for up to:
///
/// * ~0.75s — one window — when nothing else is pressed;
/// * ~0.95s when another key owns a live repeat stream, which renews the
///   released key's grace to one window past its masker's repeat window;
/// * ~1.5s when a single tap masks it, granting a window measured from one
///   window past the tap.
///
/// Only the last of those is visible as movement in the *wrong* direction, and
/// only after the newer key stops winning the recency tie-break.
/// `a_reversal_bounds_how_long_a_released_direction_keeps_moving` pins the
/// figure, so raising these windows shows up as a failing number rather than a
/// vague complaint about feel.
pub const PRESS_HOLD_WINDOW: f64 = 0.75;

/// How long an event keeps a key held once that key has delivered a second
/// consecutive event, proving its repeat stream is live.
///
/// Gaps are now the repeat interval rather than the initial delay, so this is
/// what bounds release-detection latency.
pub const REPEAT_HOLD_WINDOW: f64 = 0.20;

/// Gap below which a Jump event in [`HoldMode::Timeout`] is taken to belong to
/// the same auto-repeat train as the previous one rather than to be a second
/// physical tap. Comfortably above every *default* OS repeat interval
/// (~25-90ms) and comfortably below the fastest cadence a player can tap at.
pub const JUMP_REPEAT_DEDUP: f64 = 0.12;

/// Once a Jump event stream has been classified as a live auto-repeat train,
/// a gap up to this long is absorbed as a hiccup (a frame stall, a latency
/// burst) rather than read as a fresh tap.
///
/// Forgiveness is earned, spent, and must be earned again: an absorbed gap must
/// be preceded by [`JUMP_STREAM_CONFIRM_GAPS`] *consecutive* gaps tighter than
/// [`JUMP_REPEAT_DEDUP`], and absorbing one resets that count to zero. Nothing
/// in the [`JUMP_REPEAT_DEDUP`]..this band can therefore renew the train's
/// credibility on its own.
///
/// What that does and does not buy, measured rather than asserted:
///
/// * At *uniform* spacing in that band — a mash at a steady 0.15s straight out
///   of a held jump — exactly the first tap is absorbed and no two taps in a row
///   ever are. `a_mash_after_a_held_jump_still_latches` pins that.
/// * At *ragged* spacing that straddles [`JUMP_REPEAT_DEDUP`] — real human
///   mashing, e.g. 0.14s ± 0.04s — taps are still lost, in *runs*, and the loss
///   is not bounded at one. Measured over 200 such taps: 28.5% lost, worst run
///   4, i.e. about half a second of dead Jump key mid-panic. Practically all of
///   that is the dedup boundary rather than this rule — a gap landing under
///   [`JUMP_REPEAT_DEDUP`] is an OS repeat as far as timing can tell, and timing
///   is the only signal there is. Of those 57 lost taps, 56 were sub-dedup gaps
///   and 1 was a forgiven hiccup; under a one-gap confirmation the split was 56
///   and 43. Same accepted limit the slow-repeat paragraph below describes,
///   from the other side.
///   `a_jittered_mash_loses_a_bounded_but_nonzero_share_of_taps` pins every one
///   of those figures so a regression reads as a changed number.
///
/// A player mashing that fast is mashing *at* the boundary of what timing can
/// resolve; the honest fix is a terminal that reports releases
/// ([`HoldMode::Explicit`]), not a wider window here.
///
/// This is also the honest upper edge of the supported range. Repeat interval
/// is user-configurable far beyond it — macOS allows up to ~1.8s, GNOME up to
/// ~2s — and past this value a repeat is indistinguishable on timing alone from
/// a deliberate tap, because timing is the only signal there is. A *held* Jump
/// key at such a rate therefore latches one press edge per repeat rather than
/// staying a single hold; `a_held_jump_key_latches_at_most_two_presses` covers
/// both sides of the boundary and asserts that degradation explicitly rather
/// than leaving it to be discovered.
///
/// Widening the window to swallow slow repeat rates would mean inferring the
/// hold from `held[Jump]` again, which is exactly what swallowed genuine second
/// taps before.
pub const JUMP_STREAM_TOLERANCE: f64 = 0.20;

/// How many consecutive gaps tighter than [`JUMP_REPEAT_DEDUP`] must precede a
/// gap before [`JUMP_STREAM_TOLERANCE`] will absorb it.
///
/// One is enough to stop a *uniform* in-band cadence from renewing itself, but
/// not a ragged one: alternating tight/loose gaps let each tight gap re-arm the
/// forgiveness the next loose gap spends, and the oscillation absorbs every tap
/// indefinitely: 0 of 12 taps latched at alternating 0.10s/0.16s, 101 of 200 at
/// 0.14s ± 0.04s. A run of four costs a genuine stall nothing — a real repeat
/// train delivers tight gaps by the dozen before it hiccups — and takes those to
/// 5 of 12 and 143 of 200, roughly halving the forgiveness rule's own share of
/// the jittered loss versus a run of three (1 tap vs 2, out of 58 total drops).
/// The cost of going higher is a stall landing inside the first N repeats of a
/// genuine held-key train now latches a phantom press instead of being
/// forgiven — inert in practice, since that window sits well past both
/// `coyote_time` and a freshly-refilled `jump_buffer_time` expiring mid-air
/// (`a_held_jump_key_launches_exactly_one_jump` proves this against the real
/// simulation). Five and above measure no better than four on either case, so
/// four is the value that clears the largest share of the rule's own
/// contribution for that one extra repeat interval of cost.
pub const JUMP_STREAM_CONFIRM_GAPS: u32 = 4;

/// One frame of input, consumed by the simulation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputState {
    pub move_left: bool,
    pub move_right: bool,
    /// Level state, not an edge: raises the horizontal speed cap while down.
    pub run_held: bool,
    /// Inferred, best-effort: in [`HoldMode::Timeout`] there is no release event
    /// to read, so this is whatever the hold/expiry machinery currently guesses.
    /// No simulation code consumes it — the gameplay-relevant jump edges are
    /// [`InputState::jump_pressed`] and [`InputState::jump_released`], which are
    /// decided by a separate and far narrower mechanism.
    pub jump_held: bool,
    /// Latched rising edge; survives frames in which no fixed step ran.
    pub jump_pressed: bool,
    /// Latched falling edge; drives the variable-height jump cut.
    ///
    /// Only ever set in [`HoldMode::Explicit`]. [`HoldMode::Timeout`] has no
    /// release to observe and refuses to guess one — `expire_stale_holds`
    /// bypasses `set_held` precisely so an inferred expiry cannot cut a jump
    /// short — so variable jump height is unavailable on those terminals and
    /// every jump there flies its full arc.
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
    /// Consulted only in [`HoldMode::Timeout`]; see
    /// [`InputCollector::refresh_deadlines`].
    expiry_at: [f64; KEY_COUNT],
    /// Timestamp of the last Jump press/repeat event, used only for the
    /// Timeout-mode tap-vs-repeat dedup decision. Independent of `last_seen`,
    /// which is read before it is written and serves direction conflicts only.
    jump_last_press_at: f64,
    /// Whether the current Jump event stream has been classified as a live
    /// auto-repeat train rather than a run of fresh taps. Timeout-mode only.
    jump_stream_live: bool,
    /// How many gaps in a row, counting back from the last Jump event, were
    /// inside [`JUMP_REPEAT_DEDUP`]. Gates [`JUMP_STREAM_TOLERANCE`]: a hiccup
    /// is forgiven only on the strength of a *run* of tight repeats, and
    /// forgiving one zeroes the run, so neither an already-forgiven gap nor a
    /// ragged tight/loose alternation can keep the train credible.
    /// Timeout-mode only.
    jump_consecutive_repeat_gaps: u32,
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
            jump_last_press_at: f64::NEG_INFINITY,
            jump_stream_live: false,
            jump_consecutive_repeat_gaps: 0,
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

    /// Decides whether one Jump event is a fresh physical press, and latches the
    /// rising edge if so.
    ///
    /// A false→true transition on `held[Jump]` cannot answer this in
    /// [`HoldMode::Timeout`]: the hold lingers for a whole [`PRESS_HOLD_WINDOW`]
    /// after a press, so a genuine second tap inside that window is no
    /// transition at all and gets swallowed. The gap since the previous Jump
    /// event is the only signal that separates "the OS is repeating the key I am
    /// still holding" from "I tapped again", and repeat intervals are an order
    /// of magnitude tighter than any cadence a player can tap at.
    fn latch_jump_press(&mut self, now: f64) {
        if self.mode == HoldMode::Explicit {
            // Real press/repeat/release events already disambiguate: the key
            // reads as up only once a genuine release has arrived.
            self.set_held(GameKey::Jump, true);
            return;
        }
        let gap = now - self.jump_last_press_at;
        let is_repeat_gap = gap < JUMP_REPEAT_DEDUP;
        // A forgiven hiccup must be backed by a run of tight repeats, and
        // spends that run. Without the gate the "live" flag renewed itself off
        // its own forgiveness and a sustained mash inside JUMP_STREAM_TOLERANCE
        // latched nothing at all; with a run of one, a ragged mash straddling
        // JUMP_REPEAT_DEDUP re-armed it on every other tap and did the same.
        let is_forgivable_hiccup = self.jump_stream_live
            && self.jump_consecutive_repeat_gaps >= JUMP_STREAM_CONFIRM_GAPS
            && gap < JUMP_STREAM_TOLERANCE;
        if is_repeat_gap || is_forgivable_hiccup {
            self.jump_stream_live = true;
        } else {
            self.jump_stream_live = false;
            self.jump_pressed_latch = true;
        }
        self.jump_consecutive_repeat_gaps = if is_repeat_gap {
            self.jump_consecutive_repeat_gaps.saturating_add(1)
        } else {
            0
        };
        self.jump_last_press_at = now;
        // Deliberately not `set_held`: the edge decision above has already been
        // made, and routing through it would latch a second press.
        self.held[GameKey::Jump as usize] = true;
    }

    /// Records a press or auto-repeat at `now` seconds since application start.
    pub fn on_press(&mut self, key: GameKey, now: f64) {
        self.last_seen[key as usize] = now;
        if self.mode == HoldMode::Timeout {
            self.refresh_deadlines(key, now);
        }
        if key == GameKey::Jump {
            self.latch_jump_press(now);
        } else {
            self.set_held(key, true);
        }
    }

    /// Rolls the inferred-hold deadlines forward for one event on `key`.
    ///
    /// The binding constraint is not the auto-repeat interval but repeat
    /// *ownership*: the terminal delivers a single repeat stream, so pressing a
    /// second key moves repeats to it and starves the first key for as long as
    /// the second stays down. A plain per-key window is therefore never long
    /// enough on its own — holding Jump through a jump starves the direction key
    /// for the whole flight. So every key carries a durable absolute deadline,
    /// fed from two sources: its own events, and grace granted by whichever key
    /// took the stream away from it.
    fn refresh_deadlines(&mut self, key: GameKey, now: f64) {
        let index = key as usize;
        // A second consecutive event on an already-held key proves this key owns
        // a live repeat stream, so from here its gaps are repeat intervals rather
        // than the much longer initial delay. Purely a fact about this one
        // event: losing the stream is already handled by the grace below, so
        // there is nothing worth remembering across calls.
        let repeating = self.held[index];
        self.expiry_at[index] = if repeating {
            // Hard set rather than `max`: a key that can speak for itself has no
            // use for grace granted while it was starved, and discarding that
            // grace is what keeps release detection snappy.
            now + REPEAT_HOLD_WINDOW
        } else {
            // `max` preserves any grace this key was granted while masked; a
            // first event must never walk a deadline backwards.
            self.expiry_at[index].max(now + PRESS_HOLD_WINDOW)
        };

        for other in ALL_KEYS {
            // Jump is deliberately excluded. Its held flag is pure edge-latching
            // bookkeeping that no simulation code reads, so grace buys it
            // nothing — and it costs everything: a repeating direction key would
            // hold Jump down forever, and every tap after the first would be
            // swallowed as a no-op instead of latching a press edge.
            if other == key || other == GameKey::Jump {
                continue;
            }
            let other_index = other as usize;
            if !self.held[other_index] {
                continue;
            }
            // `other` just lost the repeat stream to this key. Keep it alive for
            // one full initial delay measured from where this key's own hold
            // lapses, so the terminal has a fair chance to hand the stream back
            // before `other` is written off. A masking key that keeps repeating
            // renews this indefinitely.
            self.expiry_at[other_index] =
                self.expiry_at[other_index].max(self.expiry_at[index] + PRESS_HOLD_WINDOW);
        }
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
    /// [`InputCollector::refresh_deadlines`].
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

    /// Both directions reading as held would otherwise cancel to a dead stop: in
    /// [`HoldMode::Timeout`] because a tap lingers for [`PRESS_HOLD_WINDOW`], and
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
        // Far beyond any inferred-hold window: no release arrived, so the key is
        // still down.
        let state = collector.finish_frame(10.0);
        assert!(state.move_right);
    }

    #[test]
    fn timeout_mode_infers_release_when_repeats_stop() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        // Press at t=0 so the elapsed comparison lands exactly on the boundary.
        collector.on_press(GameKey::Left, 0.0);
        assert!(collector.finish_frame(0.0).move_left);
        assert!(collector.finish_frame(PRESS_HOLD_WINDOW - 0.01).move_left);
        assert!(!collector.finish_frame(PRESS_HOLD_WINDOW).move_left);
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
        assert!(collector.finish_frame(PRESS_HOLD_WINDOW - 0.01).run_held);
        assert!(!collector.finish_frame(PRESS_HOLD_WINDOW).run_held);
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
        // no further events even though it is still physically down. The loop
        // runs past 1.275s, a measured full jump flight at the shipped tuning.
        let mut last_jump = 0.0;
        let mut step = 1;
        loop {
            let now = f64::from(step) * 0.05;
            if now > 1.35 {
                break;
            }
            collector.on_press(GameKey::Jump, now);
            assert!(
                collector.finish_frame(now).move_left,
                "direction lost at t={now}"
            );
            last_jump = now;
            step += 1;
        }
        assert!(last_jump > 1.275, "the masking must outlast a whole flight");

        // Left's deadline tracks Jump's rather than its own last event: it sits
        // one full initial delay past where Jump's hold lapses, so it outlives
        // Jump instead of expiring in lockstep with it (that lockstep was Bug A).
        let deadline = last_jump + REPEAT_HOLD_WINDOW + PRESS_HOLD_WINDOW;
        assert!(collector.finish_frame(deadline - 0.01).move_left);
        assert!(
            !collector.finish_frame(deadline).move_left,
            "the direction must still expire once input really falls silent"
        );
    }

    #[test]
    fn a_held_direction_survives_an_entire_jump_worth_of_jump_repeats() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Right, 0.0);

        // Jump is held down for the whole flight, so the terminal's single repeat
        // stream never returns to Right. 1.4s comfortably outlasts a real jump
        // arc (1.275s measured at the shipped tuning).
        let mut step = 0;
        loop {
            let now = f64::from(step) * 0.04;
            if now > 1.4 {
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
        for step in 0..25 {
            last_jump = f64::from(step) * 0.04;
            collector.on_press(GameKey::Jump, last_jump);
        }
        assert!(collector.finish_frame(last_jump).move_right);

        // Right's own last event was at t=0.0, far beyond any window of its own,
        // yet its deadline was carried forward by every Jump repeat that masked
        // it, so it lapses one initial delay past Jump's final repeat window.
        let deadline = last_jump + REPEAT_HOLD_WINDOW + PRESS_HOLD_WINDOW;
        assert!(collector.finish_frame(deadline - 0.01).move_right);
        assert!(!collector.finish_frame(deadline).move_right);
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

        // Run owned the repeat stream throughout, so its own deadline is one
        // repeat window past its final event.
        assert!(
            collector
                .finish_frame(last_run + REPEAT_HOLD_WINDOW - 0.01)
                .run_held
        );
        let state = collector.finish_frame(last_run + REPEAT_HOLD_WINDOW);
        assert!(!state.run_held);
        // Right, however, was masked by every one of those repeats, so its
        // deadline sits a further initial delay out and it does not die
        // alongside Run.
        assert!(
            state.move_right,
            "a masked direction must not expire in lockstep with its masker"
        );
        let deadline = last_run + REPEAT_HOLD_WINDOW + PRESS_HOLD_WINDOW;
        assert!(collector.finish_frame(deadline - 0.01).move_right);
        assert!(
            !collector.finish_frame(deadline).move_right,
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
        while now < PRESS_HOLD_WINDOW * 2.0 {
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
        // repeats. Under the old shared-silence timer both keys expired together,
        // dropping a direction the player was still holding.
        let state = collector.finish_frame(0.20 + REPEAT_HOLD_WINDOW);
        assert!(!state.jump_held, "Jump's own window ends here");
        assert!(
            state.move_left,
            "the still-held direction must not die with the key that masked it"
        );

        // Left's grace came from Jump's *first* press at t=0.05, which was worth
        // a full initial delay; the tighter repeat windows of the Jump repeats
        // that followed never pushed it any further out.
        let deadline = 0.05 + PRESS_HOLD_WINDOW + PRESS_HOLD_WINDOW;
        assert!(collector.finish_frame(deadline - 0.01).move_left);
        assert!(!collector.finish_frame(deadline).move_left);
    }

    #[test]
    fn a_tapped_direction_hands_control_back_to_the_still_held_opposite() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        collector.on_press(GameKey::Right, 0.0);
        assert!(collector.finish_frame(0.0).move_right);

        // A quick opposite tap while Right stays down. Right is masked from here
        // on, so its deadline moves out to one initial delay past Left's.
        collector.on_press(GameKey::Left, 0.10);
        let state = collector.finish_frame(0.10);
        assert!(state.move_left, "the newer key wins the recency tie-break");
        assert!(!state.move_right);

        // Left is released immediately and neither key sends another event. A
        // single tap is indistinguishable from a key still inside its initial
        // repeat delay, so it outvotes Right for a full PRESS_HOLD_WINDOW.
        assert!(
            collector
                .finish_frame(0.10 + PRESS_HOLD_WINDOW - 0.01)
                .move_left
        );
        let state = collector.finish_frame(0.10 + PRESS_HOLD_WINDOW);
        assert!(
            !state.move_left,
            "the tap must stop outvoting Right after one window"
        );
        assert!(
            state.move_right,
            "Right reads through again with no fresh event of its own"
        );
        assert!(
            !collector
                .finish_frame(0.10 + PRESS_HOLD_WINDOW + PRESS_HOLD_WINDOW)
                .move_right
        );
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
    fn a_lone_held_key_survives_a_real_initial_repeat_delay() {
        // macOS, GNOME and X11 defaults for "delay until repeat". A held key
        // gets no event of its own until the first of these elapses.
        for delay in [0.375, 0.50, 0.66] {
            let mut collector = InputCollector::new(HoldMode::Timeout);
            collector.on_press(GameKey::Right, 0.0);

            let mut now = 0.0;
            while now < delay {
                assert!(
                    collector.finish_frame(now).move_right,
                    "the hold died at t={now}, before its first repeat at {delay}"
                );
                now += 1.0 / 120.0;
            }

            collector.on_press(GameKey::Right, delay);
            assert!(collector.finish_frame(delay).move_right);
        }
    }

    #[test]
    fn a_single_jump_tap_does_not_starve_a_held_direction_through_a_flight() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        // Right is genuinely held and owns the repeat stream. Jump is tapped
        // once and never repeats, so it never renews Right's masking grace —
        // Right has to survive the whole flight on its own events.
        collector.on_press(GameKey::Right, 0.0);
        collector.on_press(GameKey::Jump, 0.05);

        let mut step = 0;
        loop {
            let now = f64::from(step) * 0.03;
            if now > 1.35 {
                break;
            }
            if step > 0 {
                collector.on_press(GameKey::Right, now);
            }
            assert!(
                collector.finish_frame(now).move_right,
                "direction lost at t={now}"
            );
            assert!(
                collector.finish_frame(now + 0.015).move_right,
                "direction lost between repeats at t={now}"
            );
            step += 1;
        }
    }

    #[test]
    fn every_jump_tap_latches_while_a_direction_repeats() {
        // Jump's latch decision is the gap since its own previous event, so it
        // is independent of whether its hold has lapsed and a repeating
        // direction key cannot suppress a tap. Back when the decision was a
        // false→true transition on `held[Jump]`, masking grace pinned Jump down
        // forever and every tap after the first was swallowed as a no-op.
        let mut collector = InputCollector::new(HoldMode::Timeout);
        let taps = [17, 66, 100];
        let mut jumps = 0;
        for step in 0..=117 {
            let now = f64::from(step) * 0.03;
            collector.on_press(GameKey::Right, now);
            if taps.contains(&step) {
                collector.on_press(GameKey::Jump, now);
            }
            let state = collector.finish_frame(now);
            assert!(state.move_right, "the direction must stay held throughout");
            if state.jump_pressed {
                jumps += 1;
            }
            collector.acknowledge_edges();
        }
        assert_eq!(jumps, taps.len(), "every tap must latch its own press edge");
    }

    #[test]
    fn holding_jump_latches_exactly_one_press() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        let mut jumps = 0;
        for step in 0..=100 {
            let now = f64::from(step) * 0.03;
            collector.on_press(GameKey::Jump, now);
            if collector.finish_frame(now).jump_pressed {
                jumps += 1;
            }
            collector.acknowledge_edges();
        }
        assert_eq!(jumps, 1, "a held jump key must not auto-fire");
    }

    /// Replaces an earlier test that demanded a held Jump key latch exactly one
    /// press *ever*. Meeting that bound required inferring the hold from
    /// `held[Jump]`, which is what swallowed genuine second taps arriving inside
    /// [`PRESS_HOLD_WINDOW`]. The gap-based rule cannot tell the very first
    /// auto-repeat — arriving one OS initial delay later, far outside any repeat
    /// interval — from a deliberate second tap, so it may latch one extra edge
    /// there. What must still hold, and is what this asserts, is that the count
    /// stays *bounded*: a held key must never machine-gun. The extra edge is
    /// harmless in play, proven end to end by
    /// `a_held_jump_key_launches_exactly_one_jump` in `tests/input_to_physics`.
    #[test]
    fn a_held_jump_key_latches_at_most_two_presses() {
        for initial_delay in [0.225, 0.375, 0.50, 0.66] {
            // 0.03-0.09 are the OS defaults, absorbed into a single hold. 0.30
            // and 0.50 are custom repeat rates past
            // [`JUMP_STREAM_TOLERANCE`], where gaps are wider than any tap
            // cadence and timing — the only signal available — can no longer
            // separate a repeat from a deliberate press. Recorded here rather
            // than left to be discovered: at those rates a held key latches one
            // edge per repeat, and `absorbed` says which contract applies.
            for (interval, absorbed) in [
                (0.03, true),
                (0.05, true),
                (0.09, true),
                (0.30, false),
                (0.50, false),
            ] {
                let mut collector = InputCollector::new(HoldMode::Timeout);
                let mut jumps = 0;
                let mut events = 0;
                let mut now = 0.0;
                let mut next_event = 0.0;
                let mut gap = initial_delay;
                while now < 2.0 {
                    if now >= next_event {
                        collector.on_press(GameKey::Jump, now);
                        next_event = now + gap;
                        gap = interval;
                        events += 1;
                    }
                    if collector.finish_frame(now).jump_pressed {
                        jumps += 1;
                    }
                    collector.acknowledge_edges();
                    now += 1.0 / 120.0;
                }
                let case = format!("delay {initial_delay}, interval {interval}");
                if absorbed {
                    assert!(jumps <= 2, "{case}: a held key auto-fired {jumps} times");
                } else {
                    assert_eq!(
                        jumps, events,
                        "{case}: known degradation past JUMP_STREAM_TOLERANCE — \
                         every repeat latches. Fewer edges than events would mean \
                         input was being swallowed instead, which is the failure \
                         that actually matters"
                    );
                }
            }
        }
    }

    /// The worst case of the inferred-hold design, pinned to numbers so that
    /// changing a window fails a test instead of quietly changing how the game
    /// feels. See [`PRESS_HOLD_WINDOW`].
    #[test]
    fn a_reversal_bounds_how_long_a_released_direction_keeps_moving() {
        let mut collector = InputCollector::new(HoldMode::Timeout);

        // Right is genuinely held long enough to own a live repeat stream.
        let mut last_right = 0.0;
        for step in 0..=10 {
            last_right = f64::from(step) * 0.05;
            collector.on_press(GameKey::Right, last_right);
            assert!(collector.finish_frame(last_right).move_right);
        }

        // The player reverses: Right is released, Left is tapped once, and Left
        // is released too. Neither key sends another event from here on, which
        // is the worst case — a tap grants the longest masking grace of all.
        let reversal = last_right + 0.05;
        collector.on_press(GameKey::Left, reversal);

        // The reversal is honoured immediately. While both keys read as held it
        // is recency, not deadlines, that decides, so the stale Right never
        // fights the input the player actually gave.
        let ghost_from = reversal + PRESS_HOLD_WINDOW;
        let mut now = reversal;
        while now < ghost_from {
            let state = collector.finish_frame(now);
            assert!(state.move_left, "the reversal was not honoured at t={now}");
            assert!(!state.move_right, "the stale direction won at t={now}");
            now += 1.0 / 120.0;
        }

        // Left's tap stops outvoting Right one window after it, and Right —
        // released long ago, but granted grace by that very tap — reads through
        // again. This is the ghost: the character resumes moving in a direction
        // the player let go of well over a second earlier.
        assert!(collector.finish_frame(ghost_from).move_right);

        let ghost_until = reversal + 2.0 * PRESS_HOLD_WINDOW;
        assert!(collector.finish_frame(ghost_until - 0.01).move_right);
        let state = collector.finish_frame(ghost_until);
        assert!(!state.move_right, "the ghost must expire");
        assert!(!state.move_left, "all movement must have stopped");

        // The two figures that matter in play.
        assert!(
            (ghost_until - ghost_from - 0.75).abs() < 1e-9,
            "ghost movement now lasts {}s, not 0.75s",
            ghost_until - ghost_from
        );
        assert!(
            (ghost_until - reversal - 1.5).abs() < 1e-9,
            "movement now outlives the last real key event by {}s, not 1.5s",
            ghost_until - reversal
        );
    }

    #[test]
    fn repeated_jump_taps_at_a_human_cadence_each_latch() {
        // The exact spacing band that the inferred-hold model swallowed: slower
        // than any auto-repeat, faster than PRESS_HOLD_WINDOW, so every tap
        // after the first landed while Jump still read as held.
        let mut collector = InputCollector::new(HoldMode::Timeout);
        let mut jumps = 0;
        for tap in 0..4 {
            let now = f64::from(tap) * 0.35;
            collector.on_press(GameKey::Jump, now);
            if collector.finish_frame(now).jump_pressed {
                jumps += 1;
            }
            collector.acknowledge_edges();
        }
        assert_eq!(jumps, 4, "every deliberate tap must launch a jump");
    }

    #[test]
    fn every_realistic_tap_cadence_latches_every_tap() {
        for spacing in [
            0.15, 0.20, 0.25, 0.30, 0.35, 0.45, 0.55, 0.65, 0.75, 0.90, 1.20,
        ] {
            let mut collector = InputCollector::new(HoldMode::Timeout);
            let mut jumps = 0;
            let mut now = 0.0;
            let mut taps = 0;
            let mut next_tap = 0.0;
            while taps < 6 {
                if now >= next_tap {
                    collector.on_press(GameKey::Jump, now);
                    next_tap = now + spacing;
                    taps += 1;
                }
                if collector.finish_frame(now).jump_pressed {
                    jumps += 1;
                }
                collector.acknowledge_edges();
                now += 1.0 / 120.0;
            }
            assert_eq!(jumps, 6, "only {jumps} of 6 taps latched at {spacing}s");
        }
    }

    #[test]
    fn sub_threshold_jump_events_collapse_to_one_press() {
        // Below JUMP_REPEAT_DEDUP nothing human is happening: this is an OS
        // repeat train and must read as a single hold.
        for spacing in [0.03, 0.05, 0.08, 0.10] {
            let mut collector = InputCollector::new(HoldMode::Timeout);
            let mut jumps = 0;
            for step in 0..20 {
                let now = f64::from(step) * spacing;
                collector.on_press(GameKey::Jump, now);
                if collector.finish_frame(now).jump_pressed {
                    jumps += 1;
                }
                collector.acknowledge_edges();
            }
            assert_eq!(jumps, 1, "a {spacing}s stream is a hold, not taps");
        }
    }

    #[test]
    fn tap_cadence_and_direction_masking_compose() {
        // Round 5 fixed taps being swallowed by a repeating direction key; this
        // round fixes taps being swallowed by Jump's own lingering hold. Neither
        // fix may undo the other.
        for spacing in [
            0.15, 0.20, 0.25, 0.30, 0.35, 0.45, 0.55, 0.65, 0.75, 0.90, 1.20,
        ] {
            let mut collector = InputCollector::new(HoldMode::Timeout);
            let mut jumps = 0;
            let mut now = 0.0;
            let mut taps = 0;
            let mut next_tap = 0.0;
            while taps < 6 {
                collector.on_press(GameKey::Right, now);
                if now >= next_tap {
                    collector.on_press(GameKey::Jump, now);
                    next_tap = now + spacing;
                    taps += 1;
                }
                let state = collector.finish_frame(now);
                assert!(
                    state.move_right,
                    "the direction dropped at t={now} with {spacing}s taps"
                );
                if state.jump_pressed {
                    jumps += 1;
                }
                collector.acknowledge_edges();
                now += 0.03;
            }
            assert_eq!(jumps, 6, "only {jumps} of 6 taps latched at {spacing}s");
        }
    }

    #[test]
    fn a_stalled_frame_mid_repeat_train_does_not_latch_a_phantom_press() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        let mut jumps = 0;
        let mut now = 0.0;
        for step in 0..30 {
            // One 0.15s hiccup — beyond JUMP_REPEAT_DEDUP but inside
            // JUMP_STREAM_TOLERANCE — in an otherwise steady train.
            now += if step == 10 { 0.15 } else { 0.03 };
            collector.on_press(GameKey::Jump, now);
            if collector.finish_frame(now).jump_pressed {
                jumps += 1;
            }
            collector.acknowledge_edges();
        }
        assert_eq!(jumps, 1, "a stalled frame must not read as a new tap");
    }

    #[test]
    fn a_mash_after_a_held_jump_still_latches() {
        // Cadences inside the JUMP_REPEAT_DEDUP..JUMP_STREAM_TOLERANCE band,
        // which a cold collector already handles (see
        // `every_realistic_tap_cadence_latches_every_tap`). The bug was that a
        // *warm* one did not: the live-train flag renewed itself off its own
        // forgiveness, so an unbroken chain of taps at any of these spacings
        // latched zero presses for as long as the player kept mashing.
        for spacing in [0.13, 0.15, 0.19] {
            let mut collector = InputCollector::new(HoldMode::Timeout);
            let mut now = 0.0;

            // A realistic held Jump: one OS initial delay, then a tight repeat
            // train. This is what establishes the live train.
            collector.on_press(GameKey::Jump, now);
            now += 0.375;
            collector.on_press(GameKey::Jump, now);
            for _ in 0..20 {
                now += 0.03;
                collector.on_press(GameKey::Jump, now);
            }
            collector.finish_frame(now);
            collector.acknowledge_edges();

            // The player lets go and panic-mashes mid-air.
            const TAPS: usize = 10;
            let mut jumps = 0;
            let mut dropped_run = 0;
            let mut worst_dropped_run = 0;
            for tap in 0..TAPS {
                now += spacing;
                collector.on_press(GameKey::Jump, now);
                if collector.finish_frame(now).jump_pressed {
                    jumps += 1;
                    dropped_run = 0;
                } else {
                    dropped_run += 1;
                    worst_dropped_run = worst_dropped_run.max(dropped_run);
                    assert_eq!(
                        tap, 0,
                        "only the tap at the transition may be forgiven ({spacing}s spacing)"
                    );
                }
                collector.acknowledge_edges();
            }
            assert_eq!(
                jumps,
                TAPS - 1,
                "a {spacing}s mash after a held jump latched only {jumps} of {TAPS} taps"
            );
            assert_eq!(
                worst_dropped_run, 1,
                "two taps in a row were absorbed at {spacing}s spacing"
            );
        }
    }

    /// Drives a warm collector through `gaps` and reports (latched, total,
    /// worst run of consecutive drops).
    fn mash_a_warm_collector(gaps: &[f64]) -> (usize, usize, usize) {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        let mut now = 0.0;

        // A realistic held Jump: one OS initial delay, then a tight repeat
        // train. This is what establishes the live train.
        collector.on_press(GameKey::Jump, now);
        now += 0.375;
        collector.on_press(GameKey::Jump, now);
        for _ in 0..20 {
            now += 0.03;
            collector.on_press(GameKey::Jump, now);
        }
        collector.finish_frame(now);
        collector.acknowledge_edges();

        let mut latched = 0;
        let mut run = 0;
        let mut worst_run = 0;
        for gap in gaps {
            now += gap;
            collector.on_press(GameKey::Jump, now);
            if collector.finish_frame(now).jump_pressed {
                latched += 1;
                run = 0;
            } else {
                run += 1;
                worst_run = worst_run.max(run);
            }
            collector.acknowledge_edges();
        }
        (latched, gaps.len(), worst_run)
    }

    /// Records what a *ragged* mash actually costs, as opposed to the uniform
    /// one `a_mash_after_a_held_jump_still_latches` covers. Both cases below
    /// absorbed every single tap, indefinitely, while forgiveness needed only
    /// one preceding tight gap: an alternation across [`JUMP_REPEAT_DEDUP`]
    /// re-armed the rule as fast as it was spent. Requiring
    /// [`JUMP_STREAM_CONFIRM_GAPS`] in a row breaks that loop, but does not and
    /// cannot make the loss zero — every gap that lands under
    /// [`JUMP_REPEAT_DEDUP`] is an OS repeat as far as timing can tell, and
    /// timing is all there is. These numbers are the accepted cost, pinned so a
    /// regression reads as a changed figure rather than as a player complaining
    /// the Jump key died mid-panic.
    #[test]
    fn a_jittered_mash_loses_a_bounded_but_nonzero_share_of_taps() {
        // The exact oscillation repro: gaps alternating either side of
        // JUMP_REPEAT_DEDUP.
        let alternating: Vec<f64> = (0..12)
            .map(|tap| if tap % 2 == 0 { 0.10 } else { 0.16 })
            .collect();
        let (latched, total, worst_run) = mash_a_warm_collector(&alternating);
        assert_eq!(
            (latched, total, worst_run),
            (5, 12, 3),
            "alternating-gap mash: {latched} of {total} latched, worst drop run {worst_run}"
        );

        // A realistic ragged human mash: 0.14s mean, ±0.04s of jitter, from a
        // xorshift64 so the figures are reproducible without a dependency.
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next_gap = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            // Uniform over [0.10, 0.18] in 0.1ms steps.
            0.10 + f64::from(u32::try_from(seed % 801).unwrap()) * 0.0001
        };
        let jittered: Vec<f64> = (0..200).map(|_| next_gap()).collect();
        let (latched, total, worst_run) = mash_a_warm_collector(&jittered);
        assert_eq!(
            (latched, total, worst_run),
            (143, 200, 4),
            "jittered mash: {latched} of {total} latched, worst drop run {worst_run}"
        );

        // Where the loss comes from. Gaps under JUMP_REPEAT_DEDUP are lost to
        // the dedup boundary itself and no rule here can recover them; the
        // forgiveness rule accounts for whatever is left over. Under the round-8
        // gate (JUMP_STREAM_CONFIRM_GAPS = 1) these same 200 gaps latched 101
        // with a drop run of 11 — 43 taps lost to the rule on top of the 56 the
        // boundary takes — and the alternating case latched 0 of 12. The loss
        // was mostly the rule. It no longer is.
        let below_dedup = jittered.iter().filter(|g| **g < JUMP_REPEAT_DEDUP).count();
        assert_eq!(
            below_dedup, 56,
            "the jitter must straddle JUMP_REPEAT_DEDUP"
        );
        assert_eq!(
            total - latched - below_dedup,
            1,
            "forgiveness costs 1 tap on top of the {below_dedup} the dedup \
             boundary takes; it cost 43 under the round-8 gate"
        );
    }

    #[test]
    fn a_live_repeat_stream_tightens_the_release_window() {
        let mut collector = InputCollector::new(HoldMode::Timeout);
        let mut last = 0.0;
        for step in 0..=14 {
            last = f64::from(step) * 0.05;
            collector.on_press(GameKey::Right, last);
            assert!(collector.finish_frame(last).move_right);
        }
        assert!(last >= 0.7, "the stream must run well past a first press");

        // The key proved it can speak for itself, so it is written off one repeat
        // interval after falling silent rather than one whole initial delay.
        assert!(
            collector
                .finish_frame(last + REPEAT_HOLD_WINDOW - 0.01)
                .move_right
        );
        assert!(!collector.finish_frame(last + REPEAT_HOLD_WINDOW).move_right);
    }

    #[test]
    fn explicit_mode_never_consults_a_hold_window() {
        let mut collector = InputCollector::new(HoldMode::Explicit);
        collector.on_press(GameKey::Right, 0.0);
        collector.on_press(GameKey::Run, 0.0);
        collector.on_press(GameKey::Jump, 0.0);
        collector.acknowledge_edges();

        let state = collector.finish_frame(10.0);
        assert!(
            state.move_right,
            "no window may apply where releases are real"
        );
        assert!(state.run_held);
        assert!(state.jump_held);

        collector.on_release(GameKey::Right);
        collector.on_release(GameKey::Run);
        collector.on_release(GameKey::Jump);
        let state = collector.finish_frame(10.0);
        assert!(!state.move_right);
        assert!(!state.run_held);
        assert!(!state.jump_held);
        assert!(state.jump_released, "a real release is authoritative");
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
