//! Platforms, house walls and chests.

use crate::inventory::Stack;
use world::{World, NO_TILE};

pub const PLATFORM: u16 = 19;
pub const TABLE: u16 = 14;
pub const CHAIR: u16 = 15;
pub const CHEST: u16 = 21;
/// `TileID.Containers2`.
pub const CHEST2: u16 = 467;
pub const WOOD_PLATFORM: u16 = 94;
pub const STONE_WALL: u16 = 26;
pub const WOOD_WALL: u16 = 93;
/// `(item, wall)`: the walls the engine places, and what breaking them gives
/// (`KillWall_GetItemDrops`).
const WALL_ITEMS: [(u16, u16); 2] = [(STONE_WALL, 1), (WOOD_WALL, 4)];
/// Walls edits may create.
pub const WALLS: [u16; 2] = [1, 4];
pub const SHEETS: [&str; 3] = ["Item_26", "Item_93", "Item_94"];
/// `Chest.maxItems`.
pub const SLOTS: usize = formats::wld::CHEST_SLOTS;

pub fn wall_of(item: u16) -> Option<u16> {
    WALL_ITEMS.iter().find(|w| w.0 == item).map(|w| w.1)
}

pub fn item_of_wall(wall: u16) -> Option<u16> {
    WALL_ITEMS.iter().find(|w| w.1 == wall).map(|w| w.0)
}

pub fn is_chest(t: u16) -> bool {
    t == CHEST || t == CHEST2
}

/// A chest's top-left tile and what it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Chest {
    pub x: i64,
    pub y: i64,
    pub items: [Option<Stack>; SLOTS],
}

impl Chest {
    pub fn new(x: i64, y: i64) -> Self {
        Self { x, y, items: [None; SLOTS] }
    }

    pub fn is_empty(&self) -> bool {
        self.items.iter().all(|s| s.is_none())
    }
}

/// The top-left tile of the 2x2 chest that has cell `(x, y)`.
pub fn chest_origin(world: &World, x: i64, y: i64) -> (i64, i64) {
    let (fx, fy) = world.frame(x, y);
    (x - (fx as i64 % 36) / 18, y - (fy as i64 % 36) / 18)
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Open,
    Platform,
    Solid,
}

/// `TileFrame` of a flat platform: its frame x from what is beside it.
pub fn platform_frame(world: &World, x: i64, y: i64) -> i16 {
    let side = |x: i64| match world.tile(x, y) {
        NO_TILE => Side::Open,
        PLATFORM => Side::Platform,
        _ if world.solid(x, y) => Side::Solid,
        _ => Side::Open,
    };
    match (side(x - 1), side(x + 1)) {
        (Side::Platform, Side::Platform) => 0,
        (Side::Platform, Side::Open) => 18,
        (Side::Open, Side::Platform) => 36,
        (Side::Solid, Side::Platform) => 54,
        (Side::Platform, Side::Solid) => 72,
        (Side::Solid, Side::Open) => 108,
        (Side::Open, Side::Solid) => 126,
        _ => 90,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use formats::wld::Tile;

    fn world() -> World {
        let mut important = vec![false; 20];
        important[PLATFORM as usize] = true;
        World::parse(&formats::synth::write_world("h", 20, 10, &important, |_, y| Tile {
            tile: if y == 9 { 1 } else { NO_TILE },
            ..Tile::default()
        }))
        .unwrap()
    }

    #[test]
    fn platform_ends_join_blocks() {
        let mut w = world();
        w.set_tile(2, 5, 1, -1, -1);
        for x in 3..6 {
            w.set_tile(x, 5, PLATFORM, 0, 0);
        }
        let f: Vec<i16> = (3..6).map(|x| platform_frame(&w, x, 5)).collect();
        assert_eq!(f, [54, 0, 18]);
        w.set_tile(7, 5, PLATFORM, 0, 0);
        w.set_tile(8, 5, PLATFORM, 0, 0);
        assert_eq!(platform_frame(&w, 7, 5), 36, "open on the left");
        assert_eq!(platform_frame(&w, 10, 5), 90, "alone");
        w.set_tile(11, 5, 1, -1, -1);
        assert_eq!(platform_frame(&w, 10, 5), 126, "block on the right only");
        assert_eq!(platform_frame(&w, 12, 5), 108, "block on the left only");
    }

    #[test]
    fn walls_give_their_item() {
        assert_eq!(wall_of(WOOD_WALL), Some(4));
        assert_eq!(item_of_wall(1), Some(STONE_WALL));
        assert_eq!(item_of_wall(2), None);
    }
}
