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
    pub walk_accel: f32,
    pub ground_decel: f32,
    pub air_accel: f32,
    pub gravity: f32,
    /// Negative: upward.
    pub jump_velocity: f32,
    pub max_fall_speed: f32,
    pub coyote_time: f32,
    pub jump_buffer_time: f32,
    /// Upward velocity retained when the jump key is released early.
    pub jump_cut_multiplier: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            max_walk_speed: 8.0,
            walk_accel: 40.0,
            ground_decel: 60.0,
            air_accel: 30.0,
            gravity: 30.0,
            jump_velocity: -18.0,
            max_fall_speed: 40.0,
            coyote_time: 0.1,
            jump_buffer_time: 0.1,
            jump_cut_multiplier: 0.5,
        }
    }
}

impl PhysicsConfig {
    /// The per-axis collision sweep resolves against the tiles the body already
    /// overlaps, so it is only safe while a single step moves the body less than
    /// one tile. This must hold for every timestep the simulation ever runs at.
    pub fn is_tunnel_safe(&self, dt: f32) -> bool {
        self.max_fall_speed * dt < 1.0 && self.max_walk_speed * dt < 1.0
    }
}

fn apply_horizontal(player: &mut Player, input: &InputState, cfg: &PhysicsConfig, dt: f32) {
    let direction = i32::from(input.move_right) - i32::from(input.move_left);

    if direction == 0 {
        let decel = if player.grounded {
            cfg.ground_decel
        } else {
            cfg.air_accel
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

    // Turning around uses the (stronger) deceleration rate first, so a reversal
    // reads as a skid into the new direction rather than an instant flip.
    let reversing = player.velocity.x != 0.0 && player.velocity.x.signum() != direction;
    let accel = match (reversing, player.grounded) {
        (true, true) => cfg.ground_decel,
        (false, true) => cfg.walk_accel,
        (_, false) => cfg.air_accel,
    };

    player.velocity.x =
        (player.velocity.x + direction * accel * dt).clamp(-cfg.max_walk_speed, cfg.max_walk_speed);
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

    player.velocity.y = (player.velocity.y + cfg.gravity * dt).min(cfg.max_fall_speed);

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

    fn ground_map() -> TileMap {
        let mut map = TileMap::new(64, 64);
        for x in 0..64 {
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
