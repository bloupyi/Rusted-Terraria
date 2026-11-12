//! Which critters `SpawnAnNPC` puts on a spot, the night's odds of
//! `setFireFlyChance`, and the bug net.

use world::mob::Kind;

/// `Item_1991`: the bug net, which catches critters (`ItemCheck_CatchCritters`).
pub const BUG_NET: u16 = 1991;

/// The net and the caught critters' pictures.
pub const SHEETS: [&str; 21] = [
    "Item_1991",
    "Item_2019",
    "Item_2018",
    "Item_2003",
    "Item_2015",
    "Item_2016",
    "Item_2017",
    "Item_261",
    "Item_1992",
    "Item_2004",
    "Item_1994",
    "Item_1995",
    "Item_1996",
    "Item_1997",
    "Item_1998",
    "Item_1999",
    "Item_2000",
    "Item_2001",
    "Item_2002",
    "Item_2006",
    "Item_2007",
];

/// `fireFlyChance`, `fireFlyFriendly`, `fireFlyMultiple` and `butterflyChance`,
/// rolled at dusk; 0 until the first one, which `Main.rand.Next(0)` always passes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Odds {
    pub firefly: i32,
    pub firefly_friendly: i32,
    pub firefly_multiple: i32,
    pub butterfly: i32,
}

/// `Main.rand.Next(lo, hi)` from `r(n)` = `Main.rand.Next(n)`.
fn range(r: &mut impl FnMut(i32) -> i32, lo: i32, hi: i32) -> i32 {
    lo + r(hi - lo)
}

impl Odds {
    /// `NPC.setFireFlyChance`, without the stink bugs' share.
    pub fn roll(r: &mut impl FnMut(i32) -> i32) -> Self {
        let (firefly, firefly_friendly, firefly_multiple) = if r(9) == 0 {
            (range(r, 5, 10), range(r, 1, 4), range(r, 3, 8))
        } else if r(3) == 0 {
            (999_999, 999_999, 999_999)
        } else {
            (range(r, 10, 60), range(r, 2, 15), range(r, 6, 30))
        };
        let butterfly = if r(3) == 0 {
            999_999
        } else {
            let b = range(r, 1, 21);
            if r(5) == 0 {
                999_999
            } else {
                b
            }
        };
        Odds { firefly, firefly_friendly, firefly_multiple, butterfly }
    }
}

/// Where a spawn is tried: the block under it and what is around.
#[derive(Clone, Copy, Debug)]
pub struct Spot {
    pub tile: u16,
    /// Liquid over the block.
    pub water: bool,
    /// At or above `worldSurface`.
    pub surface: bool,
    /// Below `rockLayer`.
    pub caverns: bool,
    /// Tiles from the world's spawn point.
    pub from_spawn: i64,
    pub world_width: i64,
    pub day: bool,
    /// Ticks since dawn or dusk.
    pub time: f64,
    pub raining: bool,
    /// `TooWindyForButterflies`: `|windSpeedTarget| >= 0.4`.
    pub windy: bool,
    /// Birds about (`CountNPCS(74) + CountNPCS(297) + CountNPCS(298)`).
    pub birds: usize,
}

/// A critter and where it goes from the spot's feet, in pixels.
pub type Pick = (Kind, f32, f32);

fn grass(t: u16) -> bool {
    matches!(t, 2 | 109 | 477 | 492)
}

/// Birds: a blue jay or a cardinal one time in four each, else a bird.
fn bird(r: &mut impl FnMut(i32) -> i32) -> Kind {
    match r(4) {
        0 => Kind::BlueJay,
        1 => Kind::Cardinal,
        _ => Kind::Bird,
    }
}

/// A firefly (a lightning bug on hallowed grass) and up to four more around it.
fn fireflies(s: &Spot, multiple: i32, r: &mut impl FnMut(i32) -> i32) -> Vec<Pick> {
    let kind = if s.tile == 109 { Kind::LightningBug } else { Kind::Firefly };
    let mut v = vec![(kind, 0.0, 0.0)];
    for at in [(-16.0, 0.0), (16.0, 0.0), (0.0, -16.0), (0.0, 16.0)] {
        if r(multiple) == 0 {
            v.push((kind, at.0, at.1));
        }
    }
    v
}

/// A butterfly and maybe one either side.
fn butterflies(r: &mut impl FnMut(i32) -> i32) -> Vec<Pick> {
    let mut v = vec![(Kind::Butterfly, 0.0, 0.0)];
    for dx in [-16.0, 16.0] {
        if r(4) == 0 {
            v.push((Kind::Butterfly, dx, 0.0));
        }
    }
    v
}

