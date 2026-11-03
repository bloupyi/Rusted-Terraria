//! Sky, sun, moon and forest backgrounds, laid out with the game's rules.
//!
//! As in the game, positions are worked out in screen pixels at background
//! zoom 1, from the top-left of the unzoomed view; the caller maps them to
//! world pixels.

use std::collections::HashMap;

use formats::wld::Env;
use render::atlas::SheetRef;
use render::{Sprite, SPRITE_FLIP};

pub const DAY_TICKS: f64 = 54000.0;
pub const NIGHT_TICKS: f64 = 32400.0;
const SKY: &str = "Background_0";
/// Forest layers are pushed this far down, mountains `BACK_PUSH`.
const BACK_PUSH: f32 = 30.0;
const FRONT_PUSH: f32 = BACK_PUSH - 180.0;
const GLOBAL_SCALE: f32 = 2.0;

/// Back mountains and the three tree layers of a forest background style; -1: none.
pub fn forest_set(style: u8) -> ([i32; 2], [i32; 3]) {
    match style {
        1 => ([7, 8], [50, 51, 52]),
        2 => ([7, 8], [53, 54, 55]),
        3 => ([7, 90], [91, -1, 92]),
        31 => ([7, 90], [91, -1, 11]),
        4 => ([93, 94], [-1, -1, -1]),
        5 => ([93, 94], [-1, -1, 55]),
        51 => ([93, 94], [-1, -1, 11]),
        6 => ([171, 172], [173, -1, -1]),
        7 => ([176, 177], [178, -1, -1]),
        71 => ([176, 177], [178, -1, 11]),
        72 => ([176, 177], [178, -1, 52]),
        73 => ([176, 177], [178, -1, 55]),
        8 => ([179, 180], [184, -1, -1]),
        9 => ([277, 278], [279, -1, -1]),
        10 => ([280, 281], [282, -1, -1]),
        11 => ([7, 331], [330, 329, 328]),
        12 => ([7, 336], [335, 334, 333]),
        13 => ([7, -1], [343, 342, 341]),
        _ => ([7, 8], [9, 10, 11]),
    }
}

