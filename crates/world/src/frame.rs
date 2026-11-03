//! Frame (sheet cell) of a block from its eight neighbours: the same choices
//! the game makes when it frames a block, reorganised as ordered rule tables.
//!
//! Neighbours are first reduced to a code: the neighbour's type, `AIR` or
//! `MERGE` (a neighbour this block blends into, such as dirt next to stone).
//! Each table is scanned in order and the first matching rule gives one of
//! three variant cells; a rule without cells stops the table.

use crate::tile_sets::*;
use crate::{flags, World, NO_TILE};

const AIR: i32 = -1;
const MERGE: i32 = -2;
const DIRT: i32 = 0;
const MUD: i32 = 59;
const SNOW: i32 = 147;

/// Neighbour codes in the order up, down, left, right, then the diagonals up-left,
/// up-right, down-left, down-right.
type Around = [i32; 8];
const U: usize = 0;
const D: usize = 1;
const L: usize = 2;
const R: usize = 3;

/// What one neighbour slot must be. `Me` is the block's own (merged) type,
/// `Base` the soil a grass grows on.
#[derive(Clone, Copy)]
enum C {
    Any,
    Me,
    NotMe,
    Base,
    MeOrBase,
    NeitherMeNorBase,
    Air,
    NotAir,
    Merge,
}

use C::*;

type Cells = Option<[(u8, u8); 3]>;

struct Rule {
    when: [C; 8],
    cells: Cells,
}

const fn r(u: C, d: C, l: C, rt: C, cells: [(u8, u8); 3]) -> Rule {
    Rule { when: [u, d, l, rt, Any, Any, Any, Any], cells: Some(cells) }
}

#[allow(clippy::too_many_arguments)]
const fn rd(u: C, d: C, l: C, rt: C, ul: C, ur: C, dl: C, dr: C, cells: [(u8, u8); 3]) -> Rule {
    Rule { when: [u, d, l, rt, ul, ur, dl, dr], cells: Some(cells) }
}

const fn stop(u: C, d: C, l: C, rt: C) -> Rule {
    Rule { when: [u, d, l, rt, Any, Any, Any, Any], cells: None }
}

/// Three cells in a row starting at `(x, y)`.
const fn row(x: u8, y: u8) -> [(u8, u8); 3] {
    [(x, y), (x + 1, y), (x + 2, y)]
}

/// Three cells in a column starting at `(x, y)`.
const fn col(x: u8, y: u8) -> [(u8, u8); 3] {
    [(x, y), (x, y + 1), (x, y + 2)]
}

/// Every other cell of a column, for blend cells that interleave two shapes.
const fn col2(x: u8, y: u8) -> [(u8, u8); 3] {
    [(x, y), (x, y + 2), (x, y + 4)]
}

/// Three cells spaced `step` apart on a row.
const fn spaced(x: u8, y: u8, step: u8) -> [(u8, u8); 3] {
    [(x, y), (x + step, y), (x + 2 * step, y)]
}

/// Plain blocks: open sides get an outline.
static PLAIN: &[Rule] = &[
    rd(Me, Me, Me, Me, NotMe, NotMe, Any, Any, row(6, 1)),
    rd(Me, Me, Me, Me, Any, Any, NotMe, NotMe, row(6, 2)),
    rd(Me, Me, Me, Me, NotMe, Any, NotMe, Any, col(10, 0)),
    rd(Me, Me, Me, Me, Any, NotMe, Any, NotMe, col(11, 0)),
    r(Me, Me, Me, Me, row(1, 1)),
    r(NotMe, Me, Me, Me, row(1, 0)),
    r(Me, NotMe, Me, Me, row(1, 2)),
    r(Me, Me, NotMe, Me, col(0, 0)),
    r(Me, Me, Me, NotMe, col(4, 0)),
    r(NotMe, Me, NotMe, Me, spaced(0, 3, 2)),
    r(NotMe, Me, Me, NotMe, spaced(1, 3, 2)),
    r(Me, NotMe, NotMe, Me, spaced(0, 4, 2)),
    r(Me, NotMe, Me, NotMe, spaced(1, 4, 2)),
    r(Me, Me, NotMe, NotMe, col(5, 0)),
    r(NotMe, NotMe, Me, Me, row(6, 4)),
    r(NotMe, Me, NotMe, NotMe, row(6, 0)),
    r(Me, NotMe, NotMe, NotMe, row(6, 3)),
    r(NotMe, NotMe, NotMe, Me, col(9, 0)),
    r(NotMe, NotMe, Me, NotMe, col(12, 0)),
    r(NotMe, NotMe, NotMe, NotMe, row(9, 3)),
];

