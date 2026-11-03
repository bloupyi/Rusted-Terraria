//! XACT 3 banks, as used for the game's music: the wave bank (`Wave Bank.xwb`)
//! holds MS-ADPCM tracks, the sound bank (`Sound Bank.xsb`) maps cue names
//! (`Music_1`...) to tracks. Only what that needs is read: simple cues whose
//! sound plays one track of the first wave bank.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

use crate::reader::{bad, Error, Reader};

const CODEC_PCM: u8 = 0;
const CODEC_ADPCM: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wave {
    pub codec: u8,
    pub channels: u16,
    pub rate: u32,
    /// Bytes per ADPCM block, all channels.
    pub block: u32,
    pub offset: u64,
    pub length: u32,
}

/// Entries of a wave bank, read from the head of the file.
pub fn wave_bank(data: &[u8]) -> Result<Vec<Wave>, Error> {
    let mut r = Reader::new(data);
    if r.bytes(4)? != b"WBND" {
        return bad("not a wave bank");
    }
    let _version = r.u32()?;
    let _header_version = r.u32()?;
    let mut seg = [(0u32, 0u32); 5];
    for s in &mut seg {
        *s = (r.u32()?, r.u32()?);
    }
    r.seek(seg[0].0 as usize)?;
    let flags = r.u32()?;
    let count = r.u32()?;
    r.bytes(64)?;
    let entry_size = r.u32()? as usize;
    if flags & 0x0002_0000 != 0 || entry_size < 24 {
        return bad("compact wave banks are not supported");
    }
    let data_start = seg[4].0 as u64;
    (0..count as usize)
        .map(|i| {
            r.seek(seg[1].0 as usize + i * entry_size)?;
            let _flags_duration = r.u32()?;
            let f = r.u32()?;
            let offset = r.u32()?;
            let length = r.u32()?;
            let codec = (f & 3) as u8;
            let channels = ((f >> 2) & 7) as u16;
            let rate = (f >> 5) & 0x3FFFF;
            let align = (f >> 23) & 0xFF;
            let block = if codec == CODEC_ADPCM { (align + 22) * channels as u32 } else { align };
            Ok(Wave { codec, channels, rate, block, offset: data_start + offset as u64, length })
        })
        .collect()
}

/// Cue name to track index, for simple cues playing a single track.
pub fn sound_bank(data: &[u8]) -> Result<HashMap<String, u16>, Error> {
    let mut r = Reader::new(data);
    if r.bytes(4)? != b"SDBK" {
        return bad("not a sound bank");
    }
    // Versions, crc, build time, platform.
    r.bytes(2 + 2 + 2 + 8 + 1)?;
    let simple = r.i16()? as u16;
    let _complex = r.i16()?;
    r.bytes(2)?;
    let _total = r.i16()?;
    r.bytes(1)?;
    let _sounds = r.i16()?;
    let names_len = r.i16()? as u16 as usize;
    r.bytes(2)?;
    let simple_at = r.u32()? as usize;
    let _complex_at = r.u32()?;
    let names_at = r.u32()? as usize;
    r.seek(names_at)?;
    let names: Vec<String> =
        r.bytes(names_len)?.split(|&b| b == 0).map(|s| String::from_utf8_lossy(s).into_owned()).collect();
    let mut cues = HashMap::new();
    for (i, name) in names.iter().take(simple as usize).enumerate() {
        r.seek(simple_at + i * 5 + 1)?;
        let sound = r.u32()? as usize;
        r.seek(sound)?;
        let flags = r.u8()?;
        if flags & 1 != 0 {
            continue;
        }
        r.bytes(2 + 1 + 2 + 1 + 2)?;
        let track = r.i16()? as u16;
        if r.u8()? == 0 {
            cues.insert(name.clone(), track);
        }
    }
    Ok(cues)
}

/// Decodes one track to interleaved 16-bit samples.
pub fn read_wave(file: &mut (impl Read + Seek), w: &Wave) -> Result<Vec<i16>, Error> {
    let mut data = vec![0; w.length as usize];
    file.seek(SeekFrom::Start(w.offset))?;
    file.read_exact(&mut data)?;
    match w.codec {
        CODEC_PCM => Ok(data.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()),
        CODEC_ADPCM => adpcm(&data, w.channels as usize, w.block as usize),
        c => bad(format!("unsupported wave codec {c}")),
    }
}

