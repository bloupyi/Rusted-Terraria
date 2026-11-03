mod audio;
mod game;
mod inventory;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use game::Game;
use glam::Vec2;
use inventory::{Item, HOTBAR};
use render::{Camera, Renderer, Scene, Sprite};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};
use world::light::Rect;
use world::player::Input;

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
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    /// Free camera (phase 1 behaviour) instead of following the player.
    free: bool,
    keys: Keys,
    cursor: Vec2,
    buttons: [bool; 2],
    drag: Option<Vec2>,
    last: Instant,
    frames: u32,
    frame_time: Duration,
    title_at: Instant,
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
            let (cx, cy) = self.game.player.center();
            self.camera.center = Vec2::new(cx, cy);
        }
        if self.drag.is_none() {
            let (tx, ty) = self.cursor_tile(size);
            let w = self.screen_to_world(self.cursor, size);
            if self.buttons[0] && !self.game.attack(w.x, w.y, self.free) {
                self.game.dig(tx, ty, self.free);
            }
            if self.buttons[1] {
                self.game.place(tx, ty, self.free);
            }
        }
        for sfx in self.game.sfx.drain(..) {
            if let Some(a) = &mut self.audio {
                a.play(sfx, 0.6);
            }
        }
        (self.camera.sky, self.camera.background) = self.game.sky();
        self.camera.time += dt;
    }

    /// Hotbar in the top-left corner, as world sprites under the current camera.
    /// Each slot shows its item, with a bar for the stack size.
    fn hotbar(&self, size: Vec2) -> Vec<Sprite> {
        const SLOT: f32 = 40.0;
        let z = self.camera.zoom;
        let at = |x: f32, y: f32| -> [f32; 2] { self.screen_to_world(Vec2::new(x, y), size).into() };
        let rect = |x: f32, y: f32, w: f32, h: f32, color: [f32; 4]| Sprite::flat(at(x, y), [w / z, h / z], color);
        let inv = &self.game.inventory;
        let mut rects = Vec::new();
        for i in 0..HOTBAR {
            let x = 10.0 + i as f32 * (SLOT + 4.0);
            let frame = if i == inv.selected { [0.9, 0.8, 0.3, 0.9] } else { [0.1, 0.1, 0.15, 0.6] };
            rects.push(rect(x, 10.0, SLOT, SLOT, frame));
            rects.push(rect(x + 3.0, 13.0, SLOT - 6.0, SLOT - 6.0, [0.05, 0.05, 0.08, 0.7]));
            if let Some(s) = inv.slots[i] {
                rects.extend(self.game.item_icon(s.item, at(x + 8.0, 16.0), (SLOT - 16.0) / z, 0));
                if s.count > 1 {
                    let w = (SLOT - 8.0) * s.count as f32 / inventory::MAX_STACK as f32;
                    rects.push(rect(x + 4.0, 10.0 + SLOT - 7.0, w.max(2.0), 3.0, [0.9, 0.9, 0.9, 0.9]));
                }
            }
        }
        // Health under the hotbar.
        let width = HOTBAR as f32 * (SLOT + 4.0) - 4.0;
        rects.push(rect(10.0, 56.0, width, 8.0, [0.1, 0.1, 0.15, 0.6]));
        let hp = self.game.hp.max(0) as f32 / game::MAX_HP as f32;
        rects.push(rect(11.0, 57.0, (width - 2.0) * hp, 6.0, [0.85, 0.15, 0.15, 0.95]));
        rects
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
                    KeyCode::KeyT => k.fast_time = down,
                    KeyCode::Escape => el.exit(),
                    KeyCode::KeyL if press => self.camera.lighting = !self.camera.lighting,
                    KeyCode::KeyF if press => {
                        self.free = !self.free;
                        if !self.free {
                            // Drop the player where the camera is.
                            let c = self.camera.center;
                            (self.game.player.x, self.game.player.y) = (c.x, c.y);
                            (self.game.player.vx, self.game.player.vy) = (0.0, 0.0);
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
                    MouseButton::Left => self.buttons[0] = down,
                    MouseButton::Right => self.buttons[1] = down,
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
                self.update(dt.as_secs_f32().min(0.1), size);
                let view = self.view(size);
                let cursor = self.cursor_tile(size);
                let hud = self.hotbar(size);
                let Some(renderer) = &mut self.renderer else { return };
                self.game.sync(renderer, view);
                let (under, mut over) = self.game.sprites(view, Some(cursor), self.free);
                over.extend(hud);
                let stats = renderer.render(&self.camera, &under, &over);
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
                        },
                    };
                    if let Some(window) = &self.window {
                        window.set_title(&format!(
                            "Rusted Terraria - {} - {fps:.0} FPS - {ms:.2} ms - {} draws, {mode} - {:02}:{:02} - {tool} - {} liquid chunks",
                            self.name,
                            stats.draws,
                            clock as u32,
                            (clock.fract() * 60.0) as u32,
                            self.game.sim.active(),
                        ));
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
    let mut free = false;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--vsync" => vsync = true,
            "--free" => free = true,
            _ => path = Some(PathBuf::from(a)),
        }
    }
    let Some(path) = path else {
        eprintln!("usage: rusted-terraria [--vsync] [--free] <world.wld>");
        std::process::exit(2);
    };

    let start = Instant::now();
    let world = world::World::load(&path).unwrap_or_else(|e| {
        eprintln!("{}: {e}", path.display());
        std::process::exit(1);
    });
    let parsed = start.elapsed();
    let images = formats::install::images_dir();
    match &images {
        Some(d) => println!("assets: {}", d.display()),
        None => println!("assets: Terraria install not found (set {}), using placeholders", formats::install::ENV_VAR),
    }
    let scene = Scene::build(&world, images.as_deref(), &game::EXTRA_TILES, &game::sprite_sheets());
    println!(
        "{} ({}x{}, v{}): parse {parsed:.2?}, scene {:.2?}, {} sheets missing",
        world.header.name,
        world.width,
        world.height,
        world.header.version,
        start.elapsed() - parsed,
        scene.mesher.atlas.missing
    );

    let audio = images.as_deref().and_then(|d| d.parent()).and_then(|c| audio::Audio::new(c.join("Sounds")));
    println!("audio: {}", if audio.is_some() { "on" } else { "off (no output device or no install)" });

    let world_px = Vec2::new(world.width as f32, world.height as f32) * 16.0;
    let game = Game::new(world, scene);
    let (cx, cy) = game.player.center();
    let mut app = App {
        name: game.world.header.name.clone(),
        world_px,
        vsync,
        game,
        audio,
        window: None,
        renderer: None,
        camera: Camera {
            center: Vec2::new(cx, cy),
            zoom: 1.0,
            lighting: true,
            sky: [1.0; 3],
            background: [0.0; 3],
            time: 0.0,
        },
        free,
        keys: Keys::default(),
        cursor: Vec2::ZERO,
        buttons: [false; 2],
        drag: None,
        last: Instant::now(),
        frames: 0,
        frame_time: Duration::ZERO,
        title_at: Instant::now(),
    };
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app).expect("event loop");
}