/// Extra vertical offset of a back mountain texture.
fn mountain_offset(tex: i32) -> f32 {
    match tex {
        59 => -550.0,
        93 | 168..=170 => -50.0,
        171 => -100.0,
        172 => 130.0,
        176 => -760.0,
        177 => -200.0,
        179 => -100.0,
        180..=183 => -350.0,
        246 | 247 => -150.0,
        263 => -700.0,
        269 => -600.0,
        270 => -50.0,
        271 => -300.0,
        272 => -380.0,
        277 => -260.0,
        278 => -120.0,
        280 => -170.0,
        281 => -300.0,
        283 => -800.0,
        332 | 340 => -25.0,
        _ => 0.0,
    }
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

/// Every sheet the world's backgrounds may draw.
pub fn sheets(env: &Env) -> Vec<String> {
    let mut v = vec![SKY.to_string(), "Sun".to_string(), format!("Moon_{}", env.moon_type.min(8))];
    for &style in &env.tree_bg {
        let (mnt, trees) = forest_set(style);
        for t in mnt.into_iter().chain(trees).filter(|&t| t >= 0) {
            v.push(name(t));
            if t == 173 {
                v.extend((251..=254).map(name));
            }
        }
    }
    v.sort();
    v.dedup();
    v
}

/// Sky, sun and moon colours (0..=255) for the time of day.
pub struct Colors {
    pub sky: [f32; 3],
    pub sun: [f32; 3],
    pub moon: [f32; 3],
}

pub fn colors(day: bool, time: f64, blood_moon: bool) -> Colors {
    let lerp = |t: f64, k: [f64; 3], base: [f64; 3]| [0, 1, 2].map(|i| (t * k[i] + base[i]) as u8 as f32);
    let mut c = Colors { sky: [255.0; 3], sun: [255.0; 3], moon: [255.0; 3] };
    if day {
        if time < 13500.0 {
            let t = time / 13500.0;
            c.sun = lerp(t, [200.0, 180.0, 250.0], [55.0, 75.0, 5.0]);
            c.sky = lerp(t, [230.0, 220.0, 220.0], [25.0, 35.0, 35.0]);
        }
        if time > 45900.0 {
            let t = 1.0 - (time / DAY_TICKS - 0.85) * (1.0 / 0.15);
            c.sun = lerp(t, [120.0, 100.0, 120.0], [55.0, 25.0, 55.0]);
            c.sky = lerp(t, [200.0, 85.0, 135.0], [35.0; 3]);
        } else if time > 37800.0 {
            let t = 1.0 - (time / DAY_TICKS - 0.7) * (1.0 / 0.15);
            c.sun = lerp(t, [80.0, 130.0, 100.0], [175.0, 125.0, 155.0]);
            c.sky = lerp(t, [20.0, 135.0, 85.0], [235.0, 120.0, 170.0]);
        }
    } else {
        let t = if time < 16200.0 { 1.0 - time / 16200.0 } else { (time / NIGHT_TICKS - 0.5) * 2.0 };
        if blood_moon {
            c.moon = lerp(t, [10.0, 170.0, 200.0], [205.0, 55.0, 55.0]);
            c.sky = [(75.0 - t * 40.0) as u8 as f32, (t * 20.0 + 15.0) as u8 as f32, (t * 20.0 + 15.0) as u8 as f32];
        } else if time < 16200.0 {
            c.moon = lerp(t, [10.0, 70.0, 100.0], [205.0, 155.0, 155.0]);
            c.sky = lerp(t, [30.0; 3], [5.0; 3]);
        } else {
            c.moon = lerp(t, [50.0, 100.0, 100.0], [205.0, 155.0, 155.0]);
            c.sky = lerp(t, [20.0, 30.0, 30.0], [5.0; 3]);
        }
    }
    c
}

/// Light the sky casts on tiles, from the sky colour.
pub fn tile_light(sky: [f32; 3]) -> [f32; 3] {
    let sum = sky[0] + sky[1] + sky[2];
    sky.map(|c| ((sum + c * 7.0) / 10.0).floor() / 255.0)
}

/// The view as the game sees it: top-left of the unzoomed screen and its size.
pub struct View {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub struct Scene<'a> {
    pub env: &'a Env,
    pub width: u32,
    pub height: u32,
    pub day: bool,
    pub time: f64,
    /// Ticks since start, for animated layers.
    pub ticks: u32,
    pub sheets: &'a HashMap<String, SheetRef>,
}

/// `Math.IEEERemainder`: the remainder against the nearest multiple, ties to even.
fn ieee_rem(a: f64, b: f64) -> f64 {
    a - b * (a / b).round_ties_even()
}

/// Darkens the sky towards space, from the height of the view centre.
fn atmosphere(s: &Scene, y: f32) -> f32 {
    let n = s.width as f32 / 4200.0;
    let a = (y / 16.0 - (65.0 + 10.0 * n * n)) / (s.env.world_surface as f32 / 5.0);
    a.clamp(0.0, 1.0)
}

/// Sprites in screen pixels, back to front.
pub fn sprites(s: &Scene, v: &View) -> Vec<Sprite> {
    let mut out = Vec::new();
    let surface = s.env.world_surface as f32;
    if v.y >= surface * 16.0 + 16.0 {
        return out;
    }
    let mut sky = colors(s.day, s.time, s.env.blood_moon && !s.day).sky;
    let atmo = atmosphere(s, v.y + v.h / 2.0);
    sky = if atmo <= 0.01 { [0.0; 3] } else { sky.map(|c| (c * atmo).floor()) };
    let tint = [sky[0] / 255.0, sky[1] / 255.0, sky[2] / 255.0, 1.0];

    let mut put = |tex: &str, pos: [f32; 2], src: [u32; 4], size: [f32; 2], color: [f32; 4], flip: bool| {
        if let Some(r) = s.sheets.get(tex) {
            out.push(Sprite {
                pos,
                size,
                color,
                uv: [(r.x + src[0]) as f32, (r.y + src[1]) as f32, src[2] as f32, src[3] as f32],
                flags: r.layer | if flip { SPRITE_FLIP } else { 0 },
            });
        }
    };

    // Sky gradient: one 48 px strip repeated, stretched to the screen height.
    let top = -v.y / (surface * 16.0 - 600.0) * 200.0;
    if let Some(r) = s.sheets.get(SKY) {
        let (w, h) = (r.w as f32, r.h as f32);
        let start = (-ieee_rem(v.x as f64 * 0.1, w as f64) - (w / 2.0) as i32 as f64) as i32 as f32;
        let top = top as i32 as f32;
        let mut height = v.h.max(h);
        if top + height < h {
            height += h - (top + height);
        }
        for k in 0..(v.w / w) as i32 + 2 {
            put(SKY, [start + w * k as f32, top], [0, 0, r.w, r.h], [w, height], tint, false);
        }
    }

    if v.y / 16.0 < surface + 2.0 {
        sun_and_moon(s, v, top as i32 as f32, &mut put);
    }

    if v.y / 16.0 > surface + 10.0 {
        return out;
    }
    let zone = forest_zone(s.env, ((v.x + v.w / 2.0) / 16.0) as i32);
    let (mnt, trees) = forest_set(s.env.tree_bg[zone]);
    let (first_mnt, _) = forest_set(s.env.tree_bg[0]);
    let magic = -(v.y - 300.0) / (surface.max(1.0) * 16.0);
    let adj = sc_adj(s, v);
    let size = |tex: i32| s.sheets.get(&name(tex)).map(|r| (r.w, r.h));
    let ticks = s.ticks;
    let src = |tex: i32| -> Option<[u32; 4]> {
        let (w, h) = size(tex)?;
        Some(match animated(tex) {
            Some(per) => {
                let f = (ticks / per) % 4;
                let (fw, fh) = (w / 2, h / 2);
                [(f % 2) * fw, (f / 2) * fh + 2, fw, fh - 4]
            }
            None => [0, 0, w, h],
        })
    };
    let mut layer = |tex: i32, start: f32, step: f32, loops: i32, top: f32, scale: f32, flip: bool| {
        let drawn = if tex == 173 { 251 + (ticks / 10 % 4) as i32 } else { tex };
        let Some(r) = src(tex) else { return };
        for k in 0..loops {
            let size = [r[2] as f32 * scale, r[3] as f32 * scale];
            put(&name(drawn), [start + step * k as f32, top], r, size, tint, flip);
        }
    };
    let start_of = |parallax: f64, width: i32| -> f32 {
        (-ieee_rem(v.x as f64 * parallax, width as f64) - (width / 2) as f64) as i32 as f32
    };

    // Far mountains, then nearer mountains sized from the first zone's texture.
    let scale = GLOBAL_SCALE;
    let mut width = (1024.0 * scale) as i32;
    let mut start = start_of(0.15, width);
    let mut loops = v.w as i32 / width + 2;
    let mut top = magic * 1300.0 + 1090.0 + adj + BACK_PUSH;
    let mut scale_now = scale;
    if mnt[0] >= 0 {
        layer(mnt[0], start, width as f32, loops, top + mountain_offset(mnt[0]), scale_now, false);
    }
    if first_mnt[1] > -1 {
        scale_now = 1.15 * scale;
        width = (size(first_mnt[1]).map_or(1, |s| s.0) as f32 * scale_now) as i32;
        start = start_of(0.2, width);
        loops = v.w as i32 / width + 2 + (first_mnt[1] == 172) as i32;
        top = magic * 1400.0 + 1260.0 + adj + BACK_PUSH;
    }
    if mnt[1] > -1 {
        layer(mnt[1], start, width as f32, loops, top + mountain_offset(mnt[1]), scale_now, false);
    }

    // Three tree layers, each with its own scale, parallax and height.
    let push = FRONT_PUSH;
    if trees[0] >= 0 {
        let (mut sc, mut par) = (1.25, 0.4);
        match trees[0] {
            91 => (sc, par) = (1.2, 0.27000001072883606),
            173 => (sc, par) = (1.3, 0.25),
            178 => (sc, par) = (1.2, 0.30000001192092896),
            184 => (sc, par) = (1.2, 0.25),
            282 => sc = 1.4,
            _ => {}
        }
        let sc = sc * scale;
        let w = (size(trees[0]).map_or(0, |s| s.0) as f32 * sc) as i32;
        let mut top = match trees[0] {
            91 | 173 | 184 => magic * 1600.0 + 1400.0,
            _ => magic * 1800.0 + 1500.0,
        } + adj
            + push;
        top += match trees[0] {
            50 => -50.0,
            53 => -100.0,
            91 | 173 => 200.0,
            178 => 75.0,
            330 | 335 => -150.0,
            343 => -300.0,
            _ => 0.0,
        };
        if w > 0 {
            layer(trees[0], start_of(par, w), w as f32, v.w as i32 / w + 2, top, sc, false);
        }
    }
    if trees[1] >= 0 {
        let sc = 1.31 * scale;
        let w = ((size(trees[1]).map_or(0, |s| s.0) as f32 * sc) as i32).max(1);
        let mut top = match trees[1] {
            329 => magic * 1900.0 + 1600.0,
            334 => magic * 1900.0 + 1650.0,
            _ => magic * 1950.0 + 1750.0,
        } + adj
            + push;
        top += match trees[1] {
            51 => -50.0,
            54 => -100.0,
            329 | 334 => -150.0,
            342 => -300.0,
            _ => 0.0,
        };
        let flip = matches!(trees[1], 51 | 54);
        layer(trees[1], start_of(0.43, w), w as f32, v.w as i32 / w + 2, top, sc, flip);
    }
    if trees[2] >= 0 {
        let (sc, par) = if trees[0] == 91 { (1.3, 0.42) } else { (1.34, 0.49) };
        let sc = sc * scale;
        let w = ((size(trees[2]).map_or(0, |s| s.0) as f32 * sc) as i32).max(1);
        let mut top = match trees[2] {
            328 => magic * 2600.0 + 2300.0,
            333 => magic * 2800.0 + 2550.0,
            _ => magic * 2100.0 + 2000.0,
        } + adj
            + push;
        top += match trees[2] {
            52 => -50.0,
            55 => -100.0,
            92 => 150.0,
            328 | 333 => -150.0,
            341 => -50.0,
            _ => 0.0,
        };
        layer(trees[2], start_of(par, w), w as f32, v.w as i32 / w + 2, top, sc, false);
    }
    out
}

/// Which of the four forest zones holds tile column `x`.
pub fn forest_zone(env: &Env, x: i32) -> usize {
    if x < env.tree_x[0] {
        0
    } else if x < env.tree_x[1] {
        1
    } else if x >= env.tree_x[2] {
        3
    } else {
        2
    }
}

/// Lifts the backgrounds as the view goes down, more on small worlds.
fn sc_adj(s: &Scene, v: &View) -> f32 {
    let mut n = s.env.world_surface as f32 * 16.0 / (v.y + v.h);
    let m = ((s.height as f32 * 0.15 * 16.0 - v.y).max(0.0)) * 0.00025;
    n *= 0.45 - m * m;
    n * match s.height {
        0..=1200 => -500.0,
        1201..=1800 => -300.0,
        _ => -150.0,
    }
}

type Put<'a> = dyn FnMut(&str, [f32; 2], [u32; 4], [f32; 2], [f32; 4], bool) + 'a;

/// The sun crosses the screen over the day and the moon over the night, highest at noon and midnight.
fn sun_and_moon(s: &Scene, v: &View, top: f32, put: &mut Put) {
    let c = colors(s.day, s.time, s.env.blood_moon && !s.day);
    let (len, tex) =
        if s.day { (DAY_TICKS, "Sun".to_string()) } else { (NIGHT_TICKS, format!("Moon_{}", s.env.moon_type.min(8))) };
    let Some(r) = s.sheets.get(&tex) else { return };
    let w = r.w as f32;
    let x = (s.time / len * (v.w as f64 + w as f64 * 2.0)) as i32 as f32 - w;
    let arc = (s.time / len * 2.0 - 1.0).abs().powi(2);
    let y = (top as f64 + arc * 250.0 + 180.0) as i32 as f32;
    let mut scale = (1.2 - arc * 0.4) as f32;
    if s.day {
        scale *= 1.1;
        let h = r.h as f32;
        let size = [w * scale, h * scale];
        let pos = [x - size[0] / 2.0, y - size[1] / 2.0];
        put(&tex, pos, [0, 0, r.w, r.h], size, [1.0; 4], false);
        let sun = c.sun.map(|k| k / 255.0);
        put(&tex, pos, [0, 0, r.w, r.h], size, [sun[0], sun[1], sun[2], sun[2]], false);
    } else {
        let phase = s.env.moon_phase.clamp(0, 7) as u32;
        let size = [w * scale, w * scale];
        let moon = c.moon.map(|k| k / 255.0);
        put(
            &tex,
            [x - size[0] / 2.0, y - size[1] / 2.0],
            [0, r.w * phase, r.w, r.w],
            size,
            [moon[0], moon[1], moon[2], 1.0],
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remainder_rounds_to_nearest() {
        assert_eq!(ieee_rem(10.0, 4.0), 2.0);
        assert_eq!(ieee_rem(6.0, 4.0), -2.0);
        assert_eq!(ieee_rem(5.0, 4.0), 1.0);
    }

    #[test]
    fn sky_follows_the_day() {
        assert_eq!(colors(true, 0.0, false).sky, [25.0, 35.0, 35.0]);
        assert_eq!(colors(true, 27000.0, false).sky, [255.0; 3]);
        assert_eq!(colors(false, 16200.0, false).sky, [5.0; 3]);
        assert_eq!(tile_light([255.0; 3]), [1.0; 3]);
    }

    #[test]
    fn zones_split_at_tree_x() {
        let env = Env { tree_x: [100, 200, 300], ..Env::default() };
        assert_eq!([50, 150, 250, 350].map(|x| forest_zone(&env, x)), [0, 1, 2, 3]);
    }
}
