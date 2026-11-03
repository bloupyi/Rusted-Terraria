//! Game state outside the window: the world, liquids, player, time of day, and
//! keeping the GPU copies (meshes, light, map) in step with edits.

use render::{ColorRect, Renderer, Scene};
use world::light::Rect;
use world::liquid::{LiquidSim, OBSIDIAN};
use world::player::{self, Input, Player};
use world::{Liquid, World, CHUNK, NO_TILE};

pub const LIQUID_TICK: f32 = 1.0 / 30.0;
/// Seconds for a full day: 15 minutes of day, 9 of night.
pub const DAY_LENGTH: f32 = 24.0 * 60.0;
const DAYTIME: f32 = 15.0 / 24.0;
/// Reach for breaking and placing, in tiles from the player's centre.
const REACH: f32 = 10.0;
const STALE_LIGHT: u8 = 1;
const STALE_LOD: u8 = 2;
const LIGHT_BUDGET: usize = 64;
const LOD_BUDGET: usize = 256;
/// Off-screen chunks remeshed per frame; on-screen ones always are.
const MESH_BUDGET: usize = 64;
/// Tile types placeable from the start even if the world has none.
pub const EXTRA_TILES: [u16; 3] = [0, 1, OBSIDIAN];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Block,
    Liquid(Liquid),
}

pub struct Game {
    pub world: World,
    pub scene: Scene,
    pub sim: LiquidSim,
    pub player: Player,
    /// 0 at dawn, 1 a full day later.
    pub time: f32,
    pub tool: Tool,
    blocks: Vec<u16>,
    block: usize,
    stale: Vec<u8>,
    lod_queue: Vec<u32>,
    /// Per chunk, `DIRTY_*` bits not yet remeshed.
    mesh_pending: Vec<u8>,
    mesh_queue: std::collections::VecDeque<u32>,
    liquid_acc: f32,
}

impl Game {
    pub fn new(world: World, scene: Scene) -> Self {
        let important = &world.header.frame_important;
        let (mut blocks, _) = world.used_types();
        blocks.extend_from_slice(&EXTRA_TILES);
        blocks.retain(|&t| !important.get(t as usize).copied().unwrap_or(false));
        blocks.sort_unstable();
        blocks.dedup();
        let sim = LiquidSim::new(&world);
        let player = Player::spawn(&world, world.width / 2);
        let stale = vec![0; world.chunks.len()];
        let mesh_pending = vec![0; world.chunks.len()];
        Self {
            world,
            scene,
            sim,
            player,
            time: 0.25,
            tool: Tool::Block,
            blocks,
            block: 0,
            stale,
            lod_queue: Vec::new(),
            mesh_pending,
            mesh_queue: Default::default(),
            liquid_acc: 0.0,
        }
    }

    pub fn block(&self) -> u16 {
        self.blocks[self.block]
    }

    pub fn cycle_block(&mut self, step: i32) {
        self.block = (self.block as i32 + step).rem_euclid(self.blocks.len() as i32) as usize;
        self.tool = Tool::Block;
    }

    pub fn update(&mut self, input: Option<Input>, dt: f32, time_scale: f32) {
        self.time = (self.time + dt * time_scale / DAY_LENGTH).fract();
        if let Some(input) = input {
            self.player.update(&self.world, input, dt);
        }
        // At most one tick per frame: under load liquids slow down instead of piling up work.
        self.liquid_acc = (self.liquid_acc + dt).min(LIQUID_TICK);
        if self.liquid_acc >= LIQUID_TICK {
            self.liquid_acc -= LIQUID_TICK;
            self.sim.tick(&mut self.world);
        }
    }

    /// Hours on a 24 h clock, dawn at 4:30.
    pub fn clock(&self) -> f32 {
        (4.5 + self.time * 24.0) % 24.0
    }

