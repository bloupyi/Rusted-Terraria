//! Wind and rain: `Main.UpdateWeather`, the rain part of `UpdateTime`,
//! `StartRain`, `StopRain`, `ChangeRain` and the drops of `Rain`.

use formats::wld::Env;
use world::World;

/// `Main.maxRain`.
pub const MAX_RAIN: usize = 750;
pub const SHEET: &str = "Rain";
/// `Main.MaxWorldViewSize.X`.
const MAX_VIEW_W: f32 = 1920.0;
const DAY: i32 = 86400;
/// `Dust.dustWater()` for the forest's water.
pub const SPLASH_DUST: u16 = 33;

/// `Main.rand.Next(n)`: 0 to `n - 1`.
pub type Roll<'a> = dyn FnMut(i32) -> i32 + 'a;

fn next(r: &mut Roll, lo: i32, hi: i32) -> i32 {
    lo + r((hi - lo).max(1))
}

pub struct Drop {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub scale: f32,
    pub rot: f32,
    /// Column on the `Rain` sheet: `waterStyle * 3` and one of three.
    pub kind: u8,
}

#[derive(Default)]
pub struct Weather {
    /// `windSpeedTarget` and `windSpeedCurrent`; above 0 blows to the right.
    pub wind_target: f32,
    pub wind: f32,
    wind_counter: i32,
    extreme_counter: i32,
    pub raining: bool,
    pub rain_time: i32,
    pub max_raining: f32,
    /// How far the rain has set in, 0 to `max_raining`: darkens the sky and thickens the drops.
    pub cloud_alpha: f32,
    pub clouds: i32,
    clouds_temp: i32,
    weather_counter: i32,
    pub drops: Vec<Drop>,
    /// `_shouldUseStormMusic` and `_shouldUseWindyDayMusic`.
    pub storm: bool,
    pub windy_day: bool,
    /// How white the sky flashes, 0 to 1, and the flash under way.
    pub lightning: f32,
    lightning_speed: f32,
    lightning_decay: f32,
    /// Ticks until the thunder of the last flash is heard.
    thunder_delay: i32,
}

impl Weather {
    /// As `WorldFile` loads it: the wind settled on its target, rain as saved.
    pub fn new(env: &Env) -> Self {
        Self {
            wind_target: env.wind,
            wind: env.wind,
            raining: env.raining,
            rain_time: env.rain_time,
            max_raining: env.max_raining,
            cloud_alpha: if env.raining { env.max_raining } else { 0.0 },
            clouds: env.clouds as i32,
            clouds_temp: env.clouds as i32,
            ..Default::default()
        }
    }

    pub fn stop_rain(&mut self) {
        self.rain_time = 0;
        self.raining = false;
        self.max_raining = 0.0;
    }

    pub fn start_rain(&mut self, r: &mut Roll) {
        let hour = DAY / 24;
        let mut time = next(r, hour * 8, DAY);
        for (chance, hours) in [(3, 1), (4, 2), (5, 2), (6, 3), (7, 4), (8, 5)] {
            if r(chance) == 0 {
                time += next(r, 0, hour * hours);
            }
        }
        let mut k = 1.0f32;
        for (chance, add) in [(2, 0.05), (3, 0.1), (4, 0.15), (5, 0.2)] {
            if r(chance) == 0 {
                k += add;
            }
        }
        self.rain_time = (time as f32 * k) as i32;
        self.change_rain(r);
        self.raining = true;
    }

    /// A new strength, heavier the more clouds there are.
    pub fn change_rain(&mut self, r: &mut Roll) {
        let (wide, narrow) = if self.clouds > 150 {
            ((20, 91), (40, 91))
        } else if self.clouds > 100 {
            ((10, 71), (20, 61))
        } else {
            ((5, 41), (5, 31))
        };
        let (lo, hi) = if r(3) == 0 { wide } else { narrow };
        self.max_raining = next(r, lo, hi) as f32 * 0.01;
    }

    /// `UpdateTime`'s rain at `day_rate` ticks of game time a tick. `strong`:
    /// a player has at least 120 life, without which it never starts to rain.
    pub fn update_rain(&mut self, day_rate: i32, strong: bool, r: &mut Roll) {
        if self.raining {
            self.rain_time -= day_rate;
            if day_rate > 0 {
                let hour = DAY / day_rate / 24;
                if self.rain_time <= 0 {
                    self.stop_rain();
                } else if r(hour * 2) == 0 {
                    self.change_rain(r);
                }
            }
        } else if strong && day_rate != 0 && r((DAY as f64 / day_rate as f64 * 5.75) as i32) == 0 {
            self.start_rain(r);
        }
        let step = 0.0005 * day_rate.max(1) as f32;
        if self.raining {
            if self.cloud_alpha > self.max_raining {
                self.cloud_alpha = (self.cloud_alpha - step).max(self.max_raining);
            } else if self.cloud_alpha < self.max_raining {
                self.cloud_alpha = (self.cloud_alpha + step).min(self.max_raining);
            }
        } else {
            self.cloud_alpha = (self.cloud_alpha - step).max(0.0);
        }
    }

