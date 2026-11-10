struct Camera {
    // clip = world_px * scale + offset
    scale: vec2<f32>,
    offset: vec2<f32>,
    lod_pages_x: u32,
    lighting: u32,
    world: vec2<u32>,
    // Skylight colour for the time of day, seconds in w.
    sky: vec4<f32>,
    // Per `water_N` sheet: x, y, layer, 1 if loaded.
    water: array<vec4<u32>, 15>,
    // Water style, top of the underground backdrop in world pixels.
    liquid: vec4<f32>,
    // Grass wind counter in turns (fractional part), wind speed.
    wind: vec4<f32>,
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
    // Top-left texel of the frame, and the block shape (0 full, 1 half, 2..5 slopes).
    @location(4) @interpolate(flat) base: vec2<u32>,
    @location(5) @interpolate(flat) shape: u32,
}

fn corner(vi: u32) -> vec2<f32> {
    return vec2<f32>(f32(vi & 1u), f32(vi >> 1u));
}

// `p` turned clockwise (y down) by `rot` radians about `about`.
fn turn(p: vec2<f32>, about: vec2<f32>, rot: f32) -> vec2<f32> {
    let d = p - about;
    let s = sin(rot);
    let k = cos(rot);
    return about + vec2<f32>(d.x * k - d.y * s, d.x * s + d.y * k);
}

@vertex
fn vs_tile(@builtin(vertex_index) vi: u32, @location(0) a: u32, @location(1) b: u32, @location(2) d: u32) -> VOut {
    let c = corner(vi);
    var out: VOut;
    let tile = vec2<f32>(f32(a & 0x7fffu), f32((a >> 15u) & 0x7fffu));
    let wall = ((a >> 30u) & 1u) != 0u;
    var size = vec2<f32>(select(16.0, 32.0, wall));
    var pad = vec2<f32>(select(0.0, -8.0, wall));
    var u = c.x;
    if d != 0u {
        // The game's draw of the cell: centred sideways, shifted down by `top`.
        size = vec2<f32>(f32(d & 127u), f32((d >> 7u) & 127u));
        pad = vec2<f32>((16.0 - size.x) / 2.0, f32((d >> 14u) & 127u) - 64.0);
        u = select(c.x, 1.0 - c.x, ((d >> 21u) & 1u) != 0u);
    }
    var p = tile * 16.0 + pad + c * size;
    if ((d >> 23u) & 1u) != 0u {
        // Plants lean with the wind about the bottom middle of their cell.
        let wind = cam.wind.y;
        let phase = tile.x * 0.5 + floor(tile.y / 100.0) * 0.5;
        let k = (cos(cam.wind.x * 6.2831855 + phase) * 0.5 + wind) * clamp((abs(wind) - 0.08) / 0.1, 0.0, 1.0);
        let shift = vec2<f32>(k, abs(k));
        p = turn(p + shift, tile * 16.0 + vec2<f32>(8.0, 16.0) + shift, k * 0.1);
    }
    out.pos = vec4<f32>(p * cam.scale + cam.offset, 0.0, 1.0);
    out.base = vec2<u32>(b & 0xfffu, (b >> 12u) & 0xfffu);
    out.texel = vec2<f32>(out.base) + vec2<f32>(u, c.y) * size;
    out.layer = (b >> 24u) & 31u;
    out.shape = b >> 29u;
    out.tile = tile + (pad + c * size) / 16.0;
    return out;
}

