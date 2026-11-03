//! A box moved against solid tiles, one axis at a time, shared by the player
//! and mobs. Units are world pixels (16 per tile) and seconds, y down.

use crate::{World, NO_TILE};

pub const GRAVITY: f32 = 1800.0;
pub const MAX_FALL: f32 = 640.0;
const PLATFORM: u16 = 19;

#[derive(Clone, Copy, Debug, Default)]
pub struct Body {
    /// Top-left corner.
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub w: f32,
    pub h: f32,
    pub on_ground: bool,
}

impl Body {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, ..Self::default() }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// Whether the body overlaps tile `(tx, ty)`.
    pub fn overlaps(&self, tx: i64, ty: i64) -> bool {
        let (x0, y0) = (tx as f32 * 16.0, ty as f32 * 16.0);
        self.x < x0 + 16.0 && self.x + self.w > x0 && self.y < y0 + 16.0 && self.y + self.h > y0
    }

    pub fn touches(&self, o: &Body) -> bool {
        self.x < o.x + o.w && self.x + self.w > o.x && self.y < o.y + o.h && self.y + self.h > o.y
    }

    /// Moves by the velocity for `dt`; `drop` falls through platforms.
    pub fn step(&mut self, world: &World, dt: f32, drop: bool) {
        // Sub-steps keep each move under a tile so nothing is tunnelled through.
        let steps = ((self.vx.abs().max(self.vy.abs()) * dt) / 8.0).ceil().max(1.0) as u32;
        let h = dt / steps as f32;
        let grounded = self.on_ground;
        self.on_ground = false;
        for _ in 0..steps {
            self.move_x(world, self.vx * h, grounded || self.on_ground);
            self.move_y(world, self.vy * h, drop);
        }
    }

    pub fn in_liquid(&self, world: &World) -> bool {
        let (cx, cy) = self.center();
        world.liquid((cx / 16.0).floor() as i64, (cy / 16.0).floor() as i64) > 64
    }

    fn tiles(&self, x: f32, y: f32) -> (i64, i64, i64, i64) {
        let eps = 0.01;
        (
            (x / 16.0).floor() as i64,
            ((x + self.w - eps) / 16.0).floor() as i64,
            (y / 16.0).floor() as i64,
            ((y + self.h - eps) / 16.0).floor() as i64,
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
            let floor = ((self.y + self.h) / 16.0).floor() * 16.0 - 16.0 - self.h;
            if !self.blocked(world, nx, floor) {
                (self.x, self.y) = (nx, floor);
                return;
            }
        }
        self.x = if dx > 0.0 { ((nx + self.w) / 16.0).floor() * 16.0 - self.w } else { (nx / 16.0).ceil() * 16.0 };
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
            self.y = ((ny + self.h) / 16.0).floor() * 16.0 - self.h;
            self.on_ground = true;
        } else {
            self.y = (ny / 16.0).ceil() * 16.0;
        }
        self.vy = 0.0;
    }

    /// Feet cross the top of a platform tile during this move.
    fn lands_on_platform(&self, world: &World, ny: f32) -> bool {
        let (feet, nfeet) = (self.y + self.h, ny + self.h);
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
