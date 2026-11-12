//! Trees from acorns: `KillTile_GetTreeDrops`, the sapling's `TileObjectData`
//! and `WorldGen.GrowTree` for the common tree.

use world::frame::shape;
use world::{World, NO_TILE};

pub const SAPLING: u16 = 20;
pub const ACORN: u16 = 27;
pub const TREE: u16 = 5;
pub const PALM: u16 = world::draw::PALM;

/// Item pictures of what a shaken tree can drop.
pub const SHEETS: [&str; 10] = [
    "Item_832",
    "Item_933",
    "Item_3360",
    "Item_3361",
    "Item_5517",
    "Item_4366",
    "Item_5629",
    "Item_194",
    "Item_2504",
    "Item_620",
];

/// `TreeTypes`, from the ground under the trunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Forest,
    Corrupt,
    Mushroom,
    Crimson,
    Jungle,
    Snow,
    Hallowed,
    Palm,
    PalmCorrupt,
    PalmCrimson,
    PalmHallowed,
    Ash,
}

impl Kind {
    pub fn of(soil: u16) -> Option<Kind> {
        Some(match soil {
            2 | 477 => Kind::Forest,
            23 | 661 => Kind::Corrupt,
            70 => Kind::Mushroom,
            199 | 662 => Kind::Crimson,
            60 => Kind::Jungle,
            147 => Kind::Snow,
            109 | 492 => Kind::Hallowed,
            53 => Kind::Palm,
            112 => Kind::PalmCorrupt,
            234 => Kind::PalmCrimson,
            116 => Kind::PalmHallowed,
            633 => Kind::Ash,
            _ => return None,
        })
    }

    /// `TreeTypeDropsAcorns`.
    pub fn acorns(self) -> bool {
        !matches!(self, Kind::Mushroom | Kind::Jungle)
    }

    /// The item id of its wood (`KillTile_GetTreeDrops`, the palm's case of
    /// `KillTile_GetItemDrops`); none for a mushroom tree.
    pub fn wood(self) -> Option<u16> {
        Some(match self {
            Kind::Ash => 5215,
            Kind::Corrupt | Kind::PalmCorrupt => 619,
            Kind::Crimson | Kind::PalmCrimson => 911,
            Kind::Jungle => 620,
            Kind::Hallowed | Kind::PalmHallowed => 621,
            Kind::Snow => 2503,
            Kind::Palm => 2504,
            Kind::Mushroom => return None,
            Kind::Forest => 9,
        })
    }

    fn forest_like(self) -> bool {
        matches!(self, Kind::Forest | Kind::Snow | Kind::Hallowed | Kind::Ash)
    }
}

/// `GetTreeBottom`: the ground cell under the tree that `(x, y)` is part of.
pub fn bottom(world: &World, x: i64, y: i64) -> (i64, i64) {
    let (mut x, mut y) = (x, y);
    let trunk = |t: u16| t == TREE || t == PALM || t == NO_TILE || t == 72;
    if world.tile(x, y) != PALM {
        let (fx, fy) = world.frame(x, y);
        let (a, b) = (fx / 22, fy / 22);
        match (a, b) {
            (3, ..=2) => x += 1,
            (4, 3..=5) => x -= 1,
            (1, 6..=8) => x -= 1,
            (2, 6..=8) => x += 1,
            (2, 9..) => x += 1,
            (3, 9..) => x -= 1,
            _ => {}
        }
    }
    while y < world.height as i64 - 1 && trunk(world.tile(x, y)) {
        y += 1;
    }
    (x, y)
}

/// `IsTileALeafyTreeTop`.
pub fn leafy_top(world: &World, x: i64, y: i64) -> bool {
    let (fx, fy) = world.frame(x, y);
    match world.tile(x, y) {
        PALM => fx >= 88,
        TREE => fx == 22 && (198..=242).contains(&fy),
        _ => false,
    }
}

