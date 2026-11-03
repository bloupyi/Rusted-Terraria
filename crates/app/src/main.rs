use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::Vec2;
use render::{Camera, Renderer, Scene};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const TITLE_EVERY: Duration = Duration::from_millis(500);
const PAN_SPEED: f32 = 1200.0;
const ZOOM_STEP: f32 = 1.15;

struct App {
    name: String,
    world_px: Vec2,
    vsync: bool,
    scene: Option<Scene>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    keys: [bool; 5],
    cursor: Vec2,
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

    fn update(&mut self, dt: f32) {
        let [l, r, u, d, fast] = self.keys;
        let dir = Vec2::new((r as i32 - l as i32) as f32, (d as i32 - u as i32) as f32);
        let speed = PAN_SPEED * if fast { 4.0 } else { 1.0 } / self.camera.zoom;
        self.camera.center = (self.camera.center + dir * speed * dt).clamp(Vec2::ZERO, self.world_px);
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
        let scene = self.scene.take().expect("scene");
        self.renderer = Some(Renderer::new(&instance, surface, (size.width, size.height), self.vsync, scene));
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
                let PhysicalKey::Code(code) = event.physical_key else { return };
                let slot = match code {
                    KeyCode::KeyA | KeyCode::ArrowLeft => 0,
                    KeyCode::KeyD | KeyCode::ArrowRight => 1,
                    KeyCode::KeyW | KeyCode::ArrowUp => 2,
                    KeyCode::KeyS | KeyCode::ArrowDown => 3,
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => 4,
                    KeyCode::Escape => return el.exit(),
                    KeyCode::KeyL if down && !event.repeat => {
                        self.camera.lighting = !self.camera.lighting;
                        return;
                    }
                    _ => return,
                };
                self.keys[slot] = down;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = Vec2::new(position.x as f32, position.y as f32);
                if let Some(prev) = self.drag {
                    self.camera.center -= (p - prev) / self.camera.zoom;
                    self.drag = Some(p);
                }
                self.cursor = p;
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left | MouseButton::Middle, .. } => {
                self.drag = (state == ElementState::Pressed).then_some(self.cursor);
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
                self.update(dt.as_secs_f32().min(0.1));
                let Some(renderer) = &mut self.renderer else { return };
                let stats = renderer.render(&self.camera);
                self.frames += 1;
                self.frame_time += dt;
                if now - self.title_at >= TITLE_EVERY {
                    let fps = self.frames as f32 / (now - self.title_at).as_secs_f32();
                    let ms = self.frame_time.as_secs_f64() * 1000.0 / self.frames as f64;
                    let mode = if stats.lod { "map".to_string() } else { format!("{} inst", stats.instances) };
                    if let Some(window) = &self.window {
                        window.set_title(&format!(
                            "Rusted Terraria - {} - {fps:.0} FPS - {ms:.2} ms - {} draws, {mode}",
                            self.name, stats.draws
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
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--vsync" => vsync = true,
            _ => path = Some(PathBuf::from(a)),
        }
    }
    let Some(path) = path else {
        eprintln!("usage: rusted-terraria [--vsync] <world.wld>");
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
    let scene = Scene::build(&world, images.as_deref());
    println!(
        "{} ({}x{}, v{}): parse {parsed:.2?}, scene {:.2?}, {} sheets missing",
        world.header.name,
        world.width,
        world.height,
        world.header.version,
        start.elapsed() - parsed,
        scene.atlas.missing
    );

    let world_px = Vec2::new(world.width as f32, world.height as f32) * 16.0;
    let mut app = App {
        name: world.header.name.clone(),
        world_px,
        vsync,
        scene: Some(scene),
        window: None,
        renderer: None,
        camera: Camera { center: world_px * Vec2::new(0.5, 0.3), zoom: 1.0, lighting: true },
        keys: [false; 5],
        cursor: Vec2::ZERO,
        drag: None,
        last: Instant::now(),
        frames: 0,
        frame_time: Duration::ZERO,
        title_at: Instant::now(),
    };
    drop(world);
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app).expect("event loop");
}
