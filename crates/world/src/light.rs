//! Per-tile light, row-major over the whole world. RGB is light from tiles and
//! lava, alpha is skylight (scaled by the time of day at draw time, so the day
//! cycle needs no recompute).
//!
//! Sources: open sky (columns down to the first block or wall), light-emitting
//! tiles and liquids. Propagation: directional sweeps (right, left, down, up)
//! where light leaving a cell is multiplied by that cell's transmission (so a
//! block facing the sky is fully lit), repeated so light can turn two corners.
//! Sweeps run in parallel over rows or bands of columns.
//!
//! Edits relight a window: the edited rect plus `REACH` tiles on each side,
//! beyond which any source has faded out.

use rayon::prelude::*;

use crate::{Chunk, Liquid, World, CHUNK, NO_TILE};

/// Transmission per tile, out of 256.
const AIR: u16 = 228;
const LIQUID: u16 = 205;
const SOLID: u16 = 140;
const ROUNDS: usize = 2;
/// 255 * (AIR / 256)^REACH < 1.
pub const REACH: u32 = 50;

/// Light a tile emits. Torch colours per style (frame_y / 22), ordered as the
/// item list of the public wiki.
pub fn emission(tile: u16, frame_y: i16) -> Option<[u8; 3]> {
    const TORCH: [[u8; 3]; 24] = [
        [255, 200, 140], // torch
        [80, 120, 255],  // blue
        [255, 60, 60],   // red
        [80, 255, 80],   // green
        [200, 80, 255],  // purple
        [255, 255, 255], // white
        [255, 255, 80],  // yellow
        [200, 80, 255],  // demon
        [160, 255, 40],  // cursed
        [140, 220, 255], // ice
        [255, 150, 40],  // orange
        [255, 230, 60],  // ichor
        [220, 240, 255], // ultrabright
        [255, 180, 120], // bone
        [255, 160, 220], // rainbow
        [255, 120, 220], // pink
        [255, 210, 130], // desert
        [80, 200, 255],  // coral
        [180, 120, 255], // corrupt
        [255, 110, 80],  // crimson
        [255, 200, 255], // hallowed
        [160, 255, 120], // jungle
        [70, 100, 255],  // mushroom
        [255, 140, 255], // aether
    ];
    Some(match tile {
        4 => TORCH[(frame_y.max(0) / 22) as usize % TORCH.len()],
        33 | 34 | 42 | 93 | 100 => [240, 210, 160], // candle, chandelier, lantern, lamp, candelabra
        35 => [255, 160, 60],                       // jack 'o lantern
        92 | 95 => [255, 200, 140],                 // lamp post, chinese lantern
        215 => [255, 170, 100],                     // campfire
        270 => [190, 255, 120],                     // firefly in a bottle
        271 => [120, 220, 255],                     // lightning bug in a bottle
        37 => [130, 70, 50],                        // meteorite
        58 => [200, 70, 30],                        // hellstone
        70 => [40, 70, 200],                        // mushroom grass
        _ => return None,
    })
}

fn liquid_emission(kind: u8) -> Option<[u8; 3]> {
    match kind {
        k if k == Liquid::Lava as u8 => Some([255, 120, 40]),
        k if k == Liquid::Honey as u8 => Some([60, 40, 0]),
        k if k == Liquid::Shimmer as u8 => Some([120, 90, 160]),
        _ => None,
    }
}

pub struct LightMap {
    pub width: u32,
    pub height: u32,
    /// RGBA8, index `y * width + x`.
    pub rgba: Vec<[u8; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl Rect {
    pub fn of_chunk(cx: u32, cy: u32, world: &World) -> Self {
        let (x0, y0) = (cx * CHUNK, cy * CHUNK);
        Self { x0, y0, x1: (x0 + CHUNK).min(world.width), y1: (y0 + CHUNK).min(world.height) }
    }

    pub fn width(&self) -> u32 {
        self.x1 - self.x0
    }
}

impl LightMap {
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        self.rgba[(y * self.width + x) as usize]
    }

    pub fn compute(world: &World) -> Self {
        let all = Rect { x0: 0, y0: 0, x1: world.width, y1: world.height };
        Self { width: world.width, height: world.height, rgba: window(world, all) }
    }

    /// Recomputes `r` after an edit inside it.
    pub fn update(&mut self, world: &World, r: Rect) {
        let win = Rect {
            x0: r.x0.saturating_sub(REACH),
            y0: r.y0.saturating_sub(REACH),
            x1: (r.x1 + REACH).min(world.width),
            y1: (r.y1 + REACH).min(world.height),
        };
        let light = window(world, win);
        let ww = win.width() as usize;
        for y in r.y0..r.y1 {
            let src = (y - win.y0) as usize * ww + (r.x0 - win.x0) as usize;
            let dst = (y * self.width + r.x0) as usize;
            self.rgba[dst..dst + r.width() as usize].copy_from_slice(&light[src..src + r.width() as usize]);
        }
    }
}

/// Light of every tile of `win`, row-major, from the sources inside it.
fn window(world: &World, win: Rect) -> Vec<[u8; 4]> {
    let (w, h) = (win.width() as usize, (win.y1 - win.y0) as usize);
    let important = &world.header.frame_important;
    let solid = |t: u16| t != NO_TILE && !important.get(t as usize).copied().unwrap_or(false);

    // Sky reaches down each column until the first block or wall.
    let sky: Vec<u32> = (win.x0..win.x1)
        .into_par_iter()
        .map(|x| {
            (0..win.y1)
                .find(|&y| {
                    let c = world.chunk(x / CHUNK, y / CHUNK);
                    let i = Chunk::index(x % CHUNK, y % CHUNK);
                    solid(c.tile[i]) || c.wall[i] != 0
                })
                .unwrap_or(win.y1)
        })
        .collect();

    let mut light = vec![[0u8; 4]; w * h];
    let mut pass = vec![0u16; w * h];
    light.par_chunks_mut(w).zip(pass.par_chunks_mut(w)).enumerate().for_each(|(row, (lrow, prow))| {
        let y = win.y0 + row as u32;
        let (cy, ly) = (y / CHUNK, y % CHUNK);
        for (col, (l, p)) in lrow.iter_mut().zip(prow.iter_mut()).enumerate() {
            let x = win.x0 + col as u32;
            let c = world.chunk(x / CHUNK, cy);
            let i = Chunk::index(x % CHUNK, ly);
            let t = c.tile[i];
            let wet = c.liquid[i] != 0;
            *p = if solid(t) {
                SOLID
            } else if wet {
                LIQUID
            } else {
                AIR
            };
            let e = if wet { liquid_emission(c.liquid_kind[i]) } else { None };
            if let Some([r, g, b]) = e.or_else(|| emission(t, c.frame_y[i])) {
                *l = [r, g, b, 0];
            }
            if y < sky[col] {
                l[3] = 255;
            }
        }
    });

    for _ in 0..ROUNDS {
        light.par_chunks_mut(w).zip(pass.par_chunks(w)).for_each(|(l, p)| {
            sweep(l.iter_mut().zip(p));
            sweep(l.iter_mut().zip(p).rev());
        });
        vertical(&mut light, &pass, w);
    }
    light
}

fn sweep<'a>(cells: impl Iterator<Item = (&'a mut [u8; 4], &'a u16)>) {
    let mut carry = [0; 4];
    for (l, &p) in cells {
        carry = step(l, carry, p);
    }
}

