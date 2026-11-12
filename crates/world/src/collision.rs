//! Box against tiles as the game's `Collision` does it: positions in world
//! pixels, velocities in pixels per tick, y down, gravity pulling down.

use crate::tile_sets::{PLATFORMS, SOLID, SOLID_TOP};
use crate::{flags, World, NO_TILE};
use formats::wld::Liquid;

pub type V2 = [f32; 2];

/// What collision needs to know about a cell.
#[derive(Clone, Copy, Default)]
struct Cell {
    /// Present and not actuated.
    active: bool,
    ty: u16,
    half: bool,
    /// 0 flat; 1 and 2 top surfaces falling to the right and to the left, 3 and 4 the same on the bottom.
    slope: u8,
    frame_y: i16,
}

impl Cell {
    fn solid(&self) -> bool {
        SOLID.has(self.ty)
    }

    fn solid_top(&self) -> bool {
        SOLID_TOP.has(self.ty)
    }

    fn platform(&self) -> bool {
        PLATFORMS.has(self.ty)
    }

    /// Blocks a moving box: a solid tile or the top row of a solid-top one.
    fn blocks(&self) -> bool {
        self.active && (self.solid() || (self.solid_top() && self.frame_y == 0))
    }
}

fn cell(world: &World, x: i64, y: i64) -> Cell {
    let t = world.tile(x, y);
    if t == NO_TILE {
        return Cell::default();
    }
    let f = world.flags(x, y);
    let shape = (f & flags::SHAPE_MASK) >> flags::SHAPE_SHIFT;
    Cell {
        active: f & flags::INACTIVE == 0,
        ty: t,
        half: shape == 1,
        slope: shape.saturating_sub(1) as u8,
        frame_y: world.frame(x, y).1,
    }
}

/// A box of `size` at `pos` (top-left) in `world`.
pub struct Box<'a> {
    pub world: &'a World,
    pub pos: V2,
    pub size: [i32; 2],
}

/// Result of [`Box::tiles`]: the allowed velocity, and whether a ceiling or a floor stopped it.
pub struct Hit {
    pub vel: V2,
    pub up: bool,
    pub down: bool,
}

