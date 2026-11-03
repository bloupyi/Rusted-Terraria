//! Terraria's multiplayer wire format, written from the public protocol
//! description (packet ids and field layouts as documented for 1.4).
//!
//! Frames are `len: u16` (header included), `type: u8`, then a little-endian
//! body. Strings are a 7-bit varint length then UTF-8. Only the packets this
//! game uses are read; the others are skipped.

use formats::wld::{flags, Liquid, Tile, NO_TILE};
use world::World;

use crate::net::{MobState, Msg};

/// Release number sent in the greeting, the one of the 1.4.5 world files.
pub const RELEASE: i32 = 326;
/// Tiles per section, as the world is streamed to a joining client.
pub const SECTION_W: u32 = 200;
pub const SECTION_H: u32 = 150;

/// Frame-important tile types for this release, one bit per type: frames of
/// these travel with the tile, the others are framed by the receiver.
const IMPORTANT: [u8; 95] = [
    56, 252, 63, 189, 30, 4, 134, 32, 128, 231, 254, 255, 255, 71, 6, 96, 243, 239, 33, 0, 32, 120, 4, 15, 0, 130, 150,
    31, 152, 250, 255, 64, 0, 224, 248, 239, 255, 255, 127, 244, 25, 192, 14, 32, 220, 31, 240, 23, 252, 15, 96, 124,
    152, 59, 248, 63, 240, 227, 255, 24, 241, 111, 10, 230, 192, 255, 62, 196, 251, 223, 177, 63, 248, 255, 255, 255,
    255, 251, 225, 165, 253, 127, 151, 3, 0, 0, 224, 253, 239, 31, 123, 32, 0, 128, 3,
];
pub const TILE_TYPES: usize = 754;

pub fn important_table() -> Vec<bool> {
    (0..TILE_TYPES).map(|t| IMPORTANT[t / 8] & (1 << (t % 8)) != 0).collect()
}

/// Wire ids of the monsters.
const NPC_SLIME: i16 = 1;
const NPC_ZOMBIE: i16 = 3;

pub mod id {
    pub const CONNECT: u8 = 1;
    pub const DISCONNECT: u8 = 2;
    pub const SLOT: u8 = 3;
    pub const PLAYER_INFO: u8 = 4;
    pub const REQUEST_WORLD: u8 = 6;
    pub const WORLD_INFO: u8 = 7;
    pub const REQUEST_TILES: u8 = 8;
    pub const STATUS: u8 = 9;
    pub const SECTION: u8 = 10;
    pub const SECTION_FRAME: u8 = 11;
    pub const SPAWN: u8 = 12;
    pub const PLAYER: u8 = 13;
    pub const ACTIVE: u8 = 14;
    pub const HP: u8 = 16;
    pub const TIME: u8 = 18;
    pub const TILE_SQUARE: u8 = 20;
    pub const NPC: u8 = 23;
    pub const STRIKE: u8 = 28;
    pub const MANA: u8 = 42;
    pub const LIQUID: u8 = 48;
    pub const COMPLETE: u8 = 49;
    pub const BUFFS: u8 = 50;
    pub const UUID: u8 = 68;
}

pub struct W(Vec<u8>);

impl W {
    pub fn new(tag: u8) -> Self {
        W(vec![0, 0, tag])
    }
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    pub fn i16(&mut self, v: i16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend_from_slice(b);
        self
    }
    pub fn str(&mut self, s: &str) -> &mut Self {
        let mut n = s.len();
        while n >= 0x80 {
            self.0.push(n as u8 | 0x80);
            n >>= 7;
        }
        self.0.push(n as u8);
        self.bytes(s.as_bytes())
    }
    /// Literal network text: mode 0, then the string.
    pub fn text(&mut self, s: &str) -> &mut Self {
        self.u8(0).str(s)
    }
    pub fn done(&mut self) -> Vec<u8> {
        let mut v = std::mem::take(&mut self.0);
        let len = v.len() as u16;
        v[..2].copy_from_slice(&len.to_le_bytes());
        v
    }
}

pub struct R<'a>(pub &'a [u8]);

