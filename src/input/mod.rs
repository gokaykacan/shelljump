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

/// How long a key stays "held" in [`HoldMode::Timeout`] without a fresh event.
/// Comfortably longer than a typical ~30ms auto-repeat interval, short enough
/// that a release is noticed within a few frames.
pub const HOLD_TIMEOUT: f64 = 0.15;

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
    last_seen: [f64; KEY_COUNT],
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

    fn expire_stale_holds(&mut self, now: f64) {
        if self.mode != HoldMode::Timeout {
            return;
        }
        for key in ALL_KEYS {
            let index = key as usize;
            if self.held[index] && now - self.last_seen[index] >= HOLD_TIMEOUT {
                self.set_held(key, false);
            }
        }
    }

    /// Produces this frame's input. Edge latches persist until
    /// [`InputCollector::acknowledge_edges`] is called.
    pub fn finish_frame(&mut self, now: f64) -> InputState {
        self.expire_stale_holds(now);
        InputState {
            move_left: self.held[GameKey::Left as usize],
            move_right: self.held[GameKey::Right as usize],
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