/// Down then up, in parallel over bands of columns.
fn vertical(light: &mut [[u8; 4]], pass: &[u16], w: usize) {
    const BAND: usize = 64;
    let mut bands: Vec<Vec<_>> = (0..w.div_ceil(BAND)).map(|_| Vec::new()).collect();
    for (l, p) in light.chunks_mut(w).zip(pass.chunks(w)) {
        for (b, lp) in bands.iter_mut().zip(l.chunks_mut(BAND).zip(p.chunks(BAND))) {
            b.push(lp);
        }
    }
    bands.into_par_iter().for_each(|mut rows| {
        let mut acc = [[0u8; 4]; BAND];
        for _ in 0..2 {
            acc.fill([0; 4]);
            for (l, p) in rows.iter_mut() {
                for ((c, a), &p) in l.iter_mut().zip(&mut acc).zip(p.iter()) {
                    *a = step(c, *a, p);
                }
            }
            rows.reverse();
        }
    });
}

/// Takes the light arriving in `cell`, returns what leaves it.
#[inline]
fn step(cell: &mut [u8; 4], arriving: [u8; 4], pass: u16) -> [u8; 4] {
    let mut out = [0; 4];
    for c in 0..4 {
        cell[c] = cell[c].max(arriving[c]);
        out[c] = ((cell[c] as u16 * pass) >> 8) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::wld::Tile;

    fn cave_world(torch: bool) -> World {
        // Ground from y = 10, a sealed cave at (20..40, 30..34) with a torch at (21, 33).
        let mut important = vec![false; 5];
        important[4] = true;
        World::parse(&formats::synth::write_world("l", 96, 96, &important, |x, y| {
            let mut t = Tile::default();
            if y >= 10 && !((20..40).contains(&x) && (30..34).contains(&y)) {
                t.tile = 1;
            }
            if torch && (x, y) == (21, 33) {
                t.tile = 4;
                t.frame_y = 0;
            }
            t
        }))
        .unwrap()
    }

    #[test]
    fn sky_torch_and_falloff() {
        let l = LightMap::compute(&cave_world(true));
        assert_eq!(l.get(5, 9), [0, 0, 0, 255]);
        assert!(l.get(5, 10)[3] > 220, "block facing the sky is lit");
        assert!(l.get(5, 14)[3] < 30, "{:?}", l.get(5, 14));
        assert_eq!(l.get(5, 40), [0; 4]);
        assert_eq!(&l.get(21, 33)[..3], &[255, 200, 140]);
        let (near, far) = (l.get(25, 33)[0], l.get(38, 30)[0]);
        assert!(near > far && far > 0, "{near} {far}");
        assert!(l.get(30, 40)[0] < far);
    }

    #[test]
    fn local_update_matches_full() {
        let mut world = cave_world(false);
        let mut l = LightMap::compute(&world);
        assert_eq!(l.get(25, 33), [0; 4]);
        world.set_tile(21, 33, 4, 0, 0);
        l.update(&world, Rect::of_chunk(0, 1, &world));
        let full = LightMap::compute(&world);
        assert_eq!(l.get(21, 33), full.get(21, 33));
        assert!(l.get(25, 33)[0] > 0);
        // Inside the reach of the edit, identical to a full recompute.
        for y in 32..64 {
            for x in 0..32 {
                assert_eq!(l.get(x, y), full.get(x, y), "{x},{y}");
            }
        }
    }
}
