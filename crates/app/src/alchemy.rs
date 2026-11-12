//! Herbs and the rest of `WorldGen.UpdateWorld`'s random tile updates the
//! engine has: herbs growing and blooming (`GrowAlch`), new herbs
//! (`PlantAlch`), grass spreading to dirt and plants coming back on it.

use world::World;

/// Herb tiles: just planted, grown, blooming (`Main.tileAlch`).
pub const SAPLING: u16 = 82;
pub const GROWN: u16 = 83;
pub const BLOOMING: u16 = 84;
/// The placed bottle: the alchemy station (`TileID.Bottles`).
pub const BOTTLE_TILE: u16 = 13;
pub const BOTTLE: u16 = 31;
pub const BOTTLED_WATER: u16 = 126;
pub const MUSHROOM: u16 = 5;
pub const GLOWING_MUSHROOM: u16 = 183;
pub const CACTUS: u16 = 276;
/// Not a tile: a recipe station met by water near the player (`needWater`).
pub const NEAR_WATER: u16 = u16::MAX;
const GRASS: u16 = 2;
const DIRT: u16 = 0;
const PLANTS: u16 = 3;
const MUSHROOM_GRASS: u16 = 70;
const MUD: u16 = 59;
const GLOWING_PLANTS: u16 = 71;
/// `WorldGen.beachDistance`.
const BEACH: i64 = 380;

/// Herbs, seeds, bottles and the potions' other ingredients.
pub const SHEETS: &[&str] = &[
    "Item_5",
    "Item_31",
    "Item_126",
    "Item_183",
    "Item_275",
    "Item_276",
    "Item_307",
    "Item_308",
    "Item_309",
    "Item_310",
    "Item_311",
    "Item_312",
    "Item_313",
    "Item_314",
    "Item_315",
    "Item_316",
    "Item_317",
    "Item_318",
    "Item_319",
    "Item_320",
    "Item_323",
    "Item_2357",
    "Item_2358",
    "Item_28",
    "Item_110",
    "Item_188",
    "Item_189",
];

/// The herb item and its seed by style (`frameX / 18`).
pub fn herb_and_seed(style: u16) -> (u16, u16) {
    if style == 6 {
        (2358, 2357)
    } else {
        (313 + style, 307 + style)
    }
}

/// `GetItemDrop_Bottles`: what a placed bottle of `style` gives back.
pub fn bottle_item(style: i16) -> u16 {
    const ITEMS: [u16; 9] = [BOTTLE, 28, 110, 350, 351, 2234, 2244, 2257, 2258];
    ITEMS.get(style as usize).copied().unwrap_or(BOTTLE)
}

/// The herb style a seed plants.
pub fn seed_style(item: u16) -> Option<u16> {
    match item {
        307..=312 => Some(item - 307),
        2357 => Some(6),
        _ => None,
    }
}

/// What the herbs' blooming depends on.
#[derive(Clone, Copy, Debug)]
pub struct Sky {
    pub day: bool,
    /// `Main.time`: ticks into the day or the night.
    pub time: f64,
    pub blood_moon: bool,
    pub moon_phase: i32,
    pub raining: bool,
}

/// `IsAlchemyPlantHarvestable`, raining aside for the fireblossom's depth.
pub fn harvestable(style: u16, y: i64, surface: f64, sky: Sky) -> bool {
    match style {
        0 => sky.day,
        1 => !sky.day,
        3 => !sky.day && (sky.blood_moon || sky.moon_phase == 0),
        4 => sky.raining,
        5 => (!sky.raining || y as f64 > surface) && sky.time > 40500.0,
        _ => false,
    }
}

/// Styles harvestable now on the surface, a bit each, for drawing.
pub fn bloom_mask(sky: Sky) -> u8 {
    (0..7).filter(|&s| harvestable(s, 0, 0.0, sky)).fold(0, |m, s| m | 1 << s)
}

