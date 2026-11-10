//! Backgrounds below the surface, following the game: dirt, rock and magma
//! layers tiled in world space (horizontal parallax only) and lit by the
//! tiles in front of them, then the underworld's parallax layers.

use std::collections::HashMap;

use formats::wld::Env;
use render::atlas::SheetRef;
use render::{Sprite, SPRITE_LIT};
use world::scene::Zones;

/// The game's default cave parallax (background scroll setting 60).
const PARALLAX: f64 = 0.88;
const ROW: f64 = 96.0;
/// Rock and magma layers repeat every 128 px whatever their width.
const DEEP: f64 = 128.0;
const TINT: [f32; 4] = [0.9, 0.9, 0.9, 1.0];
/// Flat colour when the game's sheets are missing.
const FALLBACK: [f32; 4] = [0.30, 0.21, 0.15, 1.0];

fn name(tex: i32) -> String {
    format!("Background_{tex}")
}

fn underworld_name(tex: usize) -> String {
    format!("Backgrounds/Underworld {tex}")
}

/// Sheets of an underground style: surface strip, dirt, rock strip, rock, magma
/// strip, magma, hell strip; -1 where the style draws nothing.
pub fn textures(style: i32, env: &Env, world_id: i32) -> [i32; 7] {
    let hell = env.hell_style;
    let mut t = [-1; 7];
    let mut set = |v: [i32; 5]| t[..5].copy_from_slice(&v);
    match style {
        0 => t[..6].copy_from_slice(&[1, 2, 4, 3, 6, 5]),
        1 => {
            let ice = match env.ice_style {
                0 => [40, 33, 34, 32],
                1 => [160, 118, 161, 117],
                2 => [164, 165, 166, 167],
                _ => [162, 120, 163, 119],
            };
            set([ice[0], ice[1], ice[2], ice[3], 128 + hell]);
        }
        2 => set([62, 63, 64, 65, 143 + hell]),
        3 => set([66, 67, 68, 69, 128 + hell]),
        4 => set([70, 71, 68, 72, 128 + hell]),
        5 => set([73, 74, 75, 76, 131 + hell]),
        6 => set([77, 78, 79, 80, 134 + hell]),
        7 => set([77, 81, 79, 82, 134 + hell]),
        8 => set([83, 84, 85, 86, 137 + hell]),
        9 => set([83, 87, 88, 89, 137 + hell]),
        10 => set([121, 122, 123, 124, 140 + hell]),
        11 if env.jungle_style == 0 => set([153, 147, 148, 149, 150 + hell]),
        11 => set([146, 154, 155, 156, 157 + hell]),
        12 => set([66, 67, 68, 193 + world_id.rem_euclid(4), 128 + hell]),
        13 => set([66, 67, 68, 188 + world_id.rem_euclid(5), 128 + hell]),
        14 => set([66, 67, 68, 197 + world_id.rem_euclid(3), 128 + hell]),
        15 => set([40, 33, 34, 200, 128 + hell]),
        16 => set([40, 33, 34, 201 + world_id.rem_euclid(2), 128 + hell]),
        17 => set([40, 33, 34, 203 + world_id.rem_euclid(4), 128 + hell]),
        18..=21 => {
            let b = 290 + 2 * (style - 18);
            t[0] = b;
            t[1] = b + 1;
        }
        _ => {}
    }
    if (0..3).contains(&hell) {
        t[5] = 125 + hell;
    }
    t[6] = 185 + hell;
    t
}

/// Style for an unzoomed screen at `(sx, sy)` of height `sh` (world pixels):
/// the cavern zone under its centre column, then the biomes around it.
pub fn style(env: &Env, height: u32, z: &Zones, (sx, sy, sh): (f64, f64, f64), sw: f64) -> i32 {
    let x = ((sx + (sw as i32 / 2) as f64) / 16.0) as i32;
    let zone = env.cave_x.iter().position(|&b| x <= b).unwrap_or(3);
    let mut s = env.cave_style[zone] + 3;
    if z.snow_tiles > 1500 && (sy + sh + 1200.0) / 16.0 < height as f64 - 250.0 {
        s = 1;
    }
    if z.jungle_tiles > 140 && (s != 1 || z.jungle_tiles > z.snow_tiles) {
        s = 11;
    }
    let ty = sy / 16.0;
    if z.beach {
        s = if z.corrupt {
            19
        } else if z.crimson {
            21
        } else if z.hallow {
            20
        } else {
            18
        };
    } else if ty > env.rock_layer + 60.0 && ty < magma_layer(env, height) - 60.0 {
        let evil = [z.corrupt, z.crimson, z.hallow].iter().position(|&b| b);
        if let Some(i) = evil {
            s = if z.snow { 15 } else { 12 } + i as i32;
        }
    }
    if z.glowshroom {
        s = 2;
    }
    s
}

