//! Small geometry primitives shared by the simulation. Terminal-free.

/// A 2D vector in world units (tiles). Y increases downward.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Axis-aligned bounding box in world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    pub fn from_center(center: Vec2, half_extents: Vec2) -> Self {
        Self {
            min: Vec2::new(center.x - half_extents.x, center.y - half_extents.y),
            max: Vec2::new(center.x + half_extents.x, center.y + half_extents.y),
        }
    }

    pub fn center(&self) -> Vec2 {
        Vec2::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
        )
    }

    pub fn translate_x(&mut self, delta: f32) {
        self.min.x += delta;
        self.max.x += delta;
    }

    pub fn translate_y(&mut self, delta: f32) {
        self.min.y += delta;
        self.max.y += delta;
    }
}

/// Moves `value` toward `target` by at most `max_delta`.
pub fn move_toward(value: f32, target: f32, max_delta: f32) -> f32 {
    let diff = target - value;
    if diff.abs() <= max_delta {
        target
    } else {
        value + max_delta.copysign(diff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_round_trips_through_its_center() {
        let center = Vec2::new(3.25, -1.5);
        let half = Vec2::new(0.4, 0.45);
        let aabb = Aabb::from_center(center, half);
        assert!((aabb.center().x - center.x).abs() < 1e-6);
        assert!((aabb.center().y - center.y).abs() < 1e-6);
    }

    #[test]
    fn move_toward_snaps_without_overshooting() {
        assert_eq!(move_toward(0.0, 5.0, 10.0), 5.0);
        assert_eq!(move_toward(0.0, -5.0, 10.0), -5.0);
        assert_eq!(move_toward(0.0, 5.0, 2.0), 2.0);
        assert_eq!(move_toward(0.0, -5.0, 2.0), -2.0);
    }
}
