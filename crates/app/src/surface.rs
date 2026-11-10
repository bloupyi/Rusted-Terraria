//! Surface background layers of every biome, faded in and out as the game
//! does when the view moves between areas.

use formats::wld::Env;
use render::{Sprite, SPRITE_FLIP};
use world::scene::Zones;

use crate::backdrop::{forest_set, ieee_rem, mountain_offset, Scene, View};

const SPEED: f32 = 0.05;
const GLOBAL_SCALE: f32 = 2.0;
const BACK_PUSH: f32 = 30.0;
const FRONT_PUSH: f32 = BACK_PUSH - 180.0;

fn is_forest(style: usize) -> bool {
    matches!(style, 0 | 10 | 11 | 12)
}

/// The world's style for a background area, to tell forests apart.
fn area_style(env: &Env, style: usize) -> i32 {
    match style {
        0 => env.tree_bg[0] as i32,
        10..=12 => env.tree_bg[style - 9] as i32,
        1 => env.biome_bg[0] as i32,
        3 => env.biome_bg[1] as i32,
        7 => env.biome_bg[2] as i32,
        6 => env.biome_bg[3] as i32,
        8 => env.biome_bg[4] as i32,
        2 => env.biome_bg[5] as i32,
        4 => env.biome_bg[6] as i32,
        9 => env.biome_bg[7] as i32,
        _ => -1,
    }
}

/// Surface background area for zones around the view centred on column `x`.
pub fn preferred(env: &Env, z: &Zones, x: i32) -> usize {
    if z.beach {
        if z.hallow {
            6
        } else if z.corrupt {
            if z.blood_tiles <= z.evil_tiles {
                1
            } else {
                8
            }
        } else if z.crimson {
            8
        } else if z.honey_tiles <= 400 {
            4
        } else {
            3
        }
    } else if z.glowshroom {
        9
    } else if z.desert {
        if z.corrupt && z.evil_tiles > z.blood_tiles && z.evil_tiles > z.holy_tiles {
            5
        } else if z.crimson && z.blood_tiles > z.holy_tiles {
            14
        } else if z.hallow {
            13
        } else {
            2
        }
    } else if z.hallow {
        6
    } else if z.corrupt {
        if z.blood_tiles <= z.evil_tiles {
            1
        } else {
            8
        }
    } else if z.crimson {
        8
    } else if z.jungle {
        3
    } else if z.snow {
        7
    } else if x < env.tree_x[0] {
        0
    } else if x < env.tree_x[1] {
        10
    } else if x >= env.tree_x[2] {
        12
    } else {
        11
    }
}

/// Desert layers: pure, corrupt, hallow, crimson, or `None` when the style has no such variant.
fn desert_set(bg: u8, area: usize) -> Option<[i32; 3]> {
    let k = match bg {
        51..=53 => 303 + (bg - 51) as i32,
        _ if area != 2 => return None,
        1 => return Some([108, 109, -1]),
        2 => return Some([207, 208, -1]),
        3 => return Some([217, 218, -1]),
        4 => return Some([248, 249, 250]),
        _ => return Some([21, 20, -1]),
    };
    let v = match area {
        5 => 1,
        13 => 2,
        14 => 3,
        _ => 0,
    };
    Some([306 + 4 * v, k + 4 * v, -1])
}

fn desert_swap(env: &Env, target: usize, from: usize) -> bool {
    let variant = |s| matches!(s, 2 | 5 | 13 | 14);
    target != from
        && variant(target)
        && variant(from)
        && desert_set(env.biome_bg[5], target).is_some()
        && desert_set(env.biome_bg[5], from).is_some()
}

/// Far mountain area a background area shows, and a second one for mushrooms.
fn far_area(env: &Env, style: usize) -> (usize, Option<usize>) {
    match style {
        2 => (1, None),
        3 if env.biome_bg[1] == 5 => (15, None),
        1 | 5 | 13 | 14 => (2, None),
        6 if env.biome_bg[3] == 3 => (6, None),
        6 => (2, None),
        4 => (3, None),
        7 => (4, None),
        8 => (5, None),
        9 => (0, Some(6)),
        10..=12 => (style, None),
        _ => (0, None),
    }
}

/// Current area and the opacity of each area's near and far layers.
pub struct Fade {
    style: usize,
    delay: i32,
    instant: i32,
    front: [f32; 16],
    back: [f32; 16],
}