const UNDERWORLD: [[usize; 5]; 3] = [[0, 1, 2, 3, 4], [5, 6, 7, 8, 9], [10, 11, 12, 13, 9]];

fn underworld_set(env: &Env) -> [usize; 5] {
    UNDERWORLD[(env.underworld_bg as usize).min(2)]
}

/// Every sheet the world's underground may draw.
pub fn sheets(env: &Env, world_id: i32) -> Vec<String> {
    let mut v: Vec<String> = (0..22).flat_map(|s| textures(s, env, world_id)).filter(|&t| t >= 0).map(name).collect();
    v.extend(underworld_set(env).map(underworld_name));
    v.sort();
    v.dedup();
    v
}

/// First row of the magma layer, in tiles.
pub fn magma_layer(env: &Env, height: u32) -> f64 {
    let bottom = height as f64 - 330.0;
    env.world_surface + (((bottom - env.world_surface) / 6.0) as i32 * 6) as f64 - 5.0
}

/// Where dirt rows end and rock rows start, in world pixels.
pub fn rock_transition(env: &Env) -> f64 {
    let span = (env.rock_layer as i32 - env.world_surface as i32) * 16;
    let rows = (span as f64 / ROW).ceil() as i32;
    (env.world_surface as i32 * 16 + rows * ROW as i32 + 32) as f64
}

/// `Math.IEEERemainder`.
fn ieee_rem(a: f64, b: f64) -> f64 {
    a - b * (a / b).round_ties_even()
}

pub struct View {
    /// Left edge of the unzoomed screen, world pixels: drives the parallax.
    pub screen_x: f64,
    /// Top edge and size of the unzoomed screen, world pixels.
    pub screen_y: f64,
    pub screen_w: f64,
    pub screen_h: f64,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

struct Out<'a> {
    sheets: &'a HashMap<String, SheetRef>,
    v: &'a View,
    sprites: Vec<Sprite>,
}

impl Out<'_> {
    /// Rows of `tex` from `top` down to `bottom`, `period` px repeats sideways
    /// from the sheet's column 16, `src_y` the sheet row of the frame.
    #[allow(clippy::too_many_arguments)]
    fn rows(&mut self, tex: i32, period: f64, top: f64, bottom: f64, row: f64, src_y: u32) {
        let Some(r) = self.sheets.get(&name(tex)) else { return };
        let origin =
            self.v.screen_x + (-ieee_rem(period + self.v.screen_x * PARALLAX, period) - (period / 2.0).floor()).trunc();
        let first = ((self.v.y0 - top) / row).floor().max(0.0);
        let mut y = top + first * row;
        while y < bottom.min(self.v.y1) {
            let h = row.min(bottom - y);
            let mut x = origin + ((self.v.x0 - origin) / period).floor() * period;
            while x < self.v.x1 {
                self.sprites.push(Sprite {
                    pos: [x as f32, y as f32],
                    size: [period as f32, h as f32],
                    color: TINT,
                    uv: [(r.x + 16) as f32, (r.y + src_y) as f32, period as f32, h as f32],
                    flags: r.layer | SPRITE_LIT,
                    ..Default::default()
                });
                x += period;
            }
            y += row;
        }
    }
}