impl Box<'_> {
    fn span(&self) -> (i64, i64, i64, i64) {
        let (w, h) = (self.world.width as i64, self.world.height as i64);
        let [x, y] = self.pos;
        let [bw, bh] = self.size.map(|k| k as f32);
        (
            ((x / 16.0) as i64 - 1).clamp(0, w - 1),
            (((x + bw) / 16.0) as i64 + 2).clamp(0, w - 1),
            ((y / 16.0) as i64 - 1).clamp(0, h - 40),
            (((y + bh) / 16.0) as i64 + 2).clamp(0, h - 40),
        )
    }

    /// `Collision.TileCollision`: the part of `vel` the tiles allow.
    pub fn tiles(&self, vel: V2, fall_through: bool, fall2: bool) -> Hit {
        self.tiles_grav(vel, fall_through, fall2, 1)
    }

    /// `tiles` for a box whose gravity points `grav_dir` (-1: up).
    pub fn tiles_grav(&self, vel: V2, fall_through: bool, fall2: bool, grav_dir: i32) -> Hit {
        let [px, py] = self.pos;
        let [w, h] = self.size.map(|k| k as f32);
        let next = [px + vel[0], py + vel[1]];
        let mut out = Hit { vel, up: false, down: false };
        let (x0, x1, y0, y1) = self.span();
        let (mut side_x, mut side_y, mut floor_x, mut floor_y) = (-1i64, -1i64, -1i64, -1i64);
        let mut floor = (y1 + 3) as f32 * 16.0;
        let hoik = !(self.size[0] <= 16 || self.size[1] <= 16);
        for i in x0..x1 {
            for j in y0..y1 {
                let c = cell(self.world, i, j);
                if !c.blocks() {
                    continue;
                }
                let tx = i as f32 * 16.0;
                let (mut ty, mut th) = (j as f32 * 16.0, 16.0);
                if c.half {
                    ty += 8.0;
                    th -= 8.0;
                }
                if !(next[0] + w > tx && next[0] < tx + 16.0 && next[1] + h > ty && next[1] < ty + th) {
                    continue;
                }
                let mut sloped = false;
                let mut skip = false;
                if c.slope > 2 {
                    skip = (c.slope == 3 && py + vel[0].abs() >= ty && px >= tx)
                        || (c.slope == 4 && py + vel[0].abs() >= ty && px + w <= tx + 16.0);
                } else if c.slope > 0 {
                    sloped = true;
                    skip = (c.slope == 1 && py + h - vel[0].abs() <= ty + th && px >= tx)
                        || (c.slope == 2 && py + h - vel[0].abs() <= ty + th && px + w <= tx + 16.0);
                }
                if skip {
                    continue;
                }
                if py + h <= ty {
                    out.down = true;
                    if (!(c.solid_top() && fall_through) || !(vel[1] <= 1.0 || fall2)) && floor > ty {
                        floor_x = i;
                        floor_y = j + (th < 16.0) as i64;
                        if floor_x != side_x && !sloped {
                            out.vel[1] = ty - (py + h) - if grav_dir == -1 { 0.01 } else { 0.0 };
                            floor = ty;
                        }
                    }
                } else if px + w <= tx && !c.solid_top() {
                    let left = cell(self.world, i - 1, j).slope;
                    if !hoik || i < 1 || (left != 2 && left != 4) {
                        (side_x, side_y) = (i, j);
                        if side_y != floor_y {
                            out.vel[0] = tx - (px + w);
                        }
                        if floor_x == side_x {
                            out.vel[1] = vel[1];
                        }
                    }
                } else if px >= tx + 16.0 && !c.solid_top() {
                    let right = cell(self.world, i + 1, j).slope;
                    if !hoik || (right != 1 && right != 3) {
                        (side_x, side_y) = (i, j);
                        if side_y != floor_y {
                            out.vel[0] = tx + 16.0 - px;
                        }
                        if floor_x == side_x {
                            out.vel[1] = vel[1];
                        }
                    }
                } else if py >= ty + th && !c.solid_top() {
                    out.up = true;
                    (floor_x, floor_y) = (i, j);
                    out.vel[1] = ty + th - py + if grav_dir == 1 { 0.01 } else { 0.0 };
                    if floor_y == side_y {
                        out.vel[0] = vel[0];
                    }
                }
            }
        }
        out
    }

    /// `Collision.StepUp`: lifts the box onto a one-tile step ahead of it.
    /// Returns the new top and the speed the drawn offset catches up at.
    pub fn step_up(&self, vel: V2) -> Option<(f32, f32)> {
        self.step_up_grav(vel, 1)
    }

    /// `step_up` for a box whose gravity points `grav_dir`; with -1 it steps
    /// along a ceiling and returns its new top, its vertical speed stopped.
    pub fn step_up_grav(&self, vel: V2, grav_dir: i32) -> Option<(f32, f32)> {
        let [px, py] = self.pos;
        let [w, h] = self.size;
        let g = grav_dir as i64;
        let dir = (vel[0] > 0.0) as i64 - (vel[0] < 0.0) as i64;
        let nx = px + vel[0];
        let col = ((nx + (w / 2) as f32 + ((w / 2 + 1) as i64 * dir) as f32) / 16.0) as i64;
        let row = if g == 1 { ((py + h as f32 - 1.0) / 16.0) as i64 } else { ((py as f64 + 0.1) / 16.0) as i64 };
        let rows = (h / 16 + (h % 16 != 0) as i32) as i64;
        let world = self.world;
        let inner = |x: i64, y: i64| x > 1 && y > 1 && x < world.width as i64 - 2 && y < world.height as i64 - 2;
        if !inner(col, row) || row >= world.height as i64 - 40 {
            return None;
        }
        if (1..rows + 2).any(|k| !inner(col, row - k * g)) || !inner(col - dir, row - rows * g) {
            return None;
        }
        let open = |c: Cell| !c.active || !c.solid() || c.solid_top();
        let clear = (2..rows + 1).all(|k| open(cell(world, col, row - k * g)));
        let behind = open(cell(world, col - dir, row - rows * g));
        if g == -1 {
            let (near, far) = (cell(world, col, row + 1), cell(world, col, row + rows + 1));
            let head = open(near) || near.slope != 0 || (near.half && open(far));
            let (step, below) = (cell(world, col, row), cell(world, col, row + 1));
            let foot = (step.active && step.solid() && !step.solid_top()) || (below.half && below.active);
            let (cx, cx1) = (col as f32 * 16.0, col as f32 * 16.0 + 16.0);
            if !(cx < nx + w as f32 && cx1 > nx) || !(foot && head && clear && behind) {
                return None;
            }
            if step.slope > 2 || below.platform() {
                return None;
            }
            let ceiling = row as f32 * 16.0 + 16.0;
            let drop = ceiling - py;
            return (ceiling > py && drop <= 16.1).then_some((ceiling, if drop < 9.0 { 1.0 } else { 2.0 }));
        }
        let above = cell(world, col, row - 1);
        let over = cell(world, col, row - (rows + 1));
        let mid = px + (w / 2) as f32;
        let (cx, cx1) = (col as f32 * 16.0, col as f32 * 16.0 + 16.0);
        let head = open(above)
            || (above.slope == 1 && mid > cx)
            || (above.slope == 2 && mid < cx1)
            || (above.half && open(over));
        let step = cell(world, col, row);
        let top = cell(world, col, row - 1);
        let top_slope = matches!(step.slope, 1 | 2);
        let mut foot = (step.active
            && (!top_slope || (step.slope == 1 && mid < cx) || (step.slope == 2 && mid > cx1))
            && (!top_slope || py + h as f32 > row as f32 * 16.0)
            && step.solid()
            && !step.solid_top())
            || (top.half && top.active);
        foot &= !step.solid_top() || !top.solid_top();
        if !(cx < nx + w as f32 && cx1 > nx) || !(foot && head && clear && behind) {
            return None;
        }
        let mut floor = row as f32 * 16.0;
        if top.half {
            floor -= 8.0;
        } else if step.half {
            floor += 8.0;
        }
        let rise = py + h as f32 - floor;
        (rise > 0.0 && rise <= 16.1).then_some((floor - h as f32, if rise < 9.0 { 1.0 } else { 2.0 }))
    }

    /// `Collision.StepDown`: keeps a walking box on the ground down a one-tile step.
    /// Returns the new top and the speed the drawn offset catches up at.
    pub fn step_down(&self, vel: V2) -> Option<(f32, f32)> {
        let [px, py] = self.pos;
        let [w, h] = self.size;
        let nx = px + vel[0];
        let ny = ((py + h as f32) / 16.0).floor() * 16.0 - h as f32;
        let (x0, x1) = ((nx / 16.0) as i64, ((nx + w as f32) / 16.0) as i64);
        let row = ((ny + h as f32 + 4.0) / 16.0) as i64;
        let rows = (h / 16 + (h % 16 != 0) as i32) as i64;
        let mut floor = (row + rows) as f32 * 16.0;
        let bottom = self.world.height as f32 - 42.0;
        for i in x0..=x1 {
            for j in row..=row + 1 {
                if !(i > 1 && j > 1 && i < self.world.width as i64 - 2 && j < self.world.height as i64 - 2) {
                    continue;
                }
                let c = cell(self.world, i, j);
                if j as f32 >= bottom || (c.active && (c.solid() || c.solid_top())) {
                    let top = j as f32 * 16.0 + if c.half { 8.0 } else { 0.0 };
                    let (rx, ry) = (i as f32 * 16.0, j as f32 * 16.0 - 17.0);
                    let hits = rx < px + w as f32 && px < rx + 16.0 && ry < py + h as f32 && py < ry + 16.0;
                    if hits && top < floor {
                        floor = top;
                    }
                }
            }
        }
        let drop = floor - (py + h as f32);
        (drop > 7.0 && drop < 17.0).then_some((floor - h as f32, if drop > 9.0 { 2.5 } else { 1.5 }))
    }

    /// `Collision.SlopeCollision`: rides the box along sloped tiles it overlaps.
    /// Returns position and velocity, whether it stands on a sloped platform or
    /// falls through one, and whether a ceiling or a floor stopped it.
    pub fn slopes(&self, vel: V2, fall: bool) -> (V2, V2, bool, bool, bool, bool) {
        let [px, py] = self.pos;
        let [w, h] = self.size.map(|k| k as f32);
        let (mut stair, mut stair_fall) = (false, false);
        let mut hit = [false; 5];
        let (mut low, mut high) = (py, py);
        let mut pos = self.pos;
        let mut v = vel;
        let (x0, x1, y0, y1) = self.span();
        for i in x0..x1 {
            for j in y0..y1 {
                let c = cell(self.world, i, j);
                if !c.blocks() {
                    continue;
                }
                let tx = i as f32 * 16.0;
                let (mut ty, mut th) = (j as f32 * 16.0, 16.0);
                if c.half {
                    ty += 8.0;
                    th -= 8.0;
                }
                if !(px + w > tx && px < tx + 16.0 && py + h > ty && py < ty + th) {
                    continue;
                }
                if c.platform() {
                    let jy = j as f32 * 16.0;
                    if vel[1] < 0.0
                        || py + h < jy
                        || py + h - (1.0 + vel[0].abs()) > jy + 16.0
                        || (((c.slope == 1 && vel[0] >= 0.0) || (c.slope == 2 && vel[0] <= 0.0))
                            && (py + h) / 16.0 - 1.0 == j as f32)
                    {
                        continue;
                    }
                }
                let falls = fall && c.platform();
                let ty = j as f32 * 16.0;
                if !(px + w > tx && px < tx + 16.0 && py + h > ty && py < ty + 16.0) {
                    continue;
                }
                if c.slope == 3 || c.slope == 4 {
                    let d = if c.slope == 3 { px - tx } else { tx + 16.0 - (px + w) };
                    if d >= 0.0 {
                        if py <= ty + 16.0 - d {
                            let lift = ty + 16.0 - py - d;
                            if py + lift > high {
                                pos[1] = py + lift;
                                high = pos[1];
                                v[1] = v[1].max(0.0101);
                                hit[c.slope as usize] = true;
                            }
                        }
                    } else if py > ty && pos[1] < ty + 16.0 {
                        pos[1] = ty + 16.0;
                        v[1] = v[1].max(0.0101);
                    }
                }
                if c.slope != 1 && c.slope != 2 {
                    continue;
                }
                let d = if c.slope == 1 { px - tx } else { tx + 16.0 - (px + w) };
                if d >= 0.0 {
                    if py + h < ty + d {
                        continue;
                    }
                    let lift = ty - (py + h) + d;
                    if py + lift >= low {
                        continue;
                    }
                    if falls {
                        stair_fall = true;
                        continue;
                    }
                    stair = c.platform();
                    pos[1] = py + lift;
                    low = pos[1];
                    v[1] = v[1].min(0.0);
                    hit[c.slope as usize] = true;
                    continue;
                }
                if c.platform() && py + h - 4.0 - vel[0].abs() > ty {
                    stair_fall |= falls;
                    continue;
                }
                let top = ty - h;
                if pos[1] <= top {
                    continue;
                }
                if falls {
                    stair_fall = true;
                    continue;
                }
                stair = c.platform();
                pos[1] = top;
                v[1] = v[1].min(0.0);
            }
        }
        let want = [pos[0] - px, pos[1] - py];
        let r = self.tiles(want, false, false);
        let (got, mut up, down) = (r.vel, r.up, r.down);
        if got[1] > want[1] {
            let d = want[1] - got[1];
            pos[1] = py + got[1];
            if hit[1] {
                pos[0] = px - d;
            }
            if hit[2] {
                pos[0] = px + d;
            }
            v = [0.0, 0.0];
            up = false;
        } else if got[1] < want[1] {
            let d = got[1] - want[1];
            pos[1] = py + got[1];
            if hit[3] {
                pos[0] = px - d;
            }
            if hit[4] {
                pos[0] = px + d;
            }
            v = [0.0, 0.0];
        }
        (pos, v, stair, stair_fall, up, down)
    }

    /// `Collision.WalkDownSlope`: a box standing on a top slope follows it down.
    pub fn walk_down_slope(&self, vel: V2, gravity: f32) -> V2 {
        if vel[1] != gravity {
            return vel;
        }
        let [px, py] = self.pos;
        let [w, h] = self.size;
        let (ww, wh) = (self.world.width as i64, self.world.height as i64);
        let x0 = ((px / 16.0) as i64).clamp(0, ww - 1);
        let x1 = (((px + w as f32) / 16.0) as i64).clamp(0, ww - 1);
        let row = (((py + h as f32 + 4.0) / 16.0) as i64).clamp(0, wh - 42);
        let mut best = (row + 3) as f32 * 16.0;
        let mut at: Option<(i64, i64)> = None;
        let want = if vel[0] < 0.0 { 2 } else { 1 };
        let rect = |x: i32, y: i32, rw: i32, rh: i32| (x, y, rw, rh);
        let me = rect(px as i32, py as i32, w, h);
        for i in x0..=x1 {
            for j in row..=row + 1 {
                let c = cell(self.world, i, j);
                if !c.active || (!c.solid() && !c.solid_top()) {
                    continue;
                }
                let top = j as f32 * 16.0 + if c.half { 8.0 } else { 0.0 };
                let r = rect(i as i32 * 16, j as i32 * 16 - 17, 16, 16);
                let meets = r.0 < me.0 + me.2 && me.0 < r.0 + r.2 && r.1 < me.1 + me.3 && me.1 < r.1 + r.3;
                if !meets || top > best {
                    continue;
                }
                if top == best {
                    if c.slope == 0 {
                        continue;
                    }
                    let prev_sloped = at.is_some_and(|(x, y)| cell(self.world, x, y).slope != 0);
                    if prev_sloped && c.slope != want {
                        continue;
                    }
                }
                best = top;
                at = Some((i, j));
            }
        }
        let mut v = vel;
        if let Some((i, j)) = at {
            let c = cell(self.world, i, j);
            let (tx, ty) = (i as f32 * 16.0, j as f32 * 16.0);
            match c.slope {
                2 if py + h as f32 >= ty + (tx + 16.0 - (px + w as f32)) && vel[0] < 0.0 => v[1] += vel[0].abs(),
                1 if py + h as f32 >= ty + (px - tx) && vel[0] > 0.0 => v[1] += vel[0].abs(),
                _ => {}
            }
        }
        v
    }
}

