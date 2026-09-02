//! AABB-versus-tilemap resolution. Deterministic, allocation-free, terminal-free.

use crate::math::Aabb;
use crate::world::TileMap;

/// Shrinks tile spans by a hair so a body resting exactly on a tile boundary is
/// not considered to overlap the tile it is touching. Without it, walking along
/// a floor would snag on every seam between floor tiles.
pub const SKIN: f32 = 1.0e-4;

/// Inclusive tile index range covered by the world-space interval `[lo, hi]`.
fn tile_span(lo: f32, hi: f32) -> (i32, i32) {
    let first = (lo + SKIN).floor() as i32;
    let last = (hi - SKIN).floor() as i32;
    (first, last.max(first))
}

fn column_blocked(map: &TileMap, x: i32, y0: i32, y1: i32) -> bool {
    (y0..=y1).any(|y| map.is_solid(x, y))
}

fn row_blocked(map: &TileMap, y: i32, x0: i32, x1: i32) -> bool {
    (x0..=x1).any(|x| map.is_solid(x, y))
}

/// Resolves horizontal overlap after the body has already been displaced by
/// `dx`. Returns `true` if the body was pushed back out of solid tiles.
pub fn resolve_x(map: &TileMap, body: &mut Aabb, dx: f32) -> bool {
    if dx == 0.0 {
        return false;
    }
    let (y0, y1) = tile_span(body.min.y, body.max.y);
    let (x0, x1) = tile_span(body.min.x, body.max.x);

    if dx > 0.0 {
        for x in x0..=x1 {
            if column_blocked(map, x, y0, y1) {
                body.translate_x(x as f32 - body.max.x);
                return true;
            }
        }
    } else {
        for x in (x0..=x1).rev() {
            if column_blocked(map, x, y0, y1) {
                body.translate_x((x + 1) as f32 - body.min.x);
                return true;
            }
        }
    }
    false
}

/// Resolves vertical overlap after the body has already been displaced by `dy`.
/// Returns `true` if the body was pushed back out of solid tiles.
pub fn resolve_y(map: &TileMap, body: &mut Aabb, dy: f32) -> bool {
    if dy == 0.0 {
        return false;
    }
    let (x0, x1) = tile_span(body.min.x, body.max.x);
    let (y0, y1) = tile_span(body.min.y, body.max.y);

    if dy > 0.0 {
        for y in y0..=y1 {
            if row_blocked(map, y, x0, x1) {
                body.translate_y(y as f32 - body.max.y);
                return true;
            }
        }
    } else {
        for y in (y0..=y1).rev() {
            if row_blocked(map, y, x0, x1) {
                body.translate_y((y + 1) as f32 - body.min.y);
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec2;

    fn floor_map() -> TileMap {
        TileMap::from_rows(&["....", "....", "####"])
    }

    #[test]
    fn tile_span_ignores_a_boundary_it_only_touches() {
        // A body whose bottom edge sits exactly on y = 2.0 must not be treated
        // as overlapping the tile row starting at 2.
        let (first, last) = tile_span(1.1, 2.0);
        assert_eq!((first, last), (1, 1));
    }

    #[test]
    fn falling_body_settles_exactly_on_the_tile_top() {
        let map = floor_map();
        let mut body = Aabb::from_center(Vec2::new(1.5, 2.2), Vec2::new(0.4, 0.45));
        assert!(resolve_y(&map, &mut body, 0.3));
        assert!((body.max.y - 2.0).abs() < 1e-5);
    }

    #[test]
    fn no_resolution_when_nothing_overlaps() {
        let map = floor_map();
        let mut body = Aabb::from_center(Vec2::new(1.5, 1.0), Vec2::new(0.4, 0.45));
        let before = body;
        assert!(!resolve_y(&map, &mut body, 0.1));
        assert_eq!(body, before);
    }
}
