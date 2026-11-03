struct Camera {
    // clip = world_px * scale + offset
    scale: vec2<f32>,
    offset: vec2<f32>,
    lod_pages_x: u32,
    lighting: u32,
    world: vec2<u32>,
}

@group(0) @binding(0) var<uniform> cam: Camera;
@group(0) @binding(1) var atlas: texture_2d_array<f32>;
@group(0) @binding(2) var lod: texture_2d_array<f32>;
// Per-tile light, same page layout as `lod`.
@group(0) @binding(3) var light: texture_2d_array<f32>;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) texel: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    // World position in tiles.
    @location(2) tile: vec2<f32>,
}

fn corner(vi: u32) -> vec2<f32> {
    return vec2<f32>(f32(vi & 1u), f32(vi >> 1u));
}

@vertex
fn vs_tile(@builtin(vertex_index) vi: u32, @location(0) a: u32, @location(1) b: u32) -> VOut {
    let c = corner(vi);
    let wall = (a >> 30u) != 0u;
    let size = select(16.0, 32.0, wall);
    let pad = select(0.0, -8.0, wall);
    let tile = vec2<f32>(f32(a & 0x7fffu), f32((a >> 15u) & 0x7fffu));
    var out: VOut;
    out.pos = vec4<f32>((tile * 16.0 + pad + c * size) * cam.scale + cam.offset, 0.0, 1.0);
    out.texel = vec2<f32>(f32(b & 0xfffu), f32((b >> 12u) & 0xfffu)) + c * size;
    out.layer = b >> 24u;
    out.tile = tile + (pad + c * size) / 16.0;
    return out;
}

@fragment
fn fs_tile(in: VOut) -> @location(0) vec4<f32> {
    let c = textureLoad(atlas, vec2<i32>(floor(in.texel)), i32(in.layer), 0);
    if c.a == 0.0 {
        discard;
    }
    return c * vec4<f32>(smooth_light(in.tile), 1.0);
}

const PAGE: f32 = 2048.0;

fn light_at(t: vec2<i32>) -> vec3<f32> {
    let p = clamp(t, vec2<i32>(0), vec2<i32>(cam.world) - 1);
    let page = p / 2048;
    return textureLoad(light, p % 2048, page.y * i32(cam.lod_pages_x) + page.x, 0).rgb;
}

// Bilinear between tile centres.
fn smooth_light(tile: vec2<f32>) -> vec3<f32> {
    if cam.lighting == 0u {
        return vec3<f32>(1.0);
    }
    let q = tile - 0.5;
    let i = vec2<i32>(floor(q));
    let f = q - floor(q);
    let top = mix(light_at(i), light_at(i + vec2<i32>(1, 0)), f.x);
    let bottom = mix(light_at(i + vec2<i32>(0, 1)), light_at(i + vec2<i32>(1, 1)), f.x);
    return mix(top, bottom, f.y);
}

@vertex
fn vs_lod(@builtin(vertex_index) vi: u32, @builtin(instance_index) page: u32) -> VOut {
    let c = corner(vi);
    let origin = vec2<f32>(f32(page % cam.lod_pages_x), f32(page / cam.lod_pages_x)) * PAGE;
    var out: VOut;
    out.pos = vec4<f32>((origin + c * PAGE) * 16.0 * cam.scale + cam.offset, 0.0, 1.0);
    out.texel = c * PAGE;
    out.layer = page;
    out.tile = vec2<f32>(0.0);
    return out;
}

@fragment
fn fs_lod(in: VOut) -> @location(0) vec4<f32> {
    let p = min(vec2<i32>(floor(in.texel)), vec2<i32>(2047));
    let c = textureLoad(lod, p, i32(in.layer), 0);
    if cam.lighting == 0u {
        return c;
    }
    return c * vec4<f32>(textureLoad(light, p, i32(in.layer), 0).rgb, 1.0);
}
