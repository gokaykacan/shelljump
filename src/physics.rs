//! Player movement integration. Deterministic, fixed-timestep, terminal-free.

use crate::collision;
use crate::entities::{Facing, Player};
use crate::input::InputState;
use crate::math::move_toward;
use crate::world::TileMap;

/// Every tunable that shapes how the player feels to control.
///
/// Units are tiles and seconds, with Y increasing downward.
#[derive(Clone, Copy, Debug)]
pub struct PhysicsConfig {
    pub max_walk_speed: f32,
    /// Top speed while the run action is held.
    pub max_run_speed: f32,
    pub walk_accel: f32,
    /// Ground acceleration while the run action is held.
    pub run_accel: f32,
    pub ground_decel: f32,
    pub air_accel: f32,
    /// Airborne deceleration with no direction held. Far weaker than
    /// [`PhysicsConfig::ground_decel`] so horizontal momentum carries through a
    /// jump instead of evaporating mid-arc.
    pub air_decel: f32,
    pub gravity: f32,
    /// Negative: upward.
    pub jump_velocity: f32,
    pub max_fall_speed: f32,
    pub coyote_time: f32,
    pub jump_buffer_time: f32,
    /// Upward velocity retained when the jump key is released early.
    pub jump_cut_multiplier: f32,
    /// Width of the rising-speed band below zero in which gravity is softened.
    pub apex_threshold: f32,
    /// Gravity scale applied inside that band.
    pub apex_gravity_multiplier: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            max_walk_speed: 8.0,
            max_run_speed: 13.0,
            walk_accel: 40.0,
            run_accel: 55.0,
            ground_decel: 60.0,
            air_accel: 30.0,
            air_decel: 4.0,
            gravity: 30.0,
            jump_velocity: -18.0,
            max_fall_speed: 40.0,
            coyote_time: 0.1,
            jump_buffer_time: 0.1,
            jump_cut_multiplier: 0.5,
            apex_threshold: 3.0,
            apex_gravity_multiplier: 0.55,
        }
    }
}

impl PhysicsConfig {
    /// The per-axis collision sweep resolves against the tiles the body already
    /// overlaps, so it is only safe while a single step moves the body less than
    /// one tile. This must hold for every timestep the simulation ever runs at.
    pub fn is_tunnel_safe(&self, dt: f32) -> bool {
        let max_horizontal = self.max_walk_speed.max(self.max_run_speed);
        self.max_fall_speed * dt < 1.0 && max_horizontal * dt < 1.0
    }
}

fn apply_horizontal(player: &mut Player, input: &InputState, cfg: &PhysicsConfig, dt: f32) {
    let direction = i32::from(input.move_right) - i32::from(input.move_left);

    if direction == 0 {
        let decel = if player.grounded {
            cfg.ground_decel
        } else {
            cfg.air_decel
        };
        player.velocity.x = move_toward(player.velocity.x, 0.0, decel * dt);
        return;
    }

    let direction = direction as f32;
    player.facing = if direction > 0.0 {
        Facing::Right
    } else {
        Facing::Left
    };

    // Turning around on the ground brakes to a stop first, so a reversal reads as
    // a skid into the new direction rather than an instant flip. The brake rate
    // is deliberately independent of the run action.
    let reversing = player.velocity.x != 0.0 && player.velocity.x.signum() != direction;
    if reversing && player.grounded {
        player.velocity.x = move_toward(player.velocity.x, 0.0, cfg.ground_decel * dt);
        return;
    }

    // Rate-limited approach to a signed target speed rather than an add-then-clamp:
    // dropping the cap (releasing run mid-flight) then decays momentum smoothly
    // instead of snapping it down.
    // The cap is deliberately independent of `grounded`: holding run after leaving
    // the ground at walk speed unlocks the run cap mid-air (at the air rate). That
    // is intended platformer feel, not a missing `grounded` check.
    let cap = if input.run_held {
        cfg.max_run_speed
    } else {
        cfg.max_walk_speed
    };
    let accel = if !player.grounded {
        cfg.air_accel
    } else if input.run_held {
        cfg.run_accel
    } else {
        cfg.walk_accel
    };

    player.velocity.x = move_toward(player.velocity.x, direction * cap, accel * dt);
}

