//! XNB container (XNA 4.0 / FNA) and its Texture2D and SoundEffect payloads.

use crate::reader::{bad, Error, Reader};

pub const FLAG_HIDEF: u8 = 0x01;
pub const FLAG_LZ4: u8 = 0x40;
pub const FLAG_LZX: u8 = 0x80;

const TEXTURE2D_READER: &str = "Microsoft.Xna.Framework.Content.Texture2DReader";
const SOUND_EFFECT_READER: &str = "Microsoft.Xna.Framework.Content.SoundEffectReader";
const SURFACE_COLOR: i32 = 0;
const WAVE_FORMAT_PCM: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub platform: u8,
    pub version: u8,
    pub flags: u8,
    pub file_size: u32,
    /// Size of the content after the header, only present when compressed.
    pub decompressed_size: Option<u32>,
}

impl Header {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        if r.bytes(3)? != b"XNB" {
            return bad("not an XNB file");
        }
        let platform = r.u8()?;
        let version = r.u8()?;
        let flags = r.u8()?;
        let file_size = r.u32()?;
        if flags & FLAG_LZX != 0 && flags & FLAG_LZ4 != 0 {
            return bad("XNB has both LZX and LZ4 flags");
        }
        let decompressed_size = if flags & (FLAG_LZX | FLAG_LZ4) != 0 { Some(r.u32()?) } else { None };
        Ok(Self { platform, version, flags, file_size, decompressed_size })
    }

    pub fn size(&self) -> usize {
        if self.decompressed_size.is_some() {
            14
        } else {
            10
        }
    }
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major, mip 0 only. XNA content is premultiplied alpha by default.
    pub rgba: Vec<u8>,
}

/// Returns the content bytes following the header, decompressed if needed.
pub fn content(data: &[u8]) -> Result<(Header, Vec<u8>), Error> {
    let h = Header::parse(data)?;
    let end = (h.file_size as usize).min(data.len());
    let body = data.get(h.size()..end).ok_or(Error::Eof)?;
    let out = match h.decompressed_size {
        None => body.to_vec(),
        Some(size) if h.flags & FLAG_LZX != 0 => decompress_lzx(body, size as usize)?,
        // Terraria 1.4.5 ships LZX (13 993 files) or uncompressed (22), never MonoGame's LZ4.
        Some(_) => return bad("LZ4 XNB not supported (not used by Terraria)"),
    };
    Ok((h, out))
}

/// XNA splits the LZX stream in frames: `[0xFF, frame_size: u16be, block_size: u16be]`
/// or `[block_size: u16be]` with an implicit 32 KiB frame. 64 KiB window.
fn decompress_lzx(mut src: &[u8], size: usize) -> Result<Vec<u8>, Error> {
    let mut lzx = lzxd::Lzxd::new(lzxd::WindowSize::KB64);
    let mut out = Vec::with_capacity(size);
    while out.len() < size && src.len() >= 2 {
        let (frame, block, hdr) = if src[0] == 0xFF {
            if src.len() < 5 {
                return Err(Error::Eof);
            }
            let f = u16::from_be_bytes([src[1], src[2]]) as usize;
            let b = u16::from_be_bytes([src[3], src[4]]) as usize;
            (f, b, 5)
        } else {
            (0x8000, u16::from_be_bytes([src[0], src[1]]) as usize, 2)
        };
        if frame == 0 || block == 0 {
            break;
        }
        let chunk = src.get(hdr..hdr + block).ok_or(Error::Eof)?;
        let frame = frame.min(size - out.len());
        let dec = lzx.decompress_next(chunk, frame).map_err(|e| Error::Format(format!("lzx: {e}")))?;
        out.extend_from_slice(dec);
        src = &src[hdr + block..];
    }
    if out.len() != size {
        return bad(format!("lzx: got {} bytes, expected {size}", out.len()));
    }
    Ok(out)
}