/// The critters of `SpawnAnNPC` with `spawnFriendly` (town folks near): on
/// the grasses, and below the surface on any block, by weather and hour.
pub fn friendly(s: &Spot, odds: &Odds, r: &mut impl FnMut(i32) -> i32) -> Vec<Pick> {
    if s.water {
        // By day on the surface two times in three it is ducks.
        if s.surface && s.day && r(3) != 0 {
            return Vec::new();
        }
        return vec![(Kind::Goldfish, 0.0, 0.0)];
    }
    if !grass(s.tile) && s.surface {
        return Vec::new();
    }
    if s.raining {
        return if r(3) != 0 { vec![(Kind::Worm, 0.0, 0.0)] } else { Vec::new() };
    }
    if !s.windy && !s.day && r(odds.firefly_friendly) == 0 && s.surface {
        return fireflies(s, odds.firefly_multiple, r);
    }
    if !s.day && r(5) == 0 && s.surface {
        // The owls' turn.
        return Vec::new();
    }
    if s.day && s.time < 18000.0 && r(3) != 0 && s.surface {
        return vec![(bird(r), 0.0, 0.0)];
    }
    if !s.windy && s.day && r(odds.butterfly) == 0 && s.surface {
        return butterflies(r);
    }
    if r(2) == 0 && s.surface {
        return vec![(bird(r), 0.0, 0.0)];
    }
    if s.caverns {
        return Vec::new();
    }
    if r(3) == 0 {
        return if s.surface { vec![(Kind::Squirrel, 0.0, 0.0)] } else { Vec::new() };
    }
    vec![(Kind::Bunny, 0.0, 0.0)]
}

/// The critters of the ordinary surface spawns by day, which come before the
/// slimes: bunnies, squirrels and butterflies on grass, birds in the morning.
pub fn surface_day(s: &Spot, odds: &Odds, r: &mut impl FnMut(i32) -> i32) -> Vec<Pick> {
    let near = |part: i64| s.from_spawn < s.world_width / part;
    if !s.water && near(2) && r(15) == 0 && (grass(s.tile) || matches!(s.tile, 147 | 161)) {
        if matches!(s.tile, 147 | 161) {
            // Penguins.
            return Vec::new();
        }
        if !s.windy && !s.raining && r(odds.butterfly) == 0 {
            return butterflies(r);
        }
        if r(3) == 0 && s.surface {
            return vec![(Kind::Squirrel, 0.0, 0.0)];
        }
        return vec![(Kind::Bunny, 0.0, 0.0)];
    }
    if !s.water && near(3) && s.time < 18000.0 && grass(s.tile) && r(4) == 0 && s.surface && s.birds < 6 {
        return vec![(bird(r), 0.0, 0.0)];
    }
    if !s.water && near(3) && r(15) == 0 && (grass(s.tile) || s.tile == 147) {
        return vec![(bird(r), 0.0, 0.0)];
    }
    Vec::new()
}

/// The fireflies of the ordinary surface spawns at night, before the monsters.
pub fn surface_night(s: &Spot, odds: &Odds, r: &mut impl FnMut(i32) -> i32) -> Vec<Pick> {
    if !s.windy && grass(s.tile) && !s.raining && r(odds.firefly) == 0 && s.surface {
        return fireflies(s, odds.firefly_multiple, r);
    }
    Vec::new()
}

/// The glowing snails among the mushrooms on the surface.
pub fn mushroom(s: &Spot, r: &mut impl FnMut(i32) -> i32) -> Vec<Pick> {
    if s.tile == 70 && s.surface && r(3) != 0 && r(6) == 0 {
        return vec![(Kind::GlowingSnail, 0.0, 0.0)];
    }
    Vec::new()
}

