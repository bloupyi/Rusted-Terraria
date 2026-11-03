//! Liquid flow as a cellular automaton over active chunks.
//!
//! Each tick, cells are visited bottom to top: a liquid first falls into the
//! cell below, then levels with its side neighbours. Water touching lava turns
//! the lava cell into obsidian. A chunk where nothing moved goes to sleep until
//! a change next to it wakes it.

use crate::{Liquid, World, CHUNK};

pub const OBSIDIAN: u16 = 56;
const WATER: u8 = Liquid::Water as u8;
const LAVA: u8 = Liquid::Lava as u8;

pub struct LiquidSim {
    active: Vec<bool>,
    list: Vec<u32>,
    flip: bool,
}

impl LiquidSim {
    /// Starts with every chunk holding liquid awake; settled ones sleep after a tick.
    pub fn new(world: &World) -> Self {
        let mut sim = Self { active: vec![false; world.chunks.len()], list: Vec::new(), flip: false };
        for (ci, c) in world.chunks.iter().enumerate() {
            if c.liquid.iter().any(|&l| l != 0) {
                sim.wake_chunk(world, (ci as u32 % world.chunks_x) as i64, (ci as u32 / world.chunks_x) as i64);
            }
        }
        sim
    }

    pub fn active(&self) -> usize {
        self.list.len()
    }

    fn wake_chunk(&mut self, world: &World, cx: i64, cy: i64) {
        if cx < 0 || cy < 0 || cx >= world.chunks_x as i64 || cy >= world.chunks_y as i64 {
            return;
        }
        let ci = (cy as u32 * world.chunks_x + cx as u32) as usize;
        if !self.active[ci] {
            self.active[ci] = true;
            self.list.push(ci as u32);
        }
    }

    /// Wakes the chunks around a tile after an edit there.
    pub fn wake(&mut self, world: &World, x: u32, y: u32) {
        let (cx, cy) = ((x / CHUNK) as i64, (y / CHUNK) as i64);
        for (dx, dy) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)] {
            self.wake_chunk(world, cx + dx, cy + dy);
        }
    }

    pub fn tick(&mut self, world: &mut World) {
        let mut list = std::mem::take(&mut self.list);
        // Bottom chunks first so liquid falls through chunk borders in one pass.
        list.sort_unstable_by_key(|&ci| std::cmp::Reverse(ci / world.chunks_x));
        for &ci in &list {
            self.active[ci as usize] = false;
        }
        self.flip = !self.flip;
        for ci in list {
            let (cx, cy) = (ci % world.chunks_x, ci / world.chunks_x);
            if step_chunk(world, cx, cy, self.flip) {
                for (dx, dy) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)] {
                    self.wake_chunk(world, cx as i64 + dx, cy as i64 + dy);
                }
            }
        }
    }
}

/// Open to liquid: in bounds and not a block.
fn open(world: &World, x: i64, y: i64) -> bool {
    world.in_bounds(x, y) && !world.solid(x, y)
}

fn liquid_at(world: &World, x: u32, y: u32) -> (u8, u8) {
    let (c, i) = world.cell(x, y);
    (c.liquid[i], c.liquid_kind[i])
}

/// Water and lava meeting: the lava side hardens.
fn mixes(a: u8, b: u8) -> bool {
    (a == WATER && b == LAVA) || (a == LAVA && b == WATER)
}

fn harden(world: &mut World, x: u32, y: u32) {
    world.set_liquid(x, y, 0, 0);
    world.set_tile(x, y, OBSIDIAN, -1, -1);
}

fn step_chunk(world: &mut World, cx: u32, cy: u32, flip: bool) -> bool {
    let x0 = cx * CHUNK;
    let x1 = (x0 + CHUNK).min(world.width);
    let y0 = cy * CHUNK;
    let y1 = (y0 + CHUNK).min(world.height);
    if world.chunk(cx, cy).liquid.iter().all(|&l| l == 0) {
        return false;
    }
    let mut moved = false;
    for y in (y0..y1).rev() {
        for k in 0..x1 - x0 {
            let x = if flip { x1 - 1 - k } else { x0 + k };
            moved |= step_cell(world, x, y);
        }
    }
    moved
}

