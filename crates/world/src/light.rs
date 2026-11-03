//! Per-tile light, row-major over the whole world, following the game's
//! light map. RGB is light from tiles, liquids and the underworld, alpha is
//! sunlight (scaled by the sky colour at draw time, so the day cycle needs no
//! recompute). Values are stored at `ONE` per unit, so sources brighter than
//! 1 (white torches...) reach as far as in the game.
//!
//! Sources: above the surface line, every cell whose wall lets light through
//! and whose block does not stop sunlight; light-emitting tiles (generated
//! table, `tile_light`) and liquids; open cells of the underworld.
//! Propagation as in the game: per column down and up, then per row right
//! and left, twice; light leaving a cell is multiplied by that cell's decay
//! (air, block, water, honey) and stops below the game's cut-off.
//!
//! Edits relight a window: the edited rect plus `REACH` tiles on each side,
//! beyond which any source has faded out.

use rayon::prelude::*;

use crate::tile_light::{RULES, TORCH};
use crate::tile_sets::{BLOCK_LIGHT, CRACKED_BRICKS, NO_SUN_LIGHT, WALL_LIGHT};
use crate::{flags, Chunk, Liquid, World, CHUNK, NO_TILE};

/// Stored value of a light of 1.
pub const ONE: f32 = 128.0;
/// The game stops carrying light below 0.0185.
const CUTOFF: u16 = 3;
const ROUNDS: usize = 2;
/// 255 * 0.91^REACH < CUTOFF.
pub const REACH: u32 = 50;
/// Rows above the bottom where the underworld glow starts.
const UNDERWORLD: u32 = 200;

/// Cell kinds for propagation, and their decay per channel out of 256 (RGB, sun).
const AIR: u8 = 0;
const SOLID: u8 = 1;
const CRACKED: u8 = 2;
const WATER: u8 = 3;
const HONEY: u8 = 4;
const DECAY: [[u16; 4]; 5] = [
    [233, 233, 233, 233],
    [143, 143, 143, 143],
    [205, 205, 205, 205],
    // 0.91 * (0.88, 0.96, 1.015) at the mean of the game's 0.98..1.00 ripple.
    [203, 221, 234, 219],
    // 0.91 * (0.75, 0.7, 0.6).
    [175, 163, 140, 159],
];

/// What a frame comparison in a light rule looks at.
#[derive(Clone, Copy)]
pub enum Field {
    X,
    Y,
}

#[derive(Clone, Copy)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Light of a tile type: lit when every guard holds, colour by style
/// (`frame_y / div`, or style -1 when `div` is 0; -1 also stands for any other style).
pub struct Rule {
    pub tile: u16,
    pub guards: &'static [(Field, Op, i16)],
    pub div: i16,
    pub styles: &'static [(i16, [f32; 3])],
}

/// Middle of the game's pulsing text colour (190..255), used where light pulses with it.
const PULSE: f32 = 270.0 - 222.0;

