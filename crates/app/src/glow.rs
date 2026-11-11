//! Lights that move (`Lighting.AddLight`): held torches, dust, falling stars
//! and items on the ground.

use crate::magic::{Bolt, Dust, Star};
use world::light::Glow;

/// Item of each torch style, as `BiomeTorchHoldStyle` maps them back.
pub const TORCH_ITEMS: [u16; 24] = [
    8, 427, 428, 429, 430, 431, 432, 433, 523, 974, 1245, 1333, 2274, 3004, 3045, 3114, 4383, 4384, 4385, 4386, 4387,
    4388, 5293, 5353,
];

/// `TorchID.Dust`: the flame of each torch style.
pub const TORCH_DUST: [u16; 24] =
    [6, 59, 60, 61, 62, 63, 64, 65, 75, 135, 158, 169, 156, 234, 66, 242, 293, 294, 295, 296, 297, 298, 307, 310];

/// `Lighting.AddLight` at a world point.
pub fn at(pos: [f32; 2], rgb: [f32; 3]) -> Option<Glow> {
    (pos[0] >= 0.0 && pos[1] >= 0.0).then(|| ((pos[0] / 16.0) as u32, (pos[1] / 16.0) as u32, rgb))
}

/// Colour of gem dust 86..92, as both the dust and the bolts light with it.
fn gem(kind: u16) -> [f32; 3] {
    match kind.wrapping_sub(85) {
        1 => [0.9, 0.0, 0.9],
        2 => [0.9, 0.9, 0.0],
        3 => [0.0, 0.1, 1.3],
        4 => [0.0, 1.0, 0.1],
        5 => [1.0, 0.1, 0.1],
        6 => [1.3, 1.3, 1.3],
        _ => [1.0; 3],
    }
}

/// `Dust.UpdateDust` for the gem dust 86..92, brighter with its scale, and for
/// torch flames: dust 6 and the coloured torches' 59..65.
pub fn dust(d: &Dust) -> Option<Glow> {
    let n = d.scale * 1.4;
    match d.kind {
        86..=92 => {
            let n = (d.scale * 0.6).min(1.0);
            at(d.pos, gem(d.kind).map(|c| n * n * c))
        }
        6 => {
            let n = n.min(0.6);
            at(d.pos, [n, n * 0.65, n * 0.4])
        }
        59..=65 => {
            let n = n.min(0.8);
            let c = world::light::emission(4, 0, 22 * (1 + d.kind as i16 - 59))?;
            at(d.pos, c.map(|c| c * n))
        }
        _ => None,
    }
}

/// End of `AI_029_GemStaffs_ActuallyGemStaffs`: the bolt in its gem's colour.
pub fn bolt(b: &Bolt) -> Option<Glow> {
    at(b.center(), gem(b.dust()))
}

/// `ProjLight` for projectile 12, once out of its spawner: `light` 1.
pub fn star(s: &Star) -> Option<Glow> {
    if s.wait > 0 {
        return None;
    }
    at(s.center(), [0.9, 0.8, 0.1])
}

/// `Main.essScale`: 1 down to 0.7 and back, 0.01 a tick.
#[derive(Clone, Copy, Debug)]
pub struct Ess {
    pub scale: f32,
    dir: f32,
}

impl Default for Ess {
    fn default() -> Self {
        Self { scale: 1.0, dir: -1.0 }
    }
}

impl Ess {
    pub fn tick(&mut self) {
        self.scale += self.dir * 0.01;
        if self.scale > 1.0 {
            (self.dir, self.scale) = (-1.0, 1.0);
        }
        if self.scale < 0.7 {
            (self.dir, self.scale) = (1.0, 0.7);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amethyst_dust_and_stars_glow_with_the_game_colours() {
        let d = Dust { kind: 86, gravity: false, pos: [40.0, 70.0], vel: [0.0; 2], scale: 1.0, rot: 0.0, variant: 0 };
        let (x, y, rgb) = dust(&d).unwrap();
        assert_eq!((x, y), (2, 4));
        assert!((rgb[0] - 0.36 * 0.9).abs() < 1e-6 && rgb[1] == 0.0);
        let big = Dust { scale: 3.0, ..d };
        assert_eq!(dust(&big).unwrap().2, [0.9, 0.0, 0.9]);
        assert_eq!(dust(&Dust { kind: 5, ..d }), None);
        assert_eq!(dust(&Dust { kind: 6, ..d }).unwrap().2, [0.6, 0.6 * 0.65, 0.6 * 0.4]);
        let b = Bolt::new(124, [8.0, 8.0], [1.0, 0.0], 9, 1.0, None);
        assert_eq!(bolt(&b), Some((0, 0, [0.0, 1.0, 0.1])));

        let mut s = Star::new([100.0, 100.0], [0.0, 10.0]);
        assert_eq!(star(&s), None, "still a spawner");
        s.wait = 0;
        assert_eq!(star(&s), Some((6, 6, [0.9, 0.8, 0.1])));
    }

    #[test]
    fn essence_pulses_between_0_7_and_1() {
        let mut e = Ess::default();
        let seen: Vec<f32> = (0..120)
            .map(|_| {
                e.tick();
                e.scale
            })
            .collect();
        let (lo, hi) = seen.iter().fold((2.0f32, 0.0f32), |(a, b), &v| (a.min(v), b.max(v)));
        assert!((lo - 0.7).abs() < 1e-4 && (hi - 1.0).abs() < 1e-4, "{lo} {hi}");
    }
}
