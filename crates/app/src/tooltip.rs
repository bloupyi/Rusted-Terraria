//! `MouseText_DrawItemTooltip_GetLinesInfo` for the items the engine has: no
//! prefixes, buffs, armor sets or shops yet, and no player bonuses.

use formats::lang::Texts;

use crate::item_ids::*;

pub const WHITE: [u8; 3] = [255, 255, 255];

/// `MouseText_DrawItemTooltip_GetItemNameColor` without the animated rarities.
pub fn rarity_color(rare: i8) -> [u8; 3] {
    match rare {
        -11 => [255, 175, 0],
        -1 => [130, 130, 130],
        1 => [150, 150, 255],
        2 => [150, 255, 150],
        3 => [255, 200, 150],
        4 => [255, 150, 150],
        5 => [255, 150, 255],
        6 => [210, 160, 255],
        7 => [150, 255, 10],
        8 => [255, 255, 10],
        9 => [5, 200, 255],
        10 => [255, 40, 100],
        r if r >= 11 => [180, 40, 255],
        _ => WHITE,
    }
}

/// The lines and their colours, the hover name first. `key` is the `ItemID` name.
pub fn lines(texts: &Texts, name: &str, key: &str, s: &Stats, stack: u16) -> Vec<(String, [u8; 3])> {
    let tip = |n: usize| texts.get(&format!("LegacyTooltip.{n}")).unwrap_or("").to_string();
    let has = |f: u16| s.flags & f != 0;
    let name = if stack > 1 { format!("{name} ({stack})") } else { name.to_string() };
    let mut out = vec![(name, rarity_color(s.rare))];
    let mut add = |text: String| out.push((text, WHITE));
    if s.damage > 0 {
        let class = if has(MELEE) {
            2
        } else if has(RANGED) {
            3
        } else if has(MAGIC) {
            4
        } else if has(SUMMON) {
            53
        } else {
            55
        };
        add(format!("{}{}", s.damage, tip(class)));
        // The player's base 4% with the held item's own crit taken back out.
        if has(MELEE | RANGED | MAGIC) {
            add(format!("{}{}", 4 + s.crit as i32, tip(5)));
        }
        if s.use_style != 0 && !has(SUMMON) {
            let speed = [8, 20, 25, 30, 35, 45, 55].iter().position(|&t| s.use_animation <= t).unwrap_or(7);
            add(tip(6 + speed));
        }
        let kb = s.knock_back;
        let knock = if kb == 0.0 {
            0
        } else {
            1 + [1.5, 3.0, 4.0, 6.0, 7.0, 9.0, 11.0].iter().position(|&t| kb <= t).unwrap_or(7)
        };
        add(tip(14 + knock));
    }
    if has(EQUIP) {
        add(tip(23));
    }
    if s.defense > 0 {
        add(format!("{}{}", s.defense, tip(25)));
    }
    if s.pick > 0 {
        add(format!("{}{}", s.pick, tip(26)));
    }
    if s.axe > 0 {
        add(format!("{}{}", s.axe as i32 * 5, tip(27)));
    }
    if s.hammer > 0 {
        add(format!("{}{}", s.hammer, tip(28)));
    }
    if s.tile_boost != 0 {
        let sign = if s.tile_boost > 0 { "+" } else { "" };
        add(format!("{sign}{}{}", s.tile_boost, tip(54)));
    }
    if has(PLACES) {
        if has(CONSUMABLE) {
            add(tip(33));
        }
    } else if has(AMMO) {
        add(tip(34));
    } else if has(CONSUMABLE) {
        add(tip(35));
    }
    if has(MATERIAL) {
        add(tip(36));
    }
    if has(CONTAINER) {
        add(texts.get("CommonItemTooltip.Container").unwrap_or("").to_string());
    }
    if let Some(t) = texts.get(&format!("ItemTooltip.{key}")) {
        for l in t.split('\n') {
            add(l.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(key: &str) -> Stats {
        ITEMS.iter().find(|e| e.1 == key).unwrap().2
    }

    #[test]
    fn copper_pickaxe_lines() {
        let s = find("CopperPickaxe");
        let lines = lines(&Texts::default(), "Pick", "CopperPickaxe", &s, 1);
        let text: Vec<&str> = lines.iter().map(|l| l.0.as_str()).collect();
        // Damage, crit, speed, knockback, pick power, range.
        assert_eq!(text, ["Pick", "4", "4", "", "", "35", "-1"]);
    }

    #[test]
    fn blocks_and_materials() {
        let wood = lines(&Texts::default(), "Wood", "Wood", &find("Wood"), 12);
        assert_eq!(wood.len(), 3, "placeable, material");
        assert_eq!(wood[0].0, "Wood (12)");
        let gel = lines(&Texts::default(), "Gel", "Gel", &find("Gel"), 1);
        assert_eq!(gel.len(), 3, "ammo, material");
        let eye = lines(&Texts::default(), "Eye", "SuspiciousLookingEye", &find("SuspiciousLookingEye"), 1);
        assert_eq!(eye[0].1, [150, 150, 255]);
        assert_eq!(eye.len(), 2, "consumable");
    }
}
