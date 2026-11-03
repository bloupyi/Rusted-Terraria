//! World storage: 32x32 chunks, struct of arrays, column-major inside a chunk.
//!
//! 32x32: a u16 field is 2 KiB (fits L1 next to its neighbours), the .wld column
//! order writes contiguously, and ~20k chunks on a large map is enough work units
//! for rayon without per-chunk overhead mattering.

use std::path::Path;

use formats::wld::{self, Tile};
use rayon::prelude::*;

pub mod body;
pub mod frame;
pub mod light;
pub mod liquid;
pub mod tile_light;
pub mod mob;
pub mod player;
pub mod tile_sets;

pub use formats::wld::{flags, Liquid, NO_TILE};

pub const CHUNK: u32 = 32;
pub const CHUNK_TILES: usize = (CHUNK * CHUNK) as usize;

pub struct Chunk {
    pub tile: [u16; CHUNK_TILES],
    pub frame_x: [i16; CHUNK_TILES],
    pub frame_y: [i16; CHUNK_TILES],
    pub wall: [u16; CHUNK_TILES],
    pub liquid: [u8; CHUNK_TILES],
    pub liquid_kind: [u8; CHUNK_TILES],
    pub tile_paint: [u8; CHUNK_TILES],
    pub wall_paint: [u8; CHUNK_TILES],
    pub flags: [u16; CHUNK_TILES],
}

impl Chunk {
    fn empty() -> Box<Self> {
        Box::new(Self {
            tile: [NO_TILE; CHUNK_TILES],
            frame_x: [-1; CHUNK_TILES],
            frame_y: [-1; CHUNK_TILES],
            wall: [0; CHUNK_TILES],
            liquid: [0; CHUNK_TILES],
            liquid_kind: [0; CHUNK_TILES],
            tile_paint: [0; CHUNK_TILES],
            wall_paint: [0; CHUNK_TILES],
            flags: [0; CHUNK_TILES],
        })
    }

    #[inline]
    pub fn index(lx: u32, ly: u32) -> usize {
        (lx * CHUNK + ly) as usize
    }

    fn fill(&mut self, i: usize, n: usize, t: &Tile) {
        let r = i..i + n;
        self.tile[r.clone()].fill(t.tile);
        self.frame_x[r.clone()].fill(t.frame_x);
        self.frame_y[r.clone()].fill(t.frame_y);
        self.wall[r.clone()].fill(t.wall);
        self.liquid[r.clone()].fill(t.liquid);
        self.liquid_kind[r.clone()].fill(t.liquid_kind as u8);
        self.tile_paint[r.clone()].fill(t.tile_paint);
        self.wall_paint[r.clone()].fill(t.wall_paint);
        self.flags[r].fill(t.flags);
    }
}

pub struct World {
    pub header: wld::Header,
    pub width: u32,
    pub height: u32,
    pub chunks_x: u32,
    pub chunks_y: u32,
    /// Row-major: `cy * chunks_x + cx`.
    pub chunks: Vec<Box<Chunk>>,
    /// Per chunk, `DIRTY_*` bits set by edits since the last `take_dirty`.
    dirty: Vec<u8>,
    dirty_list: Vec<u32>,
}

/// Blocks or walls changed: the chunk's tile mesh and light need rebuilding.
pub const DIRTY_TILES: u8 = 1;
/// Only liquids changed.
pub const DIRTY_LIQUID: u8 = 2;

impl World {
    pub fn load(path: &Path) -> Result<Self, formats::Error> {
        Self::parse(&std::fs::read(path)?)
    }

    pub fn parse(data: &[u8]) -> Result<Self, formats::Error> {
        let header = wld::Header::parse(data)?;
        let (width, height) = (header.width, header.height);
        let chunks_x = width.div_ceil(CHUNK);
        let chunks_y = height.div_ceil(CHUNK);
        let mut chunks: Vec<Box<Chunk>> = (0..chunks_x * chunks_y).into_par_iter().map(|_| Chunk::empty()).collect();
        wld::read_tiles(data, &header, |x, y, n, t| {
            let (cx, lx) = (x / CHUNK, x % CHUNK);
            let (mut y, end) = (y, y + n);
            while y < end {
                let (cy, ly) = (y / CHUNK, y % CHUNK);
                let take = (CHUNK - ly).min(end - y);
                chunks[(cy * chunks_x + cx) as usize].fill(Chunk::index(lx, ly), take as usize, t);
                y += take;
            }
        })?;
        let dirty = vec![0; chunks.len()];
        Ok(Self { header, width, height, chunks_x, chunks_y, chunks, dirty, dirty_list: Vec::new() })
    }

    #[inline]
    pub fn chunk(&self, cx: u32, cy: u32) -> &Chunk {
        &self.chunks[(cy * self.chunks_x + cx) as usize]
    }

