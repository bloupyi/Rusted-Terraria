//! The player, moved as the game's `Player.Update` does it: one call per
//! 60 Hz tick, velocities in pixels per tick.

use crate::body::Body;
use crate::collision::{self, Box};
use crate::World;

pub const WIDTH: f32 = 20.0;
pub const HEIGHT: f32 = 42.0;
const SIZE: [i32; 2] = [WIDTH as i32, HEIGHT as i32];
const RUN_ACCEL: f32 = 0.08;
const MAX_RUN: f32 = 3.0;
const RUN_SLOWDOWN: f32 = 0.2;
const HONEY: u8 = formats::wld::Liquid::Honey as u8;
const SHIMMER: u8 = formats::wld::Liquid::Shimmer as u8;

#[derive(Default, Clone, Copy)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    /// Drops through platforms.
    pub down: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Player {
    /// Velocity in pixels per tick.
    pub body: Body,
    pub facing_left: bool,
    /// Ticks of jump left while it is held.
    pub jump: i32,
    pub release_jump: bool,
    /// The liquid the player is in.
    pub wet: Option<u8>,
    stair_fall: bool,
    /// Drawn offset after a step, caught up over a few ticks.
    pub gfx_off_y: f32,
    step_speed: f32,
    /// Row of the legs sheet.
    pub leg_frame: u32,
    leg_counter: f64,
    /// Top-left before the last tick, to draw between ticks.
    pub prev: [f32; 2],
}

impl std::ops::Deref for Player {
    type Target = Body;
    fn deref(&self) -> &Body {
        &self.body
    }
}

impl std::ops::DerefMut for Player {
    fn deref_mut(&mut self) -> &mut Body {
        &mut self.body
    }
}

impl Player {
    pub fn at(x: f32, y: f32) -> Self {
        Self {
            body: Body::new(x, y, WIDTH, HEIGHT),
            facing_left: false,
            jump: 0,
            release_jump: true,
            wet: None,
            stair_fall: false,
            gfx_off_y: 0.0,
            step_speed: 1.0,
            leg_frame: 0,
            leg_counter: 0.0,
            prev: [x, y],
        }
    }

    /// Standing on the first block from the top of column `tx`.
    pub fn spawn(world: &World, tx: u32) -> Self {
        let ty = (0..world.height as i64)
            .find(|&y| (0..2).any(|dx| world.solid(tx as i64 + dx, y)))
            .unwrap_or(world.height as i64);
        Self::at(tx as f32 * 16.0, ty as f32 * 16.0 - HEIGHT)
    }

    /// Top-left `alpha` of the way from the previous tick to this one.
    pub fn drawn(&self, alpha: f32) -> [f32; 2] {
        [self.prev[0] + (self.x - self.prev[0]) * alpha, self.prev[1] + (self.y - self.prev[1]) * alpha]
    }

    fn pos(&self) -> [f32; 2] {
        [self.x, self.y]
    }