/// `Collision.CanHit`: a walk of tiles from the middle of one box to the
/// other's, blocked by a solid tile or a gap between two full blocks.
pub fn can_hit(world: &World, a: V2, a_size: [i32; 2], b: V2, b_size: [i32; 2]) -> bool {
    let (w, h) = (world.width as i64, world.height as i64);
    let tile = |p: V2, s: [i32; 2]| {
        let x = (p[0] as i64 + (s[0] / 2) as i64) / 16;
        let y = (p[1] as i64 + (s[1] / 2) as i64) / 16;
        (x.clamp(1, w - 1), y.clamp(1, h - 40))
    };
    let full = |x: i64, y: i64| {
        let c = cell(world, x, y);
        c.active && c.solid() && !c.solid_top() && c.slope == 0 && !c.half
    };
    let ((mut x, mut y), (x2, y2)) = (tile(a, a_size), tile(b, b_size));
    loop {
        if x == x2 && y == y2 {
            return true;
        }
        if (x - x2).abs() > (y - y2).abs() {
            x += if x >= x2 { -1 } else { 1 };
            if full(x, y - 1) && full(x, y + 1) {
                return false;
            }
        } else {
            y += if y >= y2 { -1 } else { 1 };
            if full(x - 1, y) && full(x + 1, y) {
                return false;
            }
        }
        let c = cell(world, x, y);
        if c.active && c.solid() && !c.solid_top() {
            return false;
        }
    }
}