const COEF: [(i32, i32); 7] = [(256, 0), (512, -256), (0, 0), (192, 64), (240, 0), (460, -208), (392, -232)];
const ADAPT: [i32; 16] = [230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230];

/// Microsoft ADPCM: per block a predictor, step and two seed samples per
/// channel, then 4-bit codes with channels interleaved.
pub fn adpcm(data: &[u8], channels: usize, block: usize) -> Result<Vec<i16>, Error> {
    if channels == 0 || channels > 2 || block < 7 * channels {
        return bad("bad ADPCM layout");
    }
    let per_block = (block - 7 * channels) * 2 / channels + 2;
    let mut out = Vec::with_capacity(data.len() / block * per_block * channels);
    for b in data.chunks_exact(block) {
        let mut r = Reader::new(b);
        let mut pred = [(0, 0); 2];
        let (mut delta, mut s1, mut s2) = ([0i32; 2], [0i32; 2], [0i32; 2]);
        for p in pred.iter_mut().take(channels) {
            *p = *COEF.get(r.u8()? as usize).ok_or(Error::Format("bad ADPCM predictor".into()))?;
        }
        for d in delta.iter_mut().take(channels) {
            *d = r.i16()? as i32;
        }
        for s in s1.iter_mut().take(channels) {
            *s = r.i16()? as i32;
        }
        for s in s2.iter_mut().take(channels) {
            *s = r.i16()? as i32;
        }
        out.extend(s2[..channels].iter().map(|&v| v as i16));
        out.extend(s1[..channels].iter().map(|&v| v as i16));
        let mut c = 0;
        for &byte in &b[7 * channels..] {
            for code in [byte >> 4, byte & 15] {
                let signed = if code >= 8 { code as i32 - 16 } else { code as i32 };
                let p = (s1[c] * pred[c].0 + s2[c] * pred[c].1) >> 8;
                let v = (p + signed * delta[c]).clamp(i16::MIN as i32, i16::MAX as i32);
                (s2[c], s1[c]) = (s1[c], v);
                delta[c] = ((ADAPT[code as usize] * delta[c]) >> 8).max(16);
                out.push(v as i16);
                c = (c + 1) % channels;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adpcm_mono_block() {
        // Predictor 0 (s1 only), delta 16, seeds 100 then 200, codes +1 and -1.
        let mut b = vec![0u8];
        b.extend_from_slice(&16i16.to_le_bytes());
        b.extend_from_slice(&200i16.to_le_bytes());
        b.extend_from_slice(&100i16.to_le_bytes());
        b.push(0x1F);
        let s = adpcm(&b, 1, b.len()).unwrap();
        assert_eq!(s.len(), 4);
        assert_eq!(&s[..2], &[100, 200]);
        // 200 + 1 * 16, then delta stays at 16 (230 * 16 >> 8 = 14, floored to 16).
        assert_eq!(s[2], 216);
        assert_eq!(s[3], 200);
    }

    #[test]
    fn real_music_banks() {
        let Some(dir) = std::env::var_os(crate::install::ENV_VAR) else {
            eprintln!("TERRARIA_PATH not set, skipped");
            return;
        };
        let content = std::path::Path::new(&dir).join("Content");
        let cues = sound_bank(&std::fs::read(content.join("Sound Bank.xsb")).unwrap()).unwrap();
        assert_eq!(cues.get("Music_1"), Some(&2));
        let mut f = std::fs::File::open(content.join("Wave Bank.xwb")).unwrap();
        let mut head = vec![0; 64 * 1024];
        f.read_exact(&mut head).unwrap();
        let waves = wave_bank(&head).unwrap();
        assert!(waves.len() >= cues.len());
        let w = waves[cues["Music_1"] as usize];
        assert_eq!((w.codec, w.channels), (CODEC_ADPCM, 2));
        let samples = read_wave(&mut f, &w).unwrap();
        assert!(samples.len() > w.rate as usize * 2 * 30, "at least 30 s");
        assert!(samples.iter().any(|&s| s.unsigned_abs() > 1000));
    }
}