    fn at_box<'a>(&self, world: &'a World) -> Box<'a> {
        Box { world, pos: self.pos(), size: SIZE }
    }

    pub fn tick(&mut self, world: &World, input: Input) {
        self.prev = self.pos();
        let (gravity, max_fall, jump_height, jump_speed) = match self.wet {
            None => (0.4, 10.0, 15, 5.01),
            Some(HONEY) => (0.1, 3.0, 15, 5.01),
            Some(SHIMMER) => (0.15, 10.0, 23, 5.51),
            Some(_) => (0.2, 5.0, 30, 6.01),
        };
        self.horizontal(input);
        self.jump_movement(input, jump_height, jump_speed);
        self.vy = (self.vy + gravity).min(max_fall + 0.01);

        let wet = collision::wet(world, self.pos(), SIZE);
        if wet.is_none() && self.wet.is_some() {
            self.jump = self.jump.min(jump_height / 5);
        }
        self.wet = wet;

        let v = self.at_box(world).walk_down_slope([self.vx, self.vy], gravity);
        (self.vx, self.vy) = (v[0], v[1]);
        if self.vy == gravity {
            if let Some((y, speed)) = self.at_box(world).step_down([self.vx, self.vy]) {
                self.step_to(y, speed);
            }
        }
        if self.vy >= gravity && !input.down {
            if let Some((y, speed)) = self.at_box(world).step_up([self.vx, self.vy]) {
                self.step_to(y, speed);
            }
        }

        let fall_through = input.down;
        let before = [self.vx, self.vy];
        let hit = self.at_box(world).tiles(before, fall_through, false);
        let mut moved = hit.vel;
        if let Some(kind) = self.wet {
            let k = match kind {
                HONEY => 0.25,
                SHIMMER => 0.375,
                _ => 0.5,
            };
            for a in 0..2 {
                if hit.vel[a] == before[a] {
                    moved[a] *= k;
                }
            }
        } else if hit.up {
            self.jump = 0;
        }
        (self.x, self.y) = (self.x + moved[0], self.y + moved[1]);
        (self.vx, self.vy) = (hit.vel[0], hit.vel[1]);

        if input.down {
            self.stair_fall = true;
        }
        let (pos, vel, stair, stair_fall, up) = self.at_box(world).slopes([self.vx, self.vy], self.stair_fall);
        if stair_fall {
            self.stair_fall = true;
        } else if !fall_through {
            self.stair_fall = false;
        }
        if stair && (pos[1] - self.y).abs() > 8.0 + self.vx.abs() {
            self.gfx_off_y -= pos[1] - self.y;
            self.step_speed = 4.0;
        }
        (self.x, self.y, self.vx, self.vy) = (pos[0], pos[1], vel[0], vel[1]);
        if up {
            self.vy = 0.01;
            self.jump = 0;
        }

        let k = (1.0 + self.vx.abs() / 3.0) * self.step_speed;
        self.gfx_off_y =
            if self.gfx_off_y > 0.0 { (self.gfx_off_y - k).max(0.0) } else { (self.gfx_off_y + k).min(0.0) };
        self.gfx_off_y = self.gfx_off_y.clamp(-32.0, 32.0);
        self.on_ground = self.vy == 0.0;
        self.animate();
    }

    fn step_to(&mut self, y: f32, speed: f32) {
        self.gfx_off_y += self.y - y;
        self.y = y;
        self.step_speed = speed;
    }

    /// `HorizontalMovement` without items, mounts or wind.
    fn horizontal(&mut self, input: Input) {
        let dir = input.right as i32 - input.left as i32;
        if dir != 0 {
            self.facing_left = dir < 0;
        }
        if input.left && self.vx > -MAX_RUN {
            if self.vx > RUN_SLOWDOWN {
                self.vx -= RUN_SLOWDOWN;
            }
            self.vx -= RUN_ACCEL;
        } else if input.right && self.vx < MAX_RUN {
            if self.vx < -RUN_SLOWDOWN {
                self.vx += RUN_SLOWDOWN;
            }
            self.vx += RUN_ACCEL;
        } else {
            let s = if self.vy == 0.0 { RUN_SLOWDOWN } else { RUN_SLOWDOWN * 0.5 };
            self.vx = if self.vx > s {
                self.vx - s
            } else if self.vx < -s {
                self.vx + s
            } else {
                0.0
            };
        }
    }

    /// `JumpMovement` for a plain jump.
    fn jump_movement(&mut self, input: Input, height: i32, speed: f32) {
        if input.jump {
            if self.jump > 0 {
                if self.vy == 0.0 {
                    self.jump = 0;
                } else {
                    self.vy = -speed;
                    self.jump -= 1;
                }
            } else if self.vy == 0.0 && self.release_jump {
                self.vy = -speed;
                self.jump = height;
            }
            self.release_jump = false;
        } else {
            self.jump = 0;
            self.release_jump = true;
        }
    }

    /// The legs part of `PlayerFrame`: 0 standing, 5 in the air, 7 to 19 walking.
    pub fn animate(&mut self) {
        if self.vy != 0.0 {
            self.leg_counter = 0.0;
            self.leg_frame = 5;
        } else if self.vx != 0.0 {
            self.leg_counter += self.vx.abs() as f64 * 1.3;
            while self.leg_counter > 8.0 {
                self.leg_counter -= 8.0;
                self.leg_frame += 1;
            }
            if self.leg_frame < 7 {
                self.leg_frame = 19;
            } else if self.leg_frame > 19 {
                self.leg_frame = 7;
            }
        } else {
            self.leg_counter = 0.0;
            self.leg_frame = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PLATFORM: u16 = 19;
    use crate::{flags, NO_TILE};
    use formats::wld::Tile;

    fn world(f: impl Fn(u32, u32) -> u16) -> World {
        cells(|x, y| Tile { tile: f(x, y), frame_x: 0, frame_y: 0, ..Tile::default() })
    }

    fn cells(f: impl Fn(u32, u32) -> Tile) -> World {
        let mut important = vec![false; 20];
        important[PLATFORM as usize] = true;
        World::parse(&formats::synth::write_world("p", 64, 100, &important, f)).unwrap()
    }

    fn run(p: &mut Player, w: &World, input: Input, secs: f32) {
        for _ in 0..(secs * 60.0) as u32 {
            p.tick(w, input);
        }
    }

    #[test]
    fn falls_lands_walks_and_steps_up() {
        // Floor at y = 40, a one-tile step at x >= 20, a two-tile wall at x = 30.
        let w = world(
            |x, y| {
                if y >= 40 || (x >= 20 && y == 39) || (x == 30 && (37..39).contains(&y)) {
                    1
                } else {
                    NO_TILE
                }
            },
        );
        let mut p = Player::at(5.0 * 16.0, 0.0);
        run(&mut p, &w, Input::default(), 2.0);
        assert!(p.on_ground);
        assert_eq!(p.y + HEIGHT, 40.0 * 16.0);
        run(&mut p, &w, Input { right: true, ..Input::default() }, 3.0);
        assert_eq!(p.y + HEIGHT, 39.0 * 16.0, "stepped up");
        assert_eq!(p.x + WIDTH, 30.0 * 16.0, "stopped by the wall");
        run(&mut p, &w, Input { jump: true, right: true, ..Input::default() }, 1.0);
        assert!(p.x > 30.0 * 16.0, "jumped over: {}", p.x);
    }

    #[test]
    fn platforms_hold_unless_dropping() {
        let w = world(|_, y| match y {
            20 => PLATFORM,
            y if y >= 40 => 1,
            _ => NO_TILE,
        });
        let mut p = Player::at(100.0, 0.0);
        run(&mut p, &w, Input::default(), 2.0);
        assert_eq!(p.y + HEIGHT, 20.0 * 16.0);
        run(&mut p, &w, Input { down: true, ..Input::default() }, 2.0);
        assert_eq!(p.y + HEIGHT, 40.0 * 16.0);
        // Jumping up through the platform from below.
        let mut p = Player::at(100.0, 22.0 * 16.0);
        p.vy = -10.0;
        run(&mut p, &w, Input { jump: true, ..Input::default() }, 0.2);
        assert!(p.y < 20.0 * 16.0);
    }

    #[test]
    fn walks_up_a_slope() {
        // Floor at y = 40 and a four-tile ramp up to the right onto a ledge at y = 36.
        let w = cells(|x, y| {
            let solid = Tile { tile: 1, frame_x: 0, frame_y: 0, ..Tile::default() };
            match x {
                20..24 if y == 39 - (x - 20) => Tile { flags: 3 << flags::SHAPE_SHIFT, ..solid },
                20..24 if y > 39 - (x - 20) => solid,
                24.. if y >= 36 => solid,
                _ if y >= 40 => solid,
                _ => Tile { tile: NO_TILE, ..Tile::default() },
            }
        });
        let mut p = Player::at(5.0 * 16.0, 40.0 * 16.0 - HEIGHT);
        run(&mut p, &w, Input { right: true, ..Input::default() }, 3.0);
        assert_eq!(p.y + HEIGHT, 36.0 * 16.0, "on the ledge at x {}", p.x);
        assert!(p.x > 30.0 * 16.0, "kept walking: {}", p.x);
    }

    #[test]
    fn water_slows_the_fall() {
        let w = cells(|_, y| match y {
            20..40 => Tile { tile: NO_TILE, liquid: 255, ..Tile::default() },
            40.. => Tile { tile: 1, frame_x: 0, frame_y: 0, ..Tile::default() },
            _ => Tile { tile: NO_TILE, ..Tile::default() },
        });
        let mut p = Player::at(100.0, 0.0);
        while p.wet.is_none() {
            p.tick(&w, Input::default());
        }
        run(&mut p, &w, Input::default(), 0.5);
        assert!(p.vy <= 5.01 && p.vy > 0.0, "{}", p.vy);
        run(&mut p, &w, Input::default(), 2.0);
        assert_eq!(p.y + HEIGHT, 40.0 * 16.0);
    }
}
