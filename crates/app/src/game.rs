//! Game state outside the window: the world, liquids, player, time of day, and
//! keeping the GPU copies (meshes, light, map) in step with edits.

use render::atlas::SheetRef;
use render::{Renderer, Scene, Sprite, SPRITE_FLIP, SPRITE_LIT};

use crate::inventory::{Inventory, Item};
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
/// Tile types edits may create even if the world has none.
pub const EXTRA_TILES: [u16; 2] = [OBSIDIAN, WOOD];
pub const TREE: u16 = 5;
pub const WOOD: u16 = 30;
const TORCH: u16 = 4;
/// Grass, vines and other plants: break without dropping anything.
const PLANTS: [u16; 13] = [3, 24, 52, 61, 62, 73, 74, 110, 113, 115, 201, 205, 233];
/// Objects that hang from the tile above them.
const HANGING: [u16; 8] = [34, 42, 52, 62, 91, 95, 115, 205];
const MAX_OBJECT: usize = 64;
/// Dirt-brown backdrop under the surface, lit like the tiles.
const UNDERGROUND: [f32; 4] = [0.30, 0.21, 0.15, 1.0];
/// Walked pixels per leg frame.
const WALK_FRAME: f32 = 6.0;
/// Body sheets back to front: sheet, column, row (`u32::MAX`: leg frame) and tint.
const PLAYER_LAYERS: [(&str, u32, u32, [f32; 4]); 12] = [
    ("Player_0_10", 0, u32::MAX, SKIN),
    ("Player_0_11", 0, u32::MAX, [0.40, 0.30, 0.22, 1.0]),
    ("Player_0_12", 0, u32::MAX, [0.25, 0.17, 0.12, 1.0]),
    ("Player_0_3", 0, 0, SKIN),
    ("Player_0_6", 0, 0, [0.85, 0.82, 0.70, 1.0]),
    ("Player_0_0", 0, 0, SKIN),
    ("Player_0_1", 0, 0, [1.0; 4]),
    ("Player_0_2", 0, 0, [0.25, 0.45, 0.85, 1.0]),
    ("Player_Hair_1", 0, 0, [0.50, 0.28, 0.14, 1.0]),
    ("Player_0_4", 2, 0, SKIN),
    ("Player_0_7", 2, 0, [0.30, 0.45, 0.75, 1.0]),
    ("Player_0_5", 2, 0, SKIN),
];
const SKIN: [f32; 4] = [1.0, 0.80, 0.65, 1.0];
/// Sheets packed beside the tiles for sprites.
pub fn sprite_sheets() -> Vec<String> {
    let mut v: Vec<String> = PLAYER_LAYERS.iter().map(|l| l.0.to_string()).collect();
    v.extend(["Tree_Tops_0", "Tree_Branches_0", "Item_205", "Item_206", "Item_207", "Item_1128"].map(String::from));
    v
}

/// An object as taken from the world, replayed when placed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Template {
    pub tile: u16,
    /// `(dx, dy, frame_x, frame_y)` from the bottom-left cell, `dy <= 0`.
    pub cells: Vec<(i32, i32, i16, i16)>,
}
const DROP_SIZE: f32 = 10.0;
/// Dropped items fly to the player from this far, and are picked up from `PICKUP`.
const MAGNET: f32 = 5.0 * 16.0;
const PICKUP: f32 = 1.5 * 16.0;

/// An item lying in the world.
pub struct Drop {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub item: Item,
    pub count: u16,
}

pub struct Game {
    pub world: World,
    pub scene: Scene,
    pub sim: LiquidSim,
    pub player: Player,
    /// 0 at dawn, 1 a full day later.
    pub time: f32,
    pub inventory: Inventory,
    pub drops: Vec<Drop>,
    pub templates: Vec<Template>,
    /// Per column, the first row of the underground backdrop.
    pub surface: Vec<u32>,
    stale: Vec<u8>,
    lod_queue: Vec<u32>,
    /// Per chunk, `DIRTY_*` bits not yet remeshed.
    mesh_pending: Vec<u8>,
    mesh_queue: std::collections::VecDeque<u32>,
    liquid_acc: f32,
}

