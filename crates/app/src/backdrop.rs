//! Sky, sun, moon and forest backgrounds, laid out with the game's rules.
//!
//! As in the game, positions are worked out in screen pixels at background
//! zoom 1, from the top-left of the unzoomed view; the caller maps them to
//! world pixels.

use std::collections::HashMap;

use formats::wld::Env;
use render::atlas::SheetRef;
use render::{Sprite, SPRITE_FLIP};

use crate::surface::{self, Fade};

pub const DAY_TICKS: f64 = 54000.0;
pub const NIGHT_TICKS: f64 = 32400.0;
const SKY: &str = "Background_0";

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
pub fn mountain_offset(tex: i32) -> f32 {
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

/// Every sheet the world's backgrounds may draw.
pub fn sheets(env: &Env) -> Vec<String> {
    let mut v = vec![SKY.to_string(), "Sun".to_string(), format!("Moon_{}", env.moon_type.min(8))];
    v.extend(surface::sheets(env));
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
pub fn ieee_rem(a: f64, b: f64) -> f64 {
    a - b * (a / b).round_ties_even()
}

/// Darkens the sky towards space, from the height of the view centre.
fn atmosphere(s: &Scene, y: f32) -> f32 {
    let n = s.width as f32 / 4200.0;
    let a = (y / 16.0 - (65.0 + 10.0 * n * n)) / (s.env.world_surface as f32 / 5.0);
    a.clamp(0.0, 1.0)
}

/// Sprites in screen pixels, back to front.
pub fn sprites(s: &Scene, v: &View, fade: &Fade, holy_tiles: i32) -> Vec<Sprite> {
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
                ..Default::default()
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

    surface::layers(s, v, fade, holy_tiles, tint, &mut out);
    out
}

/// Lifts the backgrounds as the view goes down, more on small worlds.
pub fn sc_adj(s: &Scene, v: &View) -> f32 {
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
}