/// `CheckAlch`: the ground each herb style keeps to.
fn holds(style: u16, ground: u16) -> bool {
    let any = [78, 380].contains(&ground);
    any || match style {
        0 => [2, 109, 477, 492].contains(&ground),
        1 => ground == 60,
        2 => [0, 59].contains(&ground),
        3 => [23, 25, 199, 203, 661, 662].contains(&ground),
        4 => [53, 116].contains(&ground),
        5 => [57, 633].contains(&ground),
        6 => [147, 161, 163, 164, 200].contains(&ground),
        _ => false,
    }
}

/// `PlaceAlch`: a sapling of `style` can go at `(x, y)`.
pub fn can_plant(world: &World, x: i64, y: i64, style: u16) -> bool {
    let below = world.tile(x, y + 1);
    let shape = world::frame::shape(world, x, y + 1);
    let (liquid, kind) = liquid_at(world, x, y);
    let drowned = match style {
        4 => liquid > 0 && kind == 1,
        5 => liquid > 0 && kind != 1,
        6 => liquid > 0 && kind == 1,
        _ => liquid > 0,
    };
    world.tile(x, y) == world::NO_TILE && below != world::NO_TILE && shape == 0 && holds(style, below) && !drowned
}

/// `CheckAlch`: a herb at `(x, y)` without its ground, or in lava, dies.
pub fn dies(world: &World, x: i64, y: i64) -> bool {
    let style = world.frame(x, y).0.max(0) as u16 / 18;
    let below = world.tile(x, y + 1);
    let (liquid, kind) = liquid_at(world, x, y);
    below == world::NO_TILE
        || world::frame::shape(world, x, y + 1) == 1
        || !holds(style, below)
        || (style != 5 && liquid > 0 && kind == 1)
}

fn liquid_at(world: &World, x: i64, y: i64) -> (u8, u8) {
    if !world.in_bounds(x, y) {
        return (0, 0);
    }
    let (c, i) = world.cell(x as u32, y as u32);
    (c.liquid[i], c.liquid_kind[i])
}

/// `PlaceSuitableHerbHere`: the style that grows on `ground` at column `x`.
pub fn style_for(ground: u16, x: i64, width: i64) -> Option<u16> {
    match ground {
        2 | 109 => Some(0),
        60 => Some(1),
        0 | 59 => Some(2),
        23 | 25 | 199 | 203 | 661 | 662 => Some(3),
        53 | 116 if (BEACH..=width - BEACH).contains(&x) => Some(4),
        57 | 633 => Some(5),
        147 | 161 | 163 | 164 | 200 => Some(6),
        _ => None,
    }
}

/// What harvesting a herb gives (`KillTile_GetItemDrops`): the herb, and
/// `roll % 3 + 1` seeds when it blooms. Saplings give nothing.
pub fn harvest(tile: u16, style: u16, y: i64, surface: f64, sky: Sky, roll: u64) -> Vec<(u16, u16)> {
    if tile != GROWN && tile != BLOOMING {
        return Vec::new();
    }
    let (herb, seed) = herb_and_seed(style);
    let mut out = vec![(herb, 1)];
    if tile == BLOOMING || harvestable(style, y, surface, sky) {
        out.push((seed, (roll % 3) as u16 + 1));
    }
    out
}

/// A change `GrowAlch` makes to a herb.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Growth {
    Dies,
    Becomes(u16),
}

/// `GrowAlch` for the herb at `(x, y)`; `roll` draws from the game's random numbers.
pub fn grow(world: &World, x: i64, y: i64, surface: f64, sky: Sky, mut roll: impl FnMut(u64) -> u64) -> Option<Growth> {
    let tile = world.tile(x, y);
    let style = world.frame(x, y).0.max(0) as u16 / 18;
    let (liquid, kind) = liquid_at(world, x, y);
    // Only the fireblossom stands lava, and only moonglow and waterleaf water.
    if liquid > 0 && (kind != 1 || style != 5) && (kind != 0 || (style != 1 && style != 4)) {
        return Some(Growth::Dies);
    }
    if tile == SAPLING {
        let grows = if roll(50) == 0 {
            style != 6 || roll(2) == 0
        } else {
            let height = world.height as i64;
            let when = match style {
                0 => sky.day,
                1 => !sky.day,
                4 => sky.raining,
                2 => y as f64 > surface,
                5 => y > height - 200,
                _ => false,
            };
            when && roll(50) == 0
        };
        return grows.then_some(Growth::Becomes(GROWN));
    }
    if style == 2 && roll(3) != 0 {
        // The blinkroot blinks.
        return if tile == GROWN {
            (roll(2) == 0).then_some(Growth::Becomes(BLOOMING))
        } else if tile == BLOOMING || roll(5) == 0 {
            Some(Growth::Becomes(GROWN))
        } else {
            None
        };
    }
    (style == 6 && tile == GROWN && roll(30) == 0).then_some(Growth::Becomes(BLOOMING))
}

