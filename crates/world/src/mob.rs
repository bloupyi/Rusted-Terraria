//! Monsters: a [`Body`] with a simple behaviour toward a target point.
//!
//! Slimes wait on the ground, then hop toward the target. Zombies walk toward
//! it and jump when a wall stops them. Demon eyes fly at it and bounce off
//! blocks. The guide strolls around his home. The eye boss hovers over the
//! target, then charges it three times, faster once below half life.

use crate::body::{Body, GRAVITY, MAX_FALL};
use crate::World;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Slime,
    Zombie,
    DemonEye,
    Guide,
    EyeBoss,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Slime, Kind::Zombie, Kind::DemonEye, Kind::Guide, Kind::EyeBoss];

    pub fn from_u8(v: u8) -> Option<Kind> {
        Kind::ALL.get(v as usize).copied()
    }

    /// The game's `NPC.SetDefaults` for the blue slime, zombie, demon eye,
    /// guide and eye of Cthulhu: size, life, contact damage, defense and
    /// knockback taken.
    fn stats(self) -> (f32, f32, i32, i32, i32, f32) {
        match self {
            Kind::Slime => (24.0, 18.0, 25, 7, 2, 1.0),
            Kind::Zombie => (18.0, 40.0, 45, 14, 6, 0.5),
            Kind::DemonEye => (30.0, 32.0, 60, 18, 2, 0.8),
            Kind::Guide => (18.0, 40.0, 250, 10, 15, 0.5),
            Kind::EyeBoss => (100.0, 110.0, 2800, 15, 12, 0.0),
        }
    }

    pub fn size(self) -> (f32, f32) {
        let (w, h, ..) = self.stats();
        (w, h)
    }

    pub fn max_hp(self) -> i32 {
        self.stats().2
    }

    /// Damage dealt to the player on contact.
    pub fn damage(self) -> i32 {
        if self.friendly() {
            0
        } else {
            self.stats().3
        }
    }

    pub fn defense(self) -> i32 {
        self.stats().4
    }

    pub fn knock_back_resist(self) -> f32 {
        self.stats().5
    }

    pub fn friendly(self) -> bool {
        self == Kind::Guide
    }

    pub fn flies(self) -> bool {
        matches!(self, Kind::DemonEye | Kind::EyeBoss)
    }
}

const SLIME_WAIT: f32 = 1.2;
const SLIME_HOP: (f32, f32) = (130.0, 380.0);
const ZOMBIE_SPEED: f32 = 70.0;
const ZOMBIE_JUMP: f32 = 400.0;
const EYE_ACCEL: f32 = 250.0;
const EYE_SPEED: f32 = 170.0;
const GUIDE_SPEED: f32 = 50.0;
/// The guide stays this close to home.
const GUIDE_RANGE: f32 = 20.0 * 16.0;
const BOSS_HOVER: (f32, f32) = (3.0, 260.0);
const BOSS_DASH: (f32, f32) = (0.9, 520.0);
/// Seconds a hit monster ignores its behaviour while knocked back.
const STUN: f32 = 0.3;

#[derive(Clone, Copy, Debug)]
pub struct Mob {
    pub kind: Kind,
    pub body: Body,
    pub hp: i32,
    pub facing_left: bool,
    /// Behaviour clock: slime wait, zombie walk animation.
    pub timer: f32,
    /// Seconds left of knockback after a hit.
    pub stun: f32,
    /// Behaviour step: guide walk direction, boss charges left.
    pub state: i32,
    /// Where it appeared, for the guide's home.
    pub home: f32,
    /// Ticks before the player's swing can hit it again.
    pub melee_cooldown: i32,
}

impl Mob {
    /// Feet at `(x, y)`.
    pub fn new(kind: Kind, x: f32, y: f32) -> Self {
        let (w, h) = kind.size();
        Self {
            kind,
            body: Body::new(x - w / 2.0, y - h, w, h),
            hp: kind.max_hp(),
            facing_left: false,
            timer: 0.0,
            stun: 0.0,
            state: 0,
            home: x,
            melee_cooldown: 0,
        }
    }

