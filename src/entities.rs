//! Simulation entities. Pure data plus geometry helpers.

use crate::math::{Aabb, Vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Facing {
    Left,
    #[default]
    Right,
}

/// The player body is slightly narrower and shorter than one tile so it slips
/// cleanly through single-tile gaps and does not catch on floor seams.
pub const PLAYER_HALF_EXTENTS: Vec2 = Vec2::new(0.4, 0.45);

const _: () = assert!(PLAYER_HALF_EXTENTS.x * 2.0 < 1.0);
const _: () = assert!(PLAYER_HALF_EXTENTS.y * 2.0 < 1.0);

#[derive(Clone, Copy, Debug)]
pub struct Player {
    /// Centre of the body, in world units.
    pub position: Vec2,
    pub velocity: Vec2,
    pub half_extents: Vec2,
    pub grounded: bool,
    /// Remaining time during which a jump is still allowed after leaving ground.
    pub coyote_timer: f32,
    /// Remaining time during which a pending jump press stays queued.
    pub jump_buffer_timer: f32,
    pub facing: Facing,
}

impl Player {
    pub fn new(position: Vec2) -> Self {
        Self {
            position,
            velocity: Vec2::ZERO,
            half_extents: PLAYER_HALF_EXTENTS,
            grounded: false,
            coyote_timer: 0.0,
            jump_buffer_timer: 0.0,
            facing: Facing::Right,
        }
    }

    pub fn aabb(&self) -> Aabb {
        Aabb::from_center(self.position, self.half_extents)
    }
}
