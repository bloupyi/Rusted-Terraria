//! The critters' AIs and frames: the walkers of `AI_007_TownEntities`, the
//! birds' `aiStyle` 24, the fish's 16, the fireflies' 64, `AI_065_Butterflies`,
//! the worms' 66 and the snails' 67.

use super::{Kind, Mob, Sky};
use crate::collision;
use crate::World;

/// `Main.tileSolid` of a present block, platforms too.
fn solid(world: &World, x: i64, y: i64) -> bool {
    let (active, solid, ..) = collision::shape(world, x, y);
    active && solid
}

/// A present block solid all over, not only on top.
fn block(world: &World, x: i64, y: i64) -> bool {
    let (active, solid, top, ..) = collision::shape(world, x, y);
    active && solid && !top
}

/// `Collision.SolidTiles`, `x0..=x1` either way round (`SolidTilesVersatile`).
fn solid_tiles(world: &World, x0: i64, x1: i64, y0: i64, y1: i64) -> bool {
    let (x0, x1) = (x0.min(x1), x0.max(x1));
    (x0..=x1).any(|x| (y0..=y1).any(|y| block(world, x, y)))
}

/// `Rectangle.Distance` of a box to a point.
fn box_distance(b: [f32; 4], p: (f32, f32)) -> f32 {
    let dx = (b[0] - p.0).max(p.0 - (b[0] + b[2])).max(0.0);
    let dy = (b[1] - p.1).max(p.1 - (b[1] + b[3])).max(0.0);
    dx.hypot(dy)
}

impl Mob {
    fn center_tile(&self) -> (i64, i64) {
        let (cx, cy) = self.body.center();
        ((cx / 16.0) as i64, (cy / 16.0) as i64)
    }

