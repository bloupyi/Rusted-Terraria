//! Player body: an axis-aligned box moved against solid tiles, one axis at a
//! time. Units are world pixels (16 per tile) and seconds, y down.

use crate::{World, NO_TILE};

pub const WIDTH: f32 = 20.0;
pub const HEIGHT: f32 = 42.0;
const GRAVITY: f32 = 1800.0;
const MAX_FALL: f32 = 640.0;
const RUN_SPEED: f32 = 190.0;
const RUN_ACCEL: f32 = 1400.0;
const AIR_ACCEL: f32 = 900.0;
const JUMP_SPEED: f32 = 420.0;
/// Holding jump while rising cuts gravity, for a higher jump.
const JUMP_HOLD_GRAVITY: f32 = 0.55;
/// Liquids slow everything down.
const SWIM: f32 = 0.5;
const PLATFORM: u16 = 19;

#[derive(Default, Clone, Copy)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    /// Drops through platforms.
    pub down: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Player {
    /// Top-left corner.
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub on_ground: bool,
    pub facing_left: bool,
    /// Distance walked on the ground, for the walk animation.
    pub walked: f32,
}

impl Player {
    /// Standing on the first block from the top of column `tx`.
    pub fn spawn(world: &World, tx: u32) -> Self {
        let ty = (0..world.height as i64)
            .find(|&y| (0..2).any(|dx| world.solid(tx as i64 + dx, y)))
            .unwrap_or(world.height as i64);
        Self {
            x: tx as f32 * 16.0,
            y: ty as f32 * 16.0 - HEIGHT,
            vx: 0.0,
            vy: 0.0,
            on_ground: false,
            facing_left: false,
            walked: 0.0,
        }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + WIDTH / 2.0, self.y + HEIGHT / 2.0)
    }

    /// Whether the body overlaps tile `(tx, ty)`.
    pub fn overlaps(&self, tx: i64, ty: i64) -> bool {
        let (x0, y0) = (tx as f32 * 16.0, ty as f32 * 16.0);
        self.x < x0 + 16.0 && self.x + WIDTH > x0 && self.y < y0 + 16.0 && self.y + HEIGHT > y0
    }

    pub fn update(&mut self, world: &World, input: Input, dt: f32) {
        let wet = self.in_liquid(world);
        let k = if wet { SWIM } else { 1.0 };
        let dir = input.right as i32 as f32 - input.left as i32 as f32;
        let accel = if self.on_ground { RUN_ACCEL } else { AIR_ACCEL };
        let target = dir * RUN_SPEED * k;
        let dv = (target - self.vx).clamp(-accel * dt, accel * dt);
        self.vx += dv;
        if dir != 0.0 {
            self.facing_left = dir < 0.0;
        }
        if input.jump && (self.on_ground || wet) {
            self.vy = -JUMP_SPEED * if wet { 0.6 } else { 1.0 };
        }
        let g = if input.jump && self.vy < 0.0 { GRAVITY * JUMP_HOLD_GRAVITY } else { GRAVITY };
        self.vy = (self.vy + g * k * dt).min(MAX_FALL * k);

        // Sub-steps keep each move under a tile so nothing is tunnelled through.
        let steps = ((self.vx.abs().max(self.vy.abs()) * dt) / 8.0).ceil().max(1.0) as u32;
        let h = dt / steps as f32;
        let grounded = self.on_ground;
        self.on_ground = false;
        for _ in 0..steps {
            self.move_x(world, self.vx * h, grounded || self.on_ground);
            self.move_y(world, self.vy * h, input.down);
        }
        self.walked = if self.on_ground { self.walked + (self.vx * dt).abs() } else { 0.0 };
    }

    fn in_liquid(&self, world: &World) -> bool {
        let (cx, cy) = self.center();
        world.liquid((cx / 16.0).floor() as i64, (cy / 16.0).floor() as i64) > 64
    }

    fn tiles(&self, x: f32, y: f32) -> (i64, i64, i64, i64) {
        let eps = 0.01;
        (
            (x / 16.0).floor() as i64,
            ((x + WIDTH - eps) / 16.0).floor() as i64,
            (y / 16.0).floor() as i64,
            ((y + HEIGHT - eps) / 16.0).floor() as i64,
        )
    }

    /// World edges are walls.
    fn blocked(&self, world: &World, x: f32, y: f32) -> bool {
        let (x0, x1, y0, y1) = self.tiles(x, y);
        (y0..=y1).any(|ty| (x0..=x1).any(|tx| !world.in_bounds(tx, ty) || world.solid(tx, ty)))
    }

    fn move_x(&mut self, world: &World, dx: f32, grounded: bool) {
        if dx == 0.0 {
            return;
        }
        let nx = self.x + dx;
        if !self.blocked(world, nx, self.y) {
            self.x = nx;
            return;
        }
        // Walk up single-tile steps.
        let up = self.y - 16.0;
        if grounded && !self.blocked(world, nx, up) && !self.blocked(world, self.x, up) {
            let floor = ((self.y + HEIGHT) / 16.0).floor() * 16.0 - 16.0 - HEIGHT;
            if !self.blocked(world, nx, floor) {
                (self.x, self.y) = (nx, floor);
                return;
            }
        }
        self.x = if dx > 0.0 { ((nx + WIDTH) / 16.0).floor() * 16.0 - WIDTH } else { (nx / 16.0).ceil() * 16.0 };
        self.vx = 0.0;
    }

    fn move_y(&mut self, world: &World, dy: f32, drop: bool) {
        if dy == 0.0 {
            return;
        }
        let ny = self.y + dy;
        let platform = dy > 0.0 && !drop && self.lands_on_platform(world, ny);
        if !platform && !self.blocked(world, self.x, ny) {
            self.y = ny;
            return;
        }
        if dy > 0.0 {
            self.y = ((ny + HEIGHT) / 16.0).floor() * 16.0 - HEIGHT;
            self.on_ground = true;
        } else {
            self.y = (ny / 16.0).ceil() * 16.0;
        }
        self.vy = 0.0;
    }

    /// Feet cross the top of a platform tile during this move.
    fn lands_on_platform(&self, world: &World, ny: f32) -> bool {
        let (feet, nfeet) = (self.y + HEIGHT, ny + HEIGHT);
        let row = (nfeet / 16.0).floor();
        if feet > row * 16.0 + 0.01 {
            return false;
        }
        let (x0, x1, _, _) = self.tiles(self.x, ny);
        (x0..=x1).any(|tx| {
            let t = world.tile(tx, row as i64);
            t != NO_TILE && t == PLATFORM
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::wld::Tile;

    fn world(f: impl Fn(u32, u32) -> u16) -> World {
        let mut important = vec![false; 20];
        important[PLATFORM as usize] = true;
        World::parse(&formats::synth::write_world("p", 64, 64, &important, |x, y| Tile {
            tile: f(x, y),
            frame_x: 0,
            frame_y: 0,
            ..Tile::default()
        }))
        .unwrap()
    }

    fn run(p: &mut Player, w: &World, input: Input, secs: f32) {
        for _ in 0..(secs * 60.0) as u32 {
            p.update(w, input, 1.0 / 60.0);
        }
    }

    #[test]
    fn falls_lands_walks_and_steps_up() {
        // Floor at y = 40, a one-tile step at x >= 20, a two-tile wall at x = 30.
        let w = world(
            |x, y| {
                if y >= 40 || (x >= 20 && y == 39) || (x == 30 && (37..39).contains(&y)) {
                    1
                } else {
                    NO_TILE
                }
            },
        );
        let mut p =
            Player { x: 5.0 * 16.0, y: 0.0, vx: 0.0, vy: 0.0, on_ground: false, facing_left: false, walked: 0.0 };
        run(&mut p, &w, Input::default(), 2.0);
        assert!(p.on_ground);
        assert_eq!(p.y + HEIGHT, 40.0 * 16.0);
        run(&mut p, &w, Input { right: true, ..Input::default() }, 3.0);
        assert_eq!(p.y + HEIGHT, 39.0 * 16.0, "stepped up");
        assert_eq!(p.x + WIDTH, 30.0 * 16.0, "stopped by the wall");
        run(&mut p, &w, Input { jump: true, right: true, ..Input::default() }, 1.0);
        assert!(p.x > 30.0 * 16.0, "jumped over: {}", p.x);
    }

    #[test]
    fn platforms_hold_unless_dropping() {
        let w = world(|_, y| match y {
            20 => PLATFORM,
            y if y >= 40 => 1,
            _ => NO_TILE,
        });
        let mut p = Player { x: 100.0, y: 0.0, vx: 0.0, vy: 0.0, on_ground: false, facing_left: false, walked: 0.0 };
        run(&mut p, &w, Input::default(), 2.0);
        assert_eq!(p.y + HEIGHT, 20.0 * 16.0);
        run(&mut p, &w, Input { down: true, ..Input::default() }, 2.0);
        assert_eq!(p.y + HEIGHT, 40.0 * 16.0);
        // Jumping up through the platform from below.
        let mut p =
            Player { x: 100.0, y: 22.0 * 16.0, vx: 0.0, vy: -500.0, on_ground: false, facing_left: false, walked: 0.0 };
        run(&mut p, &w, Input { jump: true, ..Input::default() }, 0.2);
        assert!(p.y < 20.0 * 16.0);
    }
}
