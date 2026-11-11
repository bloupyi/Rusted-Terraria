//! Doors as `WorldGen.PlaceDoor`, `OpenDoor` and `CloseDoor` change them: a
//! closed door is a column of three `CLOSED` cells, an open one two columns
//! of `OPEN` cells swung to one side.

use crate::collision::{shape, solid_tile};
use crate::tile_sets::{CUT, DRIP};
use crate::World;

pub const CLOSED: u16 = 10;
pub const OPEN: u16 = 11;

/// Present and of type `t` (`ValidateTileSquareIsActiveAndOfType`).
fn is(world: &World, x: i64, y: i64, t: u16) -> bool {
    shape(world, x, y).0 && world.tile(x, y) == t
}

fn solid(world: &World, x: i64, y: i64) -> bool {
    let (active, solid, ..) = shape(world, x, y);
    active && solid
}

/// `PlaceTile` for a door with the cursor on the empty cell `(i, j)`: the
/// top row of the three it fills, hanging under a block with one under it.
pub fn place_top(world: &World, i: i64, j: i64) -> Option<i64> {
    let active = |y: i64| shape(world, i, y).0;
    if !active(j - 1) && !active(j - 2) && solid(world, i, j - 3) {
        return solid_tile(world, i, j + 1).then_some(j - 2);
    }
    if active(j + 1) || active(j + 2) || !solid(world, i, j + 3) {
        return None;
    }
    (solid(world, i, j - 1) && solid_tile(world, i, j + 3)).then_some(j)
}

/// `PlaceDoor` from the `top` row; `variants` picks one of three pictures per cell.
pub fn place(world: &mut World, i: i64, top: i64, variants: [i16; 3]) {
    for (k, v) in variants.into_iter().enumerate() {
        world.set_tile(i as u32, (top + k as i64) as u32, CLOSED, v * 18, k as i16 * 18);
    }
}

/// Top row of the door that `(i, j)` belongs to.
pub fn top(world: &World, i: i64, j: i64) -> i64 {
    j - (world.frame(i, j).1 % 54 / 18) as i64
}

/// An open door's cells: the left column and the top row.
pub fn open_cells(world: &World, i: i64, j: i64) -> (i64, i64) {
    let fx = world.frame(i, j).0;
    (i - (fx % 72 / 18 % 2) as i64, top(world, i, j))
}

/// Where a closed door at `(i, j)` swings toward `dir`.
pub struct Swing {
    pub left: i64,
    pub top: i64,
    /// The column it swings into; cuttable plants there break first.
    pub column: i64,
    frame_x: i16,
    frame_y: i16,
}

/// `OpenDoor`'s checks: a whole closed door with nothing solid where it swings.
pub fn swing(world: &World, i: i64, j: i64, dir: i32) -> Option<Swing> {
    if world.tile(i, j) != CLOSED {
        return None;
    }
    let (fx, fy) = world.frame(i, j);
    let mut style = (fy / 54) as i32;
    let mut frame_x = 0;
    if fx >= 54 {
        style += 36 * (fx / 54) as i32;
        frame_x += 72 * (fx / 54);
    }
    let top = top(world, i, j);
    if !(0..3).all(|k| is(world, i, top + k, CLOSED)) {
        return None;
    }
    let (left, column) = if dir == -1 {
        frame_x += 36;
        (i - 1, i - 1)
    } else {
        (i, i + 1)
    };
    let blocked = (0..3).any(|k| {
        let t = world.tile(column, top + k);
        shape(world, column, top + k).0 && !CUT.has(t) && t != 165 && !DRIP.has(t)
    });
    (!blocked).then_some(Swing { left, top, column, frame_x, frame_y: (style % 36 * 54) as i16 })
}

/// Opens the door; the swing column must be clear by now.
pub fn open(world: &mut World, s: &Swing) {
    for dx in 0..2 {
        for dy in 0..3 {
            let (x, y) = ((s.left + dx) as u32, (s.top + dy) as u32);
            world.set_tile(x, y, OPEN, s.frame_x + dx as i16 * 18, s.frame_y + dy as i16 * 18);
        }
    }
}

