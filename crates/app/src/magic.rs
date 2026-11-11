//! Mana and the gem staffs: `Player.UpdateManaRegen` without buffs or
//! accessories, the six staffs from amethyst to diamond and their bolts
//! (projectiles 121 to 126, `AI_029_GemStaffs` with the feature each gets
//! from `PackGemStaffFeatures`), and the dust they leave. Then where mana comes from: falling stars at
//! night (projectiles 720 and 12), the mana crystal they make, and the mana
//! stars monsters drop.

use world::collision::{self, V2};
use world::World;

pub const AMETHYST_STAFF: u16 = 739;
pub const DIAMOND_STAFF: u16 = 744;
pub const FALLEN_STAR: u16 = 75;
pub const MANA_CRYSTAL: u16 = 109;
pub const MANA_STAR: u16 = 184;
/// `statManaMax` to start with, and as far as mana crystals take it.
pub const MAX_MANA: i32 = 20;
pub const MANA_CRYSTAL_CAP: i32 = 200;
/// Sheets the staff, the gems, the bolt and the stars draw with.
pub const SHEETS: &[&str] = &[
    "Item_739",
    "Item_740",
    "Item_741",
    "Item_742",
    "Item_743",
    "Item_744",
    "Item_703",
    "Item_21",
    "Item_705",
    "Item_19",
    "Item_706",
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
    "Projectile_122",
    "Projectile_123",
    "Projectile_124",
    "Projectile_125",
    "Projectile_126",
    "Projectile_12",
    "Extra_91",
    "Dust",
];
/// Gem stone tiles 63 to 68 drop gems 177 to 182 (`KillTile_GetItemDrops`).
pub const GEM_STONES: std::ops::RangeInclusive<u16> = 63..=68;
pub const fn gem(tile: u16) -> u16 {
    tile - 63 + 177
}
/// The bolt a staff shoots, 121 for amethyst to 126 for diamond.
pub fn staff_bolt(item: u16) -> Option<u16> {
    (AMETHYST_STAFF..=DIAMOND_STAFF).contains(&item).then(|| item - AMETHYST_STAFF + 121)
}
/// The bolt's box, and `Projectile.oldPos` it keeps for its trail.
pub const BOLT_SIZE: f32 = 10.0;
const TRAIL: usize = 30;
/// `FindTargetWithLineOfSight(500)` for the homing.
pub const HOMING_RANGE: f32 = 500.0;
/// `localNPCHitCooldown`: ticks before a bolt can hit the same monster again.
pub const HIT_COOLDOWN: i32 = 60;
/// The topaz burst: a 160 px box at 0.35 of the damage and knockback.
pub const BURST: f32 = 160.0;

/// `PackGemStaffFeatures` without armour: one feature per gem.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Features {
    pub homing: bool,
    pub burst: bool,
    pub twins: bool,
    pub fast_then_slow: bool,
    pub spread: bool,
    pub bigger: bool,
}

impl Features {
    pub fn of(bolt: u16) -> Self {
        Self {
            homing: bolt == 121,
            burst: bolt == 122,
            twins: bolt == 123,
            fast_then_slow: bolt == 124,
            spread: bolt == 125,
            bigger: bolt == 126,
        }
    }

    /// `GetDamageMultiplier`.
    pub fn damage_multiplier(self) -> f32 {
        let mut k: f32 = 1.0;
        if self.twins {
            k = k.min(0.6);
        }
        if self.spread {
            k = k.min(0.5);
        }
        k
    }
}

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

/// A gem bolt: invisible itself, drawn as a glow and a trail.
pub struct Bolt {
    /// Projectile 121 to 126.
    pub kind: u16,
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
    /// Monsters it can still hit, and `armorPenetration`.
    pub penetrate: i32,
    pub armor_pen: i32,
    /// The sapphire pair: which of the two (`ai[1]`) and its clock (`localAI[0]`).
    pub twin: Option<f32>,
    swirl: f32,
    /// Shrinks once the emerald bolt has stopped.
    pub scale: f32,
    /// Ticks lived, for the glow's pulse.
    pub age: u32,
    /// Monsters hit lately by index, and the ticks before they can be hit again.
    pub immune: Vec<(usize, i32)>,
}

