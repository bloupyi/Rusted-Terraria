//! CPU side of the scene: per-chunk instances (built in parallel) grouped by
//! chunk row, and the one-texel-per-tile map used when zoomed out.

use bytemuck::{Pod, Zeroable};
use rayon::prelude::*;
use world::light::LightMap;
use world::{Chunk, World, CHUNK, NO_TILE};

use crate::atlas::{Atlas, SheetRef};

pub const LOD_PAGE: u32 = 2048;
const WALL_BIT: u32 = 1 << 30;
const LIQUID_BIT: u32 = 1 << 31;

/// `a`: tile x (15 bits) | tile y (15 bits) << 15 | wall << 30 | liquid << 31.
/// `b`: atlas u (12) | atlas v (12) << 12 | layer (8) << 24,
/// or for a liquid: amount (8) | kind (8) << 8.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Instance {
    pub a: u32,
    pub b: u32,
}

/// One chunk row. `statics` holds the walls of every chunk then the tiles of
/// every chunk, so any horizontal run of visible chunks is one contiguous range
/// per layer. Liquids live in their own buffer since they change every tick.
pub struct RowMesh {
    pub statics: Vec<Instance>,
    /// `chunks_x + 1` prefix offsets, per layer.
    pub wall_off: Vec<u32>,
    pub tile_off: Vec<u32>,
    pub liquids: Vec<Instance>,
    pub liquid_off: Vec<u32>,
}

/// A rebuilt row after edits: `None` parts are unchanged.
pub struct RowUpdate {
    pub cy: u32,
    pub statics: Option<(Vec<Instance>, Vec<u32>, Vec<u32>)>,
    pub liquids: Option<(Vec<Instance>, Vec<u32>)>,
}

/// Keeps every chunk's instances so an edit only rebuilds its own chunks.
pub struct Mesher {
    pub atlas: Atlas,
    chunks: Vec<[Vec<Instance>; 3]>,
    chunks_x: u32,
}

/// Everything the GPU needs at startup, built on the CPU in parallel. The
/// renderer takes the large buffers; the mesher and light stay for updates.
pub struct Scene {
    pub mesher: Mesher,
    pub rows: Vec<RowMesh>,
    pub chunks_x: u32,
    pub lod_pages_x: u32,
    pub lod_pages_y: u32,
    pub lod: Vec<Vec<u8>>,
    /// Same page layout as `lod`.
    pub light: LightMap,
}

impl Scene {
    /// `extra_tiles`: types that edits may create, loaded even if absent from the world.
    pub fn build(world: &World, images: Option<&std::path::Path>, extra_tiles: &[u16]) -> Self {
        let (mut tiles, walls) = world.used_types();
        tiles.extend_from_slice(extra_tiles);
        tiles.sort_unstable();
        tiles.dedup();
        let atlas = Atlas::load(images, &tiles, &walls);
        let ((mesher, rows), ((lod_pages_x, lod_pages_y, lod), light)) =
            rayon::join(|| Mesher::new(world, atlas), || rayon::join(|| build_lod(world), || LightMap::compute(world)));
        let mut scene = Self { mesher, rows, chunks_x: world.chunks_x, lod_pages_x, lod_pages_y, lod, light };
        let atlas = &scene.mesher.atlas;
        scene.lod.par_iter_mut().enumerate().for_each(|(p, page)| {
            let (ox, oy) = ((p as u32 % lod_pages_x) * LOD_PAGE, (p as u32 / lod_pages_x) * LOD_PAGE);
            colorize(world, atlas, ox, oy, LOD_PAGE, page);
        });
        scene
    }
}

impl Mesher {
    fn new(world: &World, atlas: Atlas) -> (Self, Vec<RowMesh>) {
        let chunks: Vec<[Vec<Instance>; 3]> = (0..world.chunks_x * world.chunks_y)
            .into_par_iter()
            .map(|i| build_chunk(world, &atlas, i % world.chunks_x, i / world.chunks_x, false))
            .collect();
        let m = Self { atlas, chunks, chunks_x: world.chunks_x };
        let rows = (0..world.chunks_y)
            .into_par_iter()
            .map(|cy| {
                let (statics, wall_off, tile_off) = m.statics(cy);
                let (liquids, liquid_off) = m.layers(cy, &[1]);
                RowMesh { statics, wall_off, tile_off, liquids, liquid_off: liquid_off.into_iter().next().unwrap() }
            })
            .collect();
        (m, rows)
    }

    fn statics(&self, cy: u32) -> (Vec<Instance>, Vec<u32>, Vec<u32>) {
        let (v, mut offs) = self.layers(cy, &[0, 2]);
        let tile_off = offs.pop().unwrap();
        (v, offs.pop().unwrap(), tile_off)
    }

