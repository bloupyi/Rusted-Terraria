struct Camera {
    // clip = world_px * scale + offset
    scale: vec2<f32>,
    offset: vec2<f32>,
    lod_pages_x: u32,
    lighting: u32,
    world: vec2<u32>,
    // Skylight colour for the time of day, seconds in w.
    sky: vec4<f32>,
    // Per liquid kind: sheet x, y, layer, frame count (0: flat colour).
    liquids: array<vec4<u32>, 4>,
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
    // Liquid kind + 1, 0 for sheets.
    @location(3) @interpolate(flat) liquid: u32,
    // Top-left texel of the frame, and the block shape (0 full, 1 half, 2..5 slopes).
    @location(4) @interpolate(flat) base: vec2<u32>,
    @location(5) @interpolate(flat) shape: u32,
}

fn corner(vi: u32) -> vec2<f32> {
    return vec2<f32>(f32(vi & 1u), f32(vi >> 1u));
}

@vertex
fn vs_tile(@builtin(vertex_index) vi: u32, @location(0) a: u32, @location(1) b: u32) -> VOut {
    let c = corner(vi);
    var out: VOut;
    let tile = vec2<f32>(f32(a & 0x7fffu), f32((a >> 15u) & 0x7fffu));
    if (a >> 31u) != 0u {
        // Liquid: fills the bottom `amount / 255` of its tile.
        let amount = f32(b & 0xffu) / 255.0;
        // Surface cells ripple; the wave follows world x so neighbours line up.
        let wx = tile.x + c.x;
        let ripple = select(0.0, 0.07 * sin(1.7 * wx + 2.2 * cam.sky.w) + 0.04 * sin(0.6 * wx - 1.3 * cam.sky.w), amount < 1.0);
        let top = clamp(1.0 - amount + ripple, 0.0, 1.0);
        let local = vec2<f32>(c.x, mix(top, 1.0, c.y));
        out.pos = vec4<f32>((tile + local) * 16.0 * cam.scale + cam.offset, 0.0, 1.0);
        out.tile = tile + local;
        // Sheet rows from the surface down; cells under liquid use the body rows only.
        let v = select((1.0 - top) * 16.0 * c.y, 8.0 + 8.0 * c.y, amount >= 1.0);
        out.texel = vec2<f32>(c.x * 16.0, v);
        out.liquid = ((b >> 8u) & 0xffu) + 1u;
        out.base = vec2<u32>(0u);
        out.shape = 0u;
        return out;
    }
    let wall = ((a >> 30u) & 1u) != 0u;
    let size = select(16.0, 32.0, wall);
    let pad = select(0.0, -8.0, wall);
    out.pos = vec4<f32>((tile * 16.0 + pad + c * size) * cam.scale + cam.offset, 0.0, 1.0);
    out.base = vec2<u32>(b & 0xfffu, (b >> 12u) & 0xfffu);
    out.texel = vec2<f32>(out.base) + c * size;
    out.layer = (b >> 24u) & 31u;
    out.shape = b >> 29u;
    out.tile = tile + (pad + c * size) / 16.0;
    out.liquid = 0u;
    return out;
}

// Premultiplied flat colours; alpha also scales the textured liquids.
fn liquid_color(kind: u32) -> vec4<f32> {
    switch kind {
        case 1u: { return vec4<f32>(0.95, 0.40, 0.08, 1.0) * 0.95; }
        case 2u: { return vec4<f32>(0.95, 0.65, 0.10, 1.0) * 0.80; }
        case 3u: { return vec4<f32>(0.80, 0.60, 1.00, 1.0) * 0.65; }
        default: { return vec4<f32>(0.15, 0.35, 0.90, 1.0) * 0.55; }
    }
}

