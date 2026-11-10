//! Mining as the game's `Player.GetPickaxeDamage` and `HitTile` do it: each
//! hit adds damage to the tile or wall, 100 breaks it, and damage left on
//! other tiles wears off as more hits land.

use crate::tile_sets::{CLOUDS, DUNGEON, NO_FAIL, ROPE};

/// What a hit is on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Tile,
    Wall,
}

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub x: i64,
    pub y: i64,
    pub target: Target,
    pub damage: i32,
    ttl: i32,
    /// Which of the four crack drawings, never the same twice in a row.
    pub crack: u8,
}

const MAX: usize = 500;
const TTL: i32 = 60;

/// Damage on recently hit tiles and walls, most recent last.
#[derive(Default)]
pub struct HitTiles {
    hits: Vec<Hit>,
    last_crack: Option<u8>,
    seed: u32,
}

impl HitTiles {
    pub fn iter(&self) -> impl Iterator<Item = &Hit> {
        self.hits.iter()
    }

    pub fn damage(&self, x: i64, y: i64, target: Target) -> i32 {
        self.find(x, y, target).map_or(0, |i| self.hits[i].damage)
    }

    fn find(&self, x: i64, y: i64, target: Target) -> Option<usize> {
        self.hits.iter().position(|h| (h.x, h.y, h.target) == (x, y, target))
    }

    /// Damage the tile would have after `amount` more, without adding it.
    pub fn would_have(&self, x: i64, y: i64, target: Target, amount: i32) -> i32 {
        self.damage(x, y, target) + amount
    }

    /// Adds `amount` and returns the damage now on the tile. A first hit of 0 leaves nothing.
    pub fn add(&mut self, x: i64, y: i64, target: Target, amount: i32) -> i32 {
        let i = match self.find(x, y, target) {
            Some(i) => i,
            None if amount == 0 => return 0,
            None => {
                if self.hits.len() == MAX {
                    self.hits.remove(0);
                }
                let crack = self.next_crack();
                self.hits.push(Hit { x, y, target, damage: 0, ttl: 0, crack });
                self.hits.len() - 1
            }
        };
        let mut h = self.hits.remove(i);
        h.damage += amount;
        h.ttl = TTL;
        self.hits.push(h);
        h.damage
    }

    pub fn clear(&mut self, x: i64, y: i64, target: Target) {
        self.hits.retain(|h| (h.x, h.y, h.target) != (x, y, target));
    }

    /// One step of wear on every entry; `gone` says the tile or wall is no longer there.
    pub fn prune(&mut self, gone: impl Fn(&Hit) -> bool) {
        self.hits.retain_mut(|h| {
            if h.ttl <= 1 {
                return false;
            }
            h.ttl -= 1;
            h.damage -= match h.ttl {
                0..12 => 10,
                12..24 => 7,
                24..36 => 5,
                36..48 => 2,
                _ => 0,
            };
            h.damage >= 0 && !gone(h)
        });
    }

    fn next_crack(&mut self) -> u8 {
        loop {
            self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let c = (self.seed >> 16) as u8 % 4;
            if self.last_crack != Some(c) {
                self.last_crack = Some(c);
                return c;
            }
        }
    }
}

/// Where a pickaxe hit lands, for the depth rules.
pub struct Place {
    pub x: i64,
    pub y: i64,
    pub world_width: u32,
    pub world_surface: f64,
    pub underworld: i64,
}