    /// Out of bounds reads as empty.
    #[inline]
    pub fn tile(&self, x: i64, y: i64) -> u16 {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return NO_TILE;
        }
        let (x, y) = (x as u32, y as u32);
        self.chunk(x / CHUNK, y / CHUNK).tile[Chunk::index(x % CHUNK, y % CHUNK)]
    }

    #[inline]
    pub fn wall(&self, x: i64, y: i64) -> u16 {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return 0;
        }
        let (x, y) = (x as u32, y as u32);
        self.chunk(x / CHUNK, y / CHUNK).wall[Chunk::index(x % CHUNK, y % CHUNK)]
    }

    #[inline]
    pub fn liquid(&self, x: i64, y: i64) -> u8 {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return 0;
        }
        let (x, y) = (x as u32, y as u32);
        self.chunk(x / CHUNK, y / CHUNK).liquid[Chunk::index(x % CHUNK, y % CHUNK)]
    }

    /// Wiring, actuator, paint-visibility and shape bits (`flags::*`); 0 out of bounds.
    #[inline]
    pub fn flags(&self, x: i64, y: i64) -> u16 {
        if !self.in_bounds(x, y) {
            return 0;
        }
        let (c, i) = self.cell(x as u32, y as u32);
        c.flags[i]
    }

    /// Stored frame of a frame-important tile; out of bounds reads as `(-1, -1)`.
    pub fn frame(&self, x: i64, y: i64) -> (i16, i16) {
        if !self.in_bounds(x, y) {
            return (-1, -1);
        }
        let (c, i) = self.cell(x as u32, y as u32);
        (c.frame_x[i], c.frame_y[i])
    }

    #[inline]
    pub fn in_bounds(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && x < self.width as i64 && y < self.height as i64
    }

    /// Chunk and index of an in-bounds tile.
    #[inline]
    pub fn cell(&self, x: u32, y: u32) -> (&Chunk, usize) {
        (self.chunk(x / CHUNK, y / CHUNK), Chunk::index(x % CHUNK, y % CHUNK))
    }

    #[inline]
    fn cell_mut(&mut self, x: u32, y: u32) -> (&mut Chunk, usize) {
        let ci = ((y / CHUNK) * self.chunks_x + x / CHUNK) as usize;
        (&mut self.chunks[ci], Chunk::index(x % CHUNK, y % CHUNK))
    }

    fn mark(&mut self, x: u32, y: u32, bits: u8) {
        let ci = (y / CHUNK) * self.chunks_x + x / CHUNK;
        let d = &mut self.dirty[ci as usize];
        if *d == 0 {
            self.dirty_list.push(ci);
        }
        *d |= bits;
    }

    /// Block framing reads all eight neighbours, so edits on a chunk edge dirty the next chunks too.
    fn mark_around(&mut self, x: u32, y: u32, bits: u8) {
        for (dx, dy) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let (nx, ny) = (x as i64 + dx, y as i64 + dy);
            if self.in_bounds(nx, ny) {
                self.mark(nx as u32, ny as u32, bits);
            }
        }
    }

    pub fn set_tile(&mut self, x: u32, y: u32, tile: u16, frame_x: i16, frame_y: i16) {
        let (c, i) = self.cell_mut(x, y);
        (c.tile[i], c.frame_x[i], c.frame_y[i]) = (tile, frame_x, frame_y);
        self.mark_around(x, y, DIRTY_TILES);
    }

    pub fn set_wall(&mut self, x: u32, y: u32, wall: u16) {
        let (c, i) = self.cell_mut(x, y);
        c.wall[i] = wall;
        self.mark_around(x, y, DIRTY_TILES);
    }

    pub fn set_liquid(&mut self, x: u32, y: u32, amount: u8, kind: u8) {
        let (c, i) = self.cell_mut(x, y);
        (c.liquid[i], c.liquid_kind[i]) = (amount, if amount == 0 { 0 } else { kind });
        // The liquid mesh reads the cell above to fill full cells.
        self.mark(x, y, DIRTY_LIQUID);
        if y + 1 < self.height {
            self.mark(x, y + 1, DIRTY_LIQUID);
        }
    }

    /// Chunks edited since the last call, as `(cx, cy, DIRTY_* bits)`.
    pub fn take_dirty(&mut self) -> Vec<(u32, u32, u8)> {
        let list = std::mem::take(&mut self.dirty_list);
        list.into_iter()
            .map(|ci| {
                let bits = std::mem::take(&mut self.dirty[ci as usize]);
                (ci % self.chunks_x, ci / self.chunks_x, bits)
            })
            .collect()
    }

    /// Solid for collisions, liquids and light: a tile that is not frame-important.
    #[inline]
    pub fn solid(&self, x: i64, y: i64) -> bool {
        let t = self.tile(x, y);
        t != NO_TILE && !self.header.frame_important.get(t as usize).copied().unwrap_or(false)
    }

    /// The world as a minimal `.wld` that [`World::parse`] reads back: tiles,
    /// walls, liquids, paint and flags, no other world data.
    pub fn to_wld(&self) -> Vec<u8> {
        formats::synth::write_world(&self.header.name, self.width, self.height, &self.header.frame_important, |x, y| {
            self.cell_data(x, y)
        })
    }

    /// Everything stored for an in-bounds tile.
    pub fn cell_data(&self, x: u32, y: u32) -> formats::wld::Tile {
        let (c, i) = self.cell(x, y);
        formats::wld::Tile {
            tile: c.tile[i],
            frame_x: c.frame_x[i],
            frame_y: c.frame_y[i],
            tile_paint: c.tile_paint[i],
            wall: c.wall[i],
            wall_paint: c.wall_paint[i],
            liquid: c.liquid[i],
            liquid_kind: [Liquid::Water, Liquid::Lava, Liquid::Honey, Liquid::Shimmer][c.liquid_kind[i] as usize & 3],
            flags: c.flags[i],
        }
    }

    /// Replaces an in-bounds tile whole.
    pub fn set_cell(&mut self, x: u32, y: u32, t: &formats::wld::Tile) {
        self.put_cell(x, y, t);
        self.mark_around(x, y, DIRTY_TILES | DIRTY_LIQUID);
    }

    /// [`World::set_cell`] without marking chunks, for filling a world before its scene is built.
    pub fn put_cell(&mut self, x: u32, y: u32, t: &formats::wld::Tile) {
        let (c, i) = self.cell_mut(x, y);
        (c.tile[i], c.frame_x[i], c.frame_y[i], c.tile_paint[i]) = (t.tile, t.frame_x, t.frame_y, t.tile_paint);
        (c.wall[i], c.wall_paint[i], c.flags[i]) = (t.wall, t.wall_paint, t.flags);
        (c.liquid[i], c.liquid_kind[i]) = (t.liquid, if t.liquid == 0 { 0 } else { t.liquid_kind as u8 });
    }

    /// Distinct tile and wall types present, for loading only the sheets needed.
    pub fn used_types(&self) -> (Vec<u16>, Vec<u16>) {
        let (tiles, walls) = self
            .chunks
            .par_iter()
            .fold(
                || (vec![false; 1 << 16], vec![false; 1 << 16]),
                |(mut t, mut w), c| {
                    for i in 0..CHUNK_TILES {
                        t[c.tile[i] as usize] = true;
                        w[c.wall[i] as usize] = true;
                    }
                    (t, w)
                },
            )
            .reduce(
                || (vec![false; 1 << 16], vec![false; 1 << 16]),
                |(mut a, mut b), (c, d)| {
                    a.iter_mut().zip(c).for_each(|(x, y)| *x |= y);
                    b.iter_mut().zip(d).for_each(|(x, y)| *x |= y);
                    (a, b)
                },
            );
        let pick = |v: Vec<bool>, skip: u16| {
            v.iter().enumerate().filter(|&(i, &b)| b && i as u16 != skip).map(|(i, _)| i as u16).collect()
        };
        (pick(tiles, NO_TILE), pick(walls, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_storage() {
        let (w, h) = (70, 100);
        let f = |x: u32, y: u32| Tile {
            tile: if (x + y).is_multiple_of(3) || y > 40 { (x % 5) as u16 } else { NO_TILE },
            wall: (y / 10) as u16,
            ..Tile::default()
        };
        let world = World::parse(&formats::synth::write_world("t", w, h, &[], f)).unwrap();
        assert_eq!((world.chunks_x, world.chunks_y), (3, 4));
        for x in 0..w {
            for y in 0..h {
                assert_eq!(world.tile(x as i64, y as i64), f(x, y).tile, "{x},{y}");
                assert_eq!(world.wall(x as i64, y as i64), f(x, y).wall);
            }
        }
        assert_eq!(world.tile(-1, 0), NO_TILE);
        let (tiles, walls) = world.used_types();
        assert_eq!(tiles, vec![0, 1, 2, 3, 4]);
        assert_eq!(walls, (1..10).collect::<Vec<_>>());
    }

    #[test]
    fn round_trips_through_wld() {
        let mut world = World::parse(&formats::synth::write_world("r", 40, 40, &[false, true], |x, y| Tile {
            tile: if y > 30 {
                0
            } else if y == 30 && x == 3 {
                1
            } else {
                NO_TILE
            },
            frame_x: if y == 30 && x == 3 { 18 } else { -1 },
            frame_y: if y == 30 && x == 3 { 36 } else { -1 },
            ..Tile::default()
        }))
        .unwrap();
        world.set_liquid(5, 29, 200, Liquid::Lava as u8);
        world.set_wall(6, 6, 4);
        let back = World::parse(&world.to_wld()).unwrap();
        assert_eq!(back.tile(3, 30), 1);
        assert_eq!(back.frame(3, 30), (18, 36));
        assert_eq!(back.tile(0, 35), 0);
        assert_eq!((back.liquid(5, 29), back.cell(5, 29).0.liquid_kind[back.cell(5, 29).1]), (200, 1));
        assert_eq!(back.wall(6, 6), 4);
    }
}
