//! Mana and the first magic weapon: `Player.UpdateManaRegen` without buffs or
//! accessories, the Amethyst Staff and its bolt (projectile 121,
//! `AI_029_GemStaffs` with the homing it gets from `PackGemStaffFeatures`),
//! and the dust both leave. Then where mana comes from: falling stars at
//! night (projectiles 720 and 12), the mana crystal they make, and the mana
//! stars monsters drop.

use world::collision::{self, V2};
use world::World;

pub const AMETHYST_STAFF: u16 = 739;
pub const FALLEN_STAR: u16 = 75;
pub const MANA_CRYSTAL: u16 = 109;
pub const MANA_STAR: u16 = 184;
/// `statManaMax` to start with, and as far as mana crystals take it.
pub const MAX_MANA: i32 = 20;
pub const MANA_CRYSTAL_CAP: i32 = 200;
/// Sheets the staff, the gems, the bolt and the stars draw with.
pub const SHEETS: &[&str] = &[
    "Item_739",
    "Item_177",
    "Item_178",
    "Item_179",
    "Item_180",
    "Item_181",
    "Item_182",
    "Item_75",
    "Item_109",
    "Item_184",
    "Projectile_121",
    "Projectile_12",
    "Extra_91",
    "Dust",
];
/// Gem stone tiles 63 to 68 drop gems 177 to 182 (`KillTile_GetItemDrops`).
pub const GEM_STONES: std::ops::RangeInclusive<u16> = 63..=68;
pub const fn gem(tile: u16) -> u16 {
    tile - 63 + 177
}
/// Dust of the amethyst bolt: `type - 121 + 86`.
pub const AMETHYST_DUST: u16 = 86;
/// The bolt's box, and `Projectile.oldPos` it keeps for its trail.
pub const BOLT_SIZE: f32 = 10.0;
const TRAIL: usize = 30;
/// `FindTargetWithLineOfSight(500)` for the homing.
pub const HOMING_RANGE: f32 = 500.0;

pub struct Mana {
    pub cur: i32,
    /// `statManaMax2`.
    pub max: i32,
    /// `manaRegenDelay` and `manaRegenCount`.
    delay: f32,
    count: i32,
}

impl Default for Mana {
    fn default() -> Self {
        Self { cur: MAX_MANA, max: MAX_MANA, delay: 0.0, count: 0 }
    }
}

impl Mana {
    /// `CheckMana(cost, pay: true)`.
    pub fn pay(&mut self, cost: i32) -> bool {
        if self.cur < cost {
            return false;
        }
        self.cur -= cost;
        true
    }

    /// `ApplyManaRegenerationDelay`: longer the emptier the stars.
    pub fn delay_regen(&mut self) {
        let empty = 1.0 - self.cur as f32 / self.max as f32;
        self.delay = ((empty * 60.0 * 4.0 + 45.0) * 0.7) as i32 as f32;
    }

    /// `UpdateManaRegen`: once the delay runs out, a point every 120 counts,
    /// twice as fast standing still. True when the stars just filled up.
    pub fn regen(&mut self, still: bool) -> bool {
        if self.delay > 0.0 {
            self.delay -= 1.0;
            if still {
                self.delay -= 1.0;
            }
        }
        let regen = if self.delay <= 0.0 {
            let base = self.max / 3 + 1;
            let regen = if still { base * 2 } else { base };
            let k = 0.5 + 0.5 * (self.cur as f32 / self.max as f32);
            ((regen as f32 * k) as i32).max(2)
        } else {
            0
        };
        self.count += regen;
        let mut filled = false;
        while self.count >= 120 {
            self.count -= 120;
            let gained = self.cur < self.max;
            if gained {
                self.cur += 1;
            }
            if self.cur >= self.max {
                filled |= gained;
                self.cur = self.max;
            }
        }
        filled
    }

    /// `ItemCheck_UseManaCrystal`: 20 more, up to 200.
    pub fn crystal(&mut self) -> bool {
        if self.max >= MANA_CRYSTAL_CAP {
            return false;
        }
        self.max += 20;
        self.cur += 20;
        true
    }

    /// Picking up a mana star (`PickupItem`): 100 back.
    pub fn star(&mut self) {
        self.cur = (self.cur + 100).min(self.max);
    }
}

/// A falling star: first `AI_148_StarSpawner` waiting three seconds out of
/// sight, then projectile 12 (`aiStyle` 5) coming down until it lands.
pub struct Star {
    /// Top-left of its 18x18 box.
    pub pos: V2,
    pub vel: V2,
    pub rot: f32,
    /// Ticks the spawner still waits; drawn and moved only after.
    pub wait: i32,
    /// Hits tiles only once it has been clear of them (`ai[1]`).
    collide: bool,
    pub sound_delay: i32,
    pub time_left: i32,
    pub age: u32,
}

pub const STAR_SIZE: f32 = 18.0;

impl Star {
    pub fn new(pos: V2, vel: V2) -> Self {
        Self { pos, vel, rot: 0.0, wait: 180, collide: false, sound_delay: 0, time_left: 3600, age: 0 }
    }

