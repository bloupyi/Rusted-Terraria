//! Per-tile light, RGB8, row-major over the whole world.
//!
//! Sources: open sky (columns down to the first block or wall), light-emitting
//! tiles and lava. Propagation: directional sweeps (right, left, down, up)
//! where light leaving a cell is multiplied by that cell's transmission (so a
//! block facing the sky is fully lit), repeated so light can turn two corners.
//! Sweeps run in parallel over rows or bands of columns.

use rayon::prelude::*;

use crate::{Chunk, Liquid, World, CHUNK, NO_TILE};

/// Transmission per tile, out of 256.
const AIR: u16 = 228;
const LIQUID: u16 = 205;
const SOLID: u16 = 140;
const ROUNDS: usize = 2;
const SKY: [u8; 4] = [255, 255, 255, 255];

/// TODO: incomplete table, single colour per type (torch styles, gems and
/// glowing blocks not handled).
fn emission(tile: u16) -> Option<[u8; 3]> {
    Some(match tile {
        4 => [255, 200, 140],                       // torch
        33 | 34 | 42 | 93 | 100 => [240, 210, 160], // candle, chandelier, lantern, lamp, candelabra
        215 => [255, 170, 100],                     // campfire
        _ => return None,
    })
}

const LAVA: [u8; 3] = [255, 120, 40];

pub struct LightMap {
    pub width: u32,
    pub height: u32,
    /// RGBA8, alpha unused (255), index `y * width + x`.
    pub rgba: Vec<[u8; 4]>,
}

impl LightMap {
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        self.rgba[(y * self.width + x) as usize]
    }
}

pub fn compute(world: &World) -> LightMap {
    let (w, h) = (world.width as usize, world.height as usize);
    let important = &world.header.frame_important;
    let solid = |t: u16| t != NO_TILE && !important.get(t as usize).copied().unwrap_or(false);

    // Sky reaches down each column until the first block or wall.
    let sky: Vec<u32> = (0..world.width)
        .into_par_iter()
        .map(|x| {
            (0..world.height)
                .find(|&y| {
                    let c = world.chunk(x / CHUNK, y / CHUNK);
                    let i = Chunk::index(x % CHUNK, y % CHUNK);
                    solid(c.tile[i]) || c.wall[i] != 0
                })
                .unwrap_or(world.height)
        })
        .collect();

    let mut light = vec![[0, 0, 0, 255]; w * h];
    let mut pass = vec![0u16; w * h];
    light.par_chunks_mut(w).zip(pass.par_chunks_mut(w)).enumerate().for_each(|(y, (lrow, prow))| {
        let (cy, ly) = (y as u32 / CHUNK, y as u32 % CHUNK);
        for (x, (l, p)) in lrow.iter_mut().zip(prow.iter_mut()).enumerate() {
            let (cx, lx) = (x as u32 / CHUNK, x as u32 % CHUNK);
            let c = world.chunk(cx, cy);
            let i = Chunk::index(lx, ly);
            let t = c.tile[i];
            *p = if solid(t) {
                SOLID
            } else if c.liquid[i] != 0 {
                LIQUID
            } else {
                AIR
            };
            let e = if c.liquid[i] != 0 && c.liquid_kind[i] == Liquid::Lava as u8 { Some(LAVA) } else { emission(t) };
            if (y as u32) < sky[x] {
                *l = SKY;
            } else if let Some([r, g, b]) = e {
                *l = [r, g, b, 255];
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
    LightMap { width: world.width, height: world.height, rgba: light }
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
    for c in 0..3 {
        cell[c] = cell[c].max(arriving[c]);
        out[c] = ((cell[c] as u16 * pass) >> 8) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::wld::Tile;

    #[test]
    fn sky_torch_and_falloff() {
        // Ground from y = 10, a sealed cave at (20..40, 30..34) with a torch at (21, 33).
        let (w, h) = (64, 64);
        let mut important = vec![false; 5];
        important[4] = true;
        let world = World::parse(&formats::synth::write_world("l", w, h, &important, |x, y| {
            let mut t = Tile::default();
            if y >= 10 && !((20..40).contains(&x) && (30..34).contains(&y)) {
                t.tile = 1;
            }
            if (x, y) == (21, 33) {
                t.tile = 4;
            }
            t
        }))
        .unwrap();
        let l = compute(&world);
        assert_eq!(l.get(5, 9), SKY);
        assert!(l.get(5, 10)[0] > 220, "block facing the sky is lit");
        assert!(l.get(5, 14)[0] < 30, "{:?}", l.get(5, 14));
        assert_eq!(l.get(5, 40)[0], 0);
        assert_eq!(&l.get(21, 33)[..3], &[255, 200, 140]);
        let (near, far) = (l.get(25, 33)[0], l.get(38, 30)[0]);
        assert!(near > far && far > 0, "{near} {far}");
        assert!(l.get(30, 40)[0] < far);
    }
}