/// Blends into a merging neighbour when no side is open.
static BLEND_CLOSED: &[Rule] = &[
    r(Merge, Me, Me, Me, row(8, 6)),
    r(Me, Merge, Me, Me, row(8, 5)),
    r(Me, Me, Merge, Me, col(9, 7)),
    r(Me, Me, Me, Merge, col(8, 7)),
    r(Merge, Me, Merge, Me, col2(2, 5)),
    r(Merge, Me, Me, Merge, col2(3, 5)),
    r(Me, Merge, Merge, Me, col2(2, 6)),
    r(Me, Merge, Me, Merge, col2(3, 6)),
    r(Me, Me, Merge, Merge, col(10, 7)),
    r(Merge, Merge, Me, Me, row(8, 10)),
    r(Merge, Me, Merge, Merge, col(11, 5)),
    r(Me, Merge, Merge, Merge, col(11, 8)),
    r(Merge, Merge, Me, Merge, col(12, 8)),
    r(Merge, Merge, Merge, Me, col(12, 5)),
    r(Merge, Merge, Merge, Merge, row(6, 11)),
    // Inner corners; when several diagonals merge the last one listed in the
    // game wins, hence the reversed order.
    rd(Me, Me, Me, Me, Any, Any, Any, Merge, col2(0, 5)),
    rd(Me, Me, Me, Me, Any, Any, Merge, Any, col2(1, 5)),
    rd(Me, Me, Me, Me, Any, Merge, Any, Any, col2(0, 6)),
    rd(Me, Me, Me, Me, Merge, Any, Any, Any, col2(1, 6)),
];

/// Single blends across an open side; skipped for grasses.
static BLEND_EDGE: &[Rule] = &[
    r(Air, Merge, Me, Me, row(13, 0)),
    r(Merge, Air, Me, Me, row(13, 1)),
    r(Me, Me, Air, Merge, row(13, 2)),
    r(Me, Me, Merge, Air, row(13, 3)),
];

/// Blends with at least one open side.
static BLEND_OPEN: &[Rule] = &[
    r(Merge, Me, Air, Me, col(4, 8)),
    r(Me, Merge, Air, Me, col(4, 5)),
    stop(NotAir, NotAir, Air, Me),
    r(Merge, Me, Me, Air, col(5, 8)),
    r(Me, Merge, Me, Air, col(5, 5)),
    stop(NotAir, NotAir, Me, Air),
    r(Air, Me, Merge, Me, row(0, 11)),
    r(Air, Me, Me, Merge, row(3, 11)),
    stop(Air, Me, NotAir, NotAir),
    r(Me, Air, Merge, Me, row(0, 12)),
    r(Me, Air, Me, Merge, row(3, 12)),
    stop(Me, Air, NotAir, NotAir),
    r(Merge, Merge, Air, Air, col(6, 12)),
    r(Merge, NotAir, Air, Air, col(7, 8)),
    r(NotAir, Merge, Air, Air, col(7, 5)),
    stop(NotAir, NotAir, Air, Air),
    r(Air, Air, Merge, Merge, row(9, 11)),
    r(Air, Air, Merge, NotAir, row(0, 14)),
    r(Air, Air, NotAir, Merge, row(3, 14)),
    stop(Air, Air, NotAir, NotAir),
    r(Merge, Air, Air, Air, col(6, 8)),
    r(Air, Merge, Air, Air, col(6, 5)),
    r(Air, Air, Merge, Air, row(0, 13)),
    r(Air, Air, Air, Merge, row(3, 13)),
];