impl Default for Fade {
    fn default() -> Self {
        Self { style: 0, delay: 0, instant: 10, front: [0.0; 16], back: [0.0; 16] }
    }
}

impl Fade {
    /// One frame towards area `want`.
    pub fn step(&mut self, env: &Env, want: usize) {
        if self.delay < 0 {
            self.delay += 1;
        } else if want != self.style {
            self.transition(env, want);
        }
        let mut speed = SPEED;
        if self.instant > 0 {
            self.instant -= 1;
            self.style = want;
            speed = 1.0;
        }
        let (far, far2) = far_area(env, self.style);
        for i in 0..16 {
            let on = i == far || Some(i) == far2;
            if on || (self.back[i] != 0.0 && !(desert_swap(env, far, i) && self.back[far] < 1.0)) {
                self.back[i] = (self.back[i] + if on { speed } else { -speed }).clamp(0.0, 1.0);
            }
        }
        let s = self.style;
        for i in 0..16 {
            if i == s {
                self.front[i] = (self.front[i] + speed).min(1.0);
            } else if self.front[i] != 0.0 && !(desert_swap(env, s, i) && self.front[s] < 1.0) {
                self.front[i] = (self.front[i] - speed).max(0.0);
            }
        }
    }

    fn transition(&mut self, env: &Env, want: usize) {
        if is_forest(self.style) && is_forest(want) && area_style(env, self.style) == area_style(env, want) {
            self.instant = 1;
            return;
        }
        self.delay += 1;
        if self.delay > if is_forest(want) { 60 } else { 30 } {
            self.delay = if is_forest(want) { 0 } else { -60 };
            self.style = want;
        }
    }
}

/// `(scale, parallax, top a, top b)` overrides of a layer texture.
fn offsets(tex: i32) -> (Option<f32>, Option<f64>, Option<(f32, f32)>) {
    let (s, p, t): (f32, f64, (f32, f32)) = match tex {
        258 => (1.25, 0.23, (1800.0, 850.0)),
        259 => (1.31, 0.33, (1950.0, 1500.0)),
        260 => (1.34, 0.4, (2100.0, 1650.0)),
        263 => (1.25, 0.23, (1800.0, 1450.0)),
        264 => (1.31, 0.33, (1950.0, 1700.0)),
        265 => (1.34, 0.4, (2100.0, 2000.0)),
        267 => (1.25, 0.23, (1700.0, 1300.0)),
        266 => (1.31, 0.33, (1950.0, 1600.0)),
        268 => (1.34, 0.41, (2100.0, 1850.0)),
        248 => (1.3, 0.37, (1800.0, 1100.0)),
        249 => (1.4, 0.43, (1950.0, 1200.0)),
        250 => (1.7, 0.49, (2000.0, 1000.0)),
        255 => (1.15, 0.25, (1800.0, 1450.0)),
        256 => (1.21, 0.32, (1950.0, 1550.0)),
        257 => (1.34, 0.4, (2100.0, 1550.0)),
        240 => (1.15, 0.3, (1800.0, 1500.0)),
        241 => (1.21, 0.43, (1950.0, 1300.0)),
        242 => (1.34, 0.49, (2100.0, 1400.0)),
        243 => (1.15, 0.25, (1800.0, 1400.0)),
        244 => (1.21, 0.35, (1950.0, 1550.0)),
        245 => (1.24, 0.45, (2100.0, 1650.0)),
        279 => (2.5, 0.3499999940395355, (1850.0, 1750.0)),
        282 => (2.6, 0.25, (1800.0, 1340.0)),
        298 => (1.25, 0.3499999940395355, (2000.0, 1700.0)),
        300 => (2.41, 0.5299999713897705, (3000.0, 2050.0)),
        301 => (2.36, 0.41499999165534973, (2200.0, 1700.0)),
        302 => (2.25, 0.23999999463558197, (1400.0, 950.0)),
        337 => (1.01, 0.3499999940395355, (2500.0, 2200.0)),
        338 => (0.98, 0.23999999463558197, (1900.0, 1650.0)),
        339 => (0.94, 0.20000000298023224, (1600.0, 1350.0)),
        328 => return (Some(2.01), Some(0.3400000035762787), None),
        329 => return (Some(2.29), Some(0.25999999046325684), None),
        333 => return (Some(2.01), Some(0.3700000047683716), None),
        _ => {
            let (p, t) = match tex {
                273 => (Some(0.49000000953674316), Some((2100.0, 1560.0))),
                234 => (Some(0.23), Some((1700.0, 1150.0))),
                235 => (Some(0.33), Some((1950.0, 1550.0))),
                236 => (Some(0.41), Some((2100.0, 2000.0))),
                261 => (Some(0.27), None),
                262 => (Some(0.4), None),
                219 => (Some(0.25), None),
                220 => (Some(0.34), None),
                221 => (Some(0.43), None),
                222 => (None, Some((1800.0, 1400.0))),
                223 => (None, Some((2150.0, 1850.0))),
                224 => (None, Some((2500.0, 2400.0))),
                237 => (None, Some((1800.0, 1500.0))),
                238 => (None, Some((1950.0, 1500.0))),
                239 => (None, Some((2100.0, 1900.0))),
                284 => (Some(0.23), Some((1600.0, 900.0))),
                285 => (Some(0.36), Some((1900.0, 960.0))),
                286 => (Some(0.42), Some((2100.0, 1200.0))),
                287 => (Some(0.21), Some((1700.0, 1560.0))),
                288 => (Some(0.33), Some((1950.0, 1730.0))),
                289 => (Some(0.41), Some((2100.0, 1400.0))),
                207 => (Some(0.32), None),
                299 => (Some(0.2800000011920929), Some((1600.0, 1400.0))),
                303..=305 | 307..=309 | 311..=313 | 315..=317 => (Some(0.3499999940395355), Some((2400.0, 1900.0))),
                306 | 310 | 314 | 318 => (Some(0.25999999046325684), Some((1800.0, 1700.0))),
                319 => (Some(0.6000000238418579), Some((4600.0, 3250.0))),
                320 => (Some(0.30000001192092896), Some((2250.0, 1950.0))),
                321 => (Some(0.25), None),
                322 => (Some(0.44999998807907104), Some((2400.0, 1950.0))),
                323 => (Some(0.35), Some((1900.0, 1500.0))),
                324 => (Some(0.25), Some((1600.0, 1300.0))),
                325 => (Some(0.49000000953674316), Some((2800.0, 2400.0))),
                326 => (Some(0.33000001311302185), Some((2000.0, 1740.0))),
                327 => (Some(0.2199999988079071), Some((1450.0, 1165.0))),
                330 => (Some(0.25), Some((1750.0, 1450.0))),
                334 => (Some(0.25999999046325684), None),
                335 => (Some(0.2199999988079071), Some((1650.0, 1470.0))),
                341 => (Some(0.30000001192092896), None),
                342 => (Some(0.25), None),
                343 => (Some(0.20000000298023224), Some((1650.0, 1550.0))),
                _ => (None, None),
            };
            return (None, p, t);
        }
    };
    (Some(s), Some(p), Some(t))
}

