//! Framebuffer and scene composition. Pure data: no terminal, no crossterm.
//! The crossterm writer lives in [`terminal`] and only consumes what is here.

pub mod terminal;

use crate::camera::Camera;
use crate::entities::Facing;
use crate::math::{Aabb, Vec2};
use crate::world::TileMap;

/// Sub-pixels per terminal cell. Each cell renders `▀` with the upper half in
/// the foreground colour and the lower half in the background colour, which
/// makes a sub-pixel roughly square given typical terminal cell proportions.
pub const SUBPIXELS_PER_CELL: i32 = 2;

/// Sub-pixels along each axis of one world tile.
pub const PIXELS_PER_TILE: i32 = 4;

const HALF_BLOCK: char = '\u{2580}';

/// Below this the viewport is replaced by a message instead of the game.
pub const MIN_COLUMNS: u16 = 20;
pub const MIN_ROWS: u16 = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    fn lerp(self, other: Rgb, t: f32) -> Rgb {
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb::new(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub glyph: char,
    pub fg: Rgb,
    pub bg: Rgb,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            glyph: ' ',
            fg: Rgb::new(0, 0, 0),
            bg: Rgb::new(0, 0, 0),
        }
    }
}

/// A horizontal span of cells sharing one colour pair, ready to be emitted as a
/// single positioned write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawRun {
    pub x: u16,
    pub y: u16,
    pub len: u16,
    pub fg: Rgb,
    pub bg: Rgb,
}

#[derive(Clone, Debug)]
pub struct Framebuffer {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
}

impl Framebuffer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); width as usize * height as usize],
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn pixel_height(&self) -> i32 {
        self.height as i32 * SUBPIXELS_PER_CELL
    }

    /// Reallocates to the given size. Contents are not preserved, so callers
    /// must force a full redraw afterwards.
    pub fn resize(&mut self, width: u16, height: u16) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.cells.clear();
        self.cells
            .resize(width as usize * height as usize, Cell::default());
    }

    pub fn fill(&mut self, cell: Cell) {
        self.cells.fill(cell);
    }

    pub fn cell(&self, x: u16, y: u16) -> Cell {
        self.cells[y as usize * self.width as usize + x as usize]
    }

    pub fn set_cell(&mut self, x: u16, y: u16, cell: Cell) {
        if x >= self.width || y >= self.height {
            return;
        }
        self.cells[y as usize * self.width as usize + x as usize] = cell;
    }

    /// Writes one half-block sub-pixel. Out-of-range writes are dropped, which
    /// is what lets the scene draw straight through the viewport edges.
    pub fn set_pixel(&mut self, x: i32, y: i32, color: Rgb) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.pixel_height() {
            return;
        }
        let index = (y as usize / SUBPIXELS_PER_CELL as usize) * self.width as usize + x as usize;
        let cell = &mut self.cells[index];
        cell.glyph = HALF_BLOCK;
        if y % SUBPIXELS_PER_CELL == 0 {
            cell.fg = color;
        } else {
            cell.bg = color;
        }
    }

    pub fn draw_text(&mut self, x: u16, y: u16, text: &str, fg: Rgb, bg: Rgb) {
        for (offset, glyph) in text.chars().enumerate() {
            let Ok(offset) = u16::try_from(offset) else {
                return;
            };
            let Some(column) = x.checked_add(offset) else {
                return;
            };
            if column >= self.width {
                return;
            }
            self.set_cell(column, y, Cell { glyph, fg, bg });
        }
    }

    pub fn copy_from(&mut self, other: &Framebuffer) {
        self.resize(other.width, other.height);
        self.cells.copy_from_slice(&other.cells);
    }
}