/// The worms, mice and snails of the ordinary spawns underground, away from
/// the evil and the snow, before the monsters.
pub fn underground(deep_half: bool, r: &mut impl FnMut(i32) -> i32) -> Vec<Pick> {
    if r(8) == 0 {
        return vec![(Kind::Worm, 0.0, 0.0)];
    }
    if r(13) == 0 {
        return vec![(Kind::Mouse, 0.0, 0.0)];
    }
    if deep_half && r(13) == 0 {
        return vec![(Kind::Snail, 0.0, 0.0)];
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spot() -> Spot {
        Spot {
            tile: 2,
            water: false,
            surface: true,
            caverns: false,
            from_spawn: 10,
            world_width: 4200,
            day: true,
            time: 30000.0,
            raining: false,
            windy: false,
            birds: 0,
        }
    }

    fn rng(seed: u64) -> impl FnMut(i32) -> i32 {
        let mut s = seed;
        move |n| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            if n <= 0 {
                0
            } else {
                (s % n as u64) as i32
            }
        }
    }

    fn count(f: impl Fn(&mut dyn FnMut(i32) -> i32) -> Vec<Pick>, kind: Kind) -> usize {
        let mut r = rng(7);
        (0..3000).map(|_| f(&mut r).iter().filter(|p| p.0 == kind).count()).sum()
    }

    #[test]
    fn town_spawns_by_hour_and_weather() {
        let odds = Odds { firefly: 20, firefly_friendly: 3, firefly_multiple: 10, butterfly: 5 };
        let s = spot();
        let morning = Spot { time: 1000.0, ..s };
        let night = Spot { day: false, ..s };
        let wet = Spot { raining: true, ..s };
        let birds = |s: Spot| {
            [Kind::Bird, Kind::BlueJay, Kind::Cardinal]
                .iter()
                .map(|&k| count(|r| friendly(&s, &odds, &mut |n| r(n)), k))
                .sum::<usize>()
        };
        assert!(birds(morning) > birds(s), "birds sing in the morning");
        assert!(count(|r| friendly(&s, &odds, &mut |n| r(n)), Kind::Bunny) > 100);
        assert!(count(|r| friendly(&s, &odds, &mut |n| r(n)), Kind::Butterfly) > 50);
        assert_eq!(count(|r| friendly(&night, &odds, &mut |n| r(n)), Kind::Butterfly), 0);
        assert!(count(|r| friendly(&night, &odds, &mut |n| r(n)), Kind::Firefly) > 500);
        assert!(count(|r| friendly(&wet, &odds, &mut |n| r(n)), Kind::Worm) > 1500);
        let pond = Spot { water: true, ..s };
        assert_eq!(friendly(&pond, &odds, &mut rng(1)), vec![(Kind::Goldfish, 0.0, 0.0)]);
        let hallow = Spot { tile: 109, day: false, ..s };
        assert!(count(|r| friendly(&hallow, &odds, &mut |n| r(n)), Kind::LightningBug) > 0);
    }

    #[test]
    fn wild_spawns_keep_their_odds() {
        let odds = Odds { firefly: 10, firefly_friendly: 3, firefly_multiple: 999_999, butterfly: 999_999 };
        let s = spot();
        // One time in 15: two in three bunnies, one in three squirrels.
        let bunnies = count(|r| surface_day(&s, &odds, &mut |n| r(n)), Kind::Bunny);
        let squirrels = count(|r| surface_day(&s, &odds, &mut |n| r(n)), Kind::Squirrel);
        assert!((100..300).contains(&bunnies) && squirrels < bunnies, "{bunnies} {squirrels}");
        assert_eq!(count(|r| surface_day(&s, &odds, &mut |n| r(n)), Kind::Butterfly), 0);
        let far = Spot { from_spawn: 3000, ..s };
        assert_eq!(count(|r| surface_day(&far, &odds, &mut |n| r(n)), Kind::Bunny), 0);
        let night = Spot { day: false, ..s };
        let flies = count(|r| surface_night(&night, &odds, &mut |n| r(n)), Kind::Firefly);
        assert!((200..400).contains(&flies), "{flies}");
        let windy = Spot { windy: true, ..night };
        assert_eq!(count(|r| surface_night(&windy, &odds, &mut |n| r(n)), Kind::Firefly), 0);
        let cave = Spot { surface: false, tile: 1, ..s };
        assert!(count(|r| underground(true, &mut |n| r(n)), Kind::Worm) > 300);
        assert!(count(|r| underground(false, &mut |n| r(n)), Kind::Snail) == 0);
        let shrooms = Spot { tile: 70, ..s };
        assert!(count(|r| mushroom(&shrooms, &mut |n| r(n)), Kind::GlowingSnail) > 100);
        assert_eq!(count(|r| mushroom(&cave, &mut |n| r(n)), Kind::GlowingSnail), 0);
    }

    #[test]
    fn odds_are_rolled_at_dusk() {
        let mut r = rng(3);
        let all: Vec<_> = (0..500).map(|_| Odds::roll(&mut |n| r(n))).collect();
        assert!(all.iter().all(|o| (5..60).contains(&o.firefly) || o.firefly == 999_999));
        assert!(all.iter().any(|o| o.butterfly == 999_999) && all.iter().any(|o| o.butterfly < 21));
    }
}