/// `PlantAlch`: a column and the depth to look for ground from, and whether
/// few enough herbs are near the ground found there to plant one.
pub fn plant_spot(world: &World, x: i64, start: i64) -> Option<(i64, i64, u16)> {
    let height = world.height as i64;
    let mut y = start;
    while y < height - 20 && world.tile(x, y) == world::NO_TILE {
        y += 1;
    }
    let ground = world.tile(x, y);
    if ground == world::NO_TILE || world.tile(x, y - 1) != world::NO_TILE || liquid_at(world, x, y - 1).0 != 0 {
        return None;
    }
    let r = (15.0 * (world.width as f64 / 4200.0)) as i64;
    let (w, h) = (world.width as i64, height);
    let near = (((x - r).clamp(4, w - 4))..=((x + r).clamp(4, w - 4)))
        .flat_map(|i| (((y - r).clamp(4, h - 4))..=((y + r).clamp(4, h - 4))).map(move |j| (i, j)))
        .filter(|&(i, j)| [SAPLING, GROWN, BLOOMING].contains(&world.tile(i, j)))
        .count();
    let style = style_for(ground, x, w)?;
    (near < 5 && can_plant(world, x, y - 1, style)).then_some((x, y - 1, style))
}

/// The depth `PlantAlch` starts looking from, by the game's odds.
pub fn plant_depth(world: &World, surface: f64, rock: f64, mut roll: impl FnMut(u64) -> u64) -> i64 {
    let h = world.height as i64;
    let span = |a: i64, b: i64, r: &mut dyn FnMut(u64) -> u64| a + r((b - a).max(1) as u64) as i64;
    if roll(40) == 0 {
        span(((rock + h as f64) / 2.0) as i64, h - 20, &mut roll)
    } else if roll(10) != 0 {
        span(surface as i64, h - 20, &mut roll)
    } else {
        span(20, h - 20, &mut roll)
    }
}

/// A plant to place (cell, tile, `frameX`) and the cells grass spreads to.
pub type Growing = (Option<(i64, i64, u16, i16)>, Vec<(i64, i64)>);