/// `Collision.WetCollision`: the liquid kind touching the middle of the box, if any.
pub fn wet(world: &World, pos: V2, size: [i32; 2]) -> Option<u8> {
    let [w, h] = size;
    let (mw, mh) = (10.min(w), h / 2);
    let (cx, cy) = (pos[0] + (w / 2) as f32 - (mw / 2) as f32, pos[1] + (h / 2) as f32 - (mh / 2) as f32);
    let b = Box { world, pos, size };
    let (x0, x1, y0, y1) = b.span();
    let hits =
        |x: f32, y: f32, ch: i32| cx + mw as f32 > x && cx < x + 16.0 && cy + mh as f32 > y && cy < y + ch as f32;
    for i in x0..x1 {
        for j in y0..y1 {
            let kind = |y: i64| {
                let (c, k) = world.cell(i as u32, y as u32);
                c.liquid_kind[k]
            };
            let amount = world.liquid(i, j);
            if amount > 0 {
                let gap = (256 - amount as i32) as f32 / 32.0 * 2.0;
                if hits(i as f32 * 16.0, j as f32 * 16.0 + gap, 16 - gap as i32) {
                    return Some(kind(j));
                }
            } else if j > 0
                && world.tile(i, j) != NO_TILE
                && cell(world, i, j).slope != 0
                && world.liquid(i, j - 1) > 0
                && hits(i as f32 * 16.0, j as f32 * 16.0, 16)
            {
                return Some(kind(j - 1));
            }
        }
    }
    None
}