impl R<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (a, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*a)
    }
    pub fn u8(&mut self) -> Option<u8> {
        self.take::<1>().map(|b| b[0])
    }
    pub fn i16(&mut self) -> Option<i16> {
        self.take().map(i16::from_le_bytes)
    }
    pub fn u16(&mut self) -> Option<u16> {
        self.take().map(u16::from_le_bytes)
    }
    pub fn i32(&mut self) -> Option<i32> {
        self.take().map(i32::from_le_bytes)
    }
    pub fn f32(&mut self) -> Option<f32> {
        self.take().map(f32::from_le_bytes)
    }
    pub fn str(&mut self) -> Option<String> {
        let (mut n, mut shift) = (0usize, 0);
        loop {
            let b = self.u8()?;
            n |= ((b & 0x7F) as usize) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return None;
            }
        }
        let s = self.0.get(..n)?;
        self.0 = &self.0[n..];
        Some(String::from_utf8_lossy(s).into_owned())
    }
    /// Network text; substitutions are dropped.
    pub fn text(&mut self) -> Option<String> {
        let mode = self.u8()?;
        let s = self.str()?;
        if mode != 0 {
            for _ in 0..self.u8()? {
                self.text()?;
            }
        }
        Some(s)
    }
}

/// Header fields of the world info a client needs.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldInfo {
    pub time: f32,
    pub width: u32,
    pub height: u32,
    pub world_id: i32,
    pub name: String,
}

/// What a frame carries, for the handshake and the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Packet {
    Connect(String),
    Disconnect(String),
    Slot(u8),
    RequestWorld,
    WorldInfo(WorldInfo),
    RequestTiles,
    Section {
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        tiles: Vec<Tile>,
    },
    Complete,
    /// One monster slot; `None` when it is free.
    Npc(u16, Option<MobState>),
    Msg(Msg),
    /// Read but not used here.
    Other(u8),
}

// Time of day: ours runs 0..1 from dawn, day then night; the wire counts
// ticks from the start of the day (54000) or of the night (32400).
const DAY_TICKS: f32 = 54000.0;
const NIGHT_TICKS: f32 = 32400.0;
const DAY_PART: f32 = DAY_TICKS / (DAY_TICKS + NIGHT_TICKS);

fn time_to_wire(t: f32) -> (bool, i32) {
    if t < DAY_PART {
        (true, (t / DAY_PART * DAY_TICKS) as i32)
    } else {
        (false, ((t - DAY_PART) / (1.0 - DAY_PART) * NIGHT_TICKS) as i32)
    }
}

fn time_from_wire(day: bool, ticks: i32) -> f32 {
    if day {
        (ticks as f32 / DAY_TICKS).clamp(0.0, 1.0) * DAY_PART
    } else {
        DAY_PART + (ticks as f32 / NIGHT_TICKS).clamp(0.0, 1.0) * (1.0 - DAY_PART)
    }
}

/// One tile in the section layout; `rle` repeats it that many more times.
pub fn write_tile(o: &mut Vec<u8>, t: &Tile, rle: u16, important: &[bool]) {
    let at = o.len();
    o.extend_from_slice(&[0, 0, 0]);
    let (mut f0, mut f1, mut f2) = (0u8, 0u8, 0u8);
    if t.tile != NO_TILE {
        f0 |= 0x02;
        if t.tile > 0xFF {
            f0 |= 0x20;
            o.extend_from_slice(&t.tile.to_le_bytes());
        } else {
            o.push(t.tile as u8);
        }
        if important.get(t.tile as usize).copied().unwrap_or(false) {
            o.extend_from_slice(&t.frame_x.to_le_bytes());
            o.extend_from_slice(&t.frame_y.to_le_bytes());
        }
        if t.tile_paint != 0 {
            f2 |= 0x08;
            o.push(t.tile_paint);
        }
    }
    if t.wall != 0 {
        f0 |= 0x04;
        o.push(t.wall as u8);
        if t.wall_paint != 0 {
            f2 |= 0x10;
            o.push(t.wall_paint);
        }
    }
    if t.liquid != 0 {
        // Shimmer has no code in this layout and goes as water.
        f0 |= match t.liquid_kind {
            Liquid::Lava => 0x10,
            Liquid::Honey => 0x18,
            Liquid::Water | Liquid::Shimmer => 0x08,
        };
        o.push(t.liquid);
    }
    let w = |bit| t.flags & bit != 0;
    f1 |= (w(flags::WIRE_RED) as u8) << 1 | (w(flags::WIRE_BLUE) as u8) << 2 | (w(flags::WIRE_GREEN) as u8) << 3;
    f1 |= (((t.flags & flags::SHAPE_MASK) >> flags::SHAPE_SHIFT) as u8) << 4;
    f2 |= (w(flags::ACTUATOR) as u8) << 1 | (w(flags::INACTIVE) as u8) << 2 | (w(flags::WIRE_YELLOW) as u8) << 5;
    if t.wall > 0xFF {
        f2 |= 0x40;
        o.push((t.wall >> 8) as u8);
    }
    if rle > 0xFF {
        f0 |= 0x80;
        o.extend_from_slice(&rle.to_le_bytes());
    } else if rle > 0 {
        f0 |= 0x40;
        o.push(rle as u8);
    }
    // Header: only as many flag bytes as are used.
    let used = if f2 != 0 {
        3
    } else if f1 != 0 {
        2
    } else {
        1
    };
    if used >= 2 {
        f0 |= 0x01;
    }
    if used == 3 {
        f1 |= 0x01;
    }
    o[at] = f0;
    o[at + 1] = f1;
    o[at + 2] = f2;
    o.drain(at + used..at + 3);
}