/// `ShakeTree`'s gift from a leafy top, by the game's odds: an item id and how
/// many, or nothing (critters and the like are left out).
pub fn shake(kind: Kind, r: &mut dyn FnMut(i32) -> i32) -> Option<(u16, u16)> {
    use Kind::*;
    let forest = kind == Forest;
    if r(300) == 0 && forest {
        return Some((832, 1));
    }
    if r(300) == 0 && forest {
        return Some((933, 1));
    }
    if r(200) == 0 && kind == Jungle {
        return Some((3360, 1));
    }
    if r(200) == 0 && kind == Jungle {
        return Some((3361, 1));
    }
    if r(1000) == 0 && forest {
        return Some((5517, 1));
    }
    if r(1000) == 0 && forest {
        return Some((4366, 1));
    }
    if r(100) == 0 && kind.forest_like() {
        return Some((5629, 1));
    }
    if r(7) == 0 && kind.forest_like() {
        return Some((ACORN, range(r, 1, 3) as u16));
    }
    if r(8) == 0 && kind == Mushroom {
        return Some((194, 1));
    }
    if r(12) == 0 {
        return kind.wood().map(|w| (w, range(r, 1, 4) as u16));
    }
    if r(20) == 0 {
        if r(30) == 0 {
            let mut n = 1;
            if r(5) == 0 {
                n += 1;
            }
            if r(10) == 0 {
                n += 1;
            }
            return Some((73, n));
        }
        if r(10) == 0 {
            let mut n = range(r, 1, 21);
            if r(3) == 0 {
                n += range(r, 1, 21);
            }
            if r(4) == 0 {
                n += range(r, 1, 21);
            }
            return Some((72, n as u16));
        }
        return Some((71, range(r, 50, 100) as u16));
    }
    None
}

/// `genRand.Next(lo, hi)`.
fn range(r: &mut dyn FnMut(i32) -> i32, lo: i32, hi: i32) -> i32 {
    lo + r(hi - lo)
}

/// `GrowPalmTree` for the sapling cell at `(x, y)`: a leaning trunk of 10 to
/// 20 cells on sand, its top with leaves.
pub fn grow_palm(world: &World, x: i64, y: i64, r: &mut dyn FnMut(i32) -> i32) -> Option<Vec<(i64, i64, i16, i16)>> {
    let mut j = y;
    while world.tile(x, j) == SAPLING {
        j += 1;
    }
    let soil = world.tile(x, j);
    let wet = world.in_bounds(x, j - 1) && world.liquid(x, j - 1) != 0;
    if soil == NO_TILE
        || shape(world, x, j) != 0
        || !crate::alchemy::PLANT_WALLS.contains(&world.wall(x, j - 1))
        || wet
        || !matches!(soil, 53 | 234 | 116 | 112)
        || !room(world, x, x, j - 2, j - 1)
        || !room(world, x - 1, x + 1, j - 20, j - 3)
    {
        return None;
    }
    let height = 10 + r(11) as i64;
    let lean = (r(17) - 8) as i16 * 2;
    let mut off = 0i16;
    let mut out = Vec::new();
    for k in 0..height {
        let cy = j - 1 - k;
        if k == 0 {
            out.push((x, cy, 66, 0));
            continue;
        }
        if k == height - 1 {
            out.push((x, cy, 22 * (4 + r(3)) as i16, off));
            continue;
        }
        if off != lean {
            // The game's condition always holds past the first quarter; it
            // still rolls on the way, as here.
            let t = k as f64 / height as f64;
            if t >= 0.25 {
                if !(t < 0.5 && r(13) == 0) && !(t < 0.7 && r(9) == 0) && t < 0.95 {
                    r(5);
                }
                off += lean.signum() * 2;
            }
        }
        out.push((x, cy, 22 * r(3) as i16, off));
    }
    Some(out)
}

/// The sapling's soils, each with the first of its three pictures:
/// forests, snow, jungle, corruption, crimson, hallow, then the sands that
/// grow palms and ash.
const SOILS: [(&[u16], i16); 11] = [
    (&[2, 477], 0),
    (&[147], 3),
    (&[60], 6),
    (&[23, 661], 9),
    (&[199, 662], 12),
    (&[109, 492], 15),
    (&[53], 18),
    (&[116], 21),
    (&[234], 24),
    (&[112], 27),
    (&[633], 30),
];

/// `IsTileTypeFitForTree`.
fn fit_for_tree(t: u16) -> bool {
    matches!(t, 2 | 23 | 60 | 70 | 109 | 147 | 199 | 477 | 492 | 633 | 661 | 662)
}

/// The tree cell gives an acorn half the time: a leafy top or branch.
pub fn leafy(fx: i16, fy: i16) -> bool {
    fx >= 22 && fy >= 198
}