/// The liquid kind at a cell.
fn liquid_kind(world: &World, x: i64, y: i64) -> u8 {
    let (c, k) = world.cell(x as u32, y as u32);
    c.liquid_kind[k]
}

/// Whether a box `[x, y, w, h]` overlaps the liquid of cells around `pos`
/// accepted by `keep`, each liquid's top lowered by how little it holds.
fn liquid_hit(world: &World, pos: V2, size: [i32; 2], rect: [f32; 4], keep: impl Fn(i64, i64, i64) -> bool) -> bool {
    let (x0, x1, y0, y1) = Box { world, pos, size }.span();
    let [rx, ry, rw, rh] = rect;
    for i in x0..x1 {
        for j in y0..y1 {
            let amount = world.liquid(i, j);
            if amount == 0 || !keep(i, j, y0) {
                continue;
            }
            let gap = (256 - amount as i32) as f32 / 32.0;
            let (x, y, h) = (i as f32 * 16.0, j as f32 * 16.0 + gap * 2.0, 16 - (gap * 2.0) as i32);
            if rx + rw > x && rx < x + 16.0 && ry + rh > y && ry < y + h as f32 {
                return true;
            }
        }
    }
    false
}

/// `Collision.LavaCollision`: lava anywhere over the box.
pub fn lava(world: &World, pos: V2, size: [i32; 2]) -> bool {
    let rect = [pos[0], pos[1], size[0] as f32, size[1] as f32];
    liquid_hit(world, pos, size, rect, |i, j, _| liquid_kind(world, i, j) == Liquid::Lava as u8)
}