/// Textures that animate over a 2x2 grid of frames, and the ticks per frame.
fn animated(tex: i32) -> Option<u32> {
    match tex {
        235 => Some(20),
        219..=221 | 271..=273 | 302 | 303 | 305 | 307 | 309 | 311 | 313 | 315 | 317 | 326 | 337 | 338 | 341 => Some(15),
        281 => Some(5),
        _ => None,
    }
}

fn name(tex: i32) -> String {
    format!("Background_{tex}")
}

fn snow_sets(bg: u8) -> ([i32; 2], [i32; 3]) {
    match bg {
        1 => ([35, 36], [97, 96, 95]),
        2 => ([98, 99], [-1; 3]),
        21 => ([98, 99], [95, 96, 97]),
        22 => ([98, 99], [37, 38, 39]),
        3 => ([98, 100], [-1; 3]),
        31 => ([98, 100], [95, 96, 97]),
        32 => ([98, 100], [37, 38, 39]),
        4 => ([98, 101], [-1; 3]),
        41 => ([98, 101], [95, 96, 97]),
        42 => ([98, 101], [37, 38, 39]),
        5 => ([-1, -1], [258, 259, 260]),
        6 => ([-1, -1], [263, 264, 265]),
        7 => ([269, 270], [267, 266, 268]),
        8 => ([35, 36], [299, 298, -1]),
        _ => ([35, 36], [37, 38, 39]),
    }
}

fn corrupt_set(bg: u8) -> [i32; 3] {
    match bg {
        1 => [56, 57, 58],
        2 => [211, 212, 213],
        3 => [225, 226, 227],
        4 => [240, 241, 242],
        51 => [324, 323, 322],
        52 => [324, 226, 322],
        _ => [12, 13, 14],
    }
}