/// The sapling's two cells, top first, with `(x, y)` the lower one, if one can be
/// planted there: free and dry, on a whole block of a soil it grows on.
/// `pick`: one of its three pictures.
pub fn plant(world: &World, x: i64, y: i64, pick: i16) -> Option<[(i64, i64, i16, i16); 2]> {
    let soil = world.tile(x, y + 1);
    let first = SOILS.iter().find(|s| s.0.contains(&soil))?.1;
    let dry = |y| !world.in_bounds(x, y) || world.liquid(x, y) == 0;
    let free = world.tile(x, y) == NO_TILE && world.tile(x, y - 1) == NO_TILE;
    let fx = (first + pick) * 18;
    (free && world.solid(x, y + 1) && shape(world, x, y + 1) == 0 && dry(y) && dry(y - 1))
        .then_some([(x, y - 1, fx, 0), (x, y, fx, 18)])
}

/// `EmptyTileCheck` ignoring saplings: only small plants in the way.
fn room(world: &World, x0: i64, x1: i64, y0: i64, y1: i64) -> bool {
    const PASS: [u16; 27] = [
        3, 24, 32, 61, 62, 69, 71, 73, 74, 82, 83, 84, 110, 113, 184, 201, 233, 352, 485, 529, 530, 637, 655, 20, 590,
        595, 615,
    ];
    if x0 < 0 || y0 < 0 || x1 >= world.width as i64 || y1 >= world.height as i64 {
        return false;
    }
    (x0..=x1).all(|x| (y0..=y1).all(|y| world.tile(x, y) == NO_TILE || PASS.contains(&world.tile(x, y))))
}

/// `AttemptToGrowTreeFromSapling` and `GrowTree` for the sapling cell at
/// `(x, y)`: the tree cells and their frames, or `None` when it cannot grow.
/// Palm and ash saplings, and any sapling under the surface, wait.
pub fn grow(
    world: &World,
    x: i64,
    y: i64,
    underground: bool,
    r: &mut dyn FnMut(i32) -> i32,
) -> Option<Vec<(i64, i64, i16, i16)>> {
    let style = world.frame(x, y).0 / 54;
    if underground || style >= 10 {
        return None;
    }
    if style >= 6 {
        return grow_palm(world, x, y, r);
    }
    let mut j = y;
    while world.tile(x, j) == SAPLING {
        j += 1;
    }
    let wet = (-1..=1).any(|dx| world.in_bounds(x + dx, j - 1) && world.liquid(x + dx, j - 1) != 0);
    let soil = world.tile(x, j);
    let whole = |x| world.tile(x, j) != NO_TILE && shape(world, x, j) == 0;
    let side_fit = |x| world.tile(x, j) != NO_TILE && fit_for_tree(world.tile(x, j));
    if wet
        || !whole(x)
        || !fit_for_tree(soil)
        || !crate::alchemy::PLANT_WALLS.contains(&world.wall(x, j - 1))
        || !(side_fit(x - 1) || side_fit(x + 1))
    {
        return None;
    }
    let height = 5 + r(12) as i64;
    let mut clear = height + 4;
    if soil == 60 {
        clear += 5;
    }
    let fits = (soil == 70 && room(world, x - 2, x + 2, j - clear, j - 3) && room(world, x - 1, x + 1, j - 2, j - 1))
        || room(world, x - 2, x + 2, j - clear, j - 1);
    if !fits {
        return None;
    }
    let mut out = Vec::new();
    let (mut left, mut right) = (false, false);
    for k in j - height..j {
        let v = r(3) as i16 * 22;
        let mut kind = r(10);
        if k == j - 1 || k == j - height {
            kind = 0;
        }
        while ((kind == 5 || kind == 7) && left) || ((kind == 6 || kind == 7) && right) {
            kind = r(10);
        }
        left = kind == 5 || kind == 7;
        right = kind == 6 || kind == 7;
        let (fx, fy) = match kind {
            1 => (0, 66),
            2 => (22, 0),
            3 => (44, 66),
            4 => (22, 66),
            5 => (88, 0),
            6 => (66, 66),
            7 => (110, 66),
            _ => (0, 0),
        };
        out.push((x, k, fx, fy + v));
        if left {
            let v = r(3) as i16 * 22;
            let (fx, fy) = if r(3) < 2 { (44, 198) } else { (66, 0) };
            out.push((x - 1, k, fx, fy + v));
        }
        if right {
            let v = r(3) as i16 * 22;
            let (fx, fy) = if r(3) < 2 { (66, 198) } else { (88, 66) };
            out.push((x + 1, k, fx, fy + v));
        }
    }
    // Roots on the sides that have ground.
    let root_ok = |x| whole(x) && fit_for_tree(world.tile(x, j));
    let (l, rt) = (root_ok(x - 1), root_ok(x + 1));
    let mut roots = r(3);
    if !l {
        roots = match roots {
            0 => 2,
            1 => 3,
            n => n,
        };
    }
    if !rt {
        roots = match roots {
            0 => 1,
            2 => 3,
            n => n,
        };
    }
    if l && !rt {
        roots = 2;
    }
    if rt && !l {
        roots = 1;
    }
    if roots == 0 || roots == 1 {
        out.push((x + 1, j - 1, 22, 132 + r(3) as i16 * 22));
    }
    if roots == 0 || roots == 2 {
        out.push((x - 1, j - 1, 44, 132 + r(3) as i16 * 22));
    }
    let v = r(3) as i16 * 22;
    let base = match roots {
        0 => Some(88),
        1 => Some(0),
        2 => Some(66),
        _ => None,
    };
    if let Some(fx) = base {
        let at = out.iter().position(|c| (c.0, c.1) == (x, j - 1)).unwrap();
        out[at].2 = fx;
        out[at].3 = 132 + v;
    }
    let fx = if r(13) != 0 { 22 } else { 0 };
    let at = out.iter().position(|c| (c.0, c.1) == (x, j - height)).unwrap();
    out[at].2 = fx;
    out[at].3 = 198 + r(3) as i16 * 22;
    Some(out)
}

