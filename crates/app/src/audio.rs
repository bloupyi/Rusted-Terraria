//! Sound effects: `Content/Sounds/*.xnb` decoded once, resampled to the output
//! rate and mixed in the device callback. Silent when there is no device or no
//! game install.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

/// What the game asks to hear; each maps to a family of sound files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sfx {
    Dig,
    Tink,
    Grab,
    Splash,
    Swing,
    PlayerHit,
    PlayerKilled,
    MobHit,
    MobKilled,
    Zombie,
}

impl Sfx {
    /// File stem and number of variants (`<stem>_<n>`, one picked at random).
    fn files(self) -> (&'static str, u32) {
        match self {
            Sfx::Dig => ("Dig", 3),
            Sfx::Tink => ("Tink", 3),
            Sfx::Grab => ("Grab", 1),
            Sfx::Splash => ("Splash", 2),
            Sfx::Swing => ("Item_1", 1),
            Sfx::PlayerHit => ("Player_Hit", 3),
            Sfx::PlayerKilled => ("Player_Killed", 1),
            Sfx::MobHit => ("NPC_Hit_1", 1),
            Sfx::MobKilled => ("NPC_Killed_1", 1),
            Sfx::Zombie => ("Zombie", 3),
        }
    }
}

type Clip = Arc<Vec<[f32; 2]>>;

struct Voice {
    clip: Clip,
    pos: usize,
    volume: f32,
}

pub struct Audio {
    dir: PathBuf,
    rate: u32,
    clips: HashMap<String, Option<Clip>>,
    voices: Arc<Mutex<Vec<Voice>>>,
    seed: u32,
    _stream: cpal::Stream,
}

impl Audio {
    /// `sounds`: `<Terraria>/Content/Sounds`.
    pub fn new(sounds: PathBuf) -> Option<Self> {
        let device = cpal::default_host().default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let config = supported.config();
        let voices = Arc::new(Mutex::new(Vec::new()));
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => stream::<f32>(&device, &config, voices.clone()),
            cpal::SampleFormat::I16 => stream::<i16>(&device, &config, voices.clone()),
            cpal::SampleFormat::U16 => stream::<u16>(&device, &config, voices.clone()),
            _ => None,
        }?;
        stream.play().ok()?;
        Some(Self {
            dir: sounds,
            rate: config.sample_rate,
            clips: HashMap::new(),
            voices,
            seed: 0x2545_F491,
            _stream: stream,
        })
    }

    pub fn play(&mut self, sfx: Sfx, volume: f32) {
        let (stem, variants) = sfx.files();
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        let name = if variants == 1 { stem.to_string() } else { format!("{stem}_{}", self.seed % variants) };
        let (dir, rate) = (&self.dir, self.rate);
        let clip = self.clips.entry(name).or_insert_with_key(|name| {
            let data = std::fs::read(dir.join(format!("{name}.xnb"))).ok()?;
            let sound = formats::xnb::read_sound(&data).ok()?;
            Some(Arc::new(resample(&sound, rate)))
        });
        if let Some(clip) = clip.clone() {
            let mut voices = self.voices.lock().unwrap();
            if voices.len() < 32 {
                voices.push(Voice { clip, pos: 0, volume });
            }
        }
    }
}

fn stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    voices: Arc<Mutex<Vec<Voice>>>,
) -> Option<cpal::Stream> {
    let channels = config.channels as usize;
    device
        .build_output_stream(
            *config,
            move |out: &mut [T], _| {
                let mut voices = voices.lock().unwrap();
                for frame in out.chunks_mut(channels) {
                    let mut mix = [0.0f32; 2];
                    for v in voices.iter_mut() {
                        if let Some(s) = v.clip.get(v.pos) {
                            mix[0] += s[0] * v.volume;
                            mix[1] += s[1] * v.volume;
                            v.pos += 1;
                        }
                    }
                    for (c, o) in frame.iter_mut().enumerate() {
                        *o = T::from_sample(mix[c.min(1)].clamp(-1.0, 1.0));
                    }
                }
                voices.retain(|v| v.pos < v.clip.len());
            },
            |e| eprintln!("audio: {e}"),
            None,
        )
        .ok()
}

/// To stereo f32 at `rate`, by linear interpolation.
fn resample(sound: &formats::xnb::Sound, rate: u32) -> Vec<[f32; 2]> {
    let ch = sound.channels as usize;
    let frames = sound.samples.len() / ch;
    let at = |i: usize, c: usize| sound.samples[i * ch + c.min(ch - 1)] as f32 / 32768.0;
    if frames == 0 {
        return Vec::new();
    }
    let n = (frames as u64 * rate as u64 / sound.rate as u64) as usize;
    (0..n)
        .map(|j| {
            let t = j as f64 * sound.rate as f64 / rate as f64;
            let (i, f) = (t as usize, (t.fract()) as f32);
            let k = (i + 1).min(frames - 1);
            [0, 1].map(|c| at(i, c) + (at(k, c) - at(i, c)) * f)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_doubles_mono_to_stereo() {
        let s = formats::xnb::Sound { channels: 1, rate: 22050, samples: vec![0, 16384, 0] };
        let r = resample(&s, 44100);
        assert_eq!(r.len(), 6);
        assert_eq!(r[1], [0.25, 0.25]);
        assert_eq!(r[2], [0.5, 0.5]);
    }
}
