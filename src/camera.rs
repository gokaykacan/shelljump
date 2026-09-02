//! World-to-viewport transform. Knows tiles, not terminal cells.

use crate::math::Vec2;

/// A viewport onto the world, positioned by its top-left corner in tiles.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub offset: Vec2,
    /// Visible extent in world units.
    pub viewport: Vec2,
}

impl Camera {
    pub fn new(viewport: Vec2) -> Self {
        Self {
            offset: Vec2::ZERO,
            viewport: Self::sanitize(viewport),
        }
    }

    fn sanitize(viewport: Vec2) -> Vec2 {
        // A zero-sized viewport would make the follow maths meaningless; a
        // degenerate terminal still has to produce a valid camera.
        Vec2::new(viewport.x.max(1.0), viewport.y.max(1.0))
    }

    pub fn set_viewport(&mut self, viewport: Vec2) {
        self.viewport = Self::sanitize(viewport);
    }

    /// Centres horizontally on `target`, clamped to the level bounds.
    ///
    /// Vertically the level is pinned so its bottom edge meets the bottom of the
    /// viewport. Milestone-1 levels are short enough to fit, and pinning keeps
    /// the ground visible in terminals both taller and shorter than the level.
    pub fn follow(&mut self, target: Vec2, level: Vec2) {
        let max_x = (level.x - self.viewport.x).max(0.0);
        self.offset.x = (target.x - self.viewport.x * 0.5).clamp(0.0, max_x);
        self.offset.y = level.y - self.viewport.y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVEL: Vec2 = Vec2::new(100.0, 14.0);

    fn camera() -> Camera {
        Camera::new(Vec2::new(20.0, 12.0))
    }

    #[test]
    fn follows_the_target_when_away_from_the_edges() {
        let mut cam = camera();
        cam.follow(Vec2::new(50.0, 10.0), LEVEL);
        assert_eq!(cam.offset.x, 40.0);
    }

    #[test]
    fn clamps_at_the_left_edge() {
        let mut cam = camera();
        cam.follow(Vec2::new(1.0, 10.0), LEVEL);
        assert_eq!(cam.offset.x, 0.0);
    }

    #[test]
    fn clamps_at_the_right_edge() {
        let mut cam = camera();
        cam.follow(Vec2::new(99.0, 10.0), LEVEL);
        assert_eq!(cam.offset.x, 80.0);
    }

    #[test]
    fn a_viewport_wider_than_the_level_stays_at_the_origin() {
        let mut cam = Camera::new(Vec2::new(200.0, 12.0));
        cam.follow(Vec2::new(50.0, 10.0), LEVEL);
        assert_eq!(cam.offset.x, 0.0);
    }

    #[test]
    fn the_level_floor_stays_pinned_to_the_viewport_bottom() {
        let mut cam = camera();
        cam.follow(Vec2::new(50.0, 10.0), LEVEL);
        assert_eq!(cam.offset.y + cam.viewport.y, LEVEL.y);
    }

    #[test]
    fn a_degenerate_viewport_is_clamped_to_something_usable() {
        let cam = Camera::new(Vec2::new(0.0, 0.0));
        assert_eq!(cam.viewport, Vec2::new(1.0, 1.0));
    }
}