/// Light a tile emits, 1 = full.
pub fn emission(tile: u16, frame_x: i16, frame_y: i16) -> Option<[f32; 3]> {
    let fixed = |r: f32, g: f32, b: f32| Some([r, g, b]);
    match tile {
        4 if frame_x < 66 => {
            let style = (frame_y.max(0) / 22) as usize;
            return match style {
                7 => fixed(0.75, 0.3, 0.75),
                14 => fixed(1.0, 1.0, 1.0),
                23 => {
                    let (a, b) = ((0.9 + PULSE / 900.0).min(1.0), (0.9 + PULSE / 125.0).min(1.0));
                    fixed(a * 0.9, b * 0.55, a * 1.2)
                }
                _ => TORCH.get(style).copied().flatten(),
            };
        }
        37 => return fixed(0.56, 0.43, 0.15),
        70 | 71 | 72 | 190 | 348 | 528 | 578 => return fixed(0.0, 0.2 + (0.175 + PULSE / 1000.0) / 2.0, 1.0),
        349 if frame_x >= 36 => return fixed(0.0, 0.2 + (0.175 + PULSE / 1000.0) / 2.0, 1.0),
        61 | 703 if frame_x == 144 => {
            let (a, b) = (1.0 + PULSE / 400.0, 0.8 - PULSE / 400.0);
            return fixed(0.42 * b, 0.81 * a, 0.52 * b);
        }
        26 | 695 if frame_x >= 54 => return fixed(0.5, 0.2, 0.1),
        31 | 696 if frame_x >= 36 => return fixed(0.5, 0.2, 0.1),
        26 | 31 | 695 | 696 => return fixed(0.31, 0.1, 0.44),
        215 if frame_y < 36 => {
            let base = match frame_x / 54 {
                1 => [0.7, 1.0, 0.5],
                2 => [0.75, 0.3, 0.75],
                3 => [0.45, 0.75, 1.0],
                4 => [1.15, 1.15, 0.5],
                5 => [1.0, 1.0, 1.0],
                6 => [0.75, 1.2825, 1.2],
                7 => [0.95, 0.65, 1.3],
                8 => [1.4, 0.85, 0.55],
                9 => [0.25, 1.3, 0.8],
                10 => [0.95, 0.4, 1.4],
                11 => [1.4, 0.7, 0.5],
                12 => [1.25, 0.6, 1.2],
                13 => [0.75, 1.45, 0.9],
                14 => [0.25, 0.65, 1.0],
                _ => [0.9, 0.3, 0.1],
            };
            let flicker = 0.175 + PULSE / 700.0;
            return Some(base.map(|c| c + flicker));
        }
        _ => {}
    }
    let i = RULES.binary_search_by_key(&tile, |r| r.tile).ok()?;
    let r = &RULES[i];
    let holds = |&(f, op, v): &(Field, Op, i16)| {
        let a = match f {
            Field::X => frame_x,
            Field::Y => frame_y,
        };
        match op {
            Op::Eq => a == v,
            Op::Ne => a != v,
            Op::Lt => a < v,
            Op::Le => a <= v,
            Op::Gt => a > v,
            Op::Ge => a >= v,
        }
    };
    if !r.guards.iter().all(holds) {
        return None;
    }
    let style = if r.div == 0 { -1 } else { frame_y.max(0) / r.div };
    let pick = |s: i16| r.styles.iter().find(|e| e.0 == s).map(|e| e.1);
    pick(style).or_else(|| pick(-1))
}

/// Light from liquid in a cell, 1 = full.
fn liquid_emission(kind: u8) -> Option<[f32; 3]> {
    match kind {
        k if k == Liquid::Lava as u8 => {
            let n = 0.55 + PULSE / 900.0;
            Some([n, n * 0.6, n * 0.2])
        }
        k if k == Liquid::Shimmer as u8 => {
            let (a, b) = (0.7 + PULSE / 900.0, 0.7 + PULSE / 125.0);
            Some([a * 0.6, b * 0.25, a * 0.9])
        }
        _ => None,
    }
}

fn store(c: f32) -> u8 {
    (c * ONE).clamp(0.0, 255.0) as u8
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
    // Worlds without saved flags (generated ones) get a surface line and no underworld.
    let surface = world.header.env.as_ref().map_or(world.height as f64 * 0.3, |e| e.world_surface) as u32;
    let underworld = if world.header.env.is_some() { world.height.saturating_sub(UNDERWORLD) } else { u32::MAX };

    let mut light = vec![[0u8; 4]; w * h];
    let mut kind = vec![AIR; w * h];
    light.par_chunks_mut(w).zip(kind.par_chunks_mut(w)).enumerate().for_each(|(row, (lrow, krow))| {
        let y = win.y0 + row as u32;
        let (cy, ly) = (y / CHUNK, y % CHUNK);
        for (col, (l, k)) in lrow.iter_mut().zip(krow.iter_mut()).enumerate() {
            let x = win.x0 + col as u32;
            let c = world.chunk(x / CHUNK, cy);
            let i = Chunk::index(x % CHUNK, ly);
            let (t, f, liquid, lk) = (c.tile[i], c.flags[i], c.liquid[i], c.liquid_kind[i]);
            let active = t != NO_TILE && f & flags::INACTIVE == 0;
            let sloped = (f & flags::SHAPE_MASK) >> flags::SHAPE_SHIFT >= 2;
            *k = if active && BLOCK_LIGHT.has(t) && !sloped && t != 131 {
                if CRACKED_BRICKS.has(t) {
                    CRACKED
                } else {
                    SOLID
                }
            } else if liquid > 128 && lk != Liquid::Lava as u8 {
                if lk == Liquid::Honey as u8 {
                    HONEY
                } else {
                    WATER
                }
            } else {
                AIR
            };

            let open = WALL_LIGHT.has(c.wall[i]) && (!active || !NO_SUN_LIGHT.has(t) || sloped);
            if y <= surface && open && liquid < 200 {
                l[3] = store(1.0);
            }
            let mut rgb = [0.0f32; 3];
            if y > underworld && open && liquid < 200 {
                rgb = [0.55, 0.33, 0.11];
            }
            let tile_light = if active { emission(t, c.frame_x[i], c.frame_y[i]) } else { None };
            let liquid_light = if liquid > 0 { liquid_emission(lk) } else { None };
            for e in [tile_light, liquid_light].into_iter().flatten() {
                rgb = [0, 1, 2].map(|n| rgb[n].max(e[n]));
            }
            for n in 0..3 {
                l[n] = store(rgb[n]);
            }
        }
    });

    for _ in 0..ROUNDS {
        vertical(&mut light, &kind, w);
        light.par_chunks_mut(w).zip(kind.par_chunks(w)).for_each(|(l, k)| {
            sweep(l.iter_mut().zip(k));
            sweep(l.iter_mut().zip(k).rev());
        });
    }
    light
}

