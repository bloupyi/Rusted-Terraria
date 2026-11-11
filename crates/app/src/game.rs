//! Game state outside the window: the world, liquids, player, time of day, and
//! keeping the GPU copies (meshes, light, map) in step with edits.

use render::atlas::SheetRef;
use render::{Renderer, Scene, Sprite, SPRITE_FLIP, SPRITE_LIT};

use crate::audio::Sfx;
use crate::combat_text::{self, CombatText};
use crate::inventory::{Inventory, Item, Stack};
use crate::items::{self, Furniture, Material, Out, Tool, ToolStats, RECIPES};
use crate::magic::{self, Bolt, Dust, Star};
use crate::net::{MobState, Msg};
use crate::{armor, backdrop, cave, glow, item_ids, pots};
use formats::wld::Env;
use world::light::{self, Glow, Rect};
use world::liquid::{LiquidSim, OBSIDIAN};
use world::mining::{self, HitTiles, Place, Target};
use world::mob::{Kind, Mob};
use world::player::{self, Input, Player};
use world::{Liquid, World, CHUNK, NO_TILE};

pub const LIQUID_TICK: f32 = 1.0 / 30.0;
/// Liquids flow within this many chunks of a player, across and down.
const LIQUID_RANGE: (u32, u32) = (8, 6);
/// Seconds for a full day: 15 minutes of day, 9 of night.
pub const DAY_LENGTH: f32 = 24.0 * 60.0;
const DAYTIME: f32 = 15.0 / 24.0;
/// Reach for breaking and placing, in tiles from the player's centre.
const REACH: f32 = 10.0;
const STALE_LIGHT: u8 = 1;
const STALE_LOD: u8 = 2;
/// Light out of date from flowing liquid only, refreshed at most every `LIQUID_LIGHT_EVERY`.
const STALE_LIQUID_LIGHT: u8 = 4;
const LIQUID_LIGHT_EVERY: std::time::Duration = std::time::Duration::from_millis(100);
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
const POT: u16 = 28;
/// `Item.potionDelay`: a minute of potion sickness.
const POTION_DELAY: i32 = 3600;
/// Grass, vines and other plants: break without dropping anything.
const PLANTS: [u16; 13] = [3, 24, 52, 61, 62, 73, 74, 110, 113, 115, 201, 205, 233];
/// Objects that hang from the tile above them.
const HANGING: [u16; 8] = [34, 42, 52, 62, 91, 95, 115, 205];
const MAX_OBJECT: usize = 64;
/// Tiles under the ground line where the underground music starts.
const UNDERGROUND_DEPTH: u32 = 30;
pub const MAX_HP: i32 = 100;
const MAX_MOBS: usize = 5;
const SPAWN_EVERY: f32 = 2.0;
/// Monsters appear this many tiles to the side, and vanish past `DESPAWN`.
const SPAWN_DIST: (i64, i64) = (35, 55);
const DESPAWN: f32 = 100.0 * 16.0;
/// Crafting stations count within this many tiles of the player.
const STATION_REACH: i64 = 4;
/// Knockback for hits other players send, which carry only the damage.
const CLIENT_KNOCK_BACK: f32 = 5.0;
/// `Player.GetWeaponCrit` with no bonuses: 4 in 100.
const CRIT_CHANCE: u64 = 4;
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
    // Leaf styles the trees of `world::draw::tree_foliage` use.
    v.extend((0..=20).flat_map(|s| [format!("Tree_Tops_{s}"), format!("Tree_Branches_{s}")]));
    v.extend(
        ["Item_205", "Item_206", "Item_207", "Item_1128", "NPC_1", "NPC_2", "NPC_3", "NPC_4", "NPC_22", "TileCracks"]
            .map(String::from),
    );
    v.extend(
        ["Inventory_Back", "Inventory_Back9", "Inventory_Back13", "Inventory_Back14", "Heart", "Mana"]
            .map(String::from),
    );
    v.extend(items::sheets());
    v.extend(pots::SHEETS.iter().map(|s| s.to_string()));
    v.extend(magic::SHEETS.iter().map(|s| s.to_string()));
    v.extend(glow::TORCH_ITEMS.iter().map(|id| format!("Item_{id}")));
    v.extend(armor::SHEETS.iter().map(|s| s.to_string()));
    v.extend(armor::picture_sheets());
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
/// `WorldItem.DefaultGrabDelay`: 100 ticks.
const GRAB_DELAY: f32 = 100.0 / 60.0;
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
    /// Seconds before the player who threw it can take it back (`grabDelayTime`).
    pub grab_delay: f32,
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
    /// Damage numbers in flight.
    pub combat_text: Vec<CombatText>,
    /// The game's texts in the player's language; empty without an install.
    pub texts: formats::lang::Texts,
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
    /// Ticks without damage after a hit.
    immune: i32,
    /// `lifeRegenTime` and `lifeRegenCount`.
    regen_time: f32,
    regen_count: i32,
    /// `potionDelay`: ticks of potion sickness left.
    pub potion_delay: i32,
    pub mana: magic::Mana,
    /// Bolts in flight and dust in the air.
    pub bolts: Vec<Bolt>,
    /// Falling stars and their spawners.
    pub stars: Vec<Star>,
    pub dust: Vec<Dust>,
    /// `itemRotation` while a staff is held out.
    item_rotation: f32,
    /// Where the last bolt was aimed, for the ruby staff's second shot.
    aim: (f32, f32),
    /// Ticks before the swing can hit another monster.
    attack_cd: i32,
    /// World point the held item is used at this frame.
    use_target: Option<(f32, f32)>,
    /// The use button was up last tick, so a swing without auto reuse can start.
    release_use: bool,
    /// Ticks left in the swing and before the next hit.
    item_animation: i32,
    item_time: i32,
    /// Damage on recently hit tiles and walls.
    pub hits: HitTiles,
    spawn_acc: f32,
    spawn_x: u32,
    rng: u64,
    /// Per column, the first solid row as loaded, two rows down.
    pub surface: Vec<u32>,
    /// Sky and background state from the world file, or guessed from the terrain.
    pub env: Env,
    stale: Vec<u8>,
    lod_queue: Vec<u32>,
    /// Per chunk, `DIRTY_*` bits not yet remeshed.
    mesh_pending: Vec<u8>,
    /// Per chunk, the tree cells with a top or a branch; filled for visible chunks in `sync`.
    tree_cells: Vec<Option<Vec<(u32, u32)>>>,
    liquid_light_at: std::time::Instant,
    mesh_queue: std::collections::VecDeque<u32>,
    liquid_acc: f32,
    /// Surface background areas fading in and out, and the tick they were last stepped.
    bg_fade: crate::surface::Fade,
    bg_tick: Option<u32>,
    /// Wind phases for plants and trees, and the time not yet turned into ticks.
    pub wind: world::draw::Wind,
    /// Part of a 60 Hz tick not yet run.
    pub tick_acc: f32,
    /// Pulse of glowing items on the ground.
    ess: glow::Ess,
    /// Moving lights in the light map now, relit away when they change.
    glows: Vec<Glow>,
}

impl Game {
    pub fn new(world: World, scene: Scene) -> Self {
        let sim = LiquidSim::new(&world);
        let player = Player::spawn(&world, world.width / 2);
        let stale = vec![0; world.chunks.len()];
        let mesh_pending = vec![0; world.chunks.len()];
        let surface = surface(&world);
        let spawn_x = world.width / 2;
        let env = world.header.env.clone().unwrap_or_else(|| guess_env(&world, &surface));
        let time = match &world.header.env {
            Some(e) if e.day_time => (e.time / (backdrop::DAY_TICKS + backdrop::NIGHT_TICKS)) as f32,
            Some(e) => ((backdrop::DAY_TICKS + e.time) / (backdrop::DAY_TICKS + backdrop::NIGHT_TICKS)) as f32,
            None => 0.25,
        };
        let mut g = Self {
            world,
            scene,
            sim,
            player,
            time,
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
            texts: Default::default(),
            combat_text: Vec::new(),
            mobs: Vec::new(),
            sfx: Vec::new(),
            others: Vec::new(),
            client: false,
            outbox: Vec::new(),
            hp: MAX_HP,
            immune: 0,
            regen_time: 0.0,
            regen_count: 0,
            potion_delay: 0,
            mana: Default::default(),
            bolts: Vec::new(),
            stars: Vec::new(),
            dust: Vec::new(),
            item_rotation: 0.0,
            aim: (0.0, 0.0),
            attack_cd: 0,
            use_target: None,
            release_use: true,
            item_animation: 0,
            item_time: 0,
            hits: HitTiles::default(),
            spawn_acc: 0.0,
            spawn_x,
            rng: 0x9E37_79B9_7F4A_7C15,
            surface,
            env,
            stale,
            lod_queue: Vec::new(),
            tree_cells: vec![None; mesh_pending.len()],
            liquid_light_at: std::time::Instant::now(),
            mesh_pending,
            mesh_queue: Default::default(),
            liquid_acc: 0.0,
            bg_fade: Default::default(),
            bg_tick: None,
            wind: Default::default(),
            tick_acc: 0.0,
            ess: Default::default(),
            glows: Vec::new(),
        };
        // The guide lives at the spawn point.
        let (gx, gy) = (g.player.center().0, g.player.y + player::HEIGHT);
        g.mobs.push(Mob::new(Kind::Guide, gx, gy));
        g
    }

