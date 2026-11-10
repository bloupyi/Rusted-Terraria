//! Biome zones from the tiles around a point, as the game's scene scan counts
//! them: a 169 x 124 tile box (a 1920 x 1200 screen plus 25 tiles each side).

use formats::wld::Env;

use crate::{World, NO_TILE};

const SCAN: (i64, i64) = (1920 / 16 + 49, 1200 / 16 + 49);
const BEACH: i64 = 380;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Zones {
    pub snow_tiles: i32,
    pub jungle_tiles: i32,
    /// Hallow, corruption and crimson counts after cancelling each other.
    pub holy_tiles: i32,
    pub evil_tiles: i32,
    pub blood_tiles: i32,
    pub honey_tiles: i32,
    pub snow: bool,
    pub jungle: bool,
    pub corrupt: bool,
    pub crimson: bool,
    pub hallow: bool,
    pub glowshroom: bool,
    pub desert: bool,
    pub beach: bool,
}

/// Zones around tile `(x, y)`.
pub fn scan(world: &World, env: &Env, x: i64, y: i64) -> Zones {
    let (w, h) = (world.width as i64, world.height as i64);
    // The game clamps both coordinates against the world width.
    let (x, y) = (x.clamp(0, w - 1), y.clamp(0, w - 1));
    let (x0, y0) = (x - SCAN.0 / 2, y - SCAN.1 / 2);
    let ocean = (env.world_surface + env.rock_layer) / 2.0 + 40.0;
    let ocean_depths = |i: i64, j: i64| j as f64 <= ocean && (i < BEACH || i > w - BEACH);
    let mut n = [0i32; 1024];
    let mut ocean_sand = 0;
    for i in x0.clamp(0, w)..(x0 + SCAN.0).clamp(0, w) {
        for j in y0.clamp(0, h)..(y0 + SCAN.1).clamp(0, h) {
            let t = world.tile(i, j);
            if t != NO_TILE && (t as usize) < n.len() {
                n[t as usize] += 1;
                if matches!(t, 53 | 397 | 396 | 400 | 403 | 401) && ocean_depths(i, j) {
                    ocean_sand += 1;
                }
            }
        }
    }
    let sum = |ids: &[usize]| ids.iter().map(|&i| n[i]).sum::<i32>();
    let sunflowers = n[27] * -10;
    let holy = sum(&[109, 492, 110, 113, 117, 116, 164, 403, 402]).max(0);
    let snow = sum(&[147, 148, 161, 162, 164, 163, 200]);
    let jungle = sum(&[60, 61, 62, 74, 226, 225]);
    let evil = (sum(&[23, 661, 24, 25, 32, 112, 163, 400, 398]) + sunflowers).max(0);
    let blood = (sum(&[199, 662, 201, 203, 200, 401, 399, 234, 352]) + sunflowers).max(0);
    let mushroom = sum(&[70, 71, 72, 528]);
    let sand = sum(&[53, 112, 116, 234, 397, 398, 402, 399, 396, 400, 403, 401]);
    let underworld = y > h - 200;
    let (holy, evil, blood) = ((holy - evil - blood).max(0), (evil - holy).max(0), (blood - holy).max(0));
    Zones {
        snow_tiles: snow,
        jungle_tiles: jungle,
        holy_tiles: holy,
        evil_tiles: evil,
        blood_tiles: blood,
        honey_tiles: n[229],
        snow: snow >= 1500,
        jungle: jungle >= 140 && !underworld,
        corrupt: evil >= 300,
        crimson: blood >= 300,
        hallow: holy >= 125,
        glowshroom: mushroom >= 100,
        desert: (sand - ocean_sand).max(0) >= 1500,
        beach: ocean_depths(x, y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::wld::Tile;

    fn world(f: impl Fn(u32, u32) -> u16) -> World {
        World::parse(&formats::synth::write_world("t", 1000, 400, &[], |x, y| Tile {
            tile: f(x, y),
            ..Tile::default()
        }))
        .unwrap()
    }

    #[test]
    fn counts_pick_zones() {
        let env = Env { world_surface: 100.0, rock_layer: 200.0, ..Env::default() };
        // 40 x 40 of snow is 1600 tiles; 10 x 10 of mushroom grass 100.
        let w = world(|x, y| match (x, y) {
            (480..520, 280..320) => 147,
            (500..510, 330..340) => 70,
            _ => NO_TILE,
        });
        let z = scan(&w, &env, 500, 300);
        assert!(z.snow && z.glowshroom && !z.jungle && !z.beach);
        assert_eq!(z.snow_tiles, 1600);
        // Far from the patch nothing is counted; near the edge above the ocean level is beach.
        let z = scan(&w, &env, 100, 100);
        assert!(!z.snow && z.beach);
    }

    #[test]
    fn hallow_cancels_evil() {
        let env = Env::default();
        let w = world(|x, y| match (x, y) {
            (480..500, 280..300) => 23,
            (500..520, 280..300) => 109,
            _ => NO_TILE,
        });
        let z = scan(&w, &env, 500, 290);
        assert!(!z.corrupt && !z.hallow);
    }
}