/// Grass over its soil, first pass.
static GRASS_EDGES: &[Rule] = &[
    r(NeitherMeNorBase, MeOrBase, Base, Me, row(0, 11)),
    r(NeitherMeNorBase, MeOrBase, Me, Base, row(3, 11)),
    stop(NeitherMeNorBase, MeOrBase, Any, Any),
    r(MeOrBase, NeitherMeNorBase, Base, Me, row(0, 12)),
    r(MeOrBase, NeitherMeNorBase, Me, Base, row(3, 12)),
    stop(MeOrBase, NeitherMeNorBase, Any, Any),
    r(Base, Me, NeitherMeNorBase, MeOrBase, col(4, 8)),
    stop(Any, Any, NeitherMeNorBase, MeOrBase),
    r(Base, Me, MeOrBase, NeitherMeNorBase, col(5, 8)),
    r(NeitherMeNorBase, Me, MeOrBase, NeitherMeNorBase, col(5, 5)),
    stop(Any, Any, MeOrBase, NeitherMeNorBase),
    rd(Me, Me, Me, Me, NotMe, NotMe, NotMe, Base, row(6, 18)),
    rd(Me, Me, Me, Me, NotMe, Base, NotMe, NotMe, row(6, 19)),
    rd(Me, Me, Me, Me, NotMe, NotMe, Base, NotMe, row(6, 20)),
    rd(Me, Me, Me, Me, Base, NotMe, NotMe, NotMe, row(6, 21)),
    rd(Me, Me, Me, Me, NotMe, NotMe, NotMe, NotMe, spaced(8, 13, 3)),
    rd(Me, Me, Me, Me, NotMe, Any, Any, NotMe, row(2, 17)),
    rd(Me, Me, Me, Me, Any, NotMe, NotMe, Any, row(5, 17)),
    rd(Me, Me, Me, Me, NotMe, Me, Me, Me, col2(3, 6)),
    rd(Me, Me, Me, Me, Me, NotMe, Me, Me, col2(2, 6)),
    rd(Me, Me, Me, Me, Me, Me, NotMe, Me, col2(3, 5)),
    rd(Me, Me, Me, Me, Me, Me, Me, NotMe, col2(2, 5)),
    stop(Me, Me, Me, Me),
    rd(Me, Base, Me, Me, Air, Air, Any, Any, row(6, 1)),
    rd(Base, Me, Me, Me, Any, Any, Air, Air, row(6, 2)),
    rd(Me, Me, Base, Me, Any, Air, Any, Air, col(11, 0)),
    rd(Me, Me, Me, Base, Air, Any, Air, Any, col(10, 0)),
    rd(Me, Base, Me, Me, Any, NotAir, Any, Any, col2(3, 6)),
    rd(Me, Base, Me, Me, NotAir, Any, Any, Any, col2(2, 6)),
    stop(Me, Base, Me, Me),
    rd(Base, Me, Me, Me, Any, Any, Any, NotAir, col2(3, 5)),
    rd(Base, Me, Me, Me, Any, Any, NotAir, Any, col2(2, 5)),
    stop(Base, Me, Me, Me),
    rd(Me, Me, Me, Base, NotAir, Any, Any, Any, col2(3, 5)),
    rd(Me, Me, Me, Base, Any, Any, NotAir, Any, col2(3, 6)),
    stop(Me, Me, Me, Base),
    rd(Me, Me, Base, Me, Any, NotAir, Any, Any, col2(2, 5)),
    rd(Me, Me, Base, Me, Any, Any, Any, NotAir, col2(2, 6)),
];

