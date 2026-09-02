//! Fixed-timestep accounting. Holds no wall clock: callers supply elapsed seconds.

/// Simulation timestep. Chosen so that `max_fall_speed * FIXED_DT` stays well
/// under one tile, which is what keeps the per-axis collision sweep from
/// tunnelling through thin geometry.
pub const FIXED_DT: f32 = 1.0 / 120.0;

/// Rendering cap. Simulation rate is independent of this.
pub const TARGET_FPS: u32 = 60;

/// Longest frame the accumulator will absorb. Anything beyond this (a debugger
/// pause, a suspended process) is discarded rather than simulated, so the game
/// never tries to catch up with a huge burst of steps.
pub const MAX_FRAME_TIME: f32 = 0.25;

/// Upper bound on steps per frame. Excess accumulated time is dropped.
pub const MAX_STEPS_PER_FRAME: u32 = 8;

#[derive(Debug, Default)]
pub struct FixedClock {
    accumulator: f32,
}

impl FixedClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Folds `elapsed` seconds into the accumulator and reports how many fixed
    /// steps should run this frame.
    pub fn begin_frame(&mut self, elapsed: f32) -> u32 {
        self.accumulator += elapsed.clamp(0.0, MAX_FRAME_TIME);
        let steps = (self.accumulator / FIXED_DT) as u32;
        if steps >= MAX_STEPS_PER_FRAME {
            self.accumulator = 0.0;
            MAX_STEPS_PER_FRAME
        } else {
            self.accumulator -= steps as f32 * FIXED_DT;
            steps
        }
    }

    pub fn accumulator(&self) -> f32 {
        self.accumulator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_frames_accumulate_until_a_step_is_due() {
        let mut clock = FixedClock::new();
        assert_eq!(clock.begin_frame(FIXED_DT * 0.4), 0);
        assert_eq!(clock.begin_frame(FIXED_DT * 0.4), 0);
        assert_eq!(clock.begin_frame(FIXED_DT * 0.4), 1);
    }

    #[test]
    fn a_sixty_fps_frame_yields_two_steps_at_one_twenty_hertz() {
        let mut clock = FixedClock::new();
        assert_eq!(clock.begin_frame(1.0 / 60.0), 2);
    }

    #[test]
    fn a_long_stall_is_dropped_rather_than_caught_up() {
        let mut clock = FixedClock::new();
        assert_eq!(clock.begin_frame(30.0), MAX_STEPS_PER_FRAME);
        assert_eq!(clock.accumulator(), 0.0);
        assert_eq!(clock.begin_frame(0.0), 0);
    }

    #[test]
    fn negative_or_zero_elapsed_is_harmless() {
        let mut clock = FixedClock::new();
        assert_eq!(clock.begin_frame(-1.0), 0);
        assert_eq!(clock.accumulator(), 0.0);
    }
}