pub fn read_tile(r: &mut R, important: &[bool]) -> Option<(Tile, u16)> {
    let f0 = r.u8()?;
    let f1 = if f0 & 0x01 != 0 { r.u8()? } else { 0 };
    let f2 = if f1 & 0x01 != 0 { r.u8()? } else { 0 };
    let mut t = Tile::default();
    if f0 & 0x02 != 0 {
        t.tile = if f0 & 0x20 != 0 { r.u16()? } else { r.u8()? as u16 };
        if important.get(t.tile as usize).copied().unwrap_or(false) {
            (t.frame_x, t.frame_y) = (r.i16()?, r.i16()?);
        }
        if f2 & 0x08 != 0 {
            t.tile_paint = r.u8()?;
        }
    }
    if f0 & 0x04 != 0 {
        t.wall = r.u8()? as u16;
        if f2 & 0x10 != 0 {
            t.wall_paint = r.u8()?;
        }
    }
    let liquid = (f0 & 0x18) >> 3;
    if liquid != 0 {
        t.liquid = r.u8()?;
        t.liquid_kind = [Liquid::Water, Liquid::Water, Liquid::Lava, Liquid::Honey][liquid as usize];
    }
    let b = |byte: u8, bit: u8, flag: u16| if byte & bit != 0 { flag } else { 0 };
    t.flags = b(f1, 0x02, flags::WIRE_RED)
        | b(f1, 0x04, flags::WIRE_BLUE)
        | b(f1, 0x08, flags::WIRE_GREEN)
        | b(f2, 0x20, flags::WIRE_YELLOW)
        | b(f2, 0x02, flags::ACTUATOR)
        | b(f2, 0x04, flags::INACTIVE)
        | (((f1 >> 4) & 7) as u16) << flags::SHAPE_SHIFT;
    if f2 & 0x40 != 0 {
        t.wall |= (r.u8()? as u16) << 8;
    }
    let rle = match f0 >> 6 {
        0 => 0,
        1 => r.u8()? as u16,
        _ => r.u16()?,
    };
    Some((t, rle))
}

/// One section of the world, rows top to bottom, deflated.
pub fn section(world: &World, x0: u32, y0: u32, w: u32, h: u32) -> Vec<u8> {
    let important = &world.header.frame_important;
    let mut body = Vec::new();
    body.extend_from_slice(&(x0 as i32).to_le_bytes());
    body.extend_from_slice(&(y0 as i32).to_le_bytes());
    body.extend_from_slice(&(w as i16).to_le_bytes());
    body.extend_from_slice(&(h as i16).to_le_bytes());
    let cells: Vec<Tile> =
        (y0..y0 + h).flat_map(|y| (x0..x0 + w).map(move |x| (x, y))).map(|(x, y)| world.cell_data(x, y)).collect();
    let mut i = 0;
    while i < cells.len() {
        let mut n = 1;
        while i + n < cells.len() && n <= u16::MAX as usize && cells[i + n] == cells[i] {
            n += 1;
        }
        write_tile(&mut body, &cells[i], (n - 1) as u16, important);
        i += n;
    }
    // No chests, signs or tile entities.
    body.extend_from_slice(&[0; 6]);
    let mut o = W::new(id::SECTION);
    o.u8(1).bytes(&miniz_oxide::deflate::compress_to_vec(&body, 6));
    o.done()
}

