//! Bows and wooden arrows: `ItemCheck_Shoot` with `PickAmmo`, and projectile 1
//! with `aiStyle` 1.

use world::collision::{self, V2};
use world::World;

pub const WOODEN_ARROW: u16 = 40;
/// Wooden, then copper, tin, iron, lead, silver, tungsten, gold and platinum.
pub const BOWS: [u16; 9] = [39, 3504, 3498, 99, 3492, 3510, 3486, 3516, 3480];
/// Sheets these draw with.
pub const SHEETS: &[&str] = &[
    "Item_39",
    "Item_3504",
    "Item_3498",
    "Item_99",
    "Item_3492",
    "Item_3510",
    "Item_3486",
    "Item_3516",
    "Item_3480",
    "Item_40",
    "Projectile_1",
];
/// The arrow's box.
pub const SIZE: f32 = 10.0;

pub fn is_bow(id: u16) -> bool {
    BOWS.contains(&id)
}

pub struct Arrow {
    /// Top-left of its box.
    pub pos: V2,
    pub vel: V2,
    pub damage: i32,
    pub knock_back: f32,
    /// `ai[0]`: ticks flown, held at 15 once it falls.
    pub ai: f32,
    pub time_left: i32,
}

impl Arrow {
    pub fn new(center: V2, vel: V2, damage: i32, knock_back: f32) -> Self {
        let h = SIZE / 2.0;
        Self { pos: [center[0] - h, center[1] - h], vel, damage, knock_back, ai: 0.0, time_left: 1200 }
    }

    pub fn center(&self) -> V2 {
        [self.pos[0] + SIZE / 2.0, self.pos[1] + SIZE / 2.0]
    }

    /// Its picture points along its flight.
    pub fn rotation(&self) -> f32 {
        self.vel[1].atan2(self.vel[0]) + 1.57
    }

    /// One tick of `AI_001` and `HandleMovement`: straight for 15 ticks, then
    /// falling 0.1 more each tick up to 16. False once it hit a tile or ran out.
    pub fn advance(&mut self, world: &World) -> bool {
        self.ai += 1.0;
        if self.ai >= 15.0 {
            self.ai = 15.0;
            self.vel[1] += 0.1;
        }
        self.vel[1] = self.vel[1].min(16.0);
        let b = collision::Box { world, pos: self.pos, size: [SIZE as i32; 2] };
        let last = self.vel;
        let vel = b.tiles(last, true, true).vel;
        self.pos = [self.pos[0] + vel[0], self.pos[1] + vel[1]];
        self.vel = vel;
        self.time_left -= 1;
        vel == last && self.time_left > 0
    }
}