/// Grass, second pass: one missing diagonal inside soil.
static GRASS_DIAGONALS: &[Rule] = &[
    rd(MeOrBase, MeOrBase, MeOrBase, MeOrBase, NeitherMeNorBase, MeOrBase, MeOrBase, MeOrBase, col2(3, 6)),
    rd(MeOrBase, MeOrBase, MeOrBase, MeOrBase, MeOrBase, NeitherMeNorBase, MeOrBase, MeOrBase, col2(2, 6)),
    rd(MeOrBase, MeOrBase, MeOrBase, MeOrBase, MeOrBase, MeOrBase, NeitherMeNorBase, MeOrBase, col2(3, 5)),
    rd(MeOrBase, MeOrBase, MeOrBase, MeOrBase, MeOrBase, MeOrBase, MeOrBase, NeitherMeNorBase, col2(2, 5)),
];

/// Grass, third pass: lips and caps.
static GRASS_LIPS: &[Rule] = &[
    rd(NeitherMeNorBase, Me, NeitherMeNorBase, Me, Any, Any, Any, NeitherMeNorBase, row(5, 15)),
    rd(NeitherMeNorBase, Me, Me, NeitherMeNorBase, Any, Any, NeitherMeNorBase, Any, row(8, 15)),
    rd(Me, NeitherMeNorBase, NeitherMeNorBase, Me, Any, NeitherMeNorBase, Any, Any, row(5, 16)),
    rd(Me, NeitherMeNorBase, Me, NeitherMeNorBase, NeitherMeNorBase, Any, Any, Any, row(8, 16)),
    rd(NeitherMeNorBase, Me, Me, Me, Any, Any, NeitherMeNorBase, NeitherMeNorBase, spaced(8, 12, 3)),
    rd(Me, NeitherMeNorBase, Me, Me, NeitherMeNorBase, NeitherMeNorBase, Any, Any, spaced(8, 14, 3)),
    rd(Me, Me, NeitherMeNorBase, Me, Any, NeitherMeNorBase, Any, NeitherMeNorBase, spaced(7, 13, 3)),
    rd(Me, Me, Me, NeitherMeNorBase, NeitherMeNorBase, Any, NeitherMeNorBase, Any, spaced(9, 13, 3)),
    r(NeitherMeNorBase, MeOrBase, Base, Base, row(2, 15)),
    r(MeOrBase, NeitherMeNorBase, Base, Base, row(2, 16)),
    r(Base, Base, NeitherMeNorBase, MeOrBase, col(0, 15)),
    r(Base, Base, MeOrBase, NeitherMeNorBase, col(1, 15)),
    r(Me, Base, Base, Base, row(11, 16)),
    r(Base, Me, Base, Base, row(11, 15)),
    r(Base, Base, Me, Base, row(11, 17)),
    r(Base, Base, Base, Me, row(8, 17)),
];

/// Grass, fourth pass: an open side with soil on one diagonal.
static GRASS_SLOPES: &[Rule] = &[
    rd(NeitherMeNorBase, Me, Me, Me, Any, Any, MeOrBase, NeitherMeNorBase, row(0, 18)),
    rd(NeitherMeNorBase, Me, Me, Me, Any, Any, NeitherMeNorBase, MeOrBase, row(3, 18)),
    stop(NeitherMeNorBase, Me, Me, Me),
    rd(Me, NeitherMeNorBase, Me, Me, MeOrBase, NeitherMeNorBase, Any, Any, row(0, 19)),
    rd(Me, NeitherMeNorBase, Me, Me, NeitherMeNorBase, MeOrBase, Any, Any, row(3, 19)),
    stop(Me, NeitherMeNorBase, Me, Me),
    rd(Me, Me, NeitherMeNorBase, Me, Any, MeOrBase, Any, NeitherMeNorBase, row(3, 20)),
    rd(Me, Me, NeitherMeNorBase, Me, Any, NeitherMeNorBase, Any, MeOrBase, row(0, 20)),
    stop(Me, Me, NeitherMeNorBase, Me),
    rd(Me, Me, Me, NeitherMeNorBase, MeOrBase, Any, NeitherMeNorBase, Any, row(0, 21)),
    rd(Me, Me, Me, NeitherMeNorBase, NeitherMeNorBase, Any, MeOrBase, Any, row(3, 21)),
];