fn read_section(r: &mut R, important: &[bool]) -> Option<Packet> {
    let deflated;
    let mut r = if r.u8()? != 0 {
        deflated = miniz_oxide::inflate::decompress_to_vec(r.0).ok()?;
        R(&deflated)
    } else {
        R(r.0)
    };
    let (x, y) = (r.i32()?, r.i32()?);
    let (w, h) = (r.i16()?, r.i16()?);
    if x < 0 || y < 0 || w < 0 || h < 0 {
        return None;
    }
    let n = w as usize * h as usize;
    let mut tiles = Vec::with_capacity(n);
    while tiles.len() < n {
        let (t, rle) = read_tile(&mut r, important)?;
        let k = (rle as usize + 1).min(n - tiles.len());
        tiles.extend(std::iter::repeat_n(t, k));
    }
    Some(Packet::Section { x: x as u32, y: y as u32, w: w as u32, h: h as u32, tiles })
}

/// The whole world info, in the documented order; what this game does not
/// track is sent as zero.
pub fn world_info(header: &formats::wld::Header, time: f32) -> Vec<u8> {
    let (day, ticks) = time_to_wire(time);
    let (w, h) = (header.width as i16, header.height as i16);
    let mut o = W::new(id::WORLD_INFO);
    o.i32(ticks).u8(day as u8).u8(0).i16(w).i16(h);
    // Spawn, surface and cavern layer.
    o.i16(w / 2).i16(-1).i16(h * 3 / 10).i16(h * 4 / 10);
    o.i32(header.world_id).str(&header.name).u8(0);
    // Unique id, generator version, moon type, backgrounds and styles.
    o.bytes(&[0; 16]).bytes(&[0; 8]).u8(0).bytes(&[0; 14]).bytes(&[0; 3]);
    o.f32(0.0).u8(0).bytes(&[0; 12]).bytes(&[0; 4]).bytes(&[0; 12]).bytes(&[0; 4]).bytes(&[0; 4]).bytes(&[0; 9]);
    o.f32(0.0).bytes(&[0; 7]);
    // Ore tiers: copper, iron, silver, gold, then undecided hardmode ores.
    for t in [7i16, 6, 9, 8, -1, -1, -1] {
        o.i16(t);
    }
    o.u8(0).bytes(&[0; 8]).f32(0.0);
    o.done()
}

