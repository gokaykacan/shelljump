//! ShellJump entry point: owns the terminal, drives the frame loop.

use std::io;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use shelljump::game::Game;
use shelljump::input::terminal::EventPump;
use shelljump::render::terminal::TerminalRenderer;
use shelljump::render::{self, Framebuffer};
use shelljump::terminal::{self, TerminalGuard};
use shelljump::time::{FIXED_DT, FixedClock, TARGET_FPS};

fn main() -> ExitCode {
    terminal::install_panic_hook();

    let result = match TerminalGuard::enter() {
        Ok(guard) => {
            let outcome = run(&guard);
            drop(guard);
            outcome
        }
        Err(error) => Err(error),
    };

    if let Err(error) = result {
        eprintln!("shelljump: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run(guard: &TerminalGuard) -> io::Result<()> {
    let frame_budget = Duration::from_secs_f64(1.0 / f64::from(TARGET_FPS));

    // No Resize event arrives at startup, so the initial size must be queried.
    let (mut columns, mut rows) = terminal::size()?;
    let mut frame = Framebuffer::new(columns, rows);
    let mut renderer = TerminalRenderer::new(columns, rows);
    let mut pump = EventPump::new(guard.hold_mode());
    let mut clock = FixedClock::new();
    let mut game = Game::test_level(render::viewport_tiles(columns, rows));

    let epoch = Instant::now();
    let mut previous = epoch;

    loop {
        let now = Instant::now();
        let elapsed = now.duration_since(previous).as_secs_f32();
        previous = now;
        let deadline = now + frame_budget;

        let input = pump.finish_frame(now.duration_since(epoch).as_secs_f64());
        if input.quit_requested {
            break;
        }

        if let Some((new_columns, new_rows)) = input.resized {
            columns = new_columns;
            rows = new_rows;
            frame.resize(columns, rows);
            renderer.invalidate();
            game.set_viewport(render::viewport_tiles(columns, rows));
        }

        let steps = clock.begin_frame(elapsed);
        let mut step_input = input;
        for _ in 0..steps {
            game.step(&step_input, FIXED_DT);
            step_input.consume_edges();
        }
        if steps > 0 {
            // Only retire the latched edges once something acted on them.
            pump.acknowledge_edges();
        }

        if columns >= render::MIN_COLUMNS && rows >= render::MIN_ROWS {
            render::draw_scene(&mut frame, &game.snapshot());
        } else {
            render::draw_too_small(&mut frame);
        }
        renderer.present(&frame)?;

        // Spends the rest of the frame collecting input rather than sleeping
        // blind, which keeps the 60 FPS cap without adding input latency.
        pump.pump_until(deadline, epoch)?;
    }

    renderer.reset_colors()
}