impl Game {
    pub fn new(world: World, scene: Scene) -> Self {
        let sim = LiquidSim::new(&world);
        let player = Player::spawn(&world, world.width / 2);
        let stale = vec![0; world.chunks.len()];
        let mesh_pending = vec![0; world.chunks.len()];
        let surface = surface(&world);
        Self {
            world,
            scene,
            sim,
            player,
            time: 0.25,
            inventory: Inventory::new(&[
                Item::Bucket(Some(Liquid::Water)),
                Item::Bucket(Some(Liquid::Lava)),
                Item::Bucket(Some(Liquid::Honey)),
                Item::Bucket(Some(Liquid::Shimmer)),
                Item::Bucket(None),
            ]),
            drops: Vec::new(),
            templates: Vec::new(),
            surface,
            stale,
            lod_queue: Vec::new(),
            mesh_pending,
            mesh_queue: Default::default(),
            liquid_acc: 0.0,
        }
    }

    pub fn update(&mut self, input: Option<Input>, dt: f32, time_scale: f32) {
        self.time = (self.time + dt * time_scale / DAY_LENGTH).fract();
        if let Some(input) = input {
            self.player.update(&self.world, input, dt);
        }
        self.update_drops(dt);
        // At most one tick per frame: under load liquids slow down instead of piling up work.
        self.liquid_acc = (self.liquid_acc + dt).min(LIQUID_TICK);
        if self.liquid_acc >= LIQUID_TICK {
            self.liquid_acc -= LIQUID_TICK;
            self.sim.tick(&mut self.world);
        }
    }

    /// Drops fall onto blocks, fly to a nearby player and go into the inventory.
    fn update_drops(&mut self, dt: f32) {
        let (px, py) = self.player.center();
        let world = &self.world;
        let inventory = &mut self.inventory;
        self.drops.retain_mut(|d| {
            let (dx, dy) = (px - (d.x + DROP_SIZE / 2.0), py - (d.y + DROP_SIZE / 2.0));
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < PICKUP {
                d.count = inventory.add(d.item, d.count);
                return d.count > 0;
            }
            if dist < MAGNET {
                (d.vx, d.vy) = (dx / dist * 300.0, dy / dist * 300.0);
                (d.x, d.y) = (d.x + d.vx * dt, d.y + d.vy * dt);
                return true;
            }
            d.vx *= 0.9;
            d.vy = (d.vy + 1200.0 * dt).min(500.0);
            let (nx, ny) = (d.x + d.vx * dt, d.y + d.vy * dt);
            let tx = ((nx + DROP_SIZE / 2.0) / 16.0).floor() as i64;
            let below = ((ny + DROP_SIZE) / 16.0).floor() as i64;
            if world.solid(tx, below) || !world.in_bounds(tx, below) {
                d.y = below as f32 * 16.0 - DROP_SIZE;
                d.vy = 0.0;
            } else {
                d.y = ny;
            }
            if !world.solid(((nx + DROP_SIZE / 2.0) / 16.0).floor() as i64, ((d.y + 1.0) / 16.0).floor() as i64) {
                d.x = nx;
            }
            true
        });
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

    fn important(&self, t: u16) -> bool {
        self.world.header.frame_important.get(t as usize).copied().unwrap_or(false)
    }

    /// Breaks the block or object at the tile; with the free camera nothing drops.
    pub fn dig(&mut self, tx: i64, ty: i64, free: bool) {
        if self.in_reach(tx, ty, free) {
            self.break_tile(tx, ty, !free);
        }
    }

    /// Removes a block, a whole object or a whole tree, then whatever rested on it.
    fn break_tile(&mut self, tx: i64, ty: i64, drop: bool) {
        let t = self.world.tile(tx, ty);
        if t == NO_TILE {
            return;
        }
        let (cells, item) = if t == TREE {
            let cells = self.connected(tx, ty, |_, _| true, 512);
            let n = cells.len() as u16;
            (cells, Some((Item::Block(WOOD), n)))
        } else if self.important(t) {
            let cells = self.object_cells(tx, ty);
            let item = (!PLANTS.contains(&t)).then(|| (Item::Object(self.template(t, &cells)), 1));
            (cells, item)
        } else {
            (vec![(tx, ty)], Some((Item::Block(t), 1)))
        };
        for &(x, y) in &cells {
            self.world.set_tile(x as u32, y as u32, NO_TILE, -1, -1);
            self.sim.wake(&self.world, x as u32, y as u32);
        }
        if let Some((item, count)) = item.filter(|_| drop) {
            self.drops.push(Drop {
                x: tx as f32 * 16.0 + (16.0 - DROP_SIZE) / 2.0,
                y: ty as f32 * 16.0 + (16.0 - DROP_SIZE) / 2.0,
                vx: 0.0,
                vy: -120.0,
                item,
                count,
            });
        }
        for &(x, y) in &cells {
            for (dx, dy) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
                let (nx, ny) = (x + dx, y + dy);
                let n = self.world.tile(nx, ny);
                if n == NO_TILE {
                    continue;
                }
                let rests = match dy {
                    -1 => n == TREE || (self.important(n) && !HANGING.contains(&n)),
                    1 => HANGING.contains(&n),
                    _ => n == TORCH,
                };
                if rests && !(n == TORCH && self.torch_held(nx, ny)) {
                    self.break_tile(nx, ny, drop);
                }
            }
        }
    }