fn jungle_set(bg: u8) -> [i32; 3] {
    match bg {
        1 => [59, 60, 61],
        2 => [222, 223, 224],
        3 => [237, 238, 239],
        4 => [284, 285, 286],
        5 => [271, 272, 273],
        6 => [302, 301, 300],
        _ => [15, 16, 17],
    }
}

fn hallow_set(bg: u8) -> [i32; 3] {
    match bg {
        1 => [102, 103, 104],
        2 => [219, 220, 221],
        3 => [243, 244, 245],
        4 => [-1, 261, 262],
        5 => [327, 326, 325],
        _ => [29, 30, 31],
    }
}

fn crimson_set(bg: u8) -> [i32; 3] {
    match bg {
        1 => [105, 106, 107],
        2 => [174, -1, 175],
        3 => [214, 215, 216],
        4 => [-1, 229, 230],
        5 => [255, 256, 257],
        6 => [339, 338, 337],
        _ => [43, 44, 45],
    }
}

fn mushroom_set(bg: u8) -> [i32; 3] {
    match bg {
        1 => [231, 232, 233],
        2 => [234, 235, 236],
        3 => [287, 288, 289],
        4 => [321, 320, 319],
        _ => [46, 47, 48],
    }
}

fn ocean(bg: u8) -> i32 {
    match bg {
        1 => 110,
        2 => 111,
        3 => 209,
        4 => 210,
        5 => 283,
        6 => 332,
        7 => 340,
        _ => 28,
    }
}

/// Every sheet the world's surface areas may draw.
pub fn sheets(env: &Env) -> Vec<String> {
    let b = env.biome_bg;
    let mut v = vec![18, 19, 22, 23, 24, 25, 26, 27, 42, 246, 247, ocean(b[6])];
    for &style in &env.tree_bg {
        let (mnt, trees) = forest_set(style);
        v.extend(mnt);
        v.extend(trees);
        if trees[0] == 173 {
            v.extend(251..=254);
        }
    }
    let (mnt, snow) = snow_sets(b[2]);
    v.extend(mnt);
    v.extend(snow);
    v.extend(corrupt_set(b[0]));
    v.extend(jungle_set(b[1]));
    v.extend(hallow_set(b[3]));
    v.extend(crimson_set(b[4]));
    v.extend(mushroom_set(b[7]));
    for area in [2, 5, 13, 14] {
        v.extend(desert_set(b[5], area).unwrap_or([26, 27, -1]));
    }
    v.retain(|&t| t >= 0);
    v.sort();
    v.dedup();
    v.into_iter().map(name).collect()
}

struct Layers<'a, 'b> {
    s: &'a Scene<'b>,
    v: &'a View,
    magic: f32,
    adj: f32,
    tint: [f32; 4],
    out: &'a mut Vec<Sprite>,
}

impl Layers<'_, '_> {
    fn top(&self, a: f32, b: f32, push: f32) -> f32 {
        self.magic * a + b + self.adj + push
    }

    fn width(&self, tex: i32) -> u32 {
        self.s.sheets.get(&name(tex)).map_or(0, |r| r.w)
    }

    /// Repeats `tex` across the screen at `top`, `scale` and `parallax`.
    #[allow(clippy::too_many_arguments)]
    fn draw(&mut self, tex: i32, scale: f32, par: f64, top: f32, color: [f32; 4], flip: bool, shift: i32, extra: i32) {
        if tex < 0 {
            return;
        }
        let drawn = if tex == 173 { 251 + (self.s.ticks / 10 % 4) as i32 } else { tex };
        let (Some(r), Some(d)) = (self.s.sheets.get(&name(tex)), self.s.sheets.get(&name(drawn))) else { return };
        let w = (r.w as f32 * scale) as i32;
        if w <= 0 {
            return;
        }
        let src = match animated(tex) {
            Some(per) => {
                let f = (self.s.ticks / per) % 4;
                let (fw, fh) = (r.w / 2, r.h / 2);
                [(f % 2) * fw, (f / 2) * fh + 2, fw, fh - 4]
            }
            None => [0, 0, r.w, r.h],
        };
        let start = (-ieee_rem(self.v.x as f64 * par, w as f64) - (w / 2) as f64) as i32 + shift;
        for k in 0..self.v.w as i32 / w + 2 + extra {
            self.out.push(Sprite {
                pos: [(start + w * k) as f32, top],
                size: [src[2] as f32 * scale, src[3] as f32 * scale],
                color,
                uv: [(d.x + src[0]) as f32, (d.y + src[1]) as f32, src[2] as f32, src[3] as f32],
                flags: d.layer | if flip { SPRITE_FLIP } else { 0 },
                ..Default::default()
            });
        }
    }

