//! Liquid quads following the game's liquid renderer: a visible level per cell
//! (gaps between two liquid cells are filled, waterfalls fade below each
//! cell), edge fractions from the neighbours, a frame in the 48 px wide
//! `water_N` sheets, and the sub-rectangle of the cell that is drawn.

use world::tile_sets::{PLATFORMS, SOLID, SOLID_TOP};
use world::{flags, World, CHUNK, NO_TILE};

use crate::mesh::Instance;

/// Cells fading under a liquid cell, per kind.
pub const WATERFALL: [u32; 4] = [10, 3, 2, 10];
/// Cells around a chunk that its liquid depends on.
const SIDE: u32 = 6;
const ABOVE: u32 = 16;

#[derive(Clone, Copy, Default)]
struct Cell {
    level: f32,
    visible: f32,
    /// Waterfall step: opacity is `1 - step / (WATERFALL + 1)`; `None` hides the cell.
    step: Option<u8>,
    solid: bool,
    half: bool,
    has: bool,
    has_visible: bool,
    wall: bool,
    frame: (i32, i32),
    left_edge: bool,
    right_edge: bool,
    top_edge: bool,
    bottom_edge: bool,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    vleft: f32,
    vright: f32,
    vtop: f32,
    vbottom: f32,
    kind: u8,
    vkind: u8,
}

