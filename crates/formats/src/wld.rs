//! Terraria world file (.wld), versions with section offsets.
//!
//! Field layout comes from community format documentation and observation of
//! saved files. Uncertain points are marked TODO.

use crate::reader::{bad, Error, Reader};

/// Oldest version accepted. TODO: 1.3.x (< 225) shares the layout as far as
/// documented, but has not been checked against real files.
pub const MIN_VERSION: i32 = 225;
/// Newest version the tile layout below is known for (1.4.4.x).
/// TODO: newer versions are parsed with the same layout; the section end check
/// will reject them if the tile record changed.
pub const KNOWN_VERSION: i32 = 279;

const MAGIC: &[u8; 7] = b"relogic";
const FILE_TYPE_WORLD: u8 = 2;

#[derive(Debug, Clone)]
pub struct Header {
    pub version: i32,
    pub revision: u32,
    pub sections: Vec<u32>,
    /// Per tile type: frame_x/frame_y are stored in the file.
    pub frame_important: Vec<bool>,
    pub name: String,
    pub seed: String,
    pub world_id: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Liquid {
    #[default]
    Water = 0,
    Lava = 1,
    Honey = 2,
    Shimmer = 3,
}

pub mod flags {
    pub const WIRE_RED: u16 = 1 << 0;
    pub const WIRE_BLUE: u16 = 1 << 1;
    pub const WIRE_GREEN: u16 = 1 << 2;
    pub const WIRE_YELLOW: u16 = 1 << 3;
    pub const ACTUATOR: u16 = 1 << 4;
    pub const INACTIVE: u16 = 1 << 5;
    pub const INVISIBLE_BLOCK: u16 = 1 << 6;
    pub const INVISIBLE_WALL: u16 = 1 << 7;
    pub const FULLBRIGHT_BLOCK: u16 = 1 << 8;
    pub const FULLBRIGHT_WALL: u16 = 1 << 9;
    /// Raw 3-bit shape: 0 full, 1 half block, 2..=5 slopes.
    pub const SHAPE_SHIFT: u16 = 10;
    pub const SHAPE_MASK: u16 = 0b111 << SHAPE_SHIFT;
}

pub const NO_TILE: u16 = u16::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    /// `NO_TILE` when inactive.
    pub tile: u16,
    /// -1 when the type is not frame-important (framed at runtime).
    pub frame_x: i16,
    pub frame_y: i16,
    pub tile_paint: u8,
    /// 0 = no wall.
    pub wall: u16,
    pub wall_paint: u8,
    /// 0..=255, 0 = none.
    pub liquid: u8,
    pub liquid_kind: Liquid,
    pub flags: u16,
}

impl Default for Tile {
    fn default() -> Self {
        Self {
            tile: NO_TILE,
            frame_x: -1,
            frame_y: -1,
            tile_paint: 0,
            wall: 0,
            wall_paint: 0,
            liquid: 0,
            liquid_kind: Liquid::Water,
            flags: 0,
        }
    }
}

impl Header {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        let version = r.i32()?;
        if version < MIN_VERSION {
            return bad(format!("world version {version} not supported (min {MIN_VERSION})"));
        }
        if r.bytes(7)? != MAGIC || r.u8()? != FILE_TYPE_WORLD {
            return bad("bad relogic magic");
        }
        let revision = r.u32()?;
        let _favorite = r.u64()?;
        let n = r.i16()?;
        if n < 3 {
            return bad("too few sections");
        }
        let sections = (0..n).map(|_| r.u32()).collect::<Result<Vec<_>, _>>()?;

        let count = r.i16()?.max(0) as usize;
        let bits = r.bytes(count.div_ceil(8))?;
        let frame_important = (0..count).map(|i| bits[i / 8] & (1 << (i % 8)) != 0).collect();