@fragment
fn fs_tile(in: VOut) -> @location(0) vec4<f32> {
    if in.liquid != 0u {
        var c = liquid_color(in.liquid - 1u);
        let sheet = cam.liquids[(in.liquid - 1u) & 3u];
        if sheet.w != 0u {
            let frame = u32(cam.sky.w * 8.0) % sheet.w;
            let t = clamp(vec2<i32>(floor(in.texel)), vec2<i32>(0), vec2<i32>(15));
            let p = vec2<i32>(sheet.xy) + vec2<i32>(i32(frame) * 18, 0) + t;
            c = vec4<f32>(textureLoad(atlas, p, i32(sheet.z), 0).rgb * c.a, c.a);
        }
        return vec4<f32>(c.rgb * smooth_light(in.tile), c.a);
    }
    var t = vec2<i32>(floor(in.texel));
    if in.shape != 0u {
        let local = t - vec2<i32>(in.base);
        let y = shaped_row(in.shape, local);
        if y < 0 {
            discard;
        }
        t.y = i32(in.base.y) + y;
    }
    let c = textureLoad(atlas, t, i32(in.layer), 0);
    if c.a == 0.0 {
        discard;
    }
    return c * vec4<f32>(smooth_light(in.tile), 1.0);
}

// Sheet row shown at `local` (pixels in the tile) for a half block or slope,
// -1 where the shape leaves the cell empty. Slopes are 2-pixel columns of the
// frame slid along the diagonal, with the flat edge kept whole.
fn shaped_row(shape: u32, local: vec2<i32>) -> i32 {
    let y = local.y;
    if shape == 1u {
        return select(-1, y - 8, y >= 8);
    }
    // Shapes 2 and 4 rise to the left, 3 and 5 to the right.
    let col = local.x / 2;
    let i = select(7 - col, col, shape == 2u || shape == 4u);
    if shape <= 3u {
        if y >= 14 {
            return y;
        }
        return select(-1, y - 2 * i, y >= 2 * i);
    }
    if y < 2 {
        return y;
    }
    return select(-1, y + 2 * i, y < 16 - 2 * i);
}

const PAGE: f32 = 2048.0;

fn light_at(t: vec2<i32>) -> vec3<f32> {
    let p = clamp(t, vec2<i32>(0), vec2<i32>(cam.world) - 1);
    let page = p / 2048;
    return combine(textureLoad(light, p % 2048, page.y * i32(cam.lod_pages_x) + page.x, 0));
}

// RGB: tiles and liquids, alpha: skylight scaled by the time of day.
fn combine(l: vec4<f32>) -> vec3<f32> {
    return max(l.rgb, l.a * cam.sky.rgb);
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
    out.liquid = 0u;
    out.base = vec2<u32>(0u);
    out.shape = 0u;
    return out;
}

@fragment
fn fs_lod(in: VOut) -> @location(0) vec4<f32> {
    let p = min(vec2<i32>(floor(in.texel)), vec2<i32>(2047));
    let c = textureLoad(lod, p, i32(in.layer), 0);
    if cam.lighting == 0u {
        return c;
    }
    return c * vec4<f32>(combine(textureLoad(light, p, i32(in.layer), 0)), 1.0);
}

struct RectOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) texel: vec2<f32>,
    @location(2) @interpolate(flat) flags: u32,
    @location(3) tile: vec2<f32>,
    @location(4) @interpolate(flat) textured: u32,
}

@vertex
fn vs_rect(
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec4<f32>,
    @location(4) flags: u32,
) -> RectOut {
    let c = corner(vi);
    var out: RectOut;
    let world = pos + c * size;
    out.pos = vec4<f32>(world * cam.scale + cam.offset, 0.0, 1.0);
    out.color = color;
    let u = select(c.x, 1.0 - c.x, (flags & 512u) != 0u);
    out.texel = uv.xy + vec2<f32>(u, c.y) * uv.zw;
    out.flags = flags;
    out.tile = world / 16.0;
    out.textured = u32(uv.z > 0.0);
    return out;
}

@fragment
fn fs_rect(in: RectOut) -> @location(0) vec4<f32> {
    var c = in.color;
    if in.textured != 0u {
        // Clamp inside the region so edge texels never bleed into a neighbour.
        let t = textureLoad(atlas, vec2<i32>(floor(in.texel - 0.001)), i32(in.flags & 255u), 0);
        if t.a == 0.0 {
            discard;
        }
        c = t * in.color;
    }
    if (in.flags & 256u) != 0u {
        c = vec4<f32>(c.rgb * smooth_light(in.tile), c.a);
    }
    return c;
}
