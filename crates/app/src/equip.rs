//! Accessories and buffs: what `ApplyEquipFunctional` and `UpdateBuffs` change
//! in the player for the ones the engine has.

use world::player::Moves;

pub const LIFE_CRYSTAL: u16 = 29;
pub const MAGIC_MIRROR: u16 = 50;
/// `statLifeMax` as far as life crystals take it.
pub const LIFE_CRYSTAL_CAP: i32 = 400;
/// `Player.maxBuffs`.
pub const MAX_BUFFS: usize = 44;
/// Accessory slots after head, body and legs (`armor[3..8]`).
pub const ACCESSORIES: usize = 5;
/// The buffs the potions above give, with their `BuffID` names.
const BUFFS: [(u16, &str); 20] = [
    (1, "ObsidianSkin"),
    (2, "Regeneration"),
    (3, "Swiftness"),
    (4, "Gills"),
    (5, "Ironskin"),
    (6, "ManaRegeneration"),
    (7, "MagicPower"),
    (8, "Featherfall"),
    (9, "Spelunker"),
    (10, "Invisibility"),
    (11, "Shine"),
    (12, "NightOwl"),
    (13, "Battle"),
    (14, "Thorns"),
    (15, "WaterWalking"),
    (16, "Archery"),
    (17, "Hunter"),
    (18, "Gravitation"),
    (104, "Mining"),
    (24, "OnFire"),
];
/// `BuffID.OnFire`, from lava.
pub const ON_FIRE: u16 = 24;

/// `Main.debuff`: buffs a right click cannot take off.
pub fn is_debuff(kind: u16) -> bool {
    kind == ON_FIRE
}

/// Sheets of the accessories, mirror, crystal, potions and buff icons.
pub const SHEETS: &[&str] = &[
    "Item_29",
    "Item_49",
    "Item_50",
    "Item_53",
    "Item_54",
    "Item_158",
    "Item_159",
    "Item_212",
    "Item_285",
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
    "Item_2322",
];

pub fn buff_sheets() -> Vec<String> {
    BUFFS.iter().map(|b| format!("Buff_{}", b.0)).collect()
}

/// The `BuffName` and `BuffDescription` keys of buff `kind`.
pub fn buff_keys(kind: u16) -> Option<(String, String)> {
    let name = BUFFS.iter().find(|b| b.0 == kind)?.1;
    Some((format!("BuffName.{name}"), format!("BuffDescription.{name}")))
}

/// A buff the player has and its ticks left (`buffType`, `buffTime`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Buff {
    pub kind: u16,
    pub time: i32,
}

/// What worn accessories and buffs add up to this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effects {
    pub moves: Moves,
    pub life_regen: i32,
    pub defense: i32,
    /// `pickSpeed`: scales a pickaxe's use time, 0.3 at least.
    pub pick_speed: f32,
    /// Buff 11: light around the player.
    pub shine: bool,
    /// Buff 1: `lavaImmune`, and no catching fire.
    pub lava_immune: bool,
    /// Buff 4: breathes under water.
    pub gills: bool,
    /// Buff 6: `manaRegenBuff`.
    pub mana_regen: bool,
    /// `magicDamage` and `arrowDamage`.
    pub magic_damage: f32,
    pub arrow_damage: f32,
    /// Buff 16: faster arrows.
    pub archery: bool,
    /// `thorns`: share of a monster's contact damage hit back at it.
    pub thorns: f32,
    /// Buff 13: `enemySpawns`, more monsters.
    pub battle: bool,
    /// Buff 10: `invis`, fewer monsters.
    pub invis: bool,
    /// Buff 24: burning (`onFire`).
    pub on_fire: bool,
    /// Buff 12: `nightVision`, light carries further.
    pub night_vision: bool,
    /// Buff 9: `findTreasure`, treasure tiles stand out.
    pub spelunker: bool,
    /// Buff 17: `detectCreature`, monsters stand out.
    pub hunter: bool,
}

/// `ResetEffects`, then `UpdateBuffs` and `ApplyEquipFunctional` for `worn`
/// accessories, with `moveSpeed` applied as `UpdateEquips` ends.
pub fn effects(worn: impl IntoIterator<Item = u16>, buffs: &[Buff]) -> Effects {
    let mut e = Effects {
        moves: Moves::default(),
        life_regen: 0,
        defense: 0,
        pick_speed: 1.0,
        shine: false,
        lava_immune: false,
        gills: false,
        mana_regen: false,
        magic_damage: 1.0,
        arrow_damage: 1.0,
        archery: false,
        thorns: 0.0,
        battle: false,
        invis: false,
        on_fire: false,
        night_vision: false,
        spelunker: false,
        hunter: false,
    };
    for b in buffs.iter().filter(|b| b.time > 0) {
        match b.kind {
            1 => e.lava_immune = true,
            4 => e.gills = true,
            6 => e.mana_regen = true,
            7 => e.magic_damage += 0.2,
            10 => e.invis = true,
            9 => e.spelunker = true,
            12 => e.night_vision = true,
            17 => e.hunter = true,
            18 => e.moves.grav_control = true,
            13 => e.battle = true,
            14 => e.thorns = e.thorns.max(1.0),
            15 => e.moves.water_walk = true,
            16 => {
                e.archery = true;
                e.arrow_damage *= 1.1;
            }
            ON_FIRE => e.on_fire = true,
            2 => e.life_regen += 4,
            3 => e.moves.move_speed += 0.25,
            5 => e.defense += 8,
            8 => e.moves.slow_fall = true,
            11 => e.shine = true,
            104 => e.pick_speed -= 0.25,
            _ => {}
        }
    }
    for id in worn {
        if let Some(s) = crate::item_ids::stats(id) {
            e.life_regen += s.life_regen as i32;
        }
        match id {
            53 => e.moves.cloud = true,
            54 => e.moves.acc_run = 6.0,
            158 => e.moves.no_fall = true,
            159 => e.moves.jump_boost = true,
            285 => e.moves.move_speed += 0.05,
            212 => e.moves.move_speed += 0.1,
            _ => {}
        }
    }
    e.pick_speed = e.pick_speed.max(0.3);
    e
}