fn step_cell(world: &mut World, x: u32, y: u32) -> bool {
    let (mut amount, kind) = liquid_at(world, x, y);
    if amount == 0 {
        return false;
    }
    let (xi, yi) = (x as i64, y as i64);
    let mut moved = false;

    // Fall.
    let below_open = open(world, xi, yi + 1);
    if below_open {
        let (b, bk) = liquid_at(world, x, y + 1);
        if b != 0 && mixes(kind, bk) {
            let (hx, hy) = if kind == LAVA { (x, y) } else { (x, y + 1) };
            harden(world, hx, hy);
            return true;
        }
        if b == 0 || bk == kind {
            let m = amount.min(255 - b);
            if m > 0 {
                world.set_liquid(x, y + 1, b + m, kind);
                amount -= m;
                world.set_liquid(x, y, amount, kind);
                moved = true;
            }
        }
    }
    if amount == 0 {
        return moved;
    }
    // Level with side neighbours, only once resting on something.
    let (b, _) = if below_open { liquid_at(world, x, y + 1) } else { (255, kind) };
    if below_open && b < 255 {
        return moved;
    }
    let mut cells = [(x, amount); 3];
    let mut n = 1;
    let mut total = amount as u32;
    for dx in [-1i64, 1] {
        let nx = xi + dx;
        if !open(world, nx, yi) {
            continue;
        }
        let (l, lk) = liquid_at(world, nx as u32, y);
        if l != 0 && lk != kind {
            if mixes(kind, lk) {
                let (hx, hy) = if lk == LAVA { (nx as u32, y) } else { (x, y) };
                harden(world, hx, hy);
                return true;
            }
            continue;
        }
        if amount > l.saturating_add(1) {
            cells[n] = (nx as u32, l);
            n += 1;
            total += l as u32;
        }
    }
    if n == 1 {
        // A thin film that cannot spread any more dries up.
        if amount <= 2 && !below_open {
            world.set_liquid(x, y, 0, 0);
            return true;
        }
        return moved;
    }
    let avg = total / n as u32;
    let rem = total % n as u32;
    for (j, &(cx, _)) in cells[..n].iter().enumerate() {
        let v = avg + if j == 0 { rem } else { 0 };
        world.set_liquid(cx, y, v as u8, kind);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::wld::Tile;

    fn total(world: &World) -> u32 {
        world.chunks.iter().flat_map(|c| c.liquid.iter()).map(|&l| l as u32).sum()
    }

    #[test]
    fn falls_levels_and_sleeps() {
        // A box 20 wide with a floor at y = 40, a full water cell dropped at (5, 2).
        let mut world = World::parse(&formats::synth::write_world("w", 64, 64, &[], |x, y| Tile {
            tile: if y == 40 || x == 0 || x == 21 { 1 } else { crate::NO_TILE },
            liquid: if (5..9).contains(&x) && y == 2 { 255 } else { 0 },
            ..Tile::default()
        }))
        .unwrap();
        let start = total(&world);
        let mut sim = LiquidSim::new(&world);
        for _ in 0..2000 {
            sim.tick(&mut world);
        }
        assert_eq!(sim.active(), 0, "settled");
        let floor: Vec<u8> = (1..21).map(|x| liquid_at(&world, x, 39).0).collect();
        let (lo, hi) = (*floor.iter().min().unwrap(), *floor.iter().max().unwrap());
        // Neighbours stop exchanging at a difference of 1: a slope of at most 1.5 px over the box.
        assert!(lo > 30 && hi - lo <= 24, "{floor:?}");
        // Only the dried-up film may be lost.
        assert!(start - total(&world) < 40, "{} -> {}", start, total(&world));
        assert_eq!(liquid_at(&world, 5, 2).0, 0);
    }

    #[test]
    fn water_on_lava_makes_obsidian() {
        let mut world = World::parse(&formats::synth::write_world("o", 32, 32, &[], |x, y| Tile {
            tile: if y == 20 { 1 } else { crate::NO_TILE },
            liquid: if x == 5 && (y == 10 || y == 19) { 255 } else { 0 },
            liquid_kind: if y == 19 { Liquid::Lava } else { Liquid::Water },
            ..Tile::default()
        }))
        .unwrap();
        let mut sim = LiquidSim::new(&world);
        for _ in 0..200 {
            sim.tick(&mut world);
        }
        assert_eq!(world.tile(5, 19), OBSIDIAN);
    }
}