/// Dirt, rock and magma layers in world pixels, back to front.
#[allow(clippy::too_many_arguments)]
pub fn layers(
    env: &Env,
    world_id: i32,
    height: u32,
    zones: &Zones,
    v: &View,
    ticks: u32,
    sheets: &HashMap<String, SheetRef>,
) -> Vec<Sprite> {
    let mut o = Out { sheets, v, sprites: Vec::new() };
    let surface = env.world_surface * 16.0;
    if surface > v.y1 {
        return o.sprites;
    }
    let tex = textures(style(env, height, zones, (v.screen_x, v.screen_y, v.screen_h), v.screen_w), env, world_id);
    if !sheets.contains_key(&name(tex[1])) {
        let top = surface.max(v.y0);
        o.sprites.push(Sprite {
            flags: SPRITE_LIT,
            ..Sprite::flat([v.x0 as f32, top as f32], [(v.x1 - v.x0) as f32, (v.y1 - top) as f32], FALLBACK)
        });
        return o.sprites;
    }
    let width = |t: i32| sheets.get(&name(t)).map_or(DEEP, |r| r.w as f64 - 32.0);
    // Lava glow frames advance every 8 ticks.
    let frame = (ticks / 8 % 3) as f64;
    let rock = rock_transition(env);
    let magma = magma_layer(env, height) * 16.0;
    let hell = (height as f64 - 200.0) * 16.0;

    o.rows(tex[0], width(tex[0]), surface, surface + 16.0, 16.0, 0);
    o.rows(tex[1], width(tex[1]), surface + 16.0, rock - 16.0, ROW, 0);
    o.rows(tex[2], DEEP, rock - 16.0, rock, 16.0, 0);

    let rock_rows = ((magma + 600.0 - rock) / ROW).floor().max(0.0);
    let rock_end = rock + rock_rows * ROW;
    let magma_top = magma + 608.0;
    let magma_end = magma_top + ((hell - magma_top) / ROW).ceil().max(0.0) * ROW;
    if magma <= v.y1 {
        // The game blanks the screen down to the hell strip before the deep layers.
        let (top, bottom) = (v.y0.max(rock), magma_end.min(v.y1));
        if top < bottom {
            o.sprites.push(Sprite::flat(
                [v.x0 as f32, top as f32],
                [(v.x1 - v.x0) as f32, (bottom - top) as f32],
                [0.0, 0.0, 0.0, 1.0],
            ));
        }
    }
    o.rows(tex[3], DEEP, rock, rock_end, ROW, 0);
    if rock <= v.y1 && magma <= v.y1 {
        o.rows(tex[4], DEEP, rock_end, rock_end + 16.0, 16.0, frame as u32 * 16);
    }
    if magma <= v.y1 {
        o.rows(tex[5], DEEP, magma_top, magma_end, ROW, (frame * ROW) as u32);
        o.rows(tex[6], DEEP, magma_end, magma_end + 16.0, 16.0, frame as u32 * 16);
    }
    o.sprites
}

