//! Damage numbers as the game's `CombatText` moves and fades them.

pub const DAMAGED_FRIENDLY: [u8; 4] = [255, 80, 90, 255];
pub const DAMAGED_FRIENDLY_CRIT: [u8; 4] = [255, 100, 30, 255];
pub const DAMAGED_HOSTILE: [u8; 4] = [255, 160, 80, 255];
pub const DAMAGED_HOSTILE_CRIT: [u8; 4] = [255, 100, 30, 255];
pub const HEAL_LIFE: [u8; 4] = [100, 255, 100, 255];
pub const HEAL_MANA: [u8; 4] = [100, 100, 255, 255];
/// `OthersDamagedHostile`: what another player's hits show, at 40%.
pub fn others(c: [u8; 4]) -> [u8; 4] {
    c.map(|v| (v as f32 * 0.4) as u8)
}

const MAX: usize = 100;

#[derive(Clone, Debug)]
pub struct CombatText {
    /// Middle of the text, in world pixels. The game keeps the top-left, which
    /// is this less half the measured text both when made and when drawn.
    pub center: [f32; 2],
    velocity: [f32; 2],
    pub alpha: f32,
    alpha_up: bool,
    pub text: String,
    pub scale: f32,
    pub rotation: f32,
    pub color: [u8; 4],
    life: i32,
    pub crit: bool,
}

/// `NewText` over `rect` (`x, y, w, h`); `jitter` gives `Main.rand.Next(lo, hi + 1)`.
pub fn spawn(
    texts: &mut Vec<CombatText>,
    rect: [f32; 4],
    color: [u8; 4],
    amount: i32,
    crit: bool,
    mut jitter: impl FnMut(i32, i32) -> i32,
) {
    if texts.len() >= MAX {
        return;
    }
    let [x, y, w, h] = rect.map(|v| v as i32 as f32);
    let jx = jitter(-((w * 0.5) as i32), (w * 0.5) as i32) as f32;
    let jy = jitter(-((h * 0.5) as i32), (h * 0.5) as i32) as f32;
    let mut t = CombatText {
        center: [x + w * 0.5 + jx, y + h * 0.25 + jy],
        velocity: [0.0, -7.0],
        alpha: 1.0,
        alpha_up: false,
        text: amount.to_string(),
        scale: 0.0,
        rotation: 0.0,
        color,
        life: 60,
        crit,
    };
    if crit {
        t.life *= 2;
        t.velocity[1] *= 2.0;
        t.velocity[0] = jitter(-25, 25) as f32 * 0.05;
        t.rotation = (t.life / 2) as f32 * 0.002;
        if t.velocity[0] < 0.0 {
            t.rotation = -t.rotation;
        }
    }
    texts.push(t);
}

/// `UpdateCombatText`: one tick of every text; gone ones are dropped.
pub fn update(texts: &mut Vec<CombatText>) {
    texts.retain_mut(|t| {
        t.alpha += if t.alpha_up { 0.05 } else { -0.05 };
        if t.alpha <= 0.6 {
            t.alpha_up = true;
        }
        if t.alpha >= 1.0 {
            (t.alpha, t.alpha_up) = (1.0, false);
        }
        t.velocity[1] *= 0.92;
        if t.crit {
            t.velocity[1] *= 0.92;
        }
        t.velocity[0] *= 0.93;
        t.center = [t.center[0] + t.velocity[0], t.center[1] + t.velocity[1]];
        t.life -= 1;
        if t.life <= 0 {
            t.scale -= 0.1;
            if t.scale < 0.1 {
                return false;
            }
            t.life = 0;
            if t.crit {
                t.alpha_up = false;
                t.scale += 0.07;
            }
            return true;
        }
        if t.crit {
            t.rotation += if t.velocity[0] < 0.0 { 0.001 } else { -0.001 };
        }
        t.scale = (t.scale + 0.1).min(1.0);
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rises_slows_and_fades_out() {
        let mut texts = Vec::new();
        spawn(&mut texts, [100.0, 200.0, 20.0, 40.0], DAMAGED_HOSTILE, 12, false, |_, _| 0);
        assert_eq!(texts[0].center, [110.0, 210.0]);
        update(&mut texts);
        assert_eq!(texts[0].scale, 0.1);
        let y = texts[0].center[1];
        for _ in 0..58 {
            update(&mut texts);
        }
        assert!((texts[0].scale - 1.0).abs() < 1e-6, "full size for its whole life");
        assert!(y - texts[0].center[1] > 70.0 && y - texts[0].center[1] < 90.0, "about 87 px up in all");
        let mut ticks = 0;
        while !texts.is_empty() {
            update(&mut texts);
            ticks += 1;
        }
        assert!((9..=11).contains(&ticks), "shrinks away in about ten ticks: {ticks}");
    }

    #[test]
    fn crits_last_twice_as_long_and_tilt() {
        let mut texts = Vec::new();
        spawn(&mut texts, [0.0; 4], DAMAGED_HOSTILE_CRIT, 30, true, |lo, _| lo);
        assert!(texts[0].rotation < 0.0, "leans the way it drifts");
        let mut ticks = 0;
        while !texts.is_empty() {
            update(&mut texts);
            ticks += 1;
        }
        assert!(ticks > 120, "{ticks}");
    }
}