/// `Collision.DrownCollision` the right way up: water or honey over a 10 by
/// 12 box at the top of the head. A solid block in the first row checked
/// keeps out what sits in it.
pub fn drown(world: &World, pos: V2, size: [i32; 2]) -> bool {
    let (mw, mh) = (10.min(size[0]), 12.min(size[1]));
    let rect = [pos[0] + (size[0] / 2) as f32 - (mw / 2) as f32, pos[1] - 2.0, mw as f32, mh as f32];
    liquid_hit(world, pos, size, rect, |i, j, top| {
        let k = liquid_kind(world, i, j);
        let c = cell(world, i, j);
        k != Liquid::Lava as u8
            && k != Liquid::Shimmer as u8
            && (j != top || world.tile(i, j) == NO_TILE || !c.solid() || c.solid_top())
    })
}

/// `Collision.WaterCollision` with lava walked on too: the fall `vel[1]`
/// cut short at the top of any liquid under the box's feet.
pub fn water_top(world: &World, pos: V2, vel: V2, size: [i32; 2]) -> f32 {
    let (x0, x1, y0, y1) = Box { world, pos, size }.span();
    let [w, h] = size.map(|k| k as f32);
    let (nx, ny) = (pos[0] + vel[0], pos[1] + vel[1]);
    let mut vy = vel[1];
    for i in x0..x1 {
        for j in y0..y1 {
            let amount = world.liquid(i, j);
            if amount == 0 || j == 0 || world.liquid(i, j - 1) != 0 {
                continue;
            }
            let lh = (amount / 32 * 2 + 2) as f32;
            let (x, y) = (i as f32 * 16.0, j as f32 * 16.0 + 16.0 - lh);
            if nx + w > x && nx < x + 16.0 && ny + h > y && ny < y + lh && pos[1] + h <= y {
                vy = y - (pos[1] + h);
            }
        }
    }
    vy
}