    /// `UpdateWindyDayState`: storm and windy day, each with a margin so they
    /// do not flicker on and off.
    pub fn update_state(&mut self, day: bool, time: f64) {
        const MIN_WIND: f32 = 0.34;
        const MAX_WIND: f32 = 0.4;
        const MIN_RAIN: f32 = 0.4;
        const MAX_RAIN: f32 = 0.5;
        let wind = self.wind_target.abs();
        if self.cloud_alpha == 0.0 {
            self.storm = false;
            if !(10800.0..=43200.0).contains(&time) || !day {
                self.windy_day = false;
                return;
            }
            if wind < MIN_WIND {
                self.windy_day = false;
            }
            if wind >= MAX_WIND {
                self.windy_day = true;
            }
        } else {
            if self.cloud_alpha < MIN_RAIN || wind < MIN_WIND {
                self.storm = false;
            } else if self.cloud_alpha >= MAX_RAIN && wind >= MAX_WIND {
                self.storm = true;
            }
            self.windy_day = false;
        }
    }

    /// `NewLightning`: a flash now, its thunder a little later.
    pub fn new_lightning(&mut self, r: &mut Roll) {
        self.thunder_delay = if r(3) == 0 {
            next(r, 1, 31)
        } else if r(2) == 0 {
            next(r, 31, 121)
        } else {
            next(r, 11, 61)
        };
        self.lightning_decay = r(1_000_000) as f32 / 1e6 * 0.05 + 0.008;
        self.lightning_speed = r(1_000_000) as f32 / 1e6 * 0.05 + 0.05;
    }

    /// `UpdateWeather`'s storm: the flash grows and fades, and over a view
    /// above the caverns a storm strikes now and then, more often the
    /// harder it rains and blows. True when thunder is heard this tick.
    pub fn update_lightning(&mut self, open_sky: bool, r: &mut Roll) -> bool {
        let mut thunder = false;
        if self.thunder_delay > 0 {
            self.thunder_delay -= 1;
            thunder = self.thunder_delay == 0;
        }
        if self.lightning_speed > 0.0 {
            self.lightning += self.lightning_speed;
            if self.lightning >= 1.0 {
                self.lightning = 1.0;
                self.lightning_speed = 0.0;
            }
        } else if self.lightning > 0.0 {
            self.lightning = (self.lightning - self.lightning_decay).max(0.0);
        } else if self.thunder_delay <= 0 && open_sky && self.storm {
            let (mut lo, mut hi) = (600.0f32, 1600.0f32);
            for (k, a, b) in [(0.8, 0.6, 0.8), (0.7, 0.7, 0.9), (0.6, 0.8, 0.95)] {
                if self.max_raining > k {
                    lo *= a;
                    hi *= b;
                }
            }
            for (k, a, b) in [(0.7, 0.6, 0.8), (0.6, 0.7, 0.9), (0.5, 0.8, 0.95)] {
                if self.wind_target.abs() > k {
                    lo *= a;
                    hi *= b;
                }
            }
            let mut n = next(r, lo as i32, hi as i32) as f32;
            n *= (1.0 - self.max_raining + 1.0) / 2.0;
            n *= (1.0 - self.wind_target + 1.0) / 2.0;
            if r(n as i32) == 0 {
                self.new_lightning(r);
            }
        }
        thunder
    }

