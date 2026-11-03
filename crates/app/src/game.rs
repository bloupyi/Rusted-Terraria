//! Game state outside the window: the world, liquids, player, time of day, and
//! keeping the GPU copies (meshes, light, map) in step with edits.

use render::atlas::SheetRef;
use render::{Renderer, Scene, Sprite, SPRITE_FLIP, SPRITE_LIT};

use crate::audio::Sfx;
use crate::inventory::{Inventory, Item};
use crate::items::{self, Furniture, Material, Out, Tool, RECIPES, USE_TIME};
use crate::net::{MobState, Msg};
use world::light::Rect;
use world::liquid::{LiquidSim, OBSIDIAN};
use world::mob::{Kind, Mob};
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
/// Obsidian from liquids, wood from trees, and the craftable furniture.
pub const EXTRA_TILES: [u16; 6] = [OBSIDIAN, WOOD, 4, items::WORK_BENCH, items::FURNACE, items::ANVIL];
pub const TREE: u16 = 5;
pub const WOOD: u16 = 30;
const TORCH: u16 = 4;
/// Grass, vines and other plants: break without dropping anything.
const PLANTS: [u16; 13] = [3, 24, 52, 61, 62, 73, 74, 110, 113, 115, 201, 205, 233];
/// Objects that hang from the tile above them.
const HANGING: [u16; 8] = [34, 42, 52, 62, 91, 95, 115, 205];
const MAX_OBJECT: usize = 64;
/// Tiles under the ground line where the underground music starts.
const UNDERGROUND_DEPTH: u32 = 30;
pub const MAX_HP: i32 = 100;
/// Seconds without damage after a hit.
const INVULNERABLE: f32 = 0.67;
const MAX_MOBS: usize = 5;
const SPAWN_EVERY: f32 = 2.0;
/// Monsters appear this many tiles to the side, and vanish past `DESPAWN`.
const SPAWN_DIST: (i64, i64) = (35, 55);
const DESPAWN: f32 = 100.0 * 16.0;
/// Damage with no weapon in hand.
const FIST_DAMAGE: i32 = 2;
/// Crafting stations count within this many tiles of the player.
const STATION_REACH: i64 = 4;
const ATTACK_EVERY: f32 = 0.35;
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
    v.extend(
        ["Tree_Tops_0", "Tree_Branches_0", "Item_205", "Item_206", "Item_207", "Item_1128", "NPC_1", "NPC_3"]
            .map(String::from),
    );
    v.extend(items::sheets());
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
    pub mobs: Vec<Mob>,
    /// Sounds to play this frame.
    pub sfx: Vec<Sfx>,
    /// Other players by network id.
    pub others: Vec<(u8, Player)>,
    /// Monsters come from the host instead of being simulated here.
    pub client: bool,
    /// Edits to send to the other players.
    pub outbox: Vec<Msg>,
    pub hp: i32,
    invulnerable: f32,
    attack_cd: f32,
    /// Seconds before the held tool can be used again.
    use_cd: f32,
    /// Tile being mined and how far, out of 100.
    mining: Option<(i64, i64, f32)>,
    spawn_acc: f32,
    spawn_x: u32,
    rng: u64,
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
        let spawn_x = world.width / 2;
        Self {
            world,
            scene,
            sim,
            player,
            time: 0.25,
            inventory: Inventory::new(&[
                Item::Tool(Tool::Sword(items::Tier::Copper)),
                Item::Tool(Tool::Pickaxe(items::Tier::Copper)),
                Item::Tool(Tool::Axe(items::Tier::Copper)),
                Item::Bucket(Some(Liquid::Water)),
                Item::Bucket(Some(Liquid::Lava)),
                Item::Bucket(Some(Liquid::Honey)),
                Item::Bucket(Some(Liquid::Shimmer)),
                Item::Bucket(None),
            ]),
            drops: Vec::new(),
            templates: Vec::new(),
            mobs: Vec::new(),
            sfx: Vec::new(),
            others: Vec::new(),
            client: false,
            outbox: Vec::new(),
            hp: MAX_HP,
            invulnerable: 0.0,
            attack_cd: 0.0,
            use_cd: 0.0,
            mining: None,
            spawn_acc: 0.0,
            spawn_x,
            rng: 0x9E37_79B9_7F4A_7C15,
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
        self.update_mobs(dt, input.is_some());
        self.update_drops(dt);
        // At most one tick per frame: under load liquids slow down instead of piling up work.
        self.liquid_acc = (self.liquid_acc + dt).min(LIQUID_TICK);
        if self.liquid_acc >= LIQUID_TICK {
            self.liquid_acc -= LIQUID_TICK;
            self.sim.tick(&mut self.world);
        }
    }

    fn random(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    /// Spawns, moves and despawns monsters; contact hurts the player when `fight`.
    fn update_mobs(&mut self, dt: f32, fight: bool) {
        self.invulnerable -= dt;
        self.attack_cd -= dt;
        self.use_cd -= dt;
        self.spawn_acc += dt;
        if fight && !self.client && self.spawn_acc >= SPAWN_EVERY {
            self.spawn_acc = 0.0;
            if self.mobs.len() < MAX_MOBS {
                self.try_spawn();
            }
            if self.mobs.iter().any(|m| m.kind == Kind::Zombie) && self.random().is_multiple_of(4) {
                self.sfx.push(Sfx::Zombie);
            }
        }
        if !self.client {
            let players: Vec<(f32, f32)> =
                std::iter::once(self.player.center()).chain(self.others.iter().map(|(_, p)| p.center())).collect();
            let dist = |m: &Mob, p: &(f32, f32)| (m.body.center().0 - p.0).abs();
            for m in &mut self.mobs {
                let near = *players.iter().min_by(|a, b| dist(m, a).total_cmp(&dist(m, b))).unwrap();
                m.update(&self.world, near, dt);
            }
            let height = self.world.height as f32 * 16.0;
            self.mobs.retain(|m| players.iter().any(|p| dist(m, p) < DESPAWN) && m.body.y < height);
        }
        if !fight || self.invulnerable > 0.0 {
            return;
        }
        if let Some(m) = self.mobs.iter().find(|m| m.body.touches(&self.player.body)) {
            self.hp -= m.kind.damage();
            self.invulnerable = INVULNERABLE;
            self.sfx.push(if self.hp <= 0 { Sfx::PlayerKilled } else { Sfx::PlayerHit });
            let away = if self.player.center().0 < m.body.center().0 { -1.0 } else { 1.0 };
            (self.player.vx, self.player.vy) = (away * 250.0, -250.0);
            if self.hp <= 0 {
                self.hp = MAX_HP;
                self.player = Player::spawn(&self.world, self.spawn_x);
            }
        }
    }

    /// A monster on open ground to one side, off screen: slimes by day, zombies too at night.
    fn try_spawn(&mut self) {
        let r = self.random();
        let (px, py) = self.player.center();
        let (ptx, pty) = ((px / 16.0) as i64, (py / 16.0) as i64);
        let side = if r & 1 == 0 { -1 } else { 1 };
        let d = SPAWN_DIST.0 + ((r >> 1) % (SPAWN_DIST.1 - SPAWN_DIST.0) as u64) as i64;
        let x = ptx + side * d;
        let w = &self.world;
        let open = |y: i64| (1..=3).all(|k| w.in_bounds(x, y - k) && !w.solid(x, y - k) && w.liquid(x, y - k) == 0);
        let Some(y) = (pty - 25..pty + 25).find(|&y| w.solid(x, y) && open(y)) else { return };
        let night = self.time >= DAYTIME;
        let kind = if night && (r >> 8) % 10 < 6 { Kind::Zombie } else { Kind::Slime };
        self.mobs.push(Mob::new(kind, x as f32 * 16.0 + 8.0, y as f32 * 16.0));
    }

    /// Hits the monster under the world point if in reach. True if one was there.
    pub fn attack(&mut self, wx: f32, wy: f32, free: bool) -> bool {
        let (tx, ty) = ((wx / 16.0).floor() as i64, (wy / 16.0).floor() as i64);
        if !self.in_reach(tx, ty, free) {
            return false;
        }
        let Some(i) = self.mobs.iter().position(|m| {
            let b = &m.body;
            wx >= b.x && wx < b.x + b.w && wy >= b.y && wy < b.y + b.h
        }) else {
            return false;
        };
        let damage = self.weapon_damage();
        if self.attack_cd <= 0.0 && self.client {
            self.attack_cd = ATTACK_EVERY;
            self.sfx.extend([Sfx::Swing, Sfx::MobHit]);
            self.outbox.push(Msg::Hit { index: i as u16, damage: damage as u16 });
        } else if self.attack_cd <= 0.0 {
            self.attack_cd = ATTACK_EVERY;
            self.sfx.push(Sfx::Swing);
            let from = self.player.center().0;
            self.hit_mob(i, damage, from);
        }
        true
    }

    fn weapon_damage(&self) -> i32 {
        match self.inventory.selected().map(|s| s.item) {
            Some(Item::Tool(t)) => t.damage(),
            _ => FIST_DAMAGE,
        }
    }

    /// Hurts a monster; a dead slime leaves gel.
    fn hit_mob(&mut self, i: usize, damage: i32, from_x: f32) {
        if !self.mobs[i].hit(damage, from_x) {
            self.sfx.push(Sfx::MobHit);
            return;
        }
        let m = self.mobs.swap_remove(i);
        self.sfx.push(Sfx::MobKilled);
        if m.kind == Kind::Slime {
            let (x, y) = m.body.center();
            let count = 1 + (self.random() % 2) as u16;
            self.drops.push(Drop { x, y, vx: 0.0, vy: -150.0, item: Item::Material(Material::Gel), count });
        }
    }

    /// Monsters as sent to clients.
    pub fn mob_states(&self) -> Vec<MobState> {
        self.mobs
            .iter()
            .map(|m| MobState {
                kind: m.kind as u8,
                x: m.body.x,
                y: m.body.y,
                facing_left: m.facing_left,
                hurt: m.stun > 0.0,
            })
            .collect()
    }

    /// Applies a message from player `from`.
    pub fn apply(&mut self, from: u8, msg: &Msg) {
        match *msg {
            Msg::Player { id, x, y, vx, vy, facing_left } => {
                let i = self.others.iter().position(|o| o.0 == id).unwrap_or_else(|| {
                    self.others.push((id, Player::at(x, y)));
                    self.others.len() - 1
                });
                let p = &mut self.others[i].1;
                let walked = (x - p.x).abs();
                (p.x, p.y, p.vx, p.vy, p.facing_left) = (x, y, vx, vy, facing_left);
                p.on_ground = vy == 0.0;
                p.walked = if p.on_ground { p.walked + walked } else { 0.0 };
            }
            Msg::Tile { x, y, ref cell } if self.world.in_bounds(x as i64, y as i64) => {
                self.world.set_cell(x, y, cell);
                self.sim.wake(&self.world, x, y);
            }
            Msg::Liquid { x, y, amount, kind } if self.world.in_bounds(x as i64, y as i64) => {
                self.world.set_liquid(x, y, amount, kind);
                self.sim.wake(&self.world, x, y);
            }
            Msg::Mobs(ref states) if self.client => {
                self.mobs = states
                    .iter()
                    .map(|s| {
                        let kind = if s.kind == Kind::Zombie as u8 { Kind::Zombie } else { Kind::Slime };
                        let mut m = Mob::new(kind, 0.0, 0.0);
                        // Grounded while the snapshot keeps it at the same height.
                        let old = self.mobs.iter().find(|o| o.kind == kind && (o.body.x - s.x).abs() < 16.0);
                        m.body.on_ground = old.is_some_and(|o| o.body.y == s.y);
                        m.timer = old.map_or(0.0, |o| o.timer + 1.0 / 60.0);
                        (m.body.x, m.body.y, m.facing_left) = (s.x, s.y, s.facing_left);
                        m.stun = if s.hurt { 0.1 } else { 0.0 };
                        m
                    })
                    .collect();
            }
            Msg::Hit { index, damage } if !self.client && (index as usize) < self.mobs.len() => {
                let from_x = self.others.iter().find(|o| o.0 == from).map_or(0.0, |o| o.1.center().0);
                self.hit_mob(index as usize, damage as i32, from_x);
            }
            Msg::Leave { id } => self.others.retain(|o| o.0 != id),
            Msg::Time(t) if self.client => self.time = t,
            _ => {}
        }
    }

    /// Drops fall onto blocks, fly to a nearby player and go into the inventory.
    fn update_drops(&mut self, dt: f32) {
        let (px, py) = self.player.center();
        let world = &self.world;
        let inventory = &mut self.inventory;
        let sfx = &mut self.sfx;
        self.drops.retain_mut(|d| {
            let (dx, dy) = (px - (d.x + DROP_SIZE / 2.0), py - (d.y + DROP_SIZE / 2.0));
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < PICKUP {
                let before = d.count;
                d.count = inventory.add(d.item, d.count);
                if d.count < before {
                    sfx.push(Sfx::Grab);
                }
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

    /// Music cue for where the player is: underground (4), night (3) or day (1).
    /// Cue names are the sound bank's; which is which follows the public soundtrack list.
    pub fn music(&self) -> &'static str {
        let (x, y) = self.player.center();
        let column = ((x / 16.0) as usize).min(self.surface.len() - 1);
        if (y / 16.0) as u32 > self.surface[column] + UNDERGROUND_DEPTH {
            "Music_4"
        } else if self.time >= DAYTIME {
            "Music_3"
        } else {
            "Music_1"
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

    fn important(&self, t: u16) -> bool {
        self.world.header.frame_important.get(t as usize).copied().unwrap_or(false)
    }

    /// Breaks the block or object at the tile; with the free camera nothing drops.
    /// With the free camera anything breaks at once and drops nothing. Otherwise
    /// blocks need a pickaxe, trees an axe and walls a hammer, over several uses;
    /// objects and plants come off by hand.
    pub fn dig(&mut self, tx: i64, ty: i64, free: bool) {
        if !self.in_reach(tx, ty, free) {
            return;
        }
        if free {
            self.break_tile(tx, ty, false);
            return;
        }
        if self.use_cd > 0.0 {
            return;
        }
        let tool = match self.inventory.selected().map(|s| s.item) {
            Some(Item::Tool(t)) => Some(t),
            _ => None,
        };
        let t = self.world.tile(tx, ty);
        let power = match (t, tool) {
            (NO_TILE, Some(Tool::Hammer(h))) if self.world.wall(tx, ty) != 0 => Tool::Hammer(h).power() / 2.0,
            (NO_TILE, _) => return,
            (TREE, Some(Tool::Axe(a))) => Tool::Axe(a).power(),
            (TREE, _) => return,
            (t, _) if self.important(t) => 100.0,
            (t, Some(Tool::Pickaxe(p))) => Tool::Pickaxe(p).power() / items::hardness(t),
            _ => return,
        };
        self.use_cd = USE_TIME;
        let done = match self.mining {
            Some((x, y, p)) if (x, y) == (tx, ty) => p + power,
            _ => power,
        };
        if done < 100.0 {
            self.mining = Some((tx, ty, done));
            self.sfx.push(if items::HARD_SOUND.contains(&t) { Sfx::Tink } else { Sfx::Dig });
            return;
        }
        self.mining = None;
        if t == NO_TILE {
            self.world.set_wall(tx as u32, ty as u32, 0);
            self.send_tile(tx as u32, ty as u32);
            self.sfx.push(Sfx::Dig);
        } else {
            self.break_tile(tx, ty, true);
        }
    }

    fn send_tile(&mut self, x: u32, y: u32) {
        self.outbox.push(Msg::Tile { x, y, cell: self.world.cell_data(x, y) });
    }

    /// Mining progress on a tile, 0 to 1, for the crack overlay.
    pub fn mining(&self) -> Option<(i64, i64, f32)> {
        self.mining.map(|(x, y, p)| (x, y, p / 100.0))
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
        self.sfx.push(if items::HARD_SOUND.contains(&t) { Sfx::Tink } else { Sfx::Dig });
        for &(x, y) in &cells {
            self.world.set_tile(x as u32, y as u32, NO_TILE, -1, -1);
            self.send_tile(x as u32, y as u32);
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
        let cells: Vec<_> = cells
            .iter()
            .map(|&(x, y)| {
                let (fx, fy) = self.world.frame(x, y);
                ((x - x0) as i32, (y - y1) as i32, fx, fy)
            })
            .collect();
        self.add_template(tile, cells)
    }

    fn add_template(&mut self, tile: u16, mut cells: Vec<(i32, i32, i16, i16)>) -> u16 {
        cells.sort_unstable();
        let tpl = Template { tile, cells };
        let id = self.templates.iter().position(|t| *t == tpl).unwrap_or_else(|| {
            self.templates.push(tpl);
            self.templates.len() - 1
        });
        id as u16
    }

    /// The first style of a craftable object, frames 18 px apart.
    fn furniture(&mut self, f: Furniture) -> Item {
        let (tile, w, h) = f.tile();
        let cells =
            (0..w).flat_map(|dx| (0..h).map(move |dy| (dx, dy - (h - 1), dx as i16 * 18, dy as i16 * 18))).collect();
        Item::Object(self.add_template(tile, cells))
    }

    /// Station tiles around the player.
    fn stations(&self) -> Vec<u16> {
        let (x, y) = self.player.center();
        let (tx, ty) = ((x / 16.0) as i64, (y / 16.0) as i64);
        let mut v: Vec<u16> = (ty - STATION_REACH..=ty + STATION_REACH)
            .flat_map(|y| (tx - STATION_REACH..=tx + STATION_REACH).map(move |x| (x, y)))
            .map(|(x, y)| self.world.tile(x, y))
            .filter(|t| [items::WORK_BENCH, items::FURNACE, items::ANVIL].contains(t))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Recipes that can be made here with what the player carries.
    pub fn craftable(&self) -> Vec<usize> {
        let stations = self.stations();
        RECIPES
            .iter()
            .enumerate()
            .filter(|(_, r)| r.station.is_none_or(|s| stations.contains(&s)))
            .filter(|(_, r)| r.inputs.iter().all(|&(item, n)| self.inventory.count(item) >= n as u32))
            .map(|(i, _)| i)
            .collect()
    }

    /// What a recipe gives, for its icon.
    pub fn recipe_item(&mut self, i: usize) -> Item {
        match RECIPES[i].out {
            Out::Item(item) => item,
            Out::Furniture(f) => self.furniture(f),
        }
    }

    /// Makes recipe `i` if it can be made here; what does not fit drops at the player.
    pub fn craft(&mut self, i: usize) -> bool {
        if !self.craftable().contains(&i) {
            return false;
        }
        let r = &RECIPES[i];
        for &(item, n) in r.inputs {
            self.inventory.remove(item, n);
        }
        let item = self.recipe_item(i);
        let rest = self.inventory.add(item, r.count);
        if rest > 0 {
            let (x, y) = self.player.center();
            self.drops.push(Drop { x, y, vx: 0.0, vy: 0.0, item, count: rest });
        }
        self.sfx.push(Sfx::Grab);
        true
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
            Item::Tool(_) | Item::Material(_) => return,
            Item::Block(t) => {
                let anchored = [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                    self.world.tile(tx + dx, ty + dy) != NO_TILE || self.world.wall(tx + dx, ty + dy) != 0
                });
                if self.world.tile(tx, ty) != NO_TILE || !anchored || self.player.overlaps(tx, ty) {
                    return;
                }
                self.world.set_liquid(x, y, 0, 0);
                self.world.set_tile(x, y, t, -1, -1);
                self.send_tile(x, y);
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
                    self.send_tile(cx as u32, cy as u32);
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
                self.outbox.push(Msg::Liquid { x, y, amount: 255, kind: kind as u8 });
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
                self.outbox.push(Msg::Liquid { x, y, amount: 0, kind: 0 });
                let kind = [Liquid::Water, Liquid::Lava, Liquid::Honey, Liquid::Shimmer][k as usize & 3];
                self.inventory.set_selected(Item::Bucket(Some(kind)));
            }
        }
        if matches!(stack.item, Item::Block(_) | Item::Object(_)) {
            self.sfx.push(Sfx::Dig);
            if !free {
                self.inventory.consume();
            }
        } else {
            self.sfx.push(Sfx::Splash);
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
            Item::Tool(t) => match self.sheet(t.sheet()) {
                Some(s) => fit(s, [0, 0, s.w, s.h]),
                None => flat([180, 180, 190, 255]),
            },
            Item::Material(m) => match self.sheet(m.sheet()) {
                Some(s) => {
                    let mut v = fit(s, [0, 0, s.w, s.h]);
                    if m == Material::Gel {
                        v[0].color = [0.45, 0.65, 1.0, 0.85];
                    }
                    v
                }
                None => flat([90, 140, 255, 255]),
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
        self.mob_sprites(&mut over);
        for (_, p) in &self.others {
            self.player_sprites(p, false, &mut over);
        }
        if !free {
            self.player_sprites(&self.player, self.invulnerable > 0.0, &mut over);
        }
        for d in &self.drops {
            over.extend(self.item_icon(d.item, [d.x - 2.0, d.y - 4.0], DROP_SIZE + 4.0, SPRITE_LIT));
        }
        if let Some((tx, ty, p)) = self.mining() {
            over.push(Sprite::flat([tx as f32 * 16.0, ty as f32 * 16.0], [16.0, 16.0], [0.0, 0.0, 0.0, p * 0.6]));
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

    fn mob_sprites(&self, out: &mut Vec<Sprite>) {
        for m in &self.mobs {
            let b = &m.body;
            let (cx, bottom) = (b.x + b.w / 2.0, b.y + b.h);
            let hurt = m.stun > 0.0;
            // Monster sheets face left.
            let flags = SPRITE_LIT | if m.facing_left { 0 } else { SPRITE_FLIP };
            let (sheet, w, h, frame, color) = match m.kind {
                Kind::Slime => {
                    let frame = if b.on_ground { (m.timer * 4.0) as u32 % 2 } else { 1 };
                    ("NPC_1", 32, 26, frame, [0.45, 0.95, 0.45, 1.0])
                }
                Kind::Zombie => {
                    let frame = if b.on_ground { (m.timer * 8.0) as u32 % 3 } else { 2 };
                    ("NPC_3", 38, 48, frame, [1.0; 4])
                }
            };
            let color = if hurt { [1.0, 0.35, 0.35, 1.0] } else { color };
            match self.sheet(sheet) {
                Some(s) => {
                    let mut sp =
                        Sprite::sheet([cx - w as f32 / 2.0, bottom - h as f32 + 2.0], s, [0, frame * h, w, h], flags);
                    sp.color = color;
                    out.push(sp);
                }
                None => out.push(Sprite { flags: SPRITE_LIT, ..Sprite::flat([b.x, b.y], [b.w, b.h], color) }),
            }
        }
    }

    /// Layered body sheets, tinted, with the walk cycle on the legs.
    fn player_sprites(&self, p: &Player, dim: bool, out: &mut Vec<Sprite>) {
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
            s.color = if dim { color.map(|c| c * 0.5) } else { color };
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
    use crate::inventory::Stack;
    use crate::items::Tier;
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

    /// Hits a tile until it breaks; returns the number of hits.
    fn mine(g: &mut Game, x: i64, y: i64) -> u32 {
        let mut hits = 0;
        while g.world.tile(x, y) != NO_TILE {
            g.use_cd = 0.0;
            g.dig(x, y, false);
            hits += 1;
            assert!(hits < 50, "never breaks");
        }
        hits
    }

    #[test]
    fn tools_mine_by_power_and_hardness() {
        let mut g = game();
        g.inventory.selected = 0; // sword: no mining
        g.dig(16, 20, false);
        assert!(g.mining().is_none());
        g.inventory.selected = 1; // copper pickaxe: 35 / 2 per hit on stone
        g.dig(16, 20, false);
        let (_, _, p) = g.mining().unwrap();
        assert!((p - 0.175).abs() < 1e-4);
        g.dig(16, 20, false);
        assert_eq!(g.mining().unwrap().2, p, "waits for the use time");
        assert_eq!(mine(&mut g, 16, 20), 5);
        g.inventory.slots[1] = Some(Stack { item: Item::Tool(Tool::Pickaxe(Tier::Iron)), count: 1 });
        assert_eq!(mine(&mut g, 17, 20), 5, "40 / 2: still five hits");
    }

    #[test]
    fn crafting_needs_inputs_and_stations() {
        let mut g = game();
        let bench = RECIPES.iter().position(|r| r.out == Out::Furniture(Furniture::WorkBench)).unwrap();
        let sword = RECIPES.iter().position(|r| r.out == Out::Item(Item::Tool(Tool::Sword(Tier::Wood)))).unwrap();
        assert!(!g.craft(bench), "no wood");
        g.inventory.add(Item::Block(items::WOOD), 17);
        assert!(g.craft(bench));
        assert_eq!(g.inventory.count(Item::Block(items::WOOD)), 7);
        assert!(g.inventory.slots.iter().flatten().any(|s| matches!(s.item, Item::Object(_))));

        // The fixture's benches are in reach of the spawn, so the sword can be made.
        assert!(g.craftable().contains(&sword));
        g.player = Player::spawn(&g.world, 40);
        assert!(!g.craftable().contains(&sword), "too far from a bench");
        g.player = Player::spawn(&g.world, 15);
        assert!(g.craft(sword));
        assert_eq!(g.inventory.count(Item::Block(items::WOOD)), 0);
        assert_eq!(g.inventory.count(Item::Tool(Tool::Sword(Tier::Wood))), 1);
    }

    #[test]
    fn objects_drop_and_place_again() {
        let mut g = game();
        g.dig(11, 19, false);
        assert_eq!((g.world.tile(10, 19), g.world.tile(11, 19)), (NO_TILE, NO_TILE));
        assert_eq!(g.world.tile(12, 19), BENCH, "the bench beside stays");
        g.use_cd = 0.0;
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
        g.inventory.selected = 1;
        mine(&mut g, 12, 20);
        assert_eq!((g.world.tile(12, 19), g.world.tile(13, 19)), (NO_TILE, NO_TILE));
    }

    #[test]
    fn music_follows_depth_and_time() {
        let mut g = game();
        assert_eq!(g.music(), "Music_1");
        g.time = 0.9;
        assert_eq!(g.music(), "Music_3");
        g.player.y += 60.0 * 16.0;
        assert_eq!(g.music(), "Music_4");
    }

    #[test]
    fn monsters_hurt_and_die() {
        let mut g = game();
        let (px, py) = g.player.center();
        g.mobs.push(Mob::new(Kind::Slime, px, py + 21.0));
        g.update_mobs(0.0, true);
        assert_eq!(g.hp, MAX_HP - Kind::Slime.damage());
        g.update_mobs(0.1, true);
        assert_eq!(g.hp, MAX_HP - Kind::Slime.damage(), "invulnerable for a moment");
        for _ in 0..3 {
            let (mx, my) = g.mobs[0].body.center();
            assert!(g.attack(mx, my, false));
            g.attack_cd = 0.0;
        }
        assert!(g.mobs.is_empty());
    }
}