    pub fn update(&mut self, input: Option<Input>, dt: f32, time_scale: f32) {
        self.time = (self.time + dt * time_scale / DAY_LENGTH).fract();
        // The game's 60 Hz tick; a long frame catches up at most a few ticks.
        self.tick_acc = (self.tick_acc + dt * 60.0).min(4.0);
        let target = self.use_target.take();
        while self.tick_acc >= 1.0 {
            self.tick_acc -= 1.0;
            self.wind.tick(self.env.wind);
            self.ess.tick();
            self.use_item(target);
            self.torch_flame();
            if let Some(input) = input {
                let facing = self.player.facing_left;
                self.player.tick(&self.world, input);
                if self.item_animation > 0 && !self.held_tool().use_turn {
                    self.player.facing_left = facing;
                }
                self.contact_damage();
                self.life_regen();
                self.potion_delay = (self.potion_delay - 1).max(0);
                let still = self.player.vx.abs() < 0.05 && self.player.vy.abs() < 0.05;
                if self.mana.regen(still) {
                    self.sfx.push(Sfx::MaxMana);
                }
            }
            self.update_bolts();
            self.update_stars();
            self.dust.retain_mut(|d| d.update());
            for (_, p) in &mut self.others {
                p.animate();
            }
            combat_text::update(&mut self.combat_text);
        }
        self.update_mobs(dt, input.is_some());
        self.update_drops(dt);
        // At most one tick per frame: under load liquids slow down instead of piling up work.
        self.liquid_acc = (self.liquid_acc + dt).min(LIQUID_TICK);
        if self.liquid_acc >= LIQUID_TICK {
            self.liquid_acc -= LIQUID_TICK;
            // Only around players: far liquid waits until someone comes near.
            let w = &self.world;
            let near: Vec<_> = std::iter::once(&self.player)
                .chain(self.others.iter().map(|(_, p)| p))
                .map(|p| {
                    let (x, y) = p.center();
                    let (cx, cy) = ((x / 16.0) as u32 / CHUNK, (y / 16.0).max(0.0) as u32 / CHUNK);
                    let (rx, ry) = LIQUID_RANGE;
                    (
                        cx.saturating_sub(rx),
                        cy.saturating_sub(ry),
                        (cx + rx + 1).min(w.chunks_x),
                        (cy + ry + 1).min(w.chunks_y),
                    )
                })
                .collect();
            self.sim.tick_near(&mut self.world, &near);
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
        self.spawn_acc += dt;
        if fight && !self.client && self.spawn_acc >= SPAWN_EVERY {
            self.spawn_acc = 0.0;
            if self.mobs.iter().filter(|m| !m.kind.friendly() && m.kind != Kind::EyeBoss).count() < MAX_MOBS {
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
            // The boss leaves at dawn; the guide never leaves.
            let day = self.time < DAYTIME;
            self.mobs.retain(|m| {
                m.kind.friendly()
                    || (players.iter().any(|p| dist(m, p) < DESPAWN)
                        && m.body.y < height
                        && !(day && m.kind == Kind::EyeBoss))
            });
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
        let roll = (r >> 8) % 10;
        if night && roll < 3 {
            // Demon eyes come in the air, above the ground found.
            let air = y - 10;
            if (0..3).all(|k| w.in_bounds(x, air - k) && !w.solid(x, air - k)) {
                self.mobs.push(Mob::new(Kind::DemonEye, x as f32 * 16.0 + 8.0, air as f32 * 16.0));
            }
            return;
        }
        let kind = if night && roll < 7 { Kind::Zombie } else { Kind::Slime };
        self.mobs.push(Mob::new(kind, x as f32 * 16.0 + 8.0, y as f32 * 16.0));
    }

    /// `Update_NPCCollision` and `Hurt`: touching a monster costs its damage,
    /// varied, then 40 ticks without damage and a knockback away from it.
    fn contact_damage(&mut self) {
        if self.immune > 0 {
            self.immune -= 1;
            return;
        }
        let p = self.player.body;
        let Some(m) = self.mobs.iter().find(|m| m.kind.damage() > 0 && m.body.touches(&p)) else { return };
        let dir = if m.body.center().0 < p.center().0 { 1.0 } else { -1.0 };
        let damage = armor::damage_taken(self.damage_var(m.kind.damage()), self.defense());
        self.hp -= damage;
        self.regen_time = 0.0;
        let b = self.player.body;
        self.text_at([b.x, b.y, b.w, b.h], combat_text::DAMAGED_FRIENDLY, damage, false);
        self.immune = if damage == 1 { 20 } else { 40 };
        (self.player.vx, self.player.vy) = (4.5 * dir, -3.5);
        self.sfx.push(if self.hp <= 0 { Sfx::PlayerKilled } else { Sfx::PlayerHit });
        if self.hp <= 0 {
            self.hp = MAX_HP;
            self.player = Player::spawn(&self.world, self.spawn_x);
        }
    }

    /// `headSlot`, `bodySlot` and `legSlot` of what the player wears.
    fn worn_pictures(&self) -> [i16; 3] {
        let [h, b, l] = self.inventory.worn().map(|w| w.and_then(item_ids::stats));
        [h.map_or(0, |s| s.head_slot), b.map_or(0, |s| s.body_slot), l.map_or(0, |s| s.leg_slot)]
    }

    /// `statDefense` of what the player wears.
    pub fn defense(&self) -> i32 {
        armor::defense(self.inventory.worn())
    }

    /// `UpdateLifeRegen` without buffs, debuffs or accessories: after five
    /// seconds unhurt life comes back faster and faster, more when standing still.
    fn life_regen(&mut self) {
        self.regen_time = (self.regen_time + 1.0).min(3600.0);
        let t = self.regen_time;
        let steps = [300.0, 600.0, 900.0, 1200.0, 1500.0, 1800.0, 2400.0, 3000.0, 3600.0];
        let mut regen = steps.iter().filter(|&&s| t >= s).count() as f32;
        regen *= if self.player.vx != 0.0 { 0.5 } else { 1.25 };
        if self.env.expert {
            regen /= 2.0;
        }
        regen *= MAX_HP as f32 / 400.0 * 0.85 + 0.15;
        // `Math.Round` rounds halves to even.
        self.regen_count += regen.round_ties_even() as i32;
        while self.regen_count >= 120 {
            self.regen_count -= 120;
            self.hp = (self.hp + 1).min(MAX_HP);
        }
    }

    /// `Player.Heal`: life back, shown in green over the player.
    fn heal(&mut self, n: i32) {
        self.hp = (self.hp + n).min(MAX_HP);
        let b = self.player.body;
        self.text_at([b.x, b.y, b.w, b.h], combat_text::HEAL_LIFE, n, false);
    }

    /// Drinks `id` if it heals (`ItemCheck` with `ApplyPotionDelay` and
    /// `ApplyLifeAndOrMana`): potion sickness for a minute, life back at once.
    fn drink(&mut self, id: u16) -> bool {
        let Some(s) = item_ids::stats(id) else { return false };
        let potion = s.flags & item_ids::POTION != 0;
        if s.heal_life <= 0 || (potion && self.potion_delay > 0) {
            return false;
        }
        if potion {
            self.potion_delay = POTION_DELAY;
        }
        self.item_animation = s.use_animation as i32;
        self.sfx.push(Sfx::Drink);
        self.heal(s.heal_life as i32);
        true
    }

    /// `QuickHeal` (H): the potion whose healing best fits the missing life.
    pub fn quick_heal(&mut self) {
        if self.hp >= MAX_HP || self.potion_delay > 0 || self.item_animation > 0 {
            return;
        }
        let missing = MAX_HP - self.hp;
        let mut best: Option<(usize, i32)> = None;
        for (i, slot) in self.inventory.slots.iter().enumerate() {
            let Some(Stack { item: Item::Id(id), .. }) = slot else { continue };
            let Some(s) = item_ids::stats(*id) else { continue };
            if s.flags & item_ids::POTION == 0 || s.heal_life <= 0 {
                continue;
            }
            let d = s.heal_life as i32 - missing;
            let better = match best {
                None => true,
                Some((_, b)) if b < 0 => d > b,
                Some((_, b)) => d < b && d >= 0,
            };
            if better {
                best = Some((i, d));
            }
        }
        let Some((i, _)) = best else { return };
        let Some(Stack { item: Item::Id(id), .. }) = self.inventory.slots[i] else { return };
        if self.drink(id) {
            self.inventory.remove(Item::Id(id), 1);
        }
    }

    /// `WorldGen.SpawnThingsFromPot`, for the pot whose top-left cell is `(x, y)`, broken at row `ty`.
    fn shatter_pot(&mut self, x: i64, y: i64, ty: i64) {
        let style = self.world.frame(x, y).1 as i32 / 36;
        self.sfx.push(match style {
            7..=9 => Sfx::Grass,
            16..=24 => Sfx::MobKilled,
            _ => Sfx::Shatter,
        });
        let torches = self
            .inventory
            .slots
            .iter()
            .flatten()
            .filter(|s| matches!(s.item, Item::Object(t) if self.templates[t as usize].tile == TORCH))
            .map(|s| s.count as u32)
            .sum();
        let wall = self.world.wall(x, ty);
        let pot = pots::Pot {
            style,
            y: ty as f64,
            world_surface: self.env.world_surface,
            rock_layer: self.env.rock_layer,
            height: self.world.height as f64,
            dungeon_wall: matches!(wall, 7..=9 | 94..=99),
            in_liquid: self.world.liquid(x, ty) > 0,
            expert: self.env.expert,
            hurt: self.hp < MAX_HP,
            torches,
        };
        let loot = pots::loot(&pot, |n| (self.random() % n.max(1) as u64) as i32);
        for (id, count) in loot {
            let item = match id {
                pots::TORCH => self.furniture(Furniture::Torch),
                965 => Item::Block(213),
                _ => Item::Id(id),
            };
            self.toss(x as f32 * 16.0 + 8.0, ty as f32 * 16.0, item, count);
        }
    }

    /// `Main.DamageVar`: within 15% either way.
    fn damage_var(&mut self, damage: i32) -> i32 {
        let k = (self.random() % 31) as i32 - 15;
        (damage as f32 * (1.0 + k as f32 * 0.01)) as i32
    }

    /// `CombatText.NewText` over `rect`.
    fn text_at(&mut self, rect: [f32; 4], color: [u8; 4], amount: i32, crit: bool) {
        let mut texts = std::mem::take(&mut self.combat_text);
        combat_text::spawn(&mut texts, rect, color, amount, crit, |lo, hi| {
            lo + (self.random() % (hi - lo + 1) as u64) as i32
        });
        self.combat_text = texts;
    }

    /// The number `StrikeNPC` shows over monster `i` for `taken` damage.
    fn mob_text(&mut self, i: usize, taken: i32, crit: bool, from_net: bool) {
        let m = &self.mobs[i];
        let b = m.body;
        let color = match (m.kind.friendly(), crit) {
            (true, false) => combat_text::DAMAGED_FRIENDLY,
            (true, true) => combat_text::DAMAGED_FRIENDLY_CRIT,
            (false, false) => combat_text::DAMAGED_HOSTILE,
            (false, true) => combat_text::DAMAGED_HOSTILE_CRIT,
        };
        let color = if from_net && !m.kind.friendly() { combat_text::others(color) } else { color };
        self.text_at([b.x, b.y, b.w, b.h], color, taken, crit);
    }

    /// Hurts a monster; a dead slime leaves gel. `from_net`: another player's hit.
    fn hit_mob(&mut self, i: usize, damage: i32, knock_back: f32, dir: f32, crit: bool, from_net: bool) {
        let (taken, killed) = self.mobs[i].strike(damage, knock_back, dir, crit);
        self.mob_text(i, taken, crit, from_net);
        if !killed {
            self.sfx.push(Sfx::MobHit);
            return;
        }
        let m = self.mobs.swap_remove(i);
        self.sfx.push(Sfx::MobKilled);
        let (x, y) = m.body.center();
        let r = self.random();
        let drop = match m.kind {
            Kind::Slime => Some((Item::Material(Material::Gel), 1 + (r % 2) as u16)),
            Kind::DemonEye if r.is_multiple_of(2) => Some((Item::Material(Material::Lens), 1)),
            Kind::EyeBoss => Some((Item::Block(items::DEMONITE_ORE), 30 + (r % 30) as u16)),
            _ => None,
        };
        if let Some((item, count)) = drop {
            self.drops.push(Drop { x, y, vx: 0.0, vy: -150.0, item, count, grab_delay: 0.0 });
        }
        if !m.kind.friendly() {
            self.drop_life_and_mana(x, y);
        }
    }

    /// `NPCLoot_DropCommonLifeAndMana` without luck: now and then a heart or a
    /// mana star, and a mana star every other kill while mana is short.
    fn drop_life_and_mana(&mut self, x: f32, y: f32) {
        let short = self.mana.cur < self.mana.max;
        if self.random().is_multiple_of(6) {
            if self.random().is_multiple_of(2) && short {
                self.toss(x, y, Item::Id(magic::MANA_STAR), 1);
            } else if self.random().is_multiple_of(2) && self.hp < MAX_HP {
                self.toss(x, y, Item::Id(pots::HEART), 1);
            }
        }
        if self.random().is_multiple_of(2) && short {
            self.toss(x, y, Item::Id(magic::MANA_STAR), 1);
        }
    }

    /// `Item.NewItem`: an item dropped with a small random toss.
    fn toss(&mut self, x: f32, y: f32, item: Item, count: u16) {
        let vx = ((self.random() % 61) as f32 - 30.0) * 0.1 * 60.0;
        let vy = ((self.random() % 26) as f32 - 40.0) * 0.1 * 60.0;
        self.drops.push(Drop { x, y, vx, vy, item, count, grab_delay: 0.0 });
    }

    /// The eye boss, at night and if none is around, from above the player.
    pub fn summon_boss(&mut self) -> bool {
        if self.client || self.time < DAYTIME || self.mobs.iter().any(|m| m.kind == Kind::EyeBoss) {
            return false;
        }
        let (px, py) = self.player.center();
        let side = if self.random().is_multiple_of(2) { -1.0 } else { 1.0 };
        self.mobs.push(Mob::new(Kind::EyeBoss, px + side * 600.0, py - 400.0));
        self.inventory.consume();
        self.sfx.push(Sfx::Roar);
        true
    }

    /// The boss's life, 0 to 1, while one is around.
    pub fn boss_life(&self) -> Option<f32> {
        self.mobs.iter().find(|m| m.kind == Kind::EyeBoss).map(|m| m.hp.max(0) as f32 / Kind::EyeBoss.max_hp() as f32)
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
                hp: m.hp,
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
                (p.x, p.y, p.vx, p.vy, p.facing_left) = (x, y, vx, vy, facing_left);
                p.prev = [x, y];
                p.on_ground = vy == 0.0;
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
                        let kind = Kind::from_u8(s.kind).unwrap_or(Kind::Slime);
                        let mut m = Mob::new(kind, 0.0, 0.0);
                        // Grounded while the snapshot keeps it at the same height.
                        let old = self.mobs.iter().find(|o| o.kind == kind && (o.body.x - s.x).abs() < 16.0);
                        m.body.on_ground = old.is_some_and(|o| o.body.y == s.y);
                        m.timer = old.map_or(0.0, |o| o.timer + 1.0 / 60.0);
                        (m.body.x, m.body.y, m.facing_left) = (s.x, s.y, s.facing_left);
                        m.stun = if s.hurt { 0.1 } else { 0.0 };
                        m.hp = s.hp;
                        m
                    })
                    .collect();
            }
            Msg::Hit { index, damage } if !self.client && (index as usize) < self.mobs.len() => {
                let from_x = self.others.iter().find(|o| o.0 == from).map_or(0.0, |o| o.1.center().0);
                let dir = if self.mobs[index as usize].body.center().0 < from_x { -1.0 } else { 1.0 };
                self.hit_mob(index as usize, damage as i32, CLIENT_KNOCK_BACK, dir, false, true);
            }
            Msg::Leave { id } => self.others.retain(|o| o.0 != id),
            Msg::Time(t) if self.client => self.time = t,
            _ => {}
        }
    }

    /// Drops fall onto blocks, fly to a nearby player and go into the inventory.
    fn update_drops(&mut self, dt: f32) {
        let mut stars = 0;
        let night = self.time >= DAYTIME;
        // Mana stars come only to a player short of mana or holding a magic weapon.
        let magic_held = matches!(self.inventory.selected().map(|s| s.item), Some(Item::Id(id))
            if item_ids::stats(id).is_some_and(|s| s.flags & item_ids::MAGIC != 0));
        let wants_stars = magic_held || self.mana.cur < self.mana.max;
        let (px, py) = self.player.center();
        let world = &self.world;
        let inventory = &mut self.inventory;
        let sfx = &mut self.sfx;
        let mut hearts = 0;
        self.drops.retain_mut(|d| {
            let (dx, dy) = (px - (d.x + DROP_SIZE / 2.0), py - (d.y + DROP_SIZE / 2.0));
            d.grab_delay = (d.grab_delay - dt).max(0.0);
            // Out of reach while the thrower's delay runs.
            let dist = if d.grab_delay > 0.0 { f32::INFINITY } else { (dx * dx + dy * dy).sqrt() };
            if d.item == Item::Id(magic::FALLEN_STAR) && !night {
                // Fallen stars vanish at daybreak.
                return false;
            }
            let dist = if d.item == Item::Id(magic::MANA_STAR) && !wants_stars { f32::INFINITY } else { dist };
            if dist < PICKUP && d.item == Item::Id(magic::MANA_STAR) {
                stars += 1;
                sfx.push(Sfx::Grab);
                return false;
            }
            if dist < PICKUP && d.item == Item::Id(pots::HEART) {
                hearts += 1;
                sfx.push(Sfx::Grab);
                return false;
            }
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
        // `PickupItem`: a heart is 20 life, never an item.
        for _ in 0..hearts {
            self.heal(20);
        }
        // A mana star is 100 mana.
        for _ in 0..stars {
            self.mana.star();
            let b = self.player.body;
            self.text_at([b.x, b.y, b.w, b.h], combat_text::HEAL_MANA, 100, false);
        }
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

    /// The game's day and its tick within it, from `time`.
    pub fn ticks(&self) -> (bool, f64) {
        let day = (backdrop::DAY_TICKS + backdrop::NIGHT_TICKS) * self.time as f64;
        if self.time < DAYTIME {
            (true, day)
        } else {
            (false, day - backdrop::DAY_TICKS)
        }
    }

    /// `water_N` sheet for water seen from tile row `y`: the forest style above
    /// ground, the cavern ones below, blood red on a blood moon night.
    pub fn water_style(&self, y: f32) -> u32 {
        let y = y as f64;
        if self.env.blood_moon && !self.ticks().0 {
            9
        } else if y > self.env.rock_layer + 40.0 {
            8
        } else if y > self.env.world_surface {
            7
        } else {
            0
        }
    }

    /// Skylight colour and the colour behind everything (the sky's lower tone).
    pub fn sky(&self) -> ([f32; 3], [f32; 3]) {
        let (day, t) = self.ticks();
        let sky = backdrop::colors(day, t, self.env.blood_moon).sky;
        let low = [132.0, 170.0, 248.0];
        (backdrop::tile_light(sky), [0, 1, 2].map(|i| sky[i] * low[i] / (255.0 * 255.0)))
    }

    /// Sky, sun or moon and surface backgrounds, in world pixels for a view
    /// centred on `center` at `zoom` in a `size` window. `ticks` drives animations.
    pub fn backdrop(&mut self, center: [f32; 2], zoom: f32, size: (u32, u32), ticks: u32) -> Vec<Sprite> {
        let (day, time) = self.ticks();
        let (w, h) = (size.0 as f32, size.1 as f32);
        let zones = world::scene::scan(&self.world, &self.env, (center[0] / 16.0) as i64, (center[1] / 16.0) as i64);
        let want =
            crate::surface::preferred(&self.env, &zones, ((center[0] - w / 2.0 + (w as i32 / 2) as f32) / 16.0) as i32);
        // The game steps the fades once per frame at 60 frames a second.
        let steps = self.bg_tick.map_or(1, |t| ticks.saturating_sub(t).min(600));
        for _ in 0..steps {
            self.bg_fade.step(&self.env, want);
        }
        self.bg_tick = Some(ticks);
        let scene = backdrop::Scene {
            env: &self.env,
            width: self.world.width,
            height: self.world.height,
            day,
            time,
            ticks,
            sheets: &self.scene.mesher.atlas.named,
        };
        let view = backdrop::View { x: center[0] - w / 2.0, y: center[1] - h / 2.0, w, h };
        let sheets = &self.scene.mesher.atlas.named;
        let mut v = backdrop::sprites(&scene, &view, &self.bg_fade, zones.holy_tiles);
        v.extend(cave::underworld(&self.env, self.world.height, center, (w, h), zoom, ticks, sheets));
        for s in &mut v {
            s.pos = [0, 1].map(|i| center[i] + (s.pos[i] - [w, h][i] / 2.0) / zoom);
            s.size = s.size.map(|k| k / zoom);
        }
        let (hw, hh) = (w / (2.0 * zoom), h / (2.0 * zoom));
        let cave_view = cave::View {
            screen_x: (center[0] - w / 2.0) as f64,
            screen_y: (center[1] - h / 2.0) as f64,
            screen_w: w as f64,
            screen_h: h as f64,
            x0: (center[0] - hw) as f64,
            y0: (center[1] - hh) as f64,
            x1: (center[0] + hw) as f64,
            y1: (center[1] + hh) as f64,
        };
        v.extend(cave::layers(
            &self.env,
            self.world.header.world_id,
            self.world.height,
            &zones,
            &cave_view,
            ticks,
            sheets,
        ));
        v
    }

    fn in_reach(&self, tx: i64, ty: i64, free: bool) -> bool {
        let (cx, cy) = self.player.center();
        let (dx, dy) = ((tx as f32 + 0.5) * 16.0 - cx, (ty as f32 + 0.5) * 16.0 - cy);
        self.world.in_bounds(tx, ty) && (free || (dx * dx + dy * dy).sqrt() <= REACH * 16.0)
    }

    fn important(&self, t: u16) -> bool {
        self.world.header.frame_important.get(t as usize).copied().unwrap_or(false)
    }

    /// Uses the held item at a world point for this frame; with the free
    /// camera the tile there breaks at once and drops nothing.
    pub fn use_at(&mut self, wx: f32, wy: f32, free: bool) {
        let (tx, ty) = ((wx / 16.0).floor() as i64, (wy / 16.0).floor() as i64);
        if free {
            if self.world.in_bounds(tx, ty) {
                self.break_tile(tx, ty, false);
            }
            return;
        }
        self.use_target = Some((wx, wy));
    }

    fn held_tool(&self) -> ToolStats {
        match self.inventory.selected().map(|s| s.item) {
            Some(Item::Tool(t)) => t.stats(),
            _ => ToolStats::default(),
        }
    }

    /// One tick of the held item, as the game's `ItemCheck` counts it: a swing
    /// starts while the button is held (again only after a release without
    /// auto reuse), a tool hits whenever the item time has run out during a
    /// swing, and the blade hits monsters all through it.
    fn use_item(&mut self, target: Option<(f32, f32)>) {
        let s = self.held_tool();
        let held = target.is_some();
        if let Some(Stack { item: Item::Id(id), .. }) = self.inventory.selected() {
            if held && self.item_animation == 0 && self.release_use {
                if magic::staff_bolt(id).is_some() {
                    self.cast(id, target.unwrap());
                } else if id == magic::MANA_CRYSTAL {
                    self.use_crystal();
                } else if self.drink(id) {
                    self.inventory.consume();
                }
            }
            if let Some(bolt) = magic::staff_bolt(id).filter(|_| self.item_animation > 0) {
                self.mana.delay_regen();
                // The ruby staff shoots again halfway through.
                let half = item_ids::stats(id).map_or(0, |s| s.use_animation as i32 / 2);
                if magic::Features::of(bolt).spread && self.item_animation == half {
                    self.shoot(id, self.aim);
                }
            }
        }
        if held && self.item_animation == 0 && s.use_animation > 0 && (self.release_use || s.auto_reuse) {
            self.item_animation = s.use_animation;
            if let Some((wx, _)) = target {
                self.player.facing_left = wx < self.player.center().0;
            }
            if s.damage > 0 {
                self.sfx.push(Sfx::Swing);
            }
        }
        if self.item_animation > 0 {
            self.item_animation -= 1;
        }
        self.release_use = !held;
        if self.item_time > 0 {
            self.item_time -= 1;
        }
        if self.attack_cd > 0 {
            self.attack_cd -= 1;
        }
        for m in &mut self.mobs {
            m.melee_cooldown = (m.melee_cooldown - 1).max(0);
        }
        if let Some((wx, wy)) = target {
            let (x, y) = ((wx / 16.0).floor() as i64, (wy / 16.0).floor() as i64);
            let tool = s.pick > 0 || s.axe > 0 || s.hammer > 0;
            if tool && self.item_time == 0 && self.item_animation > 0 && self.in_tool_range(x, y, s.tile_boost) {
                self.hit_with(s, x, y);
            }
        }
        if self.item_animation > 0 && s.damage > 0 {
            self.melee(s);
        }
    }

    /// Size of the held item's picture, which the swing's reach follows.
    fn held_frame(&self) -> (f32, f32) {
        let sheet = match self.inventory.selected().map(|s| s.item) {
            Some(Item::Tool(t)) => self.sheet(t.sheet()),
            _ => None,
        };
        sheet.map_or((32.0, 32.0), |s| (s.w as f32, s.h as f32))
    }

    /// `ItemCheck_ApplyUseStyle` for a swing: where the item is held and its
    /// angle at this point of the swing.
    fn swing_pose(&self, s: ToolStats) -> ([f32; 2], f32) {
        let (w, h) = self.held_frame();
        let p = &self.player;
        let dir = if p.facing_left { -1.0 } else { 1.0 };
        let (anim, max) = (self.item_animation as f32, s.use_animation as f32);
        let mid = p.x + player::WIDTH * 0.5;
        let step =
            |table: &[(f32, f32)], v: f32, base: f32| table.iter().rev().find(|t| v >= t.0).map_or(base, |t| t.1);
        let at = if anim < max * 0.333 {
            let n = step(&[(33.0, 14.0), (52.0, 24.0), (64.0, 28.0), (92.0, 38.0)], w, 10.0);
            [mid + (w * 0.5 - n) * dir, p.y + 24.0]
        } else if anim < max * 0.666 {
            let n = step(&[(33.0, 18.0), (52.0, 24.0), (64.0, 28.0), (92.0, 38.0)], w, 10.0);
            let m = step(&[(33.0, 8.0), (53.0, 12.0), (65.0, 14.0)], h, 10.0);
            [mid + (w * 0.5 - n) * dir, p.y + m]
        } else {
            let n = step(&[(33.0, 14.0), (48.0, 18.0), (52.0, 24.0), (64.0, 28.0), (92.0, 38.0)], w, 6.0);
            let m = step(&[(53.0, 12.0), (65.0, 14.0)], h, 10.0);
            [mid - (w * 0.5 - n) * dir, p.y + m]
        };
        (at, (anim / max - 0.5) * -dir * 3.5 - dir * 0.3)
    }

    /// `ItemCheck_GetMeleeHitbox` for a swing: x, y, width, height.
    fn melee_hitbox(&self, s: ToolStats) -> [i32; 4] {
        let (w, h) = self.held_frame();
        let ([x, y], _) = self.swing_pose(s);
        let dir = if self.player.facing_left { -1 } else { 1 };
        let (mut rx, mut ry) = (x as i32, y as i32);
        let (mut rw, mut rh) = ((w * s.scale) as i32, (h * s.scale) as i32);
        if dir == -1 {
            rx -= rw;
        }
        ry -= rh;
        let (anim, max) = (self.item_animation as f64, s.use_animation as f64);
        if anim < max * 0.333 {
            if dir == -1 {
                rx -= (rw as f64 * 1.4 - rw as f64) as i32;
            }
            rw = (rw as f64 * 1.4) as i32;
            ry += (rh as f64 * 0.5) as i32;
            rh = (rh as f64 * 1.1) as i32;
        } else if anim >= max * 0.666 {
            if dir == 1 {
                rx -= (rw as f64 * 1.2) as i32;
            }
            rw *= 2;
            ry -= (rh as f64 * 1.4 - rh as f64) as i32;
            rh = (rh as f64 * 1.4) as i32;
        }
        [rx, ry, rw, rh]
    }

    /// `ItemCheck_MeleeHitNPCs`: each monster in the swing once per swing, and
    /// after a hit none for a third of it.
    fn melee(&mut self, s: ToolStats) {
        let [x, y, w, h] = self.melee_hitbox(s).map(|v| v as f32);
        let dir = if self.player.facing_left { -1.0 } else { 1.0 };
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            let b = &m.body;
            let inside = x < b.x + b.w && b.x < x + w && y < b.y + b.h && b.y < y + h;
            if m.kind.friendly() || m.melee_cooldown > 0 || self.attack_cd > 0 || !inside {
                continue;
            }
            self.mobs[i].melee_cooldown = self.item_animation;
            self.attack_cd = ((s.use_animation as f64 * 0.33) as i32).max(1);
            self.strike(i, s.damage, s.knock_back, dir, 0);
            // A kill shifts the list; the cooldown stops more hits this tick anyway.
            break;
        }
    }

    /// The player's hit on monster `i`: varied damage, maybe a crit; on a
    /// client the host applies it.
    /// `armor_pen` adds back up to half the defense (`checkArmorPenetration`).
    fn strike(&mut self, i: usize, damage: i32, knock_back: f32, dir: f32, armor_pen: i32) {
        let def = self.mobs[i].kind.defense();
        let pen = if armor_pen <= 0 || def <= 0 { 0 } else { armor_pen.min(def) / 2 };
        let damage = self.damage_var(damage) + pen;
        let crit = self.random() % 100 < CRIT_CHANCE;
        if self.client {
            // The number shows here as `StrikeNPC` would work it out.
            let def = self.mobs[i].kind.defense() as f64 * 0.5;
            let taken = ((damage as f64 - def).max(1.0) * if crit { 2.0 } else { 1.0 }) as i32;
            self.mob_text(i, taken.max(1), crit, false);
            let damage = if crit { damage * 2 } else { damage };
            self.sfx.push(Sfx::MobHit);
            self.outbox.push(Msg::Hit { index: i as u16, damage: damage as u16 });
        } else {
            self.hit_mob(i, damage, knock_back, dir, crit, false);
        }
    }

    /// `ItemCheck_TryStartUse` for a gem staff: pays the mana, faces the cursor
    /// and shoots at it.
    fn cast(&mut self, id: u16, (wx, wy): (f32, f32)) {
        let Some(s) = item_ids::stats(id) else { return };
        if !self.mana.pay(s.mana as i32) {
            return;
        }
        self.item_animation = s.use_animation as i32;
        self.sfx.push(Sfx::Staff);
        let cx = self.player.center().0;
        self.player.facing_left = wx - cx <= 0.0;
        self.shoot(id, (wx, wy));
        let dir = if self.player.facing_left { -1.0 } else { 1.0 };
        let v = self.bolts.last().map_or([dir, 0.0], |b| b.vel);
        self.item_rotation = (v[1] * dir).atan2(v[0] * dir);
    }

    /// `ItemCheck_Shoot` for a gem staff: emerald goes five times as fast,
    /// sapphire shoots a pair, ruby starts somewhere on a 12 px circle; the
    /// damage is cut by `GetDamageMultiplier`.
    fn shoot(&mut self, id: u16, (wx, wy): (f32, f32)) {
        let (Some(s), Some(kind)) = (item_ids::stats(id), magic::staff_bolt(id)) else { return };
        self.aim = (wx, wy);
        let f = magic::Features::of(kind);
        let (cx, cy) = self.player.center();
        let (dx, dy) = (wx - cx, wy - cy);
        let dir = if self.player.facing_left { -1.0 } else { 1.0 };
        let l = (dx * dx + dy * dy).sqrt();
        let speed = s.shoot_speed * if f.fast_then_slow { 5.0 } else { 1.0 };
        let vel = if l == 0.0 { [dir * speed, 0.0] } else { [dx * speed / l, dy * speed / l] };
        let mut at = [cx, cy];
        if f.spread {
            let a = (self.random() % 10_000) as f32 / 10_000.0 * std::f32::consts::TAU;
            at = [at[0] + a.cos() * 12.0, at[1] + a.sin() * 12.0];
        }
        let damage = (s.damage as f32 * f.damage_multiplier()) as i32;
        let twins: &[Option<f32>] = if f.twins { &[Some(0.0), Some(1.0)] } else { &[None] };
        for &twin in twins {
            self.bolts.push(Bolt::new(kind, at, vel, damage, s.knock_back, twin));
        }
    }

    /// `ItemCheck_UseManaCrystal`: a star more, shown in blue.
    fn use_crystal(&mut self) {
        if !self.mana.crystal() {
            return;
        }
        if let Some(s) = item_ids::stats(magic::MANA_CRYSTAL) {
            self.item_animation = s.use_animation as i32;
        }
        self.sfx.push(Sfx::Crystal);
        let b = self.player.body;
        self.text_at([b.x, b.y, b.w, b.h], combat_text::HEAL_MANA, 20, false);
        self.inventory.consume();
    }

    /// `WorldGen.UpdateWorld`'s starfall at night: now and then a spawner
    /// high in the sky (one in 15 near a player above ground), which drops a
    /// star three seconds later. Stars hit monsters for 1000 and leave a
    /// fallen star where they land; daybreak takes the ones still falling.
    fn update_stars(&mut self) {
        if self.time < DAYTIME {
            self.stars.clear();
            return;
        }
        let w = self.world.width as i64;
        let chance = 10.0 * w as f64 / 4200.0;
        if !self.client && ((self.random() % 8000) as f64) < chance {
            let mut x = ((self.random() % (w - 50).max(1) as u64) as i64 + 100) as f32 * 16.0;
            let y = (self.random() % ((self.world.height as f64 * 0.05) as u64).max(1)) as f32 * 16.0;
            let near = self.random().is_multiple_of(15) && self.player.y < self.env.world_surface as f32 * 16.0;
            if near {
                let r = 1 + self.random() % 639;
                x = self.player.x + (self.random() % (2 * r + 1)) as f32 - r as f32;
            }
            if !magic::solid_in(&self.world, [x, y], 16.0) {
                let vx = (self.random() % 201) as f32 - 100.0;
                let vy = (self.random() % 200) as f32 + 100.0;
                let k = 12.0 / (vx * vx + vy * vy).sqrt();
                let mut vel = [vx * k, vy * k];
                if near {
                    // Turned toward the player it came for.
                    vel[0] = (vel[0] * 0.35).abs() * if x + 8.0 < self.player.center().0 { 1.0 } else { -1.0 };
                }
                self.stars.push(Star::new([x, y], vel));
            }
        }
        let mut i = 0;
        while i < self.stars.len() {
            if self.stars[i].wait > 0 {
                self.stars[i].wait -= 1;
                i += 1;
                continue;
            }
            if self.stars[i].sound_delay == 0 {
                self.stars[i].sound_delay = 20 + (self.random() % 40) as i32;
                self.sfx.push(Sfx::StarFall);
            }
            self.stars[i].sound_delay -= 1;
            let flying = self.stars[i].advance(&self.world);
            let [x, y] = self.stars[i].pos;
            let size = magic::STAR_SIZE;
            let dir = if self.stars[i].vel[0] < 0.0 { -1.0 } else { 1.0 };
            let hit = self.mobs.iter().position(|m| {
                let b = &m.body;
                !m.kind.friendly()
                    && m.melee_cooldown == 0
                    && x < b.x + b.w
                    && b.x < x + size
                    && y < b.y + b.h
                    && b.y < y + size
            });
            if let Some(m) = hit {
                self.mobs[m].melee_cooldown = 10;
                let damage = self.damage_var(1000);
                self.hit_mob(m, damage, 10.0, dir, false, false);
            }
            if flying {
                i += 1;
                continue;
            }
            // `Kill`: a fallen star where it stopped.
            let star = self.stars.swap_remove(i);
            self.sfx.push(Sfx::StarHit);
            let [cx, cy] = star.center();
            self.toss(cx, cy, Item::Id(magic::FALLEN_STAR), 1);
        }
    }

    /// `Projectile.Update` for each bolt: dust, homing, moving, hitting, running out.
    fn update_bolts(&mut self) {
        let mut i = 0;
        while i < self.bolts.len() {
            if self.bolt_tick(i) {
                i += 1;
            } else {
                let b = self.bolts.swap_remove(i);
                self.bolt_killed(&b);
            }
        }
    }

    fn bolt_tick(&mut self, i: usize) -> bool {
        let (pos, vel, kind) = (self.bolts[i].pos, self.bolts[i].vel, self.bolts[i].dust());
        for _ in 0..2 {
            if self.random().is_multiple_of(5) {
                let mut d = self.new_dust(kind, pos, vel, 1.2);
                d.vel = [d.vel[0] * 0.3, d.vel[1] * 0.3];
                self.dust.push(d);
            }
        }
        if !self.bolts[i].sounded {
            self.bolts[i].sounded = true;
            self.sfx.push(Sfx::GemBolt);
        }
        let f = self.bolts[i].features();
        if f.homing {
            let size = [magic::BOLT_SIZE as i32; 2];
            let c = self.bolts[i].center();
            let target = self
                .mobs
                .iter()
                .filter(|m| !m.kind.friendly())
                .map(|m| (m.body.center(), m))
                .map(|((x, y), m)| (((x - c[0]).powi(2) + (y - c[1]).powi(2)).sqrt(), [x, y], m))
                .filter(|&(d, _, m)| {
                    let b = &m.body;
                    d < magic::HOMING_RANGE
                        && world::collision::can_hit(&self.world, pos, size, [b.x, b.y], [b.w as i32, b.h as i32])
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|t| t.1);
            if let Some(t) = target {
                self.bolts[i].home(t);
            }
        }
        let b = &mut self.bolts[i];
        b.swirl();
        b.age += 1;
        for im in &mut b.immune {
            im.1 -= 1;
        }
        b.immune.retain(|im| im.1 > 0);
        if !b.slow_down() || !b.advance(&self.world) {
            return false;
        }

        // `Projectile.Damage`: every monster under it not hit lately, while it can pierce.
        let b = &self.bolts[i];
        let c = b.center();
        let boxes = b.hitboxes();
        let mut hits: Vec<usize> = (0..self.mobs.len())
            .filter(|&m| {
                let mb = &self.mobs[m].body;
                !self.mobs[m].kind.friendly()
                    && !b.immune.iter().any(|im| im.0 == m)
                    && boxes
                        .iter()
                        .any(|&[x, y, w, h]| x < mb.x + mb.w && mb.x < x + w && y < mb.y + mb.h && mb.y < y + h)
                    && world::collision::can_hit(&self.world, c, [0, 0], [mb.center().0, mb.center().1], [0, 0])
            })
            .collect();
        hits.truncate(b.penetrate as usize);
        let (damage, kb, ap) = (b.damage, b.knock_back, b.armor_pen);
        let dir = if b.vel[0] > 0.0 { 1.0 } else { -1.0 };
        // Highest first, so a monster killed and swapped out leaves the rest in place.
        for &m in hits.iter().rev() {
            self.strike(m, damage, kb, dir, ap);
        }
        let b = &mut self.bolts[i];
        b.immune.extend(hits.iter().map(|&m| (m, magic::HIT_COOLDOWN)));
        b.penetrate -= hits.len() as i32;
        if b.penetrate <= 0 {
            return false;
        }
        b.time_left -= 1;
        b.time_left > 0
    }

    /// `Projectile.Kill` of a bolt: a puff of its dust, and for topaz a burst
    /// hitting everything around at 0.35 of the damage.
    fn bolt_killed(&mut self, b: &Bolt) {
        self.sfx.push(Sfx::Dig);
        let l = (b.vel[0] * b.vel[0] + b.vel[1] * b.vel[1]).sqrt();
        let n = if l < 1.0 { [0.0, 0.0] } else { [b.vel[0] / l, b.vel[1] / l] };
        for _ in 0..15 {
            let mut d = self.new_dust(b.dust(), b.pos, n, 1.2);
            let k = (self.random() % 1000) as f32 / 1000.0;
            d.scale *= 1.25;
            d.vel = [(d.vel[0] * 2.2 + n[0] * 1.5) * k, (d.vel[1] * 2.2 + n[1] * 1.5) * k];
            self.dust.push(d);
        }
        let f = b.features();
        if !f.burst {
            return;
        }
        for _ in 0..30 {
            let mut d = self.new_dust(b.dust(), b.pos, n, 1.2);
            d.gravity = self.random().is_multiple_of(5);
            let k = (self.random() % 1000) as f32 / 1000.0;
            d.scale *= 1.0 + (self.random() % 1000) as f32 / 1000.0;
            if d.gravity {
                d.scale *= 0.6;
            }
            d.vel = [d.vel[0] * 4.2 * k, d.vel[1] * 4.2 * k];
            self.dust.push(d);
        }
        let damage = (b.damage as f32 * (0.35 / f.damage_multiplier())) as i32;
        let kb = b.knock_back * 0.35;
        let c = b.center();
        let h = magic::BURST / 2.0;
        let hits: Vec<usize> = (0..self.mobs.len())
            .filter(|&m| {
                let mb = &self.mobs[m].body;
                // Only a monster the bolt itself just hit is spared.
                !self.mobs[m].kind.friendly()
                    && !b.immune.iter().any(|im| im.0 == m && im.1 == magic::HIT_COOLDOWN)
                    && c[0] - h < mb.x + mb.w
                    && mb.x < c[0] + h
                    && c[1] - h < mb.y + mb.h
                    && mb.y < c[1] + h
                    && world::collision::can_hit(&self.world, c, [0, 0], [mb.center().0, mb.center().1], [0, 0])
            })
            .collect();
        for &m in hits.iter().rev() {
            let dir = if self.mobs[m].body.center().0 > c[0] { 1.0 } else { -1.0 };
            self.strike(m, damage, kb, dir, b.armor_pen);
        }
    }

    /// Gem dust in a bolt's box.
    fn new_dust(&mut self, kind: u16, pos: [f32; 2], speed: [f32; 2], scale: f32) -> Dust {
        let size = magic::BOLT_SIZE as i32;
        Dust::new(kind, pos, size, size, speed, scale, |n| (self.random() % n.max(1) as u64) as i32)
    }

    /// `IsInTileInteractionRange`: a box of tiles around the player's.
    fn in_tool_range(&self, x: i64, y: i64, boost: i32) -> bool {
        let (rx, ry) = ((player::TILE_RANGE_X + boost) as i64, (player::TILE_RANGE_Y + boost) as i64);
        let p = &self.player;
        let lx = (p.x / 16.0) as i64 - rx;
        let hx = ((p.x + player::WIDTH) / 16.0).ceil() as i64 - 1 + rx;
        let ly = (p.y / 16.0) as i64 - ry;
        let hy = ((p.y + player::HEIGHT) / 16.0).ceil() as i64 - 1 + ry;
        self.world.in_bounds(x, y) && (lx..=hx).contains(&x) && (ly..=hy).contains(&y)
    }

    /// `ItemCheck_UseMiningTools`: pickaxes on blocks and objects, axes on
    /// trees, hammers on walls with nothing in front.
    fn hit_with(&mut self, s: ToolStats, x: i64, y: i64) {
        use world::tile_sets::{AXE, HAMMER, NO_FAIL};
        let t = self.world.tile(x, y);
        let mut walls = true;
        if t != NO_TILE {
            let (axe_tile, hammer_tile) = (AXE.has(t), HAMMER.has(t));
            if (s.pick > 0 && !axe_tile && !hammer_tile) || (s.axe > 0 && axe_tile) || (s.hammer > 0 && hammer_tile) {
                walls = false;
            }
            let base = if NO_FAIL.has(t) { 100 } else { 0 };
            if hammer_tile {
                walls = false;
                if s.hammer > 0 {
                    // Altars need a hammer of 80 in hardmode.
                    self.damage_tile(x, y, if t == 26 { 0 } else { base + s.hammer });
                    self.item_time = s.use_time;
                }
            } else if axe_tile {
                if s.axe > 0 {
                    let axe = if t == 80 { s.axe * 3 } else { s.axe };
                    self.damage_tile(x, y, base + (axe as f32 * 1.2) as i32);
                    self.item_time = s.use_time;
                }
            } else if s.pick > 0 {
                let at = Place {
                    x,
                    y,
                    world_width: self.world.width,
                    world_surface: self.env.world_surface,
                    underworld: self.world.height as i64 - 200,
                };
                let mut damage = mining::pick_damage(t, self.world.frame(x, y).1, s.pick, &at);
                if mining::transforms(t) && self.hits.would_have(x, y, Target::Tile, damage) >= 100 {
                    damage = 0;
                }
                self.damage_tile(x, y, damage);
            }
            if s.pick > 0 {
                self.item_time = s.use_time;
            }
        }
        if walls && t == NO_TILE && s.hammer > 0 && self.world.wall(x, y) != 0 && self.can_smash_wall(x, y) {
            let total = self.hits.add(x, y, Target::Wall, (s.hammer as f32 * 1.5) as i32);
            if total >= 100 {
                self.hits.clear(x, y, Target::Wall);
                self.world.set_wall(x as u32, y as u32, 0);
                self.send_tile(x as u32, y as u32);
            }
            self.sfx.push(Sfx::Dig);
            self.prune_hits();
            self.item_time = s.use_time / 2;
        }
    }

    /// `CanPlayerSmashWall`: a house wall, or a natural one at an edge.
    fn can_smash_wall(&self, x: i64, y: i64) -> bool {
        use world::tile_sets::WALL_HOUSE;
        let w = &self.world;
        WALL_HOUSE.has(w.wall(x, y))
            || (x - 1..x + 2).any(|i| (y - 1..y + 2).any(|j| w.wall(i, j) == 0 || WALL_HOUSE.has(w.wall(i, j))))
    }

    /// Adds damage to a tile: 100 breaks it, less leaves grass and moss bare.
    fn damage_tile(&mut self, x: i64, y: i64, damage: i32) {
        let t = self.world.tile(x, y);
        if self.hits.add(x, y, Target::Tile, damage) >= 100 {
            self.hits.clear(x, y, Target::Tile);
            self.break_tile(x, y, true);
        } else {
            let bare = mining::after_failed_hit(t);
            if bare != t {
                self.world.set_tile(x as u32, y as u32, bare, -1, -1);
                self.send_tile(x as u32, y as u32);
            }
            self.sfx.push(if items::HARD_SOUND.contains(&t) { Sfx::Tink } else { Sfx::Dig });
        }
        if damage != 0 {
            self.prune_hits();
        }
    }

    fn prune_hits(&mut self) {
        let w = &self.world;
        self.hits.prune(|h| match h.target {
            Target::Tile => w.tile(h.x, h.y) == NO_TILE,
            Target::Wall => w.wall(h.x, h.y) == 0,
        });
    }

    fn send_tile(&mut self, x: u32, y: u32) {
        self.outbox.push(Msg::Tile { x, y, cell: self.world.cell_data(x, y) });
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
        } else if t == POT {
            let cells = self.object_cells(tx, ty);
            let (x0, y0) = (cells.iter().map(|c| c.0).min().unwrap(), cells.iter().map(|c| c.1).min().unwrap());
            if drop {
                self.shatter_pot(x0, y0, ty);
            }
            (cells, None)
        } else if self.important(t) {
            let cells = self.object_cells(tx, ty);
            let item = (!PLANTS.contains(&t)).then(|| (Item::Object(self.template(t, &cells)), 1));
            (cells, item)
        } else if magic::GEM_STONES.contains(&t) {
            (vec![(tx, ty)], Some((Item::Id(magic::gem(t)), 1)))
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
                grab_delay: 0.0,
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
            self.drops.push(Drop { x, y, vx: 0.0, vy: 0.0, item, count: rest, grab_delay: 0.0 });
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
            Item::Material(Material::SuspiciousEye) => {
                self.summon_boss();
                return;
            }
            Item::Tool(_) | Item::Material(_) | Item::Id(_) => return,
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

    /// `DropSelectedItem`: throws the held stack (the carried one first) ahead of
    /// the player. `animate`: the throw key, which holds the hand for 10 ticks.
    pub fn throw_held(&mut self, animate: bool) {
        if self.item_animation > 0 && animate {
            return;
        }
        let Some(stack) = self.inventory.take_held() else { return };
        let (cx, cy) = self.player.center();
        let dir = if self.player.facing_left { -1.0 } else { 1.0 };
        self.drops.push(Drop {
            x: cx - DROP_SIZE / 2.0,
            y: cy - DROP_SIZE / 2.0,
            // The game's (4 * direction + velocity.X, -2) in pixels per tick.
            vx: (4.0 * dir + self.player.vx) * 60.0,
            vy: -2.0 * 60.0,
            item: stack.item,
            count: stack.count,
            grab_delay: GRAB_DELAY,
        });
        if animate {
            self.item_animation = 10;
        }
    }

    /// A swing or use is under way (the inventory ignores clicks then).
    pub fn using_item(&self) -> bool {
        self.item_animation != 0 || self.item_time != 0
    }

    /// The game's item id of `item`, for its name.
    pub fn item_id(&self, item: Item) -> Option<u16> {
        let placing = |tile: u16| {
            let i = item_ids::TILE_ITEM.binary_search_by_key(&tile, |e| e.0).ok()?;
            Some(item_ids::TILE_ITEM[i].1)
        };
        let sheet = |name: &str| name.strip_prefix("Item_")?.parse().ok();
        match item {
            Item::Block(t) => placing(t),
            Item::Object(id) => placing(self.templates[id as usize].tile),
            Item::Tool(t) => sheet(t.sheet()),
            Item::Material(m) => sheet(m.sheet()),
            Item::Bucket(None) => Some(205),
            Item::Bucket(Some(Liquid::Water)) => Some(206),
            Item::Bucket(Some(Liquid::Lava)) => Some(207),
            Item::Bucket(Some(Liquid::Honey)) => Some(1128),
            Item::Bucket(Some(Liquid::Shimmer)) => None,
            Item::Id(id) => Some(id),
        }
    }

    /// `item`'s `ItemID` name and the defaults its tooltip shows.
    pub fn item_data(&self, item: Item) -> Option<(&'static str, item_ids::Stats)> {
        let id = self.item_id(item)?;
        let i = item_ids::ITEMS.binary_search_by_key(&id, |e| e.0).ok()?;
        Some((item_ids::ITEMS[i].1, item_ids::ITEMS[i].2))
    }

    /// `item`'s name in the game's language (`Lang.GetItemName`).
    pub fn item_name(&self, item: Item) -> Option<&str> {
        self.texts.get(&format!("ItemName.{}", self.item_data(item)?.0))
    }

    /// The larger side of `item`'s picture, in its own pixels.
    pub fn item_size(&self, item: Item) -> f32 {
        let sheet = |name: &str| self.sheet(name).map_or(16.0, |s| s.w.max(s.h) as f32);
        match item {
            Item::Block(_) => 16.0,
            Item::Tool(t) => sheet(t.sheet()),
            Item::Material(m) => sheet(m.sheet()),
            Item::Bucket(_) => sheet("Item_205"),
            Item::Id(id) => sheet(&format!("Item_{id}")),
            Item::Object(id) => {
                let cells = &self.templates[id as usize].cells;
                let w = cells.iter().map(|c| c.0).max().unwrap_or(0) + 1;
                let h = 1 - cells.iter().map(|c| c.1).min().unwrap_or(0);
                (w.max(h) * 16) as f32
            }
        }
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
            Item::Id(id) => match self.sheet(&format!("Item_{id}")) {
                Some(s) => fit(s, [0, 0, s.w, s.h]),
                None => flat([200, 200, 200, 255]),
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
        self.tree_tops(view, &mut under);

        let mut over = Vec::new();
        self.mob_sprites(&mut over);
        for (_, p) in &self.others {
            self.player_sprites(p, [0; 3], false, 2, &mut over);
        }
        if !free {
            // `holdStyle` 1 raises the arm: body frame 3, front arm frame 5.
            let torch = self.inventory.selected().and_then(|s| self.torch(s.item)).is_some();
            let arm = if torch && self.player.wet.is_none() && self.item_animation == 0 { 5 } else { 2 };
            self.player_sprites(&self.player, self.worn_pictures(), self.immune > 0, arm, &mut over);
            self.held_item_sprite(&mut over);
        }
        for d in &self.drops {
            over.extend(self.item_icon(d.item, [d.x - 2.0, d.y - 4.0], DROP_SIZE + 4.0, SPRITE_LIT));
        }
        self.magic_sprites(&mut over);
        self.crack_sprites(&mut over);
        let tool = self.held_tool();
        let reach = |x, y| {
            if free || tool.use_animation == 0 {
                self.in_reach(x, y, free)
            } else {
                self.in_tool_range(x, y, tool.tile_boost)
            }
        };
        if let Some((tx, ty)) = cursor.filter(|&(x, y)| reach(x, y)) {
            over.push(Sprite::flat([tx as f32 * 16.0, ty as f32 * 16.0], [16.0, 16.0], [0.25, 0.25, 0.25, 0.25]));
        }
        (under, over)
    }

    /// `DrawTileCracks`: four stages of cracks from 20 damage, on full blocks and tree trunks.
    fn crack_sprites(&self, out: &mut Vec<Sprite>) {
        use world::tile_sets::{SOLID, SOLID_TOP};
        let Some(sheet) = self.sheet("TileCracks") else { return };
        for h in self.hits.iter().filter(|h| h.damage >= 20) {
            let t = self.world.tile(h.x, h.y);
            let t = match h.target {
                Target::Tile if t == NO_TILE => continue,
                Target::Wall if self.world.wall(h.x, h.y) == 0 => continue,
                // Behind an empty cell the game reads type 0, a full block.
                Target::Wall if t == NO_TILE => 0,
                _ => t,
            };
            let (fx, fy) = self.world.frame(h.x, h.y);
            let trunk = t == TREE || t == 72;
            let shown = if t == 10 {
                false
            } else if SOLID.has(t) && !SOLID_TOP.has(t) {
                true
            } else if t == TREE {
                let (a, b) = (fx / 22, fy / 22);
                b < 9
                    && (!(a == 1 || a == 2) || !(6..=8).contains(&b))
                    && (a != 3 || b > 2)
                    && (a != 4 || !(3..=5).contains(&b))
                    && (a != 5 || !(6..=8).contains(&b))
            } else {
                t == 72 && fx <= 34
            };
            let shape = (self.world.flags(h.x, h.y) & world::flags::SHAPE_MASK) >> world::flags::SHAPE_SHIFT;
            if !shown || (h.target == Target::Tile && shape != 0) {
                continue;
            }
            let row = match h.damage {
                80.. => 3,
                60.. => 2,
                40.. => 1,
                _ => 0,
            };
            let col = if trunk { 4 + h.crack as u32 / 2 } else { h.crack as u32 };
            let pos = [h.x as f32 * 16.0, h.y as f32 * 16.0];
            out.push(Sprite::sheet(pos, sheet, [col * 18, row * 18, 16, 16], SPRITE_LIT));
        }
    }

    /// Leafy tops and branches, drawn from the tree's top and branch cells.
    fn tree_tops(&self, view: Rect, out: &mut Vec<Sprite>) {
        // Tops reach 9 tiles over their cell, branches 2 to the side: one chunk around covers them.
        let (cx0, cy0) = ((view.x0 / CHUNK).saturating_sub(1), (view.y0 / CHUNK).saturating_sub(1));
        let cx1 = (view.x1.div_ceil(CHUNK) + 1).min(self.world.chunks_x);
        let cy1 = (view.y1.div_ceil(CHUNK) + 1).min(self.world.chunks_y);
        let cells = (cy0..cy1)
            .flat_map(|cy| (cx0..cx1).map(move |cx| (cy * self.world.chunks_x + cx) as usize))
            .filter_map(|ci| self.tree_cells[ci].as_ref())
            .flatten();
        for &(x, y) in cells {
            let (xi, yi) = (x as i64, y as i64);
            let (fx, fy) = self.world.frame(xi, yi);
            let frame = match fy {
                220 => 1,
                242 => 2,
                _ => 0,
            };
            let dx = match fx {
                22 => 0,
                44 => 1,
                66 => -1,
                _ => continue,
            };
            let Some((style, w, h, frame)) = world::draw::tree_foliage(&self.world, xi, yi, dx, frame) else {
                continue;
            };
            let (px, py) = (x as f32 * 16.0, y as f32 * 16.0);
            let frame = frame as u32;
            // Tops and branches lean about their foot, except in front of a wall.
            let k = if self.world.wall(xi, yi) == 0 && (yi as f64) < self.env.world_surface {
                world::draw::wind_cycle(xi, yi, self.wind.tree, self.env.wind)
            } else {
                0.0
            };
            let (sheet, pivot, origin, src, rot) = match fx {
                22 => (
                    "Tree_Tops",
                    [px + 8.0 + 2.0 * k, py + 16.0 + 2.0 * k.abs()],
                    [(w / 2) as f32, h as f32],
                    [frame * (w + 2), 0, w, h],
                    0.08,
                ),
                44 => (
                    "Tree_Branches",
                    [px + 16.0 + k.max(0.0) + 2.0 * k.abs(), py + 12.0],
                    [40.0, 24.0],
                    [0, frame * 42, 40, 40],
                    0.06,
                ),
                _ => (
                    "Tree_Branches",
                    [px + k.min(0.0) - 2.0 * k.abs(), py + 18.0],
                    [0.0, 30.0],
                    [42, frame * 42, 40, 40],
                    0.06,
                ),
            };
            let Some(sheet) = self.sheet(&format!("{sheet}_{style}")) else { continue };
            if src[0] + src[2] <= sheet.w && src[1] + src[3] <= sheet.h {
                let pos = [pivot[0] - origin[0], pivot[1] - origin[1]];
                out.push(Sprite { rot: k * rot, pivot: origin, ..Sprite::sheet(pos, sheet, src, SPRITE_LIT) });
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
                Kind::DemonEye => ("NPC_2", 38, 24, (m.timer * 6.0) as u32 % 2, [1.0; 4]),
                Kind::Guide => {
                    let frame = if !b.on_ground {
                        1
                    } else if b.vx != 0.0 {
                        2 + (m.timer * 10.0) as u32 % 14
                    } else {
                        0
                    };
                    ("NPC_22", 40, 56, frame, [1.0; 4])
                }
                Kind::EyeBoss => {
                    let frame = (m.timer * 8.0) as u32 % 3 + if m.enraged() { 3 } else { 0 };
                    ("NPC_4", 110, 166, frame, [1.0; 4])
                }
            };
            let color = if hurt { [1.0, 0.35, 0.35, 1.0] } else { color };
            match self.sheet(sheet) {
                Some(s) => {
                    // Fliers are drawn around their centre, walkers on their feet.
                    let top = if m.kind.flies() { b.y + b.h / 2.0 - h as f32 / 2.0 } else { bottom - h as f32 + 2.0 };
                    let mut sp = Sprite::sheet([cx - w as f32 / 2.0, top], s, [0, frame * h, w, h], flags);
                    sp.color = color;
                    out.push(sp);
                }
                None => out.push(Sprite { flags: SPRITE_LIT, ..Sprite::flat([b.x, b.y], [b.w, b.h], color) }),
            }
        }
    }

    /// Every `Lighting.AddLight` of this frame.
    fn glows(&self) -> Vec<Glow> {
        let mut v = Vec::new();
        if let Some((style, rgb)) = self.inventory.selected().and_then(|s| self.torch(s.item)) {
            // `ItemCheck_EmitHeldItemLight`: by the flame, a tick ahead.
            if self.player.wet.is_none() {
                let ([x, y], dir) = self.torch_hold(style);
                v.extend(glow::at([x + 12.0 * dir + self.player.vx, y - 14.0 + self.player.vy], rgb));
            }
        }
        v.extend(self.dust.iter().filter_map(glow::dust));
        v.extend(self.bolts.iter().filter_map(glow::bolt));
        v.extend(self.stars.iter().filter_map(glow::star));
        for d in &self.drops {
            let center = [d.x + DROP_SIZE / 2.0, d.y + DROP_SIZE / 2.0];
            let ess = self.ess.scale * 0.5;
            v.extend(match d.item {
                Item::Id(magic::FALLEN_STAR) => glow::at([d.x + DROP_SIZE, center[1]], [0.8, 0.7, 0.1]),
                Item::Id(pots::HEART) => glow::at(center, [0.5, 0.1, 0.1].map(|c| c * ess)),
                Item::Id(magic::MANA_STAR) => glow::at(center, [0.1, 0.1, 0.5].map(|c| c * ess)),
                item => self
                    .torch(item)
                    .filter(|_| world::collision::wet(&self.world, [d.x, d.y], [DROP_SIZE as i32; 2]).is_none())
                    .and_then(|(_, rgb)| glow::at(center, rgb)),
            });
        }
        v
    }

    /// The flames of `ItemCheck_EmitHeldItemLight`: now and then a speck off the
    /// held torch, more often while it swings.
    fn torch_flame(&mut self) {
        let Some((style, _)) = self.inventory.selected().and_then(|s| self.torch(s.item)) else { return };
        let max = if self.item_animation > 0 { 7 } else { 30 };
        if self.player.wet.is_some() || !self.random().is_multiple_of(max) {
            return;
        }
        let ([x, y], dir) = self.torch_hold(style);
        let at = [x + if dir < 0.0 { -16.0 } else { 6.0 }, y - 14.0];
        let kind = glow::TORCH_DUST[style.min(glow::TORCH_DUST.len() - 1)];
        let mut d = Dust::new(kind, at, 4, 4, [0.0; 2], 1.0, |n| (self.random() % n.max(1) as u64) as i32);
        d.gravity = self.random().is_multiple_of(3);
        d.vel = [d.vel[0] * 0.3, d.vel[1] * 0.3 - 1.5];
        self.dust.push(d);
    }

    /// Style and light of a torch item.
    fn torch(&self, item: Item) -> Option<(usize, [f32; 3])> {
        let Item::Object(id) = item else { return None };
        let t = &self.templates[id as usize];
        let fy = t.cells.first().filter(|_| t.tile == 4)?.3;
        Some(((fy / 22) as usize, light::emission(4, 0, fy)?))
    }

    /// `ItemCheck_ApplyHoldStyle` 1: where a held torch's bottom corner is,
    /// beside the hand, and the way the player faces.
    fn torch_hold(&self, style: usize) -> ([f32; 2], f32) {
        let w =
            glow::TORCH_ITEMS.get(style).and_then(|id| self.sheet(&format!("Item_{id}"))).map_or(16.0, |s| s.w as f32);
        let dir = if self.player.facing_left { -1.0 } else { 1.0 };
        ([self.player.x + player::WIDTH * 0.5 + (w * 0.5 + 2.0) * dir, self.player.y + 24.0], dir)
    }

    /// A torch held up beside the player (`holdStyle` 1), gone under water.
    fn held_torch_sprite(&self, style: usize, out: &mut Vec<Sprite>) {
        let Some(sheet) = glow::TORCH_ITEMS.get(style).and_then(|id| self.sheet(&format!("Item_{id}"))) else {
            return;
        };
        if self.player.wet.is_some() {
            return;
        }
        let ([x, y], _) = self.torch_hold(style);
        let [dx, dy] = self.player.drawn(self.tick_acc);
        let (x, y) = ((x + dx - self.player.x).floor(), (y + dy - self.player.y + self.player.gfx_off_y).floor());
        let left = self.player.facing_left;
        let (w, h) = (sheet.w as f32, sheet.h as f32);
        let grip = [if left { w } else { 0.0 }, h];
        let flags = SPRITE_LIT | if left { SPRITE_FLIP } else { 0 };
        out.push(Sprite::sheet([x - grip[0], y - grip[1]], sheet, [0, 0, sheet.w, sheet.h], flags));
    }

    /// The item in the player's hand during a swing, turning about its grip
    /// (`useStyle` 1 in `DrawPlayer`).
    fn held_item_sprite(&self, out: &mut Vec<Sprite>) {
        if let Some((style, _)) = self.inventory.selected().and_then(|s| self.torch(s.item)) {
            self.held_torch_sprite(style, out);
            return;
        }
        if let Some(Item::Id(id)) = self.inventory.selected().map(|s| s.item).filter(|_| self.item_animation > 0) {
            if magic::staff_bolt(id).is_some() {
                self.held_staff_sprite(id, out);
                return;
            }
        }
        let Some(Item::Tool(t)) = self.inventory.selected().map(|s| s.item) else { return };
        let s = t.stats();
        let Some(sheet) = self.sheet(t.sheet()).filter(|_| self.item_animation > 0) else { return };
        let ([x, y], rot) = self.swing_pose(s);
        // Drawn where the player is drawn, between ticks and after a step.
        let [dx, dy] = self.player.drawn(self.tick_acc);
        let (x, y) = (x + dx - self.player.x, y + dy - self.player.y + self.player.gfx_off_y);
        let (w, h) = (sheet.w as f32 * s.scale, sheet.h as f32 * s.scale);
        let left = self.player.facing_left;
        let grip = [if left { w } else { 0.0 }, h];
        let flags = SPRITE_LIT | if left { SPRITE_FLIP } else { 0 };
        let mut sprite = Sprite::sheet([x - grip[0], y - grip[1]], sheet, [0, 0, sheet.w, sheet.h], flags);
        sprite.size = [w, h];
        (sprite.rot, sprite.pivot) = (rot, grip);
        out.push(sprite);
    }

    /// A staff held out toward the cursor (`useStyle` 5 with `Item.staff`):
    /// its butt at `itemLocation`, turned 45 degrees past `itemRotation`.
    fn held_staff_sprite(&self, id: u16, out: &mut Vec<Sprite>) {
        let Some(sheet) = self.sheet(&format!("Item_{id}")) else { return };
        let left = self.player.facing_left;
        let dir = if left { -1.0 } else { 1.0 };
        let [dx, dy] = self.player.drawn(self.tick_acc);
        let (cx, cy) = (dx + player::WIDTH / 2.0, dy + player::HEIGHT / 2.0 + self.player.gfx_off_y);
        let r = self.item_rotation;
        let (x, y) = ((cx + (r.cos() * 6.0 * dir).floor()).floor(), (cy + (r.sin() * 6.0 * dir).floor()).floor());
        let (w, h) = (sheet.w as f32, sheet.h as f32);
        let grip = [if left { w } else { 0.0 }, h];
        let flags = SPRITE_LIT | if left { SPRITE_FLIP } else { 0 };
        let mut sprite = Sprite::sheet([x - grip[0], y - grip[1]], sheet, [0, 0, sheet.w, sheet.h], flags);
        (sprite.rot, sprite.pivot) = (r + 0.785 * dir, grip);
        out.push(sprite);
    }

    /// Bolts and dust, added to what is under them (`Color` with no alpha).
    /// The bolt is its picture glowing over a trail, which the game draws
    /// with a shader and here is plain strips fading from white to violet.
    fn magic_sprites(&self, out: &mut Vec<Sprite>) {
        for b in &self.bolts {
            // `DrawProj_GemStaffShot`: head, tail and the white the head starts from.
            let rgb = |c: [f32; 3]| c.map(|c| c / 255.0);
            let white = [1.0; 3];
            let (head, tail, start) = match b.kind {
                122 => (rgb([255.0, 165.0, 0.0]), rgb([255.0, 165.0, 0.0]), white),
                // Sapphire: blue throughout, at half alpha, which `RainbowRod` adds up the same.
                123 => (rgb([60.0, 90.0, 255.0]), rgb([60.0, 90.0, 255.0]), rgb([60.0, 90.0, 255.0])),
                124 => (rgb([50.0, 205.0, 50.0]), rgb([0.0, 128.0, 0.0]), white),
                125 => (rgb([255.0, 0.0, 0.0]), rgb([255.0, 0.0, 0.0]), white),
                126 => (rgb([192.0, 192.0, 192.0]), rgb([192.0, 192.0, 192.0]), white),
                _ => (rgb([238.0, 130.0, 238.0]), rgb([128.0, 0.0, 128.0]), white),
            };
            let n = b.trail.len();
            let h = magic::BOLT_SIZE / 2.0;
            for (k, w) in b.trail.windows(2).enumerate() {
                let (a, z) = ([w[0].0[0] + h, w[0].0[1] + h], [w[1].0[0] + h, w[1].0[1] + h]);
                let p = k as f32 / (n - 1).max(1) as f32;
                let toward = ((p + 0.2) / 0.7).clamp(0.0, 1.0);
                let fade = (1.0 - p / 0.98).max(0.0) * 0.5;
                let c: Vec<f32> = (0..3)
                    .map(|i| {
                        let mid = head[i] + (tail[i] - head[i]) * p;
                        (start[i] + (mid - start[i]) * toward) * fade
                    })
                    .collect();
                let (vx, vy) = (z[0] - a[0], z[1] - a[1]);
                let len = (vx * vx + vy * vy).sqrt();
                let mut s = Sprite::flat([a[0], a[1] - 8.0], [len, 16.0], [c[0], c[1], c[2], 0.0]);
                (s.rot, s.pivot) = (vy.atan2(vx), [0.0, 8.0]);
                out.push(s);
            }
            if let Some(sheet) = self.sheet(&format!("Projectile_{}", b.kind)) {
                let t = b.age as f32 / 60.0;
                let k = 0.6 * (1.0 + (t * std::f32::consts::TAU).cos() * 0.2) * b.scale;
                let size = [sheet.w as f32 * 1.25 * k, sheet.h as f32 * k];
                let [cx, cy] = b.center();
                let mut s = Sprite::sheet([cx - size[0] / 2.0, cy - size[1] / 2.0], sheet, [0, 0, sheet.w, sheet.h], 0);
                (s.size, s.pivot, s.rot) = (size, [size[0] / 2.0, size[1] / 2.0], b.rotation());
                s.color = [80.0 / 255.0, 80.0 / 255.0, 80.0 / 255.0, 0.0];
                // Four copies on top of each other.
                out.extend([s; 4]);
                if b.features().bigger {
                    // A pulsing cross twice as long each way, at half the glow.
                    let q = 1.0 + (t * std::f32::consts::TAU * 4.0).cos() * 0.2;
                    for scale in [[1.0, 2.0], [2.0, 1.0]] {
                        let size = [sheet.w as f32 * scale[0] * q, sheet.h as f32 * scale[1] * q];
                        let mut x = s;
                        x.pos = [cx - size[0] / 2.0, cy - size[1] / 2.0];
                        (x.size, x.pivot) = (size, [size[0] / 2.0, size[1] / 2.0]);
                        x.rot = b.rotation() + std::f32::consts::FRAC_PI_4;
                        x.color = [40.0 / 255.0, 40.0 / 255.0, 40.0 / 255.0, 0.0];
                        out.push(x);
                    }
                }
            }
        }
        self.star_sprites(out);
        let Some(sheet) = self.sheet("Dust") else { return };
        for d in &self.dust {
            let half = 4.0 * d.scale;
            let mut s = Sprite::sheet([d.pos[0] - half, d.pos[1] - half], sheet, d.frame(), 0);
            (s.size, s.pivot, s.rot) = ([8.0 * d.scale; 2], [half, half], d.rot);
            s.color = [1.0, 1.0, 1.0, 0.0];
            out.push(s);
        }
    }

    /// Falling stars as `DrawProj` draws projectile 12: three faint blue
    /// streaks circling ahead of it, a white pulse behind, the spinning star.
    fn star_sprites(&self, out: &mut Vec<Sprite>) {
        use std::f32::consts::{PI, TAU};
        let (Some(glow), Some(star)) = (self.sheet("Extra_91"), self.sheet("Projectile_12")) else { return };
        let (gw, gh) = (glow.w as f32, glow.h as f32);
        let streak = |at: [f32; 2], rot: f32, scale: f32, color: [f32; 4]| {
            let pivot = [gw / 2.0 * scale, 10.0 * scale];
            let mut s = Sprite::sheet([at[0] - pivot[0], at[1] - pivot[1]], glow, [0, 0, glow.w, glow.h], 0);
            (s.size, s.pivot, s.rot, s.color) = ([gw * scale, gh * scale], pivot, rot, color);
            s
        };
        for st in self.stars.iter().filter(|s| s.wait == 0) {
            let t = st.age as f32 / 60.0;
            let [cx, cy] = st.center();
            let rot = st.vel[1].atan2(st.vel[0]) + PI / 2.0;
            let ahead = [cx + st.vel[0], cy + st.vel[1]];
            let blue = [0.0, 0.0, 0.2, 0.0];
            for (k, scale) in [1.5, 1.1, 1.3].into_iter().enumerate() {
                let a = TAU * t + k as f32 * TAU / 3.0;
                // (0, -10) turned by `a`.
                let at = [ahead[0] + 10.0 * a.sin(), ahead[1] - 10.0 * a.cos()];
                out.push(streak(at, rot, scale, blue));
            }
            let behind = [cx - st.vel[0] * 0.5, cy - st.vel[1] * 0.5];
            for k in [0.0, 0.5] {
                let p = ((t % 0.5) / 0.5 + k) % 1.0;
                let pulse = if p * 2.0 > 1.0 { 2.0 - p * 2.0 } else { p * 2.0 };
                out.push(streak(behind, rot, 0.3 + p * 0.5, [0.5 * pulse, 0.5 * pulse, 0.5 * pulse, 0.0]));
            }
            let (w, h) = (star.w as f32 * 1.1, star.h as f32 * 1.1);
            let mut s = Sprite::sheet([cx - w / 2.0, cy - h / 2.0], star, [0, 0, star.w, star.h], 0);
            (s.size, s.pivot, s.rot) = ([w, h], [w / 2.0, h / 2.0], st.rot);
            s.color = [0.8; 4];
            out.push(s);
        }
    }

    /// Layered body sheets, tinted, with the walk cycle on the legs.
    /// `arm`: column of the front arm frame.
    /// `PlayerDrawLayers` for the composite body: skin and clothes, with the worn
    /// armour (`head`, `body`, `legs` picture numbers, 0 for none) in place of
    /// the hair, the shirt and the pants and shoes.
    fn player_sprites(&self, p: &Player, worn: [i16; 3], dim: bool, arm: u32, out: &mut Vec<Sprite>) {
        let [x, y] = p.drawn(self.tick_acc);
        let (x, y) = (x, y + p.gfx_off_y);
        let pos = [x + player::WIDTH / 2.0 - 20.0, y + player::HEIGHT - 54.0];
        let legs = p.leg_frame;
        let flip = if p.facing_left { SPRITE_FLIP } else { 0 } | SPRITE_LIT;
        let [head, body, leg] = worn;
        let mut any = false;
        let mut draw = |name: &str, col: u32, row: u32, color: [f32; 4], out: &mut Vec<Sprite>| {
            let Some(sheet) = self.sheet(name) else { return };
            if (col + 1) * 40 > sheet.w || (row + 1) * 56 > sheet.h {
                return;
            }
            let mut s = Sprite::sheet(pos, sheet, [col * 40, row * 56, 40, 56], flip);
            s.color = if dim { color.map(|c| c * 0.5) } else { color };
            out.push(s);
            any = true;
        };
        let body_sheet = format!("Armor/Armor_{body}");
        if body > 0 {
            // `DrawPlayer_12_SkinComposite_BackArmShirt`: the back arm and shoulder.
            draw(&body_sheet, arm, 2, [1.0; 4], out);
            draw(&body_sheet, 1, 1, [1.0; 4], out);
        }
        for &(name, col, row, color) in &PLAYER_LAYERS {
            let hidden = match name {
                "Player_0_11" | "Player_0_12" => leg > 0,
                "Player_0_6" | "Player_0_7" => body > 0,
                "Player_Hair_1" => head > 0,
                _ => false,
            };
            if !hidden {
                let row = if row == u32::MAX { legs } else { row };
                let col = if col == 2 { arm } else { col };
                draw(name, col, row, color, out);
            }
            match name {
                "Player_0_12" if leg > 0 => draw(&format!("Armor_Legs_{leg}"), 0, legs, [1.0; 4], out),
                "Player_0_6" if body > 0 => draw(&body_sheet, 0, 0, [1.0; 4], out),
                "Player_Hair_1" if head > 0 => draw(&format!("Armor_Head_{head}"), 0, 0, [1.0; 4], out),
                _ => {}
            }
        }
        if body > 0 {
            // `DrawPlayer_28_ArmOverItemComposite`: the front shoulder and arm.
            draw(&body_sheet, 0, 1, [1.0; 4], out);
            draw(&body_sheet, arm, 0, [1.0; 4], out);
        }
        if !any {
            out.push(Sprite::flat([x, y], [player::WIDTH, player::HEIGHT], [0.85, 0.65, 0.45, 1.0]));
        }
    }

    /// Rebuilds what edits touched; relights and redraws the map within budgets.
    /// `view`: visible tile rect.
    pub fn sync(&mut self, renderer: &mut Renderer, view: Rect) {
        let cxs = self.world.chunks_x;
        for (cx, cy, bits) in self.world.take_dirty() {
            let ci = (cy * cxs + cx) as usize;
            self.tree_cells[ci] = None;
            if self.stale[ci] & STALE_LOD == 0 {
                self.lod_queue.push(ci as u32);
            }
            self.stale[ci] |= STALE_LOD | if bits & world::DIRTY_TILES != 0 { STALE_LIGHT } else { STALE_LIQUID_LIGHT };
            if self.mesh_pending[ci] == 0 {
                self.mesh_queue.push_back(ci as u32);
            }
            self.mesh_pending[ci] |= bits;
        }

        // Visible chunks plus one around.
        let (cx0, cy0) = ((view.x0 / CHUNK).saturating_sub(1), (view.y0 / CHUNK).saturating_sub(1));
        let cx1 = (view.x1.div_ceil(CHUNK) + 1).min(self.world.chunks_x);
        let cy1 = (view.y1.div_ceil(CHUNK) + 1).min(self.world.chunks_y);

        for cy in cy0..cy1 {
            for cx in cx0..cx1 {
                let ci = (cy * cxs + cx) as usize;
                if self.tree_cells[ci].is_none() {
                    self.tree_cells[ci] = Some(tree_cells(&self.world, cx, cy));
                }
            }
        }

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

        // Moving lights: relight around where they were and where they are.
        let glows = self.glows();
        if glows != self.glows {
            let (cw, ch) = (self.world.chunks_x, self.world.chunks_y);
            for &(x, y, _) in self.glows.iter().chain(&glows) {
                let (x0, y0) = (x.saturating_sub(light::REACH) / CHUNK, y.saturating_sub(light::REACH) / CHUNK);
                let (x1, y1) = (((x + light::REACH) / CHUNK + 1).min(cw), ((y + light::REACH) / CHUNK + 1).min(ch));
                for cy in y0..y1 {
                    for cx in x0..x1 {
                        self.stale[(cy * cw + cx) as usize] |= STALE_LIGHT;
                    }
                }
            }
            self.glows = glows;
        }

        let mut light_bits = STALE_LIGHT;
        if self.liquid_light_at.elapsed() >= LIQUID_LIGHT_EVERY {
            self.liquid_light_at = std::time::Instant::now();
            light_bits |= STALE_LIQUID_LIGHT;
        }
        let mut relight = Vec::new();
        'scan: for cy in cy0..cy1 {
            for cx in cx0..cx1 {
                if self.stale[(cy * self.world.chunks_x + cx) as usize] & light_bits != 0 {
                    relight.push((cx, cy));
                    if relight.len() == LIGHT_BUDGET {
                        break 'scan;
                    }
                }
            }
        }
        if !relight.is_empty() {
            let rects: Vec<Rect> = relight.iter().map(|&(cx, cy)| Rect::of_chunk(cx, cy, &self.world)).collect();
            // Neighbouring chunks share one window; scattered ones get their own
            // instead of one box spanning everything between them.
            let area = |r: &Rect| r.width() as u64 * (r.y1 - r.y0) as u64;
            let mut boxes: Vec<(Rect, u64)> = Vec::new();
            for r in &rects {
                let merged =
                    |b: &Rect| Rect { x0: b.x0.min(r.x0), y0: b.y0.min(r.y0), x1: b.x1.max(r.x1), y1: b.y1.max(r.y1) };
                match boxes.iter_mut().find(|(b, sum)| area(&merged(b)) <= (sum + area(r)) * 2) {
                    Some((b, sum)) => (*b, *sum) = (merged(b), *sum + area(r)),
                    None => boxes.push((*r, area(r))),
                }
            }
            for (b, _) in boxes {
                self.scene.light.update(&self.world, b, &self.glows);
            }
            for (r, (cx, cy)) in rects.into_iter().zip(relight) {
                renderer.update_light(&self.scene.light, r);
                self.stale[(cy * self.world.chunks_x + cx) as usize] &= !(STALE_LIGHT | STALE_LIQUID_LIGHT);
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

/// Tree cells of a chunk that carry a top or a branch.
fn tree_cells(world: &World, cx: u32, cy: u32) -> Vec<(u32, u32)> {
    let r = Rect::of_chunk(cx, cy, world);
    (r.y0..r.y1)
        .flat_map(|y| (r.x0..r.x1).map(move |x| (x, y)))
        .filter(|&(x, y)| world.tile(x as i64, y as i64) == TREE && world.frame(x as i64, y as i64).1 >= 198)
        .collect()
}

/// Background state for worlds without one: surface line at the average ground level.
fn guess_env(world: &World, surface: &[u32]) -> Env {
    let ground = surface.iter().map(|&y| y as f64).sum::<f64>() / surface.len().max(1) as f64;
    let w = world.width as i32;
    Env { world_surface: ground, rock_layer: ground + world.height as f64 * 0.1, tree_x: [w; 3], ..Env::default() }
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
        let mut important = vec![false; 30];
        important[BENCH as usize] = true;
        important[TORCH as usize] = true;
        important[POT as usize] = true;
        let world = World::parse(&formats::synth::write_world("g", 64, 64, &important, |x, y| {
            let (tile, fx) = match (x, y) {
                (_, 20..) => (1, 0),
                (10..=13, 19) => (BENCH, ((x - 10) % 2) as i16 * 18),
                (20, 19) => (TORCH, 0),
                (30..=31, 18..=19) => (POT, ((x - 30) * 18) as i16),
                _ => (NO_TILE, 0),
            };
            let fy = if tile == POT { (y - 18) as i16 * 18 } else { 0 };
            Tile { tile, frame_x: fx, frame_y: fy, ..Tile::default() }
        }))
        .unwrap();
        let scene = Scene::build(&world, None, &EXTRA_TILES, &[], Vec::new());
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

    /// One tick with the tool held on a tile.
    fn swing(g: &mut Game, x: i64, y: i64) {
        g.use_at(x as f32 * 16.0 + 8.0, y as f32 * 16.0 + 8.0, false);
        let target = g.use_target.take();
        g.use_item(target);
    }

    /// Holds the tool on a tile until it breaks; returns the ticks it took.
    fn mine(g: &mut Game, x: i64, y: i64) -> u32 {
        let mut ticks = 0;
        while g.world.tile(x, y) != NO_TILE {
            swing(g, x, y);
            ticks += 1;
            assert!(ticks < 600, "never breaks");
        }
        ticks
    }

    #[test]
    fn tools_mine_at_the_game_rate() {
        let mut g = game();
        g.inventory.selected = 0; // sword: no mining
        swing(&mut g, 16, 20);
        assert_eq!(g.hits.damage(16, 20, Target::Tile), 0);
        g.inventory.selected = 1; // copper pickaxe: 35 on stone every 15 ticks
        swing(&mut g, 16, 20);
        assert_eq!(g.hits.damage(16, 20, Target::Tile), 35);
        swing(&mut g, 16, 20);
        assert_eq!(g.hits.damage(16, 20, Target::Tile), 35, "waits for the use time");
        assert_eq!(mine(&mut g, 16, 20), 29, "hits on ticks 0, 15 and 30");
        g.inventory.slots[1] = Some(Stack { item: Item::Tool(Tool::Pickaxe(Tier::Iron)), count: 1 });
        g.item_animation = 0;
        g.item_time = 0;
        assert_eq!(mine(&mut g, 17, 20), 27, "40 a hit, the third on tick 26 after the swing restarts");
        swing(&mut g, 22, 20);
        assert_eq!(g.hits.damage(22, 20, Target::Tile), 0, "out of reach");
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
    fn pots_shatter_into_loot() {
        let mut g = game();
        g.break_tile(31, 19, true);
        assert_eq!(g.world.tile(30, 18), NO_TILE, "the whole pot");
        assert!(!g.drops.is_empty(), "always something");
        assert!(g.drops.iter().all(|d| !matches!(d.item, Item::Object(t) if g.templates[t as usize].tile == POT)));
    }

    #[test]
    fn potions_heal_then_sicken() {
        let mut g = game();
        g.inventory.slots[5] = Some(Stack { item: Item::Id(28), count: 2 });
        g.hp = 30;
        g.quick_heal();
        assert_eq!((g.hp, g.potion_delay), (80, POTION_DELAY));
        assert_eq!(g.inventory.count(Item::Id(28)), 1);
        g.item_animation = 0;
        g.quick_heal();
        assert_eq!(g.hp, 80, "sick for a minute");
        g.drops.push(Drop { x: 0.0, y: 0.0, vx: 0.0, vy: 0.0, item: Item::Id(pots::HEART), count: 1, grab_delay: 0.0 });
        pick_up(&mut g);
        assert_eq!(g.hp, 100, "a heart is 20 life");
        assert_eq!(g.inventory.count(Item::Id(pots::HEART)), 0);
    }

    #[test]
    fn staff_spends_mana_and_its_bolt_hits() {
        let mut g = game();
        g.inventory.slots[9] = Some(Stack { item: Item::Id(magic::AMETHYST_STAFF), count: 1 });
        g.inventory.selected = 9;
        let (px, py) = g.player.center();
        g.mobs = vec![Mob::new(Kind::Slime, px + 100.0, 320.0)];
        let target = Some((px + 100.0, py));
        g.use_item(target);
        assert_eq!((g.mana.cur, g.bolts.len()), (13, 1));
        assert!(!g.player.facing_left);
        for _ in 0..60 {
            g.update_bolts();
        }
        assert!(g.bolts.is_empty());
        assert!(g.mobs.is_empty() || g.mobs[0].hp < Kind::Slime.max_hp(), "hit");
        assert!(!g.dust.is_empty(), "a puff of amethyst");

        (g.mana.cur, g.item_animation, g.release_use) = (6, 0, true);
        g.use_item(target);
        assert!(g.bolts.is_empty() && g.item_animation == 0, "not enough mana");
    }

    #[test]
    fn gem_staffs_shoot_their_own_way() {
        let staff = |g: &mut Game, id: u16| {
            g.inventory.slots[9] = Some(Stack { item: Item::Id(id), count: 1 });
            g.inventory.selected = 9;
            (g.mana.max, g.mana.cur, g.item_animation, g.release_use) = (200, 200, 0, true);
            g.bolts.clear();
        };
        let mut g = game();
        let (px, py) = g.player.center();
        let target = Some((px + 100.0, py));

        staff(&mut g, 741);
        g.use_item(target);
        assert_eq!(
            g.bolts.iter().map(|b| (b.kind, b.twin, b.damage)).collect::<Vec<_>>(),
            [(123, Some(0.0), 10), (123, Some(1.0), 10)]
        );

        // Ruby: a second bolt halfway through the swing, without more mana.
        staff(&mut g, 743);
        g.use_item(target);
        let mana = g.mana.cur;
        for _ in 0..30 {
            g.use_item(None);
        }
        assert_eq!((g.bolts.len(), g.mana.cur), (2, mana));
        assert_eq!(g.bolts[0].damage, 9, "half of 18");

        staff(&mut g, 742);
        g.use_item(target);
        let s = item_ids::stats(742).unwrap().shoot_speed;
        assert!((g.bolts[0].vel[0] - s * 5.0).abs() < 1e-3);

        // Topaz: the bolt bursts on the floor and hurts a slime beside it.
        staff(&mut g, 740);
        g.bolts.push(Bolt::new(122, [200.0, 300.0], [0.0, 8.0], 16, 1.0, None));
        g.mobs = vec![Mob::new(Kind::Slime, 250.0, 320.0)];
        for _ in 0..10 {
            g.update_bolts();
        }
        assert!(g.bolts.is_empty());
        assert!(g.mobs.is_empty() || g.mobs[0].hp < Kind::Slime.max_hp(), "burst");
    }

    #[test]
    fn bars_and_staffs_are_crafted_from_ore_and_gems() {
        let out = |id: u16| RECIPES.iter().find(|r| r.out == Out::Item(Item::Id(id))).unwrap();
        assert_eq!(out(706).inputs, [(Item::Block(169), 4)]);
        assert_eq!(out(744).inputs, [(Item::Id(706), 7), (Item::Id(182), 8)]);
        assert_eq!(out(744).station, Some(items::ANVIL));
    }

    #[test]
    fn armour_is_crafted_worn_and_takes_off_damage() {
        let out = |id: u16| RECIPES.iter().find(|r| r.out == Out::Item(Item::Id(id))).unwrap();
        assert_eq!(out(727).inputs, [(Item::Block(WOOD), 20)]);
        assert_eq!(out(727).station, Some(items::WORK_BENCH));
        assert_eq!(out(80).inputs, [(Item::Material(Material::CopperBar), 20)]);
        assert_eq!(out(698).inputs, [(Item::Id(706), 25)]);
        assert_eq!(out(704).inputs, [(Item::Block(167), 3)]);
        for id in [
            727, 728, 729, 89, 80, 76, 687, 688, 689, 90, 81, 77, 690, 691, 692, 91, 82, 78, 693, 694, 695, 92, 83, 79,
            696, 697, 698,
        ] {
            assert!(armor::slot_of(id).is_some() && RECIPES.iter().any(|r| r.out == Out::Item(Item::Id(id))), "{id}");
        }

        let mut g = game();
        g.mobs.clear();
        for (i, id) in [89, 80, 76].into_iter().enumerate() {
            g.inventory.slots[i] = Some(Stack { item: Item::Id(id), count: 1 });
            assert!(g.inventory.equip(i));
        }
        assert_eq!(g.defense(), 6);
        assert_eq!(g.worn_pictures(), [1, 1, 1]);
        let (px, py) = g.player.center();
        g.mobs.push(Mob::new(Kind::Slime, px, py + 21.0));
        g.contact_damage();
        let lost = MAX_HP - g.hp;
        assert!((3..=5).contains(&lost), "7 within 15%, less 3: {lost}");
    }

    #[test]
    fn stars_fall_into_crystals_and_monsters_drop_mana() {
        let mut g = game();
        g.time = 0.8;
        g.stars.push(Star::new([200.0, 16.0], [0.0, 12.0]));
        for _ in 0..300 {
            g.update_stars();
        }
        assert!(g.stars.iter().all(|s| s.wait > 0), "the first one landed");
        assert!(g.drops.iter().any(|d| d.item == Item::Id(magic::FALLEN_STAR)));
        g.drops.clear();
        g.inventory.add(Item::Id(magic::FALLEN_STAR), 5);
        let crystal = RECIPES.iter().position(|r| r.out == Out::Item(Item::Id(magic::MANA_CRYSTAL))).unwrap();
        assert!(g.craft(crystal), "no station needed");
        g.inventory.selected =
            g.inventory.slots.iter().position(|s| s.is_some_and(|s| s.item == Item::Id(magic::MANA_CRYSTAL))).unwrap();
        g.use_item(Some((0.0, 0.0)));
        assert_eq!((g.mana.cur, g.mana.max), (40, 40));
        assert_eq!(g.inventory.count(Item::Id(magic::MANA_CRYSTAL)), 0);

        g.mana.cur = 0;
        g.drops.push(Drop {
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            item: Item::Id(magic::MANA_STAR),
            count: 1,
            grab_delay: 0.0,
        });
        pick_up(&mut g);
        assert_eq!(g.mana.cur, 40, "100 back, up to the max");
        g.drops.push(Drop {
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            item: Item::Id(magic::MANA_STAR),
            count: 1,
            grab_delay: 0.0,
        });
        pick_up(&mut g);
        assert_eq!(g.drops.len(), 1, "left lying while mana is full");

        g.drops.clear();
        g.mana.cur = 0;
        for _ in 0..40 {
            g.drop_life_and_mana(0.0, 0.0);
        }
        let stars = g.drops.iter().filter(|d| d.item == Item::Id(magic::MANA_STAR)).count();
        assert!((10..40).contains(&stars), "{stars}");
    }

    #[test]
    fn gem_stones_drop_gems() {
        let mut g = game();
        g.world.set_tile(16, 19, 67, -1, -1);
        g.break_tile(16, 19, true);
        assert_eq!(g.drops.last().map(|d| d.item), Some(Item::Id(181)));
    }

    #[test]
    fn objects_drop_and_place_again() {
        let mut g = game();
        g.inventory.selected = 1;
        mine(&mut g, 11, 19);
        assert_eq!((g.world.tile(10, 19), g.world.tile(11, 19)), (NO_TILE, NO_TILE));
        assert_eq!(g.world.tile(12, 19), BENCH, "the bench beside stays");
        (g.item_animation, g.item_time) = (0, 0);
        assert_eq!(mine(&mut g, 20, 19), 1, "torches go in one hit");
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
    fn held_torches_and_ground_stars_glow() {
        let mut g = game();
        assert!(g.glows().is_empty());
        let torch = g.furniture(Furniture::Torch);
        g.inventory.slots[0] = Some(Stack { item: torch, count: 1 });
        g.inventory.selected = 0;
        let (px, py) = (g.player.x, g.player.y);
        g.player.facing_left = false;
        // No sheet here: a 16 px wide torch, its flame 12 px ahead of it and 14 up.
        let x = px + player::WIDTH * 0.5 + 10.0 + 12.0;
        assert_eq!(g.glows(), [((x / 16.0) as u32, ((py + 10.0) / 16.0) as u32, [1.0, 0.95, 0.8])]);
        g.player.wet = Some(0);
        assert!(g.glows().is_empty(), "out under water");

        g.toss(100.0, 100.0, Item::Id(magic::FALLEN_STAR), 1);
        let d = &g.drops[0];
        let want = (((d.x + DROP_SIZE) / 16.0) as u32, ((d.y + DROP_SIZE / 2.0) / 16.0) as u32, [0.8, 0.7, 0.1]);
        assert_eq!(g.glows(), [want]);
    }

    #[test]
    fn guide_is_left_alone_and_the_boss_comes_at_night() {
        let mut g = game();
        let (gx, gy) = g.mobs[0].body.center();
        g.mobs[0].body.x = g.player.x + 20.0;
        for _ in 0..30 {
            g.use_at(gx, gy, false);
            g.update(Some(Input::default()), 1.0 / 60.0, 1.0);
        }
        assert_eq!(g.mobs[0].hp, Kind::Guide.max_hp(), "the guide cannot be hit");
        assert_eq!(g.hp, MAX_HP, "nor hurts");

        g.inventory.slots[9] = Some(Stack { item: Item::Material(Material::SuspiciousEye), count: 1 });
        g.inventory.selected = 9;
        assert!(!g.summon_boss(), "not by day");
        g.time = 0.8;
        assert!(g.summon_boss());
        assert!(g.inventory.slots[9].is_none(), "used up");
        assert_eq!(g.boss_life(), Some(1.0));
        let i = g.mobs.iter().position(|m| m.kind == Kind::EyeBoss).unwrap();
        g.hit_mob(i, Kind::EyeBoss.max_hp() * 2, 0.0, 1.0, false, false);
        assert_eq!(g.boss_life(), None);
        assert!(g.drops.iter().any(|d| d.item == Item::Block(items::DEMONITE_ORE) && d.count >= 30));
        g.time = 0.8;
        g.mobs.push(Mob::new(Kind::EyeBoss, 0.0, 0.0));
        g.time = 0.1;
        g.update_mobs(0.0, false);
        assert_eq!(g.boss_life(), None, "gone at dawn");
        assert_eq!(g.mobs[0].kind, Kind::Guide, "the guide stays");
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
        assert_eq!(g.mobs[0].kind, Kind::Guide);
        g.mobs.clear();
        let (px, py) = g.player.center();
        g.mobs.push(Mob::new(Kind::Slime, px, py + 21.0));
        g.contact_damage();
        let lost = MAX_HP - g.hp;
        assert!((5..=8).contains(&lost), "7 within 15%: {lost}");
        assert_eq!(g.immune, 40);
        assert_eq!((g.player.vx, g.player.vy), (-4.5, -3.5), "knocked away");
        g.contact_damage();
        assert_eq!(MAX_HP - g.hp, lost, "immune for a moment");

        // A wooden sword: 7 a hit less 1 for the slime's defense, so four or five swings.
        g.inventory.slots[0] = Some(Stack { item: Item::Tool(Tool::Sword(Tier::Wood)), count: 1 });
        g.inventory.selected = 0;
        g.mobs[0].body.x = g.player.x + player::WIDTH;
        let mut swings = 0;
        while !g.mobs.is_empty() {
            let (mx, my) = g.mobs[0].body.center();
            g.mobs[0].body.x = g.player.x + player::WIDTH;
            (g.mobs[0].body.vx, g.mobs[0].body.vy) = (0.0, 0.0);
            g.use_at(mx, my, false);
            let target = g.use_target.take();
            g.use_item(target);
            if g.item_animation == Tool::Sword(Tier::Wood).stats().use_animation - 1 {
                swings += 1;
            }
            // No auto reuse: let go between swings.
            if g.item_animation == 0 {
                g.use_item(None);
            }
            assert!(swings < 10, "never dies");
        }
        assert!((4..=6).contains(&swings), "{swings} swings");
    }

    #[test]
    fn thrown_items_fly_ahead_and_wait_before_coming_back() {
        let mut g = game();
        g.inventory.selected = 0;
        let before = g.inventory.count(Item::Tool(Tool::Sword(items::Tier::Copper)));
        g.player.facing_left = false;
        g.throw_held(true);
        assert_eq!(g.item_animation, 10);
        assert_eq!(g.drops.len(), 1);
        assert!(g.drops[0].vx > 0.0, "thrown the way the player faces");
        assert_eq!(g.inventory.count(Item::Tool(Tool::Sword(items::Tier::Copper))), before - 1);
        g.update_drops(1.0);
        assert_eq!(g.drops.len(), 1, "not taken back at once");
        // Bring it back beside the player once the delay is over.
        let (cx, cy) = g.player.center();
        (g.drops[0].x, g.drops[0].y) = (cx - 5.0, cy - 5.0);
        g.update_drops(1.0);
        assert!(g.drops.is_empty(), "picked up again");
        g.throw_held(true);
        assert_eq!(g.drops.len(), 0, "not during the 10 ticks");
    }

    #[test]
    fn life_comes_back_after_a_while_unhurt() {
        let mut g = game();
        let mut healed = |vx: f32| {
            (g.hp, g.regen_time, g.regen_count, g.player.vx) = (50, 0.0, 0, vx);
            for _ in 0..1200 {
                g.life_regen();
            }
            g.hp - 50
        };
        // Nothing before ten seconds at 100 life, then one count a tick.
        assert_eq!(healed(0.0), 5);
        assert_eq!(healed(1.0), 2, "slower on the move");
    }

    #[test]
    fn items_know_their_game_id() {
        let g = game();
        assert_eq!(g.item_id(Item::Tool(Tool::Pickaxe(items::Tier::Copper))), Some(3509));
        assert_eq!(g.item_id(Item::Block(items::STONE)), Some(3));
        assert_eq!(g.item_id(Item::Block(items::WOOD)), Some(9));
        assert_eq!(g.item_id(Item::Bucket(Some(Liquid::Water))), Some(206));
        assert_eq!(g.item_name(Item::Block(items::STONE)), None, "no texts without an install");
        assert!(item_ids::TILE_ITEM.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(item_ids::ITEMS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn swing_hitbox_follows_the_arc() {
        let mut g = game();
        g.inventory.slots[0] = Some(Stack { item: Item::Tool(Tool::Sword(Tier::Wood)), count: 1 });
        g.inventory.selected = 0;
        let s = Tool::Sword(Tier::Wood).stats();
        let (px, py) = (g.player.x as i32, g.player.y as i32);
        // Start of the swing: up and behind, twice as wide.
        g.item_animation = 19;
        let [x, y, w, h] = g.melee_hitbox(s);
        let (fw, fh) = g.held_frame();
        assert_eq!((w, h), ((fw * 2.0) as i32, (fh * 1.4) as i32));
        assert!(x < px && y < py);
        // End: low and in front.
        g.item_animation = 2;
        let [x, y, ..] = g.melee_hitbox(s);
        assert!(x > px && y > py - 16, "{x},{y} from {px},{py}");
        // A swing to the left faces left and ends on that side.
        g.item_animation = 0;
        g.release_use = true;
        let (cx, cy) = g.player.center();
        g.use_item(Some((cx - 40.0, cy)));
        assert!(g.player.facing_left);
        g.item_animation = 2;
        let [x, _, w, _] = g.melee_hitbox(s);
        assert!(x + w < px + 20, "{x}+{w} from {px}");
    }
}
