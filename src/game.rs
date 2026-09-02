//! Game state: the simulation as a whole. Terminal-free and directly testable.

use crate::camera::Camera;
use crate::entities::Player;
use crate::input::InputState;
use crate::math::Vec2;
use crate::physics::{self, PhysicsConfig};
use crate::render::{PlayerView, RenderSnapshot};
use crate::world::{self, TileMap};

pub struct Game {
    pub map: TileMap,
    pub player: Player,
    pub camera: Camera,
    pub config: PhysicsConfig,
}

impl Game {
    pub fn new(map: TileMap, spawn: Vec2, viewport: Vec2) -> Self {
        let mut game = Self {
            map,
            player: Player::new(spawn),
            camera: Camera::new(viewport),
            config: PhysicsConfig::default(),
        };
        game.camera.follow(game.player.position, game.level_size());
        game
    }

    /// The milestone-1 level with its default spawn.
    pub fn test_level(viewport: Vec2) -> Self {
        Self::new(world::test_level(), world::TEST_LEVEL_SPAWN, viewport)
    }

    pub fn level_size(&self) -> Vec2 {
        Vec2::new(self.map.width() as f32, self.map.height() as f32)
    }

    /// Applies a new viewport without disturbing simulation state.
    pub fn set_viewport(&mut self, viewport: Vec2) {
        self.camera.set_viewport(viewport);
        self.camera.follow(self.player.position, self.level_size());
    }

    /// Advances the simulation by exactly one fixed step.
    pub fn step(&mut self, input: &InputState, dt: f32) {
        physics::step_player(&mut self.player, &self.map, input, &self.config, dt);
        self.camera.follow(self.player.position, self.level_size());
    }

    pub fn snapshot(&self) -> RenderSnapshot<'_> {
        RenderSnapshot {
            map: &self.map,
            camera: self.camera,
            player: PlayerView {
                body: self.player.aabb(),
                facing: self.player.facing,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::viewport_tiles;
    use crate::time::FIXED_DT;

    #[test]
    fn the_player_settles_on_the_level_floor_and_stays_put() {
        let mut game = Game::test_level(viewport_tiles(80, 24));
        let idle = InputState::default();
        for _ in 0..600 {
            game.step(&idle, FIXED_DT);
        }
        assert!(game.player.grounded);

        let resting = game.player.position;
        for _ in 0..240 {
            game.step(&idle, FIXED_DT);
            assert_eq!(
                game.player.position, resting,
                "a resting player must not drift or jitter"
            );
        }
    }

    #[test]
    fn resizing_the_viewport_leaves_the_player_untouched() {
        let mut game = Game::test_level(viewport_tiles(80, 24));
        let idle = InputState::default();
        for _ in 0..300 {
            game.step(&idle, FIXED_DT);
        }
        let before = game.player.position;
        game.set_viewport(viewport_tiles(200, 60));
        assert_eq!(game.player.position, before);
        game.set_viewport(viewport_tiles(1, 1));
        assert_eq!(game.player.position, before);
    }

    #[test]
    fn walking_right_never_leaves_the_level() {
        let mut game = Game::test_level(viewport_tiles(80, 24));
        let right = InputState {
            move_right: true,
            ..InputState::default()
        };
        for _ in 0..6000 {
            game.step(&right, FIXED_DT);
            let body = game.player.aabb();
            assert!(body.min.x >= 1.0 - 1e-3, "walked through the left wall");
            assert!(
                body.max.x <= game.map.width() as f32 - 1.0 + 1e-3,
                "walked through the right wall"
            );
            assert!(
                body.max.y <= game.map.height() as f32 - 1.0 + 1e-3,
                "fell through the bedrock"
            );
        }
    }
}