/// The underworld's five parallax layers in unzoomed screen pixels, far to near,
/// for a screen of `w` x `h` centred on `center` (world pixels).
pub fn underworld(
    env: &Env,
    height: u32,
    center: [f32; 2],
    (w, h): (f32, f32),
    zoom: f32,
    ticks: u32,
    sheets: &HashMap<String, SheetRef>,
) -> Vec<Sprite> {
    let mut out = Vec::new();
    let layer_y = (height as f32 - 200.0) * 16.0;
    let screen = [center[0] - w / 2.0, center[1] - h / 2.0];
    if screen[1] + h < (height as f32 - 220.0) * 16.0 {
        return out;
    }
    let push_up = (zoom.clamp(1.0, 2.0) - 1.0) * 0.5 * 200.0;
    let anim = ((ticks as f32 / 60.0) % 3600.0 * 8.0) as u32 % 4;
    let set = underworld_set(env);
    for layer in (0..5).rev() {
        let tex = set[layer];
        let Some(r) = sheets.get(&underworld_name(tex)) else { continue };
        let (tw, th) = (r.w as f32, r.h as f32);
        let mut half = [tw * 0.5, th * 0.5];
        let depth = layer as f32 * 2.0 + 3.0;
        let scroll = 1.0 / depth;
        let mut src = [0.0, 0.0, tw, th];
        let mut scale = 1.3;
        let mut off = [0.0f32, 0.0];
        let quarter = |order_x_first: bool| {
            let (a, b) = if order_x_first { (anim % 2, anim >> 1) } else { (anim >> 1, anim % 2) };
            [a as f32 * (tw / 2.0).floor(), b as f32 * (th / 2.0).floor(), (tw / 2.0).floor(), (th / 2.0).floor()]
        };
        match tex {
            1 => {
                src = quarter(false);
                half = half.map(|k| k * 0.5);
                off[1] += 175.0;
            }
            2 => off[1] += 100.0,
            3 => off[1] += 75.0,
            4 => scale = 0.5,
            6..=8 => {
                src = quarter(true);
                half = half.map(|k| k * 0.5);
                match tex {
                    6 => off[1] -= 60.0,
                    7 => {
                        off[0] -= 400.0;
                        off[1] += 90.0;
                    }
                    _ => off[1] += 90.0,
                }
            }
            9 => off[1] -= 30.0,
            10 => off[1] += 250.0 * depth,
            11 => off[1] += 100.0 * depth,
            12 => off[1] += 20.0 * depth,
            13 => {
                off[1] += 20.0 * depth;
                src = quarter(true);
                half = half.map(|k| k * 0.5);
            }
            _ => {}
        }
        half = half.map(|k| k * scale);
        off[1] -= push_up;
        let step = scale * src[2];
        let mut first = ((center[0] * scroll - half[0] + off[0] - (w as i32 >> 1) as f32) as i32 as f32 / step) as i32;
        half = half.map(f32::floor);
        let count = (w / step).ceil() as i32;
        let stride = (scale * ((src[2] - 1.0) / scroll)) as i32;
        let mut pos = [0, 1].map(|i| {
            let base = [((first - 2) * stride) as f32, layer_y][i];
            ((base + half[i] - center[i]) * scroll + center[i] - screen[i] - half[i] + off[i]).floor()
        });
        while pos[0] + step < 0.0 {
            first += 1;
            pos[0] += step;
        }
        for _ in first - 2..=first + 4 + count {
            out.push(Sprite {
                pos,
                size: [src[2] * scale, src[3] * scale],
                color: [1.0; 4],
                uv: [r.x as f32 + src[0], r.y as f32 + src[1], src[2], src[3]],
                flags: r.layer,
                ..Default::default()
            });
            if layer == 0 {
                let top = pos[1] + src[3] * scale;
                out.push(Sprite::flat(
                    [pos[0], top],
                    [src[2] * scale, (h - top).max(0.0)],
                    [11.0 / 255.0, 3.0 / 255.0, 7.0 / 255.0, 1.0],
                ));
            }
            pos[0] += step;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Env {
        Env {
            world_surface: 465.0,
            rock_layer: 681.0,
            cave_x: [2853, 4713, 6400],
            cave_style: [7, 6, 5, 4],
            hell_style: 2,
            ..Env::default()
        }
    }

    #[test]
    fn layer_depths() {
        let e = env();
        // 216 tiles of dirt: 36 rows of 96 px, plus the 32 px of strips.
        assert_eq!(rock_transition(&e), 465.0 * 16.0 + 36.0 * 96.0 + 32.0);
        // (1800 - 330 - 465) / 6 = 167.5 -> 167 steps of 6 tiles.
        assert_eq!(magma_layer(&e, 1800), 465.0 + 167.0 * 6.0 - 5.0);
    }

    fn at(e: &Env, z: &Zones, x: f64, y: f64) -> i32 {
        style(e, 1800, z, (x * 16.0 - 800.0, y * 16.0, 900.0), 1600.0)
    }

    #[test]
    fn zones_pick_styles() {
        let e = env();
        let z = Zones::default();
        assert_eq!(at(&e, &z, 100.0, 700.0), 10);
        assert_eq!(at(&e, &z, 2853.0, 700.0), 10);
        assert_eq!(at(&e, &z, 2854.0, 700.0), 9);
        assert_eq!(at(&e, &z, 6399.0, 700.0), 8);
        assert_eq!(at(&e, &z, 7000.0, 700.0), 7);
        assert_eq!(textures(10, &e, 0), [121, 122, 123, 124, 142, 127, 187]);
    }

    #[test]
    fn biomes_override_zones() {
        let e = env();
        let snow = Zones { snow_tiles: 2000, snow: true, ..Zones::default() };
        assert_eq!(at(&e, &snow, 100.0, 700.0), 1);
        // Snow needs the screen bottom 1200 px above the underworld.
        assert_eq!(at(&e, &snow, 100.0, 1500.0), 10);
        // More jungle than snow wins.
        let mixed = Zones { jungle_tiles: 2500, jungle: true, ..snow };
        assert_eq!(at(&e, &mixed, 100.0, 700.0), 11);
        // Evil only between rock + 60 and magma - 60.
        let ice_evil = Zones { crimson: true, ..snow };
        assert_eq!(at(&e, &ice_evil, 100.0, 700.0), 1);
        assert_eq!(at(&e, &ice_evil, 100.0, 900.0), 16);
        assert_eq!(at(&e, &Zones { corrupt: true, ..Zones::default() }, 100.0, 900.0), 12);
        assert_eq!(at(&e, &Zones { beach: true, hallow: true, ..Zones::default() }, 100.0, 300.0), 20);
        assert_eq!(at(&e, &Zones { glowshroom: true, beach: true, ..Zones::default() }, 100.0, 300.0), 2);
    }
}
