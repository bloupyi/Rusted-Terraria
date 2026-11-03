//! The player: input on top of a [`Body`].

use crate::body::{Body, GRAVITY, MAX_FALL};
use crate::World;

pub const WIDTH: f32 = 20.0;
pub const HEIGHT: f32 = 42.0;
const RUN_SPEED: f32 = 190.0;
const RUN_ACCEL: f32 = 1400.0;
const AIR_ACCEL: f32 = 900.0;
const JUMP_SPEED: f32 = 420.0;
/// Holding jump while rising cuts gravity, for a higher jump.
const JUMP_HOLD_GRAVITY: f32 = 0.55;
/// Liquids slow everything down.
const SWIM: f32 = 0.5;

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
    pub body: Body,
    pub facing_left: bool,
    /// Distance walked on the ground, for the walk animation.
    pub walked: f32,
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
        Self { body: Body::new(x, y, WIDTH, HEIGHT), facing_left: false, walked: 0.0 }
    }

    /// Standing on the first block from the top of column `tx`.
    pub fn spawn(world: &World, tx: u32) -> Self {
        let ty = (0..world.height as i64)
            .find(|&y| (0..2).any(|dx| world.solid(tx as i64 + dx, y)))
            .unwrap_or(world.height as i64);
        Self::at(tx as f32 * 16.0, ty as f32 * 16.0 - HEIGHT)
    }

    pub fn update(&mut self, world: &World, input: Input, dt: f32) {
        let wet = self.in_liquid(world);
        let k = if wet { SWIM } else { 1.0 };
        let dir = input.right as i32 as f32 - input.left as i32 as f32;
        let accel = if self.on_ground { RUN_ACCEL } else { AIR_ACCEL };
        let target = dir * RUN_SPEED * k;
        let dv = (target - self.vx).clamp(-accel * dt, accel * dt);
        self.vx += dv;
        if dir != 0.0 {
            self.facing_left = dir < 0.0;
        }
        if input.jump && (self.on_ground || wet) {
            self.vy = -JUMP_SPEED * if wet { 0.6 } else { 1.0 };
        }
        let g = if input.jump && self.vy < 0.0 { GRAVITY * JUMP_HOLD_GRAVITY } else { GRAVITY };
        self.vy = (self.vy + g * k * dt).min(MAX_FALL * k);
        self.body.step(world, dt, input.down);
        self.walked = if self.on_ground { self.walked + (self.vx * dt).abs() } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PLATFORM: u16 = 19;
    use crate::NO_TILE;
    use formats::wld::Tile;

    fn world(f: impl Fn(u32, u32) -> u16) -> World {
        let mut important = vec![false; 20];
        important[PLATFORM as usize] = true;
        World::parse(&formats::synth::write_world("p", 64, 64, &important, |x, y| Tile {
            tile: f(x, y),
            frame_x: 0,
            frame_y: 0,
            ..Tile::default()
        }))
        .unwrap()
    }

    fn run(p: &mut Player, w: &World, input: Input, secs: f32) {
        for _ in 0..(secs * 60.0) as u32 {
            p.update(w, input, 1.0 / 60.0);
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
        p.vy = -500.0;
        run(&mut p, &w, Input { jump: true, ..Input::default() }, 0.2);
        assert!(p.y < 20.0 * 16.0);
    }
}
