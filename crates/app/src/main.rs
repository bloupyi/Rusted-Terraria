mod audio;
mod backdrop;
mod cave;
mod combat_text;
mod game;
mod inventory;
mod item_ids;
mod items;
mod magic;
mod net;
mod pots;
mod proto;
mod surface;
mod tooltip;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use game::Game;
use glam::Vec2;
use inventory::{Item, HOTBAR, SLOTS};
use render::{Camera, Renderer, Scene, Sprite};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};
use world::light::Rect;
use world::player::{self, Input, Player};

/// Screen pixels of a hotbar or crafting slot, and the gap after it.
const SLOT: f32 = 40.0;
const SLOT_GAP: f32 = 4.0;
/// Top of the crafting rows, under the inventory.
fn craft_y() -> f32 {
    ui::inventory_bottom(SLOTS) + 10.0
}

/// Top-left of slot `i` of a row of ten starting at `y`.
fn slot_at(i: usize, y: f32) -> Vec2 {
    Vec2::new(10.0 + (i % HOTBAR) as f32 * (SLOT + SLOT_GAP), y + (i / HOTBAR) as f32 * (SLOT + SLOT_GAP))
}

/// Which slot of rows starting at `y` holds screen point `p`, among `n`.
fn slot_under(p: Vec2, y: f32, n: usize) -> Option<usize> {
    (0..n).find(|&i| {
        let o = slot_at(i, y);
        p.x >= o.x && p.x < o.x + SLOT && p.y >= o.y && p.y < o.y + SLOT
    })
}

const TITLE_EVERY: Duration = Duration::from_millis(500);
const PAN_SPEED: f32 = 1200.0;
const ZOOM_STEP: f32 = 1.15;
/// Holding T runs the day this much faster.
const FAST_TIME: f32 = 120.0;

#[derive(Default)]
struct Keys {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    shift: bool,
    jump: bool,
    fast_time: bool,
}

struct App {
    name: String,
    world_px: Vec2,
    vsync: bool,
    game: Game,
    audio: Option<audio::Audio>,
    net: Option<net::Net>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    /// Free camera (phase 1 behaviour) instead of following the player.
    free: bool,
    keys: Keys,
    cursor: Vec2,
    buttons: [bool; 2],
    /// The left button went down on the interface: no digging until it is released.
    ui_click: bool,
    /// The inventory is open (Escape, as in the game), with the crafting rows under it.
    inventory_open: bool,
    split: ui::StackSplit,
    /// The right button went down since the last update (`Main.mouseRightRelease`).
    right_fresh: bool,
    /// Craftable recipes and what each makes, while the crafting rows are open.
    recipes: Vec<(usize, Item)>,
    drag: Option<Vec2>,
    last: Instant,
    frames: u32,
    frame_time: Duration,
    /// `--profile`: time per phase (update, sync, sprites, render), printed with the title.
    profile: Option<[Duration; 4]>,
    title_at: Instant,
    fonts: ui::Fonts,
    hud: ui::Hud,
}

impl App {
    fn screen_to_world(&self, p: Vec2, size: Vec2) -> Vec2 {
        self.camera.center + (p - size * 0.5) / self.camera.zoom
    }

    fn cursor_tile(&self, size: Vec2) -> (i64, i64) {
        let w = self.screen_to_world(self.cursor, size) / 16.0;
        (w.x.floor() as i64, w.y.floor() as i64)
    }

