//! Monsters, ticked at 60 Hz as `NPC.UpdateNPC` does: slimes, zombies and
//! demon eyes run the game's AIs; the guide and the eye boss keep a simple
//! behaviour of their own.

use crate::body::{Body, GRAVITY, MAX_FALL};
use crate::collision::{self, V2};
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

const GUIDE_SPEED: f32 = 50.0;
const ZOMBIE_JUMP: f32 = 400.0;
/// The guide stays this close to home.
const GUIDE_RANGE: f32 = 20.0 * 16.0;
const BOSS_HOVER: (f32, f32) = (3.0, 260.0);
const BOSS_DASH: (f32, f32) = (0.9, 520.0);
/// Seconds a hit monster is drawn hurt.
const STUN: f32 = 0.3;
const TICK: f32 = 1.0 / 60.0;
/// The game turns the eye with 3.14, not pi.
#[allow(clippy::approx_constant)]
const HALF_TURN: f32 = 3.14;
const HONEY: u8 = formats::wld::Liquid::Honey as u8;
const SHIMMER: u8 = formats::wld::Liquid::Shimmer as u8;

/// What the AIs read of the world clock.
#[derive(Clone, Copy, Debug)]
pub struct Sky {
    pub day: bool,
    /// `Main.worldSurface`, in tiles.
    pub surface: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Mob {
    pub kind: Kind,
    /// Velocity in pixels per tick.
    pub body: Body,
    pub hp: i32,
    pub facing_left: bool,
    /// Seconds alive: the guide's and the boss's behaviour clock.
    pub timer: f32,
    /// Seconds left drawn hurt after a hit.
    pub stun: f32,
    /// Behaviour step: guide walk direction, boss charges left.
    pub state: i32,
    /// Where it appeared, for the guide's home.
    pub home: f32,
    /// Ticks before the player's swing can hit it again.
    pub melee_cooldown: i32,
    pub ai: [f32; 4],
    pub direction: i32,
    pub direction_y: i32,
    pub sprite_dir: i32,
    old_vel: V2,
    old_x: f32,
    collide_x: bool,
    collide_y: bool,
    wet: Option<u8>,
    stair_fall: bool,
    /// Drawn offset after a step up, caught up over a few ticks.
    pub gfx_off_y: f32,
    step_speed: f32,
    just_hit: bool,
    ai_action: i32,
    pub frame: u32,
    frame_counter: f32,
    pub rotation: f32,
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
            ai: [0.0; 4],
            direction: 0,
            direction_y: 0,
            sprite_dir: -1,
            old_vel: [0.0; 2],
            old_x: x - w / 2.0,
            collide_x: false,
            collide_y: false,
            wet: None,
            stair_fall: false,
            gfx_off_y: 0.0,
            step_speed: 0.0,
            just_hit: false,
            ai_action: 0,
            frame: 0,
            frame_counter: 0.0,
            rotation: 0.0,
        }
    }

    /// Below half life: the boss's second phase.
    pub fn enraged(&self) -> bool {
        self.kind == Kind::EyeBoss && self.hp * 2 < self.kind.max_hp()
    }

    /// One tick against the player whose box is `target` (x, y, w, h).
    pub fn tick(&mut self, world: &World, target: [f32; 4], sky: Sky) {
        self.timer += TICK;
        self.stun = (self.stun - TICK).max(0.0);
        match self.kind {
            Kind::Guide | Kind::EyeBoss => {
                let b = &mut self.body;
                (b.vx, b.vy) = (b.vx * 60.0, b.vy * 60.0);
                self.wander(world, (target[0] + target[2] / 2.0, target[1] + target[3] / 2.0));
                let b = &mut self.body;
                (b.vx, b.vy) = (b.vx / 60.0, b.vy / 60.0);
            }
            _ => self.update_npc(world, target, sky),
        }
        self.just_hit = false;
    }

    /// `NPC.UpdateNPC`: gravity, the AI, movement against tiles, the frame.
    fn update_npc(&mut self, world: &World, target: [f32; 4], sky: Sky) {
        let (gravity, max_fall) = self.gravity(world, sky);
        let k = (1.0 + self.body.vx.abs() / 3.0) * self.step_speed;
        self.gfx_off_y =
            if self.gfx_off_y > 0.0 { (self.gfx_off_y - k).max(0.0) } else { (self.gfx_off_y + k).min(0.0) };
        self.gfx_off_y = self.gfx_off_y.clamp(-16.0, 16.0);
        match self.kind {
            Kind::Slime => self.slime(world, target, sky),
            Kind::Zombie => self.fighter(world, target, sky),
            _ => self.floating_eye(target, sky),
        }
        let b = &mut self.body;
        if self.kind != Kind::DemonEye {
            b.vy = (b.vy + gravity).min(max_fall);
        }
        if b.vx.abs() < 0.005 {
            b.vx = 0.0;
        }
        self.collide(world, gravity);
        self.find_frame();
    }

    /// `UpdateNPC_UpdateGravity`: weaker high in the sky, and in liquids.
    fn gravity(&self, world: &World, sky: Sky) -> (f32, f32) {
        let n = (world.width as f32 / 4200.0).powi(2);
        let k = ((self.body.y / 16.0 - (60.0 + 10.0 * n)) / (sky.surface / 6.0)).clamp(0.25, 1.0);
        match self.wet {
            None => (0.3 * k, 10.0),
            Some(SHIMMER) => (0.15, 5.5),
            Some(HONEY) => (0.1, 4.0),
            Some(_) => (0.2, 7.0),
        }
    }

    fn at_box<'a>(&self, world: &'a World) -> collision::Box<'a> {
        let b = &self.body;
        collision::Box { world, pos: [b.x, b.y], size: [b.w as i32, b.h as i32] }
    }

    /// `UpdateCollision`: slopes, liquids, tiles, then stairs.
    fn collide(&mut self, world: &World, gravity: f32) {
        let v = self.at_box(world).walk_down_slope([self.body.vx, self.body.vy], gravity);
        let b = self.at_box(world);
        self.wet = collision::wet(world, b.pos, b.size);
        let fall = self.kind == Kind::DemonEye || (self.kind == Kind::Zombie && self.direction_y == 1);
        self.old_vel = v;
        let hit = self.at_box(world).tiles(v, fall, fall);
        let mut vel = hit.vel;
        if hit.up {
            vel[1] = 0.01;
        }
        let mut step = vel;
        if let Some(kind) = self.wet {
            let k = match kind {
                HONEY => 0.25,
                SHIMMER => 0.375,
                _ => 0.5,
            };
            for a in 0..2 {
                if vel[a] == v[a] {
                    step[a] *= k;
                }
            }
        }
        self.collide_x = vel[0] != v[0];
        self.collide_y = vel[1] != v[1];
        self.old_x = self.body.x;
        let b = &mut self.body;
        (b.x, b.y, b.vx, b.vy) = (b.x + step[0], b.y + step[1], vel[0], vel[1]);

        if fall {
            self.stair_fall = true;
        }
        let (pos, vel, stair, stair_fall, _) = self.at_box(world).slopes([self.body.vx, self.body.vy], self.stair_fall);
        if stair_fall {
            self.stair_fall = true;
        } else if !fall {
            self.stair_fall = false;
        }
        let b = &mut self.body;
        if stair && (pos[1] - b.y).abs() > 8.0 {
            self.gfx_off_y -= pos[1] - b.y;
            self.step_speed = 2.0;
        }
        (b.x, b.y, b.vx, b.vy) = (pos[0], pos[1], vel[0], vel[1]);
        b.on_ground = b.vy == 0.0;
    }

    /// `TargetClosest`: faces the target's middle.
    fn face(&mut self, target: [f32; 4]) {
        let (cx, cy) = self.body.center();
        let (tx, ty) = (target[0] as i32 + target[2] as i32 / 2, target[1] as i32 + target[3] as i32 / 2);
        self.direction = if (tx as f32) < cx { -1 } else { 1 };
        self.direction_y = if (ty as f32) < cy { -1 } else { 1 };
    }

    fn solid_collision(&self, world: &World) -> bool {
        self.at_box(world).solid_collision()
    }

    /// `AI_001_Slimes`: waits on the ground, then two small hops and a big one.
    fn slime(&mut self, world: &World, target: [f32; 4], sky: Sky) {
        let aggressive = !sky.day || self.hp != self.kind.max_hp() || self.body.y > sky.surface * 16.0;
        let ai = &mut self.ai;
        if ai[2] > 1.0 {
            ai[2] -= 1.0;
        }
        if self.wet.is_some() {
            let b = &mut self.body;
            if self.collide_y {
                b.vy = -2.0;
            }
            if b.vy < 0.0 && ai[3] == b.x {
                self.direction *= -1;
                ai[2] = 200.0;
            }
            if b.vy > 0.0 {
                ai[3] = b.x;
            }
            if b.vy > 2.0 {
                b.vy *= 0.9;
            }
            b.vy = (b.vy - 0.5).max(-4.0);
            if ai[2] == 1.0 && aggressive {
                self.face(target);
            }
        }
        self.ai_action = 0;
        if self.ai[2] == 0.0 {
            self.ai[0] = -100.0;
            self.ai[2] = 1.0;
            self.face(target);
        }
        let dir = self.direction as f32;
        if self.body.vy == 0.0 {
            if self.collide_y && self.old_vel[1] != 0.0 && self.solid_collision(world) {
                self.body.x -= self.body.vx + dir;
            }
            if self.ai[3] == self.body.x {
                self.direction *= -1;
                self.ai[2] = 200.0;
            }
            self.ai[3] = 0.0;
            let b = &mut self.body;
            b.vx *= 0.8;
            if b.vx > -0.1 && b.vx < 0.1 {
                b.vx = 0.0;
            }
            let ai = &mut self.ai;
            if aggressive {
                ai[0] += 1.0;
            }
            ai[0] += 1.0;
            let n = -1000.0;
            let jump = if ai[0] >= n * 2.0 && ai[0] <= n * 1.5 {
                3
            } else if ai[0] >= n && ai[0] <= n * 0.5 {
                2
            } else if ai[0] >= 0.0 {
                1
            } else {
                0
            };
            if jump > 0 {
                if aggressive && self.ai[2] == 1.0 {
                    self.face(target);
                }
                let dir = self.direction as f32;
                let b = &mut self.body;
                if jump == 3 {
                    b.vy = -8.0;
                    b.vx += 3.0 * dir;
                    self.ai[0] = -200.0;
                    self.ai[3] = b.x;
                } else {
                    b.vy = -6.0;
                    b.vx += 2.0 * dir;
                    self.ai[0] = -120.0 + if jump == 1 { n } else { n * 2.0 };
                }
            } else if self.ai[0] >= -30.0 {
                self.ai_action = 1;
            }
        } else if (self.direction == 1 && self.body.vx < 3.0) || (self.direction == -1 && self.body.vx > -3.0) {
            if self.collide_x && self.body.vx.abs() == 0.2 {
                self.body.x -= 1.4 * dir;
            }
            if self.collide_y && self.old_vel[1] != 0.0 && self.solid_collision(world) {
                self.body.x -= self.body.vx + dir;
            }
            let b = &mut self.body;
            if (self.direction == -1 && b.vx < 0.01) || (self.direction == 1 && b.vx > -0.01) {
                b.vx += 0.2 * dir;
            } else {
                b.vx *= 0.93;
            }
        }
    }

    /// `AI_002_FloatingEye`: bounces off blocks and swoops at the target;
    /// flies away by day above ground.
    fn floating_eye(&mut self, target: [f32; 4], sky: Sky) {
        let b = &mut self.body;
        if self.collide_x {
            b.vx = self.old_vel[0] * -0.5;
            if self.direction == -1 && b.vx > 0.0 && b.vx < 2.0 {
                b.vx = 2.0;
            }
            if self.direction == 1 && b.vx < 0.0 && b.vx > -2.0 {
                b.vx = -2.0;
            }
        }
        if self.collide_y {
            b.vy = self.old_vel[1] * -0.5;
            if b.vy > 0.0 && b.vy < 1.0 {
                b.vy = 1.0;
            }
            if b.vy < 0.0 && b.vy > -1.0 {
                b.vy = -1.0;
            }
        }
        if sky.day && b.y <= sky.surface * 16.0 {
            self.direction_y = -1;
            self.direction = if b.vx > 0.0 { 1 } else { -1 };
        } else {
            self.face(target);
        }
        let (sx, sy) = (4.0, 1.5);
        let b = &mut self.body;
        if self.direction == -1 && b.vx > -sx {
            b.vx -= 0.1;
            if b.vx > sx {
                b.vx -= 0.1;
            } else if b.vx > 0.0 {
                b.vx += 0.05;
            }
            b.vx = b.vx.max(-sx);
        } else if self.direction == 1 && b.vx < sx {
            b.vx += 0.1;
            if b.vx < -sx {
                b.vx += 0.1;
            } else if b.vx < 0.0 {
                b.vx -= 0.05;
            }
            b.vx = b.vx.min(sx);
        }
        if self.direction_y == -1 && b.vy > -sy {
            b.vy -= 0.04;
            if b.vy > sy {
                b.vy -= 0.05;
            } else if b.vy > 0.0 {
                b.vy += 0.03;
            }
            b.vy = b.vy.max(-sy);
        } else if self.direction_y == 1 && b.vy < sy {
            b.vy += 0.04;
            if b.vy < -sy {
                b.vy += 0.05;
            } else if b.vy < 0.0 {
                b.vy -= 0.03;
            }
            b.vy = b.vy.min(sy);
        }
        if self.wet.is_some() {
            if b.vy > 0.0 {
                b.vy *= 0.95;
            }
            b.vy = (b.vy - 0.5).max(-4.0);
            self.face(target);
        }
    }

    /// `AI_003_Fighters` for the zombie: walks at the target, steps up single
    /// blocks and jumps higher walls; wanders off when stuck or by day.
    /// Doors are not handled.
    fn fighter(&mut self, world: &World, target: [f32; 4], sky: Sky) {
        let [tx, ty, tw, th] = target;
        let b = self.body;
        if ty + th == b.y + b.h {
            self.direction_y = -1;
        }
        let mut can_jump = false;
        let still = b.vx == 0.0 && !self.just_hit;
        let turning = b.vy == 0.0 && ((b.vx > 0.0 && self.direction < 0) || (b.vx < 0.0 && self.direction > 0));
        let ai = &mut self.ai;
        if b.x == self.old_x || ai[3] >= 60.0 || turning {
            ai[3] += 1.0;
        } else if b.vx.abs() > 0.9 && ai[3] > 0.0 {
            ai[3] -= 1.0;
        }
        if ai[3] > 600.0 || self.just_hit {
            ai[3] = 0.0;
        }
        let rect = |x: f32, y: f32, w: f32, h: f32| (x as i32, y as i32, w as i32, h as i32);
        let (p, m) = (rect(tx, ty, tw, th), rect(b.x, b.y, b.w, b.h));
        if p.0 < m.0 + m.2 && m.0 < p.0 + p.2 && p.1 < m.1 + m.3 && m.1 < p.1 + p.3 {
            ai[3] = 0.0;
        }
        let discouraged = sky.day && b.y <= sky.surface * 16.0;
        if self.ai[3] < 60.0 && !discouraged {
            self.face(target);
            if self.direction_y > 0 && ty + th / 2.0 <= b.y + b.h {
                self.direction_y = -1;
            }
        } else {
            if b.vx == 0.0 {
                if b.vy == 0.0 {
                    self.ai[0] += 1.0;
                    if self.ai[0] >= 2.0 {
                        self.direction *= -1;
                        self.sprite_dir = self.direction;
                        self.ai[0] = 0.0;
                    }
                }
            } else {
                self.ai[0] = 0.0;
            }
            if self.direction == 0 {
                self.direction = 1;
            }
        }
        let max = 1.0;
        let b = &mut self.body;
        if b.vx < -max || b.vx > max {
            if b.vy == 0.0 {
                (b.vx, b.vy) = (b.vx * 0.8, b.vy * 0.8);
            }
        } else if b.vx < max && self.direction == 1 {
            b.vx = (b.vx + 0.07).min(max);
        } else if b.vx > -max && self.direction == -1 {
            b.vx = (b.vx - 0.07).max(-max);
        }

        let (w, h) = (b.w, b.h);
        if b.vy == 0.0 {
            let below = (b.y + h + 7.0) as i64 / 16;
            let above = (b.y - 9.0) as i64 / 16;
            let (x0, x1) = (b.x as i64 / 16, (b.x + w) as i64 / 16);
            for i in (b.x + 8.0) as i64 / 16..=(b.x + w - 8.0) as i64 / 16 {
                if collision::solid_no_platforms(world, i, above) {
                    can_jump = false;
                    break;
                }
                let (active, solid, ..) = collision::shape(world, i, below);
                if i >= x0 && i <= x1 && active && solid {
                    can_jump = true;
                }
            }
            if !can_jump && b.vy < 0.0 {
                b.vy = 0.0;
            }
        }

        if b.vy >= 0.0 {
            let dir = (b.vx > 0.0) as i64 - (b.vx < 0.0) as i64;
            let px = b.x + b.vx;
            let half = (w as i32 / 2) as f32;
            let col = ((px + half + ((w as i32 / 2 + 1) as i64 * dir) as f32) / 16.0) as i64;
            let row = ((b.y + h - 1.0) / 16.0) as i64;
            let inside = col >= 4 && row >= 4 && col < world.width as i64 - 4 && row < world.height as i64 - 4;
            let s = |x: i64, y: i64| collision::shape(world, x, y);
            let open = |x: i64, y: i64| {
                let (a, so, top, ..) = s(x, y);
                !a || !so || top
            };
            let (t0, t1) = (s(col, row), s(col, row - 1));
            let (back_active, back_solid, ..) = s(col - dir, row - 3);
            if inside
                && ((col * 16) as f32) < px + w
                && ((col * 16 + 16) as f32) > px
                && ((t0.0 && !t0.4 && !t1.4 && t0.1 && !t0.2) || (t1.3 && t1.0))
                && (!t1.0 || !t1.1 || t1.2 || (t1.3 && open(col, row - 4)))
                && open(col, row - 2)
                && open(col, row - 3)
                && (!back_active || !back_solid)
            {
                let mut top = (row * 16) as f32;
                if t0.3 {
                    top += 8.0;
                }
                if t1.3 {
                    top -= 8.0;
                }
                if top < b.y + h {
                    let d = b.y + h - top;
                    if d <= 16.1 {
                        self.gfx_off_y += b.y + h - top;
                        b.y = top - h;
                        self.step_speed = if d < 9.0 { 1.0 } else { 2.0 };
                    }
                }
            }
        }

        if can_jump {
            let col = ((b.x + (w as i32 / 2) as f32 + (15 * self.direction) as f32) / 16.0) as i64;
            let row = ((b.y + h - 15.0) / 16.0) as i64;
            let wall = |y: i64| collision::solid_no_platforms(world, col, y);
            if (b.vx < 0.0 && self.sprite_dir == -1) || (b.vx > 0.0 && self.sprite_dir == 1) {
                if h >= 32.0 && wall(row - 2) {
                    b.vy = if wall(row - 3) { -8.0 } else { -7.0 };
                } else if wall(row - 1) {
                    b.vy = -6.0;
                } else if b.y + h - (row * 16) as f32 > 20.0 && !collision::shape(world, col, row).4 && wall(row) {
                    b.vy = -5.0;
                } else if self.direction_y < 0
                    && !collision::solid_allow_bottom_slope(world, col, row + 1)
                    && !collision::solid_allow_bottom_slope(world, col + self.direction as i64, row + 1)
                {
                    b.vy = -8.0;
                    b.vx *= 1.5;
                } else {
                    (self.ai[1], self.ai[2]) = (0.0, 0.0);
                }
                if b.vy == 0.0 && still && self.ai[3] == 1.0 {
                    b.vy = -5.0;
                }
            }
        } else {
            (self.ai[1], self.ai[2]) = (0.0, 0.0);
        }
    }

    /// `FindFrame` for the three monsters.
    pub fn find_frame(&mut self) {
        let b = &self.body;
        match self.kind {
            Kind::Slime => {
                let pose = if self.ai_action == 1 {
                    4
                } else if b.vy != 0.0 || b.vx != 0.0 {
                    1
                } else {
                    0
                };
                self.frame_counter += 1.0 + (pose > 0) as u8 as f32 + (pose == 4) as u8 as f32;
                if self.frame_counter >= 8.0 {
                    self.frame += 1;
                    self.frame_counter = 0.0;
                }
                if self.frame >= 2 {
                    self.frame = 0;
                }
            }
            Kind::DemonEye => {
                if b.vx > 0.0 {
                    self.sprite_dir = 1;
                    self.rotation = b.vy.atan2(b.vx);
                }
                if b.vx < 0.0 {
                    self.sprite_dir = -1;
                    self.rotation = b.vy.atan2(b.vx) + HALF_TURN;
                }
                self.frame_counter += 1.0;
                if self.frame_counter >= 8.0 {
                    self.frame += 1;
                    self.frame_counter = 0.0;
                }
                if self.frame >= 2 {
                    self.frame = 0;
                }
            }
            Kind::Zombie => {
                if b.vy == 0.0 && self.direction != 0 {
                    self.sprite_dir = self.direction;
                }
                if b.vy != 0.0 || (self.direction == -1 && b.vx > 0.0) || (self.direction == 1 && b.vx < 0.0) {
                    self.frame_counter = 0.0;
                    self.frame = 2;
                } else if b.vx == 0.0 {
                    self.frame_counter = 0.0;
                    self.frame = 0;
                } else {
                    self.frame_counter += b.vx.abs();
                    self.frame = match self.frame_counter {
                        c if c < 8.0 => 0,
                        c if c < 16.0 => 1,
                        c if c < 24.0 => 2,
                        c if c < 32.0 => 1,
                        _ => {
                            self.frame_counter = 0.0;
                            self.frame
                        }
                    };
                }
            }
            _ => {}
        }
        self.facing_left = self.sprite_dir < 0;
    }

    /// The guide strolls around his home; the boss hovers over the target,
    /// then charges it three times, faster once below half life. Velocities
    /// in pixels per second here.
    fn wander(&mut self, world: &World, target: (f32, f32)) {
        let b = &mut self.body;
        let dt = TICK;
        if self.kind == Kind::Guide {
            // Walk a few seconds, rest a few seconds, turn home when too far.
            if self.timer >= 2.5 {
                self.timer = 0.0;
                self.state = if self.state != 0 {
                    0
                } else if b.x < self.home {
                    1
                } else {
                    -1
                };
            }
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
            let k = if b.in_liquid(world) { 0.5 } else { 1.0 };
            b.vy = (b.vy + GRAVITY * k * dt).min(MAX_FALL * k);
            b.step(world, dt, false);
            return;
        }
        let fast = if self.enraged() { 1.4 } else { 1.0 };
        let b = &mut self.body;
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
        // Flies through blocks.
        b.x += b.vx * dt;
        b.y += b.vy * dt;
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
            let (mut vx, mut vy) = (self.body.vx, self.body.vy);
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
            (self.body.vx, self.body.vy) = (vx, vy);
        }
        self.stun = STUN;
        self.just_hit = true;
        (taken, self.hp <= 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NO_TILE;
    use formats::wld::Tile;

    const NIGHT: Sky = Sky { day: false, surface: 200.0 };

    /// Floor at y = 100, a two-tile wall at x = 30, a step at x = 40.
    fn world() -> World {
        World::parse(&formats::synth::write_world("m", 64, 200, &[], |x, y| Tile {
            tile: if y >= 100 || (x == 30 && (98..100).contains(&y)) || (x == 40 && y == 99) { 1 } else { NO_TILE },
            ..Tile::default()
        }))
        .unwrap()
    }

    /// A player's box around `(x, y)`.
    fn at(x: f32, y: f32) -> [f32; 4] {
        [x - 10.0, y - 21.0, 20.0, 42.0]
    }

    fn run(m: &mut Mob, w: &World, target: [f32; 4], sky: Sky, secs: f32) {
        for _ in 0..(secs * 60.0) as u32 {
            m.tick(w, target, sky);
        }
    }

    #[test]
    fn slime_hops_twice_then_leaps() {
        let w = world();
        let mut m = Mob::new(Kind::Slime, 5.0 * 16.0, 100.0 * 16.0);
        let x0 = m.body.x;
        let mut jumps = Vec::new();
        for t in 0..1200 {
            let grounded = m.body.vy == 0.0;
            m.tick(&w, at(25.0 * 16.0, 99.0 * 16.0), NIGHT);
            if grounded && m.body.vy < 0.0 {
                jumps.push((t, m.body.vy));
            }
        }
        // At night: 2 a tick from -100, so 50 ticks before the first hop.
        assert_eq!(jumps[0], (49, -6.0 + 0.3));
        assert_eq!(jumps[1].1, -5.7);
        assert_eq!(jumps[2].1, -8.0 + 0.3, "{jumps:?}");
        assert!(m.body.x > x0 + 48.0, "{} -> {}", x0, m.body.x);
        assert!(m.body.x + m.body.w <= 30.0 * 16.0 + 0.01, "the wall stops it");
    }

    #[test]
    fn zombie_steps_up_and_jumps_the_wall() {
        let w = world();
        let mut m = Mob::new(Kind::Zombie, 20.0 * 16.0, 100.0 * 16.0);
        run(&mut m, &w, at(55.0 * 16.0, 99.0 * 16.0), NIGHT, 4.0);
        assert!(m.body.x > 31.0 * 16.0, "over the wall: {}", m.body.x);
        run(&mut m, &w, at(55.0 * 16.0, 99.0 * 16.0), NIGHT, 4.0);
        assert!(m.body.x > 41.0 * 16.0, "up the step: {}", m.body.x);
        assert!(m.body.on_ground);
        assert!(!m.facing_left);
        assert!(m.body.vx > 0.9 && m.body.vx <= 1.0, "walks at 1: {}", m.body.vx);
    }

    #[test]
    fn demon_eye_swoops_at_night_and_leaves_by_day() {
        let w = world();
        let start = (10.0 * 16.0, 70.0 * 16.0);
        let mut m = Mob::new(Kind::DemonEye, start.0, start.1);
        let target = (20.0 * 16.0, 80.0 * 16.0);
        let (mut closest, mut span) = (f32::MAX, (f32::MAX, f32::MIN));
        for _ in 0..600 {
            m.tick(&w, at(target.0, target.1), NIGHT);
            let (cx, cy) = m.body.center();
            closest = closest.min((cx - target.0).hypot(cy - target.1));
            span = (span.0.min(cx), span.1.max(cx));
            assert!(m.body.vx.abs() <= 4.0 && m.body.vy.abs() <= 1.5 + 1e-4);
        }
        assert!(closest < 64.0, "{closest}");
        assert!(span.0 < target.0 - 100.0 && span.1 > target.0 + 100.0, "swoops past on both sides: {span:?}");
        let y = m.body.y;
        run(&mut m, &w, at(target.0, target.1), Sky { day: true, ..NIGHT }, 6.0);
        assert!(m.body.y < y - 100.0, "flies up by day: {y} -> {}", m.body.y);
    }

    #[test]
    fn guide_stays_home() {
        let w = world();
        let mut m = Mob::new(Kind::Guide, 15.0 * 16.0, 100.0 * 16.0);
        run(&mut m, &w, at(60.0 * 16.0, 99.0 * 16.0), NIGHT, 30.0);
        assert!((m.body.x - m.home).abs() <= GUIDE_RANGE + 16.0);
        assert!(m.body.on_ground);
    }

    #[test]
    fn boss_hovers_then_charges() {
        let w = world();
        let target = (32.0 * 16.0, 99.0 * 16.0);
        let mut m = Mob::new(Kind::EyeBoss, 10.0 * 16.0, 70.0 * 16.0);
        run(&mut m, &w, at(target.0, target.1), NIGHT, 2.5);
        let (cx, cy) = m.body.center();
        assert!((cx - target.0).abs() < 32.0 && (cy - (target.1 - BOSS_HOVER.1)).abs() < 32.0, "{cx},{cy}");
        assert_eq!(m.state, 0);
        run(&mut m, &w, at(target.0, target.1), NIGHT, 1.0);
        assert!(m.state > 0, "charging");
        assert!(m.body.vy > 5.0, "toward the target below");
        assert!(!m.strike(1500, 5.0, -1.0, false).1 && m.body.vy > 5.0, "no knockback");
        assert!(m.enraged());
    }

    #[test]
    fn hits_knock_back_and_kill() {
        let mut m = Mob::new(Kind::Slime, 100.0, 100.0);
        // 10 less half of 2 defense; 9 is over a tenth of 25 life, so a full 5 back and up.
        assert_eq!(m.strike(10, 5.0, 1.0, false), (9, false));
        assert_eq!((m.body.vx, m.body.vy), (5.0, -3.75));
        assert!(m.stun > 0.0 && m.just_hit);
        let mut z = Mob::new(Kind::Zombie, 100.0, 100.0);
        assert_eq!(z.strike(4, 5.0, -1.0, false), (1, false), "at least 1");
        assert_eq!((z.body.vx, z.body.vy), (-1.25, -0.9375), "a light hit: scaled by the resist twice");
        assert_eq!(m.strike(9, 0.0, 1.0, true), (16, true), "crits double");
    }
}
