//! Armour: which slot an item goes in, its defense and the set bonuses of
//! `ArmorSetBonuses`, for the wood and pre-hardmode metal sets.

use crate::item_ids;

/// Sheets of the sets below: their items, then the pictures drawn on the player.
pub const SHEETS: &[&str] = &[
    "Item_727", "Item_728", "Item_729", "Item_89", "Item_80", "Item_76", "Item_687", "Item_688", "Item_689", "Item_90",
    "Item_81", "Item_77", "Item_690", "Item_691", "Item_692", "Item_91", "Item_82", "Item_78", "Item_693", "Item_694",
    "Item_695", "Item_92", "Item_83", "Item_79", "Item_696", "Item_697", "Item_698", "Item_704", "Extra_54",
    "Extra_58",
];

/// Head, body and leg pictures of the sets above (`headSlot`, `bodySlot`, `legSlot`).
pub fn picture_sheets() -> Vec<String> {
    let mut v = Vec::new();
    for id in SHEETS.iter().filter_map(|s| s.strip_prefix("Item_")?.parse::<u16>().ok()) {
        let Some(s) = item_ids::stats(id) else { continue };
        if s.head_slot > 0 {
            v.push(format!("Armor_Head_{}", s.head_slot));
        }
        if s.body_slot > 0 {
            v.push(format!("Armor/Armor_{}", s.body_slot));
        }
        if s.leg_slot > 0 {
            v.push(format!("Armor_Legs_{}", s.leg_slot));
        }
    }
    v
}

/// The equipment slot an item goes in: 0 head, 1 body, 2 legs.
pub fn slot_of(id: u16) -> Option<usize> {
    let s = item_ids::stats(id)?;
    [s.head_slot, s.body_slot, s.leg_slot].iter().position(|&n| n > 0)
}

/// `ArmorSetBonuses.Initialize` for these sets: defense the full set adds.
pub fn set_bonus(head: u16, body: u16, legs: u16) -> i32 {
    match (head, body, legs) {
        (727, 728, 729) => 1,
        (89, 80, 76) | (687, 688, 689) | (90, 81, 77) => 2,
        (91, 82, 78) | (92, 83, 79) | (690, 691, 692) | (693, 694, 695) => 3,
        (696, 697, 698) => 4,
        _ => 0,
    }
}

/// `statDefense`: the pieces worn, and their set bonus.
pub fn defense(worn: [Option<u16>; 3]) -> i32 {
    let pieces: i32 = worn.iter().flatten().filter_map(|&id| item_ids::stats(id)).map(|s| s.defense as i32).sum();
    let [h, b, l] = worn.map(|w| w.unwrap_or(0));
    pieces + set_bonus(h, b, l)
}

/// `Main.CalculateDamagePlayersTake` in normal mode: half the defense off, at least 1.
pub fn damage_taken(damage: i32, defense: i32) -> i32 {
    (damage as f64 - defense as f64 * 0.5).max(1.0) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_add_their_bonus() {
        assert_eq!(slot_of(89), Some(0));
        assert_eq!(slot_of(80), Some(1));
        assert_eq!(slot_of(76), Some(2));
        assert_eq!(slot_of(20), None);
        // Copper: 1 + 2 + 1, and 2 for the set.
        assert_eq!(defense([Some(89), Some(80), Some(76)]), 6);
        assert_eq!(defense([Some(89), Some(81), Some(76)]), 1 + 3 + 1, "mixed sets get nothing");
        assert_eq!(defense([Some(696), Some(697), Some(698)]), 5 + 6 + 5 + 4);
        assert_eq!(damage_taken(14, 6), 11);
        assert_eq!(damage_taken(3, 20), 1);
    }
}