/// Collects the spans that must be repainted. `previous` of `None`, or a
/// previous buffer of a different size, produces a full redraw.
pub fn compute_runs(current: &Framebuffer, previous: Option<&Framebuffer>, out: &mut Vec<DrawRun>) {
    out.clear();
    let previous =
        previous.filter(|prev| prev.width == current.width && prev.height == current.height);

    for y in 0..current.height {
        let mut x = 0;
        while x < current.width {
            let cell = current.cell(x, y);
            let changed = previous.is_none_or(|prev| prev.cell(x, y) != cell);
            if !changed {
                x += 1;
                continue;
            }
            let start = x;
            let (fg, bg) = (cell.fg, cell.bg);
            while x < current.width {
                let next = current.cell(x, y);
                if next.fg != fg || next.bg != bg {
                    break;
                }
                if previous.is_some_and(|prev| prev.cell(x, y) == next) {
                    break;
                }
                x += 1;
            }
            out.push(DrawRun {
                x: start,
                y,
                len: x - start,
                fg,
                bg,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

const SKY_TOP: Rgb = Rgb::new(58, 126, 214);
const SKY_BOTTOM: Rgb = Rgb::new(158, 206, 246);
const GRASS: Rgb = Rgb::new(86, 186, 92);
const GRASS_SHADE: Rgb = Rgb::new(62, 150, 70);
const DIRT: Rgb = Rgb::new(146, 100, 60);
const DIRT_SHADE: Rgb = Rgb::new(116, 76, 44);
const OVERLAY_BG: Rgb = Rgb::new(16, 18, 24);
const OVERLAY_FG: Rgb = Rgb::new(224, 228, 236);

const TRANSPARENT: u8 = 0;
const PLAYER_PALETTE: [Rgb; 6] = [
    Rgb::new(0, 0, 0), // unused: index 0 is transparent
    Rgb::new(226, 68, 60),
    Rgb::new(246, 206, 162),
    Rgb::new(34, 34, 44),
    Rgb::new(62, 112, 220),
    Rgb::new(96, 62, 42),
];

/// Player sprite, drawn facing right and mirrored for the other direction.
const PLAYER_SPRITE: [[u8; 4]; 4] = [
    [0, 1, 1, 0], // cap
    [0, 2, 3, 0], // face and eye
    [4, 4, 4, 2], // torso with a forward hand
    [5, 0, 5, 0], // boots
];

// ---------------------------------------------------------------------------
// Scene composition
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct PlayerView {
    pub body: Aabb,
    pub facing: Facing,
}

/// Everything the renderer needs for one frame. Plain borrowed simulation data.
#[derive(Clone, Copy, Debug)]
pub struct RenderSnapshot<'a> {
    pub map: &'a TileMap,
    pub camera: Camera,
    pub player: PlayerView,
}

fn draw_sky(fb: &mut Framebuffer) {
    let rows = fb.pixel_height();
    let denominator = (rows - 1).max(1) as f32;
    for y in 0..rows {
        let color = SKY_TOP.lerp(SKY_BOTTOM, y as f32 / denominator);
        for x in 0..fb.width() as i32 {
            fb.set_pixel(x, y, color);
        }
    }
}

fn draw_tile(fb: &mut Framebuffer, origin_x: i32, origin_y: i32, capped: bool) {
    for row in 0..PIXELS_PER_TILE {
        for column in 0..PIXELS_PER_TILE {
            let color = if capped && row == 0 {
                if column % 2 == 0 { GRASS } else { GRASS_SHADE }
            } else if (row + column) % 3 == 0 {
                DIRT_SHADE
            } else {
                DIRT
            };
            fb.set_pixel(origin_x + column, origin_y + row, color);
        }
    }
}

fn draw_tiles(fb: &mut Framebuffer, snapshot: &RenderSnapshot, offset_px: (i32, i32)) {
    let camera = &snapshot.camera;
    let first_x = camera.offset.x.floor() as i32;
    let last_x = (camera.offset.x + camera.viewport.x).ceil() as i32;
    let first_y = camera.offset.y.floor() as i32;
    let last_y = (camera.offset.y + camera.viewport.y).ceil() as i32;

    for ty in first_y..=last_y {
        for tx in first_x..=last_x {
            if !snapshot.map.is_solid(tx, ty) {
                continue;
            }
            let capped = !snapshot.map.is_solid(tx, ty - 1);
            draw_tile(
                fb,
                tx * PIXELS_PER_TILE - offset_px.0,
                ty * PIXELS_PER_TILE - offset_px.1,
                capped,
            );
        }
    }
}

fn draw_player(fb: &mut Framebuffer, player: &PlayerView, offset_px: (i32, i32)) {
    let sprite_width = PLAYER_SPRITE[0].len() as i32;
    let sprite_height = PLAYER_SPRITE.len() as i32;

    let center_px = (player.body.center().x * PIXELS_PER_TILE as f32).round() as i32 - offset_px.0;
    let feet_px = (player.body.max.y * PIXELS_PER_TILE as f32).round() as i32 - offset_px.1;
    let origin_x = center_px - sprite_width / 2;
    let origin_y = feet_px - sprite_height;

    for (row, line) in PLAYER_SPRITE.iter().enumerate() {
        let mut line = *line;
        if player.facing == Facing::Left {
            line.reverse();
        }
        for (column, &index) in line.iter().enumerate() {
            if index == TRANSPARENT {
                continue;
            }
            fb.set_pixel(
                origin_x + column as i32,
                origin_y + row as i32,
                PLAYER_PALETTE[index as usize],
            );
        }
    }
}

/// Composes one frame into `fb`.
pub fn draw_scene(fb: &mut Framebuffer, snapshot: &RenderSnapshot) {
    // Snapping the camera to whole sub-pixels keeps tile edges from shimmering
    // as the fractional camera offset drifts.
    let offset_px = (
        (snapshot.camera.offset.x * PIXELS_PER_TILE as f32).round() as i32,
        (snapshot.camera.offset.y * PIXELS_PER_TILE as f32).round() as i32,
    );
    draw_sky(fb);
    draw_tiles(fb, snapshot, offset_px);
    draw_player(fb, &snapshot.player, offset_px);
}

/// Replaces the frame with a legible notice when the terminal cannot fit the
/// game. Simulation keeps running underneath.
pub fn draw_too_small(fb: &mut Framebuffer) {
    fb.fill(Cell {
        glyph: ' ',
        fg: OVERLAY_FG,
        bg: OVERLAY_BG,
    });
    // Kept short: this screen is shown precisely when the terminal is narrow,
    // so anything longer would be clipped exactly where it is needed.
    let lines = ["too small", "need 20x10", "q = quit"];
    let first_row = fb.height().saturating_sub(lines.len() as u16) / 2;
    for (index, line) in lines.iter().enumerate() {
        let Some(row) = first_row.checked_add(index as u16) else {
            break;
        };
        if row >= fb.height() {
            break;
        }
        let column = fb.width().saturating_sub(line.len() as u16) / 2;
        fb.draw_text(column, row, line, OVERLAY_FG, OVERLAY_BG);
    }
}

/// Viewport size in world units for a terminal of the given cell dimensions.
pub fn viewport_tiles(columns: u16, rows: u16) -> Vec2 {
    Vec2::new(
        columns as f32 / PIXELS_PER_TILE as f32,
        (rows as i32 * SUBPIXELS_PER_CELL) as f32 / PIXELS_PER_TILE as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_pixel_packs_two_sub_rows_into_one_cell() {
        let mut fb = Framebuffer::new(2, 1);
        fb.set_pixel(0, 0, Rgb::new(1, 2, 3));
        fb.set_pixel(0, 1, Rgb::new(4, 5, 6));
        let cell = fb.cell(0, 0);
        assert_eq!(cell.glyph, HALF_BLOCK);
        assert_eq!(cell.fg, Rgb::new(1, 2, 3));
        assert_eq!(cell.bg, Rgb::new(4, 5, 6));
    }

    #[test]
    fn out_of_range_pixels_are_dropped() {
        let mut fb = Framebuffer::new(2, 1);
        fb.set_pixel(-1, 0, Rgb::new(9, 9, 9));
        fb.set_pixel(0, -1, Rgb::new(9, 9, 9));
        fb.set_pixel(2, 0, Rgb::new(9, 9, 9));
        fb.set_pixel(0, 2, Rgb::new(9, 9, 9));
        assert_eq!(fb.cell(0, 0), Cell::default());
        assert_eq!(fb.cell(1, 0), Cell::default());
    }

    #[test]
    fn draw_text_clips_at_the_right_edge() {
        let mut fb = Framebuffer::new(3, 1);
        fb.draw_text(1, 0, "abc", OVERLAY_FG, OVERLAY_BG);
        assert_eq!(fb.cell(1, 0).glyph, 'a');
        assert_eq!(fb.cell(2, 0).glyph, 'b');
    }

    #[test]
    fn viewport_tiles_converts_cells_to_world_units() {
        let viewport = viewport_tiles(80, 24);
        assert_eq!(viewport.x, 20.0);
        assert_eq!(viewport.y, 12.0);
    }

    #[test]
    fn a_full_redraw_covers_every_cell() {
        let fb = Framebuffer::new(4, 2);
        let mut runs = Vec::new();
        compute_runs(&fb, None, &mut runs);
        let covered: u16 = runs.iter().map(|run| run.len).sum();
        assert_eq!(covered, 8);
    }

    #[test]
    fn runs_break_where_colours_change() {
        let mut fb = Framebuffer::new(4, 1);
        fb.set_cell(
            2,
            0,
            Cell {
                glyph: 'x',
                fg: Rgb::new(1, 1, 1),
                bg: Rgb::new(2, 2, 2),
            },
        );
        let mut runs = Vec::new();
        compute_runs(&fb, None, &mut runs);
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].len, 2);
        assert_eq!(runs[1].len, 1);
        assert_eq!(runs[2].len, 1);
    }
}
