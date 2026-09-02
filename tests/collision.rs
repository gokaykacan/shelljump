//! Headless collision behaviour. No terminal is initialised anywhere here.

use shelljump::entities::Player;
use shelljump::input::InputState;
use shelljump::math::Vec2;
use shelljump::physics::{PhysicsConfig, step_player};
use shelljump::time::FIXED_DT;
use shelljump::world::TileMap;

/// A 12x10 room: solid border, open interior.
fn room() -> TileMap {
    TileMap::from_rows(&[
        "############",
        "#..........#",
        "#..........#",
        "#..........#",
        "#..........#",
        "#..........#",
        "#..........#",
        "#..........#",
        "#..........#",
        "############",
    ])
}

/// A room low enough that a full-height jump reaches the ceiling.
fn low_room() -> TileMap {
    TileMap::from_rows(&[
        "############",
        "#..........#",
        "#..........#",
        "#..........#",
        "############",
    ])
}

fn idle() -> InputState {
    InputState::default()
}

fn held(left: bool, right: bool) -> InputState {
    InputState {
        move_left: left,
        move_right: right,
        ..InputState::default()
    }
}

fn held_run(left: bool, right: bool) -> InputState {
    InputState {
        move_left: left,
        move_right: right,
        run_held: true,
        ..InputState::default()
    }
}

fn run(player: &mut Player, map: &TileMap, input: &InputState, steps: u32) {
    let config = PhysicsConfig::default();
    for _ in 0..steps {
        step_player(player, map, input, &config, FIXED_DT);
    }
}

#[test]
fn landing_grounds_the_player_exactly_on_the_tile_top() {
    let map = room();
    let mut player = Player::new(Vec2::new(6.0, 2.0));
    run(&mut player, &map, &idle(), 400);

    assert!(player.grounded);
    assert_eq!(player.velocity.y, 0.0);
    // Floor tiles start at row 9, so the feet must settle exactly on y = 9.
    assert!((player.aabb().max.y - 9.0).abs() < 1e-4);
}

#[test]
fn a_resting_player_does_not_jitter() {
    let map = room();
    let mut player = Player::new(Vec2::new(6.0, 2.0));
    run(&mut player, &map, &idle(), 400);
    let resting = player.position;

    for _ in 0..1000 {
        run(&mut player, &map, &idle(), 1);
        assert_eq!(player.position, resting);
        assert!(player.grounded);
    }
}

#[test]
fn a_ceiling_bump_stops_the_rise_but_keeps_horizontal_momentum() {
    let map = low_room();
    let mut player = Player::new(Vec2::new(3.0, 2.0));
    run(&mut player, &map, &idle(), 200);
    assert!(player.grounded);

    let jump = InputState {
        jump_pressed: true,
        jump_held: true,
        move_right: true,
        ..InputState::default()
    };
    step_player(
        &mut player,
        &map,
        &jump,
        &PhysicsConfig::default(),
        FIXED_DT,
    );
    assert!(player.velocity.y < 0.0);

    let holding = held(false, true);
    let mut bumped = false;
    for _ in 0..400 {
        run(&mut player, &map, &holding, 1);
        if player.velocity.y == 0.0 && player.aabb().min.y <= 1.0 + 1e-3 {
            bumped = true;
            break;
        }
    }
    assert!(bumped, "the player should have hit the ceiling");
    // Ceiling tiles end at row 1, so the head stops exactly there.
    assert!((player.aabb().min.y - 1.0).abs() < 1e-4);
    assert!(
        player.velocity.x > 0.0,
        "a ceiling bump must not kill horizontal speed"
    );
}

#[test]
fn walking_into_a_wall_stops_at_the_tile_boundary() {
    let map = room();
    let mut player = Player::new(Vec2::new(6.0, 2.0));
    run(&mut player, &map, &idle(), 400);

    run(&mut player, &map, &held(false, true), 2000);
    assert_eq!(player.velocity.x, 0.0);
    // The right wall starts at column 11.
    assert!((player.aabb().max.x - 11.0).abs() < 1e-4);

    run(&mut player, &map, &held(true, false), 2000);
    assert_eq!(player.velocity.x, 0.0);
    assert!((player.aabb().min.x - 1.0).abs() < 1e-4);
}

#[test]
fn a_body_at_terminal_velocity_does_not_tunnel_through_a_single_tile_floor() {
    let config = PhysicsConfig::default();
    assert!(
        config.is_tunnel_safe(FIXED_DT),
        "the tunnelling guard must hold for the shipped tuning"
    );

    // One-tile-thick platform with open space above and below it.
    let map = TileMap::from_rows(&[
        "......", "......", "......", "......", "######", "......", "......", "......",
    ]);

    let mut player = Player::new(Vec2::new(3.0, 0.5));
    for _ in 0..600 {
        // Force the worst case every step: full terminal velocity downward.
        player.velocity.y = config.max_fall_speed;
        step_player(&mut player, &map, &idle(), &config, FIXED_DT);
        if player.grounded {
            break;
        }
    }

    assert!(player.grounded, "the fall should have been stopped");
    assert!(
        (player.aabb().max.y - 4.0).abs() < 1e-4,
        "landed at {} instead of the platform top",
        player.aabb().max.y
    );
}

