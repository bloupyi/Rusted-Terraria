use bytemuck::{Pod, Zeroable};
use glam::Vec2;
use wgpu::util::DeviceExt;

use crate::atlas::LAYER_SIZE;
use crate::mesh::{Instance, Scene, LOD_PAGE};

/// Below this many screen pixels per tile, the map texture replaces tile instances.
const LOD_PIXELS_PER_TILE: f32 = 2.0;
const CHUNK_PX: f32 = (world::CHUNK * 16) as f32;

pub struct Camera {
    /// World pixels (16 per tile), y down.
    pub center: Vec2,
    /// Screen pixels per world pixel.
    pub zoom: f32,
}

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
    _pad: u32,
}

struct Row {
    buffer: Option<wgpu::Buffer>,
    wall_off: Vec<u32>,
    tile_off: Vec<u32>,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    tile_pipeline: wgpu::RenderPipeline,
    lod_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    camera: wgpu::Buffer,
    rows: Vec<Row>,
    chunks_x: u32,
    lod_pages: (u32, u32),
}

impl Renderer {
    pub fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        (width, height): (u32, u32),
        vsync: bool,
        scene: Scene,
    ) -> Self {
        let Scene { atlas, rows, chunks_x, lod_pages_x: pages_x, lod_pages_y: pages_y, lod: pages } = scene;
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
        for (r, tex) in &atlas.uploads {
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
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                tex_entry(1),
                tex_entry(2),
            ],
        });
        let view = |t: &wgpu::Texture| {
            t.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        let (atlas_view, lod_view) = (view(&atlas_tex), view(&lod_tex));
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: camera.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&lod_view) },
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
            attributes: &wgpu::vertex_attr_array![0 => Uint32, 1 => Uint32],
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
        let tile_pipeline = pipeline("vs_tile", "fs_tile", &[Some(instance_layout)]);
        let lod_pipeline = pipeline("vs_lod", "fs_lod", &[]);

        let rows = rows
            .into_iter()
            .map(|r| Row {
                buffer: (!r.instances.is_empty()).then(|| {
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: None,
                        contents: bytemuck::cast_slice(&r.instances),
                        usage: wgpu::BufferUsages::VERTEX,
                    })
                }),
                wall_off: r.wall_off,
                tile_off: r.tile_off,
            })
            .collect();

        Self {
            surface,
            device,
            queue,
            config,
            tile_pipeline,
            lod_pipeline,
            bind_group,
            camera,
            rows,
            chunks_x,
            lod_pages: (pages_x, pages_y),
        }
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

    pub fn render(&mut self, cam: &Camera) -> Stats {
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
            _pad: 0,
        };
        self.queue.write_buffer(&self.camera, 0, bytemuck::bytes_of(&uniform));

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
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.10, g: 0.12, b: 0.18, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            if stats.lod {
                pass.set_pipeline(&self.lod_pipeline);
                pass.draw(0..4, 0..self.lod_pages.0 * self.lod_pages.1);
                stats.draws = 1;
            } else {
                pass.set_pipeline(&self.tile_pipeline);
                // Walls overhang their tile by 8 px: widen by one tile.
                let half = size / (2.0 * cam.zoom) + 16.0;
                let lo = ((cam.center - half) / CHUNK_PX).floor().max(Vec2::ZERO);
                let hi = ((cam.center + half) / CHUNK_PX).ceil();
                let (x0, x1) = (lo.x as usize, (hi.x as usize).min(self.chunks_x as usize));
                let (y0, y1) = (lo.y as usize, (hi.y as usize).min(self.rows.len()));
                // Walls of all visible rows first, then tiles, so tiles cover wall overhang.
                for walls in [true, false] {
                    for row in self.rows.get(y0..y1).unwrap_or(&[]) {
                        let off = if walls { &row.wall_off } else { &row.tile_off };
                        let Some(buf) = &row.buffer else { continue };
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
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        stats
    }
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