impl Bolt {
    /// `Projectile.NewProjectile` at `center`, then `FinalizeProjectile_GemStaves`:
    /// emerald lives twice as long and pierces once more, as does diamond; ruby
    /// ignores 4 defense.
    pub fn new(kind: u16, center: V2, vel: V2, damage: i32, knock_back: f32, twin: Option<f32>) -> Self {
        let h = BOLT_SIZE / 2.0;
        let f = Features::of(kind);
        Self {
            kind,
            pos: [center[0] - h, center[1] - h],
            vel,
            damage,
            knock_back,
            time_left: if f.fast_then_slow { 600 } else { 300 },
            sounded: false,
            trail: Vec::new(),
            penetrate: 1 + (f.fast_then_slow || f.bigger) as i32,
            armor_pen: if f.spread { 4 } else { 0 },
            twin,
            swirl: 0.0,
            scale: 1.0,
            age: 0,
            immune: Vec::new(),
        }
    }

    pub fn features(&self) -> Features {
        Features::of(self.kind)
    }

    /// Its dust: `type - 121 + 86`.
    pub fn dust(&self) -> u16 {
        self.kind - 121 + 86
    }

    pub fn center(&self) -> V2 {
        [self.pos[0] + BOLT_SIZE / 2.0, self.pos[1] + BOLT_SIZE / 2.0]
    }

    pub fn rotation(&self) -> f32 {
        self.vel[1].atan2(self.vel[0])
    }

    /// The homing of `AI_029_GemStaffs_ActuallyGemStaffs` toward `target`'s centre.
    pub fn home(&mut self, target: V2) {
        let max_speed = if self.features().fast_then_slow { 16.0 } else { 8.0 };
        let (grow, push, turn) = (1.1, 0.1, 0.1);
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

    /// The sapphire pair weaving about their line, half a turn apart.
    pub fn swirl(&mut self) {
        let Some(twin) = self.twin else { return };
        use std::f32::consts::{PI, TAU};
        let wave = |t: f32| (t * TAU * 0.05 + twin * PI).sin();
        let d = wave(self.swirl + 1.0) - wave(self.swirl);
        self.swirl += 1.0;
        let a = self.rotation() + PI / 2.0;
        self.pos = [self.pos[0] + a.cos() * d * 6.0, self.pos[1] + a.sin() * d * 6.0];
    }

    /// The emerald bolt slows to a stop, then shrinks away. False once gone.
    pub fn slow_down(&mut self) -> bool {
        if !self.features().fast_then_slow {
            return true;
        }
        if len(self.vel) > 0.1 {
            self.vel = [self.vel[0] * 0.97, self.vel[1] * 0.97];
        } else {
            self.scale -= 0.005;
        }
        self.scale >= 0.1
    }

    /// `HandleMovement`: a 2x2 box at the centre against the tiles. False when
    /// it hit one, which kills it, except the emerald bolt, which stops there.
    pub fn advance(&mut self, world: &World) -> bool {
        self.trail.insert(0, (self.pos, self.rotation()));
        self.trail.truncate(TRAIL);
        let c = self.center();
        let b = collision::Box { world, pos: [c[0] - 1.0, c[1] - 1.0], size: [2, 2] };
        let last = self.vel;
        let vel = b.tiles(last, true, true).vel;
        self.vel = vel;
        self.pos = [self.pos[0] + vel[0], self.pos[1] + vel[1]];
        if vel == last {
            return true;
        }
        if self.features().fast_then_slow {
            self.pos = [self.pos[0] + vel[0] * 0.99, self.pos[1] + vel[1] * 0.99];
            let n = norm(last);
            self.vel = [n[0] * 0.0001, n[1] * 0.0001];
            return true;
        }
        false
    }

    /// `Damage_GetHitbox`, grown by 4 on each side for `aiStyle` 29, and by 30
    /// more for diamond. The emerald bolt also hits from its last five places.
    pub fn hitboxes(&self) -> Vec<[f32; 4]> {
        let grow = if self.features().bigger { 34.0 } else { 4.0 };
        let at = |p: V2| [p[0] - grow, p[1] - grow, BOLT_SIZE + 2.0 * grow, BOLT_SIZE + 2.0 * grow];
        let mut v = vec![at(self.pos)];
        if self.features().fast_then_slow {
            v.extend(self.trail.iter().take(5).map(|t| at(t.0)));
        }
        v
    }
}

/// A speck of dust (`Dust`): slows and shrinks away, falling unless `noGravity`.
pub struct Dust {
    pub kind: u16,
    pub gravity: bool,
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
        let mut s = (1.0 + (r(41) - 20) as f32 * 0.01) * scale;
        let mut vel = [vx, vy];
        if flame(kind) {
            vel = [vel[0] * 0.3, (r(16) - 10) as f32 * 0.1];
            s *= 0.7;
        }
        Self { kind, gravity: false, pos: [x, y], vel, scale: s, rot: 0.0, variant }
    }