@fragment
fn fs_tile(in: VOut) -> @location(0) vec4<f32> {
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

// RGB: tiles and liquids, alpha: sunlight scaled by the sky colour; 1 is stored as 128.
fn combine(l: vec4<f32>) -> vec3<f32> {
    return min(max(l.rgb, l.a * cam.sky.rgb) * (255.0 / 128.0), vec3<f32>(1.0));
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
    @location(5) rot: f32,
    @location(6) pivot: vec2<f32>,
) -> RectOut {
    let c = corner(vi);
    var out: RectOut;
    let world = turn(pos + c * size, pos + pivot, rot);
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

struct LiquidOut {
    @builtin(position) pos: vec4<f32>,
    // Pixel in the cell's frame of its `water_N` sheet, before animation.
    @location(0) texel: vec2<f32>,
    @location(1) tile: vec2<f32>,
    @location(2) @interpolate(flat) cell: vec2<u32>,
    // Frame x, y; kind | sheet << 2 | surface << 6 | waterfall << 7.
    @location(3) @interpolate(flat) frame: vec2<u32>,
    @location(4) @interpolate(flat) bits: u32,
    @location(5) @interpolate(flat) opacity: f32,
}

const WATERFALL = array<f32, 4>(10.0, 3.0, 2.0, 10.0);
const OPACITY = array<f32, 4>(0.6, 0.95, 0.95, 0.75);

// See `liquid::chunk` for the instance layout.
@vertex
fn vs_liquid(@builtin(vertex_index) vi: u32, @location(0) a: u32, @location(1) b: u32, @location(2) d: u32) -> LiquidOut {
    let c = corner(vi);
    var out: LiquidOut;
    let cell = vec2<u32>(a & 0x3fffu, (a >> 14u) & 0xfffu);
    let kind = (a >> 26u) & 3u;
    let src = vec2<u32>(b & 63u, (b >> 6u) & 63u);
    let size = vec2<f32>(f32(((b >> 12u) & 15u) + 1u), f32(((b >> 16u) & 15u) + 1u));
    let off = vec2<f32>(f32((b >> 20u) & 15u), f32((b >> 24u) & 15u));
    let px = vec2<f32>(cell) * 16.0 + off + c * size;
    out.pos = vec4<f32>(px * cam.scale + cam.offset, 0.0, 1.0);
    out.texel = vec2<f32>(src) + c * size;
    out.tile = px / 16.0;
    out.cell = cell;
    out.frame = src;
    var sheets = array<u32, 4>(u32(cam.liquid.x), 1u, 11u, 14u);
    var falls = WATERFALL;
    let surface = ((b >> 28u) & 1u) != 0u && f32(cell.y) > cam.liquid.y / 16.0 - 40.0;
    // Shimmer has no waterfall frames.
    let fall = src.x == 16u && kind != 3u;
    out.bits = kind | (sheets[kind] << 2u) | (u32(surface) << 6u) | (u32(fall) << 7u);
    out.opacity = 1.0 - f32(a >> 28u) / (falls[kind] + 1.0);
    return out;
}

fn hue(h: f32) -> vec3<f32> {
    let k = (vec3<f32>(0.0, 8.0, 4.0) + h * 12.0) % 12.0;
    return 0.5 - 0.5 * clamp(min(k - 3.0, 9.0 - k), vec3<f32>(-1.0), vec3<f32>(1.0));
}

fn noise(x: u32, y: u32) -> u32 {
    let a = 36469u * (x & 0xffffu) + (x >> 16u);
    let b = 18012u * (y & 0xffffu) + (y >> 16u);
    return (a << 16u) + b;
}

fn shimmer_wave(p: vec2<f32>, t: f32) -> f32 {
    return sin(((p.x + p.y / 6.0) / 10.0 - t / 360.0) * 6.2831855);
}

fn remap(v: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    return mix(c, d, clamp((v - a) / (b - a), 0.0, 1.0));
}

// Glitter at a tile corner, premultiplied with no alpha (added).
fn glitter(top: bool, p: vec2<f32>, t: f32) -> vec3<f32> {
    let color = hue(fract((p.x + p.y / 6.0 + t / 30.0) / 6.0));
    if top {
        return color * 0.5;
    }
    let wave = remap(shimmer_wave(p, t), -0.5, 1.0, 0.0, 0.35);
    let n = sin(f32(noise(u32(p.x), u32(p.y))) / 10.0 + t / 180.0);
    return color * remap(wave * n, 0.0, 0.5, 0.0, 1.0);
}

fn corners_mix(v: array<vec3<f32>, 4>, f: vec2<f32>) -> vec3<f32> {
    return mix(mix(v[0], v[1], f.x), mix(v[2], v[3], f.x), f.y);
}

fn sheet_texel(sheet: u32, p: vec2<f32>) -> vec4<f32> {
    let s = cam.water[sheet];
    let t = vec2<i32>(s.xy) + vec2<i32>(floor(p));
    return textureLoad(atlas, t, i32(s.z), 0);
}

fn liquid(in: LiquidOut, front: bool) -> vec4<f32> {
    let kind = in.bits & 3u;
    let sheet = (in.bits >> 2u) & 15u;
    // Frames advance 6 per second with no wind, waterfalls 0.5.
    let t = cam.sky.w;
    let frame = select(u32(t * 6.0) % 16u, u32(t * 0.5) % 16u, (in.bits & 128u) != 0u);
    var p = in.texel;
    p.y = select(p.y + f32(frame * 80u), 1280.0 + p.y - f32(in.frame.y), (in.bits & 64u) != 0u);
    p = clamp(p, vec2<f32>(0.0), vec2<f32>(47.0, 1359.0));
    var opacity = OPACITY;
    let op = min(1.0, in.opacity * select(1.0, opacity[kind], front));
    if cam.water[sheet].w == 0u {
        var flat = LIQUID_FLAT;
        return vec4<f32>(flat[kind] * op * smooth_light(in.tile), op);
    }
    let c = sheet_texel(sheet, p);
    if kind != 3u {
        return vec4<f32>(c.rgb * smooth_light(in.tile), c.a) * op;
    }
    // Shimmer is unlit: a tint waving along the world, plus coloured glitter.
    let ticks = t * 60.0;
    let cell = vec2<f32>(in.cell);
    let f = in.tile - cell;
    var tint: array<vec3<f32>, 4>;
    var spark: array<vec3<f32>, 4>;
    let top = in.frame.x != 16u || in.frame.y != 48u;
    for (var i = 0u; i < 4u; i++) {
        let q = cell + vec2<f32>(f32(i & 1u), f32(i >> 1u));
        let w = 0.1 + shimmer_wave(q, ticks) * 0.4;
        tint[i] = mix(vec3<f32>(0.64705884, 0.50980395, 0.93333334), vec3<f32>(0.8039216, 0.8039216, 1.0), w);
        spark[i] = glitter(top, q, ticks);
    }
    var out = vec4<f32>(c.rgb * corners_mix(tint, f), c.a) * op;
    if top || (in.cell.x + in.cell.y) % 2u == 0u {
        let g = cell + 0.5;
        var n = (g.x + g.y / 6.0) / 10.0 - ticks / 360.0;
        if !top {
            n += g.x + g.y;
        }
        let sf = u32((i32(n) % 16 + 16) % 16);
        let s = sheet_texel(sheet, vec2<f32>(in.texel.x + 48.0, in.texel.y + f32(sf * 80u)));
        out += vec4<f32>(s.rgb * corners_mix(spark, f) * in.opacity, 0.0);
    }
    return out;
}

// Flat colours when the sheets are missing.
const LIQUID_FLAT = array<vec3<f32>, 4>(
    vec3<f32>(0.15, 0.35, 0.90),
    vec3<f32>(0.95, 0.40, 0.08),
    vec3<f32>(0.95, 0.65, 0.10),
    vec3<f32>(0.80, 0.60, 1.00),
);

// Behind walls at full opacity, hidden under the underground backdrop.
@fragment
fn fs_liquid_back(in: LiquidOut) -> @location(0) vec4<f32> {
    if in.tile.y * 16.0 >= cam.liquid.y {
        discard;
    }
    return liquid(in, false);
}

@fragment
fn fs_liquid_front(in: LiquidOut) -> @location(0) vec4<f32> {
    return liquid(in, true);
}
