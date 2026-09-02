//! Framebuffer diffing and resize behaviour. No terminal is touched.

use shelljump::game::Game;
use shelljump::input::InputState;
use shelljump::render::{
    Cell, DrawRun, Framebuffer, Rgb, compute_runs, draw_scene, draw_too_small, viewport_tiles,
};
use shelljump::time::FIXED_DT;

fn painted_cells(runs: &[DrawRun]) -> u32 {
    runs.iter().map(|run| u32::from(run.len)).sum()
}

#[test]
fn an_identical_frame_produces_no_output() {
    let game = Game::test_level(viewport_tiles(80, 24));
    let mut frame = Framebuffer::new(80, 24);
    draw_scene(&mut frame, &game.snapshot());

    let previous = frame.clone();
    let mut runs = Vec::new();
    compute_runs(&frame, Some(&previous), &mut runs);
    assert!(runs.is_empty(), "an unchanged frame must diff to nothing");
    assert_eq!(painted_cells(&runs), 0);
}

#[test]
fn a_first_frame_repaints_everything() {
    let game = Game::test_level(viewport_tiles(80, 24));
    let mut frame = Framebuffer::new(80, 24);
    draw_scene(&mut frame, &game.snapshot());

    let mut runs = Vec::new();
    compute_runs(&frame, None, &mut runs);
    assert_eq!(painted_cells(&runs), 80 * 24);
}

#[test]
fn only_the_changed_region_is_repainted_when_the_player_moves() {
    let mut game = Game::test_level(viewport_tiles(80, 24));
    let idle = InputState::default();
    for _ in 0..600 {
        game.step(&idle, FIXED_DT);
    }

    let mut previous = Framebuffer::new(80, 24);
    draw_scene(&mut previous, &game.snapshot());

    let right = InputState {
        move_right: true,
        ..InputState::default()
    };
    for _ in 0..12 {
        game.step(&right, FIXED_DT);
    }
    let mut frame = Framebuffer::new(80, 24);
    draw_scene(&mut frame, &game.snapshot());

    let mut runs = Vec::new();
    compute_runs(&frame, Some(&previous), &mut runs);
    let painted = painted_cells(&runs);
    assert!(painted > 0, "the player moved, so something must repaint");
    assert!(
        painted < 80 * 24,
        "a small move repainted the whole screen ({painted} cells)"
    );
}

#[test]
fn a_size_mismatch_forces_a_full_repaint() {
    let mut frame = Framebuffer::new(40, 12);
    frame.fill(Cell {
        glyph: 'x',
        fg: Rgb::new(1, 2, 3),
        bg: Rgb::new(4, 5, 6),
    });
    let stale = Framebuffer::new(80, 24);

    let mut runs = Vec::new();
    compute_runs(&frame, Some(&stale), &mut runs);
    assert_eq!(painted_cells(&runs), 40 * 12);
}

#[test]
fn resizing_reallocates_and_keeps_drawing_safely() {
    let mut game = Game::test_level(viewport_tiles(80, 24));
    let mut frame = Framebuffer::new(80, 24);
    draw_scene(&mut frame, &game.snapshot());

    for (columns, rows) in [(200u16, 60u16), (24, 10), (80, 24), (1, 1), (300, 100)] {
        frame.resize(columns, rows);
        assert_eq!(frame.width(), columns);
        assert_eq!(frame.height(), rows);
        game.set_viewport(viewport_tiles(columns, rows));
        draw_scene(&mut frame, &game.snapshot());

        let mut runs = Vec::new();
        compute_runs(&frame, None, &mut runs);
        assert_eq!(painted_cells(&runs), u32::from(columns) * u32::from(rows));
    }
}

#[test]
fn a_zero_sized_terminal_does_not_panic() {
    let game = Game::test_level(viewport_tiles(1, 1));
    let mut frame = Framebuffer::new(0, 0);
    draw_scene(&mut frame, &game.snapshot());
    draw_too_small(&mut frame);

    let mut runs = Vec::new();
    compute_runs(&frame, None, &mut runs);
    assert_eq!(painted_cells(&runs), 0);
}

#[test]
fn the_too_small_notice_fits_inside_a_tiny_terminal() {
    let mut frame = Framebuffer::new(12, 3);
    draw_too_small(&mut frame);
    let mut runs = Vec::new();
    compute_runs(&frame, None, &mut runs);
    assert_eq!(painted_cells(&runs), 12 * 3);

    // Every line must be legible below the minimum playable width.
    let rendered: String = (0..frame.height())
        .map(|y| {
            (0..frame.width())
                .map(|x| frame.cell(x, y).glyph)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("|");
    assert!(
        rendered.contains("too small"),
        "notice was clipped: {rendered}"
    );
    assert!(rendered.contains("20x10"), "notice was clipped: {rendered}");
}

#[test]
fn the_scene_never_writes_outside_the_framebuffer() {
    // Camera clamping plus pixel-level clipping must hold at every size.
    for (columns, rows) in [(20u16, 10u16), (80, 24), (200, 50)] {
        let mut game = Game::test_level(viewport_tiles(columns, rows));
        let mut frame = Framebuffer::new(columns, rows);
        let right = InputState {
            move_right: true,
            ..InputState::default()
        };
        for _ in 0..2000 {
            game.step(&right, FIXED_DT);
        }
        draw_scene(&mut frame, &game.snapshot());
        assert_eq!(frame.width(), columns);
        assert_eq!(frame.height(), rows);
    }
}
