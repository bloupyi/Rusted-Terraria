//! `WorldGen.SpawnThingsFromPot` for a world with no special seed, before
//! hard mode: what a broken pot leaves, by depth and by luck.

/// Item sheets the loot draws with.
pub const SHEETS: &[&str] = &[
    "Item_28",
    "Item_40",
    "Item_42",
    "Item_58",
    "Item_71",
    "Item_72",
    "Item_73",
    "Item_74",
    "Item_166",
    "Item_188",
    "Item_265",
    "Item_282",
    "Item_286",
    "Item_288",
    "Item_289",
    "Item_290",
    "Item_291",
    "Item_292",
    "Item_293",
    "Item_294",
    "Item_295",
    "Item_296",
    "Item_297",
    "Item_298",
    "Item_299",
    "Item_300",
    "Item_301",
    "Item_302",
    "Item_303",
    "Item_304",
    "Item_305",
    "Item_327",
    "Item_965",
    "Item_974",
    "Item_2322",
    "Item_2323",
    "Item_2324",
    "Item_2325",
    "Item_2326",
    "Item_2327",
    "Item_2329",
    "Item_2350",
    "Item_4383",
    "Item_4385",
    "Item_4386",
    "Item_4388",
    "Item_4423",
    "Item_4870",
];

pub const HEART: u16 = 58;
pub const TORCH: u16 = 8;

/// Where the pot stands and what the game checks about the closest player.
pub struct Pot {
    /// `frameY / 36`.
    pub style: i32,
    pub y: f64,
    pub world_surface: f64,
    pub rock_layer: f64,
    pub height: f64,
    pub dungeon_wall: bool,
    pub in_liquid: bool,
    pub expert: bool,
    pub hurt: bool,
    /// Torches the player carries (counted up to 20).
    pub torches: u32,
}