#[test]
fn the_player_fits_through_a_one_tile_gap() {
    let map = TileMap::from_rows(&["#####", "#...#", "##.##", "#...#", "#...#", "#####"]);
    let mut player = Player::new(Vec2::new(2.5, 1.5));
    run(&mut player, &map, &idle(), 400);

    assert!(player.grounded);
    assert!(
        (player.aabb().max.y - 5.0).abs() < 1e-4,
        "the player should have fallen through the gap to the floor"
    );
}

#[test]
fn walking_along_a_tiled_floor_never_snags_on_a_seam() {
    let map = room();
    let mut player = Player::new(Vec2::new(2.0, 2.0));
    run(&mut player, &map, &idle(), 400);

    let holding = held(false, true);
    let mut stalled_before_the_wall = false;
    for _ in 0..1200 {
        run(&mut player, &map, &holding, 1);
        if player.velocity.x == 0.0 && player.aabb().max.x < 10.5 {
            stalled_before_the_wall = true;
            break;
        }
    }
    assert!(!stalled_before_the_wall, "caught on a floor seam");
}

#[test]
fn running_into_a_wall_stops_at_the_tile_boundary() {
    let map = room();
    let mut player = Player::new(Vec2::new(6.0, 2.0));
    run(&mut player, &map, &idle(), 400);

    run(&mut player, &map, &held_run(false, true), 2000);
    assert_eq!(player.velocity.x, 0.0);
    assert!((player.aabb().max.x - 11.0).abs() < 1e-4);

    run(&mut player, &map, &held_run(true, false), 2000);
    assert_eq!(player.velocity.x, 0.0);
    assert!((player.aabb().min.x - 1.0).abs() < 1e-4);
}

#[test]
fn a_ceiling_bump_at_run_speed_keeps_horizontal_momentum() {
    let map = low_room();
    let mut player = Player::new(Vec2::new(3.0, 2.0));
    run(&mut player, &map, &idle(), 200);
    assert!(player.grounded);

    let jump = InputState {
        jump_pressed: true,
        jump_held: true,
        move_right: true,
        run_held: true,
        ..InputState::default()
    };
    step_player(
        &mut player,
        &map,
        &jump,
        &PhysicsConfig::default(),
        FIXED_DT,
    );
    assert!(player.velocity.y < 0.0);

    let holding = held_run(false, true);
    let mut bumped = false;
    for _ in 0..400 {
        run(&mut player, &map, &holding, 1);
        if player.velocity.y == 0.0 && player.aabb().min.y <= 1.0 + 1e-3 {
            bumped = true;
            break;
        }
    }
    assert!(bumped, "the player should have hit the ceiling");
    assert!((player.aabb().min.y - 1.0).abs() < 1e-4);
    assert!(
        player.velocity.x > 0.0,
        "a ceiling bump must not kill horizontal speed"
    );
}

#[test]
fn running_along_a_tiled_floor_never_snags_on_a_seam() {
    let map = room();
    let mut player = Player::new(Vec2::new(2.0, 2.0));
    run(&mut player, &map, &idle(), 400);

    let holding = held_run(false, true);
    let mut stalled_before_the_wall = false;
    for _ in 0..1200 {
        run(&mut player, &map, &holding, 1);
        if player.velocity.x == 0.0 && player.aabb().max.x < 10.5 {
            stalled_before_the_wall = true;
            break;
        }
    }
    assert!(
        !stalled_before_the_wall,
        "caught on a floor seam at run speed"
    );
}

#[test]
fn a_body_at_run_speed_does_not_tunnel_through_a_single_tile_wall() {
    let config = PhysicsConfig::default();
    assert!(config.is_tunnel_safe(FIXED_DT));

    // One-tile-thick pillar standing on the floor.
    let map = TileMap::from_rows(&[
        "..........",
        "..........",
        ".....#....",
        ".....#....",
        "##########",
    ]);

    let mut player = Player::new(Vec2::new(1.5, 3.0));
    run(&mut player, &map, &idle(), 200);
    assert!(player.grounded);

    let holding = held_run(false, true);
    let mut stopped = false;
    for _ in 0..400 {
        // Force the worst case every step: full run speed into the pillar.
        player.velocity.x = config.max_run_speed;
        step_player(&mut player, &map, &holding, &config, FIXED_DT);
        if player.velocity.x == 0.0 {
            stopped = true;
            break;
        }
    }

    assert!(stopped, "the run should have been stopped by the pillar");
    assert!(
        (player.aabb().max.x - 5.0).abs() < 1e-4,
        "stopped at {} instead of the pillar face",
        player.aabb().max.x
    );
}
