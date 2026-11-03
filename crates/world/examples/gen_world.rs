//! Writes a synthetic world (no game data) for benchmarks:
//! `cargo run --release -p world --example gen_world -- out.wld [width height]`

use formats::wld::{Liquid, Tile};

fn hash(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).expect("usage: gen_world <out.wld> [width height]");
    let w = args.get(2).map_or(8400, |s| s.parse().unwrap());
    let h = args.get(3).map_or(2400, |s| s.parse().unwrap());
    let mut important = vec![false; 700];
    important[4] = true;
    let surface = |x: u32| (h as f32 * 0.3 + (x as f32 * 0.01).sin() * 30.0 + (x as f32 * 0.07).sin() * 6.0) as u32;
    let cave = |x: u32, y: u32| {
        let v = (x as f32 * 0.031).sin() * (y as f32 * 0.043).cos() + (x as f32 * 0.011 + y as f32 * 0.017).sin();
        v > 1.1
    };
    let tile = |x: u32, y: u32| {
        let s = surface(x);
        let mut t = Tile::default();
        if y < s {
            if y + 1 == s && x.is_multiple_of(37) {
                t.tile = 4;
                t.frame_x = 0;
                t.frame_y = 0;
            }
            return t;
        }
        if y > s + 2 {
            t.wall = if y > s + 60 { 1 } else { 2 };
        }
        if cave(x, y) {
            if hash(x / 8, y / 8).is_multiple_of(9) {
                t.liquid = 255;
                t.liquid_kind = if y > h * 3 / 4 { Liquid::Lava } else { Liquid::Water };
            }
            return t;
        }
        t.tile = if y < s + 40 {
            0
        } else if hash(x, y).is_multiple_of(7) {
            7
        } else {
            1
        };
        t
    };
    let start = std::time::Instant::now();
    let data = formats::synth::write_world("Synthetic", w, h, &important, tile);
    std::fs::write(out, &data).unwrap();
    println!("{w}x{h}, {} MiB, {:.2?}", data.len() >> 20, start.elapsed());
}