    /// `AI_007_TownEntities` as it runs for the town critters: stroll a while
    /// and rest a while near where they first stood, hop up steps, and run
    /// from a hostile monster in sight within 200 px.
    pub(super) fn critter(&mut self, world: &World, target: [f32; 4], sky: Sky, threats: &[[f32; 4]]) {
        if !self.targeted {
            self.face(target);
            self.sprite_dir = self.direction;
            self.home = (self.body.center().0 / 16.0).floor();
        }
        let mouse = self.kind == Kind::Mouse;
        let size = [self.body.w as i32, self.body.h as i32];
        let (cx, cy) = self.body.center();
        let tile_x = ((self.body.x + (self.body.w as i32 / 2) as f32) / 16.0) as i64;
        let home = self.home as i64;
        self.direction_y = -1;
        if self.direction == 0 {
            self.direction = 1;
        }

        let (mut left, mut right, mut scared) = (None::<f32>, None::<f32>, false);
        for t in threats {
            let tc = (t[0] + t[2] / 2.0, t[1] + t[3] / 2.0);
            if (tc.0 - cx).hypot(tc.1 - cy) >= 200.0
                || !collision::can_hit(world, [cx, cy], [0, 0], [tc.0, tc.1], [0, 0])
            {
                continue;
            }
            scared = true;
            let dx = tc.0 - cx;
            if dx < 0.0 && left.is_none_or(|l| dx > l) {
                left = Some(dx);
            }
            if dx > 0.0 && right.is_none_or(|r| dx < r) {
                right = Some(dx);
            }
        }
        if scared {
            // The side the nearest danger is on.
            let side = match (left, right) {
                (None, _) => 1,
                (Some(l), Some(r)) => {
                    if r < -l {
                        1
                    } else {
                        -1
                    }
                }
                (Some(_), None) => -1,
            };
            if self.ai[0] == 8.0 {
                if self.direction == -side {
                    self.ai[0] = 1.0;
                    self.ai[1] = (300 + self.rand(300)) as f32;
                    self.ai[2] = 0.0;
                    self.local_ai[3] = 0.0;
                }
            } else if self.ai[0] != 1.0 {
                self.ai[0] = 1.0;
                self.ai[1] = (120 + self.rand(120)) as f32;
                self.ai[2] = 0.0;
                self.local_ai[3] = 0.0;
                self.direction = -side;
            } else if self.direction != -side {
                self.direction = -side;
            }
        }

        let drowning = collision::drown(world, [self.body.x, self.body.y], size);
        if self.ai[0] == 0.0 {
            if self.local_ai[3] > 0.0 {
                self.local_ai[3] -= 1.0;
            }
            let b = &mut self.body;
            if mouse {
                b.vx *= 0.5;
            }
            if b.vx > 0.1 {
                b.vx -= 0.1;
            } else if b.vx < -0.1 {
                b.vx += 0.1;
            } else {
                b.vx = 0.0;
            }
            if self.ai[1] > 0.0 {
                self.ai[1] -= 1.0;
            }
            if (self.wet.is_some() && drowning) || self.ai[1] <= 0.0 {
                self.ai[0] = 1.0;
                self.ai[1] = (200 + self.rand(300) + self.rand_range(200, 400)) as f32;
                self.ai[2] = 0.0;
                self.local_ai[3] = 0.0;
            }
            // By day they keep within 50 tiles of home, and now and then turn.
            if sky.day {
                if tile_x < home - 25 || tile_x > home + 25 {
                    if self.local_ai[3] == 0.0 {
                        if tile_x < home - 50 && self.direction == -1 {
                            self.direction = 1;
                        } else if tile_x > home + 50 && self.direction == 1 {
                            self.direction = -1;
                        }
                    }
                } else if self.rand(80) == 0 && self.local_ai[3] == 0.0 {
                    self.local_ai[3] = 200.0;
                    self.direction *= -1;
                }
            }
        } else if self.ai[0] == 1.0 {
            if !drowning {
                if tile_x < home - 35 || tile_x > home + 35 {
                    let x = self.body.x;
                    if (x < (home * 16) as f32 && self.direction == -1)
                        || (x > (home * 16) as f32 && self.direction == 1)
                    {
                        self.ai[1] -= 5.0;
                    }
                }
                self.ai[1] -= 1.0;
            }
            if self.ai[1] <= 0.0 {
                self.ai[0] = 0.0;
                self.ai[1] = (300 + self.rand(300) - self.rand(100)) as f32;
                self.ai[2] = 0.0;
                self.local_ai[3] = 60.0;
            }
            let (max, acc) = match self.kind {
                Kind::Mouse => (2.0, 1.0),
                Kind::Squirrel => (1.5, 0.07),
                _ => (1.0, 0.07),
            };
            let dir = self.direction;
            let b = &mut self.body;
            if b.vx < -max || b.vx > max {
                if b.vy == 0.0 {
                    (b.vx, b.vy) = (b.vx * 0.8, b.vy * 0.8);
                }
            } else if b.vx < max && dir == 1 {
                b.vx = (b.vx + acc).min(max);
            } else if b.vx > -max && dir == -1 {
                b.vx -= acc;
            }
            if self.body.vy >= 0.0 {
                if let Some((y, speed)) = self.at_box(world).step_up([self.body.vx, self.body.vy]) {
                    self.gfx_off_y += self.body.y - y;
                    self.body.y = y;
                    self.step_speed = speed;
                }
            }
            if self.body.vy == 0.0 {
                self.walk_obstacles(world, scared, drowning, mouse);
            }
            let b = &self.body;
            let still = b.x == self.old_x && b.y == self.old_y && b.vx == self.old_vel[0] && b.vy == 0.0;
            if still && !scared {
                self.local_ai[2] += 1.0;
                if self.local_ai[2] > 30.0 {
                    self.local_ai[2] = 0.0;
                    self.ai[0] = 1.0;
                    self.ai[1] = (200 + self.rand(300) + self.rand_range(200, 400)) as f32;
                    self.ai[2] = 0.0;
                    self.direction *= -1;
                    self.body.vx *= -1.0;
                }
            }
        } else if self.ai[0] == 8.0 {
            self.body.vx *= 0.8;
            self.ai[1] -= 1.0;
            if self.ai[1] < 60.0 && scared {
                self.ai[1] = 180.0;
            }
            if self.ai[1] <= 0.0 {
                self.ai[0] = 0.0;
                self.ai[1] = (60 + self.rand(60)) as f32;
                self.ai[2] = 0.0;
                self.local_ai[3] = (30 + self.rand(60)) as f32;
            }
        }
    }

