//! Tile and wall sheets: loading from the game install and packing into
//! texture-array layers, so every chunk draws with a single bind group.

use std::path::Path;

use formats::xnb::{self, Texture};
use rayon::prelude::*;

pub const LAYER_SIZE: u32 = 4096;

#[derive(Clone, Copy, Debug)]
pub struct SheetRef {
    pub layer: u32,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// Placeholder texture: frame offsets are ignored.
    pub fallback: bool,
}

pub struct Atlas {
    pub layers: u32,
    pub tiles: Vec<Option<SheetRef>>,
    pub walls: Vec<Option<SheetRef>>,
    /// Average colour per type, for the zoomed-out map.
    pub tile_color: Vec<[u8; 4]>,
    pub wall_color: Vec<[u8; 4]>,
    /// Pixels to upload, in packing order.
    pub uploads: Vec<(SheetRef, Texture)>,
    pub missing: usize,
}

impl Atlas {
    /// `images` is `<Terraria>/Content/Images`; `None` uses placeholders only.
    pub fn load(images: Option<&Path>, tile_types: &[u16], wall_types: &[u16]) -> Self {
        let jobs: Vec<(bool, u16)> =
            tile_types.iter().map(|&t| (false, t)).chain(wall_types.iter().map(|&w| (true, w))).collect();
        let loaded: Vec<(bool, u16, Texture, bool)> = jobs
            .par_iter()
            .map(|&(wall, id)| {
                let name = if wall { format!("Wall_{id}.xnb") } else { format!("Tiles_{id}.xnb") };
                let tex = images
                    .and_then(|d| std::fs::read(d.join(name)).ok())
                    .and_then(|data| xnb::read_texture(&data).ok())
                    .filter(|t| t.width <= LAYER_SIZE && t.height <= LAYER_SIZE);
                match tex {
                    Some(t) => (wall, id, t, false),
                    None => (wall, id, placeholder(wall, id), true),
                }
            })
            .collect();

        let max = |wall: bool| loaded.iter().filter(|l| l.0 == wall).map(|l| l.1 as usize + 1).max().unwrap_or(0);
        let mut atlas = Atlas {
            layers: 1,
            tiles: vec![None; max(false)],
            walls: vec![None; max(true)],
            tile_color: vec![[0; 4]; max(false)],
            wall_color: vec![[0; 4]; max(true)],
            uploads: Vec::new(),
            missing: loaded.iter().filter(|l| l.3).count(),
        };

        let mut order: Vec<usize> = (0..loaded.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(loaded[i].2.height));
        let colors: Vec<[u8; 4]> = loaded.par_iter().map(|l| average(&l.2)).collect();
        let mut refs = vec![None; loaded.len()];
        let (mut layer, mut x, mut y, mut shelf) = (0, 0, 0, 0);
        for i in order {
            let (wall, id, ref tex, fallback) = loaded[i];
            if x + tex.width > LAYER_SIZE {
                (x, y, shelf) = (0, y + shelf, 0);
            }
            if y + tex.height > LAYER_SIZE {
                (layer, x, y, shelf) = (layer + 1, 0, 0, 0);
            }
            let r = SheetRef { layer, x, y, w: tex.width, h: tex.height, fallback };
            x += tex.width;
            shelf = shelf.max(tex.height);
            if wall {
                atlas.walls[id as usize] = Some(r);
                atlas.wall_color[id as usize] = colors[i];
            } else {
                atlas.tiles[id as usize] = Some(r);
                atlas.tile_color[id as usize] = colors[i];
            }
            refs[i] = Some(r);
        }
        atlas.uploads = refs.into_iter().zip(loaded).map(|(r, l)| (r.unwrap(), l.2)).collect();
        atlas.layers = layer + 1;
        atlas
    }
}

fn placeholder(wall: bool, id: u16) -> Texture {
    let size = if wall { 32 } else { 16 };
    let h = (id as u32 ^ if wall { 0x5555 } else { 0 }).wrapping_mul(0x9E37_79B1);
    let base = [(h >> 8) as u8 | 0x40, (h >> 16) as u8 | 0x40, (h >> 24) as u8 | 0x40];
    let dim = if wall { 2 } else { 1 };
    let mut rgba = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let edge = x == 0 || y == 0 || x == size - 1 || y == size - 1;
            let k = if edge { 2 * dim } else { dim };
            rgba.extend_from_slice(&[base[0] / k, base[1] / k, base[2] / k, 255]);
        }
    }
    Texture { width: size as u32, height: size as u32, rgba }
}

fn average(t: &Texture) -> [u8; 4] {
    let mut sum = [0u64; 4];
    for p in t.rgba.chunks_exact(4).filter(|p| p[3] > 0) {
        for k in 0..4 {
            sum[k] += p[k] as u64;
        }
    }
    let n = sum[3].max(1);
    // Premultiplied: divide by total alpha to get the opaque colour.
    [(sum[0] * 255 / n) as u8, (sum[1] * 255 / n) as u8, (sum[2] * 255 / n) as u8, 255]
}