    /// `UpdateWeather`: the wind eases towards a target that drifts and
    /// sometimes gusts, and the clouds come and go.
    pub fn update_wind(&mut self, strong: bool, r: &mut Roll) {
        const MAX: f32 = 0.8;
        let target = self.wind_target * (1.0 + 5.0 / 9.0 * self.max_raining);
        let step = 0.0003 + (target - self.wind).abs() * 0.0015;
        if self.wind < target {
            self.wind = (self.wind + step).min(target);
        } else if self.wind > target {
            self.wind = (self.wind - step).max(target);
        }
        self.wind_counter -= 1;
        if self.wind_counter <= 0 {
            let sign = if self.wind_target < 0.0 { -1.0 } else { 1.0 };
            let drift = if r(4) == 0 {
                25
            } else if r(2) == 0 {
                50
            } else {
                100
            };
            self.wind_target += next(r, -drift, drift + 1) as f32 * 0.001;
            let cap = |w: f32| if !strong && w.abs() > 0.35 { 0.35 * w.signum() } else { w };
            self.wind_target = cap(self.wind_target);
            self.extreme_counter -= 1;
            // `ResetWindCounter` draws from its own generator.
            self.wind_counter = next(r, 900, 2701);
            if self.extreme_counter <= 0 {
                self.extreme_counter = next(r, 10, 31);
                if r(30) < 13 {
                    if r(2) == 0 {
                        self.wind_target = 0.0;
                        self.wind_counter = next(r, 7200, 28801);
                    } else {
                        self.wind_target = next(r, -200, 201) as f32 * 0.001;
                    }
                } else if r(20) < 13 {
                    self.wind_target = next(r, -400, 401) as f32 * 0.001;
                } else {
                    self.wind_target = next(r, -850, 851) as f32 * 0.001;
                }
                self.wind_target = cap(self.wind_target);
                let w = self.wind_target.abs();
                if w > 0.3 {
                    self.extreme_counter += next(r, 5, 11);
                }
                if w > 0.5 {
                    self.extreme_counter += next(r, 10, 21);
                }
                if w > 0.7 {
                    self.extreme_counter += next(r, 15, 31);
                }
            }
            if r(3) != 0 && ((sign < 0.0 && self.wind_target > 0.0) || (sign > 0.0 && self.wind_target < 0.0)) {
                self.wind_target *= -1.0;
            }
        }
        self.wind_target = self.wind_target.clamp(-MAX, MAX);
        self.update_clouds(r);
    }

    /// `numClouds`, which sets how heavy the next rain is.
    fn update_clouds(&mut self, r: &mut Roll) {
        if r(60) == 0 {
            self.clouds_temp += next(r, -1, 2);
        }
        if (r(1300) as f32) < 25.0 {
            self.clouds_temp -= 1;
        }
        if (r(1000) as f32) < 200.0 * self.cloud_alpha && self.clouds_temp < 100 {
            self.clouds_temp += 1;
        }
        if (r(1000) as f32) < 50.0 * self.cloud_alpha {
            self.clouds_temp += 1;
        }
        if self.clouds_temp > 66 && r(100) == 0 {
            self.clouds_temp -= next(r, 1, 3);
        }
        if self.clouds_temp < 50 && r(100) == 0 {
            self.clouds_temp += next(r, 1, 3);
        }
        if self.clouds_temp > 100 && self.cloud_alpha == 0.0 {
            self.clouds_temp = 100;
        }
        self.clouds_temp = self.clouds_temp.max(-20);
        if self.cloud_alpha > 0.0 {
            while (self.clouds as f32) < 200.0 * self.cloud_alpha {
                self.clouds = (self.clouds + r(30)).min(200);
                self.clouds_temp = self.clouds_temp.max(self.clouds);
            }
        }
        self.weather_counter -= 1;
        if self.weather_counter > 0 {
            return;
        }
        if r(2) == 0 {
            self.clouds_temp += if r(2) == 0 { r(250) } else { r(100) };
        }
        self.clouds_temp = self.clouds_temp.clamp(0, 200);
        self.clouds = self.clouds_temp;
        self.weather_counter = next(r, 3600, 10800);
    }

    /// `Rain.MakeRain` over the view `[x, y, w, h]` in world pixels: new drops
    /// just above it, upwind, where open sky meets no wall. Only while it rains
    /// on a view at or above the surface (`SceneMetrics.ZoneRain`).
    pub fn make_rain(
        &mut self,
        world: &World,
        view: [f32; 4],
        surface: f64,
        player_vel: [f32; 2],
        water_style: u8,
        r: &mut Roll,
    ) {
        let [vx, vy, vw, vh] = view;
        if self.cloud_alpha <= 0.0 || !self.raining || ((vy + vh / 2.0) / 16.0) as f64 > surface {
            return;
        }
        let n = vw / MAX_VIEW_W * 25.0 * (0.25 + self.cloud_alpha);
        let cap = ((MAX_RAIN as f32 * self.cloud_alpha) as usize).min(MAX_RAIN);
        let max_x = (world.width as f32 - 1.0) * 16.0;
        let mut i = 0;
        while (i as f32) < n {
            i += 1;
            let mut margin = 600;
            if player_vel[1] < 0.0 {
                margin += (player_vel[1].abs() * 30.0) as i32;
            }
            let mut x = next(r, vx as i32 - margin, (vx + vw) as i32 + margin) as f32;
            let y = vy - next(r, 20, 100) as f32;
            x = (x - self.wind * 15.0 * 40.0 + player_vel[0] * 40.0).clamp(0.0, max_x);
            let (tx, ty) = ((x as i64 / 16), (y as i64 / 16).clamp(0, world.height as i64 - 1));
            if world.solid(tx, ty) || world.wall(tx, ty) > 0 || self.drops.len() >= cap {
                continue;
            }
            let scale = 1.0 + next(r, -20, 21) as f32 * 0.01;
            let vel = [self.wind * 18.0 * scale, 14.0 * scale];
            let kind = water_style * 3 + r(3) as u8;
            self.drops.push(Drop { pos: [x, y], vel, scale, rot: vel[0].atan2(-vel[1]), kind });
        }
    }