    /// The walk's look ahead on the ground: hop up a wall of one to three
    /// blocks with room above, else turn (or freeze when scared).
    fn walk_obstacles(&mut self, world: &World, scared: bool, drowning: bool, mouse: bool) {
        let b = self.body;
        let col = ((b.x + (b.w as i32 / 2) as f32 + (15 * self.direction) as f32) / 16.0) as i64;
        let row = ((b.y + b.h - 16.0) / 16.0) as i64;
        if b.x == self.local_ai[3] {
            self.direction *= -1;
            self.local_ai[3] = 180.0;
        }
        if drowning {
            self.local_ai[3] = self.local_ai[3].min(180.0);
            if self.local_ai[3] > 0.0 {
                self.local_ai[3] -= 1.0;
            }
        } else {
            self.local_ai[3] = -1.0;
        }
        let dir = self.direction as i64;
        let moving = (b.vx < 0.0 && dir == -1) || (b.vx > 0.0 && dir == 1);
        if moving {
            let short = (b.h as i32) / 16 < 3;
            let (mut turn, mut freeze) = (false, false);
            let mut blocked = || {
                if scared {
                    freeze = true;
                }
                turn = true;
            };
            if block(world, col, row - 2) && (!short || block(world, col, row - 1)) {
                if !solid_tiles(world, col - dir * 2, col - dir, row - 5, row - 1)
                    && !solid_tiles(world, col, col, row - 5, row - 3)
                {
                    self.body.vy = -6.0;
                } else if mouse {
                    let (cx, cy) = self.center_tile();
                    if collision::solid_tile(world, cx + dir, cy) {
                        self.direction *= -1;
                        self.body.vx = 0.0;
                    }
                } else {
                    blocked();
                }
            } else if block(world, col, row - 1) {
                if !solid_tiles(world, col - dir * 2, col - dir, row - 4, row - 1)
                    && !solid_tiles(world, col, col, row - 4, row - 2)
                {
                    self.body.vy = -5.0;
                } else {
                    blocked();
                }
            } else if b.y + b.h - (row * 16) as f32 > 20.0
                && solid(world, col, row)
                && !collision::shape(world, col, row).4
            {
                if !solid_tiles(world, col - dir * 2, col, row - 3, row - 1) {
                    self.body.vy = -4.4;
                } else {
                    blocked();
                }
            }
            if freeze {
                self.body.vx = 0.0;
                self.ai[0] = 8.0;
                self.ai[1] = 240.0;
            } else if turn {
                self.direction *= -1;
                self.body.vx *= -1.0;
            }
            if self.body.vy < 0.0 {
                self.local_ai[3] = self.body.x;
            }
        }
        if self.body.vy < 0.0 && self.wet.is_some() {
            self.body.vy *= 1.2;
        }
        if self.body.vy < 0.0 && !mouse {
            self.body.vy *= 1.2;
        }
    }