    fn faded(&self, a: f32) -> [f32; 4] {
        self.tint.map(|c| c * a)
    }

    /// A layer whose overrides apply before the global scale.
    #[allow(clippy::too_many_arguments)]
    fn plain(&mut self, tex: i32, scale: f32, par: f64, (a, b): (f32, f32), color: [f32; 4], flip: bool) {
        let (s, p, t) = offsets(tex);
        let top = t.map_or(self.top(a, b, FRONT_PUSH), |(a, b)| self.top(a, b, FRONT_PUSH));
        self.draw(tex, s.unwrap_or(scale) * GLOBAL_SCALE, p.unwrap_or(par), top, color, flip, 0, 0);
    }

    /// A layer whose overrides apply after the global scale.
    #[allow(clippy::too_many_arguments)]
    fn late(
        &mut self,
        tex: i32,
        scale: f32,
        par: f64,
        (a, b): (f32, f32),
        extra_top: f32,
        color: [f32; 4],
        flip: bool,
    ) {
        let (s, p, t) = offsets(tex);
        let top = t.map_or(self.top(a, b, FRONT_PUSH), |(a, b)| self.top(a, b, FRONT_PUSH));
        let scale = s.unwrap_or(scale * GLOBAL_SCALE);
        self.draw(tex, scale, p.unwrap_or(par), top + extra_top, color, flip, 0, 0);
    }

    fn forest(&mut self, trees: [i32; 3], c: [f32; 4]) {
        let push = FRONT_PUSH;
        if trees[0] >= 0 {
            let t0 = trees[0];
            let (mut sc, mut par) = match t0 {
                91 => (1.2, 0.27000001072883606),
                173 => (1.3, 0.25),
                178 => (1.2, 0.30000001192092896),
                184 => (1.2, 0.25),
                282 => (1.4, 0.4),
                _ => (1.25, 0.4),
            };
            let mut top = self.top(1800.0, 1500.0, push);
            let (s, p, t) = offsets(t0);
            sc = s.unwrap_or(sc) * GLOBAL_SCALE;
            // The game applies the overrides again after the global scale.
            sc = s.unwrap_or(sc);
            par = p.unwrap_or(par);
            if let Some((a, b)) = t {
                top = self.top(a, b, push);
            }
            if matches!(t0, 91 | 173 | 184) {
                top = self.top(1600.0, 1400.0, push);
            }
            top += match t0 {
                50 => -50.0,
                53 => -100.0,
                91 | 173 => 200.0,
                178 => 75.0,
                330 | 335 => -150.0,
                343 => -300.0,
                _ => 0.0,
            };
            self.draw(t0, sc, par, top, c, false, 0, 0);
        }
        if trees[1] >= 0 {
            let t1 = trees[1];
            let (s, p, _) = offsets(t1);
            let top = match t1 {
                329 => self.top(1900.0, 1600.0, push),
                334 => self.top(1900.0, 1650.0, push),
                _ => self.top(1950.0, 1750.0, push),
            } + match t1 {
                51 => -50.0,
                54 => -100.0,
                329 | 334 => -150.0,
                342 => -300.0,
                _ => 0.0,
            };
            let sc = s.unwrap_or(1.31 * GLOBAL_SCALE);
            self.draw(t1, sc, p.unwrap_or(0.43), top, c, matches!(t1, 51 | 54), 0, 0);
        }
        if trees[2] >= 0 {
            let t2 = trees[2];
            let (s, p, _) = offsets(t2);
            let (mut sc, mut par) = (s.unwrap_or(1.34 * GLOBAL_SCALE), p.unwrap_or(0.49));
            if trees[0] == 91 {
                (sc, par) = (1.3 * GLOBAL_SCALE, 0.42);
            }
            let top = match t2 {
                328 => self.top(2600.0, 2300.0, push),
                333 => self.top(2800.0, 2550.0, push),
                _ => self.top(2100.0, 2000.0, push),
            } + match t2 {
                52 => -50.0,
                55 => -100.0,
                92 => 150.0,
                328 | 333 => -150.0,
                341 => -50.0,
                _ => 0.0,
            };
            self.draw(t2, sc, par, top, c, false, 0, 0);
        }
    }