    fn update(&mut self, dt: f32, size: Vec2) {
        let open = self.inventory_open;
        for _ in 0..self.hud.update(dt, self.game.inventory.selected, !open) {
            self.split.tick(self.buttons[0] || self.buttons[1]);
            let slot = ui::inventory_under(self.cursor.into(), SLOTS).filter(|_| open && self.buttons[1]);
            if let Some(i) = slot.filter(|_| !self.game.using_item()) {
                for _ in 0..self.split.takes() {
                    if self.game.inventory.right_click(i) {
                        self.game.sfx.push(audio::Sfx::MenuTick);
                        self.split.took();
                    }
                }
            }
        }
        let k = &self.keys;
        let time_scale = if k.fast_time { FAST_TIME } else { 1.0 };
        if self.free {
            let dir = Vec2::new((k.right as i32 - k.left as i32) as f32, (k.down as i32 - k.up as i32) as f32);
            let speed = PAN_SPEED * if k.shift { 4.0 } else { 1.0 } / self.camera.zoom;
            self.camera.center = (self.camera.center + dir * speed * dt).clamp(Vec2::ZERO, self.world_px);
            self.game.update(None, dt, time_scale);
        } else {
            let input = Input { left: k.left, right: k.right, jump: k.jump || k.up, down: k.down };
            self.game.update(Some(input), dt, time_scale);
            let [x, y] = self.game.player.drawn(self.game.tick_acc);
            let y = y + self.game.player.gfx_off_y;
            self.camera.center = Vec2::new(x + player::WIDTH / 2.0, y + player::HEIGHT / 2.0);
        }
        self.recipes = if self.inventory_open {
            self.game.craftable().into_iter().map(|i| (i, self.game.recipe_item(i))).collect()
        } else {
            Vec::new()
        };
        if self.drag.is_none() && !self.ui_click {
            let (tx, ty) = self.cursor_tile(size);
            let w = self.screen_to_world(self.cursor, size);
            if self.buttons[0] {
                self.game.use_at(w.x, w.y, self.free);
            }
            if self.buttons[1] && !self.over_ui() {
                // A right click in the world throws what the cursor carries.
                if self.game.inventory.mouse.is_some() {
                    if self.right_fresh {
                        self.game.throw_held(false);
                    }
                } else {
                    self.game.place(tx, ty, self.free);
                }
            }
        }
        self.right_fresh = false;
        self.sync_net();
        for sfx in self.game.sfx.drain(..) {
            if let Some(a) = &mut self.audio {
                a.play(sfx, 0.6);
            }
        }
        if let Some(a) = &mut self.audio {
            a.music(self.game.music());
        }
        (self.camera.sky, self.camera.background) = self.game.sky();
        self.camera.water_style = self.game.water_style(self.camera.center.y / 16.0);
        self.camera.surface = self.game.env.world_surface as f32 * 16.0;
        self.camera.wind = [self.game.wind.grass.fract() as f32, self.game.env.wind];
        self.camera.time += dt;
    }