    /// `Rain.Update`: drops fall until they hit a block or water, or pass
    /// 100 px under the view. Gives where each hit lands, for a splash.
    pub fn update_drops(&mut self, world: &World, view_bottom: f32) -> Vec<[f32; 2]> {
        let mut splashes = Vec::new();
        self.drops.retain_mut(|d| {
            d.pos = [d.pos[0] + d.vel[0], d.pos[1] + d.vel[1]];
            let (x0, y0) = ((d.pos[0] / 16.0).floor() as i64, (d.pos[1] / 16.0).floor() as i64);
            let (x1, y1) = (((d.pos[0] + 2.0) / 16.0).floor() as i64, ((d.pos[1] + 2.0) / 16.0).floor() as i64);
            let cells = || (x0..=x1).flat_map(move |x| (y0..=y1).map(move |y| (x, y)));
            let hit = cells().any(|(x, y)| world.solid(x, y))
                || cells().any(|(x, y)| world.in_bounds(x, y) && world.liquid(x, y) > 0);
            if hit || d.pos[1] > view_bottom + 100.0 {
                splashes.push([d.pos[0] - d.vel[0], d.pos[1] - d.vel[1]]);
                return false;
            }
            true
        });
        splashes
    }
}

/// The end of `SetBackColor`: lightning whitens the sky colour.
pub fn flash(c: [f32; 3], lightning: f32) -> [f32; 3] {
    if lightning <= 0.0 {
        return c;
    }
    c.map(|v| ((v / 255.0 + (1.0 - v / 255.0) * lightning) * 255.0) as u8 as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng(mut s: u64) -> impl FnMut(i32) -> i32 {
        move |n| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s % n.max(1) as u64) as i32
        }
    }

    #[test]
    fn rain_lasts_hours_and_sets_in_slowly() {
        let mut r = rng(7);
        let mut w = Weather::default();
        w.start_rain(&mut r);
        assert!(w.raining && w.rain_time >= DAY / 3 && (0.05..0.91).contains(&w.max_raining));
        w.update_rain(1, true, &mut r);
        assert!((w.cloud_alpha - 0.0005).abs() < 1e-6);
        for _ in 0..4000 {
            w.update_rain(1, true, &mut r);
        }
        assert_eq!(w.cloud_alpha, w.max_raining);
        w.rain_time = 1;
        w.update_rain(1, true, &mut r);
        assert!(!w.raining && w.max_raining == 0.0);
        let before = w.cloud_alpha;
        w.update_rain(1, true, &mut r);
        assert!(w.cloud_alpha < before);
    }

    #[test]
    fn weak_players_get_no_rain_and_calm_wind() {
        let mut r = rng(3);
        let mut w = Weather::default();
        for _ in 0..DAY * 4 {
            w.update_rain(1, false, &mut r);
            w.update_wind(false, &mut r);
            assert!(w.wind_target.abs() <= 0.35 + 1e-6);
        }
        assert!(!w.raining);
        // The current eases towards the target, never jumps.
        let mut w = Weather { wind_target: 0.8, ..Default::default() };
        w.wind_counter = 1_000_000;
        w.update_wind(true, &mut r);
        assert!(w.wind > 0.0 && w.wind < 0.01);
    }

    #[test]
    fn storms_flash_and_thunder() {
        let mut r = rng(11);
        let mut w = Weather { cloud_alpha: 0.9, max_raining: 0.9, wind_target: 0.8, ..Default::default() };
        w.update_state(true, 20000.0);
        assert!(w.storm && !w.windy_day);
        let (mut flashed, mut thunder) = (false, false);
        for _ in 0..20_000 {
            thunder |= w.update_lightning(true, &mut r);
            flashed |= w.lightning >= 1.0;
        }
        assert!(flashed && thunder);
        assert_eq!(flash([0.0, 100.0, 255.0], 1.0), [255.0; 3]);
        assert_eq!(flash([10.0; 3], 0.0), [10.0; 3]);
        // A dry windy day, and a calm one.
        let mut w = Weather { wind_target: 0.45, ..Default::default() };
        w.update_state(true, 20000.0);
        assert!(w.windy_day && !w.storm);
        w.wind_target = 0.36;
        w.update_state(true, 20000.0);
        assert!(w.windy_day, "until it drops under 0.34");
        w.update_state(false, 20000.0);
        assert!(!w.windy_day);
    }
}