    /// Below half life: the boss's second phase.
    pub fn enraged(&self) -> bool {
        self.kind == Kind::EyeBoss && self.hp * 2 < self.kind.max_hp()
    }

    pub fn update(&mut self, world: &World, target: (f32, f32), dt: f32) {
        let stuck = self.stuck(world, if target.0 < self.body.center().0 { -1.0 } else { 1.0 });
        let b = &mut self.body;
        let (cx, _) = b.center();
        let dir = if target.0 < cx { -1.0 } else { 1.0 };
        self.timer += dt;
        if self.stun > 0.0 {
            self.stun -= dt;
        } else {
            match self.kind {
                Kind::Slime => {
                    if b.on_ground {
                        b.vx = 0.0;
                        if self.timer >= SLIME_WAIT {
                            self.timer = 0.0;
                            (b.vx, b.vy) = (dir * SLIME_HOP.0, -SLIME_HOP.1);
                            self.facing_left = dir < 0.0;
                        }
                    }
                }
                Kind::Zombie => {
                    b.vx = dir * ZOMBIE_SPEED;
                    self.facing_left = dir < 0.0;
                    // A wall too high to step up: jump.
                    if b.on_ground && stuck {
                        b.vy = -ZOMBIE_JUMP;
                    }
                }
                Kind::DemonEye => {
                    let (cx, cy) = b.center();
                    let (dx, dy) = (target.0 - cx, target.1 - cy);
                    let d = dx.hypot(dy).max(1.0);
                    b.vx += dx / d * EYE_ACCEL * dt;
                    b.vy += dy / d * EYE_ACCEL * dt;
                    let v = b.vx.hypot(b.vy);
                    if v > EYE_SPEED {
                        (b.vx, b.vy) = (b.vx / v * EYE_SPEED, b.vy / v * EYE_SPEED);
                    }
                    self.facing_left = b.vx < 0.0;
                }
                Kind::Guide => {
                    // Walk a few seconds, rest a few seconds, turn home when too far.
                    if self.timer >= 2.5 {
                        self.timer = 0.0;
                        self.state = if self.state != 0 {
                            0
                        } else if self.body.x < self.home {
                            1
                        } else {
                            -1
                        };
                    }
                    let b = &mut self.body;
                    if (b.x - self.home).abs() > GUIDE_RANGE {
                        self.state = if b.x < self.home { 1 } else { -1 };
                    }
                    b.vx = self.state as f32 * GUIDE_SPEED;
                    if self.state != 0 {
                        self.facing_left = self.state < 0;
                    }
                    let stuck = self.state != 0 && b.on_ground && {
                        let ahead = self.state as f32;
                        let x = if ahead > 0.0 { b.x + b.w + 1.0 } else { b.x - 1.0 };
                        let feet = ((b.y + b.h) / 16.0).floor() as i64 - 1;
                        world.solid((x / 16.0).floor() as i64, feet) && world.solid((x / 16.0).floor() as i64, feet - 1)
                    };
                    if stuck {
                        b.vy = -ZOMBIE_JUMP;
                    }
                }
                Kind::EyeBoss => {
                    let fast = if self.hp * 2 < self.kind.max_hp() { 1.4 } else { 1.0 };
                    let (cx, cy) = b.center();
                    if self.state == 0 {
                        // Hover above the target, then start charging.
                        let (dx, dy) = (target.0 - cx, target.1 - BOSS_HOVER.1 - cy);
                        let d = dx.hypot(dy).max(1.0);
                        let speed = (d * 2.0).min(300.0 * fast);
                        (b.vx, b.vy) = (dx / d * speed, dy / d * speed);
                        if self.timer >= BOSS_HOVER.0 / fast {
                            (self.timer, self.state) = (BOSS_DASH.0, 3);
                        }
                    } else if self.timer >= BOSS_DASH.0 / fast {
                        self.timer = 0.0;
                        self.state -= 1;
                        if self.state >= 0 {
                            let (dx, dy) = (target.0 - cx, target.1 - cy);
                            let d = dx.hypot(dy).max(1.0);
                            (b.vx, b.vy) = (dx / d * BOSS_DASH.1 * fast, dy / d * BOSS_DASH.1 * fast);
                        }
                        self.state = self.state.max(0);
                    }
                    self.facing_left = target.0 < cx;
                }
            }
        }
        if self.kind == Kind::EyeBoss {
            // Flies through blocks.
            self.body.x += self.body.vx * dt;
            self.body.y += self.body.vy * dt;
            return;
        }
        if self.kind == Kind::DemonEye {
            let (vx, vy) = (self.body.vx, self.body.vy);
            self.body.step(world, dt, false);
            // Bounce off what stopped it.
            if self.body.vx == 0.0 && vx != 0.0 {
                self.body.vx = -vx * 0.6;
            }
            if self.body.vy == 0.0 && vy != 0.0 {
                self.body.vy = -vy * 0.6;
            }
            return;
        }
        let k = if self.body.in_liquid(world) { 0.5 } else { 1.0 };
        self.body.vy = (self.body.vy + GRAVITY * k * dt).min(MAX_FALL * k);
        self.body.step(world, dt, false);
    }

