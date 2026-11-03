//! World storage: 32x32 chunks, struct of arrays, column-major inside a chunk.
//!
//! 32x32: a u16 field is 2 KiB (fits L1 next to its neighbours), the .wld column
//! order writes contiguously, and ~20k chunks on a large map is enough work units
//! for rayon without per-chunk overhead mattering.

use std::path::Path;

use formats::wld::{self, Tile};
use rayon::prelude::*;

pub mod light;

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
}

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
        Ok(Self { header, width, height, chunks_x, chunks_y, chunks })
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
}