fn matches(c: C, v: i32, me: i32, base: i32) -> bool {
    match c {
        Any => true,
        Me => v == me,
        NotMe => v != me,
        Base => v == base,
        MeOrBase => v == me || v == base,
        NeitherMeNorBase => v != me && v != base,
        Air => v == AIR,
        NotAir => v != AIR,
        Merge => v == MERGE,
    }
}

/// `Some(Some(cells))` on a match, `Some(None)` when a rule stops the table.
fn scan(rules: &[Rule], n: &Around, me: i32, base: i32) -> Option<Cells> {
    rules
        .iter()
        .find(|rule| rule.when.iter().zip(n).all(|(&c, &v)| matches(c, v, me, base)))
        .map(|rule| rule.cells)
}

fn pick(cells: Cells, variant: usize) -> Option<(u8, u8)> {
    cells.map(|c| c[variant])
}

fn grass_base(t: i32) -> Option<i32> {
    let t16 = t as u16;
    match t {
        2 | 23 | 109 | 199 | 477 | 492 => Some(DIRT),
        60 | 70 | 661 | 662 => Some(MUD),
        633 => Some(57),
        _ if MOSS.has(t16) => Some(1),
        _ if MOSS_BRICK.has(t16) => Some(38),
        _ => None,
    }
}

/// Neighbour codes before any merging, as the game reads them: stone-like
/// types count as stone, and a slope facing away from this block is open.
fn read_around(world: &World, x: i64, y: i64) -> (Around, [u8; 9]) {
    const OFF: [(i64, i64); 8] = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)];
    let mut n = [AIR; 8];
    let mut shapes = [0u8; 9];
    for (k, (dx, dy)) in OFF.iter().enumerate() {
        let t = world.tile(x + dx, y + dy);
        shapes[k] = shape(world, x + dx, y + dy);
        if t != NO_TILE {
            n[k] = if STONE.has(t) { 1 } else { t as i32 };
        }
    }
    shapes[8] = shape(world, x, y);
    (n, shapes)
}

/// 0 full, 1 half block, 2..=5 slopes (the game's block type).
pub fn shape(world: &World, x: i64, y: i64) -> u8 {
    ((world.flags(x, y) & flags::SHAPE_MASK) >> flags::SHAPE_SHIFT) as u8
}

/// Frame of block `(x, y)` in pixels on its sheet, `variant` in `0..3`.
pub fn block(world: &World, x: i64, y: i64, variant: usize) -> (u32, u32) {
    let (cx, cy) = block_cell(world, x, y, variant);
    (cx as u32 * 18, cy as u32 * 18)
}