    fn snow(&mut self, set: [i32; 3], c: [f32; 4]) {
        let layers = [(1.25, 0.4, (1800.0, 1500.0)), (1.31, 0.43, (1950.0, 1750.0)), (1.34, 0.49, (2100.0, 2000.0))];
        for (tex, (sc, par, ab)) in set.into_iter().zip(layers) {
            if tex < 0 {
                continue;
            }
            let (s, p, t) = offsets(tex);
            let (a, b) = t.unwrap_or(ab);
            let top = self.top(a, b, FRONT_PUSH) + mountain_offset(tex);
            self.draw(tex, s.unwrap_or(sc) * GLOBAL_SCALE, p.unwrap_or(par), top, c, false, 0, 0);
        }
    }

    fn hallow(&mut self, set: [i32; 3], c: [f32; 4]) {
        let layers = [(1.25, 0.4, (1800.0, 1500.0)), (1.31, 0.43, (1950.0, 1750.0)), (1.34, 0.49, (2100.0, 2000.0))];
        for (tex, (sc, par, ab)) in set.into_iter().zip(layers) {
            if tex > 0 {
                self.plain(tex, sc, par, ab, c, false);
            }
        }
    }

    fn crimson(&mut self, set: [i32; 3], c: [f32; 4]) {
        let layers = [(1.25, 0.4, (1800.0, 1500.0)), (1.31, 0.43, (1950.0, 1750.0)), (1.34, 0.49, (2100.0, 2000.0))];
        for (i, (tex, (sc, par, (a, b)))) in set.into_iter().zip(layers).enumerate() {
            if tex < 0 {
                continue;
            }
            let (s, p, t) = offsets(tex);
            let (a, b) = t.unwrap_or((a, b));
            let mut top = self.top(a, b, FRONT_PUSH);
            let (mut shift, mut extra) = (0, 0);
            match (i, tex) {
                (0, 105) => top += 50.0,
                (0, 174) => top -= 350.0,
                (2, 175) => {
                    shift = -1000;
                    top -= 400.0;
                    extra = 1;
                }
                _ => {}
            }
            self.draw(tex, s.unwrap_or(sc) * GLOBAL_SCALE, p.unwrap_or(par), top, c, false, shift, extra);
        }
    }

    fn corrupt(&mut self, set: [i32; 3], c: [f32; 4]) {
        let layers = [(1.25, 0.4, (1800.0, 1500.0)), (1.31, 0.43, (1950.0, 1750.0)), (1.34, 0.49, (2100.0, 2000.0))];
        let lift = if set[0] == 56 { -100.0 } else { 0.0 };
        for (i, (tex, (sc, par, ab))) in set.into_iter().zip(layers).enumerate() {
            let (s, p, t) = offsets(tex);
            let (a, b) = t.unwrap_or(ab);
            let top = self.top(a, b, FRONT_PUSH) + lift;
            self.draw(tex, s.unwrap_or(sc) * GLOBAL_SCALE, p.unwrap_or(par), top, c, i == 1, 0, 0);
        }
    }

    fn desert(&mut self, set: [i32; 3], c: [f32; 4]) {
        let layers = [(1.25, 0.37, (1800.0, 1750.0)), (1.34, 0.49, (2100.0, 2150.0)), (1.34, 0.49, (2100.0, 2150.0))];
        for (tex, (sc, par, ab)) in set.into_iter().zip(layers) {
            if tex > 0 {
                self.plain(tex, sc, par, ab, c, false);
            }
        }
    }

    fn jungle(&mut self, set: [i32; 3], c: [f32; 4]) {
        let lift = |t: i32| match t {
            59 => -200.0,
            60 => -175.0,
            61 => -150.0,
            _ => 0.0,
        } + mountain_offset(t);
        self.late(set[0], 1.25, 0.4, (1800.0, 1660.0), lift(set[0]), c, false);
        self.late(set[1], 1.31, 0.43, (1950.0, 1840.0), lift(set[1]), c, true);
        self.late(set[2], 1.34, 0.49, (2100.0, 2060.0), lift(set[2]), c, false);
    }