/// `CloseDoor`: the open door at `(i, j)` swings shut unless `occupied`
/// (a player or a monster) blocks its hinge column. `variant` picks the
/// closed picture. Returns the left column and top row of the cells changed.
pub fn close(
    world: &mut World,
    i: i64,
    j: i64,
    variant: i16,
    occupied: impl Fn(i64, i64) -> bool,
) -> Option<(i64, i64)> {
    if world.tile(i, j) != OPEN {
        return None;
    }
    let (fx, fy) = world.frame(i, j);
    let style_x = if fx >= 72 { 54 * (fx / 72) } else { 0 };
    let top = j - (fy % 54 / 18) as i64;
    let (hinge, dir) = match fx % 72 {
        0 => (i, 1),
        18 => (i - 1, 1),
        36 => (i + 1, -1),
        _ => (i, -1),
    };
    let left = if dir == -1 { hinge - 1 } else { hinge };
    if (top..top + 3).any(|y| occupied(hinge, y)) {
        return None;
    }
    if !(0..2).all(|dx| (0..3).all(|dy| is(world, left + dx, top + dy, OPEN))) {
        return None;
    }
    for x in left..left + 2 {
        for y in top..top + 3 {
            let (ux, uy) = (x as u32, y as u32);
            if x == hinge {
                let fy = world.frame(x, y).1;
                world.set_tile(ux, uy, CLOSED, variant * 18 + style_x, fy);
            } else {
                world.set_tile(ux, uy, crate::NO_TILE, -1, -1);
            }
        }
    }
    Some((left, top))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NO_TILE;
    use formats::wld::Tile;

    /// Ground at y = 20, a ceiling at y = 16 over x = 10.
    fn world() -> World {
        World::parse(&formats::synth::write_world("d", 32, 64, &[], |x, y| Tile {
            tile: if y >= 20 || (x == 10 && y == 16) { 1 } else { NO_TILE },
            ..Tile::default()
        }))
        .unwrap()
    }

    #[test]
    fn placed_between_ceiling_and_floor() {
        let mut w = world();
        assert_eq!(place_top(&w, 10, 19), Some(17), "cursor on the bottom cell");
        assert_eq!(place_top(&w, 10, 17), Some(17), "cursor on the top cell");
        assert_eq!(place_top(&w, 12, 19), None, "nothing to hang from");
        place(&mut w, 10, 17, [0, 2, 1]);
        assert_eq!((w.tile(10, 18), w.frame(10, 18)), (CLOSED, (36, 18)));
        assert_eq!(top(&w, 10, 19), 17);
    }

    #[test]
    fn opens_either_way_and_closes() {
        let mut w = world();
        place(&mut w, 10, 17, [0, 0, 0]);
        let s = swing(&w, 10, 18, -1).unwrap();
        assert_eq!((s.left, s.column), (9, 9));
        open(&mut w, &s);
        assert_eq!(w.frame(9, 17), (36, 0));
        assert_eq!(w.frame(10, 19), (54, 36));
        assert_eq!(open_cells(&w, 10, 19), (9, 17));
        assert_eq!(close(&mut w, 9, 18, 1, |x, y| (x, y) == (10, 18)), None, "someone in the doorway");
        assert_eq!(close(&mut w, 9, 18, 1, |_, _| false), Some((9, 17)));
        assert_eq!((w.tile(9, 18), w.tile(10, 18), w.frame(10, 18)), (NO_TILE, CLOSED, (18, 18)));

        let s = swing(&w, 10, 17, 1).unwrap();
        open(&mut w, &s);
        assert_eq!((w.tile(11, 19), w.frame(10, 17), w.frame(11, 17)), (OPEN, (0, 0), (18, 0)));
        assert_eq!(open_cells(&w, 11, 17), (10, 17));
        w.set_tile(9, 18, 1, -1, -1);
        assert!(close(&mut w, 11, 19, 0, |_, _| false).is_some());
        assert!(swing(&w, 10, 18, -1).is_none(), "a block where it would swing");
    }
}