fn block_cell(world: &World, x: i64, y: i64, variant: usize) -> (u8, u8) {
    let raw = world.tile(x, y);
    let mut me = if STONE.has(raw) { 1 } else { raw as i32 };
    let (mut n, sh) = read_around(world, x, y);
    let slope = |s: u8| if s >= 2 { s - 1 } else { 0 };
    let my_slope = slope(sh[8]);
    let half = |s: u8| s == 1;
    // Sloped neighbours open the side they lean away from.
    if matches!(slope(sh[L]), 1 | 3) {
        n[L] = AIR;
    }
    if matches!(slope(sh[R]), 2 | 4) {
        n[R] = AIR;
    }
    if matches!(slope(sh[U]), 3 | 4) {
        n[U] = AIR;
    }
    if matches!(slope(sh[D]), 1 | 2) {
        n[D] = AIR;
    }
    match my_slope {
        1 => (n[U], n[R]) = (AIR, AIR),
        2 => (n[U], n[L]) = (AIR, AIR),
        3 => (n[D], n[R]) = (AIR, AIR),
        4 => (n[D], n[L]) = (AIR, AIR),
        _ => {}
    }
    if me == 668 {
        me = DIRT;
    }
    if me == 697 {
        me = 51;
    }
    let me16 = me as u16;
    let each = |n: &mut Around, f: &dyn Fn(i32) -> bool, to: i32| {
        for v in n.iter_mut() {
            if *v > AIR && f(*v) {
                *v = to;
            }
        }
    };
    each(&mut n, &|v| v == DIRT || v == 668, DIRT);
    // Bricks treat each other as the same block, minus a few soils.
    if BRICK.has(me16) {
        let exclude: &dyn Fn(i32) -> bool = match me {
            161 | 163 | 164 | 200 | 738 => &|v| v == SNOW,
            162 => &|v| v == SNOW || v == 161,
            747 => &|v| v == 53,
            60 | 70 | 661 | 662 => &|v| v == MUD,
            633 => &|v| v == 57,
            _ => &|_| false,
        };
        each(&mut n, &|v| BRICK.has(v as u16) && !exclude(v), me);
    }
    // Sloped or half blocks next to this one read as joined vertically.
    let not_platform = |v: i32| v > AIR && !PLATFORMS.has(v as u16);
    if matches!(my_slope, 1 | 2) && not_platform(n[D]) {
        n[D] = me;
    }
    if not_platform(n[U]) && matches!(slope(sh[U]), 1 | 2) {
        n[U] = me;
    }
    if matches!(my_slope, 3 | 4) && not_platform(n[U]) {
        n[U] = me;
    }
    if not_platform(n[D]) && matches!(slope(sh[D]), 3 | 4) {
        n[D] = me;
    }
    if not_platform(n[U]) && half(sh[U]) {
        n[U] = me;
    }
    for side in [L, R] {
        if n[side] > AIR && half(sh[side]) {
            if half(sh[8]) {
                n[side] = me;
            } else if n[side] != me {
                n[side] = AIR;
            }
        }
    }
    if half(sh[8]) {
        for side in [L, R] {
            if n[side] != me {
                n[side] = AIR;
            }
        }
        n[U] = AIR;
    }
    if half(sh[D]) {
        n[D] = AIR;
    }
    if FORCED_DIRT_MERGING.has(me16) {
        each(&mut n, &|v| v == DIRT, me);
    }
    if me == DIRT {
        // Dirt joins whatever blends into it.
        each(
            &mut n,
            &|v| {
                let v = v as u16;
                MERGE_DIRT.has(v) || v == SNOW as u16 || DIRT_SPECIAL.has(v) || FORCED_DIRT_MERGING.has(v) || v == 191
            },
            me,
        );
        each(&mut n, &|v| v == MUD, MERGE);
    } else if MERGE_DIRT.has(me16) {
        each(&mut n, &|v| v == DIRT, MERGE);
    }
    if me == DIRT {
        each(&mut n, &|v| MOSS.has(v as u16) || MOSS_BRICK.has(v as u16), me);
    } else if MOSS.has(me16) || MOSS_BRICK.has(me16) {
        each(&mut n, &|v| v == DIRT, me);
    } else if me == 1 {
        each(&mut n, &|v| MOSS.has(v as u16), me);
    } else if me == 38 {
        each(&mut n, &|v| MOSS_BRICK.has(v as u16), me);
    }
    if GRASS.has(me16) {
        each(&mut n, &|v| ORE.has(v as u16), me);
    } else if ORE.has(me16) {
        each(&mut n, &|v| GRASS.has(v as u16), me);
    }
    // Non-solid neighbours are open space from here on.
    each(&mut n, &|v| v != me && !SOLID.has(v as u16), AIR);

    let mut cell = None;
    let base = grass_base(me);
    if let Some(base) = base {
        match me {
            2 | 477 => each(&mut n, &|v| v == 23, base),
            23 => each(&mut n, &|v| v == 2, base),
            _ => {}
        }
        if let Some(c) = scan(GRASS_EDGES, &n, me, base) {
            cell = pick(c, variant);
        }
        for table in [GRASS_DIAGONALS, GRASS_LIPS] {
            if let Some(Some(c)) = scan(table, &n, me, base) {
                cell = Some(c[variant]);
            }
        }
        if let Some(c) = scan(GRASS_SLOPES, &n, me, base) {
            cell = pick(c, variant).or(cell);
        }
        let soil = |v: i32| v == me || v == base;
        if n[..4].iter().all(|&v| soil(v)) && n[4..].iter().all(|&v| v != AIR) {
            cell = Some(if (x + y) % 2 == 1 { row(6, 11)[variant] } else { row(1, 1)[variant] });
        }
        each(&mut n, &|v| v == base || (base == MUD && v == MUD) || (base == DIRT && v == 668), MERGE);
    }
    if cell.is_none() && (MERGE_DIRT.has(me16) || CHECKS_FOR_MERGE.has(me16)) {
        let mut m = n;
        for v in &mut m[..4] {
            if *v > AIR && *v != me {
                *v = AIR;
            }
        }
        if m[..4].iter().all(|&v| v != AIR) {
            cell = scan(BLEND_CLOSED, &m, me, AIR).and_then(|c| pick(c, variant));
        } else {
            if base.is_none() {
                cell = scan(BLEND_EDGE, &m, me, AIR).and_then(|c| pick(c, variant));
            }
            if let Some(c) = scan(BLEND_OPEN, &m, me, AIR).and_then(|c| pick(c, variant)) {
                cell = Some(c);
            }
        }
    }
    if let Some(c) = cell {
        return c;
    }
    if base.is_some() {
        n.iter_mut().filter(|v| **v == MERGE).for_each(|v| *v = me);
    }
    scan(PLAIN, &n, me, AIR).and_then(|c| pick(c, variant)).unwrap_or(row(1, 1)[variant])
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::wld::Tile;

    fn world(rows: &[&str]) -> World {
        let (w, h) = (rows[0].len() as u32 + 12, rows.len() as u32 + 12);
        World::parse(&formats::synth::write_world("f", w, h, &[], |x, y| {
            let c = rows.get((y as usize).wrapping_sub(6)).and_then(|r| r.as_bytes().get((x as usize).wrapping_sub(6)));
            Tile {
                tile: match c {
                    Some(b'd') => 0,
                    Some(b's') => 1,
                    Some(b'g') => 2,
                    Some(b'w') => 30,
                    _ => NO_TILE,
                },
                ..Tile::default()
            }
        }))
        .unwrap()
    }

    fn cell(w: &World, x: i64, y: i64) -> (u8, u8) {
        block_cell(w, x + 6, y + 6, 0)
    }

    #[test]
    fn plain_shapes() {
        let w = world(&["sss", "sss", "sss", "...", "s.."]);
        assert_eq!(cell(&w, 1, 1), (1, 1));
        assert_eq!(cell(&w, 1, 0), (1, 0));
        assert_eq!(cell(&w, 0, 0), (0, 3));
        assert_eq!(cell(&w, 2, 2), (1, 4));
        assert_eq!(cell(&w, 0, 4), (9, 3));
    }

    #[test]
    fn open_diagonals_pick_inner_corners() {
        let w = world(&[".s.", "sss", "sss"]);
        assert_eq!(cell(&w, 1, 1), (6, 1));
    }

    #[test]
    fn stone_blends_into_dirt_and_dirt_joins_stone() {
        let w = world(&["sss", "sss", "ddd"]);
        assert_eq!(cell(&w, 1, 1), (8, 5));
        assert_eq!(cell(&w, 1, 2), (1, 2));
    }

    #[test]
    fn bricks_join_each_other() {
        let w = world(&["www", "sss", "sss"]);
        assert_eq!(cell(&w, 1, 1), (1, 1));
    }

    #[test]
    fn grass_on_dirt() {
        let w = world(&["ggg", "ddd", "ddd"]);
        // Top grass with soil under it: the top edge cell.
        assert_eq!(cell(&w, 1, 0), (1, 0));
    }
}
