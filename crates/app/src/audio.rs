//! Sound effects and music. Effects (`Content/Sounds/*.xnb`) are decoded once,
//! resampled to the output rate and mixed in the device callback. Music tracks
//! come from the XACT banks, decoded on a thread when first asked for, and
//! cross-fade when the cue changes. Silent when there is no device or no game
//! install.

use std::collections::HashMap;
use std::io::Read;
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
    Roar,
    MenuTick,
    Shatter,
    Grass,
    Drink,
    Staff,
    GemBolt,
    MaxMana,
    Crystal,
    StarFall,
    StarHit,
    Bow,
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
            Sfx::Roar => ("Roar_0", 1),
            Sfx::MenuTick => ("Menu_Tick", 1),
            Sfx::Shatter => ("Shatter", 1),
            Sfx::Grass => ("Grass", 1),
            Sfx::Drink => ("Item_3", 1),
            Sfx::Staff => ("Item_43", 1),
            Sfx::GemBolt => ("Item_205", 1),
            Sfx::MaxMana => ("MaxMana", 1),
            Sfx::Crystal => ("Item_29", 1),
            Sfx::StarFall => ("Item_9", 1),
            Sfx::StarHit => ("Item_10", 1),
            Sfx::Bow => ("Item_5", 1),
        }
    }
}

type Clip = Arc<Vec<[f32; 2]>>;

struct Voice {
    clip: Clip,
    pos: usize,
    volume: f32,
}

/// A looping music track at its own rate, read with linear interpolation.
struct Track {
    samples: Arc<Vec<i16>>,
    channels: usize,
    /// Source frames per output frame.
    step: f64,
    pos: f64,
    gain: f32,
    target: f32,
}

#[derive(Default)]
struct Mixer {
    voices: Vec<Voice>,
    music: Vec<Track>,
}

/// Seconds for a full cross-fade between tracks.
const FADE: f32 = 2.0;
pub const MUSIC_VOLUME: f32 = 0.5;

/// The music banks: cue names and the wave bank entries.
struct Banks {
    path: PathBuf,
    cues: HashMap<String, u16>,
    waves: Vec<formats::xact::Wave>,
}

impl Banks {
    fn open(content: &std::path::Path) -> Option<Self> {
        let cues = formats::xact::sound_bank(&std::fs::read(content.join("Sound Bank.xsb")).ok()?).ok()?;
        let path = content.join("Wave Bank.xwb");
        let mut head = vec![0; 64 * 1024];
        std::fs::File::open(&path).ok()?.read_exact(&mut head).ok()?;
        let waves = formats::xact::wave_bank(&head).ok()?;
        Some(Self { path, cues, waves })
    }
}

pub struct Audio {
    dir: PathBuf,
    rate: u32,
    clips: HashMap<String, Option<Clip>>,
    mixer: Arc<Mutex<Mixer>>,
    banks: Option<Arc<Banks>>,
    cue: String,
    seed: u32,
    _stream: cpal::Stream,
}

impl Audio {
    /// `content`: `<Terraria>/Content`.
    pub fn new(content: PathBuf) -> Option<Self> {
        let device = cpal::default_host().default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let config = supported.config();
        let mixer = Arc::new(Mutex::new(Mixer::default()));
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => stream::<f32>(&device, &config, mixer.clone()),
            cpal::SampleFormat::I16 => stream::<i16>(&device, &config, mixer.clone()),
            cpal::SampleFormat::U16 => stream::<u16>(&device, &config, mixer.clone()),
            _ => None,
        }?;
        stream.play().ok()?;
        Some(Self {
            dir: content.join("Sounds"),
            rate: config.sample_rate,
            clips: HashMap::new(),
            mixer,
            banks: Banks::open(&content).map(Arc::new),
            cue: String::new(),
            seed: 0x2545_F491,
            _stream: stream,
        })
    }

    /// Switches the music to `cue` (`Music_3`...), fading out the current track.
    pub fn music(&mut self, cue: &str) {
        if self.cue == cue {
            return;
        }
        self.cue = cue.to_string();
        let Some(banks) = self.banks.clone() else { return };
        let Some(&wave) = banks.cues.get(cue).and_then(|&t| banks.waves.get(t as usize)) else { return };
        let (mixer, rate, cue) = (self.mixer.clone(), self.rate, cue.to_string());
        std::thread::spawn(move || {
            let samples = std::fs::File::open(&banks.path)
                .map_err(formats::Error::from)
                .and_then(|mut f| formats::xact::read_wave(&mut f, &wave));
            let samples = match samples {
                Ok(s) if !s.is_empty() => s,
                Ok(_) => return,
                Err(e) => return eprintln!("music {cue}: {e}"),
            };
            let mut m = mixer.lock().unwrap();
            for t in &mut m.music {
                t.target = 0.0;
            }
            m.music.push(Track {
                samples: Arc::new(samples),
                channels: wave.channels.max(1) as usize,
                step: wave.rate as f64 / rate as f64,
                pos: 0.0,
                gain: 0.0,
                target: MUSIC_VOLUME,
            });
        });
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
            let voices = &mut self.mixer.lock().unwrap().voices;
            if voices.len() < 32 {
                voices.push(Voice { clip, pos: 0, volume });
            }
        }
    }
}