const SANDS: [u16; 4] = [53, 112, 116, 234];
const CACTUS: u16 = world::draw::CACTUS;

/// `CheckCactus`: a cactus cell stands while its trunk reaches sand and, for
/// an arm, while it touches the rest of the plant.
pub fn cactus_holds(world: &World, x: i64, y: i64) -> bool {
    let (bx, by) = world::draw::cactus_base(world, x, y);
    let ground = world.tile(bx, by);
    if !SANDS.contains(&ground) || shape(world, bx, by) != 0 {
        return false;
    }
    let is = |x, y| world.tile(x, y) == CACTUS;
    if x != bx {
        is(x, y + 1) || is(x - 1, y) || is(x + 1, y)
    } else {
        is(x, y + 1) || SANDS.contains(&world.tile(x, y + 1))
    }
}

/// `GrowCactus` at `(x, y)`, a cactus cell or open desert sand: where a new
/// cactus cell goes, if anywhere. Too much water near, a crowd of cacti or
/// a plant at its full size stop it.
pub fn grow_cactus(world: &World, x: i64, y: i64, r: &mut dyn FnMut(i32) -> i32) -> Option<(i64, i64)> {
    let t = world.tile(x, y);
    let empty = |x, y| world.tile(x, y) == NO_TILE;
    let is = |x, y| world.tile(x, y) == CACTUS;
    let wet = |x, y| world.in_bounds(x, y) && world.liquid(x, y) > 0;
    if y <= 0 || shape(world, x, y) != 0 || (t != CACTUS && !SANDS.contains(&t)) || wet(x, y - 1) {
        return None;
    }
    let mut water = 0u32;
    for i in (x - 50).max(0)..(x + 50).min(world.width as i64) {
        for j in (y - 25).max(0)..(y + 25).min(world.height as i64) {
            water += world.liquid(i, j) as u32;
        }
    }
    if water / 255 > 25 {
        return None;
    }
    if t != CACTUS {
        if !empty(x, y - 1) || !empty(x - 1, y - 1) || !empty(x + 1, y - 1) {
            return None;
        }
        let (mut cacti, mut sand) = (0, 0);
        'outer: for m in x - 6..=x + 6 {
            for n in y - 3..=y + 1 {
                if is(m, n) {
                    cacti += 1;
                    if cacti >= 4 {
                        return None;
                    }
                }
                if SANDS.contains(&world.tile(m, n)) {
                    sand += 1;
                    if sand > 10 {
                        break 'outer;
                    }
                }
            }
        }
        return (sand > 10).then_some((x, y - 1));
    }
    let (bx, below) = world::draw::cactus_base(world, x, y);
    let under = below - 1 - y;
    let side = x - bx;
    let room = 11 - under;
    let crowd =
        (bx - 2..=bx + 2).flat_map(|i| (y - room..=y + under).map(move |j| (i, j))).filter(|&(i, j)| is(i, j)).count();
    if crowd as i32 >= range(r, 11, 13) {
        return None;
    }
    if side == 0 {
        if under == 0 {
            return empty(x, y - 1).then_some((x, y - 1));
        }
        let arm = |d: i64| {
            is(x, y - 1)
                && empty(x + d, y)
                && empty(x + 2 * d, y + 1)
                && empty(x + d, y - 1)
                && empty(x + d, y + 1)
                && empty(x + 2 * d, y)
        };
        let k = r(3);
        if k == 0 && arm(-1) {
            return Some((x - 1, y));
        }
        if k == 1 && arm(1) {
            return Some((x + 1, y));
        }
        if (under as i32) < range(r, 2, 8) && !is(x - 1, y - 1) && !is(x + 1, y - 1) && empty(x, y - 1) {
            return Some((x, y - 1));
        }
        None
    } else {
        (empty(x, y - 1) && empty(x, y - 2) && empty(x + side, y - 1) && is(x - side, y - 1)).then_some((x, y - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soils_pick_the_sapling() {
        assert_eq!(SOILS.iter().find(|s| s.0.contains(&2)).unwrap().1, 0);
        assert!(fit_for_tree(2) && !fit_for_tree(53));
        assert!(leafy(22, 198) && leafy(66, 242) && !leafy(0, 198) && !leafy(22, 132));
        assert_eq!(Kind::of(53).and_then(Kind::wood), Some(2504));
        assert!(!Kind::Jungle.acorns() && Kind::Palm.acorns());
    }

    #[test]
    fn cacti_grow_arms_on_sand() {
        let w = World::parse(&formats::synth::write_world("d", 120, 60, &[], |_, y| formats::wld::Tile {
            tile: if y >= 40 { 53 } else { NO_TILE },
            ..Default::default()
        }))
        .unwrap();
        let mut w = w;
        let mut s = 9u64;
        let mut r = |n: i32| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s % n.max(1) as u64) as i32
        };
        assert_eq!(grow_cactus(&w, 60, 40, &mut r), Some((60, 39)), "a new cactus on open sand");
        w.set_tile(60, 39, CACTUS, -1, -1);
        for _ in 0..400 {
            let cells: Vec<_> =
                (55..66).flat_map(|x| (25..40).map(move |y| (x, y))).filter(|&(x, y)| w.tile(x, y) == CACTUS).collect();
            let (x, y) = cells[r(cells.len() as i32) as usize];
            if let Some((nx, ny)) = grow_cactus(&w, x, y, &mut r) {
                w.set_tile(nx as u32, ny as u32, CACTUS, -1, -1);
            }
        }
        let cells: Vec<_> =
            (50..70).flat_map(|x| (20..40).map(move |y| (x, y))).filter(|&(x, y)| w.tile(x, y) == CACTUS).collect();
        let height = (20..40).filter(|&y| w.tile(60, y) == CACTUS).count();
        assert!((2..=8).contains(&height), "{height}");
        assert!(cells.len() < 13 && cells.iter().any(|c| c.0 != 60), "{cells:?}");
        assert!(cells.iter().all(|&(x, y)| cactus_holds(&w, x, y)));
        // Without the trunk's foot, the rest falls.
        w.set_tile(60, 39, NO_TILE, -1, -1);
        assert!(!cactus_holds(&w, 60, 38));
    }

    #[test]
    fn shaking_gives_something_now_and_then() {
        let mut s = 5u64;
        let mut r = |n: i32| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s % n.max(1) as u64) as i32
        };
        let mut acorns = 0;
        let mut any = 0;
        for _ in 0..10_000 {
            if let Some((id, n)) = shake(Kind::Forest, &mut r) {
                any += 1;
                if id == ACORN {
                    acorns += n as i32;
                    assert!((1..=2).contains(&n));
                }
            }
        }
        // About 1 in 7 gives acorns, a fifth or so of trees give something.
        assert!((1500..2700).contains(&acorns), "{acorns}");
        assert!((2000..3200).contains(&any), "{any}");
        assert!((0..1000).all(|_| shake(Kind::Mushroom, &mut r).is_none_or(|(id, _)| id != ACORN)));
    }
}