/// Decodes one frame, without its length.
pub fn decode(frame: &[u8], important: &[bool]) -> Option<Packet> {
    let (&tag, body) = frame.split_first()?;
    let mut r = R(body);
    Some(match tag {
        id::CONNECT => Packet::Connect(r.str()?),
        id::DISCONNECT => Packet::Disconnect(r.text()?),
        id::SLOT => Packet::Slot(r.u8()?),
        id::REQUEST_WORLD => Packet::RequestWorld,
        id::WORLD_INFO => {
            let ticks = r.i32()?;
            let day = r.u8()? & 1 != 0;
            let _moon = r.u8()?;
            let (w, h) = (r.i16()?, r.i16()?);
            for _ in 0..4 {
                r.i16()?;
            }
            let world_id = r.i32()?;
            let name = r.str()?;
            if w <= 0 || h <= 0 {
                return None;
            }
            Packet::WorldInfo(WorldInfo {
                time: time_from_wire(day, ticks),
                width: w as u32,
                height: h as u32,
                world_id,
                name,
            })
        }
        id::REQUEST_TILES => Packet::RequestTiles,
        id::SECTION => read_section(&mut r, important)?,
        id::COMPLETE => Packet::Complete,
        id::PLAYER => {
            let pid = r.u8()?;
            let control = r.u8()?;
            let pulley = r.u8()?;
            let _misc = r.u8()?;
            let _sleep = r.u8()?;
            let _item = r.u8()?;
            let (x, y) = (r.f32()?, r.f32()?);
            let (vx, vy) = if pulley & 0x04 != 0 { (r.f32()?, r.f32()?) } else { (0.0, 0.0) };
            Packet::Msg(Msg::Player { id: pid, x, y, vx, vy, facing_left: control & 0x40 == 0 })
        }
        id::ACTIVE => {
            let pid = r.u8()?;
            if r.u8()? != 0 {
                return Some(Packet::Other(tag));
            }
            Packet::Msg(Msg::Leave { id: pid })
        }
        id::TIME => {
            let day = r.u8()? != 0;
            Packet::Msg(Msg::Time(time_from_wire(day, r.i32()?)))
        }
        id::TILE_SQUARE => {
            let size = r.u16()?;
            if size & 0x8000 != 0 {
                r.u8()?;
            }
            let (x, y) = (r.i16()?, r.i16()?);
            if size & 0x7FFF != 1 || x < 0 || y < 0 {
                return Some(Packet::Other(tag));
            }
            let (cell, _) = read_tile(&mut r, important)?;
            Packet::Msg(Msg::Tile { x: x as u32, y: y as u32, cell })
        }
        id::NPC => {
            let index = r.i16()?;
            let (x, y) = (r.f32()?, r.f32()?);
            let _v = (r.f32()?, r.f32()?);
            let _target = r.u16()?;
            let (f0, f1) = (r.u8()?, r.u8()?);
            for bit in [0x04, 0x08, 0x10, 0x20] {
                if f0 & bit != 0 {
                    r.f32()?;
                }
            }
            let net_id = r.i16()?;
            if f1 & 0x01 != 0 {
                r.u8()?;
            }
            if f1 & 0x04 != 0 {
                r.f32()?;
            }
            // Full life when the field is left out.
            let hurt = f0 & 0x80 == 0;
            let kind = match net_id {
                NPC_SLIME => Some(world::mob::Kind::Slime as u8),
                NPC_ZOMBIE => Some(world::mob::Kind::Zombie as u8),
                _ => None,
            };
            if index < 0 {
                return None;
            }
            Packet::Npc(index as u16, kind.map(|kind| MobState { kind, x, y, facing_left: f0 & 0x40 == 0, hurt }))
        }
        id::STRIKE => {
            let index = r.i16()?;
            let damage = r.i16()?;
            if index < 0 || damage < 0 {
                return None;
            }
            Packet::Msg(Msg::Hit { index: index as u16, damage: damage as u16 })
        }
        id::LIQUID => {
            let (x, y) = (r.i16()?, r.i16()?);
            if x < 0 || y < 0 {
                return None;
            }
            Packet::Msg(Msg::Liquid { x: x as u32, y: y as u32, amount: r.u8()?, kind: r.u8()? & 3 })
        }
        _ => Packet::Other(tag),
    })
}

/// A monster slot; `None` frees it.
pub fn npc(index: u16, m: Option<&MobState>) -> Vec<u8> {
    let mut o = W::new(id::NPC);
    let (x, y, right, hurt, net_id) = match m {
        Some(m) => {
            let net_id = if m.kind == world::mob::Kind::Zombie as u8 { NPC_ZOMBIE } else { NPC_SLIME };
            (m.x, m.y, !m.facing_left, m.hurt, net_id)
        }
        None => (0.0, 0.0, false, false, 0),
    };
    o.i16(index as i16).f32(x).f32(y).f32(0.0).f32(0.0).u16(255);
    o.u8(if right { 0x40 } else { 0 } | if hurt { 0 } else { 0x80 }).u8(0).i16(net_id);
    if hurt {
        // Life as a one-byte field: hurt but alive.
        o.u8(1).u8(1);
    }
    o.done()
}

/// Frames for a game message; monsters go one slot per frame.
pub fn encode(m: &Msg) -> Vec<Vec<u8>> {
    let one = |w: &mut W| vec![w.done()];
    match m {
        Msg::Welcome { .. } => Vec::new(),
        Msg::Player { id: pid, x, y, vx, vy, facing_left } => one(W::new(id::PLAYER)
            .u8(*pid)
            .u8(if *facing_left { 0 } else { 0x40 })
            .u8(0x04)
            .u8(0)
            .u8(0)
            .u8(0)
            .f32(*x)
            .f32(*y)
            .f32(*vx)
            .f32(*vy)),
        Msg::Tile { x, y, cell } => {
            let mut o = W::new(id::TILE_SQUARE);
            o.u16(1).i16(*x as i16).i16(*y as i16);
            write_tile(&mut o.0, cell, 0, &important_table());
            one(&mut o)
        }
        Msg::Liquid { x, y, amount, kind } => {
            one(W::new(id::LIQUID).i16(*x as i16).i16(*y as i16).u8(*amount).u8(*kind))
        }
        Msg::Mobs(mobs) => mobs.iter().enumerate().map(|(i, s)| npc(i as u16, Some(s))).collect(),
        Msg::Hit { index, damage } => {
            one(W::new(id::STRIKE).i16(*index as i16).i16(*damage as i16).f32(0.0).u8(0).u8(0))
        }
        Msg::Leave { id: pid } => one(W::new(id::ACTIVE).u8(*pid).u8(0)),
        Msg::Time(t) => {
            let (day, ticks) = time_to_wire(*t);
            one(W::new(id::TIME).u8(day as u8).i32(ticks).i16(0).i16(0))
        }
    }
}