/// Skips the type reader list; checks the primary object is read by `reader`.
fn primary(r: &mut Reader, reader: &str) -> Result<(), Error> {
    let readers = r.var_u32()?;
    let mut names = Vec::with_capacity(readers as usize);
    for _ in 0..readers {
        names.push(r.string()?);
        r.i32()?;
    }
    let _shared = r.var_u32()?;
    let id = r.var_u32()? as usize;
    let name = id.checked_sub(1).and_then(|i| names.get(i)).ok_or(Error::Format("bad reader id".into()))?;
    if !name.starts_with(reader) {
        return bad(format!("expected {reader}, found {name}"));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Sound {
    pub channels: u16,
    pub rate: u32,
    /// Interleaved samples.
    pub samples: Vec<i16>,
}

/// A SoundEffect: a WAVEFORMATEX then the sample data. Only 16-bit PCM, which is
/// what every file under `Content/Sounds` holds (1.4.5).
pub fn read_sound(data: &[u8]) -> Result<Sound, Error> {
    let (_, content) = content(data)?;
    let mut r = Reader::new(&content);
    primary(&mut r, SOUND_EFFECT_READER)?;
    let format_len = r.u32()? as usize;
    let mut f = Reader::new(r.bytes(format_len)?);
    let tag = f.i16()? as u16;
    let channels = f.i16()? as u16;
    let rate = f.u32()?;
    let _avg = f.u32()?;
    let _align = f.i16()?;
    let bits = f.i16()?;
    if tag != WAVE_FORMAT_PCM || bits != 16 || channels == 0 {
        return bad(format!("unsupported sound format {tag}, {bits} bits, {channels} channels"));
    }
    let len = r.u32()? as usize;
    let samples = r.bytes(len)?.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    Ok(Sound { channels, rate, samples })
}

pub fn read_texture(data: &[u8]) -> Result<Texture, Error> {
    let (_, content) = content(data)?;
    let mut r = Reader::new(&content);
    primary(&mut r, TEXTURE2D_READER)?;
    let format = r.i32()?;
    let width = r.u32()?;
    let height = r.u32()?;
    let _mips = r.u32()?;
    let len = r.u32()? as usize;
    if format != SURFACE_COLOR {
        // Every texture under Content/Images (1.4.5) is Color; DXT never occurs.
        return bad(format!("unsupported surface format {format}"));
    }
    if len != width as usize * height as usize * 4 {
        return bad("texture size mismatch");
    }
    Ok(Texture { width, height, rgba: r.bytes(len)?.to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_str(v: &mut Vec<u8>, s: &str) {
        v.push(s.len() as u8);
        v.extend_from_slice(s.as_bytes());
    }

    fn texture_content(w: u32, h: u32) -> Vec<u8> {
        let mut c = vec![1];
        push_str(&mut c, "Microsoft.Xna.Framework.Content.Texture2DReader, Microsoft.Xna.Framework.Graphics");
        c.extend_from_slice(&0i32.to_le_bytes());
        c.push(0);
        c.push(1);
        for v in [0, w, h, 1, w * h * 4] {
            c.extend_from_slice(&v.to_le_bytes());
        }
        c.extend((0..w * h * 4).map(|i| i as u8));
        c
    }

    fn wrap(flags: u8, content: &[u8], payload: &[u8]) -> Vec<u8> {
        let compressed = flags & (FLAG_LZ4 | FLAG_LZX) != 0;
        let hdr = if compressed { 14 } else { 10 };
        let mut v = b"XNBw\x05".to_vec();
        v.push(flags);
        v.extend_from_slice(&((hdr + payload.len()) as u32).to_le_bytes());
        if compressed {
            v.extend_from_slice(&(content.len() as u32).to_le_bytes());
        }
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn header_uncompressed() {
        let c = texture_content(2, 2);
        let f = wrap(FLAG_HIDEF, &c, &c);
        let h = Header::parse(&f).unwrap();
        assert_eq!((h.platform, h.version, h.flags), (b'w', 5, FLAG_HIDEF));
        assert_eq!(h.file_size as usize, f.len());
        assert_eq!(h.decompressed_size, None);
        assert!(Header::parse(b"XNA\x77\x05\x00\0\0\0\0").is_err());
    }

    #[test]
    fn texture_uncompressed() {
        let c = texture_content(3, 2);
        let t = read_texture(&wrap(0, &c, &c)).unwrap();
        assert_eq!((t.width, t.height, t.rgba.len()), (3, 2, 24));
        assert_eq!(t.rgba[5], 5);
    }

    #[test]
    fn sound_pcm() {
        let mut c = vec![1];
        push_str(&mut c, "Microsoft.Xna.Framework.Content.SoundEffectReader");
        c.extend_from_slice(&0i32.to_le_bytes());
        c.push(0);
        c.push(1);
        c.extend_from_slice(&18u32.to_le_bytes());
        for v in [1u16, 1] {
            c.extend_from_slice(&v.to_le_bytes());
        }
        for v in [22050u32, 44100] {
            c.extend_from_slice(&v.to_le_bytes());
        }
        for v in [2u16, 16, 0] {
            c.extend_from_slice(&v.to_le_bytes());
        }
        c.extend_from_slice(&4u32.to_le_bytes());
        c.extend_from_slice(&[1, 0, 0xff, 0xff]);
        let s = read_sound(&wrap(0, &c, &c)).unwrap();
        assert_eq!((s.channels, s.rate, s.samples), (1, 22050, vec![1, -1]));
        assert!(read_texture(&wrap(0, &c, &c)).is_err());
    }

    #[test]
    fn lz4_rejected() {
        let c = texture_content(16, 16);
        assert!(read_texture(&wrap(FLAG_LZ4, &c, &c)).is_err());
    }

    #[test]
    fn real_tile_sheet() {
        let Some(dir) = std::env::var_os(crate::install::ENV_VAR) else {
            eprintln!("TERRARIA_PATH not set, skipped");
            return;
        };
        let path = std::path::Path::new(&dir).join("Content/Images/Tiles_0.xnb");
        let data = std::fs::read(path).unwrap();
        let h = Header::parse(&data).unwrap();
        assert_eq!(h.version, 5);
        let t = read_texture(&data).unwrap();
        assert!(t.width >= 16 && t.height >= 16);
        assert_eq!(t.rgba.len(), (t.width * t.height * 4) as usize);
    }
}