    /// The game's hotbar (or the open inventory with the crafting rows) and life
    /// hearts, the boss bar, the name under the cursor and the carried item, as
    /// world sprites under the current camera.
    fn hud(&self, size: Vec2) -> Vec<Sprite> {
        let z = self.camera.zoom;
        let game = &self.game;
        let mut p = ui::Painter {
            sheets: &game.scene.mesher.atlas.named,
            fonts: &self.fonts,
            origin: self.screen_to_world(Vec2::ZERO, size).into(),
            unit: 1.0 / z,
            out: Vec::new(),
        };
        // `DrawCombatText`, in the world: four dim copies a pixel away, then the number.
        let mut world = ui::Painter { sheets: p.sheets, fonts: p.fonts, origin: [0.0; 2], unit: 1.0, out: Vec::new() };
        for t in &game.combat_text {
            let font = if t.crit { ui::FontId::CombatCrit } else { ui::FontId::CombatText };
            let [w, h] = world.measure(font, &t.text);
            let k = t.scale * t.alpha;
            let [r, g, b, a] = t.color.map(|v| v as f32 / 255.0 * k);
            for (dx, dy, c) in [
                (-1.0, 0.0, [r * 0.3, g * 0.3, b * 0.3, a]),
                (1.0, 0.0, [r * 0.3, g * 0.3, b * 0.3, a]),
                (0.0, -1.0, [r * 0.3, g * 0.3, b * 0.3, a]),
                (0.0, 1.0, [r * 0.3, g * 0.3, b * 0.3, a]),
                (0.0, 0.0, [r, g, b, a]),
            ] {
                let at = [t.center[0] + dx, t.center[1] + dy];
                world.string(font, &t.text, at, [w / 2.0, h / 2.0], t.scale, t.rotation, c);
            }
        }
        p.out = world.out;
        let inv = &game.inventory;
        let icon = |p: &mut ui::Painter, item: Item, [cx, cy]: [f32; 2], s: f32| {
            let side = game.item_size(item).min(ui::ICON_LIMIT) * s;
            let icon = game.item_icon(item, p.world([cx - side / 2.0, cy - side / 2.0]), side / z, 0);
            p.out.extend(icon);
        };
        let slot_icon = |p: &mut ui::Painter, i: usize, c: [f32; 2], s: f32| {
            if let Some(stack) = inv.slots[i] {
                icon(p, stack.item, c, s);
            }
        };
        if self.inventory_open {
            let counts: Vec<u16> = inv.slots.iter().map(|s| s.map_or(0, |s| s.count)).collect();
            let label = game.texts.get("LegacyInterface.4").unwrap_or("");
            self.hud.draw_inventory(&mut p, label, &counts, slot_icon);
        } else {
            let counts: Vec<u16> = (0..HOTBAR).map(|i| inv.slots[i].map_or(0, |s| s.count)).collect();
            let selected = inv.slots[inv.selected].map(|s| s.item);
            // `Lang.inter[37]` ("Items") over an empty hand.
            let title =
                selected.and_then(|i| game.item_name(i)).or_else(|| game.texts.get("LegacyInterface.37")).unwrap_or("");
            self.hud.draw_hotbar(&mut p, inv.selected, &counts, title, slot_icon);
        }
        let life = game.texts.get("LegacyInterface.0").unwrap_or("");
        self.hud.draw_life(&mut p, size.x, game.hp.max(0), game::MAX_HP, life);
        let mana = game.texts.get("LegacyInterface.2").unwrap_or("");
        self.hud.draw_mana(&mut p, size.x, game.mana.cur, game.mana.max, mana);
        if let Some(life) = game.boss_life() {
            let (w, x) = (size.x * 0.5, size.x * 0.25);
            p.rect([x, size.y - 40.0], w, 14.0, [0.1, 0.1, 0.15, 0.7]);
            p.rect([x + 2.0, size.y - 38.0], (w - 4.0) * life, 10.0, [0.75, 0.1, 0.2, 0.95]);
        }
        let craft_y = craft_y();
        for (k, &(r, item)) in self.recipes.iter().enumerate() {
            let hover = slot_under(self.cursor, craft_y, self.recipes.len()) == Some(k);
            let o = slot_at(k, craft_y);
            let frame = if hover { [0.4, 0.8, 0.5, 0.9] } else { [0.2, 0.35, 0.25, 0.7] };
            p.rect(o.into(), SLOT, SLOT, frame);
            p.rect((o + 3.0).into(), SLOT - 6.0, SLOT - 6.0, [0.05, 0.05, 0.08, 0.7]);
            let icon = game.item_icon(item, p.world((o + 8.0).into()), (SLOT - 16.0) / z, 0);
            p.out.extend(icon);
            let count = items::RECIPES[r].count;
            if count > 1 {
                let at = [o.x + 10.0 * 0.75, o.y + 26.0 * 0.75];
                p.text(ui::FontId::ItemStack, &count.to_string(), at, 0.75, [1.0; 4], 0.75);
            }
        }
        // `ItemSlot.MouseHover`: the tooltip of what is under the cursor.
        let cursor = self.cursor.into();
        if let Some(stack) = self.slot_under_cursor().and_then(|i| inv.slots[i]) {
            if let (Some(name), Some((key, stats))) = (game.item_name(stack.item), game.item_data(stack.item)) {
                let id = game.item_id(stack.item).unwrap_or(0);
                let lines = tooltip::lines(&game.texts, id, name, key, &stats, stack.count);
                self.hud.draw_item_tooltip(&mut p, cursor, size.into(), &lines, inv.mouse.is_some());
            }
        }
        if let Some(m) = inv.mouse {
            self.hud.draw_mouse_item(&mut p, cursor, m.count, |p, c, s| icon(p, m.item, c, s));
        }
        p.out
    }