/// What a client tells about itself after getting its slot.
pub fn greeting(slot: u8) -> Vec<Vec<u8>> {
    let mut v = Vec::new();
    let mut info = W::new(id::PLAYER_INFO);
    info.u8(slot).u8(0).u8(0).str("Player").u8(0).u8(0).u8(0).u8(0);
    // Hair, skin, eyes, shirt, undershirt, pants, shoes.
    for c in [
        [215, 90, 55],
        [255, 125, 90],
        [105, 90, 75],
        [175, 165, 140],
        [160, 180, 215],
        [255, 230, 175],
        [160, 105, 60],
    ] {
        info.bytes(&c);
    }
    v.push(info.u8(0).done());
    v.push(W::new(id::UUID).str("00000000-0000-4000-8000-000000000000").done());
    v.push(W::new(id::HP).u8(slot).i16(100).i16(100).done());
    v.push(W::new(id::MANA).u8(slot).i16(20).i16(20).done());
    let mut buffs = W::new(id::BUFFS);
    buffs.u8(slot).bytes(&[0; 44]);
    v.push(buffs.done());
    v.push(W::new(id::REQUEST_WORLD).done());
    v
}

pub fn connect() -> Vec<u8> {
    W::new(id::CONNECT).str(&format!("Terraria{RELEASE}")).done()
}

pub fn request_tiles() -> Vec<u8> {
    W::new(id::REQUEST_TILES).i32(-1).i32(-1).done()
}

pub fn spawn(slot: u8) -> Vec<u8> {
    W::new(id::SPAWN).u8(slot).i16(-1).i16(-1).i32(0).u8(1).done()
}

pub fn slot(slot: u8) -> Vec<u8> {
    W::new(id::SLOT).u8(slot).done()
}

pub fn disconnect(reason: &str) -> Vec<u8> {
    W::new(id::DISCONNECT).text(reason).done()
}

