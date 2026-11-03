//! Writes .wld files in the layout `wld` reads, for tests and benchmarks
//! without game files. Only sections 0..2 carry data; the rest are empty.

use crate::wld::{flags::*, Liquid, Tile, KNOWN_VERSION, NO_TILE};

pub fn write_world(
    name: &str,
    width: u32,
    height: u32,
    frame_important: &[bool],
    tile: impl Fn(u32, u32) -> Tile,
) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&KNOWN_VERSION.to_le_bytes());
    v.extend_from_slice(b"relogic\x02");
    v.extend_from_slice(&1u32.to_le_bytes());
    v.extend_from_slice(&0u64.to_le_bytes());
    v.extend_from_slice(&3i16.to_le_bytes());
    let ptrs = v.len();
    v.extend_from_slice(&[0; 12]);
    v.extend_from_slice(&(frame_important.len() as i16).to_le_bytes());
    let mut bits = vec![0u8; frame_important.len().div_ceil(8)];
    for (i, _) in frame_important.iter().enumerate().filter(|(_, &b)| b) {
        bits[i / 8] |= 1 << (i % 8);
    }
    v.extend_from_slice(&bits);

    let s0 = v.len() as u32;
    for s in [name, "0"] {
        v.push(s.len() as u8);
        v.extend_from_slice(s.as_bytes());
    }
    v.extend_from_slice(&[0; 8 + 16 + 4 + 16]);
    v.extend_from_slice(&(height as i32).to_le_bytes());
    v.extend_from_slice(&(width as i32).to_le_bytes());

    let s1 = v.len() as u32;
    for x in 0..width {
        let mut y = 0;
        while y < height {
            let t = tile(x, y);
            let mut run = 1;
            while y + run < height && run < i16::MAX as u32 && tile(x, y + run) == t {
                run += 1;
            }
            encode(&mut v, &t, run - 1, frame_important);
            y += run;
        }
    }
    let s2 = v.len() as u32;
    for (i, p) in [s0, s1, s2].iter().enumerate() {
        v[ptrs + i * 4..ptrs + i * 4 + 4].copy_from_slice(&p.to_le_bytes());
    }
    v
}

fn encode(v: &mut Vec<u8>, t: &Tile, repeat: u32, important: &[bool]) {
    let mut body = Vec::new();
    let (mut h1, mut h2, mut h3, mut h4) = (0u8, 0u8, 0u8, 0u8);
    if t.tile != NO_TILE {
        h1 |= 0x02;
        body.push(t.tile as u8);
        if t.tile > 0xFF {
            h1 |= 0x20;
            body.push((t.tile >> 8) as u8);
        }
        if important.get(t.tile as usize).copied().unwrap_or(false) {
            body.extend_from_slice(&t.frame_x.to_le_bytes());
            body.extend_from_slice(&t.frame_y.to_le_bytes());
        }
        if t.tile_paint != 0 {
            h3 |= 0x08;
            body.push(t.tile_paint);
        }
    }
    if t.wall != 0 {
        h1 |= 0x04;
        body.push(t.wall as u8);
        if t.wall_paint != 0 {
            h3 |= 0x10;
            body.push(t.wall_paint);
        }
    }
    if t.liquid != 0 {
        h1 |= match t.liquid_kind {
            Liquid::Water => 0x08,
            Liquid::Lava => 0x10,
            Liquid::Honey => 0x18,
            Liquid::Shimmer => {
                h3 |= 0x80;
                0x08
            }
        };
        body.push(t.liquid);
    }
    for (f, bit) in [(WIRE_RED, 0x02), (WIRE_BLUE, 0x04), (WIRE_GREEN, 0x08)] {
        if t.flags & f != 0 {
            h2 |= bit;
        }
    }
    h2 |= (((t.flags & SHAPE_MASK) >> SHAPE_SHIFT) as u8) << 4;
    for (f, bit) in [(ACTUATOR, 0x02), (INACTIVE, 0x04), (WIRE_YELLOW, 0x20)] {
        if t.flags & f != 0 {
            h3 |= bit;
        }
    }
    if t.wall > 0xFF {
        h3 |= 0x40;
        body.push((t.wall >> 8) as u8);
    }
    for (f, bit) in [(INVISIBLE_BLOCK, 0x02), (INVISIBLE_WALL, 0x04), (FULLBRIGHT_BLOCK, 0x08), (FULLBRIGHT_WALL, 0x10)]
    {
        if t.flags & f != 0 {
            h4 |= bit;
        }
    }
    match repeat {
        0 => {}
        1..=0xFF => {
            h1 |= 0x40;
            body.push(repeat as u8);
        }
        _ => {
            h1 |= 0x80;
            body.extend_from_slice(&(repeat as u16).to_le_bytes());
        }
    }
    if h4 != 0 {
        h3 |= 0x01;
    }
    if h3 != 0 {
        h2 |= 0x01;
    }
    if h2 != 0 {
        h1 |= 0x01;
    }
    v.push(h1);
    if h1 & 1 != 0 {
        v.push(h2);
    }
    if h2 & 1 != 0 {
        v.push(h3);
    }
    if h3 & 1 != 0 {
        v.push(h4);
    }
    v.extend_from_slice(&body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wld::{read_tiles, Header};

    #[test]
    fn roundtrip() {
        let important = [false, true];
        let f = |x: u32, y: u32| Tile {
            tile: if y > 3 { (x % 2) as u16 } else { NO_TILE },
            frame_x: if y > 3 && x % 2 == 1 { 18 } else { -1 },
            frame_y: if y > 3 && x % 2 == 1 { 0 } else { -1 },
            wall: if y > 5 { 300 } else { 0 },
            liquid: if y == 2 { 128 } else { 0 },
            liquid_kind: if x == 1 && y == 2 { Liquid::Shimmer } else { Liquid::Water },
            flags: if x == 2 { WIRE_YELLOW | FULLBRIGHT_WALL | (3 << SHAPE_SHIFT) } else { 0 },
            ..Tile::default()
        };
        let data = write_world("rt", 5, 600, &important, f);
        let h = Header::parse(&data).unwrap();
        let mut n = 0;
        read_tiles(&data, &h, |x, y, run, t| {
            for k in 0..run {
                assert_eq!(*t, f(x, y + k));
            }
            n += run;
        })
        .unwrap();
        assert_eq!(n, 5 * 600);
    }
}