fn move_and_collide(player: &mut Player, map: &TileMap, dt: f32) {
    let dx = player.velocity.x * dt;
    player.position.x += dx;
    let mut body = player.aabb();
    if collision::resolve_x(map, &mut body, dx) {
        player.position.x = body.center().x;
        player.velocity.x = 0.0;
    }

    let dy = player.velocity.y * dt;
    player.position.y += dy;
    let mut body = player.aabb();
    if collision::resolve_y(map, &mut body, dy) {
        player.position.y = body.center().y;
        // Landing grounds the player; a ceiling bump only kills upward speed and
        // leaves horizontal momentum untouched.
        player.grounded = dy > 0.0;
        player.velocity.y = 0.0;
    } else {
        player.grounded = false;
    }
}

/// Advances the player by exactly one fixed step.
pub fn step_player(
    player: &mut Player,
    map: &TileMap,
    input: &InputState,
    cfg: &PhysicsConfig,
    dt: f32,
) {
    if player.grounded {
        player.coyote_timer = cfg.coyote_time;
    } else {
        player.coyote_timer = (player.coyote_timer - dt).max(0.0);
    }

    if input.jump_pressed {
        player.jump_buffer_timer = cfg.jump_buffer_time;
    } else {
        player.jump_buffer_timer = (player.jump_buffer_timer - dt).max(0.0);
    }

    apply_horizontal(player, input, cfg, dt);

    if player.jump_buffer_timer > 0.0 && player.coyote_timer > 0.0 {
        player.velocity.y = cfg.jump_velocity;
        player.grounded = false;
        player.coyote_timer = 0.0;
        player.jump_buffer_timer = 0.0;
    }

    if input.jump_released && player.velocity.y < 0.0 {
        player.velocity.y *= cfg.jump_cut_multiplier;
    }

    // Hang time: gravity is softened in a narrow band of slow upward speed near
    // the top of an arc. The bound is strictly negative on both sides, so a body
    // at rest or already falling never enters it and free fall is untouched.
    let gravity =
        if !player.grounded && player.velocity.y < 0.0 && player.velocity.y > -cfg.apex_threshold {
            cfg.gravity * cfg.apex_gravity_multiplier
        } else {
            cfg.gravity
        };

    player.velocity.y = (player.velocity.y + gravity * dt).min(cfg.max_fall_speed);

    move_and_collide(player, map, dt);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec2;
    use crate::time::FIXED_DT;

    fn open_map() -> TileMap {
        TileMap::new(64, 64)
    }

    /// Wide enough that several seconds at the run cap never reach the
    /// out-of-bounds wall that fences the map in.
    fn ground_map() -> TileMap {
        let mut map = TileMap::new(256, 64);
        for x in 0..256 {
            map.set(x, 20, crate::world::Tile::Solid);
        }
        map
    }

    fn grounded_player() -> Player {
        let mut player = Player::new(Vec2::new(8.0, 20.0 - 0.45));
        player.grounded = true;
        player
    }

    fn idle() -> InputState {
        InputState::default()
    }

    fn jump_press() -> InputState {
        InputState {
            jump_pressed: true,
            jump_held: true,
            ..InputState::default()
        }
    }

    fn walk_right() -> InputState {
        InputState {
            move_right: true,
            ..InputState::default()
        }
    }

    fn run_right() -> InputState {
        InputState {
            move_right: true,
            run_held: true,
            ..InputState::default()
        }
    }

    #[test]
    fn default_config_is_tunnel_safe_at_the_fixed_timestep() {
        assert!(PhysicsConfig::default().is_tunnel_safe(FIXED_DT));
    }

    #[test]
    fn gravity_accumulates_while_airborne() {
        let cfg = PhysicsConfig::default();
        let map = open_map();
        let mut player = Player::new(Vec2::new(8.0, 4.0));
        for _ in 0..10 {
            step_player(&mut player, &map, &idle(), &cfg, FIXED_DT);
        }
        let expected = cfg.gravity * FIXED_DT * 10.0;
        assert!((player.velocity.y - expected).abs() < 1e-3);
    }

    #[test]
    fn falling_speed_is_clamped_to_terminal_velocity() {
        let cfg = PhysicsConfig::default();
        let map = open_map();
        let mut player = Player::new(Vec2::new(8.0, 0.0));
        for _ in 0..2000 {
            player.position.y = 0.0;
            step_player(&mut player, &map, &idle(), &cfg, FIXED_DT);
        }
        assert!((player.velocity.y - cfg.max_fall_speed).abs() < 1e-3);
    }

    #[test]
    fn a_grounded_jump_launches_upward() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut player = grounded_player();
        step_player(&mut player, &map, &jump_press(), &cfg, FIXED_DT);
        assert!(!player.grounded);
        assert!(player.velocity.y < 0.0);
        assert!((player.velocity.y - (cfg.jump_velocity + cfg.gravity * FIXED_DT)).abs() < 1e-4);
    }

    #[test]
    fn releasing_jump_early_cuts_the_rise() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();

        let mut held = grounded_player();
        let mut cut = grounded_player();
        step_player(&mut held, &map, &jump_press(), &cfg, FIXED_DT);
        step_player(&mut cut, &map, &jump_press(), &cfg, FIXED_DT);

        let release = InputState {
            jump_released: true,
            ..InputState::default()
        };
        let hold = InputState {
            jump_held: true,
            ..InputState::default()
        };
        step_player(&mut cut, &map, &release, &cfg, FIXED_DT);
        step_player(&mut held, &map, &hold, &cfg, FIXED_DT);

        assert!(
            cut.velocity.y > held.velocity.y,
            "cut jump must rise slower"
        );
        // Cutting when already falling must not accelerate the descent.
        let mut falling = Player::new(Vec2::new(8.0, 4.0));
        falling.velocity.y = 5.0;
        step_player(&mut falling, &map, &release, &cfg, FIXED_DT);
        assert!(falling.velocity.y > 5.0);
    }

    #[test]
    fn jump_cut_only_applies_on_the_release_edge() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut player = grounded_player();
        step_player(&mut player, &map, &jump_press(), &cfg, FIXED_DT);
        let after_launch = player.velocity.y;

        // A frame where jump is simply not held must not cut the jump.
        step_player(&mut player, &map, &idle(), &cfg, FIXED_DT);
        assert!(player.velocity.y > after_launch);
        assert!((player.velocity.y - (after_launch + cfg.gravity * FIXED_DT)).abs() < 1e-4);
    }

    #[test]
    fn coyote_time_allows_a_jump_just_after_walking_off() {
        let cfg = PhysicsConfig::default();
        let map = open_map();
        let mut player = Player::new(Vec2::new(8.0, 4.0));
        player.grounded = true;
        player.coyote_timer = cfg.coyote_time;

        // Fall for slightly less than the coyote window, then jump.
        let steps = ((cfg.coyote_time / FIXED_DT) as u32) - 1;
        for _ in 0..steps {
            step_player(&mut player, &map, &idle(), &cfg, FIXED_DT);
        }
        assert!(player.coyote_timer > 0.0);
        step_player(&mut player, &map, &jump_press(), &cfg, FIXED_DT);
        assert!(
            player.velocity.y < 0.0,
            "jump inside coyote window must fire"
        );
    }

    #[test]
    fn coyote_time_expires_just_outside_the_window() {
        let cfg = PhysicsConfig::default();
        let map = open_map();
        let mut player = Player::new(Vec2::new(8.0, 4.0));
        player.grounded = true;
        player.coyote_timer = cfg.coyote_time;

        let steps = ((cfg.coyote_time / FIXED_DT) as u32) + 2;
        for _ in 0..steps {
            step_player(&mut player, &map, &idle(), &cfg, FIXED_DT);
        }
        assert_eq!(player.coyote_timer, 0.0);
        step_player(&mut player, &map, &jump_press(), &cfg, FIXED_DT);
        assert!(
            player.velocity.y > 0.0,
            "jump outside coyote window must not fire"
        );
    }

    #[test]
    fn a_buffered_jump_fires_on_landing() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        // Start just above the floor so landing happens within the buffer window.
        let mut player = Player::new(Vec2::new(8.0, 20.0 - 0.45 - 0.05));

        step_player(&mut player, &map, &jump_press(), &cfg, FIXED_DT);
        assert!(player.jump_buffer_timer > 0.0);

        let hold = InputState {
            jump_held: true,
            ..InputState::default()
        };
        let mut jumped = false;
        for _ in 0..12 {
            step_player(&mut player, &map, &hold, &cfg, FIXED_DT);
            if player.velocity.y < 0.0 {
                jumped = true;
                break;
            }
        }
        assert!(jumped, "the buffered press should fire on touchdown");
    }

    #[test]
    fn a_jump_buffered_too_early_expires() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut player = Player::new(Vec2::new(8.0, 4.0));

        step_player(&mut player, &map, &jump_press(), &cfg, FIXED_DT);
        let steps = ((cfg.jump_buffer_time / FIXED_DT) as u32) + 2;
        let hold = InputState {
            jump_held: true,
            ..InputState::default()
        };
        for _ in 0..steps {
            step_player(&mut player, &map, &hold, &cfg, FIXED_DT);
        }
        assert_eq!(player.jump_buffer_timer, 0.0);

        // Now land: nothing should be queued any more.
        for _ in 0..600 {
            step_player(&mut player, &map, &hold, &cfg, FIXED_DT);
            if player.grounded {
                break;
            }
        }
        assert!(player.grounded);
        assert!(
            player.velocity.y >= 0.0,
            "an expired buffer must not launch a jump"
        );
    }

    #[test]
    fn walking_accelerates_up_to_the_speed_cap() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut player = grounded_player();
        let right = InputState {
            move_right: true,
            ..InputState::default()
        };
        step_player(&mut player, &map, &right, &cfg, FIXED_DT);
        assert!((player.velocity.x - cfg.walk_accel * FIXED_DT).abs() < 1e-4);
        assert_eq!(player.facing, Facing::Right);

        for _ in 0..600 {
            step_player(&mut player, &map, &right, &cfg, FIXED_DT);
        }
        assert!((player.velocity.x - cfg.max_walk_speed).abs() < 1e-3);
    }

    #[test]
    fn releasing_direction_decelerates_to_a_stop() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut player = grounded_player();
        player.velocity.x = cfg.max_walk_speed;

        step_player(&mut player, &map, &idle(), &cfg, FIXED_DT);
        assert!(player.velocity.x < cfg.max_walk_speed);
        assert!(player.velocity.x > 0.0, "stopping must not be instant");

        for _ in 0..600 {
            step_player(&mut player, &map, &idle(), &cfg, FIXED_DT);
        }
        assert_eq!(player.velocity.x, 0.0);
    }

    #[test]
    fn reversing_direction_skids_instead_of_flipping() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut player = grounded_player();
        player.velocity.x = cfg.max_walk_speed;

        let left = InputState {
            move_left: true,
            ..InputState::default()
        };
        step_player(&mut player, &map, &left, &cfg, FIXED_DT);
        assert!(
            player.velocity.x > 0.0,
            "momentum must survive the first reversal step"
        );
        assert_eq!(player.facing, Facing::Left, "facing flips immediately");

        let mut crossed_zero = false;
        for _ in 0..600 {
            step_player(&mut player, &map, &left, &cfg, FIXED_DT);
            if player.velocity.x < 0.0 {
                crossed_zero = true;
                break;
            }
        }
        assert!(
            crossed_zero,
            "reversal must eventually build speed the other way"
        );
    }

    #[test]
    fn the_run_cap_is_tunnel_safe_at_the_fixed_timestep() {
        let cfg = PhysicsConfig::default();
        assert!(
            cfg.max_run_speed * FIXED_DT < 1.0,
            "running is now the horizontal worst case for the collision sweep"
        );
        assert!(cfg.is_tunnel_safe(FIXED_DT));
    }

    #[test]
    fn running_accelerates_up_to_the_run_cap() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut player = grounded_player();
        step_player(&mut player, &map, &run_right(), &cfg, FIXED_DT);
        assert!((player.velocity.x - cfg.run_accel * FIXED_DT).abs() < 1e-4);

        for _ in 0..600 {
            step_player(&mut player, &map, &run_right(), &cfg, FIXED_DT);
        }
        assert!((player.velocity.x - cfg.max_run_speed).abs() < 1e-3);
    }

    #[test]
    fn running_reaches_a_higher_top_speed_than_walking() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut walking = grounded_player();
        let mut running = grounded_player();
        for _ in 0..600 {
            step_player(&mut walking, &map, &walk_right(), &cfg, FIXED_DT);
            step_player(&mut running, &map, &run_right(), &cfg, FIXED_DT);
        }
        assert!(running.velocity.x > walking.velocity.x);
        assert!(
            walking.velocity.x <= cfg.max_walk_speed + 1e-4,
            "walking must never exceed the walk cap"
        );
        assert!((running.velocity.x - cfg.max_run_speed).abs() < 1e-3);
    }

    #[test]
    fn releasing_run_in_flight_decays_smoothly_toward_the_walk_cap() {
        let cfg = PhysicsConfig::default();
        let map = open_map();
        let mut player = Player::new(Vec2::new(8.0, 4.0));
        player.velocity.x = cfg.max_run_speed;

        let mut previous = player.velocity.x;
        let mut reached_cap = false;
        for _ in 0..64 {
            step_player(&mut player, &map, &walk_right(), &cfg, FIXED_DT);
            let drop = previous - player.velocity.x;
            assert!(drop > 0.0, "decay must be monotonic");
            assert!(
                drop <= cfg.air_accel * FIXED_DT + 1e-6,
                "velocity snapped by {drop} instead of decaying at the air rate"
            );
            previous = player.velocity.x;
            if (player.velocity.x - cfg.max_walk_speed).abs() < 1e-6 {
                reached_cap = true;
                break;
            }
        }
        assert!(reached_cap, "the decay never settled on the walk cap");

        step_player(&mut player, &map, &walk_right(), &cfg, FIXED_DT);
        assert!((player.velocity.x - cfg.max_walk_speed).abs() < 1e-6);
    }

    #[test]
    fn releasing_run_on_the_ground_decays_smoothly_toward_the_walk_cap() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut player = grounded_player();
        player.velocity.x = cfg.max_run_speed;

        let mut previous = player.velocity.x;
        let mut reached_cap = false;
        for _ in 0..64 {
            step_player(&mut player, &map, &walk_right(), &cfg, FIXED_DT);
            let drop = previous - player.velocity.x;
            assert!(drop > 0.0, "decay must be monotonic");
            assert!(
                drop <= cfg.walk_accel * FIXED_DT + 1e-6,
                "velocity snapped by {drop} instead of decaying at the walk rate"
            );
            previous = player.velocity.x;
            if (player.velocity.x - cfg.max_walk_speed).abs() < 1e-6 {
                reached_cap = true;
                break;
            }
        }
        assert!(reached_cap, "the decay never settled on the walk cap");

        step_player(&mut player, &map, &walk_right(), &cfg, FIXED_DT);
        assert!((player.velocity.x - cfg.max_walk_speed).abs() < 1e-6);
    }

    /// Deliberate: the speed cap ignores `grounded`, so run speed is reachable in
    /// mid-air even when the player never held run while on the ground.
    #[test]
    fn holding_run_in_flight_unlocks_the_run_cap_without_running_on_the_ground() {
        let cfg = PhysicsConfig::default();
        let map = open_map();
        let mut player = Player::new(Vec2::new(8.0, 4.0));
        player.velocity.x = cfg.max_walk_speed;

        step_player(&mut player, &map, &run_right(), &cfg, FIXED_DT);
        assert!(
            (player.velocity.x - (cfg.max_walk_speed + cfg.air_accel * FIXED_DT)).abs() < 1e-4,
            "mid-air run must climb past the walk cap at the air rate"
        );

        // Bounded so the player neither falls out of the open map nor drifts into
        // its out-of-bounds wall before the cap is reached.
        for _ in 0..32 {
            step_player(&mut player, &map, &run_right(), &cfg, FIXED_DT);
        }
        assert!((player.velocity.x - cfg.max_run_speed).abs() < 1e-3);
    }

    #[test]
    fn tapping_run_below_the_walk_cap_does_not_jolt_the_velocity() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut player = grounded_player();
        step_player(&mut player, &map, &walk_right(), &cfg, FIXED_DT);

        let before = player.velocity.x;
        step_player(&mut player, &map, &run_right(), &cfg, FIXED_DT);
        assert!((player.velocity.x - (before + cfg.run_accel * FIXED_DT)).abs() < 1e-4);

        let tapped = player.velocity.x;
        step_player(&mut player, &map, &walk_right(), &cfg, FIXED_DT);
        assert!((player.velocity.x - (tapped + cfg.walk_accel * FIXED_DT)).abs() < 1e-4);
    }

    #[test]
    fn holding_run_without_a_direction_does_not_change_deceleration() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut plain = grounded_player();
        let mut with_run = grounded_player();
        plain.velocity.x = cfg.max_walk_speed;
        with_run.velocity.x = cfg.max_walk_speed;

        let run_only = InputState {
            run_held: true,
            ..InputState::default()
        };
        for _ in 0..40 {
            step_player(&mut plain, &map, &idle(), &cfg, FIXED_DT);
            step_player(&mut with_run, &map, &run_only, &cfg, FIXED_DT);
            assert_eq!(plain.velocity.x, with_run.velocity.x);
        }
    }

    #[test]
    fn reversing_at_run_speed_skids_instead_of_flipping() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut player = grounded_player();
        player.velocity.x = cfg.max_run_speed;

        let left = InputState {
            move_left: true,
            run_held: true,
            ..InputState::default()
        };
        step_player(&mut player, &map, &left, &cfg, FIXED_DT);
        assert!(
            (player.velocity.x - (cfg.max_run_speed - cfg.ground_decel * FIXED_DT)).abs() < 1e-4,
            "the brake rate must not depend on the run action"
        );
        assert_eq!(player.facing, Facing::Left, "facing flips immediately");

        let mut crossed_zero = false;
        for _ in 0..600 {
            step_player(&mut player, &map, &left, &cfg, FIXED_DT);
            if player.velocity.x < 0.0 {
                crossed_zero = true;
                break;
            }
        }
        assert!(
            crossed_zero,
            "reversal must eventually build speed the other way"
        );
    }

    #[test]
    fn acceleration_ranks_air_below_walk_below_run() {
        let cfg = PhysicsConfig::default();
        let ground = ground_map();

        // The airborne body holds run too: run must not strengthen air control.
        let mut airborne = Player::new(Vec2::new(8.0, 4.0));
        step_player(&mut airborne, &open_map(), &run_right(), &cfg, FIXED_DT);

        let mut walking = grounded_player();
        step_player(&mut walking, &ground, &walk_right(), &cfg, FIXED_DT);

        let mut running = grounded_player();
        step_player(&mut running, &ground, &run_right(), &cfg, FIXED_DT);

        assert!(airborne.velocity.x < walking.velocity.x);
        assert!(walking.velocity.x < running.velocity.x);
    }

    #[test]
    fn gravity_is_softened_inside_the_apex_band() {
        let cfg = PhysicsConfig::default();
        let map = open_map();
        let mut player = Player::new(Vec2::new(8.0, 4.0));
        player.velocity.y = -cfg.apex_threshold * 0.5;

        let before = player.velocity.y;
        step_player(&mut player, &map, &idle(), &cfg, FIXED_DT);
        let expected = before + cfg.gravity * cfg.apex_gravity_multiplier * FIXED_DT;
        assert!((player.velocity.y - expected).abs() < 1e-4);
    }

    #[test]
    fn gravity_is_full_outside_the_apex_band() {
        let cfg = PhysicsConfig::default();
        let map = open_map();

        // Rising fast, well below the band; at rest on its upper edge; and
        // exactly on its lower edge. None of these may be softened.
        for start in [cfg.jump_velocity, 0.0, -cfg.apex_threshold] {
            let mut player = Player::new(Vec2::new(8.0, 4.0));
            player.velocity.y = start;
            step_player(&mut player, &map, &idle(), &cfg, FIXED_DT);
            assert!(
                (player.velocity.y - (start + cfg.gravity * FIXED_DT)).abs() < 1e-4,
                "gravity was softened at velocity.y = {start}"
            );
        }
    }

    #[test]
    fn a_cut_jump_still_hangs_at_its_lower_peak() {
        let cfg = PhysicsConfig::default();
        let map = ground_map();
        let mut player = grounded_player();
        step_player(&mut player, &map, &jump_press(), &cfg, FIXED_DT);

        let release = InputState {
            jump_released: true,
            ..InputState::default()
        };
        step_player(&mut player, &map, &release, &cfg, FIXED_DT);

        let mut softened_steps = 0;
        for _ in 0..600 {
            let before = player.velocity.y;
            step_player(&mut player, &map, &idle(), &cfg, FIXED_DT);
            let gained = player.velocity.y - before;
            assert!(player.velocity.y.is_finite());
            if before < 0.0 && before > -cfg.apex_threshold {
                assert!(
                    (gained - cfg.gravity * cfg.apex_gravity_multiplier * FIXED_DT).abs() < 1e-4
                );
                softened_steps += 1;
            }
            if player.grounded {
                break;
            }
        }
        assert!(
            softened_steps > 0,
            "the cut arc never passed through the apex band"
        );
    }

    #[test]
    fn airborne_idle_deceleration_is_gentler_than_on_the_ground() {
        let cfg = PhysicsConfig::default();
        assert!(
            cfg.air_decel < cfg.ground_decel,
            "letting go mid-air must not brake as hard as skidding on the ground"
        );
    }

    #[test]
    fn releasing_direction_mid_air_carries_momentum_through_the_jump() {
        let cfg = PhysicsConfig::default();
        let map = open_map();
        let mut player = Player::new(Vec2::new(8.0, 4.0));
        player.velocity.x = cfg.max_walk_speed;

        // 0.6s is a realistic slice of one jump's airtime. Before the air_decel
        // split this reused air_accel (30.0) and hit exactly zero in ~0.267s.
        let steps = (0.6 / FIXED_DT) as u32;
        let mut previous = player.velocity.x;
        for _ in 0..steps {
            step_player(&mut player, &map, &idle(), &cfg, FIXED_DT);
            assert!(!player.grounded, "the test must stay in the air");
            assert!(
                player.velocity.x < previous,
                "airborne decay must stay monotonic"
            );
            previous = player.velocity.x;
        }

        assert!(
            player.velocity.x >= cfg.max_walk_speed * 0.65,
            "expected most of the entry speed to survive, kept {}",
            player.velocity.x / cfg.max_walk_speed
        );
    }

    #[test]
    fn air_control_is_weaker_than_ground_control() {
        let cfg = PhysicsConfig::default();
        let map = open_map();
        let right = InputState {
            move_right: true,
            ..InputState::default()
        };

        let mut airborne = Player::new(Vec2::new(8.0, 4.0));
        step_player(&mut airborne, &map, &right, &cfg, FIXED_DT);

        let ground = ground_map();
        let mut grounded = grounded_player();
        step_player(&mut grounded, &ground, &right, &cfg, FIXED_DT);

        assert!(airborne.velocity.x < grounded.velocity.x);
    }
}
