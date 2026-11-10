use bytemuck::{Pod, Zeroable};
use glam::Vec2;
use wgpu::util::DeviceExt;

use crate::atlas::LAYER_SIZE;
use world::light::{LightMap, Rect};

use crate::mesh::{water_sheet, Instance, RowUpdate, Scene, LOD_PAGE, WATER_SHEETS};

/// Below this many screen pixels per tile, the map texture replaces tile instances.
const LOD_PIXELS_PER_TILE: f32 = 2.0;
const CHUNK_PX: f32 = (world::CHUNK * 16) as f32;

pub struct Camera {
    /// World pixels (16 per tile), y down.
    pub center: Vec2,
    /// Screen pixels per world pixel.
    pub zoom: f32,
    pub lighting: bool,
    /// Skylight colour for the time of day, and the background behind the world.
    pub sky: [f32; 3],
    pub background: [f32; 3],
    /// Seconds, for animated liquids.
    pub time: f32,
    /// `water_N` sheet for water, from the biome and depth.
    pub water_style: u32,
    /// Top of the underground backdrop, world pixels: liquid behind tiles stops there.
    pub surface: f32,
}

/// A rectangle in world pixels: flat colour, or an atlas region tinted by
/// `color` (premultiplied). Player, mobs, drops, tree tops, the hotbar.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug)]
pub struct Sprite {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    /// Atlas texels `x, y, w, h`; `w == 0` draws flat colour.
    pub uv: [f32; 4],
    /// Atlas layer | `SPRITE_LIT` | `SPRITE_FLIP`.
    pub flags: u32,
}

/// Multiplied by the tile light under it.
pub const SPRITE_LIT: u32 = 1 << 8;
/// Mirrored horizontally.
pub const SPRITE_FLIP: u32 = 1 << 9;

impl Sprite {
    pub fn flat(pos: [f32; 2], size: [f32; 2], color: [f32; 4]) -> Self {
        Self { pos, size, color, ..Self::default() }
    }

    /// `src`: region of `sheet` in its own pixels.
    pub fn sheet(pos: [f32; 2], sheet: &crate::atlas::SheetRef, src: [u32; 4], flags: u32) -> Self {
        Self {
            pos,
            size: [src[2] as f32, src[3] as f32],
            color: [1.0; 4],
            uv: [(sheet.x + src[0]) as f32, (sheet.y + src[1]) as f32, src[2] as f32, src[3] as f32],
            flags: sheet.layer | flags,
        }
    }
}

const MAX_RECTS: u64 = 4096;

#[derive(Default, Clone, Copy)]
pub struct Stats {
    pub draws: u32,
    pub instances: u64,
    pub lod: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    scale: [f32; 2],
    offset: [f32; 2],
    lod_pages_x: u32,
    lighting: u32,
    world: [u32; 2],
    sky: [f32; 4],
    /// Per `water_N` sheet: x, y, layer, 1 if loaded.
    water: [[u32; 4]; WATER_SHEETS],
    /// Water style, surface in world pixels.
    liquid: [f32; 4],
}

struct Row {
    statics: Option<wgpu::Buffer>,
    liquids: Option<wgpu::Buffer>,
    /// Walls and tiles index `statics`, liquids index `liquids`.
    offsets: [Vec<u32>; 3],
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    tile_pipeline: wgpu::RenderPipeline,
    lod_pipeline: wgpu::RenderPipeline,
    rect_pipeline: wgpu::RenderPipeline,
    liquid_back: wgpu::RenderPipeline,
    liquid_front: wgpu::RenderPipeline,
    rects: wgpu::Buffer,
    light_tex: wgpu::Texture,
    lod_tex: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    camera: wgpu::Buffer,
    rows: Vec<Row>,
    chunks_x: u32,
    lod_pages: (u32, u32),
    world: [u32; 2],
    water: [[u32; 4]; WATER_SHEETS],
}