    /// Concatenates `layers` of every chunk of row `cy`, with prefix offsets per layer.
    fn layers(&self, cy: u32, layers: &[usize]) -> (Vec<Instance>, Vec<Vec<u32>>) {
        let row = &self.chunks[(cy * self.chunks_x) as usize..((cy + 1) * self.chunks_x) as usize];
        let total: usize = row.iter().flat_map(|c| layers.iter().map(|&l| c[l].len())).sum();
        let mut v = Vec::with_capacity(total);
        let offs = layers
            .iter()
            .map(|&l| {
                let mut off = vec![v.len() as u32];
                for c in row {
                    v.extend_from_slice(&c[l]);
                    off.push(v.len() as u32);
                }
                off
            })
            .collect();
        (v, offs)
    }

    /// Rebuilds the dirty chunks and returns the rows to re-upload.
    pub fn update(&mut self, world: &World, dirty: &[(u32, u32, u8)]) -> Vec<RowUpdate> {
        let rebuilt: Vec<_> = dirty
            .par_iter()
            .map(|&(cx, cy, bits)| {
                let liquids_only = bits & world::DIRTY_TILES == 0;
                (cx, cy, bits, build_chunk(world, &self.atlas, cx, cy, liquids_only))
            })
            .collect();
        let mut rows: Vec<(u32, u8)> = Vec::new();
        for (cx, cy, bits, [w, l, t]) in rebuilt {
            let chunk = &mut self.chunks[(cy * self.chunks_x + cx) as usize];
            chunk[1] = l;
            if bits & world::DIRTY_TILES != 0 {
                (chunk[0], chunk[2]) = (w, t);
            }
            match rows.iter_mut().find(|r| r.0 == cy) {
                Some(r) => r.1 |= bits,
                None => rows.push((cy, bits)),
            }
        }
        rows.par_iter()
            .map(|&(cy, bits)| {
                let (liquids, off) = self.layers(cy, &[1]);
                RowUpdate {
                    cy,
                    statics: (bits & world::DIRTY_TILES != 0).then(|| self.statics(cy)),
                    liquids: Some((liquids, off.into_iter().next().unwrap())),
                }
            })
            .collect()
    }

    /// Map texels (RGBA8, `size`² from `(ox, oy)`) after an edit.
    pub fn lod_texels(&self, world: &World, ox: u32, oy: u32, size: u32) -> Vec<u8> {
        let mut img = vec![0; (size * size * 4) as usize];
        colorize(world, &self.atlas, ox, oy, size, &mut img);
        img
    }
}

/// `liquids_only` skips walls and tiles (left empty) when only liquids changed.
fn build_chunk(world: &World, atlas: &Atlas, cx: u32, cy: u32, liquids_only: bool) -> [Vec<Instance>; 3] {
    let important = &world.header.frame_important;
    let c: &Chunk = world.chunk(cx, cy);
    let (mut walls, mut liquids, mut tiles) = (Vec::new(), Vec::new(), Vec::new());
    let is_block = |t: u16| t != NO_TILE && !important.get(t as usize).copied().unwrap_or(false);
    for lx in 0..CHUNK {
        let x = cx * CHUNK + lx;
        if x >= world.width {
            break;
        }
        for ly in 0..CHUNK {
            let y = cy * CHUNK + ly;
            if y >= world.height {
                break;
            }
            let i = Chunk::index(lx, ly);
            let (xi, yi) = (x as i64, y as i64);
            if c.liquid[i] != 0 {
                // Liquid below more liquid fills its tile.
                let amount = if world.liquid(xi, yi - 1) != 0 { 255 } else { c.liquid[i] as u32 };
                liquids.push(Instance { a: x | (y << 15) | LIQUID_BIT, b: amount | (c.liquid_kind[i] as u32) << 8 });
            }
            if liquids_only {
                continue;
            }
            let wall = c.wall[i];
            if let Some(Some(s)) = atlas.walls.get(wall as usize).filter(|_| wall != 0) {
                let n = |dx, dy| world.wall(xi + dx, yi + dy) != 0;
                let (fx, fy) = block_frame(n(0, -1), n(0, 1), n(-1, 0), n(1, 0), variant(x, y));
                if let Some(inst) = instance(s, x, y, fx * 36, fy * 36, 32, true) {
                    walls.push(inst);
                }
            }
            let t = c.tile[i];
            if let Some(Some(s)) = atlas.tiles.get(t as usize).filter(|_| t != NO_TILE) {
                let (fx, fy) = if is_block(t) {
                    let n = |dx, dy| is_block(world.tile(xi + dx, yi + dy));
                    let (fx, fy) = block_frame(n(0, -1), n(0, 1), n(-1, 0), n(1, 0), variant(x, y));
                    (fx * 18, fy * 18)
                } else {
                    (c.frame_x[i].max(0) as u32, c.frame_y[i].max(0) as u32)
                };
                if let Some(inst) = instance(s, x, y, fx, fy, 16, false) {
                    tiles.push(inst);
                }
            }
        }
    }
    [walls, liquids, tiles]
}