        r.seek(sections[0] as usize)?;
        let name = r.string()?;
        let seed = r.string()?;
        let _generator_version = r.u64()?;
        let _guid = r.bytes(16)?;
        let world_id = r.i32()?;
        let _bounds = r.bytes(16)?;
        let height = r.i32()?;
        let width = r.i32()?;
        if !(1..=1 << 16).contains(&width) || !(1..=1 << 16).contains(&height) {
            return bad(format!("bad world size {width}x{height}"));
        }
        Ok(Self {
            version,
            revision,
            sections,
            frame_important,
            name,
            seed,
            world_id,
            width: width as u32,
            height: height as u32,
        })
    }
}

/// Parses section 2. Tiles are stored column-major (x outer, y inner) with
/// vertical run-length repeats; `sink(x, y, run, tile)` gets each run.
pub fn read_tiles(data: &[u8], h: &Header, mut sink: impl FnMut(u32, u32, u32, &Tile)) -> Result<(), Error> {
    let mut r = Reader::new(data);
    r.seek(h.sections[1] as usize)?;
    let v269 = h.version >= 269;
    for x in 0..h.width {
        let mut y = 0;
        while y < h.height {
            let (t, run) = read_tile(&mut r, &h.frame_important, v269)?;
            let run = run.min(h.height - y);
            sink(x, y, run, &t);
            y += run;
        }
    }
    if r.pos != h.sections[2] as usize {
        return bad(format!(
            "tile section ends at {} instead of {} (unknown tile layout for version {})",
            r.pos, h.sections[2], h.version
        ));
    }
    Ok(())
}