    pub fn center(&self) -> V2 {
        [self.pos[0] + STAR_SIZE / 2.0, self.pos[1] + STAR_SIZE / 2.0]
    }

    /// One tick in flight: spin, then move, against the tiles once clear of
    /// them. False when it lands.
    pub fn advance(&mut self, world: &World) -> bool {
        self.age += 1;
        if !self.collide && !solid_in(world, self.pos, STAR_SIZE) {
            self.collide = true;
        }
        let dir = if self.vel[0] < 0.0 { -1.0 } else { 1.0 };
        self.rot += (self.vel[0].abs() + self.vel[1].abs()) * 0.01 * dir;
        let vel = if self.collide {
            let s = STAR_SIZE as i32;
            collision::Box { world, pos: self.pos, size: [s, s] }.tiles(self.vel, true, true).vel
        } else {
            self.vel
        };
        let landed = vel != self.vel;
        self.vel = vel;
        self.pos = [self.pos[0] + vel[0], self.pos[1] + vel[1]];
        self.time_left -= 1;
        !landed && self.time_left > 0
    }
}

/// `Collision.SolidCollision`: any solid tile under the square box.
pub fn solid_in(world: &World, pos: V2, size: f32) -> bool {
    let (x0, y0) = ((pos[0] / 16.0).floor() as i64, (pos[1] / 16.0).floor() as i64);
    let (x1, y1) = (((pos[0] + size) / 16.0).floor() as i64, ((pos[1] + size) / 16.0).floor() as i64);
    (x0..=x1).any(|x| (y0..=y1).any(|y| world.solid(x, y)))
}

/// The amethyst bolt: invisible itself, drawn as a glow and a trail.
pub struct Bolt {
    /// Top-left of its 10x10 box.
    pub pos: V2,
    pub vel: V2,
    pub damage: i32,
    pub knock_back: f32,
    pub time_left: i32,
    /// The launch sound has played (`soundDelay`).
    pub sounded: bool,
    /// Past positions and rotations, newest first (`oldPos`, `oldRot`).
    pub trail: Vec<(V2, f32)>,
}

impl Bolt {
    /// `Projectile.NewProjectile` at `center`.
    pub fn new(center: V2, vel: V2, damage: i32, knock_back: f32) -> Self {
        let h = BOLT_SIZE / 2.0;
        Self {
            pos: [center[0] - h, center[1] - h],
            vel,
            damage,
            knock_back,
            time_left: 300,
            sounded: false,
            trail: Vec::new(),
        }
    }

    pub fn center(&self) -> V2 {
        [self.pos[0] + BOLT_SIZE / 2.0, self.pos[1] + BOLT_SIZE / 2.0]
    }

    pub fn rotation(&self) -> f32 {
        self.vel[1].atan2(self.vel[0])
    }

    /// The homing of `AI_029_GemStaffs_ActuallyGemStaffs` toward `target`'s centre.
    pub fn home(&mut self, target: V2) {
        let (max_speed, grow, push, turn) = (8.0, 1.1, 0.1, 0.1);
        let speed = len(self.vel);
        if speed < max_speed {
            let n = norm(self.vel);
            self.vel = [(self.vel[0] + n[0] * push) * grow, (self.vel[1] + n[1] * push) * grow];
        }
        let c = self.center();
        let to = [target[0] - c[0], target[1] - c[1]];
        let dist = len(to);
        let mut k = remap(dist, HOMING_RANGE, 0.0, 0.0, 1.0) * turn + 0.01;
        if speed < max_speed {
            k += 0.05;
            k *= remap(speed, 1.0, 3.0, 10.0, 1.0);
        }
        // `Utils.rotateTowards`.
        let v = len(self.vel);
        let a = angle_towards(self.vel[1].atan2(self.vel[0]), to[1].atan2(to[0]), k);
        self.vel = [a.cos() * v, a.sin() * v];
        // Then a pull straight at it from close by.
        let pull = if dist > 0.0 { max_speed.min(dist) / dist } else { 0.0 };
        let t = remap(dist, 150.0, 0.0, 0.0, 0.2);
        self.vel = [lerp(self.vel[0], to[0] * pull, t), lerp(self.vel[1], to[1] * pull, t)];
    }

    /// `HandleMovement`: a 2x2 box at the centre against the tiles. False when
    /// it hit one, which kills it.
    pub fn advance(&mut self, world: &World) -> bool {
        self.trail.insert(0, (self.pos, self.rotation()));
        self.trail.truncate(TRAIL);
        let c = self.center();
        let b = collision::Box { world, pos: [c[0] - 1.0, c[1] - 1.0], size: [2, 2] };
        let vel = b.tiles(self.vel, true, true).vel;
        let hit = vel != self.vel;
        self.vel = vel;
        self.pos = [self.pos[0] + vel[0], self.pos[1] + vel[1]];
        !hit
    }

    /// `Damage_GetHitbox`: the box grown by 4 on each side for `aiStyle` 29.
    pub fn hitbox(&self) -> [f32; 4] {
        [self.pos[0] - 4.0, self.pos[1] - 4.0, BOLT_SIZE + 8.0, BOLT_SIZE + 8.0]
    }
}

