//! How the game draws a tile's cell: size, vertical offset and mirroring
//! (`tile_draw::RULES`), and the biome variant of tree trunks.

use crate::{World, NO_TILE};

#[derive(Clone, Copy)]
pub enum Field {
    X,
    Y,
    /// Tile column.
    Col,
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

#[derive(Clone, Copy)]
pub enum Set {
    W(i32),
    H(i32),
    Top(i32),
    TopAdd(i32),
    Flip,
}

/// `set` applies when every `(field, op, div, rem, value)` holds for
/// `field / div` (or `field % rem` when `rem` is not 0).
pub struct Step {
    pub when: &'static [(Field, Op, i32, i32, i32)],
    pub set: Set,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Draw {
    pub w: u32,
    pub h: u32,
    /// Pixels down from the top of the cell.
    pub top: i32,
    pub flip: bool,
}

impl Draw {
    pub const CELL: Draw = Draw { w: 16, h: 16, top: 0, flip: false };
}

/// Draw of a cell of `tile` at column `x` with frame `(fx, fy)`.
pub fn cell(tile: u16, x: u32, fx: i32, fy: i32) -> Draw {
    let mut d = Draw::CELL;
    let Ok(i) = crate::tile_draw::RULES.binary_search_by_key(&tile, |r| r.0) else { return d };
    for s in crate::tile_draw::RULES[i].1 {
        let holds = s.when.iter().all(|&(f, op, div, rem, v)| {
            let a = match f {
                Field::X => fx,
                Field::Y => fy,
                Field::Col => x as i32,
            };
            let a = if rem != 0 { a % rem } else { a / div };
            match op {
                Op::Eq => a == v,
                Op::Ne => a != v,
                Op::Lt => a < v,
                Op::Le => a <= v,
                Op::Gt => a > v,
                Op::Ge => a >= v,
            }
        });
        if holds {
            match s.set {
                Set::W(v) => d.w = v as u32,
                Set::H(v) => d.h = v as u32,
                Set::Top(v) => d.top = v,
                Set::TopAdd(v) => d.top += v,
                Set::Flip => d.flip = true,
            }
        }
    }
    d
}

/// Walls that let the wind through.
pub fn wall_allows_wind(wall: u16) -> bool {
    matches!(wall, 0 | 150 | 138 | 145 | 107 | 152 | 140 | 139 | 141 | 106 | 245 | 315 | 317)
}

/// Whether a plant cell sways in the wind: a swaying type, above the surface,
/// out of liquid and not behind a closed wall.
pub fn sways(world: &World, x: i64, y: i64, tile: u16, fx: i32) -> bool {
    let swaying = matches!(
        tile,
        3 | 20
            | 24
            | 61
            | 703
            | 71
            | 73
            | 74
            | 83
            | 84
            | 110
            | 113
            | 201
            | 227
            | 529
            | 590
            | 595
            | 615
            | 624
            | 700
            | 656
            | 701
            | 637
    );
    swaying
        && !(tile == 227 && matches!(fx, 204 | 238 | 408 | 442 | 476))
        && (y as f64) < surface(world)
        && world.liquid(x, y) == 0
        && wall_allows_wind(world.wall(x, y))
}

/// Wind phase counters, in turns; the game advances them once per tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Wind {
    pub tree: f64,
    pub grass: f64,
}

impl Wind {
    pub fn tick(&mut self, wind: f32) {
        let n = ((wind.abs() as f64 - 0.08) / (1.2 - 0.08)).clamp(0.0, 1.0);
        self.tree += 1.0 / 240.0 + 1.0 / 240.0 * n * 2.0;
        self.grass += 1.0 / 180.0 + 1.0 / 180.0 * n * 4.0;
    }
}

/// Sideways sway of a cell at tile `(x, y)` for a counter and wind speed,
/// 0 when the wind is too weak; the caller checks the cell is in the open.
pub fn wind_cycle(x: i64, y: i64, counter: f64, wind: f32) -> f32 {
    let phase = x as f32 * 0.5 + (y / 100) as f32 * 0.5;
    let v = (counter * std::f64::consts::TAU + phase as f64).cos() as f32 * 0.5 + wind;
    v * ((wind.abs() - 0.08) / (0.18 - 0.08)).clamp(0.0, 1.0)
}

/// Trunk variant of a tree cell from the ground under its trunk: -1 for
/// forest, then corruption, jungle, hallow, snow, crimson, underground jungle,
/// mushroom. Trunk sheets sit 176 px apart in `Tiles_5`.
pub fn tree_biome(world: &World, x: i64, y: i64, fx: i32, fy: i32) -> i32 {
    let mut x = x;
    if fx == 66 && fy <= 45 {
        x += 1;
    }
    if fx == 88 && (66..=110).contains(&fy) {
        x -= 1;
    }
    if fy >= 198 {
        match fx {
            66 => x -= 1,
            44 => x += 1,
            _ => {}
        }
    } else if fy >= 132 {
        match fx {
            22 => x -= 1,
            44 => x += 1,
            _ => {}
        }
    }
    let tree = world.tile(x, y);
    let mut y = y;
    while world.tile(x, y) == tree && tree != NO_TILE {
        y += 1;
    }
    match world.tile(x, y) {
        23 | 661 => 0,
        60 if y as f64 <= surface(world) => 1,
        60 => 5,
        70 => 6,
        109 | 492 => 2,
        147 => 3,
        199 | 662 => 4,
        _ => -1,
    }
}

fn surface(world: &World) -> f64 {
    world.header.env.as_ref().map_or(world.height as f64 * 0.3, |e| e.world_surface)
}

/// Leaf style of a tree top or branch, the frame width and height of its
/// sheet, and the frame offset, from the ground under column `x + dx`.
/// `None` when no known ground is found within 100 cells.
pub fn tree_foliage(world: &World, x: i64, y: i64, dx: i64, frame: i32) -> Option<(i32, u32, u32, i32)> {
    let env = world.header.env.clone().unwrap_or_default();
    let col = x + dx;
    let (mut w, mut h, mut frame) = (80, 80, frame);
    for k in 0..100 {
        let floor = y + k;
        if !world.in_bounds(col, floor) {
            return None;
        }
        let style = match world.tile(col, floor) {
            2 | 477 => {
                let zone = env.tree_x.iter().position(|&b| col <= b as i64).unwrap_or(3);
                match env.tree_tops[zone] {
                    0 => 0,
                    5 => 10,
                    n => 5 + n,
                }
            }
            23 | 661 => 1,
            70 => 14,
            199 | 662 => 5,
            60 => {
                (w, h) = (114, 96);
                if floor as f64 > surface(world) {
                    w = 116;
                    13
                } else if env.tree_tops[5] == 1 {
                    w = 116;
                    11
                } else {
                    2
                }
            }
            147 => match env.tree_tops[6] {
                0 if x % 10 == 0 => 18,
                0 => 12,
                n @ (2 | 3 | 32 | 4 | 42 | 5 | 7) => {
                    let half = world.width as i64 / 2;
                    let first = if n % 2 == 0 { x < half } else { x > half };
                    if first {
                        16
                    } else {
                        17
                    }
                }
                _ => 4,
            },
            109 | 492 => {
                // Hallow foliage follows the hallow background, not stored here: the default.
                h = 140;
                frame += [0, 3, 6][x.rem_euclid(3) as usize];
                3
            }
            _ => continue,
        };
        return Some((style, w, h, frame));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::wld::Tile;

    #[test]
    fn grass_plants_and_tables() {
        // Plants are 20 px tall, mirrored on even columns.
        assert_eq!(cell(3, 4, 0, 0), Draw { w: 16, h: 20, top: 0, flip: true });
        assert_eq!(cell(3, 5, 0, 0), Draw { w: 16, h: 20, top: 0, flip: false });
        // Only the bottom row of a table reaches 18 px.
        assert_eq!(cell(14, 0, 0, 0), Draw::CELL);
        assert_eq!(cell(14, 0, 0, 18).h, 18);
        assert_eq!(cell(1, 0, 0, 0), Draw::CELL);
    }

    #[test]
    fn wind_needs_a_breeze() {
        assert_eq!(wind_cycle(3, 10, 0.25, 0.05), 0.0);
        // At full strength the cycle swings 0.5 around the wind speed.
        assert!((wind_cycle(0, 0, 0.0, 0.3) - 0.8).abs() < 1e-5);
        assert!((wind_cycle(0, 0, 0.5, 0.3) + 0.2).abs() < 1e-5);
        let mut w = Wind::default();
        w.tick(1.2);
        assert!((w.grass - 5.0 / 180.0).abs() < 1e-12 && (w.tree - 3.0 / 240.0).abs() < 1e-12);
    }

    #[test]
    fn trunk_takes_its_ground() {
        // A two-cell trunk at x = 5 on each ground, above the surface (0.3 of the height).
        for (ground, biome) in [(23, 0), (2, -1), (60, 1)] {
            let w = World::parse(&formats::synth::write_world("t", 20, 40, &[], |x, y| Tile {
                tile: match (x, y) {
                    (5, 8 | 9) => 5,
                    (_, 10) => ground,
                    _ => NO_TILE,
                },
                ..Tile::default()
            }))
            .unwrap();
            assert_eq!(tree_biome(&w, 5, 8, 0, 0), biome);
        }
    }
}