    /// A block right in front at body height, above what a step-up handles.
    fn stuck(&self, world: &World, dir: f32) -> bool {
        let b = &self.body;
        let x = if dir > 0.0 { b.x + b.w + 1.0 } else { b.x - 1.0 };
        let tx = (x / 16.0).floor() as i64;
        let feet = ((b.y + b.h) / 16.0).floor() as i64 - 1;
        world.solid(tx, feet) && world.solid(tx, feet - 1)
    }

    /// `NPC.StrikeNPC`: takes `damage` less half the defense, doubled on a
    /// crit, and a knockback toward `dir` scaled by how much it resists.
    /// Returns the damage taken and whether it died.
    pub fn strike(&mut self, damage: i32, knock_back: f32, dir: f32, crit: bool) -> (i32, bool) {
        let taken = ((damage as f64 - self.kind.defense() as f64 * 0.5).max(1.0) * if crit { 2.0 } else { 1.0 }) as i32;
        let taken = taken.max(1);
        self.hp -= taken;
        let resist = self.kind.knock_back_resist();
        if knock_back > 0.0 && resist > 0.0 {
            let mut k = knock_back * resist;
            // Soft caps past 8, 10, 12 and 14, and 16 at most.
            for (over, keep) in [(8.0, 0.9), (10.0, 0.8), (12.0, 0.7), (14.0, 0.6)] {
                if k > over {
                    k = over + (k - over) * keep;
                }
            }
            k = k.min(16.0);
            if crit {
                k *= 1.4;
            }
            // The body moves in pixels per second; the game's knockback is per tick.
            let (mut vx, mut vy) = (self.body.vx / 60.0, self.body.vy / 60.0);
            let lift = if self.kind.flies() { 0.5 } else { 0.75 };
            if taken * 10 > self.kind.max_hp() {
                if dir < 0.0 && vx > -k {
                    if vx > 0.0 {
                        vx -= k;
                    }
                    vx = (vx - k).max(-k);
                } else if dir > 0.0 && vx < k {
                    if vx < 0.0 {
                        vx += k;
                    }
                    vx = (vx + k).min(k);
                }
                let ky = -k * lift;
                if vy > ky {
                    vy = (vy + ky).max(ky);
                }
            } else {
                vy = -k * lift * resist;
                vx = k * dir * resist;
            }
            (self.body.vx, self.body.vy) = (vx * 60.0, vy * 60.0);
            self.stun = STUN;
        }
        (taken, self.hp <= 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NO_TILE;
    use formats::wld::Tile;

    /// Floor at y = 40, a two-tile wall at x = 30.
    fn world() -> World {
        World::parse(&formats::synth::write_world("m", 64, 64, &[], |x, y| Tile {
            tile: if y >= 40 || (x == 30 && (38..40).contains(&y)) { 1 } else { NO_TILE },
            ..Tile::default()
        }))
        .unwrap()
    }

    fn run(m: &mut Mob, w: &World, target: (f32, f32), secs: f32) {
        for _ in 0..(secs * 60.0) as u32 {
            m.update(w, target, 1.0 / 60.0);
        }
    }

    #[test]
    fn slime_hops_toward_target() {
        let w = world();
        let mut m = Mob::new(Kind::Slime, 10.0 * 16.0, 40.0 * 16.0);
        let x0 = m.body.x;
        run(&mut m, &w, (25.0 * 16.0, 0.0), 6.0);
        assert!(m.body.x > x0 + 48.0, "{} -> {}", x0, m.body.x);
        assert!(m.body.x + m.body.w <= 30.0 * 16.0 + 0.01, "the wall stops it");
    }

    #[test]
    fn zombie_walks_and_jumps_the_wall() {
        let w = world();
        let mut m = Mob::new(Kind::Zombie, 20.0 * 16.0, 40.0 * 16.0);
        run(&mut m, &w, (50.0 * 16.0, 0.0), 6.0);
        assert!(m.body.x > 31.0 * 16.0, "over the wall: {}", m.body.x);
        assert!(m.body.on_ground);
        assert!(!m.facing_left);
    }

    #[test]
    fn demon_eye_flies_to_target() {
        let w = world();
        let mut m = Mob::new(Kind::DemonEye, 10.0 * 16.0, 20.0 * 16.0);
        let target = (20.0 * 16.0, 25.0 * 16.0);
        run(&mut m, &w, target, 3.0);
        let (cx, cy) = m.body.center();
        assert!((cx - target.0).hypot(cy - target.1) < 64.0, "{cx},{cy}");
    }

    #[test]
    fn guide_stays_home() {
        let w = world();
        let mut m = Mob::new(Kind::Guide, 15.0 * 16.0, 40.0 * 16.0);
        run(&mut m, &w, (60.0 * 16.0, 0.0), 30.0);
        assert!((m.body.x - m.home).abs() <= GUIDE_RANGE + 16.0);
        assert!(m.body.on_ground);
    }

    #[test]
    fn boss_hovers_then_charges() {
        let w = world();
        let target = (32.0 * 16.0, 39.0 * 16.0);
        let mut m = Mob::new(Kind::EyeBoss, 10.0 * 16.0, 10.0 * 16.0);
        run(&mut m, &w, target, 2.5);
        let (cx, cy) = m.body.center();
        assert!((cx - target.0).abs() < 32.0 && (cy - (target.1 - BOSS_HOVER.1)).abs() < 32.0, "{cx},{cy}");
        assert_eq!(m.state, 0);
        run(&mut m, &w, target, 1.0);
        assert!(m.state > 0, "charging");
        assert!(m.body.vy > 300.0, "toward the target below");
        assert!(!m.strike(1500, 5.0, -1.0, false).1 && m.body.vy > 300.0, "no knockback");
        assert!(m.enraged());
    }

    #[test]
    fn hits_knock_back_and_kill() {
        let mut m = Mob::new(Kind::Slime, 100.0, 100.0);
        // 10 less half of 2 defense; 9 is over a tenth of 25 life, so a full 5 back and up.
        assert_eq!(m.strike(10, 5.0, 1.0, false), (9, false));
        assert_eq!((m.body.vx, m.body.vy), (300.0, -225.0));
        assert!(m.stun > 0.0);
        let mut z = Mob::new(Kind::Zombie, 100.0, 100.0);
        assert_eq!(z.strike(4, 5.0, -1.0, false), (1, false), "at least 1");
        assert_eq!((z.body.vx, z.body.vy), (-75.0, -56.25), "a light hit: scaled by the resist twice");
        assert_eq!(m.strike(9, 0.0, 1.0, true), (16, true), "crits double");
    }
}
