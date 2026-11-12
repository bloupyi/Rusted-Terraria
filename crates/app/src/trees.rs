//! Trees from acorns: `KillTile_GetTreeDrops`, the sapling's `TileObjectData`
//! and `WorldGen.GrowTree` for the common tree.

use world::frame::shape;
use world::{World, NO_TILE};

pub const SAPLING: u16 = 20;
pub const ACORN: u16 = 27;

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

/// `TreeTypeDropsAcorns`, from the soil under the tree: not jungle or mushroom trees.
pub fn drops_acorns(soil: u16) -> bool {
    !matches!(soil, 60 | 70)
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
    if underground || style >= 6 {
        return None;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soils_pick_the_sapling() {
        assert_eq!(SOILS.iter().find(|s| s.0.contains(&2)).unwrap().1, 0);
        assert!(fit_for_tree(2) && !fit_for_tree(53) && drops_acorns(2) && !drops_acorns(60));
        assert!(leafy(22, 198) && leafy(66, 242) && !leafy(0, 198) && !leafy(22, 132));
    }
}