    /// Skylight colour and background colour for the time of day.
    pub fn sky(&self) -> ([f32; 3], [f32; 3]) {
        let night = [0.10, 0.12, 0.22];
        let (day_bg, night_bg) = ([0.42, 0.62, 0.95], [0.02, 0.03, 0.08]);
        // 0 at night, 1 in full day, ramping over the first and last 8% of daytime.
        let d = if self.time < DAYTIME {
            let t = self.time / DAYTIME;
            (t / 0.08).min((1.0 - t) / 0.08).min(1.0)
        } else {
            0.0
        };
        // Warm tint around sunrise and sunset.
        let warm = 1.0 - (2.0 * d - 1.0).abs();
        let day = [1.0, 1.0 - 0.25 * warm, 1.0 - 0.45 * warm];
        let mix = |a: [f32; 3], b: [f32; 3]| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * d);
        (mix(night, day), mix(night_bg, day_bg))
    }

    fn in_reach(&self, tx: i64, ty: i64, free: bool) -> bool {
        let (cx, cy) = self.player.center();
        let (dx, dy) = ((tx as f32 + 0.5) * 16.0 - cx, (ty as f32 + 0.5) * 16.0 - cy);
        self.world.in_bounds(tx, ty) && (free || (dx * dx + dy * dy).sqrt() <= REACH * 16.0)
    }

    /// Removes the block at the tile, or the whole object if it is frame-important.
    pub fn dig(&mut self, tx: i64, ty: i64, free: bool) {
        if !self.in_reach(tx, ty, free) {
            return;
        }
        let t = self.world.tile(tx, ty);
        if t == NO_TILE {
            return;
        }
        let important = self.world.header.frame_important.get(t as usize).copied().unwrap_or(false);
        let mut todo = vec![(tx, ty)];
        let mut removed = 0;
        while let Some((x, y)) = todo.pop() {
            if self.world.tile(x, y) != t || removed >= 64 {
                continue;
            }
            self.world.set_tile(x as u32, y as u32, NO_TILE, -1, -1);
            self.sim.wake(&self.world, x as u32, y as u32);
            removed += 1;
            if important {
                todo.extend([(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]);
            }
        }
    }

    /// Places the current block, or pours the current liquid.
    pub fn place(&mut self, tx: i64, ty: i64, free: bool) {
        if !self.in_reach(tx, ty, free) || self.world.solid(tx, ty) {
            return;
        }
        let (x, y) = (tx as u32, ty as u32);
        match self.tool {
            Tool::Block => {
                let anchored = [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                    self.world.tile(tx + dx, ty + dy) != NO_TILE || self.world.wall(tx + dx, ty + dy) != 0
                });
                if self.world.tile(tx, ty) != NO_TILE || !anchored || self.player.overlaps(tx, ty) {
                    return;
                }
                self.world.set_liquid(x, y, 0, 0);
                self.world.set_tile(x, y, self.block(), -1, -1);
            }
            Tool::Liquid(kind) => self.world.set_liquid(x, y, 255, kind as u8),
        }
        self.sim.wake(&self.world, x, y);
    }

    pub fn player_rects(&self, cursor: Option<(i64, i64)>, free: bool) -> Vec<ColorRect> {
        let mut rects = Vec::new();
        if !free {
            let p = &self.player;
            rects.push(ColorRect {
                pos: [p.x, p.y],
                size: [player::WIDTH, player::HEIGHT],
                color: [0.85, 0.65, 0.45, 1.0],
            });
            rects.push(ColorRect { pos: [p.x + 2.0, p.y + 22.0], size: [16.0, 20.0], color: [0.25, 0.35, 0.7, 1.0] });
        }
        if let Some((tx, ty)) = cursor.filter(|&(x, y)| self.in_reach(x, y, free)) {
            rects.push(ColorRect {
                pos: [tx as f32 * 16.0, ty as f32 * 16.0],
                size: [16.0, 16.0],
                color: [0.25, 0.25, 0.25, 0.25],
            });
        }
        rects
    }

    /// Rebuilds what edits touched; relights and redraws the map within budgets.
    /// `view`: visible tile rect.
    pub fn sync(&mut self, renderer: &mut Renderer, view: Rect) {
        let cxs = self.world.chunks_x;
        for (cx, cy, bits) in self.world.take_dirty() {
            let ci = (cy * cxs + cx) as usize;
            if self.stale[ci] & STALE_LOD == 0 {
                self.lod_queue.push(ci as u32);
            }
            self.stale[ci] |= STALE_LIGHT | STALE_LOD;
            if self.mesh_pending[ci] == 0 {
                self.mesh_queue.push_back(ci as u32);
            }
            self.mesh_pending[ci] |= bits;
        }

        // Visible chunks plus one around.
        let (cx0, cy0) = ((view.x0 / CHUNK).saturating_sub(1), (view.y0 / CHUNK).saturating_sub(1));
        let cx1 = (view.x1.div_ceil(CHUNK) + 1).min(self.world.chunks_x);
        let cy1 = (view.y1.div_ceil(CHUNK) + 1).min(self.world.chunks_y);

        // Meshes: everything pending on screen, then a budget of the oldest off screen.
        let mut remesh = Vec::new();
        let mut take = |ci: u32, pending: &mut [u8]| {
            let bits = std::mem::take(&mut pending[ci as usize]);
            if bits != 0 {
                remesh.push((ci % cxs, ci / cxs, bits));
            }
        };
        for cy in cy0..cy1 {
            for cx in cx0..cx1 {
                take(cy * cxs + cx, &mut self.mesh_pending);
            }
        }
        let mut off_screen = 0;
        while off_screen < MESH_BUDGET {
            let Some(ci) = self.mesh_queue.pop_front() else { break };
            if self.mesh_pending[ci as usize] != 0 {
                take(ci, &mut self.mesh_pending);
                off_screen += 1;
            }
        }
        if !remesh.is_empty() {
            renderer.update_rows(self.scene.mesher.update(&self.world, &remesh));
        }

        let mut relight = Vec::new();
        'scan: for cy in cy0..cy1 {
            for cx in cx0..cx1 {
                if self.stale[(cy * self.world.chunks_x + cx) as usize] & STALE_LIGHT != 0 {
                    relight.push((cx, cy));
                    if relight.len() == LIGHT_BUDGET {
                        break 'scan;
                    }
                }
            }
        }
        if !relight.is_empty() {
            let rects: Vec<Rect> = relight.iter().map(|&(cx, cy)| Rect::of_chunk(cx, cy, &self.world)).collect();
            let bbox = rects.iter().fold(rects[0], |a, r| Rect {
                x0: a.x0.min(r.x0),
                y0: a.y0.min(r.y0),
                x1: a.x1.max(r.x1),
                y1: a.y1.max(r.y1),
            });
            self.scene.light.update(&self.world, bbox);
            for (r, (cx, cy)) in rects.into_iter().zip(relight) {
                renderer.update_light(&self.scene.light, r);
                self.stale[(cy * self.world.chunks_x + cx) as usize] &= !STALE_LIGHT;
            }
        }

        // Map texels, oldest edits first.
        let n = self.lod_queue.len().min(LOD_BUDGET);
        for ci in self.lod_queue.drain(..n) {
            let (cx, cy) = (ci % self.world.chunks_x, ci / self.world.chunks_x);
            let texels = self.scene.mesher.lod_texels(&self.world, cx * CHUNK, cy * CHUNK, CHUNK);
            renderer.update_lod(cx * CHUNK, cy * CHUNK, CHUNK, &texels);
            self.stale[ci as usize] &= !STALE_LOD;
        }
    }
}