/// The liquid quads of chunk `(cx, cy)`, in the instance layout of `vs_liquid`:
/// `a`: x (14) | y (12) << 14 | kind (2) << 26 | waterfall step (4) << 28,
/// `b`: src x (6) | src y (6) << 6 | w - 1 (4) << 12 | h - 1 (4) << 16 |
/// offset x (4) << 20 | offset y (4) << 24 | surface frame << 28.
pub fn chunk(world: &World, cx: u32, cy: u32) -> Vec<Instance> {
    let (x0, y0) = ((cx * CHUNK) as i64 - SIDE as i64, (cy * CHUNK) as i64 - ABOVE as i64);
    let (w, h) = ((CHUNK + 2 * SIDE) as usize, (CHUNK + ABOVE + SIDE) as usize);
    let mut g = vec![Cell::default(); w * h];
    let at = |x: usize, y: usize| x * h + y;
    let mut any = false;
    for x in 0..w {
        for y in 0..h {
            let (wx, wy) = (x0 + x as i64, y0 + y as i64);
            if !world.in_bounds(wx, wy) {
                continue;
            }
            let (c, i) = world.cell(wx as u32, wy as u32);
            let t = c.tile[i];
            let above = if y > 0 { g[at(x, y - 1)] } else { Cell::default() };
            let shape = (c.flags[i] & flags::SHAPE_MASK) >> flags::SHAPE_SHIFT;
            let cell = &mut g[at(x, y)];
            cell.level = c.liquid[i] as f32 / 255.0;
            cell.has = c.liquid[i] != 0;
            any |= cell.has;
            cell.half = t != NO_TILE && shape == 1 && above.has && !PLATFORMS.has(t);
            cell.solid = t != NO_TILE && SOLID.has(t) && !SOLID_TOP.has(t);
            cell.wall = c.wall[i] != 0;
            cell.kind = c.liquid_kind[i] & 3;
            if cell.half && !cell.has {
                cell.kind = above.kind;
            }
        }
    }
    if !any {
        return Vec::new();
    }

    // Visible level: empty cells between two liquid cells take their mean.
    for x in 1..w - 1 {
        for y in 1..h - 1 {
            let (up, down, l, r) = (g[at(x, y - 1)], g[at(x, y + 1)], g[at(x - 1, y)], g[at(x + 1, y)]);
            let c = &mut g[at(x, y)];
            let mut v = 0.0;
            if c.half && up.has {
                v = 1.0;
            } else if !c.has {
                if up.has && down.has && up.kind == down.kind && !up.solid && !down.solid {
                    v = up.level + down.level;
                    c.kind = up.kind;
                }
                if l.has && r.has && l.kind == r.kind && !l.solid && !r.solid {
                    v = f32::max(v, l.level + r.level);
                    c.kind = l.kind;
                }
                v *= 0.5;
            } else {
                v = c.level;
            }
            c.visible = v;
            c.has_visible = v != 0.0;
        }
    }

    // Waterfalls: each open liquid cell fades into the cells below it.
    for x in 0..w {
        for y in 0..h {
            let c = g[at(x, y)];
            if c.has_visible && (!c.solid || c.half) {
                g[at(x, y)].step = Some(0);
                g[at(x, y)].vkind = c.kind;
                let n = WATERFALL[c.kind as usize];
                for k in 1..=n as usize {
                    if y + k >= h || g[at(x, y + k)].solid {
                        break;
                    }
                    let fade = 1.0 - k as f32 / (n + 1) as f32;
                    let b = &mut g[at(x, y + k)];
                    b.visible = b.visible.max(c.visible * fade);
                    b.step = Some(k as u8);
                    b.vkind = c.kind;
                }
            }
            let c = &mut g[at(x, y)];
            if c.solid && !c.half {
                c.visible = 1.0;
                c.has_visible = false;
            } else {
                c.has_visible = c.visible != 0.0;
            }
        }
    }

    // Edges and how far each one cuts into the cell, then the sheet frame.
    for x in 1..w - 1 {
        for y in 1..h - 1 {
            let (up, down, l, r) = (g[at(x, y - 1)], g[at(x, y + 1)], g[at(x - 1, y)], g[at(x + 1, y)]);
            let c = &mut g[at(x, y)];
            if !c.has_visible {
                (c.left_edge, c.right_edge, c.top_edge, c.bottom_edge) = (false, false, false, false);
                continue;
            }
            let open = |n: &Cell| !n.has_visible && !n.solid && !n.half;
            let rest = 1.0 - c.visible;
            let (mut left, mut right, mut top, mut bottom) = (0.0, 1.0, 0.0, 1.0);
            if !up.has_visible {
                top += down.visible * rest;
            }
            if open(&down) {
                bottom -= up.visible * rest;
            }
            if open(&l) {
                left += r.visible * rest;
            }
            if open(&r) {
                right -= l.visible * rest;
            }
            (c.left, c.right, c.top, c.bottom) = (left, right, top, bottom);
            c.top_edge = (!up.has_visible && !up.solid) || top != 0.0;
            c.bottom_edge = (!down.has_visible && !down.solid) || bottom != 1.0;
            c.left_edge = (!l.has_visible && !l.solid) || left != 0.0;
            c.right_edge = (!r.has_visible && !r.solid) || right != 1.0;
            let mut f = (0, 0);
            if !c.left_edge {
                f.0 += if c.right_edge { 32 } else { 16 };
            }
            if c.left_edge && c.right_edge {
                f = (16, if c.top_edge { 16 } else { 32 });
            } else if !c.top_edge {
                f.1 += if !c.left_edge && !c.right_edge { 48 } else { 16 };
            }
            if f.1 == 16 && (c.left_edge ^ c.right_edge) && (y0 + y as i64) % 2 == 0 {
                f.1 += 16;
            }
            c.frame = f;
        }
    }

    // Edge fractions smoothed with the neighbours along the edge.
    for x in 1..w - 1 {
        for y in 1..h - 1 {
            let (up, down, l, r) = (g[at(x, y - 1)], g[at(x, y + 1)], g[at(x - 1, y)], g[at(x + 1, y)]);
            let c = &mut g[at(x, y)];
            if !c.has_visible {
                continue;
            }
            (c.vleft, c.vright, c.vtop, c.vbottom) = (c.left, c.right, c.top, c.bottom);
            if up.has_visible && down.has_visible {
                if c.left_edge {
                    c.vleft = (c.left * 2.0 + up.left + down.left) * 0.25;
                }
                if c.right_edge {
                    c.vright = (c.right * 2.0 + up.right + down.right) * 0.25;
                }
            }
            if l.has_visible && r.has_visible {
                if c.top_edge {
                    c.vtop = (c.top * 2.0 + l.top + r.top) * 0.25;
                }
                if c.bottom_edge {
                    c.vbottom = (c.bottom * 2.0 + l.bottom + r.bottom) * 0.25;
                }
            }
        }
    }

    // Surface cells beside a one-sided edge follow it; others next to a narrow fall square up.
    for x in 1..w - 1 {
        for y in 1..h - 1 {
            let (down, l, r) = (g[at(x, y + 1)], g[at(x - 1, y)], g[at(x + 1, y)]);
            let c = &mut g[at(x, y)];
            if !c.has {
                continue;
            }
            if c.top_edge && !c.bottom_edge && (c.left_edge ^ c.right_edge) {
                if c.right_edge {
                    c.vright = down.vright;
                    c.vtop = l.vtop;
                } else {
                    c.vleft = down.vleft;
                    c.vtop = r.vtop;
                }
            } else if down.frame == (16, 32) {
                if c.vleft > 0.5 {
                    c.vleft = 0.0;
                    c.frame = (0, 0);
                } else if c.vright < 0.5 {
                    c.vright = 1.0;
                    c.frame = (32, 0);
                }
            }
        }
    }

    // Inner corners: a full cell under an edge takes the corner frames.
    for x in 1..w - 1 {
        for y in 1..h - 1 {
            let (up, l, r) = (g[at(x, y - 1)], g[at(x - 1, y)], g[at(x + 1, y)]);
            let c = &mut g[at(x, y)];
            if !c.has || c.bottom_edge || c.left_edge || c.top_edge || c.right_edge {
                continue;
            }
            if l.top_edge && up.left_edge {
                c.frame.0 = ((16.0 - up.vleft * 16.0) as i32).max(4) - 4;
                c.frame.1 = 48 + ((16.0 - l.vtop * 16.0) as i32).max(4) - 4;
            } else if r.top_edge && up.right_edge {
                c.frame.0 = 32 - ((up.vright * 16.0) as i32 - 4).min(16);
                c.frame.1 = 48 + ((16.0 - r.vtop * 16.0) as i32).max(4) - 4;
            } else {
                continue;
            }
            (c.vleft, c.vtop, c.vright, c.vbottom) = (0.0, 0.0, 1.0, 1.0);
        }
    }

    let mut out = Vec::new();
    for lx in 0..CHUNK {
        for ly in 0..CHUNK {
            let (x, y) = ((lx + SIDE) as usize, (ly + ABOVE) as usize);
            let (wx, wy) = (x0 + x as i64, y0 + y as i64);
            let c = g[at(x, y)];
            if !world.in_bounds(wx, wy) || !c.has_visible {
                continue;
            }
            let Some(step) = c.step else { continue };
            if !(c.wall || !c.half || !c.has || c.level >= 1.0) {
                continue;
            }
            let left = c.vleft.min(0.75);
            let right = c.vright.max(0.25);
            let top = c.vtop.min(0.75);
            let mut bottom = c.vbottom.max(0.25);
            if c.half && c.solid && bottom > 0.5 {
                bottom = 0.5;
            }
            let sx = (16.0 - right * 16.0) as i32 + c.frame.0;
            let sy = (16.0 - bottom * 16.0) as i32 + c.frame.1;
            let sw = ((right - left) * 16.0).ceil() as i32;
            let sh = ((bottom - top) * 16.0).ceil() as i32;
            if sw <= 0 || sh <= 0 {
                continue;
            }
            let (ox, oy) = ((left * 16.0).floor() as u32, (top * 16.0).floor() as u32);
            let surface = c.frame == (16, 0);
            out.push(Instance {
                a: wx as u32 | (wy as u32) << 14 | (c.vkind as u32) << 26 | (step as u32) << 28,
                b: sx.clamp(0, 63) as u32
                    | (sy.clamp(0, 63) as u32) << 6
                    | ((sw.min(16) - 1) as u32) << 12
                    | ((sh.min(16) - 1) as u32) << 16
                    | ox.min(15) << 20
                    | oy.min(15) << 24
                    | (surface as u32) << 28,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::wld::{Liquid, Tile};

    fn world(cell: impl Fn(u32, u32) -> Tile) -> World {
        World::parse(&formats::synth::write_world("l", 64, 64, &[], cell)).unwrap()
    }

    fn decode(i: &Instance) -> (u32, u32, u32, u32, [u32; 6], bool) {
        let b = i.b;
        let src = [b & 63, (b >> 6) & 63, ((b >> 12) & 15) + 1, ((b >> 16) & 15) + 1, (b >> 20) & 15, (b >> 24) & 15];
        (i.a & 0x3fff, (i.a >> 14) & 0xfff, (i.a >> 26) & 3, i.a >> 28, src, (b >> 28) & 1 != 0)
    }

    #[test]
    fn pool_frames() {
        // Water from y = 10 down to 13 in x = 5..10, stone floor and walls around it.
        let w = world(|x, y| {
            let inside = (5..10).contains(&x) && (10..14).contains(&y);
            let stone = !inside && (4..11).contains(&x) && (10..15).contains(&y);
            Tile {
                tile: if stone { 1 } else { NO_TILE },
                liquid: if inside { 255 } else { 0 },
                liquid_kind: Liquid::Water,
                ..Tile::default()
            }
        });
        let quads: Vec<_> = chunk(&w, 0, 0).iter().map(decode).collect();
        assert_eq!(quads.len(), 20);
        // Top row: the surface frame, full cells.
        for q in quads.iter().filter(|q| q.1 == 10) {
            assert_eq!(q.4, [16, 0, 16, 16, 0, 0], "{q:?}");
            assert!(q.5);
        }
        // Deeper cells: the body frame, no edges.
        for q in quads.iter().filter(|q| q.1 > 10) {
            assert_eq!(q.4, [16, 48, 16, 16, 0, 0], "{q:?}");
            assert_eq!(q.3, 0);
        }
    }

    #[test]
    fn waterfall_fades_below_a_ledge() {
        // A water cell at (20, 20) with open air below falls for 10 cells.
        let w = world(|x, y| Tile {
            liquid: if (x, y) == (20, 20) { 255 } else { 0 },
            liquid_kind: Liquid::Water,
            ..Tile::default()
        });
        let quads: Vec<_> = chunk(&w, 0, 0).iter().map(decode).collect();
        let steps: Vec<(u32, u32)> = quads.iter().filter(|q| q.0 == 20).map(|q| (q.1, q.3)).collect();
        assert_eq!(steps, (0..=10).map(|k| (20 + k, k)).collect::<Vec<_>>());
    }

    #[test]
    fn half_filled_cell_cuts_from_the_top() {
        // A row of half-full lava on a stone floor.
        let w = world(|x, y| Tile {
            tile: if y == 21 { 1 } else { NO_TILE },
            liquid: if y == 20 && (10..20).contains(&x) { 128 } else { 0 },
            liquid_kind: Liquid::Lava,
            ..Tile::default()
        });
        let q = chunk(&w, 0, 0).iter().map(decode).find(|q| (q.0, q.1) == (15, 20)).unwrap();
        assert_eq!(q.2, 1);
        // Height is the cut of the top edge by the level: about half a cell.
        assert_eq!(q.4[1], 0);
        assert!((7..=9).contains(&q.4[3]), "{q:?}");
        assert_eq!(q.4[5], 16 - q.4[3]);
    }
}
