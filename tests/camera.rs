//! Camera framing. Headless.

use shelljump::camera::Camera;
use shelljump::game::Game;
use shelljump::input::InputState;
use shelljump::math::Vec2;
use shelljump::render::viewport_tiles;
use shelljump::time::FIXED_DT;
use shelljump::world::{Tile, TileMap};

const LEVEL: Vec2 = Vec2::new(100.0, 14.0);

/// A walled corridor with an unbroken floor, so a player holding "right" can
/// traverse the entire level without needing to jump.
fn flat_level() -> TileMap {
    let mut map = TileMap::new(100, 14);
    for x in 0..100 {
        map.set(x, 13, Tile::Solid);
    }
    for y in 0..14 {
        map.set(0, y, Tile::Solid);
        map.set(99, y, Tile::Solid);
    }
    map
}

#[test]
fn the_camera_centres_on_the_target_in_open_ground() {
    let mut camera = Camera::new(Vec2::new(20.0, 12.0));
    let target = Vec2::new(50.0, 10.0);
    camera.follow(target, LEVEL);
    let target_in_view = target.x - camera.offset.x;
    assert!((target_in_view - camera.viewport.x * 0.5).abs() < 1e-5);
}

#[test]
fn the_camera_never_shows_world_left_of_the_origin() {
    let mut camera = Camera::new(Vec2::new(20.0, 12.0));
    for x in 0..30 {
        camera.follow(Vec2::new(x as f32, 10.0), LEVEL);
        assert!(camera.offset.x >= 0.0, "camera scrolled past the left edge");
    }
}

#[test]
fn the_camera_never_shows_world_past_the_right_edge() {
    let mut camera = Camera::new(Vec2::new(20.0, 12.0));
    for x in 70..120 {
        camera.follow(Vec2::new(x as f32, 10.0), LEVEL);
        assert!(
            camera.offset.x + camera.viewport.x <= LEVEL.x + 1e-5,
            "camera scrolled past the right edge"
        );
    }
}

#[test]
fn the_ground_stays_visible_in_terminals_of_every_height() {
    for rows in [10u16, 24, 60, 200] {
        let mut camera = Camera::new(viewport_tiles(80, rows));
        camera.follow(Vec2::new(50.0, 12.0), LEVEL);
        let floor_in_view = LEVEL.y - camera.offset.y;
        assert!(
            (floor_in_view - camera.viewport.y).abs() < 1e-4,
            "the level floor should sit on the viewport bottom at {rows} rows"
        );
    }
}

#[test]
fn the_camera_tracks_the_player_across_the_whole_level() {
    let mut game = Game::new(flat_level(), Vec2::new(3.0, 11.0), viewport_tiles(80, 24));
    let right = InputState {
        move_right: true,
        ..InputState::default()
    };

    let mut max_offset: f32 = 0.0;
    for _ in 0..8000 {
        game.step(&right, FIXED_DT);
        let offset = game.camera.offset.x;
        assert!(offset >= 0.0);
        assert!(offset + game.camera.viewport.x <= game.level_size().x + 1e-4);
        max_offset = max_offset.max(offset);
    }

    let expected_max = game.level_size().x - game.camera.viewport.x;
    assert!(
        (max_offset - expected_max).abs() < 1e-3,
        "camera should reach the right edge clamp, got {max_offset}"
    );
}

#[test]
fn a_viewport_wider_than_the_level_pins_to_the_origin() {
    let mut camera = Camera::new(Vec2::new(400.0, 12.0));
    camera.follow(Vec2::new(50.0, 10.0), LEVEL);
    assert_eq!(camera.offset.x, 0.0);
}