/// Everything a client needs after asking for tiles: a status line, every
/// section, the framing order and the go to spawn.
pub fn world_frames(world: &World) -> Vec<Vec<u8>> {
    let (sx, sy) = (world.width.div_ceil(SECTION_W), world.height.div_ceil(SECTION_H));
    let mut v = vec![W::new(id::STATUS).i32(0).text("Receiving tile data").u8(0).done()];
    for j in 0..sy {
        for i in 0..sx {
            let (x, y) = (i * SECTION_W, j * SECTION_H);
            v.push(section(world, x, y, SECTION_W.min(world.width - x), SECTION_H.min(world.height - y)));
        }
    }
    v.push(W::new(id::SECTION_FRAME).i16(0).i16(0).i16(sx as i16 - 1).i16(sy as i16 - 1).done());
    v.push(W::new(id::COMPLETE).done());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(frame: &[u8]) -> &[u8] {
        assert_eq!(u16::from_le_bytes([frame[0], frame[1]]) as usize, frame.len());
        &frame[2..]
    }

    #[test]
    fn strings_use_varint_lengths() {
        let long = "x".repeat(300);
        let f = W::new(1).str(&long).done();
        assert_eq!(&f[3..5], &[0xAC, 0x02]);
        assert_eq!(decode(body(&f), &[]), Some(Packet::Connect(long)));
    }

    #[test]
    fn tiles_round_trip() {
        let important = important_table();
        assert!(important[4] && important[18] && !important[1], "torch, bench, stone");
        let tiles = [
            Tile::default(),
            Tile { tile: 1, wall: 300, wall_paint: 2, flags: flags::WIRE_RED | flags::INACTIVE, ..Tile::default() },
            Tile { tile: 18, frame_x: 18, frame_y: 0, tile_paint: 5, ..Tile::default() },
            Tile {
                tile: 600,
                liquid: 255,
                liquid_kind: Liquid::Honey,
                flags: 3 << flags::SHAPE_SHIFT,
                ..Tile::default()
            },
            Tile { liquid: 10, liquid_kind: Liquid::Lava, ..Tile::default() },
        ];
        for (i, t) in tiles.iter().enumerate() {
            let mut o = Vec::new();
            write_tile(&mut o, t, i as u16 * 200, &important);
            let mut r = R(&o);
            assert_eq!(read_tile(&mut r, &important), Some((*t, i as u16 * 200)));
            assert!(r.0.is_empty());
        }
    }

    #[test]
    fn sections_carry_the_world() {
        let important = important_table();
        let w = World::parse(&formats::synth::write_world("s", 250, 160, &important, |x, y| Tile {
            tile: if y > 100 {
                1
            } else if (x, y) == (3, 100) {
                4
            } else {
                NO_TILE
            },
            frame_x: if (x, y) == (3, 100) { 22 } else { -1 },
            frame_y: if (x, y) == (3, 100) { 0 } else { -1 },
            wall: (x % 7 == 0) as u16 * 2,
            ..Tile::default()
        }))
        .unwrap();
        let frames = world_frames(&w);
        assert_eq!(frames.len(), 1 + 2 * 2 + 2);
        let mut got =
            World::parse(&formats::synth::write_world("g", 250, 160, &important, |_, _| Tile::default())).unwrap();
        for f in &frames {
            if let Some(Packet::Section { x, y, w: sw, h: sh, tiles }) = decode(body(f), &important) {
                assert_eq!(tiles.len() as u32, sw * sh);
                for (i, t) in tiles.iter().enumerate() {
                    got.put_cell(x + i as u32 % sw, y + i as u32 / sw, t);
                }
            }
        }
        for y in 0..160 {
            for x in 0..250 {
                assert_eq!(got.cell_data(x, y), w.cell_data(x, y), "{x},{y}");
            }
        }
        assert_eq!(decode(body(frames.last().unwrap()), &important), Some(Packet::Complete));
        let info = world_info(&w.header, 0.25);
        match decode(body(&info), &important) {
            Some(Packet::WorldInfo(i)) => {
                assert_eq!((i.width, i.height, i.name.as_str()), (250, 160, "s"));
                assert!((i.time - 0.25).abs() < 1e-3);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn game_messages_round_trip() {
        let important = important_table();
        let msgs = [
            Msg::Player { id: 1, x: 1.5, y: -2.0, vx: 3.0, vy: 4.0, facing_left: true },
            Msg::Tile { x: 10, y: 20, cell: Tile { tile: 18, frame_x: 36, frame_y: 0, wall: 4, ..Tile::default() } },
            Msg::Liquid { x: 1, y: 2, amount: 200, kind: 1 },
            Msg::Hit { index: 7, damage: 12 },
            Msg::Leave { id: 2 },
            Msg::Time(0.8),
        ];
        for m in msgs {
            let f = encode(&m);
            assert_eq!(f.len(), 1);
            let got = decode(body(&f[0]), &important);
            match (&m, got) {
                (Msg::Time(a), Some(Packet::Msg(Msg::Time(b)))) => assert!((a - b).abs() < 1e-3),
                (_, got) => assert_eq!(got, Some(Packet::Msg(m.clone()))),
            }
        }
        let s = MobState { kind: world::mob::Kind::Zombie as u8, x: 5.0, y: 6.0, facing_left: false, hurt: true };
        let f = encode(&Msg::Mobs(vec![s, MobState { hurt: false, facing_left: true, ..s }]));
        assert_eq!(decode(body(&f[0]), &important), Some(Packet::Npc(0, Some(s))));
        assert_eq!(
            decode(body(&f[1]), &important),
            Some(Packet::Npc(1, Some(MobState { hurt: false, facing_left: true, ..s })))
        );
        assert_eq!(decode(body(&npc(3, None)), &important), Some(Packet::Npc(3, None)));
        assert_eq!(decode(&[id::PLAYER, 0], &important), None);
    }
}