    /// The inventory slot (the hotbar's when closed) under the cursor.
    fn slot_under_cursor(&self) -> Option<usize> {
        if self.inventory_open {
            ui::inventory_under(self.cursor.into(), SLOTS)
        } else {
            self.hud.hotbar_under(self.cursor.into())
        }
    }

    fn over_ui(&self) -> bool {
        self.slot_under_cursor().is_some()
            || (self.inventory_open && slot_under(self.cursor, craft_y(), self.recipes.len()).is_some())
    }

    /// A left press on the interface: moves items in the open inventory, picks a
    /// hotbar slot or crafts. True if it hit one.
    fn click_ui(&mut self) -> bool {
        if self.inventory_open {
            if let Some(i) = ui::inventory_under(self.cursor.into(), SLOTS) {
                if !self.game.using_item() && self.game.inventory.left_click(i) {
                    self.game.sfx.push(audio::Sfx::Grab);
                }
                return true;
            }
        } else if let Some(i) = self.hud.hotbar_under(self.cursor.into()) {
            self.game.inventory.selected = i;
            return true;
        }
        if let Some(k) = slot_under(self.cursor, craft_y(), self.recipes.len()) {
            self.game.craft(self.recipes[k].0);
            return true;
        }
        false
    }

    /// Applies what other players sent, then sends our player, edits and, as host, the world
    /// to new clients and the monsters.
    fn sync_net(&mut self) {
        let Some(net) = &mut self.net else { return };
        for (from, msg) in net.poll() {
            // The host relays what a client does to the other clients.
            if net.host && !matches!(msg, net::Msg::Hit { .. }) {
                net.send(&msg, Some(from));
            }
            self.game.apply(from, &msg);
        }
        for id in net.joined() {
            net.send_to(id, &net::Msg::Welcome { id, world: self.game.world.to_wld() });
        }
        let p = &self.game.player;
        let me = net::Msg::Player { id: net.id, x: p.x, y: p.y, vx: p.vx, vy: p.vy, facing_left: p.facing_left };
        net.send(&me, None);
        for m in self.game.outbox.drain(..) {
            net.send(&m, None);
        }
        if net.host {
            net.send(&net::Msg::Mobs(self.game.mob_states()), None);
            net.send(&net::Msg::Time(self.game.time), None);
        }
    }