/// `UpdateWorld_GrassGrowth` for forest grass at `(x, y)`: the plant it grows
/// above (`PlaceTile` for type 3, with its frame), and the dirt it spreads to.
pub fn grass_growth(world: &World, x: i64, y: i64, surface: f64, mut roll: impl FnMut(u64) -> u64) -> Growing {
    let mut plant = None;
    let (liquid, _) = liquid_at(world, x, y - 1);
    if world.tile(x, y - 1) == world::NO_TILE && liquid == 0 && roll(12) == 0 && fits_flower(world, x, y - 1) {
        let walls_ok = [world.wall(x, y - 1), world.wall(x, y)].iter().all(|w| PLANT_WALLS.contains(w));
        if walls_ok {
            const RARE: [i16; 14] = [6, 7, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
            let fx = if roll(50) == 0 {
                144
            } else if roll(35) == 0 || (63..=70).contains(&world.wall(x, y - 1)) {
                RARE[roll(RARE.len() as u64) as usize] * 18
            } else {
                roll(6) as i16 * 18
            };
            plant = Some((x, y - 1, PLANTS, fx));
        }
    }
    let spread = (x - 1..x + 2)
        .flat_map(|i| (y - 1..y + 2).map(move |j| (i, j)))
        .filter(|&(i, j)| (i, j) != (x, y) && world.tile(i, j) == DIRT && takes_grass(world, i, j, DIRT, surface))
        .collect();
    (plant, spread)
}

/// `WallID.Sets.AllowsPlantsToGrow`.
pub const PLANT_WALLS: [u16; 27] = [
    0, 150, 138, 145, 107, 152, 140, 139, 141, 106, 245, 315, 317, 63, 64, 65, 66, 67, 68, 69, 81, 70, 264, 268, 265,
    74, 80,
];

/// `IsFitToPlaceFlowerIn` for the forest's plants.
fn fits_flower(world: &World, x: i64, y: i64) -> bool {
    let below = world.tile(x, y + 1);
    y >= 1
        && below != world::NO_TILE
        && world::frame::shape(world, x, y + 1) == 0
        && [2, 78, 380, 477, 579].contains(&below)
}

/// `SpreadGrass` without repeating: `dirt` at `(x, y)` turns to grass when
/// it touches air and no lava, above the surface for plain dirt.
fn takes_grass(world: &World, x: i64, y: i64, dirt: u16, surface: f64) -> bool {
    if !world.in_bounds(x, y) || (dirt == DIRT && y as f64 >= surface) {
        return false;
    }
    let mut buried = true;
    for i in x - 1..x + 2 {
        for j in y - 1..y + 2 {
            if !world.solid(i, j) {
                buried = false;
            }
            let (l, k) = liquid_at(world, i, j);
            if l > 0 && k == 1 {
                return false;
            }
        }
    }
    !buried
}

/// `UpdateWorld_GrassGrowth` underground for mushroom grass: glowing
/// mushrooms one time in ten, and mud turning to mushroom grass.
pub fn mushroom_growth(world: &World, x: i64, y: i64, mut roll: impl FnMut(u64) -> u64) -> Growing {
    let mut plant = None;
    let flat = world::frame::shape(world, x, y) == 0;
    if world.tile(x, y - 1) == world::NO_TILE && roll(10) == 0 && flat {
        plant = Some((x, y - 1, GLOWING_PLANTS, roll(5) as i16 * 18));
    }
    let spread = (x - 1..x + 2)
        .flat_map(|i| (y - 1..y + 2).map(move |j| (i, j)))
        .filter(|&(i, j)| (i, j) != (x, y) && world.tile(i, j) == MUD && takes_grass(world, i, j, MUD, 0.0))
        .collect();
    (plant, spread)
}

pub const GRASS_TILE: u16 = GRASS;
pub const MUSHROOM_GRASS_TILE: u16 = MUSHROOM_GRASS;

#[cfg(test)]
mod tests {
    use super::*;
    use formats::wld::Tile;
    use world::NO_TILE;

    const DAY: Sky = Sky { day: true, time: 20000.0, blood_moon: false, moon_phase: 3, raining: false };
    const NIGHT: Sky = Sky { day: false, ..DAY };

    fn world(f: impl Fn(u32, u32) -> Tile) -> World {
        let mut important = vec![false; 90];
        for t in [3, 13, 71, SAPLING, GROWN, BLOOMING] {
            important[t as usize] = true;
        }
        World::parse(&formats::synth::write_world("a", 64, 100, &important, f)).unwrap()
    }

    fn tile(t: u16) -> Tile {
        Tile { tile: t, ..Tile::default() }
    }

    #[test]
    fn herbs_bloom_by_their_own_clock() {
        assert!(harvestable(0, 0, 0.0, DAY) && !harvestable(0, 0, 0.0, NIGHT));
        assert!(harvestable(1, 0, 0.0, NIGHT) && !harvestable(1, 0, 0.0, DAY));
        assert!(!harvestable(3, 0, 0.0, NIGHT) && harvestable(3, 0, 0.0, Sky { moon_phase: 0, ..NIGHT }));
        assert!(!harvestable(5, 0, 0.0, DAY) && harvestable(5, 0, 0.0, Sky { time: 41000.0, ..DAY }));
        assert_eq!(bloom_mask(DAY), 1);
        assert_eq!(bloom_mask(Sky { blood_moon: true, ..NIGHT }), 0b1010);
    }

    #[test]
    fn harvests_give_seeds_when_blooming() {
        assert_eq!(harvest(SAPLING, 0, 0, 0.0, DAY, 0), []);
        assert_eq!(harvest(GROWN, 0, 0, 0.0, NIGHT, 0), [(313, 1)]);
        assert_eq!(harvest(GROWN, 0, 0, 0.0, DAY, 2), [(313, 1), (307, 3)]);
        assert_eq!(harvest(BLOOMING, 6, 0, 0.0, NIGHT, 0), [(2358, 1), (2357, 1)]);
        assert_eq!((seed_style(310), seed_style(2357), seed_style(5)), (Some(3), Some(6), None));
    }

    #[test]
    fn herbs_keep_to_their_ground() {
        // Grass at x 5, sand at x 6, dirt at x 7, ground at y 50.
        let w = world(|x, y| match (x, y) {
            (5, 50) => tile(2),
            (6, 50) => tile(53),
            (7, 50) => tile(0),
            (8, 49) => Tile { tile: NO_TILE, liquid: 255, ..Tile::default() },
            (8, 50) => tile(2),
            _ => tile(NO_TILE),
        });
        assert!(can_plant(&w, 5, 49, 0) && !can_plant(&w, 5, 49, 4));
        assert!(can_plant(&w, 6, 49, 4) && can_plant(&w, 7, 49, 2));
        assert!(!can_plant(&w, 8, 49, 0), "not under water");
        assert!(!can_plant(&w, 9, 49, 0), "not in the air");
        assert_eq!((style_for(2, 5, 64), style_for(53, 5, 64), style_for(53, 400, 1000)), (Some(0), None, Some(4)));
    }

    #[test]
    fn saplings_grow_and_drown() {
        let w = world(|x, y| match (x, y) {
            (_, 50) => tile(2),
            (5, 49) => Tile { tile: SAPLING, frame_x: 0, frame_y: 0, ..Tile::default() },
            (6, 49) => Tile { tile: SAPLING, frame_x: 0, frame_y: 0, liquid: 100, ..Tile::default() },
            (7, 49) => Tile { tile: SAPLING, frame_x: 18, frame_y: 0, ..Tile::default() },
            _ => tile(NO_TILE),
        });
        assert_eq!(grow(&w, 5, 49, 30.0, DAY, |_| 0), Some(Growth::Becomes(GROWN)));
        assert_eq!(grow(&w, 5, 49, 30.0, DAY, |n| if n == 50 { 1 } else { 0 }), None);
        assert_eq!(grow(&w, 6, 49, 30.0, DAY, |_| 1), Some(Growth::Dies));
        assert_eq!(grow(&w, 7, 49, 30.0, DAY, |_| 1), None, "moonglow grows only at night by its own odds");
    }

    #[test]
    fn grass_spreads_to_dirt_in_the_open_and_grows_plants() {
        // Grass at (10, 30), dirt beside it and one buried deep under it.
        let w = world(|x, y| match (x, y) {
            (10, 30) => tile(2),
            (9 | 11, 30) | (10, 31) | (9..=11, 32..) => tile(0),
            (8 | 12, 30..) | (9 | 11, 31) => tile(0),
            _ => tile(NO_TILE),
        });
        let (plant, spread) = grass_growth(&w, 10, 30, 50.0, |n| if n == 50 || n == 35 { 1 } else { 0 });
        assert_eq!(plant, Some((10, 29, 3, 0)));
        assert!(spread.contains(&(9, 30)) && spread.contains(&(11, 30)));
        assert!(!spread.contains(&(10, 31)), "buried dirt stays dirt");
        let (_, deep) = grass_growth(&w, 10, 30, 30.0, |_| 1);
        assert!(deep.is_empty(), "no grass below the surface");
    }
}