/// A speck of dust (`Dust`), without gravity: slows and shrinks away.
pub struct Dust {
    pub kind: u16,
    pub pos: V2,
    pub vel: V2,
    pub scale: f32,
    pub rot: f32,
    /// One of the three pictures of its kind.
    pub variant: u32,
}

impl Dust {
    /// `Dust.NewDust` in the `w` x `h` box at `pos`; `r(n)` is `Main.rand.Next(n)`.
    pub fn new(kind: u16, pos: V2, w: i32, h: i32, speed: V2, scale: f32, mut r: impl FnMut(i32) -> i32) -> Self {
        let (w, h) = (w.max(5), h.max(5));
        let x = pos[0] + r(w - 4) as f32 + 4.0;
        let y = pos[1] + r(h - 4) as f32 + 4.0;
        let vx = (r(41) - 20) as f32 * 0.1 + speed[0];
        let vy = (r(41) - 20) as f32 * 0.1 + speed[1];
        let variant = r(3) as u32;
        let s = (1.0 + (r(41) - 20) as f32 * 0.01) * scale;
        Self { kind, pos: [x, y], vel: [vx, vy], scale: s, rot: 0.0, variant }
    }

    /// `UpdateDust` for dust without gravity. False once it is gone.
    pub fn update(&mut self) -> bool {
        self.pos = [self.pos[0] + self.vel[0], self.pos[1] + self.vel[1]];
        self.vel[0] *= 0.99;
        self.rot += self.vel[0] * 0.5;
        self.vel = [self.vel[0] * 0.92, self.vel[1] * 0.92];
        self.scale -= 0.04;
        self.scale >= 0.1
    }

    /// Its 8x8 picture on the `Dust` sheet.
    pub fn frame(&self) -> [u32; 4] {
        let k = self.kind as u32;
        [k % 100 * 10, self.variant * 10 + k / 100 * 30, 8, 8]
    }
}

fn len(v: V2) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

fn norm(v: V2) -> V2 {
    let l = len(v);
    if l == 0.0 {
        [0.0, 0.0]
    } else {
        [v[0] / l, v[1] / l]
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// `Utils.Remap`, clamped.
fn remap(v: f32, from_min: f32, from_max: f32, to_min: f32, to_max: f32) -> f32 {
    let t = ((v - from_min) / (from_max - from_min)).clamp(0.0, 1.0);
    lerp(to_min, to_max, t)
}

fn wrap(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let a = (a + PI).rem_euclid(TAU) - PI;
    if a == -PI {
        PI
    } else {
        a
    }
}

/// `Utils.AngleTowards`.
fn angle_towards(cur: f32, target: f32, max: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let (mut cur, target) = (wrap(cur), wrap(target));
    if cur < target {
        if target - cur > PI {
            cur += TAU;
        }
    } else if cur - target > PI {
        cur -= TAU;
    }
    wrap(cur + (target - cur).clamp(-max, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mana_waits_then_comes_back_faster_when_still() {
        let mut m = Mana::default();
        assert!(m.pay(7) && m.pay(7));
        assert!(!m.pay(7), "6 left");
        m.delay_regen();
        // ((1 - 6/20) * 240 + 45) * 0.7 = 149
        assert_eq!(m.delay, 149.0);
        let ticks = |still: bool| {
            let mut m = Mana { cur: 6, max: MAX_MANA, delay: 0.0, count: 0 };
            m.delay = 149.0;
            let mut t = 0;
            while !m.regen(still) {
                t += 1;
            }
            t
        };
        let (moving, still) = (ticks(false), ticks(true));
        assert!(still < moving, "{still} {moving}");
        assert!((150..1500).contains(&moving), "{moving}");
    }

    #[test]
    fn crystals_and_stars() {
        let mut m = Mana { cur: 5, ..Default::default() };
        assert!(m.crystal());
        assert_eq!((m.cur, m.max), (25, 40));
        m.star();
        assert_eq!(m.cur, 40);
        m.max = MANA_CRYSTAL_CAP;
        assert!(!m.crystal(), "200 at most");
    }

    #[test]
    fn bolt_turns_toward_a_target() {
        let mut b = Bolt::new([0.0, 0.0], [6.0, 0.0], 15, 3.25);
        let mut closest = f32::MAX;
        for _ in 0..60 {
            b.home([0.0, 200.0]);
            b.pos = [b.pos[0] + b.vel[0], b.pos[1] + b.vel[1]];
            let c = b.center();
            closest = closest.min(len([c[0], c[1] - 200.0]));
            assert!(len(b.vel) < 9.0, "grows while under 8: {:?}", b.vel);
        }
        assert!(closest < 16.0, "{closest}");
    }

    #[test]
    fn angles_wrap() {
        use std::f32::consts::PI;
        assert!((angle_towards(3.0, -3.0, 0.1) - (3.1)).abs() < 1e-5, "the short way round");
        assert!((angle_towards(0.0, PI / 2.0, 0.1) - 0.1).abs() < 1e-6);
    }
}