#[inline]
fn read_tile(r: &mut Reader, important: &[bool], v269: bool) -> Result<(Tile, u32), Error> {
    use flags::*;
    let h1 = r.u8()?;
    let h2 = if h1 & 0x01 != 0 { r.u8()? } else { 0 };
    let h3 = if h2 & 0x01 != 0 { r.u8()? } else { 0 };
    let h4 = if v269 && h3 & 0x01 != 0 { r.u8()? } else { 0 };
    let mut t = Tile::default();

    if h1 & 0x02 != 0 {
        let mut ty = r.u8()? as u16;
        if h1 & 0x20 != 0 {
            ty |= (r.u8()? as u16) << 8;
        }
        t.tile = ty;
        if important.get(ty as usize).copied().unwrap_or(false) {
            t.frame_x = r.i16()?;
            t.frame_y = r.i16()?;
        }
        if h3 & 0x08 != 0 {
            t.tile_paint = r.u8()?;
        }
    }
    if h1 & 0x04 != 0 {
        t.wall = r.u8()? as u16;
        if h3 & 0x10 != 0 {
            t.wall_paint = r.u8()?;
        }
    }
    let liquid_bits = (h1 & 0x18) >> 3;
    if liquid_bits != 0 {
        t.liquid = r.u8()?;
        // TODO: shimmer as h3 bit 7 is from documentation, not seen in a real save yet.
        t.liquid_kind = if h3 & 0x80 != 0 {
            Liquid::Shimmer
        } else {
            match liquid_bits {
                1 => Liquid::Water,
                2 => Liquid::Lava,
                _ => Liquid::Honey,
            }
        };
    }
    if h2 & 0x02 != 0 {
        t.flags |= WIRE_RED;
    }
    if h2 & 0x04 != 0 {
        t.flags |= WIRE_BLUE;
    }
    if h2 & 0x08 != 0 {
        t.flags |= WIRE_GREEN;
    }
    t.flags |= (((h2 >> 4) & 0b111) as u16) << SHAPE_SHIFT;
    if h3 & 0x02 != 0 {
        t.flags |= ACTUATOR;
    }
    if h3 & 0x04 != 0 {
        t.flags |= INACTIVE;
    }
    if h3 & 0x20 != 0 {
        t.flags |= WIRE_YELLOW;
    }
    if h3 & 0x40 != 0 {
        t.wall |= (r.u8()? as u16) << 8;
    }
    for (bit, f) in [(0x02, INVISIBLE_BLOCK), (0x04, INVISIBLE_WALL), (0x08, FULLBRIGHT_BLOCK), (0x10, FULLBRIGHT_WALL)] {
        if h4 & bit != 0 {
            t.flags |= f;
        }
    }
    let repeat = match h1 >> 6 {
        0 => 0,
        1 => r.u8()? as u32,
        2 => r.i16()? as u16 as u32,
        _ => return bad("bad tile run-length mode"),
    };
    Ok((t, repeat + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal world written by hand: 2x3 tiles, frame-important type 1.
    fn synthetic(version: i32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&version.to_le_bytes());
        v.extend_from_slice(b"relogic\x02");
        v.extend_from_slice(&7u32.to_le_bytes());
        v.extend_from_slice(&0u64.to_le_bytes());
        v.extend_from_slice(&3i16.to_le_bytes());
        let ptrs = v.len();
        v.extend_from_slice(&[0; 12]);
        v.extend_from_slice(&2i16.to_le_bytes());
        v.push(0b10);

        let s0 = v.len() as u32;
        for s in ["Test", "1234"] {
            v.push(s.len() as u8);
            v.extend_from_slice(s.as_bytes());
        }
        v.extend_from_slice(&0u64.to_le_bytes());
        v.extend_from_slice(&[0; 16]);
        v.extend_from_slice(&42i32.to_le_bytes());
        v.extend_from_slice(&[0; 16]);
        v.extend_from_slice(&3i32.to_le_bytes());
        v.extend_from_slice(&2i32.to_le_bytes());

        let s1 = v.len() as u32;
        // x=0: dirt (type 0) + wall 300 + red wire, repeated 2 more times
        v.extend_from_slice(&[0x01 | 0x02 | 0x04 | 0x40, 0x01 | 0x02, 0x40, 0, 44, 1, 2]);
        // x=1, y=0: frame-important type 1, frame (18, 36), lava 255
        v.extend_from_slice(&[0x02 | 0x10, 1, 18, 0, 36, 0, 255]);
        // x=1, y=1..3: empty, i16 run
        v.extend_from_slice(&[0x80, 1, 0]);
        let s2 = v.len() as u32;
        for (i, p) in [s0, s1, s2].iter().enumerate() {
            v[ptrs + i * 4..ptrs + i * 4 + 4].copy_from_slice(&p.to_le_bytes());
        }
        v
    }

    #[test]
    fn header() {
        let h = Header::parse(&synthetic(279)).unwrap();
        assert_eq!((h.version, h.revision, h.width, h.height), (279, 7, 2, 3));
        assert_eq!((h.name.as_str(), h.seed.as_str(), h.world_id), ("Test", "1234", 42));
        assert_eq!(h.frame_important, vec![false, true]);
        assert!(Header::parse(&synthetic(100)).is_err());
    }

    #[test]
    fn tiles() {
        let data = synthetic(279);
        let h = Header::parse(&data).unwrap();
        let mut runs = Vec::new();
        read_tiles(&data, &h, |x, y, n, t| runs.push((x, y, n, *t))).unwrap();
        assert_eq!(runs.len(), 3);
        let (x, y, n, t) = runs[0];
        assert_eq!((x, y, n, t.tile, t.wall, t.frame_x), (0, 0, 3, 0, 300, -1));
        assert_eq!(t.flags, flags::WIRE_RED);
        let (_, _, _, t) = runs[1];
        assert_eq!((t.tile, t.frame_x, t.frame_y, t.liquid, t.liquid_kind), (1, 18, 36, 255, Liquid::Lava));
        assert_eq!((runs[2].1, runs[2].2, runs[2].3.tile), (1, 2, NO_TILE));
    }

    /// Set RUSTED_TEST_WLD to a real world file; skipped otherwise.
    #[test]
    fn real_world() {
        let Some(path) = std::env::var_os("RUSTED_TEST_WLD") else {
            eprintln!("RUSTED_TEST_WLD not set, skipped");
            return;
        };
        let data = std::fs::read(path).unwrap();
        let h = Header::parse(&data).unwrap();
        assert!(h.width >= 1000 && h.height >= 300);
        let mut cells = 0u64;
        read_tiles(&data, &h, |_, _, n, _| cells += n as u64).unwrap();
        assert_eq!(cells, h.width as u64 * h.height as u64);
    }
}
