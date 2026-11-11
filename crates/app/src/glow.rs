//! Lights that move (`Lighting.AddLight`): held torches, dust, falling stars
//! and items on the ground.

use crate::magic::{Dust, Star};
use world::light::Glow;

/// Item of each torch style, as `BiomeTorchHoldStyle` maps them back.
pub const TORCH_ITEMS: [u16; 24] = [
    8, 427, 428, 429, 430, 431, 432, 433, 523, 974, 1245, 1333, 2274, 3004, 3045, 3114, 4383, 4384, 4385, 4386, 4387,
    4388, 5293, 5353,
];

/// `Lighting.AddLight` at a world point.
pub fn at(pos: [f32; 2], rgb: [f32; 3]) -> Option<Glow> {
    (pos[0] >= 0.0 && pos[1] >= 0.0).then(|| ((pos[0] / 16.0) as u32, (pos[1] / 16.0) as u32, rgb))
}

/// `Dust.UpdateDust` for the gem dust 86..92: brighter with its scale.
pub fn dust(d: &Dust) -> Option<Glow> {
    if !(86..=92).contains(&d.kind) {
        return None;
    }
    let n = (d.scale * 0.6).min(1.0);
    let k = match d.kind - 85 {
        1 => [0.9, 0.0, 0.9],
        2 => [0.9, 0.9, 0.0],
        3 => [0.0, 0.1, 1.3],
        4 => [0.0, 1.0, 0.1],
        5 => [1.0, 0.1, 0.1],
        6 => [1.3, 1.3, 1.3],
        _ => [1.0; 3],
    };
    at(d.pos, k.map(|c| n * n * c))
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
        let d = Dust { kind: 86, pos: [40.0, 70.0], vel: [0.0; 2], scale: 1.0, rot: 0.0, variant: 0 };
        let (x, y, rgb) = dust(&d).unwrap();
        assert_eq!((x, y), (2, 4));
        assert!((rgb[0] - 0.36 * 0.9).abs() < 1e-6 && rgb[1] == 0.0);
        let big = Dust { scale: 3.0, ..d };
        assert_eq!(dust(&big).unwrap().2, [0.9, 0.0, 0.9]);
        assert_eq!(dust(&Dust { kind: 6, ..d }), None);

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