    fn mushroom(&mut self, set: [i32; 3], alpha: f32) {
        // The game flickers the glow with a random term and the text pulse; their middle values.
        let glow =
            [([0.3, 0.45, 0.9], 0.5, 5000.0), ([0.3, 0.525, 0.9], 0.625, 5000.0), ([0.375, 0.6, 0.9], 0.75, 3000.0)];
        let layers = [(1.25, 0.4, (1800.0, 1400.0)), (1.32, 0.43, (1950.0, 1675.0)), (1.36, 0.49, (2100.0, 1950.0))];
        for ((tex, (k, m, div)), (sc, par, ab)) in set.into_iter().zip(glow).zip(layers) {
            let j = 0.035 + (270.0 - 222.0) / div;
            let g = [k[0], k[1] + j / 2.0, k[2] + j];
            let base = self.faded(alpha);
            let mut c = base;
            for i in 0..3 {
                c[i] = c[i].max((g[i] * m * alpha).min(1.0));
            }
            self.late(tex, sc, par, ab, 0.0, c, false);
        }
    }
}

/// Far and near layers of every area still visible, back to front.
pub fn layers(s: &Scene, v: &View, fade: &Fade, holy_tiles: i32, tint: [f32; 4], out: &mut Vec<Sprite>) {
    let surface = s.env.world_surface as f32;
    if v.y / 16.0 > surface + 10.0 || v.y >= surface * 16.0 + 16.0 {
        return;
    }
    let env = s.env;
    let b = env.biome_bg;
    let mut l = Layers {
        s,
        v,
        magic: -(v.y - 300.0) / (surface.max(1.0) * 16.0),
        adj: crate::backdrop::sc_adj(s, v),
        tint,
        out,
    };
    let forest = |k: usize| forest_set(env.tree_bg[k]);
    let snow = snow_sets(b[2]);

    // Far mountains: one per area.
    let width = (1024.0 * GLOBAL_SCALE) as i32;
    let top = l.top(1300.0, 1090.0, BACK_PUSH);
    let far = |i: usize| match i {
        0 => forest(0).0[0],
        10..=12 => forest(i - 9).0[0],
        1 if b[5] != 4 => 23,
        2 => 24,
        4 => snow.0[0],
        5 if b[4] != 5 => 24,
        6 if b[3] == 3 => 246,
        _ => -1,
    };
    for i in [0, 10, 11, 12, 1, 2, 4, 5, 6] {
        let (tex, a) = (far(i), fade.back[i]);
        if a > 0.0 && tex >= 0 {
            l.draw_fixed(tex, GLOBAL_SCALE, 0.15, width, top + mountain_offset(tex), l.faded(a), 0);
        }
    }

    // Hallowed light shafts, stronger with more hallowed tiles.
    if holy_tiles > 0 {
        let k = ((holy_tiles - 125) as f32 / (600 - 125) as f32).clamp(0.0, 0.5);
        let scale = 1.1 * GLOBAL_SCALE;
        let w = (2100.0 * scale * 1.05) as i32;
        let top = l.top(1400.0, if b[3] == 3 { 1100.0 } else { 900.0 }, BACK_PUSH);
        let start = (-ieee_rem(v.x as f64 * 0.17, w as f64) - (w / 2) as f64) as i32;
        let c = [tint[0] * k, tint[1] * k, tint[2] * k, k * 0.8];
        let loops = v.w as i32 / width + 2;
        for i in 0..loops {
            let x = (start + w * i) as f32;
            l.put(18, [x, top], scale, c);
            l.put(19, [x + 1900.0, top + 100.0], scale * 0.9, c);
        }
    }

    // Near mountains, sized from the first forest's texture when it has one.
    let first = forest(0).0[1];
    let (mut scale, mut par, mut width, mut extra, mut top) = (GLOBAL_SCALE, 0.15, width, 0, top);
    if first > -1 {
        scale = 1.15 * GLOBAL_SCALE;
        par = 0.2;
        width = ((l.width(first).max(1)) as f32 * scale) as i32;
        extra = (first == 172) as i32;
        top = l.top(1400.0, 1260.0, BACK_PUSH);
    }
    let near = |i: usize| match i {
        0 => forest(0).0[1],
        10..=12 => forest(i - 9).0[1],
        1 if b[5] != 4 => 22,
        2 => 25,
        3 => ocean(b[6]),
        4 => snow.0[1],
        5 if b[4] != 5 => 42,
        6 if b[3] == 3 => 247,
        _ => -1,
    };
    for i in [0, 1, 2, 3, 4, 5, 6, 10, 11, 12] {
        let (tex, a) = (near(i), fade.back[i]);
        if a > 0.0 && tex >= 0 {
            l.draw_fixed(tex, scale, par, width, top + mountain_offset(tex), l.faded(a), extra);
        }
    }

    for i in 0..16 {
        let a = fade.front[i];
        if a <= 0.0 {
            continue;
        }
        let c = l.faded(a);
        match i {
            0 => l.forest(forest(0).1, c),
            10..=12 => l.forest(forest(i - 9).1, c),
            1 => l.corrupt(corrupt_set(b[0]), c),
            2 | 5 | 13 | 14 => l.desert(desert_set(b[5], i).unwrap_or([26, 27, -1]), c),
            3 => l.jungle(jungle_set(b[1]), c),
            6 => l.hallow(hallow_set(b[3]), c),
            7 => l.snow(snow.1, c),
            8 => l.crimson(crimson_set(b[4]), c),
            9 => l.mushroom(mushroom_set(b[7]), a),
            _ => {}
        }
    }
}