#[inline]
fn instance(s: &SheetRef, x: u32, y: u32, fx: u32, fy: u32, size: u32, wall: bool) -> Option<Instance> {
    let (fx, fy) = if s.fallback { (0, 0) } else { (fx, fy) };
    if fx + size > s.w || fy + size > s.h {
        return None;
    }
    Some(Instance {
        a: x | (y << 15) | if wall { WALL_BIT } else { 0 },
        b: (s.x + fx) | ((s.y + fy) << 12) | (s.layer << 24),
    })
}

#[inline]
fn variant(x: u32, y: u32) -> u32 {
    (x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77)).rotate_right(13) % 3
}

/// Frame cell (in grid units) of a block from its 4 neighbours.
/// TODO: approximate layout read off block sheets; no tile merging (dirt/stone
/// blends, grass), no slopes or half blocks yet.
fn block_frame(up: bool, down: bool, left: bool, right: bool, v: u32) -> (u32, u32) {
    match (up, down, left, right) {
        (true, true, true, true) => (1 + v, 1),
        (false, true, true, true) => (1 + v, 0),
        (true, false, true, true) => (1 + v, 2),
        (true, true, false, true) => (0, v),
        (true, true, true, false) => (4, v),
        (false, true, false, true) => (2 * v, 3),
        (false, true, true, false) => (1 + 2 * v, 3),
        (true, false, false, true) => (2 * v, 4),
        (true, false, true, false) => (1 + 2 * v, 4),
        (true, true, false, false) => (5, v),
        (false, false, true, true) => (6 + v, 4),
        (false, true, false, false) => (6 + v, 0),
        (true, false, false, false) => (6 + v, 3),
        (false, false, false, true) => (9, v),
        (false, false, true, false) => (12, v),
        (false, false, false, false) => (9 + v, 3),
    }
}

/// Opaque versions of the shader's liquid colours.
pub const LIQUID_LOD: [[u8; 4]; 4] =
    [[40, 90, 230, 255], [240, 100, 20, 255], [240, 165, 25, 255], [205, 155, 255, 255]];

/// Empty map pages of `LOD_PAGE`² texels, row-major; `colorize` fills them.
fn build_lod(world: &World) -> (u32, u32, Vec<Vec<u8>>) {
    let px = world.width.div_ceil(LOD_PAGE);
    let py = world.height.div_ceil(LOD_PAGE);
    (px, py, (0..px * py).into_par_iter().map(|_| vec![0u8; (LOD_PAGE * LOD_PAGE * 4) as usize]).collect())
}

/// One texel per tile, RGBA8 premultiplied, `size` texels per line from `(ox, oy)`.
/// Parallel over lines for whole pages, sequential for a chunk.
fn colorize(world: &World, atlas: &Atlas, ox: u32, oy: u32, size: u32, img: &mut [u8]) {
    let line = |(row, line): (usize, &mut [u8])| {
        let y = oy + row as u32;
        if y >= world.height {
            return;
        }
        for (col, px) in line.chunks_exact_mut(4).enumerate() {
            let x = ox + col as u32;
            if x >= world.width {
                break;
            }
            let (c, i) = world.cell(x, y);
            let (t, w) = (c.tile[i], c.wall[i]);
            if let Some(c) = atlas.tile_color.get(t as usize).filter(|_| t != NO_TILE) {
                px.copy_from_slice(c);
            } else if c.liquid[i] != 0 {
                px.copy_from_slice(&LIQUID_LOD[c.liquid_kind[i] as usize & 3]);
            } else if let Some(c) = atlas.wall_color.get(w as usize).filter(|_| w != 0) {
                px.copy_from_slice(&[c[0] / 2, c[1] / 2, c[2] / 2, 255]);
            } else {
                px.copy_from_slice(&[0; 4]);
            }
        }
    };
    if size > CHUNK {
        img.par_chunks_mut((size * 4) as usize).enumerate().for_each(line);
    } else {
        img.chunks_mut((size * 4) as usize).enumerate().for_each(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::wld::{Liquid, Tile};

    #[test]
    fn liquid_instances() {
        // Lava column at x = 3: partial surface at y = 5, full below.
        let world = World::parse(&formats::synth::write_world("l", 40, 40, &[], |x, y| Tile {
            liquid: if x == 3 && (5..8).contains(&y) { 100 } else { 0 },
            liquid_kind: Liquid::Lava,
            ..Tile::default()
        }))
        .unwrap();
        let (_, rows) = Mesher::new(&world, Atlas::load(None, &[], &[]));
        let liquids = &rows[0].liquids;
        let got: Vec<(u32, u32)> = liquids.iter().map(|i| ((i.a >> 15) & 0x7fff, i.b)).collect();
        let lava = (Liquid::Lava as u32) << 8;
        assert_eq!(got, vec![(5, 100 | lava), (6, 255 | lava), (7, 255 | lava)]);
        assert!(liquids.iter().all(|i| i.a & LIQUID_BIT != 0 && i.a & 0x7fff == 3));
    }
}