    /// `UpdateDust`. False once it is gone.
    pub fn update(&mut self) -> bool {
        self.pos = [self.pos[0] + self.vel[0], self.pos[1] + self.vel[1]];
        if self.gravity {
            self.vel[1] += if flame(self.kind) { 0.05 } else { 0.1 };
        }
        self.vel[0] *= 0.99;
        self.rot += self.vel[0] * 0.5;
        self.scale -= 0.01;
        if !self.gravity {
            self.vel = [self.vel[0] * 0.92, self.vel[1] * 0.92];
            self.scale -= 0.04;
        }
        self.scale >= 0.1
    }

    /// Its 8x8 picture on the `Dust` sheet.
    pub fn frame(&self) -> [u32; 4] {
        let k = self.kind as u32;
        [k % 100 * 10, self.variant * 10 + k / 100 * 30, 8, 8]
    }
}

/// Torch flames and the like, which `NewDust` starts slow and small and
/// which fall gently.
pub fn flame(kind: u16) -> bool {
    matches!(kind, 6 | 29 | 59..=65 | 75 | 135 | 158 | 169 | 228 | 242 | 269 | 279 | 293..=298 | 302 | 307 | 310)
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
        let mut b = Bolt::new(121, [0.0, 0.0], [6.0, 0.0], 15, 3.25, None);
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
    fn each_gem_has_its_feature() {
        assert_eq!(staff_bolt(743), Some(125));
        assert_eq!(staff_bolt(745), None);
        let ruby = Bolt::new(125, [0.0; 2], [1.0, 0.0], 9, 1.0, None);
        assert_eq!((ruby.penetrate, ruby.armor_pen, ruby.dust()), (1, 4, 90));
        assert_eq!(Features::of(125).damage_multiplier(), 0.5);
        assert_eq!(Features::of(123).damage_multiplier(), 0.6);
        let diamond = Bolt::new(126, [0.0; 2], [1.0, 0.0], 9, 1.0, None);
        assert_eq!(diamond.penetrate, 2);
        assert_eq!(diamond.hitboxes(), [[-39.0, -39.0, 78.0, 78.0]]);

        // Emerald: slows down, then fades out.
        let mut e = Bolt::new(124, [0.0; 2], [30.0, 0.0], 9, 1.0, None);
        assert_eq!((e.penetrate, e.time_left), (2, 600));
        let mut ticks = 0;
        while e.slow_down() {
            ticks += 1;
        }
        assert!(len(e.vel) <= 0.1 && (300..400).contains(&ticks), "{ticks}");

        // Sapphire: the two weave on either side of the line.
        let mut a = Bolt::new(123, [0.0; 2], [5.0, 0.0], 9, 1.0, Some(0.0));
        let mut b = Bolt::new(123, [0.0; 2], [5.0, 0.0], 9, 1.0, Some(1.0));
        for _ in 0..5 {
            a.swirl();
            b.swirl();
        }
        let (ya, yb) = (a.pos[1] + 5.0, b.pos[1] + 5.0);
        assert!(ya > 0.0 && (ya + yb).abs() < 1e-3, "{ya} {yb}");
    }

    #[test]
    fn torch_flames_fall_slowly() {
        let mut d = Dust::new(6, [0.0; 2], 4, 4, [0.0; 2], 1.0, |n| n / 2);
        assert!((d.scale - 0.7).abs() < 1e-6);
        d.gravity = true;
        d.vel = [0.0; 2];
        d.update();
        assert!((d.vel[1] - 0.05).abs() < 1e-6);
    }

    #[test]
    fn angles_wrap() {
        use std::f32::consts::PI;
        assert!((angle_towards(3.0, -3.0, 0.1) - (3.1)).abs() < 1e-5, "the short way round");
        assert!((angle_towards(0.0, PI / 2.0, 0.1) - 0.1).abs() < 1e-6);
    }
}