impl Layers<'_, '_> {
    /// Mountains: a fixed tile width shared by every area's texture.
    #[allow(clippy::too_many_arguments)]
    fn draw_fixed(&mut self, tex: i32, scale: f32, par: f64, w: i32, top: f32, color: [f32; 4], extra: i32) {
        let Some(r) = self.s.sheets.get(&name(tex)) else { return };
        let start = (-ieee_rem(self.v.x as f64 * par, w as f64) - (w / 2) as f64) as i32;
        let src = match animated(tex) {
            Some(per) => {
                let f = (self.s.ticks / per) % 4;
                let (fw, fh) = (r.w / 2, r.h / 2);
                [(f % 2) * fw, (f / 2) * fh + 2, fw, fh - 4]
            }
            None => [0, 0, r.w, r.h],
        };
        for k in 0..self.v.w as i32 / w.max(1) + 2 + extra {
            self.out.push(Sprite {
                pos: [(start + w * k) as f32, top],
                size: [src[2] as f32 * scale, src[3] as f32 * scale],
                color,
                uv: [(r.x + src[0]) as f32, (r.y + src[1]) as f32, src[2] as f32, src[3] as f32],
                flags: r.layer,
                ..Default::default()
            });
        }
    }

    fn put(&mut self, tex: i32, pos: [f32; 2], scale: f32, color: [f32; 4]) {
        let Some(r) = self.s.sheets.get(&name(tex)) else { return };
        self.out.push(Sprite {
            pos,
            size: [r.w as f32 * scale, r.h as f32 * scale],
            color,
            uv: [r.x as f32, r.y as f32, r.w as f32, r.h as f32],
            flags: r.layer,
            ..Default::default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas_follow_zones() {
        let env = Env { tree_x: [100, 200, 300], ..Env::default() };
        let z = Zones::default();
        assert_eq!([50, 150, 250, 350].map(|x| preferred(&env, &z, x)), [0, 10, 11, 12]);
        assert_eq!(preferred(&env, &Zones { snow: true, jungle: true, ..z }, 0), 3);
        assert_eq!(preferred(&env, &Zones { beach: true, glowshroom: true, ..z }, 0), 4);
        assert_eq!(preferred(&env, &Zones { desert: true, hallow: true, ..z }, 0), 13);
        assert_eq!(preferred(&env, &Zones { corrupt: true, evil_tiles: 400, blood_tiles: 500, ..z }, 0), 8);
    }

    #[test]
    fn areas_fade_after_a_delay() {
        let env = Env::default();
        let mut f = Fade::default();
        for _ in 0..10 {
            f.step(&env, 7);
        }
        assert_eq!((f.style, f.front[7], f.back[4]), (7, 1.0, 1.0));
        // Leaving for a forest waits 61 frames, then fades over 20.
        for _ in 0..61 {
            f.step(&env, 0);
        }
        assert_eq!(f.style, 0);
        f.step(&env, 0);
        assert!(f.front[0] > 0.0 && f.front[7] < 1.0);
        for _ in 0..20 {
            f.step(&env, 0);
        }
        assert_eq!((f.front[0], f.front[7], f.back[0], f.back[4]), (1.0, 0.0, 1.0, 0.0));
    }

    #[test]
    fn desert_variants() {
        assert_eq!(desert_set(0, 2), Some([21, 20, -1]));
        assert_eq!(desert_set(0, 5), None);
        assert_eq!(desert_set(52, 14), Some([318, 316, -1]));
        assert_eq!(desert_set(53, 13), Some([314, 313, -1]));
    }
}