impl Renderer {
    pub fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        (width, height): (u32, u32),
        vsync: bool,
        scene: &mut Scene,
    ) -> Self {
        let (pages_x, pages_y, chunks_x) = (scene.lod_pages_x, scene.lod_pages_y, scene.chunks_x);
        let (rows, pages) = (std::mem::take(&mut scene.rows), std::mem::take(&mut scene.lod));
        let atlas = &mut scene.mesher.atlas;
        let water: [[u32; 4]; WATER_SHEETS] =
            std::array::from_fn(|i| atlas.named.get(&water_sheet(i)).map_or([0; 4], |s| [s.x, s.y, s.layer, 1]));
        let light = &scene.light;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .expect("no GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .expect("no GPU device");

        let caps = surface.get_capabilities(&adapter);
        // Non-sRGB target: sheets are blended in gamma space like the original.
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let present_mode = [wgpu::PresentMode::Immediate, wgpu::PresentMode::Mailbox]
            .into_iter()
            .find(|m| !vsync && caps.present_modes.contains(m))
            .unwrap_or(wgpu::PresentMode::Fifo);
        let mut config =
            surface.get_default_config(&adapter, width.max(1), height.max(1)).expect("surface unsupported");
        config.format = format;
        config.present_mode = present_mode;
        config.desired_maximum_frame_latency = 1;
        surface.configure(&device, &config);

        let atlas_tex = texture_array(&device, "atlas", LAYER_SIZE, atlas.layers);
        for (r, tex) in std::mem::take(&mut atlas.uploads) {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &atlas_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: r.x, y: r.y, z: r.layer },
                    aspect: wgpu::TextureAspect::All,
                },
                &tex.rgba,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(tex.width * 4), rows_per_image: None },
                wgpu::Extent3d { width: tex.width, height: tex.height, depth_or_array_layers: 1 },
            );
        }
        let lod_tex = texture_array(&device, "lod", LOD_PAGE, pages.len() as u32);
        for (i, page) in pages.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &lod_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: i as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                page,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(LOD_PAGE * 4), rows_per_image: None },
                wgpu::Extent3d { width: LOD_PAGE, height: LOD_PAGE, depth_or_array_layers: 1 },
            );
        }

        // One page per LOD page, uploaded straight from the row-major light map.
        let light_tex = texture_array(&device, "light", LOD_PAGE, pages.len() as u32);
        for page in 0..pages.len() as u32 {
            let (ox, oy) = ((page % pages_x) * LOD_PAGE, (page / pages_x) * LOD_PAGE);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &light_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: page },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&light.rgba[(oy * light.width + ox) as usize..]),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(light.width * 4), rows_per_image: None },
                wgpu::Extent3d {
                    width: LOD_PAGE.min(light.width - ox),
                    height: LOD_PAGE.min(light.height - oy),
                    depth_or_array_layers: 1,
                },
            );
        }
        let world = [light.width, light.height];

        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                tex_entry(1),
                tex_entry(2),
                tex_entry(3),
            ],
        });
        let view = |t: &wgpu::Texture| {
            t.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        let (atlas_view, lod_view, light_view) = (view(&atlas_tex), view(&lod_tex), view(&light_tex));
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: camera.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&lod_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&light_view) },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Uint32, 1 => Uint32, 2 => Uint32],
        };
        let pipeline = |vs: &str, fs: &str, buffers: &[Option<wgpu::VertexBufferLayout>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(vs),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let tile_pipeline = pipeline("vs_tile", "fs_tile", &[Some(instance_layout.clone())]);
        let liquid_back = pipeline("vs_liquid", "fs_liquid_back", &[Some(instance_layout.clone())]);
        let liquid_front = pipeline("vs_liquid", "fs_liquid_front", &[Some(instance_layout)]);
        let lod_pipeline = pipeline("vs_lod", "fs_lod", &[]);
        let rect_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Sprite>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4, 4 => Uint32],
        };
        let rect_pipeline = pipeline("vs_rect", "fs_rect", &[Some(rect_layout)]);
        let rects = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rects"),
            size: MAX_RECTS * std::mem::size_of::<Sprite>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let rows = rows
            .into_iter()
            .map(|r| Row {
                statics: vertex_buffer(&device, &r.statics),
                liquids: vertex_buffer(&device, &r.liquids),
                offsets: [r.wall_off, r.liquid_off, r.tile_off],
            })
            .collect();

        Self {
            surface,
            device,
            queue,
            config,
            tile_pipeline,
            lod_pipeline,
            rect_pipeline,
            liquid_back,
            liquid_front,
            rects,
            light_tex,
            lod_tex,
            bind_group,
            camera,
            rows,
            chunks_x,
            lod_pages: (pages_x, pages_y),
            world,
            water,
        }
    }

    pub fn update_rows(&mut self, updates: Vec<RowUpdate>) {
        for u in updates {
            let row = &mut self.rows[u.cy as usize];
            if let Some((v, wall_off, tile_off)) = u.statics {
                row.statics = vertex_buffer(&self.device, &v);
                (row.offsets[0], row.offsets[2]) = (wall_off, tile_off);
            }
            if let Some((v, off)) = u.liquids {
                row.liquids = vertex_buffer(&self.device, &v);
                row.offsets[1] = off;
            }
        }
    }

    /// Re-uploads the light of `r`, which must lie within one page (a chunk does).
    pub fn update_light(&self, light: &LightMap, r: Rect) {
        let (px, py) = (r.x0 / LOD_PAGE, r.y0 / LOD_PAGE);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.light_tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x: r.x0 % LOD_PAGE, y: r.y0 % LOD_PAGE, z: py * self.lod_pages.0 + px },
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&light.rgba[(r.y0 * light.width + r.x0) as usize..]),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(light.width * 4), rows_per_image: None },
            wgpu::Extent3d { width: r.width(), height: r.y1 - r.y0, depth_or_array_layers: 1 },
        );
    }

    /// Re-uploads `size`² map texels at `(ox, oy)`, within one page.
    pub fn update_lod(&self, ox: u32, oy: u32, size: u32, texels: &[u8]) {
        let (px, py) = (ox / LOD_PAGE, oy / LOD_PAGE);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.lod_tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x: ox % LOD_PAGE, y: oy % LOD_PAGE, z: py * self.lod_pages.0 + px },
                aspect: wgpu::TextureAspect::All,
            },
            texels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size * 4), rows_per_image: None },
            wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        );
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(self.config.width as f32, self.config.height as f32)
    }

    /// `under` is drawn behind walls (backdrops), `over` in front of everything.
    pub fn render(&mut self, cam: &Camera, under: &[Sprite], over: &[Sprite]) -> Stats {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Stats::default();
            }
            _ => return Stats::default(),
        };
        let size = self.size();
        let scale = Vec2::new(2.0, -2.0) * cam.zoom / size;
        let uniform = CameraUniform {
            scale: scale.into(),
            offset: (-cam.center * scale).into(),
            lod_pages_x: self.lod_pages.0,
            lighting: cam.lighting as u32,
            world: self.world,
            sky: [cam.sky[0], cam.sky[1], cam.sky[2], cam.time],
            water: self.water,
            liquid: [cam.water_style as f32, cam.surface, 0.0, 0.0],
        };
        self.queue.write_buffer(&self.camera, 0, bytemuck::bytes_of(&uniform));
        let under = &under[..under.len().min(MAX_RECTS as usize / 4)];
        let over = &over[..over.len().min(MAX_RECTS as usize - under.len())];
        self.queue.write_buffer(&self.rects, 0, bytemuck::cast_slice(under));
        let over_at = std::mem::size_of_val(under) as u64;
        self.queue.write_buffer(&self.rects, over_at, bytemuck::cast_slice(over));
        let (n_under, n_over) = (under.len() as u32, over.len() as u32);

        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut stats = Stats { lod: cam.zoom * 16.0 < LOD_PIXELS_PER_TILE, ..Default::default() };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: cam.background[0] as f64,
                            g: cam.background[1] as f64,
                            b: cam.background[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            if n_under > 0 {
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_vertex_buffer(0, self.rects.slice(..));
                pass.draw(0..4, 0..n_under);
                stats.draws += 1;
            }
            if stats.lod {
                pass.set_pipeline(&self.lod_pipeline);
                pass.draw(0..4, 0..self.lod_pages.0 * self.lod_pages.1);
                stats.draws = 1;
            } else {
                // Walls overhang their tile by 8 px: widen by one tile.
                let half = size / (2.0 * cam.zoom) + 16.0;
                let lo = ((cam.center - half) / CHUNK_PX).floor().max(Vec2::ZERO);
                let hi = ((cam.center + half) / CHUNK_PX).ceil();
                let (x0, x1) = (lo.x as usize, (hi.x as usize).min(self.chunks_x as usize));
                let (y0, y1) = (lo.y as usize, (hi.y as usize).min(self.rows.len()));
                // Each layer over all visible rows before the next, so tiles cover wall overhang.
                // Liquids go twice, as in the game: opaque behind walls above the
                // underground backdrop, then translucent in front of tiles.
                for (layer, pipeline) in
                    [(1, &self.liquid_back), (0, &self.tile_pipeline), (2, &self.tile_pipeline), (1, &self.liquid_front)]
                {
                    pass.set_pipeline(pipeline);
                    for row in self.rows.get(y0..y1).unwrap_or(&[]) {
                        let off = &row.offsets[layer];
                        let buf = if layer == 1 { &row.liquids } else { &row.statics };
                        let Some(buf) = buf else { continue };
                        if x0 >= x1 || off[x0] == off[x1] {
                            continue;
                        }
                        pass.set_vertex_buffer(0, buf.slice(..));
                        pass.draw(0..4, off[x0]..off[x1]);
                        stats.draws += 1;
                        stats.instances += (off[x1] - off[x0]) as u64;
                    }
                }
            }
            if n_over > 0 {
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_vertex_buffer(0, self.rects.slice(..));
                pass.draw(0..4, n_under..n_under + n_over);
                stats.draws += 1;
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        stats
    }
}

fn vertex_buffer(device: &wgpu::Device, v: &[Instance]) -> Option<wgpu::Buffer> {
    (!v.is_empty()).then(|| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(v),
            usage: wgpu::BufferUsages::VERTEX,
        })
    })
}

fn texture_array(device: &wgpu::Device, label: &str, size: u32, layers: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: layers.max(1) },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