    /// `aiStyle` 24: sits until the player comes within 100 px or it is hit,
    /// then flies off the other way over the ground, rising before it.
    pub(super) fn bird(&mut self, world: &World, target: [f32; 4]) {
        self.no_gravity = true;
        if self.ai[0] == 0.0 {
            self.no_gravity = false;
            self.face(target);
            let b = self.body;
            let near = {
                let (x, y, w, h) = (b.x as i32 - 100, b.y as i32 - 100, b.w as i32 + 200, b.h as i32 + 200);
                let t = target.map(|v| v as i32);
                t[0] < x + w && x < t[0] + t[2] && t[1] < y + h && y < t[1] + t[3]
            };
            if self.released || b.vx != 0.0 || b.vy < 0.0 || b.vy as f64 > 0.3 {
                self.ai[0] = 1.0;
                self.direction = -self.direction;
            } else if near || self.hp < self.kind.max_hp() {
                self.ai[0] = 1.0;
                self.body.vy -= 6.0;
                self.direction = -self.direction;
            }
        } else {
            let max = 3.0;
            let b = &mut self.body;
            if self.collide_x {
                self.direction *= -1;
                b.vx = self.old_vel[0] * -0.5;
                if self.direction == -1 && b.vx > 0.0 && b.vx < max - 1.0 {
                    b.vx = max - 1.0;
                }
                if self.direction == 1 && b.vx < 0.0 && b.vx > -max + 1.0 {
                    b.vx = -max + 1.0;
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
            if self.direction == -1 && b.vx > -max {
                b.vx -= 0.1;
                if b.vx > max {
                    b.vx -= 0.1;
                } else if b.vx > 0.0 {
                    b.vx -= 0.05;
                }
                b.vx = b.vx.max(-max);
            } else if self.direction == 1 && b.vx < max {
                b.vx += 0.1;
                if b.vx < -max {
                    b.vx += 0.1;
                } else if b.vx < 0.0 {
                    b.vx += 0.05;
                }
                b.vx = b.vx.min(max);
            }
            let col = ((b.x + (b.w as i32 / 2) as f32) / 16.0) as i64 + self.direction as i64;
            let row = ((b.y + b.h) / 16.0) as i64;
            let ground = (row..row + 15)
                .filter(|&y| world.in_bounds(col, y))
                .find(|&y| solid(world, col, y) || world.liquid(col, y) > 0);
            match ground {
                None => b.vy += 0.05,
                Some(y) => {
                    b.vy -= 0.1;
                    if y < row + 5 {
                        b.vy -= 0.2;
                    }
                }
            }
            b.vy = b.vy.clamp(-4.0, 2.0);
        }
        if self.wet.is_some() {
            let b = &mut self.body;
            self.ai[1] = 0.0;
            if b.vy > 0.0 {
                b.vy *= 0.95;
            }
            b.vy = (b.vy - 0.5).max(-4.0);
            self.face(target);
        }
    }

    /// `aiStyle` 16 for the goldfish: swims to and fro bobbing up and down,
    /// and flops about out of water.
    pub(super) fn fish(&mut self, world: &World, target: [f32; 4]) {
        if self.direction == 0 {
            self.face(target);
        }
        if self.wet.is_some() {
            let b = &mut self.body;
            let (cx, cy) = (((b.x + b.w / 2.0) / 16.0) as i64, ((b.y + b.h) / 16.0) as i64);
            for y in [cy, cy + 1] {
                match collision::slope(world, cx, y) {
                    1 => {
                        self.direction = 1;
                        b.vx = b.vx.abs();
                    }
                    2 => {
                        self.direction = -1;
                        b.vx = -b.vx.abs();
                    }
                    _ => continue,
                }
                break;
            }
            if self.collide_x {
                b.vx *= -1.0;
                self.direction *= -1;
            }
            if self.collide_y {
                if b.vy > 0.0 {
                    b.vy = -b.vy.abs();
                    self.direction_y = -1;
                    self.ai[0] = -1.0;
                } else if b.vy < 0.0 {
                    b.vy = b.vy.abs();
                    self.direction_y = 1;
                    self.ai[0] = 1.0;
                }
            }
            if self.ai[0] == 0.0 {
                self.ai[0] = 1.0;
            }
            b.vx += self.direction as f32 * 0.1;
            if b.vx < -1.0 || b.vx > 1.0 {
                b.vx *= 0.95;
            }
            if self.ai[0] == -1.0 {
                b.vy -= 0.01;
                if b.vy < -0.3 {
                    self.ai[0] = 1.0;
                }
            } else {
                b.vy += 0.01;
                if b.vy > 0.3 {
                    self.ai[0] = -1.0;
                }
            }
            let (cx, cy) = (((b.x + b.w / 2.0) / 16.0) as i64, ((b.y + b.h / 2.0) / 16.0) as i64);
            let present = |y: i64| world.tile(cx, y) != crate::NO_TILE;
            if world.liquid(cx, cy - 1) > 128 && (present(cy + 1) || present(cy + 2)) {
                self.ai[0] = -1.0;
            }
            let b = &mut self.body;
            if b.vy > 0.4 || b.vy < -0.4 {
                b.vy *= 0.95;
            }
        } else {
            if self.body.vy == 0.0 {
                self.body.vy = self.rand_range(-50, -20) as f32 * 0.1;
                self.body.vx = self.rand_range(-20, 20) as f32 * 0.1;
                self.direction = if self.rand(2) == 0 { 1 } else { -1 };
            }
            let b = &mut self.body;
            b.vy = (b.vy + 0.3).min(10.0);
            self.ai[0] = 1.0;
        }
        self.rotation = (self.body.vy * self.direction as f32 * 0.1).clamp(-0.2, 0.2);
    }

    /// The new heading of a firefly or butterfly every so often: toward the
    /// player when far, else anywhere at `speed` (hundredths, `lo..hi`).
    fn drift(&mut self, target: [f32; 4], lo: i32, hi: i32) -> (f32, f32) {
        self.face(target);
        let far = (self.body.center().0 - (target[0] + target[2] / 2.0)).abs();
        if far > 700.0 && self.local_ai[3] == 0.0 {
            let speed = if far > 1000.0 {
                self.rand_range(150, 201)
            } else if far > 850.0 {
                self.rand_range(100, 151)
            } else {
                self.rand_range(50, 151)
            } as f32
                * 0.01;
            let dx = self.direction * self.rand_range(100, 251);
            let mut dy = self.rand_range(-50, 51);
            if self.body.y > target[1] - 100.0 {
                dy -= self.rand_range(100, 251);
            }
            let k = speed / ((dx * dx + dy * dy) as f32).sqrt();
            (dx as f32 * k, dy as f32 * k)
        } else {
            self.local_ai[3] = 1.0;
            let speed = self.rand_range(lo, hi) as f32 * 0.01;
            let dx = self.rand_range(-100, 101);
            let dy = self.rand_range(-100, 101);
            let k = speed / ((dx * dx + dy * dy) as f32).sqrt();
            (dx as f32 * k, dy as f32 * k)
        }
    }

    /// The fliers' easing toward `(hx, hy)` over `n` ticks, kept off the
    /// ground (`down` tiles) and within 30 tiles above it.
    fn hover(&mut self, world: &World, (hx, mut hy): (f32, f32), n: f32, down: i64, in_bounds: bool) -> (f32, f32) {
        let b = &mut self.body;
        b.vx = (b.vx * (n - 1.0) + hx) / n;
        b.vy = (b.vy * (n - 1.0) + hy) / n;
        let (cx, cy) = (((b.x + b.w / 2.0) / 16.0) as i64, ((b.y + b.h / 2.0) / 16.0) as i64);
        let inside = |x: i64, y: i64| {
            !in_bounds || (x >= 2 && y >= 2 && x < world.width as i64 - 2 && y < world.height as i64 - 2)
        };
        if b.vy > 0.0 {
            for y in cy..cy + down {
                if inside(cx, y) && (solid(world, cx, y) || world.liquid(cx, y) > 0) {
                    hy *= -1.0;
                    if b.vy > 0.0 {
                        b.vy *= 0.9;
                    }
                }
            }
        }
        if b.vy < 0.0 && !(cy..cy + 30).any(|y| inside(cx, y) && solid(world, cx, y)) {
            hy *= -1.0;
            if b.vy < 0.0 {
                b.vy *= 0.9;
            }
        }
        (hx, hy)
    }

    /// `aiStyle` 64: drifts about and blinks on now and then at night or underground.
    pub(super) fn firefly(&mut self, world: &World, target: [f32; 4], sky: Sky) {
        let mut head = (self.ai[0], self.ai[1]);
        self.local_ai[0] -= 1.0;
        if self.ai[3] == 0.0 {
            self.ai[3] = self.rand_range(75, 111) as f32 * 0.01;
        }
        if self.local_ai[0] <= 0.0 {
            self.local_ai[0] = self.rand_range(60, 180) as f32;
            head = self.drift(target, 5, 151);
        }
        if self.local_ai[2] > 0.0 {
            self.local_ai[2] -= 1.0;
        } else if self.local_ai[1] > 0.0 {
            self.local_ai[1] -= 1.0;
        } else {
            self.local_ai[1] = self.rand_range(30, 180) as f32;
            if !sky.day || self.body.y / 16.0 > sky.surface + 10.0 {
                self.local_ai[2] = self.rand_range(10, 30) as f32;
            }
        }
        head = self.hover(world, head, 80.0, 4, true);
        if self.collide_x {
            head.0 = if self.body.vx < 0.0 { head.0.abs() } else { -head.0.abs() };
            self.body.vx *= -0.2;
        }
        if self.body.vx < 0.0 {
            self.direction = -1;
        }
        if self.body.vx > 0.0 {
            self.direction = 1;
        }
        (self.ai[0], self.ai[1]) = head;
    }

    /// The light a lit firefly or a glowing snail gives (`Lighting.AddLight`).
    pub fn glow(&self) -> Option<[f32; 3]> {
        let s = self.scale();
        match self.kind {
            Kind::Firefly if self.local_ai[2] > 3.0 => Some([0.1095 * s, 0.15 * s, 0.0615 * s]),
            Kind::LightningBug if self.local_ai[2] > 3.0 => Some([0.10125 * s, 0.21375 * s, 0.225 * s]),
            Kind::GlowingSnail => Some([0.1, 0.2, 0.7]),
            _ => None,
        }
    }

    /// `AI_065_Butterflies`: like the fireflies, quicker, in one of eight
    /// colours, and pushed away from hostile monsters within 100 px.
    pub(super) fn butterfly(&mut self, world: &World, target: [f32; 4], threats: &[[f32; 4]]) {
        let mut head = (self.ai[0], self.ai[1]);
        if self.ai[2] == 0.0 {
            let n = self.rand(100);
            let colour = match n {
                0 => 5,
                1..=2 => 1,
                3..=8 => 2,
                9..=18 => 7,
                19..=33 => 3,
                34..=52 => 6,
                53..=74 => 4,
                _ => 0,
            };
            self.ai[2] = (1 + colour) as f32;
        }
        if self.ai[3] == 0.0 {
            self.ai[3] = self.rand_range(75, 111) as f32 * 0.01;
        }
        self.local_ai[0] -= 1.0;
        if self.local_ai[0] <= 0.0 {
            self.local_ai[0] = self.rand_range(90, 240) as f32;
            head = self.drift(target, 26, 301);
        }
        head = self.hover(world, head, 60.0, 3, false);
        if self.local_ai[1] > 0.0 {
            self.local_ai[1] -= 1.0;
        } else {
            self.local_ai[1] = 15.0;
            let c = self.body.center();
            let near: Vec<_> = threats.iter().filter(|t| box_distance(**t, c) <= 100.0).collect();
            if !near.is_empty() {
                let (mut ax, mut ay) = (0.0, 0.0);
                for t in &near {
                    let (dx, dy) = (c.0 - (t[0] + t[2] / 2.0), c.1 - (t[1] + t[3] / 2.0));
                    let l = dx.hypot(dy);
                    if l > 0.0 {
                        (ax, ay) = (ax + dx / l, ay + dy / l);
                    }
                }
                let n = near.len() as f32;
                let b = &mut self.body;
                (b.vx, b.vy) = (b.vx + ax / n * 2.0, b.vy + ay / n * 2.0);
                let l = b.vx.hypot(b.vy);
                if l > 16.0 {
                    (b.vx, b.vy) = (b.vx / l * 16.0, b.vy / l * 16.0);
                }
            }
        }
        if self.collide_x {
            head.0 = if self.body.vx < 0.0 { head.0.abs() } else { -head.0.abs() };
            self.body.vx *= -0.2;
        }
        if self.body.vx < 0.0 {
            self.direction = -1;
        }
        if self.body.vx > 0.0 {
            self.direction = 1;
        }
        (self.ai[0], self.ai[1]) = head;
    }

    /// `aiStyle` 66: crawls for 10 to 30 s, rests 5 to 15 s.
    pub(super) fn worm(&mut self, target: [f32; 4]) {
        if self.body.vy == 0.0 {
            if self.ai[0] == 1.0 {
                if self.direction == 0 {
                    self.face(target);
                }
                if self.collide_x {
                    self.direction *= -1;
                }
                self.body.vx = 0.2 * self.direction as f32;
            } else {
                self.body.vx = 0.0;
            }
            self.local_ai[1] -= 1.0;
            if self.local_ai[1] <= 0.0 {
                if self.ai[0] == 1.0 {
                    self.ai[0] = 0.0;
                    self.local_ai[1] = self.rand_range(300, 900) as f32;
                } else {
                    self.ai[0] = 1.0;
                    self.local_ai[1] = self.rand_range(600, 1800) as f32;
                }
            }
        } else if self.direction == 0 {
            self.direction = if self.body.vx < 0.0 { -1 } else { 1 };
        }
    }

    /// `aiStyle` 67: creeps along floors, walls and ceilings, turning at
    /// each corner, and now and then lets go and falls.
    pub(super) fn snail(&mut self, world: &World, target: [f32; 4]) {
        if self.kind == Kind::Snail {
            if self.ai[3] != 0.0 {
                let s = self.ai[3];
                let (w, h) = ((12.0 * s) as i32 as f32, (12.0 * s) as i32 as f32);
                let b = &mut self.body;
                if w != b.w {
                    b.x = b.x + (b.w as i32 / 2) as f32 - w - 2.0;
                    b.w = w;
                }
                if h != b.h {
                    b.y = b.y + b.h - h;
                    b.h = h;
                }
            }
            if self.ai[3] == 0.0 {
                self.ai[3] = self.rand_range(80, 111) as f32 * 0.01;
            }
        }
        let speed = if self.kind == Kind::GlowingSnail { 0.6 } else { 0.3 };
        if self.ai[0] == 0.0 {
            self.face(target);
            self.direction_y = 1;
            self.ai[0] = 1.0;
            if self.direction > 0 {
                self.sprite_dir = 1;
            }
        }
        let mut climbing = false;
        if self.ai[2] == 0.0 && self.rand(7200) == 0 {
            self.ai[2] = 2.0;
        }
        if !self.collide_x && !self.collide_y {
            self.local_ai[3] += 1.0;
            if self.local_ai[3] > 5.0 {
                self.ai[2] = 2.0;
            }
        } else {
            self.local_ai[3] = 0.0;
        }
        if self.ai[2] > 0.0 {
            self.ai[1] = 0.0;
            self.ai[0] = 1.0;
            self.direction_y = 1;
            let b = self.body;
            if b.vy > speed {
                self.rotation += self.direction as f32 * 0.1;
            } else {
                self.rotation = 0.0;
            }
            self.sprite_dir = self.direction;
            self.body.vx = speed * self.direction as f32;
            self.no_gravity = false;
            let col = ((b.x + b.w / 2.0 + (b.w as i32 / 2 * -self.direction) as f32) / 16.0) as i64;
            let row = ((b.y + b.h + 8.0) / 16.0) as i64;
            if !matches!(collision::slope(world, col, row), 1 | 2) && self.collide_y {
                self.ai[2] -= 1.0;
            }
            let row = ((b.y + b.h - 4.0) / 16.0) as i64;
            let col = ((b.x + b.w / 2.0 + (b.w as i32 / 2 * self.direction) as f32) / 16.0) as i64;
            if matches!(collision::slope(world, col, row), 3 | 4) {
                self.direction *= -1;
            }
            if self.collide_x && b.vy == 0.0 {
                climbing = true;
                self.ai[2] = 0.0;
                self.direction_y = -1;
                self.ai[1] = 1.0;
            }
            if b.vy == 0.0 {
                if self.local_ai[1] == b.x {
                    self.local_ai[2] += 1.0;
                    if self.local_ai[2] > 10.0 {
                        self.direction = 1;
                        self.body.vx = speed;
                        self.local_ai[2] = 0.0;
                    }
                } else {
                    self.local_ai[2] = 0.0;
                    self.local_ai[1] = b.x;
                }
            }
        }
        if self.ai[2] != 0.0 {
            return;
        }
        self.no_gravity = true;
        if self.ai[1] == 0.0 {
            if self.collide_y {
                self.ai[0] = 2.0;
            }
            if !self.collide_y && self.ai[0] == 2.0 {
                self.direction = -self.direction;
                self.ai[1] = 1.0;
                self.ai[0] = 1.0;
            }
            if self.collide_x {
                self.direction_y = -self.direction_y;
                self.ai[1] = 1.0;
            }
        } else {
            if self.collide_x {
                self.ai[0] = 2.0;
            }
            if !self.collide_x && self.ai[0] == 2.0 {
                self.direction_y = -self.direction_y;
                self.ai[1] = 0.0;
                self.ai[0] = 1.0;
            }
            if self.collide_y {
                self.direction = -self.direction;
                self.ai[1] = 0.0;
            }
        }
        if !climbing {
            self.turn_snail();
        }
        self.body.vx = speed * self.direction as f32;
        self.body.vy = speed * self.direction_y as f32;
    }

    /// The snail's turn toward the surface it is on, 0.1 a tick.
    #[allow(clippy::approx_constant)]
    fn turn_snail(&mut self) {
        let before = self.rotation;
        let (dir, dir_y) = (self.direction, self.direction_y);
        let mut set = |r: f32, s: i32| {
            self.rotation = r;
            self.sprite_dir = s;
        };
        if dir_y < 0 {
            if dir < 0 {
                if self.collide_x {
                    set(1.57, -1);
                } else if self.collide_y {
                    set(3.14, 1);
                }
            } else if self.collide_y {
                set(3.14, -1);
            } else if self.collide_x {
                set(4.71, 1);
            }
        } else if dir < 0 {
            if self.collide_y {
                set(0.0, -1);
            } else if self.collide_x {
                set(1.57, 1);
            }
        } else if self.collide_x {
            set(4.71, -1);
        } else if self.collide_y {
            set(0.0, 1);
        }
        let goal = self.rotation;
        let mut r = before;
        if r as f64 > 6.28 {
            r -= 6.28;
        }
        if r < 0.0 {
            r += 6.28;
        }
        let d = (r - goal).abs();
        let k = 0.1;
        if r > goal {
            if d as f64 > 3.14 {
                r += k;
            } else {
                r -= k;
                if r < goal {
                    r = goal;
                }
            }
        }
        if r < goal {
            if d as f64 > 3.14 {
                r -= k;
            } else {
                r += k;
                if r > goal {
                    r = goal;
                }
            }
        }
        self.rotation = r;
    }

    /// `FindFrame` of the critters.
    pub(super) fn critter_frame(&mut self) {
        let b = self.body;
        let n = self.kind.frames();
        match self.kind {
            Kind::Bunny => {
                if b.vy == 0.0 {
                    if self.direction != 0 {
                        self.sprite_dir = self.direction;
                    }
                    if b.vx == 0.0 {
                        self.frame = 0;
                        self.frame_counter = 0.0;
                    } else {
                        self.frame_counter += b.vx.abs() + 1.0;
                        if self.frame_counter > 6.0 {
                            self.frame += 1;
                            self.frame_counter = 0.0;
                        }
                        if self.frame >= n {
                            self.frame = 0;
                        }
                    }
                } else {
                    self.frame_counter = 0.0;
                    self.frame = if b.vy < 0.0 { 4 } else { 6 };
                }
            }
            Kind::Squirrel => {
                if b.vy == 0.0 {
                    if self.direction != 0 {
                        self.sprite_dir = self.direction;
                    }
                    if b.vx == 0.0 {
                        self.frame = 0;
                        self.frame_counter = 0.0;
                    } else {
                        self.frame_counter += b.vx.abs() * 0.5 + 1.0;
                        if self.frame_counter > 6.0 {
                            self.frame += 1;
                            self.frame_counter = 0.0;
                        }
                        if self.frame >= n - 1 {
                            self.frame = 1;
                        }
                    }
                } else {
                    self.frame_counter = 0.0;
                    self.frame = if b.vy < 0.0 { 2 } else { 4 };
                }
            }
            Kind::Mouse => {
                if b.vy == 0.0 && self.direction != 0 {
                    self.sprite_dir = self.direction;
                }
                if b.vy == 0.0 && b.vx == 0.0 {
                    self.frame_counter += 1.0;
                    if self.frame_counter > 8.0 {
                        self.frame += 1;
                        self.frame_counter = 0.0;
                    }
                    if self.frame > 1 {
                        self.frame = 0;
                    }
                } else {
                    self.frame_counter += b.vx.abs() * 0.5 + 1.0;
                    if self.frame_counter > 6.0 {
                        self.frame += 1;
                        self.frame_counter = 0.0;
                    }
                    if self.frame >= n {
                        self.frame = 2;
                    }
                }
            }
            Kind::Bird | Kind::BlueJay | Kind::Cardinal => {
                self.sprite_dir = self.direction;
                self.rotation = b.vx * 0.1;
                if b.vx == 0.0 && b.vy == 0.0 {
                    self.frame = 4;
                    self.frame_counter = 0.0;
                } else {
                    self.frame_counter += 1.0;
                    if self.frame_counter >= 4.0 {
                        self.frame += 1;
                        self.frame_counter = 0.0;
                    }
                    if self.frame >= n - 1 {
                        self.frame = 0;
                    }
                }
            }
            Kind::Goldfish => {
                self.sprite_dir = self.direction;
                self.frame_counter += 1.0;
                let base = if self.wet.is_some() { 0 } else { 4 };
                let steps = if self.wet.is_some() { 4.0 } else { 2.0 };
                if self.frame_counter >= steps * 6.0 {
                    self.frame_counter = 0.0;
                }
                self.frame = base + (self.frame_counter / 6.0) as u32;
            }
            Kind::Firefly | Kind::LightningBug => {
                self.sprite_dir = self.direction;
                self.frame_counter += 1.0;
                if self.frame_counter < 4.0 {
                    self.frame = 0;
                } else {
                    self.frame = 1;
                    if self.frame_counter >= 7.0 {
                        self.frame_counter = 0.0;
                    }
                }
                if self.local_ai[2] <= 0.0 {
                    self.frame += 2;
                }
            }
            Kind::Butterfly => {
                self.rotation = b.vx * 0.3;
                self.sprite_dir = self.direction;
                self.frame_counter += 1.0 + (b.vx.abs() + b.vy.abs()) / 2.0;
                let k = 7.0;
                self.frame = if self.frame_counter < k {
                    0
                } else if self.frame_counter < k * 2.0 {
                    1
                } else if self.frame_counter < k * 3.0 {
                    2
                } else {
                    if self.frame_counter >= k * 4.0 - 1.0 {
                        self.frame_counter = 0.0;
                    }
                    1
                };
                self.frame += 3 * (self.ai[2] as u32).saturating_sub(1);
            }
            Kind::Worm => {
                if b.vy == 0.0 {
                    self.rotation = 0.0;
                    if b.vx == 0.0 {
                        self.frame = 1;
                        self.frame_counter = 0.0;
                    } else {
                        self.frame_counter += 1.0;
                        if self.frame_counter > 12.0 {
                            self.frame_counter = 0.0;
                            self.frame = (self.frame + 1) % 2;
                        }
                    }
                } else {
                    self.rotation += self.direction as f32 * 0.1;
                    self.frame = 1;
                }
            }
            Kind::Snail | Kind::GlowingSnail => {
                if b.vy > 1.0 {
                    self.frame_counter += 1.0;
                    if self.frame_counter < 6.0 {
                        self.frame = 4;
                    } else {
                        self.frame = 5;
                        if self.frame_counter >= 11.0 {
                            self.frame_counter = 0.0;
                        }
                    }
                } else if b.vx != 0.0 || b.vy != 0.0 {
                    self.frame_counter += 1.0;
                    let c = self.frame_counter;
                    self.frame = match c {
                        c if c < 10.0 => 0,
                        c if c < 20.0 => 1,
                        c if c < 30.0 => 2,
                        c if c < 40.0 => 3,
                        c if c < 50.0 => 2,
                        _ => {
                            if c >= 59.0 {
                                self.frame_counter = 0.0;
                            }
                            1
                        }
                    };
                }
            }
            _ => {}
        }
    }
}