/// `(item id, stack)` of each item it spawns. `next(n)` is `rand.Next(n)`.
pub fn loot(p: &Pot, mut next: impl FnMut(i32) -> i32) -> Vec<(u16, u16)> {
    let mut range = |lo: i32, hi: i32| lo + next(hi - lo);
    let above_caverns = p.y < p.rock_layer;
    let above_hell = p.y < p.height - 200.0;
    let s = p.style;
    let value_scale = match s {
        4..=6 | 34..=36 => 1.25,
        7..=9 => 1.75,
        10..=12 => 1.9,
        13..=15 => 2.1,
        16..=18 | 22..=24 => 1.6,
        19..=21 => 3.5,
        25..=27 => 10.0,
        31..=33 => 2.0,
        _ => 1.0,
    };
    let value_scale = (value_scale * 2.0 + 1.0) / 3.0;
    if range(0, 35) == 0 && p.dungeon_wall && p.y > p.world_surface {
        return vec![(327, 1)];
    }
    if range(0, 45) == 0 || (range(0, 45) == 0 && p.expert) {
        // A potion, more kinds the deeper; a recall potion or two in place of the rarer ones.
        let (table, n): (&[u16], i32) = if p.y < p.world_surface {
            (&[292, 298, 299, 290, 2322, 2324, 2325], 10)
        } else if above_caverns {
            (&[289, 298, 299, 290, 303, 291, 304, 2322, 2329], 11)
        } else if above_hell {
            (&[296, 295, 299, 302, 303, 305, 301, 302, 297, 304, 2322, 2323, 2327, 2329], 15)
        } else {
            (&[296, 295, 293, 288, 294, 297, 304, 305, 301, 302, 288, 300, 2323, 2326], 14)
        };
        let k = range(0, n);
        let mut out: Vec<(u16, u16)> = table.get(k as usize).map(|&i| (i, 1)).into_iter().collect();
        if k >= 7 && above_hell {
            out.push((2350, range(1, 3) as u16));
        }
        let glowing = if p.y < p.world_surface || above_caverns {
            false
        } else if above_hell {
            range(0, 15) == 0
        } else {
            range(0, 5) == 0
        };
        if glowing {
            out.push((4870, 1));
        }
        return out;
    }
    let mut kind = range(0, 7);
    if p.expert {
        kind -= 1;
    }
    let few_torches = p.torches < 20;
    if kind == 0 && p.hurt {
        let mut hearts = 1 + range(0, 2);
        if p.expert {
            hearts += range(0, 2) + range(0, 2);
        }
        return vec![(HEART, 1); hearts as usize];
    }
    if kind == 1 || (kind == 0 && few_torches) {
        let mut n = range(2, 7);
        if p.expert {
            n += range(1, 7);
        }
        let (mut torch, mut wet) = (TORCH, 282);
        match s {
            22..=24 => (n, torch) = (n + range(2, 7), 4386),
            16..=18 => (n, torch) = (n + range(2, 7), 4385),
            7..=9 => (n, torch) = (((n + range(2, 7)) as f32 * 1.5) as i32, 4388),
            4..=6 => (torch, wet) = (974, 286),
            34..=36 => (n, torch) = (n + range(2, 7), 4383),
            _ => {}
        }
        return vec![(if p.in_liquid { wet } else { torch }, n as u16)];
    }
    match kind {
        2 => {
            let n = range(10, 21);
            let mut item = 40;
            if above_caverns && range(0, 2) == 0 {
                item = 42;
            }
            if !above_hell {
                item = 265;
            }
            return vec![(item, n as u16)];
        }
        3 => {
            let item = if above_hell { 28 } else { 188 };
            let n = if p.expert && range(0, 3) != 0 { 2 } else { 1 };
            return vec![(item, n)];
        }
        4 if (34..=36).contains(&s) || above_hell => {
            let item = if (34..=36).contains(&s) { 4423 } else { 166 };
            let mut n = range(0, 4) + 1;
            if p.expert {
                n += range(0, 4);
            }
            return vec![(item, n as u16)];
        }
        _ => {}
    }
    if (kind == 4 || kind == 5) && above_hell {
        return vec![(965, range(20, 41) as u16)];
    }
    // Coins.
    let mut value = (200 + range(-100, 101)) as f32;
    if p.y < p.world_surface {
        value *= 0.5;
    } else if above_caverns {
        value *= 0.75;
    } else if p.y > p.height - 250.0 {
        value *= 1.25;
    }
    value *= 1.0 + range(-20, 21) as f32 * 0.01;
    for (chance, lo, hi) in [(4, 5, 11), (8, 10, 21), (12, 20, 41), (16, 40, 81), (20, 50, 101)] {
        if range(0, chance) == 0 {
            value *= 1.0 + range(lo, hi) as f32 * 0.01;
        }
    }
    if p.expert {
        value *= 2.5;
        for (chance, k) in [(2, 1.25), (3, 1.5), (4, 1.75)] {
            if range(0, chance) == 0 {
                value *= k;
            }
        }
    }
    value *= value_scale;
    let mut out = Vec::new();
    while value as i32 > 0 {
        let (unit, item) =
            [(1_000_000.0, 74), (10_000.0, 73), (100.0, 72)].into_iter().find(|&(u, _)| value > u).unwrap_or((1.0, 71));
        let mut n = (value / unit) as i32;
        if n > 50 && range(0, 2) == 0 {
            n /= range(0, 3) + 1;
        }
        if range(0, 2) == 0 {
            n /= range(0, if item == 71 { 4 } else { 3 }) + 1;
        }
        if item == 71 {
            n = n.max(1);
        }
        value -= unit * n as f32;
        if n > 0 {
            out.push((item, n as u16));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pot(y: f64) -> Pot {
        Pot {
            style: 0,
            y,
            world_surface: 300.0,
            rock_layer: 400.0,
            height: 1200.0,
            dungeon_wall: false,
            in_liquid: false,
            expert: false,
            hurt: false,
            torches: 99,
        }
    }

    fn rolls(seq: &[i32]) -> impl FnMut(i32) -> i32 + '_ {
        let mut i = 0;
        move |n| {
            let v = seq.get(i).copied().unwrap_or(0).min(n - 1);
            i += 1;
            v
        }
    }

    #[test]
    fn healing_potion_on_a_three() {
        // No key, no rare potion (twice), then the fourth kind.
        assert_eq!(loot(&pot(350.0), rolls(&[1, 1, 1, 3])), [(28, 1)]);
        assert_eq!(loot(&pot(1100.0), rolls(&[1, 1, 1, 3])), [(188, 1)]);
    }

    #[test]
    fn hearts_only_when_hurt() {
        let mut p = pot(350.0);
        p.hurt = true;
        assert_eq!(loot(&p, rolls(&[1, 1, 1, 0, 1])), [(HEART, 1), (HEART, 1)]);
        p.hurt = false;
        p.torches = 0;
        assert_eq!(loot(&p, rolls(&[1, 1, 1, 0, 0])), [(TORCH, 2)], "torches for the unlit");
    }

    #[test]
    fn coins_add_up() {
        let total: u32 = loot(&pot(350.0), rolls(&[1, 1, 1, 6, 100, 20, 1, 1, 1, 1, 1, 1, 1, 1]))
            .iter()
            .map(|&(i, n)| n as u32 * [1, 100, 10_000, 1_000_000][(i - 71) as usize])
            .sum();
        assert!((150..=300).contains(&total), "{total}");
    }
}