/// `GetPickaxeDamage`: damage a pickaxe of `pick` power does to tile `t`; 0 when it is too weak.
pub fn pick_damage(t: u16, frame_y: i16, pick: i32, at: &Place) -> i32 {
    let mut n = if NO_FAIL.has(t) { 100 } else { 0 };
    n += if DUNGEON.has(t) || matches!(t, 58 | 25 | 117 | 203) {
        pick / 2
    } else if t == 85 {
        pick
    } else if t == 48 || t == 232 || CLOUDS.has(t) {
        pick * 2
    } else if t == 226 {
        pick / 4
    } else if t == 107 || t == 221 {
        pick / 2
    } else if t == 108 || t == 222 {
        pick / 3
    } else if t == 111 || t == 223 {
        pick / 4
    } else if t == 211 {
        pick / 5
    } else {
        pick
    };
    let below_surface = at.y as f64 > at.world_surface;
    if t == 211 && pick < 200 {
        n = 0;
    }
    let too_weak = match t {
        25 | 203 | 117 => pick < 65,
        37 => pick < 50,
        22 | 204 => below_surface && pick < 55,
        56 => pick < 55,
        77 => pick < 65 && at.y >= at.underworld,
        58 => pick < 65,
        226 | 237 => pick < 210,
        137 => pick < 210 && (1..=4).contains(&(frame_y / 18)),
        t if DUNGEON.has(t) => {
            let w = at.world_width as f64;
            pick < 100 && below_surface && ((at.x as f64) < w * 0.35 || at.x as f64 > w * 0.65)
        }
        107 | 221 => pick < 100,
        108 | 222 => pick < 110,
        111 | 223 => pick < 150,
        _ => false,
    };
    if too_weak {
        n = 0;
    }
    if matches!(t, 147 | 0 | 40 | 53 | 57 | 59 | 123 | 224 | 397) {
        n += pick;
    }
    if t == 404 {
        n += 5;
    }
    if t == 165 || ROPE.has(t) || t == 199 {
        n = 100;
    }
    n
}

/// Grass and moss that a finishing hit turns back into their base block
/// (`DoesPickTargetTransformOnKill`).
pub fn transforms(t: u16) -> bool {
    use crate::tile_sets::{MOSS, MOSS_BRICK};
    matches!(t, 2 | 477 | 492 | 23 | 60 | 70 | 109 | 199 | 662 | 661 | 633) || MOSS.has(t) || MOSS_BRICK.has(t)
}

/// What a hit that does not break tile `t` leaves there (`KillTile` with `fail`).
pub fn after_failed_hit(t: u16) -> u16 {
    use crate::tile_sets::{MOSS, MOSS_BRICK};
    match t {
        2 | 23 | 109 | 199 | 477 | 492 => 0,
        633 => 57,
        60 | 661 | 662 | 70 => 59,
        t if MOSS.has(t) => 1,
        t if MOSS_BRICK.has(t) => 38,
        t => t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Place = Place { x: 100, y: 100, world_width: 1000, world_surface: 200.0, underworld: 900 };

    #[test]
    fn damage_follows_the_tile() {
        assert_eq!(pick_damage(1, 0, 35, &AT), 35, "stone");
        assert_eq!(pick_damage(0, 0, 35, &AT), 70, "dirt counts twice");
        assert_eq!(pick_damage(4, 0, 35, &AT), 135, "torches never fail");
        assert_eq!(pick_damage(58, 0, 55, &AT), 0, "hellstone needs 65");
        assert_eq!(pick_damage(58, 0, 65, &AT), 32);
        let deep = Place { y: 300, ..AT };
        assert_eq!(pick_damage(22, 0, 40, &AT), 40, "demonite above the surface");
        assert_eq!(pick_damage(22, 0, 40, &deep), 0, "and not below");
    }

    #[test]
    fn damage_wears_off_as_other_tiles_are_hit() {
        let mut h = HitTiles::default();
        assert_eq!(h.add(1, 1, Target::Tile, 0), 0, "no entry for nothing");
        assert_eq!(h.add(1, 1, Target::Tile, 35), 35);
        assert_eq!(h.add(1, 1, Target::Tile, 35), 70);
        for _ in 0..12 {
            h.prune(|_| false);
        }
        assert_eq!(h.damage(1, 1, Target::Tile), 70, "full for 12 more hits");
        h.prune(|_| false);
        assert_eq!(h.damage(1, 1, Target::Tile), 68);
        for _ in 0..40 {
            h.prune(|_| false);
        }
        assert_eq!(h.damage(1, 1, Target::Tile), 0, "worn off");
        h.add(2, 2, Target::Wall, 10);
        h.prune(|h| h.target == Target::Wall);
        assert_eq!(h.iter().count(), 0, "the wall went away");
    }
}
