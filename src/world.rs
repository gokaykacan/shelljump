//! Tile map and the hardcoded milestone-1 level.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tile {
    #[default]
    Empty,
    Solid,
}

impl Tile {
    pub fn is_solid(self) -> bool {
        matches!(self, Tile::Solid)
    }
}

#[derive(Clone, Debug)]
pub struct TileMap {
    width: usize,
    height: usize,
    tiles: Vec<Tile>,
}

impl TileMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            tiles: vec![Tile::Empty; width * height],
        }
    }

    /// Builds a map from ASCII rows: `#` is solid, anything else is empty.
    ///
    /// # Panics
    /// Panics if `rows` is empty or the rows are not all the same length.
    pub fn from_rows(rows: &[&str]) -> Self {
        assert!(!rows.is_empty(), "tile map needs at least one row");
        let width = rows[0].chars().count();
        assert!(width > 0, "tile map needs at least one column");
        let mut tiles = Vec::with_capacity(width * rows.len());
        for (y, row) in rows.iter().enumerate() {
            let mut count = 0;
            for ch in row.chars() {
                tiles.push(if ch == '#' { Tile::Solid } else { Tile::Empty });
                count += 1;
            }
            assert_eq!(count, width, "tile map row {y} has an inconsistent width");
        }
        Self {
            width,
            height: rows.len(),
            tiles,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn set(&mut self, x: usize, y: usize, tile: Tile) {
        self.tiles[y * self.width + x] = tile;
    }

    /// Tile lookup with out-of-bounds semantics: everything outside the map is
    /// solid except the open sky above it, so the player is fenced in without
    /// needing an explicit kill plane.
    pub fn tile(&self, x: i32, y: i32) -> Tile {
        if y < 0 {
            return Tile::Empty;
        }
        if x < 0 || y >= self.height as i32 || x >= self.width as i32 {
            return Tile::Solid;
        }
        self.tiles[y as usize * self.width + x as usize]
    }

    pub fn is_solid(&self, x: i32, y: i32) -> bool {
        self.tile(x, y).is_solid()
    }
}

/// Rows of the milestone-1 level. Bottom row is unbroken bedrock and both edges
/// are walled, so the player cannot leave the world.
const TEST_LEVEL_ROWS: &[&str] = &[
    "#..................................................................................................#",
    "#..................................................................................................#",
    "#..................................................................................................#",
    "#..................................................................................................#",
    "#..................................................................................................#",
    "#..................................................................................................#",
    "#...........#####.........................................#####....................................#",
    "#.......................#####.......................................................#####..........#",
    "#...................................................####...........................................#",
    "#...........#####...................####............................#####.................####.....#",
    "#.............................................#####................................................#",
    "#.............................####.....................###................###......................#",
    "##################....##################.....#################...#############.....#################",
    "####################################################################################################",
];

pub fn test_level() -> TileMap {
    TileMap::from_rows(TEST_LEVEL_ROWS)
}

/// Player spawn for [`test_level`], as a body centre in world units.
pub const TEST_LEVEL_SPAWN: crate::math::Vec2 = crate::math::Vec2::new(3.5, 9.0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_rows_map_to_tiles() {
        let map = TileMap::from_rows(&["..#", "#.."]);
        assert_eq!(map.width(), 3);
        assert_eq!(map.height(), 2);
        assert!(map.is_solid(2, 0));
        assert!(!map.is_solid(0, 0));
        assert!(map.is_solid(0, 1));
    }

    #[test]
    fn outside_the_map_is_solid_except_overhead() {
        let map = TileMap::from_rows(&["...", "..."]);
        assert!(!map.is_solid(1, -1), "sky above the map must be passable");
        assert!(!map.is_solid(1, -50));
        assert!(map.is_solid(-1, 0));
        assert!(map.is_solid(3, 0));
        assert!(map.is_solid(1, 2));
    }

    #[test]
    fn test_level_is_rectangular_and_sealed() {
        let map = test_level();
        assert_eq!(map.width(), 100);
        assert_eq!(map.height(), 14);
        let bottom = map.height() as i32 - 1;
        for x in 0..map.width() as i32 {
            assert!(map.is_solid(x, bottom), "bedrock missing at column {x}");
        }
        for y in 0..map.height() as i32 {
            assert!(map.is_solid(0, y), "left wall missing at row {y}");
            assert!(
                map.is_solid(map.width() as i32 - 1, y),
                "right wall missing at row {y}"
            );
        }
    }

    #[test]
    fn test_level_has_a_pit_with_bedrock_beneath_it() {
        let map = test_level();
        assert!(!map.is_solid(19, 12), "expected an open pit at column 19");
        assert!(map.is_solid(19, 13), "pit must still have a floor below it");
    }

    #[test]
    fn spawn_point_is_in_open_air() {
        let map = test_level();
        let spawn = TEST_LEVEL_SPAWN;
        assert!(!map.is_solid(spawn.x as i32, spawn.y as i32));
    }
}