    fn torch_held(&self, x: i64, y: i64) -> bool {
        let w = &self.world;
        w.wall(x, y) != 0 || w.solid(x, y + 1) || w.solid(x - 1, y) || w.solid(x + 1, y)
    }

    /// Cells of the same tile type reachable from `(tx, ty)` through steps that `link` accepts.
    fn connected(
        &self,
        tx: i64,
        ty: i64,
        link: impl Fn((i64, i64), (i64, i64)) -> bool,
        max: usize,
    ) -> Vec<(i64, i64)> {
        let t = self.world.tile(tx, ty);
        let mut cells = vec![(tx, ty)];
        let mut i = 0;
        while i < cells.len() && cells.len() < max {
            let (x, y) = cells[i];
            i += 1;
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let n = (x + dx, y + dy);
                if self.world.tile(n.0, n.1) == t && !cells.contains(&n) && link((x, y), n) {
                    cells.push(n);
                }
            }
        }
        cells
    }

    /// One object: neighbours of the same type whose frame continues this cell's
    /// (one frame step of 16 to 20 px right or down), so side by side objects stay apart.
    fn object_cells(&self, tx: i64, ty: i64) -> Vec<(i64, i64)> {
        let w = &self.world;
        let step = |a: i16, b: i16, d: i64| (16..=20).contains(&((b as i64 - a as i64) * d));
        self.connected(
            tx,
            ty,
            |(x, y), (nx, ny)| {
                let ((fx, fy), (gx, gy)) = (w.frame(x, y), w.frame(nx, ny));
                if ny == y {
                    gy == fy && step(fx, gx, nx - x)
                } else {
                    gx == fx && step(fy, gy, ny - y)
                }
            },
            MAX_OBJECT,
        )
    }

    /// Index of the template for these cells, added if new.
    fn template(&mut self, tile: u16, cells: &[(i64, i64)]) -> u16 {
        let x0 = cells.iter().map(|c| c.0).min().unwrap();
        let y1 = cells.iter().map(|c| c.1).max().unwrap();
        let mut cells: Vec<_> = cells
            .iter()
            .map(|&(x, y)| {
                let (fx, fy) = self.world.frame(x, y);
                ((x - x0) as i32, (y - y1) as i32, fx, fy)
            })
            .collect();
        cells.sort_unstable();
        let tpl = Template { tile, cells };
        let id = self.templates.iter().position(|t| *t == tpl).unwrap_or_else(|| {
            self.templates.push(tpl);
            self.templates.len() - 1
        });
        id as u16
    }

    /// Uses the selected item on the tile: places a block or object, pours or fills a bucket.
    /// Nothing is used up with the free camera.
    pub fn place(&mut self, tx: i64, ty: i64, free: bool) {
        let Some(stack) = self.inventory.selected() else { return };
        if !self.in_reach(tx, ty, free) || self.world.solid(tx, ty) {
            return;
        }
        let (x, y) = (tx as u32, ty as u32);
        match stack.item {
            Item::Block(t) => {
                let anchored = [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                    self.world.tile(tx + dx, ty + dy) != NO_TILE || self.world.wall(tx + dx, ty + dy) != 0
                });
                if self.world.tile(tx, ty) != NO_TILE || !anchored || self.player.overlaps(tx, ty) {
                    return;
                }
                self.world.set_liquid(x, y, 0, 0);
                self.world.set_tile(x, y, t, -1, -1);
            }
            Item::Object(id) => {
                let tpl = &self.templates[id as usize];
                let w = &self.world;
                let cells: Vec<_> =
                    tpl.cells.iter().map(|&(dx, dy, fx, fy)| (tx + dx as i64, ty + dy as i64, fx, fy)).collect();
                let blocked = |&(cx, cy, ..): &(i64, i64, i16, i16)| {
                    !w.in_bounds(cx, cy) || w.tile(cx, cy) != NO_TILE || self.player.overlaps(cx, cy)
                };
                if cells.iter().any(blocked) {
                    return;
                }
                // Stands on blocks, hangs from them, or sits on a wall; a single cell may cling to a side.
                let top = cells.iter().map(|c| c.1).min().unwrap();
                let held = cells.iter().any(|&(cx, cy, ..)| {
                    (cy == ty && w.solid(cx, cy + 1))
                        || (cy == top && w.solid(cx, cy - 1))
                        || w.wall(cx, cy) != 0
                        || (cells.len() == 1 && (w.solid(cx - 1, cy) || w.solid(cx + 1, cy)))
                });
                if !held {
                    return;
                }
                let tile = tpl.tile;
                for (cx, cy, fx, fy) in cells {
                    self.world.set_liquid(cx as u32, cy as u32, 0, 0);
                    self.world.set_tile(cx as u32, cy as u32, tile, fx, fy);
                    self.sim.wake(&self.world, cx as u32, cy as u32);
                }
            }
            Item::Bucket(Some(kind)) => {
                let (amount, k) = {
                    let (c, i) = self.world.cell(x, y);
                    (c.liquid[i], c.liquid_kind[i])
                };
                if amount != 0 && k != kind as u8 {
                    return;
                }
                self.world.set_liquid(x, y, 255, kind as u8);
                if !free {
                    self.inventory.set_selected(Item::Bucket(None));
                }
            }
            Item::Bucket(None) => {
                let (amount, k) = {
                    let (c, i) = self.world.cell(x, y);
                    (c.liquid[i], c.liquid_kind[i])
                };
                // Scoops a cell that is at least half full.
                if amount < 128 {
                    return;
                }
                self.world.set_liquid(x, y, 0, 0);
                let kind = [Liquid::Water, Liquid::Lava, Liquid::Honey, Liquid::Shimmer][k as usize & 3];
                self.inventory.set_selected(Item::Bucket(Some(kind)));
            }
        }
        if matches!(stack.item, Item::Block(_) | Item::Object(_)) && !free {
            self.inventory.consume();
        }
        self.sim.wake(&self.world, x, y);
    }

    fn sheet(&self, name: &str) -> Option<&SheetRef> {
        self.scene.mesher.atlas.named.get(name)
    }

    /// Sprites drawing `item` fitted in a `size` square at `pos` (world pixels).
    pub fn item_icon(&self, item: Item, pos: [f32; 2], size: f32, flags: u32) -> Vec<Sprite> {
        let atlas = &self.scene.mesher.atlas;
        let flat = |c: [u8; 4]| vec![Sprite::flat(pos, [size; 2], c.map(|v| v as f32 / 255.0))];
        // One sheet region scaled to fit, centred.
        let fit = |sheet: &SheetRef, src: [u32; 4]| {
            let k = size / src[2].max(src[3]) as f32;
            let mut s = Sprite::sheet(pos, sheet, src, flags);
            s.size = [src[2] as f32 * k, src[3] as f32 * k];
            s.pos = [pos[0] + (size - s.size[0]) / 2.0, pos[1] + (size - s.size[1]) / 2.0];
            vec![s]
        };
        match item {
            Item::Block(t) => match atlas.tiles.get(t as usize).copied().flatten() {
                Some(s) if !s.fallback => fit(&s, [18, 18, 16, 16]),
                _ => flat(atlas.tile_color.get(t as usize).copied().unwrap_or([255; 4])),
            },
            Item::Bucket(l) => {
                let name = match l {
                    None => "Item_205",
                    Some(Liquid::Water) => "Item_206",
                    Some(Liquid::Lava) => "Item_207",
                    Some(Liquid::Honey) => "Item_1128",
                    Some(Liquid::Shimmer) => "",
                };
                match (self.sheet(name), l) {
                    (Some(s), _) => fit(s, [0, 0, s.w, s.h]),
                    (None, Some(l)) => flat(render::mesh::LIQUID_LOD[l as usize]),
                    (None, None) => flat([150, 150, 160, 255]),
                }
            }
            Item::Object(id) => {
                let tpl = &self.templates[id as usize];
                let Some(sheet) = atlas.tiles.get(tpl.tile as usize).copied().flatten().filter(|s| !s.fallback) else {
                    return flat(atlas.tile_color.get(tpl.tile as usize).copied().unwrap_or([255; 4]));
                };
                let w = tpl.cells.iter().map(|c| c.0).max().unwrap() + 1;
                let top = tpl.cells.iter().map(|c| c.1).min().unwrap();
                let h = 1 - top;
                let k = size / (w.max(h) * 16) as f32;
                let (ox, oy) =
                    (pos[0] + (size - w as f32 * 16.0 * k) / 2.0, pos[1] + (size - h as f32 * 16.0 * k) / 2.0);
                tpl.cells
                    .iter()
                    .filter(|c| c.2 >= 0 && c.3 >= 0 && (c.2 as u32 + 16) <= sheet.w && (c.3 as u32 + 16) <= sheet.h)
                    .map(|&(dx, dy, fx, fy)| {
                        let mut s = Sprite::sheet([0.0; 2], &sheet, [fx as u32, fy as u32, 16, 16], flags);
                        s.pos = [ox + dx as f32 * 16.0 * k, oy + (dy - top) as f32 * 16.0 * k];
                        s.size = [16.0 * k; 2];
                        s
                    })
                    .collect()
            }
        }
    }

    /// Backdrop and tree tops (behind the world), then player, drops and cursor (in front).
    pub fn sprites(&self, view: Rect, cursor: Option<(i64, i64)>, free: bool) -> (Vec<Sprite>, Vec<Sprite>) {
        let mut under = Vec::new();
        // One strip per column (or group of columns when zoomed out) below the ground line.
        let step = ((view.x1 - view.x0) / 512).max(1);
        let bottom = view.y1 as f32 * 16.0;
        for x in (view.x0 - view.x0 % step..view.x1).step_by(step as usize) {
            let top = self.surface[x as usize].max(view.y0) as f32 * 16.0;
            if top < bottom {
                let pos = [x as f32 * 16.0, top];
                under.push(Sprite {
                    flags: SPRITE_LIT,
                    ..Sprite::flat(pos, [step as f32 * 16.0, bottom - top], UNDERGROUND)
                });
            }
        }
        self.tree_tops(view, &mut under);

        let mut over = Vec::new();
        if !free {
            self.player_sprites(&mut over);
        }
        for d in &self.drops {
            over.extend(self.item_icon(d.item, [d.x - 2.0, d.y - 4.0], DROP_SIZE + 4.0, SPRITE_LIT));
        }
        if let Some((tx, ty)) = cursor.filter(|&(x, y)| self.in_reach(x, y, free)) {
            over.push(Sprite::flat([tx as f32 * 16.0, ty as f32 * 16.0], [16.0, 16.0], [0.25, 0.25, 0.25, 0.25]));
        }
        (under, over)
    }

    /// Leafy tops and branches, drawn from the tree's top and branch cells.
    fn tree_tops(&self, view: Rect, out: &mut Vec<Sprite>) {
        let (Some(tops), Some(branches)) = (self.sheet("Tree_Tops_0"), self.sheet("Tree_Branches_0")) else { return };
        // Tops reach 5 tiles over their cell, branches 2 to the side.
        for y in view.y0..(view.y1 + 5).min(self.world.height) {
            for x in view.x0.saturating_sub(2)..(view.x1 + 2).min(self.world.width) {
                if self.world.tile(x as i64, y as i64) != TREE {
                    continue;
                }
                let (fx, fy) = self.world.frame(x as i64, y as i64);
                if fy < 198 {
                    continue;
                }
                let style = ((fy - 198) / 22).clamp(0, 2) as u32;
                let (px, py) = (x as f32 * 16.0, y as f32 * 16.0);
                let s = match fx {
                    22 => Sprite::sheet([px - 32.0, py + 16.0 - 80.0], tops, [style * 82, 0, 80, 80], SPRITE_LIT),
                    44 => Sprite::sheet([px - 24.0, py - 12.0], branches, [0, style * 42, 40, 40], SPRITE_LIT),
                    66 => Sprite::sheet([px, py - 12.0], branches, [42, style * 42, 40, 40], SPRITE_LIT),
                    _ => continue,
                };
                out.push(s);
            }
        }
    }

    /// Layered body sheets, tinted, with the walk cycle on the legs.
    fn player_sprites(&self, out: &mut Vec<Sprite>) {
        let p = &self.player;
        let pos = [p.x + player::WIDTH / 2.0 - 20.0, p.y + player::HEIGHT - 54.0];
        let legs = if !p.on_ground {
            5
        } else if p.vx.abs() > 10.0 {
            6 + (p.walked / WALK_FRAME) as u32 % 14
        } else {
            0
        };
        let flip = if p.facing_left { SPRITE_FLIP } else { 0 } | SPRITE_LIT;
        let mut any = false;
        for &(name, col, row, color) in &PLAYER_LAYERS {
            let Some(sheet) = self.sheet(name) else { continue };
            let row = if row == u32::MAX { legs } else { row };
            if (col + 1) * 40 > sheet.w || (row + 1) * 56 > sheet.h {
                continue;
            }
            let mut s = Sprite::sheet(pos, sheet, [col * 40, row * 56, 40, 56], flip);
            s.color = color;
            out.push(s);
            any = true;
        }
        if !any {
            out.push(Sprite::flat([p.x, p.y], [player::WIDTH, player::HEIGHT], [0.85, 0.65, 0.45, 1.0]));
        }
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

/// Per column, the first block from the top as the world was loaded, a couple of rows down.
fn surface(world: &World) -> Vec<u32> {
    (0..world.width)
        .map(|x| (0..world.height).find(|&y| world.solid(x as i64, y as i64)).map_or(world.height, |y| y + 2))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::wld::Tile;

    const BENCH: u16 = 18;

    /// Floor at y = 20; two 2x1 benches side by side at x = 10..14, a torch at (20, 19).
    fn game() -> Game {
        let mut important = vec![false; 20];
        important[BENCH as usize] = true;
        important[TORCH as usize] = true;
        let world = World::parse(&formats::synth::write_world("g", 64, 64, &important, |x, y| {
            let (tile, fx) = match (x, y) {
                (_, 20..) => (1, 0),
                (10..=13, 19) => (BENCH, ((x - 10) % 2) as i16 * 18),
                (20, 19) => (TORCH, 0),
                _ => (NO_TILE, 0),
            };
            Tile { tile, frame_x: fx, frame_y: 0, ..Tile::default() }
        }))
        .unwrap();
        let scene = Scene::build(&world, None, &EXTRA_TILES, &[]);
        let mut g = Game::new(world, scene);
        g.player = Player::spawn(&g.world, 15);
        g
    }

    fn pick_up(g: &mut Game) {
        for d in &mut g.drops {
            (d.x, d.y) = g.player.center();
        }
        g.update_drops(0.0);
    }

    #[test]
    fn objects_drop_and_place_again() {
        let mut g = game();
        g.dig(11, 19, false);
        assert_eq!((g.world.tile(10, 19), g.world.tile(11, 19)), (NO_TILE, NO_TILE));
        assert_eq!(g.world.tile(12, 19), BENCH, "the bench beside stays");
        g.dig(20, 19, false);
        pick_up(&mut g);
        let objects: Vec<_> =
            g.inventory.slots.iter().flatten().filter(|s| matches!(s.item, Item::Object(_))).collect();
        assert_eq!(objects.len(), 2, "bench and torch");
        assert_eq!(g.templates[0], Template { tile: BENCH, cells: vec![(0, 0, 0, 0), (1, 0, 18, 0)] });

        g.inventory.selected =
            g.inventory.slots.iter().position(|s| s.is_some_and(|s| s.item == Item::Object(0))).unwrap();
        g.place(17, 18, true); // floating: refused
        assert_eq!(g.world.tile(17, 18), NO_TILE);
        g.place(17, 19, false);
        assert_eq!((g.world.tile(17, 19), g.world.tile(18, 19)), (BENCH, BENCH));
        assert_eq!(g.world.frame(18, 19), (18, 0));
        assert!(g.inventory.slots[g.inventory.selected].is_none(), "used up");

        // Digging the floor under the other bench breaks it too.
        g.dig(12, 20, false);
        assert_eq!((g.world.tile(12, 19), g.world.tile(13, 19)), (NO_TILE, NO_TILE));
    }
}
