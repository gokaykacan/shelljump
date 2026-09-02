//! FPS independence. The same per-step input script must produce the same
//! simulation no matter how the fixed steps are grouped into rendered frames.

use shelljump::game::Game;
use shelljump::input::InputState;
use shelljump::math::Vec2;
use shelljump::render::viewport_tiles;
use shelljump::time::{FIXED_DT, FixedClock};

/// One entry per fixed step: walking, running, reversing and jumping in turn.
fn script(steps: usize) -> Vec<InputState> {
    (0..steps)
        .map(|i| {
            let phase = i % 240;
            let jump = phase % 60;
            InputState {
                move_right: phase < 150,
                move_left: (150..200).contains(&phase),
                run_held: phase % 3 != 0,
                jump_held: jump < 20,
                jump_pressed: jump == 0,
                jump_released: jump == 20,
                ..InputState::default()
            }
        })
        .collect()
}

fn simulate(frame_time: f32, frames: usize, script: &[InputState]) -> (Vec2, Vec2) {
    let mut game = Game::test_level(viewport_tiles(80, 24));
    let mut clock = FixedClock::new();
    let mut consumed = 0;
    for _ in 0..frames {
        for _ in 0..clock.begin_frame(frame_time) {
            game.step(&script[consumed], FIXED_DT);
            consumed += 1;
        }
    }
    assert_eq!(consumed, script.len(), "the whole script must have run");
    (game.player.position, game.player.velocity)
}

#[test]
fn frame_pacing_does_not_change_the_simulation() {
    let steps = 1200;
    let script = script(steps);

    let at_sixty = simulate(1.0 / 60.0, steps / 2, &script);
    let at_one_twenty = simulate(FIXED_DT, steps, &script);

    assert_eq!(
        at_sixty.0, at_one_twenty.0,
        "two fixed steps per frame drifted from one"
    );
    assert_eq!(at_sixty.1, at_one_twenty.1);
}
