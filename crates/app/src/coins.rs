//! Coins monsters drop (`NPCLoot_DropMoney`) and the wooden door item.

/// Copper, silver, gold and platinum coins.
pub const COPPER: u16 = 71;
pub const PLATINUM: u16 = 74;
/// The wooden door item; it places tile 10.
pub const DOOR: u16 = 25;
/// Sheets these draw with: inventory pictures, then the spinning coins on the ground.
pub const SHEETS: &[&str] =
    &["Item_25", "Item_71", "Item_72", "Item_73", "Item_74", "Coin_0", "Coin_1", "Coin_2", "Coin_3"];

pub fn is_coin(id: u16) -> bool {
    (COPPER..=PLATINUM).contains(&id)
}

/// `NPCLoot_DropMoney` without luck: `value` copper varied up, then split
/// into coin stacks, each sometimes smaller so more stacks fall.
/// `next(lo, hi)` is `Main.rand.Next(lo, hi)`.
pub fn money(value: f32, blood_moon: bool, mut next: impl FnMut(i32, i32) -> i32) -> Vec<(u16, u16)> {
    let mut v = value;
    v *= 1.0 + next(-20, 76) as f32 * 0.01;
    for (chance, lo, hi) in [(2, 5, 11), (4, 10, 21), (8, 15, 31), (16, 20, 41), (32, 25, 51), (64, 50, 101)] {
        if next(0, chance) == 0 {
            v *= 1.0 + next(lo, hi) as f32 * 0.01;
        }
    }
    if blood_moon {
        v *= 1.0 + next(0, 101) as f32 * 0.01;
    }
    let mut out = Vec::new();
    while v as i32 > 0 {
        let (unit, id) = if v > 1_000_000.0 {
            (1_000_000.0, PLATINUM)
        } else if v > 10_000.0 {
            (10_000.0, 73)
        } else if v > 100.0 {
            (100.0, 72)
        } else {
            (1.0, COPPER)
        };
        let mut n = (v / unit) as i32;
        if n > 50 && next(0, 5) == 0 {
            n /= next(0, 3) + 1;
        }
        if next(0, 5) == 0 {
            n /= next(0, if id == COPPER { 4 } else { 3 }) + 1;
        }
        if id == COPPER {
            n = n.max(1);
        }
        v -= unit * n as f32;
        if n > 0 {
            out.push((id, n as u16));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_luck_rolls_drop_the_value() {
        // Every roll at its lowest but the first: 25 * 0.8 = 20 copper.
        assert_eq!(money(25.0, false, |lo, _| if lo == -20 { -20 } else { lo.max(1) }), [(COPPER, 20)]);
    }

    #[test]
    fn large_values_split_into_bigger_coins() {
        // 30000 untouched: 3 gold.
        let out = money(30000.0, false, |lo, hi| if lo == -20 { 0 } else { (hi - 1).max(lo) });
        assert_eq!(out, [(73, 3)]);
        let out = money(250.0, false, |lo, hi| if lo == -20 { 0 } else { (hi - 1).max(lo) });
        assert_eq!(out, [(72, 2), (COPPER, 50)]);
    }
}