fn sweep<'a>(cells: impl Iterator<Item = (&'a mut [u8; 4], &'a u8)>) {
    let mut carry = [0; 4];
    for (l, &k) in cells {
        carry = step(l, carry, k);
    }
}

/// Down then up, in parallel over bands of columns.
fn vertical(light: &mut [[u8; 4]], kind: &[u8], w: usize) {
    const BAND: usize = 64;
    let mut bands: Vec<Vec<_>> = (0..w.div_ceil(BAND)).map(|_| Vec::new()).collect();
    for (l, k) in light.chunks_mut(w).zip(kind.chunks(w)) {
        for (b, lk) in bands.iter_mut().zip(l.chunks_mut(BAND).zip(k.chunks(BAND))) {
            b.push(lk);
        }
    }
    bands.into_par_iter().for_each(|mut rows| {
        let mut acc = [[0u8; 4]; BAND];
        for _ in 0..2 {
            acc.fill([0; 4]);
            for (l, k) in rows.iter_mut() {
                for ((c, a), &k) in l.iter_mut().zip(&mut acc).zip(k.iter()) {
                    *a = step(c, *a, k);
                }
            }
            rows.reverse();
        }
    });
}

/// Takes the light arriving in `cell`, returns what leaves it.
#[inline]
fn step(cell: &mut [u8; 4], arriving: [u8; 4], kind: u8) -> [u8; 4] {
    let decay = &DECAY[kind as usize];
    let mut out = [0; 4];
    for c in 0..4 {
        cell[c] = cell[c].max(arriving[c]);
        let v = (cell[c] as u16 * decay[c]) >> 8;
        out[c] = if v < CUTOFF { 0 } else { v as u8 };
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
        assert_eq!(l.get(5, 9), [0, 0, 0, 128]);
        assert!(l.get(5, 10)[3] > 110, "block facing the sky is lit");
        assert!(l.get(5, 14)[3] < 30, "{:?}", l.get(5, 14));
        assert_eq!(l.get(5, 40), [0; 4]);
        // The game's torch colour (1, 0.95, 0.8).
        assert_eq!(&l.get(21, 33)[..3], &[128, 121, 102]);
        let (near, far) = (l.get(25, 33)[0], l.get(38, 30)[0]);
        assert!(near > far && far > 0, "{near} {far}");
        assert!(l.get(30, 40)[0] < far);
    }

    #[test]
    fn emission_follows_the_game() {
        // Torch style by frame_y, unlit past frame_x 66.
        assert_eq!(emission(4, 0, 22 * 5), Some([1.4, 1.4, 1.4]));
        assert_eq!(emission(4, 66, 0), None);
        // Generated rules: candle lit only on its first frame column.
        assert!(emission(33, 0, 0).is_some());
        assert_eq!(emission(33, 18, 0), None);
        assert_eq!(emission(1, 0, 0), None);
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
