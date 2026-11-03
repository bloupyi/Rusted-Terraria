//! Monsters: a [`Body`] with a simple behaviour toward a target point.
//!
//! Slimes wait on the ground, then hop toward the target. Zombies walk toward
//! it and jump when a wall stops them.

use crate::body::{Body, GRAVITY, MAX_FALL};
use crate::World;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Slime,
    Zombie,
}

impl Kind {
    pub fn size(self) -> (f32, f32) {
        match self {
            Kind::Slime => (28.0, 20.0),
            Kind::Zombie => (18.0, 40.0),
        }
    }

    pub fn max_hp(self) -> i32 {
        match self {
            Kind::Slime => 25,
            Kind::Zombie => 45,
        }
    }

    /// Damage dealt to the player on contact.
    pub fn damage(self) -> i32 {
        match self {
            Kind::Slime => 7,
            Kind::Zombie => 14,
        }
    }
}

const SLIME_WAIT: f32 = 1.2;
const SLIME_HOP: (f32, f32) = (130.0, 380.0);
const ZOMBIE_SPEED: f32 = 70.0;
const ZOMBIE_JUMP: f32 = 400.0;
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
        }
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
            }
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

    /// Takes damage with a knockback away from `from_x`. Returns true when it dies.
    pub fn hit(&mut self, damage: i32, from_x: f32) -> bool {
        self.hp -= damage;
        let away = if self.body.center().0 < from_x { -1.0 } else { 1.0 };
        (self.body.vx, self.body.vy) = (away * 220.0, -200.0);
        self.stun = STUN;
        self.hp <= 0
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
    fn hits_knock_back_and_kill() {
        let mut m = Mob::new(Kind::Slime, 100.0, 100.0);
        assert!(!m.hit(10, 0.0));
        assert!(m.body.vx > 0.0 && m.stun > 0.0);
        assert!(m.hit(20, 0.0));
    }
}