/// `AddBuff`: a buff already there keeps the longer time, else it takes the first free place.
pub fn add_buff(buffs: &mut Vec<Buff>, kind: u16, time: i32) {
    if let Some(b) = buffs.iter_mut().find(|b| b.kind == kind) {
        b.time = b.time.max(time);
    } else if buffs.len() < MAX_BUFFS {
        buffs.push(Buff { kind, time });
    }
}

/// Counts every buff down a tick and drops the ones run out.
pub fn tick_buffs(buffs: &mut Vec<Buff>) {
    for b in buffs.iter_mut() {
        b.time -= 1;
    }
    buffs.retain(|b| b.time > 0);
}

/// Top-left of buff icon `i` (`DrawInterface_Resources_Buffs`): 11 a row, 38 apart, rows 50 apart.
pub fn icon_at(i: usize) -> [f32; 2] {
    [32.0 + (i % 11) as f32 * 38.0, 76.0 + (i / 11) as f32 * 50.0]
}

/// The buff icon under `p`, icons `size` wide and high.
pub fn icon_under(p: [f32; 2], count: usize, size: [f32; 2]) -> Option<usize> {
    (0..count).find(|&i| {
        let [x, y] = icon_at(i);
        p[0] > x && p[0] < x + size[0] && p[1] > y && p[1] < y + size[1]
    })
}

/// `Lang.LocalizedDuration` abbreviated with only the largest unit: whole
/// minutes count one more past the first, as the game does. `None` for 2
/// ticks or less, which show no time.
pub fn duration(ticks: i32, short_min: &str, short_sec: &str) -> Option<String> {
    if ticks <= 2 {
        return None;
    }
    let secs = ticks / 60;
    let (m, s) = (secs / 60, secs % 60);
    Some(if m > 0 { format!("{} {short_min}", if secs > 60 { m + 1 } else { m }) } else { format!("{s} {short_sec}") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessories_and_buffs_add_up() {
        let e = effects([54, 285, 212, 158, 49], &[Buff { kind: 3, time: 10 }, Buff { kind: 5, time: 10 }]);
        assert!((e.moves.move_speed - 1.4).abs() < 1e-6);
        assert_eq!((e.moves.acc_run, e.moves.no_fall, e.moves.cloud), (6.0, true, false));
        assert_eq!(e.defense, 8);
        assert_eq!(e.life_regen, crate::item_ids::stats(49).unwrap().life_regen as i32);
        assert!(e.life_regen > 0);
        assert_eq!(effects([], &[Buff { kind: 104, time: 1 }]).pick_speed, 0.75);
        let e = effects([], &[9, 10, 17, 18].map(|kind| Buff { kind, time: 1 }));
        assert!(e.spelunker && e.invis && e.hunter && e.moves.grav_control);
    }

    #[test]
    fn buffs_keep_the_longer_time_and_run_out() {
        let mut b = Vec::new();
        add_buff(&mut b, 3, 100);
        add_buff(&mut b, 3, 50);
        add_buff(&mut b, 5, 1);
        assert_eq!(b, [Buff { kind: 3, time: 100 }, Buff { kind: 5, time: 1 }]);
        tick_buffs(&mut b);
        assert_eq!(b, [Buff { kind: 3, time: 99 }]);
    }

    #[test]
    fn durations_show_the_largest_unit() {
        assert_eq!(duration(28800, "m", "s").as_deref(), Some("9 m"));
        assert_eq!(duration(3600, "m", "s").as_deref(), Some("1 m"));
        assert_eq!(duration(59 * 60 + 30, "m", "s").as_deref(), Some("59 s"));
        assert_eq!(duration(2, "m", "s"), None);
        assert_eq!(icon_at(12), [70.0, 126.0]);
    }

    #[test]
    fn potions_give_their_buff() {
        for (id, buff) in [(289, 2), (290, 3), (292, 5), (295, 8), (298, 11), (2322, 104)] {
            let s = crate::item_ids::stats(id).unwrap();
            assert_eq!(s.buff_type, buff, "{id}");
            assert!(s.buff_time > 0);
        }
        for id in [49, 53, 54, 158, 159, 212, 285] {
            assert!(crate::item_ids::stats(id).unwrap().flags & crate::item_ids::ACCESSORY != 0, "{id}");
        }
    }
}