fn stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mixer: Arc<Mutex<Mixer>>,
) -> Option<cpal::Stream> {
    let channels = config.channels as usize;
    let fade = MUSIC_VOLUME / (FADE * config.sample_rate as f32);
    device
        .build_output_stream(
            *config,
            move |out: &mut [T], _| {
                let m = &mut *mixer.lock().unwrap();
                for frame in out.chunks_mut(channels) {
                    let mut mix = [0.0f32; 2];
                    for v in m.voices.iter_mut() {
                        if let Some(s) = v.clip.get(v.pos) {
                            mix[0] += s[0] * v.volume;
                            mix[1] += s[1] * v.volume;
                            v.pos += 1;
                        }
                    }
                    for t in m.music.iter_mut() {
                        let s = t.next();
                        t.gain += (t.target - t.gain).clamp(-fade, fade);
                        mix[0] += s[0] * t.gain;
                        mix[1] += s[1] * t.gain;
                    }
                    for (c, o) in frame.iter_mut().enumerate() {
                        *o = T::from_sample(mix[c.min(1)].clamp(-1.0, 1.0));
                    }
                }
                m.voices.retain(|v| v.pos < v.clip.len());
                m.music.retain(|t| t.target > 0.0 || t.gain > 0.0);
            },
            |e| eprintln!("audio: {e}"),
            None,
        )
        .ok()
}

impl Track {
    /// The next output frame, looping at the end.
    fn next(&mut self) -> [f32; 2] {
        let frames = self.samples.len() / self.channels;
        let i = self.pos as usize;
        let f = self.pos.fract() as f32;
        let k = (i + 1) % frames;
        let at = |i: usize, c: usize| self.samples[i * self.channels + c.min(self.channels - 1)] as f32 / 32768.0;
        let out = [0, 1].map(|c| at(i, c) + (at(k, c) - at(i, c)) * f);
        self.pos += self.step;
        if self.pos >= frames as f64 {
            self.pos -= frames as f64;
        }
        out
    }
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
    fn music_loops_and_interpolates() {
        let mut t = Track {
            samples: Arc::new(vec![0, 0, 16384, -16384]),
            channels: 2,
            step: 0.5,
            pos: 0.0,
            gain: 1.0,
            target: 1.0,
        };
        assert_eq!(t.next(), [0.0, 0.0]);
        assert_eq!(t.next(), [0.25, -0.25]);
        assert_eq!(t.next(), [0.5, -0.5]);
        // Between the last frame and the first again.
        assert_eq!(t.next(), [0.25, -0.25]);
        assert_eq!(t.pos, 0.0);
    }

    #[test]
    fn resample_doubles_mono_to_stereo() {
        let s = formats::xnb::Sound { channels: 1, rate: 22050, samples: vec![0, 16384, 0] };
        let r = resample(&s, 44100);
        assert_eq!(r.len(), 6);
        assert_eq!(r[1], [0.25, 0.25]);
        assert_eq!(r[2], [0.5, 0.5]);
    }
}