    fn view(&self, size: Vec2) -> Rect {
        let half = size / (2.0 * self.camera.zoom * 16.0);
        let lo = ((self.camera.center / 16.0) - half).floor().max(Vec2::ZERO);
        let hi = ((self.camera.center / 16.0) + half).ceil();
        let (w, h) = (self.game.world.width, self.game.world.height);
        Rect { x0: (lo.x as u32).min(w), y0: (lo.y as u32).min(h), x1: (hi.x as u32).min(w), y1: (hi.y as u32).min(h) }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            el.create_window(Window::default_attributes().with_title("Rusted Terraria").with_maximized(true))
                .expect("window"),
        );
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(
            el.owned_display_handle(),
        )));
        let surface = instance.create_surface(window.clone()).expect("surface");
        let size = window.inner_size();
        let t = Instant::now();
        self.renderer =
            Some(Renderer::new(&instance, surface, (size.width, size.height), self.vsync, &mut self.game.scene));
        println!("gpu upload: {:.2?}", t.elapsed());
        self.window = Some(window);
        self.last = Instant::now();
        self.title_at = self.last;
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(renderer) = &mut self.renderer else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => renderer.resize(s.width, s.height),
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                let press = down && !event.repeat;
                let PhysicalKey::Code(code) = event.physical_key else { return };
                let k = &mut self.keys;
                match code {
                    KeyCode::KeyA | KeyCode::ArrowLeft => k.left = down,
                    KeyCode::KeyD | KeyCode::ArrowRight => k.right = down,
                    KeyCode::KeyW | KeyCode::ArrowUp => k.up = down,
                    KeyCode::KeyS | KeyCode::ArrowDown => k.down = down,
                    KeyCode::Space => k.jump = down,
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => k.shift = down,
                    KeyCode::KeyY => k.fast_time = down,
                    // The game's Throw key.
                    KeyCode::KeyT if press => self.game.throw_held(true),
                    KeyCode::KeyH if press => self.game.quick_heal(),
                    KeyCode::Escape if press => {
                        self.inventory_open = !self.inventory_open;
                        if !self.inventory_open {
                            // `GetOrDropItem`: back into the inventory, the rest thrown.
                            self.game.inventory.return_mouse();
                            self.game.throw_held(false);
                        }
                    }
                    KeyCode::KeyL if press => self.camera.lighting = !self.camera.lighting,
                    KeyCode::KeyF if press => {
                        self.free = !self.free;
                        if !self.free {
                            // Drop the player where the camera is.
                            let c = self.camera.center;
                            self.game.player = Player::at(c.x, c.y);
                        }
                    }
                    KeyCode::KeyQ if press => self.game.inventory.cycle(-1),
                    KeyCode::KeyE if press => self.game.inventory.cycle(1),
                    KeyCode::Digit1 if press => self.game.inventory.selected = 0,
                    KeyCode::Digit2 if press => self.game.inventory.selected = 1,
                    KeyCode::Digit3 if press => self.game.inventory.selected = 2,
                    KeyCode::Digit4 if press => self.game.inventory.selected = 3,
                    KeyCode::Digit5 if press => self.game.inventory.selected = 4,
                    KeyCode::Digit6 if press => self.game.inventory.selected = 5,
                    KeyCode::Digit7 if press => self.game.inventory.selected = 6,
                    KeyCode::Digit8 if press => self.game.inventory.selected = 7,
                    KeyCode::Digit9 if press => self.game.inventory.selected = 8,
                    KeyCode::Digit0 if press => self.game.inventory.selected = 9,
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = Vec2::new(position.x as f32, position.y as f32);
                if let Some(prev) = self.drag {
                    self.camera.center -= (p - prev) / self.camera.zoom;
                    self.drag = Some(p);
                }
                self.cursor = p;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let down = state == ElementState::Pressed;
                match button {
                    MouseButton::Left => {
                        self.buttons[0] = down;
                        self.ui_click = down && self.click_ui();
                    }
                    MouseButton::Right => {
                        self.buttons[1] = down;
                        self.right_fresh = down;
                    }
                    MouseButton::Middle if self.free => self.drag = down.then_some(self.cursor),
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 60.0,
                };
                let size = renderer.size();
                let before = self.screen_to_world(self.cursor, size);
                self.camera.zoom = (self.camera.zoom * ZOOM_STEP.powf(steps)).clamp(1.0 / 64.0, 8.0);
                // Keep the point under the cursor fixed.
                self.camera.center += before - self.screen_to_world(self.cursor, size);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now - self.last;
                self.last = now;
                let size = renderer.size();
                let t0 = Instant::now();
                self.update(dt.as_secs_f32().min(0.1), size);
                let view = self.view(size);
                let cursor = self.cursor_tile(size);
                let hud = self.hud(size);
                let Some(renderer) = &mut self.renderer else { return };
                let t1 = Instant::now();
                self.game.sync(renderer, view);
                let t2 = Instant::now();
                let (mut under, mut over) = self.game.sprites(view, Some(cursor), self.free);
                let ticks = (self.camera.time * 60.0) as u32;
                let center = self.camera.center.to_array();
                under.splice(0..0, self.game.backdrop(center, self.camera.zoom, (size.x as u32, size.y as u32), ticks));
                over.extend(hud);
                let t3 = Instant::now();
                let stats = renderer.render(&self.camera, &under, &over);
                if let Some(p) = &mut self.profile {
                    for (acc, d) in p.iter_mut().zip([t1 - t0, t2 - t1, t3 - t2, t3.elapsed()]) {
                        *acc += d;
                    }
                }
                self.frames += 1;
                self.frame_time += dt;
                if now - self.title_at >= TITLE_EVERY {
                    let fps = self.frames as f32 / (now - self.title_at).as_secs_f32();
                    let ms = self.frame_time.as_secs_f64() * 1000.0 / self.frames as f64;
                    let mode = if stats.lod { "map".to_string() } else { format!("{} inst", stats.instances) };
                    let clock = self.game.clock();
                    let tool = match self.game.inventory.selected() {
                        None => "empty".to_string(),
                        Some(s) => match s.item {
                            Item::Block(t) => format!("block {t} x{}", s.count),
                            Item::Bucket(None) => "empty bucket".to_string(),
                            Item::Bucket(Some(l)) => format!("{l:?} bucket"),
                            Item::Object(i) => format!("object {} x{}", self.game.templates[i as usize].tile, s.count),
                            Item::Tool(t) => format!("{t:?}"),
                            Item::Material(m) => format!("{m:?} x{}", s.count),
                            Item::Id(id) => format!("item {id} x{}", s.count),
                        },
                    };
                    if let Some(window) = &self.window {
                        window.set_title(&format!(
                            "Rusted Terraria - {} - {fps:.0} FPS - {ms:.2} ms - {} draws, {mode} - {:02}:{:02} - {tool} - {} liquid chunks - {} players",
                            self.name,
                            stats.draws,
                            clock as u32,
                            (clock.fract() * 60.0) as u32,
                            self.game.sim.active(),
                            self.game.others.len() + 1,
                        ));
                    }
                    if let Some(p) = &mut self.profile {
                        let ms = p.map(|d| d.as_secs_f64() * 1000.0 / self.frames as f64);
                        println!(
                            "{fps:.0} FPS, zoom {:.3}: update {:.2} ms, sync {:.2} ms, sprites {:.2} ms, render {:.2} ms, {} draws, {mode}",
                            self.camera.zoom, ms[0], ms[1], ms[2], ms[3], stats.draws
                        );
                        *p = [Duration::ZERO; 4];
                    }
                    (self.frames, self.frame_time, self.title_at) = (0, Duration::ZERO, now);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

fn main() {
    let mut path = None;
    let mut vsync = false;
    let mut profile = false;
    let mut zoom = 1.0;
    let mut free = false;
    let mut host = None;
    let mut join = None;
    let mut at = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--vsync" => vsync = true,
            "--profile" => profile = true,
            "--zoom" => zoom = args.next().and_then(|z| z.parse().ok()).unwrap_or(1.0f32).clamp(1.0 / 64.0, 8.0),
            "--free" => free = true,
            "--host" => host = Some(net::DEFAULT_PORT),
            "--port" => host = args.next().and_then(|p| p.parse().ok()),
            "--join" => join = args.next(),
            "--at" => {
                at = args.next().and_then(|s| {
                    s.split_once(',').and_then(|(x, y)| Some((x.parse::<f32>().ok()?, y.parse::<f32>().ok()?)))
                })
            }
            _ => path = Some(PathBuf::from(a)),
        }
    }
    let usage = || -> ! {
        eprintln!(
            "usage: rusted-terraria [--vsync] [--profile] [--zoom <z>] [--free] [--at <x,y>] [--host | --port <port>] <world.wld>"
        );
        eprintln!("       rusted-terraria [--vsync] [--free] --join <host[:port]>");
        std::process::exit(2);
    };

    let start = Instant::now();
    let (world, net) = match (join, path) {
        (Some(addr), _) => {
            let addr = if addr.contains(':') { addr } else { format!("{addr}:{}", net::DEFAULT_PORT) };
            println!("joining {addr}...");
            let (net, world) = net::Net::join(&addr).unwrap_or_else(|e| {
                eprintln!("{addr}: {e}");
                std::process::exit(1);
            });
            (world, Some(net))
        }
        (None, Some(path)) => {
            let world = world::World::load(&path).unwrap_or_else(|e| {
                eprintln!("{}: {e}", path.display());
                std::process::exit(1);
            });
            let net = host.map(|port| {
                let n = net::Net::host(port).unwrap_or_else(|e| {
                    eprintln!("port {port}: {e}");
                    std::process::exit(1);
                });
                println!("hosting on port {port}");
                n
            });
            (world, net)
        }
        (None, None) => usage(),
    };
    let parsed = start.elapsed();
    let images = formats::install::images_dir();
    match &images {
        Some(d) => println!("assets: {}", d.display()),
        None => println!("assets: Terraria install not found (set {}), using placeholders", formats::install::ENV_VAR),
    }
    let mut sheets = game::sprite_sheets();
    let env = world.header.env.clone().unwrap_or_default();
    sheets.extend(backdrop::sheets(&env));
    sheets.extend(cave::sheets(&env, world.header.world_id));
    let culture = formats::lang::culture();
    let exe = formats::install::exe_path().and_then(|p| std::fs::read(p).ok());
    let texts = exe.and_then(|e| formats::lang::Texts::load(&e, &culture).ok()).unwrap_or_default();
    println!("texts: {culture}, {} entries", texts.len());
    let fonts_dir = images.as_deref().and_then(|d| d.parent()).map(|c| c.join("Fonts"));
    let (fonts, pages) = ui::Fonts::load(fonts_dir.as_deref(), &texts.chars());
    let scene = Scene::build(&world, images.as_deref(), &game::EXTRA_TILES, &sheets, pages);
    println!(
        "{} ({}x{}, v{}): parse {parsed:.2?}, scene {:.2?}, {} sheets missing",
        world.header.name,
        world.width,
        world.height,
        world.header.version,
        start.elapsed() - parsed,
        scene.mesher.atlas.missing
    );

    let audio = images.as_deref().and_then(|d| d.parent()).and_then(|c| audio::Audio::new(c.to_path_buf()));
    println!("audio: {}", if audio.is_some() { "on" } else { "off (no output device or no install)" });

    let world_px = Vec2::new(world.width as f32, world.height as f32) * 16.0;
    let mut game = Game::new(world, scene);
    game.texts = texts;
    game.client = net.as_ref().is_some_and(|n| !n.host);
    let (cx, cy) = at.map_or(game.player.center(), |(x, y)| (x * 16.0, y * 16.0));
    let mut app = App {
        name: game.world.header.name.clone(),
        world_px,
        vsync,
        game,
        audio,
        net,
        window: None,
        renderer: None,
        camera: Camera {
            center: Vec2::new(cx, cy),
            zoom,
            lighting: true,
            sky: [1.0; 3],
            background: [0.0; 3],
            time: 0.0,
            water_style: 0,
            surface: 0.0,
            wind: [0.0; 2],
        },
        free,
        keys: Keys::default(),
        cursor: Vec2::ZERO,
        buttons: [false; 2],
        ui_click: false,
        inventory_open: false,
        split: ui::StackSplit::default(),
        right_fresh: false,
        recipes: Vec::new(),
        drag: None,
        last: Instant::now(),
        frames: 0,
        frame_time: Duration::ZERO,
        profile: profile.then_some([Duration::ZERO; 4]),
        title_at: Instant::now(),
        fonts,
        hud: ui::Hud::default(),
    };
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app).expect("event loop");
}