/// `WorldGen.SolidTile`: a whole, flat, solid block.
pub fn solid_tile(world: &World, x: i64, y: i64) -> bool {
    let c = cell(world, x, y);
    c.active && c.solid() && !c.solid_top() && !c.half && c.slope == 0
}

/// `WorldGen.SolidTileNoPlatforms`: solid or solid on top, but not a platform.
/// Outside the world counts as solid.
pub fn solid_no_platforms(world: &World, x: i64, y: i64) -> bool {
    if !world.in_bounds(x, y) {
        return true;
    }
    let c = cell(world, x, y);
    c.active && !c.platform() && (c.solid() || c.solid_top())
}

/// `WorldGen.SolidTileAllowBottomSlope`: something to stand on that is not
/// sloped on top nor half a block.
pub fn solid_allow_bottom_slope(world: &World, x: i64, y: i64) -> bool {
    if !world.in_bounds(x, y) {
        return true;
    }
    let c = cell(world, x, y);
    c.active && (c.solid() || c.solid_top()) && !matches!(c.slope, 1 | 2) && !c.half
}

/// The shape of a present block: 0 flat or half, 1 and 2 sloped on top, 3 and 4 on the bottom.
pub fn slope(world: &World, x: i64, y: i64) -> u8 {
    let c = cell(world, x, y);
    if c.active {
        c.slope
    } else {
        0
    }
}

/// The tile queries of the fighters' step-up: present, `tileSolid`,
/// `tileSolidTop`, half block, sloped on top.
pub fn shape(world: &World, x: i64, y: i64) -> (bool, bool, bool, bool, bool) {
    let c = cell(world, x, y);
    (c.active, c.solid(), c.solid_top(), c.half, matches!(c.slope, 1 | 2))
}

impl Box<'_> {
    /// `Collision.SolidCollision`: a solid block, not one solid only on top, overlaps the box.
    pub fn solid_collision(&self) -> bool {
        let (x0, x1, y0, y1) = self.span();
        let [px, py] = self.pos;
        let [w, h] = self.size.map(|k| k as f32);
        (x0..x1).any(|i| {
            (y0..y1).any(|j| {
                let c = cell(self.world, i, j);
                if !c.active || !c.solid() || c.solid_top() {
                    return false;
                }
                let (tx, ty, th) = (
                    i as f32 * 16.0,
                    j as f32 * 16.0 + if c.half { 8.0 } else { 0.0 },
                    if c.half { 8.0 } else { 16.0 },
                );
                px + w > tx && px < tx + 16.0 && py + h > ty && py < ty + th
            })
        })
    }
}
